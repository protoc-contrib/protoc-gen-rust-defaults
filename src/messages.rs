//! Walks the messages of every file being generated and reads their
//! `protoc.contrib.defaults` options into the [`Message`]s the emitter turns
//! into `set_defaults` methods.
//!
//! Every message is checked, including the ones that end up with no method --
//! a `skip`ped message's annotations still have to fit its fields, as in
//! protoc-gen-go-defaults, so dropping `skip` later cannot surface an error
//! nobody saw when it was written.

use std::collections::HashMap;

use anyhow::{Result, anyhow};
use buffa::ExtensionSet;
use buffa_codegen::{
    CodeGenConfig,
    context::CodeGenContext,
    generated::{
        compiler::CodeGeneratorRequest,
        descriptor::{
            DescriptorProto, EnumDescriptorProto, EnumValueDescriptorProto, FieldDescriptorProto,
            field_descriptor_proto::Type,
        },
    },
};

use crate::check::{self, Value};
use crate::presence::Scope;
use protoc_types::contrib::defaults::{ONEOF, SKIP, UNEXPORTED, VALUE};

/// A message that gets a generated method.
#[derive(Debug)]
pub struct Message {
    /// The message's full proto name, without the leading dot.
    pub fqn: String,
    /// The proto package it is declared in, which its output file is named
    /// for.
    pub package: String,
    /// Its Rust path, from that package's module.
    pub path: String,
    /// Which method the defaults are emitted as.
    pub method: Method,
    /// What to default, in field declaration order.
    pub items: Vec<Item>,
}

/// The method a message's defaults are emitted as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    /// `pub fn set_defaults`.
    SetDefaults,
    /// `(protoc.contrib.defaults.unexported)`: only `pub(crate) fn
    /// apply_defaults`, for a hand-written `set_defaults` to wrap.
    ApplyDefaults,
}

/// One thing a method defaults.
#[derive(Debug)]
pub enum Item {
    Field(Field),
    Oneof(Oneof),
}

/// A field outside any oneof, and what it defaults to.
#[derive(Debug)]
pub struct Field {
    /// The proto field name; the emitter makes it an ident the way buffa does.
    pub name: String,
    /// How buffa types the field, which decides what "unset" means.
    pub shape: Shape,
    pub value: Value,
}

/// How buffa types a field with a default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// The bare value: implicit presence, or required. Unset is the zero
    /// value.
    Bare,
    /// `Option<T>`: explicit presence. Unset is `None`.
    Optional,
    /// `MessageField<T>`. Unset is no message.
    Message,
}

/// A oneof with a default arm or arms to recurse into.
#[derive(Debug)]
pub struct Oneof {
    /// The proto oneof name; the struct field holding it.
    pub name: String,
    /// The Rust path of buffa's enum for it, from the package module.
    pub path: String,
    /// The arm `(protoc.contrib.defaults.oneof)` names, set when the oneof is.
    pub default: Option<Arm>,
    /// The proto names of the message arms whose `set_defaults` is called
    /// when active.
    pub recurse: Vec<String>,
}

/// The arm a unset oneof is set to.
#[derive(Debug)]
pub struct Arm {
    /// The proto field name, which buffa makes the variant.
    pub name: String,
    /// Whether the arm holds a message, which is created empty and filled
    /// with the arm's default, rather than a scalar assigned it outright.
    pub message: bool,
    /// The arm's own default, if annotated.
    pub value: Option<Value>,
}

/// The schema-wide lookups a field's default is resolved through.
pub struct Schema<'a> {
    /// Every message in the request, by full name, with its package.
    messages: HashMap<String, (&'a DescriptorProto, &'a str)>,
    /// Every enum in the request, by full name, with its package.
    enums: HashMap<String, (&'a EnumDescriptorProto, &'a str)>,
    /// buffa's own type-path resolution, so an enum or message is named here
    /// exactly as buffa named it -- extern paths and all.
    context: CodeGenContext<'a>,
    /// Where buffa's module tree is mounted, when the output is mounted in a
    /// parallel tree of its own; `None` when it is included into buffa's.
    proto_module: Option<&'a str>,
}

