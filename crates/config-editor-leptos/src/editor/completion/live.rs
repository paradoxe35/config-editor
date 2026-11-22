//! Suggestions in the live editor: the list at the caret (kept by a memo
//! while it is asked for), accepting one, and the value slots and stops an
//! accepted block leaves for Tab.

use crate::editor::text::SharedText;
use std::sync::Arc;

use leptos::prelude::*;

use super::super::edit::{Change, Edit};
use super::super::language::ConfigLanguage;
use super::super::languages::Language;
use super::super::protect::ProtectState;
use super::super::schema::SchemaBundle;
use super::super::state::EditorState;
use super::list_mode::{ListMode, ListQuery};
use super::{
    Completion, RowSize, accept_reopens_suggestions, completions_in, inserted_closers, next_slot,
    stop_ranges,
};
use crate::EditorStrings;
use crate::editor::text::{TextRange, TextSize, utf16_of};
use crate::strings::fill;

/// The suggestions part of the editor state.
#[derive(Clone, Copy)]
pub(crate) struct CompletionState {
    /// How the list was opened (see [`ListMode`]); set through
    /// [`EditorState::set_list_mode`]. It shows when it is open and has
    /// options.
    pub(crate) mode: RwSignal<ListMode>,
    /// The highlighted option.
    pub(crate) active: RwSignal<usize>,
    pub(crate) suggestions: Memo<Vec<Completion>>,
    /// Which list `suggestions` holds: a new number each time it changes,
    /// so the rows rendered of an old list are never kept for a new one.
    pub(crate) listing: Memo<usize>,
    /// How high the rows of `suggestions` are.
    pub(crate) row_size: Memo<RowSize>,
    /// How far the list is scrolled, in pixels (see `window`).
    pub(crate) list_top: RwSignal<f64>,
    /// An inserted default left selected after an accept, as (anchor,
    /// caret) bytes: typing replaces it, Tab keeps it and moves past it,
    /// Enter keeps it and starts a new line. It, the block and its values
    /// follow every edit (see `EditorState::follow_typing_aids`).
    pub(crate) placeholder: StoredValue<Option<(TextSize, TextSize)>>,
    /// The block an accepted suggestion wrote: Tab walks its empty value
    /// slots.
    pub(crate) slot_region: StoredValue<Option<TextRange>>,
    /// The values that block wrote (defaults, examples): Tab selects them.
    pub(crate) value_stops: StoredValue<Vec<TextRange>>,
}

/// Where suggestions are asked for, for what adjusts them (see `protect`).
#[cfg_attr(
    not(feature = "protect"),
    allow(dead_code, reason = "only protection reads it")
)]
pub(crate) struct SuggestionPlace<'a> {
    pub(crate) behavior: &'a dyn ConfigLanguage,
    pub(crate) source: &'a str,
    pub(crate) cursor: usize,
    pub(crate) explicit: bool,
    pub(crate) schema: Option<&'a serde_json::Value>,
}

/// What the list is computed from.
pub(crate) struct Sources {
    pub(crate) text: SharedText,
    pub(crate) caret: RwSignal<TextSize>,
    pub(crate) edit_language: RwSignal<Language>,
    pub(crate) schema: Option<Arc<SchemaBundle>>,
    pub(crate) read_only: Signal<bool>,
    /// The `completions` setting.
    pub(crate) enabled: Memo<bool>,
    pub(crate) protect: ProtectState,
    /// The text's multi-line string bodies (no suggestions there), shared
    /// with the view.
    pub(crate) string_bodies: Memo<Vec<std::ops::Range<usize>>>,
    /// The built-in texts (reasons, reference and end-tag details).
    pub(crate) strings: Memo<EditorStrings>,
}

impl CompletionState {
    pub(crate) fn new(sources: Sources) -> Self {
        let Sources {
            text,
            caret,
            edit_language,
            schema,
            read_only,
            enabled,
            protect,
            string_bodies,
            strings,
        } = sources;
        let mode = RwSignal::new(ListMode::Closed);
        // Only what the options depend on: the rest of the mode changing
        // works nothing out again.
        let query = Memo::new(move |_| mode.with(|mode| mode.query()));
        let suggestions = Memo::new(move |_| {
            let Some(ListQuery {
                explicit,
                replace_word,
            }) = query.get()
            else {
                return Vec::new();
            };
            if !enabled.get() {
                return Vec::new();
            }
            let behavior = edit_language.get_untracked().behavior();
            let at = caret.get();
            text.with(|text| {
                let options = string_bodies.with(|bodies| {
                    completions_in(
                        text,
                        at,
                        explicit,
                        schema.as_deref(),
                        !read_only.get(),
                        behavior,
                        replace_word,
                        bodies,
                    )
                });
                strings.with(|strings| {
                    let mut options = if read_only.get() {
                        options
                    } else {
                        protect.adjust_suggestions(
                            options,
                            &SuggestionPlace {
                                behavior,
                                source: text,
                                // Protection reads the caret as the
                                // language API counts it.
                                cursor: utf16_of(text, at),
                                explicit,
                                schema: schema.as_deref().map(|bundle| &bundle.schema),
                            },
                            strings,
                        )
                    };
                    word_end_tags(&mut options, strings);
                    options
                })
            })
        });
        let listing = Memo::new(move |previous: Option<&usize>| {
            suggestions.track();
            previous.map_or(0, |listing| listing + 1)
        });
        let row_size = Memo::new(move |_| suggestions.with(|options| RowSize::of(options)));
        Self {
            mode,
            active: RwSignal::new(0),
            suggestions,
            listing,
            row_size,
            list_top: RwSignal::new(0.0),
            placeholder: StoredValue::new(None),
            slot_region: StoredValue::new(None),
            value_stops: StoredValue::new(Vec::new()),
        }
    }
}

