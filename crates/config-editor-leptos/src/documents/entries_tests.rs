//! Adding and closing as the view decides them, apart from the DOM: which
//! tabs may be closed, what the gestures ask the host, and the document
//! the host adds for the "+".

use std::sync::{Arc, Mutex, MutexGuard};

use leptos::prelude::*;

use super::{Callbacks, Documents, Focus, focus_place};
use crate::EditorSettings;
use crate::documents::model::{CloseRequest, Document};
use crate::reactive::later::testing::{hold, run_held, waiting_timers};

/// What the host was asked.
#[derive(Default)]
struct Asked {
    closes: Vec<String>,
    adds: usize,
    selects: Vec<String>,
}

type Shared = Arc<Mutex<Asked>>;

fn lock(asked: &Shared) -> MutexGuard<'_, Asked> {
    asked.lock().expect("not poisoned")
}

/// A view over `list` with `on_close` (when `closes`) and `on_add` (when
/// `adds`), and what it asks the host.
fn view(list: &[Document], closes: bool, adds: bool) -> (Documents, Shared) {
    let asked: Shared = Arc::default();
    let on_close = closes.then(|| {
        let asked = asked.clone();
        Callback::new(move |request: CloseRequest| lock(&asked).closes.push(request.id))
    });
    let on_add = adds.then(|| {
        let asked = asked.clone();
        Callback::new(move |()| lock(&asked).adds += 1)
    });
    let on_select = {
        let asked = asked.clone();
        Some(Callback::new(move |id: String| {
            lock(&asked).selects.push(id)
        }))
    };
    let documents = Documents::new(
        Callbacks {
            on_select,
            on_close,
            on_add,
            ..Callbacks::default()
        },
        Signal::stored(EditorSettings::default()),
    );
    documents.sync(list);
    (documents, asked)
}

fn files() -> Vec<Document> {
    vec![
        Document::new("main", "burrow.yaml", "listen: 7835\n").closable(false),
        Document::new("tls", "conf.d/tls.yaml", "cert: a.pem\n"),
    ]
}

/// Whether the tab of `id` may be closed (what draws its close button and
/// lets a middle click and `Delete` ask).
fn closes(documents: Documents, id: &str) -> bool {
    let entry = documents.entry(id).expect("listed");
    untrack(|| documents.closes(&entry))
}

#[test]
fn only_closable_documents_ask_to_close() {
    let owner = Owner::new();
    owner.set();
    let (documents, asked) = view(&files(), true, false);
    assert!(!closes(documents, "main"));
    assert!(closes(documents, "tls"));
    documents.request_close("main");
    documents.request_close("tls");
    assert_eq!(lock(&asked).closes, ["tls"]);
}

#[test]
fn documents_are_closable_by_default_and_never_without_on_close() {
    let owner = Owner::new();
    owner.set();
    assert!(Document::new("a", "a.yaml", "").closable);
    let list = vec![
        Document::new("a", "a.yaml", ""),
        Document::new("b", "b.yaml", "").closable(true),
    ];
    let (documents, _) = view(&list, true, false);
    assert!(closes(documents, "a") && closes(documents, "b"));
    let (documents, asked) = view(&list, false, false);
    assert!(!closes(documents, "a") && !closes(documents, "b"));
    documents.request_close("a");
    assert!(lock(&asked).closes.is_empty());
}

#[test]
fn closable_switches_live() {
    let owner = Owner::new();
    owner.set();
    let mut list = files();
    let (documents, asked) = view(&list, true, false);
    list[0].closable = true;
    list[1].closable = false;
    documents.sync(&list);
    assert!(closes(documents, "main"));
    assert!(!closes(documents, "tls"));
    documents.request_close("tls");
    documents.request_close("main");
    assert_eq!(lock(&asked).closes, ["main"]);
}

