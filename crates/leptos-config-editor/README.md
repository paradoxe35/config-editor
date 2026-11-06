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
            language=Language::YAML
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
    language=Language::XML   // what the host stores and receives
    edit_as=Language::HCL    // what the user edits
    value=xml
    on_change=move |text| xml.set(text)
/>
```

Invalid text is never sent to the host. Values the host language cannot
carry exactly (for example an empty list in XML) are flagged as warnings on
the value, and comments do not survive a conversion. With a schema, a single
XML element where the schema expects a list reads as a one-item list, so it
validates and converts as one (`Schema::convert` does the same headlessly).

## Headless API

```rust
use leptos_config_editor::{Language, Schema};

let schema = Schema::compile(include_str!("schema.json"))?;
let problems = schema.validate(Language::TOML, &text); // Vec<Diagnostic>

let value = Language::YAML.parse(&text)?;              // serde_json::Value
let hcl = Language::YAML.convert(&text, Language::HCL)?;
assert!(hcl.losses.is_empty());

let language: Language = "yml".parse()?;               // names and extensions
let language = Language::from_path("infra/main.tf");   // Some(Language::HCL)
```

## XML mapping

Child elements and attributes are object properties, element text is a typed
scalar (`<listen>7835</listen>` is the number 7835), and repeated siblings are
an array. A top-level element marked `config-root="true"` is a transparent
wrapper; without it, every top-level element is a document property.

## Your own language

Every built-in language implements the public `ConfigLanguage` trait; yours
can too. Only `info` and `parse` are required — each further method adds a
capability (diagnostic positions, completion, hover, highlighting, editing,
conversion):

```rust
use leptos_config_editor::Language;
use leptos_config_editor::language::{ConfigLanguage, LanguageInfo, ParseError};

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

`leptos_config_editor::language` also exports the helpers the built-in
languages use (tokens, schema lookups, inline-container scanning, indent
detection). `tests/custom_language.rs` is a complete example.

## Features

All on by default; turn off what a host does not use to shrink the build.

| Feature             | Enables                                                                                                                 |
| ------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| `schema-validation` | JSON Schema diagnostics, completions, and hover (`jsonschema`). Without it the editor still checks syntax and converts. |
| `yaml`              | `Language::YAML` (`noyalib`)                                                                                            |
| `json`              | `Language::JSON`                                                                                                        |
| `toml`              | `Language::TOML` (`toml`)                                                                                               |
| `hcl`               | `Language::HCL` (`hcl-rs`)                                                                                              |
| `xml`               | `Language::XML` (`quick-xml`)                                                                                           |

```toml
leptos-config-editor = { version = "0.1", default-features = false, features = ["schema-validation", "yaml"] }
```

With no language feature, use your own languages through `Language::new`.

[Leptos]: https://leptos.dev
