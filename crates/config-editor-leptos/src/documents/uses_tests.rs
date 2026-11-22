//! Find uses across a documents view's open documents: every other open
//! document whose schema names the map (its own, or by `from`) is read,
//! each walked once per request and none while typing; a key at a pointer
//! another schema's keyword names is a definition; the host's uses for an
//! open document's id are left out for its live ones.

use leptos::prelude::*;

use super::entries::{Callbacks, Documents};
use crate::editor::key_names::{NameRef, OpenUses, SchemaIdentity};
use crate::editor::work_count::{Work, counted};
use crate::{Document, EditorSettings};

const SERVER: &str = r#"{"$id": "https://example.com/server", "type": "object",
  "properties": {"default_client": {"type": "string", "x-config-editor-keys": "/clients"}}}"#;
const DROP_IN: &str = r#"{"$id": "https://example.com/drop-in", "type": "object",
  "properties": {"client": {"type": "string",
    "x-config-editor-keys": {"pointer": "/clients", "from": "https://example.com/server"}}}}"#;
const OTHER: &str = r#"{"$id": "https://example.com/other", "type": "object",
  "properties": {"client": {"type": "string", "x-config-editor-keys": "/clients"}}}"#;

fn view(list: &[Document]) -> Documents {
    let documents = Documents::new(
        Callbacks::default(),
        Signal::stored(EditorSettings::default()),
    );
    documents.sync(list);
    documents
}

fn uses_from(documents: Documents, id: &str) -> super::uses::ViewUses {
    super::uses::ViewUses {
        documents,
        id: id.to_string(),
    }
}

fn home() -> NameRef {
    NameRef {
        schema: SchemaIdentity::Id("https://example.com/server".into()),
        pointer: "/clients".into(),
        name: "home".into(),
    }
}

fn list() -> Vec<Document> {
    vec![
        Document::new(
            "main",
            "server.yaml",
            "clients:\n  home: {}\ndefault_client: home\n",
        )
        .schema(SERVER),
        Document::new("second", "second.yaml", "default_client: home\n").schema(SERVER),
        Document::new("drop", "server.d/drop.yaml", "client: home\n").schema(DROP_IN),
        Document::new("other", "other.yaml", "client: home\n").schema(OTHER),
        Document::new("plain", "plain.yaml", "default_client: home\n"),
    ]
}

#[test]
fn the_other_open_documents_of_the_names_schema_are_searched() {
    let owner = Owner::new();
    owner.set();
    let documents = view(&list());
    let open = uses_from(documents, "main");
    let found: Vec<(String, String, Vec<String>)> = open
        .uses(&home())
        .into_iter()
        .map(|document| {
            let paths = document
                .hits
                .iter()
                .map(|hit| hit.path.to_pointer())
                .collect();
            (document.id, document.label, paths)
        })
        .collect();
    assert_eq!(
        found,
        [
            (
                "second".to_string(),
                "second.yaml".to_string(),
                vec!["/default_client".to_string()]
            ),
            (
                "drop".to_string(),
                "server.d/drop.yaml".to_string(),
                vec!["/client".to_string()]
            ),
        ]
    );
    assert!(open.is_open("drop"));
    assert!(!open.is_open("closed.yaml"));
    // A drop-in's keyword makes the server's `/clients` a names map.
    let server = SchemaIdentity::Id("https://example.com/server".into());
    assert!(open.names_map(&server, "/clients"));
    assert!(!open.names_map(&server, "/groups"));
}

#[test]
fn a_request_walks_each_document_once_and_typing_walks_none() {
    let owner = Owner::new();
    owner.set();
    let mut list = list();
    let documents = view(&list);
    let open = uses_from(documents, "main");
    // The two documents whose schema names the map: once each.
    let (_, walks) = counted(Work::UsesWalk, || open.uses(&home()));
    assert_eq!(walks, 2);
    let (_, walks) = counted(Work::UsesWalk, || {
        for index in 0..10 {
            list[1].value = format!("default_client: home\n# {index}\n");
            documents.sync(&list);
        }
    });
    assert_eq!(walks, 0);
    let (found, walks) = counted(Work::UsesWalk, || open.uses(&home()));
    assert_eq!((found.len(), walks), (2, 2));
}
