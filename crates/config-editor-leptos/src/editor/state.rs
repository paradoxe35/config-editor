//! The editor's shared state: every signal the parts of the editor read or
//! write, in one `Copy` handle passed to components and event handlers.

use std::ops::Range;
use std::sync::Arc;

use leptos::prelude::*;

use super::bridge::Bridge;
use super::commands::{line_at_cursor, offset_at};
use super::completion::{
    Completion, accept_reopens_suggestions, apply_completion, completions, inserted_closers,
    next_slot,
};
use super::components::HoverCard;
use super::diagnostics::{Diagnostic, SchemaBundle, Severity, shift_diagnostics, validate};
use super::diff::{DiffKind, DiffPeek, LineDiff, diff_lines};
use super::history::{EditHistory, Selection, Snapshot};
use super::language::ConfigLanguage;
use super::languages::Language;
use super::line_rows::RenderedLine;
use super::metrics::read_editor_metrics;
use super::positioning::set_caret_visible;

/// Rows rendered above and below the visible ones, so fast scrolling does
/// not show blank lines before the next frame.
const OVERSCAN_ROWS: usize = 40;

/// Rows rendered before the editor element exists to measure.
const INITIAL_ROWS: usize = 120;

/// Which optional behaviours are on (from the component's props).
#[derive(Clone, Copy)]
pub(super) struct Features {
    pub(super) completions: bool,
    pub(super) auto_trigger: bool,
    pub(super) hover_docs: bool,
    pub(super) highlighting: bool,
    pub(super) auto_closing: bool,
    pub(super) diagnostics: bool,
    pub(super) diff: bool,
}

/// Closing text the editor wrote after the caret: a bracket, a quote, or
/// markup such as an XML end tag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Closer {
    /// UTF-16 distance from the document end to where it starts; typing
    /// before it does not change it.
    pub(super) distance: usize,
    /// UTF-16 length.
    pub(super) len: usize,
    /// For a closer that ends a value (a string, an element's text): the
    /// character just before the value (`"`, the `>` of `<k>`). Once the text
    /// before the caret no longer ends with it, a value was typed, and Enter
    /// steps past the closer. `None` for brackets, which Enter opens instead.
    pub(super) opener: Option<char>,
}

/// What the component hands over to build the state.
pub(super) struct Init {
    pub(super) host_language: Language,
    pub(super) edit_language: Language,
    pub(super) schema: Option<Arc<SchemaBundle>>,
    pub(super) schema_error: Option<String>,
    /// The host's text (controlled value or initial value).
    pub(super) host_text: String,
    /// The host's diff baseline, in the host language.
    pub(super) baseline: Signal<String>,
    pub(super) read_only: Signal<bool>,
    pub(super) features: Features,
    /// The host's `on_change`; edits reach it directly or through the
    /// Edit-as conversion (see `sync`).
    pub(super) host_on_change: Option<Callback<String>>,
}

/// A programmatic edit, applied with [`EditorState::commit`].
pub(super) struct Edit {
    pub(super) before: String,
    pub(super) before_selection: Selection,
    pub(super) text: String,
    pub(super) caret: usize,
    /// The other end of a selection left after the edit.
    pub(super) anchor: Option<usize>,
    /// Scroll the caret into view (the default).
    pub(super) reveal: bool,
}

impl Edit {
    pub(super) fn new(
        before: String,
        before_selection: Selection,
        text: String,
        caret: usize,
    ) -> Self {
        Self {
            before,
            before_selection,
            text,
            caret,
            anchor: None,
            reveal: true,
        }
    }

    pub(super) fn selecting_from(mut self, anchor: usize) -> Self {
        self.anchor = Some(anchor);
        self
    }

    /// Leaves the scroll position alone (an edit made with the mouse, where
    /// the caret is already in view).
    pub(super) fn quiet(mut self) -> Self {
        self.reveal = false;
        self
    }
}

#[derive(Clone, Copy)]
pub(super) struct EditorState {
    pub(super) host_language: Language,
    /// The language on screen; `edit_as` switches change it.
    pub(super) edit_language: RwSignal<Language>,
    pub(super) schema: StoredValue<Option<Arc<SchemaBundle>>>,
    pub(super) read_only: Signal<bool>,
    pub(super) features: Features,

