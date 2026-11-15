//! A host's own text through `replaceValue` (a reset, a discard) comes
//! without ranges: the read-only ranges its editor holds go with the text
//! they cover, moved from where the edits since put them into the new
//! text. Placed again where they were given, they would lock other text:
//! ranges a host gave for the document it held, or moved there from a
//! late echo, name places in that text, not in the reset's (QA 37,
//! FW37-2: a lock fell a line below its field after a reset, and the
//! field was edited and saved).

use config_editor_leptos::{EditorHandle, TextRange};

use crate::remap::moved_between;

/// The ranges of the editor of `handle`, where they are now in its
/// document `current` (before it mounts, the ranges `given` for it),
/// moved to `text`; `None` when there are none, or when `text` is
/// `current`.
pub(crate) fn moved_to(
    handle: &EditorHandle,
    current: &str,
    text: &str,
    given: impl FnOnce() -> Vec<TextRange>,
) -> Option<Vec<TextRange>> {
    if current == text {
        return None;
    }
    let ranges =
        config_editor_leptos::js_support::read_only_ranges_now(handle).unwrap_or_else(given);
    if ranges.is_empty() {
        return None;
    }
    moved_between(current, text, Some(ranges), None).0
}