impl<'a> Schema<'a> {
    /// Indexes `request` for resolving defaults against.
    ///
    /// `proto_module` is where buffa's tree is mounted when the output goes
    /// into a module tree of its own, and `None` when it is included into
    /// buffa's.
    #[must_use]
    pub fn new(
        request: &'a CodeGeneratorRequest,
        config: &'a CodeGenConfig,
        proto_module: Option<&'a str>,
    ) -> Self {
        let mut messages = HashMap::new();
        let mut enums = HashMap::new();
        for file in &request.proto_file {
            let package = file.package.as_deref().unwrap_or_default();
            for message in &file.message_type {
                index(message, package, package, &mut messages, &mut enums);
            }
            for enumeration in &file.enum_type {
                enums.insert(
                    join(package, name(enumeration.name.as_deref())),
                    (enumeration, package),
                );
            }
        }
        Self {
            messages,
            enums,
            context: CodeGenContext::for_generate(
                &request.proto_file,
                &request.file_to_generate,
                config,
            ),
            proto_module,
        }
    }

    /// The Rust path of the type `fqn` names, written from the module of
    /// package `from`.
    ///
    /// A type in the same package is named relative to it, the way buffa
    /// names it, which resolves both in buffa's module and in a parallel
    /// tree that glob-imports it. A type in another package is reached from
    /// `proto_module` in a parallel tree, since that tree only has modules
    /// for packages with defaults in them, and with buffa's own `super` hops
    /// otherwise. An extern type is named by its absolute path either way.
    pub(crate) fn type_path(&self, fqn: &str, from: &str) -> String {
        let package = self.package_of(fqn).unwrap_or_default();
        // buffa keys its type map by the dotted form a descriptor's
        // `type_name` uses.
        let dotted = format!(".{fqn}");
        let absolute = self.context.rust_type(&dotted).unwrap_or(fqn);
        let external = absolute.starts_with("::") || absolute.starts_with("crate::");
        if let (Some(root), false, false) = (self.proto_module, external, package == from) {
            return format!("{root}::{absolute}");
        }
        self.context
            .rust_type_relative(&dotted, from, 0)
            .unwrap_or_else(|| absolute.to_owned())
    }

    /// The name of the first value of `enum_type` that `matches`.
    pub(crate) fn enum_value(
        &self,
        enum_type: &str,
        matches: impl Fn(&EnumValueDescriptorProto) -> bool,
    ) -> Option<&'a str> {
        self.enums
            .get(enum_type)?
            .0
            .value
            .iter()
            .find(|value| matches(value))
            .and_then(|value| value.name.as_deref())
    }

    /// Whether buffa types a field of `enum_type` as the bare enum, rather
    /// than `EnumValue<E>`.
    pub(crate) fn is_enum_closed(&self, enum_type: &str) -> bool {
        self.context
            .is_enum_closed(&format!(".{enum_type}"))
            .unwrap_or(false)
    }

    /// Whether a `recurse` into a field of message type `fqn` has an inherent
    /// `set_defaults` to call.
    ///
    /// It does when the message has defaults of its own, so this plugin
    /// generates one, and when it is `skip` or `unexported`, which say a
    /// hand-written one takes the generated one's place -- and a missing one
    /// is a compile error rather than a recursion silently skipped. Anything else -- a
    /// message without defaults, a well-known or foreign type -- has nothing
    /// to call, and recursing into it is a no-op, as it is in
    /// protoc-gen-go-defaults.
    pub(crate) fn recurses_into(&self, fqn: &str) -> bool {
        self.messages.get(fqn).is_some_and(|(message, _)| {
            has_defaults(message) || flag(message, &SKIP) || flag(message, &UNEXPORTED)
        })
    }

    fn package_of(&self, fqn: &str) -> Option<&'a str> {
        self.messages
            .get(fqn)
            .map(|(_, package)| *package)
            .or_else(|| self.enums.get(fqn).map(|(_, package)| *package))
    }

    /// The Rust path of buffa's enum for the oneof `name` of message `fqn`,
    /// from the package module: the oneof's `PascalCase` name, in the module
    /// buffa re-exports it from -- the message's own module of nested items.
    fn oneof_path(&self, fqn: &str, package: &str, name: &str) -> String {
        let message = self.type_path(fqn, package);
        let (parent, short) = match message.rsplit_once("::") {
            Some((parent, short)) => (format!("{parent}::"), short),
            None => (String::new(), message.as_str()),
        };
        // A message's module is renamed when it collides with a sub-package
        // of the same name, and buffa records the name it chose against the
        // message's name within its package. One it recorded nothing for is
        // the plain `snake_case` of the message's own name.
        let within = fqn
            .strip_prefix(package)
            .and_then(|rest| rest.strip_prefix('.'))
            .unwrap_or(fqn);
        let recorded = self.context.nested_module_name(package, within);
        let module = if recorded.contains('.') {
            crate::idents::module(short)
        } else {
            buffa_codegen::idents::escape_mod_ident(&recorded)
        };
        format!(
            "{parent}{module}::{}",
            buffa_codegen::idents::make_field_ident(&crate::idents::pascal_case(name))
        )
    }
}

