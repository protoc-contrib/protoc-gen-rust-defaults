//! Types named where buffa put them: nested messages, enums and oneofs, a
//! message whose module is a keyword, and enums and messages in another
//! package. A wrong path is a compile error, so most of this file's work is
//! done by its compiling; the assertions check the paths lead to the right
//! values.

use buffa::{EnumValue, MessageField};
use defaults_fixture::proto::example::v1::{Outer, Type, outer, r#type};
use defaults_fixture::proto::other::v1::{Shelf, Tier};

#[test]
fn fills_a_nested_message_with_its_own_enum_and_oneof() {
    let mut message = Outer::default();
    message.set_defaults();

    let inner = message.inner.as_option().expect("initialized");
    assert_eq!(
        inner.color,
        EnumValue::Known(outer::inner::Color::COLOR_BLUE)
    );
    assert_eq!(
        inner.choice,
        Some(outer::inner::Choice::Label("inner".to_owned()))
    );
}

#[test]
fn reaches_another_packages_enum_and_message() {
    let mut message = Outer::default();
    message.set_defaults();

    assert_eq!(message.tier, EnumValue::Known(Tier::TIER_GOLD));
    assert_eq!(message.shelf.aisle, "A1");
    assert_eq!(message.shelf.tier, EnumValue::Known(Tier::TIER_SILVER));
}

#[test]
fn initializes_a_message_without_defaults_and_stops_there() {
    let mut message = Outer::default();
    message.set_defaults();
    assert!(message.plain.is_set());
    assert_eq!(message.plain.note, "");
}

#[test]
fn recurses_into_a_set_message_from_another_package() {
    let mut message = Outer {
        shelf: MessageField::some(Shelf {
            aisle: "Z9".to_owned(),
            ..Default::default()
        }),
        ..Default::default()
    };
    message.set_defaults();
    assert_eq!(message.shelf.aisle, "Z9");
    assert_eq!(message.shelf.tier, EnumValue::Known(Tier::TIER_SILVER));
}

#[test]
fn names_a_oneof_inside_a_keyword_module() {
    let mut message = Type::default();
    message.set_defaults();
    assert_eq!(message.kind, Some(r#type::Kind::Type(5)));
}