impl EditorState {
    /// Whether the list is showing: asked for and with options. A request
    /// that filtered down to nothing shows no popup, so no key treats it as
    /// one (Escape goes on to the find widget, Tab to the focus).
    pub(crate) fn suggestions_showing(&self) -> bool {
        self.list_mode().is_open() && !self.completion().suggestions.with_untracked(Vec::is_empty)
    }

    /// How the list was opened (untracked).
    pub(crate) fn list_mode(&self) -> ListMode {
        self.completion().mode.get_untracked()
    }

    /// Opens, closes or changes the list as `mode` says.
    pub(crate) fn set_list_mode(&self, mode: ListMode) {
        if self.list_mode() != mode {
            self.completion().mode.set(mode);
        }
    }

    /// Escape in the text: closes the list when it shows (`true`); a request
    /// with no options is dropped and left for the next thing Escape closes.
    pub(crate) fn escape_suggestions(&self) -> bool {
        let showing = self.suggestions_showing();
        self.close_suggestions();
        showing
    }

    pub(crate) fn close_suggestions(&self) {
        self.set_list_mode(ListMode::Closed);
    }

    /// Writes `option` into the document, remembering what it inserted for
    /// Tab and closer handling.
    pub(crate) fn accept(&self, option: &Completion, reveal: bool) {
        let mode = self.list_mode();
        let before = self.doc().text.get_untracked();
        // The word it replaces, written over with its insert.
        let replaced = TextRange::new(option.start, option.end.max(option.start));
        let change = Change::replace(&before, replaced, &option.insert);
        let at = self.doc().caret.get_untracked();
        let edit = Edit::command(before.clone(), change, (at, at));
        // The caret where the option puts it in its insert, a default it
        // wrote left selected.
        let insert_len = TextSize::of(option.insert.as_str());
        let caret = edit
            .selected()
            .1
            .checked_sub(insert_len)
            .unwrap_or_default()
            + option.cursor_offset;
        let anchor = caret.checked_sub(option.selected).unwrap_or_default();
        let edit = edit.selecting(anchor, caret);
        let updated = edit.text.clone();
        if updated == before {
            self.say_why_read_only(option);
        }
        // Closers after the replaced text stay (an XML end tag the editor
        // wrote before a value was accepted into it, followed through the
        // edit); the insert adds its own.
        let inserted = inserted_closers(option, &updated, (anchor, caret), self.behavior());
        // Tab walks the empty values the insert wrote after the caret, on one
        // line (`{ server = "|", key_name = "" }`) or several.
        let insert_start = caret.checked_sub(option.cursor_offset).unwrap_or_default();
        let insert_end = insert_start + insert_len;
        let has_slots = next_slot(&updated, caret, insert_end, self.behavior()).is_some();
        // The values it wrote are stops too.
        let stops: Vec<TextRange> =
            stop_ranges(&option.insert, option.cursor_offset, &option.fills)
                .into_iter()
                .map(|stop| stop + insert_start)
                .collect();
        let region = (option.insert.contains('\n') || has_slots || !stops.is_empty())
            .then(|| TextRange::new(insert_start, insert_end));
        let reopen =
            accept_reopens_suggestions(option, self.config().settings.auto_trigger.get_untracked());
        self.hide_hover();
        if !self.commit(if reveal { edit } else { edit.quiet() }) {
            self.completion().placeholder.set_value(None);
            self.forget_typing_aids();
            self.close_suggestions();
            return;
        }
        // Kept from here on by position, through every edit (see
        // `follow_typing_aids`).
        self.typing()
            .closers
            .update_value(|closers| closers.extend(inserted));
        self.completion()
            .placeholder
            .set_value((anchor != caret).then_some((anchor, caret)));
        self.completion().value_stops.set_value(stops);
        self.completion().slot_region.set_value(region);
        self.completion().active.set(0);
        // Opened again, as asked for: it offers what may follow.
        self.set_list_mode(if reopen {
            ListMode::Asked {
                replace_word: mode.replaces_word(),
                heeded: mode.heeded(),
            }
        } else {
            ListMode::Closed
        });
    }

    /// A read-only key accepted as it stands changes nothing, so no refusal
    /// says why: its reason (the option's detail) is said here.
    pub(crate) fn say_why_read_only(&self, option: &Completion) {
        if option.read_only {
            self.a11y().announce(option.detail.clone());
        }
    }

    /// Accepts the highlighted suggestion.
    pub(crate) fn accept_active(&self) {
        if let Some(option) = self.active_suggestion() {
            self.accept(&option, true);
        }
    }

    /// The highlighted suggestion.
    pub(crate) fn active_suggestion(&self) -> Option<Completion> {
        let active = self.completion().active.get_untracked();
        self.completion()
            .suggestions
            .with_untracked(|options| options.get(active).cloned())
    }
}

/// The end tag XML suggests, worded: "Closes <server>", "end tag".
fn word_end_tags(options: &mut [Completion], strings: &EditorStrings) {
    for option in options.iter_mut().filter(|option| option.closes_tag) {
        option.detail = fill(&strings.closes_tag, &[("name", &option.label)]);
        option.type_label = strings.end_tag.clone();
    }
}
