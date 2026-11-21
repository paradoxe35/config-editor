//! Protected fields in the live editor: the locks of the text on screen
//! (kept by a memo), the check every edit passes, and the tooltip saying
//! why one was refused.

use std::sync::Arc;

use leptos::prelude::*;

use super::super::codec;
use super::super::edit::{Change, Edit, Origin};
use super::super::edit_as::SaveError;
use super::super::language::ConfigLanguage;
use super::super::languages::Language;
use super::super::parse_cache;
use super::super::paths::FieldPath;
use super::super::popups::PopupState;
use super::super::schema::SchemaBundle;
use super::super::secret_options::SecretOptions;
use super::super::state::EditorState;
use super::carry::Moves;
use super::follow::follow_ranges;
use super::foreign::DocumentPlaceholders;
use super::glued::{first_glued, glued_elsewhere, glued_in_save, glued_message};
use super::guard::{Blocked, Carets, judge};
use super::host_ranges::{HostRanges, followed_to};
use super::locate::LineIndex;
use super::locks::{HoldCause, Protected, Sources, protect};
use super::save::{ReadOnlyChange, check_save, locked_pointers, range_pointers};
use super::secrets::SecretState;
use super::texts::{Reason, save_held, save_held_placeholder, save_waits};
use super::whole_guard::Placeholders;
use crate::EditorStrings;
use crate::editor::sync::{HostChannel, Link};
use crate::editor::text::{self, TextRange, TextSize, utf16_of};

/// The host's side of protection (the component's props, when given).
pub(crate) struct ProtectInit {
    pub(crate) paths: Option<Signal<Vec<FieldPath>>>,
    pub(crate) secrets: Option<SecretOptions>,
    /// The placeholders of the other editors of a documents view (see
    /// `foreign`).
    pub(crate) foreign: Option<Signal<Arc<Vec<DocumentPlaceholders>>>>,
}

/// The bookkeeping of one editor's protection.
#[derive(Clone, Default)]
pub(crate) struct Marks {
    /// Counts the texts the host gave (see [`ProtectState::trust_text`]).
    epoch: u64,
    /// The placeholder a mouse press landed inside, until it is released.
    pub(crate) pressed: Option<TextRange>,
    /// The last change event was held back (see
    /// [`EditorState::note_held_back`]): new locks from the host that release
    /// it send the text on screen then.
    held_back: bool,
    /// The last edit followed, so the locks follow it as the guard judged
    /// it (see `locks::Sources::caret` and `change`).
    followed: Option<Followed>,
    /// The locks the guard found for the text of the edit it took (see
    /// `guard::judge`): the memo of locks takes them for that text instead
    /// of finding them again, so the editor holds the spans as judged.
    foreseen: Option<(String, Protected)>,
    /// Every token the host gave since the editor was made (see
    /// `own_placeholders`).
    pub(crate) given: std::collections::HashSet<String>,
}

/// An edit the locks follow: the text it was made on and the text it made,
/// the caret it left there (none for an undo or redo) and what it
/// recorded it did (`Edit::change`). It moves only the locks of
/// its text before into its text after: locks found again on the text
/// after (new paths or ranges from the host) are already where they are.
#[derive(Clone)]
struct Followed {
    before: Arc<str>,
    after: Arc<str>,
    caret: Option<TextSize>,
    change: Option<Arc<Change>>,
}

impl Followed {
    /// Whether this edit made `text` from the text `previous` was found in.
    fn made(&self, previous: Option<&Protected>, text: &str) -> bool {
        *self.after == *text && previous.is_some_and(|previous| *self.before == *previous.text)
    }
}

/// The protection signals of one editor.
#[derive(Clone, Copy)]
pub(crate) struct ProtectState {
    pub(crate) host_paths: Signal<Vec<FieldPath>>,
    /// The document's record of its host, which keeps the ranges it gave
    /// (see `sync::host_inputs`): what is locked is derived from them.
    channel: StoredValue<HostChannel, LocalStorage>,
    /// Counts the changes of the host's ranges (see
    /// [`EditorState::host_ranges_given`]).
    ranges_given: RwSignal<u64>,
    /// The host's ranges as ranges of the text on screen, followed
    /// through edits; empty while editing as another language (there the
    /// fields they cover are locked by path).
    pub(crate) ranges: RwSignal<Vec<TextRange>>,
    /// The host's ranges in its own text while editing as another
    /// language, followed through each conversion back.
    in_host: HostRanges,
    /// The text of a host range that the text on screen does not hold
    /// where the range is placed, and the range's index: the edits the
    /// host has not heard changed it before the host locked it. The range
    /// locks nothing on screen meanwhile; a save, a change event and the
    /// value wait (P1) until the text holds it again.
    pub(super) ranges_changed: RwSignal<Option<(String, usize)>>,
    /// The locks of the text on screen.
    pub(crate) protected: Memo<Arc<Protected>>,
    /// The popups, where the lock tip shows.
    popups: PopupState,
    /// Small bookkeeping, in one slot: every editor keeps its handles.
    pub(crate) marks: StoredValue<Marks>,
    /// Secret placeholders (see `secrets`).
    pub(crate) secrets: SecretState,
    /// The placeholders of the other editors of a documents view, this
    /// one's own among them (see `foreign`).
    pub(crate) foreign: Signal<Arc<Vec<DocumentPlaceholders>>>,
}

