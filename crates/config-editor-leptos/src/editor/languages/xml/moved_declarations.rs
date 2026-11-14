//! An element a view moved keeps its namespaces: no view shows a
//! declaration or a prefix, so a save declares on the moved element
//! itself what the host bound its names to (`namespace_scopes`), instead
//! of waiting on a prefix the user cannot see (QA `E36-3`: `<c:scan>` cut
//! from `<p xmlns:c="…">` and pasted under `<q>`) or silently moving it
//! to another namespace (QA `E37-6`: `<t>` pasted under
//! `<q xmlns="urn:d">`, or `<c:t>` under `<q xmlns:c="urn:other">`).

use super::attribute_layout::Tag;
use super::identity::{Host, Identity};
use super::namespace_scopes::{Binding, HostNamespaces, InScope, declaration, declared, used};
use super::*;

/// What the host texts bind each prefix to: for each qualified name
/// (`c:scan`, `d:a`) the namespaces in scope where the host uses it,
/// and for each prefix the namespaces its declarations bind it to.
#[derive(Default)]
struct Bindings {
    by_name: HashMap<String, Vec<String>>,
    by_prefix: HashMap<String, Vec<String>>,
}

/// `namespace` noted among `known`, once.
fn note(known: &mut Vec<String>, namespace: &str) {
    if !known.iter().any(|known| known == namespace) {
        known.push(namespace.to_string());
    }
}

impl Bindings {
    fn of(hosts: &[&str]) -> Self {
        let mut bindings = Bindings::default();
        for host in hosts {
            crate::editor::work_count::note(crate::editor::work_count::Work::NamespaceScan);
            let mut reader = Reader::from_str(host);
            let mut scopes: Vec<Vec<(String, String)>> = Vec::new();
            loop {
                let (element, empty) = match reader.read_event() {
                    Ok(Event::Start(element)) => (element, false),
                    Ok(Event::Empty(element)) => (element, true),
                    Ok(Event::End(_)) => {
                        scopes.pop();
                        continue;
                    }
                    Ok(Event::Eof) | Err(_) => break,
                    Ok(_) => continue,
                };
                let declared = declarations_of(&element);
                for (prefix, namespace) in &declared {
                    note(bindings.by_prefix.entry(prefix.clone()).or_default(), namespace);
                }
                scopes.push(declared);
                for qualified in prefixed_names(&element) {
                    let prefix = qualified.split(':').next().unwrap_or_default();
                    let bound = scopes.iter().rev().flatten().find(|(name, _)| name == prefix);
                    if let Some((_, namespace)) = bound {
                        note(bindings.by_name.entry(qualified.clone()).or_default(), namespace);
                    }
                }
                if empty {
                    scopes.pop();
                }
            }
        }
        bindings
    }

    /// The one namespace the hosts bind `qualified` in, else its prefix
    /// to; `None` where they bind it to several (which one an element
    /// copied from one of them means is the user's to say).
    fn namespace(&self, qualified: &str) -> Option<&str> {
        let prefix = qualified.split(':').next().unwrap_or_default();
        let known = self.by_name.get(qualified).or_else(|| self.by_prefix.get(prefix))?;
        match known.as_slice() {
            [one] => Some(one),
            _ => None,
        }
    }
}

/// The `xmlns:p` declarations of a start tag: each prefix with its
/// namespace as written.
fn declarations_of(element: &BytesStart<'_>) -> Vec<(String, String)> {
    element
        .attributes()
        .flatten()
        .filter_map(|attribute| {
            let key: &str = attribute.key.as_ref();
            let prefix = key.strip_prefix("xmlns:")?;
            Some((prefix.to_string(), attribute.value.to_string()))
        })
        .collect()
}

/// The element's and its attributes' names with a prefix other than
/// `xml` and `xmlns`, element first.
fn prefixed_names(element: &BytesStart<'_>) -> Vec<String> {
    let name = element.name();
    let element_name = name.prefix().map(|_| name.as_ref().to_string());
    let attributes = element.attributes().flatten().filter_map(|attribute| {
        attribute.key.prefix().map(|_| attribute.key.as_ref().to_string())
    });
    element_name
        .into_iter()
        .chain(attributes)
        .filter(|qualified| !qualified.starts_with("xml:") && !qualified.starts_with("xmlns:"))
        .collect()
}

