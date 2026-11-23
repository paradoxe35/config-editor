//! What the list of uses shows of a line that holds a secret: the line as
//! problems quote it, each secret's text (a value the schema marks secret,
//! a string a placeholder is glued into) shown as `…` by the same hiding
//! the problems use (`diagnostics::SecretQuotes`), so a use written beside
//! a secret (`{role: admin, password: hunter2}`) never shows it, in the row
//! or in its accessible name.

use leptos::prelude::*;

use super::UseHit;
use crate::editor::diagnostics::{Placeholders, SecretQuotes};
use crate::editor::language::ConfigLanguage;
use crate::editor::schema::SchemaBundle;
use crate::editor::state::EditorState;

impl EditorState {
    /// Hides what the lines of `hits` quote of a secret of `text` (written
    /// in `behavior`), by this editor's schema and placeholders.
    pub(in crate::editor) fn hide_use_secrets(
        &self,
        text: &str,
        behavior: &dyn ConfigLanguage,
        hits: &mut [UseHit],
    ) {
        if hits.is_empty() {
            return;
        }
        let protect = self.protect();
        let (accepted, patterns) = (protect.accepted_tokens(), protect.placeholder_patterns());
        let quotes = self.config().schema.with_value(|schema| {
            SecretQuotes::of(
                text,
                schema.as_deref(),
                behavior,
                Placeholders {
                    accepted: &accepted,
                    patterns: &patterns,
                },
            )
        });
        hide(&quotes, hits);
    }
}

/// [`EditorState::hide_use_secrets`] for a document with no editor yet (a
/// documents view's tab never shown): by its schema's secret marks.
pub(crate) fn hide_use_secrets_by_schema(
    text: &str,
    language: crate::Language,
    schema: &SchemaBundle,
    hits: &mut [UseHit],
) {
    if hits.is_empty() {
        return;
    }
    let quotes = SecretQuotes::of(
        text,
        Some(schema),
        language.behavior(),
        Placeholders::default(),
    );
    hide(&quotes, hits);
}

fn hide(quotes: &SecretQuotes, hits: &mut [UseHit]) {
    for hit in hits {
        hit.text = quotes.hide(&hit.text);
    }
}
