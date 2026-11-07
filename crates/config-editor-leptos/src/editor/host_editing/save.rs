//! The save command (`Ctrl+S`, `EditorHandle::save`): the document, in the
//! host's language, handed to `on_save`. Nothing is ever saved by itself.

use leptos::prelude::*;

use super::super::state::EditorState;
use super::super::sync;
use super::SaveRequest;

impl EditorState {
    /// Sends the document to `on_save`, converting an Edit-as edit still
    /// waiting out the typing pause first. Returns whether it was sent: not
    /// without `on_save`, and not while the text on screen cannot be written
    /// in the host's language (invalid, or holding a value it cannot, such
    /// as a `null` for TOML): then the caret goes to the problem, whose
    /// message shows, and nothing is sent.
    pub(in crate::editor) fn save(&self) -> bool {
        let Some(on_save) = self.editing.on_save else {
            return false;
        };
        super::super::input::flush_input(*self);
        match self.host_document() {
            Some(text) => {
                self.editing.sent.update_value(|sent| sent.note(&text));
                on_save.run(SaveRequest {
                    text,
                    language: self.config.host_language,
                });
                true
            }
            None => {
                self.show_conversion_problem();
                false
            }
        }
    }

    /// The document in the host's language now, converting a pending Edit-as
    /// edit (which reaches `on_change` first); `None` when the text on screen
    /// cannot convert.
    pub(in crate::editor) fn host_document(&self) -> Option<String> {
        if self.bridge().is_none() {
            return Some(self.doc.text.get_untracked());
        }
        let text = self.doc.text.get_untracked();
        if text != self.sync.synced_edit.get_value() {
            sync::convert_now(*self, text, self.sync.edit_language.get_untracked());
        }
        let converted = self
            .doc
            .text
            .with_untracked(|text| *text == self.sync.synced_edit.get_value());
        converted.then(|| self.sync.host_text.get_value())
    }

    /// Moves to the first conversion problem with a place and shows its
    /// message, as `F8` does; just focuses the editor without one.
    fn show_conversion_problem(&self) {
        let place = self.sync.conversion_problems.with_untracked(|problems| {
            problems
                .iter()
                .find_map(|problem| Some((problem.at.line?, problem.at.column?)))
        });
        match place {
            Some((line, column)) => {
                self.focus_at(line, column);
                self.problems.current.set(Some((line, column)));
            }
            None => {
                if let Some(element) = self.textarea() {
                    let _ = element.focus();
                }
            }
        }
    }
}
