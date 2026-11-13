//! The [`ConfigEditor`] component. Its parts live in submodules:
//!
//! - `state`: the shared [`EditorState`] handle every part reads and writes.
//! - `sync`: the controlled value and Edit-as conversion and switching.
//! - `keys`, `input`: keyboard and other textarea events.
//! - `components`: the view, one component per part of the screen.
//! - The rest is the editing core the parts call into: languages, schema
//!   completion and diagnostics, conversion, diff, history, commands.

use leptos::prelude::*;

use crate::reactive::{Kept, size_observer, window_listener};

mod a11y;
mod bridge;
mod byte_order_mark;
mod caret_tip;
mod change_summary;
#[cfg_attr(not(feature = "diff"), path = "editor/off/changes.rs")]
mod changes;
mod codec;
mod commands;
mod comment_scope;
mod completion;
mod components;
mod diagnostics;
mod diff;
mod document;
mod drag_move;
mod edit;
mod edit_as;
#[cfg(feature = "schema-validation")]
mod false_schema;
mod fill;
mod flat_keys;
mod flow_syntax;
mod fold_ranges;
#[cfg_attr(not(feature = "folding"), path = "editor/off/folding.rs")]
mod folding;
mod handle;
mod highlight;
mod history;
mod host_diagnostics;
mod host_editing;
mod hover;
mod hover_focus;
mod hover_path;
mod hover_problems;
mod input;
mod kept_focus;
pub use input::flush_pending_input;
pub use sync::flush_pending_edits;
mod keyboard;
mod keys;
pub mod language;
mod languages;
mod layout;
mod line_breaks;
mod line_endings;
mod line_rows;
mod live_settings;
mod metrics;
mod outside_press;
mod parse_cache;
mod paths;
mod pattern_engine;
mod popup_pointer;
mod popup_under;
mod positioning;
mod problem_hit;
mod problem_walk;
mod problems;
mod projection;
#[cfg_attr(not(feature = "protect"), path = "editor/off/protect.rs")]
mod protect;
mod readings;
#[cfg_attr(not(feature = "search"), path = "editor/off/search.rs")]
mod search;
mod secret_options;
mod source_area;
mod state;
mod sync;
// Used by the tests that mount a YAML editor (all of them need YAML).
#[cfg(all(test, feature = "yaml"))]
mod test_state;
mod text_change;
mod theme;
mod touch;
mod typing;
pub(crate) mod work_count;

pub use change_summary::ChangeSummary;
#[cfg(feature = "diff")]
pub use changes::unified_diff;
pub use codec::{Conversion, ConvertError, float_of, float_value, string_of, string_value};
pub use completion::{HoverDocs, Suggestion};
pub use components::{ConfigEditorFooter, ConfigEditorToolbar};
pub use diagnostics::{
    Diagnostic, DiagnosticOrigin, Schema, SchemaError, SchemaSource, Severity, check_syntax,
};
pub use handle::EditorHandle;
pub use host_diagnostics::{DiagnosticTarget, HostDiagnostic, is_web_url};
pub use host_editing::{BaselineConflict, SaveRequest};
pub use language::ParseError;
pub use languages::{Language, UnknownLanguage};
pub use paths::{FieldPath, TextPosition, TextRange};
pub use secret_options::SecretOptions;
pub use theme::{EditorTheme, ThemeColor, ThemeColors, UnknownTheme};

use crate::EditorSettings;
use crate::cssom::cssom;
use changes::DiffPeekPopup;
use components::{
    Announcer, BaselineNotice, CodeLayer, CompletionList, DiagnosticOverlay, EditAsNotice, Gutter,
    HoverCardView, ProblemTip, ReadOnlyNotice, SourceInput, StatusBar, Toolbar,
};
use fill::FillProbe;
use positioning::frame_probe_style;
use protect::ProtectTipView;
use search::{FindWidget, GoToLineWidget, SearchHighlights};
use state::{EditorState, Init};