/// What the memo of locks reads.
pub(crate) struct Inputs {
    pub(crate) source: RwSignal<String>,
    pub(crate) edit_language: RwSignal<Language>,
    pub(crate) host_language: Language,
    pub(crate) schema: Option<Arc<SchemaBundle>>,
    pub(crate) host_text: super::super::sync::HostTextReader,
    pub(crate) popups: PopupState,
    pub(crate) channel: HostChannel,
}

impl ProtectState {
    pub(crate) fn new(init: ProtectInit, inputs: Inputs) -> Self {
        let paths = init.paths.unwrap_or_default();
        let secrets = init.secrets.unwrap_or_default();
        let foreign = init.foreign.unwrap_or_default();
        if let Err(error) = secrets.check_placeholder() {
            // Also a document problem: no token is a placeholder meanwhile.
            leptos::logging::error!("config-editor: secrets.placeholder: {error}");
        }
        let Inputs {
            source,
            edit_language,
            host_language,
            schema,
            host_text,
            popups: inputs_popups,
            channel,
        } = inputs;
        let secrets = host_text.with_value(|host| {
            source.with_untracked(|shown| {
                SecretState::new(
                    secrets,
                    (host_language.behavior(), host),
                    (edit_language.get_untracked().behavior(), shown),
                    schema.as_deref().map(|bundle| &bundle.schema),
                )
            })
        });
        // The ranges given before the editor mounted (a binding's), in the
        // text it mounts with.
        let mounted_with = channel.with(|link| {
            let rewrite = |from: &str, to: &str, ranges: &[TextRange]| {
                let reading = schema.as_deref().map(|bundle| &bundle.schema);
                followed_to(host_language.behavior(), from, to, ranges, reading)
            };
            link.placed(&rewrite).0
        });
        let in_host = host_text.with_value(|host| HostRanges::new(host, &[]));
        let local = RwSignal::new(if edit_language.get_untracked() == host_language {
            mounted_with
        } else {
            host_text.with_value(|host| in_host.keep(host, mounted_with));
            Vec::new()
        });
        let ranges_given = RwSignal::new(0);
        let channel = StoredValue::new_local(channel);
        let marks = StoredValue::new(Marks::default());
        let covered = Covered(StoredValue::new(None));
        let protected = Memo::new_with_compare(
            move |previous: Option<&Arc<Protected>>| {
                let epoch = marks.with_value(|marks| marks.epoch);
                // Values pinned in an earlier text of the same host text and
                // language carry over a text that does not parse.
                let previous = previous
                    .map(Arc::as_ref)
                    .filter(|previous| previous.epoch == epoch);
                let edit = edit_language.get();
                let reading = schema.as_deref().map(|bundle| &bundle.schema);
                let marking = schema
                    .as_deref()
                    .filter(|bundle| bundle.has_marks)
                    .map(|bundle| &bundle.schema);
                let fields = paths.get();
                let mut range_fields = Vec::new();
                let local_ranges = if edit == host_language {
                    local.get()
                } else {
                    // Edit-as: the host's ranges lock the fields they cover.
                    ranges_given.track();
                    if channel.with_value(|channel| channel.with(|link| link.has_ranges())) {
                        host_text.with_value(|host| {
                            let placed = in_host.in_text(host_language.behavior(), host, reading);
                            range_fields = covered.found(host_language, host, &placed, reading);
                        });
                    }
                    Vec::new()
                };
                source.with(|text| {
                    let followed = marks
                        .with_value(|marks| marks.followed.clone())
                        .filter(|followed| followed.made(previous, text));
                    let caret = followed.as_ref().and_then(|followed| followed.caret);
                    let change = followed
                        .as_ref()
                        .and_then(|followed| followed.change.clone());
                    // Taken only for the text it was found for: the memo may
                    // run between the check and the write (the host's
                    // ranges followed first), on the text before.
                    let fits = |(before, foreseen): &(String, Protected)| {
                        previous.is_some_and(|previous| previous.text == *before)
                            && foreseen_fits(foreseen, previous, text, &fields, &local_ranges)
                            && foreseen.range_fields == range_fields
                            && foreseen.marking == marking.is_some()
                    };
                    let foreseen = marks
                        .try_update_value(|marks| {
                            marks
                                .foreseen
                                .as_ref()
                                .is_some_and(fits)
                                .then(|| marks.foreseen.take())
                        })
                        .flatten()
                        .flatten();
                    if let Some((_, mut protected)) = foreseen {
                        protected.epoch = epoch;
                        return Arc::new(protected);
                    }
                    let mut protected = protect(
                        edit.behavior(),
                        text,
                        Sources {
                            marking_schema: marking,
                            reading_schema: reading,
                            paths: &fields,
                            ranges: &local_ranges,
                            range_fields: &range_fields,
                            previous,
                            caret,
                            change: change.as_deref(),
                        },
                    );
                    protected.epoch = epoch;
                    Arc::new(protected)
                })
            },
            locks_changed,
        );
        ProtectState {
            host_paths: paths,
            channel,
            ranges_given,
            ranges: local,
            in_host,
            ranges_changed: RwSignal::new(None),
            protected,
            popups: inputs_popups,
            marks,
            secrets,
            foreign,
        }
    }

    /// The field the parse reads a placeholder given whole in with text
    /// beside it, after an edit taken for leaving the placeholder and its
    /// edges alone (see `whole_guard::Held`), while `text` (written in
    /// `behavior`) does not read it as the host's text does. The hold is
    /// kept until the host gives a text: an undo may bring such a text
    /// back.
    pub(crate) fn placeholder_held(
        &self,
        behavior: &dyn ConfigLanguage,
        text: &str,
    ) -> Option<String> {
        let held = self.secrets.held.get_value()?;
        let origin = self
            .protected
            .with_untracked(|protected| protected.origin.clone());
        let still = self.secrets.known.with_untracked(|known| {
            self.secrets.matcher.with_value(|matcher| {
                held.still(behavior, (known, matcher), text, origin.as_deref())
            })
        });
        still.then_some(held.field)
    }

