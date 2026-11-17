//! The form a value the user changed in a view is written in. A value
//! the user left alone keeps its host text (`links::untouched`, and the
//! patches that leave equal values as written); a changed one has no
//! host text of its own, so the save decides its form here:
//! - a number or bare value spelled as the user typed it, or in the form
//!   of the one it replaces (`typed_spellings`);
//! - a whole float a view showed as an integer stays a float
//!   (`whole_floats`);
//! - a value edited in a view that writes every scalar as text keeps the
//!   host's type (`text_scalars`);
//! - a quoted string emptied, saved and typed again keeps its quotes
//!   (`emptied_quotes`);
//! - an integer past 64 bits is saved as typed where the host holds it as
//!   written (`wide_numbers`).

pub(super) mod emptied_quotes;
pub(super) mod text_scalars;
pub(super) mod typed_spellings;
#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod typed_spellings_tests;
pub(super) mod whole_floats;
pub(super) mod wide_numbers;
