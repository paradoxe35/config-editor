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
[`@pngwasi/config-editor-react`]; they wrap this crate.

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
| `value`                        | `Signal<String>`                           | —                | Controlled text. Updates flow in; the caret stays on the same text.                                                                  |
| `on_change`                    | `Fn(String)`                               | —                | Called with the document after every edit (after each typing pause while editing as another language).                               |
| `on_edit_as_change`            | `Fn(Language)`                             | —                | Called when an `edit_as` switch completes. A switch waits while the document cannot convert yet (TOML has no `null`).                |

While editing as another language, edits reach `on_change` after each typing
pause. Call `config_editor_leptos::flush_pending_edits()` before reading the
document from code (a Save button) to send an edit still waiting first.
| `baseline`                     | `Signal<String>`                           | `initial_value`  | Text the diff gutter compares against; set it to the saved text to clear the diff.                                                   |
| `schema`                       | `&str`, `String` or `Schema`               | —                | JSON Schema (draft 2020-12 and earlier) for diagnostics, completion and hover. Pass a compiled `Schema` to share one across editors. |
| `theme`                        | `EditorTheme` or a signal                  | `VsCodeDark`     | Color theme.                                                                                                                         |
| `colors`                       | `ThemeColors` or a signal                  | —                | Colors overriding the theme's.                                                                                                       |
| `read_only`                    | `bool` or a signal                         | `false`          | View mode: navigation, selection, highlighting, diagnostics, diffs and hover keep working.                                           |
| `file_name`                    | `String`                                   | `untitled.<ext>` | Name shown in the toolbar.                                                                                                           |
| `class`                        | `String`                                   | —                | Extra class on the editor root.                                                                                                      |
| `styled`                       | `bool`                                     | `true`           | Inject `PRESET_CSS`. Set `false` to serve or replace the styles yourself.                                                            |
| `show_toolbar` / `show_footer` | `bool`                                     | `false`          | Built-in file toolbar and status bar (problem counts, line count).                                                                   |
| `show_diagnostics`             | `bool`                                     | `true`           | Syntax and schema problems, underlined with a message on hover.                                                                      |
| `enable_completions`           | `bool`                                     | `true`           | Schema suggestions (`Ctrl+Space`).                                                                                                   |
| `auto_trigger_completions`     | `bool`                                     | `true`           | Open suggestions while typing; when `false`, only `Ctrl+Space` opens them.                                                           |
| `enable_hover_docs`            | `bool`                                     | `true`           | Schema description, type, default and examples when hovering a key.                                                                  |
| `enable_highlighting`          | `bool`                                     | `true`           | Syntax colors.                                                                                                                       |
| `enable_auto_closing`          | `bool`                                     | `true`           | Close brackets, quotes and XML tags; wrap selections; delete empty pairs.                                                            |
| `show_diff`                    | `bool`                                     | `true`           | Added, modified and removed line markers with a before/after preview and "Revert this change".                                       |
| `on_change_summary`            | `Fn(ChangeSummary)`                        | —                | Called with the lines added, removed and modified against `baseline`, and the number of changes, whenever that changes.              |

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

## Keyboard

| Keys                                         | Action                                                                           |
| -------------------------------------------- | -------------------------------------------------------------------------------- |
| `Ctrl+Space`                                 | Open suggestions (with schema examples)                                          |
| `↑` `↓` `PageUp` `PageDown`, `Enter` / `Tab` | Move through suggestions, accept                                                 |
| `Tab` after accepting a block                | Jump to the next empty value                                                     |
| `Tab` / `Shift+Tab`                          | Indent / outdent the line or selected lines                                      |
| `Enter`                                      | New line with the right indentation; steps out of a just-typed string or element |
| `Ctrl+Enter` / `Ctrl+Shift+Enter`            | Open a line below / above                                                        |
| `Ctrl+/`                                     | Toggle comments                                                                  |
| `Ctrl+Z`, `Ctrl+Shift+Z` / `Ctrl+Y`          | Undo, redo                                                                       |
| `F8` / `Shift+F8`                            | Next / previous problem                                                          |
| `Alt+F5` / `Shift+Alt+F5`                    | Next / previous change against the baseline                                      |
| `Alt+F3` / `Shift+Alt+F3`                    | Next / previous change, opening its preview with the focus on Revert             |
| `Enter` or `R` in a change preview           | Revert this change (`Escape` closes the preview and returns to the text)         |
| `Home`                                       | First non-blank character, then the line start                                   |
| `Ctrl+C` / `Ctrl+X` with nothing selected    | Copy / cut the whole line (pasting it inserts above)                             |
| `Escape`                                     | Close popups, then collapse the selection                                        |

`Cmd` works in place of `Ctrl` on macOS.

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
| `Schema`                                   | A compiled JSON Schema: `compile`, `validate(language, text)`, `convert(from, text, to)`. Cheap to clone and share.                                                                                                                                         |
| `SchemaSource`                             | What the `schema` prop takes: JSON text or a `Schema`.                                                                                                                                                                                                      |
| `check_syntax(language, text)`             | Syntax problems only.                                                                                                                                                                                                                                       |
| `unified_diff(baseline, current, context)` | The `diff -u` of two texts (`--- baseline` / `+++ current` headers, `@@` hunks with `context` lines around changes, `\ No newline at end of file`); empty when they have the same lines. Line endings are compared as LF.                                  |
| `ChangeSummary`                            | `added`, `removed`, `modified` lines and `hunks` (separate changes), as the diff gutter counts them; `ChangeSummary::between(baseline, current)`, `is_empty()`.                                                                                             |
| `Diagnostic`                               | `line`, `column`, `end_column` (one-based, `None` when unplaced), `message`, `severity` (`Severity::Error` / `Warning`).                                                                                                                                    |
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

`config_editor_leptos::language` also exports the other helpers the built-in
languages use (tokens, schema lookups, inline-container scanning, indent
detection). `tests/custom_language.rs` is a complete example.

## Themes and colors

Eleven themes ship as `EditorTheme`: `VsCodeDark`, `VsCodeLight`,
`Obsidian`, `ObsidianLight`, `Dracula`, `Nord`, `AtomOneDark`,
`GitHubLight`, `GitHubDark`, `SolarizedLight`, `ViraInspired`
(`EditorTheme::ALL`; slugs such as `"vscode-dark"` parse with `FromStr`).

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
[`PRESET_CSS`]: https://docs.rs/config-editor-leptos/latest/config_editor_leptos/constant.PRESET_CSS.html
[`@pngwasi/config-editor`]: https://www.npmjs.com/package/@pngwasi/config-editor
[`@pngwasi/config-editor-react`]: https://www.npmjs.com/package/@pngwasi/config-editor-react
