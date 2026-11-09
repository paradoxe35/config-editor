//! The problem tip: the messages of every problem covering the place under
//! the pointer (or a focused underline's start), or where F8 or a tap went.
//! Each problem gets its own lines (see `entries`). It yields to the hover
//! card on the same line, which lists them (see `hover_problems`). It stays inside
//! the editor (in an editor too short for it, the window): above its
//! line when it fits there, else below, else on the roomier side, no
//! taller than the room there and scrolled (F8 scrolls to the problem it
//! went to), so every message can be reached.

use leptos::html::Div;
use leptos::prelude::*;

use super::super::super::metrics::read_editor_metrics;
use super::super::super::positioning::PopupFrame;
use super::super::super::problem_hit::tip_column;
use super::super::super::state::EditorState;
use super::entries::{Entry, entries_view, problems_at};
use crate::cssom::{Declarations, cssom};
use crate::reactive::later;
use wasm_bindgen::JsCast;

/// Widest the tip gets, in pixels.
const TIP_MAX_WIDTH: f64 = 360.0;
/// Narrowest the tip gets, in pixels, however narrow the editor.
const TIP_MIN_WIDTH: f64 = 120.0;
/// Least height given to the tip when neither side has room, in pixels.
const TIP_MIN_HEIGHT: f64 = 48.0;
/// Gap between the line and the tip, in pixels.
const TIP_GAP: f64 = 4.0;
/// Space kept between the tip and the editor's (or window's) edges.
const TIP_MARGIN: f64 = 4.0;

/// What the tip shows.
#[derive(Clone, Debug, PartialEq)]
struct TipContent {
    anchor: Anchor,
    entries: Vec<Entry>,
}

/// Where the tip points: one-based `line`, from rendered column `column`
/// (zero-based; tabs count two), where the innermost problem shown starts
/// there. The same while the pointer moves over the same problems, so the
/// tip stays put.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Anchor {
    line: usize,
    column: f64,
}

#[component]
pub(crate) fn ProblemTip(state: EditorState) -> impl IntoView {
    let tip_ref = state.problems().tip_ref;
    let focused = state.problems().tip_focused;
    let content = Memo::new(move |_| tip_content(state));
    // The tip's full height, measured once its entries are drawn; it
    // decides the side and the height (see `place`). Zero until the
    // current entries are measured.
    let natural = RwSignal::new(0.0_f64);
    let placement = Memo::new(move |_| {
        let anchor = content.with(|content| content.as_ref().map(|content| content.anchor))?;
        placement(state, anchor, natural.get())
    });
    let visible = Memo::new(move |_| placement.with(Option::is_some));
    Effect::new(move |seen: Option<Option<TipContent>>| {
        let now = content.get();
        if seen.is_some_and(|seen| seen != now) && natural.get_untracked() != 0.0 {
            // New entries: measured again before they are shown.
            natural.set(0.0);
        }
        if visible.get() {
            // Once its entries and its showing are on the page (before it
            // is painted).
            later::next_frame(state, move |_| measure(tip_ref, natural, true));
        }
        now
    });
    // Until measured, its place is a guess that may cover its own line:
    // it stays unseen and lets the pointer through, so a click (or a tap,
    // whose mouse events follow at once) on the underline reaches it.
    let measured = move || natural.get() > 0.0;
    // It takes the pointer only to be reached: with a docs link, or taller
    // than its room (to scroll). Otherwise clicks go through to the text
    // under it, as when there is no tip.
    let interactive = move || {
        if !measured() {
            return false;
        }
        let docs = content.with(|content| {
            content
                .as_ref()
                .is_some_and(|content| content.entries.iter().any(|entry| entry.docs_url.is_some()))
        });
        let scrolls =
            placement.with(|placed| placed.is_some_and(|placed| natural.get() > placed.max_height));
        docs || scrolls
    };
    view! {
        <div
            class="config-editor__problem-tip"
            class:config-editor__problem-tip--interactive=interactive
            class:config-editor__problem-tip--unmeasured=move || !measured()
            // A dialog while it has the focus (a second Ctrl+K Ctrl+I, or
            // Tab to its links), to scroll it; a tooltip otherwise.
            role=move || if focused.get() { "dialog" } else { "tooltip" }
            aria-label=move || focused.get().then(|| state.config().settings.strings.with(|strings| strings.problem_tip_label.clone()))
            tabindex="-1"
            node_ref=tip_ref
            on:keydown=move |event| tip_key(state, &event)
            on:focusin=move |_| focused.set(true)
            on:focusout=move |event| leave_tip_focus(state, &event)
            hidden=move || placement.with(Option::is_none)
            style=cssom(move || placement.get().map(Placement::style))
            on:mouseenter=move |_| state.problems().tip_entered.set_value(true)
            on:mouseleave=move |_| leave_tip(state)
            // Scrolling it or clicking a link keeps the caret.
            on:mousedown=move |event| event.prevent_default()
        >
            {move || content.get().map(|content| entries_view(state, content.entries))}
        </div>
    }
}