    /// The field of a placeholder given whole that `text` (written in
    /// `behavior`), which parses, reads with other text beside it in a
    /// value out of its field (see `glued::glued_elsewhere`). Read only
    /// while some placeholder is out of its field (fewer real ones on
    /// screen than the host wrote in fields) or a copy of one is written
    /// (more on screen than are real): one in its field is judged at each
    /// keystroke, and a parse per change would cost a key a read of the
    /// document. A copy is judged here, by the parse, wherever the edits
    /// that glued it were made (text typed beside it while the text did not
    /// parse, a repair far from it, its key renamed off a field that held
    /// it).
    pub(crate) fn glued_away(&self, behavior: &dyn ConfigLanguage, text: &str) -> Option<String> {
        let secrets = self.secrets;
        let real = secrets.tokens.with_untracked(Vec::len);
        let away = secrets.known.with_untracked(|known| {
            let written: usize = known
                .iter()
                .filter(|known| known.whole.is_some() && known.field.is_some())
                .map(|known| known.count)
                .sum();
            written > 0 && real < written
        });
        if !away && !self.copies_written(text, real) {
            return None;
        }
        let parsed = parse_cache::parsed(behavior, text);
        let document = parsed.as_ref().as_ref().ok()?;
        secrets
            .known
            .with_untracked(|known| glued_elsewhere(document, known)?.field.clone())
    }

    /// Whether `text` writes more of the host's tokens given whole than the
    /// `real` ones: a copy is written (one pass of the pattern).
    fn copies_written(&self, text: &str, real: usize) -> bool {
        let secrets = self.secrets;
        secrets.known.with_untracked(|known| {
            if !known.iter().any(|known| known.whole.is_some()) {
                return false;
            }
            secrets.matcher.with_value(|matcher| {
                matcher
                    .find(text)
                    .into_iter()
                    .filter(|range| {
                        known.iter().any(|known| {
                            known.whole.is_some()
                                && known.token == text.get(range.clone()).unwrap_or_default()
                        })
                    })
                    .count()
                    > real
            })
        })
    }

    /// Takes the next text as the host's own (a controlled value, a reload,
    /// an Edit-as switch): the locked values are read from it afresh, never
    /// carried from the text it replaces.
    pub(crate) fn trust_text(&self) {
        self.marks.update_value(|marks| {
            marks.epoch += 1;
            marks.followed = None;
        });
    }

    /// Shows `message` at byte `offset` of `text` for a moment: why an
    /// edit was refused.
    pub(crate) fn flash(&self, message: String, text: &str, offset: TextSize) {
        // Placed on screen, in UTF-16 units of the text.
        self.popups.flash_lock(message, utf16_of(text, offset));
    }

    /// Hides the lock tip (only a hover one with `hover_only`).
    pub(crate) fn hide_tip(&self, hover_only: bool) {
        self.popups.hide_lock(hover_only);
    }

    /// The lock tip, while one shows.
    #[cfg(test)]
    pub(crate) fn tip(&self) -> Option<super::super::popups::LockTip> {
        self.popups.lock_tip_untracked()
    }
}

/// The fields the host's ranges cover in its text, kept while the text and
/// the ranges stay the same: an edit in an Edit-as view leaves the host's
/// text as it is until it converts back, so it is not read again for each.
#[derive(Clone, Copy)]
struct Covered(StoredValue<Option<CoveredFields>>);

/// A host text, its read-only ranges and the fields they
/// cover.
type CoveredFields = (String, Vec<TextRange>, Vec<FieldPath>);

impl Covered {
    /// [`covered_fields`], from the last ones found when they were of the
    /// same text and ranges.
    fn found(
        self,
        language: Language,
        host: &str,
        ranges: &[TextRange],
        reading: Option<&serde_json::Value>,
    ) -> Vec<FieldPath> {
        let kept = self.0.with_value(|kept| {
            kept.as_ref()
                .filter(|(text, known, _)| text == host && known == ranges)
                .map(|(_, _, fields)| fields.clone())
        });
        kept.unwrap_or_else(|| {
            let fields = covered_fields(language, host, ranges, reading);
            self.0
                .set_value(Some((host.to_string(), ranges.to_vec(), fields.clone())));
            fields
        })
    }
}

/// Whether a locked field's own key is written at the start of another
/// line too, after its indent (a second `limits:` or `id =`).
fn locked_key_again(protected: &Protected) -> bool {
    let starts: Vec<&str> = protected.text.lines().map(str::trim_start).collect();
    protected.locks.iter().any(|lock| {
        let Some(key) = lock
            .key
            .and_then(|key| text::slice::slice(&protected.text, key))
        else {
            return false;
        };
        let written = |line: &&str| {
            line.strip_prefix(key)
                .is_some_and(|rest| !rest.starts_with(|c: char| c.is_alphanumeric() || c == '_'))
        };
        !key.trim().is_empty() && starts.iter().filter(|line| written(line)).count() > 1
    })
}

