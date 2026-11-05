use leptos::{ev, leptos_dom::helpers::window_event_listener, prelude::*};
use std::sync::Arc;
use wasm_bindgen::JsCast;

use crate::highlight::highlight_line;

mod history;
use history::EditHistory;
mod diff;
use diff::{DiffKind, LineDiff, diff_lines, peek_popup_style};
mod keyboard;
use keyboard::{
    delete_empty_pair, paired_edit, replace_selection, skip_closing_pair, utf16_to_byte_index,
};
mod diagnostics;
#[cfg(test)]
use diagnostics::SchemaBundle;
use diagnostics::{build_schema, validate_yaml};
mod completion;
use completion::{
    KeyHover, accept_reopens_suggestions, apply_completion, completion_popup_style, completions,
    key_hover, scroll_completion_into_view, should_auto_trigger,
};
mod commands;
use commands::{
    indent_unit, indent_yaml_lines, line_at_cursor, newline_with_indent, toggle_yaml_line_comments,
};
mod metrics;
use metrics::read_editor_metrics;

const HOVER_DELAY_MS: i32 = 350;

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
struct DiffPeek {
    line: usize,
    title: String,
    before: Vec<String>,
    after: Vec<String>,
}

#[derive(Clone, PartialEq)]
struct HoverCard {
    info: KeyHover,
    line: usize,
    style: String,
}

fn pointer_text_position(element: &web_sys::HtmlTextAreaElement, x: f64, y: f64) -> (usize, usize) {
    let target: &web_sys::Element = element.unchecked_ref();
    let rect = target.get_bounding_client_rect();
    let metrics = read_editor_metrics(target);
    let line =
        (y - rect.top() - metrics.padding_top + element.scroll_top() as f64) / metrics.line_height;
    let column = (x - rect.left() - metrics.padding_left + element.scroll_left() as f64)
        / (metrics.font_size * 0.6);
    (
        line.max(0.0).floor() as usize,
        column.max(0.0).floor() as usize,
    )
}

fn hover_card_style(element: &web_sys::HtmlTextAreaElement, line: usize, column: usize) -> String {
    let target: &web_sys::Element = element.unchecked_ref();
    let rect = target.get_bounding_client_rect();
    let metrics = read_editor_metrics(target);
    let Some(window) = web_sys::window() else {
        return String::new();
    };
    let viewport_width = window
        .inner_width()
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(1024.0);
    let viewport_height = window
        .inner_height()
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(768.0);
    let left = (rect.left() + metrics.padding_left + column as f64 * metrics.font_size * 0.6
        - element.scroll_left() as f64)
        .clamp(8.0, (viewport_width - 348.0).max(8.0));
    let line_top = rect.top() + metrics.padding_top + line as f64 * metrics.line_height
        - element.scroll_top() as f64;
    if line_top > 240.0 && line_top + metrics.line_height + 240.0 > viewport_height {
        format!("left:{left:.1}px;top:{line_top:.1}px;transform:translateY(calc(-100% - 4px))")
    } else {
        format!(
            "left:{left:.1}px;top:{:.1}px",
            line_top + metrics.line_height + 4.0
        )
    }
}

