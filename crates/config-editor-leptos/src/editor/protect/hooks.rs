//! What the rest of the editor asks of protection (`off/protect.rs` answers
//! the same questions with nothing protected): the marks of each line, the
//! suggestions around protected fields, the placeholder problems, and the
//! tooltip's state.

use serde_json::Value;

use leptos::prelude::*;

use super::super::completion::{Completion, SuggestionPlace};
use super::super::diagnostics::{Diagnostic, Severity};
use super::super::language::ConfigLanguage;
use super::super::line_rows::MarkedLines;
use super::glued::glued_placeholders;
use super::live::ProtectState;
use super::problems::unknown_placeholders;
use super::suggest::{Around, adjust};
use super::view::line_marks;
use crate::EditorStrings;
use crate::strings::fill;

impl ProtectState {
    /// The marks of every line of `text` (the text on screen); empty when
    /// nothing is protected. Tracked.
    pub(crate) fn line_marks(&self, text: &str, strings: &EditorStrings) -> MarkedLines {
        self.protected.with(|protected| {
            self.secrets.tokens.with(|tokens| {
                let hint = self
                    .secrets
                    .options
                    .with_value(|options| options.hint.clone())
                    .unwrap_or_else(|| strings.secret_hint.clone());
                line_marks(text, protected, tokens, &hint, strings)
            })
        })
    }

    /// `options` without what a protected field rules out, or the reference
    /// forms in a secret field's value. Tracked.
    pub(crate) fn adjust_suggestions(
        &self,
        options: Vec<Completion>,
        place: &SuggestionPlace<'_>,
        strings: &EditorStrings,
    ) -> Vec<Completion> {
        let prefixes = self
            .secrets
            .options
            .with_value(|options| options.reference_prefixes.clone());
        self.host_paths.with(|host_paths| {
            self.protected.with(|protected| {
                adjust(
                    options,
                    &Around {
                        behavior: place.behavior,
                        source: place.source,
                        cursor: place.cursor,
                        explicit: place.explicit,
                        prefixes: &prefixes,
                        schema: place.schema,
                        host_paths,
                        protected,
                        strings,
                    },
                )
            })
        })
    }

    /// The host's secret placeholders, each with the field whose current
    /// secret it stands for (the schema check accepts it there), and the
    /// host's value of a field holding one alone with a multi-line form's
    /// whitespace around it. Tracked.
    pub(crate) fn accepted_tokens(&self) -> Vec<(String, String)> {
        self.secrets.known_tokens()
    }

    /// Whether the host gave any placeholder. Tracked.
    pub(crate) fn has_placeholders(&self) -> bool {
        self.secrets.known.with(|known| !known.is_empty())
    }

    /// The document problem (an error) saying the host's placeholder pattern
    /// cannot be used, worded as the npm package throws it; then no token is
    /// a placeholder.
    pub(crate) fn placeholder_pattern_problem(
        &self,
        strings: &EditorStrings,
    ) -> Option<Diagnostic> {
        let error = self
            .secrets
            .options
            .with_value(|options| options.check_placeholder().err())?;
        Some(Diagnostic {
            message: fill(&strings.invalid_placeholder, &[("error", &error)]),
            severity: Severity::Error,
            ..Diagnostic::default()
        })
    }

    /// A warning on each placeholder of `text` that is not real: one of the
    /// host's out of its own field, or one the host did not give in a field
    /// `schema` marks secret. `on_screen`: `text` is the text on screen,
    /// whose real placeholders are followed through every edit (and stand
    /// while it does not parse). Tracked.
    pub(crate) fn placeholder_problems(
        &self,
        behavior: &dyn ConfigLanguage,
        text: &str,
        schema: Option<&Value>,
        strings: &EditorStrings,
        on_screen: bool,
    ) -> Vec<Diagnostic> {
        self.secrets.matcher.with_value(|matcher| {
            self.secrets.known.with(|known| {
                let followed = if on_screen {
                    self.secrets.tokens.get_untracked()
                } else {
                    Vec::new()
                };
                let mut problems = unknown_placeholders(
                    behavior, text, matcher, known, &followed, schema, strings,
                );
                problems.extend(glued_placeholders(behavior, text, known, strings));
                problems
            })
        })
    }

    /// Byte ranges of the tokens of `text` the host's placeholder pattern
    /// (and check) match, real or not: what a host problem with no end
    /// marks whole when it starts on one.
    pub(crate) fn placeholder_spans(&self, text: &str) -> Vec<std::ops::Range<usize>> {
        self.secrets
            .matcher
            .with_value(|matcher| matcher.find(text))
    }

    /// Whether the tooltip shows.
    pub(crate) fn tip_open(&self) -> bool {
        self.tip.with_untracked(Option::is_some)
    }

    /// Subscribes to what the protected text at a place is (the locks and
    /// the placeholders).
    pub(crate) fn track(&self) {
        self.protected.track();
        self.secrets.tokens.track();
    }
}

/// The pointers of the fields `root` marks secret in `document`.
#[cfg(feature = "schema-validation")]
pub(crate) fn secret_pointers(document: &Value, root: &Value) -> Vec<String> {
    super::marks::schema_marks(document, root).secret
}
