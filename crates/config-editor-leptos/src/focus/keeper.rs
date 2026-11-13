//! Focus that stays when the focused control goes away.
//!
//! A control that is removed, disabled, hidden or moved while it has the
//! focus leaves the focus on the page's `BODY`: a keyboard user starts over
//! from the top of the page and a screen reader says nothing. Which control
//! is going is not the code's to know at every place that can remove one (a
//! count that drops to zero, a notice dismissed, a list the host reorders),
//! so one keeper watches a view for it: it remembers the control that has
//! the focus, and after each batch of changes to the view, a control that
//! has gone gets the focus back when it is only moved, or else a successor
//! the view names. A focus the user moved elsewhere (a click on the page,
//! the window or the browser's own controls taking it) is never taken back.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{FocusEvent, HtmlElement, MutationObserver, MutationObserverInit};

use super::in_place;
use crate::reactive::later;

/// What blocks a control from taking the focus, on it or around it.
const BLOCKED: &str = "[hidden], [inert]";

/// Names the control to take the focus from the one that went.
type Successor = dyn Fn(&HtmlElement) -> Option<HtmlElement>;

/// What the keeper knows.
#[derive(Default)]
struct Watch {
    /// The control of the view that has the focus.
    last: RefCell<Option<HtmlElement>>,
    /// The focus left with no control to go to, and nothing has said why
    /// yet: a click on the page, or the control moving or going.
    blurred: Cell<bool>,
    /// The control, or what holds it, was moved, removed, disabled or
    /// hidden since the last look at the focus.
    touched: Cell<bool>,
    /// A look at the focus waits for the next frame.
    waiting: Cell<bool>,
}

/// Keeps the focus inside `root` while the current owner lives. When the
/// control that has it goes away, `successor` is asked, with that control,
/// for the one to take it (`None`: leave it).
pub(crate) fn keep_focus(
    root: &HtmlElement,
    successor: impl Fn(&HtmlElement) -> Option<HtmlElement> + 'static,
) {
    let watch = Rc::new(Watch::default());
    let alive = StoredValue::new(());
    let focusin = {
        let watch = Rc::clone(&watch);
        Closure::<dyn Fn(FocusEvent)>::new(move |event: FocusEvent| {
            watch.blurred.set(false);
            *watch.last.borrow_mut() = event
                .target()
                .and_then(|target| target.dyn_into::<HtmlElement>().ok());
        })
    };
    let focusout = {
        let watch = Rc::clone(&watch);
        Closure::<dyn Fn(FocusEvent)>::new(move |event: FocusEvent| {
            if event.related_target().is_some() {
                // The focus went to another control; its focusin says which.
                watch.last.borrow_mut().take();
                return;
            }
            watch.blurred.set(true);
            // A change to the control that caused it is heard before this
            // timer; with none, the focus was taken away, and is not ours
            // to return.
            let watch = Rc::clone(&watch);
            later::after(alive, 0, move |_| {
                if watch.blurred.replace(false) && !watch.touched.get() {
                    watch.last.borrow_mut().take();
                }
            });
        })
    };
    let successor: Rc<Successor> = Rc::new(successor);
    let changed = {
        let watch = Rc::clone(&watch);
        Closure::<dyn Fn(web_sys::js_sys::Array)>::new(move |records: web_sys::js_sys::Array| {
            // The rows re-render on every keystroke and scroll: while the
            // focus stays on a usable control, nothing to read.
            if still_kept(&watch) || !concerns_last(&watch, &records) {
                return;
            }
            watch.touched.set(true);
            // The view's own focus moves (a switch of document focuses the
            // editor it shows) come in the next frame: this look comes
            // after them, and finds the focus placed.
            if watch.waiting.replace(true) {
                return;
            }
            let (watch, successor) = (Rc::clone(&watch), Rc::clone(&successor));
            later::next_frame(alive, move |_| {
                watch.waiting.set(false);
                watch.touched.set(false);
                restore(&watch, successor.as_ref());
            });
        })
    };
    let Ok(observer) = MutationObserver::new(changed.as_ref().unchecked_ref()) else {
        return;
    };
    let options = MutationObserverInit::new();
    options.set_child_list(true);
    options.set_subtree(true);
    options.set_attributes(true);
    options.set_attribute_filter(&web_sys::js_sys::Array::of3(
        &"disabled".into(),
        &"hidden".into(),
        &"inert".into(),
    ));
    let _ = observer.observe_with_options(root, &options);
    let _ = root.add_event_listener_with_callback("focusin", focusin.as_ref().unchecked_ref());
    let _ = root.add_event_listener_with_callback("focusout", focusout.as_ref().unchecked_ref());
    // Browser objects stay on the one wasm thread.
    let held = StoredValue::new_local((root.clone(), observer, focusin, focusout, changed));
    on_cleanup(move || {
        held.with_value(|(root, observer, focusin, focusout, _)| {
            observer.disconnect();
            let _ = root
                .remove_event_listener_with_callback("focusin", focusin.as_ref().unchecked_ref());
            let _ = root
                .remove_event_listener_with_callback("focusout", focusout.as_ref().unchecked_ref());
        });
    });
}

/// Whether the control that had the focus (if any) still has it and may:
/// a constant check, before any mutation record is read.
fn still_kept(watch: &Watch) -> bool {
    let last = watch.last.borrow();
    let Some(last) = last.as_ref() else {
        return true;
    };
    let active = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.active_element());
    active.as_ref() == Some(last.as_ref()) && usable(last)
}

/// Whether any of the mutation `records` added, removed, disabled or hid
/// the control that has the focus, or what holds it.
fn concerns_last(watch: &Watch, records: &web_sys::js_sys::Array) -> bool {
    let Some(last) = watch.last.borrow().clone() else {
        return false;
    };
    let holds = |node: &web_sys::Node| node.contains(Some(&last));
    records.iter().any(|record| {
        let Ok(record) = record.dyn_into::<web_sys::MutationRecord>() else {
            return false;
        };
        let moved = |list: web_sys::NodeList| {
            (0..list.length()).any(|index| list.item(index).is_some_and(|node| holds(&node)))
        };
        if record.type_() == "attributes" {
            return record.target().is_some_and(|node| holds(&node));
        }
        moved(record.added_nodes()) || moved(record.removed_nodes())
    })
}

/// After a change to the view: gives the focus back, or on, when the
/// control that had it has gone.
fn restore(watch: &Watch, successor: &Successor) {
    let Some(last) = watch.last.borrow().clone() else {
        return;
    };
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return;
    };
    let active = document.active_element();
    let focused = active.as_ref() == Some(last.as_ref());
    let dropped = active.is_none() || active == document.body().map(Into::into);
    // Not the focus that the user or a control moved elsewhere.
    if !(focused || dropped && document.has_focus().unwrap_or(false)) {
        return;
    }
    watch.blurred.set(false);
    let target = if usable(&last) {
        // Only moved, which blurs it: it takes the focus back. Still
        // focused, nothing happened to it.
        dropped.then_some(last)
    } else {
        successor(&last)
    };
    if let Some(target) = target.filter(usable) {
        let _ = in_place(&target);
    }
}

/// Whether `element` is on the page and may have the focus.
fn usable(element: &HtmlElement) -> bool {
    element.is_connected()
        && !element.matches(":disabled").unwrap_or(false)
        && element.closest(BLOCKED).ok().flatten().is_none()
}
