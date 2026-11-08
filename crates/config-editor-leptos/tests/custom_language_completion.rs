//! Schema completion and hover docs for an external language, through the
//! public API only: the INI example (`tests/ini`) implements
//! `completion_context`, `existing_keys`, `property_insert`, `format_value`
//! and `hover_key`, and `Schema::suggestions` / `Schema::hover` give what
//! the editor would show.

use config_editor_leptos::{Language, Schema, TextPosition};

mod ini;
use ini::Ini;

static INI: Ini = Ini;
const LANGUAGE: Language = Language::new(&INI);

const SCHEMA: &str = r#"{
    "type": "object",
    "properties": {
        "name": {"type": "string", "description": "Application name"},
        "debug": {"type": "boolean", "description": "Verbose logging"},
        "server": {
            "type": "object",
            "description": "Where the app listens",
            "properties": {
                "host": {"type": "string", "description": "Address to bind"},
                "port": {"type": "integer", "description": "Port to listen on"},
                "mode": {"enum": ["fast", "safe"], "description": "Request handling"}
            }
        }
    }
}"#;

fn schema() -> Schema {
    Schema::compile(SCHEMA).unwrap()
}

/// `marked` without its `|`, and the UTF-16 offset of the `|`.
fn unmark(marked: &str) -> (String, usize) {
    let at = marked.find('|').expect("a | caret marker");
    (
        marked.replacen('|', "", 1),
        marked[..at].encode_utf16().count(),
    )
}

/// The labels offered at the `|` in `marked`, sorted.
fn offered(marked: &str) -> Vec<String> {
    let (source, cursor) = unmark(marked);
    let mut labels: Vec<String> = schema()
        .suggestions(LANGUAGE, &source, cursor)
        .iter()
        .map(|suggestion| suggestion.label().to_string())
        .collect();
    labels.sort();
    labels
}

/// Accepts `label` at the `|` in `marked`; the document with `|` at the
/// caret after it.
fn accept(marked: &str, label: &str) -> String {
    let (source, cursor) = unmark(marked);
    let suggestions = schema().suggestions(LANGUAGE, &source, cursor);
    let suggestion = suggestions
        .iter()
        .find(|suggestion| suggestion.label() == label)
        .unwrap_or_else(|| panic!("{label} is not offered in {marked:?}: {suggestions:?}"));
    let (text, selection) = suggestion.accept(&source);
    assert!(selection.is_empty(), "nothing is left selected");
    let byte = text
        .char_indices()
        .map(|(index, _)| index)
        .chain([text.len()])
        .find(|index| text[..*index].encode_utf16().count() == selection.end)
        .expect("the caret on a character boundary");
    format!("{}|{}", &text[..byte], &text[byte..])
}

#[test]
fn schema_keys_are_offered_where_the_caret_is_skipping_present_ones() {
    // The top level: its keys and sections, without `name` already there.
    assert_eq!(offered("name = app\n|"), ["debug", "server"]);
    // A section offers its own keys, without `host` already there; the
    // caret's own half-typed key hides nothing.
    assert_eq!(
        offered("debug = true\n[server]\nhost = 0.0.0.0\n|"),
        ["mode", "port"]
    );
    assert_eq!(offered("[server]\nport = 80\nmo|"), ["mode"]);
    // A section already written is not offered again.
    assert_eq!(offered("|\n[server]\n"), ["debug", "name"]);
}

#[test]
fn schema_values_are_offered_after_the_separator() {
    assert_eq!(offered("[server]\nmode = |"), ["fast", "safe"]);
    assert_eq!(offered("debug = |"), ["false", "true"]);
    // Written as the language writes values: bare, a space after `=`.
    assert_eq!(
        accept("[server]\nmode = f|", "fast"),
        "[server]\nmode = fast|"
    );
    assert_eq!(accept("debug =|", "true"), "debug = true|");
}

#[test]
fn accepting_a_key_writes_the_entry_with_the_caret_after_it() {
    assert_eq!(accept("[server]\npo|", "port"), "[server]\nport = |");
    assert_eq!(accept("de|", "debug"), "debug = |");
    // An object key is a section.
    assert_eq!(
        accept("name = app\nser|", "server"),
        "name = app\n[server]\n|"
    );
}

#[test]
fn hover_shows_the_schema_docs_of_the_key_under_the_pointer() {
    let schema = schema();
    let source = "name = app\n[server]\nport = 80\n";
    let docs = schema
        .hover(LANGUAGE, source, TextPosition::new(3, 2))
        .expect("docs over `port`");
    assert_eq!(docs.key, "port");
    assert_eq!(docs.description.as_deref(), Some("Port to listen on"));
    assert_eq!(docs.type_label, "integer");
    let docs = schema
        .hover(LANGUAGE, source, TextPosition::new(1, 1))
        .expect("docs over `name`");
    assert_eq!(docs.description.as_deref(), Some("Application name"));
    // A header's name is its section.
    let docs = schema
        .hover(LANGUAGE, source, TextPosition::new(2, 3))
        .expect("docs over `[server]`");
    assert_eq!(docs.description.as_deref(), Some("Where the app listens"));
    // Past the key: nothing.
    assert_eq!(
        schema.hover(LANGUAGE, source, TextPosition::new(3, 7)),
        None
    );
}
