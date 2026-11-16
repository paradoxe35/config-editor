//! Secret placeholders in the live editor: which tokens are real (the
//! host's), where they are in the text on screen (followed through every
//! edit, found again when the text is replaced), and keeping each atomic:
//! edits touching one take it whole, and the caret never lands inside.

use std::sync::Arc;

use leptos::prelude::*;
use web_sys::KeyboardEvent;

use super::super::edit::{Change, Edit};
use super::super::language::ConfigLanguage;
use super::super::parse_cache;
use super::super::secret_options::SecretOptions;
use super::super::state::{EditorState, directed_selection, select, textarea_selection};
use super::super::text_change::edited_bytes;
use super::follow_items;
use super::foreign::foreign_tokens;
use super::tokens::{
    Known, SecretMatcher, follow, held_in_fields, in_comment, locate_written, snap, snap_selection,
    widen,
};
use super::unplaced::placed_where_written;
use super::whole_guard::{Held, WholeIndex};
use crate::editor::text::{TextRange, TextSize, byte_range, slice, text_size};

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
    /// The tokens of `known` given as a field's whole value, looked up by
    /// token and by field (built again only when `known` changes).
    pub(crate) whole: Memo<Arc<WholeIndex>>,
    /// The ranges of the real tokens in the text on screen.
    pub(crate) tokens: RwSignal<Vec<TextRange>>,
    /// The last text an edit's check found to parse, or made of one by an
    /// edit to one scalar (see `whole_guard`).
    pub(crate) parsing: StoredValue<Option<String>>,
    /// An edit taken while the parse reads a placeholder given whole with
    /// text beside it (see `whole_guard::Held`): what reaches the host
    /// waits until it reads as the host's text again.
    pub(crate) held: StoredValue<Option<Held>>,
}

impl SecretState {
    /// With `reading`, the schema the views read the host's text by.
    pub(crate) fn new(
        options: SecretOptions,
        host: Written<'_>,
        shown: Written<'_>,
        reading: Option<&serde_json::Value>,
    ) -> Self {
        let matcher = SecretMatcher::new(&options);
        let known = matcher.known(host.0, host.1, reading);
        // By text, then by path only if the document says they are not all
        // in their fields.
        let written = locate_written(&matcher, shown.1, &known);
        let tokens = if in_their_fields(shown.0, shown.1, &known, &written) {
            written
        } else {
            matcher.locate(shown.0, shown.1, &known)
        };
        let known = RwSignal::new(known);
        SecretState {
            options: StoredValue::new(options),
            matcher: StoredValue::new(Arc::new(matcher)),
            known,
            whole: Memo::new(move |_| Arc::new(known.with(|known| WholeIndex::new(known)))),
            tokens: RwSignal::new(tokens),
            parsing: StoredValue::new(None),
            held: StoredValue::new(None),
        }
    }

    /// The host's tokens in fields, each with the field it stands in, and
    /// the value of a field that holds its token alone as the host wrote
    /// it (with a block's last line break): each stands for the field's
    /// current secret.
    pub(crate) fn known_tokens(&self) -> Vec<(String, String)> {
        self.known.with(|known| {
            known
                .iter()
                .filter_map(|known| Some((known, known.field.as_ref()?)))
                .flat_map(|(known, field)| {
                    let written = known.whole.as_ref().filter(|whole| **whole != known.token);
                    std::iter::once(&known.token)
                        .chain(written)
                        .map(|value| (value.clone(), field.clone()))
                })
                .collect()
        })
    }
}

