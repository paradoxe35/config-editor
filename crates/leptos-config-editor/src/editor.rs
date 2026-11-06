use leptos::{ev, leptos_dom::helpers::window_event_listener, prelude::*};
use wasm_bindgen::JsCast;

mod bridge;
mod chrome;
mod codec;
mod highlight;
mod inline;
mod language;
pub use chrome::{ConfigEditorFooter, ConfigEditorToolbar};
mod languages;
pub use codec::{Conversion, ConvertError};
pub use diagnostics::{Diagnostic, Schema, SchemaError, SchemaSource, Severity, check_syntax};
pub use language::ParseError;
pub use languages::{Language, UnknownLanguage};
mod theme;
pub use theme::{EditorTheme, UnknownTheme};
mod history;
use history::EditHistory;
mod diff;
use diff::{DiffKind, DiffPeek, LineDiff, diff_lines, peek_popup_style, peek_survives_edit};
mod keyboard;
use keyboard::{delete_empty_pair, paired_edit, replace_selection, skip_closing_pair};
mod diagnostics;
use bridge::Bridge;
use diagnostics::{DIAGNOSTICS_DEBOUNCE_MS, validate};
mod completion;
use completion::{
    COMPLETION_PAGE_ROWS, KeyHover, accept_reopens_suggestions, apply_completion,
    completion_popup_style, completions, key_hover, scroll_completion_into_view,
    should_auto_trigger,
};
mod commands;
use commands::{
    backspace_indent, cycle_offset, indent_lines, indent_unit, line_at_cursor, offset_at,
    typed_closer,
};
mod line_rows;
use line_rows::{LineRows, RenderedLine};
mod metrics;
mod positioning;
use positioning::{hover_card_style, pointer_text_position, schedule_hover, set_caret_visible};

/// Converts CRLF and CR to LF; the textarea and all line math assume LF.
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

/// The preset stylesheet: layout plus every bundled theme.
///
/// The editor injects it unless `styled=false`, in which case a host can serve
/// it itself.
pub const PRESET_CSS: &str = concat!(include_str!("preset.css"), include_str!("themes.css"));

/// Applies a programmatic edit: records undo, syncs the textarea and state,
/// and notifies the host. Every keydown edit goes through here to keep that
/// order consistent.
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

