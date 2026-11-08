//! The change preview a diff marker opens: the lines before and now, and
//! "Revert this change".

use leptos::prelude::*;
use web_sys::KeyboardEvent;

use super::super::super::positioning::PopupFrame;
use super::super::super::state::EditorState;
use super::super::hunk_for_peek;
use super::super::keys::go_to_change;
use super::super::line_diff::{DiffPeek, peek_popup_style, peek_survives_edit};
use super::tab_order::tab_past_edge;
use crate::EditorStrings;
use crate::cssom::{Declarations, cssom};
use crate::reactive::Relay;
use crate::strings::fill;
use std::ops::Range;

#[component]
pub(in crate::editor) fn DiffPeekPopup(state: EditorState) -> impl IntoView {
    close_when_edited(state);
    // Each preview is a view of its own, so it reads what it follows
    // through relays (see `reactive`).
    let follows = PeekFollows {
        style: Relay::new(move || peek_style(state)),
        read_only: Relay::new(move || state.config().read_only.get()),
    };
    move || {
        state
            .changes()
            .peek
            .get()
            .map(|peek| view! { <Peek state=state follows=follows peek=peek /> })
    }
}

/// What an open preview follows.
#[derive(Clone, Copy)]
struct PeekFollows {
    /// Its position (see [`peek_style`]).
    style: Relay<Declarations>,
    read_only: Relay<bool>,
}

/// Where the open preview goes, under its line; re-measured as the text
/// scrolls or the layout changes.
fn peek_style(state: EditorState) -> Declarations {
    let Some((line, old_count, new_count)) = state.changes().peek.with(|peek| {
        peek.as_ref()
            .map(|peek| (peek.line, peek.before.len(), peek.after.len()))
    }) else {
        return Declarations::new();
    };
    state.view().layout_revision.track();
    state.view().scroll_y.track();
    match (state.view().textarea_ref.get(), state.view().root_ref.get()) {
        (Some(editor), Some(root)) => {
            // The row the line is on, folded lines skipped.
            let row = state
                .view()
                .projection
                .with(|projection| projection.row_of_line(line.saturating_sub(1)));
            peek_popup_style(
                &PopupFrame::measure(state),
                &editor,
                &root,
                row + 1,
                old_count,
                new_count,
            )
        }
        _ => Declarations::new().set("visibility", "hidden"),
    }
}

/// Close the preview when an edit touches its anchor.
fn close_when_edited(state: EditorState) {
    Effect::new(move |_| {
        // The preview is read untracked so closing it cannot re-trigger this.
        state.changes().diff.track();
        state.doc().text.track();
        let survives = state.changes().peek.with_untracked(|peek| {
            peek.as_ref().is_none_or(|peek| {
                let current: Vec<String> = state
                    .doc()
                    .text
                    .with_untracked(|text| text.split('\n').map(str::to_string).collect());
                state
                    .changes()
                    .diff
                    .with_untracked(|line_diff| peek_survives_edit(peek, &current, line_diff))
            })
        });
        if !survives {
            state.changes().peek.set(None);
        }
    });
}

/// "Revert this change (lines 3–5)": the lines of the change a revert
/// restores, which may be more than the one line the preview shows.
fn revert_label(strings: &EditorStrings, span: &Option<(Range<usize>, usize)>) -> String {
    match span {
        Some((lines, _)) if lines.len() > 1 => fill(
            &strings.revert_lines_title,
            &[("first", &(lines.start + 1)), ("last", &lines.end)],
        ),
        Some((lines, removed)) if lines.len() == 1 && *removed > 0 => fill(
            &strings.revert_line_and_removed_title,
            &[("line", &(lines.start + 1))],
        ),
        _ => strings.revert_title.clone(),
    }
}

/// Closes the preview; when it had the focus, the text takes it back.
fn close(state: EditorState) {
    state.changes().peek.set(None);
    if let Some(element) = state.textarea() {
        let (top, left) = (element.scroll_top(), element.scroll_left());
        let _ = element.focus();
        element.set_scroll_top(top);
        element.set_scroll_left(left);
    }
}

