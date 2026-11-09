//! Whether the host gives the editor its height (`height: 100%` in an
//! element of a set height, a documents view's panel): then the text area
//! takes whatever the toolbar and footer leave, however short or tall,
//! instead of keeping its own minimum and its 80% of the window.
//!
//! CSS cannot ask whether a height was given, so a probe does: a float of
//! no width and `height: 100%`, which is the editor's height when that is
//! set and nothing when the content sizes the editor (a percentage of an
//! automatic height). A float takes no room beside the frame.

use leptos::prelude::*;

use crate::reactive::size_observer;

/// The class the root takes while its height is given.
pub(super) const FILLS_CLASS: &str = " config-editor__fills";

/// The probe, which sets `fills` while the editor's height is given.
#[component]
pub(super) fn FillProbe(fills: RwSignal<bool>) -> impl IntoView {
    let probe: NodeRef<leptos::html::Div> = NodeRef::new();
    Effect::new(move |_| {
        if let Some(element) = probe.get() {
            let measured = element.clone();
            untrack(|| {
                size_observer(&element, move || {
                    let given = measured.get_bounding_client_rect().height() > 0.5;
                    if fills
                        .try_get_untracked()
                        .is_some_and(|fills| fills != given)
                    {
                        fills.set(given);
                    }
                });
            });
        }
    });
    view! { <div class="config-editor__fill-probe" aria-hidden="true" node_ref=probe></div> }
}
