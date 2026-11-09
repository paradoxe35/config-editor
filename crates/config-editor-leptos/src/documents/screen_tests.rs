//! What the view tells the host of each document's screen (the language
//! on screen, whether the text is written back), asking for a language
//! again, a kept text and a read-only reason, apart from the DOM.

use std::sync::{Arc, Mutex, MutexGuard};

use leptos::prelude::*;

use super::entries::{Callbacks, Documents};
use super::model::Document;
use crate::{EditorSettings, Language};

fn files() -> Vec<Document> {
    vec![
        Document::new("main", "burrow.yaml", "listen: 7835\n").closable(false),
        Document::new("tls", "conf.d/tls.yaml", "cert: a.pem\n"),
    ]
}

/// What `on_edit_as_change` and `on_sync_change` heard, in order.
type Heard = Arc<Mutex<Vec<String>>>;

/// A view over `list` reporting the screen of each document.
fn screen_view(list: &[Document]) -> (Documents, Heard) {
    let heard: Heard = Arc::default();
    let languages = heard.clone();
    let syncs = heard.clone();
    let documents = Documents::new(
        Callbacks {
            on_edit_as_change: Some(Callback::new(move |(id, language): (String, Language)| {
                lock_heard(&languages).push(format!("{id} {language}"));
            })),
            on_sync_change: Some(Callback::new(move |(id, in_sync): (String, bool)| {
                lock_heard(&syncs).push(format!("{id} {in_sync}"));
            })),
            ..Callbacks::default()
        },
        Signal::stored(EditorSettings::default()),
    );
    documents.sync(list);
    (documents, heard)
}

fn lock_heard(heard: &Heard) -> MutexGuard<'_, Vec<String>> {
    heard.lock().expect("not poisoned")
}

fn heard(heard: &Heard) -> Vec<String> {
    std::mem::take(&mut *lock_heard(heard))
}

#[test]
fn the_screen_of_each_document_is_told_once_per_change() {
    let owner = Owner::new();
    owner.set();
    let (documents, told) = screen_view(&files());
    let main = documents.entry("main").expect("listed");
    let tls = documents.entry("tls").expect("listed");
    // Mounted, each editor tells its language and its sync state; the same
    // again (the editor's own report of what it mounted with) is not news.
    documents.note_language(&main, Language::YAML);
    documents.note_sync(&main, true);
    documents.note_language(&main, Language::YAML);
    documents.note_sync(&main, true);
    assert_eq!(heard(&told), ["main YAML", "main true"]);
    // Two documents at once, each its own.
    documents.note_language(&tls, Language::TOML);
    documents.note_sync(&tls, false);
    documents.note_sync(&main, false);
    documents.note_sync(&tls, false);
    documents.note_language(&main, Language::YAML);
    documents.note_sync(&tls, true);
    assert_eq!(
        heard(&told),
        ["tls TOML", "tls false", "main false", "tls true"]
    );
}

#[test]
fn the_handle_asks_for_a_language_again_and_the_list_still_rules() {
    let owner = Owner::new();
    owner.set();
    let mut list = files();
    list[0].edit_as = Some(Language::TOML);
    let (documents, _) = screen_view(&list);
    let main = documents.entry("main").expect("listed");
    assert!(documents.edit_as("main", Language::TOML));
    assert_eq!(main.edit_as.get_untracked(), Some(Language::TOML));
    // Its own language is no `edit_as`.
    assert!(documents.edit_as("main", Language::YAML));
    assert_eq!(main.edit_as.get_untracked(), None);
    assert!(!documents.edit_as("missing", Language::TOML));
    // The next list says TOML again.
    documents.sync(&list);
    assert_eq!(main.edit_as.get_untracked(), Some(Language::TOML));
}

#[test]
fn a_kept_text_and_a_read_only_reason_follow_the_list() {
    let owner = Owner::new();
    owner.set();
    let mut list = files();
    list[1] = Document::new("tls", "conf.d/tls.yaml", "cert: a.pem\n")
        .edit_as_text(Language::TOML, "cert = \n")
        .read_only_reason("Managed by the operator");
    assert!(list[1].read_only);
    let (documents, _) = screen_view(&list);
    let tls = documents.entry("tls").expect("listed");
    assert_eq!(tls.edit_as.get_untracked(), Some(Language::TOML));
    assert_eq!(tls.edit_as_text.get_value().as_deref(), Some("cert = \n"));
    assert_eq!(
        tls.read_only_reason.get_untracked().as_deref(),
        Some("Managed by the operator")
    );
    // Not mounted yet: a new list's text replaces it; the reason updates live.
    list[1].edit_as_text = Some("cert = \"b\"\n".into());
    list[1].read_only_reason = None;
    documents.sync(&list);
    assert_eq!(
        tls.edit_as_text.get_value().as_deref(),
        Some("cert = \"b\"\n")
    );
    assert_eq!(tls.read_only_reason.get_untracked(), None);
}
