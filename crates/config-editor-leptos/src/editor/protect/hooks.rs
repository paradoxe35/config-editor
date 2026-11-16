//! What the rest of the editor asks of protection (`off/protect.rs` answers
//! the same questions with nothing protected): the marks of each line, the
//! suggestions around protected fields, the placeholder problems, and the
//! tooltip's state.

use serde_json::Value;

use leptos::prelude::*;

use super::super::completion::{Completion, SuggestionPlace};
use super::super::diagnostics::{Diagnostic, PlaceholderPattern, Severity};
use super::super::language::ConfigLanguage;
use super::super::line_rows::MarkedLines;
use super::foreign::{DocumentPlaceholders, foreign_placeholders};
use super::glued::glued_placeholders;
use super::live::{ProtectState, range_changed_message};
use super::locate::LineIndex;
use super::locks::Lock;
use super::problems::unknown_placeholders;
use super::suggest::{Around, adjust};
use super::texts::{save_held, save_held_placeholder};
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

    /// What finds the placeholders of every document a problem may quote:
    /// the host's pattern, and each other one of the documents of a
    /// documents view (a pattern once). Tracked.
    pub(crate) fn placeholder_patterns(&self) -> Vec<PlaceholderPattern> {
        let mut matchers = vec![self.secrets.matcher.get_value()];
        self.foreign.with(|others| {
            for other in others.iter() {
                let matcher = other.matcher();
                if !matchers.iter().any(|known| known.finds_as(matcher)) {
                    matchers.push(matcher.clone());
                }
            }
        });
        matchers
            .into_iter()
            .map(|matcher| -> PlaceholderPattern {
                Box::new(move |text: &str| matcher.find_in(text))
            })
            .collect()
    }

    /// Whether the host gave any placeholder, or another document of a
    /// documents view did. Tracked.
    pub(crate) fn has_placeholders(&self) -> bool {
        self.secrets.known.with(|known| !known.is_empty())
            || self.foreign.with(|foreign| !foreign.is_empty())
    }

    /// This editor's placeholders, as the other editors of a documents
    /// view read them (`source`: its document's file name): every token
    /// its host gave since it was made, not only those of its last text,
    /// so a copy written elsewhere while it was one stays one there after
    /// a reload (else a paste could split it, and pieces of a token reach
    /// that document's host). Tracked.
    pub(crate) fn own_placeholders(&self, source: String) -> Option<DocumentPlaceholders> {
        let matcher = self.secrets.matcher.get_value();
        let tokens = self.secrets.known.with(|known| {
            self.marks
                .try_update_value(|marks| {
                    marks
                        .given
                        .extend(known.iter().map(|known| known.token.clone()));
                    marks.given.clone()
                })
                .unwrap_or_default()
        });
        DocumentPlaceholders::new(source, matcher, tokens)
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

    /// A warning on each placeholder of `text` that is not real: one of
    /// another document's (see `foreign`), one of the host's out of its own
    /// field, or one the host did not give in a field
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
                let foreign = self
                    .foreign
                    .with(|others| foreign_placeholders(text, matcher, known, others, strings));
                let mut problems = unknown_placeholders(
                    behavior, text, matcher, known, &followed, schema, strings,
                );
                // Another document's token in a secret field is that, not
                // a token no host gave.
                let flagged: std::collections::HashSet<_> = foreign
                    .iter()
                    .map(|other| (other.line, other.column))
                    .collect();
                problems.retain(|problem| !flagged.contains(&(problem.line, problem.column)));
                problems.extend(foreign);
                problems.extend(glued_placeholders(behavior, text, known, strings));
                problems
            })
        })
    }

    /// A warning at each locked field held (see `locks::Protected::held`),
    /// at a host range whose text the text on screen does not hold (see
    /// `ProtectState::ranges_changed`), and at a placeholder the parse
    /// reads with text beside it (see `ProtectState::placeholder_held`,
    /// `glued_away`): why the change, the value and the save wait. `text`
    /// is written in `behavior`. Tracked.
    pub(crate) fn held_problems(
        &self,
        behavior: &dyn ConfigLanguage,
        text: &str,
        strings: &EditorStrings,
    ) -> Vec<Diagnostic> {
        let mut problems = self.held_lock_problems(text, strings);
        problems.extend(self.range_changed_problem(text, strings));
        problems.extend(self.held_placeholder_problem(behavior, text, strings));
        problems
    }

    /// The warning of a host range the text on screen changed before the
    /// host locked it, where the range is placed now.
    fn range_changed_problem(&self, text: &str, strings: &EditorStrings) -> Option<Diagnostic> {
        let (locked, index) = self.ranges_changed.get()?;
        let start = self
            .ranges
            .with(|ranges| ranges.get(index).map_or(0, |range| range.start));
        let lines = LineIndex::new(text);
        Some(warning_at(
            &lines,
            text,
            start.min(lines.units_of(text.len())),
            range_changed_message(strings, &locked),
        ))
    }

    /// The placeholder hold's warning, at the first copy of its token
    /// written in `text` (else its first token, else the first line).
    fn held_placeholder_problem(
        &self,
        behavior: &dyn ConfigLanguage,
        text: &str,
        strings: &EditorStrings,
    ) -> Option<Diagnostic> {
        if self.secrets.known.with_untracked(Vec::is_empty) {
            return None;
        }
        let field = self
            .placeholder_held(behavior, text)
            .or_else(|| self.glued_away(behavior, text))?;
        let token = self.secrets.known.with_untracked(|known| {
            known
                .iter()
                .find(|known| known.field.as_deref() == Some(field.as_str()))
                .map(|known| known.token.clone())
        });
        let real = self.secrets.tokens.get_untracked();
        let lines = LineIndex::new(text);
        let start = token
            .and_then(|token| {
                let found: Vec<usize> = text
                    .match_indices(token.as_str())
                    .map(|(at, _)| lines.units_of(at))
                    .collect();
                found
                    .iter()
                    .copied()
                    .find(|at| !real.iter().any(|real| real.start == *at))
                    .or_else(|| found.first().copied())
            })
            .unwrap_or(0);
        Some(warning_at(
            &lines,
            text,
            start,
            save_held_placeholder(strings, &field),
        ))
    }

    /// [`Self::held_problems`]' warnings at the locked fields held, each
    /// naming the cause recorded for it.
    fn held_lock_problems(&self, text: &str, strings: &EditorStrings) -> Vec<Diagnostic> {
        self.protected.with(|protected| {
            let lines = LineIndex::new(text);
            protected
                .held
                .causes()
                .map(|(pointer, cause)| {
                    let reason = protected.held_reason(pointer);
                    let message = save_held(strings, pointer, &reason, cause);
                    // Marked where its lock starts; one the text no longer
                    // has (commented out, cut), where the lock nearest to
                    // it by path starts (a field holding it first), else
                    // on the first line: F8 shows it there.
                    let start = protected
                        .locks
                        .iter()
                        .find(|lock| lock.pointer.as_ref() == Some(pointer))
                        .or_else(|| nearest_lock(&protected.locks, pointer))
                        .and_then(|lock| lock.ranges.first())
                        .map_or(0, |range| range.start);
                    warning_at(&lines, text, start, message)
                })
                .collect()
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
    #[cfg(test)]
    pub(crate) fn tip_open(&self) -> bool {
        self.tip().is_some()
    }

    /// Subscribes to what the protected text at a place is (the locks and
    /// the placeholders).
    pub(crate) fn track(&self) {
        self.protected.track();
        self.secrets.tokens.track();
    }
}

/// The pointers of the fields `root` marks secret in `document` (see
/// [`super::marks::secret_fields`]).
#[cfg(feature = "schema-validation")]
pub(crate) fn secret_pointers(document: &Value, root: &Value, repeats_arrays: bool) -> Vec<String> {
    super::marks::secret_fields(document, root, repeats_arrays)
}

/// A warning saying `message` at UTF-16 `start` of `text` (whose lines are
/// `lines`), one character wide.
fn warning_at(lines: &LineIndex<'_>, text: &str, start: usize, message: String) -> Diagnostic {
    let line = lines.line_of(start);
    let line_start = lines.byte_of(lines.line_bounds(line).start);
    let column = text[line_start..lines.byte_of(start)].chars().count() + 1;
    Diagnostic {
        line: Some(line + 1),
        column: Some(column),
        end_column: Some(column + 1),
        message,
        severity: Severity::Warning,
        ..Diagnostic::default()
    }
}

/// The lock of `locks` nearest to the field at `pointer` by path: the one
/// sharing the most leading segments with it (a field holding it first),
/// the first one at a tie (a range, with no path, shares none).
fn nearest_lock<'l>(locks: &'l [Lock], pointer: &str) -> Option<&'l Lock> {
    let shared = |lock: &Lock| {
        lock.pointer.as_deref().map_or(0, |other| {
            pointer
                .split('/')
                .zip(other.split('/'))
                .skip(1)
                .take_while(|(mine, theirs)| mine == theirs)
                .count()
        })
    };
    // `max_by_key` keeps the last of equals: reversed, the first.
    locks
        .iter()
        .filter(|lock| !lock.ranges.is_empty())
        .rev()
        .max_by_key(|lock| shared(lock))
}