/// `saved` (a save's text, written from the value `identity` edits the
/// host to) with each element keeping the namespaces its names had in the
/// host (`previous`, or `origin` for an item pasted back from the text the
/// session began on): one moved into or out of a default namespace, or
/// where its prefix is bound otherwise or not at all, declares its own
/// right after its name (QA `E36-3`, `E37-6`). An element the edit added
/// declares a prefix no declaration in scope binds as the hosts bind it;
/// a prefix no host binds is left for the read-back to refuse.
pub(super) fn declared_where_moved(saved: String, (previous, origin): (&str, Option<&str>), identity: &Identity<'_>) -> String {
    if !previous.contains("xmlns") && !origin.is_some_and(|origin| origin.contains("xmlns")) {
        return saved;
    }
    crate::editor::work_count::note(crate::editor::work_count::Work::NamespaceScan);
    let hosts: Vec<&str> = std::iter::once(previous).chain(origin.filter(|origin| *origin != previous)).collect();
    let (mut host, mut first, mut bindings) = (None, None, None);
    let markup = declarations::Markup::of(&saved);
    let tokens = tokenize(&saved);
    let plan = plan_paths(&saved, &tokens);
    let mut inserts: Vec<(usize, String)> = Vec::new();
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
        let tag = Tag::of(&saved, token, &markup);
        let own = declared(&saved, &tag);
        let owns = |prefix: &Option<String>| own.iter().any(|(declared, _)| declared == prefix);
        let wanted = plan.path_of(token.start).and_then(|path| identity.host_path(path)).and_then(|(path, side)| {
            let read = match side {
                Host::Previous => &*host.get_or_insert_with(|| HostNamespaces::of(previous)),
                Host::Origin => first.get_or_insert_with(|| origin.map(HostNamespaces::of)).as_ref()?,
            };
            read.at(&path).map(<[Binding]>::to_vec)
        });
        scope.push(own.clone());
        let mut added: Vec<Binding> = Vec::new();
        match wanted {
            // A host element: the namespaces its names had there.
            Some(wanted) => added.extend(scope.missing(&wanted).filter(|(prefix, _)| !owns(prefix)).cloned()),
            // Added: a prefix nothing in scope binds, as the hosts bind it.
            None => {
                for (prefix, qualified) in used(&saved, token, &tag) {
                    let Some(prefix) = prefix.filter(|prefix| scope.bound(Some(prefix)).is_none()) else { continue };
                    if added.iter().any(|(declared, _)| declared.as_deref() == Some(prefix)) {
                        continue;
                    }
                    let known = bindings.get_or_insert_with(|| Bindings::of(&hosts));
                    if let Some(namespace) = known.namespace(qualified) {
                        added.push((Some(prefix.to_string()), namespace.to_string()));
                    }
                }
            }
        }
        if !added.is_empty() {
            inserts.push((tag.name_end, added.iter().map(declaration).collect()));
        }
        for binding in added {
            scope.add(binding);
        }
        if saved[..token.end].ends_with("/>") {
            scope.pop();
        }
    }
    if inserts.is_empty() {
        return saved;
    }
    let mut out = String::with_capacity(saved.len() + inserts.iter().map(|(_, text)| text.len()).sum::<usize>());
    let mut from = 0;
    for (at, text) in inserts {
        out.push_str(&saved[from..at]);
        out.push_str(&text);
        from = at;
    }
    out.push_str(&saved[from..]);
    out
}

#[cfg(test)]
mod tests {
    use super::super::identity::Identity;
    use super::declared_where_moved;
    use serde_json::Value;

    /// Each element is the host's at its own path (as for a host that does
    /// not read); one at a path the host has none is added.
    fn by_path(saved: &str, hosts: (&str, Option<&str>)) -> String {
        declared_where_moved(saved.to_string(), hosts, &Identity::of(None, &Value::Null))
    }

