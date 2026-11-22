//! Go to definition (`F12`, `Ctrl`/`Cmd`+click, the hover card's
//! Definition link) on a known name, and the hover card's "defined in"
//! line and name links, in the live editor.

use leptos::prelude::*;

use super::definition::{key_written, name_at};
use super::{GoToDefinition, KeyOrigin, Offered};
use crate::editor::a11y::caret_position;
use crate::editor::completion::{FieldHit, HoverValue, KeyHover, NameLinks, ValueDoc, field_at};
use crate::editor::state::EditorState;
use crate::editor::text::TextSize;

impl EditorState {
    /// The known name `field` of `text` is, read with the editor's names.
    fn known_name(&self, text: &str, field: &FieldHit) -> Option<Offered> {
        let schema = self.config().schema.get_value()?;
        let behavior = self.behavior();
        self.config()
            .key_names
            .with_value(|input| name_at(text, field, &schema, behavior, input.source.as_ref()))
    }

    /// Adds to `hover` (the card's docs of `field`, if the schema has any)
    /// where the value is defined, when it is a known name with a source,
    /// and the links to its definition and uses on a name or a defining
    /// key. `None` when there are neither docs nor a name.
    pub(in crate::editor) fn add_name_docs(
        &self,
        text: &str,
        field: &FieldHit,
        hover: Option<KeyHover>,
    ) -> Option<KeyHover> {
        let known = self.known_name(text, field);
        let searched = self.searched_in(text, field);
        let mut hover = match hover {
            Some(hover) => hover,
            None if searched.is_some() => KeyHover::bare(field.key.clone(), field.start_column),
            None => return None,
        };
        if let Some(searched) = &searched {
            hover.name_links = Some(NameLinks {
                field: field.clone(),
                definition: !searched.definition && known.is_some(),
            });
        }
        let Some(name) = known else {
            return Some(hover);
        };
        let Some(line) = self.strings_untracked(|strings| name.origin.defined_in(strings)) else {
            return Some(hover);
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
        Some(hover)
    }

    /// Where the definition of the known name `field` of `text` is: its
    /// key in the document, or the place to tell the host of.
    fn definition_of(&self, text: &str, field: &FieldHit) -> Option<Definition> {
        let name = self.known_name(text, field)?;
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
    }

    /// Where the definition of the known name at byte `at` is (on the
    /// value, or just past its end).
    pub(in crate::editor) fn definition_at(&self, at: TextSize) -> Option<Definition> {
        self.doc().text.with_untracked(|text| {
            let (line, column) = caret_position(text, at);
            let (line, column) = (line - 1, column - 1);
            [Some(column), column.checked_sub(1)]
                .into_iter()
                .flatten()
                .filter_map(|at| field_at(text, line, at, self.behavior()))
                .find_map(|field| self.definition_of(text, &field))
        })
    }

    /// [`definition_at`](Self::definition_at) the caret.
    pub(in crate::editor) fn definition_at_caret(&self) -> Option<Definition> {
        self.definition_at(self.caret_now())
    }

    /// Goes to the definition of the known name at the caret: its key, when
    /// the document defines it; else the host hears where it is
    /// (`on_go_to_definition`). Returns whether there was a name to go to.
    pub(in crate::editor) fn go_to_definition(&self) -> bool {
        self.go_to(self.definition_at_caret())
    }

    /// Goes to the definition of the known name `field` (the hover card's
    /// Definition link). Returns whether there was one.
    pub(in crate::editor) fn go_to_definition_of(&self, field: &FieldHit) -> bool {
        let definition = self
            .doc()
            .text
            .with_untracked(|text| self.definition_of(text, field));
        self.go_to(definition)
    }

    fn go_to(&self, definition: Option<Definition>) -> bool {
        match definition {
            None => false,
            Some(Definition::Key(at)) => {
                self.hide_hover();
                self.focus_range(at, at);
                true
            }
            Some(Definition::Elsewhere(target)) => {
                self.hide_hover();
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
