//! The places a language's [`CompletionContext`] names in the text, as
//! bytes: the public language API counts them in UTF-16 units, and they
//! are converted here, once per suggestion run.

use super::*;

/// The places of a [`CompletionContext`], as bytes of its text.
#[derive(Clone)]
pub(super) struct Placed {
    /// What an accepted suggestion replaces (`replace_start..replace_end`).
    pub(super) replace: TextRange,
    /// The other places the key being renamed is written (`paired_names`).
    pub(super) paired_names: Vec<TextRange>,
    /// What an inserted entry replaces instead (`insert_range`).
    pub(super) insert_range: Option<TextRange>,
    /// What a block laid out over lines replaces (the `block`'s range).
    pub(super) block: Option<TextRange>,
}

impl Placed {
    /// The places `context` names in `source`.
    pub(super) fn of(source: &str, context: &CompletionContext) -> Self {
        let range = |start: usize, end: usize| {
            let start = byte_of(source, start);
            TextRange::new(start, byte_of(source, end).max(start))
        };
        Self {
            replace: range(context.replace_start, context.replace_end),
            paired_names: context
                .paired_names
                .iter()
                .map(|&(start, end)| range(start, end))
                .collect(),
            insert_range: context.insert_range.map(|(start, end)| range(start, end)),
            block: context
                .block
                .as_ref()
                .map(|block| range(block.start, block.end)),
        }
    }
}
