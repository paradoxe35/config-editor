//! Which namespace each name of an element is in: its own name (in the
//! default namespace when it has no prefix) and each prefixed attribute's.
//! No view shows a declaration, so an element a view moved under another
//! parent must keep the namespaces its names had in the host
//! (`moved_declarations` declares them on it, `kept_markup` checks it).

use super::attribute_layout::Tag;
use super::*;

/// A namespace binding: a prefix (`None`: the default namespace) and the
/// namespace it binds, as written between the quotes (empty: none,
/// `xmlns=""`).
pub(super) type Binding = (Option<String>, String);

/// The namespace `xml:` names are in, bound without a declaration.
const XML_NAMESPACE: &str = "http://www.w3.org/XML/1998/namespace";

/// The bindings the declarations among `tag`'s attributes make.
pub(super) fn declared(source: &str, tag: &Tag) -> Vec<Binding> {
    tag.attributes
        .iter()
        .filter_map(|attribute| {
            let value = &source[attribute.value.clone()];
            binding(&source[attribute.name.clone()], value.get(1..value.len().saturating_sub(1)).unwrap_or_default())
        })
        .collect()
}

/// The binding the attribute `name="value"` (unquoted) makes, if it is a
/// declaration.
pub(super) fn binding(name: &str, value: &str) -> Option<Binding> {
    let prefix = match name.strip_prefix("xmlns")? {
        "" => None,
        rest => Some(rest.strip_prefix(':')?.to_string()),
    };
    Some((prefix, value.to_string()))
}

/// The declaration making `binding` (` xmlns="urn:a"`, ` xmlns:c="urn:c"`).
pub(super) fn declaration(binding: &Binding) -> String {
    let quote = if binding.1.contains('"') { '\'' } else { '"' };
    match &binding.0 {
        Some(prefix) => format!(" xmlns:{prefix}={quote}{}{quote}", binding.1),
        None => format!(" xmlns={quote}{}{quote}", binding.1),
    }
}

/// The prefixes the names of the start tag `token` (read as `tag`) use: its
/// own (`None` when unprefixed, in the default namespace), then each
/// prefixed attribute's but `xml` and `xmlns`; each with the name using it.
pub(super) fn used<'a>(source: &'a str, token: &Token, tag: &Tag) -> Vec<(Option<&'a str>, &'a str)> {
    let start = token.start + 1;
    let element = (written_names::prefix(source, token), &source[start..tag.name_end]);
    let attributes = tag.attributes.iter().filter_map(|attribute| {
        let name = &source[attribute.name.clone()];
        let (prefix, _) = name.split_once(':')?;
        (prefix != "xml" && prefix != "xmlns").then_some((Some(prefix), name))
    });
    std::iter::once(element).chain(attributes).collect()
}

/// The bindings in scope while a text is walked, innermost last.
#[derive(Default)]
pub(super) struct InScope(Vec<Vec<Binding>>);

impl InScope {
    /// The namespace `prefix` is bound to (`None`: the default one, which is
    /// none, `""`, until declared), `None` where nothing binds it.
    pub(super) fn bound(&self, prefix: Option<&str>) -> Option<&str> {
        let declared = self.0.iter().rev().flat_map(|frame| frame.iter().rev()).find(|(bound, _)| bound.as_deref() == prefix);
        match (declared, prefix) {
            (Some((_, namespace)), _) => Some(namespace),
            (None, None) => Some(""),
            (None, Some("xml")) => Some(XML_NAMESPACE),
            (None, Some(_)) => None,
        }
    }

    /// Enters an element declaring `bindings`.
    pub(super) fn push(&mut self, bindings: Vec<Binding>) {
        self.0.push(bindings);
    }

    /// Adds `binding` to the innermost element's.
    pub(super) fn add(&mut self, binding: Binding) {
        if let Some(frame) = self.0.last_mut() {
            frame.push(binding);
        }
    }

    /// Leaves the innermost element.
    pub(super) fn pop(&mut self) {
        self.0.pop();
    }

    /// What this scope binds the names of the start tag `token` (read as
    /// `tag`, entered) to: its own name's first, then its prefixed
    /// attributes', each once; a prefix nothing binds is left out.
    pub(super) fn names_bound(&self, source: &str, token: &Token, tag: &Tag) -> Vec<Binding> {
        let mut bindings: Vec<Binding> = Vec::new();
        for (prefix, _) in used(source, token, tag) {
            let Some(namespace) = self.bound(prefix) else { continue };
            let binding = (prefix.map(str::to_string), namespace.to_string());
            if !bindings.contains(&binding) {
                bindings.push(binding);
            }
        }
        bindings
    }

    /// Of `wanted` (the bindings an element's names had in the host), those
    /// this scope binds otherwise.
    pub(super) fn missing<'w>(&self, wanted: &'w [Binding]) -> impl Iterator<Item = &'w Binding> {
        wanted.iter().filter(|(prefix, namespace)| self.bound(prefix.as_deref()) != Some(namespace.as_str()))
    }
}

/// The namespaces of each host element's names, by its path (as
/// `identity` names host elements): its own name's first, then its
/// prefixed attributes', each once; a prefix nothing binds is left out.
pub(super) struct HostNamespaces(HashMap<Vec<String>, Vec<Binding>>);

impl HostNamespaces {
    pub(super) fn of(text: &str) -> Self {
        crate::editor::work_count::note(crate::editor::work_count::Work::NamespaceScan);
        let markup = declarations::Markup::of(text);
        let tokens = tokenize(text);
        let plan = plan_paths(text, &tokens);
        let mut by_path = HashMap::new();
        let mut scope = InScope::default();
        for token in &tokens {
            match token.kind {
                Kind::EndTag => {
                    scope.pop();
                    continue;
                }
                Kind::OpenTag { closing: false, .. } => {}
                _ => continue,
            }
            let tag = Tag::of(text, token, &markup);
            scope.push(declared(text, &tag));
            if let Some(path) = plan.path_of(token.start) {
                by_path.insert(strings(path), scope.names_bound(text, token, &tag));
            }
            if text[..token.end].ends_with("/>") {
                scope.pop();
            }
        }
        HostNamespaces(by_path)
    }

    pub(super) fn at(&self, path: &[String]) -> Option<&[Binding]> {
        self.0.get(path).map(Vec::as_slice)
    }
}

#[cfg(test)]
mod tests {
    use super::HostNamespaces;

    #[test]
    fn each_element_names_the_namespaces_its_names_are_in() {
        let text = "<r xmlns:c=\"urn:c\">\n  <p xmlns=\"urn:d\">\n    <t c:a=\"1\" b=\"2\" xml:lang=\"en\"/>\n  </p>\n  <c:q xmlns:c='urn:o'><z/></c:q>\n  <e:u/>\n</r>\n";
        let read = HostNamespaces::of(text);
        let at = |path: &[&str]| read.at(&path.iter().map(|segment| segment.to_string()).collect::<Vec<_>>()).map(<[_]>::to_vec);
        let binding = |prefix: Option<&str>, namespace: &str| (prefix.map(str::to_string), namespace.to_string());
        assert_eq!(at(&["r"]), Some(vec![binding(None, "")]));
        assert_eq!(at(&["r", "p", "t"]), Some(vec![binding(None, "urn:d"), binding(Some("c"), "urn:c")]));
        assert_eq!(at(&["r", "q"]), Some(vec![binding(Some("c"), "urn:o")]));
        assert_eq!(at(&["r", "q", "z"]), Some(vec![binding(None, "")]));
        // A prefix nothing binds: left out.
        assert_eq!(at(&["r", "u"]), Some(vec![]));
    }
}
