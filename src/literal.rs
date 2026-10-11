//! The two option values that are strings standing for something else:
//! `(protoc.contrib.defaults.value).duration` and `.timestamp`.
//!
//! Both are parsed here, at generation time, into the seconds and nanoseconds
//! the generated code assigns — so a malformed literal fails `buf generate`
//! rather than compiling into a parse that can fail at runtime. The one
//! exception is a timestamp of `"now"`, which can only be read off the clock
//! when the defaults are applied.
//!
//! Each accepts exactly what protoc-gen-go-defaults accepts, so one annotation
//! means the same thing to both plugins, with one deliberate difference noted
//! on [`timestamp`].

use chrono::{DateTime, FixedOffset, NaiveDateTime};

/// A `google.protobuf.Duration` or `Timestamp` value, as its two wire fields.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Instant {
    pub seconds: i64,
    pub nanos: i32,
}

/// What a `timestamp` default stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Timestamp {
    /// `"now"`: read off the clock when the defaults are applied.
    Now,
    /// A fixed instant, resolved here.
    At(Instant),
}

const NANOS_PER_SECOND: u64 = 1_000_000_000;

/// The units [`duration`] accepts, largest first, in nanoseconds. A duration
/// names each at most once and in this order, as `prometheus/common/model`'s
/// `ParseDuration` requires.
const UNITS: &[(&str, u64)] = &[
    ("y", 365 * 24 * 60 * 60 * NANOS_PER_SECOND),
    ("w", 7 * 24 * 60 * 60 * NANOS_PER_SECOND),
    ("d", 24 * 60 * 60 * NANOS_PER_SECOND),
    ("h", 60 * 60 * NANOS_PER_SECOND),
    ("m", 60 * NANOS_PER_SECOND),
    ("s", NANOS_PER_SECOND),
    ("ms", 1_000_000),
];

/// Parses a duration in the grammar of Prometheus' `model.ParseDuration`, the
/// one protoc-gen-go-defaults uses: `[0-9]+<unit>` pairs with units drawn from
/// `y w d h m s ms`, largest first, each at most once — `"1h30m"`, `"42w"` —
/// or a bare `"0"`.
///
/// A superset of what Go's `time.ParseDuration` reads in the units that
/// matter for defaults (`"2d"`), and a subset everywhere else: no fractions,
/// no sign, no `us` or `ns`.
///
/// # Errors
///
/// Prometheus' own messages, so a schema fails the same way under either
/// plugin.
pub fn duration(s: &str) -> Result<Instant, String> {
    match s {
        "0" => return Ok(Instant::default()),
        "" => return Err("empty duration string".to_owned()),
        _ => {}
    }
    let invalid = || format!("not a valid duration string: {s:?}");

    let mut rest = s;
    let mut total: u64 = 0;
    // Index into UNITS one past the last unit read, so the next must be
    // strictly smaller.
    let mut next_unit = 0;
    while !rest.is_empty() {
        let digits = rest
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(rest.len());
        if digits == 0 {
            return Err(invalid());
        }
        let value: u64 = rest[..digits].parse().map_err(|_| invalid())?;
        rest = &rest[digits..];

        let letters = rest
            .find(|c: char| c.is_ascii_digit())
            .unwrap_or(rest.len());
        if letters == 0 {
            return Err(invalid());
        }
        let unit = &rest[..letters];
        rest = &rest[letters..];

        let Some(position) = UNITS.iter().position(|(name, _)| *name == unit) else {
            return Err(format!("unknown unit {unit:?} in duration {s:?}"));
        };
        if position < next_unit {
            return Err(invalid());
        }
        next_unit = position + 1;

        // Bounded by what Go's `time.Duration` holds, so a default that one
        // plugin accepts the other does too.
        let nanos = value
            .checked_mul(UNITS[position].1)
            .and_then(|nanos| total.checked_add(nanos))
            .filter(|total| i64::try_from(*total).is_ok())
            .ok_or_else(|| "duration out of range".to_owned())?;
        total = nanos;
    }

    Ok(Instant {
        seconds: i64::try_from(total / NANOS_PER_SECOND).expect("bounded by i64 above"),
        nanos: i32::try_from(total % NANOS_PER_SECOND).expect("below one second"),
    })
}

