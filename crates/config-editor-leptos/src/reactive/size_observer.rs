//! Watching an element's size, with the watch going with its owner.

use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;

/// Runs `run` each time `element` changes size (and once when the watch
/// starts) until the current owner is cleaned up, when the observer is
/// disconnected and its closure dropped (kept on the Rust side, as
/// [`window_listener`](super::window_listener) does).
pub(crate) fn size_observer(element: &web_sys::Element, run: impl Fn() + 'static) {
    let callback = Closure::<dyn Fn()>::new(run);
    let Ok(observer) = web_sys::ResizeObserver::new(callback.as_ref().unchecked_ref()) else {
        return;
    };
    observer.observe(element);
    // Browser objects stay on the one wasm thread.
    let registered = StoredValue::new_local((observer, callback));
    on_cleanup(move || registered.with_value(|(observer, _)| observer.disconnect()));
}
