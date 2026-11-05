use leptos::{ev, leptos_dom::helpers::window_event_listener, prelude::*};
use std::sync::Arc;
use wasm_bindgen::JsCast;

mod language;
mod chrome;
pub use chrome::{ConfigEditorFooter, ConfigEditorToolbar};
mod languages;
pub use languages::Language;
mod theme;
pub use theme::EditorTheme;
mod history;
use history::EditHistory;
mod diff;
use diff::{DiffKind, DiffPeek, LineDiff, diff_lines, peek_popup_style, peek_survives_edit};
mod keyboard;
use keyboard::{delete_empty_pair, paired_edit, replace_selection, skip_closing_pair};
mod diagnostics;
use diagnostics::{DIAGNOSTICS_DEBOUNCE_MS, build_schema, validate};
mod completion;
use completion::{
    KeyHover, accept_reopens_suggestions, apply_completion, completion_popup_style, completions,
    key_hover, scroll_completion_into_view, should_accept_with_keyboard, should_auto_trigger,
};
mod commands;
use commands::{cycle_offset, indent_lines, indent_unit, line_at_cursor, offset_at};
mod line_rows;
use line_rows::{LineRows, RenderedLine};
mod metrics;
mod positioning;
use positioning::{hover_card_style, pointer_text_position, schedule_hover, set_caret_visible};

/// Files edited on Windows commonly use CRLF. The textarea works in LF, so normalize
/// at the boundary to keep diagnostics, completions, and diffs line-based.
fn normalize_newlines(text: String) -> String {
    if text.contains('\r') {
        text.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        text
    }
}

#[derive(Clone, PartialEq)]
struct HoverCard {
    info: KeyHover,
    line: usize,
    style: String,
}

pub const PRESET_CSS: &str = concat!(include_str!("preset.css"), include_str!("themes.css"));

/// Single funnel for programmatic edits: record undo, sync the textarea,
/// update reactive state, and notify the host. All keydown-driven mutations
/// (pairing, Enter, Tab, comment toggle, completion accept, undo/redo) go
/// through here so ordering can never drift between call sites.
#[allow(clippy::too_many_arguments)]
fn commit_edit(
    updated: String,
    new_cursor: usize,
    selection_end: Option<usize>,
    before: String,
    before_cursor: usize,
    source: RwSignal<String>,
    cursor: RwSignal<usize>,
    current_line: RwSignal<usize>,
    history: RwSignal<EditHistory>,
    on_change: Option<Callback<String>>,
    editor: Option<web_sys::HtmlTextAreaElement>,
    scroll_y: RwSignal<f64>,
    ensure_visible: bool,
) {
    history.update(|history| history.record_action(before, before_cursor));
    if let Some(editor) = editor.as_ref() {
        editor.set_value(&updated);
    }
    source.set(updated);
    if let Some(on_change) = on_change.as_ref() {
        on_change.run(source.get_untracked());
    }
    let end = selection_end.unwrap_or(new_cursor);
    cursor.set(end);
    current_line.set(line_at_cursor(&source.get_untracked(), end));
    if ensure_visible {
        if let Some(editor) = editor {
            set_caret_visible(&editor, &source.get_untracked(), new_cursor, scroll_y);
        }
    } else if let Some(editor) = editor {
        let _ = editor.set_selection_range(end as u32, end as u32);
    }
}

