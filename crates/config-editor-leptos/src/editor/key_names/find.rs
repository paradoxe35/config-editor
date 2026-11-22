//! Find uses (`Shift+F12`, the hover card's Uses link, the handle's
//! `find_uses`) in the live editor: what to search for at a place, the
//! list of the open documents' uses at once, the host's when they come,
//! and going to the one chosen.

use std::sync::Arc;

use leptos::prelude::*;

use super::definition::{Searched, searched_at};
use super::list::{UseGroup, UsePlace, UseRow, UsesList};
use super::uses::name_written_at;
use super::{FindUses, GoToUse, Use, UseHit, UsesReply, uses_in};
use crate::FieldPath;
use crate::editor::a11y::caret_position;
use crate::editor::completion::{FieldHit, field_at};
use crate::editor::paths::{line_start, split_line};
use crate::editor::state::EditorState;
use crate::editor::text::{TextRange, TextSize};
use crate::strings::{counted, fill};

/// Where a use chosen in the list is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::editor) enum UseTarget {
    /// At this stretch of the editor's own text.
    Here(TextRange),
    /// At this path of another open document, by id.
    Open(String, FieldPath),
    /// In a file the host listed: what `on_go_to_use` hears.
    Host(GoToUse),
}

impl EditorState {
    /// What Find uses searches for at `field` of `text`.
    pub(in crate::editor) fn searched_in(&self, text: &str, field: &FieldHit) -> Option<Searched> {
        let schema = self.config().schema.get_value()?;
        let input = self.config().key_names.get_value();
        let names_map = |pointer: &str| input.open.names_map(&schema.identity, pointer);
        searched_at(
            text,
            field,
            &schema,
            self.behavior(),
            input.source.as_ref(),
            &names_map,
        )
    }

    /// What Find uses searches for at byte `at` of the text: on a name or
    /// a defining key, or just past its end.
    pub(in crate::editor) fn searched_near(&self, at: TextSize) -> Option<Searched> {
        let behavior = self.behavior();
        self.doc().text.with_untracked(|text| {
            let (line, column) = caret_position(text, at);
            let (line, column) = (line - 1, column - 1);
            [Some(column), column.checked_sub(1)]
                .into_iter()
                .flatten()
                .filter_map(|column| field_at(text, line, column, behavior))
                .find_map(|field| self.searched_in(text, &field))
                .or_else(|| {
                    // A name the caret's reading of its line does not reach
                    // (an item of a list written inline): by where names are
                    // written.
                    let schema = self.config().schema.get_value()?;
                    let name =
                        name_written_at(text, behavior, &schema.schema, &schema.identity, at)?;
                    Some(Searched {
                        name,
                        definition: false,
                    })
                })
        })
    }

    /// Lists the uses of the name (or defining key) at byte `at`: the open
    /// documents' at once, the host's (`on_find_uses`) when they come.
    /// Returns whether there was a name there.
    pub(in crate::editor) fn find_uses_at(&self, at: TextSize) -> bool {
        let Some(searched) = self.searched_near(at) else {
            return false;
        };
        self.list_uses(searched, at);
        true
    }

    /// Lists the uses of the name (or defining key) `field` on zero-based
    /// `line` (the hover card's Uses link). Returns whether it is one.
    pub(in crate::editor) fn find_uses_on(&self, line: usize, field: &FieldHit) -> bool {
        let found = self.doc().text.with_untracked(|text| {
            let searched = self.searched_in(text, field)?;
            let start = line_start(text, line);
            let row = split_line(text, line).unwrap_or_default();
            let column = row
                .char_indices()
                .nth(field.start_column)
                .map_or(row.len(), |(at, _)| at);
            Some((searched, start + TextSize::from(column as u32)))
        });
        let Some((searched, at)) = found else {
            return false;
        };
        self.list_uses(searched, at);
        true
    }

