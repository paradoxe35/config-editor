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
use super::super::parse_cache;
use super::super::secret_options::SecretOptions;
use super::super::state::{EditorState, directed_selection, textarea_selection};
use super::tokens::{
    Known, SecretMatcher, follow, held_in_fields, locate_written, snap, snap_selection, widen,
};

/// Known tokens one edit may write and still have found by path (a
/// paste); more (an undo restoring a whole text) are found by text.
const MAX_WRITTEN_BY_PATH: usize = 8;

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
        // By text, then by path only if the document says they are not all
        // in their fields.
        let written = locate_written(&matcher, shown.1, &known);
        let tokens = if in_their_fields(shown.0, shown.1, &known, &written) {
            written
        } else {
            matcher.locate(shown.0, shown.1, &known)
        };
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

/// UTF-16 `range` of `text`, as a string.
fn utf16_slice(text: &str, range: Range<usize>) -> String {
    let units: Vec<u16> = text.encode_utf16().collect();
    String::from_utf16_lossy(&units[range.start.min(units.len())..range.end.min(units.len())])
}

/// Whether `followed` (UTF-16 ranges of `text`) are each a token in its own
/// field: the parsed document holds every known token in its field as often
/// as the ranges show it. A text that does not parse keeps them as they
/// are, until it parses again.
fn in_their_fields(
    behavior: &dyn ConfigLanguage,
    text: &str,
    known: &[Known],
    followed: &[Range<usize>],
) -> bool {
    let parsed = parse_cache::parsed(behavior, text);
    let Ok(document) = parsed.as_ref() else {
        return true;
    };
    let units: Vec<u16> = text.encode_utf16().collect();
    let mut shown: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for range in followed {
        let written = String::from_utf16_lossy(
            &units[range.start.min(units.len())..range.end.min(units.len())],
        );
        *shown.entry(written).or_default() += 1;
    }
    let held = held_in_fields(document, known);
    held.iter()
        .all(|(token, count)| shown.get(*token).copied().unwrap_or(0) == *count)
        && shown
            .iter()
            .all(|(token, count)| held.get(token.as_str()) == Some(count))
}

/// Whether `after` is `before` with the UTF-16 range `start..end` replaced.
fn replaces(before: &str, after: &str, (start, end): (usize, usize)) -> bool {
    let old: Vec<u16> = before.encode_utf16().collect();
    let new: Vec<u16> = after.encode_utf16().collect();
    let kept = old.len().checked_sub(end).filter(|_| start <= end);
    kept.is_some_and(|kept| {
        new.len() >= start + kept
            && old[..start] == new[..start]
            && old[end..] == new[new.len() - kept..]
    })
}

impl EditorState {
    /// The edit pipeline's prepare stage: an edit cutting a placeholder
    /// takes it whole, leaving just the caret.
    pub(crate) fn widen_edit(&self, edit: &mut Edit) {
        let widened = self.protect().secrets.tokens.with_untracked(|tokens| {
            if tokens.is_empty() {
                return None;
            }
            // The selection the edit replaced when it is known and the
            // texts agree with it: a token pasted over another shares its
            // first characters, which a diff of the texts would keep.
            let (start, end) = Some(edit.replaced(edit.caret()))
                .filter(|selection| replaces(&edit.before, &edit.text, *selection))
                .unwrap_or_else(|| edit_range(&edit.before, &edit.text, edit.caret()));
            widen(tokens, &edit.before, &edit.text, start..end)
        });
        if let Some((text, _, caret)) = widened {
            edit.text = text;
            edit.collapse_to(caret);
        }
    }

    /// Follows an applied edit with the placeholders: they move with the
    /// text; an edit writing a token's text (a paste) finds each again by its
    /// path, so one put back into its own field is real again and one put in
    /// another field is not. Whether each is still in its own field after
    /// other edits (a key renamed above one) is checked at the typing pause
    /// ([`Self::recheck_tokens`]): it takes a parse, too slow for every key
    /// on a large document.
    pub(crate) fn follow_tokens(&self, before: &str, after: &str, caret: usize) {
        let secrets = self.protect().secrets;
        if secrets.known.with_untracked(Vec::is_empty) {
            return;
        }
        let (start, end) = edit_range(before, after, caret);
        let inserted_len = (after.encode_utf16().count() + end - start)
            .saturating_sub(before.encode_utf16().count());
        let followed = secrets
            .tokens
            .with_untracked(|tokens| follow(tokens, &(start..end), inserted_len));
        let behavior = self.behavior();
        let inserted = utf16_slice(after, start..start + inserted_len);
        let (tokens, by_text) = secrets.matcher.with_value(|matcher| {
            secrets.known.with_untracked(|known| {
                // The known tokens the edit wrote (one pass of the pattern).
                let mut written: Vec<&str> = matcher
                    .find(&inserted)
                    .into_iter()
                    .map(|range| &inserted[range])
                    .filter(|token| known.iter().any(|known| known.token == *token))
                    .collect();
                written.sort_unstable();
                written.dedup();
                if written.is_empty() {
                    return (followed, false);
                }
                if written.len() > MAX_WRITTEN_BY_PATH {
                    // A text written whole (an undo, a large paste): by
                    // text, then the fields checked in one parse.
                    return (locate_written(matcher, after, known), true);
                }
                // A token pasted: those few found by path, the rest followed.
                let mine: Vec<Known> = known
                    .iter()
                    .filter(|known| written.contains(&known.token.as_str()))
                    .cloned()
                    .collect();
                let units: Vec<u16> = after.encode_utf16().collect();
                let mut tokens: Vec<Range<usize>> = followed
                    .into_iter()
                    .filter(|range| {
                        let token = String::from_utf16_lossy(
                            &units[range.start.min(units.len())..range.end.min(units.len())],
                        );
                        !written.contains(&token.as_str())
                    })
                    .chain(matcher.locate(behavior, after, &mine))
                    .collect();
                tokens.sort_by_key(|range| range.start);
                (tokens, false)
            })
        });
        if secrets.tokens.with_untracked(|current| *current != tokens) {
            secrets.tokens.set(tokens);
        }
        // Checked in the text the edit writes: the document still holds
        // the one it replaces.
        if by_text {
            self.recheck_tokens_in(after);
        }
    }

