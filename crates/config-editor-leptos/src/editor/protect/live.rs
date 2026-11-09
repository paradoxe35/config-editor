//! Protected fields in the live editor: the locks of the text on screen
//! (kept by a memo), the check every edit passes, and the tooltip saying
//! why one was refused.

use std::ops::Range;
use std::sync::Arc;

use leptos::prelude::*;

use super::super::codec;
use super::super::diagnostics::SchemaBundle;
use super::super::edit::{Edit, Origin};
use super::super::input::edit_range;
use super::super::keyboard::utf16_to_byte_index;
use super::super::languages::Language;
use super::super::paths::{FieldPath, TextRange};
use super::super::secret_options::SecretOptions;
use super::super::state::EditorState;
use super::guard::{Blocked, check};
use super::locate::LineIndex;
use super::locks::{Protected, Sources, protect};
use super::save::{ReadOnlyChange, check_save, locked_pointers, range_pointers};
use super::secrets::SecretState;
use super::texts::save_waits;
use crate::reactive::later::{self, Timeout};

/// How long a refused edit's tooltip stays, in milliseconds.
const TIP_MS: i32 = 2500;

/// A short message at a place in the text: why an edit was refused, or
/// what the hovered protected text is.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Tip {
    pub(crate) message: String,
    /// UTF-16 offset it points at.
    pub(crate) offset: usize,
    /// Shown for the pointer (it goes when the pointer leaves).
    pub(crate) hover: bool,
}

/// The host's side of protection (the component's props, when given).
pub(crate) struct ProtectInit {
    pub(crate) paths: Option<Signal<Vec<FieldPath>>>,
    pub(crate) ranges: Option<Signal<Vec<TextRange>>>,
    pub(crate) secrets: Option<SecretOptions>,
}

/// The bookkeeping of one editor's protection.
#[derive(Clone, Default)]
pub(crate) struct Marks {
    /// The timer hiding the refusal tooltip: one per editor, which a new
    /// refusal replaces.
    tip_timer: Timeout,
    /// Counts the texts the host gave (see [`ProtectState::trust_text`]).
    epoch: u64,
    /// The placeholder a mouse press landed inside, until it is released.
    pub(crate) pressed: Option<Range<usize>>,
    /// Where typing was refused at a read-only field, and why: the rest of
    /// what is typed there is refused too (see [`EditorState::check_edit`]),
    /// until the caret moves or another edit is made.
    refused: Option<(usize, String)>,
    /// The stretch typing just wrote, one character after another (UTF-16
    /// offsets): a refusal takes back the word it was writing.
    typed: Option<(usize, usize)>,
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
    /// The locks of the text on screen.
    pub(crate) protected: Memo<Arc<Protected>>,
    pub(crate) tip: RwSignal<Option<Tip>>,
    /// Small bookkeeping, in one slot: every editor keeps its handles.
    pub(crate) marks: StoredValue<Marks>,
    /// Secret placeholders (see `secrets`).
    pub(crate) secrets: SecretState,
}

/// What the memo of locks reads.
pub(crate) struct Inputs {
    pub(crate) source: RwSignal<String>,
    pub(crate) edit_language: RwSignal<Language>,
    pub(crate) host_language: Language,
    pub(crate) schema: Option<Arc<SchemaBundle>>,
    pub(crate) host_text: StoredValue<String>,
}

