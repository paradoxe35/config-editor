//! The links follow each kind of edit as it is made, in a YAML view.

use super::super::super::edit::Edit;
use super::super::super::item_sources::ItemSource;
use super::super::super::languages::Language;
use super::super::super::work_count::{Work, counted};
use super::{Clipboard, Links, Splice, pieces};
use std::collections::HashMap;

const HOST: &str = "the host's text";

/// An edit of `text`: `from..to` replaced by `inserted`.
fn splice(from: usize, to: usize, inserted: &str) -> Splice {
    Splice::replace(from, to - from, inserted)
}

/// Every `from` in `text` replaced by `to`, each its own splice (as
/// Replace all says).
fn replacing(text: &str, from: &str, to: &str) -> Vec<Splice> {
    Splice::each(
        text.match_indices(from)
            .map(|(at, _)| (at..at + from.len(), to.to_string())),
    )
}

/// The view's text and its links as the edits leave them.
struct View {
    text: String,
    links: Links,
    clipboard: Clipboard,
}

impl View {
    fn new(text: &str) -> Self {
        View {
            text: text.to_string(),
            links: Links::fresh(HOST, text, None),
            clipboard: Clipboard::default(),
        }
    }

    /// The command making `splices` (a paste of a record's text takes its
    /// links, as the editor's own paste does).
    fn edit(&mut self, splices: Vec<Splice>) {
        let after = Splice::make(&self.text, &splices).expect("the splices fit");
        let edit = Edit::new(self.text.clone(), (0, 0), after.clone(), 0).splicing(Some(splices));
        let pieces = pieces(&edit, &self.clipboard);
        self.links.edited(
            Language::YAML.behavior(),
            &self.text,
            &pieces,
            &mut self.clipboard,
        );
        self.text = after;
    }

    /// `from..to` replaced by `inserted`.
    fn replace(&mut self, from: usize, to: usize, inserted: &str) {
        self.edit(vec![splice(from, to, inserted)]);
    }

    fn copy(&mut self, from: usize, to: usize) {
        let copied = self
            .links
            .copied((Language::YAML.behavior(), &self.text), from, to);
        self.clipboard.copied(&self.text[from..to], copied);
    }

    /// Each item's source in the host, by its pointer now.
    fn sources(&mut self) -> HashMap<String, String> {
        let told = self
            .links
            .told(Language::YAML.behavior(), &self.text, HOST, true)
            .expect("told");
        told.items
            .into_iter()
            .map(|(pointer, source)| match source {
                ItemSource::Previous(from) => (pointer, from),
                ItemSource::Origin(from) => (pointer, format!("earlier {from}")),
            })
            .collect()
    }
}

fn source(view: &mut View, pointer: &str) -> Option<String> {
    view.sources().get(pointer).cloned()
}

#[test]
fn a_line_cut_and_pasted_takes_its_link_along() {
    let mut view = View::new("a:\n  x: 1\n  y: 2\nb:\n  z: 3\n");
    let line = "  x: 1\n";
    let at = view.text.find(line).expect("x");
    view.replace(at, at + line.len(), "");
    let to = view.text.find("  z: 3").expect("z");
    view.replace(to, to, line);
    assert_eq!(source(&mut view, "/b/x").as_deref(), Some("/a/x"));
    assert_eq!(source(&mut view, "/b/z").as_deref(), Some("/b/z"));
    assert_eq!(source(&mut view, "/a/y").as_deref(), Some("/a/y"));
}

#[test]
fn a_paste_over_a_selected_line_takes_the_cut_lines_link() {
    let mut view = View::new("a:\n  x: 1\n  y: 2\nb:\n  z: 3\n  w: 4\n");
    let line = "  x: 1\n";
    let at = view.text.find(line).expect("x");
    view.replace(at, at + line.len(), "");
    let over = view.text.find("  z: 3\n").expect("z");
    view.replace(over, over + "  z: 3\n".len(), line);
    let sources = view.sources();
    assert_eq!(sources.get("/b/x").map(String::as_str), Some("/a/x"));
    assert!(!sources.values().any(|from| from == "/b/z"), "{sources:?}");
}

#[test]
fn a_copy_pasted_twice_names_its_source_each_time() {
    let mut view = View::new("a:\n  x: 1\nb:\n  z: 3\nc:\n  w: 4\n");
    let line = "  x: 1\n";
    let at = view.text.find(line).expect("x");
    view.copy(at, at + line.len());
    for before in ["  z: 3", "  w: 4"] {
        let to = view.text.find(before).expect("the line");
        view.replace(to, to, line);
    }
    for pointer in ["/a/x", "/b/x", "/c/x"] {
        assert_eq!(
            source(&mut view, pointer).as_deref(),
            Some("/a/x"),
            "{pointer}"
        );
    }
}

