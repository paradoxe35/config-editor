//! Window listeners that go with their owner, freed at once.

use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;

/// Runs `run` on each of the window's `events` until the current owner is
/// cleaned up, when the listener is removed and its closure dropped.
///
/// Leptos's `window_event_listener` hands its closure to JavaScript, which
/// frees it only when its garbage collector gets to it: memory measured
/// after an editor unmounts still held it. Here the closure stays on the
/// Rust side, with the owner.
pub(crate) fn window_listener(events: &'static [&'static str], run: impl Fn() + 'static) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let listener = Closure::<dyn Fn()>::new(run);
    for event in events {
        let _ = window.add_event_listener_with_callback(event, listener.as_ref().unchecked_ref());
    }
    // Browser objects stay on the one wasm thread.
    let registered = StoredValue::new_local((window, listener));
    on_cleanup(move || {
        registered.with_value(|(window, listener)| {
            for event in events {
                let _ = window
                    .remove_event_listener_with_callback(event, listener.as_ref().unchecked_ref());
            }
        });
    });
}