/// The fields the host's `ranges` cover in its own `host`
/// text, written in `language` (none when it does not parse): what they
/// lock while editing as another language.
fn covered_fields(
    language: Language,
    host: &str,
    ranges: &[TextRange],
    reading: Option<&serde_json::Value>,
) -> Vec<FieldPath> {
    let behavior = language.behavior();
    let Ok(document) = codec::read(behavior, host, reading) else {
        return Vec::new();
    };
    range_pointers(behavior, host, &document, ranges)
        .into_iter()
        .map(FieldPath::from)
        .collect()
}

/// The host's ranges (of its text `old`) keep their text in
/// its next text `new`: an Edit-as view locks the fields a range covers by
/// their values, and has nothing to lock of the range's other text (a
/// comment, the line break before a key), so the save back holds the
/// range's text as written (at least as often as before; a range of only
/// whitespace, which formatting writes everywhere, at least once).
fn text_ranges_kept(old: &str, new: &str, ranges: &[TextRange]) -> Result<(), ReadOnlyChange> {
    if ranges.is_empty() {
        return Ok(());
    }
    // Most ranges keep their text where the conversion kept their lines;
    // only the others are looked for.
    let moves = Moves::new(old, new);
    for range in ranges {
        let Some(text) = text::slice::slice(old, *range) else {
            continue;
        };
        let kept = moves
            .range(range)
            .is_some_and(|to| text::slice::slice(new, to) == Some(text));
        if kept || text.is_empty() {
            continue;
        }
        let wanted = if text.trim().is_empty() {
            1
        } else {
            old.matches(text).count()
        };
        if new.matches(text).count() < wanted {
            return Err(ReadOnlyChange {
                pointer: String::new(),
            });
        }
    }
    Ok(())
}

/// The host's `given` ranges (of `host`) as `placed` in `text`
/// (in the same order). One whose text `text` does not hold there takes
/// its place in `others` (the same ranges placed another way, made only
/// then) when that holds it; else it is made empty where it starts: an
/// edit the host has not heard changed it before the host locked it, so
/// it locks nothing on screen, and the first such range's text and index
/// say why the save waits. Edits that write the text back (an undo) place
/// it again (see `follow_edit`).
fn keeping_their_text(
    locked: &[String],
    text: &str,
    mut placed: Vec<TextRange>,
    others: impl FnOnce() -> Vec<TextRange>,
) -> (Vec<TextRange>, Option<(String, usize)>) {
    // A range past the text's end holds what the text has of it.
    fn slice<'t>(of: &'t str, range: &TextRange) -> Option<&'t str> {
        let end = range.end().min(TextSize::of(of));
        text::slice::slice(of, TextRange::new(range.start().min(end), end))
    }
    let mut others = Some(others);
    let mut other: Vec<TextRange> = Vec::new();
    let mut changed = None;
    for (index, (locked, placed)) in locked.iter().zip(placed.iter_mut()).enumerate() {
        let locked = locked.as_str();
        if slice(text, placed) == Some(locked) {
            continue;
        }
        if let Some(others) = others.take() {
            other = others();
        }
        match other.get(index) {
            Some(found) if slice(text, found) == Some(locked) => *placed = *found,
            _ => {
                *placed = TextRange::empty(placed.start());
                changed.get_or_insert_with(|| (locked.to_string(), index));
            }
        }
    }
    (placed, changed)
}

/// Why a save waits on a host range whose text `locked` was changed before
/// the host locked it.
pub(super) fn range_changed_message(strings: &EditorStrings, locked: &str) -> String {
    let quoted = format!("\"{locked}\"");
    save_held(
        strings,
        &quoted,
        &Reason::Range,
        HoldCause::ChangedBeforeLock,
    )
}

/// Whether the locks the guard found (`foreseen`) are those the memo would
/// find for `text`: from the same locks before, paths and host ranges.
fn foreseen_fits(
    foreseen: &Protected,
    previous: Option<&Protected>,
    text: &str,
    paths: &[FieldPath],
    ranges: &[TextRange],
) -> bool {
    let Some(previous) = previous else {
        return false;
    };
    let placed: Vec<TextRange> = foreseen
        .locks
        .iter()
        .filter(|lock| lock.pointer.is_none())
        .flat_map(|lock| lock.ranges.iter().copied())
        .collect();
    let wanted: Vec<&TextRange> = ranges.iter().filter(|range| !range.is_empty()).collect();
    foreseen.text == text
        && foreseen.origin == previous.origin
        && foreseen.paths.len() == paths.len()
        && foreseen
            .paths
            .iter()
            .zip(paths)
            .all(|(pointer, path)| *pointer == path.to_pointer())
        && placed.iter().eq(wanted)
}

/// Whether the locks memo has news for its readers: the text it was read
/// from and the documents alone do not count.
fn locks_changed(old: Option<&Arc<Protected>>, new: Option<&Arc<Protected>>) -> bool {
    match (old, new) {
        (Some(old), Some(new)) => {
            old.locks != new.locks
                || old.secret != new.secret
                || old.pins != new.pins
                || old.marked != new.marked
                || old.unplaced != new.unplaced
                || old.held != new.held
                || old.epoch != new.epoch
        }
        _ => true,
    }
}

impl EditorState {
    /// Places the host's ranges again each time the host changes them, and
    /// asks again what the old locks refused each time it changes its
    /// ranges or paths.
    ///
    /// One effect does both, in that order: with two, the one asking again
    /// could run first when the host changes its paths and then its ranges
    /// in one task (a framework binding gives each prop its own update),
    /// judge the text by ranges not placed yet, and leave an edit the old
    /// locks held back unsent once they are gone.
    pub(crate) fn connect_protection(&self) {
        let state = *self;
        Effect::new(move |previous: Option<()>| {
            state.protect().host_paths.track();
            state.protect().ranges_given.track();
            // New locks from the host (placed as they came): what the old
            // ones refused is asked again (a key at the refused place, an
            // Edit-as edit the pause could not convert).
            if previous.is_some() {
                untrack(|| state.locks_changed_by_host());
            }
        });
    }