    /// At a pause in typing: when the document no longer holds each of the
    /// host's tokens in its own field where the placeholders are (a key
    /// renamed above one, an undo through a cut), each is found again by its
    /// path: under a renamed key it is plain text.
    pub(crate) fn recheck_tokens(&self) {
        self.doc()
            .text
            .with_untracked(|text| self.recheck_tokens_in(text));
    }

    /// [`Self::recheck_tokens`] with the placeholders on `text`.
    fn recheck_tokens_in(&self, text: &str) {
        let secrets = self.protect().secrets;
        if secrets.known.with_untracked(Vec::is_empty) {
            return;
        }
        let behavior = self.behavior();
        let found = secrets.known.with_untracked(|known| {
            let kept = secrets
                .tokens
                .with_untracked(|tokens| in_their_fields(behavior, text, known, tokens));
            (!kept).then(|| {
                secrets
                    .matcher
                    .with_value(|matcher| matcher.locate(behavior, text, known))
            })
        });
        if let Some(tokens) = found
            && secrets.tokens.with_untracked(|current| *current != tokens)
        {
            secrets.tokens.set(tokens);
        }
    }

    /// Finds the placeholders again in a text replaced whole (a host
    /// value, an Edit-as switch, undo); with `host_changed`, the host's text
    /// is new and says again which tokens are real.
    pub(crate) fn rescan_tokens(&self, host_changed: bool) {
        let secrets = self.protect().secrets;
        if host_changed {
            let host = self.config().host_language.behavior();
            let known = secrets.matcher.with_value(|matcher| {
                self.sync()
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
            secrets.matcher.with_value(|matcher| {
                self.doc()
                    .text
                    .with_untracked(|text| locate_written(matcher, text, known))
            })
        });
        if secrets.tokens.with_untracked(|current| *current != tokens) {
            secrets.tokens.set(tokens);
        }
        // Where each is: checked now, which takes one parse when they are
        // all in their fields.
        self.recheck_tokens();
    }

    /// Takes the placeholders of `host` (a text the host gave, such as a
    /// reloaded baseline) as real, and finds them in the text on screen.
    pub(crate) fn adopt_host_tokens(&self, host: &str) {
        let secrets = self.protect().secrets;
        let language = self.config().host_language.behavior();
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
        let secrets = self.protect().secrets;
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

    /// A mouse press at UTF-16 `offset` (read off the pointer, since the
    /// browser's own anchor inside a placeholder is not reliable): a press
    /// inside one is remembered, for the release to anchor at its edge.
    pub(crate) fn press_at(&self, offset: usize) {
        let secrets = self.protect().secrets;
        let pressed = secrets.tokens.with_untracked(|tokens| {
            tokens
                .iter()
                .find(|token| token.start < offset && offset < token.end)
                .cloned()
        });
        self.protect()
            .marks
            .update_value(|marks| marks.pressed = pressed);
    }

    /// The mouse button released: a selection dragged from inside a
    /// placeholder takes it whole, anchored at its edge away from the drag
    /// (dragged left, at its end; right, at its start); without a drag the
    /// caret snaps out of it.
    pub(crate) fn release_press(&self) {
        let pressed = self
            .protect()
            .marks
            .try_update_value(|marks| marks.pressed.take())
            .flatten();
        let (Some(token), Some(element)) = (pressed, self.textarea()) else {
            self.snap_to_tokens();
            return;
        };
        let (start, end) = textarea_selection(&element);
        let inside = |at: usize| token.start < at && at < token.end;
        // The end the pointer moved: the one outside the placeholder.
        let focus = if !inside(start) && (start < end || !inside(end)) {
            if start < token.start { start } else { end }
        } else {
            end
        };
        if focus <= token.start {
            super::super::state::select(&element, focus, token.end, true);
            self.note_caret(focus);
        } else if focus >= token.end {
            super::super::state::select(&element, token.start, focus, false);
            self.note_caret(focus);
        } else {
            self.snap_to_tokens();
        }
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
            .protect()
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
