# config-editor-leptos

A lightweight, VS Code–style config editor component for [Leptos], plus a
headless API for the same parsing, validation, and conversion.

- **Languages:** YAML (default), JSON, TOML, HCL, XML, and your own through
  the `ConfigLanguage` trait.
- **One schema for all of them:** every language parses into the same data
  model, so a single JSON Schema drives diagnostics, completions, and hover
  documentation everywhere.
- **Edit as another language:** show a YAML file as TOML (or any other pair);
  edits are saved back in the file's own language, keeping its comments.
- **Editing:** schema completion with formatted inserts, auto-closing pairs
  and XML end tags, comment toggling, smart indentation, undo/redo, a diff
  gutter against a baseline (with per-change revert, a change count and a
  unified diff), read-only mode, and eleven bundled themes.
- **Large files:** only visible rows are rendered; typing stays fast in
  documents of thousands of lines.

For JavaScript and React, see [`@pngwasi/config-editor`] and
[`@pngwasi/config-editor-react`]; they wrap this crate. Every prop, handle
method and headless function here has a JS option, method or function there
(and a React prop, ref member or export), or a documented reason not to:
`e2e/tests/api-parity.spec.mjs` checks it.

<!-- toc -->
## Contents

- [Install](#install)
- [Quick start](#quick-start)
- [`<ConfigEditor>` props](#configeditor-props)
  - [Settings](#settings)
  - [Editing in another language](#editing-in-another-language)
  - [Changes against the baseline](#changes-against-the-baseline)
  - [Saving and unsaved edits](#saving-and-unsaved-edits)
- [Several documents](#several-documents)
- [Paths and positions](#paths-and-positions)
- [Host diagnostics](#host-diagnostics)
- [Read-only fields](#read-only-fields)
- [Secrets](#secrets)
- [Schema help](#schema-help)
- [Keyboard](#keyboard)
  - [Keyboard and screen-reader access](#keyboard-and-screen-reader-access)
  - [Find and replace](#find-and-replace)
  - [Folding](#folding)
- [Headless API](#headless-api)
- [XML mapping](#xml-mapping)
- [Your own language](#your-own-language)
  - [Describing syntax as data](#describing-syntax-as-data)
  - [Optional refinements](#optional-refinements)
- [Themes and colors](#themes-and-colors)
- [Cargo features](#cargo-features)
- [License](#license)
<!-- /toc -->

## Install

```sh
cargo add config-editor-leptos
```

Requires Leptos 0.8 with client-side rendering (`csr`), a Rust 1.88+
toolchain, and the `wasm32-unknown-unknown` target for the browser build.
The headless API also works natively (servers, CLIs, tests).

## Quick start

```rust
use leptos::prelude::*;
use config_editor_leptos::{ConfigEditor, EditorSettings, Language};

#[component]
fn Config() -> impl IntoView {
    let saved = RwSignal::new("listen: 7835\n".to_string());
    view! {
        <ConfigEditor
            language=Language::YAML
            value=saved
            on_change=move |text| saved.set(text)
            schema=include_str!("schema.json")
            settings=EditorSettings { show_toolbar: true, show_footer: true, ..Default::default() }
        />
    }
}
```

The editor injects its stylesheet ([`PRESET_CSS`]) by default.

## `<ConfigEditor>` props

| Prop                | Type                                       | Default          | Purpose                                                                                                                  |
| ------------------- | ------------------------------------------ | ---------------- | ------------------------------------------------------------------------------------------------------------------------ |
| `language`          | `Language`                                 | `Language::YAML` | Language of the document the host stores: `initial_value`, `value`, `baseline` and `on_change` are always in it.         |
| `edit_as`           | `Language`, `Option<Language>` or a signal | unset            | Show and edit the document in another language. Changing it converts the open document in place, unsaved edits included. |
| `initial_value`     | `String`                                   | `""`             | Starting text, also the diff baseline (uncontrolled).                                                                    |
| `value`             | `Signal<String>`                           | —                | Controlled text. Updates flow in; the caret stays on the same text and the scroll position is kept.                      |
| `on_change`         | `Fn(String)`                               | —                | Called with the document after every edit (after each typing pause while editing as another language).                   |
| `on_edit_as_change` | `Fn(Language)`                             | —                | Called when an `edit_as` switch completes. A switch waits while the document cannot convert yet (TOML has no `null`).    |
| `on_schema_warning` | `Fn(String)`                               | console warning  | Called once per thing in `schema` the editor cannot use, such as a `patternProperties` regex its engine cannot compile.  |

While editing as another language, edits reach `on_change` after each typing
pause. Call `config_editor_leptos::flush_pending_edits()` before reading the
document from code (a Save button) to send an edit still waiting first.
| `baseline` | `Signal<String>` | the document at mount | Text the diff gutter and the dirty flag compare against; set it to the saved text after saving. See [Saving](#saving-and-unsaved-edits). |
| `schema` | `&str`, `String` or `Schema` | — | JSON Schema (draft 2020-12 and earlier) for diagnostics, completion and hover. Pass a compiled `Schema` to share one across editors. |
| `read_only` | `bool` or a signal | `false` | View mode: navigation, selection, highlighting, diagnostics, diffs and hover keep working. Switches live, completions included. |
| `file_name` | `String` | `untitled.<ext>` | Name shown in the toolbar. |
| `class` | `String` | — | Extra class on the editor root. |
| `settings` | `EditorSettings` or a signal | defaults | The theme, colors and every on/off switch (toolbar, footer, diagnostics, diff, completions, search, folding, …); see [Settings](#settings). Every setting switches live. |
| `on_change_summary` | `Fn(ChangeSummary)` | — | Called with the lines added, removed and modified against `baseline`, and the number of changes, whenever that changes. |
| `handle` | `EditorHandle` | — | Find fields, follow the caret and jump to a field from the host; see [Paths and positions](#paths-and-positions). |
| `on_path_change` | `Fn(FieldPath)` | — | Called with the path of what the caret is on each time it changes (and once on mount), for a breadcrumb. |
| `host_diagnostics` | `Signal<Vec<HostDiagnostic>>` | — | Problems the host found (a server's validation), by path or range, shown with the editor's own; see [Host diagnostics](#host-diagnostics). |
| `read_only_paths` | `Signal<Vec<FieldPath>>` | — | Fields the user cannot change, each a whole subtree, on top of the schema's `readOnly` ones; see [Read-only fields](#read-only-fields). |
| `read_only_ranges` | `Signal<Vec<TextRange>>` | — | Stretches of the host's text the user cannot change; they follow the text through edits. |
| `secrets` | `SecretOptions` | defaults | How secret placeholders are matched, described and completed; see [Secrets](#secrets). |
| `on_save` | `Fn(SaveRequest)` | — | Called on `Ctrl+S` / `Cmd+S` and `EditorHandle::save` with the document in `language`. Nothing saves by itself; without it `Ctrl+S` is the browser's. |
| `on_dirty_change` | `Fn(bool)` | — | Called with whether the text differs from `baseline`: once on mount, then each time that changes. |
| `on_baseline_conflict` | `Fn(BaselineConflict)` | — | Called when a new `baseline` arrives under unsaved edits (the notice opens) and when that is settled. |

To build your own chrome, turn the built-in toolbar or footer off and compose
`ConfigEditorToolbar` / `ConfigEditorFooter`, which take the same `theme`
and `colors` so they match the editor.

### Settings

`settings` takes an [`EditorSettings`]: the theme, the colors and every
on/off switch, one struct with a default for each field. Name only what you
change, and pass a signal to switch any of them live: the caret, the undo
history and the folds stay.

```rust
use config_editor_leptos::{ConfigEditor, EditorSettings, EditorTheme};

let settings = RwSignal::new(EditorSettings {
    theme: EditorTheme::Nord,
    show_footer: true,
    ..EditorSettings::default()
});
view! {
    <button on:click=move |_| settings.update(|settings| settings.folding = !settings.folding)>
        "Toggle folding"
    </button>
    <ConfigEditor settings=settings initial_value="listen: 7835\n" />
}
```

The same struct is what the npm package reads from its options (camelCase
keys, `{ showFooter: true }`), and its TypeScript type is generated from it.

<!-- settings:start -->

<!-- Generated from crates/config-editor-leptos/src/settings.rs by js/config-editor/tests/settings.rs; do not edit. Regenerate with `UPDATE_GENERATED=1 cargo test -p config-editor-js --test settings`. -->

| Field | Default | |
|---|---|---|
| `theme` | `EditorTheme::VsCodeDark` | The bundled color theme. |
| `colors` | none | Colors overriding the theme's, by token (`bg`, `key`, `string`, `selection`, …), for a custom palette. |
| `styled` | `true` | Inject the preset stylesheet. Off, style the `config-editor__*` markup yourself (the preset is `PRESET_CSS`, `presetCss()` in JavaScript). |
| `show_toolbar` | `false` | Show the built-in toolbar: the file name, its language and the schema's state. |
| `show_footer` | `false` | Show the built-in status footer: the suggestions hint, problem and change counts, and the line count. |
| `show_diagnostics` | `true` | Show syntax and schema problems inline, in the footer and on hover (`F8` moves between them). |
| `show_diff` | `true` | Show line changes against the baseline in the gutter (`Alt+F5` / `Alt+F3` move between them). Off, the change summary stays at zero. |
| `completions` | `true` | Schema completions (`Ctrl+Space`). |
| `auto_trigger_completions` | `true` | Open suggestions while typing and after accepting a key. Off, only `Ctrl+Space` opens them. |
| `hover_docs` | `true` | Show the schema's description, type, default and examples when hovering a key. |
| `highlighting` | `true` | Syntax colors. |
| `auto_closing` | `true` | Auto-close quotes and brackets, wrap selections, and delete empty pairs on Backspace. |
| `search` | `true` | Find and replace (`Ctrl+F`, `Ctrl+H`, `F3`). Off, those keys are left to the browser. |
| `go_to_line` | `true` | Go to line (`Ctrl+G`). Off, the key is left to the browser. |
| `folding` | `true` | Folding by structure: chevrons in the gutter, `Ctrl+Shift+[` and `Ctrl+Shift+]` at the caret, `Ctrl+K Ctrl+0` and `Ctrl+K Ctrl+J` for everything (`Cmd` on macOS). View only: the text stays whole. Switching it off opens every fold. |
| `save_shortcut` | `true` | Whether `Ctrl+S` / `Cmd+S` saves, when a save callback is set. Off, the key is left to the browser and only the handle's `save` saves. |

<!-- settings:end -->

### Editing in another language

```rust
<ConfigEditor
    language=Language::XML   // what the host stores and receives
    edit_as=Language::HCL    // what the user edits
    value=xml
    on_change=move |text| xml.set(text)
/>
```

- Only what the user changed is written back: untouched values keep their
  exact host spelling, and the host's comments and formatting survive.
- Invalid text is never sent to the host.
- Values the host cannot carry exactly (for example an empty list in XML)
  are flagged as warnings on the value.
- With a schema, a single XML element or HCL block where the schema expects a
  list reads as a one-item list.

### Changes against the baseline

The gutter marks added lines (green bar), modified lines (blue bar) and
removed lines (a red mark between the lines around them). Clicking a marker
opens a preview of the lines before and now, with **Revert**: it restores
the baseline lines of that whole change as one edit (one undo step, sent
through `on_change`, converted back while editing as another language). The
footer shows the number of changes; clicking it moves to the next one.

```rust
use config_editor_leptos::{unified_diff, ChangeSummary, ConfigEditor, Language};

let saved = RwSignal::new(load());
let text = RwSignal::new(saved.get_untracked());
let changes = RwSignal::new(ChangeSummary::default());
view! {
    <ConfigEditor
        language=Language::YAML
        value=text
        baseline=saved
        on_change=move |next| text.set(next)
        on_change_summary=move |summary| changes.set(summary)
    />
    <button disabled=move || changes.get().is_empty()>
        {move || format!("Review {} changes", changes.get().hunks)}
    </button>
    // For a review dialog: `diff -u` output, 3 lines of context.
    <pre>{move || unified_diff(&saved.get(), &text.get(), 3)}</pre>
}
```

### Saving and unsaved edits

Nothing is saved by itself. `Ctrl+S` / `Cmd+S` in the editor (the browser's
save dialog prevented) and `EditorHandle::save` call `on_save` with a
`SaveRequest { text, language }`: the document in the host's `language`.
While editing as another language, an edit still waiting out the typing pause
is converted first (it reaches `on_change` before `on_save`); when it cannot
be written in `language` (invalid, or a `null` for TOML), nothing is sent,
`save` returns `false`, and the caret goes to the problem with its message
shown.

The text is dirty while it differs from `baseline` as shown (the document at
mount when no `baseline` is given). `on_dirty_change` reports it once on
mount and on every change; `EditorHandle::is_dirty` reads it.

```rust
use config_editor_leptos::{BaselineConflict, ConfigEditor, EditorHandle, SaveRequest};

let saved = RwSignal::new(load());
let text = RwSignal::new(saved.get_untracked());
let dirty = RwSignal::new(false);
let editor = EditorHandle::new();
view! {
    <button disabled=move || !dirty.get() on:click=move |_| { editor.save(); }>"Save"</button>
    <ConfigEditor
        handle=editor
        value=text
        on_change=move |next| text.set(next)
        baseline=saved
        on_save=move |request: SaveRequest| {
            store(&request.text);
            saved.set(request.text); // nothing unsaved any more
        }
        on_dirty_change=move |now| dirty.set(now)
        on_baseline_conflict=move |event: BaselineConflict| log(event)
    />
}
// Another client saved: `saved.set(from_server)` moves the baseline on.
```

When `baseline` changes while the user has unsaved edits, a notice opens over
the text, "The file changed on the server.", instead of mixing the two:

- **Reload** makes the new baseline the text: one undo step (`Ctrl+Z` brings
  the user's text back), sent through `on_change` (exactly the new baseline,
  also while editing as another language).
- **Keep mine** keeps the text and diffs it against the new baseline.
  `Escape` in the notice chooses it, as the choice that loses nothing.
- The notice is `role="alert"`. It takes the focus (on Keep mine) only when
  the focus was in the editor, and gives it back to the text when closed.
- It closes by itself (`BaselineConflict::Settled`) when the text comes to
  equal the new baseline, or the host sets the old one back.

Without unsaved edits a new baseline is taken silently. It is also taken
silently when it already is the text, or when it is a text the editor sent
the host (`on_change`, `on_save`): a save coming back while the user typed
on is not a change from elsewhere. Colors: the notice uses `raised`,
`raised-border`, `raised-text`, `raised-muted`, `warning`, `control-bg` and
`control-text`.

## Several documents

`<ConfigDocuments>` shows several files in one view, a main config and its
drop-ins, as VS Code-style tabs over one editor per document. Each
`Document` has its own language (from its file name unless given), text,
baseline, read-only state, `edit_as` language, schema and, optionally,
settings, host problems and protected fields; documents with
the same schema JSON share one compiled `Schema` (pass a compiled `Schema`
to share it with editors elsewhere).

```rust
use config_editor_leptos::{CloseRequest, ConfigDocuments, Document, DocumentsHandle, SaveRequest};

const SCHEMA: &str = include_str!("schema.json");
let documents = RwSignal::new(vec![
    Document::new("main", "burrow.yaml", load("burrow.yaml")).schema(SCHEMA),
    Document::new("limits", "conf.d/limits.toml", load("conf.d/limits.toml")).schema(SCHEMA),
    Document::new("clients", "conf.d/clients.json", load("conf.d/clients.json"))
        .schema(SCHEMA)
        .read_only(true),
]);
let edit = move |id: &str, change: &dyn Fn(&mut Document)| {
    documents.update(|list| list.iter_mut().filter(|d| d.id == id).for_each(change))
};
let tabs = DocumentsHandle::new();
view! {
    <button on:click=move |_| { tabs.save_all(); }>"Save all"</button>
    <ConfigDocuments
        documents=documents
        handle=tabs
        on_change=move |(id, text): (String, String)| edit(&id, &|d| d.value = text.clone())
        on_save=move |(id, request): (String, SaveRequest)| {
            store(&id, &request.text);
            edit(&id, &|d| d.baseline = Some(request.text.clone()));
        }
        on_close=move |request: CloseRequest| {
            if !request.dirty || confirm_discard(&request.id) {
                documents.update(|list| list.retain(|d| d.id != request.id));
            }
        }
    />
}
```

- **The host owns the list.** It adds, removes, renames and reorders
  documents by changing `documents`. A document keeps its editor while its
  `id` stays listed: a new `value`, `baseline`, `file_name`, `read_only` or
  `edit_as` updates it in place; a new `language` or schema mounts a new
  editor for it.
- **The view reports intents.** `on_select(id)` when a document becomes the
  active one (a tab, a key, `DocumentsHandle::select`, or the active one
  closing: the next tab takes its place, else the previous one).
  `on_close(CloseRequest { id, dirty })` when the user asks to close one
  (its close button, a middle click, `Delete` on its tab): nothing closes by
  itself, and `dirty` tells the host to ask first. Without `on_close`, tabs
  have no close button.
- **Per-document callbacks carry the id:** `on_change((id, text))`,
  `on_save((id, SaveRequest))` (`Ctrl+S` / `Cmd+S` saves the document being
  edited), `on_dirty_change((id, dirty))` (when a document is added, then on
  every change). Each tab shows a dot while its document is dirty.
- **Switching keeps each document's state.** An editor mounts the first time
  its document is shown and then stays, hidden (`visibility: hidden`,
  `inert`) while another is active, so its caret, selection, scroll
  position, undo history, folds, find widget and Edit-as language are where
  they were. Mounting lazily keeps the start cheap; keeping editors avoids
  re-parsing on every switch (and the memory that unmounting editors does
  not give back yet). Measured on the release build with ten ~2000-line
  documents: about 1.5 to 1.9 MB of wasm memory per editor once shown (21 MB
  with all ten, against 8 MB with one shown and 26 MB for ten separate
  `ConfigEditor`s), about 1,000 DOM nodes per editor; 50 switches between
  shown documents add nothing (a switch, from the click to the panel shown
  as a test driver sees it, took 49 ms at the median).
- **`DocumentsHandle`:** `select(id)` (shows it and focuses its editor),
  `active()`, `editor(id)` / `active_editor()` (the document's
  `EditorHandle`, attached once it has been shown), `is_dirty(id)`,
  `save(id)`, `save_all()` (the dirty documents; returns the ids sent),
  `focus()`.
- **Settings:** `settings` takes the same [`EditorSettings`](#settings) as
  `ConfigEditor`, for every editor of the view and for the tabs (`theme`,
  `colors` with the `Tab*` tokens; `styled` injects the preset stylesheet
  once for the whole view). A document's own `Document::settings` replace
  them for its editor. Both switch live. Also `class`, `label` (the tab
  list's accessible name, default "Open files"), `initial_active`.
- **Per document, as on `ConfigEditor`:** `host_diagnostics`,
  `read_only_paths` and `read_only_ranges` (updated in place when the list
  changes) and `secrets` (read when its editor mounts).

```rust
use config_editor_leptos::{Document, EditorSettings, FieldPath, HostDiagnostic, Severity};

let view_settings = EditorSettings { show_footer: true, ..EditorSettings::default() };
let drop_in = Document::new("tls", "conf.d/tls.toml", tls_text)
    // This one without folding; the rest as the view's.
    .settings(EditorSettings { folding: false, ..view_settings.clone() })
    .read_only_paths(vec![FieldPath::from("/cert")])
    .host_diagnostics(vec![HostDiagnostic::at_path("/cert", Severity::Error, "certificate expired")]);
// <ConfigDocuments documents=documents settings=view_settings />
```

Keyboard: the tab strip is a WAI-ARIA `tablist` (each tab a `tab` with its
`tabpanel`; one tab in the `Tab` order). `←` / `→` select the previous /
next tab (wrapping), `Home` / `End` the first / last, `Enter` moves into the
editor, `Delete` asks to close. From anywhere in the view, the editor
included, `Alt+PageDown` / `Alt+PageUp` switch to the next / previous
document, the focus going along. Browsers keep `Ctrl+Tab` and
`Ctrl+PageDown` for their own tabs; `Alt+PageDown` is free in browsers and
in the editor's keys, and is not an AltGr combination (AltGr is `Ctrl+Alt`
on Windows layouts). The strip scrolls sideways when the tabs overflow (a
mouse wheel scrolls it too) and keeps the active tab in view, at phone
widths included. Colors: the `Tab*` tokens, see
[Themes and colors](#themes-and-colors).

## Paths and positions

A [`FieldPath`] names a field the same way in every language: a JSON pointer
(`"/web/hosts/1"`) or segments (`FieldPath::root().key("web").key("hosts").index(1)`).
An [`EditorHandle`] passed as `handle` maps paths to the text on screen and
back, and jumps to fields:

```rust
use config_editor_leptos::{ConfigEditor, EditorHandle, FieldPath};

let editor = EditorHandle::new();
let crumb = RwSignal::new(FieldPath::root());
view! {
    <nav>{move || crumb.get().to_pointer()}</nav>
    <button on:click=move |_| { editor.reveal("/web/listen"); }>"Go to web.listen"</button>
    <ConfigEditor handle=editor on_path_change=move |path| crumb.set(path) value=text />
}
```

| `EditorHandle` method         | Description                                                                                                                                                                                     |
| ----------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `range_of(path)`              | `Option<TextRange>`: the key and its value when they share a line (`listen: 7835`, `<listen>7835</listen>`), the key alone when the value is a block below it, the value alone for a list item. |
| `path_at(position)`           | `Option<FieldPath>` of what a `TextPosition` is on: the key under or just before it, else the value or container holding it (the root at the top level).                                        |
| `current_path()`              | The caret's path. Reactive: read it in a closure to follow the caret (as `on_path_change` does). Computed only while something reads it.                                                        |
| `reveal(path)`                | Scrolls to the field, selects `range_of(path)` and focuses the editor; `false` when there is no such field.                                                                                     |
| `fold(path)` / `unfold(path)` | Fold the block the field's key opens / open the folds around the field; `false` when there was nothing to do.                                                                                   |
| `fold_all()` / `unfold_all()` | Fold every block / open every fold.                                                                                                                                                             |
| `folded_lines()`              | The closed folds as one-based `(first, last)` line pairs. Reactive.                                                                                                                             |
| `focus()`, `text()`           | Focus the editor; the text on screen.                                                                                                                                                           |

Positions (`TextPosition`: one-based `line` and character `column`) refer to
the text on screen, which is the `edit_as` language while editing as another
one; paths are the same in every language. Before the editor mounts the
handle answers `None`. The same lookups work headless:
`Language::YAML.range_of(text, "/web/http")` and
`Language::YAML.path_at(text, TextPosition::new(2, 5))`. A language of your
own gets them from its `key_span`, `value_span`, `hover_key` and
`completion_context`.

## Host diagnostics

When a server validates the document, pass its problems as
`host_diagnostics`. Each points at a field by path (preferred: it follows the
field through edits and `edit_as`) or at a range of the host's text, or at
the whole document:

```rust
use config_editor_leptos::{ConfigEditor, EditorSettings, HostDiagnostic, Severity};

let problems = RwSignal::new(Vec::new());
// After the server answered:
problems.set(vec![
    HostDiagnostic::at_path("/web/listen", Severity::Error, "port 80 is taken")
        .with_code("E_PORT")
        .with_source("burrow")
        .with_docs_url("https://example.com/docs/ports"),
    HostDiagnostic::document(Severity::Warning, "restart needed to apply"),
]);
let settings = EditorSettings { show_footer: true, ..EditorSettings::default() };
view! { <ConfigEditor value=text host_diagnostics=problems settings=settings /> }
```

- They are marked where the field is written, as schema problems are (the
  value, or the key of a block), next to the editor's own problems: `F8` /
  `Shift+F8` stop at them, the footer counts them, and the hover message
  adds who reported them, the code, and "Open docs ↗" (a click on the
  message opens `docs_url`). What cannot be placed (a missing field, a range
  while editing as another language, the whole document) shows in the footer.
- Severities `Error`, `Warning`, `Info` (a blue wavy underline) and `Hint`
  (a dotted one), colored by `--config-editor-error`, `-warning`, `-info`
  and `-hint` (`ThemeColor::Info`, `ThemeColor::Hint`).
- Each problem shows once: where the host marks the same text as one of the
  editor's own, only the host's shows. A new list replaces the previous one,
  and only the markers update.
- Every `Diagnostic` says where it came from (`origin`:
  `DiagnosticOrigin::Editor` or `Host`) and carries `code`, `source` and
  `docs_url`.

## Read-only fields

A field is read-only when the schema says `readOnly: true` (its reason, if
any, in `x-readonly-reason`), when the host names it in `read_only_paths`
(a whole subtree each), or when it lies in one of the host's
`read_only_ranges`. Both props may be signals that change at runtime; the
whole-document `read_only` stays as it is.

```rust
use config_editor_leptos::{ConfigEditor, FieldPath, TextPosition, TextRange};

// {"properties": {"id": {"type": "string", "readOnly": true,
//                        "x-readonly-reason": "Assigned by the server"}}}
let locked = RwSignal::new(vec![FieldPath::from("/limits")]);
let first_line = RwSignal::new(vec![TextRange::new(
    TextPosition::new(1, 1),
    TextPosition::new(2, 1),
)]);
view! {
    <ConfigEditor
        value=text
        schema=SCHEMA
        read_only_paths=locked
        read_only_ranges=first_line
    />
}
```

- A lock shows in the gutter on each line the field covers, its value is
  muted (`--config-editor-locked`, `ThemeColor::Locked`), and hovering the
  value or the lock shows the reason.
- Every way of changing the text is checked the same way, after it is made
  and before it is applied: typing, Backspace and Delete, paste, cut, drop,
  completions, Replace, `Ctrl+/`, Revert, undo and redo. An edit that would
  change a read-only value, delete its key (also by deleting a selection or
  a parent block that holds it), rename a key holding it, or add to a
  read-only block is refused whole: the text stays as it was, and a short
  tooltip at the caret says why. Replace All replaces the other matches and
  reports how many it skipped.
- Formatting around a locked field stays free: blank lines, trailing
  spaces, a comment after the value, punctuation next to it (`,` in JSON),
  and re-indenting where indentation is not structure (JSON, TOML, HCL,
  XML). In YAML, where the indent says which block a key belongs to,
  changing a locked line's indent is refused.
- Suggestions leave read-only keys and values out. Inside a read-only
  region (browsing it with `Ctrl+Space`) they are listed, with the reason as
  their detail.
- While editing as another language, the same fields are locked there,
  found by path in that language (a host range locks the fields it covers).
  An Edit-as save that would still change a read-only value, by any route,
  is refused like other unsafe saves: it is not sent, and an error says
  which field.
  `Ctrl+S` (`on_save`) is refused the same way.
- The host's own text is never held back: a new `value`, and a baseline
  taken with "Reload" from the changed-on-server notice, may change
  read-only values (and bring their own secret placeholders). A refused
  edit changes nothing, so it leaves the dirty flag as it was.
- With folding, an edit that removes or changes a fold holding a locked
  value is refused like any other, and the fold stays closed. A folded
  block holding a read-only line or a secret placeholder shows its lock or
  key on the fold's row.

## Secrets

Fields the schema marks `writeOnly: true` or `x-secret: true` are secrets.
The host never sends their values: it writes a placeholder token instead
(`«secret 3f2a»` by default), and the editor keeps it as it is unless the
user replaces it.

```rust
use config_editor_leptos::{ConfigEditor, SecretOptions};

// tls:
//   key: «secret 3f2a»
let secrets = SecretOptions::default()
    .with_placeholder(r"«secret [0-9a-f]+»") // the default
    .with_matcher(|token| token.len() < 40)  // optional: which matches are real
    .with_hint("Keeps the current secret unless you replace it")
    .with_reference_prefixes(["${", "file:"]);
view! { <ConfigEditor value=text schema=SCHEMA secrets=secrets /> }
```

- Only tokens the host gave are placeholders: a match of the pattern (and
  of the host's check) found in the host's own `value`. A look-alike typed
  or pasted in stays plain text, and one in a secret field is flagged as a
  problem (it stands for no current secret).
- A placeholder is atomic: the caret never lands inside it (clicks, arrows,
  `Home`/`End` and selections snap to its edges; `←`/`→` step over it),
  `Backspace` or `Delete` next to it removes it whole, an edit or a
  selection cutting into it replaces it whole, and copying it copies the
  token as it is. Undo brings it back as a placeholder.
- It is drawn as a chip (`--config-editor-secret-chip`,
  `--config-editor-secret-chip-border`: `ThemeColor::SecretChip`,
  `SecretChipBorder`), with a key in the gutter; hovering it shows the hint.
- In a secret field's value, suggestions offer references instead of
  values: `${VAR}` (the caret inside the braces) and `file:`; HCL writes
  `${` escaped as `$${`, so the text survives conversion.
- The schema check takes an untouched placeholder as the current secret:
  no `pattern`, `format`, length or type problem is reported on it, and
  messages about a secret field never quote what was typed there.
- Edit-as conversions and saves keep each placeholder byte-for-byte, in
  every language pair and back. Real secret values never reach the editor;
  it logs nothing about field values.

## Schema help

Completions and hover read these keywords, with or without the
`schema-validation` feature:

| Keyword                                      | Shown                                                                                                                   |
| -------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| `description`, `type`, `default`, `examples` | Suggestion detail and hover. A key is marked `required` when its parent lists it.                                       |
| `deprecated: true`                           | The key is struck through and listed last, its hover says so, and a key written in the document gets a warning.         |
| `deprecationMessage`                         | What to use instead, in the hover and the warning.                                                                      |
| `x-docs-url`                                 | A "Documentation" link in the hover, opening in a new tab (`http(s)` addresses only).                                   |
| `x-enum-descriptions` (and `x-enum-titles`)  | Arrays parallel to `enum`: each value's own line in the suggestion list and in the hover of that value in the document. |
| `title` / `description` of a `const` branch  | The same for `oneOf`/`anyOf` lists of documented constants.                                                             |

```json
{
  "mode": {
    "description": "How requests are routed.",
    "x-docs-url": "https://example.com/docs/mode",
    "enum": ["fast", "safe"],
    "x-enum-descriptions": ["Skips the checks.", "Checks everything."]
  },
  "level": {
    "oneOf": [
      { "const": "info", "title": "Info", "description": "Normal messages." },
      { "const": "trace", "title": "Trace", "description": "Everything." }
    ]
  },
  "timeout": {
    "type": "integer",
    "deprecated": true,
    "deprecationMessage": "Use deadline instead."
  }
}
```

Accepting an object key writes its required children. The caret goes to
the first one (left empty unless it has a `default`); each other takes its
`default`, else its first `examples` entry, else a typed placeholder. `Tab`
then walks the block like a snippet, selecting each written value (typing
replaces it) and stopping at each empty one.

`patternProperties` regexes are compiled once per pattern, when the schema
is built. One the editor's engine cannot compile (look-around, and Unicode
classes without `schema-validation`) is reported through
`on_schema_warning` and [`Schema::warnings`] instead of silently matching
nothing. `pattern` itself is only checked by the validator, which rejects a
schema whose regex it cannot compile.

## Keyboard

| Keys                                         | Action                                                                           |
| -------------------------------------------- | -------------------------------------------------------------------------------- |
| `Ctrl+Space`                                 | Open suggestions (with schema examples)                                          |
| `↑` `↓` `PageUp` `PageDown`, `Enter` / `Tab` | Move through suggestions, accept                                                 |
| `Tab` after accepting a block                | Jump to the next empty value, or select the next value the block wrote           |
| `Tab` / `Shift+Tab`                          | Indent / outdent the line or selected lines                                      |
| `Ctrl+M`                                     | Toggle Tab moves focus: `Tab` / `Shift+Tab` leave the editor instead of indenting |
| `Escape`, then `Tab` / `Shift+Tab`           | Leave the editor once (when the `Escape` had nothing to close)                   |
| `Enter`                                      | New line with the right indentation; steps out of a just-typed string or element |
| `Ctrl+Enter` / `Ctrl+Shift+Enter`            | Open a line below / above                                                        |
| `Ctrl+/`                                     | Toggle comments                                                                  |
| `Ctrl+Z`, `Ctrl+Shift+Z` / `Ctrl+Y`          | Undo, redo                                                                       |
| `F8` / `Shift+F8`                            | Next / previous problem                                                          |
| `Alt+F5` / `Shift+Alt+F5`                    | Next / previous change against the baseline                                      |
| `Alt+F3` / `Shift+Alt+F3`                    | Next / previous change, opening its preview with the focus on Revert             |
| `Enter` or `R` in a change preview           | Revert this change (`Escape` closes the preview and returns to the text)         |
| `Ctrl+G`                                     | Go to line: type `line` or `line:column`, `Enter` to go, `Escape` to cancel      |
| `Ctrl+S`                                     | Save through `on_save` (left to the browser without it)                          |
| `Escape` in the baseline notice              | Keep mine: keep the text, diff it against the new baseline                       |
| `Ctrl+Shift+[` / `Ctrl+Shift+]`              | Fold the innermost block around the caret (again: the one around it) / unfold    |
| `Ctrl+K Ctrl+0` / `Ctrl+K Ctrl+J`            | Fold everything / unfold everything                                              |
| `Home`                                       | First non-blank character, then the line start                                   |
| `Ctrl+C` / `Ctrl+X` with nothing selected    | Copy / cut the whole line (pasting it inserts above)                             |
| `Ctrl+F` / `Ctrl+H`                          | Find / find and replace (the selection, on one line, becomes the query)          |
| `Enter` / `Shift+Enter`, `F3` / `Shift+F3`   | Next / previous match                                                            |
| `Alt+C` / `Alt+W` / `Alt+R` in find          | Match case / whole word / regular expression                                     |
| `Enter` / `Ctrl+Alt+Enter` in replace        | Replace the current match / all matches (one undo step)                          |
| `Escape`                                     | Close popups and the find widget, then collapse the selection                    |
| `Alt+PageDown` / `Alt+PageUp`                | Next / previous document in a `ConfigDocuments` view                             |

`Cmd` works in place of `Ctrl` on macOS.

### Keyboard and screen-reader access

`Tab` indents while editing, so it would trap the focus. Two ways out, as in
VS Code: `Ctrl+M` switches `Tab` and `Shift+Tab` to moving the focus to the
next or previous element of the page (announced, and shown in the footer as
"Tab moves focus" until `Ctrl+M` again or a click on it), and right after an
`Escape` that had nothing to close (no suggestions, find widget, hover card
or preview), one `Tab` or `Shift+Tab` leaves. While the suggestion list is
open, `Tab` still accepts. A read-only editor never indents, so there `Tab`
always moves the focus. In a [`ConfigDocuments`](#several-documents) view,
the other documents' editors are inert: `Tab` out of the editor goes on to
its footer and what follows the view, `Shift+Tab` back towards the tabs.

The textarea is described (`aria-describedby`) by what is at the caret: the
message of each problem there, the editor's or the host's ("Warning from
burrow: port in use"), why a read-only value is read-only, a secret
placeholder's hint. It follows the caret a moment after it stops. `F8` and
`Shift+F8` announce the problem they move to through a polite live region
("Error 2 of 5: …"). The suggestion list is a `listbox`, hover help a
`tooltip`, the change preview a `dialog`, and the footer's counts
`aria-live`.

On a touch screen a tap places the caret, a tap on underlined or read-only
text shows its message (there is no hover), and the selection a long press
or the selection handles make is followed like one made with the keyboard.

```rust
// Nothing to set up: the keys and descriptions are always there. The
// footer's indicator is part of the built-in footer (`show_footer`).
view! { <ConfigEditor initial_value=text /> }
```

### Find and replace

`Ctrl+F` opens a find widget over the text; `Ctrl+H` adds the replace row
(never in a read-only editor). Options match case, whole words, or a regular
expression (the `regex` engine with `schema-validation`, else `regex-lite`,
whose case folding is ASCII-only); an invalid pattern is shown, not thrown.
In a regular expression's replacement, `$1` and `${name}` insert groups, and
`\n` and `\t` a line break and a tab. Replacing goes through the normal edit
path: undo, `on_change` and Edit-as all see it, and replace all is one undo
step. Up to 10,000 matches are counted and stepped through, and only those
in view are drawn, so large documents stay fast.

### Folding

Each line that starts a block gets a chevron in the gutter, shown while the
pointer is over the gutter (always on touch screens and on folded lines),
as in VS Code. A folded line shows `…` after its text; clicking it or the
chevron opens the fold. What folds comes from the language
([`fold_ranges`](#your-own-language)): YAML mappings, sequences and block
scalars by indentation; JSON objects and arrays; TOML tables and arrays of
tables (header to the next header) and multi-line arrays, inline tables and
strings; HCL blocks, objects, lists, calls and heredocs; XML elements,
comments and CDATA over several lines; runs of line comments and multi-line
block comments everywhere. A closing bracket or tag on its own line stays
visible.

Folding only changes the view. `value`, `on_change`, undo and saving see
the whole text, and copying or cutting a selection across a fold takes the
hidden lines. Hidden lines are never edited by accident: an edit that
touches a fold opens it first (typing at the end of a folded line,
Backspace at the start of the line after it, Delete at its end, a paste or
replace over a selection reaching into it), then applies to the whole text.
Editing the folded line before its end keeps the fold. Whatever moves the
caret into a fold opens it: find matches, go to line, `F8`, `Alt+F5`,
`EditorHandle::reveal`, a click on a problem, a completion's jump, undo.

A folded block that hides an error marks its line number as an error, and
one that hides changes gets a dotted change bar. Folds move with the lines
around them through edits and controlled `value` updates; one whose
structure is gone (its closing bracket deleted) disappears. The line ranges
are computed again after each edit, in one pass over the text, and the rows
stay virtualized: a 10,000-line document folds and scrolls to its end like
an unfolded one.

```rust
let editor = EditorHandle::new();
view! {
    <button on:click=move |_| editor.fold_all()>"Collapse all"</button>
    <button on:click=move |_| { editor.unfold("/web/listen"); }>"Show web.listen"</button>
    <ConfigEditor handle=editor value=text />
}
```

## Headless API

The same engine without the component:

```rust
use config_editor_leptos::{check_syntax, Language, Schema};

let schema = Schema::compile(include_str!("schema.json"))?;
// At most 20; past that, a last one without a position says more follow.
let problems = schema.validate(Language::TOML, &text); // Vec<Diagnostic>
let syntax_only = check_syntax(Language::JSON, &text);

let value = Language::YAML.parse(&text)?;              // serde_json::Value
let yaml = Language::YAML.serialize(&value)?;
let hcl = Language::YAML.convert(&text, Language::HCL)?;
assert!(hcl.losses.is_empty());
let xml = schema.convert(Language::YAML, &text, Language::XML)?; // schema-aware

let language: Language = "yml".parse()?;               // names and extensions
let language = Language::from_path("infra/main.tf");   // Some(Language::HCL)
```

| Item                                       | Description                                                                                                                                                                                                                                                 |
| ------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `Language`                                 | A language handle: `YAML`, `JSON`, `TOML`, `HCL`, `XML`, `ALL`, `new(&'static dyn ConfigLanguage)`. Methods: `parse`, `serialize`, `convert`, `label`, `extensions`, `file_icon`, `default_file_name`, `from_path`; `FromStr` accepts names and extensions. |
| `Schema`                                   | A compiled JSON Schema: `compile`, `validate(language, text)`, `convert(from, text, to)`, `warnings()`. Cheap to clone and share.                                                                                                                           |
| `SchemaSource`                             | What the `schema` prop takes: JSON text or a `Schema`.                                                                                                                                                                                                      |
| `FieldPath`, `TextPosition`, `TextRange`   | Paths (JSON pointers or segments) and one-based places in the text; `Language::range_of(text, path)` and `Language::path_at(text, position)` map between them.                                                                                              |
| `language::FoldRange`                      | A range that folds (0-based `start` stays visible, lines through `end` hide); `Language::fold_ranges(text)` lists them.                                                                                                                                     |
| `check_syntax(language, text)`             | Syntax problems only.                                                                                                                                                                                                                                       |
| `unified_diff(baseline, current, context)` | The `diff -u` of two texts (`--- baseline` / `+++ current` headers, `@@` hunks with `context` lines around changes, `\ No newline at end of file`); empty when they have the same lines. Line endings are compared as LF.                                   |
| `ChangeSummary`                            | `added`, `removed`, `modified` lines and `hunks` (separate changes), as the diff gutter counts them; `ChangeSummary::between(baseline, current)`, `is_empty()`.                                                                                             |
| `Diagnostic`                               | `line`, `column`, `end_column` (one-based, `None` when unplaced), `message`, `severity` (`Severity::Error`, `Warning`, `Info`, `Hint`), `origin`, and for host problems `code`, `source`, `docs_url`.                                                       |
| `HostDiagnostic`, `DiagnosticTarget`       | A problem the host found, by path, range or for the whole document; see [Host diagnostics](#host-diagnostics).                                                                                                                                              |
| `Conversion`                               | `text`, and `losses`: JSON pointers of values the target reads back differently.                                                                                                                                                                            |
| `ConvertError`                             | `Parse(ParseError)` or `Unsupported(message)` when the target cannot express the document.                                                                                                                                                                  |
| `ParseError`                               | `message`, `line`, `column`.                                                                                                                                                                                                                                |
| `EditorTheme`, `ThemeColor`, `ThemeColors` | Themes and color overrides (below).                                                                                                                                                                                                                         |
| `PRESET_CSS`                               | The stylesheet the editor injects.                                                                                                                                                                                                                          |

## XML mapping

Child elements and attributes are object properties, element text is a typed
scalar (`<listen>7835</listen>` is the number 7835), and repeated siblings are
an array. A top-level element marked `config-root="true"` is a transparent
wrapper; without it, every top-level element is a document property.

## Your own language

Every built-in language implements the public `ConfigLanguage` trait, and
yours can too. Only `info` and `parse` are required; each further method adds
a capability (diagnostic positions, completion, hover, highlighting, editing,
conversion):

```rust
use config_editor_leptos::Language;
use config_editor_leptos::language::{ConfigLanguage, LanguageInfo, ParseError};

struct DotEnv;

impl ConfigLanguage for DotEnv {
    fn info(&self) -> LanguageInfo {
        LanguageInfo { label: "ENV", icon: "E", extensions: &["env"] }
    }
    fn parse(&self, source: &str) -> Result<serde_json::Value, ParseError> {
        todo!("KEY=value lines into a JSON object")
    }
}

static DOTENV: DotEnv = DotEnv;
const ENV: Language = Language::new(&DOTENV);
// <ConfigEditor language=ENV .../>, ENV.convert(text, Language::YAML), …
```

| Method                                                                          | Adds                                                     |
| ------------------------------------------------------------------------------- | -------------------------------------------------------- |
| `info`, `parse` (required)                                                      | Name, icon, extensions; syntax errors and the data model |
| `serialize`                                                                     | Conversion into the language (Edit-as target)            |
| `key_span`, `value_span`                                                        | Positions for schema diagnostics                         |
| `completion_context`, `property_insert`, …                                      | Schema completion                                        |
| `hover_key`                                                                     | Hover docs                                               |
| `highlight`                                                                     | Syntax colors                                            |
| `newline`, `toggle_comments`, `auto_close`, `reindent_on_type`, `lexical_state` | Editing behavior                                         |

### Describing syntax as data

Simple syntax doesn't need code: describe it with `config`, as VS Code's
`language-configuration.json` does, and comment toggling, Enter between
brackets, and the key/value behavior below follow from it.

```rust
use config_editor_leptos::language::LanguageConfig;

fn config(&self) -> LanguageConfig {
    LanguageConfig {
        line_comments: &[";", "#"],     // Ctrl+/ writes the first
        block_comment: None,            // e.g. Some(("/*", "*/")): Ctrl+/ unwraps it
        brackets: &[('[', ']')],        // Enter opens them across lines
        separators: &["="],             // key = value
    }
}
```

### Optional refinements

Small hooks that polish editing for some kinds of languages. Each defaults to
what `config` describes (or to nothing), so skip the ones that don't fit, and
override one only when your syntax needs more than `config` says: HCL
overrides `value_follows_key` for its `name { … }` blocks, XML overrides them
all.

| Kind of language                        | Method               | Adds                                                                                                   | Helper                                                                                                             |
| --------------------------------------- | -------------------- | ------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------ |
| Key/value (`key: value`, `key = value`) | `value_follows_key`  | Completing a key that already has a value renames just the key                                         | —                                                                                                                  |
|                                         | `line_has_value`     | A space typed after a complete value ends the entry                                                    | `value_after_separator`                                                                                            |
|                                         | `empty_value_slots`  | `Tab` walks the empty values of an accepted block                                                      | `empty_slots_at_line_ends`                                                                                         |
| Markup (`<k>…</k>`)                     | `closing_markup_len` | Enter after a typed value steps past the end tag the editor wrote                                      | —                                                                                                                  |
| With trailing comments                  | `comment_start`      | Edit-as keeps a line's comment when the line is renamed or moves                                       | `comment_start_outside_quotes`                                                                                     |
| Any                                     | `fold_ranges`        | What folds; the default folds by indentation, `config`'s brackets and comments, and multi-line strings | `indent_fold_ranges`, `bracket_fold_ranges`, `comment_fold_ranges`, `string_body_fold_ranges`, `merge_fold_ranges` |

`config_editor_leptos::language` also exports the other helpers the built-in
languages use (tokens, schema lookups, inline-container scanning, indent
detection). `tests/custom_language.rs` is a complete example.

## Themes and colors

Eleven themes ship as `EditorTheme` (all of them in `EditorTheme::ALL`;
their slugs parse with `FromStr`, as the JS package names them).

<details>
<summary>The eleven themes</summary>

| `EditorTheme`    | Slug              | Name                       | Look  |
| ---------------- | ----------------- | -------------------------- | ----- |
| `VsCodeDark`     | `vscode-dark`     | VS Code Dark (the default) | dark  |
| `VsCodeLight`    | `vscode-light`    | VS Code Light              | light |
| `Obsidian`       | `obsidian`        | Obsidian Dark              | dark  |
| `ObsidianLight`  | `obsidian-light`  | Obsidian Light             | light |
| `Dracula`        | `dracula`         | Dracula                    | dark  |
| `Nord`           | `nord`            | Nord                       | dark  |
| `AtomOneDark`    | `atom-one-dark`   | Atom One Dark              | dark  |
| `GitHubLight`    | `github-light`    | GitHub Light               | light |
| `GitHubDark`     | `github-dark`     | GitHub Dark                | dark  |
| `SolarizedLight` | `solarized-light` | Solarized Light            | light |
| `ViraInspired`   | `vira-inspired`   | Vira-inspired (original)   | dark  |

</details>

Override any color for a brand palette; every token is a `ThemeColor`:

```rust
use config_editor_leptos::{EditorSettings, EditorTheme, ThemeColor, ThemeColors};

let brand = ThemeColors::new()
    .set(ThemeColor::Background, "#101418")
    .set(ThemeColor::Key, "#ffcc66")
    .set(ThemeColor::Selection, "rgb(255 204 102 / 25%)");
let settings = EditorSettings { theme: EditorTheme::Nord, colors: brand, ..EditorSettings::default() };
// <ConfigEditor settings=settings .../>
```

The same tokens are CSS variables (`--config-editor-key`, …), so a stylesheet
can set them too. The diff colors are `Added`, `Modified` and `Removed`
(`--config-editor-added`, `--config-editor-modified`,
`--config-editor-removed`, which follows `Error` unless set), the line
backgrounds `AddedLine` and `ModifiedLine`, the preview's `DiffRemoved*` and
`DiffCurrent*`, and the footer's change count `StatusChanges`
(`--config-editor-status-changes`, which follows `Modified` unless set).
The find widget uses `SearchMatch`, `SearchMatchCurrent` and
`SearchMatchCurrentBorder` (`--config-editor-search-match`, …) for matches,
and `InputBackground`, `InputBorder` and `ToggleActive` for its fields and
option buttons. Folding uses `FoldToggle` for the gutter chevrons (following
the line numbers' color unless set) and `FoldChip` / `FoldChipText` for the
`…` after a folded line. Read-only values and the gutter lock use `Locked`
(`--config-editor-locked`, which follows `Muted` unless set), secret
placeholders `SecretChip` and `SecretChipBorder`.

The tabs of `ConfigDocuments` use `TabBar` (the strip, following `Panel`),
`TabBackground` / `TabActiveBackground` (following `Panel` / `Background`),
`TabText` / `TabActiveText` (following `Muted` / `Text`), `TabBorder`
(following `Border`), `TabActiveBorder` (the line over the active tab,
following `Accent`) and `TabDirty` (the unsaved-changes dot, following
`Text`): `--config-editor-tab-bar`, `--config-editor-tab-bg`,
`--config-editor-tab-active-bg`, `--config-editor-tab-text`,
`--config-editor-tab-active-text`, `--config-editor-tab-border`,
`--config-editor-tab-active-border`, `--config-editor-tab-dirty`. The view's
root carries the theme too (`data-color-theme`), so they follow the theme.

## Cargo features

All are on by default; turn off what you don't use to shrink the build.

| Feature             | Enables                                                                                                                                                                                                                                          |
| ------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `schema-validation` | Live JSON Schema diagnostics (`jsonschema`, the heaviest dependency). Completions, hover docs and schema-shaped reading work without it, from the schema alone; leave it out when documents are validated on a server. Syntax is always checked. |
| `yaml`              | `Language::YAML` (`noyalib`)                                                                                                                                                                                                                     |
| `json`              | `Language::JSON`                                                                                                                                                                                                                                 |
| `toml`              | `Language::TOML` (`toml`)                                                                                                                                                                                                                        |
| `hcl`               | `Language::HCL` (`hcl-rs`)                                                                                                                                                                                                                       |
| `xml`               | `Language::XML` (`quick-xml`)                                                                                                                                                                                                                    |

```toml
# YAML with live schema diagnostics:
config-editor-leptos = { version = "0.1", default-features = false, features = ["schema-validation", "yaml"] }
# YAML and TOML with schema completions and hover, validated on a server:
config-editor-leptos = { version = "0.1", default-features = false, features = ["yaml", "toml"] }
```

With no language feature, use your own languages through `Language::new`.

## License

MIT. See [LICENSE](LICENSE).

[Leptos]: https://leptos.dev
[`Schema::warnings`]: https://docs.rs/config-editor-leptos/latest/config_editor_leptos/struct.Schema.html#method.warnings
[`PRESET_CSS`]: https://docs.rs/config-editor-leptos/latest/config_editor_leptos/constant.PRESET_CSS.html
[`@pngwasi/config-editor`]: https://www.npmjs.com/package/@pngwasi/config-editor
[`@pngwasi/config-editor-react`]: https://www.npmjs.com/package/@pngwasi/config-editor-react