    /// The host gave new ranges (or a text they were carried into): they
    /// are placed at once, and what the old locks refused is asked again
    /// once the effects run.
    pub(crate) fn host_ranges_given(&self) {
        self.place_ranges();
        self.protect().ranges_given.update(|given| *given += 1);
    }

    /// Whether the host gave read-only ranges.
    fn has_host_ranges(&self) -> bool {
        self.protect()
            .channel
            .try_with_value(|channel| channel.with(|link| link.has_ranges()))
            .unwrap_or(false)
    }

    /// `ranges` of `from`, a text in the host's language, in `to`, a text
    /// no edit made of it (a conversion back from another language): by
    /// position where their text was kept, else at the fields they cover.
    pub(crate) fn rewrite_ranges(
        &self,
        from: &str,
        to: &str,
        ranges: &[TextRange],
    ) -> Vec<TextRange> {
        self.config().schema.with_value(|schema| {
            let reading = schema.as_deref().map(|bundle| &bundle.schema);
            followed_to(
                self.config().host_language.behavior(),
                from,
                to,
                ranges,
                reading,
            )
        })
    }

    /// The host changed its locks: a refusal of the old ones holds no more,
    /// and a change event they held back is sent now that they release it
    /// (P4: the host has the text on screen without waiting for a key).
    pub(super) fn locks_changed_by_host(&self) {
        let text = self.doc().text.get_untracked();
        let edit = self.sync().edit_language.get_untracked();
        if edit != self.config().host_language {
            if !self.sync().is_synced(&text) {
                super::super::sync::convert_now(*self, text, edit);
            }
            return;
        }
        if self.protect().marks.with_value(|marks| marks.held_back) {
            self.notify_change();
        }
    }

    /// Notes whether the host's gate (see `sync::gate`) held the last text
    /// back, for [`Self::locks_changed_by_host`].
    pub(crate) fn note_held_back(&self, held: bool) {
        self.protect()
            .marks
            .update_value(|marks| marks.held_back = held);
    }

    /// The edit pipeline's check stage: whether `edit` keeps every lock;
    /// when not, why the field is locked.
    pub(crate) fn check_edit(&self, edit: &Edit) -> Result<(), String> {
        let change = edit.change();
        let carets = Carets {
            caret: Some(edit.caret()),
            written: edit.written_end(),
            change: change.as_ref(),
        };
        let followed = self
            .protect()
            .protected
            .with_untracked(|protected| {
                self.config().schema.with_value(|schema| {
                    judge(
                        self.behavior(),
                        schema.as_deref().map(|bundle| &bundle.schema),
                        protected,
                        (&edit.before, &edit.text),
                        carets,
                    )
                })
            })
            .map_err(|blocked: Blocked| {
                self.strings_untracked(|strings| blocked.message(strings))
            })?;
        let foreseen = followed.map(|followed| (edit.before.clone(), followed));
        self.protect()
            .marks
            .update_value(|marks| marks.foreseen = foreseen);

        match self.glued_by(&edit.before, &edit.text) {
            Some(pointer) => {
                Err(self.strings_untracked(|strings| glued_message(strings, &pointer)))
            }
            None => Ok(()),
        }
    }

    /// The field of a placeholder given whole that the edit from `before`
    /// to `after` writes text beside, or changes the value of (see
    /// `whole_guard`). Remembers whether `after` parses, so the next edit's
    /// check needs no parse to tell an edit out of every placeholder's reach.
    fn glued_by(&self, before: &str, after: &str) -> Option<String> {
        let secrets = self.protect().secrets;
        let behavior = self.behavior();
        let before_parses = secrets
            .parsing
            .with_value(|known| known.as_deref() == Some(before))
            || parse_cache::known_to_parse(behavior, before) == Some(true);
        let index = secrets.whole.get_untracked();
        let origin = self
            .protect()
            .protected
            .with_untracked(|p| p.origin.clone());
        let label = behavior.info().label;
        let earlier = secrets
            .parsed
            .with_value(|parsed| {
                parsed
                    .as_ref()
                    .filter(|(known, text, _)| *known == label && text == before)
                    .map(|(_, _, parse)| std::rc::Rc::clone(parse))
            })
            .or_else(|| parse_cache::kept(behavior, before));
        let verdict = secrets.known.with_untracked(|known| {
            secrets.tokens.with_untracked(|tokens| {
                secrets.matcher.with_value(|matcher| {
                    Placeholders {
                        behavior,
                        known,
                        index: &index,
                        tokens,
                        matcher,
                        origin: origin.as_deref(),
                        holding: secrets.held.with_value(Option::is_some),
                        earlier: earlier.as_deref().and_then(|parse| parse.as_ref().ok()),
                    }
                    .check(before, after, before_parses)
                })
            })
        });
        if verdict.glued.is_none() {
            secrets
                .parsing
                .set_value((verdict.parses == Some(true)).then(|| after.to_string()));
            // The parse the check made, for the next edit's text before.
            let parse = parse_cache::kept(behavior, after);
            secrets
                .parsed
                .set_value(parse.map(|parse| (label, after.to_string(), parse)));
        }
        // An Edit-as view's text reaches the host converted, checked there
        // (`check_host_save`), not through the hold: refused as before.
        if self.config().host_language != self.sync().edit_language.get_untracked() {
            return verdict.glued.or(verdict.held.map(|held| held.field));
        }
        if let Some(held) = verdict.held {
            secrets.held.update_value(|now| match now {
                Some(now) => {
                    now.field = held.field;
                    now.tokens.extend(held.tokens);
                    now.tokens.sort();
                    now.tokens.dedup();
                }
                None => *now = Some(held),
            });
        }
        verdict.glued
    }

