//! A baseline conflict in a document that is not shown. Its notice is
//! `role="alert"` inside the document's panel, which is `inert` and hidden
//! while another document shows, so nothing says it opened, and it stays
//! silent when the panel comes back (an alert revealed by its ancestor is
//! not announced). The view's own live region says it instead, when the
//! conflict opens in the background and when the document is shown with
//! it still open, and the tab's accessible name carries it meanwhile.

use leptos::prelude::*;

use super::entries::{Documents, Entry};
use crate::BaselineConflict;
use crate::live_region::say;

/// What the document's editor reports about its baseline conflict, to be
/// passed as its `on_baseline_conflict`: it keeps the entry's `conflicted`
/// and says the conflict when it opens while the document is not `shown`.
/// Also says it when the document is shown again with the conflict still
/// open. Call once, from the panel's view (it makes an effect).
pub(super) fn follow_conflict(
    documents: Documents,
    entry: &Entry,
    shown: Memo<bool>,
) -> Callback<BaselineConflict> {
    let message = documents.text(|strings| &strings.baseline_changed);
    let region = documents.said;
    let (name, conflicted) = (entry.file_name, entry.conflicted);
    let tell = move || {
        let said = format!("{}: {}", name.get_untracked(), untrack(message));
        say(region, said);
    };
    Effect::new(move |was_shown: Option<bool>| {
        let now = shown.get();
        if now && was_shown == Some(false) && conflicted.get_untracked() {
            tell();
        }
        now
    });
    Callback::new(move |how: BaselineConflict| {
        let open = how == BaselineConflict::Opened;
        conflicted.set(open);
        if open && !shown.get_untracked() {
            tell();
        }
    })
}
