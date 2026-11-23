//! Hover and go to definition on a known name, in one mounted editor: a
//! name the document defines says so and goes to its key; a host's name
//! says its source and goes to the host; any other value is no name.

use std::sync::Arc;

use leptos::prelude::*;

use super::go_to::Definition;
use super::{GoToDefinition, KeyName, KeyNames, KeyNamesInput};
use crate::editor::completion::field_at;
use crate::editor::languages::Language;
use crate::editor::schema::build_schema;
use crate::editor::state::EditorState;
use crate::editor::test_state::TestEditor;
use crate::editor::text::{TextRange, text_size};

const SCHEMA: &str = r##"{
  "properties": {
    "clients": {"type": "object"},
    "default_client": {"type": "string", "x-config-editor-keys": "/clients"},
    "port": {"type": "integer"},
    "routes": {
      "type": "array",
      "items": {"type": "object", "properties": {"client": {"$ref": "#/$defs/client"}}}
    },
    "reach": {
      "type": "object",
      "propertyNames": {"$ref": "#/$defs/client"},
      "additionalProperties": {"type": "array"}
    }
  },
  "$defs": {"client": {"x-config-editor-keys": "/clients", "examples": ["self"]}}
}"##;

const TEXT: &str = "clients:\n  home: {}\n  work: {}\ndefault_client: home\nport: 80\n";

/// An editor of `text` under the schema, the host giving `remote`, and
/// what `on_go_to_definition` heard.
fn mounted(text: &str) -> (Owner, EditorState, RwSignal<Vec<GoToDefinition>>) {
    mounted_as(Language::YAML, text)
}

/// [`mounted`], in `language`.
fn mounted_as(
    language: Language,
    text: &str,
) -> (Owner, EditorState, RwSignal<Vec<GoToDefinition>>) {
    let mut editor = TestEditor::new(language, text);
    editor.schema = Some(Arc::new(build_schema(SCHEMA).expect("the schema compiles")));
    let heard = RwSignal::new(Vec::new());
    editor.key_names = KeyNamesInput {
        source: Arc::new(vec![KeyNames::new(
            "/clients",
            [KeyName::new("remote").with_source("server.d/remote.yaml")],
        )]),
        on_go_to_definition: Some(Callback::new(move |target| {
            heard.update(|heard: &mut Vec<GoToDefinition>| heard.push(target));
        })),
        ..KeyNamesInput::default()
    };
    let (owner, state) = editor.mount();
    (owner, state, heard)
}

/// The hover card's value line over the value `value` on zero-based `line`.
fn value_line(state: EditorState, text: &str, line: usize, value: &str) -> Option<String> {
    let column = text.lines().nth(line)?.find(value)? + 1;
    let field = field_at(text, line, column, state.behavior())?;
    let hover = state.field_docs(text, &field)?;
    hover.value?.doc.description
}

#[test]
fn hover_on_a_known_name_says_where_it_is_defined() {
    let (_owner, state, _) = mounted(TEXT);
    assert_eq!(
        value_line(state, TEXT, 3, "home").as_deref(),
        Some("defined in this document")
    );
    let remote = TEXT.replace("default_client: home", "default_client: remote");
    assert_eq!(
        value_line(state, &remote, 3, "remote").as_deref(),
        Some("defined in server.d/remote.yaml")
    );
    let unknown = TEXT.replace("default_client: home", "default_client: nobody");
    assert_eq!(value_line(state, &unknown, 3, "nobody"), None);
    assert_eq!(value_line(state, TEXT, 4, "80"), None);
}

#[test]
fn go_to_definition_moves_to_the_key_or_tells_the_host() {
    let (_owner, state, heard) = mounted(TEXT);
    let on_value = TEXT.find("default_client: home").unwrap() + "default_client: ho".len();
    state.doc().caret.set(text_size(on_value));
    assert_eq!(
        state.definition_at_caret(),
        Some(Definition::Key(TextRange::at(
            text_size(TEXT.find("home").unwrap()),
            text_size("home".len())
        )))
    );
    let remote = TEXT.replace("default_client: home", "default_client: remote");
    let (_owner, state, heard_remote) = mounted(&remote);
    let past_value = remote.find("remote\n").unwrap() + "remote".len();
    state.doc().caret.set(text_size(past_value));
    assert!(state.go_to_definition());
    assert_eq!(
        heard_remote.get_untracked(),
        [GoToDefinition {
            schema: None,
            pointer: "/clients".into(),
            name: "remote".into(),
            source: Some("server.d/remote.yaml".into()),
        }]
    );
    // No name at the caret: the key is left alone.
    state.doc().caret.set(text_size(remote.find("80").unwrap()));
    assert!(!state.go_to_definition());
    assert_eq!(heard_remote.get_untracked().len(), 1);
    assert!(heard.get_untracked().is_empty());
}

