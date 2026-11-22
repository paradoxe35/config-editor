//! The names a documents view knows for a field naming a key defined
//! elsewhere: which open documents count (by schema identity and `from`
//! lists), the host's names and when open documents replace them, and that
//! a keystroke reads only the edited document's keys.

use leptos::prelude::*;

use super::entries::{Callbacks, Documents};
use super::key_names::identity_of;
use crate::editor::key_names::SchemaIdentity;
use crate::editor::work_count::{Work, counted};
use crate::{Document, EditorSettings, GoToDefinition, KeyName, KeyNames, Schema};

const SERVER: &str = r#"{"$id": "https://example.com/server", "type": "object"}"#;
const DROP_IN: &str = r#"{"$id": "https://example.com/drop-in", "type": "object"}"#;
const PLAIN: &str = r#"{"type": "object"}"#;
const OTHER_PLAIN: &str = r#"{"type": "object", "title": "other"}"#;

fn view(list: &[Document]) -> Documents {
    let documents = Documents::new(
        Callbacks::default(),
        Signal::stored(EditorSettings::default()),
    );
    documents.sync(list);
    documents
}

fn clients(id: &str, names: &[&str]) -> Document {
    let mut text = String::from("clients:\n");
    for name in names {
        text.push_str(&format!("  {name}:\n    weight: 1\n"));
    }
    Document::new(id, format!("server.d/{id}.yaml"), text)
}

/// The names the editor of document `id` is given at `/clients`, asked as
/// the suggestions ask its source: for its own schema, then for each
/// schema of `from`, each name once.
fn found_from(documents: Documents, id: &str, from: &[&str]) -> Vec<KeyName> {
    let entry = documents.entry(id).expect("listed");
    let schema = identity_of(entry.setup.schema.as_ref()).expect("a compiled schema");
    let source = documents.key_names_input(&entry).source;
    let mut found: Vec<KeyName> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let schemas =
        std::iter::once(schema).chain(from.iter().map(|id| SchemaIdentity::Id((*id).to_string())));
    for schema in schemas {
        // As a suggestion run asks, outside one here.
        for name in untrack(|| source.names(&schema, "/clients")) {
            if seen.insert(name.name.clone()) {
                found.push(name);
            }
        }
    }
    found
}

fn names_from(documents: Documents, id: &str, from: &[&str]) -> Vec<String> {
    found_from(documents, id, from)
        .into_iter()
        .map(|found| found.name)
        .collect()
}

fn names(documents: Documents, id: &str) -> Vec<String> {
    names_from(documents, id, &[])
}

/// Lists `list` again with document `id`'s text replaced by `text`, as a
/// host does after each keystroke.
fn retype(documents: Documents, list: &mut [Document], id: &str, text: &str) {
    if let Some(document) = list.iter_mut().find(|document| document.id == id) {
        document.value = text.to_string();
    }
    documents.sync(list);
}

#[test]
fn a_keystroke_reads_again_only_the_edited_document_s_keys() {
    let owner = Owner::new();
    owner.set();
    let mut list: Vec<Document> = (0..20)
        .map(|index| clients(&format!("d{index}"), &[&format!("c{index}")]).schema(SERVER))
        .collect();
    let documents = view(&list);
    let (first, reads) = counted(Work::KeyNamesRead, || names(documents, "d0"));
    assert_eq!(first.len(), 19);
    assert_eq!(reads, 19, "the first lookup reads each other document once");
    let (_, reads) = counted(Work::KeyNamesRead, || names(documents, "d0"));
    assert_eq!(
        reads, 0,
        "nothing changed: every document answers as before"
    );
    retype(documents, &mut list, "d7", "clients:\n  c7:\n  typed:\n");
    let (after, reads) = counted(Work::KeyNamesRead, || names(documents, "d0"));
    assert_eq!(reads, 1, "only the document typed in is read again");
    assert!(after.contains(&"typed".to_string()), "{after:?}");
}