    // The document and caret.
    pub(super) source: RwSignal<String>,
    pub(super) cursor: RwSignal<usize>,
    pub(super) current_line: RwSignal<usize>,
    pub(super) history: RwSignal<EditHistory>,
    /// The document's indent unit, detected when it is loaded, replaced or
    /// pasted in (not after every edit, so Tab then Shift+Tab round-trips).
    pub(super) indent: StoredValue<String>,
    /// Edits in the edit language; `None` while editing as another
    /// language (the conversion in `sync` reports them).
    pub(super) on_change: Option<Callback<String>>,
    pub(super) host_on_change: Option<Callback<String>>,

    // Host and Edit-as synchronisation (see `sync`).
    /// Last host text received or sent.
    pub(super) host_text: StoredValue<String>,
    /// The edited text matching `host_text`.
    pub(super) synced_edit: StoredValue<String>,
    /// Edited texts already saved, each with the host text it saved as: an
    /// undo back to one restores that host exactly, comments a fresh save
    /// could not bring back included. Cleared when the host or the edit
    /// language changes.
    pub(super) synced_hosts: StoredValue<Vec<(String, String)>>,
    /// `source` after the typing pause; validation and conversion read it.
    pub(super) diagnosed_text: RwSignal<String>,
    pub(super) load_problems: RwSignal<Vec<Diagnostic>>,
    pub(super) conversion_problems: RwSignal<Vec<Diagnostic>>,
    /// An `edit_as` switch waiting until the document can convert.
    pub(super) pending_switch: RwSignal<Option<Language>>,
    /// The last host text converted for display: (language, host, shown).
    pub(super) shown_cache: StoredValue<Option<(Language, String, String)>>,

    // Elements and layout.
    pub(super) editor_ref: NodeRef<leptos::html::Textarea>,
    pub(super) completion_ref: NodeRef<leptos::html::Div>,
    pub(super) root_ref: NodeRef<leptos::html::Div>,
    pub(super) scroll_x: RwSignal<f64>,
    pub(super) scroll_y: RwSignal<f64>,
    /// Bumped on window resize and scroll so popups re-measure.
    pub(super) layout_revision: RwSignal<usize>,

    // Suggestions.
    pub(super) suggestions_requested: RwSignal<bool>,
    pub(super) explicit_request: RwSignal<bool>,
    pub(super) active_completion: RwSignal<usize>,
    pub(super) suggestions: Memo<Vec<Completion>>,

    // Typing aids.
    /// Closers the editor inserted. Only these are typed over, deleted with
    /// their opener, or stepped past by Enter.
    pub(super) auto_closers: StoredValue<Vec<Closer>>,
    /// An inserted default left selected after an accept: typing replaces
    /// it, Tab keeps it and moves past it, Enter keeps it and starts a new
    /// line.
    pub(super) placeholder: StoredValue<Option<(usize, usize)>>,
    /// The block an accepted suggestion wrote, as (start, distance from the
    /// end): Tab walks its empty value slots.
    pub(super) slot_region: StoredValue<Option<(usize, usize)>>,
    /// The line last copied or cut with nothing selected, to paste it whole.
    pub(super) copied_line: StoredValue<Option<String>>,
    /// The selection just before the browser applies an edit, for undo.
    pub(super) pre_edit_selection: StoredValue<Selection>,

    // Hover docs.
    pub(super) hover_card: RwSignal<Option<HoverCard>>,
    pub(super) hover_pending: StoredValue<Option<(usize, String)>>,
    pub(super) hover_token: StoredValue<u64>,

    // Problems and changes.
    pub(super) diagnostics: Memo<Vec<Diagnostic>>,
    pub(super) diff: Memo<LineDiff>,
    /// Rows must not subscribe to this memo or `diff`; the row sync effect
    /// fans them out to per-row signals (see `line_rows`).
    pub(super) lines: Memo<Vec<RenderedLine>>,
    /// Only the rows in view (plus overscan) exist in the DOM.
    pub(super) visible_rows: Memo<Range<usize>>,
    pub(super) selected_diff_peek: RwSignal<Option<DiffPeek>>,
    /// The problem F8 moved to, `(line, column)`: its message shows until the
    /// next key, click or edit.
    pub(super) shown_problem: RwSignal<Option<(usize, usize)>>,
    /// Lines inside multi-line strings' bodies, shown as string text.
    pub(super) string_bodies: Memo<Vec<Range<usize>>>,
}

