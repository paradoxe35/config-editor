//! The sticky lines on screen: their numbers over the gutter, their text
//! over the text, each line placed by its depth (the layers set where each
//! depth is, so a line never re-renders as it is pushed up). Both layers
//! are mounted once; the lines in them come and go as the text scrolls,
//! reading only their row's own state (see `line_rows`).

use leptos::prelude::*;

use super::super::super::components::row_state_for;
use super::super::super::highlight;
use super::super::super::line_rows::{LineRows, RowUiState};
use super::super::super::scroll_through::{
    Swipe, gutter_touch_end, gutter_touch_move, gutter_touch_start, gutter_wheel,
};
use super::super::super::state::EditorState;
use super::Sticky;
use crate::cssom::{Declarations, cssom};
use crate::reactive::KeyedList;
use crate::settings::StickyScrollLines;

/// Where each depth's line is, and how far down the lines reach, as the
/// layers' custom properties.
fn places(sticky: Sticky) -> Declarations {
    sticky.layout.with(|layout| {
        let mut places = Declarations::new().set(
            "--config-editor-sticky-height",
            format!("{:.2}px", layout.height),
        );
        for line in &layout.lines {
            places = places.set(
                format!("--config-editor-sticky-top-{}", line.depth),
                format!("{:.2}px", line.top),
            );
        }
        places
    })
}

/// A line's own place: at its depth's top, an outer line over an inner one
/// pushed up under it.
fn place(depth: usize) -> Declarations {
    let above = usize::from(StickyScrollLines::MAX).saturating_sub(depth);
    Declarations::new()
        .set("top", format!("var(--config-editor-sticky-top-{depth})"))
        .set("z-index", above.to_string())
}

/// A press on a sticky line leaves the focus where it is and selects
/// nothing (the click then moves the caret), and is no press on the
/// gutter.
fn press(event: web_sys::MouseEvent) {
    event.prevent_default();
    event.stop_propagation();
}

/// The numbers of the sticky lines, over the gutter.
#[component]
pub(in crate::editor) fn StickyNumbers(state: EditorState, sticky: Sticky) -> impl IntoView {
    view! {
        <div
            class="config-editor__sticky-numbers"
            aria-hidden="true"
            style=cssom(move || places(sticky))
        >
            <KeyedList
                each=move || sticky.items()
                key=|item| *item
                children=move |(depth, line)| view! { <StickyNumber state=state sticky=sticky depth=depth line=line /> }
            />
        </div>
    }
}

#[component]
fn StickyNumber(state: EditorState, sticky: Sticky, depth: usize, line: usize) -> impl IntoView {
    let jump = move |_: web_sys::MouseEvent| sticky.jump(state, line);
    view! {
        <div
            class="config-editor__sticky-number"
            data-line=line + 1
            style=cssom(place(depth))
            on:mousedown=press
            on:click=jump
        >
            {line + 1}
        </div>
    }
}

/// The layer's frame: [`places`], the text's sideways scroll, and the text
/// area's width (its scroll bar stays uncovered).
fn frame(state: EditorState, sticky: Sticky) -> Declarations {
    let view = state.view();
    view.layout_revision.track();
    let width = view
        .textarea_ref
        .get()
        .map_or(0, |element| element.client_width());
    places(sticky)
        .set(
            "--config-editor-sticky-x",
            format!("{:.2}px", -view.scroll_x.get()),
        )
        .set("width", format!("{width}px"))
}

/// The text of the sticky lines, over the text. The wheel and a finger
/// over it scroll the text, as over the gutter.
#[component]
pub(in crate::editor) fn StickyText(
    state: EditorState,
    sticky: Sticky,
    rows: LineRows,
) -> impl IntoView {
    let swipe = StoredValue::new(Swipe::default());
    let wheel = move |event: web_sys::WheelEvent| gutter_wheel(state, &event);
    let touch_start = move |event: web_sys::TouchEvent| gutter_touch_start(swipe, &event);
    let touch_move = move |event: web_sys::TouchEvent| gutter_touch_move(state, swipe, &event);
    let touch_end = move |_: web_sys::TouchEvent| gutter_touch_end(swipe);
    view! {
        <div
            class="config-editor__sticky"
            aria-hidden="true"
            style=cssom(move || frame(state, sticky))
            on:wheel=wheel
            on:touchstart=touch_start
            on:touchmove=touch_move
            on:touchend=touch_end
            on:touchcancel=touch_end
        >
            <KeyedList
                each=move || sticky.items()
                key=|item| *item
                children=move |(depth, line)| view! { <StickyCode state=state sticky=sticky rows=rows depth=depth line=line /> }
            />
        </div>
    }
}

/// One sticky line's text, drawn as the code layer draws its row.
#[component]
fn StickyCode(
    state: EditorState,
    sticky: Sticky,
    rows: LineRows,
    depth: usize,
    line: usize,
) -> impl IntoView {
    let row = row_state_for(state, rows, line);
    let jump = move |_: web_sys::MouseEvent| sticky.jump(state, line);
    view! {
        <div
            class="config-editor__sticky-line"
            data-line=line + 1
            style=cssom(place(depth))
            on:mousedown=press
            on:click=jump
        >
            <span class="config-editor__sticky-code">{move || row.with(render)}</span>
        </div>
    }
}

/// A row's text with its colors and marks (muted read-only values, secret
/// placeholders), as the code layer draws it.
fn render(row: &RowUiState) -> AnyView {
    let marks = row
        .marks
        .as_ref()
        .map_or_else(Vec::new, |marks| marks.classes());
    let tokens = row.highlight.tokens(&row.text);
    highlight::render(&row.text, &tokens, &marks)
}
