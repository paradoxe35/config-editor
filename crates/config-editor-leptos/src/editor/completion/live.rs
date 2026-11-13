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
use crate::EditorStrings;
use crate::strings::fill;

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
    /// Which list `suggestions` holds: a new number each time it changes,
    /// so the rows rendered of an old list are never kept for a new one.
    pub(crate) listing: Memo<usize>,
    /// How far the list is scrolled, in pixels (see `window`).
    pub(crate) list_top: RwSignal<f64>,
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
                strings.with(|strings| {
                    let mut options = if read_only.get() {
                        options
                    } else {
                        protect.adjust_suggestions(
                            options,
                            &SuggestionPlace {
                                behavior,
                                source: text,
                                cursor: at,
                                explicit: explicit.get(),
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
        Self {
            requested,
            explicit,
            replace_word,
            active: RwSignal::new(0),
            suggestions,
            listing,
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
        self.completion().requested.get_untracked()
            && !self.completion().suggestions.with_untracked(Vec::is_empty)
    }

    /// Escape in the text: closes the list when it shows (`true`); a request
    /// with no options is dropped and left for the next thing Escape closes.
    pub(crate) fn escape_suggestions(&self) -> bool {
        let showing = self.suggestions_showing();
        if self.completion().requested.get_untracked() {
            self.close_suggestions();
        }
        showing
    }

    pub(crate) fn close_suggestions(&self) {
        self.completion().requested.set(false);
        if self.completion().replace_word.get_untracked() {
            self.completion().replace_word.set(false);
        }
    }

    /// Writes `option` into the document, remembering what it inserted for
    /// Tab and closer handling.
    pub(crate) fn accept(&self, option: &Completion, reveal: bool) {
        let before = self.doc().text.get_untracked();
        let (updated, anchor, caret) = apply_completion(&before, option);
        if updated == before {
            self.say_why_read_only(option);
        }
        // Closers after the replaced text stay (an XML end tag the editor
        // wrote before a value was accepted into it); the insert adds its own.
        let old_length = before.encode_utf16().count();
        let inserted = inserted_closers(option, &updated, (anchor, caret), self.behavior());
        self.typing().closers.update_value(|closers| {
            closers.retain(|closer| old_length - closer.distance.min(old_length) >= option.end);
            closers.extend(inserted);
        });
        self.completion()
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
        self.completion().value_stops.set_value(stops);
        self.completion().slot_region.set_value(
            (option.insert.contains('\n') || has_slots || has_stops).then(|| {
                (
                    insert_start,
                    updated.encode_utf16().count().saturating_sub(insert_end),
                )
            }),
        );
        let reopen =
            accept_reopens_suggestions(option, self.config().settings.auto_trigger.get_untracked());
        self.hide_hover();
        let at = self.doc().caret.get_untracked();
        let edit = Edit::new(before, (at, at), updated, caret).selecting_from(anchor);
        if !self.commit(if reveal { edit } else { edit.quiet() }) {
            self.completion().placeholder.set_value(None);
            self.forget_typing_aids();
            self.close_suggestions();
            return;
        }
        self.completion().active.set(0);
        self.completion().explicit.set(true);
        self.completion().requested.set(reopen);
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