impl ProtectState {
    pub(crate) fn new(init: ProtectInit, inputs: Inputs) -> Self {
        let paths = init.paths.unwrap_or_default();
        let ranges = init.ranges.unwrap_or_default();
        let secrets = init.secrets.unwrap_or_default();
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
        } = inputs;
        let secrets = host_text.with_value(|host| {
            source.with_untracked(|shown| {
                SecretState::new(
                    secrets,
                    (host_language.behavior(), host),
                    (edit_language.get_untracked().behavior(), shown),
                )
            })
        });
        let local =
            RwSignal::new(source.with_untracked(|text| offsets(text, &ranges.get_untracked())));
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
                let mut fields = paths.get();
                let local_ranges = if edit == host_language {
                    local.get()
                } else {
                    // Edit-as: the host's ranges lock the fields they cover.
                    let host_ranges = ranges.get();
                    if !host_ranges.is_empty() {
                        host_text.with_value(|host| {
                            fields.extend(covered.found(
                                host_language,
                                host,
                                &host_ranges,
                                reading,
                            ));
                        });
                    }
                    Vec::new()
                };
                source.with(|text| {
                    let mut protected = protect(
                        edit.behavior(),
                        text,
                        Sources {
                            marking_schema: marking,
                            reading_schema: reading,
                            paths: &fields,
                            ranges: &local_ranges,
                            previous,
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
            protected,
            tip: RwSignal::new(None),
            marks,
            secrets,
        }
    }

    /// Takes the next text as the host's own (a controlled value, a reload,
    /// an Edit-as switch): the locked values are read from it afresh, never
    /// carried from the text it replaces.
    pub(crate) fn trust_text(&self) {
        self.marks.update_value(|marks| marks.epoch += 1);
    }

    /// Shows `message` at `offset` for a moment.
    pub(crate) fn flash(&self, message: String, offset: usize) {
        self.tip.set(Some(Tip {
            message,
            offset,
            hover: false,
        }));
        // Native unit tests have no clock: there it stays (unless a test
        // holds the timers).
        if later::defers() {
            self.cancel_tip_timer();
            let timer = later::after(self.tip, TIP_MS, |tip| tip.set(None));
            self.marks.update_value(|marks| marks.tip_timer = timer);
        }
    }

    /// Drops the pending timer that would hide the tooltip.
    fn cancel_tip_timer(&self) {
        let timer = self
            .marks
            .try_update_value(|marks| std::mem::take(&mut marks.tip_timer));
        if let Some(timer) = timer {
            timer.cancel();
        }
    }

    /// Hides the tooltip (only a hover one with `hover_only`).
    pub(crate) fn hide_tip(&self, hover_only: bool) {
        let shown = self
            .tip
            .with_untracked(|tip| tip.as_ref().map(|tip| tip.hover));
        if shown.is_some_and(|hover| hover || !hover_only) {
            self.tip.set(None);
            self.cancel_tip_timer();
        }
    }
}

/// The fields the host's ranges cover in its text, kept while the text and
/// the ranges stay the same: an edit in an Edit-as view leaves the host's
/// text as it is until it converts back, so it is not read again for each.
#[derive(Clone, Copy)]
struct Covered(StoredValue<Option<CoveredFields>>);

/// A host text, its read-only ranges and the fields they cover.
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

/// The fields the host's `ranges` cover in its own `host` text, written in
/// `language` (none when it does not parse): what they lock while editing
/// as another language.
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
    let covered = offsets(host, ranges);
    range_pointers(behavior, host, &document, &covered)
        .into_iter()
        .map(FieldPath::from)
        .collect()
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
                || old.epoch != new.epoch
        }
        _ => true,
    }
}

/// `ranges` (positions in `text`) as UTF-16 offsets.
fn offsets(text: &str, ranges: &[TextRange]) -> Vec<Range<usize>> {
    if ranges.is_empty() {
        return Vec::new();
    }
    let lines = LineIndex::new(text);
    ranges.iter().map(|range| lines.range(*range)).collect()
}

impl EditorState {
    /// Places the host's ranges again each time the host changes them.
    pub(crate) fn connect_protection(&self) {
        let state = *self;

        Effect::new(move |previous: Option<()>| {
            state.protect().host_ranges.track();
            if previous.is_some() {
                state.place_ranges();
            }
        });
    }

    /// The edit pipeline's check stage: whether `edit` keeps every lock;
    /// when not, why the field is locked.
    pub(crate) fn check_edit(&self, edit: &Edit) -> Result<(), String> {
        // Typing on where a key was refused: the rest of the word, or the
        // spaces after it, would only be left there half-written.
        if let Some(at) = inserted_at(edit)
            && let Some(message) = self.protect().marks.with_value(|marks| {
                marks
                    .refused
                    .as_ref()
                    .filter(|(refused_at, _)| *refused_at == at)
                    .map(|(_, message)| message.clone())
            })
        {
            return Err(message);
        }
        self.protect()
            .protected
            .with_untracked(|protected| {
                self.config().schema.with_value(|schema| {
                    check(
                        self.behavior(),
                        schema.as_deref().map(|bundle| &bundle.schema),
                        protected,
                        &edit.before,
                        &edit.text,
                    )
                })
            })
            .map_err(|blocked: Blocked| self.strings_untracked(|strings| blocked.message(strings)))
    }