    /// Finds again what is kept by position in a text replaced whole (see
    /// `edit::Origin`): the host's ranges after a host text, and the secret
    /// placeholders, the host's own after a text from the host.
    pub(crate) fn find_protected_again(&self, origin: &Origin) {
        // A text from the host is its own: nothing typed is held in it.
        if matches!(origin, Origin::Reload { .. } | Origin::HostValue) {
            self.protect().secrets.held.set_value(None);
        }
        match origin {
            Origin::Reload { host } => self.adopt_host_tokens(host),
            // A new text from the host: its ranges and placeholders anew. Not
            // on an echo of the editor's own edit, where they are followed
            // (a pasted look-alike there never becomes the host's).
            Origin::HostValue => {
                self.place_ranges();
                self.rescan_tokens(true);
            }
            Origin::Switch => {
                self.follow_ranges_across_switch();
                self.rescan_tokens(false);
            }
            // An undo or redo is followed as any edit (`follow_tokens`): a
            // placeholder it writes back is found by its path or, in a text
            // that does not parse, as often as the host gave it, the ones
            // it kept first. Found again by their text, a copy written
            // before the real one would take its place.
            Origin::Command
            | Origin::Input { .. }
            | Origin::Undo
            | Origin::Redo
            | Origin::HostChange => {}
        }
    }

    /// Follows an applied edit with what is kept by position (the host's
    /// ranges, the secret placeholders).
    pub(crate) fn follow_edit(&self, edit: &Edit) {
        let (before, after) = (edit.before.as_str(), edit.text.as_str());
        let caret = edit.caret();
        let change = edit.change().map(Arc::new);
        let (shared_before, shared_after) = (Arc::<str>::from(before), Arc::<str>::from(after));
        self.protect().marks.update_value(|marks| {
            marks.followed = Some(Followed {
                before: shared_before,
                after: shared_after,
                caret: edit.written_end(),
                change: change.clone(),
            });
        });
        self.follow_tokens(before, after, caret, change.as_deref());
        if self.protect().ranges.with_untracked(Vec::is_empty) {
            return;
        }
        // A range whose text the text on screen does not hold is placed
        // from the host's text again after each edit: the edit writing its
        // text back releases the save.
        if self
            .protect()
            .ranges_changed
            .with_untracked(Option::is_some)
        {
            self.place_ranges_in(after);
            return;
        }
        self.protect().ranges.update(|ranges| {
            *ranges = follow_ranges(ranges, before, after, caret, change.as_deref());
        });
    }

    /// Places the host's ranges again (they changed, or the host replaced
    /// the text): on the text on screen in the host's language, else in the
    /// host's text, where each conversion back follows them.
    pub(crate) fn place_ranges(&self) {
        if self.config().host_language == self.sync().edit_language.get_untracked() {
            let text = self.doc().text.get_untracked();
            self.place_ranges_in(&text);
            return;
        }
        let (placed, _, document) = self.given_ranges();
        self.sync().host_text.with_value(|host| {
            let placed = if *document == *host {
                placed
            } else {
                self.rewrite_ranges(&document, host, &placed)
            };
            self.protect().in_host.keep(host, placed);
        });
        self.protect().ranges_changed.set(None);
        self.protect().ranges.set(Vec::new());
    }

    /// The host's ranges, mapped from the text each names to the document
    /// of the host log, the text each held there, and that document.
    fn given_ranges(&self) -> (Vec<TextRange>, Vec<String>, Arc<str>) {
        let rewrite =
            |from: &str, to: &str, ranges: &[TextRange]| self.rewrite_ranges(from, to, ranges);
        self.protect()
            .channel
            .try_with_value(|channel| {
                channel.with(|link| {
                    let (placed, texts) = link.placed(&rewrite);
                    (placed, texts, link.log.document().clone())
                })
            })
            .unwrap_or_else(|| (Vec::new(), Vec::new(), Arc::from("")))
    }

    /// Places the host's ranges in `text`, the text on screen in the
    /// host's language: mapped from the text each names through what the
    /// edits since recorded they did (the host log), never read off two
    /// texts but where no record reaches (a text no edit made). A range
    /// whose text `text` does not hold where it lands locks nothing there
    /// and holds the save (see [`ProtectState::ranges_changed`]).
    fn place_ranges_in(&self, text: &str) {
        let (placed, texts, document) = self.given_ranges();
        if placed.is_empty() {
            self.protect().ranges_changed.set(None);
            self.protect().ranges.set(placed);
            return;
        }
        let placed = if *document == *text {
            placed
        } else {
            self.rewrite_ranges(&document, text, &placed)
        };
        // Where the fields they covered in the text they name are written
        // in `text`, for a range whose text an edit changed.
        let followed = || {
            let given = self
                .protect()
                .channel
                .try_with_value(|channel| channel.with(Link::given_at_anchor))
                .flatten();
            match given {
                Some((anchor, ranges)) => self.rewrite_ranges(&anchor, text, &ranges),
                None => Vec::new(),
            }
        };
        let (placed, changed) = keeping_their_text(&texts, text, placed.clone(), followed);
        self.protect().ranges_changed.set(changed);
        self.protect().ranges.set(placed);
    }

