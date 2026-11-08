//! Suggestions and hover docs without an editor: what
//! [`Schema::suggestions`](crate::Schema::suggestions) and
//! [`Schema::hover`](crate::Schema::hover) return, so a language written
//! outside the crate can check its completion and hover in plain tests.

use super::*;
use crate::TextPosition;

/// A suggestion the editor offers at the caret: a key or a value the schema
/// allows there, written as the language writes it.
#[derive(Clone, PartialEq)]
pub struct Suggestion(Completion);

impl Suggestion {
    /// The key or value as listed.
    pub fn label(&self) -> &str {
        &self.0.label
    }

    /// The schema's words for it (title, description), or empty.
    pub fn detail(&self) -> &str {
        &self.0.detail
    }

    /// Its type as listed (`string`, `boolean`, `object`, …).
    pub fn type_label(&self) -> &str {
        &self.0.type_label
    }

    /// A value rather than a key.
    pub fn is_value(&self) -> bool {
        self.0.is_value
    }

    /// The schema marks the key deprecated (listed last, struck through).
    pub fn is_deprecated(&self) -> bool {
        self.0.deprecated
    }

    /// `source` with the suggestion accepted, and the selection after it
    /// (UTF-16 offsets): empty at the caret, unless the insert left a
    /// default selected so typing replaces it.
    pub fn accept(&self, source: &str) -> (String, std::ops::Range<usize>) {
        let (text, anchor, caret) = apply_completion(source, &self.0);
        (text, anchor..caret)
    }
}

impl std::fmt::Debug for Suggestion {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Suggestion")
            .field("label", &self.0.label)
            .field("insert", &self.0.insert)
            .field("is_value", &self.0.is_value)
            .finish()
    }
}

/// What the hover card shows for a key: its schema docs.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct HoverDocs {
    /// The key under the pointer.
    pub key: String,
    /// The property's type as shown (`string`, `"fast" | "safe"`, …).
    pub type_label: String,
    /// The property's `title`, shown above its description.
    pub title: Option<String>,
    /// The property's `description`.
    pub description: Option<String>,
    /// Its `default`, as written in the card.
    pub default: Option<String>,
    /// Its examples, as written in the card (at most five).
    pub examples: Vec<String>,
    /// The parent object requires the key.
    pub required: bool,
    /// The schema marks the key deprecated.
    pub deprecated: bool,
}

/// The suggestions at `cursor` (UTF-16), as `Ctrl+Space` lists them.
pub(crate) fn schema_suggestions(
    bundle: &SchemaBundle,
    behavior: &dyn ConfigLanguage,
    source: &str,
    cursor: usize,
) -> Vec<Suggestion> {
    let bodies = behavior.string_body_lines(source);
    completions_in(
        source,
        cursor,
        true,
        Some(bundle),
        true,
        behavior,
        false,
        &bodies,
    )
    .into_iter()
    .map(Suggestion)
    .collect()
}

/// The hover card's docs for the key at `position`.
pub(crate) fn schema_hover(
    bundle: &SchemaBundle,
    behavior: &dyn ConfigLanguage,
    source: &str,
    position: TextPosition,
) -> Option<HoverDocs> {
    let hover = key_hover(
        source,
        position.line.checked_sub(1)?,
        position.column.checked_sub(1)?,
        Some(bundle),
        behavior,
    )?;
    Some(HoverDocs {
        key: hover.key,
        type_label: hover.type_label,
        title: hover.title,
        description: hover.description,
        default: hover.default,
        examples: hover.examples,
        required: hover.required,
        deprecated: hover.deprecated,
    })
}
