//! An element's written name: the model keys elements by their local name
//! (`element` of `<xs:element>`), so a rewrite names each element as the
//! host element it is (`attribute_layout`), and an element the edit added
//! as its siblings, in their namespace: a save never moves an element to
//! another namespace (QA `E32-1`: an item added to `<mvc:interceptors>`
//! saved every `<mvc:interceptor>` as `<interceptor>`). One named as none
//! of its host siblings takes their prefix, else its parent's
//! ([`Children`]).

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

/// The child elements of each host element, by its path: the prefix of
/// each child's name that is in scope for a new child (one the child does
/// not declare on itself), in order.
#[derive(Default)]
pub(super) struct Children(HashMap<Vec<String>, Vec<Option<String>>>);

impl Children {
    /// Notes the host element at `path`, named with `prefix`, which
    /// `declares_own` when its start tag binds that prefix itself.
    pub(super) fn note(&mut self, path: &[Seg], prefix: Option<&str>, declares_own: bool) {
        let children = self.0.entry(strings(parent(path))).or_default();
        if !declares_own {
            children.push(prefix.map(str::to_string));
        }
    }

    /// The prefix of an element with a name new among its siblings (QA
    /// `E33-3`): its host siblings' (the first's in scope), in the
    /// namespace the parent's children are in, else the parent's own
    /// (`parent_prefix`) when it has none in scope.
    pub(super) fn new_name_prefix<'a>(&'a self, parent_host: Option<&[String]>, parent_prefix: Option<&'a str>) -> Option<&'a str> {
        match parent_host.and_then(|parent| self.0.get(parent)).and_then(|children| children.first()) {
            Some(sibling) => sibling.as_deref(),
            None => parent_prefix,
        }
    }
}

/// The path of the element holding the element at `path`.
pub(super) fn parent(path: &[Seg]) -> &[Seg] {
    let key = path.iter().rposition(|segment| matches!(segment, Seg::Key(_))).unwrap_or(0);
    &path[..key]
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
