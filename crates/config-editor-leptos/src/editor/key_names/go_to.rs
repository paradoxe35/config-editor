//! Go to definition (`F12`, `Ctrl`/`Cmd`+click) on a known name, and the
//! hover card's "defined in" line, in the live editor.

use leptos::prelude::*;

use super::definition::{key_written, name_at};
use super::{GoToDefinition, KeyOrigin, Offered};
use crate::editor::a11y::caret_position;
use crate::editor::completion::{FieldHit, HoverValue, KeyHover, ValueDoc, field_at};
use crate::editor::state::EditorState;

impl EditorState {
    /// The known name `field` of `text` is, read with the editor's names.
    fn known_name(&self, text: &str, field: &FieldHit) -> Option<Offered> {
        let schema = self.config().schema.get_value()?;
        let behavior = self.behavior();
        self.config()
            .key_names
            .with_value(|input| name_at(text, field, &schema, behavior, input.source.as_ref()))
    }

    /// Adds to `hover` (the card's docs of `field`) where the value is
    /// defined, when it is a known name with a source.
    pub(in crate::editor) fn add_defined_in(
        &self,
        text: &str,
        field: &FieldHit,
        hover: &mut KeyHover,
    ) {
        let Some(name) = self.known_name(text, field) else {
            return;
        };
        let Some(line) = self.strings_untracked(|strings| name.origin.defined_in(strings)) else {
            return;
        };
        match &mut hover.value {
            Some(value) => {
                value.doc.description = Some(match value.doc.description.take() {
                    Some(description) => format!("{description} ({line})"),
                    None => line,
                });
            }
            None => {
                hover.value = Some(HoverValue {
                    label: name.name,
                    doc: ValueDoc {
                        title: None,
                        description: Some(line),
                    },
                });
            }
        }
    }

    /// Where the definition of the known name at the caret is: its key in
    /// the document, or the place to tell the host of.
    pub(in crate::editor) fn definition_at_caret(&self) -> Option<Definition> {
        let caret = self.doc().caret.get_untracked();
        self.doc().text.with_untracked(|text| {
            let (line, column) = caret_position(text, caret);
            let (line, column) = (line - 1, column - 1);
            // On the value, or just past its end.
            let name = [Some(column), column.checked_sub(1)]
                .into_iter()
                .flatten()
                .filter_map(|at| field_at(text, line, at, self.behavior()))
                .find_map(|field| self.known_name(text, &field))?;
            if let Some(at) = key_written(text, &name, self.behavior()) {
                return Some(Definition::Key(at));
            }
            Some(Definition::Elsewhere(GoToDefinition {
                schema: name.schema.id().map(str::to_string),
                pointer: name.pointer,
                source: match name.origin {
                    KeyOrigin::Source(source) => Some(source),
                    KeyOrigin::ThisDocument | KeyOrigin::Unnamed => None,
                },
                name: name.name,
            }))
        })
    }

    /// Goes to the definition of the known name at the caret: its key, when
    /// the document defines it; else the host hears where it is
    /// (`on_go_to_definition`). Returns whether there was a name to go to.
    pub(in crate::editor) fn go_to_definition(&self) -> bool {
        match self.definition_at_caret() {
            None => false,
            Some(Definition::Key(at)) => {
                self.hide_hover();
                self.focus_range(at, at);
                true
            }
            Some(Definition::Elsewhere(target)) => {
                let callback = self
                    .config()
                    .key_names
                    .with_value(|input| input.on_go_to_definition);
                if let Some(callback) = callback {
                    callback.run(target);
                }
                true
            }
        }
    }
}

/// Where a known name is defined.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::editor) enum Definition {
    /// At this byte of the document: the start of its key.
    Key(crate::editor::text::TextSize),
    /// Elsewhere: what the host is told to open.
    Elsewhere(GoToDefinition),
}