/// Gathers the messages in `request.file_to_generate` that get a method.
///
/// # Errors
///
/// If a file scheduled for generation is not in the request, or if an
/// annotation does not fit what it annotates: protoc-gen-go-defaults' message,
/// prefixed with the message and field or oneof it was found on.
pub fn gather(request: &CodeGeneratorRequest, schema: &Schema<'_>) -> Result<Vec<Message>> {
    let mut out = Vec::new();
    for name in &request.file_to_generate {
        let file = request
            .proto_file
            .iter()
            .find(|file| file.name.as_deref() == Some(name.as_str()))
            .ok_or_else(|| anyhow!("{name} is scheduled for generation but not in the request"))?;
        let package = file.package.as_deref().unwrap_or_default();
        let scope = Scope::file(file);
        for message in &file.message_type {
            walk(message, package, package, scope, schema, &mut out)?;
        }
    }
    Ok(out)
}

fn walk(
    message: &DescriptorProto,
    package: &str,
    prefix: &str,
    scope: Scope,
    schema: &Schema<'_>,
    out: &mut Vec<Message>,
) -> Result<()> {
    if message
        .options
        .as_option()
        .and_then(|options| options.map_entry)
        .unwrap_or(false)
    {
        return Ok(());
    }
    let fqn = join(prefix, name(message.name.as_deref()));
    let scope = scope.message(message);

    let items = items(message, &fqn, package, scope, schema)?;
    if !flag(message, &SKIP) && has_defaults(message) {
        out.push(Message {
            path: schema.type_path(&fqn, package),
            package: package.to_owned(),
            method: if flag(message, &UNEXPORTED) {
                Method::ApplyDefaults
            } else {
                Method::SetDefaults
            },
            items,
            fqn: fqn.clone(),
        });
    }

    for nested in &message.nested_type {
        walk(nested, package, &fqn, scope, schema, out)?;
    }
    Ok(())
}

