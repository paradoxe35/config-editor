# leptos-config-editor

A config editor component for Leptos applications (YAML today, JSON next). It renders only the editor;
the host application owns its page layout, branding, file loading, and saving.

## Usage

```toml
[dependencies]
leptos-config-editor = { path = "crates/leptos-config-editor" }
```

```rust
use leptos::prelude::*;
use leptos_config_editor::{ConfigEditor, EditorTheme};

let initial_value = "listen: 7835\n";

view! {
    <ConfigEditor
        initial_value=initial_value
        class="my-config-editor"
        theme=EditorTheme::Obsidian
    />
}
```

`theme` can be set to any bundled [`EditorTheme`] variant. It also accepts a
reactive Leptos signal, which lets an application switch themes without
recreating the editor.

`schema_json` is optional. Pass it when schema-backed diagnostics and
completions are needed:

```rust
<ConfigEditor
    initial_value=initial_value
    schema_json=include_str!("config.schema.json")
/>
```

In uncontrolled mode, use `on_change` to persist edits:

```rust
on_change=Callback::new(|value: String| {
    // Send the updated value to your application's state or save handler.
})
```

`initial_value` is optional and defaults to an empty document. `schema_json` is
an optional JSON Schema string. When supplied, it enables live schema
diagnostics and property/value completions. Suggestions open while typing by
default and reopen after a completion is accepted, so selecting an object
property immediately offers the keys inside it. They do not open on new lines,
paste, undo/redo, or deletion. Set `auto_trigger_completions=false` to make
them explicit with `Ctrl+Space`. Completions follow `$ref`, `allOf`/`anyOf`/`oneOf`,
`properties`, `patternProperties`, `additionalProperties`, `items`, and
`prefixItems`; enum, const, `examples`/`example`, default, nullable `null`, and
boolean values can be suggested in value positions. YAML and schema validation
errors are marked at their source location with a hover- and keyboard-focusable
inline message; there is no separate Problems panel. Syntax errors are diagnosed
even without a schema.
For schema-invalid empty values, the inline marker points to the empty value
slot after the colon instead of underlining the key.
The editor pairs quotes and brackets, wraps selected text, skips existing
closing pairs, and removes an empty pair on Backspace.

`initial_value` seeds the editor and its diff baseline. It is not a controlled
value prop. For externally controlled content and live diffs, provide reactive
`value` and `baseline` signals. The editor sends each edit through `on_change`;
update `value` in that callback to keep the external source of truth in sync.
The host can accept changes by updating its baseline signal:

```rust
let baseline = RwSignal::new(original.to_string());
let final_value = RwSignal::new(original.to_string());

view! {
    <ConfigEditor
        initial_value=original
        value=final_value
        baseline=baseline
        on_change=Callback::new(move |value| final_value.set(value))
    />
    <button on:click=move |_| baseline.set(final_value.get())>
        "Apply changes"
    </button>
}
```

Added, modified, and deleted lines are marked against the baseline. Blank-line
insertions retain following-line alignment, and deletions at the end of the
document receive a gutter marker after the final line. Updating `baseline`
after applying changes clears those decorations. Click a modified-line or
deleted-line gutter marker to open an inline peek with the previous text (and
the current text for modifications). The peek can be closed with its close
button or Escape. It opens on the side with more available viewport space and
limits its height to fit, scrolling its contents when needed. Gutter change
indicators retain their thin appearance and become slightly more prominent on
hover or keyboard focus. The root demo shows the same flow.

## Editor options

| Prop                       | Default         | Description                                                                                                      |
| -------------------------- | --------------- | ---------------------------------------------------------------------------------------------------------------- |
| `language`                 | `Yaml`          | Document language (`Yaml` or `Json`); selects parsing, completions, highlighting, comments |
| `initial_value`            | empty           | Initial document                                                                                            |
| `value`                    | none            | Optional reactive final value; changes synchronize into the editor                                               |
| `baseline`                 | `initial_value` | Optional reactive baseline used to show line diffs                                                               |
| `schema_json`              | none            | JSON Schema used for validation and completions                                                                  |
| `file_name`                | per language    | Name shown when the built-in toolbar is enabled (`untitled.yaml` / `untitled.json`)                                                                  |
| `class`                    | none            | Extra class on the editor root for host styles                                                                   |
| `theme`                    | `VsCodeDark`    | Bundled `EditorTheme` preset; accepts a reactive signal for live switching                                       |
| `on_change`                | none            | Callback with the updated document                                                                               |
| `styled`                   | `true`          | Include the preset editor stylesheet                                                                             |
| `show_toolbar`             | `false`         | Show the built-in file/schema toolbar                                                                            |
| `show_footer`              | `false`         | Show the built-in status footer                                                                                  |
| `show_diagnostics`         | `true`          | Show inline parse/schema diagnostics and hover messages                                                          |
| `enable_completions`       | `true`          | Enable schema-backed completions (`Ctrl+Space`)                                                                  |
| `auto_trigger_completions` | `true`          | Open suggestions while typing and after accepting a key (values close the list)                                  |
| `enable_hover_docs`        | `true`          | Show the schema description, type, default, and examples when hovering a key                                     |
| `enable_highlighting`      | `true`          | Enable syntax colors for the document language                                                                                        |
| `enable_auto_closing`      | `true`          | Pair quotes/brackets, wrap selections, skip closing pairs, and delete empty pairs                                |
| `show_diff`                | `true`          | Show line changes against `baseline`                                                                             |
| `read_only`                | `false`         | View mode: disable typing, completions, and edits while keeping highlighting, diagnostics, diffs, and hover docs |