#[test]
fn a_suggestion_shows_its_source_with_its_own_detail() {
    use super::KeyOrigin;
    let strings = crate::strings::default_strings();
    let source = KeyOrigin::Source("a.yaml".into());
    assert_eq!(
        source.detail(Some("the home box"), strings),
        "a.yaml · the home box"
    );
    assert_eq!(source.detail(None, strings), "a.yaml");
    assert_eq!(
        KeyOrigin::ThisDocument.detail(Some(""), strings),
        "this document"
    );
    assert_eq!(KeyOrigin::Unnamed.detail(Some("d"), strings), "d");
    assert_eq!(KeyOrigin::Unnamed.defined_in(strings), None);
}

/// Replaces the document with `text`, as typing would, the caret at the end
/// of `caret_after`, and lets the typing pause's check read it.
#[cfg(feature = "json")]
fn retype(state: EditorState, text: &str, caret_after: &str) {
    let before = state.doc().text.get_untracked().to_string();
    let end = before.encode_utf16().count();
    let caret = text.find(caret_after).expect("the caret's place") + caret_after.len();
    state.commit(crate::editor::edit::Edit::new_units(
        before,
        (0, end),
        text.to_string(),
        text[..caret].encode_utf16().count(),
    ));
    state.problems().diagnosed_text.set(text.to_string());
    let _ = state.problems().diagnostics.get_untracked();
}

/// The names suggested at the caret, as `Ctrl+Space` lists them.
#[cfg(feature = "json")]
fn suggested(state: EditorState) -> Vec<String> {
    state.set_list_mode(crate::editor::completion::ListMode::Asked {
        replace_word: false,
        heeded: false,
    });
    state
        .completion()
        .suggestions
        .get_untracked()
        .into_iter()
        .map(|option| option.label)
        .collect()
}

/// A document that does not parse keeps the names of its last text that
/// did; once it parses again, its own keys are the names.
#[cfg(feature = "json")]
#[test]
fn a_document_that_does_not_parse_keeps_its_last_good_names() {
    crate::editor::parse_cache::hold();
    let caret = r#""default_client": ""#;
    let added = r#"{"clients": {"home": {}, "added": {}}, "default_client": "", "port": 80}"#;
    let (_owner, state, _) = mounted_as(Language::JSON, added);
    retype(state, added, caret);
    assert_eq!(suggested(state), ["home", "added", "remote"]);
    // A comma missing in the map: no reading of the text has `clients`.
    let broken = r#"{"clients": {"home": {} "added": {}}, "default_client": "", "port": 80}"#;
    retype(state, broken, caret);
    assert_eq!(suggested(state), ["home", "added", "remote"]);
    // Fixed, `added` removed: gone.
    let fixed = r#"{"clients": {"home": {}}, "default_client": "", "port": 80}"#;
    retype(state, fixed, caret);
    assert_eq!(suggested(state), ["home", "remote"]);
    crate::editor::parse_cache::release();
}

/// A key of an object whose `propertyNames` has the keyword: hover says
/// where the name is defined, and go to definition goes there, as on a
/// value; `(language, text, zero-based line of the reach keys)`.
fn key_name_cases() -> Vec<(Language, &'static str, usize)> {
    #[cfg_attr(not(any(feature = "toml", feature = "json")), allow(unused_mut))]
    let mut cases = vec![(
        Language::YAML,
        "clients:\n  home: {}\n  work: {}\nreach:\n  home: []\n  remote: []\n",
        4,
    )];
    #[cfg(feature = "toml")]
    cases.push((
        Language::TOML,
        "[clients.home]\n\n[clients.work]\n\n[reach]\nhome = []\nremote = []\n",
        5,
    ));
    #[cfg(feature = "json")]
    cases.push((
        Language::JSON,
        "{\n  \"clients\": {\"home\": {}, \"work\": {}},\n  \"reach\": {\n    \"home\": [],\n    \"remote\": []\n  }\n}\n",
        3,
    ));
    cases
}

#[test]
fn hover_and_go_to_definition_on_a_key_name() {
    for (language, text, line) in key_name_cases() {
        let (_owner, state, heard) = mounted_as(language, text);
        let at_line = |line: usize| {
            text.split('\n')
                .take(line)
                .map(|l| l.len() + 1)
                .sum::<usize>()
        };
        let home_line = &text[at_line(line)..];
        let home = at_line(line) + home_line.find("home").unwrap();
        let remote_line = &text[at_line(line + 1)..];
        let remote = at_line(line + 1) + remote_line.find("remote").unwrap();
        let card = |byte: usize, line: usize| {
            let column = text[at_line(line)..byte].chars().count() + 1;
            let field = field_at(text, line, column, state.behavior()).expect("a field");
            state.field_docs(text, &field)?.value?.doc.description
        };
        assert_eq!(
            card(home, line).as_deref(),
            Some("defined in this document"),
            "{language}"
        );
        assert_eq!(
            card(remote, line + 1).as_deref(),
            Some("defined in server.d/remote.yaml"),
            "{language}"
        );
        state.doc().caret.set(text_size(home + 2));
        // The key `/clients/home` is written on the line of the first
        // `home` (TOML's header starts at its `[`).
        let defined = text.find("home").unwrap();
        let Some(Definition::Key(key)) = state.definition_at_caret() else {
            panic!("{language}: no key to go to");
        };
        let at = usize::from(key.start());
        assert!(
            text[at..usize::from(key.end())].contains("home"),
            "{language}: the key is {key:?}"
        );
        assert!(
            at <= defined && !text[at..defined].contains('\n'),
            "{language}: goes to {at}"
        );
        state.doc().caret.set(text_size(remote + 2));
        assert!(state.go_to_definition(), "{language}");
        assert_eq!(
            heard.get_untracked(),
            [GoToDefinition {
                schema: None,
                pointer: "/clients".into(),
                name: "remote".into(),
                source: Some("server.d/remote.yaml".into()),
            }],
            "{language}"
        );
        // A key of an object with no such `propertyNames` is no name.
        state.doc().caret.set(text_size(defined + 2));
        assert_eq!(state.definition_at_caret(), None, "{language}");
    }
}