#[test]
fn a_document_that_does_not_parse_keeps_its_last_good_names() {
    let owner = Owner::new();
    owner.set();
    let mut list = vec![
        clients("main", &[]).schema(SERVER),
        clients("home", &["home", "office"]).schema(SERVER),
    ];
    let documents = view(&list);
    assert_eq!(names(documents, "main"), ["home", "office"]);
    retype(documents, &mut list, "home", "clients:\n  home: [\n");
    assert_eq!(names(documents, "main"), ["home", "office"]);
    retype(documents, &mut list, "home", "clients:\n  lab: {}\n");
    assert_eq!(names(documents, "main"), ["lab"]);
}

#[test]
fn an_open_document_s_live_names_replace_the_host_s_for_it() {
    let owner = Owner::new();
    owner.set();
    let host = vec![
        KeyNames::new(
            "/clients",
            vec![
                KeyName::new("saved-home").with_source("home"),
                KeyName::new("closed-lab").with_source("lab"),
                KeyName::new("loose"),
            ],
        )
        .for_schema("https://example.com/server"),
    ];
    let list = vec![
        clients("main", &[]).schema(SERVER),
        clients("home", &["unsaved-home"]).schema(SERVER),
    ];
    let documents = view(&list);
    documents.give_key_names(Signal::stored(host));
    let found = found_from(documents, "main", &[]);
    let shown: Vec<(&str, Option<&str>)> = found
        .iter()
        .map(|found| (found.name.as_str(), found.source.as_deref()))
        .collect();
    assert_eq!(
        shown,
        [
            ("unsaved-home", Some("server.d/home.yaml")),
            ("closed-lab", Some("lab")),
            ("loose", None)
        ]
    );
}

#[test]
fn documents_share_names_by_schema_id_or_by_the_same_schema_text() {
    let owner = Owner::new();
    owner.set();
    // The same `$id` in two texts is one schema; one compiled by the host
    // and handed over is the same by its `$id` too.
    let server_again = r#"{"$id": "https://example.com/server", "title": "again"}"#;
    let compiled = Schema::compile(SERVER).expect("compiles");
    let list = vec![
        clients("main", &[]).schema(SERVER),
        clients("a", &["by-id"]).schema(server_again),
        clients("b", &["compiled"]).schema(compiled),
        clients("plain", &[]).schema(PLAIN),
        clients("same-text", &["same-text"]).schema(PLAIN),
        clients("other-text", &["other-text"]).schema(OTHER_PLAIN),
        clients("no-schema", &["no-schema"]),
    ];
    let documents = view(&list);
    assert_eq!(names(documents, "main"), ["by-id", "compiled"]);
    assert_eq!(names(documents, "plain"), ["same-text"]);
}

#[test]
fn a_from_list_takes_the_names_of_the_schemas_it_names() {
    let owner = Owner::new();
    owner.set();
    let list = vec![
        clients("main", &["home"]).schema(SERVER),
        clients("tls", &["tls"]).schema(DROP_IN),
        clients("plain", &["plain"]).schema(PLAIN),
    ];
    let documents = view(&list);
    assert_eq!(names(documents, "tls"), Vec::<String>::new());
    assert_eq!(
        names_from(documents, "tls", &["https://example.com/server"]),
        ["home"]
    );
    assert_eq!(
        names_from(
            documents,
            "main",
            &["https://example.com/drop-in", "https://example.com/server"]
        ),
        ["tls"]
    );
}

