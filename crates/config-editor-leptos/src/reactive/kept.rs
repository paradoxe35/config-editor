//! A part switched on and off that is built once and then only taken off
//! the page and put back (see the module docs of `reactive`).

use leptos::html::Template;
use leptos::prelude::*;
use leptos::tachys::view::any_view::AnyViewState;
use leptos::tachys::view::{Mountable, Render};
use wasm_bindgen::JsCast;

/// `view` on the page while `when` holds. Built the first time it is
/// shown and kept: switched off, its elements leave the page, but its
/// reactive nodes stay, subscribed to the editor's long-lived state. Built
/// anew each time, each build would stay in that state's subscribers for
/// good. What a part leaves behind when switched off (an open widget) is
/// cleared by its setting's own handler, not by unmounting.
#[component]
pub(crate) fn Kept<V>(
    #[prop(into)] when: Signal<bool>,
    view: impl Fn() -> V + Send + Sync + 'static,
) -> impl IntoView
where
    V: IntoView + 'static,
{
    // Marks where the part goes; a template never renders.
    let anchor = NodeRef::<Template>::new();
    let built: StoredValue<Option<(AnyViewState, bool)>, LocalStorage> =
        StoredValue::new_local(None);
    let owner = Owner::current().expect("Kept needs an owner");
    Effect::new(move |_| {
        let show = when.get();
        let Some(anchor) = anchor.get() else {
            return;
        };
        let Some(parent) = anchor.parent_element() else {
            return;
        };
        built.update_value(|built| match (built.as_mut(), show) {
            (None, true) => {
                let mut state = owner.with(|| untrack(|| view().into_any().build()));
                state.mount(&parent, Some(anchor.unchecked_ref()));
                *built = Some((state, true));
            }
            (Some((state, mounted)), true) if !*mounted => {
                state.mount(&parent, Some(anchor.unchecked_ref()));
                *mounted = true;
            }
            (Some((state, mounted)), false) if *mounted => {
                state.unmount();
                *mounted = false;
            }
            _ => {}
        });
    });
    view! { <template node_ref=anchor></template> }
}