/// Whether `followed` (ranges of `text`) are each a token in its own
/// field: the parsed document holds every known token in its field as often
/// as the ranges show it. A text that does not parse keeps them as they
/// are, until it parses again.
fn in_their_fields(
    behavior: &dyn ConfigLanguage,
    text: &str,
    known: &[Known],
    followed: &[TextRange],
) -> bool {
    let parsed = parse_cache::parsed(behavior, text);
    let Ok(document) = parsed.as_ref() else {
        return true;
    };
    let mut shown: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    // Tokens the host wrote in comments: those followed in comments are
    // each one of them, as often as the comments hold it (read only when
    // the host wrote one there).
    let mut in_comments: std::collections::HashMap<&str, usize> = known
        .iter()
        .filter(|known| known.field.is_none())
        .map(|known| (known.token.as_str(), known.count))
        .collect();
    let comments = if in_comments.is_empty() {
        Vec::new()
    } else {
        behavior.comments(text)
    };
    for range in followed {
        let end = range.end().min(TextSize::of(text));
        let written = slice::slice(text, TextRange::new(range.start().min(end), end))
            .unwrap_or_default()
            .to_string();
        if !comments.is_empty() && in_comment(&comments, range.start()) {
            match in_comments.get_mut(written.as_str()) {
                Some(left) if *left > 0 => *left -= 1,
                _ => return false,
            }
            continue;
        }
        *shown.entry(written).or_default() += 1;
    }
    // Each comment's token is followed as often as it is in the comments,
    // up to the host's count.
    if !in_comments.is_empty() {
        let in_the_comments = |token: &str| {
            comments
                .iter()
                .map(|comment| {
                    slice::slice(text, byte_range(comment.clone()))
                        .unwrap_or_default()
                        .matches(token)
                        .count()
                })
                .sum::<usize>()
        };
        let all_followed = known
            .iter()
            .filter(|known| known.field.is_none())
            .all(|known| {
                let left = in_comments.get(known.token.as_str()).copied().unwrap_or(0);
                let written = in_the_comments(&known.token).min(known.count);
                known.count - left == written
            });
        if !all_followed {
            return false;
        }
    }
    let held = held_in_fields(document, known);
    held.iter()
        .all(|(token, count)| shown.get(*token).copied().unwrap_or(0) == *count)
        && shown
            .iter()
            .all(|(token, count)| held.get(token.as_str()) == Some(count))
}

/// The range of `before` an edit to `after` replaced when the browser did
/// not say (an `insertFromPaste` input without a `beforeinput`) and the
/// inserted text began a placeholder it replaced whole: a token pasted
/// over another shares its first characters, which the diff of the texts
/// keeps, reading the paste as an edit inside the token. The inserted text
/// ends at `caret` (a byte of `after`); it is taken to start where the
/// token it began in starts, when the range then cuts no token. `None`
/// otherwise.
fn ended_at_caret(
    tokens: &[TextRange],
    before: &str,
    after: &str,
    caret: TextSize,
) -> Option<TextRange> {
    // What follows the caret is what followed the replaced range.
    let kept = TextSize::of(after).checked_sub(caret)?;
    let end = TextSize::of(before).checked_sub(kept)?;
    if slice::after(before, end)? != slice::after(after, caret)? {
        return None;
    }
    let prefix = before
        .bytes()
        .zip(after.bytes())
        .take_while(|(old, new)| old == new)
        .count();
    let start = text_size(prefix).min(caret).min(end);
    // Only text written past the shared characters (a deletion is no
    // paste).
    if caret <= start {
        return None;
    }
    let token = tokens
        .iter()
        .find(|token| token.start() < start && start < token.end())?;
    let cuts = |range: &TextRange| {
        tokens.iter().any(|token| {
            (token.start() < range.start() && range.start() < token.end())
                || (token.start() < range.end() && range.end() < token.end())
        })
    };
    let whole = TextRange::new(token.start(), end.max(token.start()));
    (!cuts(&whole)).then_some(whole)
}

