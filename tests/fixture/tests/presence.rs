//! "Unset" means what buffa's typing of the field says it means: `None` for an
//! `Option`, the zero value for a bare field -- in proto2 and editions as much
//! as in proto3.

#![allow(
    clippy::float_cmp,
    reason = "asserting a field holds its declared default"
)]

use buffa::EnumValue;
use defaults_fixture::proto::example::v1::{Edition, Legacy, Mode, Strictness, legacy};

#[test]
fn proto2_fills_absent_optionals_and_zero_required_fields() {
    let mut message = Legacy::default();
    message.set_defaults();

    assert_eq!(message.retries, Some(3));
    assert_eq!(message.region.as_deref(), Some("us-east1"));
    assert_eq!(message.token.as_deref(), Some(b"abc".as_slice()));
    // A closed enum: the enum itself, not `EnumValue`.
    assert_eq!(message.level, Some(legacy::Level::MEDIUM));
    assert_eq!(message.ratio, Some(0.5));
    assert_eq!(message.budget, 1_000_000);
    assert_eq!(message.floor, legacy::Level::HIGH);
}

#[test]
fn proto2_leaves_an_optional_explicitly_set_to_zero() {
    let mut message = Legacy {
        retries: Some(0),
        region: Some(String::new()),
        token: Some(Vec::new()),
        level: Some(legacy::Level::LOW),
        ..Default::default()
    };
    message.set_defaults();

    assert_eq!(message.retries, Some(0));
    assert_eq!(message.region.as_deref(), Some(""));
    assert_eq!(message.token.as_deref(), Some([].as_slice()));
    assert_eq!(message.level, Some(legacy::Level::LOW));
}

#[test]
fn editions_follows_each_fields_resolved_presence() {
    let mut message = Edition {
        explicit_count: Some(0),
        ..Default::default()
    };
    message.set_defaults();

    // Explicit presence, set to zero: set.
    assert_eq!(message.explicit_count, Some(0));
    // Implicit presence, at zero: unset.
    assert_eq!(message.implicit_count, 8);
    assert_eq!(message.implicit_name, "edition");
    assert_eq!(message.mode, Some(EnumValue::Known(Mode::MODE_FAST)));
    assert_eq!(message.strictness, Some(Strictness::STRICTNESS_ON));
}

#[test]
fn editions_fills_an_absent_explicit_field() {
    let mut message = Edition::default();
    message.set_defaults();
    assert_eq!(message.explicit_count, Some(7));
}