fn items(
    message: &DescriptorProto,
    fqn: &str,
    package: &str,
    scope: Scope,
    schema: &Schema<'_>,
) -> Result<Vec<Item>> {
    let mut items = Vec::new();
    let mut oneofs_seen = Vec::new();

    for field in &message.field {
        let field_name = name(field.name.as_deref());
        let value = match field_defaults(field) {
            Some(defaults) => check::value(field, &defaults, package, schema)
                .map_err(|error| anyhow!("{fqn}: field {field_name}: {error}"))?,
            None => None,
        };

        if let Some(index) = real_oneof(field) {
            if oneofs_seen.contains(&index) {
                continue;
            }
            oneofs_seen.push(index);
            if let Some(oneof) = oneof(message, index, fqn, package, schema)? {
                items.push(Item::Oneof(oneof));
            }
            continue;
        }

        if let Some(value) = value {
            let shape = if field.r#type == Some(Type::TYPE_MESSAGE) {
                Shape::Message
            } else if scope.is_option(field) {
                Shape::Optional
            } else {
                Shape::Bare
            };
            items.push(Item::Field(Field {
                name: field_name.to_owned(),
                shape,
                value,
            }));
        }
    }
    Ok(items)
}

/// Reads the oneof at `index` of `message`: the arm its option names, and
/// the arms to recurse into.
fn oneof(
    message: &DescriptorProto,
    index: i32,
    fqn: &str,
    package: &str,
    schema: &Schema<'_>,
) -> Result<Option<Oneof>> {
    let Some(decl) = usize::try_from(index)
        .ok()
        .and_then(|index| message.oneof_decl.get(index))
    else {
        return Ok(None);
    };
    let oneof_name = name(decl.name.as_deref());
    let members: Vec<&FieldDescriptorProto> = message
        .field
        .iter()
        .filter(|field| real_oneof(field) == Some(index))
        .collect();

    let mut default = None;
    if let Some(arm) = decl
        .options
        .as_option()
        .and_then(|options| options.extension(&ONEOF))
        .filter(|arm| !arm.is_empty())
    {
        let member = members
            .iter()
            .find(|field| name(field.name.as_deref()) == arm.as_str())
            .ok_or_else(|| {
                anyhow!(
                    "{fqn}: oneof {oneof_name}: (defaults.oneof) references unknown field {arm:?}"
                )
            })?;
        default = Some(Arm {
            name: arm.clone(),
            message: member.r#type == Some(Type::TYPE_MESSAGE),
            value: resolve(member, fqn, package, schema)?,
        });
    }

    let mut recurse = Vec::new();
    for member in &members {
        if let Some(Value::Message { recurse: true, .. }) = resolve(member, fqn, package, schema)? {
            recurse.push(name(member.name.as_deref()).to_owned());
        }
    }

    if default.is_none() && recurse.is_empty() {
        return Ok(None);
    }
    Ok(Some(Oneof {
        name: oneof_name.to_owned(),
        path: schema.oneof_path(fqn, package, oneof_name),
        default,
        recurse,
    }))
}

fn resolve(
    field: &FieldDescriptorProto,
    fqn: &str,
    package: &str,
    schema: &Schema<'_>,
) -> Result<Option<Value>> {
    let Some(defaults) = field_defaults(field) else {
        return Ok(None);
    };
    check::value(field, &defaults, package, schema)
        .map_err(|error| anyhow!("{fqn}: field {}: {error}", name(field.name.as_deref())))
}

/// The oneof `field` is a member of, unless that is the synthetic oneof of a
/// proto3 `optional` field.
fn real_oneof(field: &FieldDescriptorProto) -> Option<i32> {
    if field.proto3_optional.unwrap_or(false) {
        return None;
    }
    field.oneof_index
}

fn field_defaults(
    field: &FieldDescriptorProto,
) -> Option<protoc_types::contrib::defaults::FieldDefaults> {
    field
        .options
        .as_option()
        .and_then(|options| options.extension(&VALUE))
}