    /// Before a switch away from the host's language, whose text was
    /// `before`: its ranges, as followed on screen, are kept in that text,
    /// where the host's next texts follow them from.
    pub(crate) fn leave_ranges(&self, before: &str) {
        let ranges = self.protect().ranges.get_untracked();
        self.protect().in_host.keep(before, ranges);
    }

    /// After a switch: back in the host's language its ranges are where
    /// the conversions back followed them; in another, the fields they
    /// cover are locked by path.
    fn follow_ranges_across_switch(&self) {
        let host = self.config().host_language == self.sync().edit_language.get_untracked();
        let placed = if host {
            self.config().schema.with_value(|schema| {
                let reading = schema.as_deref().map(|bundle| &bundle.schema);
                self.doc().text.with_untracked(|text| {
                    self.protect()
                        .in_host
                        .in_text(self.behavior(), text, reading)
                })
            })
        } else {
            Vec::new()
        };
        self.protect().ranges.set(placed);
    }

    /// Whether the text on screen may not reach the host, by any route
    /// (change events, the value, a save): a locked field reads otherwise
    /// than pinned (see `locks::Protected::held`), the text does not parse
    /// and holds locks, or it reads a placeholder given whole with text
    /// beside it (refused at the keystroke where it can be told; a parse
    /// reading one glued after edits made while the text did not parse,
    /// its key renamed, waits here).
    pub(crate) fn changes_held(&self) -> bool {
        // A text that does not parse writing a locked key again (a YAML key
        // written twice does not parse, and a reader taking the last would
        // read another value); one leaving its locked text alone is the
        // user's half-typed text.
        self.protect().protected.with_untracked(|protected| {
            !protected.held.is_empty()
                || (protected.document.is_none() && locked_key_again(protected))
        }) || self.unplaced_waits().is_some()
            || self
                .protect()
                .ranges_changed
                .with_untracked(Option::is_some)
            || self.placeholder_held().is_some()
            || self.glued_away().is_some()
    }

    /// Why a save of the text on screen waits, when it does: it holds text
    /// beside a placeholder the host gave as a field's whole value, or it
    /// does not parse and holds a read-only field the language cannot
    /// locate (it gives no spans), so nothing shows that its value is kept.
    pub(crate) fn save_waits(&self) -> Option<String> {
        self.glued_waits()
            .or_else(|| self.held_waits())
            .or_else(|| self.range_changed_waits())
            .or_else(|| self.unplaced_waits())
            .or_else(|| {
                let field = self.placeholder_held()?;
                Some(self.strings_untracked(|strings| save_held_placeholder(strings, &field)))
            })
    }

    /// [`ProtectState::placeholder_held`] for the text on screen.
    fn placeholder_held(&self) -> Option<String> {
        self.doc()
            .text
            .with_untracked(|text| self.protect().placeholder_held(self.behavior(), text))
    }

    /// The locked field a save of the text on screen waits on because it
    /// reads otherwise than pinned (see `locks::Protected::held`), with
    /// the cause recorded for it.
    fn held_waits(&self) -> Option<String> {
        self.protect().protected.with_untracked(|protected| {
            let (pointer, cause) = protected.held.causes().next()?;
            let reason = protected.held_reason(pointer);
            Some(self.strings_untracked(|strings| save_held(strings, pointer, &reason, cause)))
        })
    }

    /// Why a save waits on a host range whose text the text on screen does
    /// not hold (see [`ProtectState::ranges_changed`]).
    fn range_changed_waits(&self) -> Option<String> {
        let (locked, _) = self.protect().ranges_changed.get_untracked()?;
        Some(self.strings_untracked(|strings| range_changed_message(strings, &locked)))
    }

    /// The glued placeholder a save of the text on screen waits on.
    fn glued_waits(&self) -> Option<String> {
        if let Some(field) = self.glued_away() {
            return Some(self.strings_untracked(|strings| glued_message(strings, &field)));
        }
        let secrets = self.protect().secrets;
        if !secrets
            .known
            .with_untracked(|known| known.iter().any(|known| known.whole.is_some()))
        {
            return None;
        }
        let parsed = self
            .doc()
            .text
            .with_untracked(|text| parse_cache::parsed(self.behavior(), text));
        let document = parsed.as_ref().as_ref().ok()?;
        let pointer = secrets.known.with_untracked(|known| {
            let text = self.doc().text.get_untracked();
            secrets.matcher.with_value(|matcher| {
                first_glued(self.behavior(), (&text, document), known, matcher)
                    .and_then(|known| known.field.clone())
            })
        })?;
        Some(self.strings_untracked(|strings| glued_message(strings, &pointer)))
    }

    /// [`ProtectState::glued_away`] for the text on screen.
    fn glued_away(&self) -> Option<String> {
        self.doc()
            .text
            .with_untracked(|text| self.protect().glued_away(self.behavior(), text))
    }

    /// The read-only field the language cannot locate in the text on
    /// screen, which does not parse, that a save waits on.
    fn unplaced_waits(&self) -> Option<String> {
        let pointer = self.protect().protected.with_untracked(|protected| {
            protected
                .unplaced
                .first()
                .map(|(pointer, _)| pointer.clone())
        })?;
        let parses = self
            .doc()
            .text
            .with_untracked(|text| codec::read(self.behavior(), text, None).is_ok());
        (!parses).then(|| self.strings_untracked(|strings| save_waits(strings, &pointer)))
    }