/// The layouts [`timestamp`] tries before RFC3339, in the order
/// protoc-gen-go-defaults tries them, each with whether it ends in a numeric
/// offset. One that does not ends in a zone abbreviation, which chrono does
/// not parse and [`zone`] resolves instead.
const LAYOUTS: &[(&str, bool)] = &[
    // RFC822: `02 Jan 06 15:04 MST`
    ("%d %b %y %H:%M", false),
    // RFC822Z: `02 Jan 06 15:04 -0700`
    ("%d %b %y %H:%M %z", true),
    // RFC850: `Monday, 02-Jan-06 15:04:05 MST`
    ("%A, %d-%b-%y %H:%M:%S", false),
    // RFC1123: `Mon, 02 Jan 2006 15:04:05 MST`
    ("%a, %d %b %Y %H:%M:%S", false),
    // RFC1123Z: `Mon, 02 Jan 2006 15:04:05 -0700`
    ("%a, %d %b %Y %H:%M:%S %z", true),
];

/// Parses a timestamp default: `"now"`, in any case, or an instant in RFC822,
/// RFC822Z, RFC850, RFC1123, RFC1123Z or RFC3339 — the layouts
/// protoc-gen-go-defaults accepts. Surrounding whitespace is ignored.
///
/// One difference from Go, on purpose: a zone *abbreviation* (`MST`) is
/// resolved to the fixed offset RFC 822 gives it — `UT`, `GMT`, `Z` and the
/// four North American zones — and any other abbreviation is an error. Go
/// resolves an abbreviation against the time zone of the machine running the
/// generator and silently reads an unknown one as UTC, so the same schema can
/// generate different code on two machines. A numeric offset or RFC3339 is the
/// way to write an instant both plugins read identically.
///
/// # Errors
///
/// When no layout matches, with the message protoc-gen-go-defaults gives.
pub fn timestamp(s: &str) -> Result<Timestamp, String> {
    let s = s.trim();
    if s.eq_ignore_ascii_case("now") {
        return Ok(Timestamp::Now);
    }
    parse_time(s).map(Timestamp::At).ok_or_else(|| {
        "cannot parse timestamp; supported formats: RFC822 / RFC822Z / RFC850 / RFC1123 / RFC1123Z / RFC3339".to_owned()
    })
}

fn parse_time(s: &str) -> Option<Instant> {
    let parsed = LAYOUTS
        .iter()
        .find_map(|(layout, numeric)| {
            if *numeric {
                DateTime::parse_from_str(s, layout).ok()
            } else {
                let (local, abbreviation) = s.rsplit_once(' ')?;
                let offset = zone(abbreviation)?;
                let local = NaiveDateTime::parse_from_str(local, layout).ok()?;
                local.and_local_timezone(offset).single()
            }
        })
        .or_else(|| DateTime::parse_from_rfc3339(s).ok())?;
    Some(Instant {
        seconds: parsed.timestamp(),
        nanos: i32::try_from(parsed.timestamp_subsec_nanos()).ok()?,
    })
}