/// Programmatic edits do not reliably scroll a textarea to a newly-set caret.
/// Keep the caret in the scrollport so Enter and completion expansion don't
/// make the document/page grow while the insertion point moves off-screen.
fn set_caret_visible(
    editor: &web_sys::HtmlTextAreaElement,
    text: &str,
    offset: usize,
    scroll_y: RwSignal<f64>,
) {
    let offset = offset.min(text.encode_utf16().count());
    let _ = editor.set_selection_range(offset as u32, offset as u32);
    let byte_offset = utf16_to_byte_index(text, offset);
    let line = text[..byte_offset]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count() as f64;
    let target: &web_sys::Element = editor.unchecked_ref();
    let metrics = read_editor_metrics(target);
    let style =
        web_sys::window().and_then(|window| window.get_computed_style(target).ok().flatten());
    let padding_bottom = style
        .as_ref()
        .and_then(|style| style.get_property_value("padding-bottom").ok())
        .and_then(|value| {
            value
                .trim()
                .strip_suffix("px")
                .unwrap_or(&value)
                .parse::<f64>()
                .ok()
        })
        .unwrap_or(0.0);
    let current_top = editor.scroll_top() as f64;
    let client_height = editor.client_height() as f64;
    let caret_top = metrics.padding_top + line * metrics.line_height;
    let caret_bottom = caret_top + metrics.line_height;
    let visible_top = current_top + metrics.padding_top;
    let visible_bottom = current_top + client_height - padding_bottom;
    let next_top = if caret_bottom > visible_bottom {
        caret_bottom - client_height + padding_bottom
    } else if caret_top < visible_top {
        caret_top - metrics.padding_top
    } else {
        current_top
    }
    .clamp(
        0.0,
        (editor.scroll_height() as f64 - client_height).max(0.0),
    );
    editor.set_scroll_top(next_top as i32);
    scroll_y.set(next_top);
}

fn schedule_hover(callback: impl FnOnce() + 'static) {
    if let Some(window) = web_sys::window() {
        let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
            wasm_bindgen::closure::Closure::once_into_js(callback).unchecked_ref(),
            HOVER_DELAY_MS,
        );
    }
}

pub const PRESET_CSS: &str = concat!(include_str!("preset.css"), include_str!("themes.css"));

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum EditorTheme {
    #[default]
    VsCodeDark,
    VsCodeLight,
    Obsidian,
    ObsidianLight,
    Dracula,
    Nord,
    AtomOneDark,
    GitHubLight,
    GitHubDark,
    SolarizedLight,
    ViraInspired,
}

impl EditorTheme {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::VsCodeDark => "vscode-dark",
            Self::VsCodeLight => "vscode-light",
            Self::Obsidian => "obsidian",
            Self::ObsidianLight => "obsidian-light",
            Self::Dracula => "dracula",
            Self::Nord => "nord",
            Self::AtomOneDark => "atom-one-dark",
            Self::GitHubLight => "github-light",
            Self::GitHubDark => "github-dark",
            Self::SolarizedLight => "solarized-light",
            Self::ViraInspired => "vira-inspired",
        }
    }

    pub fn css_family(self) -> &'static str {
        match self {
            Self::VsCodeDark | Self::VsCodeLight | Self::Obsidian | Self::ObsidianLight => {
                self.as_str()
            }
            Self::Dracula
            | Self::Nord
            | Self::AtomOneDark
            | Self::GitHubDark
            | Self::ViraInspired => "obsidian",
            Self::GitHubLight | Self::SolarizedLight => "vscode-light",
        }
    }
}

#[component]
pub fn YamlEditorToolbar(
    children: Children,
    #[prop(optional, into)] class: Option<String>,
    #[prop(default = EditorTheme::VsCodeDark.into(), into)] theme: Signal<EditorTheme>,
) -> impl IntoView {
    view! {
        <div
            class=format!("yaml-editor yaml-editor__toolbar {}", class.unwrap_or_default())
            data-theme=move || theme.get().css_family()
            data-color-theme=move || theme.get().as_str()
        >
            {children()}
        </div>
    }
}

#[component]
pub fn YamlEditorFooter(
    children: Children,
    #[prop(optional, into)] class: Option<String>,
    #[prop(default = EditorTheme::VsCodeDark.into(), into)] theme: Signal<EditorTheme>,
) -> impl IntoView {
    view! {
        <div
            class=format!("yaml-editor yaml-editor__status {}", class.unwrap_or_default())
            data-theme=move || theme.get().css_family()
            data-color-theme=move || theme.get().as_str()
        >
            {children()}
        </div>
    }
}