/// Whether any field of `message` sets a default, or any oneof names a default
/// arm: protoc-gen-go-defaults' test for whether a message gets a method.
fn has_defaults(message: &DescriptorProto) -> bool {
    message
        .field
        .iter()
        .any(|field| field_defaults(field).is_some_and(|defaults| defaults.r#type.is_some()))
        || message.oneof_decl.iter().any(|oneof| {
            oneof
                .options
                .as_option()
                .and_then(|options| options.extension(&ONEOF))
                .is_some_and(|arm| !arm.is_empty())
        })
}

fn flag(
    message: &DescriptorProto,
    extension: &buffa::Extension<buffa::extension::codecs::Bool>,
) -> bool {
    message
        .options
        .as_option()
        .and_then(|options| options.extension(extension))
        .unwrap_or(false)
}

fn index<'a>(
    message: &'a DescriptorProto,
    package: &'a str,
    prefix: &str,
    messages: &mut HashMap<String, (&'a DescriptorProto, &'a str)>,
    enums: &mut HashMap<String, (&'a EnumDescriptorProto, &'a str)>,
) {
    let fqn = join(prefix, name(message.name.as_deref()));
    for enumeration in &message.enum_type {
        enums.insert(
            join(&fqn, name(enumeration.name.as_deref())),
            (enumeration, package),
        );
    }
    for nested in &message.nested_type {
        index(nested, package, &fqn, messages, enums);
    }
    messages.insert(fqn, (message, package));
}

fn join(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_owned()
    } else {
        format!("{prefix}.{name}")
    }
}