/// A schema-aware config editor for YAML, JSON, TOML, HCL, and XML.
///
/// See the [crate docs](crate) for an example.
#[component]
pub fn ConfigEditor(
    /// Language of the host's document. Defaults to YAML.
    #[prop(default = Language::default(), into)]
    language: Language,
    /// Language the user edits in, when it differs from `language`.
    ///
    /// `initial_value`, `value`, `baseline`, and `on_change` stay in
    /// `language`. Edits convert back after each pause in typing; invalid text
    /// is never sent, and values `language` cannot represent exactly are
    /// flagged as warnings. `None` edits in `language`.
    #[prop(default = None, into)]
    edit_as: Option<Language>,
    /// Initial text and diff baseline. For controlled content, use `value`
    /// and `baseline` instead.
    #[prop(optional, into)]
    initial_value: Option<String>,
    /// Controlled document text. Changes flow into the editor; update it from
    /// `on_change` to keep the host as the source of truth.
    #[prop(optional, into)]
    value: Option<Signal<String>>,
    /// Text to diff against. Set it to the current text (for example after
    /// saving) to clear the diff.
    #[prop(optional, into)]
    baseline: Option<Signal<String>>,
    /// JSON Schema for live diagnostics, completions, and hover docs: a JSON
    /// string (`schema=include_str!("schema.json")`) or a shared, precompiled
    /// [`Schema`]. One schema serves every language.
    #[prop(optional, into)]
    schema: Option<SchemaSource>,
    /// Name shown when the built-in toolbar is enabled.
    #[prop(optional, into)]
    file_name: Option<String>,
    /// Extra class on the editor root for host styles.
    #[prop(optional, into)]
    class: Option<String>,
    /// Color theme; may be a signal to switch themes live.
    #[prop(default = EditorTheme::VsCodeDark.into(), into)]
    theme: Signal<EditorTheme>,
    /// Called with the document, in `language`, after every edit.
    #[prop(optional, into)]
    on_change: Option<Callback<String>>,
    /// Inject [`PRESET_CSS`]. Set `false` to style the `config-editor__*`
    /// markup yourself.
    #[prop(default = true)]
    styled: bool,
    /// Show the built-in file/schema toolbar. See also [`ConfigEditorToolbar`].
    #[prop(default = false)]
    show_toolbar: bool,
    /// Show the built-in status footer. See also [`ConfigEditorFooter`].
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
    /// Open suggestions while typing and after accepting a key. When `false`,
    /// suggestions open only with `Ctrl+Space`.
    #[prop(default = true)]
    auto_trigger_completions: bool,
    /// Enable syntax colors for the document language.
    #[prop(default = true)]
    enable_highlighting: bool,
    /// Auto-close quotes and brackets, wrap selections, and delete empty
    /// pairs on Backspace.
    #[prop(default = true)]
    enable_auto_closing: bool,
    /// Show line changes against `baseline`.
    #[prop(default = true)]
    show_diff: bool,
    /// Disable editing while keeping highlighting, diagnostics, diffs, and
    /// hover docs.
    #[prop(default = Signal::from(false), into)]
    read_only: Signal<bool>,
) -> impl IntoView {
    let host_language = language;
    let language = edit_as.unwrap_or(language);
    let bridge = Bridge::new(host_language, language);
    // With a bridge, edits reach the host through the conversion effect.
    let host_on_change = on_change;
    let on_change = if bridge.is_some() {
        None
    } else {
        host_on_change
    };
    let label = language.label();
    let file_icon = language.file_icon();
    let initial_value = normalize_newlines(initial_value.unwrap_or_default());
    let (schema_bundle, schema_error) = match schema.map(SchemaSource::resolve) {
        Some(Ok(schema)) => (Some(schema), None),
        Some(Err(error)) => (None, Some(error)),
        None => (None, None),
    };
    let enable_completions =
        enable_completions && !read_only.get_untracked() && cfg!(feature = "schema-validation");
    let behavior = language.behavior();
    // Last host text received or sent, and the edited text matching it.
    let host_text = StoredValue::new(
        value
            .as_ref()
            .map(Signal::get_untracked)
            .map_or_else(|| initial_value.clone(), normalize_newlines),
    );
    let load_problems = RwSignal::new(Vec::<Diagnostic>::new());
    let shown = move |host: &str| -> String {
        let Some(bridge) = bridge else {
            return host.to_string();
        };
        match bridge.to_edit(host) {
            Ok(conversion) => {
                load_problems.set(Vec::new());
                conversion.text
            }
            Err(error) => {
                load_problems.set(vec![bridge.load_problem(&error)]);
                host.to_string()
            }
        }
    };
    let synced_edit = StoredValue::new(shown(&host_text.get_value()));
    let source = RwSignal::new(synced_edit.get_value());
    let baseline = baseline.unwrap_or_else(|| Signal::derive(move || initial_value.clone()));
    let baseline = match bridge {
        Some(bridge) => {
            let shown_baseline = Memo::new(move |_| {
                let host = normalize_newlines(baseline.get());
                bridge
                    .to_edit(&host)
                    .map_or(host, |conversion| conversion.text)
            });
            Signal::derive(move || shown_baseline.get())
        }
        None => baseline,
    };
    let selected_diff_peek = RwSignal::new(None::<DiffPeek>);
    if let Some(value) = value {
        Effect::new(move |_| {
            let external_value = normalize_newlines(value.get());
            // Ignore the host echoing our own edit; reconverting would
            // reformat the text under the caret.
            if external_value == host_text.get_value() {
                return;
            }
            host_text.set_value(external_value.clone());
            let edit = shown(&external_value);
            synced_edit.set_value(edit.clone());
            if source.get_untracked() != edit {
                source.set(edit);
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
    let schema_for_diagnostics = schema_bundle.clone();
    let schema_error_for_diagnostics = schema_error.clone();
    // Debounced so half-typed text does not flash squiggles; everything else
    // reads the live source.
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
    let conversion_problems = RwSignal::new(Vec::<Diagnostic>::new());
    if let Some(bridge) = bridge {
        Effect::new(move |_| {
            let text = diagnosed_text.get();
            if text == synced_edit.get_value() {
                conversion_problems.set(Vec::new());
                return;
            }
            let result = bridge.to_host(&text, &host_text.get_value());
            conversion_problems.set(bridge.problems(&text, &result));
            if let Ok(conversion) = result {
                synced_edit.set_value(text);
                if conversion.text != host_text.get_value() {
                    host_text.set_value(conversion.text.clone());
                    if let Some(on_change) = host_on_change {
                        on_change.run(conversion.text);
                    }
                }
            }
        });
    }
    let diagnostics = Memo::new(move |_| {
        let mut found = validate(
            &diagnosed_text.get(),
            schema_for_diagnostics.as_deref(),
            schema_error_for_diagnostics.as_deref(),
            show_diagnostics,
            behavior,
        );
        if show_diagnostics {
            found.extend(load_problems.get());
            found.extend(conversion_problems.get());
        }
        found
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
            suggestions_requested.set(request_completions);
        }
    };
    // Callers must pass a diagnostic that has a position.
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
    let use_preset_style = styled;
    let has_schema = schema_bundle.is_some();
    let diff = Memo::new(move |_| {
        source.with(|text| {
            if show_diff {
                baseline.with(|baseline| diff_lines(baseline, text))
            } else {
                LineDiff::empty(text.split('\n').count())
            }
        })
    });
    // Rows must not subscribe to this memo or `diff`; the sync effect below
    // fans it out to per-row signals (see `line_rows`).
    let lines = Memo::new(move |_| {
        let error_lines: std::collections::HashSet<usize> = diagnostics.with(|errors| {
            errors
                .iter()
                .filter(|error| error.severity == Severity::Error)
                .filter_map(|error| error.line)
                .collect()
        });
        source.with(|text| {
            diff.with(|line_diff| {
                text.split('\n')
                    .enumerate()
                    .map(|(index, line)| RenderedLine {
                        text: line.to_string(),
                        has_error: error_lines.contains(&(index + 1)),
                        change: line_diff
                            .current
                            .get(index)
                            .copied()
                            .unwrap_or(DiffKind::Unchanged),
                    })
                    .collect::<Vec<_>>()
            })
        })
    });
    let rows = LineRows::new();
    let rows_for_sync = rows.clone();
    let rows_for_gutter = rows.clone();
    let rows_for_highlight = rows.clone();
    Effect::new(move |_| {
        let line = current_line.get();
        lines.with(|lines| diff.with(|diff| rows_for_sync.sync(lines, diff, line)));
    });
    // Close the change preview when an edit touches its anchor.
    Effect::new(move |_| {
        // The preview is read untracked so closing it cannot re-trigger this.
        diff.track();
        source.track();
        let survives = selected_diff_peek.with_untracked(|peek| {
            peek.as_ref().is_none_or(|peek| {
                let current: Vec<String> =
                    source.with_untracked(|text| text.split('\n').map(str::to_string).collect());
                diff.with_untracked(|line_diff| peek_survives_edit(peek, &current, line_diff))
            })
        });
        if !survives {
            selected_diff_peek.set(None);
        }
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
                                    // Subscribe only to the row's own signal (see `line_rows`).
                                    let state = rows_for_gutter.state_for(
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
                                            // Subscribe only to the row's own signal (see `line_rows`).
                                            let state = rows_for_highlight.state_for(
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
                                                class:config-editor__diagnostic-marker-warning=diagnostic.severity == Severity::Warning
                                                class:config-editor__diagnostic-marker-eof=is_eof
                                                class:config-editor__diagnostic-marker-near-top=line <= 3
                                                tabindex="-1"
                                                aria-label=format!(
                                                    "{label} {} on line {line}: {}",
                                                    if diagnostic.severity == Severity::Warning { "warning" } else { "error" },
                                                    diagnostic.message
                                                )
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
                                    // Backspace with the list open refilters it instead of closing.
                                    let refilter = input_type == "deleteContentBackward"
                                        && suggestions_requested.get_untracked()
                                        && !suggestions.with_untracked(Vec::is_empty);
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
                                    if !refilter {
                                        explicit_request.set(false);
                                    }
                                    suggestions_requested.set(auto_trigger || refilter);
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
                                        // The root listener handles Escape; swallow
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
                                                // A prevented keydown fires no input event, so open
                                                // suggestions here when the pair starts an entry.
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
                                                // Keep the wrapped text selected; commit_edit collapses it.
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
                                    // Backspace in indentation removes one unit; a closer on a
                                    // blank line aligns with its opener.
                                    if !event.ctrl_key()
                                        && !event.meta_key()
                                        && !event.alt_key()
                                        && let Some(element) = event
                                            .target()
                                            .and_then(|target| target.dyn_into::<web_sys::HtmlTextAreaElement>().ok())
                                    {
                                        let start = element.selection_start().ok().flatten().unwrap_or(0) as usize;
                                        let end = element.selection_end().ok().flatten().unwrap_or(start as u32) as usize;
                                        let key = event.key();
                                        let text = source.get_untracked();
                                        let edit = match key.as_str() {
                                            _ if start != end => None,
                                            "Backspace" => backspace_indent(&text, start),
                                            "}" => typed_closer(&text, start, '}'),
                                            "]" => typed_closer(&text, start, ']'),
                                            _ => None,
                                        };
                                        if let Some((updated, new_cursor)) = edit {
                                            event.prevent_default();
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
                                                false,
                                            );
                                            return;
                                        }
                                    }
                                    match event.key().as_str() {
                                        "PageDown" | "PageUp" if !options.is_empty() => {
                                            event.prevent_default();
                                            let current = active_completion.get_untracked();
                                            let last = options.len() - 1;
                                            let next = if event.key() == "PageDown" {
                                                (current + COMPLETION_PAGE_ROWS).min(last)
                                            } else {
                                                current.saturating_sub(COMPLETION_PAGE_ROWS)
                                            };
                                            active_completion.set(next);
                                            if let Some(list) = completion_ref.get() {
                                                scroll_completion_into_view(&list, next);
                                            }
                                        }
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
                                        "Enter" if !options.is_empty() => {
                                            event.prevent_default();
                                            apply_selected();
                                        }
                                        "Tab" if !options.is_empty() && !event.shift_key() => {
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
                                                        // Keep the block selected; commit_edit
                                                        // collapses it.
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
                                                // `caret` is a byte index; the cursor is in UTF-16 units.
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
                                    // Return before reading source and caret so typing
                                    // with the list hidden forces no layout.
                                    let option_count = suggestions.with(Vec::len);
                                    if option_count == 0 {
                                        return "visibility:hidden".to_string();
                                    }
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