#[component]
pub fn YamlEditor(
    #[prop(optional, into)] initial_value: Option<String>,
    #[prop(optional, into)] value: Option<Signal<String>>,
    #[prop(optional, into)] baseline: Option<Signal<String>>,
    #[prop(optional, into)] schema_json: Option<String>,
    #[prop(optional, into)] file_name: Option<String>,
    #[prop(optional, into)] class: Option<String>,
    #[prop(default = EditorTheme::VsCodeDark.into(), into)] theme: Signal<EditorTheme>,
    #[prop(optional)] on_change: Option<Callback<String>>,
    #[prop(default = true)] styled: bool,
    #[prop(default = false)] show_toolbar: bool,
    #[prop(default = false)] show_footer: bool,
    #[prop(default = true)] show_diagnostics: bool,
    #[prop(default = true)] enable_completions: bool,
    #[prop(default = true)] enable_hover_docs: bool,
    #[prop(default = true)] auto_trigger_completions: bool,
    #[prop(default = true)] enable_highlighting: bool,
    #[prop(default = true)] enable_auto_closing: bool,
    #[prop(default = true)] show_diff: bool,
    #[prop(default = false)] read_only: bool,
) -> impl IntoView {
    let initial_value = normalize_newlines(initial_value.unwrap_or_default());
    let (schema_bundle, schema_error) = match schema_json {
        Some(schema_json) => match build_schema(&schema_json) {
            Ok(schema) => (Some(Arc::new(schema)), None),
            Err(error) => (None, Some(error)),
        },
        None => (None, None),
    };
    let enable_completions = enable_completions && !read_only && cfg!(feature = "completion");
    let enable_highlighting = enable_highlighting && cfg!(feature = "syntax-highlighting");
    let source = RwSignal::new(
        value
            .as_ref()
            .map(Signal::get_untracked)
            .unwrap_or_else(|| initial_value.clone()),
    );
    let baseline = baseline.unwrap_or_else(|| Signal::derive(move || initial_value.clone()));
    let selected_diff_peek = RwSignal::new(None::<DiffPeek>);
    Effect::new(move |_| {
        source.get();
        baseline.get();
        selected_diff_peek.set(None);
    });
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
    let on_change_for_input = on_change;
    let on_change_for_keydown = on_change;
    let on_change_for_apply = on_change;
    let on_change_for_completion = on_change;
    let history_for_apply = history;
    let schema_for_diagnostics = schema_bundle.clone();
    let schema_error_for_diagnostics = schema_error.clone();
    let diagnostics = Memo::new(move |_| {
        validate_yaml(
            &source.get(),
            schema_for_diagnostics.as_deref(),
            schema_error_for_diagnostics.as_deref(),
            show_diagnostics && cfg!(feature = "diagnostics"),
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
        )
    });
    let apply_selected = move || {
        let options = suggestions.get();
        if let Some(completion) = options.get(active_completion.get()) {
            let (updated, new_cursor) = apply_completion(&source.get_untracked(), completion);
            let editor = editor_ref.get();
            let before = source.get_untracked();
            history_for_apply
                .update(|history| history.record_action(before, cursor.get_untracked()));
            if let Some(editor) = editor.as_ref() {
                editor.set_value(&updated);
            }
            let request_completions =
                accept_reopens_suggestions(completion, auto_trigger_completions);
            hide_hover();
            source.set(updated);
            if let Some(on_change) = on_change_for_apply.as_ref() {
                on_change.run(source.get_untracked());
            }
            cursor.set(new_cursor);
            current_line.set(line_at_cursor(&source.get_untracked(), new_cursor));
            active_completion.set(0);
            explicit_request.set(true);
            suggestions_requested.set(request_completions);
            if let Some(editor) = editor {
                set_caret_visible(&editor, &source.get_untracked(), new_cursor, scroll_y);
            }
        }
    };

    let file_name = file_name.unwrap_or_else(|| "untitled.yaml".to_string());
    let root_class = class.unwrap_or_default();
    let show_diagnostics = show_diagnostics && cfg!(feature = "diagnostics");
    let use_preset_style = styled && cfg!(feature = "preset-style");
    let has_schema = schema_bundle.is_some();
    let diff = Memo::new(move |_| {
        if show_diff {
            diff_lines(&baseline.get(), &source.get())
        } else {
            LineDiff {
                current: vec![DiffKind::Unchanged; source.get().split('\n').count()],
                deleted_before: vec![false; source.get().split('\n').count()],
                deleted_lines_before: vec![Vec::new(); source.get().split('\n').count()],
                modified_before: vec![None; source.get().split('\n').count()],
                deleted_after: false,
                deleted_lines_after: Vec::new(),
            }
        }
    });

    view! {
        <div
            class=format!(
                "yaml-editor {}{}",
                root_class,
                if read_only { " yaml-editor__readonly" } else { "" }
            )
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
            <section class="yaml-editor__frame" aria-label="YAML editor">
                {show_toolbar.then(|| view! {
                    <div class="yaml-editor__toolbar">
                        <div class="yaml-editor__file">
                            <span class="yaml-editor__file-icon">"Y"</span>
                            <span>{file_name.clone()}</span>
                        </div>
                        <div class="yaml-editor__toolbar-meta">
                            {has_schema.then(|| view! {
                                <span class="yaml-editor__schema-state">
                                    <span class="yaml-editor__schema-check">"✓"</span>
                                    "Schema connected"
                                </span>
                            })}
                            <span class="yaml-editor__language">"YAML"</span>
                        </div>
                    </div>
                })}

                    <div class="yaml-editor__surface">
                        <div
                            class="yaml-editor__gutter"
                        >
                            <div
                                class="yaml-editor__gutter-content"
                                style:transform=move || format!("translateY(-{}px)", scroll_y.get())
                            >
                            {move || {
                                let line_count = source.get().split('\n').count();
                                let current_lines = source.get().split('\n').map(str::to_string).collect::<Vec<_>>();
                                let errors = diagnostics.get();
                                let line_diff = diff.get();
                                (1..=line_count)
                                    .map(|line| {
                                        let has_error = errors
                                            .iter()
                                            .any(|error| error.line == Some(line));
                                        let change = line_diff.current
                                            .get(line - 1)
                                            .copied()
                                            .unwrap_or(DiffKind::Unchanged);
                                        let deleted_before = line_diff
                                            .deleted_before
                                            .get(line - 1)
                                            .copied()
                                            .unwrap_or(false);
                                        let deleted_after = line == line_count && line_diff.deleted_after;
                                        let modified_peek = line_diff.modified_before.get(line - 1)
                                            .and_then(Option::as_ref)
                                            .map(|before| DiffPeek {
                                                line,
                                                title: "Modified line".to_string(),
                                                before: vec![before.clone()],
                                                after: vec![current_lines.get(line - 1).cloned().unwrap_or_default()],
                                            });
                                        let deleted_peek = deleted_before.then(|| DiffPeek {
                                            line,
                                            title: "Removed lines".to_string(),
                                            before: line_diff.deleted_lines_before.get(line - 1).cloned().unwrap_or_default(),
                                            after: Vec::new(),
                                        });
                                        let deleted_after_peek = deleted_after.then(|| DiffPeek {
                                            line,
                                            title: "Removed lines at end of document".to_string(),
                                            before: line_diff.deleted_lines_after.clone(),
                                            after: Vec::new(),
                                        });
                                        view! {
                                            <div
                                                class:yaml-editor__line-number-active=move || current_line.get() == line
                                                class:yaml-editor__line-number-error=has_error
                                                class:yaml-editor__line-number-added=change == DiffKind::Added
                                                class:yaml-editor__line-number-modified=change == DiffKind::Modified
                                                on:click=move |_| {
                                                    if let Some(editor) = editor_ref.get() {
                                                        let text = source.get_untracked();
                                                        let offset: usize = text
                                                            .split('\n')
                                                            .take(line.saturating_sub(1))
                                                            .map(|text_line| text_line.encode_utf16().count() + 1)
                                                            .sum();
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
                                                {(change == DiffKind::Added).then(|| view! {
                                                    <span
                                                        class="yaml-editor__diff-marker yaml-editor__diff-marker-added"
                                                        aria-hidden="true"
                                                    ></span>
                                                })}
                                                {modified_peek.map(|peek| {
                                                    let peek_for_click = peek.clone();
                                                    view! {
                                                        <button
                                                            type="button"
                                                            class="yaml-editor__diff-marker yaml-editor__diff-marker-modified yaml-editor__diff-change-trigger"
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
                                                {deleted_peek.map(|peek| {
                                                    let peek_for_click = peek.clone();
                                                    view! {
                                                        <button
                                                            type="button"
                                                            class="yaml-editor__diff-deleted"
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
                                                {deleted_after_peek.map(|peek| {
                                                    let peek_for_click = peek.clone();
                                                    view! {
                                                        <button
                                                            type="button"
                                                            class="yaml-editor__diff-deleted yaml-editor__diff-deleted-after"
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
                                    })
                                    .collect_view()
                            }}
                            </div>
                        </div>
                        <div class="yaml-editor__input-wrap">
                            <pre
                                class="yaml-editor__highlight"
                                aria-hidden="true"
                                style:transform=move || format!("translate({}px, {}px)", -scroll_x.get(), -scroll_y.get())
                            >
                                <code>
                                    {move || {
                                        let text = source.get();
                                        let errors = diagnostics.get();
                                        let line_diff = diff.get();
                                        text.split('\n').enumerate().map(|(index, line)| {
                                            let line_number = index + 1;
                                            let has_error = errors
                                                .iter()
                                                .any(|error| error.line == Some(line_number));
                                            let change = line_diff.current
                                                .get(index)
                                                .copied()
                                                .unwrap_or(DiffKind::Unchanged);
                                            view! {
                                                <span
                                                    class="yaml-editor__code-line"
                                                    class:yaml-editor__code-line-active=move || current_line.get() == line_number
                                                    class:yaml-editor__code-line-error=has_error
                                                    class:yaml-editor__code-line-added=change == DiffKind::Added
                                                    class:yaml-editor__code-line-modified=change == DiffKind::Modified
                                                >
                                                    {if enable_highlighting {
                                                        highlight_line(line)
                                                    } else {
                                                        view! { <span>{line.to_string()}</span> }.into_any()
                                                    }}
                                                </span>
                                            }
                                        }).collect_view()
                                    }}
                                </code>
                            </pre>
                            <div
                                class="yaml-editor__diagnostic-overlay"
                                aria-label="Inline YAML diagnostics"
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
                                                class="yaml-editor__diagnostic-marker"
                                                class:yaml-editor__diagnostic-marker-eof=is_eof
                                                class:yaml-editor__diagnostic-marker-near-top=line <= 3
                                                aria-label=format!("YAML error on line {line}: {}", diagnostic.message)
                                                data-message=diagnostic.message
                                                style=format!(
                                                    "left: {}ch; top: calc(var(--yaml-editor-padding-top) + {} * var(--yaml-editor-line-height)); width: {}ch; --yaml-editor-diagnostic-tooltip-offset: -{}ch;",
                                                    column.saturating_sub(1),
                                                    line.saturating_sub(1),
                                                    width,
                                                    column.saturating_sub(1)
                                                )
                                                on:mousedown=|event| event.prevent_default()
                                                on:click=move |_| {
                                                    if let Some(editor) = editor_ref.get() {
                                                        let text = source.get_untracked();
                                                        let line_start = text
                                                            .split('\n')
                                                            .take(line.saturating_sub(1))
                                                            .map(|text_line| text_line.encode_utf16().count() + 1)
                                                            .sum::<usize>();
                                                        let column_offset = text
                                                            .split('\n')
                                                            .nth(line.saturating_sub(1))
                                                            .unwrap_or("")
                                                            .chars()
                                                            .take(column.saturating_sub(1))
                                                            .map(char::len_utf16)
                                                            .sum::<usize>();
                                                        let offset = line_start + column_offset;
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
                                class="yaml-editor__input"
                                style=move || format!(
                                    "--yaml-editor-content-height: calc(var(--yaml-editor-padding-top) + var(--yaml-editor-padding-bottom) + {} * var(--yaml-editor-line-height));",
                                    source.get().split('\n').count()
                                )
                                aria-label="YAML source"
                                aria-autocomplete="list"
                                aria-haspopup="listbox"
                                aria-expanded=move || (!suggestions.get().is_empty()).to_string()
                                aria-invalid=move || (!diagnostics.get().is_empty()).to_string()
                                node_ref=editor_ref
                                spellcheck="false"
                                autocapitalize="off"
                                autocomplete="off"
                                wrap="off"
                                readonly=read_only
                                aria-readonly=read_only.to_string()
                                prop:value=move || source.get()
                                on:input=move |event| {
                                    if read_only {
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
                                    if let Some(on_change) = on_change_for_input.as_ref() {
                                        on_change.run(source.get_untracked());
                                    }
                                    active_completion.set(0);
                                    explicit_request.set(false);
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
                                    if read_only {
                                        if event.key() == "Escape" {
                                            event.prevent_default();
                                            suggestions_requested.set(false);
                                            explicit_request.set(false);
                                            selected_diff_peek.set(None);
                                            hide_hover();
                                        }
                                        if event.ctrl_key() && event.code() == "Space" {
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
                                            if let Some(on_change) = on_change_for_keydown.as_ref() {
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
                                        suggestions_requested.set(true);
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
                                                toggle_yaml_line_comments(&before, start, end)
                                            {
                                                if updated != before {
                                                    history.update(|history| {
                                                        history.record_action(before, start);
                                                    });
                                                    element.set_value(&updated);
                                                    source.set(updated);
                                                    if let Some(on_change) = on_change_for_keydown.as_ref() {
                                                        on_change.run(source.get_untracked());
                                                    }
                                                }
                                                cursor.set(selection_end);
                                                current_line.set(line_at_cursor(
                                                    &source.get_untracked(),
                                                    selection_end,
                                                ));
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
                                                    history.update(|history| {
                                                        history.record_action(before, start);
                                                    });
                                                    element.set_value(&updated);
                                                    source.set(updated);
                                                    if let Some(on_change) =
                                                        on_change_for_keydown.as_ref()
                                                    {
                                                        on_change.run(source.get_untracked());
                                                    }
                                                    cursor.set(new_cursor);
                                                    current_line.set(line_at_cursor(
                                                        &source.get_untracked(),
                                                        new_cursor,
                                                    ));
                                                    element
                                                        .set_selection_range(
                                                            new_cursor as u32,
                                                            new_cursor as u32,
                                                        )
                                                        .expect("editor selection range should remain valid");
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
                                                    )
                                            {
                                                event.prevent_default();
                                                history.update(|history| {
                                                    history.record_action(
                                                        source.get_untracked(),
                                                        start,
                                                    );
                                                });
                                                element.set_value(&updated);
                                                source.set(updated);
                                                if let Some(on_change) =
                                                    on_change_for_keydown.as_ref()
                                                {
                                                    on_change.run(source.get_untracked());
                                                }
                                                cursor.set(selection_end);
                                                current_line.set(line_at_cursor(
                                                    &source.get_untracked(),
                                                    selection_end,
                                                ));
                                                element
                                                    .set_selection_range(
                                                        selection_start as u32,
                                                        selection_end as u32,
                                                    )
                                                    .expect("editor selection range should remain valid");
                                                suggestions_requested.set(false);
                                                return;
                                            }
                                    }
                                    match event.key().as_str() {
                                        "ArrowDown" if !options.is_empty() => {
                                            event.prevent_default();
                                            let next = (active_completion.get_untracked() + 1) % options.len();
                                            active_completion.set(next);
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
                                            if let Some(list) = completion_ref.get() {
                                                scroll_completion_into_view(&list, next);
                                            }
                                        }
                                        "Enter" | "Tab"
                                            if !options.is_empty()
                                                && !(event.key() == "Tab" && event.shift_key()) =>
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
                                                        indent_yaml_lines(&text, start, end, event.shift_key())
                                                    {
                                                        if updated != text {
                                                            history.update(|history| {
                                                                history.record_action(
                                                                    text.clone(),
                                                                    start,
                                                                );
                                                            });
                                                            element.set_value(&updated);
                                                            source.set(updated);
                                                            if let Some(on_change) =
                                                                on_change_for_keydown.as_ref()
                                                            {
                                                                on_change.run(source.get_untracked());
                                                            }
                                                        }
                                                        cursor.set(selection_end);
                                                        current_line.set(line_at_cursor(
                                                            &source.get_untracked(),
                                                            selection_end,
                                                        ));
                                                        element
                                                            .set_selection_range(
                                                                selection_start as u32,
                                                                selection_end as u32,
                                                            )
                                                            .expect("editor selection range should remain valid");
                                                        suggestions_requested.set(false);
                                                    }
                                                    return;
                                                }
                                                let (updated, new_cursor) =
                                                    replace_selection(&text, start, end, &indent_unit(&text));
                                                history.update(|history| {
                                                    history.record_action(text, start);
                                                });
                                                element.set_value(&updated);
                                                source.set(updated);
                                                if let Some(on_change) = on_change_for_keydown.as_ref() {
                                                    on_change.run(source.get_untracked());
                                                }
                                                cursor.set(new_cursor);
                                                current_line.set(line_at_cursor(&source.get_untracked(), new_cursor));
                                                suggestions_requested.set(false);
                                                set_caret_visible(
                                                    &element,
                                                    &source.get_untracked(),
                                                    new_cursor,
                                                    scroll_y,
                                                );
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
                                                let insertion = newline_with_indent(&text, start);
                                                let (updated, new_cursor) = replace_selection(&text, start, end, &insertion);
                                                history.update(|history| {
                                                    history.record_action(text, start);
                                                });
                                                element.set_value(&updated);
                                                source.set(updated);
                                                if let Some(on_change) = on_change_for_keydown.as_ref() {
                                                    on_change.run(source.get_untracked());
                                                }
                                                cursor.set(new_cursor);
                                                current_line.set(line_at_cursor(&source.get_untracked(), new_cursor));
                                                suggestions_requested.set(false);
                                                set_caret_visible(
                                                    &element,
                                                    &source.get_untracked(),
                                                    new_cursor,
                                                    scroll_y,
                                                );
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
                                    <div class="yaml-editor__hover" role="tooltip" style=card.style>
                                        <div class="yaml-editor__hover-header">
                                            <code class="yaml-editor__hover-key">{info.key}</code>
                                            <span class="yaml-editor__hover-type">{info.type_label}</span>
                                            {info.required.then(|| view! {
                                                <span class="yaml-editor__hover-required">"required"</span>
                                            })}
                                        </div>
                                        {info.description.map(|description| view! {
                                            <p class="yaml-editor__hover-description">{description}</p>
                                        })}
                                        {info.default.map(|default| view! {
                                            <div class="yaml-editor__hover-meta">
                                                <span>"Default"</span>
                                                <code>{default}</code>
                                            </div>
                                        })}
                                        {(!examples.is_empty()).then(|| view! {
                                            <div class="yaml-editor__hover-meta">
                                                <span>"Examples"</span>
                                                <code>{examples}</code>
                                            </div>
                                        })}
                                    </div>
                                }
                            })}
                            <div
                                class="yaml-editor__completions"
                                class:hidden=move || suggestions.get().is_empty()
                                role="listbox"
                                aria-label="YAML suggestions"
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
                                            class="yaml-editor__completion"
                                            class:yaml-editor__completion-active=selected
                                            on:mousedown=move |event| event.prevent_default()
                                            on:mouseenter=move |_| active_completion.set(index)
                                            on:click=move |_| {
                                                let (updated, new_cursor) = apply_completion(&source.get_untracked(), &apply);
                                                let editor = editor_ref.get();
                                                history.update(|history| {
                                                    history.record_action(
                                                        source.get_untracked(),
                                                        apply.start,
                                                    );
                                                });
                                                if let Some(editor) = editor.as_ref() {
                                                    editor.set_value(&updated);
                                                }
                                                let request_completions = accept_reopens_suggestions(&apply, auto_trigger_completions);
                                                hide_hover();
                                                source.set(updated);
                                                if let Some(on_change) = on_change_for_completion.as_ref() {
                                                    on_change.run(source.get_untracked());
                                                }
                                                cursor.set(new_cursor);
                                                current_line.set(line_at_cursor(&source.get_untracked(), new_cursor));
                                                active_completion.set(0);
                                                explicit_request.set(true);
                                                suggestions_requested.set(request_completions);
                                                if let Some(editor) = editor {
                                                    editor
                                                        .set_selection_range(
                                                            new_cursor as u32,
                                                            new_cursor as u32,
                                                        )
                                                        .expect("editor selection range should remain valid");
                                                }
                                            }
                                        >
                                            <span class="yaml-editor__completion-icon" aria-hidden="true">
                                                {if option.is_value { "V" } else { "K" }}
                                            </span>
                                            <span class="yaml-editor__completion-content">
                                                <span class="yaml-editor__completion-primary">
                                                    <span class="yaml-editor__completion-key">{option.label}</span>
                                                    <span class="yaml-editor__completion-type">{option.type_label}</span>
                                                </span>
                                                {(!detail.is_empty()).then(|| view! {
                                                    <span class="yaml-editor__completion-detail" title=detail.clone()>
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
                    {show_footer.then(|| view! { <div class="yaml-editor__status">
                        <span class="yaml-editor__hint">
                            {enable_completions.then(|| view! { <span><kbd>Ctrl</kbd><span>"+"</span><kbd>Space</kbd><span>"for suggestions"</span></span> })}
                        </span>
                        <span class="yaml-editor__status-meta">
                            {show_diagnostics.then(|| view! {
                                <span
                                    class="yaml-editor__diagnostic-status"
                                    class:yaml-editor__diagnostic-status-error=move || !diagnostics.get().is_empty()
                                    role="status"
                                    aria-live="polite"
                                    title=move || {
                                        diagnostics.get().first()
                                            .filter(|diagnostic| diagnostic.line.is_none())
                                            .map(|diagnostic| diagnostic.message.clone())
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
                                </span>
                            })}
                            <span class="yaml-editor__line-count">{move || format!("{} lines", source.get().split('\n').count())}</span>
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
                            <div class=format!("yaml-editor__diff-peek-line {class_name}")>
                                <span aria-hidden="true">{sign}</span>
                                <code>{if line.is_empty() { " ".to_string() } else { line }}</code>
                            </div>
                        }).collect_view()
                    };
                    view! {
                        <section
                            class="yaml-editor__diff-peek"
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
                            <header class="yaml-editor__diff-peek-header">
                                <strong>{format!("{title} · line {line}")}</strong>
                                <button
                                    type="button"
                                    aria-label="Close change preview"
                                    on:click=move |_| selected_diff_peek.set(None)
                                >"×"</button>
                            </header>
                            <div class="yaml-editor__diff-peek-columns">
                                {(!before.is_empty()).then(|| view! {
                                    <section class="yaml-editor__diff-peek-side">
                                        <h3>"Before"</h3>
                                        {render_lines(before, "−", "yaml-editor__diff-peek-removed")}
                                    </section>
                                })}
                                {(!after.is_empty()).then(|| view! {
                                    <section class="yaml-editor__diff-peek-side">
                                        <h3>"Current"</h3>
                                        {render_lines(after, "+", "yaml-editor__diff-peek-current")}
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