#[test]
fn host_names_without_a_schema_apply_only_to_schemas_without_an_id() {
    let owner = Owner::new();
    owner.set();
    let list = vec![
        clients("main", &[]).schema(SERVER),
        clients("plain", &[]).schema(PLAIN),
    ];
    let documents = view(&list);
    documents.give_key_names(Signal::stored(vec![
        KeyNames::new("/clients", vec![KeyName::new("anywhere")]),
        KeyNames::new("/clients", vec![KeyName::new("server")])
            .for_schema("https://example.com/server"),
        KeyNames::new("/groups", vec![KeyName::new("elsewhere")]),
    ]));
    assert_eq!(names(documents, "main"), ["server"]);
    assert_eq!(names(documents, "plain"), ["anywhere"]);
}

#[test]
fn a_document_s_own_host_names_add_to_the_view_s() {
    let owner = Owner::new();
    owner.set();
    let list = vec![
        clients("main", &[]).schema(SERVER).key_names(vec![
            KeyNames::new("/clients", vec![KeyName::new("own")])
                .for_schema("https://example.com/server"),
        ]),
        clients("other", &[]).schema(SERVER),
    ];
    let documents = view(&list);
    documents.give_key_names(Signal::stored(vec![
        KeyNames::new(
            "/clients",
            vec![KeyName::new("shared"), KeyName::new("own")],
        )
        .for_schema("https://example.com/server"),
    ]));
    assert_eq!(names(documents, "main"), ["shared", "own"]);
    assert_eq!(names(documents, "other"), ["shared", "own"]);
    assert_eq!(
        found_from(documents, "other", &[]).len(),
        2,
        "the other document has no own names"
    );
}

#[test]
fn a_closed_document_s_names_go_with_it() {
    let owner = Owner::new();
    owner.set();
    let mut list = vec![
        clients("main", &[]).schema(SERVER),
        clients("home", &["home"]).schema(SERVER),
    ];
    let documents = view(&list);
    assert_eq!(names(documents, "main"), ["home"]);
    list.retain(|document| document.id != "home");
    documents.sync(&list);
    assert_eq!(names(documents, "main"), Vec::<String>::new());
    // Listed again, it is read afresh.
    list.push(clients("home", &["again"]).schema(SERVER));
    documents.sync(&list);
    assert_eq!(names(documents, "main"), ["again"]);
}

#[test]
fn a_document_s_lookup_leaves_its_own_names_to_its_editor() {
    let owner = Owner::new();
    owner.set();
    let list = vec![
        clients("main", &["mine"]).schema(SERVER),
        clients("home", &["home", "mine"]).schema(SERVER),
    ];
    let documents = view(&list);
    assert_eq!(names(documents, "main"), ["home", "mine"]);
    assert_eq!(names(documents, "home"), ["mine"]);
}

/// Typing in one of 100 open documents with about 1 000 names, the field
/// typed in asking for them after each keystroke, costs about what the
/// same typing costs with no field asking (P5): the names of the other
/// documents are read once, not again per keystroke. Timed as the
/// `test_timing` tests are: both kinds of typing in turn, the fastest
/// run of each, against a generous bound.
#[test]
fn typing_among_a_hundred_open_documents_asking_their_names_stays_fast() {
    let owner = Owner::new();
    owner.set();
    let names_of = |index: usize| -> Vec<String> {
        (0..10)
            .map(|name| format!("client-{index}-{name}"))
            .collect()
    };
    let mut list: Vec<Document> = (0..100)
        .map(|index| {
            let names = names_of(index);
            let names: Vec<&str> = names.iter().map(String::as_str).collect();
            clients(&format!("d{index}"), &names).schema(SERVER)
        })
        .collect();
    let documents = view(&list);
    assert_eq!(names(documents, "d0").len(), 990);
    let typed = list[0].value.clone();
    // The source the editor of `d0` asks, as its suggestions ask it after
    // each keystroke in a value of the field.
    let entry = documents.entry("d0").expect("listed");
    let schema = identity_of(entry.setup.schema.as_ref()).expect("a compiled schema");
    let source = documents.key_names_input(&entry).source;
    let mut keystroke = 0;
    let growth = crate::editor::test_timing::growth(7, (&false, &true), |&asks| {
        for _ in 0..20 {
            keystroke += 1;
            let text = format!("{typed}  typed-{keystroke}:\n");
            retype(documents, &mut list, "d0", &text);
            if asks {
                std::hint::black_box(untrack(|| source.names(&schema, "/clients")));
            }
        }
    });
    assert!(growth.ratio() < 3.0, "without names, then with: {growth}");
    // And the absolute bound: twenty keystrokes asking the names, each in
    // well under a frame.
    assert!(
        growth.large < std::time::Duration::from_millis(200),
        "{growth}"
    );
}

