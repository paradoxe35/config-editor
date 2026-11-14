//! Scrolling the text from outside it, as the wheel or a finger over the
//! text itself does: from a popup over it (`popup_pointer`) and from the
//! gutter beside it. The text scrolls while it can; at its ends, the
//! nearest scrolling container of the editor, else the page, as the
//! browser goes on from the text itself.

use leptos::prelude::*;
use web_sys::{Element, TouchEvent, WheelEvent};

use super::metrics::read_editor_metrics;
use super::state::EditorState;

/// The wheel over the gutter: the text scrolls (the page at its ends), as
/// with the wheel over the text. Ctrl+wheel is left to the browser (zoom).
pub(super) fn gutter_wheel(state: EditorState, event: &WheelEvent) {
    if event.ctrl_key() {
        return;
    }
    let (dx, dy) = wheel_pixels(state, event);
    // Shift turns the wheel sideways, as over the text.
    let (dx, dy) = if event.shift_key() && dx == 0.0 {
        (dy, 0.0)
    } else {
        (dx, dy)
    };
    event.prevent_default();
    scroll_past(state, dx, dy);
}

/// A finger dragging over the gutter, which the stylesheet keeps from
/// panning the page (`touch-action: pinch-zoom`): where it was, and what
/// the drag scrolls, chosen at its first move as the browser chooses for a
/// drag over the text: the text if it can move that way, else what is
/// around it.
#[derive(Clone, Copy, Default)]
pub(super) struct Swipe {
    at: Option<(f64, f64)>,
    latched: Option<Latch>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Latch {
    Text,
    Around,
}

/// A finger down on the gutter: a drag starts (two fingers pinch, and
/// scroll nothing).
pub(super) fn gutter_touch_start(swipe: StoredValue<Swipe>, event: &TouchEvent) {
    swipe.set_value(Swipe {
        at: one_finger(event),
        latched: None,
    });
}

/// The finger moved: the text (or what is around it) scrolls with it.
pub(super) fn gutter_touch_move(state: EditorState, swipe: StoredValue<Swipe>, event: &TouchEvent) {
    let Some((x, y)) = one_finger(event) else {
        swipe.set_value(Swipe::default());
        return;
    };
    let Swipe {
        at: Some((from_x, from_y)),
        latched,
    } = swipe.get_value()
    else {
        return;
    };
    let (dx, dy) = (from_x - x, from_y - y);
    let latched = match latched {
        Some(Latch::Text) => {
            scroll_text(state, dx, dy);
            Latch::Text
        }
        Some(Latch::Around) => {
            scroll_around(state, dx, dy);
            Latch::Around
        }
        None if scroll_past(state, dx, dy) => Latch::Text,
        None => Latch::Around,
    };
    swipe.set_value(Swipe {
        at: Some((x, y)),
        latched: Some(latched),
    });
}

/// The finger lifted: the drag ends.
pub(super) fn gutter_touch_end(swipe: StoredValue<Swipe>) {
    swipe.set_value(Swipe::default());
}

/// Where the one finger on the screen is; `None` for two or more.
fn one_finger(event: &TouchEvent) -> Option<(f64, f64)> {
    let touches = event.touches();
    if touches.length() != 1 {
        return None;
    }
    let touch = touches.get(0)?;
    Some((f64::from(touch.client_x()), f64::from(touch.client_y())))
}

/// The wheel's move in pixels: lines and pages count as the text's.
pub(super) fn wheel_pixels(state: EditorState, event: &WheelEvent) -> (f64, f64) {
    let unit = match event.delta_mode() {
        WheelEvent::DOM_DELTA_LINE => state
            .view()
            .textarea_ref
            .get_untracked()
            .map_or(16.0, |text| read_editor_metrics(&text).line_height),
        WheelEvent::DOM_DELTA_PAGE => state
            .view()
            .textarea_ref
            .get_untracked()
            .map_or(400.0, |text| f64::from(text.client_height())),
        _ => 1.0,
    };
    (event.delta_x() * unit, event.delta_y() * unit)
}

/// Scrolls the text by `(dx, dy)`; what it cannot take (at its ends), the
/// nearest scrolling container of the editor, else the page. Whether the
/// text moved.
pub(super) fn scroll_past(state: EditorState, dx: f64, dy: f64) -> bool {
    let (rest_x, rest_y) = scroll_text(state, dx, dy);
    if rest_x != 0.0 || rest_y != 0.0 {
        scroll_around(state, rest_x, rest_y);
    }
    rest_x != dx || rest_y != dy
}

/// Scrolls the text by `(dx, dy)`; what it did not take, along each axis.
fn scroll_text(state: EditorState, dx: f64, dy: f64) -> (f64, f64) {
    let Some(text) = state.view().textarea_ref.get_untracked() else {
        return (0.0, 0.0);
    };
    let (left, top) = (text.scroll_left(), text.scroll_top());
    text.set_scroll_left(left + dx.round() as i32);
    text.set_scroll_top(top + dy.round() as i32);
    (
        if text.scroll_left() != left { 0.0 } else { dx },
        if text.scroll_top() != top { 0.0 } else { dy },
    )
}

/// Scrolls the nearest scrolling container of the editor by `(dx, dy)`,
/// else the page.
fn scroll_around(state: EditorState, dx: f64, dy: f64) {
    let root: Option<Element> = state.view().root_ref.get_untracked().map(Into::into);
    match root.and_then(|root| scrolling_ancestor(&root, dx, dy)) {
        Some(container) => container.scroll_by_with_x_and_y(dx, dy),
        None => {
            if let Some(window) = web_sys::window() {
                window.scroll_by_with_x_and_y(dx, dy);
            }
        }
    }
}

/// The nearest container of `element` that scrolls by `(dx, dy)`, short of
/// the page itself.
fn scrolling_ancestor(element: &Element, dx: f64, dy: f64) -> Option<Element> {
    let window = web_sys::window()?;
    let page = window.document()?.scrolling_element();
    let mut at = element.parent_element();
    while let Some(container) = at {
        if page.as_ref() == Some(&container) {
            return None;
        }
        let style = window.get_computed_style(&container).ok().flatten();
        let scrolls = |axis: &str, delta: f64, room: i32| {
            delta != 0.0
                && room > 0
                && style
                    .as_ref()
                    .and_then(|style| style.get_property_value(axis).ok())
                    .is_some_and(|value| matches!(value.as_str(), "auto" | "scroll" | "overlay"))
        };
        let room_y = container.scroll_height() - container.client_height();
        let room_x = container.scroll_width() - container.client_width();
        if scrolls("overflow-y", dy, room_y) || scrolls("overflow-x", dx, room_x) {
            return Some(container);
        }
        at = container.parent_element();
    }
    None
}
