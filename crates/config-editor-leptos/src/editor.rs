//! The [`ConfigEditor`] component. Its parts live in submodules:
//!
//! - `state`: the shared [`EditorState`] handle every part reads and writes.
//! - `sync`: the controlled value and Edit-as conversion and switching.
//! - `keys`, `input`: keyboard and other textarea events.
//! - `components`: the view, one component per part of the screen.
//! - The rest is the editing core the parts call into: languages, schema
//!   completion and diagnostics, conversion, diff, history, commands.

use leptos::{ev, leptos_dom::helpers::window_event_listener, prelude::*};

mod bridge;
mod changes;
mod codec;
mod commands;
mod comment_scope;
mod completion;
mod components;
mod diagnostics;
mod diff;
mod folding;
mod go_to_line;
mod handle;
mod highlight;
mod history;
mod host_diagnostics;
mod host_editing;
mod inline;
mod input;
pub use input::flush_pending_input;
pub use sync::flush_pending_edits;
mod keyboard;
mod keys;
pub mod language;
mod languages;
mod line_rows;
mod metrics;
mod paths;
mod positioning;
mod search;
mod state;
mod sync;
mod theme;

pub use changes::{ChangeSummary, unified_diff};
pub use codec::{Conversion, ConvertError};
pub use components::{ConfigEditorFooter, ConfigEditorToolbar};
pub use diagnostics::{
    Diagnostic, DiagnosticOrigin, Schema, SchemaError, SchemaSource, Severity, check_syntax,
};
pub use handle::EditorHandle;
pub use host_diagnostics::{DiagnosticTarget, HostDiagnostic};
pub use host_editing::{BaselineConflict, SaveRequest};
pub use language::ParseError;
pub use languages::{Language, UnknownLanguage};
pub use paths::{FieldPath, TextPosition, TextRange};
pub use theme::{EditorTheme, ThemeColor, ThemeColors, UnknownTheme};

use components::{
    BaselineNotice, CodeLayer, CompletionList, DiagnosticOverlay, DiffPeekPopup, FindWidget,
    GoToLineWidget, Gutter, HoverCardView, SearchHighlights, SourceInput, StatusBar, Toolbar,
};
use state::{EditorState, Features, Init, normalize_newlines};

