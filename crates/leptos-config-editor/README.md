# leptos-config-editor

A lightweight, VS Code–style config editor component for [Leptos], plus a
headless API for the same parsing, validation, and conversion.

- **Languages:** YAML (default), JSON, TOML, HCL, XML.
- **One schema for all of them:** every language parses into the same data
  model, so a single JSON Schema drives diagnostics, completions, and hover
  documentation everywhere.
- **Conversion:** show a document in one language while the host keeps it in
  another (`edit_as`), or convert headlessly with `Language::convert`.
- **Editing:** schema completion with formatted inserts, auto-closing pairs,
  comment toggling, undo/redo, a diff gutter against a baseline, read-only
  view mode, and eleven bundled themes.

## The component

```rust
use leptos::prelude::*;
use leptos_config_editor::{ConfigEditor, Language};

#[component]
fn Settings() -> impl IntoView {
    let saved = RwSignal::new("listen: 7835\n".to_string());
    view! {
        <ConfigEditor
            language=Language::Yaml
            value=saved
            on_change=move |text| saved.set(text)
            schema=include_str!("schema.json")
        />
    }
}
```

| Prop                                                                                                                                                 | Purpose                                                                                |
| ---------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------- |
| `language`                                                                                                                                           | Language of `initial_value`, `value`, `baseline`, and `on_change` (default YAML).      |
| `edit_as`                                                                                                                                            | Edit in another language; edits convert back to `language` after each pause in typing. |
| `initial_value` / `value`                                                                                                                            | Seed text, or a host signal kept in sync.                                              |
| `on_change`                                                                                                                                          | Closure called with the updated document.                                              |
| `baseline`                                                                                                                                           | Text to diff against (gutter markers and inline peeks).                                |
| `schema`                                                                                                                                             | JSON Schema as a string, or a shared precompiled `Schema`.                             |
| `theme`                                                                                                                                              | An `EditorTheme`, optionally reactive.                                                 |
| `read_only`                                                                                                                                          | View mode: highlighting, diagnostics, diffs, and hover stay on.                        |
| `styled`, `class`                                                                                                                                    | Turn off the preset CSS and add your own class.                                        |
| `show_toolbar`, `show_footer`                                                                                                                        | Built-in chrome (or compose `ConfigEditorToolbar` / `ConfigEditorFooter`).             |
| `show_diagnostics`, `enable_completions`, `enable_hover_docs`, `auto_trigger_completions`, `enable_highlighting`, `enable_auto_closing`, `show_diff` | Feature switches.                                                                      |

### Editing in another language

```rust
<ConfigEditor
    language=Language::Xml   // what the host stores and receives
    edit_as=Language::Hcl    // what the user edits
    value=xml
    on_change=move |text| xml.set(text)
/>
```

Invalid text is never sent to the host. Values the host language cannot
carry exactly (for example a one-item list in XML) are flagged as warnings
on the value, and comments do not survive a conversion.

## Headless API

```rust
use leptos_config_editor::{Language, Schema};

let schema = Schema::compile(include_str!("schema.json"))?;
let problems = schema.validate(Language::Toml, &text); // Vec<Diagnostic>

let value = Language::Yaml.parse(&text)?;              // serde_json::Value
let hcl = Language::Yaml.convert(&text, Language::Hcl)?;
assert!(hcl.losses.is_empty());

let language: Language = "yml".parse()?;               // names and extensions
let language = Language::from_path("infra/main.tf");   // Some(Language::Hcl)
```

## XML mapping

Child elements and attributes are object properties, element text is a typed
scalar (`<listen>7835</listen>` is the number 7835), and repeated siblings are
an array. A top-level element marked `config-root="true"` is a transparent
wrapper; without it, every top-level element is a document property.

## Features

- `schema-validation` (default): JSON Schema support through `jsonschema`.
  Without it the editor still checks syntax and converts.

[Leptos]: https://leptos.dev