impl EditorState {
    /// The state, and an `edit_as` switch to retry once the document can
    /// convert (TOML has no `null`): until then it is shown in the host
    /// language, as a live switch waits.
    pub(super) fn new(init: Init) -> (Self, Option<Language>) {
        let Init {
            host_language,
            edit_language,
            schema,
            schema_error,
            host_text,
            baseline,
            read_only,
            features,
            host_on_change,
        } = init;
        let mut load_problem = Vec::new();
        let mut pending = None;
        let mut shown = host_text.clone();
        if let Some(bridge) = Bridge::new(host_language, edit_language, schema.clone()) {
            match bridge.to_edit(&host_text) {
                Ok(conversion) => {
                    load_problem = bridge.view_problems(&conversion);
                    shown = conversion.text;
                }
                Err(_) => pending = Some(edit_language),
            }
        }
        let edit_language = RwSignal::new(if pending.is_some() {
            host_language
        } else {
            edit_language
        });
        let schema_store = StoredValue::new(schema.clone());
        let source = RwSignal::new(shown.clone());
        let cursor = RwSignal::new(0usize);
        let suggestions_requested = RwSignal::new(false);
        let explicit_request = RwSignal::new(false);
        let scroll_y = RwSignal::new(0.0);
        let layout_revision = RwSignal::new(0usize);
        let editor_ref: NodeRef<leptos::html::Textarea> = NodeRef::new();
        let diagnosed_text = RwSignal::new(shown.clone());
        let load_problems = RwSignal::new(load_problem);
        let conversion_problems = RwSignal::new(Vec::new());

        let on_change = host_on_change.map(|callback| {
            Callback::new(move |text: String| {
                let bridge = Bridge::new(
                    host_language,
                    edit_language.get_untracked(),
                    schema_store.get_value(),
                );
                if bridge.is_none() {
                    callback.run(text);
                }
            })
        });

        let validated = {
            let schema = schema.clone();
            Memo::new(move |_| {
                let mut found = validate(
                    &diagnosed_text.get(),
                    schema.as_deref(),
                    schema_error.as_deref(),
                    features.diagnostics,
                    edit_language.get().behavior(),
                );
                if features.diagnostics {
                    found.extend(load_problems.get());
                    found.extend(conversion_problems.get());
                }
                found
            })
        };
        // Until validation catches up with an edit, markers move with the
        // lines the edit shifted, so they never sit on the wrong text.
        let diagnostics = Memo::new(move |_| {
            let checked = diagnosed_text.get();
            source
                .with(|current| validated.with(|found| shift_diagnostics(&checked, current, found)))
        });
        let suggestions = Memo::new(move |_| {
            if !suggestions_requested.get() {
                return Vec::new();
            }
            completions(
                &source.get(),
                cursor.get(),
                explicit_request.get(),
                schema.as_deref(),
                features.completions,
                edit_language.get_untracked().behavior(),
            )
        });
        // The baseline as shown, in the language on screen.
        let shown_cache = StoredValue::new(Some((
            edit_language.get_untracked(),
            host_text.clone(),
            shown.clone(),
        )));
        let shown_baseline = Memo::new(move |_| {
            let host = normalize_newlines(baseline.get());
            let edit = edit_language.get();
            // Usually the baseline is the host text a switch just converted.
            let cached = shown_cache.with_value(|cache| {
                cache
                    .as_ref()
                    .filter(|(language, from, _)| *language == edit && *from == host)
                    .map(|(_, _, shown)| shown.clone())
            });
            if let Some(shown) = cached {
                return shown;
            }
            match Bridge::new(host_language, edit, schema_store.get_value()) {
                Some(bridge) => bridge
                    .to_edit(&host)
                    .map_or(host, |conversion| conversion.text),
                None => host,
            }
        });
        let diff = Memo::new(move |_| {
            source.with(|text| {
                if features.diff {
                    shown_baseline.with(|baseline| diff_lines(baseline, text))
                } else {
                    LineDiff::empty(text.split('\n').count())
                }
            })
        });
        let lines = Memo::new(move |_| {
            let error_lines: std::collections::HashSet<usize> = diagnostics.with(|errors| {
                errors
                    .iter()
                    .filter(|error| error.severity == Severity::Error)
                    .filter_map(|error| error.line)
                    .collect()
            });
            source.with(|text| {
                diff.with(|line_diff| {
                    text.split('\n')
                        .enumerate()
                        .map(|(index, line)| RenderedLine {
                            text: line.to_string(),
                            has_error: error_lines.contains(&(index + 1)),
                            change: line_diff
                                .current
                                .get(index)
                                .copied()
                                .unwrap_or(DiffKind::Unchanged),
                        })
                        .collect::<Vec<_>>()
                })
            })
        });
        let visible_rows = Memo::new(move |_| {
            layout_revision.track();
            let top = scroll_y.get();
            // From the text, not `lines`: a second reader of `lines` here
            // kept the row sync from seeing diagnostic-only changes to it.
            let count = source.with(|text| line_count(text));
            let Some(editor) = editor_ref.get() else {
                return 0..count.min(INITIAL_ROWS);
            };
            let metrics = read_editor_metrics(&editor);
            let window_height = web_sys::window()
                .and_then(|window| window.inner_height().ok())
                .and_then(|height| height.as_f64())
                .unwrap_or(0.0);
            let height = f64::from(editor.client_height()).max(window_height);
            let line_height = metrics.line_height.max(1.0);
            let first = ((top - metrics.padding_top) / line_height).floor().max(0.0) as usize;
            let first = first.saturating_sub(OVERSCAN_ROWS).min(count);
            let last =
                (first + (height / line_height).ceil() as usize + 2 * OVERSCAN_ROWS).min(count);
            first..last
        });

        let string_bodies = Memo::new(move |_| {
            source.with(|text| edit_language.get().behavior().string_body_lines(text))
        });
        let state = Self {
            string_bodies,
            host_language,
            edit_language,
            schema: schema_store,
            read_only,
            features,
            source,
            cursor,
            current_line: RwSignal::new(1),
            history: RwSignal::new(EditHistory::default()),
            indent: StoredValue::new(super::commands::indent_unit(&shown)),
            on_change,
            host_on_change,
            host_text: StoredValue::new(host_text),
            synced_edit: StoredValue::new(shown),
            synced_hosts: StoredValue::new(Vec::new()),
            diagnosed_text,
            load_problems,
            conversion_problems,
            pending_switch: RwSignal::new(None),
            shown_cache,
            editor_ref,
            completion_ref: NodeRef::new(),
            root_ref: NodeRef::new(),
            scroll_x: RwSignal::new(0.0),
            scroll_y,
            layout_revision,
            suggestions_requested,
            explicit_request,
            active_completion: RwSignal::new(0),
            suggestions,
            auto_closers: StoredValue::new(Vec::new()),
            placeholder: StoredValue::new(None),
            slot_region: StoredValue::new(None),
            copied_line: StoredValue::new(None),
            pre_edit_selection: StoredValue::new((0, 0)),
            hover_card: RwSignal::new(None),
            hover_pending: StoredValue::new(None),
            hover_token: StoredValue::new(0),
            diagnostics,
            diff,
            lines,
            visible_rows,
            selected_diff_peek: RwSignal::new(None),
            shown_problem: RwSignal::new(None),
        };
        (state, pending)
    }

