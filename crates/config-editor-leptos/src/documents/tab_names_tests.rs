//! What labelling the tabs costs as the host lists the documents again
//! (QA 30, F30-1: a view of files sharing a name, Helm's `values.yaml` in
//! every chart, re-listed in quadratic time even when one value changed,
//! and renaming every file took minutes): each list costs work in
//! proportion to the tabs at most, and nothing for tabs whose name and the
//! names sharing it stay.

use leptos::prelude::*;

use super::entries::{Callbacks, Documents};
use super::model::Document;
use super::tab_names::{TabNames, tab_label};
use crate::EditorSettings;
use crate::editor::work_count::{Work, counted};

/// The documents' file names: each its own (`unique`) or all sharing one
/// (`charts/c1/values.yaml`, ...), under `folder`.
fn names(count: usize, shared: bool, folder: &str, file: &str) -> Vec<String> {
    (0..count)
        .map(|index| {
            if shared {
                format!("{folder}/c{index}/{file}.yaml")
            } else {
                format!("{folder}/{file}{index}.yaml")
            }
        })
        .collect()
}

fn listed(names: &[String]) -> Vec<Document> {
    names
        .iter()
        .enumerate()
        .map(|(index, name)| {
            Document::new(format!("d{index}"), name.clone(), format!("k: {index}\n"))
        })
        .collect()
}

/// The tabs' labels as the view holds them, against labels worked out
/// afresh for `names`.
fn assert_labels(documents: Documents, names: &[String]) {
    let held = documents.follows.names.read().get_untracked();
    let fresh = TabNames::new(names.iter().cloned());
    for name in names {
        assert_eq!(tab_label(name, &held), tab_label(name, &fresh), "{name}");
    }
}

/// The tab names read while the view of `count` documents named
/// `before` takes the list `change` makes of them.
fn work(count: usize, shared: bool, change: fn(&mut Vec<Document>, bool)) -> usize {
    let owner = Owner::new();
    owner.set();
    let documents = Documents::new(
        Callbacks::default(),
        Signal::stored(EditorSettings::default()),
    );
    let mut list = listed(&names(count, shared, "charts", "values"));
    documents.sync(&list);
    change(&mut list, shared);
    let ((), work) = counted(Work::TabNameVisit, || documents.sync(&list));
    let now: Vec<String> = list
        .iter()
        .map(|document| document.file_name.clone())
        .collect();
    assert_labels(documents, &now);
    work
}

/// The work at 800 documents is about eight times the work at 100, not
/// sixty-four.
fn assert_linear(what: &str, change: fn(&mut Vec<Document>, bool)) {
    for shared in [false, true] {
        let (few, many) = (work(100, shared, change), work(800, shared, change));
        assert!(
            many <= 10 * few + 100,
            "{what} (names shared: {shared}): {few} tab names read at 100 documents, {many} at 800"
        );
    }
}

#[test]
fn one_value_changed() {
    assert_linear("one value changed", |list, _| {
        list[0].value.push_str("x: 1\n")
    });
}

#[test]
fn one_document_added() {
    assert_linear("one added", |list, shared| {
        let count = list.len();
        let name = names(count + 1, shared, "charts", "values")
            .pop()
            .expect("one");
        list.push(Document::new("new", name, "k: 0\n"));
    });
}

#[test]
fn one_document_closed() {
    assert_linear("one closed", |list, _| {
        list.remove(list.len() / 2);
    });
}

#[test]
fn every_file_renamed() {
    assert_linear("every file renamed", |list, shared| {
        let renamed = names(list.len(), shared, "charts", "renamed");
        for (document, name) in list.iter_mut().zip(renamed) {
            document.file_name = name;
        }
    });
}

#[test]
fn every_folder_renamed() {
    assert_linear("every folder renamed", |list, shared| {
        let renamed = names(list.len(), shared, "apps", "values");
        for (document, name) in list.iter_mut().zip(renamed) {
            document.file_name = name;
        }
    });
}

/// A list that changes nothing of the names relabels no tab: its label
/// stays the same value, and the tabs reading it hear nothing.
#[test]
fn a_list_changing_no_name_keeps_the_labels() {
    let owner = Owner::new();
    owner.set();
    let documents = Documents::new(
        Callbacks::default(),
        Signal::stored(EditorSettings::default()),
    );
    let mut list = listed(&names(50, true, "charts", "values"));
    documents.sync(&list);
    let before = documents.follows.names.read().get_untracked();
    list[3].value.push_str("x: 1\n");
    list.swap(1, 2);
    documents.sync(&list);
    let after = documents.follows.names.read().get_untracked();
    assert!(std::sync::Arc::ptr_eq(&before, &after));
}
