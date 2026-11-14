//! An element's written name: the model keys elements by their local name
//! (`element` of `<xs:element>`), so a rewrite names each element as the
//! host element it is (`attribute_layout`), and an element the edit added
//! as its siblings, in their namespace: a save never moves an element to
//! another namespace (QA `E32-1`: an item added to `<mvc:interceptors>`
//! saved every `<mvc:interceptor>` as `<interceptor>`).

use super::*;

/// The prefix of the name of the start or end tag `token` of `source`
/// (`xs` of `<xs:element>`).
pub(super) fn prefix<'a>(source: &'a str, token: &Token) -> Option<&'a str> {
    let start = match token.kind {
        Kind::EndTag => token.start + 2,
        _ => token.start + 1,
    };
    let local = local_name_start(source, token);
    (local > start + 1).then(|| &source[start..local - 1])
}

/// Whether the attribute written `name` declares the namespace of an
/// element whose name has `prefix` (`xmlns:a` for `<a:item>`, `xmlns` for
/// `<item>`).
pub(super) fn binds(prefix: Option<&str>, name: &str) -> bool {
    match prefix {
        Some(prefix) => name.strip_prefix("xmlns:") == Some(prefix),
        None => name == "xmlns",
    }
}

#[cfg(test)]
mod tests {
    use super::super::tokenize;
    use super::{binds, prefix};

    #[test]
    fn a_tag_names_its_prefix_and_a_declaration_binds_it() {
        let source = "<xs:a xmlns:xs=\"u\"><b/></xs:a>";
        let prefixes: Vec<Option<&str>> = tokenize(source)
            .iter()
            .filter(|token| matches!(token.kind, super::Kind::OpenTag { closing: false, .. } | super::Kind::EndTag))
            .map(|token| prefix(source, token))
            .collect();
        assert_eq!(prefixes, [Some("xs"), None, Some("xs")]);
        assert!(binds(Some("xs"), "xmlns:xs") && binds(None, "xmlns"));
        assert!(!binds(Some("xs"), "xmlns") && !binds(None, "xmlns:xs") && !binds(Some("x"), "xmlns:xs"));
    }
}