    /// Opens the list of the uses of `searched`, pointing at byte `at`.
    fn list_uses(&self, searched: Searched, at: TextSize) {
        let Some(schema) = self.config().schema.get_value() else {
            return;
        };
        let input = self.config().key_names.get_value();
        let name = searched.name;
        let own = self.doc().text.with_untracked(|text| {
            uses_in(
                text,
                self.behavior(),
                &schema.schema,
                &schema.identity,
                &name,
            )
        });
        let mut groups = Vec::new();
        if !own.is_empty() {
            groups.push(UseGroup {
                label: self.strings_untracked(|strings| strings.key_name_this_document.clone()),
                place: UsePlace::ThisDocument,
                rows: own.iter().map(own_row).collect(),
            });
        }
        groups.extend(input.open.uses(&name).into_iter().map(|document| UseGroup {
            label: document.label,
            place: UsePlace::Open(document.id),
            rows: document.hits.iter().map(open_row).collect(),
        }));
        let uses = self.uses();
        let request = uses.request.get_value() + 1;
        uses.request.set_value(request);
        self.hide_hover();
        self.close_suggestions();
        self.hide_shown_problem();
        uses.active.set(0);
        uses.list.set(Some(UsesList {
            name: name.name.clone(),
            anchor: at,
            groups,
            searching: input.on_find_uses.is_some(),
            host_searches: input.on_find_uses.is_some(),
            request,
            opened: (self.caret_now(), self.doc().version.get_value()),
        }));
        self.say_uses();
        if let Some(on_find_uses) = input.on_find_uses {
            let state = *self;
            let reply = UsesReply(Arc::new(move |found: Option<Vec<Use>>| {
                if !state.is_disposed() {
                    state.take_host_uses(request, found);
                }
            }));
            on_find_uses.run(FindUses::new(
                name.schema.id().map(str::to_string),
                name.pointer,
                name.name,
                reply,
            ));
        }
    }

    /// [`find_uses_at`](Self::find_uses_at) the caret.
    pub(in crate::editor) fn find_uses(&self) -> bool {
        self.find_uses_at(self.caret_now())
    }

    /// The caret where the text area has it now: a caret a script or the
    /// browser moved just before a key may not have been reported yet.
    pub(in crate::editor) fn caret_now(&self) -> TextSize {
        self.textarea().map_or_else(
            || self.doc().caret.get_untracked(),
            |element| crate::editor::state::directed_selection(&element).1,
        )
    }

    /// The host's answer to request `request`: its files' uses join the
    /// list, grouped by file (those of open documents left out: their live
    /// uses are listed). An answer to an older request, or after the list
    /// closed, is dropped. `None`: the host searches no other files.
    fn take_host_uses(&self, request: u64, found: Option<Vec<Use>>) {
        let current = self.uses().list.with_untracked(|list| {
            list.as_ref()
                .is_some_and(|list| list.request == request && list.searching)
        });
        if !current {
            return;
        }
        let input = self.config().key_names.get_value();
        self.uses().list.update(|list| {
            let Some(list) = list.as_mut() else {
                return;
            };
            list.searching = false;
            let Some(found) = found else {
                list.host_searches = false;
                return;
            };
            for found in found {
                if input.open.is_open(&found.source) {
                    continue;
                }
                let row = host_row(&found);
                let group = match list
                    .groups
                    .iter()
                    .position(|group| group.place == UsePlace::Host(found.source.clone()))
                {
                    Some(at) => &mut list.groups[at],
                    None => {
                        list.groups.push(UseGroup {
                            label: found.source.clone(),
                            place: UsePlace::Host(found.source.clone()),
                            rows: Vec::new(),
                        });
                        list.groups.last_mut().expect("just pushed")
                    }
                };
                group.rows.push(row);
            }
        });
        self.say_uses();
    }

    /// Says how many uses the list holds, or that it holds none.
    fn say_uses(&self) {
        let Some(said) = self.uses().list.with_untracked(|list| {
            list.as_ref()
                .map(|list| self.strings_untracked(|strings| uses_said(list, strings)))
        }) else {
            return;
        };
        self.a11y().announce(said);
    }

