//! The built-in texts of host-controlled editing, in one place for the
//! strings setting that will make every text configurable.

/// The notice shown when the host's baseline changes under unsaved edits.
pub(in crate::editor) const BASELINE_CHANGED: &str = "The file changed on the server.";
/// What the notice's choices do, read after its message.
pub(in crate::editor) const BASELINE_CHANGED_DETAIL: &str =
    "Reload it, losing your changes, or keep your text and compare it with the new version.";
/// The notice's button that takes the new baseline as the text.
pub(in crate::editor) const RELOAD: &str = "Reload";
/// Its tooltip.
pub(in crate::editor) const RELOAD_TITLE: &str =
    "Replace your text with the new version (Ctrl+Z brings yours back)";
/// The notice's button that keeps the text and diffs it against the new
/// baseline.
pub(in crate::editor) const KEEP_MINE: &str = "Keep mine";
/// Its tooltip.
pub(in crate::editor) const KEEP_MINE_TITLE: &str =
    "Keep your text; the changes are now shown against the new version (Escape)";
