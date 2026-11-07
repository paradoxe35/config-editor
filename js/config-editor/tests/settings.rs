//! Generates what is written from `EditorSettings` (the crate's
//! `src/settings.rs`) and checks it is up to date:
//!
//! - `src/settings.d.ts`: the TypeScript `EditorSettings` interface, with
//!   each field's doc comment and default;
//! - the settings tables of the three READMEs, between
//!   `<!-- settings:start -->` and `<!-- settings:end -->`.
//!
//! The names and defaults come from the struct itself (serde, as JavaScript
//! reads it), the doc comments and Rust types from its source. A field
//! whose type has no TypeScript spelling here fails the test.
//!
//! Regenerate with `UPDATE_GENERATED=1 cargo test -p config-editor-js --test settings`.

use std::path::{Path, PathBuf};

use config_editor_leptos::{EditorSettings, EditorTheme};
use serde_json::Value;

const SOURCE: &str = include_str!("../../../crates/config-editor-leptos/src/settings.rs");
const SOURCE_PATH: &str = "crates/config-editor-leptos/src/settings.rs";

/// One field of the struct, as its source declares it.
struct Field {
    name: String,
    rust_type: String,
    /// The doc comment's paragraphs, each on one line, Rust links unwrapped.
    docs: Vec<String>,
}

/// The fields of `pub struct EditorSettings`, with their doc comments.
fn fields() -> Vec<Field> {
    let start = SOURCE
        .find("pub struct EditorSettings {")
        .expect("pub struct EditorSettings");
    let body = &SOURCE[start..];
    let end = body.find("\n}\n").expect("the struct's closing brace");
    let mut fields = Vec::new();
    let mut docs: Vec<String> = Vec::new();
    for line in body[..end].lines().skip(1) {
        let line = line.trim();
        if let Some(doc) = line.strip_prefix("///") {
            docs.push(doc.strip_prefix(' ').unwrap_or(doc).to_string());
        } else if let Some(declaration) = line.strip_prefix("pub ") {
            let (name, rust_type) = declaration
                .trim_end_matches(',')
                .split_once(": ")
                .unwrap_or_else(|| panic!("unexpected field line: {line}"));
            assert!(!docs.is_empty(), "the field `{name}` has no doc comment");
            fields.push(Field {
                name: name.to_string(),
                rust_type: rust_type.to_string(),
                docs: paragraphs(&std::mem::take(&mut docs)),
            });
        } else if line.starts_with("//") || line.is_empty() || line.starts_with("#[") {
            // Plain comments and attributes.
        } else {
            panic!("unexpected line in EditorSettings: {line}");
        }
    }
    fields
}

/// The struct's own doc comment, first paragraph.
fn summary() -> String {
    let start = SOURCE
        .find("pub struct EditorSettings {")
        .expect("pub struct EditorSettings");
    let mut lines: Vec<String> = SOURCE[..start]
        .lines()
        .rev()
        .skip_while(|line| line.starts_with("#["))
        .take_while(|line| line.starts_with("///"))
        .map(|line| line.trim_start_matches("///").trim().to_string())
        .collect();
    lines.reverse();
    paragraphs(&lines).remove(0)
}

/// Doc comment lines as paragraphs, each joined onto one line.
fn paragraphs(lines: &[String]) -> Vec<String> {
    lines
        .split(|line| line.is_empty())
        .filter(|paragraph| !paragraph.is_empty())
        .map(|paragraph| unlink(&paragraph.join(" ")))
        .collect()
}

/// Rust doc links as plain code: "[`Schema`](crate::Schema)" and
/// "[`Schema`]" become "`Schema`".
fn unlink(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(open) = rest.find("[`") {
        let Some(close) = rest[open..].find("`]") else {
            break;
        };
        out.push_str(&rest[..open]);
        let code = &rest[open + 1..open + close + 1];
        out.push_str(code);
        rest = &rest[open + close + 2..];
        if rest.starts_with('(')
            && let Some(end) = rest.find(')')
        {
            rest = &rest[end + 1..];
        }
    }
    out.push_str(rest);
    out
}

/// What JavaScript reads: each key with its default, in field order.
fn defaults() -> Vec<(String, Value)> {
    match serde_json::to_value(EditorSettings::default()).unwrap() {
        Value::Object(fields) => fields.into_iter().collect(),
        other => panic!("settings serialize to {other}"),
    }
}

/// The TypeScript spelling of a field's type.
fn ts_type(rust_type: &str) -> &'static str {
    match rust_type {
        "bool" => "boolean",
        "EditorTheme" => "ThemeName",
        "ThemeColors" => "ThemeColorMap",
        other => panic!("no TypeScript type for `{other}`: add it to ts_type in tests/settings.rs"),
    }
}

/// A default as Rust spells it.
fn rust_default(rust_type: &str, value: &Value) -> String {
    match (rust_type, value) {
        ("EditorTheme", Value::String(slug)) => {
            format!("`EditorTheme::{:?}`", slug.parse::<EditorTheme>().unwrap())
        }
        ("ThemeColors", _) => "none".to_string(),
        (_, value) => format!("`{value}`"),
    }
}

