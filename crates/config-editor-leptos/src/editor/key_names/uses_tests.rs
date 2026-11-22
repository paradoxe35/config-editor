//! Find uses in one mounted editor: on a use or on the key defining a
//! name, the list holds the document's uses at once; the host's join it
//! when they come (a stale answer dropped); an empty list says why; a use
//! chosen goes to its place or to the host; the hover card offers the
//! links on names only; and nothing is searched while typing.

use std::sync::Arc;

use leptos::prelude::*;

use super::{FindUses, GoToUse, KeyName, KeyNames, KeyNamesInput, Use, UseTarget, uses_said};
use crate::FieldPath;
use crate::editor::completion::field_at;
use crate::editor::languages::Language;
use crate::editor::schema::build_schema;
use crate::editor::state::EditorState;
use crate::editor::test_state::TestEditor;
use crate::editor::text::text_size;
use crate::editor::work_count::{Work, counted};

const SCHEMA: &str = r##"{
  "properties": {
    "clients": {"type": "object"},
    "default_client": {"$ref": "#/$defs/client"},
    "members": {"type": "array", "items": {"$ref": "#/$defs/client"}},
    "note": {"type": "string", "examples": ["home"]},
    "port": {"type": "integer"},
    "reach": {
      "type": "object",
      "propertyNames": {"$ref": "#/$defs/client"},
      "additionalProperties": {"type": "array"}
    }
  },
  "$defs": {"client": {"type": "string", "x-config-editor-keys": "/clients"}}
}"##;

const TEXT: &str = "clients:\n  home: {}\n  work: {}\ndefault_client: home\nmembers: [work, home]\nnote: home\nport: 80\nreach:\n  home: []\n";

/// What the host heard.
#[derive(Clone, Copy)]
struct Heard {
    asked: RwSignal<Vec<FindUses>>,
    gone_to: RwSignal<Vec<GoToUse>>,
}

/// An editor of `text`, with a host that searches (`on_find_uses`) when
/// `searches`.
fn mounted(text: &str, searches: bool) -> (Owner, EditorState, Heard) {
    let mut editor = TestEditor::new(Language::YAML, text);
    editor.schema = Some(Arc::new(build_schema(SCHEMA).expect("the schema compiles")));
    let heard = Heard {
        asked: RwSignal::new(Vec::new()),
        gone_to: RwSignal::new(Vec::new()),
    };
    editor.key_names = KeyNamesInput {
        source: Arc::new(vec![KeyNames::new(
            "/clients",
            [KeyName::new("remote").with_source("server.d/remote.yaml")],
        )]),
        on_find_uses: searches.then(|| {
            Callback::new(move |asked: FindUses| heard.asked.update(|all| all.push(asked)))
        }),
        on_go_to_use: Some(Callback::new(move |target: GoToUse| {
            heard.gone_to.update(|all| all.push(target));
        })),
        ..KeyNamesInput::default()
    };
    let (owner, state) = editor.mount();
    (owner, state, heard)
}

/// The byte `skip` characters into the `nth` (zero-based) `word` of `text`.
fn at(text: &str, word: &str, nth: usize, skip: usize) -> usize {
    text.match_indices(word).nth(nth).expect("the word").0 + skip
}

/// Each group's title, with its rows' line and text.
type Groups = Vec<(String, Vec<(Option<usize>, String)>)>;

/// The list's rows: each group's title, with its rows' line and text.
fn rows(state: EditorState) -> Groups {
    state.uses().list.with_untracked(|list| {
        list.as_ref()
            .map(|list| {
                list.groups
                    .iter()
                    .map(|group| {
                        let rows = group
                            .rows
                            .iter()
                            .map(|row| (row.line, row.text.clone()))
                            .collect();
                        (group.label.clone(), rows)
                    })
                    .collect()
            })
            .unwrap_or_default()
    })
}

/// What the list says of itself.
fn said(state: EditorState) -> String {
    state.uses().list.with_untracked(|list| {
        uses_said(
            list.as_ref().expect("a list"),
            crate::strings::default_strings(),
        )
    })
}

