//! The markup XML keeps in a start tag and no view shows (`xsi:type`,
//! `xsi:nil`, namespace declarations) belongs to its element: a list
//! edited in another view (items removed, added, copied, moved, all
//! changed) saves each item's markup with that item's data, an item added
//! gets none but its siblings' namespace, and none moves to a neighbour
//! (QA `E31-1`: deleting the second of three `<shape xsi:type=…>` gave the
//! third the second's type).

use crate::editor::Language;
use crate::editor::save_matrix::{report, save_value};
use serde_json::{Value, json};

const XSI: &str = "xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"";
const VIEWS: [Language; 4] = [
    Language::YAML,
    Language::JSON,
    Language::TOML,
    Language::HCL,
];

/// What each item's start tag holds besides its data.
#[derive(Clone, Copy, Debug)]
enum Kind {
    Type,
    /// Only the second item, `xsi:nil` and empty.
    Nil,
    DefaultNamespace,
    /// A declaration on the item for its prefixed attribute `p:k`, the
    /// same text on every item.
    PrefixDeclaration,
}

impl Kind {
    fn markup(self, item: usize) -> Vec<String> {
        match self {
            Kind::Type => vec![format!("xsi:type=\"T{item}\"")],
            Kind::Nil if item == 1 => vec!["xsi:nil=\"true\"".to_string()],
            Kind::Nil => Vec::new(),
            Kind::DefaultNamespace => vec![format!("xmlns=\"urn:d{item}\"")],
            Kind::PrefixDeclaration => vec![format!("xmlns:p=\"urn:p{item}\"")],
        }
    }

    fn empty(self, item: usize) -> bool {
        matches!(self, Kind::Nil) && item == 1
    }
}

/// An item of the edited list: an item of the host's (changed or not),
/// a copy of one, or a new one.
#[derive(Clone, Copy, Debug)]
enum Slot {
    Kept(usize),
    Changed(usize),
    Copy(usize),
    Fresh,
}

const OPERATIONS: [(&str, &[Slot]); 11] = {
    use Slot::*;
    [
        ("delete first", &[Kept(1), Kept(2)]),
        ("delete middle", &[Kept(0), Kept(2)]),
        ("delete last", &[Kept(0), Kept(1)]),
        ("insert first", &[Fresh, Kept(0), Kept(1), Kept(2)]),
        ("insert middle", &[Kept(0), Fresh, Kept(1), Kept(2)]),
        ("insert last", &[Kept(0), Kept(1), Kept(2), Fresh]),
        ("duplicate", &[Kept(0), Kept(1), Copy(1), Kept(2)]),
        ("move up", &[Kept(1), Kept(0), Kept(2)]),
        ("move down", &[Kept(0), Kept(2), Kept(1)]),
        ("replace all", &[Changed(0), Changed(1), Changed(2)]),
        ("delete middle, insert last", &[Kept(0), Kept(2), Fresh]),
    ]
};

fn host(kind: Kind, attributes: bool) -> String {
    let mut text = format!("<root {XSI}>\n");
    for item in 0..3 {
        let mut tag = String::from("<item");
        if attributes {
            tag.push_str(&format!(" id=\"{item}\""));
        }
        for markup in kind.markup(item) {
            tag.push(' ');
            tag.push_str(&markup);
        }
        if matches!(kind, Kind::PrefixDeclaration) {
            tag.push_str(" p:k=\"x\"");
        }
        if kind.empty(item) {
            text.push_str(&format!("  {tag}/>\n"));
        } else {
            text.push_str(&format!("  {tag}>\n    <v>{item}</v>\n  </item>\n"));
        }
    }
    text.push_str("</root>\n");
    text
}

fn fresh(attributes: bool) -> Value {
    if attributes {
        json!({"id": 9, "v": 9})
    } else {
        json!({"v": 9})
    }
}

/// The markup written in each `<item` start tag of `saved`, sorted.
fn item_markup(saved: &str) -> Vec<Vec<String>> {
    let mut tags = Vec::new();
    let mut rest = saved;
    while let Some(at) = rest.find("<item") {
        let tag_end = at + rest[at..].find('>').expect("a closed tag");
        let tag = &rest[at + "<item".len()..tag_end];
        let mut markup: Vec<String> = tag
            .split_whitespace()
            .map(|attribute| attribute.trim_end_matches('/').to_string())
            .filter(|attribute| attribute.starts_with("xmlns") || attribute.starts_with("xsi:"))
            .collect();
        markup.sort();
        tags.push(markup);
        rest = &rest[tag_end..];
    }
    tags
}