/// A name used in an item written on one line (a YAML flow map, a TOML
/// inline table): `(language, text)`, the use the last `home`.
fn one_line_cases() -> Vec<(Language, &'static str)> {
    #[cfg_attr(not(feature = "toml"), allow(unused_mut))]
    let mut cases = vec![
        (
            Language::YAML,
            "clients:\n  home: {}\n  work: {}\nroutes:\n  - {path: /x, client: home}\n",
        ),
        // The definition on one line too.
        (
            Language::YAML,
            "clients: {home: {}, work: {}}\nroutes: [{path: /x, client: home}]\n",
        ),
    ];
    #[cfg(feature = "toml")]
    cases.push((
        Language::TOML,
        "clients = {home = {}, work = {}}\nroutes = [{path = \"/x\", client = \"home\"}]\n",
    ));
    cases
}

/// On a name in an item written on one line, the hover card says where it
/// is defined, go to definition goes to its key and Find uses lists it, as
/// in block form.
#[test]
fn a_name_in_an_item_on_one_line_is_hovered_gone_to_and_searched() {
    for (language, text) in one_line_cases() {
        let (_owner, state, _) = mounted_as(language, text);
        let used = text.rfind("home").unwrap();
        let line = text[..used].matches('\n').count();
        let start = text[..used].rfind('\n').map_or(0, |at| at + 1);
        let column = text[start..used].chars().count() + 1;
        let field = field_at(text, line, column, state.behavior()).expect("a field");
        let card = state
            .field_docs(text, &field)
            .and_then(|hover| hover.value?.doc.description);
        assert_eq!(
            card.as_deref(),
            Some("defined in this document"),
            "{language}"
        );
        let defined = text.find("home").unwrap();
        for caret in [used + 1, used + "home".len()] {
            state.doc().caret.set(text_size(caret));
            let Some(Definition::Key(key)) = state.definition_at_caret() else {
                panic!("{language}: no key to go to from {caret}");
            };
            let at = usize::from(key.start());
            assert_eq!(&text[at..usize::from(key.end())], "home", "{language}");
            assert!(
                at <= defined && !text[at..defined].contains('\n'),
                "{language}: goes to {at}"
            );
            assert!(state.find_uses(), "{language}: no uses from {caret}");
            state.close_uses();
        }
    }
}

/// Where the schema names keys, `F12` and `Shift+F12` on no name say so at
/// the caret (they are the editor's keys there); where it names none,
/// they are not the editor's.
#[test]
fn finding_nothing_says_so_at_the_caret_where_the_schema_names_keys() {
    let (_owner, state, _) = mounted(TEXT);
    assert!(state.names_keys());
    let on_port = TEXT.find("port").unwrap() + 2;
    state.doc().caret.set(text_size(on_port));
    assert!(!state.go_to_definition());
    state.say_no_definition();
    let notice = state.a11y().caret_notice.get_untracked().expect("a notice");
    assert_eq!(notice.message, "No definition for 'port'");
    assert!(!notice.refused);
    assert_eq!(
        state
            .a11y()
            .announcement
            .get_untracked()
            .trim_end_matches('\u{a0}'),
        "No definition for 'port'"
    );
    assert!(!state.find_uses());
    state.say_no_uses();
    let notice = state.a11y().caret_notice.get_untracked().expect("a notice");
    assert_eq!(notice.message, "No uses of 'port'");
    // No word at the caret.
    state
        .doc()
        .caret
        .set(text_size(TEXT.find(": 80").unwrap() + 1));
    state.say_no_definition();
    let notice = state.a11y().caret_notice.get_untracked().expect("a notice");
    assert_eq!(notice.message, "No definition here");
    state.hide_caret_notice();
    assert_eq!(state.a11y().caret_notice.get_untracked(), None);

    let mut editor = TestEditor::new(Language::YAML, TEXT);
    editor.schema = Some(Arc::new(
        build_schema(r#"{"properties": {"port": {"type": "integer"}}}"#).expect("compiles"),
    ));
    let (_owner, state) = editor.mount();
    assert!(!state.names_keys());
}
