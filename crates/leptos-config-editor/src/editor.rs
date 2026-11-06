use leptos::{ev, leptos_dom::helpers::window_event_listener, prelude::*};
use wasm_bindgen::JsCast;

mod bridge;
mod chrome;
mod codec;
mod highlight;
mod inline;
pub mod language;
pub use chrome::{ConfigEditorFooter, ConfigEditorToolbar};
mod languages;
pub use codec::{Conversion, ConvertError};
pub use diagnostics::{Diagnostic, Schema, SchemaError, SchemaSource, Severity, check_syntax};
pub use language::ParseError;
pub use languages::{Language, UnknownLanguage};
mod theme;
pub use theme::{EditorTheme, ThemeColor, ThemeColors, UnknownTheme};
mod history;
use history::{EditHistory, Selection, Snapshot};
mod diff;
use diff::{DiffKind, DiffPeek, LineDiff, diff_lines, peek_popup_style, peek_survives_edit};
mod keyboard;
use keyboard::{
    delete_empty_pair, paired_edit, replace_selection, skip_closing_pair, utf16_to_byte_index,
};
mod diagnostics;
use bridge::Bridge;
use diagnostics::{DIAGNOSTICS_DEBOUNCE_MS, shift_diagnostics, validate};
mod completion;
use completion::{
    COMPLETION_PAGE_ROWS, KeyHover, accept_reopens_suggestions, apply_completion,
    completion_popup_style, completions, inserted_closers, key_hover, scroll_completion_into_view,
    should_auto_trigger, visual_column,
};
mod commands;
use commands::{
    backspace_indent, cycle_offset, indent_lines, line_at_cursor, line_range, offset_at,
    smart_home, tab_insert, typed_closer,
};
mod line_rows;
use line_rows::{LineRows, RenderedLine};
use metrics::read_editor_metrics;
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
    selection_anchor: Option<usize>,
    before: String,
    before_selection: Selection,
    source: RwSignal<String>,
    cursor: RwSignal<usize>,
    current_line: RwSignal<usize>,
    history: RwSignal<EditHistory>,
    on_change: Option<Callback<String>>,
    editor: Option<web_sys::HtmlTextAreaElement>,
    scroll_y: RwSignal<f64>,
    ensure_visible: bool,
) {
    let anchor = selection_anchor.unwrap_or(new_cursor);
    history.update(|history| {
        history.record_action(
            Snapshot {
                value: before,
                selection: before_selection,
            },
            (anchor.min(new_cursor), anchor.max(new_cursor)),
        );
    });
    if let Some(editor) = editor.as_ref() {
        editor.set_value(&updated);
    }
    source.set(updated);
    if let Some(on_change) = on_change.as_ref() {
        on_change.run(source.get_untracked());
    }
    cursor.set(new_cursor);
    current_line.set(line_at_cursor(&source.get_untracked(), new_cursor));
    if let Some(editor) = editor {
        if ensure_visible {
            set_caret_visible(&editor, &source.get_untracked(), new_cursor, scroll_y);
        }
        // The text between the anchor and the caret stays selected.
        let _ = editor
            .set_selection_range(anchor.min(new_cursor) as u32, anchor.max(new_cursor) as u32);
    }
}

fn textarea_selection(element: &web_sys::HtmlTextAreaElement) -> Selection {
    let start = element.selection_start().ok().flatten().unwrap_or(0) as usize;
    let end = element
        .selection_end()
        .ok()
        .flatten()
        .map_or(start, |end| end as usize);
    (start, end)
}

/// Rows rendered above and below the visible ones, so fast scrolling does
/// not show blank lines before the next frame.
const OVERSCAN_ROWS: usize = 40;

/// Rows rendered before the editor element exists to measure.
const INITIAL_ROWS: usize = 120;