    #[test]
    fn a_prefix_out_of_its_scope_is_declared_as_the_host_binds_it() {
        let host = "<r>\n  <p xmlns:c=\"urn:c\">\n    <c:s a=\"1\"/>\n  </p>\n  <q xmlns:c=\"urn:other\"/>\n  <t/>\n</r>\n";
        // Moved out: declared, bound as where the host used `c:s`.
        let saved = "<r>\n  <p/>\n  <t>\n    <c:s a=\"1\"/>\n  </t>\n</r>\n";
        assert_eq!(by_path(saved, (host, None)), "<r>\n  <p/>\n  <t>\n    <c:s xmlns:c=\"urn:c\" a=\"1\"/>\n  </t>\n</r>\n");
        // A prefixed attribute and a child under it: declared once, on the
        // outermost element needing it.
        let one_binding = "<r><p xmlns:c=\"urn:c\"><c:s/></p></r>";
        let saved = "<r><t c:x=\"1\"><c:y>2</c:y></t></r>";
        assert_eq!(by_path(saved, (one_binding, None)), "<r><t xmlns:c=\"urn:c\" c:x=\"1\"><c:y>2</c:y></t></r>");
        // Still in scope, a prefix no host binds, or one the host binds to
        // several namespaces (where it never writes the name): as it was.
        for saved in ["<r><p xmlns:c=\"urn:c\"><c:s/></p></r>", "<r><e:s/></r>", "<r xml:lang=\"en\"/>", "<r><c:y/></r>"] {
            assert_eq!(by_path(saved, (host, None)), saved);
        }
        // A host with no declaration is not read.
        assert_eq!(by_path("<r><c:s/></r>", ("<r/>", None)), "<r><c:s/></r>");
    }

    /// A host element whose names the save would put in another namespace
    /// (its default one, or a prefix bound otherwise) declares the host's;
    /// its children then are in theirs again.
    #[test]
    fn a_host_element_keeps_its_namespaces() {
        for (host, saved, expected) in [
            ("<r><q xmlns=\"urn:d\"><z/></q></r>", "<r><q><z/></q></r>", "<r><q xmlns=\"urn:d\"><z/></q></r>"),
            (
                "<r xmlns=\"urn:r\"><q><z/></q></r>",
                "<r xmlns=\"urn:r\"><q xmlns=\"urn:d\"><z/></q></r>",
                "<r xmlns=\"urn:r\"><q xmlns=\"urn:d\"><z xmlns=\"urn:r\"/></q></r>",
            ),
            (
                "<r><q><z/></q><s xmlns=\"urn:s\"/></r>",
                "<r xmlns=\"urn:x\"><q><z/></q><s xmlns=\"urn:s\"/></r>",
                "<r xmlns=\"urn:x\"><q xmlns=\"\"><z/></q><s xmlns=\"urn:s\"/></r>",
            ),
            (
                "<r xmlns:c=\"urn:c\"><c:q c:a=\"1\"/></r>",
                "<r xmlns:c=\"urn:o\"><c:q c:a=\"1\"/></r>",
                "<r xmlns:c=\"urn:o\"><c:q xmlns:c=\"urn:c\" c:a=\"1\"/></r>",
            ),
            ("<r xmlns:c=\"urn:c\"><c:q/></r>", "<r xmlns:c=\"urn:c\"><c:q/></r>", "<r xmlns:c=\"urn:c\"><c:q/></r>"),
            ("<r xmlns:c='a\"b'><c:q/></r>", "<r><c:q/></r>", "<r><c:q xmlns:c='a\"b'/></r>"),
        ] {
            assert_eq!(by_path(saved, (host, None)), expected, "{host}");
        }
    }

    /// A save reads the saved text once and each host text at most once,
    /// however many elements it declares a namespace on.
    #[test]
    fn the_texts_are_read_once_at_any_size() {
        use crate::editor::work_count::{Work, counted};
        let scans = |count: usize| {
            let elements: String = (0..count).map(|at| format!("<c:s{at}/>")).collect();
            let host = format!("<r><p xmlns:c=\"urn:c\">{elements}</p><q xmlns=\"urn:d\"/></r>");
            let saved = format!("<r><p/><q>{elements}</q></r>");
            let (out, scans) = counted(Work::NamespaceScan, || by_path(&saved, (&host, Some(&host))));
            assert_eq!(out.matches("xmlns:c=\"urn:c\"").count(), count);
            assert!(out.contains("<q xmlns=\"urn:d\">"), "{out}");
            scans
        };
        assert_eq!(scans(10), 3);
        assert_eq!(scans(400), 3);
    }
}
