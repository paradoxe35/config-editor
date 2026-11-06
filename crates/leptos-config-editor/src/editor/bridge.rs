//! Editing a document in one language while the host keeps it in another
//! (`<ConfigEditor language=Language::Xml edit_as=Language::Hcl>`). Language
//! free: both directions go through the shared data model (`codec`), so
//! neither language knows about the other.
//!
//! The host's text arrives through `value`/`baseline`/`initial_value` and is
//! shown converted; edits convert back before `on_change`, keeping the host's
//! own document framing (`Codec::serialize`'s `previous`).

use super::Language;
use super::codec::{self, Conversion, ConvertError};
use super::diagnostics::{Diagnostic, Severity};
use super::language::LanguageBehavior;

#[derive(Clone, Copy)]
pub(super) struct Bridge {
    pub(super) host: Language,
    pub(super) edit: Language,
}

impl Bridge {
    /// A bridge when the editing language differs from the host's.
    pub(super) fn new(host: Language, edit: Language) -> Option<Self> {
        (host != edit).then_some(Bridge { host, edit })
    }

    /// The host's text shown in the editing language.
    pub(super) fn to_edit(self, host_text: &str) -> Result<Conversion, ConvertError> {
        if host_text.trim().is_empty() {
            return Ok(Conversion {
                text: String::new(),
                losses: Vec::new(),
            });
        }
        self.host.convert(host_text, self.edit)
    }

    /// The edited text in the host's language. `previous_host` is the host's
    /// last text, whose framing the host language may keep.
    pub(super) fn to_host(
        self,
        edit_text: &str,
        previous_host: &str,
    ) -> Result<Conversion, ConvertError> {
        if edit_text.trim().is_empty() {
            return Ok(Conversion {
                text: String::new(),
                losses: Vec::new(),
            });
        }
        codec::convert(
            edit_text,
            self.edit.behavior(),
            &self.host.side(),
            Some(previous_host),
        )
    }

    /// Diagnostics for a conversion back to the host: one warning per value
    /// the host language cannot carry exactly, placed on that value in the
    /// edited text; an error when the document cannot be saved at all. Syntax
    /// errors are already reported by validation, so they add nothing here.
    pub(super) fn problems(
        self,
        edit_text: &str,
        result: &Result<Conversion, ConvertError>,
    ) -> Vec<Diagnostic> {
        let host = self.host.label();
        match result {
            Ok(conversion) => conversion
                .losses
                .iter()
                .map(|pointer| {
                    located(
                        self.edit.behavior(),
                        edit_text,
                        pointer,
                        format!("{pointer}: {host} cannot keep this value exactly; it reads back differently"),
                        Severity::Warning,
                    )
                })
                .collect(),
            Err(ConvertError::Parse(_)) => Vec::new(),
            Err(ConvertError::Unsupported(message)) => vec![located(
                self.edit.behavior(),
                edit_text,
                "",
                format!("Not saved as {host}: {message}"),
                Severity::Error,
            )],
        }
    }

    /// A document-level error for host text that cannot be shown converted.
    pub(super) fn load_problem(self, error: &ConvertError) -> Diagnostic {
        Diagnostic {
            line: Some(1),
            column: Some(1),
            end_column: Some(2),
            message: format!(
                "Cannot edit this {} as {}: {error}",
                self.host.label(),
                self.edit.label()
            ),
            severity: Severity::Error,
        }
    }
}

/// A diagnostic on the value at `pointer` in `source`, or on the document's
/// first content when the value has no span.
fn located(
    behavior: &dyn LanguageBehavior,
    source: &str,
    pointer: &str,
    message: String,
    severity: Severity,
) -> Diagnostic {
    #[cfg(feature = "schema-validation")]
    let span = behavior
        .value_span_for_path(source, pointer)
        .or_else(|| behavior.span_for_path(source, pointer))
        .or_else(|| behavior.first_span(source));
    #[cfg(not(feature = "schema-validation"))]
    let span: Option<(usize, usize, usize)> = {
        let _ = (behavior, source, pointer);
        Some((1, 1, 2))
    };
    Diagnostic {
        line: span.map(|span| span.0),
        column: span.map(|span| span.1),
        end_column: span.map(|span| span.2),
        message,
        severity,
    }
}

#[cfg(test)]
mod tests {
    use super::super::Language;
    use super::super::diagnostics::Severity;
    use super::Bridge;

    #[test]
    fn the_same_language_needs_no_bridge() {
        assert!(Bridge::new(Language::Yaml, Language::Yaml).is_none());
    }

    #[test]
    fn host_text_is_shown_converted_and_edits_convert_back_keeping_framing() {
        let bridge = Bridge::new(Language::Xml, Language::Hcl).unwrap();
        let host = "<?xml version=\"1.0\"?>\n<burrow config-root=\"true\">\n  <listen>7835</listen>\n</burrow>\n";
        let shown = bridge.to_edit(host).unwrap();
        assert_eq!(shown.text, "listen = 7835\n");
        let saved = bridge.to_host("listen = 9000\n", host).unwrap();
        assert_eq!(
            saved.text,
            "<?xml version=\"1.0\"?>\n<burrow config-root=\"true\">\n  <listen>9000</listen>\n</burrow>\n"
        );
    }

    #[test]
    fn a_value_the_host_cannot_carry_is_a_warning_on_that_value() {
        let bridge = Bridge::new(Language::Xml, Language::Yaml).unwrap();
        let edit = "hosts:\n  - only.example\nport: 80\n";
        let result = bridge.to_host(edit, "");
        let problems = bridge.problems(edit, &result);
        assert_eq!(problems.len(), 1);
        assert_eq!(problems[0].severity, Severity::Warning);
        // Placed on the line of the key that owns the list.
        assert_eq!(problems[0].line, Some(1));
        assert!(
            problems[0].message.contains("XML cannot keep"),
            "{}",
            problems[0].message
        );
    }

    #[test]
    fn a_document_the_host_cannot_express_is_an_error_and_not_saved() {
        let bridge = Bridge::new(Language::Toml, Language::Json).unwrap();
        let edit = "{\n  \"https\": null\n}\n";
        let result = bridge.to_host(edit, "");
        assert!(result.is_err());
        let problems = bridge.problems(edit, &result);
        assert_eq!(problems[0].severity, Severity::Error);
        assert!(
            problems[0].message.starts_with("Not saved as TOML"),
            "{}",
            problems[0].message
        );
    }

    #[test]
    fn syntax_errors_are_left_to_validation() {
        let bridge = Bridge::new(Language::Json, Language::Yaml).unwrap();
        let edit = "a: [\n";
        let result = bridge.to_host(edit, "");
        assert!(bridge.problems(edit, &result).is_empty());
    }
}