/// Keys inside the preview: `Escape` closes it, `R` reverts the change, and
/// the change keys move on to the next one.
fn on_peek_keydown(state: EditorState, peek: &DiffPeek, event: &KeyboardEvent) {
    let plain = !event.ctrl_key() && !event.meta_key() && !event.alt_key();
    match event.key().as_str() {
        "Escape" => {
            event.prevent_default();
            close(state);
        }
        "r" | "R" if plain => {
            event.prevent_default();
            state.revert_change(peek);
        }
        "Tab" if plain => tab_past_edge(state, event),
        _ => {
            go_to_change(state, event);
        }
    }
}

#[component]
fn Peek(state: EditorState, follows: PeekFollows, peek: DiffPeek) -> impl IntoView {
    let DiffPeek {
        line,
        before,
        after,
        ..
    } = peek.clone();
    let read_only = follows.read_only.read();
    let revert_ref = NodeRef::<leptos::html::Button>::new();
    let span = state.changes().diff.with_untracked(|line_diff| {
        hunk_for_peek(line_diff, &peek).map(|hunk| (hunk.current.clone(), hunk.removed))
    });
    let strings = state.strings_signal();
    let label = Signal::derive(move || strings.with(|strings| revert_label(strings, &span)));
    let titled = {
        let peek = peek.clone();
        move |pick: fn(&EditorStrings) -> &String| {
            let peek = peek.clone();
            move || {
                strings.with(|strings| {
                    fill(
                        pick(strings),
                        &[("title", &peek.title(strings)), ("line", &line)],
                    )
                })
            }
        }
    };
    let heading = titled(|strings| &strings.preview_heading);
    let heading_title = heading.clone();
    let dialog_label = titled(|strings| &strings.preview_label);
    // Opened from the keyboard: the revert button takes the focus.
    Effect::new(move |_| {
        if let Some(button) = revert_ref.get()
            && state.changes().focus_preview.get_value()
        {
            state.changes().focus_preview.set_value(false);
            let _ = crate::focus::in_place(&button);
        }
    });
    let style = follows.style.read();
    let keydown = {
        let peek = peek.clone();
        move |event: KeyboardEvent| on_peek_keydown(state, &peek, &event)
    };
    let revert = move |_| state.revert_change(&peek);
    let close_click = move |_| close(state);
    view! {
        <section
            class="config-editor__diff-peek"
            role="dialog"
            aria-label=dialog_label
            style=cssom(style)
            on:keydown=keydown
        >
            <header class="config-editor__diff-peek-header">
                <strong class="config-editor__diff-peek-title" title=heading_title>{heading}</strong>
                <span class="config-editor__diff-peek-actions">
                    {move || (!read_only.get()).then(|| view! {
                        <button
                            type="button"
                            class="config-editor__diff-peek-revert"
                            aria-label=label
                            title=label
                            node_ref=revert_ref
                            on:click=revert.clone()
                        >{state.text(|strings| &strings.revert)}</button>
                    })}
                    <button
                        type="button"
                        class="config-editor__diff-peek-close"
                        aria-label=state.text(|strings| &strings.close_preview)
                        on:click=close_click
                    >"×"</button>
                </span>
            </header>
            <div class="config-editor__diff-peek-columns">
                {(!before.is_empty()).then(|| view! {
                    <section class="config-editor__diff-peek-side">
                        <h3>{state.text(|strings| &strings.before)}</h3>
                        {peek_lines(before, "−", "config-editor__diff-peek-removed")}
                    </section>
                })}
                {(!after.is_empty()).then(|| view! {
                    <section class="config-editor__diff-peek-side">
                        <h3>{state.text(|strings| &strings.current)}</h3>
                        {peek_lines(after, "+", "config-editor__diff-peek-current")}
                    </section>
                })}
            </div>
        </section>
    }
}

fn peek_lines(lines: Vec<String>, sign: &'static str, class: &'static str) -> impl IntoView {
    lines
        .into_iter()
        .map(|line| {
            view! {
                <div class=format!("config-editor__diff-peek-line {class}")>
                    <span aria-hidden="true">{sign}</span>
                    <code>{if line.is_empty() { " ".to_string() } else { line }}</code>
                </div>
            }
        })
        .collect_view()
}