#[component]
pub fn ConfigEditor(
    /// Document language. Selects the text layer (parsing, completion
    /// context, inserts, hover lookup, highlighting, newline/indent,
    /// comments); everything else stays shared. Defaults to YAML.
    #[prop(default = Language::Yaml, into)]
    language: Language,
    /// Seeds the editor and its diff baseline. Not a controlled value prop:
    /// for externally controlled content and live diffs, provide reactive
    /// `value` and `baseline` signals instead.
    #[prop(optional, into)]
    initial_value: Option<String>,
    /// Optional reactive final value. Changes synchronize into the editor;
    /// the editor sends each edit through `on_change`, so update `value`
    /// in that callback to keep the external source of truth in sync.
    #[prop(optional, into)]
    value: Option<Signal<String>>,
    /// Optional reactive baseline used to show line diffs. Updating it (for
    /// example after applying changes) clears the diff decorations.
    #[prop(optional, into)]
    baseline: Option<Signal<String>>,
    /// Optional JSON Schema string. Enables live schema diagnostics and
    /// schema-aware completions and hover documentation.
    #[prop(optional, into)]
    schema_json: Option<String>,
    /// Name shown when the built-in toolbar is enabled.
    #[prop(optional, into)]
    file_name: Option<String>,
    /// Extra class on the editor root for host styles.
    #[prop(optional, into)]
    class: Option<String>,
    /// Bundled [`EditorTheme`] preset. Accepts a reactive signal, so the host
    /// can switch themes without recreating the editor.
    #[prop(default = EditorTheme::VsCodeDark.into(), into)]
    theme: Signal<EditorTheme>,
    /// Callback with the updated document after every edit.
    #[prop(optional)]
    on_change: Option<Callback<String>>,
    /// Include the preset editor stylesheet. Set `false` with a custom
    /// `class` to style the semantic `config-editor__*` markup yourself.
    #[prop(default = true)]
    styled: bool,
    /// Show the built-in file/schema toolbar. Prefer the composable
    /// [`ConfigEditorToolbar`] for host-owned chrome.
    #[prop(default = false)]
    show_toolbar: bool,
    /// Show the built-in status footer. Prefer the composable
    /// [`ConfigEditorFooter`] for host-owned chrome.
    #[prop(default = false)]
    show_footer: bool,
    /// Show inline parse/schema diagnostics and hover messages.
    #[prop(default = true)]
    show_diagnostics: bool,
    /// Enable schema-backed completions (`Ctrl+Space`).
    #[prop(default = true)]
    enable_completions: bool,
    /// Show schema description, type, default, and examples when hovering a key.
    #[prop(default = true)]
    enable_hover_docs: bool,
    /// Open suggestions while typing and after accepting a key (values close
    /// the list). Set `false` to make them explicit with `Ctrl+Space`.
    #[prop(default = true)]
    auto_trigger_completions: bool,
    /// Enable syntax colors for the document language.
    #[prop(default = true)]
    enable_highlighting: bool,
    /// Pair quotes/brackets, wrap selections, skip closing pairs, and delete
    /// empty pairs on Backspace.
    #[prop(default = true)]
    enable_auto_closing: bool,
    /// Show line changes against `baseline`.
    #[prop(default = true)]
    show_diff: bool,
    /// View mode: disable typing, completions, and edits while keeping
    /// highlighting, diagnostics, diffs, and hover docs.
    #[prop(default = Signal::from(false), into)]
    read_only: Signal<bool>,
) -> impl IntoView {
    let label = language.label();
    let file_icon = language.file_icon();
    let initial_value = normalize_newlines(initial_value.unwrap_or_default());
    let (schema_bundle, schema_error) = match schema_json {
        Some(schema_json) => match build_schema(&schema_json) {
            Ok(schema) => (Some(Arc::new(schema)), None),
            Err(error) => (None, Some(error)),
        },
        None => (None, None),
    };
    let enable_completions =
        enable_completions && !read_only.get_untracked() && cfg!(feature = "completion");
    let enable_highlighting = enable_highlighting && cfg!(feature = "syntax-highlighting");
    let behavior = language.behavior();
    let source = RwSignal::new(
        value
            .as_ref()
            .map(Signal::get_untracked)
            .unwrap_or_else(|| initial_value.clone()),
    );
    let baseline = baseline.unwrap_or_else(|| Signal::derive(move || initial_value.clone()));
    let selected_diff_peek = RwSignal::new(None::<DiffPeek>);
    if let Some(value) = value {
        Effect::new(move |_| {
            let external_value = normalize_newlines(value.get());
            if source.get_untracked() != external_value {
                source.set(external_value);
            }
        });
    }
    let cursor = RwSignal::new(0usize);
    let current_line = RwSignal::new(1usize);
    let active_completion = RwSignal::new(0usize);
    let suggestions_requested = RwSignal::new(false);
    let explicit_request = RwSignal::new(false);
    // Tracks whether the user explicitly engaged with the open list (arrow
    // keys, hover, or Ctrl+Space). Enter/Tab accept only in that case; an
    // auto-triggered list with the highlight still sitting on index 0 must
    // never hijack Enter — the user asked for a newline, not the top match.
    let completion_navigated = RwSignal::new(false);
    let hover_card = RwSignal::new(None::<HoverCard>);
    let hover_pending = StoredValue::new(None::<(usize, String)>);
    let hover_token = StoredValue::new(0u64);
    let hide_hover = move || {
        hover_token.update_value(|token| *token += 1);
        hover_pending.set_value(None);
        if hover_card.with_untracked(Option::is_some) {
            hover_card.set(None);
        }
    };
    let history = RwSignal::new(EditHistory::default());
    let scroll_x = RwSignal::new(0.0);
    let scroll_y = RwSignal::new(0.0);
    let popup_layout_revision = RwSignal::new(0usize);
    let editor_ref: NodeRef<leptos::html::Textarea> = NodeRef::new();
    let root_ref: NodeRef<leptos::html::Div> = NodeRef::new();
    let completion_ref: NodeRef<leptos::html::Div> = NodeRef::new();
    let resize_listener = window_event_listener(ev::resize, move |_| {
        popup_layout_revision.update(|revision| *revision += 1);
    });
    let scroll_listener = window_event_listener(ev::scroll, move |_| {
        popup_layout_revision.update(|revision| *revision += 1);
    });
    on_cleanup(move || {
        resize_listener.remove();
        scroll_listener.remove();
    });
    let schema_for_diagnostics = schema_bundle.clone();
    let schema_error_for_diagnostics = schema_error.clone();
    // Validation runs against a debounced snapshot so intermediate states
    // while typing (an unclosed bracket, a half typed key) do not flash
    // squiggles on every keystroke. Hover, completions, and diffs keep
    // reading the live source.
    let diagnosed_text = RwSignal::new(source.get_untracked());
    let diagnostics_token = StoredValue::new(0u64);
    Effect::new(move |_| {
        let text = source.get();
        diagnostics_token.update_value(|token| *token += 1);
        let token = diagnostics_token.get_value();
        if let Some(window) = web_sys::window() {
            let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                wasm_bindgen::closure::Closure::once_into_js(move || {
                    if diagnostics_token.get_value() == token {
                        diagnosed_text.set(text);
                    }
                })
                .unchecked_ref(),
                DIAGNOSTICS_DEBOUNCE_MS,
            );
        } else {
            diagnosed_text.set(text);
        }
    });
    let diagnostics = Memo::new(move |_| {
        validate(
            &diagnosed_text.get(),
            schema_for_diagnostics.as_deref(),
            schema_error_for_diagnostics.as_deref(),
            show_diagnostics && cfg!(feature = "diagnostics"),
            behavior,
        )
    });
    let schema_for_completions = schema_bundle.clone();
    let schema_for_hover = schema_bundle.clone();
    let hover_docs = enable_hover_docs && schema_bundle.is_some();
    let suggestions = Memo::new(move |_| {
        if !suggestions_requested.get() {
            return Vec::new();
        }
        completions(
            &source.get(),
            cursor.get(),
            explicit_request.get(),
            schema_for_completions.as_deref(),
            enable_completions,
            behavior,
        )
    });
    let apply_selected = move || {
        let options = suggestions.get();
        if let Some(completion) = options.get(active_completion.get()) {
            let (updated, new_cursor) = apply_completion(&source.get_untracked(), completion);
            let before = source.get_untracked();
            let request_completions =
                accept_reopens_suggestions(completion, auto_trigger_completions);
            hide_hover();
            commit_edit(
                updated,
                new_cursor,
                None,
                before,
                cursor.get_untracked(),
                source,
                cursor,
                current_line,
                history,
                on_change,
                editor_ref.get(),
                scroll_y,
                true,
            );
            active_completion.set(0);
            explicit_request.set(true);
            completion_navigated.set(true);
            suggestions_requested.set(request_completions);
        }
    };
    // Moves the caret to a diagnostic (F8 cycling and the status button share
    // this). Diagnostics without a position (schema-level errors) have no
    // caret target, so callers must pick a positioned index.
    let go_to_diagnostic = move |index: usize| {
        let errors = diagnostics.get();
        let Some(diagnostic) = errors.get(index) else {
            return;
        };
        let (Some(line), Some(column)) = (diagnostic.line, diagnostic.column) else {
            return;
        };
        let text = source.get_untracked();
        let offset = offset_at(&text, line, column);
        if let Some(editor) = editor_ref.get() {
            let _ = editor.focus();
            let _ = editor.set_selection_range(offset as u32, offset as u32);
        }
        cursor.set(offset);
        current_line.set(line);
        suggestions_requested.set(false);
        hide_hover();
    };

    let default_file_name = language.default_file_name();
    let file_name = file_name.unwrap_or_else(|| default_file_name.to_string());
    let root_class = class.unwrap_or_default();
    let show_diagnostics = show_diagnostics && cfg!(feature = "diagnostics");
    let use_preset_style = styled && cfg!(feature = "preset-style");
    let has_schema = schema_bundle.is_some();
    let diff = Memo::new(move |_| {
        if show_diff {
            diff_lines(&baseline.get(), &source.get())
        } else {
            LineDiff::empty(source.get().split('\n').count())
        }
    });
    // One reactive snapshot of every rendered line: text plus the flags the
    // row views display. Rows never subscribe to this memo or `diff` directly
    // (see `line_rows`): the sync effect below fans the snapshot out to
    // per-row state signals, so a keystroke re-renders only rows whose visible
    // state actually changed, with no document-wide subscription churn.
    let lines = Memo::new(move |_| {
        let text = source.get();
        let errors = diagnostics.get();
        let line_diff = diff.get();
        text.split('\n')
            .enumerate()
            .map(|(index, line)| {
                let line_number = index + 1;
                RenderedLine {
                    text: line.to_string(),
                    has_error: errors
                        .iter()
                        .any(|error| error.line == Some(line_number)),
                    change: line_diff
                        .current
                        .get(index)
                        .copied()
                        .unwrap_or(DiffKind::Unchanged),
                }
            })
            .collect::<Vec<_>>()
    });
    let rows = LineRows::new();
    Effect::new(move |_| {
        rows.sync(&lines.get(), &diff.get(), current_line.get());
    });
    // A change preview survives typing elsewhere but dismisses when the edit
    // touches its anchor; see `peek_survives_edit`. Applying changes clears
    // the preview through the baseline, which recomputes the diff.
    Effect::new(move |_| {
        let line_diff = diff.get();
        let current: Vec<String> = source.get().split('\n').map(str::to_string).collect();
        selected_diff_peek.update(|peek| {
            if peek
                .as_ref()
                .is_some_and(|peek| !peek_survives_edit(peek, &current, &line_diff))
            {
                *peek = None;
            }
        });
    });

    view! {
        <div
            class=move || {
                format!(
                    "config-editor {}{}",
                    root_class,
                    if read_only.get() {
                        " config-editor__readonly"
                    } else {
                        ""
                    }
                )
            }
            data-theme=move || theme.get().css_family()
            data-color-theme=move || theme.get().as_str()
            node_ref=root_ref
            on:keydown=move |event: web_sys::KeyboardEvent| {
                if event.key() == "Escape" {
                    // Escape is the single dismissal action for transient editor UI,
                    // regardless of which editor child currently has focus.
                    event.prevent_default();
                    suggestions_requested.set(false);
                    explicit_request.set(false);
                    selected_diff_peek.set(None);
                    hide_hover();
                }
            }
        >
            {use_preset_style.then(|| view! { <style>{PRESET_CSS}</style> })}
            <section class="config-editor__frame" aria-label=format!("{label} editor")>
                {show_toolbar.then(|| view! {
                    <div class="config-editor__toolbar">
                        <div class="config-editor__file">
                            <span class="config-editor__file-icon">{file_icon}</span>
                            <span>{file_name.clone()}</span>
                        </div>
                        <div class="config-editor__toolbar-meta">
                            {has_schema.then(|| view! {
                                <span class="config-editor__schema-state">
                                    <span class="config-editor__schema-check">"✓"</span>
                                    "Schema connected"
                                </span>
                            })}
                            <span class="config-editor__language">{label}</span>
                        </div>
                    </div>
                })}

                    <div class="config-editor__surface">
                        <div
                            class="config-editor__gutter"
                        >
                            <div
                                class="config-editor__gutter-content"
                                style:transform=move || format!("translateY(-{}px)", scroll_y.get())
                            >
                            <For
                                each={move || 0..lines.with(Vec::len)}
                                key=|index| *index
                                children=move |index| {
                                    let line = index + 1;
                                    // This row's whole visible state lives in one
                                    // signal (see `line_rows`): the closures below
                                    // subscribe only to it, so a keystroke
                                    // re-renders exactly the rows whose text,
                                    // flags, peeks, or active state changed.
                                    let state = rows.state_for(
                                        index,
                                        &lines.get_untracked(),
                                        &diff.get_untracked(),
                                        current_line.get_untracked(),
                                    );
                                    let has_error = move || state.with(|row| row.has_error);
                                    let is_added = move || state.with(|row| row.change == DiffKind::Added);
                                    let is_modified = move || state.with(|row| row.change == DiffKind::Modified);
                                    let modified_peek = move || state.with(|row| row.modified_peek.clone());
                                    let deleted_peek = move || state.with(|row| row.deleted_peek.clone());
                                    let deleted_after_peek = move || state.with(|row| row.deleted_after_peek.clone());
                                    let is_active = move || state.with(|row| row.is_active);
                                    view! {
                                            <div
                                                class:config-editor__line-number-active=is_active
                                                class:config-editor__line-number-error=has_error
                                                class:config-editor__line-number-added=is_added
                                                class:config-editor__line-number-modified=is_modified
                                                on:click=move |_| {
                                                    if let Some(editor) = editor_ref.get() {
                                                        let text = source.get_untracked();
                                                        let offset = offset_at(&text, line, 1);
                                                        let _ = editor.focus();
                                                        cursor.set(offset);
                                                        current_line.set(line);
                                                        suggestions_requested.set(false);
                                                        hide_hover();
                                                        set_caret_visible(&editor, &text, offset, scroll_y);
                                                    }
                                                }
                                            >
                                                <span aria-hidden="true">{line}</span>
                                                {move || is_added().then(|| view! {
                                                    <span
                                                        class="config-editor__diff-marker config-editor__diff-marker-added"
                                                        aria-hidden="true"
                                                    ></span>
                                                })}
                                                {move || modified_peek().map(|peek| {
                                                    let peek_for_click = peek.clone();
                                                    view! {
                                                        <button
                                                            type="button"
                                                            class="config-editor__diff-marker config-editor__diff-marker-modified config-editor__diff-change-trigger"
                                                            aria-label=format!("Show before and after for modified line {line}")
                                                            title="Show before and after"
                                                            on:click=move |_| {
                                                                selected_diff_peek.update(|selected| {
                                                                    *selected = if selected.as_ref() == Some(&peek_for_click) {
                                                                        None
                                                                    } else {
                                                                        Some(peek_for_click.clone())
                                                                    };
                                                                });
                                                            }
                                                            on:keydown=move |event: web_sys::KeyboardEvent| {
                                                                if event.key() == "Escape" {
                                                                    selected_diff_peek.set(None);
                                                                }
                                                            }
                                                        ></button>
                                                    }
                                                })}
                                                {move || deleted_peek().map(|peek| {
                                                    let peek_for_click = peek.clone();
                                                    view! {
                                                        <button
                                                            type="button"
                                                            class="config-editor__diff-deleted"
                                                            aria-label=format!("Show removed lines before line {line}")
                                                            title="Show removed lines"
                                                            on:click=move |_| {
                                                                selected_diff_peek.update(|selected| {
                                                                    *selected = if selected.as_ref() == Some(&peek_for_click) {
                                                                        None
                                                                    } else {
                                                                        Some(peek_for_click.clone())
                                                                    };
                                                                });
                                                            }
                                                            on:keydown=move |event: web_sys::KeyboardEvent| {
                                                                if event.key() == "Escape" {
                                                                    selected_diff_peek.set(None);
                                                                }
                                                            }
                                                        ></button>
                                                    }
                                                })}
                                                {move || deleted_after_peek().map(|peek| {
                                                    let peek_for_click = peek.clone();
                                                    view! {
                                                        <button
                                                            type="button"
                                                            class="config-editor__diff-deleted config-editor__diff-deleted-after"
                                                            aria-label="Show removed lines at end of document"
                                                            title="Show removed lines"
                                                            on:click=move |_| {
                                                                selected_diff_peek.update(|selected| {
                                                                    *selected = if selected.as_ref() == Some(&peek_for_click) {
                                                                        None
                                                                    } else {
                                                                        Some(peek_for_click.clone())
                                                                    };
                                                                });
                                                            }
                                                            on:keydown=move |event: web_sys::KeyboardEvent| {
                                                                if event.key() == "Escape" {
                                                                    selected_diff_peek.set(None);
                                                                }
                                                            }
                                                        ></button>
                                                    }
                                                })}
                                            </div>
                                        }
                                    }
                                    />
                            </div>
                        </div>
                        <div class="config-editor__input-wrap">
                            <pre
                                class="config-editor__highlight"
                                aria-hidden="true"
                                style:transform=move || format!("translate({}px, {}px)", -scroll_x.get(), -scroll_y.get())
                            >
                                <code>
                                    <For
                                        each={move || 0..lines.with(Vec::len)}
                                        key=|index| *index
                                        children=move |index| {
                                            // Same per-row state signal the gutter reads;
                                            // every closure below subscribes only to it.
                                            let state = rows.state_for(
                                                index,
                                                &lines.get_untracked(),
                                                &diff.get_untracked(),
                                                current_line.get_untracked(),
                                            );
                                            let row_text = move || state.with(|row| row.text.clone());
                                            let has_error = move || state.with(|row| row.has_error);
                                            let is_added = move || state.with(|row| row.change == DiffKind::Added);
                                            let is_modified = move || state.with(|row| row.change == DiffKind::Modified);
                                            let is_active = move || state.with(|row| row.is_active);
                                            view! {
                                                <span
                                                    class="config-editor__code-line"
                                                    class:config-editor__code-line-active=is_active
                                                    class:config-editor__code-line-error=has_error
                                                    class:config-editor__code-line-added=is_added
                                                    class:config-editor__code-line-modified=is_modified
                                                >
                                                    {move || {
                                                        let text = row_text();
                                                        if enable_highlighting {
                                                            behavior.highlight_line(&text)
                                                        } else {
                                                            view! { <span>{text}</span> }.into_any()
                                                        }
                                                    }}
                                                </span>
                                            }
                                        }
                                    />
                                </code>
                            </pre>
                            <div
                                class="config-editor__diagnostic-overlay"
                                aria-label=format!("Inline {label} diagnostics")
                                style:transform=move || format!("translate({}px, {}px)", -scroll_x.get(), -scroll_y.get())
                            >
                                {move || {
                                    let text = source.get();
                                    diagnostics.get().into_iter().filter_map(|diagnostic| {
                                        let line = diagnostic.line?;
                                        let column = diagnostic.column?;
                                        let line_text = text.lines().nth(line.saturating_sub(1)).unwrap_or("");
                                        let width = diagnostic.end_column
                                            .unwrap_or(column + 1)
                                            .saturating_sub(column)
                                            .max(1);
                                        let marked_text: String = line_text
                                            .chars()
                                            .skip(column.saturating_sub(1))
                                            .take(width)
                                            .collect();
                                        let is_eof = marked_text.is_empty();
                                        Some(view! {
                                            <button
                                                type="button"
                                                class="config-editor__diagnostic-marker"
                                                class:config-editor__diagnostic-marker-eof=is_eof
                                                class:config-editor__diagnostic-marker-near-top=line <= 3
                                                tabindex="-1"
                                                aria-label=format!("{label} error on line {line}: {}", diagnostic.message)
                                                data-message=diagnostic.message
                                                style=format!(
                                                    "left: {}ch; top: calc(var(--config-editor-padding-top) + {} * var(--config-editor-line-height)); width: {}ch; --config-editor-diagnostic-tooltip-offset: -{}ch;",
                                                    column.saturating_sub(1),
                                                    line.saturating_sub(1),
                                                    width,
                                                    column.saturating_sub(1)
                                                )
                                                on:mousedown=|event| event.prevent_default()
                                                on:click=move |_| {
                                                    if let Some(editor) = editor_ref.get() {
                                                        let text = source.get_untracked();
                                                        let offset = offset_at(&text, line, column);
                                                        let _ = editor.focus();
                                                        cursor.set(offset);
                                                        current_line.set(line);
                                                        suggestions_requested.set(false);
                                                        hide_hover();
                                                        let _ = editor.set_selection_range(offset as u32, offset as u32);
                                                    }
                                                }
                                            >
                                                <span>{marked_text}</span>
                                            </button>
                                        })
                                    }).collect_view()
                                }}
                            </div>
                            <textarea
                                class="config-editor__input"
                                style=move || format!(
                                    "--config-editor-content-height: calc(var(--config-editor-padding-top) + var(--config-editor-padding-bottom) + {} * var(--config-editor-line-height));",
                                    source.get().split('\n').count()
                                )
                                aria-label=format!("{label} source")
                                aria-autocomplete=move || (!read_only.get()).then_some("list")
                                aria-haspopup=move || (!read_only.get()).then_some("listbox")
                                aria-expanded=move || (!suggestions.get().is_empty()).to_string()
                                aria-invalid=move || (!diagnostics.get().is_empty()).to_string()
                                node_ref=editor_ref
                                spellcheck="false"
                                autocapitalize="off"
                                autocomplete="off"
                                wrap="off"
                                readonly=move || read_only.get()
                                aria-readonly=move || read_only.get().to_string()
                                prop:value=move || source.get()
                                on:input=move |event| {
                                    if read_only.get_untracked() {
                                        if let Some(element) = event
                                            .target()
                                            .and_then(|target| target.dyn_into::<web_sys::HtmlTextAreaElement>().ok())
                                        {
                                            element.set_value(&source.get_untracked());
                                        }
                                        return;
                                    }
                                    let value = event_target_value(&event);
                                    let before = source.get_untracked();
                                    let input_type = event
                                        .dyn_ref::<web_sys::InputEvent>()
                                        .map(web_sys::InputEvent::input_type)
                                        .unwrap_or_default();
                                    let auto_trigger = auto_trigger_completions
                                        && should_auto_trigger(&input_type, &before, &value);
                                    if before != value {
                                        history.update(|history| {
                                            history.record_input(
                                                before,
                                                cursor.get_untracked(),
                                                &input_type,
                                                event.time_stamp(),
                                            );
                                        });
                                    }
                                    if let Some(element) = event
                                        .target()
                                        .and_then(|target| target.dyn_into::<web_sys::HtmlTextAreaElement>().ok())
                                    {
                                        let position = element.selection_start().ok().flatten().unwrap_or(0) as usize;
                                        cursor.set(position);
                                        current_line.set(line_at_cursor(&value, position));
                                    }
                                    source.set(value);
                                    if let Some(on_change) = on_change.as_ref() {
                                        on_change.run(source.get_untracked());
                                    }
                                    active_completion.set(0);
                                    explicit_request.set(false);
                                    completion_navigated.set(false);
                                    suggestions_requested.set(auto_trigger);
                                    hide_hover();
                                }
                                on:mousemove=move |event: web_sys::MouseEvent| {
                                    if !hover_docs {
                                        return;
                                    }
                                    let Some(element) = event
                                        .target()
                                        .and_then(|target| target.dyn_into::<web_sys::HtmlTextAreaElement>().ok())
                                    else {
                                        return;
                                    };
                                    if !suggestions.get_untracked().is_empty() {
                                        hide_hover();
                                        return;
                                    }
                                    let (line, column) = pointer_text_position(
                                        &element,
                                        event.client_x() as f64,
                                        event.client_y() as f64,
                                    );
                                    let Some(info) = key_hover(
                                        &source.get_untracked(),
                                        line,
                                        column,
                                        schema_for_hover.as_deref(),
                                        behavior,
                                    ) else {
                                        hide_hover();
                                        return;
                                    };
                                    let key_identity = (line, info.key.clone());
                                    if hover_pending.get_value().as_ref() == Some(&key_identity) {
                                        return;
                                    }
                                    if hover_card
                                        .with_untracked(|card| card.as_ref().is_some_and(|card| card.line == line && card.info.key == info.key))
                                    {
                                        return;
                                    }
                                    hide_hover();
                                    hover_pending.set_value(Some(key_identity));
                                    let token = hover_token.get_value();
                                    let anchor = hover_card_style(&element, line, info.start_column);
                                    schedule_hover(move || {
                                        if hover_token.get_value() == token {
                                            hover_pending.set_value(None);
                                            hover_card.set(Some(HoverCard { info, line, style: anchor }));
                                        }
                                    });
                                }
                                on:mouseleave=move |_| hide_hover()
                                on:blur=move |_| {
                                    suggestions_requested.set(false);
                                    hide_hover();
                                }
                                on:click=move |event| {
                                    if let Some(element) = event
                                        .target()
                                        .and_then(|target| target.dyn_into::<web_sys::HtmlTextAreaElement>().ok())
                                    {
                                        let position = element.selection_start().ok().flatten().unwrap_or(0) as usize;
                                        cursor.set(position);
                                        current_line.set(line_at_cursor(&source.get_untracked(), position));
                                        active_completion.set(0);
                                        suggestions_requested.set(false);
                                        hide_hover();
                                    }
                                }
                                on:keyup=move |event| {
                                    if let Some(element) = event
                                        .target()
                                        .and_then(|target| target.dyn_into::<web_sys::HtmlTextAreaElement>().ok())
                                    {
                                        let position = element.selection_start().ok().flatten().unwrap_or(0) as usize;
                                        cursor.set(position);
                                        current_line.set(line_at_cursor(&source.get_untracked(), position));
                                    }
                                }
                                on:scroll=move |event| {
                                    hide_hover();
                                    if let Some(element) = event
                                        .target()
                                        .and_then(|target| target.dyn_into::<web_sys::HtmlTextAreaElement>().ok())
                                    {
                                        scroll_x.set(element.scroll_left() as f64);
                                        scroll_y.set(element.scroll_top() as f64);
                                    }
                                }
                                on:keydown=move |event: web_sys::KeyboardEvent| {
                                    hide_hover();
                                    if read_only.get_untracked() {
                                        // Escape dismissal is handled by the root
                                        // editor keydown listener; just swallow
                                        // keys that would otherwise act here.
                                        if event.key() == "Escape"
                                            || (event.ctrl_key() && event.code() == "Space")
                                        {
                                            event.prevent_default();
                                        }
                                        return;
                                    }
                                    let key = event.key().to_ascii_lowercase();
                                    let modified = event.ctrl_key() || event.meta_key();
                                    let is_undo = modified && key == "z" && !event.shift_key();
                                    let is_redo = modified
                                        && ((key == "z" && event.shift_key())
                                            || (event.ctrl_key() && key == "y"));
                                    if is_undo || is_redo {
                                        event.prevent_default();
                                        let current = source.get_untracked();
                                        let current_cursor = event
                                            .target()
                                            .and_then(|target| {
                                                target.dyn_into::<web_sys::HtmlTextAreaElement>().ok()
                                            })
                                            .and_then(|element| {
                                                element.selection_start().ok().flatten()
                                            })
                                            .map(|cursor| cursor as usize)
                                            .unwrap_or_else(|| cursor.get_untracked());
                                        let mut restored = None;
                                        history.update(|history| {
                                            restored = if is_undo {
                                                history.undo(current, current_cursor)
                                            } else {
                                                history.redo(current, current_cursor)
                                            };
                                        });
                                        if let Some(restored) = restored {
                                            source.set(restored.value.clone());
                                            cursor.set(restored.cursor);
                                            current_line.set(line_at_cursor(
                                                &restored.value,
                                                restored.cursor,
                                            ));
                                            if let Some(element) = event
                                                .target()
                                                .and_then(|target| {
                                                    target
                                                        .dyn_into::<web_sys::HtmlTextAreaElement>()
                                                        .ok()
                                                })
                                            {
                                                element.set_value(&restored.value);
                                                element
                                                    .set_selection_range(
                                                        restored.cursor as u32,
                                                        restored.cursor as u32,
                                                    )
                                                    .expect("editor selection range should remain valid");
                                            }
                                            if let Some(on_change) = on_change.as_ref() {
                                                on_change.run(restored.value);
                                            }
                                            suggestions_requested.set(false);
                                        }
                                        return;
                                    }
                                    if event.ctrl_key() && event.code() == "Space" {
                                        event.prevent_default();
                                        active_completion.set(0);
                                        explicit_request.set(true);
                                        completion_navigated.set(true);
                                        suggestions_requested.set(true);
                                        return;
                                    }
                                    if event.code() == "F8" {
                                        event.prevent_default();
                                        let errors = diagnostics.get_untracked();
                                        let text = source.get_untracked();
                                        let here = cursor.get_untracked();
                                        let mut positioned = Vec::new();
                                        for (index, diagnostic) in errors.iter().enumerate() {
                                            let (Some(line), Some(column)) =
                                                (diagnostic.line, diagnostic.column)
                                            else {
                                                continue;
                                            };
                                            positioned.push((offset_at(&text, line, column), index));
                                        }
                                        // Cycle past the caret, wrapping at the
                                        // ends like VS Code; equal offsets stay
                                        // put so repeated F8 never stalls.
                                        let offsets: Vec<usize> = positioned
                                            .iter()
                                            .map(|(offset, _)| *offset)
                                            .collect();
                                        if let Some(position) =
                                            cycle_offset(&offsets, here, event.shift_key())
                                        {
                                            go_to_diagnostic(positioned[position].1);
                                        }
                                        return;
                                    }
                                    if (event.ctrl_key() || event.meta_key())
                                        && (event.code() == "Slash" || key == "/" || key == "?")
                                    {
                                        event.prevent_default();
                                        if let Some(element) = event
                                            .target()
                                            .and_then(|target| target.dyn_into::<web_sys::HtmlTextAreaElement>().ok())
                                        {
                                            let start = element.selection_start().ok().flatten().unwrap_or(0) as usize;
                                            let end = element.selection_end().ok().flatten().unwrap_or(start as u32) as usize;
                                            let before = source.get_untracked();
                                            if let Some((updated, selection_start, selection_end)) =
                                                behavior.toggle_comments(&before, start, end)
                                            {
                                                if updated != before {
                                                    commit_edit(
                                                        updated,
                                                        selection_end,
                                                        Some(selection_end),
                                                        before,
                                                        start,
                                                        source,
                                                        cursor,
                                                        current_line,
                                                        history,
                                                        on_change,
                                                        Some(element.clone()),
                                                        scroll_y,
                                                        false,
                                                    );
                                                } else {
                                                    cursor.set(selection_end);
                                                    current_line.set(line_at_cursor(
                                                        &source.get_untracked(),
                                                        selection_end,
                                                    ));
                                                }
                                                element
                                                    .set_selection_range(
                                                        selection_start as u32,
                                                        selection_end as u32,
                                                    )
                                                    .expect("editor selection range should remain valid");
                                                suggestions_requested.set(false);
                                            }
                                        }
                                        return;
                                    }
                                    let options = suggestions.get();
                                    if enable_auto_closing
                                        && !event.ctrl_key()
                                        && !event.meta_key()
                                        && !event.alt_key()
                                        && let Some(element) = event
                                            .target()
                                            .and_then(|target| target.dyn_into::<web_sys::HtmlTextAreaElement>().ok())
                                    {
                                            let start = element.selection_start().ok().flatten().unwrap_or(0) as usize;
                                            let end = element.selection_end().ok().flatten().unwrap_or(start as u32) as usize;
                                            let key = event.key();
                                            if start == end && key.chars().count() == 1 {
                                                let character = key.chars().next().unwrap();
                                                let closing = match character {
                                                    ')' | ']' | '}' | '"' | '\'' => Some(character),
                                                    _ => None,
                                                };
                                                if let Some(closing) = closing
                                                    && let Some(next_cursor) = skip_closing_pair(
                                                        &source.get_untracked(),
                                                        start,
                                                        closing,
                                                    )
                                                {
                                                    event.prevent_default();
                                                    cursor.set(next_cursor);
                                                    current_line.set(line_at_cursor(
                                                        &source.get_untracked(),
                                                        next_cursor,
                                                    ));
                                                    let _ = element.set_selection_range(
                                                        next_cursor as u32,
                                                        next_cursor as u32,
                                                    );
                                                    return;
                                                }
                                            }

                                            if key == "Backspace" && start == end {
                                                let before = source.get_untracked();
                                                if let Some((updated, new_cursor)) =
                                                    delete_empty_pair(&before, start)
                                                {
                                                    event.prevent_default();
                                                    commit_edit(
                                                        updated,
                                                        new_cursor,
                                                        None,
                                                        before,
                                                        start,
                                                        source,
                                                        cursor,
                                                        current_line,
                                                        history,
                                                        on_change,
                                                        Some(element),
                                                        scroll_y,
                                                        false,
                                                    );
                                                    return;
                                                }
                                            }

                                            if key.chars().count() == 1
                                                && let Some(opening) = key.chars().next()
                                                && let Some((updated, selection_start, selection_end)) =
                                                    paired_edit(
                                                        &source.get_untracked(),
                                                        start,
                                                        end,
                                                        opening,
                                                        behavior,
                                                    )
                                            {
                                                event.prevent_default();
                                                // A pair that begins an entry (JSON key quote) opens its list,
                                                // since no input event follows a prevented keydown.
                                                let opens_entry = selection_start == selection_end
                                                    && behavior.entry_started(&updated, selection_start);
                                                commit_edit(
                                                    updated,
                                                    selection_end,
                                                    Some(selection_start),
                                                    source.get_untracked(),
                                                    start,
                                                    source,
                                                    cursor,
                                                    current_line,
                                                    history,
                                                    on_change,
                                                    Some(element.clone()),
                                                    scroll_y,
                                                    false,
                                                );
                                                // Paired edit with a selection keeps the wrapped
                                                // text selected; commit_edit collapses to the
                                                // end, so restore the selection range here.
                                                if selection_start != selection_end {
                                                    let _ = element.set_selection_range(
                                                        selection_start as u32,
                                                        selection_end as u32,
                                                    );
                                                }
                                                suggestions_requested.set(opens_entry);
                                                return;
                                            }
                                    }
                                    match event.key().as_str() {
                                        "ArrowDown" if !options.is_empty() => {
                                            event.prevent_default();
                                            let next = (active_completion.get_untracked() + 1) % options.len();
                                            active_completion.set(next);
                                            completion_navigated.set(true);
                                            if let Some(list) = completion_ref.get() {
                                                scroll_completion_into_view(&list, next);
                                            }
                                        }
                                        "ArrowUp" if !options.is_empty() => {
                                            event.prevent_default();
                                            let next = active_completion
                                                .get_untracked()
                                                .checked_sub(1)
                                                .unwrap_or(options.len() - 1);
                                            active_completion.set(next);
                                            completion_navigated.set(true);
                                            if let Some(list) = completion_ref.get() {
                                                scroll_completion_into_view(&list, next);
                                            }
                                        }
                                        // Enter accepts the highlighted suggestion whenever the
                                        // list is open, including one opened by typing, so a
                                        // typed key completes on Enter like the Ctrl+Space list.
                                        "Enter" if !options.is_empty() => {
                                            event.prevent_default();
                                            apply_selected();
                                        }
                                        // Tab accepts only after explicit engagement (arrow
                                        // keys, hover, Ctrl+Space, or a previous accept). An
                                        // auto-triggered list must not swallow Tab, which
                                        // indents, as in VS Code.
                                        "Tab"
                                            if !options.is_empty()
                                                && !event.shift_key()
                                                && should_accept_with_keyboard(
                                                    explicit_request.get_untracked(),
                                                    completion_navigated.get_untracked(),
                                                ) =>
                                        {
                                            event.prevent_default();
                                            apply_selected();
                                        }
                                        "Escape" if suggestions_requested.get_untracked() => {
                                            event.prevent_default();
                                            suggestions_requested.set(false);
                                        }
                                        "Tab" => {
                                            event.prevent_default();
                                            if let Some(element) = event
                                                .target()
                                                .and_then(|target| target.dyn_into::<web_sys::HtmlTextAreaElement>().ok())
                                            {
                                                let start = element.selection_start().ok().flatten().unwrap_or(0) as usize;
                                                let end = element.selection_end().ok().flatten().unwrap_or(start as u32) as usize;
                                                let text = source.get_untracked();
                                                if event.shift_key() || start != end {
                                                    if let Some((updated, selection_start, selection_end)) =
                                                        indent_lines(&text, start, end, event.shift_key())
                                                    {
                                                        if updated != text {
                                                            commit_edit(
                                                                updated,
                                                                selection_end,
                                                                Some(selection_start),
                                                                text,
                                                                start,
                                                                source,
                                                                cursor,
                                                                current_line,
                                                                history,
                                                                on_change,
                                                                Some(element.clone()),
                                                                scroll_y,
                                                                false,
                                                            );
                                                        } else {
                                                            cursor.set(selection_end);
                                                            current_line.set(line_at_cursor(
                                                                &source.get_untracked(),
                                                                selection_end,
                                                            ));
                                                        }
                                                        // Indent/outdent keeps the block
                                                        // selected; restore the range that
                                                        // commit_edit collapses.
                                                        if let Some(editor) = editor_ref.get() {
                                                            let _ = editor.set_selection_range(
                                                                selection_start as u32,
                                                                selection_end as u32,
                                                            );
                                                        }
                                                        suggestions_requested.set(false);
                                                    }
                                                    return;
                                                }
                                                let (updated, new_cursor) =
                                                    replace_selection(&text, start, end, &indent_unit(&text));
                                                commit_edit(
                                                    updated,
                                                    new_cursor,
                                                    None,
                                                    text,
                                                    start,
                                                    source,
                                                    cursor,
                                                    current_line,
                                                    history,
                                                    on_change,
                                                    Some(element),
                                                    scroll_y,
                                                    true,
                                                );
                                                suggestions_requested.set(false);
                                            }
                                        }
                                        "Enter" => {
                                            event.prevent_default();
                                            if let Some(element) = event
                                                .target()
                                                .and_then(|target| target.dyn_into::<web_sys::HtmlTextAreaElement>().ok())
                                            {
                                                let start = element.selection_start().ok().flatten().unwrap_or(0) as usize;
                                                let end = element.selection_end().ok().flatten().unwrap_or(start as u32) as usize;
                                                let text = source.get_untracked();
                                                let (insertion, caret) = behavior.newline_insertion(&text, start);
                                                let (updated, _) = replace_selection(&text, start, end, &insertion);
                                                // The caret position inside the insertion, in UTF-16 units from the start.
                                                let new_cursor = start + insertion[..caret].encode_utf16().count();
                                                commit_edit(
                                                    updated,
                                                    new_cursor,
                                                    None,
                                                    text,
                                                    start,
                                                    source,
                                                    cursor,
                                                    current_line,
                                                    history,
                                                    on_change,
                                                    Some(element),
                                                    scroll_y,
                                                    true,
                                                );
                                                suggestions_requested.set(false);
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            ></textarea>
                            {move || hover_card.get().filter(|_| suggestions.get().is_empty()).map(|card| {
                                let info = card.info;
                                let examples = info.examples.join(", ");
                                view! {
                                    <div class="config-editor__hover" role="tooltip" style=card.style>
                                        <div class="config-editor__hover-header">
                                            <code class="config-editor__hover-key">{info.key}</code>
                                            <span class="config-editor__hover-type">{info.type_label}</span>
                                            {info.required.then(|| view! {
                                                <span class="config-editor__hover-required">"required"</span>
                                            })}
                                        </div>
                                        {info.description.map(|description| view! {
                                            <p class="config-editor__hover-description">{description}</p>
                                        })}
                                        {info.default.map(|default| view! {
                                            <div class="config-editor__hover-meta">
                                                <span>"Default"</span>
                                                <code>{default}</code>
                                            </div>
                                        })}
                                        {(!examples.is_empty()).then(|| view! {
                                            <div class="config-editor__hover-meta">
                                                <span>"Examples"</span>
                                                <code>{examples}</code>
                                            </div>
                                        })}
                                    </div>
                                }
                            })}
                            <div
                                class="config-editor__completions"
                                class:hidden=move || suggestions.get().is_empty()
                                role="listbox"
                                aria-label=format!("{label} suggestions")
                                node_ref=completion_ref
                                style=move || {
                                    popup_layout_revision.get();
                                    scroll_x.get();
                                    scroll_y.get();
                                    let option_count = suggestions.get().len();
                                    editor_ref
                                        .get()
                                        .map(|editor| {
                                            completion_popup_style(
                                                &editor,
                                                &source.get(),
                                                cursor.get(),
                                                option_count,
                                            )
                                        })
                                        .unwrap_or_else(|| "visibility:hidden".to_string())
                                }
                            >
                                {move || suggestions.get().into_iter().enumerate().map(|(index, option)| {
                                    let selected = move || active_completion.get() == index;
                                    let apply = option.clone();
                                    let detail = option.detail.clone();
                                    view! {
                                        <button
                                            type="button"
                                            role="option"
                                            aria-selected=selected
                                            class="config-editor__completion"
                                            class:config-editor__completion-active=selected
                                            on:mousedown=move |event| event.prevent_default()
                                            on:mouseenter=move |_| {
                                                active_completion.set(index);
                                                completion_navigated.set(true);
                                            }
                                            on:click=move |_| {
                                                let (updated, new_cursor) = apply_completion(&source.get_untracked(), &apply);
                                                let request_completions = accept_reopens_suggestions(&apply, auto_trigger_completions);
                                                hide_hover();
                                                commit_edit(
                                                    updated,
                                                    new_cursor,
                                                    None,
                                                    source.get_untracked(),
                                                    apply.start,
                                                    source,
                                                    cursor,
                                                    current_line,
                                                    history,
                                                    on_change,
                                                    editor_ref.get(),
                                                    scroll_y,
                                                    false,
                                                );
                                                active_completion.set(0);
                                                explicit_request.set(true);
                                                completion_navigated.set(true);
                                                suggestions_requested.set(request_completions);
                                            }
                                        >
                                            <span class="config-editor__completion-icon" aria-hidden="true">
                                                {if option.is_value { "V" } else { "K" }}
                                            </span>
                                            <span class="config-editor__completion-content">
                                                <span class="config-editor__completion-primary">
                                                    <span class="config-editor__completion-key">{
                                                        option.label.chars().enumerate().map(|(index, character)| {
                                                            let highlighted = option.matched.iter().any(|(start, end)| (*start..*end).contains(&index));
                                                            if highlighted {
                                                                view! { <span class="config-editor__completion-match">{character.to_string()}</span> }.into_any()
                                                            } else {
                                                                view! { <span>{character.to_string()}</span> }.into_any()
                                                            }
                                                        }).collect_view()
                                                    }</span>
                                                    <span class="config-editor__completion-type">{option.type_label}</span>
                                                </span>
                                                {(!detail.is_empty()).then(|| view! {
                                                    <span class="config-editor__completion-detail" title=detail.clone()>
                                                        {detail.clone()}
                                                    </span>
                                                })}
                                            </span>
                                        </button>
                                    }
                                }).collect_view()}
                            </div>
                        </div>
                    </div>
                    {show_footer.then(|| view! { <div class="config-editor__status">
                        <span class="config-editor__hint">
                            {enable_completions.then(|| view! { <span><kbd>Ctrl</kbd><span>"+"</span><kbd>Space</kbd><span>"for suggestions"</span></span> })}
                        </span>
                        <span class="config-editor__status-meta">
                            {show_diagnostics.then(|| view! {
                                <button
                                    type="button"
                                    class="config-editor__diagnostic-status"
                                    class:config-editor__diagnostic-status-error=move || !diagnostics.get().is_empty()
                                    disabled=move || diagnostics.get().is_empty()
                                    aria-live="polite"
                                    title=move || {
                                        diagnostics.get().first()
                                            .filter(|diagnostic| diagnostic.line.is_none())
                                            .map(|diagnostic| diagnostic.message.clone())
                                    }
                                    on:click=move |_| {
                                        let errors = diagnostics.get_untracked();
                                        if let Some(index) = errors
                                            .iter()
                                            .position(|diagnostic| diagnostic.line.is_some() && diagnostic.column.is_some())
                                        {
                                            go_to_diagnostic(index);
                                        }
                                    }
                                >
                                    {move || {
                                        let errors = diagnostics.get();
                                        match errors.len() {
                                            0 => "No errors".to_string(),
                                            count if errors.iter().any(|error| error.line.is_none()) => {
                                                format!("{count} error(s) · {}", errors[0].message)
                                            }
                                            count => format!("{count} error(s) · hover or focus underlined text"),
                                        }
                                    }}
                                </button>
                            })}
                            <span class="config-editor__line-count">{move || format!("{} lines", source.get().split('\n').count())}</span>
                        </span>
                    </div> })}
                </section>
                {move || selected_diff_peek.get().map(|peek| {
                    let line = peek.line;
                    let title = peek.title.clone();
                    let before = peek.before.clone();
                    let after = peek.after.clone();
                    let old_line_count = before.len();
                    let new_line_count = after.len();
                    let render_lines = |lines: Vec<String>, sign: &'static str, class_name: &'static str| {
                        lines.into_iter().map(|line| view! {
                            <div class=format!("config-editor__diff-peek-line {class_name}")>
                                <span aria-hidden="true">{sign}</span>
                                <code>{if line.is_empty() { " ".to_string() } else { line }}</code>
                            </div>
                        }).collect_view()
                    };
                    view! {
                        <section
                            class="config-editor__diff-peek"
                            role="dialog"
                            aria-label=format!("{title}, line {line}")
                            style=move || {
                                popup_layout_revision.get();
                                scroll_y.get();
                                match (editor_ref.get(), root_ref.get()) {
                                    (Some(editor), Some(root)) => peek_popup_style(
                                        &editor,
                                        &root,
                                        line,
                                        old_line_count,
                                        new_line_count,
                                    ),
                                    _ => "visibility:hidden".to_string(),
                                }
                            }
                            on:keydown=move |event: web_sys::KeyboardEvent| {
                                if event.key() == "Escape" {
                                    selected_diff_peek.set(None);
                                }
                            }
                        >
                            <header class="config-editor__diff-peek-header">
                                <strong>{format!("{title} · line {line}")}</strong>
                                <button
                                    type="button"
                                    aria-label="Close change preview"
                                    on:click=move |_| selected_diff_peek.set(None)
                                >"×"</button>
                            </header>
                            <div class="config-editor__diff-peek-columns">
                                {(!before.is_empty()).then(|| view! {
                                    <section class="config-editor__diff-peek-side">
                                        <h3>"Before"</h3>
                                        {render_lines(before, "−", "config-editor__diff-peek-removed")}
                                    </section>
                                })}
                                {(!after.is_empty()).then(|| view! {
                                    <section class="config-editor__diff-peek-side">
                                        <h3>"Current"</h3>
                                        {render_lines(after, "+", "config-editor__diff-peek-current")}
                                    </section>
                                })}
                            </div>
                        </section>
                    }
                })}
        </div>
    }
}

#[cfg(test)]
mod tests;
