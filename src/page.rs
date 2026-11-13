//! The demo pages' frame: a skip link to the editor, a `main` landmark and a
//! page heading (hidden from sight), so a screen reader can orient and jump.

use leptos::prelude::*;

/// `content` in the page's `main`, under the heading `title`.
pub(crate) fn page(title: &'static str, content: impl IntoView + 'static) -> impl IntoView {
    view! {
        <a class="demo__skip" href="#demo-main">"Skip to the editor"</a>
        <main id="demo-main" tabindex="-1">
            <h1 class="demo__visually-hidden">{title}</h1>
            {content}
        </main>
    }
}