    /// `host` as shown in the language on screen; a text that cannot convert
    /// is shown as is, with the problem reported.
    pub(super) fn shown(&self, host: &str) -> String {
        let Some(bridge) = self.bridge() else {
            return host.to_string();
        };
        match bridge.to_edit(host) {
            Ok(conversion) => {
                self.load_problems.set(bridge.view_problems(&conversion));
                conversion.text
            }
            Err(error) => {
                self.load_problems.set(vec![bridge.load_problem(&error)]);
                host.to_string()
            }
        }
    }

    /// The language on screen.
    pub(super) fn behavior(&self) -> &'static dyn ConfigLanguage {
        self.edit_language.get_untracked().behavior()
    }

    /// The bridge between the host and the language on screen, when they
    /// differ.
    pub(super) fn bridge(&self) -> Option<Bridge> {
        self.bridge_to(self.edit_language.get_untracked())
    }

    pub(super) fn bridge_to(&self, edit: Language) -> Option<Bridge> {
        Bridge::new(self.host_language, edit, self.schema.get_value())
    }

    pub(super) fn label(&self) -> &'static str {
        self.edit_language.get().label()
    }

    pub(super) fn textarea(&self) -> Option<web_sys::HtmlTextAreaElement> {
        self.editor_ref.get_untracked()
    }

    /// Applies a programmatic edit: records undo, syncs the textarea and
    /// state, and notifies the host. Every keyboard edit goes through here
    /// to keep that order consistent.
    pub(super) fn commit(&self, edit: Edit) {
        let Edit {
            before,
            before_selection,
            text,
            caret,
            anchor,
            reveal,
        } = edit;
        let anchor = anchor.unwrap_or(caret);
        let (start, end) = (anchor.min(caret), anchor.max(caret));
        self.history.update(|history| {
            history.record_action(
                Snapshot {
                    value: before,
                    selection: before_selection,
                },
                (start, end),
            );
        });
        let element = self.textarea();
        if let Some(element) = element.as_ref() {
            element.set_value(&text);
        }
        self.source.set(text);
        if let Some(on_change) = self.on_change {
            on_change.run(self.source.get_untracked());
        }
        self.cursor.set(caret);
        self.current_line.set(
            self.source
                .with_untracked(|text| line_at_cursor(text, caret)),
        );
        if let Some(element) = element {
            if reveal {
                self.source
                    .with_untracked(|text| set_caret_visible(&element, text, caret, self.scroll_y));
            }
            // The text between the anchor and the caret stays selected.
            let _ = element.set_selection_range(start as u32, end as u32);
        }
    }

    /// The inserted closer starting at UTF-16 `offset`, if any.
    pub(super) fn closer_at(&self, offset: usize) -> Option<Closer> {
        let distance = self
            .source
            .with_untracked(|text| text.encode_utf16().count())
            .checked_sub(offset)?;
        self.auto_closers.with_value(|closers| {
            closers
                .iter()
                .find(|closer| closer.distance == distance)
                .copied()
        })
    }

    /// Forgets the inserted closer starting at UTF-16 `offset`.
    pub(super) fn forget_closer_at(&self, offset: usize) {
        let distance = self
            .source
            .with_untracked(|text| text.encode_utf16().count())
            .saturating_sub(offset);
        self.auto_closers
            .update_value(|closers| closers.retain(|closer| closer.distance != distance));
    }

    /// Remembers a closer written at UTF-16 `offset` of `text` (the document
    /// after the edit that wrote it).
    pub(super) fn remember_closer(
        &self,
        text: &str,
        offset: usize,
        len: usize,
        opener: Option<char>,
    ) {
        let distance = text.encode_utf16().count() - offset;
        self.auto_closers.update_value(|closers| {
            closers.push(Closer {
                distance,
                len,
                opener,
            });
        });
    }

    /// Detects the indent unit again, for a document loaded or replaced.
    pub(super) fn redetect_indent(&self) {
        let unit = self
            .source
            .with_untracked(|text| super::commands::indent_unit(text));
        self.indent.set_value(unit);
    }

    /// Records where the caret is, without touching the textarea.
    pub(super) fn note_caret(&self, offset: usize) {
        self.cursor.set(offset);
        self.current_line.set(
            self.source
                .with_untracked(|text| line_at_cursor(text, offset)),
        );
    }

    /// Puts the caret at `offset` in the textarea, in view, and records it.
    pub(super) fn place_caret(&self, offset: usize) {
        if let Some(element) = self.textarea() {
            let _ = element.set_selection_range(offset as u32, offset as u32);
            self.source
                .with_untracked(|text| set_caret_visible(&element, text, offset, self.scroll_y));
        }
        self.note_caret(offset);
    }

    pub(super) fn close_suggestions(&self) {
        self.suggestions_requested.set(false);
    }

    /// Forgets the typing aids tied to the caret's place (inserted closers,
    /// placeholder slots) and ends the undo group.
    pub(super) fn forget_typing_aids(&self) {
        self.auto_closers.update_value(Vec::clear);
        self.slot_region.set_value(None);
        self.history.update(EditHistory::seal);
    }

    pub(super) fn hide_hover(&self) {
        self.hover_token.update_value(|token| *token += 1);
        self.hover_pending.set_value(None);
        if self.hover_card.with_untracked(Option::is_some) {
            self.hover_card.set(None);
        }
    }

    /// Focuses the editor with the caret at `line`:`column`, scrolled into
    /// view, closing popups.
    pub(super) fn focus_at(&self, line: usize, column: usize) -> Option<usize> {
        let element = self.textarea()?;
        let offset = self
            .source
            .with_untracked(|text| offset_at(text, line, column));
        // Focusing scrolls back to the old caret; keep the view where the
        // user clicked (a line number, an underline), then reveal if needed.
        let (top, left) = (element.scroll_top(), element.scroll_left());
        let _ = element.focus();
        let _ = element.set_selection_range(offset as u32, offset as u32);
        element.set_scroll_top(top);
        element.set_scroll_left(left);
        self.source
            .with_untracked(|text| set_caret_visible(&element, text, offset, self.scroll_y));
        self.cursor.set(offset);
        self.current_line.set(line);
        self.close_suggestions();
        self.hide_hover();
        Some(offset)
    }

    /// Moves to diagnostic `index`, which must have a position.
    pub(super) fn go_to_diagnostic(&self, index: usize) {
        let position = self.diagnostics.with_untracked(|problems| {
            problems
                .get(index)
                .and_then(|problem| Some((problem.line?, problem.column?)))
        });
        if let Some((line, column)) = position {
            self.focus_at(line, column);
        }
        self.shown_problem.set(position);
    }

    /// Hides the message F8 showed.
    pub(super) fn hide_shown_problem(&self) {
        if self.shown_problem.get_untracked().is_some() {
            self.shown_problem.set(None);
        }
    }

    /// Writes `option` into the document, remembering what it inserted for
    /// Tab and closer handling.
    pub(super) fn accept(&self, option: &Completion, reveal: bool) {
        let before = self.source.get_untracked();
        let (updated, anchor, caret) = apply_completion(&before, option);
        // Closers after the replaced text stay (an XML end tag the editor
        // wrote before a value was accepted into it); the insert adds its own.
        let old_length = before.encode_utf16().count();
        let inserted = inserted_closers(option, &updated, (anchor, caret), self.behavior());
        self.auto_closers.update_value(|closers| {
            closers.retain(|closer| old_length - closer.distance.min(old_length) >= option.end);
            closers.extend(inserted);
        });
        self.placeholder
            .set_value((anchor != caret).then_some((anchor, caret)));
        // Tab walks the empty values the insert wrote after the caret, on one
        // line (`{ server = "|", key_name = "" }`) or several.
        let insert_start = caret.saturating_sub(option.cursor_offset);
        let insert_end = insert_start + option.insert.encode_utf16().count();
        let has_slots = next_slot(&updated, caret, insert_end, self.behavior()).is_some();
        self.slot_region
            .set_value((option.insert.contains('\n') || has_slots).then(|| {
                (
                    insert_start,
                    updated.encode_utf16().count().saturating_sub(insert_end),
                )
            }));
        let reopen = accept_reopens_suggestions(option, self.features.auto_trigger);
        self.hide_hover();
        let at = self.cursor.get_untracked();
        let edit = Edit::new(before, (at, at), updated, caret).selecting_from(anchor);
        self.commit(if reveal { edit } else { edit.quiet() });
        self.active_completion.set(0);
        self.explicit_request.set(true);
        self.suggestions_requested.set(reopen);
    }

    /// Accepts the highlighted suggestion.
    pub(super) fn accept_active(&self) {
        let options = self.suggestions.get_untracked();
        if let Some(option) = options.get(self.active_completion.get_untracked()) {
            self.accept(option, true);
        }
    }
}

