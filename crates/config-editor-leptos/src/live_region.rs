//! Live regions: the one way their text is set, so a message is said.

use leptos::prelude::*;

/// Puts `message` in a live region's text so it is said; the same message
/// again is said again (a no-break space tells the two apart). Every live
/// region of the editor and of the documents view is set through this.
pub(crate) fn say(region: RwSignal<String>, message: String) {
    let repeated = region.with_untracked(|shown| shown.trim_end_matches('\u{a0}') == message);
    let message = if repeated && !region.with_untracked(|shown| shown.ends_with('\u{a0}')) {
        format!("{message}\u{a0}")
    } else {
        message
    };
    region.set(message);
}