/// A default as JavaScript spells it.
fn js_default(value: &Value) -> String {
    match value {
        Value::Object(map) if map.is_empty() => "none".to_string(),
        value => format!("`{value}`"),
    }
}

/// Each field with its key and default, the source and serde agreeing.
fn described() -> Vec<(Field, String, Value)> {
    let fields = fields();
    let defaults = defaults();
    let names: Vec<&str> = fields.iter().map(|field| field.name.as_str()).collect();
    let keys: Vec<&str> = defaults.iter().map(|(key, _)| key.as_str()).collect();
    assert_eq!(
        names.len(),
        keys.len(),
        "fields {names:?} against keys {keys:?}"
    );
    fields
        .into_iter()
        .zip(defaults)
        .map(|(field, (key, default))| {
            assert_eq!(camel_case(&field.name), key, "field order");
            (field, key, default)
        })
        .collect()
}

fn camel_case(name: &str) -> String {
    let mut out = String::new();
    let mut upper = false;
    for character in name.chars() {
        if character == '_' {
            upper = true;
        } else if upper {
            out.extend(character.to_uppercase());
            upper = false;
        } else {
            out.push(character);
        }
    }
    out
}

const HEADER: &str = "Generated from crates/config-editor-leptos/src/settings.rs by \
                      js/config-editor/tests/settings.rs; do not edit. Regenerate with \
                      `UPDATE_GENERATED=1 cargo test -p config-editor-js --test settings`.";

fn typescript() -> String {
    let mut out = format!(
        "// {HEADER}\n\n/**\n * {}\n *\n * Every key is optional: one left out takes its default.\n */\nexport interface EditorSettings {{\n",
        summary()
    );
    for (field, key, default) in described() {
        out.push_str("  /**\n");
        for (index, paragraph) in field.docs.iter().enumerate() {
            if index > 0 {
                out.push_str("   *\n");
            }
            out.push_str(&format!("   * {paragraph}\n"));
        }
        out.push_str(&format!("   *\n   * @default {default}\n   */\n"));
        out.push_str(&format!("  {key}?: {};\n", ts_type(&field.rust_type)));
    }
    out.push_str("}\n");
    out
}

fn table(rust: bool) -> String {
    let mut out = format!("<!-- {HEADER} -->\n\n");
    out.push_str(if rust {
        "| Field | Default | |\n|---|---|---|\n"
    } else {
        "| Setting | Default | |\n|---|---|---|\n"
    });
    for (field, key, default) in described() {
        let (name, default) = if rust {
            (field.name.clone(), rust_default(&field.rust_type, &default))
        } else {
            (key, js_default(&default))
        };
        let text = field.docs.join(" ").replace('|', "\\|");
        out.push_str(&format!("| `{name}` | {default} | {text} |\n"));
    }
    out
}

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// `text` with the part between the settings markers replaced by `table`.
fn with_table(text: &str, table: &str, path: &str) -> String {
    const START: &str = "<!-- settings:start -->\n";
    const END: &str = "<!-- settings:end -->";
    let start = text
        .find(START)
        .unwrap_or_else(|| panic!("{path} has no `{}` marker", START.trim()))
        + START.len();
    let end = text[start..]
        .find(END)
        .unwrap_or_else(|| panic!("{path} has no `{END}` marker"))
        + start;
    format!("{}\n{table}\n{}", &text[..start], &text[end..])
}

#[test]
fn generated_settings_are_up_to_date() {
    let update = std::env::var_os("UPDATE_GENERATED").is_some();
    let mut generated = vec![("js/config-editor/src/settings.d.ts", typescript())];
    for (path, rust) in [
        ("crates/config-editor-leptos/README.md", true),
        ("js/config-editor/README.md", false),
        ("js/config-editor-react/README.md", false),
    ] {
        let current = std::fs::read_to_string(repo().join(path)).unwrap();
        generated.push((path, with_table(&current, &table(rust), path)));
    }
    let mut stale = Vec::new();
    for (path, text) in generated {
        let file = repo().join(path);
        let current = std::fs::read_to_string(&file).unwrap_or_default();
        if current == text {
            continue;
        }
        if update {
            std::fs::write(&file, text).unwrap();
        } else {
            stale.push(path);
        }
    }
    assert!(
        stale.is_empty(),
        "{stale:?} are out of date with {SOURCE_PATH}: run \
         `UPDATE_GENERATED=1 cargo test -p config-editor-js --test settings`"
    );
}

#[test]
fn links_unwrap_to_code() {
    assert_eq!(
        unlink("a [`Schema`](crate::Schema) or [`Language`] here"),
        "a `Schema` or `Language` here"
    );
    assert_eq!(
        camel_case("auto_trigger_completions"),
        "autoTriggerCompletions"
    );
}
