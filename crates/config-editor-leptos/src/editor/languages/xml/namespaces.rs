//! Prefixes no namespace declaration binds (`<s x:a="1">` with no
//! `xmlns:x` on it or an ancestor): no namespace-aware XML reader takes
//! such a document, so the reader refuses it as a syntax error, in the
//! host's own view (its save waits) and in another view's save that
//! removes a declaration its prefixes still need.

use super::*;

/// The prefixes each open element declares, innermost last.
#[derive(Default)]
pub(super) struct Scopes(Vec<Vec<String>>);

impl Scopes {
    /// Enters the start tag `element`: the first element or attribute name
    /// in it whose prefix no declaration in scope binds (`xml` is always
    /// bound), and whether it is an attribute's.
    pub(super) fn enter(&mut self, element: &BytesStart<'_>) -> Option<(String, bool)> {
        let declared: Vec<String> = element
            .attributes()
            .flatten()
            .filter_map(|attribute| {
                let key: &str = attribute.key.as_ref();
                key.strip_prefix("xmlns:").map(str::to_string)
            })
            .collect();
        self.0.push(declared);
        let bound = |prefix: &str| {
            prefix == "xml"
                || prefix == "xmlns"
                || self.0.iter().flatten().any(|name| name == prefix)
        };
        let name = element.name();
        if let Some(prefix) = name.prefix()
            && !bound(prefix.as_ref())
        {
            return Some((name.as_ref().to_string(), false));
        }
        element.attributes().flatten().find_map(|attribute| {
            let prefix = attribute.key.prefix()?;
            (!bound(prefix.as_ref())).then(|| (attribute.key.as_ref().to_string(), true))
        })
    }

    /// Leaves the innermost element.
    pub(super) fn leave(&mut self) {
        self.0.pop();
    }
}

/// Why a document using `qualified`'s prefix unbound is refused, at
/// `pointer`: the same words in every view.
pub(super) fn unbound(qualified: &str, pointer: &str) -> String {
    let prefix = qualified.split(':').next().unwrap_or_default();
    format!(
        "the prefix {prefix} of {qualified} is bound to no namespace (no xmlns:{prefix} declares it), which XML readers refuse: declare it, or name it without the prefix (at {pointer})"
    )
}

#[cfg(test)]
mod tests {
    use super::super::xml_parse;

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
            assert!(xml_parse(text).is_ok(), "{text}");
        }
        let error = xml_parse("<r a=\"1\">\n  <s x:attr=\"3\">t</s></r>").unwrap_err();
        assert_eq!(
            error.message,
            "the prefix x of x:attr is bound to no namespace (no xmlns:x declares it), which XML readers refuse: declare it, or name it without the prefix (at /r/s/x:attr)"
        );
        assert_eq!((error.line, error.column), (Some(2), Some(6)));
        // Validated without an editor, the same problem.
        let problems = crate::check_syntax(crate::Language::XML, "<r a=\"1\">\n  <s x:attr=\"3\">t</s></r>");
        assert_eq!(problems.len(), 1);
        assert_eq!((problems[0].message.as_str(), problems[0].line), (error.message.as_str(), Some(2)));
        // Out of the declaring element's scope.
        let error = xml_parse("<r><s xmlns:x=\"urn:a\"/><x:c>1</x:c></r>").unwrap_err();
        assert!(error.message.ends_with("(at /r/c)"), "{}", error.message);
        assert_eq!((error.line, error.column), (Some(1), Some(25)));
    }
}
