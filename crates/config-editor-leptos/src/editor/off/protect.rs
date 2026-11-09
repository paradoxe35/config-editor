//! Read-only fields and secrets, compiled out (the `protect` feature is
//! off): the hooks of `protect/` with nothing protected. The schema's
//! `readOnly`, `writeOnly` and `x-secret` marks are ignored, and the
//! `read_only_paths`, `read_only_ranges` and `secrets` props do nothing (a
//! console warning says so when one is given). The whole-document
//! `read_only` still works: it is the edit pipeline's own.

use std::sync::Arc;

use leptos::prelude::*;
use serde_json::Value;
use web_sys::KeyboardEvent;

use super::completion::{Completion, SuggestionPlace};
use super::diagnostics::{Diagnostic, SchemaBundle};
use super::edit::{Edit, Origin};
use super::language::ConfigLanguage;
use super::languages::Language;
use super::line_rows::LineMarks;
use super::paths::{FieldPath, TextRange};
use super::secret_options::SecretOptions;
use super::state::EditorState;
use crate::EditorStrings;

/// The host's side of protection (the component's props, when given).
pub(super) struct ProtectInit {
    pub(super) paths: Option<Signal<Vec<FieldPath>>>,
    pub(super) ranges: Option<Signal<Vec<TextRange>>>,
    pub(super) secrets: Option<SecretOptions>,
}

/// What protection would read.
#[allow(dead_code, reason = "the same inputs as protection's, unread here")]
pub(super) struct Inputs {
    pub(super) source: RwSignal<String>,
    pub(super) edit_language: RwSignal<Language>,
    pub(super) host_language: Language,
    pub(super) schema: Option<Arc<SchemaBundle>>,
    pub(super) host_text: StoredValue<String>,
}

/// Nothing protected.
#[derive(Clone, Copy)]
pub(super) struct ProtectState;

impl ProtectState {
    /// Says once which props the host gave (not empty, not the defaults)
    /// do nothing in this build.
    pub(super) fn new(init: ProtectInit, _inputs: Inputs) -> Self {
        let default = SecretOptions::default();
        let given = [
            (
                "read_only_paths",
                init.paths
                    .is_some_and(|paths| !paths.with_untracked(Vec::is_empty)),
            ),
            (
                "read_only_ranges",
                init.ranges
                    .is_some_and(|ranges| !ranges.with_untracked(Vec::is_empty)),
            ),
            (
                "secrets",
                init.secrets.is_some_and(|secrets| {
                    secrets.is_placeholder.is_some()
                        || secrets.placeholder != default.placeholder
                        || secrets.hint != default.hint
                        || secrets.reference_prefixes != default.reference_prefixes
                }),
            ),
        ];
        for (prop, _) in given.iter().filter(|(_, given)| *given) {
            warn_unused(prop);
        }
        Self
    }

    pub(super) fn line_marks(&self, _text: &str, _strings: &EditorStrings) -> Vec<LineMarks> {
        Vec::new()
    }

    pub(super) fn adjust_suggestions(
        &self,
        options: Vec<Completion>,
        _place: &SuggestionPlace<'_>,
        _strings: &EditorStrings,
    ) -> Vec<Completion> {
        options
    }

    pub(super) fn accepted_tokens(&self) -> Vec<(String, String)> {
        Vec::new()
    }

    pub(super) fn has_placeholders(&self) -> bool {
        false
    }

    pub(super) fn placeholder_pattern_problem(
        &self,
        _strings: &EditorStrings,
    ) -> Option<Diagnostic> {
        None
    }

    pub(super) fn placeholder_problems(
        &self,
        _behavior: &dyn ConfigLanguage,
        _text: &str,
        _schema: Option<&Value>,
        _strings: &EditorStrings,
        _on_screen: bool,
    ) -> Vec<Diagnostic> {
        Vec::new()
    }

    pub(super) fn placeholder_spans(&self, _text: &str) -> Vec<std::ops::Range<usize>> {
        Vec::new()
    }

    pub(super) fn tip_open(&self) -> bool {
        false
    }

    pub(super) fn track(&self) {}

    pub(super) fn hide_tip(&self, _hover_only: bool) {}

    /// Nothing is pinned to read afresh.
    pub(super) fn trust_text(&self) {}

    /// A refusal's reason has no tooltip to show in.
    pub(super) fn flash(&self, _message: String, _offset: usize) {}
}

/// Native unit tests have no console.
fn warn_unused(prop: &str) {
    #[cfg(target_arch = "wasm32")]
    web_sys::console::warn_1(
        &format!("config-editor: `{prop}` needs the `protect` feature; it does nothing").into(),
    );
    #[cfg(not(target_arch = "wasm32"))]
    let _ = prop;
}

impl EditorState {
    pub(super) fn connect_protection(&self) {}

    pub(super) fn widen_edit(&self, _edit: &mut Edit) {}

    pub(super) fn check_edit(&self, _edit: &Edit) -> Result<(), String> {
        Ok(())
    }

    pub(super) fn follow_edit(&self, _before: &str, _after: &str, _caret: usize) {}

    pub(super) fn note_refused(&self, _edit: &Edit, _message: String) -> Option<Edit> {
        None
    }

    pub(super) fn note_applied(&self, _edit: &Edit) {}

    pub(super) fn forget_refusal(&self) {}

    pub(super) fn find_protected_again(&self, _origin: &Origin) {}

    pub(super) fn check_host_save(&self, _old: &str, _new: &str) -> Result<(), ReadOnlyChange> {
        Ok(())
    }

    pub(super) fn save_waits(&self) -> Option<String> {
        None
    }

    pub(super) fn snap_to_tokens(&self) {}

    pub(super) fn recheck_tokens(&self) {}

    pub(super) fn press_at(&self, _offset: usize) {}

    pub(super) fn release_press(&self) {}

    pub(super) fn step_over_token(&self, _event: &KeyboardEvent) -> bool {
        false
    }

    pub(super) fn protected_note(&self, _offset: usize, _ends: bool) -> Option<(String, String)> {
        None
    }

    pub(super) fn hover_protected(&self, _line: usize, _column: usize) -> bool {
        false
    }

    #[cfg(feature = "search")]
    pub(super) fn protected_bytes(&self, _text: &str) -> Vec<std::ops::Range<usize>> {
        Vec::new()
    }
}

/// A save that would change a read-only value: never one without the
/// feature.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code, reason = "nothing is read-only without the feature")]
pub(super) struct ReadOnlyChange {
    pub(super) pointer: String,
}

/// A save refused because it would change the read-only value at
/// `pointer`.
pub(super) fn save_refused(strings: &EditorStrings, pointer: &str) -> String {
    let place = if pointer.is_empty() { "/" } else { pointer };
    crate::strings::fill(&strings.save_refused, &[("pointer", &place)])
}

/// No schema marks are read.
pub(super) fn mentions_marks(_schema: &Value) -> bool {
    false
}

#[cfg(feature = "schema-validation")]
pub(super) fn secret_pointers(_document: &Value, _root: &Value) -> Vec<String> {
    Vec::new()
}

#[component]
pub(super) fn ProtectTipView(state: EditorState) -> impl IntoView {
    let _ = state;
}
