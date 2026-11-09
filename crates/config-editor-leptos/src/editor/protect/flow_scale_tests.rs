//! Locks on many fields of one list or map, in every language and through
//! every Edit-as view: each lock's place (its key, the keys holding it) is
//! read from the containers written between brackets each walked once per
//! text, not from the list's start for every lock (a key typed in a JSON
//! view of a pom with hundreds of locked versions took half a second).

use leptos::prelude::*;
use serde_json::{Value, json};

use super::super::Language;
use super::super::edit::Edit;
use super::super::paths::FieldPath;
use super::super::test_state::TestEditor;
use super::super::work_count::{Work, counted};
use super::locks::{Sources, protect};

/// Where the services are: under the root element in XML.
fn root(language: Language) -> &'static str {
    if language.label() == "XML" {
        "/root"
    } else {
        ""
    }
}

/// `count` services, as a list (`/services/3/host`) or by name
/// (`/services/svc_3/host`), written in `language`; and the pointer of
/// each one's `host`.
fn services(language: Language, count: usize, listed: bool) -> (String, Vec<String>) {
    let service = |index: usize| json!({"host": format!("h{index}.example.com"), "port": index, "tags": ["a", "b"]});
    let services: Value = if listed {
        Value::Array((0..count).map(service).collect())
    } else {
        Value::Object(
            (0..count)
                .map(|index| (format!("svc_{index}"), service(index)))
                .collect(),
        )
    };
    let data = if root(language).is_empty() {
        json!({"services": services})
    } else {
        json!({"root": {"services": services}})
    };
    let text = language.serialize(&data).expect("writes");
    let pointers = (0..count)
        .map(|index| match listed {
            true => format!("{}/services/{index}/host", root(language)),
            false => format!("{}/services/svc_{index}/host", root(language)),
        })
        .collect();
    (text, pointers)
}

/// The entries of bracketed containers walked placing a lock on every
/// service's host, and the locks placed.
fn walked(language: Language, count: usize, listed: bool) -> (usize, usize) {
    let (text, pointers) = services(language, count, listed);
    let paths: Vec<FieldPath> = pointers
        .iter()
        .map(|pointer| FieldPath::from(pointer.as_str()))
        .collect();
    let (protected, entries) = counted(Work::FlowEntry, || {
        protect(
            language.behavior(),
            &text,
            Sources {
                paths: &paths,
                ..Sources::default()
            },
        )
    });
    (entries, protected.locks.len())
}

#[test]
fn placing_a_lock_on_every_item_walks_each_container_once_in_every_language() {
    let mut grew = Vec::new();
    for &language in Language::ALL {
        for listed in [true, false] {
            let (few, few_locks) = walked(language, 50, listed);
            let (many, many_locks) = walked(language, 400, listed);
            assert_eq!(
                (few_locks, many_locks),
                (50, 400),
                "{language} (listed: {listed})"
            );
            // Each service holds a handful of entries: eight times the
            // services, about eight times the entries walked.
            if many > 10 * few.max(50) {
                grew.push(format!(
                    "{language} (listed: {listed}): 50 locks {few} entries, 400 locks {many}"
                ));
            }
        }
    }
    assert!(grew.is_empty(), "{grew:#?}");
}

/// The entries walked by one key typed in the middle of the `view` text of
/// a `host` document of `count` listed services, each one's host locked.
fn typed_in_view(host: Language, view: Language, count: usize) -> usize {
    let (text, pointers) = services(host, count, true);
    let (_owner, state) = TestEditor {
        edit_as: Some(view),
        read_only_paths: pointers
            .iter()
            .map(|pointer| FieldPath::from(pointer.as_str()))
            .collect(),
        ..TestEditor::new(host, &text)
    }
    .mount();
    assert_eq!(
        state.sync().edit_language.get_untracked(),
        view,
        "{host} as {view}"
    );
    let locks = state
        .protect()
        .protected
        .with_untracked(|protected| protected.locks.len());
    assert_eq!(locks, count, "{host} as {view}: every lock placed");
    let before = state.doc().text.get_untracked();
    let middle = format!("h{}.example.com", count / 2);
    let byte = before.find(&middle).expect("written") + middle.len() + 1;
    let byte = before[byte..]
        .find("\n")
        .map_or(before.len(), |at| byte + at);
    let at = before[..byte].encode_utf16().count();
    let mut after = before.clone();
    after.insert(byte, ' ');
    let typed = Edit::input(
        before,
        (at, at),
        after,
        (at + 1, at + 1),
        "insertText".into(),
        0.0,
    );
    let (made, entries) = counted(Work::FlowEntry, || {
        let made = state.commit(typed);
        state.protect().protected.with_untracked(|_| ());
        made
    });
    assert!(made, "{host} as {view}: a space after a line is typed");
    entries
}

#[test]
fn a_key_typed_in_any_view_of_many_locks_walks_each_container_once() {
    let mut grew = Vec::new();
    for &host in Language::ALL {
        for &view in Language::ALL {
            if view == host {
                continue;
            }
            let few = typed_in_view(host, view, 50);
            let many = typed_in_view(host, view, 400);
            if many > 10 * few.max(50) {
                grew.push(format!(
                    "{host} as {view}: 50 locks {few} entries, 400 locks {many}"
                ));
            }
        }
    }
    assert!(grew.is_empty(), "{grew:#?}");
}
