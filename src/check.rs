//! Reads one field's `(protoc.contrib.defaults.value)` into the [`Value`] the
//! emitter assigns, checking it against the field it annotates on the way.
//!
//! The checks, and the wording of every error, are protoc-gen-go-defaults'
//! (`internal/generator/checker.go`): a schema that one plugin rejects the
//! other rejects too, with the same message. An annotation that does not fit
//! its field fails `buf generate` here, rather than compiling to code that
//! silently does something else.

use buffa_codegen::generated::descriptor::{FieldDescriptorProto, field_descriptor_proto};
use field_descriptor_proto::{Label, Type};

use crate::literal::{self, Instant, Timestamp};
use crate::messages::Schema;
use protoc_types::contrib::defaults::{FieldDefaults, field_defaults};

/// A scalar default, typed as the field holds it.
#[derive(Debug, Clone, PartialEq)]
pub enum Scalar {
    Float(f32),
    Double(f64),
    Int32(i32),
    Int64(i64),
    Uint32(u32),
    Uint64(u64),
    Bool(bool),
    String(String),
    Bytes(Vec<u8>),
}

/// What a field defaults to, resolved against the field.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// A scalar, for a field of that scalar type or its
    /// `google.protobuf.*Value` wrapper.
    Scalar(Scalar),
    /// An enum value, by the path of its enum and the value's name.
    Enum {
        /// The Rust path of the enum type.
        path: String,
        /// The value's name, which buffa keeps verbatim as the variant.
        variant: String,
        /// Whether buffa types the field as the bare enum rather than
        /// `EnumValue<E>` -- a closed enum, in proto2 or editions.
        closed: bool,
    },
    Duration(Instant),
    /// A fixed `Timestamp`, resolved at generation time.
    Timestamp(Instant),
    /// `timestamp = "now"`, read off the clock when the defaults are applied.
    Now {
        /// The Rust path of the field's `Timestamp` type, which converts from
        /// a `SystemTime`.
        path: String,
    },
    /// `message = { initialize, recurse }`. `recurse` is already narrowed to
    /// the targets that have a `set_defaults` to call; see
    /// [`Schema::recurses_into`].
    Message {
        initialize: bool,
        recurse: bool,
    },
}

const DURATION: &str = "google.protobuf.Duration";
const TIMESTAMP: &str = "google.protobuf.Timestamp";
const ANY: &str = "google.protobuf.Any";