/// Measures the tip's full height into `natural`; then (once placed by it,
/// the frame after when it changed, `again`) scrolls the problem F8 went to
/// into view.
fn measure(tip_ref: NodeRef<Div>, natural: RwSignal<f64>, again: bool) {
    let Some(tip) = tip_ref.get_untracked() else {
        return;
    };
    if tip.offset_height() == 0 {
        return;
    }
    // The borders, besides the scrolled content.
    let measured = f64::from(tip.scroll_height() + tip.offset_height() - tip.client_height());
    if natural.try_get_untracked() != Some(measured) && again {
        natural.set(measured);
        later::next_frame(natural, move |natural| measure(tip_ref, natural, false));
        return;
    }
    if let Ok(Some(current)) = tip.query_selector(".config-editor__problem-tip-entry--current")
        && let Ok(current) = current.dyn_into::<web_sys::HtmlElement>()
    {
        let top = current.offset_top();
        let bottom = top + current.offset_height();
        if top < tip.scroll_top() || bottom > tip.scroll_top() + tip.client_height() {
            tip.set_scroll_top(top - 4);
        }
    }
}

/// A key with the focus in the tip: `Enter` on it or `Alt+Enter` opens
/// the docs of the problem at the caret; the scroll keys scroll it.
fn tip_key(state: EditorState, event: &web_sys::KeyboardEvent) {
    let Some(tip) = state.problems().tip_ref.get_untracked() else {
        return;
    };
    if super::super::super::hover_focus::opens_docs_from(state, &tip, event) {
        return;
    }
    super::super::super::hover_focus::card_keys(&tip, event);
}

/// The focus left the tip, or moved inside it.
fn leave_tip_focus(state: EditorState, event: &web_sys::FocusEvent) {
    let inside = state.problems().tip_ref.get_untracked().is_some_and(|tip| {
        event
            .related_target()
            .and_then(|to| to.dyn_into::<web_sys::Node>().ok())
            .is_some_and(|to| tip.contains(Some(&to)))
    });
    if !inside {
        state.problems().tip_focused.set(false);
    }
}

/// The pointer left the tip: it closes a moment later, as when it leaves
/// an underline.
fn leave_tip(state: EditorState) {
    state.problems().tip_entered.set_value(false);
    state.leave_problem();
}

/// What the tip shows now: the problems at the hovered place, else at
/// where F8 (or a tap) went; none while the hover card lists them.
fn tip_content(state: EditorState) -> Option<TipContent> {
    let (line, column) = state.tip_place()?;
    let entries = problems_at(state, (line, column));
    if entries.is_empty() {
        return None;
    }
    let column = state.doc().text.with_untracked(|text| {
        let line_text = text.split('\n').nth(line - 1).unwrap_or("");
        state
            .problems()
            .diagnostics
            .with_untracked(|found| tip_column(found, line, column, line_text))
    });
    Some(TipContent {
        anchor: Anchor { line, column },
        entries,
    })
}

/// Where the tip goes: its `left` and `top`, and its size in the editor's
/// pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Placement {
    left: f64,
    top: f64,
    width: f64,
    max_height: f64,
    above: bool,
}

impl Placement {
    fn style(self) -> Declarations {
        Declarations::new()
            .set("left", format!("{:.1}px", self.left))
            .set("top", format!("{:.1}px", self.top))
            .set("max-width", format!("{:.1}px", self.width))
            .set("max-height", format!("{:.1}px", self.max_height))
    }
}

/// The visible box the tip stays in (the editor's text area, inside the
/// window), in the editor's pixels (see `PopupFrame`).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Area {
    top: f64,
    bottom: f64,
    left: f64,
    right: f64,
}

/// The placement for the tip pointing at `anchor` (tracks the scroll and
/// the layout), or `None` while its line is scrolled out of view.
fn placement(state: EditorState, anchor: Anchor, natural: f64) -> Option<Placement> {
    let scroll_x = state.view().scroll_x.get();
    let scroll_y = state.view().scroll_y.get();
    state.view().layout_revision.track();
    let row = state
        .view()
        .projection
        .with(|projection| projection.row_of_line(anchor.line - 1));
    let element = state.view().textarea_ref.get()?;
    // In the editor's own pixels (its container may scale or move it).
    let frame = PopupFrame::measure(state);
    let rect = frame.bounds(&element.get_bounding_client_rect());
    let metrics = read_editor_metrics(&element);
    let area = Area {
        top: rect.top.max(0.0),
        bottom: rect.bottom.min(frame.viewport_height),
        left: rect.left.max(0.0),
        right: rect.right.min(frame.viewport_width),
    };
    let line_top = rect.top + metrics.padding_top + row as f64 * metrics.line_height - scroll_y;
    if line_top + metrics.line_height <= area.top || line_top >= area.bottom {
        return None;
    }
    let x = rect.left + metrics.padding_left + anchor.column * metrics.char_width - scroll_x;
    let placed = place(
        area,
        (0.0, frame.viewport_height),
        line_top,
        metrics.line_height,
        x,
        natural,
    );
    Some(Placement {
        left: frame.left(placed.left),
        top: frame.top(placed.top),
        ..placed
    })
}