Read-only view mode keeps syntax highlighting, inline diagnostics, diff
gutter, and schema hover documentation, but the document cannot be edited:
typing, Enter/Tab insertions, bracket pairing, comment toggling, undo/redo,
and completions are all disabled. The demo app enables it with
`?readonly=1`.

The toolbar and footer are also exported as composable `ConfigEditorToolbar` and
`ConfigEditorFooter` components. Both accept arbitrary children and an optional
`class` and `theme` signal, so the host can place its own file controls, apply
actions, or status content around the editor and keep them in sync with its
theme without enabling built-in chrome.

## Themes

The preset includes eleven selectable themes, with seven dark and four light
options. `VsCodeDark` is the default and preserves the original VS Code-like
appearance. `Obsidian` provides a near-black background, blue keys, and soft
green values; `ObsidianLight` is its light-mode counterpart. `AtomOneDark` and
the GitHub Light/Dark pair add familiar editor palettes. `ViraInspired` is an
original, muted dark palette informed by Vira's
publicly described focus on lower visual noise and comfortable long sessions;
it is not an exact reproduction of Vira's proprietary theme.

| `EditorTheme` variant | Mode  | Style                                                           |
| --------------------- | ----- | --------------------------------------------------------------- |
| `VsCodeDark`          | Dark  | VS Code-inspired default                                        |
| `VsCodeLight`         | Light | VS Code-inspired light palette                                  |
| `Obsidian`            | Dark  | Near-black palette with blue keys and green values              |
| `ObsidianLight`       | Light | Soft neutral background with blue keys and green values         |
| `Dracula`             | Dark  | Purple-accented palette                                         |
| `Nord`                | Dark  | Cool blue-gray palette                                          |
| `AtomOneDark`         | Dark  | Atom One Dark-inspired palette                                  |
| `GitHubLight`         | Light | Bright neutral palette                                          |
| `GitHubDark`          | Dark  | GitHub-inspired dark palette                                    |
| `SolarizedLight`      | Light | Warm Solarized palette                                          |
| `ViraInspired`        | Dark  | Original muted palette based on publicly described design goals |

