//! An element moved out of the element declaring a prefix it uses (QA
//! `E36-3`: `<c:scan>` cut from `<p xmlns:c="…">` and pasted under
//! `<q>`): no view shows the declaration or the prefix, so a save
//! declares the prefix on the moved element itself, bound as the host
//! bound it, instead of waiting on a prefix the user cannot see.

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

/// `saved` (a save's text) with each start tag that uses a prefix no
/// declaration in scope binds declaring it, as `hosts` (the texts the
/// save is made over) bind it, right after its name. A prefix no host
/// binds is left for the read-back to refuse.
pub(super) fn declared_where_moved(saved: String, hosts: &[&str]) -> String {
    if !hosts.iter().any(|host| host.contains("xmlns:")) {
        return saved;
    }
    crate::editor::work_count::note(crate::editor::work_count::Work::NamespaceScan);
    let mut bindings: Option<Bindings> = None;
    let mut inserts: Vec<(usize, String)> = Vec::new();
    let mut reader = Reader::from_str(&saved);
    let mut scopes: Vec<Vec<String>> = Vec::new();
    loop {
        let event_start = reader.buffer_position() as usize;
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
        let mut declared: Vec<String> = declarations_of(&element).into_iter().map(|(prefix, _)| prefix).collect();
        let mut added = String::new();
        for qualified in prefixed_names(&element) {
            let prefix = qualified.split(':').next().unwrap_or_default().to_string();
            if declared.contains(&prefix) || scopes.iter().flatten().any(|bound| *bound == prefix) {
                continue;
            }
            let bindings = bindings.get_or_insert_with(|| Bindings::of(hosts));
            if let Some(namespace) = bindings.namespace(&qualified) {
                added.push_str(&format!(" xmlns:{prefix}=\"{namespace}\""));
                declared.push(prefix);
            }
        }
        if !added.is_empty() {
            let at = markup_start(&saved, event_start) + 1 + element.name().as_ref().len();
            inserts.push((at, added));
        }
        scopes.push(declared);
        if empty {
            scopes.pop();
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

/// The prefixes each open element of a saved text declares, for the
/// read-back check (`kept_markup`): which declarations of a start tag
/// are those [`declared_where_moved`] adds.
#[derive(Default)]
pub(super) struct Scopes(Vec<Vec<String>>);

impl Scopes {
    /// Enters a start tag whose markup attributes are `markup` (kept open
    /// when `opened`, not `<e/>`): its declarations of a prefix no
    /// element around it declares, to a namespace the host binds it to
    /// (`hosts_declare`), which a save adds to an element moved out of
    /// that prefix's scope.
    pub(super) fn enter(
        &mut self,
        markup: &[(String, String)],
        opened: bool,
        hosts_declare: &dyn Fn(&(String, String)) -> bool,
    ) -> Vec<(String, String)> {
        let mut own = Vec::new();
        let mut added = Vec::new();
        for declaration in markup {
            let Some(prefix) = declaration.0.strip_prefix("xmlns:") else { continue };
            own.push(prefix.to_string());
            if !self.0.iter().flatten().any(|bound| bound == prefix) && hosts_declare(declaration) {
                added.push(declaration.clone());
            }
        }
        if opened {
            self.0.push(own);
        }
        added
    }

    /// Leaves the innermost open element.
    pub(super) fn leave(&mut self) {
        self.0.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::declared_where_moved;

    #[test]
    fn a_prefix_out_of_its_scope_is_declared_as_the_host_binds_it() {
        let host = "<r>\n  <p xmlns:c=\"urn:c\">\n    <c:s a=\"1\"/>\n  </p>\n  <q xmlns:c=\"urn:other\"/>\n  <t/>\n</r>\n";
        // Moved out: declared, bound as where the host used `c:s`.
        let saved = "<r>\n  <p/>\n  <t>\n    <c:s a=\"1\"/>\n  </t>\n</r>\n";
        assert_eq!(
            declared_where_moved(saved.to_string(), &[host]),
            "<r>\n  <p/>\n  <t>\n    <c:s xmlns:c=\"urn:c\" a=\"1\"/>\n  </t>\n</r>\n"
        );
        // A prefixed attribute and a child under it: declared once, on the
        // outermost element needing it.
        let one_binding = "<r><p xmlns:c=\"urn:c\"><c:s/></p></r>";
        let saved = "<r><t c:x=\"1\"><c:y>2</c:y></t></r>";
        assert_eq!(
            declared_where_moved(saved.to_string(), &[one_binding]),
            "<r><t xmlns:c=\"urn:c\" c:x=\"1\"><c:y>2</c:y></t></r>"
        );
        // Still in scope, a prefix no host binds, or one the host binds to
        // several namespaces (where it never writes the name): as it was.
        for saved in ["<r><p xmlns:c=\"urn:c\"><c:s/></p></r>", "<r><e:s/></r>", "<r xml:lang=\"en\"/>", "<r><c:y/></r>"] {
            assert_eq!(declared_where_moved(saved.to_string(), &[host]), saved);
        }
        // A host with no declaration is not read.
        assert_eq!(declared_where_moved("<r><c:s/></r>".to_string(), &["<r/>"]), "<r><c:s/></r>");
    }

    /// A save reads the saved text once and each host text at most once,
    /// however many elements it declares a prefix on.
    #[test]
    fn the_texts_are_read_once_at_any_size() {
        use crate::editor::work_count::{Work, counted};
        let scans = |count: usize| {
            let elements: String = (0..count).map(|at| format!("<c:s{at}/>")).collect();
            let host = format!("<r><p xmlns:c=\"urn:c\">{elements}</p><q/></r>");
            let saved = format!("<r><p/><q>{elements}</q></r>");
            let (out, scans) = counted(Work::NamespaceScan, || declared_where_moved(saved, &[&host, &host]));
            assert_eq!(out.matches("xmlns:c=\"urn:c\"").count(), count);
            scans
        };
        assert_eq!(scans(10), 3);
        assert_eq!(scans(400), 3);
    }
}
