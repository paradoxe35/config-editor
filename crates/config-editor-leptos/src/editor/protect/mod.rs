//! Protected fields: read-only values (JSON Schema `readOnly`, with
//! `x-readonly-reason`, plus the host's read-only paths and ranges).
//!
//! The shared part, computed per text on screen in any language:
//! - `marks`: what the schema marks on each field;
//! - `locate`, `locks`: where each protected field is written (by path,
//!   with everything inside a container);
//! - `guard`: one check every edit passes, whatever made it;
//! - `save`: the last check before an Edit-as save reaches the host;
//! - `suggest`: what suggestions leave out;
//! - `live`: the editor's signals, the check wired in, the tooltip;
//! - `view`: the gutter lock and muted values of each line;
//! - `texts`: the built-in texts.

mod guard;
mod live;
mod locate;
mod locks;
mod marks;
mod save;
mod suggest;
mod texts;
mod view;

pub(crate) use live::{Inputs, ProtectInit, ProtectState, Tip};
pub(crate) use marks::mentions_marks;
pub(crate) use suggest::{Around, adjust as adjust_suggestions};
pub(crate) use texts::{NOTHING_REPLACED, skipped_matches};
pub(crate) use view::{LineMarks, line_marks};

#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod tests;
