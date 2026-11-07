//! Protected fields: read-only values (JSON Schema `readOnly`, with
//! `x-readonly-reason`, plus the host's read-only paths and ranges) and
//! secrets (`writeOnly`, `x-secret`) with their placeholder tokens.
//!
//! The shared part, computed per text on screen in any language:
//! - `marks`: what the schema marks on each field;
//! - `locate`, `locks`: where each protected field is written (by path,
//!   with everything inside a container);
//! - `guard`: one check every edit passes, whatever made it;
//! - `save`: the last check before an Edit-as save reaches the host;
//! - `tokens`, `secrets`: secret placeholders, atomic and followed through
//!   edits; `problems`: placeholders the host did not give;
//! - `suggest`: what suggestions leave out, and reference forms in secrets;
//! - `live`: the editor's signals, the check wired in, the tooltip;
//! - `view`: the gutter lock and muted values of each line;
//! - `texts`: the built-in texts.

mod guard;
mod live;
mod locate;
mod locks;
mod marks;
mod problems;
mod save;
mod secrets;
mod suggest;
mod texts;
mod tokens;
mod view;

pub(crate) use live::{Inputs, ProtectInit, ProtectState, Tip};
pub(crate) use marks::mentions_marks;
#[cfg(feature = "schema-validation")]
pub(crate) use marks::schema_marks;
pub(crate) use problems::unknown_placeholders;
pub(crate) use save::ReadOnlyChange;
pub(crate) use suggest::{Around, adjust as adjust_suggestions};
pub(crate) use texts::{save_refused, skipped_matches};
pub use tokens::SecretOptions;
pub(crate) use tokens::same_field;
pub(crate) use view::{LineMarks, line_marks};

#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod extend_tests;
#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod moved_token_tests;
#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod secret_tests;
#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod tests;
