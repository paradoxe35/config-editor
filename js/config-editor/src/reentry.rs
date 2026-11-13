//! Calls made while another call of the same instance applies the input
//! still pending: `update` and `replaceValue` apply it first, which runs the
//! host's `onChange`, and the host may call `update` or `replaceValue` again
//! from there. Those calls are newer than the outer one, whose options were
//! read before the input reached the host: what they set stays, and the
//! outer call leaves it alone.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;

use js_sys::{Object, Reflect};
use wasm_bindgen::prelude::*;

/// What calls inside a flush of an instance set.
#[derive(Default)]
pub(crate) struct Reentry {
    /// Flushes of the instance going on.
    depth: Cell<usize>,
    /// The keys calls inside them set (`value`, `documents`, `readOnlyRanges`,
    /// `value:<id>` for one document's text, …).
    newer: RefCell<HashSet<String>>,
}

impl Reentry {
    /// Runs `flush`, the input still pending applied; returns what calls of
    /// the instance made during it set.
    pub(crate) fn flush(&self, flush: impl FnOnce()) -> HashSet<String> {
        let outer = self.depth.get() == 0;
        self.depth.set(self.depth.get() + 1);
        flush();
        self.depth.set(self.depth.get() - 1);
        if outer {
            std::mem::take(&mut *self.newer.borrow_mut())
        } else {
            // An inner call is the newest: it keeps all it sets.
            HashSet::new()
        }
    }

    /// Notes the keys `options` sets, when a flush of the instance is going
    /// on (this call is newer than the one flushing).
    pub(crate) fn note_options(&self, options: &JsValue) {
        if self.depth.get() == 0 {
            return;
        }
        let keys = Object::keys(options.unchecked_ref::<Object>());
        let mut newer = self.newer.borrow_mut();
        for key in keys.iter() {
            let set = Reflect::get(options, &key).is_ok_and(|value| !value.is_undefined());
            if set && let Some(key) = key.as_string() {
                newer.insert(key);
            }
        }
    }

    /// Notes one key set, when a flush of the instance is going on.
    pub(crate) fn note(&self, key: &str) {
        if self.depth.get() > 0 {
            self.newer.borrow_mut().insert(key.to_string());
        }
    }
}

/// `options` without the keys in `newer`: a copy when it sets any of them.
/// A `value` left out still places the call's ranges (`readOnlyRanges`,
/// `hostDiagnostics`): they are given for it, so it becomes their
/// `rangesFor`.
pub(crate) fn without_newer(options: &JsValue, newer: &HashSet<String>) -> JsValue {
    let sets = |key: &str| {
        Reflect::get(options, &JsValue::from_str(key)).is_ok_and(|value| !value.is_undefined())
    };
    if !newer.iter().any(|key| sets(key)) {
        return options.clone();
    }
    let copy: JsValue = Object::assign(&Object::new(), options.unchecked_ref()).into();
    if newer.contains("value") && sets("value") && !sets("rangesFor") {
        let value =
            Reflect::get(options, &JsValue::from_str("value")).unwrap_or(JsValue::UNDEFINED);
        let _ = Reflect::set(&copy, &JsValue::from_str("rangesFor"), &value);
    }
    for key in newer {
        let _ = Reflect::delete_property(copy.unchecked_ref::<Object>(), &JsValue::from_str(key));
    }
    copy
}
