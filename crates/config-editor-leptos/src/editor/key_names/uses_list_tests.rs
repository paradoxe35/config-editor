//! What the list of uses shows in a mounted editor: a line holding a
//! secret quotes it as problems do (hidden, `…`, in every language with
//! fields sharing a line); a failed host search says the other files could
//! not be searched (never "No uses"); and the list starts on the use at the
//! caret (on the definition: the first use).

use std::sync::Arc;

use leptos::prelude::*;

use super::{FindUses, KeyNamesInput, Use, uses_said};
use crate::editor::languages::Language;
use crate::editor::schema::build_schema;
use crate::editor::state::EditorState;
use crate::editor::test_state::TestEditor;
use crate::editor::text::text_size;
use crate::strings::default_strings;

const SCHEMA: &str = r##"{
  "properties": {
    "clients": {"type": "object"},
    "default_client": {"$ref": "#/$defs/client"},
    "members": {"type": "array", "items": {"$ref": "#/$defs/client"}},
    "ann": {"type": "object", "properties": {
      "role": {"$ref": "#/$defs/client"},
      "password": {"type": "string", "writeOnly": true}
    }}
  },
  "$defs": {"client": {"type": "string", "x-config-editor-keys": "/clients"}}
}"##;

/// An editor of `text` in `language`; its host's `on_find_uses` requests
/// are kept when `searches`.
fn mounted(
    language: Language,
    text: &str,
    searches: bool,
) -> (Owner, EditorState, RwSignal<Vec<FindUses>>) {
    let mut editor = TestEditor::new(language, text);
    editor.schema = Some(Arc::new(build_schema(SCHEMA).expect("the schema compiles")));
    let asked = RwSignal::new(Vec::new());
    editor.key_names = KeyNamesInput {
        on_find_uses: searches
            .then(|| Callback::new(move |request: FindUses| asked.update(|all| all.push(request)))),
        ..KeyNamesInput::default()
    };
    let (owner, state) = editor.mount();
    (owner, state, asked)
}

/// Lists the uses of the name at byte `at`.
fn find_at(state: EditorState, at: usize) {
    state.doc().caret.set(text_size(at));
    assert!(state.find_uses(), "a name at {at}");
}

fn row_texts(state: EditorState) -> Vec<String> {
    state.uses().list.with_untracked(|list| {
        list.iter()
            .flat_map(|list| &list.groups)
            .flat_map(|group| &group.rows)
            .map(|row| row.text.clone())
            .collect()
    })
}

fn said(state: EditorState) -> String {
    state
        .uses()
        .list
        .with_untracked(|list| uses_said(list.as_ref().expect("a list"), default_strings()))
}

// Secrets are the `protect` feature's: without it nothing is marked secret.
#[cfg(feature = "protect")]
#[test]
fn a_use_beside_a_secret_never_shows_it() {
    let cases = [
        (
            Language::YAML,
            "clients:\n  admin: {}\nann: {role: admin, password: hunter2}\n",
        ),
        (
            Language::JSON,
            "{\"clients\": {\"admin\": {}},\n \"ann\": {\"role\": \"admin\", \"password\": \"hunter2\"}}\n",
        ),
        (
            Language::TOML,
            "ann = {role = \"admin\", password = \"hunter2\"}\n[clients.admin]\n",
        ),
    ];
    for (language, text) in cases {
        let (_owner, state, _) = mounted(language, text, false);
        let role = text.find("role").expect("the use");
        find_at(
            state,
            role + text[role..].find("admin").expect("its name") + 1,
        );
        let rows = row_texts(state);
        assert_eq!(rows.len(), 1, "{language:?}: {rows:?}");
        assert!(rows[0].contains("admin"), "{language:?}: {rows:?}");
        assert!(!rows[0].contains("hunter2"), "{language:?}: {rows:?}");
        assert!(rows[0].contains('…'), "{language:?}: {rows:?}");
    }
}

#[test]
fn a_failed_host_search_says_so_and_never_no_uses() {
    let text = "clients:\n  home: {}\n  work: {}\ndefault_client: home\n";
    let (_owner, state, asked) = mounted(Language::YAML, text, true);
    // Nothing open uses `work`: the list says the files were not searched.
    find_at(state, text.find("work").expect("the key") + 1);
    asked.with_untracked(|asked| asked[0].fail());
    assert_eq!(said(state), "The other files could not be searched");
    // With open uses: their count, and the same.
    find_at(state, text.find("home").expect("the key") + 1);
    asked.with_untracked(|asked| asked[1].fail());
    assert_eq!(said(state), "1 use. The other files could not be searched");
    assert_eq!(row_texts(state), ["default_client: home"]);
    // A search that answers is told as before.
    find_at(state, text.find("home").expect("the key") + 1);
    asked.with_untracked(|asked| asked[2].reply(vec![Use::new("a.yaml", "/x")]));
    assert_eq!(said(state), "2 uses");
}

#[test]
fn the_list_starts_on_the_use_at_the_caret() {
    let text = "clients:\n  home: {}\ndefault_client: home\nmembers: [work, home]\n";
    let (_owner, state, _) = mounted(Language::YAML, text, false);
    let second = text.rfind("home").expect("the second use");
    find_at(state, second + 2);
    assert_eq!(state.uses().active.get_untracked(), 1, "on the second use");
    state.close_uses();
    find_at(
        state,
        text.find("default_client: home").expect("a use") + 17,
    );
    assert_eq!(state.uses().active.get_untracked(), 0, "on the first use");
    state.close_uses();
    // On the definition: the first use.
    find_at(state, text.find("home").expect("the key") + 1);
    assert_eq!(state.uses().active.get_untracked(), 0, "on the definition");
}