/// The preset stylesheet: the base styles, every bundled theme, and the tabs
/// of [`ConfigDocuments`](crate::ConfigDocuments) (the parts in `src/css/`).
///
/// The editor injects it unless the `styled` setting is off: one `<style>`
/// at the end of the page's `<head>` (or in the shadow root the editor is
/// in), shared by every editor and documents view there and removed with
/// the last. With `styled` off a host can serve it itself: the crate ships
/// it whole as one file, `preset.css` (and the npm package as
/// `@pngwasi/config-editor/preset.css`), for pages under a strict
/// Content-Security-Policy that refuses the injected `<style>`.
pub const PRESET_CSS: &str = concat!(
    include_str!("css/base.css"),
    include_str!("css/themes.css"),
    include_str!("css/documents.css")
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
    /// The theme and the on/off switches; see [`EditorSettings`]. May be a
    /// signal: every setting switches live, keeping the caret, the undo
    /// history and the folds.
    #[prop(optional, into)]
    settings: Signal<EditorSettings>,
    /// Called with the document, in `language`, after every edit.
    #[prop(optional, into)]
    on_change: Option<Callback<String>>,
    /// Called with the language on screen: once on mount, then each time
    /// an `edit_as` switch completes, and when undo or redo steps back
    /// across one (a switch is an undo step). A switch waits while the document cannot be converted
    /// yet (TOML has no `null`): the host's language stays on screen until
    /// then, so this, not `edit_as`, is what is shown.
    #[prop(optional, into)]
    on_edit_as_change: Option<Callback<Language>>,
    /// Called with whether the text on screen is written back to the host's
    /// document: once on mount, then each time that changes. It is `false`
    /// while an `edit_as` edit cannot convert back (invalid text, a value
    /// the host's language cannot hold, a read-only value changed), when
    /// [`EditorHandle::value`] is `None` and `on_change` waits; `true` again
    /// once the text converts, from whatever brought it there (typing,
    /// paste, cut, drop, undo, a switch, a reload or a new `value`). Read
    /// after each pause in typing, as the conversion is. Always `true`
    /// while editing in `language`.
    #[prop(optional, into)]
    on_sync_change: Option<Callback<bool>>,
    /// Called with how the document differs from `baseline` (lines added,
    /// removed and modified, and the number of separate changes): once on
    /// mount and after each edit or baseline update that changes it. Counts
    /// what the diff gutter shows, so it stays at zero with the `show_diff`
    /// setting off. Needs the `diff` feature: without it, never called.
    #[prop(optional, into)]
    on_change_summary: Option<Callback<ChangeSummary>>,
    /// Called with the editor's own problems: syntax errors, schema
    /// problems, secret placeholders and values Edit-as cannot convert, each
    /// with [`DiagnosticOrigin::Editor`]; never the `host_diagnostics`. They
    /// are checked after each pause in typing, as the underlines are: this
    /// is called once on mount, then each time they change (not while they
    /// stay the same, whatever else is typed). Positions are in the text on
    /// screen. With the `show_diagnostics` setting off the editor finds
    /// none. A host gating a Save button on "no error" reads this instead
    /// of parsing the text again.
    #[prop(optional, into)]
    on_problems: Option<Callback<Vec<Diagnostic>>>,
    /// Called once per problem with something in `schema` the editor cannot
    /// use (see [`Schema::warnings`]), such as a `patternProperties` regex
    /// its engine cannot compile, right after the editor mounts (a microtask
    /// later, never while it is being made). Without it, each goes to the
    /// browser console as a warning.
    #[prop(optional, into)]
    on_schema_warning: Option<Callback<String>>,
    /// Disable editing while keeping highlighting, diagnostics, diffs, and
    /// hover docs.
    #[prop(default = Signal::from(false), into)]
    read_only: Signal<bool>,
    /// Why the editor is read-only (a file the host manages, a missing
    /// permission), while `read_only` is on: shown with the
    /// "Cannot edit in a read-only editor" notice when the user tries to
    /// edit, and in the text area's accessible description. May be a signal.
    #[prop(optional, into)]
    read_only_reason: MaybeProp<String>,
    /// A handle the host keeps to find fields, follow the caret and jump to
    /// a field; see [`EditorHandle`].
    #[prop(optional)]
    handle: Option<EditorHandle>,
    /// Problems the host found (its own checks: a service, a build step), by
    /// path or range,
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
    /// Fields the user cannot change, each with everything inside it, on
    /// top of those the schema marks `readOnly`; may be a signal to change
    /// them at runtime. They are found by path in the language on screen,
    /// so they hold while editing as another language too. Needs the
    /// `protect` feature: without it, nothing is locked (the console says
    /// so).
    #[prop(optional, into)]
    read_only_paths: Option<Signal<Vec<FieldPath>>>,
    /// Stretches of the host's text the user cannot change; may be a
    /// signal. They follow the text through edits; while editing as another
    /// language, the fields they cover are locked instead. Needs the
    /// `protect` feature: without it, nothing is locked (the console says
    /// so).
    #[prop(optional, into)]
    read_only_ranges: Option<Signal<Vec<TextRange>>>,
    /// How secret placeholders are matched, described and completed; see
    /// [`SecretOptions`]. Fields the schema marks `writeOnly` or `x-secret`
    /// are secrets. Needs the `protect` feature: without it, placeholders
    /// are plain text.
    #[prop(optional, into)]
    secrets: Option<SecretOptions>,
    /// Called when the user saves (`Ctrl+S` / `Cmd+S`, the browser's save
    /// dialog prevented) or the host calls [`EditorHandle::save`], with the
    /// document in `language`. Nothing saves by itself. Without it, `Ctrl+S`
    /// is left to the browser (as with the `save_shortcut` setting off). See
    /// [`SaveRequest`] for an edit that cannot convert back yet.
    #[prop(optional, into)]
    on_save: Option<Callback<SaveRequest>>,
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
    let schema_warnings = schema
        .as_ref()
        .map(|schema| schema.warnings.clone())
        .unwrap_or_default();
    // As the host wrote it: the state keeps its line endings for what it
    // hands back (see `line_endings`).
    let initial_value = initial_value.unwrap_or_default();
    let host_text = value
        .as_ref()
        .map_or_else(|| initial_value.clone(), Signal::get_untracked);
    let baseline = baseline.unwrap_or_else(|| Signal::stored(host_text.clone()));
    let (state, initial_pending) = EditorState::new(Init {
        host_language: language,
        edit_language: edit_as.get_untracked().unwrap_or(language),
        schema,
        schema_error,
        host_text,
        baseline: baseline.get_untracked(),
        opening: handle.and_then(EditorHandle::take_opening),
        read_only,
        read_only_reason: Signal::derive(move || read_only_reason.get()),
        settings,
        host_on_change: on_change,
        host_diagnostics: host_diagnostics.unwrap_or_default(),
        protect: protect::ProtectInit {
            paths: read_only_paths,
            ranges: read_only_ranges,
            secrets,
        },
        on_save,
        on_baseline_conflict,
    });
    // After the editor is made, not while: the host has its handle (a
    // JavaScript `new ConfigEditor` has returned) when it hears them.
    if !schema_warnings.is_empty() {
        crate::reactive::later::soon(state, move |_| {
            report_schema_warnings(&schema_warnings, on_schema_warning);
        });
    }
    sync::connect(
        state,
        sync::Host {
            value,
            edit_as,
            on_edit_as_change,
            initial_pending,
            on_sync_change,
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
    if let Some(callback) = on_problems {
        problems::report_own(state, callback);
    }
    input::attach_listeners(state);
    state.connect_protection();
    a11y::connect_caret_note(state);
    a11y::connect_suggestions(state);
    if let Some(handle) = handle {
        handle.attach(state);
    }
    if let Some(on_path_change) = on_path_change {
        Effect::new(move |_| on_path_change.run(state.doc().caret_path.get()));
    }
    remeasure_on_window_changes(state);
    outside_press::close_popups_on_outside_press(state);
    kept_focus::keep_focus_in_editor(state);
    outside_press::close_popups_on_escape_over_them(state);
    live_settings::follow(state);
    let rows = components::row_states(state);
    folding::mount_fold_layers(state);
    let file_name = file_name
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| language.default_file_name().to_string());
    let root_class = class.unwrap_or_default();
    let shown = state.config().settings;
    let fills = RwSignal::new(false);

    view! {
        <div
            class=move || {
                let read_only = if read_only.get() { " config-editor__readonly" } else { "" };
                let fills = if fills.get() { fill::FILLS_CLASS } else { "" };
                format!("config-editor {root_class}{read_only}{fills}")
            }
            // Code reads left to right, in a right-to-left page too: the
            // text, the gutter, the popups (all inside) and the footer.
            dir="ltr"
            data-theme=move || shown.theme.get().css_family()
            data-color-theme=move || shown.theme.get().as_str()
            style=cssom(move || shown.color_style())
            node_ref=state.view().root_ref
            on:keydown=move |event| keys::on_root_keydown(state, &event)
            on:focusout=move |event| state.tip_follows_focus(event.related_target().as_ref())
        >
            {crate::preset_style::preset_style(state.view().root_ref, shown.styled)}
            <FillProbe fills=fills />
            <section class="config-editor__frame" aria-label=state.language_text(|strings| &strings.editor_label)>
                <Kept when=shown.toolbar view=move || view! { <Toolbar state=state file_name=file_name.clone() has_schema=has_schema /> } />
                <EditAsNotice state=state />
                <BaselineNotice state=state />
                <div class="config-editor__surface">
                    <Gutter state=state rows=rows />
                    <div class="config-editor__input-wrap">
                        <div aria-hidden="true" style=cssom(frame_probe_style()) node_ref=state.view().frame_probe_ref></div>
                        <CodeLayer state=state rows=rows />
                        <DiagnosticOverlay state=state />
                        <Kept when=shown.search view=move || view! { <SearchHighlights state=state /> } />
                        <SourceInput state=state />
                        <HoverCardView state=state />
                        <ProblemTip state=state />
                        <ProtectTipView state=state />
                        <ReadOnlyNotice state=state />
                        <CompletionList state=state />
                        <Kept when=shown.search view=move || view! { <FindWidget state=state /> } />
                        <Kept when=shown.go_to_line view=move || view! { <GoToLineWidget state=state /> } />
                    </div>
                </div>
                <Kept when=shown.footer view=move || view! { <StatusBar state=state /> } />
                <Announcer state=state />
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

/// Window resizes and scrolls of the page or of any container around the
/// editor (a dialog's, a panel's) move the editor, so popups re-measure and
/// the hover card goes, as it does when the text scrolls; so does the text
/// area resized without the window (its host's layout changed, a panel
/// opened beside it), which problem messages are as wide as.
fn remeasure_on_window_changes(state: EditorState) {
    let bump = move || {
        state
            .view()
            .layout_revision
            .update(|revision| *revision += 1)
    };
    window_listener(&["resize", "scroll"], move |event| {
        if moves_editor(state, &event) {
            state.hide_hover();
            bump();
        }
    });
    let textarea = state.view().textarea_ref;
    // A new text area (or none) disconnects the watch on the old one.
    Effect::new(move |_| {
        if let Some(element) = textarea.get() {
            untrack(|| size_observer(&element, bump));
        }
    });
}

/// Whether `event` (a resize, or a scroll heard from the window) moved the
/// editor: the window resized, or the page or a container around the
/// editor scrolled; not the text itself, a popup or anything beside it.
fn moves_editor(state: EditorState, event: &web_sys::Event) -> bool {
    use wasm_bindgen::JsCast;

    if event.type_() != "scroll" {
        return true;
    }
    let Some(textarea) = state.view().textarea_ref.get_untracked() else {
        return false;
    };
    let Some(scrolled) = event
        .target()
        .and_then(|target| target.dyn_into::<web_sys::Node>().ok())
    else {
        return false;
    };
    let textarea: &web_sys::Node = textarea.unchecked_ref();
    &scrolled != textarea && scrolled.contains(Some(textarea))
}
#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod declared_syntax;
#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod odd_syntax;
#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod save_matrix;
#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod save_moves_properties;
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
mod termination_matrix;
#[cfg(test)]
mod tests;
// The timing tests' helper. Declared after the code, as every `#[cfg(test)]`
// item is: strings/usage_tests.rs reads a file only up to its first one.
#[cfg(test)]
mod test_timing;