#[test]
fn the_plus_only_asks_and_the_document_the_host_adds_is_selected_and_focused() {
    let owner = Owner::new();
    owner.set();
    hold(|| {
        let mut list = files();
        let (documents, asked) = view(&list, true, true);
        assert_eq!(documents.active_id().as_deref(), Some("main"));
        documents.request_add();
        // Asked again: one window, from the last click.
        documents.request_add();
        assert_eq!(waiting_timers(), 1);
        assert_eq!(lock(&asked).adds, 2);
        // The view creates nothing itself.
        assert_eq!(documents.ids(), ["main", "tls"]);
        // A list that adds nothing (an edit) leaves the request waiting.
        list[0].value = "listen: 7836\n".into();
        documents.sync(&list);
        assert_eq!(documents.active_id().as_deref(), Some("main"));
        list.push(Document::new("new", "untitled.yaml", ""));
        documents.sync(&list);
        assert_eq!(documents.active_id().as_deref(), Some("new"));
        assert_eq!(
            documents.focus_editor.get_untracked().as_deref(),
            Some("new")
        );
        assert!(
            documents
                .entry("new")
                .expect("listed")
                .visited
                .get_untracked()
        );
        assert_eq!(lock(&asked).selects, ["new"]);
        // Taken, its timer with it: the next document added is not selected.
        assert_eq!(waiting_timers(), 0);
        list.push(Document::new("later", "later.yaml", ""));
        documents.sync(&list);
        assert_eq!(documents.active_id().as_deref(), Some("new"));
    });
}

/// QA 23: a "+" the host did not answer stayed armed, and a document a
/// server pushed much later took the selection and the focus.
#[test]
fn a_plus_the_host_does_not_answer_in_time_selects_nothing_added_later() {
    let owner = Owner::new();
    owner.set();
    hold(|| {
        let mut list = files();
        let (documents, asked) = view(&list, true, true);
        documents.request_add();
        // The host adds nothing, and the window passes.
        run_held();
        list.push(Document::new("pushed", "pushed.yaml", ""));
        documents.sync(&list);
        assert_eq!(documents.active_id().as_deref(), Some("main"));
        assert_eq!(documents.focus_editor.get_untracked(), None);
        assert!(lock(&asked).selects.is_empty());
    });
}

#[test]
fn without_a_request_or_after_picking_a_tab_an_added_document_is_not_selected() {
    let owner = Owner::new();
    owner.set();
    hold(|| {
        let mut list = files();
        let (documents, asked) = view(&list, true, true);
        list.push(Document::new("host", "host.yaml", ""));
        documents.sync(&list);
        assert_eq!(documents.active_id().as_deref(), Some("main"));
        // The user asked, then picked another tab before the host added one.
        documents.request_add();
        documents.pick("tls", Focus::Keep);
        assert_eq!(waiting_timers(), 0);
        list.push(Document::new("late", "late.yaml", ""));
        documents.sync(&list);
        assert_eq!(documents.active_id().as_deref(), Some("tls"));
        assert_eq!(lock(&asked).selects, ["tls"]);
    });
}

#[test]
fn without_on_add_nothing_is_asked() {
    let owner = Owner::new();
    owner.set();
    let mut list = files();
    let (documents, asked) = view(&list, true, false);
    documents.request_add();
    assert_eq!(lock(&asked).adds, 0);
    list.push(Document::new("new", "new.yaml", ""));
    documents.sync(&list);
    assert_eq!(documents.active_id().as_deref(), Some("main"));
}

/// QA 23: closing the active tab with `Delete` sent the focus into the next
/// document's editor, so a second `Delete` edited its text. From the tab
/// list the focus stays on the tabs (the one now selected); from the
/// closed editor it goes into the next one; from outside it stays there.
#[test]
fn closing_the_active_document_keeps_the_focus_where_it_was() {
    assert_eq!(focus_place(true, true), Focus::Tab);
    assert_eq!(focus_place(false, true), Focus::Editor);
    assert_eq!(focus_place(false, false), Focus::Keep);
}
