//! The byte places of the UTF-16 offsets a list of suggestions names, each
//! found once: thousands of suggestions start and end at the same few
//! places, and finding one reads the text up to it.

use std::collections::HashMap;

use super::utf16_to_byte_index;

/// UTF-16 offsets of one text as bytes, each read once.
pub(crate) struct ByteOffsets<'a> {
    source: &'a str,
    found: HashMap<usize, usize>,
}

impl<'a> ByteOffsets<'a> {
    pub(crate) fn new(source: &'a str) -> Self {
        Self {
            source,
            found: HashMap::new(),
        }
    }

    /// The byte index of UTF-16 `offset` (clamped to the end).
    pub(crate) fn byte(&mut self, offset: usize) -> usize {
        let source = self.source;
        *self
            .found
            .entry(offset)
            .or_insert_with(|| utf16_to_byte_index(source, offset))
    }
}