/// Converts CRLF and CR to LF; the textarea and all line math assume LF.
pub(super) fn normalize_newlines(text: String) -> String {
    if text.contains('\r') {
        text.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        text
    }
}

/// Lines in `text` (one more than its newlines).
pub(super) fn line_count(text: &str) -> usize {
    text.bytes().filter(|byte| *byte == b'\n').count() + 1
}

pub(super) fn textarea_selection(element: &web_sys::HtmlTextAreaElement) -> Selection {
    let start = element.selection_start().ok().flatten().unwrap_or(0) as usize;
    let end = element
        .selection_end()
        .ok()
        .flatten()
        .map_or(start, |end| end as usize);
    (start, end)
}

/// Selects `start..end`, keeping the caret at the start when `backward`
/// (as a selection made with Shift+Left), so Shift+arrows extend the same end.
pub(super) fn select(
    element: &web_sys::HtmlTextAreaElement,
    start: usize,
    end: usize,
    backward: bool,
) {
    let direction = if backward { "backward" } else { "forward" };
    let _ = element.set_selection_range_with_direction(start as u32, end as u32, direction);
}

/// Whether the selection's caret is at its start.
pub(super) fn is_backward(element: &web_sys::HtmlTextAreaElement) -> bool {
    element.selection_direction().ok().flatten().as_deref() == Some("backward")
}

/// The selection as (anchor, caret): the caret is the end that moves.
pub(super) fn directed_selection(element: &web_sys::HtmlTextAreaElement) -> (usize, usize) {
    let (start, end) = textarea_selection(element);
    let backward = element.selection_direction().ok().flatten().as_deref() == Some("backward");
    if backward { (end, start) } else { (start, end) }
}
