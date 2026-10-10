//! The runtime half of
//! [protoc-gen-rust-defaults](https://github.com/protoc-contrib/protoc-gen-rust-defaults).
//!
//! The plugin emits an inherent `set_defaults(&mut self)` for each buffa
//! message with `protoc_contrib.defaults` annotations, and implements
//! [`SetDefaults`] by calling it. The trait is what code that does not know the
//! concrete message type reaches the defaults through -- an interceptor
//! applying them to every request, say -- and what a generated
//! `message = { recurse: true }` calls on the nested message.
//!
//! It stands in for protoc-gen-go-defaults' reflection-based `defaults.Apply`:
//! buffa has no mutable reflection, so a trait is the only generic way in.

/// A message whose unset fields can be filled with their declared defaults.
///
/// Generated for every message with a `(protoc_contrib.defaults.value)` or
/// `(protoc_contrib.defaults.oneof)` annotation, except those marked
/// `(protoc_contrib.defaults.skip)` or `(protoc_contrib.defaults.unexported)`.
/// For those, the plugin expects it written by hand -- and calls it when a
/// field of theirs is annotated `recurse` -- so it can be composed with logic
/// no annotation describes:
///
/// ```ignore
/// impl protoc_contrib_defaults::SetDefaults for CreateBookRequest {
///     fn set_defaults(&mut self) {
///         // The generated body, under `unexported`.
///         self.apply_defaults();
///         if self.book_id.is_empty() {
///             self.book_id = mint_id();
///         }
///     }
/// }
/// ```
pub trait SetDefaults {
    /// Fills each unset field with its default. A field that is already set
    /// is left as it is.
    fn set_defaults(&mut self);
}

/// A boxed message defaults its contents: buffa boxes a message held in a
/// oneof, and a recursive message field.
impl<T: SetDefaults + ?Sized> SetDefaults for Box<T> {
    fn set_defaults(&mut self) {
        (**self).set_defaults();
    }
}

/// The current time as a `google.protobuf.Timestamp`'s `(seconds, nanos)`,
/// for a `timestamp = "now"` default.
///
/// The two fields rather than a `Timestamp`, so generated code fills whichever
/// `Timestamp` type the consumer mapped `google.protobuf` to, and this crate
/// depends on nothing. A clock set before 1970 gives a negative `seconds` and
/// a `nanos` in `0..1_000_000_000`, as the well-known type requires.
#[must_use]
pub fn now() -> (i64, i32) {
    use std::time::{SystemTime, UNIX_EPOCH};

    let (sign, elapsed) = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(elapsed) => (1, elapsed),
        Err(before) => (-1, before.duration()),
    };
    let seconds = i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX);
    // Below one second, so it fits.
    let nanos = elapsed.subsec_nanos().cast_signed();
    if sign > 0 || nanos == 0 {
        (sign * seconds, nanos)
    } else {
        (-seconds - 1, 1_000_000_000 - nanos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Counter(u32);

    impl SetDefaults for Counter {
        fn set_defaults(&mut self) {
            self.0 += 1;
        }
    }

    #[test]
    fn a_box_defaults_what_it_holds() {
        let mut boxed = Box::new(Counter::default());
        SetDefaults::set_defaults(&mut boxed);
        assert_eq!(boxed.0, 1);
    }

    #[test]
    fn now_is_after_this_was_written() {
        let (seconds, nanos) = now();
        // 2026-01-01T00:00:00Z.
        assert!(seconds > 1_767_225_600);
        assert!((0..1_000_000_000).contains(&nanos));
    }
}
