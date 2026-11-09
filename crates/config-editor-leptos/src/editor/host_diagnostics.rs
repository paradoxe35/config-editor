//! Problems the host reports (a server's validation): [`HostDiagnostic`],
//! placed in the text by path or range and shown with the editor's own.

use super::diagnostics::Severity;
use super::paths::{FieldPath, TextRange};

mod fields;
mod merge;
mod place;
mod targets;
pub(super) use merge::merge;
pub(super) use place::{RangeText, place};
pub(super) use targets::HostTargets;

/// Where a [`HostDiagnostic`] points.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DiagnosticTarget {
    /// A field, wherever the document writes it: the same in every language,
    /// so it follows the field through edits and `edit_as`.
    Path(FieldPath),
    /// A stretch of the host's text (in the host's `language`), over one line
    /// or several. While editing as another language it marks the field the
    /// range starts in (unplaced when it starts in none).
    Range(TextRange),
    /// The whole document: shown in the footer.
    Document,
}

/// A problem the host found, such as a server's validation error, shown next
/// to the editor's own diagnostics and marked as the host's.
///
/// ```
/// use config_editor_leptos::{HostDiagnostic, Severity};
/// let problem = HostDiagnostic::at_path("/web/listen", Severity::Error, "port 80 is taken")
///     .with_code("E_PORT")
///     .with_source("burrow")
///     .with_docs_url("https://example.com/docs/ports");
/// assert_eq!(problem.code.as_deref(), Some("E_PORT"));
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct HostDiagnostic {
    /// What the problem is about.
    pub target: DiagnosticTarget,
    /// How serious it is.
    pub severity: Severity,
    /// What is wrong.
    pub message: String,
    /// A machine-readable code, shown with the message.
    pub code: Option<String>,
    /// Who reported it (`"burrow"`, `"server"`), shown with the message;
    /// `"host"` when unset.
    pub source: Option<String>,
    /// A page explaining the problem, linked from its message.
    pub docs_url: Option<String>,
}

impl HostDiagnostic {
    /// A problem with the field at `path` (a JSON pointer or a [`FieldPath`]).
    pub fn at_path(
        path: impl Into<FieldPath>,
        severity: Severity,
        message: impl Into<String>,
    ) -> Self {
        Self::new(DiagnosticTarget::Path(path.into()), severity, message)
    }

    /// A problem in `range` of the host's text.
    pub fn at_range(range: TextRange, severity: Severity, message: impl Into<String>) -> Self {
        Self::new(DiagnosticTarget::Range(range), severity, message)
    }

    /// A problem with the whole document.
    pub fn document(severity: Severity, message: impl Into<String>) -> Self {
        Self::new(DiagnosticTarget::Document, severity, message)
    }

    fn new(target: DiagnosticTarget, severity: Severity, message: impl Into<String>) -> Self {
        HostDiagnostic {
            target,
            severity,
            message: message.into(),
            code: None,
            source: None,
            docs_url: None,
        }
    }

    /// With a code.
    #[must_use]
    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }

    /// With who reported it.
    #[must_use]
    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    /// With a docs link.
    #[must_use]
    pub fn with_docs_url(mut self, url: impl Into<String>) -> Self {
        self.docs_url = Some(url.into());
        self
    }
}

#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod fields_tests;
#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml",
    feature = "schema-validation"
))]
mod merge_tests;
#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod range_tests;
#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod scale_tests;
#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod tests;

/// Whether `url` is an `http:` or `https:` URL with a host: the only
/// links a host diagnostic's documentation may open (`"http://"` alone,
/// or `"https:///path"`, is not one).
#[must_use]
pub fn is_web_url(url: &str) -> bool {
    let lower = url.trim_start().to_ascii_lowercase();
    let Some(rest) = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"))
    else {
        return false;
    };
    // The host: up to the path, query or fragment, past any `user@`.
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = authority.rsplit('@').next().unwrap_or_default();
    let host = host.split(':').next().unwrap_or_default();
    !host.is_empty() && !host.contains(char::is_whitespace)
}
#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod batch_tests;
#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod paths_at_tests;
#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod switch_tests;