/// Resolves `defaults` against `field`, declared in `package`.
///
/// Returns `None` when the annotation sets no value at all.
///
/// # Errors
///
/// protoc-gen-go-defaults' message for the first way the annotation does not
/// fit the field.
pub(crate) fn value(
    field: &FieldDescriptorProto,
    defaults: &FieldDefaults,
    package: &str,
    schema: &Schema<'_>,
) -> Result<Option<Value>, String> {
    use field_defaults::Type as Default;

    let Some(default) = &defaults.r#type else {
        return Ok(None);
    };
    if field.label == Some(Label::LABEL_REPEATED) {
        return Err("defaults are not supported on repeated or map fields".to_owned());
    }

    if let Some(scalar) = scalar(default, field)? {
        return Ok(Some(Value::Scalar(scalar)));
    }
    let kind = field.r#type.unwrap_or(Type::TYPE_MESSAGE);
    let value = match default {
        Default::Enum(number) => {
            let enum_type = enum_type(field)?;
            // Compared as Go compares it: the option is a `uint32`, so a
            // negative enum number is spelled as its two's-complement bits.
            let name = schema
                .enum_value(enum_type, |value| {
                    value.number.map(i32::cast_unsigned) == Some(*number)
                })
                .ok_or_else(|| format!("enum value {number} is not defined in {enum_type}"))?;
            enum_value(enum_type, name, package, schema)
        }
        Default::EnumName(name) => {
            let enum_type = enum_type(field)?;
            let name = schema
                .enum_value(enum_type, |value| {
                    value.name.as_deref() == Some(name.as_str())
                })
                .ok_or_else(|| format!("enum value {name:?} is not defined in {enum_type}"))?;
            enum_value(enum_type, name, package, schema)
        }
        Default::Duration(text) => {
            if message_type(field) != Some(DURATION) {
                return Err(format!(
                    "duration default requires {DURATION}, got {}",
                    describe(field)
                ));
            }
            let duration = literal::duration(text)
                .map_err(|error| format!("invalid duration {text:?}: {error}"))?;
            Value::Duration(duration)
        }
        Default::Timestamp(text) => {
            if message_type(field) != Some(TIMESTAMP) {
                return Err(format!(
                    "timestamp default requires {TIMESTAMP}, got {}",
                    describe(field)
                ));
            }
            match literal::timestamp(text)
                .map_err(|error| format!("invalid timestamp {text:?}: {error}"))?
            {
                Timestamp::At(instant) => Value::Timestamp(instant),
                Timestamp::Now => Value::Now {
                    path: schema.type_path(TIMESTAMP, package),
                },
            }
        }
        Default::Message(message) => {
            if kind != Type::TYPE_MESSAGE {
                return Err(format!(
                    "message default requires a message-typed field, got {}",
                    kind_name(kind)
                ));
            }
            match message_type(field) {
                Some(ANY) => return Err(format!("{ANY} fields are not supported")),
                Some(DURATION) => {
                    return Err(format!(
                        "use (defaults.value).duration on {DURATION} fields"
                    ));
                }
                Some(TIMESTAMP) => {
                    return Err(format!(
                        "use (defaults.value).timestamp on {TIMESTAMP} fields"
                    ));
                }
                _ => {}
            }
            let target = message_type(field).unwrap_or_default();
            Value::Message {
                initialize: message.initialize.unwrap_or(false),
                recurse: message.recurse.unwrap_or(false) && schema.recurses_into(target),
            }
        }
        // Every scalar was read above.
        _ => return Ok(None),
    };
    Ok(Some(value))
}

