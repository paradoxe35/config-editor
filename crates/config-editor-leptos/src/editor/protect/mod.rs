//! Protected fields: read-only values (JSON Schema `readOnly`, with
//! `x-readonly-reason`, plus the host's read-only paths and ranges) and
//! secrets (`writeOnly`, `x-secret`) with their placeholder tokens.
//!
//! The shared part, computed per text on screen in any language:
//! - `marks`: what the schema marks on each field;
//! - `locate`, `locks`: where each protected field is written (by path,
//!   with everything inside a container); `carry`: the locks moved
//!   through an edit leaving the text unparseable; `scan`: the fields of
//!   a text that does not parse, line by line, when there is none to
//!   carry from;
//! - `guard`: one check every edit passes, whatever made it;
//! - `save`: the last check before an Edit-as save reaches the host;
//! - `tokens`, `secrets`: secret placeholders, atomic and followed through
//!   edits; `problems`: placeholders the host did not give;
//! - `suggest`: what suggestions leave out, and reference forms in secrets;
//! - `live`: the editor's signals, the check wired in, the tooltip;
//! - `view`: the gutter lock and muted values of each line;
//! - `texts`: the built-in texts.

mod carry;
mod components;
mod guard;
mod hooks;
mod live;
mod locate;
mod locks;
mod marks;
mod problems;
mod save;
mod scan;
mod secrets;
mod suggest;
mod texts;
mod tokens;
mod view;

pub(super) use components::ProtectTipView;
#[cfg(feature = "schema-validation")]
pub(crate) use hooks::secret_pointers;
pub(crate) use live::{Inputs, ProtectInit, ProtectState};
pub(crate) use marks::mentions_marks;
pub(crate) use save::ReadOnlyChange;
pub(crate) use texts::save_refused;

#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod arrival_tests;
#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod broken_tests;
#[cfg(all(
    test,
    feature = "schema-validation",
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod browse_tests;
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
mod large_broken_tests;
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
mod placeholder_scale_tests;
#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod repair_tests;
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
