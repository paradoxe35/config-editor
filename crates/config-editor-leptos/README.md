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
use config_editor_leptos::{ConfigEditor, Language};

#[component]
fn Settings() -> impl IntoView {
    let saved = RwSignal::new("listen: 7835\n".to_string());
    view! {
        <ConfigEditor
            language=Language::YAML
            value=saved
            on_change=move |text| saved.set(text)
            schema=include_str!("schema.json")
            show_toolbar=true
            show_footer=true
        />
    }
}
```

The editor injects its stylesheet ([`PRESET_CSS`]) by default.

## `<ConfigEditor>` props

| Prop                           | Type                                       | Default          | Purpose                                                                                                                              |
| ------------------------------ | ------------------------------------------ | ---------------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| `language`                     | `Language`                                 | `Language::YAML` | Language of the document the host stores: `initial_value`, `value`, `baseline` and `on_change` are always in it.                     |
| `edit_as`                      | `Language`, `Option<Language>` or a signal | unset            | Show and edit the document in another language. Changing it converts the open document in place, unsaved edits included.             |
| `initial_value`                | `String`                                   | `""`             | Starting text, also the diff baseline (uncontrolled).                                                                                |
| `value`                        | `Signal<String>`                           | —                | Controlled text. Updates flow in; the caret stays on the same text and the scroll position is kept.                                  |
| `on_change`                    | `Fn(String)`                               | —                | Called with the document after every edit (after each typing pause while editing as another language).                               |
| `on_edit_as_change`            | `Fn(Language)`                             | —                | Called when an `edit_as` switch completes. A switch waits while the document cannot convert yet (TOML has no `null`).                |
| `on_schema_warning`            | `Fn(String)`                               | console warning  | Called once per thing in `schema` the editor cannot use, such as a `patternProperties` regex its engine cannot compile.               |

While editing as another language, edits reach `on_change` after each typing
pause. Call `config_editor_leptos::flush_pending_edits()` before reading the
document from code (a Save button) to send an edit still waiting first.
| `baseline`                     | `Signal<String>`                           | the document at mount | Text the diff gutter and the dirty flag compare against; set it to the saved text after saving. See [Saving](#saving-and-unsaved-edits). |
| `schema`                       | `&str`, `String` or `Schema`               | —                | JSON Schema (draft 2020-12 and earlier) for diagnostics, completion and hover. Pass a compiled `Schema` to share one across editors. |
| `theme`                        | `EditorTheme` or a signal                  | `VsCodeDark`     | Color theme.                                                                                                                         |
| `colors`                       | `ThemeColors` or a signal                  | —                | Colors overriding the theme's.                                                                                                       |
| `read_only`                    | `bool` or a signal                         | `false`          | View mode: navigation, selection, highlighting, diagnostics, diffs and hover keep working. Switches live, completions included.      |
| `file_name`                    | `String`                                   | `untitled.<ext>` | Name shown in the toolbar.                                                                                                           |
| `class`                        | `String`                                   | —                | Extra class on the editor root.                                                                                                      |
| `styled`                       | `bool`                                     | `true`           | Inject `PRESET_CSS`. Set `false` to serve or replace the styles yourself.                                                            |
| `show_toolbar` / `show_footer` | `bool`                                     | `false`          | Built-in file toolbar and status bar (problem counts, line count).                                                                   |
| `show_diagnostics`             | `bool`                                     | `true`           | Syntax and schema problems, underlined with a message on hover.                                                                      |
| `enable_completions`           | `bool`                                     | `true`           | Schema suggestions (`Ctrl+Space`).                                                                                                   |
| `auto_trigger_completions`     | `bool`                                     | `true`           | Open suggestions while typing; when `false`, only `Ctrl+Space` opens them.                                                           |
| `enable_hover_docs`            | `bool`                                     | `true`           | Schema docs when hovering a key or a documented value: description, type, default, examples, deprecation and a docs link.           |
| `enable_highlighting`          | `bool`                                     | `true`           | Syntax colors.                                                                                                                       |
| `enable_auto_closing`          | `bool`                                     | `true`           | Close brackets, quotes and XML tags; wrap selections; delete empty pairs.                                                            |
| `show_diff`                    | `bool`                                     | `true`           | Added, modified and removed line markers with a before/after preview and "Revert this change".                                       |
| `on_change_summary`            | `Fn(ChangeSummary)`                        | —                | Called with the lines added, removed and modified against `baseline`, and the number of changes, whenever that changes.              |
| `handle`                       | `EditorHandle`                             | —                | Find fields, follow the caret and jump to a field from the host; see [Paths and positions](#paths-and-positions).                   |
| `on_path_change`               | `Fn(FieldPath)`                            | —                | Called with the path of what the caret is on each time it changes (and once on mount), for a breadcrumb.                             |
| `host_diagnostics`             | `Signal<Vec<HostDiagnostic>>`              | —                | Problems the host found (a server's validation), by path or range, shown with the editor's own; see [Host diagnostics](#host-diagnostics). |
| `enable_search`                | `bool`                                     | `true`           | Find and replace (`Ctrl+F`, `Ctrl+H`, `F3`). When `false`, those keys are left to the browser.                                       |
| `enable_go_to_line`            | `bool`                                     | `true`           | Go to line (`Ctrl+G`). When `false`, the key is left to the browser.                                                                 |
| `on_save`                      | `Fn(SaveRequest)`                          | —                | Called on `Ctrl+S` / `Cmd+S` and `EditorHandle::save` with the document in `language`. Nothing saves by itself; without it `Ctrl+S` is the browser's. |
| `enable_save_shortcut`         | `bool` or a signal                         | `true`           | Whether `Ctrl+S` saves (with `on_save`); `false` leaves the key to the browser, and only `EditorHandle::save` saves.                  |
| `on_dirty_change`              | `Fn(bool)`                                 | —                | Called with whether the text differs from `baseline`: once on mount, then each time that changes.                                    |
| `on_baseline_conflict`         | `Fn(BaselineConflict)`                     | —                | Called when a new `baseline` arrives under unsaved edits (the notice opens) and when that is settled.                                |
| `enable_folding`               | `bool`                                     | `true`           | Folding by structure: gutter chevrons and the folding keys; see [Folding](#folding).                                                 |

To build your own chrome, turn the built-in toolbar or footer off and compose
`ConfigEditorToolbar` / `ConfigEditorFooter`, which take the same `theme`
and `colors` so they match the editor.

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

| `EditorHandle` method  | Description                                                                                                                                                                                     |
| ---------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `range_of(path)`       | `Option<TextRange>`: the key and its value when they share a line (`listen: 7835`, `<listen>7835</listen>`), the key alone when the value is a block below it, the value alone for a list item. |
| `path_at(position)`    | `Option<FieldPath>` of what a `TextPosition` is on: the key under or just before it, else the value or container holding it (the root at the top level).                                         |
| `current_path()`       | The caret's path. Reactive: read it in a closure to follow the caret (as `on_path_change` does). Computed only while something reads it.                                                       |
| `reveal(path)`         | Scrolls to the field, selects `range_of(path)` and focuses the editor; `false` when there is no such field.                                                                                      |
| `fold(path)` / `unfold(path)` | Fold the block the field's key opens / open the folds around the field; `false` when there was nothing to do.                                                                     |
| `fold_all()` / `unfold_all()` | Fold every block / open every fold.                                                                                                                                                      |
| `folded_lines()`       | The closed folds as one-based `(first, last)` line pairs. Reactive.                                                                                                                             |
| `focus()`, `text()`    | Focus the editor; the text on screen.                                                                                                                                                         |

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
use config_editor_leptos::{ConfigEditor, HostDiagnostic, Severity};

let problems = RwSignal::new(Vec::new());
// After the server answered:
problems.set(vec![
    HostDiagnostic::at_path("/web/listen", Severity::Error, "port 80 is taken")
        .with_code("E_PORT")
        .with_source("burrow")
        .with_docs_url("https://example.com/docs/ports"),
    HostDiagnostic::document(Severity::Warning, "restart needed to apply"),
]);
view! { <ConfigEditor value=text host_diagnostics=problems show_footer=true /> }
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

## Schema help

Completions and hover read these keywords, with or without the
`schema-validation` feature:

| Keyword                                      | Shown                                                                                                                       |
| -------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------- |
| `description`, `type`, `default`, `examples` | Suggestion detail and hover. A key is marked `required` when its parent lists it.                                           |
| `deprecated: true`                           | The key is struck through and listed last, its hover says so, and a key written in the document gets a warning.             |
| `deprecationMessage`                         | What to use instead, in the hover and the warning.                                                                          |
| `x-docs-url`                                 | A "Documentation" link in the hover, opening in a new tab (`http(s)` addresses only).                                       |
| `x-enum-descriptions` (and `x-enum-titles`)  | Arrays parallel to `enum`: each value's own line in the suggestion list and in the hover of that value in the document.     |
| `title` / `description` of a `const` branch  | The same for `oneOf`/`anyOf` lists of documented constants.                                                                 |

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
  "timeout": { "type": "integer", "deprecated": true, "deprecationMessage": "Use deadline instead." }
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

`Cmd` works in place of `Ctrl` on macOS.

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
| `FieldPath`, `TextPosition`, `TextRange`   | Paths (JSON pointers or segments) and one-based places in the text; `Language::range_of(text, path)` and `Language::path_at(text, position)` map between them.                                                                                          |
| `language::FoldRange`                      | A range that folds (0-based `start` stays visible, lines through `end` hide); `Language::fold_ranges(text)` lists them.                                                                                                                                   |
| `check_syntax(language, text)`             | Syntax problems only.                                                                                                                                                                                                                                       |
| `unified_diff(baseline, current, context)` | The `diff -u` of two texts (`--- baseline` / `+++ current` headers, `@@` hunks with `context` lines around changes, `\ No newline at end of file`); empty when they have the same lines. Line endings are compared as LF.                                  |
| `ChangeSummary`                            | `added`, `removed`, `modified` lines and `hunks` (separate changes), as the diff gutter counts them; `ChangeSummary::between(baseline, current)`, `is_empty()`.                                                                                             |
| `Diagnostic`                               | `line`, `column`, `end_column` (one-based, `None` when unplaced), `message`, `severity` (`Severity::Error`, `Warning`, `Info`, `Hint`), `origin`, and for host problems `code`, `source`, `docs_url`.                                                                    |
| `HostDiagnostic`, `DiagnosticTarget`       | A problem the host found, by path, range or for the whole document; see [Host diagnostics](#host-diagnostics).                                                                                                                                                |
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

| Kind of language                        | Method               | Adds                                                              | Helper                         |
| --------------------------------------- | -------------------- | ----------------------------------------------------------------- | ------------------------------ |
| Key/value (`key: value`, `key = value`) | `value_follows_key`  | Completing a key that already has a value renames just the key    | —                              |
|                                         | `line_has_value`     | A space typed after a complete value ends the entry               | `value_after_separator`        |
|                                         | `empty_value_slots`  | `Tab` walks the empty values of an accepted block                 | `empty_slots_at_line_ends`     |
| Markup (`<k>…</k>`)                     | `closing_markup_len` | Enter after a typed value steps past the end tag the editor wrote | —                              |
| With trailing comments                  | `comment_start`      | Edit-as keeps a line's comment when the line is renamed or moves  | `comment_start_outside_quotes` |
| Any                                     | `fold_ranges`        | What folds; the default folds by indentation, `config`'s brackets and comments, and multi-line strings | `indent_fold_ranges`, `bracket_fold_ranges`, `comment_fold_ranges`, `string_body_fold_ranges`, `merge_fold_ranges` |

`config_editor_leptos::language` also exports the other helpers the built-in
languages use (tokens, schema lookups, inline-container scanning, indent
detection). `tests/custom_language.rs` is a complete example.

## Themes and colors

Eleven themes ship as `EditorTheme` (all of them in `EditorTheme::ALL`;
their slugs parse with `FromStr`, as the JS package names them).

<details>
<summary>The eleven themes</summary>

| `EditorTheme` | Slug | Name | Look |
|---|---|---|---|
| `VsCodeDark` | `vscode-dark` | VS Code Dark (the default) | dark |
| `VsCodeLight` | `vscode-light` | VS Code Light | light |
| `Obsidian` | `obsidian` | Obsidian Dark | dark |
| `ObsidianLight` | `obsidian-light` | Obsidian Light | light |
| `Dracula` | `dracula` | Dracula | dark |
| `Nord` | `nord` | Nord | dark |
| `AtomOneDark` | `atom-one-dark` | Atom One Dark | dark |
| `GitHubLight` | `github-light` | GitHub Light | light |
| `GitHubDark` | `github-dark` | GitHub Dark | dark |
| `SolarizedLight` | `solarized-light` | Solarized Light | light |
| `ViraInspired` | `vira-inspired` | Vira-inspired (original) | dark |

</details>

Override any color for a brand palette; every token is a `ThemeColor`:

```rust
use config_editor_leptos::{EditorTheme, ThemeColor, ThemeColors};

let brand = ThemeColors::new()
    .set(ThemeColor::Background, "#101418")
    .set(ThemeColor::Key, "#ffcc66")
    .set(ThemeColor::Selection, "rgb(255 204 102 / 25%)");
// <ConfigEditor theme=EditorTheme::Nord colors=brand .../>
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
`…` after a folded line.

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