/// The tip for a line at `line_top` (`line_height` tall), starting at `x`,
/// `natural` pixels tall in full: above the line when it fits there, else
/// below when it fits there, else on the roomier side, at most as tall as
/// that side's room (it scrolls); shifted left to stay inside `area`. An
/// editor too short for it on either side lends it the window's height
/// (`window`: its top and bottom) when it fits there whole; else it stays
/// inside the editor and scrolls.
fn place(
    area: Area,
    window: (f64, f64),
    line_top: f64,
    line_height: f64,
    x: f64,
    natural: f64,
) -> Placement {
    let width = TIP_MAX_WIDTH
        .min(area.right - area.left - 2.0 * TIP_MARGIN)
        .max(TIP_MIN_WIDTH);
    let left = x
        .min(area.right - TIP_MARGIN - width)
        .max(area.left + TIP_MARGIN);
    let rooms = |top: f64, bottom: f64| {
        (
            line_top - TIP_GAP - (top + TIP_MARGIN),
            bottom - TIP_MARGIN - (line_top + line_height + TIP_GAP),
        )
    };
    let (mut room_above, mut room_below) = rooms(area.top, area.bottom);
    let in_window = rooms(window.0, window.1);
    if natural > room_above.max(room_below) && natural <= in_window.0.max(in_window.1) {
        (room_above, room_below) = in_window;
    }
    let above = natural <= room_above || (natural > room_below && room_above > room_below);
    let room = if above { room_above } else { room_below }.max(TIP_MIN_HEIGHT);
    let top = if above {
        line_top - TIP_GAP - natural.min(room)
    } else {
        line_top + line_height + TIP_GAP
    };
    Placement {
        left,
        top,
        width,
        max_height: room,
        above,
    }
}

#[cfg(test)]
mod tests {
    use super::{Area, Placement, place};

    const AREA: Area = Area {
        top: 100.0,
        bottom: 500.0,
        left: 0.0,
        right: 800.0,
    };

    fn fits(placed: Placement, natural: f64) {
        let height = natural.min(placed.max_height);
        assert!(placed.top >= AREA.top, "{placed:?}");
        assert!(placed.top + height <= AREA.bottom, "{placed:?}");
    }

    /// QA 19: a tall tip near the bottom went off the top, its first
    /// messages out of reach. It goes where it fits, else on the roomier
    /// side no taller than the room there (it scrolls).
    #[test]
    fn the_tip_stays_inside_the_editor_above_below_or_scrolled() {
        // A short tip above its line, as before.
        let placed = place(AREA, (AREA.top, AREA.bottom), 300.0, 20.0, 50.0, 60.0);
        assert!(placed.above);
        assert_eq!(placed.top, 300.0 - 4.0 - 60.0);
        fits(placed, 60.0);
        // Near the top: below.
        let placed = place(AREA, (AREA.top, AREA.bottom), 110.0, 20.0, 50.0, 60.0);
        assert!(!placed.above);
        fits(placed, 60.0);
        // Taller than either side: on the roomier one, capped to its room.
        for line_top in [130.0, 300.0, 470.0] {
            let placed = place(AREA, (AREA.top, AREA.bottom), line_top, 20.0, 50.0, 2000.0);
            fits(placed, 2000.0);
            assert!(placed.max_height < 2000.0);
            assert_eq!(placed.above, line_top > 300.0 - 10.0, "{line_top}");
        }
        // Near the right edge: shifted left to stay inside.
        let placed = place(AREA, (AREA.top, AREA.bottom), 300.0, 20.0, 780.0, 60.0);
        assert!(placed.left + placed.width <= AREA.right);
    }

    /// QA 20: in an editor too short for a one-message tip on either side
    /// of its line, the tip used the roomier side and scrolled. It takes
    /// the window's room instead, whole.
    #[test]
    fn a_short_editor_lends_the_tip_the_windows_room() {
        let short = Area {
            top: 100.0,
            bottom: 200.0,
            ..AREA
        };
        for line_top in [110.0, 140.0, 175.0] {
            let placed = place(short, (0.0, 600.0), line_top, 20.0, 50.0, 68.0);
            assert!(placed.max_height >= 68.0, "{line_top}: {placed:?}");
            assert!(
                placed.top >= 0.0 && placed.top + 68.0 <= 600.0,
                "{placed:?}"
            );
            // Beside its line, never over it.
            assert!(
                placed.top + 68.0 <= line_top || placed.top >= line_top + 20.0,
                "{placed:?}"
            );
        }
        // An editor with room keeps the tip inside.
        let placed = place(AREA, (0.0, 600.0), 300.0, 20.0, 50.0, 68.0);
        assert!(placed.top >= AREA.top);
        // Too tall for the window too: inside the editor, scrolled.
        let placed = place(short, (0.0, 600.0), 140.0, 20.0, 50.0, 2000.0);
        assert!(
            placed.top >= short.top && placed.max_height < 2000.0,
            "{placed:?}"
        );
    }
}
