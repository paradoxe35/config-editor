//! Problems the host reports (a server's validation): [`HostDiagnostic`],
//! placed in the text by path or range and shown with the editor's own.

use serde_json::Value;

use super::diagnostics::{Diagnostic, DiagnosticOrigin, Severity};
use super::language::ConfigLanguage;
use super::paths::{FieldPath, TextRange, problem_span};

/// Where a [`HostDiagnostic`] points.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DiagnosticTarget {
    /// A field, wherever the document writes it: the same in every language,
    /// so it follows the field through edits and `edit_as`.
    Path(FieldPath),
    /// A stretch of the host's text (in the host's `language`). While editing
    /// as another language it cannot be placed, and shows unplaced.
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

/// `problems` as diagnostics on `source` (the text on screen, in `behavior`):
/// a path where the field is written, as schema problems are marked (the
/// value, or the key of a block); a range on its first line, when
/// `ranges_apply` (the text on screen is the host's). What cannot be placed
/// has no position and shows in the footer. Duplicates show once.
pub(super) fn place(
    behavior: &dyn ConfigLanguage,
    source: &str,
    problems: &[HostDiagnostic],
    ranges_apply: bool,
) -> Vec<Diagnostic> {
    if problems.is_empty() {
        return Vec::new();
    }
    let document = problems
        .iter()
        .any(|problem| matches!(problem.target, DiagnosticTarget::Path(_)))
        .then(|| behavior.parse(source).ok())
        .flatten();
    let mut placed: Vec<Diagnostic> = Vec::new();
    for problem in problems {
        let position = match &problem.target {
            DiagnosticTarget::Path(path) => {
                let pointer = path.to_pointer();
                let written_as_block = match document
                    .as_ref()
                    .and_then(|document| document.pointer(&pointer))
                {
                    Some(Value::Object(map)) => !map.is_empty(),
                    Some(Value::Array(items)) => !items.is_empty(),
                    _ => false,
                };
                if path.is_root() {
                    None
                } else {
                    problem_span(behavior, source, &pointer, false, written_as_block)
                        .map(|span| (span.line, span.start_column, span.end_column))
                }
            }
            DiagnosticTarget::Range(range) if ranges_apply => on_first_line(source, *range),
            DiagnosticTarget::Range(_) | DiagnosticTarget::Document => None,
        };
        let diagnostic = Diagnostic {
            line: position.map(|(line, _, _)| line),
            column: position.map(|(_, column, _)| column),
            end_column: position.map(|(_, _, end)| end),
            message: problem.message.clone(),
            severity: problem.severity,
            origin: DiagnosticOrigin::Host,
            code: problem.code.clone(),
            source: Some(problem.source.clone().unwrap_or_else(|| "host".to_string())),
            docs_url: problem.docs_url.clone(),
        };
        if !placed.contains(&diagnostic) {
            placed.push(diagnostic);
        }
    }
    placed
}

/// `(line, column, end_column)` of `range`'s part on its first line, kept
/// inside the text; `None` when it starts past the last line.
fn on_first_line(source: &str, range: TextRange) -> Option<(usize, usize, usize)> {
    let line = range.start.line.max(1);
    let text = source.split('\n').nth(line - 1)?;
    let width = text.chars().count() + 1;
    let start = range.start.column.clamp(1, width);
    let end = if range.end.line > line {
        width
    } else {
        range.end.column.clamp(start, width)
    };
    // An empty range still marks one character.
    Some((line, start, end.max(start + 1)))
}

/// The editor's `own` problems and the host's, each problem once: where the
/// host marks the same text (an overlapping marker on the same line), only
/// the host's shows, as its text is what the host acts on.
pub(super) fn merge(own: &[Diagnostic], host: &[Diagnostic]) -> Vec<Diagnostic> {
    let overlaps = |mine: &Diagnostic| {
        let (Some(line), Some(column)) = (mine.line, mine.column) else {
            return false;
        };
        let end = mine.end_column.unwrap_or(column + 1).max(column + 1);
        host.iter().any(|theirs| {
            let (Some(their_line), Some(their_column)) = (theirs.line, theirs.column) else {
                return false;
            };
            let their_end = theirs
                .end_column
                .unwrap_or(their_column + 1)
                .max(their_column + 1);
            their_line == line && column < their_end && their_column < end
        })
    };
    own.iter()
        .filter(|mine| !overlaps(mine))
        .chain(host)
        .cloned()
        .collect()
}

#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod tests;