impl EditorState {
    /// The placeholders of `text` (in order) no edit may cut: the host's
    /// tokens where they are, every other copy of one written here (a
    /// placeholder is one span wherever it is: pasted where no field reads
    /// it, it is still never split), and, in a documents view, the tokens
    /// of the other documents written here (flagged as theirs, and as solid
    /// as here: removed whole or kept whole).
    pub(crate) fn solid_tokens(&self, text: &str) -> Vec<TextRange> {
        let secrets = self.protect().secrets;
        let mut tokens = secrets.tokens.get_untracked();
        // Another document's tokens are solid here too, also where this
        // document's host gave none of its own.
        let others = self.protect().foreign.get_untracked();
        if secrets.known.with_untracked(Vec::is_empty) && others.is_empty() {
            return tokens;
        }
        // The copies of the host's tokens and of the other documents'
        // tokens written here.
        let written: Vec<TextRange> = secrets.matcher.with_value(|matcher| {
            secrets.known.with_untracked(|known| {
                let own: std::collections::HashSet<&str> =
                    known.iter().map(|known| known.token.as_str()).collect();
                matcher
                    .find(text)
                    .into_iter()
                    .map(byte_range)
                    .filter(|range| {
                        slice::slice(text, *range).is_some_and(|token| own.contains(token))
                    })
                    .chain(
                        foreign_tokens(text, matcher, known, &others)
                            .into_iter()
                            .map(|(range, _)| byte_range(range)),
                    )
                    .collect()
            })
        });
        if written.is_empty() {
            return tokens;
        }
        tokens.extend(written);
        tokens.sort_by_key(|range| range.start());
        tokens.dedup_by(|later, earlier| later.start() < earlier.end());
        tokens
    }

    /// The solid tokens of the text on screen (see [`Self::solid_tokens`]).
    fn solid_tokens_on_screen(&self) -> Vec<TextRange> {
        self.doc()
            .text
            .with_untracked(|text| self.solid_tokens(text))
    }

