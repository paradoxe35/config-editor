//! Suggestions in the live editor: the list at the caret (kept by a memo
//! while it is asked for), accepting one, and the value slots and stops an
//! accepted block leaves for Tab.

use std::sync::Arc;

use leptos::prelude::*;

use super::super::diagnostics::SchemaBundle;
use super::super::edit::Edit;
use super::super::language::ConfigLanguage;
use super::super::languages::Language;
use super::super::protect::ProtectState;
use super::super::state::EditorState;
use super::{
    Completion, accept_reopens_suggestions, apply_completion, completions_in, inserted_closers,
    next_slot, stop_ranges,
};

/// The suggestions part of the editor state.
#[derive(Clone, Copy)]
pub(crate) struct CompletionState {
    /// The list is asked for (typing, `Ctrl+Space`); it shows when it has
    /// options.
    pub(crate) requested: RwSignal<bool>,
    /// Asked for with `Ctrl+Space` (or after an accept): offers more than
    /// what matches a typed prefix.
    pub(crate) explicit: RwSignal<bool>,
    /// The list was asked for over a selected word (Ctrl+Space): it offers
    /// every key, and accepting one replaces the word.
    pub(crate) replace_word: RwSignal<bool>,
    /// The highlighted option.
    pub(crate) active: RwSignal<usize>,
    pub(crate) suggestions: Memo<Vec<Completion>>,
    /// An inserted default left selected after an accept: typing replaces
    /// it, Tab keeps it and moves past it, Enter keeps it and starts a new
    /// line.
    pub(crate) placeholder: StoredValue<Option<(usize, usize)>>,
    /// The block an accepted suggestion wrote, as (start, distance from the
    /// end): Tab walks its empty value slots.
    pub(crate) slot_region: StoredValue<Option<(usize, usize)>>,
    /// The values that block wrote (defaults, examples), each as distances
    /// of its start and end from the end of the text: Tab selects them.
    pub(crate) value_stops: StoredValue<Vec<(usize, usize)>>,
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
    pub(crate) text: RwSignal<String>,
    pub(crate) caret: RwSignal<usize>,
    pub(crate) edit_language: RwSignal<Language>,
    pub(crate) schema: Option<Arc<SchemaBundle>>,
    pub(crate) read_only: Signal<bool>,
    /// The `completions` setting.
    pub(crate) enabled: Memo<bool>,
    pub(crate) protect: ProtectState,
    /// The text's multi-line string bodies (no suggestions there), shared
    /// with the view.
    pub(crate) string_bodies: Memo<Vec<std::ops::Range<usize>>>,
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
        } = sources;
        let requested = RwSignal::new(false);
        let explicit = RwSignal::new(false);
        let replace_word = RwSignal::new(false);
        let suggestions = Memo::new(move |_| {
            if !requested.get() || !enabled.get() {
                return Vec::new();
            }
            let behavior = edit_language.get_untracked().behavior();
            let at = caret.get();
            text.with(|text| {
                let options = string_bodies.with(|bodies| {
                    completions_in(
                        text,
                        at,
                        explicit.get(),
                        schema.as_deref(),
                        !read_only.get(),
                        behavior,
                        replace_word.get(),
                        bodies,
                    )
                });
                if read_only.get() {
                    return options;
                }
                protect.adjust_suggestions(
                    options,
                    &SuggestionPlace {
                        behavior,
                        source: text,
                        cursor: at,
                        explicit: explicit.get(),
                        schema: schema.as_deref().map(|bundle| &bundle.schema),
                    },
                )
            })
        });
        Self {
            requested,
            explicit,
            replace_word,
            active: RwSignal::new(0),
            suggestions,
            placeholder: StoredValue::new(None),
            slot_region: StoredValue::new(None),
            value_stops: StoredValue::new(Vec::new()),
        }
    }
}

impl EditorState {
    pub(crate) fn close_suggestions(&self) {
        self.completion.requested.set(false);
        if self.completion.replace_word.get_untracked() {
            self.completion.replace_word.set(false);
        }
    }

    /// Writes `option` into the document, remembering what it inserted for
    /// Tab and closer handling.
    pub(crate) fn accept(&self, option: &Completion, reveal: bool) {
        let before = self.doc.text.get_untracked();
        let (updated, anchor, caret) = apply_completion(&before, option);
        // Closers after the replaced text stay (an XML end tag the editor
        // wrote before a value was accepted into it); the insert adds its own.
        let old_length = before.encode_utf16().count();
        let inserted = inserted_closers(option, &updated, (anchor, caret), self.behavior());
        self.typing.closers.update_value(|closers| {
            closers.retain(|closer| old_length - closer.distance.min(old_length) >= option.end);
            closers.extend(inserted);
        });
        self.completion
            .placeholder
            .set_value((anchor != caret).then_some((anchor, caret)));
        // Tab walks the empty values the insert wrote after the caret, on one
        // line (`{ server = "|", key_name = "" }`) or several.
        let insert_start = caret.saturating_sub(option.cursor_offset);
        let insert_end = insert_start + option.insert.encode_utf16().count();
        let has_slots = next_slot(&updated, caret, insert_end, self.behavior()).is_some();
        // The values it wrote are stops too, measured from the end so typing
        // before them keeps them in place.
        let total = updated.encode_utf16().count();
        let stops: Vec<(usize, usize)> =
            stop_ranges(&option.insert, option.cursor_offset, &option.fills)
                .into_iter()
                .map(|(start, end)| (total - (insert_start + start), total - (insert_start + end)))
                .collect();
        let has_stops = !stops.is_empty();
        self.completion.value_stops.set_value(stops);
        self.completion.slot_region.set_value(
            (option.insert.contains('\n') || has_slots || has_stops).then(|| {
                (
                    insert_start,
                    updated.encode_utf16().count().saturating_sub(insert_end),
                )
            }),
        );
        let reopen =
            accept_reopens_suggestions(option, self.config.settings.auto_trigger.get_untracked());
        self.hide_hover();
        let at = self.doc.caret.get_untracked();
        let edit = Edit::new(before, (at, at), updated, caret).selecting_from(anchor);
        if !self.commit(if reveal { edit } else { edit.quiet() }) {
            self.completion.placeholder.set_value(None);
            self.forget_typing_aids();
            self.close_suggestions();
            return;
        }
        self.completion.active.set(0);
        self.completion.explicit.set(true);
        self.completion.requested.set(reopen);
    }

    /// Accepts the highlighted suggestion.
    pub(crate) fn accept_active(&self) {
        let options = self.completion.suggestions.get_untracked();
        if let Some(option) = options.get(self.completion.active.get_untracked()) {
            self.accept(option, true);
        }
    }
}
