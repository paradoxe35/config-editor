//! Positions in the text: UTF-8 byte offsets inside the editor
//! (`text-size`'s `TextSize` and `TextRange`, as rust-analyzer, rowan and
//! Biome use them), UTF-16 code units only at the browser's edge
//! ([`Utf16`]: the textarea's selection, the host's ranges as placed on
//! screen).

#![deny(
    clippy::string_slice,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

pub(crate) mod lines;
mod offset;
pub(crate) mod slice;

pub(crate) use offset::{Utf16, byte_of, byte_range, checked_range, text_size, utf16_of};
pub(crate) use text_size::{TextRange, TextSize};