    /// Refuses an Edit-as save from the host's `old` text to `new` that would
    /// change a read-only value, or write text beside a placeholder the
    /// host gave as a field's whole value.
    pub(crate) fn check_host_save(&self, old: &str, new: &str) -> Result<(), SaveError> {
        self.check_read_only_save(old, new)
            .map_err(SaveError::ReadOnly)?;
        let behavior = self.config().host_language.behavior();
        let secrets = self.protect().secrets;
        let glued = self.config().schema.with_value(|schema| {
            let reading = schema.as_deref().map(|bundle| &bundle.schema);
            secrets.known.with_untracked(|known| {
                secrets.matcher.with_value(|matcher| {
                    glued_in_save(behavior, (known, matcher), old, new, reading)
                })
            })
        });
        if let Some(pointer) = glued {
            return Err(SaveError::GluedPlaceholder { pointer });
        }
        let moved = self.config().schema.with_value(|schema| {
            let reading = schema.as_deref().map(|bundle| &bundle.schema);
            secrets.known.with_untracked(|known| {
                secrets.matcher.with_value(|matcher| {
                    super::moved_off::moved_off_in_save(behavior, matcher, known, new, reading)
                })
            })
        });
        match moved {
            Some((pointer, owner)) => Err(SaveError::MovedPlaceholder { pointer, owner }),
            None => Ok(()),
        }
    }

    /// [`Self::check_host_save`]'s read-only values.
    fn check_read_only_save(&self, old: &str, new: &str) -> Result<(), ReadOnlyChange> {
        self.config().schema.with_value(|schema| {
            let reading = schema.as_deref().map(|bundle| &bundle.schema);
            let marking = schema
                .as_deref()
                .filter(|bundle| bundle.has_marks)
                .map(|bundle| &bundle.schema);
            let behavior = self.config().host_language.behavior();
            let paths = self.protect().host_paths.get_untracked();
            let ranges = if !self.has_host_ranges() {
                Vec::new()
            } else {
                self.protect().in_host.in_text(behavior, old, reading)
            };
            if marking.is_none() && paths.is_empty() && ranges.is_empty() {
                return Ok(());
            }
            let pointers = locked_pointers(
                behavior,
                old,
                Sources {
                    marking_schema: marking,
                    reading_schema: reading,
                    paths: &paths,
                    ranges: &ranges,
                    range_fields: &[],
                    previous: None,
                    caret: None,
                    change: None,
                },
            );
            check_save(behavior, reading, old, new, &pointers)?;
            text_ranges_kept(old, new, &ranges)
        })
    }

    /// Byte ranges of `text` (the text on screen) no replace may touch
    /// (find and replace's Replace All).
    #[cfg(feature = "search")]
    pub(crate) fn protected_bytes(&self, text: &str) -> Vec<std::ops::Range<usize>> {
        let tokens = self.solid_tokens(text);
        self.protect().protected.with_untracked(|protected| {
            protected
                .locks
                .iter()
                .flat_map(|lock| lock.ranges.iter().chain(&lock.guards))
                .chain(&tokens)
                .map(|range| std::ops::Range::<usize>::from(*range))
                .collect()
        })
    }

    /// What the protected text at byte `offset` is, with its label: a
    /// secret placeholder's hint, or why a read-only field is read-only.
    /// With `ends`, an offset just at either end of it counts (the caret
    /// beside it).
    pub(crate) fn protected_note(&self, offset: TextSize, ends: bool) -> Option<(String, String)> {
        let covers = |range: &TextRange| {
            range.start() <= offset && (offset < range.end() || (ends && offset == range.end()))
        };
        let secrets = self.protect().secrets;
        if secrets
            .tokens
            .with_untracked(|tokens| tokens.iter().any(covers))
        {
            return Some(self.strings_untracked(|strings| {
                let hint = secrets
                    .options
                    .with_value(|options| options.hint.clone())
                    .unwrap_or_else(|| strings.secret_hint.clone());
                (strings.secret_label.clone(), hint)
            }));
        }
        self.protect().protected.with_untracked(|protected| {
            protected
                .locks
                .iter()
                .find(|lock| lock.ranges.iter().any(covers))
                .map(|lock| {
                    self.strings_untracked(|strings| {
                        (strings.read_only_label.clone(), lock.reason.text(strings))
                    })
                })
        })
    }

    /// Shows what the protected text at zero-based `line`:`column` is, for
    /// the pointer; its message, or `None` when nothing protected is there.
    pub(crate) fn hover_protected(&self, line: usize, column: usize) -> Option<String> {
        let no_tokens = self.protect().secrets.tokens.with_untracked(Vec::is_empty);
        if no_tokens
            && self
                .protect()
                .protected
                .with_untracked(|protected| protected.is_empty())
        {
            self.protect().hide_tip(true);
            return None;
        }
        let found = self.doc().text.with_untracked(|text| {
            let offset = LineIndex::new(text).offset(line + 1, column + 1);
            // The tip is placed on screen, in UTF-16 units of the text.
            self.protected_note(offset, false)
                .map(|(_, message)| (message, utf16_of(text, offset)))
        });
        let Some((message, offset)) = found else {
            self.protect().hide_tip(true);
            return None;
        };
        self.protect().popups.hover_lock(message.clone(), offset);
        Some(message)
    }
}