#[test]
fn list_edits_in_other_views_keep_each_items_markup_with_its_data() {
    let mut failures = Vec::new();
    for kind in [
        Kind::Type,
        Kind::Nil,
        Kind::DefaultNamespace,
        Kind::PrefixDeclaration,
    ] {
        for attributes in [false, true] {
            let host = host(kind, attributes);
            for (label, slots) in OPERATIONS {
                let expected: Vec<Vec<String>> = slots
                    .iter()
                    .enumerate()
                    .map(|(at, slot)| match slot {
                        Slot::Kept(item) => kind.markup(*item),
                        // Given content, an item is no longer nil.
                        Slot::Changed(item) if kind.empty(*item) => Vec::new(),
                        Slot::Changed(item) => kind.markup(*item),
                        // An item added is named in its siblings'
                        // namespace: the one it copies, else the one at its
                        // place (QA `E32-1`); it takes no other markup.
                        Slot::Copy(item) if matches!(kind, Kind::DefaultNamespace) => {
                            kind.markup(*item)
                        }
                        Slot::Fresh if matches!(kind, Kind::DefaultNamespace) => {
                            kind.markup(at.min(2))
                        }
                        Slot::Copy(_) | Slot::Fresh => Vec::new(),
                    })
                    .collect();
                for edit in VIEWS {
                    let case = format!("{kind:?}, attributes {attributes}, {label}, via {edit}");
                    let saved = save_value(Language::XML, edit, &host, &|value| {
                        let items = value["root"]["item"]
                            .as_array()
                            .expect("three items")
                            .clone();
                        let edited: Vec<Value> = slots
                            .iter()
                            .map(|slot| match slot {
                                Slot::Kept(item) | Slot::Copy(item) => items[*item].clone(),
                                Slot::Changed(item) => {
                                    let mut changed = items[*item].clone();
                                    if let Some(map) = changed.as_object_mut() {
                                        map.insert("v".into(), json!(10 + item));
                                    }
                                    changed
                                }
                                Slot::Fresh => fresh(attributes),
                            })
                            .collect();
                        value["root"]["item"] = Value::Array(edited);
                    });
                    // A copy of an item holding a prefix its own start tag
                    // declares: no declaration binds it there, and which
                    // namespace the copy means is the user's to say.
                    let unbound = matches!(kind, Kind::PrefixDeclaration)
                        && slots.iter().any(|slot| matches!(slot, Slot::Copy(_)));
                    let saved = match saved {
                        Ok(saved) if !unbound => saved,
                        Err(error)
                            if unbound
                                && error
                                    .contains("the prefix p of p:k is bound to no namespace") =>
                        {
                            continue;
                        }
                        other => {
                            failures.push(format!("{case}: {other:?}"));
                            continue;
                        }
                    };
                    let markup = item_markup(&saved);
                    if markup != expected {
                        failures.push(format!(
                            "{case}: markup {markup:?}, wanted {expected:?}:\n{saved}"
                        ));
                    }
                }
            }
        }
    }
    report(failures);
}

/// One edit removing an item and changing a neighbour (three items pasted
/// over with two): the save writes and checks each item as the same host
/// item, so it saves (QA `E32-4`: it waited, saying the value would be
/// written in another form, as its stages read the changed item as one
/// removed and one added).
#[test]
fn an_item_removed_beside_one_changed_saves_with_the_markup_kept() {
    let mut failures = Vec::new();
    let host = host(Kind::Type, false);
    for edit in VIEWS {
        let saved = save_value(Language::XML, edit, &host, &|value| {
            value["root"]["item"] = json!([{"v": 1}, {"v": 7}]);
        });
        match saved {
            Ok(saved) if item_markup(&saved) == [Kind::Type.markup(1), Kind::Type.markup(2)] => {}
            other => failures.push(format!("via {edit}: {other:?}")),
        }
    }
    let targets = format!(
        "<nlog {XSI}>\n  <targets>\n    <target xsi:type=\"File\" name=\"logfile\" fileName=\"app.log\" />\n    <target xsi:type=\"Console\" name=\"console\" />\n    <target xsi:type=\"Null\" name=\"blackhole\" />\n  </targets>\n</nlog>\n"
    );
    for edit in VIEWS {
        let saved = save_value(Language::XML, edit, &targets, &|value| {
            value["nlog"]["targets"]["target"] =
                json!([{"name": "logfile", "fileName": "app.log"}, {"name": "bh"}]);
        });
        if let Err(error) = saved {
            failures.push(format!("targets via {edit}: {error}"));
        }
    }
    report(failures);
}