/// The preset stylesheet: layout plus every bundled theme, and the tabs of
/// [`ConfigDocuments`](crate::ConfigDocuments).
///
/// The editor injects it unless `styled=false`, in which case a host can serve
/// it itself.
pub const PRESET_CSS: &str = concat!(
    include_str!("preset.css"),
    include_str!("themes.css"),
    include_str!("documents.css")
);

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
    /// Text to diff against, in `language`: the version the host last saved
    /// or loaded. Set it to the saved text after saving to clear the diff.
    /// Unset, it is the document at mount (`value`, else `initial_value`).
    ///
    /// A new baseline while the user has unsaved edits (the text differs
    /// from the current baseline) opens a notice instead of mixing them:
    /// "Reload" makes it the text, "Keep mine" keeps the text and diffs it
    /// against the new baseline. A baseline equal to the text, or to a text
    /// the editor sent (`on_change`, `on_save`), never asks.
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
    /// Called with how the document differs from `baseline` (lines added,
    /// removed and modified, and the number of separate changes): once on
    /// mount and after each edit or baseline update that changes it. Counts
    /// what the diff gutter shows, so it stays at zero with
    /// `show_diff=false`.
    #[prop(optional, into)]
    on_change_summary: Option<Callback<ChangeSummary>>,
    /// Called once per problem with something in `schema` the editor cannot
    /// use (see [`Schema::warnings`]), such as a `patternProperties` regex
    /// its engine cannot compile. Without it, each goes to the browser
    /// console as a warning.
    #[prop(optional, into)]
    on_schema_warning: Option<Callback<String>>,
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
    /// Find and replace: `Ctrl+F`, `Ctrl+H`, `F3`. When `false`, those keys
    /// are left to the browser.
    #[prop(default = true)]
    enable_search: bool,
    /// Go to line (`Ctrl+G`). When `false`, the key is left to the browser.
    #[prop(default = true)]
    enable_go_to_line: bool,
    /// Folding by structure: chevrons in the gutter, `Ctrl+Shift+[` and
    /// `Ctrl+Shift+]` at the caret, `Ctrl+K Ctrl+0` and `Ctrl+K Ctrl+J` for
    /// everything (`Cmd` on macOS). View only: the text stays whole.
    #[prop(default = true)]
    enable_folding: bool,
    /// Disable editing while keeping highlighting, diagnostics, diffs, and
    /// hover docs.
    #[prop(default = Signal::from(false), into)]
    read_only: Signal<bool>,
    /// A handle the host keeps to find fields, follow the caret and jump to
    /// a field; see [`EditorHandle`].
    #[prop(optional)]
    handle: Option<EditorHandle>,
    /// Problems the host found (a server's validation), by path or range,
    /// shown next to the editor's own and marked as the host's. A new list
    /// replaces the previous one; only the markers update. See
    /// [`HostDiagnostic`].
    #[prop(optional, into)]
    host_diagnostics: Option<Signal<Vec<HostDiagnostic>>>,
    /// Called with the path of what the caret is on each time it changes
    /// (and once on mount), for a breadcrumb: `/web/listen`, or the root at
    /// the top level.
    #[prop(optional, into)]
    on_path_change: Option<Callback<FieldPath>>,
    /// Called when the user saves (`Ctrl+S` / `Cmd+S`, the browser's save
    /// dialog prevented) or the host calls [`EditorHandle::save`], with the
    /// document in `language`. Nothing saves by itself. Without it, `Ctrl+S`
    /// is left to the browser. See [`SaveRequest`] for an edit that cannot
    /// convert back yet.
    #[prop(optional, into)]
    on_save: Option<Callback<SaveRequest>>,
    /// Whether `Ctrl+S` / `Cmd+S` saves (with `on_save` set). When `false`,
    /// the key is left to the browser and only [`EditorHandle::save`]
    /// saves. May be a signal.
    #[prop(default = Signal::from(true), into)]
    enable_save_shortcut: Signal<bool>,
    /// Called with whether the text differs from `baseline`: once on mount,
    /// then each time that changes (not on every keystroke).
    #[prop(optional, into)]
    on_dirty_change: Option<Callback<bool>>,
    /// Called when a new `baseline` arrives under unsaved edits and the
    /// notice opens, and again when it closes; see [`BaselineConflict`].
    #[prop(optional, into)]
    on_baseline_conflict: Option<Callback<BaselineConflict>>,
) -> impl IntoView {
    let (schema, schema_error) = match schema.map(SchemaSource::resolve) {
        Some(Ok(schema)) => (Some(schema), None),
        Some(Err(error)) => (None, Some(error)),
        None => (None, None),
    };
    let has_schema = schema.is_some();
    if let Some(schema) = &schema {
        report_schema_warnings(&schema.warnings, on_schema_warning);
    }
    let initial_value = normalize_newlines(initial_value.unwrap_or_default());
    let host_text = value
        .as_ref()
        .map(Signal::get_untracked)
        .map_or_else(|| initial_value.clone(), normalize_newlines);
    let baseline = baseline.unwrap_or_else(|| Signal::stored(host_text.clone()));
    let features = Features {
        // `read_only` switches live; the parts that offer completions read
        // it as well.
        completions: enable_completions,
        auto_trigger: auto_trigger_completions,
        hover_docs: enable_hover_docs && has_schema,
        highlighting: enable_highlighting,
        auto_closing: enable_auto_closing,
        diagnostics: show_diagnostics,
        diff: show_diff,
        search: enable_search,
        go_to_line: enable_go_to_line,
        folding: enable_folding,
    };
    let (state, initial_pending) = EditorState::new(Init {
        host_language: language,
        edit_language: edit_as.get_untracked().unwrap_or(language),
        schema,
        schema_error,
        host_text,
        baseline: baseline.get_untracked(),
        read_only,
        features,
        host_on_change: on_change,
        host_diagnostics: host_diagnostics.unwrap_or_default(),
        on_save,
        save_shortcut: enable_save_shortcut,
        on_baseline_conflict,
    });
    sync::connect(
        state,
        sync::Host {
            value,
            edit_as,
            on_edit_as_change,
            initial_pending,
        },
    );
    host_editing::connect(
        state,
        host_editing::Host {
            baseline,
            on_dirty_change,
        },
    );
    if let Some(callback) = on_change_summary {
        changes::report_changes(state, callback);
    }
    input::attach_listeners(state);
    if let Some(handle) = handle {
        handle.attach(state);
    }
    if let Some(on_path_change) = on_path_change {
        Effect::new(move |_| on_path_change.run(state.caret_path.get()));
    }
    remeasure_on_window_changes(state);
    let rows = components::row_states(state);
    if enable_folding {
        components::mount_fold_layers(state);
    }
    let file_name = file_name
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| language.default_file_name().to_string());
    let root_class = class.unwrap_or_default();

    view! {
        <div
            class=move || {
                let read_only = if read_only.get() { " config-editor__readonly" } else { "" };
                format!("config-editor {root_class}{read_only}")
            }
            data-theme=move || theme.get().css_family()
            data-color-theme=move || theme.get().as_str()
            style=move || colors.get().map(|colors| colors.to_style())
            node_ref=state.root_ref
            on:keydown=move |event| keys::on_root_keydown(state, &event)
        >
            {styled.then(|| view! { <style>{PRESET_CSS}</style> })}
            <section class="config-editor__frame" aria-label=move || format!("{} editor", state.label())>
                {show_toolbar.then(|| view! { <Toolbar state=state file_name=file_name has_schema=has_schema /> })}
                <BaselineNotice state=state />
                <div class="config-editor__surface">
                    <Gutter state=state rows=rows.clone() />
                    <div class="config-editor__input-wrap">
                        <CodeLayer state=state rows=rows />
                        <DiagnosticOverlay state=state />
                        {enable_search.then(|| view! { <SearchHighlights state=state /> })}
                        <SourceInput state=state />
                        <HoverCardView state=state />
                        <CompletionList state=state />
                        {enable_search.then(|| view! { <FindWidget state=state /> })}
                        {enable_go_to_line.then(|| view! { <GoToLineWidget state=state /> })}
                    </div>
                </div>
                {show_footer.then(|| view! { <StatusBar state=state /> })}
            </section>
            <DiffPeekPopup state=state />
        </div>
    }
}

/// Hands each schema warning to the host, or to the console without a
/// callback, so a schema feature the editor ignores is never silent.
fn report_schema_warnings(warnings: &[String], on_schema_warning: Option<Callback<String>>) {
    for warning in warnings {
        match on_schema_warning {
            Some(callback) => callback.run(warning.clone()),
            None => web_sys::console::warn_1(&format!("config-editor: {warning}").into()),
        }
    }
}

/// Window resizes and page scrolls move the editor, so popups re-measure.
fn remeasure_on_window_changes(state: EditorState) {
    let bump = move || state.layout_revision.update(|revision| *revision += 1);
    let resize = window_event_listener(ev::resize, move |_| bump());
    let scroll = window_event_listener(ev::scroll, move |_| bump());
    on_cleanup(move || {
        resize.remove();
        scroll.remove();
    });
}

#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod save_properties;
#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod save_tests;
#[cfg(test)]
mod tests;
