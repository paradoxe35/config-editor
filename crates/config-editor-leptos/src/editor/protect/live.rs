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
use super::super::languages::Language;
use super::super::paths::{FieldPath, TextRange};
use super::super::state::EditorState;
use super::guard::{Blocked, check};
use super::locate::LineIndex;
use super::locks::{Protected, Sources, protect};
use super::save::{check_save, locked_pointers, range_pointers};
use super::secrets::SecretState;
use super::texts;
use super::tokens::SecretOptions;

/// How long a refused edit's tooltip stays, in milliseconds.
#[cfg(target_arch = "wasm32")]
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

/// The host's side of protection (from the component's props).
pub(crate) struct ProtectInit {
    pub(crate) paths: Signal<Vec<FieldPath>>,
    pub(crate) ranges: Signal<Vec<TextRange>>,
    pub(crate) secrets: SecretOptions,
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
    tip_token: StoredValue<u64>,
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
        let ProtectInit {
            paths,
            ranges,
            secrets,
        } = init;
        let Inputs {
            source,
            edit_language,
            host_language,
            schema,
            host_text,
        } = inputs;
        let secrets = host_text.with_value(|host| {
            source.with_untracked(|shown| SecretState::new(secrets, host, shown))
        });
        let local =
            RwSignal::new(source.with_untracked(|text| offsets(text, &ranges.get_untracked())));
        let protected = Memo::new_with_compare(
            move |_| {
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
                        let host = host_text.get_value();
                        let behavior = host_language.behavior();
                        if let Ok(document) = codec::read(behavior, &host, reading) {
                            let covered = offsets(&host, &host_ranges);
                            fields.extend(
                                range_pointers(behavior, &host, &document, &covered)
                                    .into_iter()
                                    .map(FieldPath::from),
                            );
                        }
                    }
                    Vec::new()
                };
                source.with(|text| {
                    Arc::new(protect(
                        edit.behavior(),
                        text,
                        Sources {
                            marking_schema: marking,
                            reading_schema: reading,
                            paths: &fields,
                            ranges: &local_ranges,
                        },
                    ))
                })
            },
            |old, new| match (old, new) {
                (Some(old), Some(new)) => old.locks != new.locks || old.secret != new.secret,
                _ => true,
            },
        );
        ProtectState {
            host_paths: paths,
            host_ranges: ranges,
            ranges: local,
            protected,
            tip: RwSignal::new(None),
            tip_token: StoredValue::new(0),
            secrets,
        }
    }

    /// Shows `message` at `offset` for a moment.
    pub(crate) fn flash(&self, message: String, offset: usize) {
        self.tip.set(Some(Tip {
            message,
            offset,
            hover: false,
        }));
        self.tip_token.update_value(|token| *token += 1);
        // Native unit tests have no window to time it with.
        #[cfg(target_arch = "wasm32")]
        self.hide_tip_later();
    }

    /// Hides the tooltip after a moment, unless another took its place.
    #[cfg(target_arch = "wasm32")]
    fn hide_tip_later(&self) {
        let token = self.tip_token.get_value();
        let state = *self;
        if let Some(window) = web_sys::window() {
            use wasm_bindgen::JsCast;
            let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                wasm_bindgen::closure::Closure::once_into_js(move || {
                    if state.tip_token.try_get_value() == Some(token) {
                        let _ = state.tip.try_set(None);
                    }
                })
                .unchecked_ref(),
                TIP_MS,
            );
        }
    }

    /// Hides the tooltip (only a hover one with `hover_only`).
    pub(crate) fn hide_tip(&self, hover_only: bool) {
        let shown = self
            .tip
            .with_untracked(|tip| tip.as_ref().map(|tip| tip.hover));
        if shown.is_some_and(|hover| hover || !hover_only) {
            self.tip.set(None);
        }
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
            state.protect.host_ranges.track();
            if previous.is_some() {
                state.place_ranges();
            }
        });
    }

    /// The edit pipeline's check stage: whether `edit` keeps every lock;
    /// when not, why the field is locked.
    pub(crate) fn check_edit(&self, edit: &Edit) -> Result<(), String> {
        self.protect
            .protected
            .with_untracked(|protected| {
                self.config.schema.with_value(|schema| {
                    check(
                        self.behavior(),
                        schema.as_deref().map(|bundle| &bundle.schema),
                        protected,
                        &edit.before,
                        &edit.text,
                    )
                })
            })
            .map_err(|Blocked { reason }| reason)
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
        if self.protect.ranges.with_untracked(Vec::is_empty) {
            return;
        }
        let region = {
            let (start, end) = edit_range(before, after, caret);
            start..end
        };
        let inserted = (after.encode_utf16().count() + region.len())
            .saturating_sub(before.encode_utf16().count());
        self.protect.ranges.update(|ranges| {
            *ranges = ranges
                .iter()
                .map(|range| follow_range(range, &region, inserted))
                .collect();
        });
    }

    /// Places the host's ranges on the text on screen again (they changed,
    /// or the host replaced the text).
    pub(crate) fn place_ranges(&self) {
        let host = self.config.host_language == self.sync.edit_language.get_untracked();
        let placed = if host {
            self.protect
                .host_ranges
                .with_untracked(|ranges| self.doc.text.with_untracked(|text| offsets(text, ranges)))
        } else {
            Vec::new()
        };
        self.protect.ranges.set(placed);
    }

    /// Refuses an Edit-as save from the host's `old` text to `new` that would
    /// change a read-only value.
    pub(crate) fn check_host_save(&self, old: &str, new: &str) -> Result<(), String> {
        self.config.schema.with_value(|schema| {
            let reading = schema.as_deref().map(|bundle| &bundle.schema);
            let marking = schema
                .as_deref()
                .filter(|bundle| bundle.has_marks)
                .map(|bundle| &bundle.schema);
            let behavior = self.config.host_language.behavior();
            let paths = self.protect.host_paths.get_untracked();
            let ranges = self
                .protect
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
                },
            );
            check_save(behavior, reading, old, new, &pointers)
        })
    }

    /// Byte ranges of `text` (the text on screen) no replace may touch.
    pub(crate) fn protected_bytes(&self, text: &str) -> Vec<Range<usize>> {
        let byte = |at: usize| super::super::keyboard::utf16_to_byte_index(text, at);
        self.protect.protected.with_untracked(|protected| {
            protected
                .locks
                .iter()
                .flat_map(|lock| lock.ranges.iter().chain(&lock.guards))
                .map(|range| byte(range.start)..byte(range.end))
                .chain(self.protect.secrets.tokens.with_untracked(|tokens| {
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
    pub(crate) fn protected_note(
        &self,
        offset: usize,
        ends: bool,
    ) -> Option<(&'static str, String)> {
        let covers = |range: &Range<usize>| {
            range.start <= offset && (offset < range.end || (ends && offset == range.end))
        };
        let secrets = self.protect.secrets;
        if secrets
            .tokens
            .with_untracked(|tokens| tokens.iter().any(covers))
        {
            return Some((
                texts::SECRET_LABEL,
                secrets.options.with_value(|options| options.hint.clone()),
            ));
        }
        self.protect.protected.with_untracked(|protected| {
            protected
                .locks
                .iter()
                .find(|lock| lock.ranges.iter().any(covers))
                .map(|lock| (texts::LOCK_LABEL, lock.reason.clone()))
        })
    }

    /// Shows what the protected text at zero-based `line`:`column` is, for
    /// the pointer; `false` when nothing protected is there.
    pub(crate) fn hover_protected(&self, line: usize, column: usize) -> bool {
        let no_tokens = self.protect.secrets.tokens.with_untracked(Vec::is_empty);
        if no_tokens
            && self
                .protect
                .protected
                .with_untracked(|protected| protected.is_empty())
        {
            self.protect.hide_tip(true);
            return false;
        }
        let found = self.doc.text.with_untracked(|text| {
            let offset = LineIndex::new(text).offset(line + 1, column + 1);
            self.protected_note(offset, false)
                .map(|(_, message)| (message, offset))
        });
        let Some((message, offset)) = found else {
            self.protect.hide_tip(true);
            return false;
        };
        let same = self.protect.tip.with_untracked(|tip| {
            tip.as_ref()
                .is_some_and(|tip| tip.hover && tip.message == message)
        });
        if !same {
            self.protect.tip.set(Some(Tip {
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