    /// After typing was refused at a read-only field with `message`: what
    /// is typed there next is refused too, and what typing wrote on that
    /// line up to there is taken back (`<i` of a refused `<id`, `id ` of a
    /// refused `id =`), the edit returned to make it. Nothing half-written
    /// stays.
    pub(crate) fn note_refused(&self, edit: &Edit, message: String) -> Option<Edit> {
        let at = inserted_at(edit)?;
        let typed = self.protect().marks.with_value(|marks| marks.typed);
        let word_start = typed
            .filter(|(start, end)| *end == at && start < end)
            .map(|(start, _)| {
                let units: Vec<u16> = edit.before.encode_utf16().collect();
                let typed = String::from_utf16_lossy(&units[start..at]);
                // The entry typed on this line, past its indentation.
                let line = typed.rsplit('\n').next().unwrap_or_default();
                at - line.trim_start().encode_utf16().count()
            })
            .filter(|start| *start < at)
            // Taken back only when it is what the refusal is about: the
            // input refused without it would be let through. A word the
            // refused input does not need (typed before a quote that
            // closes a string over a locked field) is the user's and stays.
            .filter(|start| self.refused_for(edit, *start, at));
        let refused_at = word_start.unwrap_or(at);
        self.protect().marks.update_value(|marks| {
            marks.refused = Some((refused_at, message));
            marks.typed = None;
        });
        let start = word_start?;
        let before = edit.before.clone();
        let (from, to) = (
            utf16_to_byte_index(&before, start),
            utf16_to_byte_index(&before, at),
        );
        let text = format!("{}{}", &before[..from], &before[to..]);
        Some(Edit::new(before, (start, at), text, start))
    }

    /// Whether the refused `edit` is let through without what typing wrote
    /// from UTF-16 `start` to `at`, where the edit inserts.
    fn refused_for(&self, edit: &Edit, start: usize, at: usize) -> bool {
        let (from, to) = (
            utf16_to_byte_index(&edit.text, start),
            utf16_to_byte_index(&edit.text, at),
        );
        let without = format!("{}{}", &edit.text[..from], &edit.text[to..]);
        self.protect().protected.with_untracked(|protected| {
            self.config().schema.with_value(|schema| {
                check(
                    self.behavior(),
                    schema.as_deref().map(|bundle| &bundle.schema),
                    protected,
                    &edit.before,
                    &without,
                )
                .is_ok()
            })
        })
    }

    /// After an edit was made: typing goes on from where it ended, and a
    /// refusal stays only where typing was refused (see
    /// [`Self::note_refused`]).
    pub(crate) fn note_applied(&self, edit: &Edit) {
        // Typing goes on at the caret the insertion left (its end), read
        // without measuring the document on every key.
        let inserted = inserted_at(edit).map(|at| (at, edit.caret().max(at)));
        self.protect().marks.update_value(|marks| {
            let at_refusal = marks.refused.as_ref().is_some_and(|(refused_at, _)| {
                edit.replaced(edit.caret()).1 == *refused_at || edit.caret() == *refused_at
            });
            if !at_refusal {
                marks.refused = None;
            }
            marks.typed = match (marks.typed, inserted) {
                (Some((start, end)), Some((at, to))) if end == at => Some((start, to)),
                (_, inserted) => inserted,
            };
        });
    }

    /// The caret moved, or another kind of key was pressed: typing starts
    /// afresh.
    pub(crate) fn forget_refusal(&self) {
        self.protect().marks.update_value(|marks| {
            marks.refused = None;
            marks.typed = None;
        });
    }

    /// Finds again what is kept by position in a text replaced whole (see
    /// `edit::Origin`): the host's ranges after a host text, and the secret
    /// placeholders, the host's own after a text from the host.
    pub(crate) fn find_protected_again(&self, origin: &Origin) {
        match origin {
            // A placeholder an undo brings back is the host's again.
            Origin::Undo | Origin::Redo => self.rescan_tokens(false),
            Origin::Reload { host } => self.adopt_host_tokens(host),
            // A new text from the host: its ranges and placeholders anew. Not
            // on an echo of the editor's own edit, where they are followed
            // (a pasted look-alike there never becomes the host's).
            Origin::HostValue => {
                self.place_ranges();
                self.rescan_tokens(true);
            }
            Origin::Switch => {
                self.place_ranges();
                self.rescan_tokens(false);
            }
            Origin::Command | Origin::Input { .. } => {}
        }
    }

    /// Follows an applied edit with what is kept by position (the host's
    /// ranges, the secret placeholders).
    pub(crate) fn follow_edit(&self, before: &str, after: &str, caret: usize) {
        self.follow_tokens(before, after, caret);
        if self.protect().ranges.with_untracked(Vec::is_empty) {
            return;
        }
        let region = {
            let (start, end) = edit_range(before, after, caret);
            start..end
        };
        let inserted = (after.encode_utf16().count() + region.len())
            .saturating_sub(before.encode_utf16().count());
        self.protect().ranges.update(|ranges| {
            *ranges = ranges
                .iter()
                .map(|range| follow_range(range, &region, inserted))
                .collect();
        });
    }

