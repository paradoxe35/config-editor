//! Positions in the text: UTF-8 byte offsets inside the editor
//! (`text-size`'s `TextSize` and `TextRange`, as rust-analyzer, rowan and
//! Biome use them), UTF-16 code units only at the browser's edge
//! ([`Utf16`]: the textarea's selection, the host's ranges as placed on
//! screen).

pub(crate) mod graphemes;
mod offset;

#[cfg_attr(
    not(test),
    expect(
        unused_imports,
        reason = "the browser's offsets move onto it from `usize`"
    )
)]
pub(crate) use offset::Utf16;
pub(crate) use text_size::{TextRange, TextSize};