/// Widest a diagnostic message gets, in pixels (`max-width` in preset.css).
const DIAGNOSTIC_TOOLTIP_MAX_WIDTH: f64 = 360.0;

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
    /// flagged as warnings. Accepts a `Language`, an `Option<Language>`, or a
    /// signal: changing it converts the open document in place, unsaved edits
    /// included. Unset edits in `language`.
    #[prop(optional, into)]
    edit_as: MaybeProp<Language>,
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
    /// Colors overriding the theme's, for a custom palette; may be a signal.
    #[prop(optional, into)]
    colors: MaybeProp<ThemeColors>,
    /// Called with the document, in `language`, after every edit.
    #[prop(optional, into)]
    on_change: Option<Callback<String>>,
    /// Called with the language being edited each time an `edit_as` switch
    /// completes. A switch waits while the document cannot be converted yet
    /// (TOML has no `null`), so this, not `edit_as`, is what is on screen.
    #[prop(optional, into)]
    on_edit_as_change: Option<Callback<Language>>,
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
    // The editing language can change while mounted (`edit_as` may be a
    // signal); everything that depends on it reads this signal.
    let edit_language = RwSignal::new(edit_as.get_untracked().unwrap_or(host_language));
    let (schema_bundle, schema_error) = match schema.map(SchemaSource::resolve) {
        Some(Ok(schema)) => (Some(schema), None),
        Some(Err(error)) => (None, Some(error)),
        None => (None, None),
    };
    let bridge_schema = StoredValue::new(schema_bundle.clone());
    let behavior = move || edit_language.get_untracked().behavior();
    let bridge = move || {
        Bridge::new(
            host_language,
            edit_language.get_untracked(),
            bridge_schema.get_value(),
        )
    };
    let label = move || edit_language.get().label();
    let file_icon = move || edit_language.get().file_icon();
    // Edits reach the host directly in its own language, or converted through
    // the conversion effect below.
    let host_on_change = on_change;
    let on_change = host_on_change.map(|callback| {
        Callback::new(move |text: String| {
            if bridge().is_none() {
                callback.run(text);
            }
        })
    });
    let initial_value = normalize_newlines(initial_value.unwrap_or_default());
    let enable_completions =
        enable_completions && !read_only.get_untracked() && cfg!(feature = "schema-validation");
    // Last host text received or sent, and the edited text matching it.
    let host_text = StoredValue::new(
        value
            .as_ref()
            .map(Signal::get_untracked)
            .map_or_else(|| initial_value.clone(), normalize_newlines),
    );
    let load_problems = RwSignal::new(Vec::<Diagnostic>::new());
    let shown = move |host: &str| -> String {
        let Some(bridge) = bridge() else {
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
    let host_baseline = baseline.unwrap_or_else(|| Signal::derive(move || initial_value.clone()));
    let shown_baseline = Memo::new(move |_| {
        let host = normalize_newlines(host_baseline.get());
        match Bridge::new(
            host_language,
            edit_language.get(),
            bridge_schema.get_value(),
        ) {
            Some(bridge) => bridge
                .to_edit(&host)
                .map_or(host, |conversion| conversion.text),
            None => host,
        }
    });
    let baseline = Signal::derive(move || shown_baseline.get());
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
    // Closers the editor inserted, as UTF-16 distances from the document end
    // (typing inside the pair keeps them). Only these are typed over.
    let auto_closers = StoredValue::new(Vec::<usize>::new());
    // An inserted default left selected after an accept: typing replaces it,
    // Tab keeps it and moves past it, Enter keeps it and starts a new line.
    let placeholder = StoredValue::new(None::<(usize, usize)>);
    // The line last copied or cut with nothing selected, to paste it whole.
    let copied_line = StoredValue::new(None::<String>);
    // The selection just before the browser applies an edit, for undo.
    let pre_edit_selection = StoredValue::new((0usize, 0usize));
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
    {
        Effect::new(move |_| {
            let text = diagnosed_text.get();
            let Some(bridge) = Bridge::new(
                host_language,
                edit_language.get(),
                bridge_schema.get_value(),
            ) else {
                conversion_problems.set(Vec::new());
                return;
            };
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
    // Switching `edit_as` converts the open document in place. Without unsaved
    // edits the text is rebuilt from the host's own text, so returning to
    // `language` restores it exactly (comments included); unsaved edits are
    // converted directly. Undo history is per language, so it starts over.
    // A switch that cannot convert yet (TOML has no `null`) stays pending and
    // is retried after every edit, so fixing the document completes it.
    let pending_switch = RwSignal::new(None::<Language>);
    let try_switch = move |next: Language| {
        let current = edit_language.get_untracked();
        let text = source.get_untracked();
        let unsaved = text != synced_edit.get_value();
        let converted = if !unsaved {
            match Bridge::new(host_language, next, bridge_schema.get_value()) {
                Some(bridge) => bridge
                    .to_edit(&host_text.get_value())
                    .map(|conversion| conversion.text),
                None => Ok(host_text.get_value()),
            }
        } else {
            match Bridge::new(host_language, current, bridge_schema.get_value())
                .filter(|_| next == host_language)
            {
                Some(bridge) => bridge
                    .to_host(&text, &host_text.get_value())
                    .map(|conversion| conversion.text),
                None => bridge_schema.with_value(|schema| {
                    codec::convert(
                        &text,
                        current.behavior(),
                        next.behavior(),
                        None,
                        schema.as_deref().map(|bundle| &bundle.schema),
                    )
                    .map(|conversion| conversion.text)
                }),
            }
        };
        match converted {
            Ok(text) => {
                if !unsaved {
                    synced_edit.set_value(text.clone());
                }
                pending_switch.set(None);
                load_problems.set(Vec::new());
                history.set(EditHistory::default());
                hide_hover();
                suggestions_requested.set(false);
                edit_language.set(next);
                if let Some(callback) = on_edit_as_change {
                    callback.run(next);
                }
                diagnosed_text.set(text.clone());
                source.set(text);
            }
            Err(error) => {
                pending_switch.set(Some(next));
                let message = format!("Cannot switch to {next} yet: {error}");
                load_problems.set(vec![bridge::locate_error(
                    current.behavior(),
                    &text,
                    &error,
                    message,
                )]);
            }
        }
    };
    Effect::new(move |previous: Option<Language>| {
        let next = edit_as.get().unwrap_or(host_language);
        if previous.is_some() {
            if next == edit_language.get_untracked() {
                // Back to the current language: nothing to convert.
                if pending_switch.get_untracked().is_some() {
                    pending_switch.set(None);
                    load_problems.set(Vec::new());
                }
            } else {
                try_switch(next);
            }
        }
        next
    });
    Effect::new(move |_| {
        diagnosed_text.track();
        if let Some(next) = pending_switch.get_untracked() {
            try_switch(next);
        }
    });
    let validated_diagnostics = Memo::new(move |_| {
        let mut found = validate(
            &diagnosed_text.get(),
            schema_for_diagnostics.as_deref(),
            schema_error_for_diagnostics.as_deref(),
            show_diagnostics,
            edit_language.get().behavior(),
        );
        if show_diagnostics {
            found.extend(load_problems.get());
            found.extend(conversion_problems.get());
        }
        found
    });
    // Until validation catches up with an edit, markers move with the lines
    // the edit shifted, so they never sit on the wrong text.
    let diagnostics = Memo::new(move |_| {
        let validated = diagnosed_text.get();
        source.with(|current| {
            validated_diagnostics.with(|found| shift_diagnostics(&validated, current, found))
        })
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
            behavior(),
        )
    });
    // Copy or cut with nothing selected takes the whole line, as in VS Code.
    let copy_line = move |event: web_sys::ClipboardEvent, cut: bool| {
        let Some(element) = editor_ref.get() else {
            return;
        };
        let (start, end) = textarea_selection(&element);
        let (Some(clipboard), true) = (event.clipboard_data(), start == end) else {
            return;
        };
        let text = source.get_untracked();
        let (line_start, line_end) = line_range(&text, start);
        let units: Vec<u16> = text.encode_utf16().collect();
        let mut line = String::from_utf16_lossy(&units[line_start..line_end]);
        if !line.ends_with('\n') {
            line.push('\n');
        }
        if clipboard.set_data("text/plain", &line).is_err() {
            return;
        }
        event.prevent_default();
        copied_line.set_value(Some(line));
        if cut && !read_only.get_untracked() {
            // The last line takes the newline before it, leaving no empty line.
            let line_start = if line_end == units.len()
                && units.get(line_end.wrapping_sub(1)) != Some(&u16::from(b'\n'))
            {
                line_start.saturating_sub(1)
            } else {
                line_start
            };
            let (updated, _) = replace_selection(&text, line_start, line_end, "");
            commit_edit(
                updated,
                line_start,
                None,
                text,
                (start, end),
                source,
                cursor,
                current_line,
                history,
                on_change,
                Some(element),
                scroll_y,
                true,
            );
        }
    };
    // Pasting a line copied with nothing selected puts it above the caret's
    // line and keeps the caret on its text, as in VS Code.
    let paste_line = move |event: web_sys::ClipboardEvent| {
        let (Some(element), Some(clipboard)) = (editor_ref.get(), event.clipboard_data()) else {
            return;
        };
        let (start, end) = textarea_selection(&element);
        let Ok(pasted) = clipboard.get_data("text/plain") else {
            return;
        };
        if start != end
            || read_only.get_untracked()
            || copied_line.with_value(|line| line.as_deref() != Some(pasted.as_str()))
        {
            return;
        }
        event.prevent_default();
        let text = source.get_untracked();
        let (line_start, _) = line_range(&text, start);
        let (updated, _) = replace_selection(&text, line_start, line_start, &pasted);
        commit_edit(
            updated,
            start + pasted.encode_utf16().count(),
            None,
            text,
            (start, end),
            source,
            cursor,
            current_line,
            history,
            on_change,
            Some(element),
            scroll_y,
            true,
        );
    };
    let apply_selected = move || {
        let options = suggestions.get();
        if let Some(completion) = options.get(active_completion.get()) {
            let (updated, anchor, new_cursor) =
                apply_completion(&source.get_untracked(), completion);
            auto_closers.set_value(inserted_closers(completion, &updated, new_cursor));
            placeholder.set_value((anchor != new_cursor).then_some((anchor, new_cursor)));
            let before = source.get_untracked();
            let request_completions =
                accept_reopens_suggestions(completion, auto_trigger_completions);
            hide_hover();
            commit_edit(
                updated,
                new_cursor,
                Some(anchor),
                before,
                (cursor.get_untracked(), cursor.get_untracked()),
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
            // Bring the diagnostic into view, as F8 does in VS Code.
            set_caret_visible(&editor, &text, offset, scroll_y);
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
    // Only the rows in view (plus overscan) exist in the DOM; a spacer above
    // them keeps their positions, so large documents stay fast to edit.
    let visible_rows = Memo::new(move |_| {
        popup_layout_revision.track();
        let top = scroll_y.get();
        // From the text, not `lines`: a second reader of `lines` here kept the
        // row sync from seeing diagnostic-only changes to it.
        let count = source.with(|text| text.bytes().filter(|byte| *byte == b'\n').count() + 1);
        let Some(editor) = editor_ref.get() else {
            return 0..count.min(INITIAL_ROWS);
        };
        let metrics = read_editor_metrics(&editor);
        let window_height = web_sys::window()
            .and_then(|window| window.inner_height().ok())
            .and_then(|height| height.as_f64())
            .unwrap_or(0.0);
        let height = f64::from(editor.client_height()).max(window_height);
        let line_height = metrics.line_height.max(1.0);
        let first = ((top - metrics.padding_top) / line_height).floor().max(0.0) as usize;
        let first = first.saturating_sub(OVERSCAN_ROWS).min(count);
        let last = (first + (height / line_height).ceil() as usize + 2 * OVERSCAN_ROWS).min(count);
        first..last
    });
    Effect::new(move |_| {
        let line = current_line.get();
        let range = visible_rows.get();
        lines.with(|lines| diff.with(|diff| rows_for_sync.sync(lines, diff, line, range)));
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

    // Clipboard and composition listeners are added here rather than in the
    // view: a tachys element holds at most 26 attributes.
    Effect::new(move |_| {
        use wasm_bindgen::closure::Closure;
        let Some(element) = editor_ref.get() else {
            return;
        };
        let clipboard = |handler: Box<dyn Fn(web_sys::ClipboardEvent)>| {
            Closure::<dyn Fn(web_sys::ClipboardEvent)>::wrap(handler)
        };
        let listeners = [
            (
                "copy",
                clipboard(Box::new(move |event| copy_line(event, false))),
            ),
            (
                "cut",
                clipboard(Box::new(move |event| copy_line(event, true))),
            ),
            ("paste", clipboard(Box::new(paste_line))),
            (
                "compositionend",
                clipboard(Box::new(move |_| history.update(EditHistory::seal))),
            ),
        ];
        for (name, listener) in &listeners {
            let _ =
                element.add_event_listener_with_callback(name, listener.as_ref().unchecked_ref());
        }
        // Browser objects stay on the one wasm thread.
        let registered = StoredValue::new_local((element, listeners));
        on_cleanup(move || {
            registered.with_value(|(element, listeners)| {
                for (name, listener) in listeners {
                    let _ = element.remove_event_listener_with_callback(
                        name,
                        listener.as_ref().unchecked_ref(),
                    );
                }
            });
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
            style=move || colors.get().map(|colors| colors.to_style())
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
            <section class="config-editor__frame" aria-label=move || format!("{} editor", label())>
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
                            // Wide enough for the longest line number beside the
                            // diagnostic dot (52px fits four digits).
                            style:width=move || {
                                let digits = source.with(|text| text.bytes().filter(|byte| *byte == b'\n').count() + 1).to_string().len();
                                (digits > 4).then(|| format!("calc(52px + {}ch)", digits - 4))
                            }
                        >
                            <div
                                class="config-editor__gutter-content"
                                style:transform=move || format!("translateY(-{}px)", scroll_y.get())
                            >
                            <span
                                class="config-editor__row-spacer"
                                style:height=move || format!("calc({} * var(--config-editor-line-height))", visible_rows.with(|rows| rows.start))
                            ></span>
                            <For
                                each=move || visible_rows.get()
                                key=|index| *index
                                children=move |index| {
                                    let line = index + 1;
                                    // Subscribe only to the row's own signal (see `line_rows`).
                                    let active = current_line.get_untracked();
                                    // Borrow, never clone: cloning the whole document per new row made
                                    // large pastes quadratic.
                                    let state = lines.with_untracked(|lines| {
                                        diff.with_untracked(|diff| rows_for_gutter.state_for(index, lines, diff, active))
                                    });
                                    let has_error = move || state.with(|row| row.has_error);
                                    let is_added = move || state.with(|row| row.change == DiffKind::Added);
                                    let is_modified = move || state.with(|row| row.change == DiffKind::Modified);
                                    let modified_peek = move || state.with(|row| row.modified_peek.clone());
                                    let deleted_peek = move || state.with(|row| row.deleted_peek.clone());
                                    let deleted_after_peek = move || state.with(|row| row.deleted_after_peek.clone());
                                    let is_active = move || state.with(|row| row.is_active);
                                    view! {
                                            <div
                                                data-line=line
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
                                    <span
                                        class="config-editor__row-spacer"
                                        style:height=move || format!("calc({} * var(--config-editor-line-height))", visible_rows.with(|rows| rows.start))
                                    ></span>
                                    <For
                                        each=move || visible_rows.get()
                                        key=|index| *index
                                        children=move |index| {
                                            // Subscribe only to the row's own signal (see `line_rows`).
                                            let active = current_line.get_untracked();
                                            // Borrow, never clone: cloning the whole document per new row made
                                            // large pastes quadratic.
                                            let state = lines.with_untracked(|lines| {
                                                diff.with_untracked(|diff| rows_for_highlight.state_for(index, lines, diff, active))
                                            });
                                            let row_text = move || state.with(|row| row.text.clone());
                                            let has_error = move || state.with(|row| row.has_error);
                                            let is_added = move || state.with(|row| row.change == DiffKind::Added);
                                            let is_modified = move || state.with(|row| row.change == DiffKind::Modified);
                                            let is_active = move || state.with(|row| row.is_active);
                                            view! {
                                                <span
                                                    class="config-editor__code-line"
                                                    data-line=index + 1
                                                    class:config-editor__code-line-active=is_active
                                                    class:config-editor__code-line-error=has_error
                                                    class:config-editor__code-line-added=is_added
                                                    class:config-editor__code-line-modified=is_modified
                                                >
                                                    {move || {
                                                        let text = row_text();
                                                        if enable_highlighting {
                                                            highlight::render(&text, &edit_language.get().behavior().highlight(&text))
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
                                aria-label=move || format!("Inline {} diagnostics", label())
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
                                        let before: String = line_text.chars().take(column.saturating_sub(1)).collect();
                                        let visual_left = visual_column(&before);
                                        let visual_width = visual_column(&format!("{before}{marked_text}")) - visual_left;
                                        Some(view! {
                                            <button
                                                type="button"
                                                class="config-editor__diagnostic-marker"
                                                class:config-editor__diagnostic-marker-warning=diagnostic.severity == Severity::Warning
                                                class:config-editor__diagnostic-marker-eof=is_eof
                                                // Near the top of the viewport (not of the document),
                                                // the message opens below its line.
                                                class:config-editor__diagnostic-marker-near-top=move || {
                                                    let line_height = editor_ref
                                                        .get_untracked()
                                                        .map_or(22.0, |editor| read_editor_metrics(&editor).line_height);
                                                    (line as f64 - 1.0) * line_height - scroll_y.get() < 3.0 * line_height
                                                }
                                                tabindex="-1"
                                                aria-label=format!(
                                                    "{} {} on line {line}: {}",
                                                    label(),
                                                    if diagnostic.severity == Severity::Warning { "warning" } else { "error" },
                                                    diagnostic.message
                                                )
                                                data-message=diagnostic.message
                                                // Rendered columns, so tabs (two columns wide) do not
                                                // shift the underline.
                                                style=format!(
                                                    "left: {}ch; top: calc(var(--config-editor-padding-top) + {} * var(--config-editor-line-height)); width: {}ch;",
                                                    visual_left,
                                                    line.saturating_sub(1),
                                                    visual_width.max(1.0),
                                                )
                                                // Shifted left so the message stays inside the visible text area.
                                                style:--config-editor-diagnostic-tooltip-shift=move || {
                                                    let scroll = scroll_x.get();
                                                    let Some(editor) = editor_ref.get_untracked() else {
                                                        return "0px".to_string();
                                                    };
                                                    let metrics = read_editor_metrics(&editor);
                                                    let visible = f64::from(editor.client_width());
                                                    let left = metrics.padding_left
                                                        + visual_left * metrics.char_width
                                                        - scroll;
                                                    // As wide as `max-width` in preset.css allows.
                                                    let window_width = web_sys::window()
                                                        .and_then(|window| window.inner_width().ok())
                                                        .and_then(|width| width.as_f64())
                                                        .unwrap_or(visible);
                                                    let tooltip = DIAGNOSTIC_TOOLTIP_MAX_WIDTH.min(window_width - 32.0);
                                                    let shift = (visible - 8.0 - left - tooltip).min(0.0).max(8.0 - left);
                                                    format!("{shift}px")
                                                }
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
                                aria-label=move || format!("{} source", label())
                                aria-autocomplete=move || (!read_only.get()).then_some("list")
                                aria-haspopup=move || (!read_only.get()).then_some("listbox")
                                aria-expanded=move || (!suggestions.get().is_empty()).to_string()
                                aria-invalid=move || (!diagnostics.get().is_empty()).to_string()
                                node_ref=editor_ref
                                spellcheck="false"
                                autocapitalize="off"
                                autocomplete="off"
                                wrap="off"
                                // Read-only cancels edits in `beforeinput` instead of
                                // setting `readonly`, which in Chromium also stops the
                                // caret: keyboard navigation and selection keep working.
                                aria-readonly=move || read_only.get().to_string()
                                inputmode=move || read_only.get().then_some("none")
                                prop:value=move || source.get()
                                on:beforeinput=move |event: web_sys::InputEvent| {
                                    if read_only.get_untracked() {
                                        event.prevent_default();
                                        return;
                                    }
                                    if let Some(element) = event
                                        .target()
                                        .and_then(|target| target.dyn_into::<web_sys::HtmlTextAreaElement>().ok())
                                    {
                                        pre_edit_selection.set_value(textarea_selection(&element));
                                    }
                                }
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
                                    if matches!(
                                        input_type.as_str(),
                                        "insertFromPaste" | "insertFromDrop" | "deleteByCut" | "deleteByDrag"
                                    ) {
                                        auto_closers.update_value(Vec::clear);
                                    }
                                    // Backspace with the list open refilters it instead of closing.
                                    let refilter = input_type == "deleteContentBackward"
                                        && suggestions_requested.get_untracked()
                                        && !suggestions.with_untracked(Vec::is_empty);
                                    placeholder.set_value(None);
                                    let after = event
                                        .target()
                                        .and_then(|target| target.dyn_into::<web_sys::HtmlTextAreaElement>().ok())
                                        .map(|element| textarea_selection(&element));
                                    if before != value {
                                        history.update(|history| {
                                            history.record_input(
                                                Snapshot {
                                                    value: before,
                                                    selection: pre_edit_selection.get_value(),
                                                },
                                                after.unwrap_or_default(),
                                                &input_type,
                                                event.time_stamp(),
                                            );
                                        });
                                    }
                                    if let Some((position, _)) = after {
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
                                        behavior(),
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
                                    auto_closers.update_value(Vec::clear);
                                    history.update(EditHistory::seal);
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
                                        if position != cursor.get_untracked() {
                                            cursor.set(position);
                                            current_line.set(line_at_cursor(&source.get_untracked(), position));
                                        }
                                        // Caret navigation never shows suggestions. ArrowUp/Down
                                        // move inside an open list instead (handled on keydown).
                                        let navigation = match event.key().as_str() {
                                            "ArrowLeft" | "ArrowRight" | "Home" | "End" => true,
                                            "ArrowUp" | "ArrowDown" | "PageUp" | "PageDown" => {
                                                suggestions.with_untracked(Vec::is_empty)
                                            }
                                            _ => false,
                                        };
                                        if navigation {
                                            auto_closers.update_value(Vec::clear);
                                            history.update(EditHistory::seal);
                                        }
                                        if navigation && suggestions_requested.get_untracked() {
                                            suggestions_requested.set(false);
                                            explicit_request.set(false);
                                            active_completion.set(0);
                                        }
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
                                    // Smart Home: the first non-blank character, then the line start.
                                    if event.key() == "Home"
                                        && !event.ctrl_key()
                                        && !event.meta_key()
                                        && !event.alt_key()
                                        && let Some(element) = editor_ref.get()
                                    {
                                        let (start, end) = textarea_selection(&element);
                                        let backward = element.selection_direction().ok().flatten().as_deref() == Some("backward");
                                        let (anchor, caret) = if backward { (end, start) } else { (start, end) };
                                        let target = source.with_untracked(|text| smart_home(text, caret));
                                        event.prevent_default();
                                        if event.shift_key() {
                                            let direction = if target < anchor { "backward" } else { "forward" };
                                            let _ = element.set_selection_range_with_direction(
                                                anchor.min(target) as u32,
                                                anchor.max(target) as u32,
                                                direction,
                                            );
                                        } else {
                                            let _ = element.set_selection_range(target as u32, target as u32);
                                        }
                                        cursor.set(target);
                                        current_line.set(source.with_untracked(|text| line_at_cursor(text, target)));
                                        return;
                                    }
                                    if let Some((start, end)) = placeholder.get_value()
                                        && matches!(event.key().as_str(), "Enter" | "Tab")
                                        && !event.shift_key()
                                        && let Some(element) = editor_ref.get()
                                    {
                                        placeholder.set_value(None);
                                        if textarea_selection(&element) == (start, end) {
                                            // Keep the default. Tab steps past it (and its closing
                                            // quote); Enter goes on from the end of its line, so a
                                            // string or element is never split.
                                            let end = source.with_untracked(|text| {
                                                let byte = utf16_to_byte_index(text, end);
                                                let rest = &text[byte..];
                                                let skip = if event.key() == "Enter" {
                                                    rest.find('\n').unwrap_or(rest.len())
                                                } else {
                                                    usize::from(rest.starts_with(['"', '\'']))
                                                };
                                                end + rest[..skip].encode_utf16().count()
                                            });
                                            let _ = element.set_selection_range(end as u32, end as u32);
                                            cursor.set(end);
                                            suggestions_requested.set(false);
                                            if event.key() == "Tab" {
                                                event.prevent_default();
                                                return;
                                            }
                                        }
                                    }
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
                                        auto_closers.update_value(Vec::clear);
                                        let current = source.get_untracked();
                                        let mut restored = None;
                                        history.update(|history| {
                                            restored = if is_undo {
                                                history.undo(current)
                                            } else {
                                                history.redo(current)
                                            };
                                        });
                                        if let Some(restored) = restored {
                                            let (start, end) = restored.selection;
                                            source.set(restored.value.clone());
                                            cursor.set(end);
                                            current_line.set(line_at_cursor(&restored.value, end));
                                            if let Some(element) = editor_ref.get() {
                                                element.set_value(&restored.value);
                                                set_caret_visible(&element, &restored.value, end, scroll_y);
                                                let _ = element.set_selection_range(start as u32, end as u32);
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
                                                behavior().toggle_comments(&before, start, end)
                                            {
                                                if updated != before {
                                                    commit_edit(
                                                        updated,
                                                        selection_end,
                                                        Some(selection_start),
                                                        before,
                                                        (start, end),
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
                                                let distance = source.with_untracked(|text| text.encode_utf16().count()) - start;
                                                // Typed over: closers the editor wrote, and the quote
                                                // that ends the string the caret is in.
                                                if let Some(closing) = closing
                                                    && (auto_closers.with_value(|closers| closers.contains(&distance))
                                                        || matches!(closing, '"' | '\'')
                                                            && behavior().lexical_state(&source.get_untracked(), start).quote
                                                                == Some(closing))
                                                    && let Some(next_cursor) = skip_closing_pair(
                                                        &source.get_untracked(),
                                                        start,
                                                        closing,
                                                    )
                                                {
                                                    event.prevent_default();
                                                    auto_closers.update_value(|closers| closers.retain(|closer| *closer != distance));
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
                                                    let distance = before.encode_utf16().count() - start;
                                                    auto_closers.update_value(|closers| closers.retain(|closer| *closer != distance));
                                                    commit_edit(
                                                        updated,
                                                        new_cursor,
                                                        None,
                                                        before,
                                                        (start, end),
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

                                            if start == end
                                                && key.chars().count() == 1
                                                && let Some(typed) = key.chars().next()
                                                && let Some(end_tag) = behavior().auto_close(&source.get_untracked(), start, typed)
                                            {
                                                event.prevent_default();
                                                let before = source.get_untracked();
                                                let (updated, _) = replace_selection(&before, start, end, &format!("{typed}{end_tag}"));
                                                let caret = start + typed.len_utf16();
                                                commit_edit(
                                                    updated,
                                                    caret,
                                                    None,
                                                    before,
                                                    (start, end),
                                                    source,
                                                    cursor,
                                                    current_line,
                                                    history,
                                                    on_change,
                                                    Some(element.clone()),
                                                    scroll_y,
                                                    false,
                                                );
                                                suggestions_requested.set(false);
                                                return;
                                            }

                                            if key.chars().count() == 1
                                                && let Some(opening) = key.chars().next()
                                                && let Some((updated, selection_start, selection_end)) =
                                                    paired_edit(
                                                        &source.get_untracked(),
                                                        start,
                                                        end,
                                                        opening,
                                                        behavior(),
                                                    )
                                            {
                                                event.prevent_default();
                                                // A prevented keydown fires no input event, so open
                                                // suggestions here when the pair starts an entry.
                                                let opens_entry = selection_start == selection_end
                                                    && behavior().entry_started(&updated, selection_start);
                                                let distance = updated.encode_utf16().count() - selection_end;
                                                auto_closers.update_value(|closers| closers.push(distance));
                                                commit_edit(
                                                    updated,
                                                    selection_end,
                                                    Some(selection_start),
                                                    source.get_untracked(),
                                                    (start, end),
                                                    source,
                                                    cursor,
                                                    current_line,
                                                    history,
                                                    on_change,
                                                    Some(element.clone()),
                                                    scroll_y,
                                                    false,
                                                );
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
                                            ")" => typed_closer(&text, start, ')'),
                                            _ => None,
                                        };
                                        if let Some((updated, new_cursor)) = edit {
                                            event.prevent_default();
                                            commit_edit(
                                                updated,
                                                new_cursor,
                                                None,
                                                text,
                                                (start, end),
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
                                    // Accepting what is already typed would change nothing,
                                    // so Enter and Tab close the list and edit as usual.
                                    let unchanged_caret = options.get(active_completion.get_untracked()).and_then(|option| {
                                        source.with_untracked(|text| {
                                            let (updated, _, caret) = apply_completion(text, option);
                                            (updated == *text).then_some(caret)
                                        })
                                    });
                                    let options = match unchanged_caret {
                                        Some(caret) if matches!(event.key().as_str(), "Enter" | "Tab") => {
                                            // Step past the value (`"balance|"` → `"balance"|`) so
                                            // Enter starts a new line instead of splitting it.
                                            if let Some(element) = editor_ref.get() {
                                                let _ = element.set_selection_range(caret as u32, caret as u32);
                                                cursor.set(caret);
                                            }
                                            suggestions_requested.set(false);
                                            if event.key() == "Tab" {
                                                event.prevent_default();
                                                return;
                                            }
                                            Vec::new()
                                        }
                                        _ => options,
                                    };
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
                                                // A selection within one line is replaced, as typing would.
                                                let multi_line = start != end
                                                    && text
                                                        .encode_utf16()
                                                        .skip(start)
                                                        .take(end - start)
                                                        .any(|unit| unit == u16::from(b'\n'));
                                                if event.shift_key() || multi_line {
                                                    if let Some((updated, selection_start, selection_end)) =
                                                        indent_lines(&text, start, end, event.shift_key())
                                                    {
                                                        if updated != text {
                                                            commit_edit(
                                                                updated,
                                                                selection_end,
                                                                Some(selection_start),
                                                                text,
                                                                (start, end),
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
                                                    replace_selection(&text, start, end, &tab_insert(&text, start));
                                                commit_edit(
                                                    updated,
                                                    new_cursor,
                                                    None,
                                                    text,
                                                    (start, end),
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
                                                let (mut start, mut end) = textarea_selection(&element);
                                                let text = source.get_untracked();
                                                let line_start_of = |offset: usize| {
                                                    let byte = utf16_to_byte_index(&text, offset);
                                                    text[..text[..byte].rfind('\n').map_or(0, |index| index + 1)].encode_utf16().count()
                                                };
                                                let line_end_of = |offset: usize| {
                                                    let byte = utf16_to_byte_index(&text, offset);
                                                    text[..text[byte..].find('\n').map_or(text.len(), |index| byte + index)].encode_utf16().count()
                                                };
                                                // Ctrl+Enter opens a line below without splitting this one.
                                                if event.ctrl_key() || event.meta_key() {
                                                    start = line_end_of(end);
                                                    end = start;
                                                }
                                                let newline = behavior().newline(&text, start);
                                                // A line of only indentation is left empty, not with trailing spaces.
                                                let line_start = line_start_of(start);
                                                let blank_before = text
                                                    .encode_utf16()
                                                    .skip(line_start)
                                                    .take(start - line_start)
                                                    .all(|unit| unit == u16::from(b' ') || unit == u16::from(b'\t'));
                                                let start = if start == end && start > line_start && blank_before && line_end_of(end) == end {
                                                    line_start
                                                } else {
                                                    start
                                                };
                                                // `replaces` and `caret` are bytes; the cursor is in UTF-16 units.
                                                let end_byte = utf16_to_byte_index(&text, end);
                                                let replaced_end = end + text[end_byte..(end_byte + newline.replaces).min(text.len())]
                                                    .encode_utf16()
                                                    .count();
                                                let (updated, _) = replace_selection(&text, start, replaced_end, &newline.text);
                                                let new_cursor = start + newline.text[..newline.caret].encode_utf16().count();
                                                commit_edit(
                                                    updated,
                                                    new_cursor,
                                                    None,
                                                    text,
                                                    (start, end),
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
                                aria-label=move || format!("{} suggestions", label())
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
                                                let (updated, anchor, new_cursor) = apply_completion(&source.get_untracked(), &apply);
                                                auto_closers.set_value(inserted_closers(&apply, &updated, new_cursor));
                                                placeholder.set_value((anchor != new_cursor).then_some((anchor, new_cursor)));
                                                let request_completions = accept_reopens_suggestions(&apply, auto_trigger_completions);
                                                hide_hover();
                                                commit_edit(
                                                    updated,
                                                    new_cursor,
                                                    Some(anchor),
                                                    source.get_untracked(),
                                                    (cursor.get_untracked(), cursor.get_untracked()),
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
