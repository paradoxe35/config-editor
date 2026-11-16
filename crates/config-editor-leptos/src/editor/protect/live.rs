//! Protected fields in the live editor: the locks of the text on screen
//! (kept by a memo), the check every edit passes, and the tooltip saying
//! why one was refused.

use std::ops::Range;
use std::sync::Arc;

use leptos::prelude::*;

use super::super::codec;
use super::super::edit::{Change, CodeUnits, Edit, Origin};
use super::super::edit_as::SaveError;
use super::super::language::ConfigLanguage;
use super::super::languages::Language;
use super::super::parse_cache;
use super::super::paths::{FieldPath, TextRange};
use super::super::popups::PopupState;
use super::super::schema::SchemaBundle;
use super::super::secret_options::SecretOptions;
use super::super::state::EditorState;
use super::carry::Moves;
use super::follow::follow_ranges;
use super::foreign::DocumentPlaceholders;
use super::glued::{first_glued, glued_elsewhere, glued_in_save, glued_message};
use super::guard::{Blocked, Carets, judge};
use super::host_ranges::{HostRanges, offsets};
use super::locate::LineIndex;
use super::locks::{Protected, Sources, protect};
use super::save::{ReadOnlyChange, check_save, locked_pointers, range_pointers};
use super::secrets::SecretState;
use super::texts::{save_held, save_waits};
use super::whole_guard::Placeholders;

/// The host's side of protection (the component's props, when given).
pub(crate) struct ProtectInit {
    pub(crate) paths: Option<Signal<Vec<FieldPath>>>,
    pub(crate) ranges: Option<Signal<Vec<TextRange>>>,
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
    pub(crate) pressed: Option<Range<usize>>,
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
}

/// An edit the locks follow: the text it was made on and the text it made,
/// the caret it left there (UTF-16; none for an undo or redo) and what it
/// recorded it did (`Edit::change`, in UTF-16). It moves only the locks of
/// its text before into its text after: locks found again on the text
/// after (new paths or ranges from the host) are already where they are.
#[derive(Clone)]
struct Followed {
    before: Arc<str>,
    after: Arc<str>,
    caret: Option<usize>,
    change: Option<Arc<Change<CodeUnits>>>,
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
    pub(crate) host_ranges: Signal<Vec<TextRange>>,
    /// The host's ranges as UTF-16 offsets of the text on screen, followed
    /// through edits; empty while editing as another language (there the
    /// fields they cover are locked by path).
    pub(crate) ranges: RwSignal<Vec<Range<usize>>>,
    /// The host's ranges in its own text while editing as another
    /// language, followed through each conversion back.
    in_host: HostRanges,
    /// The host's ranges `ranges` was first placed from, until the effect
    /// placing them again first runs: ranges the host changed before then
    /// (an update in the task that mounts the editor) are placed then.
    placed_at_mount: StoredValue<Option<Vec<TextRange>>>,
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
}