/// A descriptor's name, which every descriptor in a request sets.
fn name(name: Option<&str>) -> &str {
    name.unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use buffa_codegen::generated::descriptor::{
        EnumValueDescriptorProto, FieldOptions, FileDescriptorProto, MessageOptions,
        OneofDescriptorProto, OneofOptions, field_descriptor_proto::Label,
    };
    use protoc_types::contrib::defaults::{
        FieldDefaults, MessageDefaults, field_defaults::Type as Default,
    };

    fn field(name: &str, kind: Type, default: Option<Default>) -> FieldDescriptorProto {
        let mut options = FieldOptions::default();
        if let Some(default) = default {
            options.set_extension(
                &VALUE,
                FieldDefaults {
                    r#type: Some(default),
                    ..FieldDefaults::default()
                },
            );
        }
        FieldDescriptorProto {
            name: Some(name.to_owned()),
            number: Some(1),
            r#type: Some(kind),
            label: Some(Label::LABEL_OPTIONAL),
            options: options.into(),
            ..FieldDescriptorProto::default()
        }
    }

    fn typed(mut field: FieldDescriptorProto, type_name: &str) -> FieldDescriptorProto {
        field.type_name = Some(type_name.to_owned());
        field
    }

    fn message(name: &str, fields: Vec<FieldDescriptorProto>) -> DescriptorProto {
        DescriptorProto {
            name: Some(name.to_owned()),
            field: fields,
            ..DescriptorProto::default()
        }
    }

    fn color() -> EnumDescriptorProto {
        EnumDescriptorProto {
            name: Some("Color".to_owned()),
            value: ["COLOR_UNSPECIFIED", "COLOR_RED"]
                .iter()
                .zip(0..)
                .map(|(name, number)| EnumValueDescriptorProto {
                    name: Some((*name).to_owned()),
                    number: Some(number),
                    ..EnumValueDescriptorProto::default()
                })
                .collect(),
            ..EnumDescriptorProto::default()
        }
    }

    /// Gathers a proto3 file of `messages` in package `pkg`, with an enum
    /// `pkg.Color` beside them.
    fn gather_file(messages: Vec<DescriptorProto>) -> Result<Vec<Message>> {
        let request = CodeGeneratorRequest {
            file_to_generate: vec!["pkg.proto".to_owned()],
            proto_file: vec![FileDescriptorProto {
                name: Some("pkg.proto".to_owned()),
                package: Some("pkg".to_owned()),
                syntax: Some("proto3".to_owned()),
                message_type: messages,
                enum_type: vec![color()],
                ..FileDescriptorProto::default()
            }],
            ..CodeGeneratorRequest::default()
        };
        let config = CodeGenConfig::default();
        let schema = Schema::new(&request, &config, None);
        gather(&request, &schema)
    }

    fn error(messages: Vec<DescriptorProto>) -> String {
        format!("{:#}", gather_file(messages).expect_err("generation fails"))
    }

    #[test]
    fn a_message_without_defaults_gets_no_method() {
        let messages = gather_file(vec![message(
            "Plain",
            vec![field("name", Type::TYPE_STRING, None)],
        )])
        .unwrap();
        assert!(messages.is_empty());
    }

    #[test]
    fn rejects_a_scalar_default_of_the_wrong_kind() {
        assert_eq!(
            error(vec![message(
                "Msg",
                vec![field("name", Type::TYPE_STRING, Some(Default::Int32(1)))]
            )]),
            "pkg.Msg: field name: expected int32 or google.protobuf.Int32Value, got string"
        );
        assert_eq!(
            error(vec![message(
                "Msg",
                vec![field("count", Type::TYPE_INT32, Some(Default::Sint32(1)))]
            )]),
            "pkg.Msg: field count: expected sint32, got int32"
        );
    }

    #[test]
    fn rejects_a_wrapper_of_the_wrong_kind() {
        let wrapper = typed(
            field("count", Type::TYPE_MESSAGE, Some(Default::Int32(1))),
            ".google.protobuf.Int64Value",
        );
        assert_eq!(
            error(vec![message("Msg", vec![wrapper])]),
            "pkg.Msg: field count: expected int32 or google.protobuf.Int32Value, got google.protobuf.Int64Value"
        );
    }

    #[test]
    fn rejects_a_duration_on_anything_but_a_duration() {
        assert_eq!(
            error(vec![message(
                "Msg",
                vec![field(
                    "name",
                    Type::TYPE_STRING,
                    Some(Default::Duration("1h".into()))
                )]
            )]),
            "pkg.Msg: field name: duration default requires google.protobuf.Duration, got string"
        );
    }

    #[test]
    fn rejects_an_unparseable_duration_or_timestamp() {
        let duration = typed(
            field(
                "ttl",
                Type::TYPE_MESSAGE,
                Some(Default::Duration("1x".into())),
            ),
            ".google.protobuf.Duration",
        );
        assert_eq!(
            error(vec![message("Msg", vec![duration])]),
            r#"pkg.Msg: field ttl: invalid duration "1x": unknown unit "x" in duration "1x""#
        );
        let timestamp = typed(
            field(
                "at",
                Type::TYPE_MESSAGE,
                Some(Default::Timestamp("soon".into())),
            ),
            ".google.protobuf.Timestamp",
        );
        assert!(
            error(vec![message("Msg", vec![timestamp])]).starts_with(
                r#"pkg.Msg: field at: invalid timestamp "soon": cannot parse timestamp"#
            )
        );
    }

    #[test]
    fn rejects_an_enum_value_the_enum_does_not_define() {
        let by_number = typed(
            field("color", Type::TYPE_ENUM, Some(Default::Enum(99))),
            ".pkg.Color",
        );
        assert_eq!(
            error(vec![message("Msg", vec![by_number])]),
            "pkg.Msg: field color: enum value 99 is not defined in pkg.Color"
        );
        let by_name = typed(
            field(
                "color",
                Type::TYPE_ENUM,
                Some(Default::EnumName("BOGUS".into())),
            ),
            ".pkg.Color",
        );
        assert_eq!(
            error(vec![message("Msg", vec![by_name])]),
            r#"pkg.Msg: field color: enum value "BOGUS" is not defined in pkg.Color"#
        );
    }

    #[test]
    fn rejects_a_default_on_a_repeated_field() {
        let mut repeated = field(
            "names",
            Type::TYPE_STRING,
            Some(Default::String("x".into())),
        );
        repeated.label = Some(Label::LABEL_REPEATED);
        assert_eq!(
            error(vec![message("Msg", vec![repeated])]),
            "pkg.Msg: field names: defaults are not supported on repeated or map fields"
        );
    }

    #[test]
    fn rejects_a_message_default_on_a_time_or_any() {
        let message_default = || {
            Some(Default::Message(Box::new(MessageDefaults {
                initialize: Some(true),
                ..MessageDefaults::default()
            })))
        };
        for (type_name, want) in [
            (
                ".google.protobuf.Duration",
                "use (defaults.value).duration on google.protobuf.Duration fields",
            ),
            (
                ".google.protobuf.Timestamp",
                "use (defaults.value).timestamp on google.protobuf.Timestamp fields",
            ),
            (
                ".google.protobuf.Any",
                "google.protobuf.Any fields are not supported",
            ),
        ] {
            let field = typed(field("f", Type::TYPE_MESSAGE, message_default()), type_name);
            assert_eq!(
                error(vec![message("Msg", vec![field])]),
                format!("pkg.Msg: field f: {want}")
            );
        }
        assert_eq!(
            error(vec![message(
                "Msg",
                vec![field("f", Type::TYPE_STRING, message_default())]
            )]),
            "pkg.Msg: field f: message default requires a message-typed field, got string"
        );
    }

    #[test]
    fn rejects_a_oneof_default_naming_no_member() {
        let mut member = field("one", Type::TYPE_STRING, None);
        member.oneof_index = Some(0);
        let mut options = OneofOptions::default();
        options.set_extension(&ONEOF, "missing".to_owned());
        let mut msg = message("Msg", vec![member]);
        msg.oneof_decl = vec![OneofDescriptorProto {
            name: Some("choice".to_owned()),
            options: options.into(),
            ..OneofDescriptorProto::default()
        }];
        assert_eq!(
            error(vec![msg]),
            r#"pkg.Msg: oneof choice: (defaults.oneof) references unknown field "missing""#
        );
    }

    #[test]
    fn checks_a_skipped_message_all_the_same() {
        let mut options = MessageOptions::default();
        options.set_extension(&SKIP, true);
        let mut msg = message(
            "Msg",
            vec![field("name", Type::TYPE_STRING, Some(Default::Bool(true)))],
        );
        msg.options = options.into();
        assert_eq!(
            error(vec![msg]),
            "pkg.Msg: field name: expected bool or google.protobuf.BoolValue, got string"
        );
    }

    #[test]
    fn an_unexported_message_gets_apply_defaults() {
        let mut options = MessageOptions::default();
        options.set_extension(&UNEXPORTED, true);
        let mut msg = message(
            "Msg",
            vec![field(
                "name",
                Type::TYPE_STRING,
                Some(Default::String("x".into())),
            )],
        );
        msg.options = options.into();
        let messages = gather_file(vec![msg]).unwrap();
        assert_eq!(messages[0].method, Method::ApplyDefaults);
    }

    #[test]
    fn recurses_only_into_a_message_with_a_set_defaults_to_call() {
        let recurse = || {
            Some(Default::Message(Box::new(MessageDefaults {
                recurse: Some(true),
                ..MessageDefaults::default()
            })))
        };
        let with = message(
            "With",
            vec![field(
                "name",
                Type::TYPE_STRING,
                Some(Default::String("x".into())),
            )],
        );
        let without = message("Without", vec![field("name", Type::TYPE_STRING, None)]);
        let parent = message(
            "Parent",
            vec![
                typed(field("with", Type::TYPE_MESSAGE, recurse()), ".pkg.With"),
                typed(
                    field("without", Type::TYPE_MESSAGE, recurse()),
                    ".pkg.Without",
                ),
            ],
        );
        let messages = gather_file(vec![with, without, parent]).unwrap();
        let parent = messages.iter().find(|m| m.fqn == "pkg.Parent").unwrap();
        let recursed: Vec<bool> = parent
            .items
            .iter()
            .map(|item| match item {
                Item::Field(Field {
                    value: Value::Message { recurse, .. },
                    ..
                }) => *recurse,
                other => panic!("unexpected {other:?}"),
            })
            .collect();
        assert_eq!(recursed, [true, false]);
    }
}