The demo includes a theme selector for all eleven options. Vira describes its
theme as a dark-first successor to Material Theme with multiple variants and
an emphasis on reducing visual noise and cognitive load ([Vira Theme](https://vira.build),
[official assets repository](https://github.com/vira-soft/vira-assets)). This
project uses those high-level design goals only; it is not affiliated with Vira
Software and does not bundle Vira theme assets.

The additional palettes are kept in `src/themes.css`; the VS Code and Obsidian palettes and
shared layout styling remain in `src/preset.css`. Applications can build on
these styles with `class` and custom CSS, or set `styled=false` and provide
their own complete theme. Typography is deliberately shared across themes:
`--config-editor-font-size`, `--config-editor-line-height`,
`--config-editor-padding-top`, and `--config-editor-padding-bottom` control every
theme, so switching palettes never changes text size or line spacing.
The editor grows with its content up to `--config-editor-max-height` (default
`80vh`), then scrolls internally. Set `--config-editor-height` for a fixed editor
height, and `--config-editor-min-height` to customize the minimum. For example:

```css
.my-config-editor {
  --config-editor-height: 32rem;
  --config-editor-min-height: 12rem;
  --config-editor-max-height: 80vh;
}
```

Suggestions are not capped to a fixed number of rows. The preset limits the
popup height to available viewport space, anchors it near the caret, and scrolls
the suggestion list internally when needed. Use Up/Down to navigate, Enter or
Tab to accept, and Escape to dismiss. Hovering a suggestion highlights it, so only one row is ever
highlighted. Each suggestion shows its symbol kind,
schema type, and description. Matching favours exact prefixes, then
case-insensitive prefixes, word initials (for example `mc` for
`max_connections`), substrings, and ordered subsequences. Schema property
suggestions omit keys already present in the current mapping; partially typed
quoted keys are replaced with valid YAML double-quoted keys. Selecting a
property inserts its schema `default` when present. For object properties, the
editor expands required fields recursively, inserts their defaults, and places
the caret at the first required field without a default. Expanded indentation
follows the document's existing style, including four-space and tab-indented
documents.

## Keyboard shortcuts

| Shortcut                                                       | Action                                                                                                |
| -------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| `Ctrl+/` (Windows/Linux), `Cmd+/` (macOS)                      | Toggle YAML line comments on the selected lines, or the current line                                  |
| `Ctrl+Space`                                                   | Open schema completions                                                                               |
| Up/Down, Enter/Tab, Escape                                     | Navigate, accept, or dismiss an open completion list                                                  |
| `Tab` / `Shift+Tab` (without an open completion list)          | Indent selected lines / outdent lines; with no selection, Tab inserts the document's indentation unit |
| `Enter` (without an open completion list)                      | Insert a newline and continue YAML indentation                                                        |
| `Ctrl+Z` / `Cmd+Z`; `Ctrl+Y` or `Ctrl+Shift+Z` / `Cmd+Shift+Z` | Undo / redo text entry and editor actions                                                             |
| Standard platform text-editing shortcuts                       | Selection, navigation, and clipboard are handled by the browser textarea                              |

YAML parsing follows the YAML 1.2-compatible parser and reports syntax errors,
including repeated mapping keys, as inline diagnostics. The YAML 1.2
specification requires mapping keys to be unique; a repeated key is not treated
as an override. See the [YAML 1.2.2 specification](https://yaml.org/spec/1.2.2/),
including its sections on mappings, node comparison, and document structure.

Use `styled=false` to make the component headless and apply your own CSS. The
DOM uses `config-editor__*` class names; `PRESET_CSS` can be used as a starting
point or selectively adapted in the host project.

```rust
view! {
    <ConfigEditor
        initial_value=initial_value
        schema_json=schema
        styled=false
        class="my-editor"
    />
}
```

```css
.my-editor .config-editor__input {
  min-height: 32rem;
  font-size: 0.875rem;
}
```

## Cargo features and graceful degradation

The Cargo features `schema-validation`, `completion`,
`syntax-highlighting`, `diagnostics`, and `preset-style` can be disabled
at build time. They are enabled by default. Each feature only removes its
own layer; everything else keeps working:

| Feature               | Prop it gates                                    | When disabled                                                                                                                                                                                                             |
| --------------------- | ------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `schema-validation`   | Schema errors from `schema_json`                 | Schema violations are no longer reported; YAML syntax and repeated-key errors still are while `diagnostics` is on. Hover docs and completions keep reading the parsed schema value. Pulls in the `jsonschema` dependency. |
| `completion`          | `enable_completions`, `auto_trigger_completions` | No suggestion popup: typing and `Ctrl+Space` stay silent. Implies `schema-validation`.                                                                                                                                    |
| `syntax-highlighting` | `enable_highlighting`                            | The highlight overlay is skipped; the textarea keeps its text and caret behavior.                                                                                                                                         |
| `diagnostics`         | `show_diagnostics`                               | No markers, no status-button count, and F8 has nowhere to go. Syntax errors are hidden too.                                                                                                                               |
| `preset-style`        | `styled`                                         | No stylesheet is injected; the semantic `config-editor__*` markup stays for host CSS.                                                                                                                                       |

Props degrade the same way at runtime: `show_diagnostics=false` hides all
markers exactly like disabling `diagnostics`, and `enable_completions=false`
silences the popup exactly like disabling `completion`. `read_only=true`
additionally forces completions off and swallows editing keys, while keeping
highlighting, diagnostics, diffs, and hover docs. Hover documentation needs
only `enable_hover_docs` plus a parsable `schema_json`; it does not need the
`schema-validation` feature because it reads the parsed schema value rather
than the validator. Features compose: `--no-default-features` leaves a plain
editing surface with Enter/Tab indentation, bracket pairing, comments,
undo/redo, and line diffs; adding back `--features diagnostics,schema-validation`
restores schema diagnostics with it.

## Non-goals: formatter and ghost text

The editor indents but never reformats. `Enter` continues the document's own
indentation unit (`indent_unit`, with a two-space fallback) and accepting an
object completion expands its required fields, but there is no
format-document command; that is deliberate scope, not a missing piece.
Suggestions likewise never render as inline ghost text inside the textarea.
They live in the completion popup, and only real, undoable edits enter the
document. Both choices keep the textarea the single source of truth, so the
highlight overlay, the UTF-16 caret accounting, and undo/redo never have to
reconcile speculative text.