/// The fixed offset RFC 822 §5.1 assigns a zone abbreviation.
fn zone(abbreviation: &str) -> Option<FixedOffset> {
    let hours = match abbreviation {
        "UT" | "UTC" | "GMT" | "Z" => 0,
        "EDT" => -4,
        "EST" | "CDT" => -5,
        "CST" | "MDT" => -6,
        "MST" | "PDT" => -7,
        "PST" => -8,
        _ => return None,
    };
    FixedOffset::east_opt(hours * 60 * 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seconds(seconds: i64) -> Instant {
        Instant { seconds, nanos: 0 }
    }

    #[test]
    fn reads_each_unit_and_their_sum() {
        assert_eq!(duration("0"), Ok(seconds(0)));
        assert_eq!(duration("1s"), Ok(seconds(1)));
        assert_eq!(duration("1m"), Ok(seconds(60)));
        assert_eq!(duration("2d"), Ok(seconds(2 * 86_400)));
        assert_eq!(duration("42w"), Ok(seconds(42 * 7 * 86_400)));
        assert_eq!(duration("1y"), Ok(seconds(365 * 86_400)));
        assert_eq!(duration("1h30m"), Ok(seconds(5_400)));
        assert_eq!(
            duration("1s500ms"),
            Ok(Instant {
                seconds: 1,
                nanos: 500_000_000
            })
        );
    }

    #[test]
    fn rejects_what_prometheus_rejects() {
        assert_eq!(duration(""), Err("empty duration string".to_owned()));
        // Prometheus reads a unit as the run of non-digits after a number, so
        // a fraction or a trailing space is an unknown unit, not bad syntax.
        for (input, unit) in [("1x", "x"), ("1.5h", "."), ("1h ", "h ")] {
            assert_eq!(
                duration(input),
                Err(format!("unknown unit {unit:?} in duration {input:?}"))
            );
        }
        for invalid in ["abc", "1", "s", "-1s", "30m1h", "1h1h"] {
            assert_eq!(
                duration(invalid),
                Err(format!("not a valid duration string: {invalid:?}")),
                "{invalid}"
            );
        }
        assert_eq!(duration("1000y"), Err("duration out of range".to_owned()));
    }

    #[test]
    fn now_is_read_at_runtime_in_any_case() {
        assert_eq!(timestamp("now"), Ok(Timestamp::Now));
        assert_eq!(timestamp(" NOW "), Ok(Timestamp::Now));
    }

    #[test]
    fn reads_every_layout_go_reads() {
        // 1952-03-11T00:00:00Z, the protoc-gen-go-defaults fixture's instant,
        // in each layout with a four-digit year.
        let want = Ok(Timestamp::At(seconds(-562_032_000)));
        assert_eq!(timestamp("1952-03-11T00:00:00Z"), want);
        assert_eq!(timestamp("Tue, 11 Mar 1952 00:00:00 GMT"), want);
        assert_eq!(timestamp("Mon, 10 Mar 1952 19:00:00 -0500"), want);
        assert_eq!(timestamp("Mon, 10 Mar 1952 19:00:00 EST"), want);

        // 1999-03-11T00:00:00Z in the two-digit-year layouts. Like Go, a year
        // below 69 is read as 20xx, so these cannot spell 1952.
        let want = Ok(Timestamp::At(seconds(921_110_400)));
        assert_eq!(timestamp("11 Mar 99 00:00 GMT"), want);
        assert_eq!(timestamp("11 Mar 99 01:00 +0100"), want);
        assert_eq!(timestamp("Thursday, 11-Mar-99 00:00:00 UTC"), want);
        assert_eq!(
            timestamp("11 Mar 52 00:00 UT"),
            Ok(Timestamp::At(seconds(2_593_728_000)))
        );
    }

    #[test]
    fn keeps_fractional_seconds() {
        assert_eq!(
            timestamp("2024-01-02T03:04:05.25+00:00"),
            Ok(Timestamp::At(Instant {
                seconds: 1_704_164_645,
                nanos: 250_000_000
            }))
        );
    }

    #[test]
    fn rejects_an_abbreviation_rfc_822_does_not_define() {
        // Go would read this against the generating machine's zone.
        assert!(timestamp("Tue, 11 Mar 1952 00:00:00 CET").is_err());
    }

    #[test]
    fn rejects_anything_else_with_gos_message() {
        assert_eq!(
            timestamp("yesterday"),
            Err("cannot parse timestamp; supported formats: RFC822 / RFC822Z / RFC850 / RFC1123 / RFC1123Z / RFC3339".to_owned())
        );
    }
}