    /// Places the host's ranges on the text on screen again (they changed,
    /// or the host replaced the text).
    pub(crate) fn place_ranges(&self) {
        let host = self.config().host_language == self.sync().edit_language.get_untracked();
        let placed = if host {
            self.protect().host_ranges.with_untracked(|ranges| {
                self.doc().text.with_untracked(|text| offsets(text, ranges))
            })
        } else {
            Vec::new()
        };
        self.protect().ranges.set(placed);
    }

    /// Why a save of the text on screen waits, when it does: the text does
    /// not parse and holds a read-only field the language cannot locate
    /// (it gives no spans), so nothing shows that its value is kept.
    pub(crate) fn save_waits(&self) -> Option<String> {
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
    /// change a read-only value.
    pub(crate) fn check_host_save(&self, old: &str, new: &str) -> Result<(), ReadOnlyChange> {
        self.config().schema.with_value(|schema| {
            let reading = schema.as_deref().map(|bundle| &bundle.schema);
            let marking = schema
                .as_deref()
                .filter(|bundle| bundle.has_marks)
                .map(|bundle| &bundle.schema);
            let behavior = self.config().host_language.behavior();
            let paths = self.protect().host_paths.get_untracked();
            let ranges = self
                .protect()
                .host_ranges
                .with_untracked(|ranges| offsets(old, ranges));
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
                    previous: None,
                },
            );
            check_save(behavior, reading, old, new, &pointers)
        })
    }

    /// Byte ranges of `text` (the text on screen) no replace may touch
    /// (find and replace's Replace All).
    #[cfg(feature = "search")]
    pub(crate) fn protected_bytes(&self, text: &str) -> Vec<Range<usize>> {
        let byte = |at: usize| super::super::keyboard::utf16_to_byte_index(text, at);
        self.protect().protected.with_untracked(|protected| {
            protected
                .locks
                .iter()
                .flat_map(|lock| lock.ranges.iter().chain(&lock.guards))
                .map(|range| byte(range.start)..byte(range.end))
                .chain(self.protect().secrets.tokens.with_untracked(|tokens| {
                    tokens
                        .iter()
                        .map(|range| byte(range.start)..byte(range.end))
                        .collect::<Vec<_>>()
                }))
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
    /// the pointer; `false` when nothing protected is there.
    pub(crate) fn hover_protected(&self, line: usize, column: usize) -> bool {
        let no_tokens = self.protect().secrets.tokens.with_untracked(Vec::is_empty);
        if no_tokens
            && self
                .protect()
                .protected
                .with_untracked(|protected| protected.is_empty())
        {
            self.protect().hide_tip(true);
            return false;
        }
        let found = self.doc().text.with_untracked(|text| {
            let offset = LineIndex::new(text).offset(line + 1, column + 1);
            self.protected_note(offset, false)
                .map(|(_, message)| (message, offset))
        });
        let Some((message, offset)) = found else {
            self.protect().hide_tip(true);
            return false;
        };
        let same = self.protect().tip.with_untracked(|tip| {
            tip.as_ref()
                .is_some_and(|tip| tip.hover && tip.message == message)
        });
        if !same {
            self.protect().tip.set(Some(Tip {
                message,
                offset,
                hover: true,
            }));
        }
        true
    }
}

/// A range after an edit replaced `region` with `inserted` units: before
/// it stays, after it shifts, across it keeps what is left.
fn follow_range(range: &Range<usize>, region: &Range<usize>, inserted: usize) -> Range<usize> {
    let map = |at: usize| {
        if at <= region.start {
            at
        } else if at >= region.end {
            at + inserted - region.len()
        } else {
            region.start + inserted
        }
    };
    map(range.start)..map(range.end).max(map(range.start))
}

/// Where `edit` inserts what typing writes (characters, a line break, a
/// command's text such as an accepted suggestion), replacing nothing; `None`
/// for any other edit (a paste, a deletion, a replacement).
fn inserted_at(edit: &Edit) -> Option<usize> {
    let typed = match &edit.origin {
        Origin::Command => true,
        Origin::Input { kind, .. } => matches!(
            kind.as_str(),
            "insertText" | "insertCompositionText" | "insertLineBreak" | "insertParagraph"
        ),
        _ => false,
    };
    let (start, end) = edit.replaced(edit.caret());
    (typed && start == end && edit.text.len() > edit.before.len()).then_some(start)
}
