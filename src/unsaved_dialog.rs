//! The documents demo's "discard unsaved changes?" question, as a modal
//! dialog: a native `<dialog>` opened with `showModal()`, so focus moves
//! into it, Tab stays inside, Escape cancels, the page behind is inert and
//! the dialog stays inside the window at any zoom. Focus goes back to
//! where it was when the question goes away.

use leptos::html;
use leptos::prelude::*;
use leptos::wasm_bindgen::JsCast;
use web_sys::{HtmlDialogElement, HtmlElement};

/// The question for the document `asking` names (empty: none asked); it
/// closes on Cancel or Escape (`asking` is set to `None`) and `discard`
/// runs on Discard.
#[component]
pub fn UnsavedDialog(
    asking: RwSignal<Option<String>>,
    #[prop(into)] discard: Callback<()>,
) -> impl IntoView {
    view! {
        <Show when=move || asking.get().is_some()>
            <OpenDialog asking=asking discard=discard />
        </Show>
    }
}

/// Keeps Tab inside `dialog`: past its last button it goes to the first,
/// before its first to the last (a modal dialog alone lets it out to the
/// browser's own controls).
fn keep_tab_inside(dialog: &HtmlDialogElement, event: &web_sys::KeyboardEvent) {
    if event.key() != "Tab" {
        return;
    }
    let Ok(buttons) = dialog.query_selector_all("button") else {
        return;
    };
    let (Some(first), Some(last)) = (
        buttons.item(0),
        buttons.item(buttons.length().saturating_sub(1)),
    ) else {
        return;
    };
    let active = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.active_element());
    let edge = if event.shift_key() { &first } else { &last };
    let wrap_to = if event.shift_key() {
        last.clone()
    } else {
        first.clone()
    };
    if active
        .is_none_or(|active| active.is_same_node(Some(edge)) || !dialog.contains(Some(&active)))
    {
        event.prevent_default();
        if let Ok(target) = wrap_to.dyn_into::<HtmlElement>() {
            let _ = target.focus();
        }
    }
}

/// The dialog while it is asked: modal from the moment it exists.
#[component]
fn OpenDialog(asking: RwSignal<Option<String>>, discard: Callback<()>) -> impl IntoView {
    let dialog = NodeRef::<html::Dialog>::new();
    let before = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.active_element())
        .and_then(|element| element.dyn_into::<HtmlElement>().ok());
    dialog.on_load(|element: HtmlDialogElement| {
        let _ = element.show_modal();
    });
    on_cleanup(move || {
        // Focus goes nowhere while the modal dialog is still open.
        if let Some(element) = dialog.get_untracked() {
            element.close();
        }
        // To where it was; if that is gone (the document was discarded),
        // to the tab now selected, once the tabs are drawn again.
        request_animation_frame(move || {
            let back = before.filter(|element| element.is_connected()).or_else(|| {
                web_sys::window()
                    .and_then(|window| window.document())
                    .and_then(|document| {
                        document
                            .query_selector("[role=tab][aria-selected=true]")
                            .ok()
                            .flatten()
                    })
                    .and_then(|element| element.dyn_into::<HtmlElement>().ok())
            });
            if let Some(element) = back {
                let _ = element.focus();
            }
        });
    });
    view! {
        <dialog
            node_ref=dialog
            class="demo__ask"
            role="alertdialog"
            aria-label="Unsaved changes"
            on:cancel=move |_: web_sys::Event| asking.set(None)
            on:keydown=move |event| {
                if let Some(element) = dialog.get_untracked() {
                    keep_tab_inside(&element, &event);
                }
            }
        >
            <p class="demo__ask-text">
                {move || format!("Discard unsaved changes to {}?", asking.get().unwrap_or_default())}
            </p>
            <div class="demo__ask-buttons">
                <button class="demo__button demo__discard" type="button" on:click=move |_| discard.run(())>"Discard"</button>
                <button class="demo__button demo__keep" type="button" on:click=move |_| asking.set(None)>"Cancel"</button>
            </div>
        </dialog>
    }
}