fn own_uses() -> Groups {
    vec![(
        "this document".to_string(),
        vec![
            (Some(4), "default_client: home".to_string()),
            (Some(5), "members: [work, home]".to_string()),
            (Some(9), "home: []".to_string()),
        ],
    )]
}

#[test]
fn find_uses_on_a_use_or_its_definition_lists_the_documents_uses() {
    let (_owner, state, _) = mounted(TEXT, false);
    // On the value `home`, then just past it; on the key defining it.
    for caret in [
        at(TEXT, "home", 1, 1),
        at(TEXT, "home", 1, 4),
        at(TEXT, "home", 0, 2),
        at(TEXT, "home", 4, 0),
    ] {
        state.close_uses();
        state.doc().caret.set(text_size(caret));
        assert!(state.find_uses(), "at {caret}");
        assert_eq!(rows(state), own_uses(), "at {caret}");
        assert_eq!(said(state), "3 uses");
    }
    // The examples word and a plain value are no names.
    for caret in [at(TEXT, "home", 3, 1), at(TEXT, "80", 0, 1)] {
        state.close_uses();
        state.doc().caret.set(text_size(caret));
        assert!(!state.find_uses(), "at {caret}");
        assert!(!state.uses().is_open());
    }
}

/// An item of a list written inline (`[work, home]`), which the caret's
/// reading of its line does not reach, is found by where it is written.
#[test]
fn find_uses_on_an_item_of_an_inline_list() {
    let (_owner, state, _) = mounted(TEXT, false);
    for caret in [
        at(TEXT, "home", 2, 0),
        at(TEXT, "home", 2, 2),
        at(TEXT, "home", 2, 4),
    ] {
        state.close_uses();
        state.doc().caret.set(text_size(caret));
        assert!(state.find_uses(), "at {caret}");
        assert_eq!(rows(state), own_uses(), "at {caret}");
    }
    state.close_uses();
    state.doc().caret.set(text_size(at(TEXT, "work", 1, 2)));
    assert!(state.find_uses());
    assert_eq!(
        rows(state)[0].1,
        [(Some(5), "members: [work, home]".to_string())]
    );
}

#[test]
fn a_name_defined_nowhere_still_lists_its_uses() {
    let text = TEXT.replace("home", "nobody");
    let (_owner, state, _) = mounted(&text, false);
    state.doc().caret.set(text_size(at(&text, "nobody", 1, 2)));
    assert!(state.find_uses());
    assert_eq!(rows(state)[0].1.len(), 3);
}

#[test]
fn an_empty_list_says_why() {
    let text = "clients:\n  home: {}\ndefault_client: work\n";
    let (_owner, state, _) = mounted(text, false);
    state.doc().caret.set(text_size(at(text, "home", 0, 1)));
    assert!(state.find_uses());
    assert_eq!(said(state), "No uses in open files");
    let (_owner, state, heard) = mounted(text, true);
    state.doc().caret.set(text_size(at(text, "home", 0, 1)));
    assert!(state.find_uses());
    assert_eq!(said(state), "Searching other files…");
    heard
        .asked
        .with_untracked(|asked| asked[0].reply(Vec::new()));
    assert_eq!(said(state), "No uses");
}

#[test]
fn the_hosts_uses_join_the_list_and_a_stale_answer_is_dropped() {
    let (_owner, state, heard) = mounted(TEXT, true);
    state.doc().caret.set(text_size(at(TEXT, "home", 1, 1)));
    assert!(state.find_uses());
    let first = heard.asked.with_untracked(|asked| asked[0].clone());
    assert_eq!(
        (
            first.schema.as_deref(),
            first.pointer.as_str(),
            first.name.as_str()
        ),
        (None, "/clients", "home")
    );
    // Asked again: the first answer is for a list the user left.
    assert!(state.find_uses());
    first.reply(vec![Use::new("old.yaml", "/default_client")]);
    assert_eq!(rows(state), own_uses());
    let second = heard.asked.with_untracked(|asked| asked[1].clone());
    second.reply(vec![
        Use::new("a.yaml", "/default_client")
            .with_line(3)
            .with_text("  default_client: home  "),
        Use::new("b.yaml", "/members/0"),
        Use::new("a.yaml", "/reach/home").with_line(7),
    ]);
    let mut want = own_uses();
    want.push((
        "a.yaml".into(),
        vec![
            (Some(3), "default_client: home".into()),
            (Some(7), "/reach/home".into()),
        ],
    ));
    want.push(("b.yaml".into(), vec![(None, "/members/0".into())]));
    assert_eq!(rows(state), want);
    assert_eq!(said(state), "6 uses");
    // An answer after the list closed changes nothing.
    state.close_uses();
    second.reply(vec![Use::new("c.yaml", "/x")]);
    assert!(!state.uses().is_open());
}