/// Go to definition from a document's editor: a name another open document
/// defines shows that document; a name only the host knows (a file not
/// open), or one with no source, goes to the host's `on_go_to_definition`.
#[test]
fn go_to_definition_shows_an_open_document_and_hands_the_rest_to_the_host() {
    let owner = Owner::new();
    owner.set();
    let asked = std::sync::Arc::new(std::sync::Mutex::new(Vec::<Option<String>>::new()));
    let told = asked.clone();
    let documents = Documents::new(
        Callbacks {
            on_go_to_definition: Some(Callback::new(move |target: GoToDefinition| {
                told.lock().expect("not poisoned").push(target.source);
            })),
            ..Callbacks::default()
        },
        Signal::stored(EditorSettings::default()),
    );
    documents.sync(&[
        clients("main", &[]).schema(SERVER),
        clients("lab", &["lab"]).schema(SERVER),
    ]);
    let entry = documents.entry("main").expect("listed");
    let go = documents
        .key_names_input(&entry)
        .on_go_to_definition
        .expect("the view takes go to definition");
    let target = |source: Option<&str>| GoToDefinition {
        schema: Some("https://example.com/server".into()),
        pointer: "/clients".into(),
        name: "lab".into(),
        source: source.map(Into::into),
    };
    assert_eq!(documents.active_id().as_deref(), Some("main"));
    untrack(|| go.run(target(Some("lab"))));
    assert_eq!(documents.active_id().as_deref(), Some("lab"));
    untrack(|| go.run(target(Some("server.d/office.yaml"))));
    untrack(|| go.run(target(None)));
    assert_eq!(
        *asked.lock().expect("not poisoned"),
        [Some("server.d/office.yaml".to_string()), None]
    );
    assert_eq!(documents.active_id().as_deref(), Some("lab"));
}

/// An open document's names show its file name, as its tab does, whatever
/// its id; go to definition from that name still finds its tab: of two
/// documents with one file name, the one defining the name.
#[test]
fn an_open_document_s_names_show_its_file_name_and_lead_to_its_tab() {
    let owner = Owner::new();
    owner.set();
    let documents = view(&[
        Document::new("d-1", "server.yaml", "clients: {}\n").schema(SERVER),
        Document::new("d-2", "server.d/lab.yaml", "clients:\n  old: {}\n").schema(SERVER),
        Document::new("d-3", "server.d/lab.yaml", "clients:\n  lab: {}\n").schema(SERVER),
    ]);
    let found = found_from(documents, "d-1", &[]);
    let shown: Vec<(&str, Option<&str>)> = found
        .iter()
        .map(|found| (found.name.as_str(), found.source.as_deref()))
        .collect();
    assert_eq!(
        shown,
        [
            ("old", Some("server.d/lab.yaml")),
            ("lab", Some("server.d/lab.yaml"))
        ]
    );
    let entry = documents.entry("d-1").expect("listed");
    let go = documents
        .key_names_input(&entry)
        .on_go_to_definition
        .expect("the view takes go to definition");
    untrack(|| {
        go.run(GoToDefinition {
            schema: Some("https://example.com/server".into()),
            pointer: "/clients".into(),
            name: "lab".into(),
            source: Some("server.d/lab.yaml".into()),
        });
    });
    assert_eq!(documents.active_id().as_deref(), Some("d-3"));
}
