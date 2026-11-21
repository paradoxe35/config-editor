//! The texts an editor sent that its host has not handed back yet, handed
//! from an editor `destroy` removed to the next one made in the same
//! element (the core's `HostChannel::handoff` and `HostChannel::new`): the
//! element is the slot the host shows the document in, so a hide and show
//! there (a component that unmounts and mounts again, a host that rebuilds
//! its editor) keeps telling that editor's late echoes apart. The record
//! is kept on the element itself, as numbers, not in the module: it goes
//! with the element when no editor takes it over.

use js_sys::{BigUint64Array, Reflect, Symbol};
use leptos_config_editor::js_support::Handoff;
use wasm_bindgen::{JsCast, JsValue};

/// The element's property that holds the record.
fn key() -> JsValue {
    Symbol::for_("@pngwasi/config-editor:handoff").into()
}

/// Leaves `handoff` (the record of an editor `element` held) for the next
/// editor made there; `None` clears what an older one left.
pub(crate) fn leave(element: &web_sys::HtmlElement, handoff: Option<Handoff>) {
    let key = key();
    match handoff {
        Some(handoff) => {
            let words = handoff.to_words();
            let array = BigUint64Array::new_with_length(words.len() as u32);
            array.copy_from(&words);
            let _ = Reflect::set(element, &key, &array);
        }
        None => {
            let _ = Reflect::delete_property(element.unchecked_ref::<js_sys::Object>(), &key);
        }
    }
}

/// Takes the record an editor destroyed in `element` left, if any.
pub(crate) fn take(element: &web_sys::HtmlElement) -> Option<Handoff> {
    let key = key();
    let left = Reflect::get(element, &key).ok()?;
    let _ = Reflect::delete_property(element.unchecked_ref::<js_sys::Object>(), &key);
    Handoff::from_words(&left.dyn_into::<BigUint64Array>().ok()?.to_vec())
}
