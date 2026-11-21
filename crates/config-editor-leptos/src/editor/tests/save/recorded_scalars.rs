//! P2 in a view that writes every scalar as text (XML): which values the
//! user wrote is read off the editor's record of the view's edits (its
//! links), never guessed by comparing the view's values with the ones it
//! showed. An element pasted from a copy keeps the host type of what it
//! copies; a text that came with no record waits where that decides the
//! save.

use crate::editor::Language;
use crate::editor::bridge::Bridge;
use crate::editor::bridge::links::{Clipboard, Inserted, Links, Piece};
use crate::editor::edit_as::SaveError;
use crate::editor::text::{TextRange, text_size};

const HOST: &str = "{\"l\": [8, 9]}\n";

/// The view of `HOST` with its first item's line written again before the
/// second's, as `source` says (typed, or pasted from a copy of that line),
/// and the save of it as the editor makes it, told by the links the edit
/// kept.
fn first_item_written_again(source: Inserted) -> (Bridge, String, Result<String, SaveError>) {
    let xml = Language::XML.behavior();
    let bridge = Bridge::new(Language::JSON, Language::XML, None).unwrap();
    let view = bridge.to_edit(HOST).unwrap().text;
    let line = |item: &str| {
        let at = view.find(item).expect("the item");
        let start = view[..at].rfind('\n').map_or(0, |at| at + 1);
        (start, at + view[at..].find('\n').unwrap() + 1)
    };
    let (first, first_end) = line("<l>8</l>");
    let (second, _) = line("<l>9</l>");
    let written = view[first..first_end].to_string();
    let mut links = Links::fresh(HOST, &view, Some(bridge.reading()));
    let mut clipboard = Clipboard::default();
    if matches!(source, Inserted::Pasted) {
        let copied = links.copied(
            (xml, &view),
            TextRange::new(text_size(first), text_size(first_end)),
        );
        clipboard.copied(&written, copied);
    }
    let piece = Piece {
        range: TextRange::new(text_size(second), text_size(second)),
        inserted: written.clone(),
        source,
    };
    let next = format!("{}{written}{}", &view[..second], &view[second..]);
    links.edited(xml, &view, &[piece], &mut clipboard);
    let identity = Language::JSON.behavior().items_hold_unshown(HOST);
    let told = links.told(xml, &next, HOST, identity);
    let untouched = links.untouched(xml, &next);
    let saved = bridge.to_host_linked(&next, HOST, HOST, told.as_ref(), untouched.as_deref());
    (bridge, next, saved)
}

fn read(saved: Result<String, SaveError>) -> serde_json::Value {
    let saved = saved.expect("saved");
    serde_json::from_str(&saved).expect(&saved)
}

#[test]
fn an_element_pasted_from_a_copy_keeps_the_host_type_of_what_it_copies() {
    let (_, _, saved) = first_item_written_again(Inserted::Pasted);
    assert_eq!(read(saved), serde_json::json!({"l": [8, 8, 9]}));
}

#[test]
fn a_text_with_no_record_waits_where_its_items_types_are_not_known() {
    // The same text given alone: nothing tells a copy from a value typed.
    let (bridge, next, _) = first_item_written_again(Inserted::Typed);
    let error = bridge.to_host_since(&next, HOST, HOST).unwrap_err();
    assert!(
        format!("{error:?}").contains("no record"),
        "the wait says why: {error:?}"
    );
}

/// The strings of `value` in order, nested lists read through.
fn leaves(value: &serde_json::Value, out: &mut Vec<serde_json::Value>) {
    match value {
        serde_json::Value::Array(items) => items.iter().for_each(|item| leaves(item, out)),
        leaf => out.push(leaf.clone()),
    }
}

/// An XML view shows nested lists flat, their leaves in order: each
/// untouched leaf stands for the host's leaf it shows, never for the host
/// item at its own index (a list, or another leaf). A switch writing the
/// unsaved view into another one keeps every leaf the user left alone.
#[test]
fn a_switch_from_an_xml_view_of_nested_lists_keeps_each_untouched_leaf() {
    let xml = Language::XML.behavior();
    for host in [
        "{\"m\": [[\"a\", \"b\"], [[\"e\"], \"f\"], \"x\"], \"n\": \"1\"}\n",
        "{\"m\": [[\"a\", \"b\"]], \"n\": \"1\"}\n",
        "{\"m\": [\"a\", [\"b\", \"e\"], [\"f\"]], \"n\": \"1\"}\n",
        "{\"m\": [[\"1\", 2], [\"3\"]], \"n\": \"1\"}\n",
    ] {
        let bridge = Bridge::new(Language::JSON, Language::XML, None).unwrap();
        let view = bridge.to_edit(host).unwrap().text;
        let at = view.find("<n>1</n>").expect("n") + "<n>".len();
        let piece = Piece {
            range: TextRange::new(text_size(at), text_size(at + 1)),
            inserted: "2".to_string(),
            source: Inserted::Typed,
        };
        let next = format!("{}2{}", &view[..at], &view[at + 1..]);
        let mut links = Links::fresh(host, &view, Some(bridge.reading()));
        links.edited(xml, &view, &[piece], &mut Clipboard::default());
        let untouched = links.untouched(xml, &next);
        let shown = bridge
            .to_other_view(&next, Language::JSON, untouched.as_deref())
            .unwrap()
            .text;
        let shown: serde_json::Value = serde_json::from_str(&shown).expect(&shown);
        let held: serde_json::Value = serde_json::from_str(host).unwrap();
        let mut expected = Vec::new();
        leaves(&held["m"], &mut expected);
        assert_eq!(
            shown["m"],
            serde_json::Value::Array(expected),
            "{host} shown as\n{view}"
        );
    }
}
