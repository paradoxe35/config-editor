//! Prefixes no namespace declaration binds (`<s x:a="1">` with no
//! `xmlns:x` on it or an ancestor): the document reads, but no
//! namespace-aware XML reader takes it. Such a document does not convert,
//! so a save from another view that removes a declaration its prefixes
//! still need waits.

use super::*;

/// Why `source` does not convert: the first element or attribute name
/// whose prefix no `xmlns:` declaration in scope binds (`xml` is always
/// bound), at its pointer. `None` when every prefix is bound.
pub(super) fn unbound_prefix(source: &str) -> Option<String> {
    if !source.contains(':') {
        return None;
    }
    let mut reader = Reader::from_str(source);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    // Each open element: its local name and the prefixes it declares.
    let mut stack: Vec<(String, Vec<String>)> = Vec::new();
    loop {
        let (element, empty) = match reader.read_event_into(&mut buf).ok()? {
            Event::Eof => return None,
            Event::Start(element) => (element.into_owned(), false),
            Event::Empty(element) => (element.into_owned(), true),
            Event::End(_) => {
                stack.pop();
                buf.clear();
                continue;
            }
            _ => {
                buf.clear();
                continue;
            }
        };
        let declared: Vec<String> = element
            .attributes()
            .flatten()
            .filter_map(|attribute| {
                let key: &str = attribute.key.as_ref();
                key.strip_prefix("xmlns:").map(str::to_string)
            })
            .collect();
        let local = element.local_name().as_ref().to_string();
        stack.push((local, declared));
        let bound = |prefix: &str| {
            prefix == "xml"
                || prefix == "xmlns"
                || stack
                    .iter()
                    .any(|(_, declared)| declared.iter().any(|name| name == prefix))
        };
        let pointer = |last: Option<&str>| -> String {
            let names = stack.iter().map(|(name, _)| name.as_str()).chain(last);
            names
                .map(|name| format!("/{}", name.replace('~', "~0").replace('/', "~1")))
                .collect()
        };
        let name = element.name();
        if let Some(prefix) = name.prefix()
            && !bound(prefix.as_ref())
        {
            let qualified = name.as_ref().to_string();
            return Some(unbound(&qualified, &pointer(None)));
        }
        for attribute in element.attributes().flatten() {
            if let Some(prefix) = attribute.key.prefix()
                && !bound(prefix.as_ref())
            {
                let qualified = attribute.key.as_ref().to_string();
                return Some(unbound(&qualified, &pointer(Some(&qualified))));
            }
        }
        if empty {
            stack.pop();
        }
        buf.clear();
    }
}

fn unbound(qualified: &str, pointer: &str) -> String {
    let prefix = qualified.split(':').next().unwrap_or_default();
    format!(
        "the prefix {prefix} of {qualified} is bound to no namespace (no xmlns:{prefix} declares it), which XML readers refuse: declare it, or name it without the prefix (at {pointer})"
    )
}

#[cfg(test)]
mod tests {
    use super::unbound_prefix;

    #[test]
    fn a_prefix_needs_a_declaration_in_scope() {
        let bound = [
            "<r xmlns:x=\"urn:a\"><s x:attr=\"3\">t</s></r>",
            "<r><s xmlns:x=\"urn:a\" x:attr=\"3\"><x:c/></s></r>",
            "<x:r xmlns:x=\"urn:a\"><x:c>1</x:c></x:r>",
            "<r xml:lang=\"en\"><a>1</a></r>",
            "<r><a>no prefix: here</a></r>",
        ];
        for text in bound {
            assert_eq!(unbound_prefix(text), None, "{text}");
        }
        assert_eq!(
            unbound_prefix("<r a=\"1\"><s x:attr=\"3\">t</s></r>").as_deref(),
            Some(
                "the prefix x of x:attr is bound to no namespace (no xmlns:x declares it), which XML readers refuse: declare it, or name it without the prefix (at /r/s/x:attr)"
            )
        );
        // Out of the declaring element's scope.
        let after = "<r><s xmlns:x=\"urn:a\"/><x:c>1</x:c></r>";
        assert!(unbound_prefix(after).is_some_and(|reason| reason.ends_with("(at /r/c)")));
    }
}
