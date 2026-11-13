//! The demo pages' frame: a skip link to the editor, a `main` landmark and a
//! page heading (hidden from sight), so a screen reader can orient and jump.

use leptos::prelude::*;
use leptos::wasm_bindgen::JsCast;
use web_sys::HtmlElement;

/// Moves focus to the editor's text area (the first one on screen: a
/// documents view keeps its other tabs' hidden), or to `main` if there is
/// none yet.
fn skip_to_editor(event: web_sys::MouseEvent) {
    event.prevent_default();
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return;
    };
    let editors = document.query_selector_all(".config-editor__input").ok();
    let editor = (0..editors.as_ref().map_or(0, |list| list.length()))
        .filter_map(|at| editors.as_ref()?.item(at)?.dyn_into::<HtmlElement>().ok())
        .find(|element| element.offset_parent().is_some());
    let target = editor.or_else(|| {
        document
            .get_element_by_id("demo-main")?
            .dyn_into::<HtmlElement>()
            .ok()
    });
    if let Some(target) = target {
        let _ = target.focus();
    }
}

/// `content` in the page's `main`, under the heading `title`.
pub(crate) fn page(title: &'static str, content: impl IntoView + 'static) -> impl IntoView {
    view! {
        <a class="demo__skip" href="#demo-main" on:click=skip_to_editor>"Skip to the editor"</a>
        <main id="demo-main" tabindex="-1">
            <h1 class="demo__visually-hidden">{title}</h1>
            {content}
        </main>
    }
}
