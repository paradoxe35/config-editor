//! Secret placeholders in the live editor: which tokens are real (the
//! host's), where they are in the text on screen (followed through every
//! edit, found again when the text is replaced), and keeping each atomic:
//! edits touching one take it whole, and the caret never lands inside.

use std::ops::Range;
use std::sync::Arc;

use leptos::prelude::*;
use web_sys::KeyboardEvent;

use super::super::edit::Edit;
use super::super::input::edit_range;
use super::super::language::ConfigLanguage;
use super::super::state::{EditorState, directed_selection};
use super::tokens::{Known, SecretMatcher, SecretOptions, locate, snap, snap_selection, widen};

/// A text and the language it is written in.
type Written<'a> = (&'static dyn ConfigLanguage, &'a str);

/// The secret placeholders of one editor.
#[derive(Clone, Copy)]
pub(crate) struct SecretState {
    pub(crate) options: StoredValue<SecretOptions>,
    pub(crate) matcher: StoredValue<Arc<SecretMatcher>>,
    /// The tokens the host's text holds, each with its field and how often.
    pub(crate) known: RwSignal<Vec<Known>>,
    /// UTF-16 ranges of the real tokens in the text on screen.
    pub(crate) tokens: RwSignal<Vec<Range<usize>>>,
}

impl SecretState {
    pub(crate) fn new(options: SecretOptions, host: Written<'_>, shown: Written<'_>) -> Self {
        let matcher = SecretMatcher::new(&options);
        let known = matcher.known(host.0, host.1);
        let tokens = locate(shown.0, shown.1, &known);
        SecretState {
            options: StoredValue::new(options),
            matcher: StoredValue::new(Arc::new(matcher)),
            known: RwSignal::new(known),
            tokens: RwSignal::new(tokens),
        }
    }

    /// The host's tokens, each with the field it stands in.
    pub(crate) fn known_tokens(&self) -> Vec<(String, String)> {
        self.known.with(|known| {
            known
                .iter()
                .map(|known| (known.token.clone(), known.pointer.clone()))
                .collect()
        })
    }
}

impl EditorState {
    /// The edit pipeline's prepare stage: an edit cutting a placeholder
    /// takes it whole, leaving just the caret.
    pub(crate) fn widen_edit(&self, edit: &mut Edit) {
        let widened = self.protect.secrets.tokens.with_untracked(|tokens| {
            if tokens.is_empty() {
                return None;
            }
            let (start, end) = edit_range(&edit.before, &edit.text, edit.caret());
            widen(tokens, &edit.before, &edit.text, start..end)
        });
        if let Some((text, _, caret)) = widened {
            edit.text = text;
            edit.collapse_to(caret);
        }
    }

    /// Finds the placeholders again after an applied edit, each in its own
    /// field: a key renamed above one, or the token pasted elsewhere, leaves
    /// it plain; put back into its own field it is real again.
    pub(crate) fn follow_tokens(&self, after: &str) {
        let secrets = self.protect.secrets;
        if secrets.known.with_untracked(Vec::is_empty) {
            return;
        }
        let tokens = secrets
            .known
            .with_untracked(|known| locate(self.behavior(), after, known));
        if secrets.tokens.with_untracked(|current| *current != tokens) {
            secrets.tokens.set(tokens);
        }
    }

    /// Finds the placeholders again in a text replaced whole (a host
    /// value, an Edit-as switch, undo); with `host_changed`, the host's text
    /// is new and says again which tokens are real.
    pub(crate) fn rescan_tokens(&self, host_changed: bool) {
        let secrets = self.protect.secrets;
        if host_changed {
            let host = self.config.host_language.behavior();
            let known = secrets.matcher.with_value(|matcher| {
                self.sync
                    .host_text
                    .with_value(|text| matcher.known(host, text))
            });
            if secrets.known.with_untracked(|current| *current != known) {
                secrets.known.set(known);
            }
        }
        let tokens = secrets.known.with_untracked(|known| {
            if known.is_empty() {
                return Vec::new();
            }
            self.doc
                .text
                .with_untracked(|text| locate(self.behavior(), text, known))
        });
        if secrets.tokens.with_untracked(|current| *current != tokens) {
            secrets.tokens.set(tokens);
        }
    }

    /// Takes the placeholders of `host` (a text the host gave, such as a
    /// reloaded baseline) as real, and finds them in the text on screen.
    pub(crate) fn adopt_host_tokens(&self, host: &str) {
        let secrets = self.protect.secrets;
        let language = self.config.host_language.behavior();
        let known = secrets
            .matcher
            .with_value(|matcher| matcher.known(language, host));
        if secrets.known.with_untracked(|current| *current != known) {
            secrets.known.set(known);
        }
        self.rescan_tokens(false);
    }

    /// Moves a selection end left inside a placeholder (by a click, Home,
    /// a vertical move) out of it: a caret to the nearest edge, a selection
    /// grown to take it whole.
    pub(crate) fn snap_to_tokens(&self) {
        let secrets = self.protect.secrets;
        let Some(element) = self.textarea() else {
            return;
        };
        let (anchor, caret) = directed_selection(&element);
        let (start, end) = (anchor.min(caret), anchor.max(caret));
        let snapped = secrets
            .tokens
            .with_untracked(|tokens| snap_selection(tokens, start, end));
        if snapped == (start, end) {
            return;
        }
        let backward = caret < anchor;
        super::super::state::select(&element, snapped.0, snapped.1, backward);
        self.note_caret(if backward { snapped.0 } else { snapped.1 });
    }

    /// `ArrowLeft` / `ArrowRight` (with `Shift`, extending) step over a
    /// placeholder whole instead of into it.
    pub(crate) fn step_over_token(&self, event: &KeyboardEvent) -> bool {
        let key = event.key();
        let forward = match key.as_str() {
            "ArrowRight" => true,
            "ArrowLeft" => false,
            _ => return false,
        };
        if event.ctrl_key() || event.meta_key() || event.alt_key() {
            return false;
        }
        let Some(element) = self.textarea() else {
            return false;
        };
        let (anchor, caret) = directed_selection(&element);
        if anchor != caret && !event.shift_key() {
            return false;
        }
        let step = if forward {
            caret + 1
        } else {
            caret.saturating_sub(1)
        };
        let target = self
            .protect
            .secrets
            .tokens
            .with_untracked(|tokens| snap(tokens, step, Some(forward)));
        if target == step {
            return false;
        }
        event.prevent_default();
        let anchor = if event.shift_key() { anchor } else { target };
        super::super::state::select(
            &element,
            anchor.min(target),
            anchor.max(target),
            target < anchor,
        );
        self.note_caret(target);
        true
    }
}