impl ProtectState {
    pub(crate) fn new(init: ProtectInit, inputs: Inputs) -> Self {
        let paths = init.paths.unwrap_or_default();
        let ranges = init.ranges.unwrap_or_default();
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
        let mounted_with = ranges.get_untracked();
        let in_host = host_text.with_value(|host| HostRanges::new(host, &mounted_with));
        let local = RwSignal::new(if edit_language.get_untracked() == host_language {
            source.with_untracked(|text| offsets(text, &mounted_with))
        } else {
            Vec::new()
        });
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
                    if !ranges.with(Vec::is_empty) {
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
            host_ranges: ranges,
            ranges: local,
            in_host,
            placed_at_mount: StoredValue::new(Some(mounted_with)),
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
        let still = self
            .secrets
            .known
            .with_untracked(|known| held.still(behavior, known, text, origin.as_deref()));
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
                            known.whole.is_some() && known.token == text[range.clone()]
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

    /// Shows `message` at `offset` for a moment: why an edit was refused.
    pub(crate) fn flash(&self, message: String, offset: usize) {
        self.popups.flash_lock(message, offset);
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

/// A host text, its read-only ranges (UTF-16 offsets) and the fields they
/// cover.
type CoveredFields = (String, Vec<Range<usize>>, Vec<FieldPath>);

impl Covered {
    /// [`covered_fields`], from the last ones found when they were of the
    /// same text and ranges.
    fn found(
        self,
        language: Language,
        host: &str,
        ranges: &[Range<usize>],
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
    let units: Vec<u16> = protected.text.encode_utf16().collect();
    let starts: Vec<&str> = protected.text.lines().map(str::trim_start).collect();
    protected.locks.iter().any(|lock| {
        let Some(key) = lock.key.as_ref().filter(|key| key.end <= units.len()) else {
            return false;
        };
        let key = String::from_utf16_lossy(&units[key.clone()]);
        let written = |line: &&str| {
            line.strip_prefix(key.as_str())
                .is_some_and(|rest| !rest.starts_with(|c: char| c.is_alphanumeric() || c == '_'))
        };
        !key.trim().is_empty() && starts.iter().filter(|line| written(line)).count() > 1
    })
}

/// The fields the host's `ranges` (UTF-16 offsets) cover in its own `host`
/// text, written in `language` (none when it does not parse): what they
/// lock while editing as another language.
fn covered_fields(
    language: Language,
    host: &str,
    ranges: &[Range<usize>],
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

/// The host's ranges (UTF-16 offsets of its text `old`) keep their text in
/// its next text `new`: an Edit-as view locks the fields a range covers by
/// their values, and has nothing to lock of the range's other text (a
/// comment, the line break before a key), so the save back holds the
/// range's text as written (at least as often as before; a range of only
/// whitespace, which formatting writes everywhere, at least once).
fn text_ranges_kept(old: &str, new: &str, ranges: &[Range<usize>]) -> Result<(), ReadOnlyChange> {
    if ranges.is_empty() {
        return Ok(());
    }
    // Most ranges keep their text where the conversion kept their lines;
    // only the others are looked for.
    let moves = Moves::new(old, new);
    let (units, new_units): (Vec<u16>, Vec<u16>) =
        (old.encode_utf16().collect(), new.encode_utf16().collect());
    for range in ranges {
        let kept = moves
            .range(range)
            .is_some_and(|to| new_units.get(to) == units.get(range.clone()));
        let Some(text) = units.get(range.clone()).map(String::from_utf16_lossy) else {
            continue;
        };
        if kept || text.is_empty() {
            continue;
        }
        let wanted = if text.trim().is_empty() {
            1
        } else {
            old.matches(&text).count()
        };
        if new.matches(&text).count() < wanted {
            return Err(ReadOnlyChange {
                pointer: String::new(),
            });
        }
    }
    Ok(())
}

/// What `edit` recorded it did, in UTF-16 code units of its text before.
fn recorded(edit: &Edit) -> Option<Change<CodeUnits>> {
    edit.change()?.in_utf16(&edit.before)
}

/// Whether the locks the guard found (`foreseen`) are those the memo would
/// find for `text`: from the same locks before, paths and host ranges.
fn foreseen_fits(
    foreseen: &Protected,
    previous: Option<&Protected>,
    text: &str,
    paths: &[FieldPath],
    ranges: &[Range<usize>],
) -> bool {
    let Some(previous) = previous else {
        return false;
    };
    let placed: Vec<Range<usize>> = foreseen
        .locks
        .iter()
        .filter(|lock| lock.pointer.is_none())
        .flat_map(|lock| lock.ranges.iter().cloned())
        .collect();
    let wanted: Vec<&Range<usize>> = ranges.iter().filter(|range| !range.is_empty()).collect();
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
    /// Places the host's ranges again each time the host changes them.
    pub(crate) fn connect_protection(&self) {
        let state = *self;

        Effect::new(move |previous: Option<()>| {
            state.protect().host_ranges.track();
            // The first run places them only if the host changed them since
            // the mount (in the same task: the effect runs after it).
            let changed = previous.is_some()
                || state
                    .protect()
                    .placed_at_mount
                    .try_update_value(Option::take)
                    .flatten()
                    .is_some_and(|placed| {
                        state
                            .protect()
                            .host_ranges
                            .with_untracked(|ranges| *ranges != placed)
                    });
            if changed {
                state.place_ranges();
            }
        });
        // New locks from the host: what the old ones refused is asked
        // again (a key at the refused place, an Edit-as edit the pause
        // could not convert).
        Effect::new(move |previous: Option<()>| {
            state.protect().host_paths.track();
            state.protect().host_ranges.track();
            if previous.is_some() {
                untrack(|| state.locks_changed_by_host());
            }
        });
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
        let change = recorded(edit);
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
                    }
                    .check(before, after, before_parses)
                })
            })
        });
        if verdict.glued.is_none() {
            secrets
                .parsing
                .set_value((verdict.parses == Some(true)).then(|| after.to_string()));
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
            Origin::Command | Origin::Input { .. } | Origin::Undo | Origin::Redo => {}
        }
    }

    /// Follows an applied edit with what is kept by position (the host's
    /// ranges, the secret placeholders).
    pub(crate) fn follow_edit(&self, edit: &Edit) {
        let (before, after, caret) = (edit.before.as_str(), edit.text.as_str(), edit.caret());
        let change = recorded(edit).map(Arc::new);
        self.protect().marks.update_value(|marks| {
            marks.followed = Some(Followed {
                before: Arc::from(before),
                after: Arc::from(after),
                caret: edit.written_end(),
                change: change.clone(),
            });
        });
        self.follow_tokens(before, after, caret, change.as_deref());
        if self.protect().ranges.with_untracked(Vec::is_empty) {
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
        let host = self.config().host_language == self.sync().edit_language.get_untracked();
        let placed = self.protect().host_ranges.with_untracked(|ranges| {
            if host {
                self.doc().text.with_untracked(|text| offsets(text, ranges))
            } else {
                self.sync()
                    .host_text
                    .with_value(|text| self.protect().in_host.place(text, ranges));
                Vec::new()
            }
        });
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
            .or_else(|| self.unplaced_waits())
            .or_else(|| {
                let field = self.placeholder_held()?;
                Some(self.strings_untracked(|strings| glued_message(strings, &field)))
            })
    }

    /// [`ProtectState::placeholder_held`] for the text on screen.
    fn placeholder_held(&self) -> Option<String> {
        self.doc()
            .text
            .with_untracked(|text| self.protect().placeholder_held(self.behavior(), text))
    }

    /// The locked field a save of the text on screen waits on because it
    /// reads otherwise than pinned (see `locks::Protected::held`): a
    /// duplicate shadows it, or it moved.
    fn held_waits(&self) -> Option<String> {
        let (pointer, reason) = self.protect().protected.with_untracked(|protected| {
            let pointer = protected.held.first()?;
            Some((pointer.clone(), protected.held_reason(pointer)))
        })?;
        Some(self.strings_untracked(|strings| save_held(strings, &pointer, &reason)))
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
            first_glued(self.behavior(), document, known).and_then(|known| known.field.clone())
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
            secrets
                .known
                .with_untracked(|known| glued_in_save(behavior, known, old, new, reading))
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
            let ranges = if self.protect().host_ranges.with_untracked(Vec::is_empty) {
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
    /// (find and replace's Replace All). Each offset is read off the
    /// text's lines, split once, never by a pass from the top (one per
    /// lock and placeholder: quadratic with a lock on every line).
    #[cfg(feature = "search")]
    pub(crate) fn protected_bytes(&self, text: &str) -> Vec<Range<usize>> {
        let byte = |at: usize| super::super::keyboard::utf16_to_byte_index(text, at);
        let behavior = self.behavior();
        let tokens = self.solid_tokens(text);
        super::super::paths::with_spans(behavior, text, &[], || {
            self.protected_bytes_in(&byte, &tokens)
        })
    }

    /// [`Self::protected_bytes`], with `byte` the byte at a UTF-16 offset and
    /// `tokens` the placeholders no replace may cut.
    #[cfg(feature = "search")]
    fn protected_bytes_in(
        &self,
        byte: &dyn Fn(usize) -> usize,
        tokens: &[Range<usize>],
    ) -> Vec<Range<usize>> {
        self.protect().protected.with_untracked(|protected| {
            protected
                .locks
                .iter()
                .flat_map(|lock| lock.ranges.iter().chain(&lock.guards))
                .chain(tokens)
                .map(|range| byte(range.start)..byte(range.end))
                .collect()
        })
    }

    /// What the protected text at UTF-16 `offset` is, with its label: a
    /// secret placeholder's hint, or why a read-only field is read-only.
    /// With `ends`, an offset just at either end of it counts (the caret
    /// beside it).
    pub(crate) fn protected_note(&self, offset: usize, ends: bool) -> Option<(String, String)> {
        let covers = |range: &Range<usize>| {
            range.start <= offset && (offset < range.end || (ends && offset == range.end))
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
            self.protected_note(offset, false)
                .map(|(_, message)| (message, offset))
        });
        let Some((message, offset)) = found else {
            self.protect().hide_tip(true);
            return None;
        };
        self.protect().popups.hover_lock(message.clone(), offset);
        Some(message)
    }
}