    /// Closes the list of uses.
    pub(in crate::editor) fn close_uses(&self) {
        let uses = self.uses();
        if uses.is_open() {
            uses.list.set(None);
        }
    }

    /// Where use `index` of the list is.
    pub(in crate::editor) fn use_target(&self, index: usize) -> Option<UseTarget> {
        self.uses().list.with_untracked(|list| {
            let list = list.as_ref()?;
            let (group, row) = list.row(index)?;
            Some(match &group.place {
                UsePlace::ThisDocument => UseTarget::Here(row.range?),
                UsePlace::Open(id) => UseTarget::Open(id.clone(), row.path.clone()),
                UsePlace::Host(source) => UseTarget::Host(GoToUse::new(
                    source.clone(),
                    row.path.clone(),
                    list.name.clone(),
                )),
            })
        })
    }

    /// Goes to use `index` of the list, which closes: selects it in this
    /// document, shows another open document there, or tells the host
    /// (`on_go_to_use`). Returns whether there was such a use.
    pub(in crate::editor) fn go_to_use(&self, index: usize) -> bool {
        let Some(target) = self.use_target(index) else {
            return false;
        };
        self.close_uses();
        let input = self.config().key_names.get_value();
        match target {
            UseTarget::Here(range) => {
                let end = self.doc().text.with_untracked(|text| TextSize::of(text));
                self.focus_range(range.start().min(end), range.end().min(end));
            }
            UseTarget::Open(id, path) => {
                input.open.go_to(&id, &path);
            }
            UseTarget::Host(target) => {
                if let Some(on_go_to_use) = input.on_go_to_use {
                    on_go_to_use.run(target);
                }
            }
        }
        true
    }
}

/// What the list says of itself: its count, or why it is empty.
pub(in crate::editor) fn uses_said(list: &UsesList, strings: &crate::EditorStrings) -> String {
    match list.count() {
        0 => empty_text(list, strings),
        count => counted(&strings.uses_one, &strings.uses_other, count),
    }
}

/// What an empty list shows: the host still searching, nothing anywhere,
/// or nothing in the open documents (no host search).
pub(in crate::editor) fn empty_text(list: &UsesList, strings: &crate::EditorStrings) -> String {
    if list.searching {
        strings.uses_searching.clone()
    } else if list.host_searches {
        strings.no_uses.clone()
    } else {
        strings.no_uses_in_open_files.clone()
    }
}

/// The list's title.
pub(in crate::editor) fn title(list: &UsesList, strings: &crate::EditorStrings) -> String {
    fill(&strings.uses_title, &[("name", &list.name)])
}

/// A row of the editor's own document.
fn own_row(hit: &UseHit) -> UseRow {
    UseRow {
        range: Some(hit.range),
        ..open_row(hit)
    }
}

/// A row of an open document.
fn open_row(hit: &UseHit) -> UseRow {
    UseRow {
        path: hit.path.clone(),
        line: Some(hit.line + 1),
        text: hit.text.trim().to_string(),
        range: None,
    }
}

/// A row the host listed.
fn host_row(found: &Use) -> UseRow {
    UseRow {
        path: found.path.clone(),
        line: found.line,
        text: found
            .text
            .as_deref()
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map_or_else(|| found.path.to_pointer(), str::to_string),
        range: None,
    }
}

/// Closes the list of uses when the text or the caret moves on from where
/// they were when it opened: a click in the text, typing, a paste, undo
/// (the keys the list takes keep it). Compared with the list's own record,
/// not the last change seen: a caret the browser reports late, at the
/// place it already had, keeps the list.
pub(in crate::editor) fn close_uses_on_moves(state: EditorState) {
    Effect::new(move |_| {
        state.doc().text.track();
        let now = (state.doc().caret.get(), state.doc().version.get_value());
        let moved = state
            .uses()
            .list
            .with_untracked(|list| list.as_ref().is_some_and(|list| list.opened != now));
        if moved {
            untrack(|| state.close_uses());
        }
    });
}
