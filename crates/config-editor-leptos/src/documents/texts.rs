//! The built-in texts of the documents view, in one place for the strings
//! setting that will make every text configurable.

/// The tab list's accessible name, unless the host gives one.
pub(super) const TAB_LIST_LABEL: &str = "Open files";
/// Shown in place of an editor while no document is open.
pub(super) const NO_DOCUMENTS: &str = "No open files";
/// The close button's tooltip.
pub(super) const CLOSE_TITLE: &str = "Close (Delete)";
/// The close button's accessible name, before the file name.
pub(super) const CLOSE_LABEL: &str = "Close";
/// The dirty dot's tooltip, and what a tab's accessible name adds for it.
pub(super) const UNSAVED: &str = "unsaved changes";
/// What a read-only document's tab adds to its name.
pub(super) const READ_ONLY: &str = "read-only";
