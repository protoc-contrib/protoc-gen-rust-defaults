//! The protoc-gen-go-defaults fixture, ported: what `SetDefaults()` does to a
//! `tests.Test` and a `tests.Types` in Go, `set_defaults()` does here.

// A default is assigned, not computed, so a float either is the declared
// value exactly or the method did not run.
#![allow(
    clippy::float_cmp,
    reason = "asserting a field holds its declared default"
)]

use buffa::{EnumValue, MessageField};
use defaults_fixture::proto::tests::{
    OneOfOne, OneOfThree, OneOfTwo, Test, TestOptional, TestUnexported, Types, test, types,
};
use protoc_contrib_defaults::SetDefaults;

fn unix_now() -> i64 {
    i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    )
    .unwrap()
}

#[test]
fn fills_every_scalar_kind() {
    let mut types = Types::default();
    types.set_defaults();

    assert_eq!(types.float, 0.42);
    assert_eq!(types.double, 0.42);
    assert_eq!(types.int32, 42);
    assert_eq!(types.int64, 42);
    assert_eq!(types.uint32, 42);
    assert_eq!(types.uint64, 42);
    assert_eq!(types.sint32, 42);
    assert_eq!(types.sint64, 42);
    assert_eq!(types.fixed32, 42);
    assert_eq!(types.fixed64, 42);
    assert_eq!(types.sfixed32, 42);
    assert_eq!(types.sfixed64, 42);
    assert!(types.bool);
    assert_eq!(types.string, "42");
    assert_eq!(types.bytes, b"42");
}