#[test]
fn an_external_paste_holds_new_items() {
    let mut view = View::new("a:\n  x: 1\nb:\n  z: 3\n");
    let to = view.text.find("  z: 3").expect("z");
    view.replace(to, to, "  x: 1\n");
    assert_eq!(source(&mut view, "/b/x"), None);
}

#[test]
fn a_value_typed_over_keeps_the_items_link() {
    let mut view = View::new("a:\n  x: 1\n  y: 2\n");
    let at = view.text.find("1").expect("1");
    view.replace(at, at + 1, "");
    view.replace(at, at, "5");
    view.replace(at + 1, at + 1, "0");
    assert_eq!(source(&mut view, "/a/x").as_deref(), Some("/a/x"));
    assert!(matches!(view.links, Links::Kept(_)));
}

#[test]
fn lines_moved_by_a_line_command_keep_their_links() {
    let mut view = View::new("l:\n  - a: 1\n  - b: 2\n  - c: 3\n");
    // The last item moved up over the one before it.
    let after = "l:\n  - a: 1\n  - c: 3\n  - b: 2\n";
    let splice = Splice::lines(&view.text, after, &[(3, 1, 2), (2, 1, 3)]).expect("lines");
    view.edit(vec![splice]);
    assert_eq!(view.text, after);
    assert_eq!(source(&mut view, "/l/1").as_deref(), Some("/l/2"));
    assert_eq!(source(&mut view, "/l/2").as_deref(), Some("/l/1"));
}

#[test]
fn lines_reindented_in_place_keep_their_links() {
    let mut view = View::new("a:\n  x: 1\n  s:\n    t: 2\nb:\n  z: 3\n");
    // `s` cut, pasted into `b`'s `z`... at its own indent, then indented.
    let lines = "  s:\n    t: 2\n";
    let at = view.text.find(lines).expect("s");
    view.replace(at, at + lines.len(), "");
    let to = view.text.len();
    view.replace(to, to, lines);
    let at = view.text.find(lines).expect("pasted");
    view.replace(at, at + lines.len(), "    s:\n      t: 2\n");
    view.replace(at - "  z: 3\n".len(), at - 1, "  z:");
    assert_eq!(source(&mut view, "/b/z/s").as_deref(), Some("/a/s"));
    assert_eq!(source(&mut view, "/b/z/s/t").as_deref(), Some("/a/s/t"));
}

#[test]
fn per_line_splices_keep_every_items_link() {
    let before = "a:\n  x: true\n  y: true\nb:\n  z: true\n";
    let after = before.replace("true", "false");
    let mut view = View::new(before);
    // Read before the edit, as a larger edit would.
    view.copy(0, 0);
    view.edit(replacing(before, "true", "false"));
    assert_eq!(view.text, after);
    for pointer in ["/a/x", "/a/y", "/b/z"] {
        assert_eq!(source(&mut view, pointer).as_deref(), Some(pointer));
    }
}

/// A Replace all over many items follows their links with work in
/// proportion to them (QA `SP38-1`).
#[test]
fn many_splices_are_followed_in_one_pass() {
    let work = |count: usize| {
        let before: String = (0..count)
            .map(|at| format!("k{at}:\n  m:\n    l:\n      - h: true\n"))
            .collect();
        let mut view = View::new(&before);
        view.copy(0, 0);
        let splices = replacing(&before, "true", "false");
        let ((), steps) = counted(Work::LinkStep, || view.edit(splices));
        steps
    };
    let (small, large) = (work(100), work(400));
    assert!(
        large <= small * 5,
        "{small} steps at 100 items, {large} at 400"
    );
}

/// An XML view's list starts on its first item's line: both have lines
/// (QA `E38-5`: a copy of the first item had no link), told apart by how
/// far they go; a flow list's items, side by side on one line, have none.
#[cfg(feature = "xml")]
#[test]
fn items_nested_on_one_line_have_lines_and_siblings_do_not() {
    let pointers = |language: Language, text: &str| -> Vec<String> {
        let value = language.behavior().parse(text).expect("reads");
        super::items(language.behavior(), text, &value)
            .into_iter()
            .map(|item| item.pointer)
            .collect()
    };
    assert_eq!(
        pointers(
            Language::XML,
            "<r>\n  <l>a</l>\n  <l>a</l>\n  <l>b</l>\n</r>\n"
        ),
        ["/r", "/r/l", "/r/l/0", "/r/l/1", "/r/l/2"]
    );
    assert_eq!(
        pointers(Language::YAML, "a:\n  l: [1, 2]\n"),
        ["/a", "/a/l"]
    );
}