/// Reads a scalar default, checked against the field's kind -- or, for the
/// kinds that have one, its `google.protobuf.*Value` wrapper. `None` for a
/// default that is not a scalar.
fn scalar(
    default: &field_defaults::Type,
    field: &FieldDescriptorProto,
) -> Result<Option<Scalar>, String> {
    use field_defaults::Type as Default;

    let (scalar, kind, wrapper) = match default {
        Default::Float(v) => (Scalar::Float(*v), Type::TYPE_FLOAT, Some("FloatValue")),
        Default::Double(v) => (Scalar::Double(*v), Type::TYPE_DOUBLE, Some("DoubleValue")),
        Default::Int32(v) => (Scalar::Int32(*v), Type::TYPE_INT32, Some("Int32Value")),
        Default::Int64(v) => (Scalar::Int64(*v), Type::TYPE_INT64, Some("Int64Value")),
        Default::Uint32(v) => (Scalar::Uint32(*v), Type::TYPE_UINT32, Some("UInt32Value")),
        Default::Uint64(v) => (Scalar::Uint64(*v), Type::TYPE_UINT64, Some("UInt64Value")),
        Default::Bool(v) => (Scalar::Bool(*v), Type::TYPE_BOOL, Some("BoolValue")),
        Default::String(v) => (
            Scalar::String(v.clone()),
            Type::TYPE_STRING,
            Some("StringValue"),
        ),
        Default::Bytes(v) => (
            Scalar::Bytes(v.clone()),
            Type::TYPE_BYTES,
            Some("BytesValue"),
        ),
        Default::Sint32(v) => (Scalar::Int32(*v), Type::TYPE_SINT32, None),
        Default::Sint64(v) => (Scalar::Int64(*v), Type::TYPE_SINT64, None),
        Default::Fixed32(v) => (Scalar::Uint32(*v), Type::TYPE_FIXED32, None),
        Default::Fixed64(v) => (Scalar::Uint64(*v), Type::TYPE_FIXED64, None),
        Default::Sfixed32(v) => (Scalar::Int32(*v), Type::TYPE_SFIXED32, None),
        Default::Sfixed64(v) => (Scalar::Int64(*v), Type::TYPE_SFIXED64, None),
        _ => return Ok(None),
    };
    match wrapper {
        Some(wrapper) => {
            expect_kind_or_wrapper(field, kind, &format!("google.protobuf.{wrapper}"))?;
        }
        None => expect_kind(field.r#type.unwrap_or(Type::TYPE_MESSAGE), kind)?,
    }
    Ok(Some(scalar))
}

fn enum_type(field: &FieldDescriptorProto) -> Result<&str, String> {
    let kind = field.r#type.unwrap_or(Type::TYPE_MESSAGE);
    if kind != Type::TYPE_ENUM {
        return Err(format!("expected enum kind, got {}", kind_name(kind)));
    }
    field
        .type_name
        .as_deref()
        .map(|name| name.trim_start_matches('.'))
        .ok_or_else(|| "enum field has no enum descriptor".to_owned())
}

fn enum_value(enum_type: &str, variant: &str, package: &str, schema: &Schema<'_>) -> Value {
    Value::Enum {
        path: schema.type_path(enum_type, package),
        variant: variant.to_owned(),
        closed: schema.is_enum_closed(enum_type),
    }
}

fn expect_kind(got: Type, want: Type) -> Result<(), String> {
    if got == want {
        return Ok(());
    }
    Err(format!(
        "expected {}, got {}",
        kind_name(want),
        kind_name(got)
    ))
}

fn expect_kind_or_wrapper(
    field: &FieldDescriptorProto,
    want: Type,
    wrapper: &str,
) -> Result<(), String> {
    if field.r#type == Some(want) || message_type(field) == Some(wrapper) {
        return Ok(());
    }
    Err(format!(
        "expected {} or {wrapper}, got {}",
        kind_name(want),
        describe(field)
    ))
}

/// The full name of a message-typed field's message, as Go's
/// `wellKnownType` reads it -- for any message, well-known or not.
fn message_type(field: &FieldDescriptorProto) -> Option<&str> {
    if field.r#type != Some(Type::TYPE_MESSAGE) {
        return None;
    }
    field
        .type_name
        .as_deref()
        .map(|name| name.trim_start_matches('.'))
}

/// A field's type as an error message names it: the message's full name for a
/// message field, the kind otherwise.
fn describe(field: &FieldDescriptorProto) -> String {
    match message_type(field) {
        Some(name) => name.to_owned(),
        None => kind_name(field.r#type.unwrap_or(Type::TYPE_MESSAGE)).to_owned(),
    }
}

/// A kind as Go's `protoreflect.Kind` prints it.
fn kind_name(kind: Type) -> &'static str {
    match kind {
        Type::TYPE_DOUBLE => "double",
        Type::TYPE_FLOAT => "float",
        Type::TYPE_INT64 => "int64",
        Type::TYPE_UINT64 => "uint64",
        Type::TYPE_INT32 => "int32",
        Type::TYPE_FIXED64 => "fixed64",
        Type::TYPE_FIXED32 => "fixed32",
        Type::TYPE_BOOL => "bool",
        Type::TYPE_STRING => "string",
        Type::TYPE_GROUP => "group",
        Type::TYPE_MESSAGE => "message",
        Type::TYPE_BYTES => "bytes",
        Type::TYPE_UINT32 => "uint32",
        Type::TYPE_ENUM => "enum",
        Type::TYPE_SFIXED32 => "sfixed32",
        Type::TYPE_SFIXED64 => "sfixed64",
        Type::TYPE_SINT32 => "sint32",
        Type::TYPE_SINT64 => "sint64",
    }
}
