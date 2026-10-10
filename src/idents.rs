//! The naming rules generated code has to agree with buffa about.
//!
//! `buffa_codegen::idents` covers most of them — field idents, keyword
//! escaping — but not the `PascalCase` to `snake_case` transform buffa uses for
//! the module holding a nested message's items, nor the `snake_case` to
//! `PascalCase` one it names a oneof's enum and variants with. buffa keeps both
//! private, so they are written out here, once each.

/// `PascalCase` to `snake_case`, matching how buffa names a message's module.
///
/// An acronym stays one word: `HTTPServer` is `http_server`, not
/// `h_t_t_p_server`.
pub(crate) fn snake_case(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len() + 2);
    for (index, &c) in chars.iter().enumerate() {
        if c.is_uppercase() && index > 0 {
            let previous = chars[index - 1];
            let next_is_lower = chars.get(index + 1).is_some_and(char::is_ascii_lowercase);
            if previous.is_lowercase() || (previous.is_uppercase() && next_is_lower) {
                out.push('_');
            }
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

/// The module name buffa gives a message's nested items: [`snake_case`], then
/// escaped if that collides with a keyword.
pub(crate) fn module(name: &str) -> String {
    buffa_codegen::idents::escape_mod_ident(&snake_case(name))
}

/// `snake_case` to `PascalCase`, as buffa names a oneof's enum and its
/// variants: each `_`-separated part with its first letter upper-cased and the
/// rest kept as written.
pub(crate) fn pascal_case(name: &str) -> String {
    name.split('_')
        .map(|part| {
            let mut chars = part.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(chars).collect()
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowers_pascal_case_a_word_at_a_time() {
        assert_eq!(snake_case("Book"), "book");
        assert_eq!(snake_case("CreateBookRequest"), "create_book_request");
    }

    #[test]
    fn keeps_an_acronym_together() {
        assert_eq!(snake_case("HTTPServer"), "http_server");
        assert_eq!(snake_case("IDs"), "i_ds");
    }

    #[test]
    fn capitalizes_each_part_and_keeps_the_rest() {
        assert_eq!(pascal_case("oneof"), "Oneof");
        assert_eq!(pascal_case("detail_kind"), "DetailKind");
        assert_eq!(pascal_case("httpServer"), "HttpServer");
    }

    #[test]
    fn escapes_a_module_name_that_is_a_keyword() {
        // `Type` lowers to `type`, which cannot be a module name.
        assert_ne!(module("Type"), "type");
        assert_eq!(module("Book"), "book");
    }
}
