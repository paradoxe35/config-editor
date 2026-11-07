//! Folding's controls: the gutter chevrons and the `…` chip after a folded
//! line. They are drawn as one layer each over the rows in view, not inside
//! every row: rows are many, and each reactive node a row holds costs
//! memory. Both keep the focus in the text.

use leptos::prelude::*;
use wasm_bindgen::JsCast;

use super::super::completion::visual_column;
use super::super::diff::DiffKind;
use super::super::folding::texts;
use super::super::state::EditorState;
use crate::reactive::KeyedList;

/// A line in view that starts a fold range.
#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) struct FoldItem {
    /// Its textarea row.
    row: usize,
    /// Its document line (0-based).
    line: usize,
    /// When closed, the last line it hides.
    closed_end: Option<usize>,
    /// Closed and hiding changed lines.
    hidden_change: bool,
    /// Where the chip goes: the line's width in rendered columns, in
    /// hundredths (kept whole so the item can be a key).
    chip_column: usize,
}

/// The fold ranges starting on the lines in view.
pub(crate) type FoldItems = Memo<Vec<FoldItem>>;

/// The [`FoldItems`] of `state`, kept in step with the rows in view.
fn fold_items(state: EditorState) -> FoldItems {
    Memo::new(move |_| {
        let rows = state.view.visible_rows.get();
        state.fold.visible_lines.with(|lines| {
            state.fold.fold_ranges.with(|ranges| {
                state.fold.folded.with(|closed| {
                    lines
                        .iter()
                        .zip(rows)
                        .filter_map(|(&line, row)| {
                            ranges
                                .binary_search_by_key(&line, |range| range.start)
                                .ok()?;
                            let closed_end = closed
                                .binary_search_by_key(&line, |fold| fold.start)
                                .ok()
                                .map(|at| closed[at].end);
                            let (hidden_change, chip_column) = match closed_end {
                                Some(end) => closed_details(state, line, end),
                                None => (false, 0),
                            };
                            Some(FoldItem {
                                row,
                                line,
                                closed_end,
                                hidden_change,
                                chip_column,
                            })
                        })
                        .collect()
                })
            })
        })
    })
}

/// Whether the fold `line..=end` hides changes, and its line's width.
fn closed_details(state: EditorState, line: usize, end: usize) -> (bool, usize) {
    let changed = state.changes.diff.with(|diff| {
        let hidden = line + 1..=end;
        hidden.clone().any(|index| {
            diff.current
                .get(index)
                .is_some_and(|change| *change != DiffKind::Unchanged)
                || diff.deleted_before.get(index).copied().unwrap_or(false)
        })
    });
    let width = state.view.lines.with(|lines| {
        lines
            .get(line)
            .map_or(0.0, |rendered| visual_column(&rendered.text))
    });
    (changed, (width * 100.0).round() as usize)
}

/// The chevrons beside the lines in view that start a fold: shown while
/// the gutter is hovered, and always on a closed fold.
#[component]
fn FoldGutter(state: EditorState, folds: FoldItems) -> impl IntoView {
    view! {
        <KeyedList
            each=move || folds.get()
            key=|item| item.clone()
            children=move |item| view! { <FoldToggle state=state item=item /> }
        />
    }
}

/// Mounts the chevron and chip layers into the editor once it is on the
/// page, each as a view of its own: built from an effect, they add nothing
/// to the depth of the editor's own build, which a host's debug build
/// already runs close to the stack's limit.
pub(crate) fn mount_fold_layers(state: EditorState) {
    let folds = fold_items(state);
    let layers: StoredValue<Vec<Box<dyn std::any::Any>>, LocalStorage> =
        StoredValue::new_local(Vec::new());
    Effect::new(move |_| {
        let Some(root) = state.view.root_ref.get() else {
            return;
        };
        let find = |selector: &str| {
            root.query_selector(selector)
                .ok()
                .flatten()
                .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok())
        };
        let (Some(gutter), Some(text)) = (
            find(".config-editor__gutter-content"),
            find(".config-editor__input-wrap"),
        ) else {
            return;
        };
        let gutter = leptos::mount::mount_to(gutter, move || {
            view! { <FoldGutter state=state folds=folds /> }
        });
        let chips = leptos::mount::mount_to(text, move || {
            view! { <FoldChips state=state folds=folds /> }
        });
        layers.set_value(vec![Box::new(gutter), Box::new(chips)]);
    });
    on_cleanup(move || {
        let _ = layers.try_update_value(Vec::clear);
    });
}

#[component]
fn FoldToggle(state: EditorState, item: FoldItem) -> impl IntoView {
    let line = item.line;
    let closed = item.closed_end.is_some();
    let label = match item.closed_end {
        Some(end) => texts::unfold_label(line + 1, end + 1),
        None => texts::fold_label(line + 1),
    };
    let toggle = move |_: web_sys::MouseEvent| state.toggle_fold(line);
    let top = format!(
        "top: calc({} * var(--config-editor-line-height));",
        item.row
    );
    view! {
        <button
            type="button"
            class="config-editor__fold-toggle"
            class:config-editor__fold-toggle-closed=closed
            data-line=line + 1
            style=top.clone()
            tabindex="-1"
            aria-label=label.clone()
            aria-expanded=(!closed).to_string()
            title=label
            on:mousedown=keep_focus
            on:click=toggle
        ></button>
        {item.hidden_change.then(|| view! {
            <span class="config-editor__fold-changes" data-line=line + 1 style=top title=texts::HIDDEN_CHANGES></span>
        })}
    }
}

/// The `…` chips after the closed folds in view; a click opens the fold.
#[component]
fn FoldChips(state: EditorState, folds: FoldItems) -> impl IntoView {
    view! {
        <div
            class="config-editor__fold-chips"
            style:transform=move || format!("translate({}px, {}px)", -state.view.scroll_x.get(), -state.view.scroll_y.get())
        >
            <KeyedList
                each=move || folds.get().into_iter().filter(|item| item.closed_end.is_some())
                key=|item| item.clone()
                children=move |item| view! { <FoldChip state=state item=item /> }
            />
        </div>
    }
}

#[component]
fn FoldChip(state: EditorState, item: FoldItem) -> impl IntoView {
    let line = item.line;
    let label = texts::unfold_label(line + 1, item.closed_end.unwrap_or(line) + 1);
    let open = move |_: web_sys::MouseEvent| {
        state.unfold_line(line);
    };
    view! {
        <button
            type="button"
            class="config-editor__fold-chip"
            data-line=line + 1
            style=format!(
                "left: {}ch; top: calc(var(--config-editor-padding-top) + {} * var(--config-editor-line-height));",
                item.chip_column as f64 / 100.0 + 1.0,
                item.row,
            )
            tabindex="-1"
            aria-label=label.clone()
            title=label
            on:mousedown=keep_focus
            on:click=open
        >
            {texts::CHIP}
        </button>
    }
}

/// A press on a fold control leaves the focus and caret in the text.
fn keep_focus(event: web_sys::MouseEvent) {
    event.prevent_default();
}