#[test]
fn a_chosen_use_goes_to_its_place_or_to_the_host() {
    let (_owner, state, heard) = mounted(TEXT, true);
    state.doc().caret.set(text_size(at(TEXT, "home", 1, 1)));
    assert!(state.find_uses());
    heard
        .asked
        .with_untracked(|asked| asked[0].reply(vec![Use::new("a.yaml", "/members/1")]));
    let Some(UseTarget::Here(range)) = state.use_target(1) else {
        panic!("the second use is in the document");
    };
    let start = usize::from(range.start());
    assert_eq!(&TEXT[start..usize::from(range.end())], "home");
    assert_eq!(start, at(TEXT, "home", 2, 0));
    assert!(state.go_to_use(3));
    assert!(!state.uses().is_open());
    assert_eq!(
        heard.gone_to.get_untracked(),
        [GoToUse::new(
            "a.yaml".into(),
            FieldPath::from("/members/1"),
            "home".into()
        )]
    );
    assert!(!state.go_to_use(0), "the list is closed");
}

#[test]
fn the_hover_card_links_names_only() {
    let (_owner, state, _) = mounted(TEXT, false);
    let links = |byte: usize| {
        let line = TEXT[..byte].matches('\n').count();
        let start = TEXT[..byte].rfind('\n').map_or(0, |at| at + 1);
        let field = field_at(TEXT, line, byte - start, state.behavior()).expect("a field");
        state
            .field_docs(TEXT, &field)
            .and_then(|hover| hover.name_links)
            .map(|links| links.definition)
    };
    // A use offers Definition and Uses; the definition itself Uses only.
    assert_eq!(links(at(TEXT, "home", 1, 1)), Some(true));
    assert_eq!(links(at(TEXT, "home", 4, 1)), Some(true));
    assert_eq!(links(at(TEXT, "home", 0, 1)), Some(false));
    assert_eq!(links(at(TEXT, "home", 3, 1)), None);
    assert_eq!(links(at(TEXT, "80", 0, 0)), None);
    let unknown = TEXT.replace("default_client: home", "default_client: nobody");
    let (_owner, state, _) = mounted(&unknown, false);
    let byte = at(&unknown, "nobody", 0, 1);
    let line = unknown[..byte].matches('\n').count();
    let start = unknown[..byte].rfind('\n').map_or(0, |at| at + 1);
    let field = field_at(&unknown, line, byte - start, state.behavior()).expect("a field");
    let hover = state.field_docs(&unknown, &field).expect("a card");
    assert_eq!(hover.name_links.map(|links| links.definition), Some(false));
}

/// Find uses walks the document once per request, and typing never does.
#[test]
fn find_uses_walks_once_per_request_and_never_while_typing() {
    let (_owner, state, _) = mounted(TEXT, false);
    let (_, walks) = counted(Work::UsesWalk, || {
        for index in 0..20 {
            let before = state.doc().text.get_untracked().to_string();
            let end = before.encode_utf16().count();
            let text = format!("{before}# {index}\n");
            state.commit(crate::editor::edit::Edit::new_units(
                before,
                (0, end),
                text.clone(),
                text.encode_utf16().count(),
            ));
            let _ = state.problems().diagnostics.get_untracked();
            let _ = state.completion().suggestions.get_untracked();
        }
    });
    assert_eq!(walks, 0);
    state.doc().caret.set(text_size(at(TEXT, "home", 1, 1)));
    let (_, walks) = counted(Work::UsesWalk, || state.find_uses());
    assert_eq!(walks, 1);
}
