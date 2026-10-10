//! Generated defaults composed with hand-written ones, reached generically
//! through `protoc_contrib::defaults::SetDefaults` (`impl_trait`), and oneofs
//! whose default arm is a well-known type.

use buffa::MessageField;
use defaults_fixture::proto::example::v1::{Draft, Envelope, Frozen, envelope};
use protoc_contrib::defaults::SetDefaults;

/// What an interceptor does: apply the defaults of a message it knows only by
/// its trait.
fn apply<M: SetDefaults>(message: &mut M) {
    message.set_defaults();
}

#[test]
fn reaches_generated_defaults_through_the_trait() {
    let mut envelope = Envelope::default();
    apply(&mut envelope);
    assert_eq!(envelope.negative, -5);
}

#[test]
fn an_unexported_message_runs_the_generated_body_and_the_hand_written_one() {
    let mut draft = Draft::default();
    apply(&mut draft);
    assert_eq!(draft.title, "untitled");
    assert_eq!(draft.slug, "untitled");

    let mut draft = Draft {
        title: "Hello World".to_owned(),
        ..Default::default()
    };
    apply(&mut draft);
    assert_eq!(draft.slug, "hello-world");
}

#[test]
fn recursion_calls_the_hand_written_impls() {
    let mut envelope = Envelope {
        frozen: MessageField::some(Frozen::default()),
        ..Default::default()
    };
    envelope.set_defaults();
    // `initialize` + `recurse` into the unexported message.
    assert_eq!(envelope.draft.slug, "untitled");
    // `recurse` alone into the skipped one, which the caller set.
    assert_eq!(envelope.frozen.state, "hand-written");
}

#[test]
fn recursion_alone_does_not_initialize() {
    let mut envelope = Envelope::default();
    envelope.set_defaults();
    assert!(envelope.frozen.is_unset());
}

#[test]
fn fills_well_known_oneof_arms() {
    let mut envelope = Envelope::default();
    envelope.set_defaults();

    let Some(envelope::Payload::Ttl(ttl)) = &envelope.payload else {
        panic!("expected the `ttl` arm, got {:?}", envelope.payload);
    };
    assert_eq!((ttl.seconds, ttl.nanos), (5_400, 0));

    let Some(envelope::Stamp::Created(created)) = &envelope.stamp else {
        panic!("expected the `created` arm, got {:?}", envelope.stamp);
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    assert!(created.seconds.abs_diff(i64::try_from(now).unwrap()) <= 1);

    let Some(envelope::Wrapped::Count(count)) = &envelope.wrapped else {
        panic!("expected the `count` arm, got {:?}", envelope.wrapped);
    };
    assert_eq!(count.value, 9);
}

#[test]
fn leaves_a_oneof_the_caller_set_and_recurses_into_its_arm() {
    let mut envelope = Envelope {
        stamp: Some(envelope::Stamp::Label(String::new())),
        wrapped: Some(envelope::Wrapped::Other(Box::default())),
        ..Default::default()
    };
    envelope.set_defaults();
    assert_eq!(envelope.stamp, Some(envelope::Stamp::Label(String::new())));
    let Some(envelope::Wrapped::Other(other)) = &envelope.wrapped else {
        panic!("the caller's arm was replaced: {:?}", envelope.wrapped);
    };
    assert_eq!(other.slug, "untitled");
}

#[test]
fn spells_literals_rust_has_no_plain_number_for() {
    let mut envelope = Envelope::default();
    envelope.set_defaults();
    assert!(envelope.not_a_number.is_nan());
    assert_eq!(envelope.big, u64::MAX);
    assert_eq!(envelope.quoted, "say \"hi\"\n");
    assert_eq!(envelope.binary, [0x00, 0xff]);
    assert!((envelope.tiny - 1e-300).abs() < f64::EPSILON);
}