#[test]
fn fills_enums_by_number_and_by_name() {
    let mut types = Types::default();
    types.set_defaults();
    assert_eq!(types.r#enum, EnumValue::Known(types::Enum::ONE));
    assert_eq!(types.enum_by_name, EnumValue::Known(types::Enum::TWO));
}

#[test]
fn fills_every_wrapper() {
    let mut types = Types::default();
    types.set_defaults();

    assert_eq!(types.double_value.value, 0.42);
    assert_eq!(types.float_value.value, 0.42);
    assert_eq!(types.int64_value.value, 42);
    assert_eq!(types.uint64_value.value, 42);
    assert_eq!(types.int32_value.value, 42);
    assert_eq!(types.uint32_value.value, 42);
    assert_eq!(types.string_value.value, "42");
    assert_eq!(types.bytes_value.value, b"42");
    // `false` is the zero value, but a wrapper's presence is the message's:
    // defaulting it still sets the wrapper.
    assert!(types.bool_value.is_set());
    assert!(!types.bool_value.value);
}

#[test]
fn fills_well_known_types() {
    let mut test = Test::default();
    test.set_defaults();

    // "42w"
    assert_eq!(test.duration_value_field.seconds, 42 * 7 * 24 * 60 * 60);
    assert_eq!(test.duration_value_field.nanos, 0);
    // "1952-03-11T00:00:00Z"
    assert_eq!(test.time_value_field_with_default.seconds, -562_032_000);
    // "now", read when the defaults are applied.
    assert!((test.time_value_field.seconds - unix_now()).abs() <= 1);

    let mut types = Types::default();
    types.set_defaults();
    // "2d"
    assert_eq!(types.duration.seconds, 2 * 24 * 60 * 60);
}

#[test]
fn initializes_a_message_and_leaves_it_to_its_own_defaults_unless_recursing() {
    let mut types = Types::default();
    types.set_defaults();
    // `{initialize: true}` without `recurse`: created, not defaulted.
    assert!(types.message.is_set());
    assert_eq!(types.message.field, "");
}

#[test]
fn sets_the_named_oneof_arm_and_recurses_into_it() {
    let mut types = Types::default();
    types.set_defaults();
    let Some(types::Oneof::Two(two)) = &types.oneof else {
        panic!("expected the `two` arm, got {:?}", types.oneof);
    };
    assert_eq!(two.string_field, "string_field");
}

#[test]
fn recurses_into_an_active_arm_the_caller_chose() {
    let mut types = Types {
        oneof: Some(types::Oneof::Three(Box::default())),
        ..Default::default()
    };
    types.set_defaults();
    let Some(types::Oneof::Three(three)) = &types.oneof else {
        panic!("the caller's arm was replaced: {:?}", types.oneof);
    };
    assert_eq!(three.string_field, "string_field");
}

#[test]
fn recurses_into_a_skipped_arm_through_its_hand_written_impl() {
    let mut test = Test {
        oneof: Some(test::Oneof::One(Box::default())),
        ..Default::default()
    };
    test.set_defaults();
    let Some(test::Oneof::One(one)) = &test.oneof else {
        panic!("the caller's arm was replaced: {:?}", test.oneof);
    };
    assert_eq!(one.string_field, "hand-written");
}

#[test]
fn leaves_an_active_scalar_arm_alone_even_at_zero() {
    // A oneof arm has explicit presence: being active is being set. The Go
    // plugin overwrites a zero-valued scalar arm here; this one does not.
    let mut test = Test {
        oneof: Some(test::Oneof::Four(EnumValue::Known(test::Type::NONE))),
        ..Default::default()
    };
    test.set_defaults();
    assert_eq!(
        test.oneof,
        Some(test::Oneof::Four(EnumValue::Known(test::Type::NONE)))
    );
}

#[test]
fn recurses_into_a_set_message_field_without_initializing_an_unset_one() {
    let mut test = Test::default();
    test.set_defaults();
    // `{recurse: true}` alone: nothing to recurse into.
    assert!(test.message_field.is_unset());

    let mut test = Test {
        message_field: MessageField::some(Test::default()),
        ..Default::default()
    };
    test.set_defaults();
    assert_eq!(test.message_field.string_field, "string_field");
    assert_eq!(test.message_field.number_field, 42);
}

#[test]
fn initializes_a_foreign_message_it_cannot_recurse_into() {
    let mut test = Test::default();
    test.set_defaults();
    assert!(test.descriptor.is_set());
}

#[test]
fn never_overwrites_a_set_field() {
    let mut test = Test {
        string_field: "mine".to_owned(),
        number_field: 7,
        enum_field: EnumValue::Known(test::Type::ONE),
        bytes: b"x".to_vec(),
        ..Default::default()
    };
    test.number_value_field.get_or_insert_default().value = 0;
    test.time_value_field.get_or_insert_default().seconds = 1;
    test.set_defaults();

    assert_eq!(test.string_field, "mine");
    assert_eq!(test.number_field, 7);
    assert_eq!(test.enum_field, EnumValue::Known(test::Type::ONE));
    assert_eq!(test.bytes, b"x");
    // Set to the zero value, but set: presence is the wrapper's.
    assert_eq!(test.number_value_field.value, 0);
    assert_eq!(test.time_value_field.seconds, 1);
}

#[test]
fn fills_only_absent_optional_fields() {
    let mut optional = TestOptional {
        number_field: Some(0),
        bool_field: Some(false),
        ..Default::default()
    };
    optional.set_defaults();

    assert_eq!(optional.string_field.as_deref(), Some("string_field"));
    assert_eq!(
        optional.enum_field,
        Some(EnumValue::Known(
            defaults_fixture::proto::tests::test_optional::Type::TWO
        ))
    );
    // Explicitly set to the zero value: left alone.
    assert_eq!(optional.number_field, Some(0));
    assert_eq!(optional.bool_field, Some(false));
}

#[test]
fn an_unexported_message_is_defaulted_through_its_hand_written_impl() {
    let mut unexported = TestUnexported::default();
    SetDefaults::set_defaults(&mut unexported);
    assert_eq!(unexported.string_field.as_deref(), Some("string_field"));
    assert_eq!(unexported.number_field, Some(42));
}

#[test]
fn a_skipped_message_gets_no_generated_method() {
    // `OneOfOne` is `skip`: its only `set_defaults` is the fixture's. The
    // generated ones fill "string_field".
    let mut one = OneOfOne::default();
    one.set_defaults();
    assert_eq!(one.string_field, "hand-written");

    let mut two = OneOfTwo::default();
    two.set_defaults();
    assert_eq!(two.string_field, "string_field");
    let mut three = OneOfThree::default();
    three.set_defaults();
    assert_eq!(three.string_field, "string_field");
}