    /// The edit pipeline's prepare stage: an edit cutting a placeholder
    /// takes it whole, leaving just the caret.
    pub(crate) fn widen_edit(&self, edit: &mut Edit) {
        let tokens = self.solid_tokens(&edit.before);
        if tokens.is_empty() {
            return;
        }
        // What the edit replaced, as it said: a token pasted over another
        // shares its first characters, which a diff of the texts would
        // keep. Read off the texts only when it did not say.
        let caret = edit.caret();
        let recorded = edit
            .change()
            .and_then(|change| change.hull())
            .map(|(was, _)| was);
        let region = recorded
            .or_else(|| ended_at_caret(&tokens, &edit.before, &edit.text, caret))
            .unwrap_or_else(|| edited_bytes(&edit.before, &edit.text, caret));
        if let Some((text, wide, caret)) = widen(&tokens, &edit.before, &edit.text, region) {
            // What the widened edit does, recorded for the followers.
            let inserted =
                slice::slice(&text, TextRange::new(wide.start(), caret.max(wide.start())))
                    .unwrap_or_default()
                    .to_string();
            edit.rewrite(Change::replace(&edit.before, wide, &inserted));
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
    pub(crate) fn follow_tokens(
        &self,
        before: &str,
        after: &str,
        caret: TextSize,
        change: Option<&Change>,
    ) {
        let secrets = self.protect().secrets;
        if secrets.known.with_untracked(Vec::is_empty) {
            return;
        }
        // What the edit replaced, as it recorded it; else read off the
        // texts and its caret.
        let replaced = change
            .and_then(Change::hull)
            .map_or_else(|| edited_bytes(before, after, caret), |(was, _)| was);
        // Tokens the edit wrote again whole are found again by their text.
        let region = secrets.tokens.with_untracked(|tokens| {
            super::rewritten::rewritten_region(tokens, before, after, &replaced).unwrap_or(replaced)
        });
        let region = secrets.known.with_untracked(|known| {
            let texts: Vec<&str> = known.iter().map(|known| known.token.as_str()).collect();
            super::rewritten::around_written(&texts, before, after, &region)
        });
        let inserted_len = (TextSize::of(after) + region.len())
            .checked_sub(TextSize::of(before))
            .unwrap_or_default();
        let followed = secrets
            .tokens
            .with_untracked(|tokens| follow(tokens, &region, inserted_len));
        let behavior = self.behavior();
        let wrote = TextRange::at(region.start(), inserted_len);
        let inserted = slice::slice(after, wrote).unwrap_or_default();
        let (tokens, by_text) = secrets.matcher.with_value(|matcher| {
            secrets.known.with_untracked(|known| {
                // The known tokens the edit wrote (one pass of the pattern).
                let mut written: Vec<&str> = matcher
                    .find(inserted)
                    .into_iter()
                    .filter_map(|range| inserted.get(range))
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
                let text_of = |range: &TextRange| {
                    let end = range.end().min(TextSize::of(after));
                    slice::slice(after, TextRange::new(range.start().min(end), end))
                        .unwrap_or_default()
                };
                // In a text that parses, every copy of a pasted token is
                // placed by its path, first ones first.
                if parse_cache::parsed(behavior, after).is_ok() {
                    let mut tokens: Vec<TextRange> = followed
                        .into_iter()
                        .filter(|range| !written.contains(&text_of(range)))
                        .chain(matcher.locate(behavior, after, &mine))
                        .collect();
                    tokens.sort_by_key(|range| range.start());
                    return (tokens, false);
                }
                // In one that does not, no path is read off its lines for
                // the tokens the edit kept (a placeholder whose key a paste
                // split would read as another field's, and a paste into it
                // would be taken): they follow their text. With the copies
                // the edit wrote that their path places, each token is real
                // as often as the host gave it, first ones first.
                let mut left: std::collections::HashMap<String, usize> =
                    std::collections::HashMap::new();
                for known in mine.iter().filter(|known| known.field.is_some()) {
                    *left.entry(known.token.clone()).or_default() += known.count;
                }
                let (kept, mut tokens): (Vec<TextRange>, Vec<TextRange>) = followed
                    .into_iter()
                    .partition(|range| left.contains_key(text_of(range)));
                let mut candidates: Vec<TextRange> = matcher
                    .locate(behavior, after, &mine)
                    .into_iter()
                    .filter(|range| wrote.contains_range(*range))
                    .chain(kept)
                    .collect();
                candidates.sort_by_key(|range| range.start());
                candidates.dedup_by(|later, earlier| later.start() < earlier.end());
                for range in candidates {
                    if let Some(count) = left.get_mut(text_of(&range))
                        && *count > 0
                    {
                        *count -= 1;
                        tokens.push(range);
                    }
                }
                tokens.sort_by_key(|range| range.start());
                (tokens, false)
            })
        });
        if secrets.tokens.with_untracked(|current| *current != tokens) {
            secrets.tokens.set(tokens);
        }
        // An edit repairing a broken host text places its tokens at once:
        // the next key beside one is held as in its field.
        self.place_unplaced(after);
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
        self.place_unplaced(text);
        let in_place = |known: &[Known]| {
            secrets
                .tokens
                .with_untracked(|tokens| in_their_fields(behavior, text, known, tokens))
        };
        // All real where they are: nothing to look for.
        let all_real = || {
            let written: usize = secrets.known.with_untracked(|known| {
                known
                    .iter()
                    .filter(|known| known.field.is_some())
                    .map(|known| known.count)
                    .sum()
            });
            secrets.tokens.with_untracked(Vec::len) >= written
        };
        let placed = secrets.known.with_untracked(|known| in_place(known));
        if placed && all_real() {
            return;
        }
        // Items shifted around a placeholder's (one deleted or inserted
        // before it, it moved, cut and pasted): its field follows its item.
        let followed = parse_cache::parsed(behavior, text)
            .as_ref()
            .as_ref()
            .ok()
            .and_then(|document| {
                secrets
                    .known
                    .with_untracked(|known| follow_items::followed(document, known))
            });
        match followed {
            Some(followed) => secrets.known.set(followed),
            // A placeholder removed or plain where it is, as before.
            None if placed => return,
            None => {}
        }
        let tokens = secrets.known.with_untracked(|known| {
            secrets
                .matcher
                .with_value(|matcher| matcher.locate(behavior, text, known))
        });
        if secrets.tokens.with_untracked(|current| *current != tokens) {
            secrets.tokens.set(tokens);
        }
    }

    /// Tokens whose fields the host's broken text could not tell (see
    /// `Known::placed`): placed where they are once `text` (on screen,
    /// where the placeholders are) parses.
    fn place_unplaced(&self, text: &str) {
        let secrets = self.protect().secrets;
        if !secrets.whole.with_untracked(|index| index.has_unplaced()) {
            return;
        }
        let behavior = self.behavior();
        let parsed = parse_cache::parsed(behavior, text);
        let Ok(document) = parsed.as_ref() else {
            return;
        };
        let placed = secrets.known.with_untracked(|known| {
            secrets.tokens.with_untracked(|tokens| {
                placed_where_written(behavior, text, document, known, tokens)
            })
        });
        if let Some(placed) = placed {
            secrets.known.set(placed);
        }
    }

    /// Finds the placeholders again in a text replaced whole (a host
    /// value, an Edit-as switch, undo); with `host_changed`, the host's text
    /// is new and says again which tokens are real.
    pub(crate) fn rescan_tokens(&self, host_changed: bool) {
        let secrets = self.protect().secrets;
        if host_changed {
            let host = self.config().host_language.behavior();
            let known = self.config().schema.with_value(|schema| {
                let reading = schema.as_deref().map(|bundle| &bundle.schema);
                secrets.matcher.with_value(|matcher| {
                    self.sync()
                        .host_text
                        .with_value(|text| matcher.known(host, text, reading))
                })
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
        let known = self.config().schema.with_value(|schema| {
            let reading = schema.as_deref().map(|bundle| &bundle.schema);
            secrets
                .matcher
                .with_value(|matcher| matcher.known(language, host, reading))
        });
        if secrets.known.with_untracked(|current| *current != known) {
            secrets.known.set(known);
        }
        self.rescan_tokens(false);
    }

    /// Whether a selection from byte `start` to `end` takes part of a
    /// placeholder, not all of it. A drag of such a selection is refused:
    /// its delete would take the placeholder whole while its drop writes
    /// only the dragged part, losing the rest.
    pub(crate) fn cuts_a_token(&self, start: TextSize, end: TextSize) -> bool {
        let region = TextRange::new(start.min(end), end.max(start));
        self.solid_tokens_on_screen()
            .iter()
            .any(|token| super::tokens::cuts(token, &region))
    }

    /// Moves a selection end left inside a placeholder (by a click, Home,
    /// a vertical move) out of it: a caret to the nearest edge, a selection
    /// grown to take it whole.
    pub(crate) fn snap_to_tokens(&self) {
        let Some(element) = self.textarea() else {
            return;
        };
        let (anchor, caret) = directed_selection(&element);
        let (start, end) = (anchor.min(caret), anchor.max(caret));
        let snapped = snap_selection(&self.solid_tokens_on_screen(), start, end);
        if snapped == (start, end) {
            return;
        }
        let backward = caret < anchor;
        select(&element, snapped.0, snapped.1, backward);
        self.note_caret(if backward { snapped.0 } else { snapped.1 });
    }

    /// A mouse press at byte `offset` (read off the pointer, since the
    /// browser's own anchor inside a placeholder is not reliable): a press
    /// inside one is remembered, for the release to anchor at its edge.
    pub(crate) fn press_at(&self, offset: TextSize) {
        let pressed = self
            .solid_tokens_on_screen()
            .into_iter()
            .find(|token| token.start() < offset && offset < token.end());
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
        let inside = |at: TextSize| token.start() < at && at < token.end();
        // The end the pointer moved: the one outside the placeholder.
        let focus = if !inside(start) && (start < end || !inside(end)) {
            if start < token.start() { start } else { end }
        } else {
            end
        };
        if focus <= token.start() {
            select(&element, focus, token.end(), true);
            self.note_caret(focus);
        } else if focus >= token.end() {
            select(&element, token.start(), focus, false);
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
        // One character on: into a placeholder, that is past it.
        let step = self.doc().text.with_untracked(|text| {
            if forward {
                slice::step_forward(text, caret)
            } else {
                slice::step_back(text, caret)
            }
        });
        let target = snap(&self.solid_tokens_on_screen(), step, Some(forward));
        if target == step {
            return false;
        }
        event.prevent_default();
        let anchor = if event.shift_key() { anchor } else { target };
        select(
            &element,
            anchor.min(target),
            anchor.max(target),
            target < anchor,
        );
        self.note_caret(target);
        true
    }
}
