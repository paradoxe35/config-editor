//! XML attributes and child elements sharing a name (`<r a="1"><a>2</a>`,
//! read as `@a` and `a`): an `@` key is an attribute through every save
//! from another view, whichever of the two is removed, renamed, added or
//! moved, and never becomes an element.

use super::Language;
use super::save_matrix::{from_every_view, report, save_value};
use serde_json::{Map, Value, json};

type Change = fn(&mut Value);

fn element(value: &mut Value) -> &mut Map<String, Value> {
    value["r"].as_object_mut().unwrap()
}

fn renamed(value: &mut Value, from: &str, to: &str) {
    let object = element(value);
    *object = std::mem::take(object)
        .into_iter()
        .map(|(key, held)| (if key == from { to.to_string() } else { key }, held))
        .collect();
}

#[test]
fn an_attribute_beside_a_child_of_its_name_stays_an_attribute() {
    let mut failures = Vec::new();
    let host = "<r a=\"1\">\n  <a>2</a> <!-- c -->\n  <z>0</z> <!-- z -->\n</r>\n";
    let cases: [(&str, Change, &str); 7] = [
        (
            "the child removed",
            |value| {
                element(value).shift_remove("a");
            },
            "<r a=\"1\">\n  <z>0</z> <!-- z -->\n</r>\n",
        ),
        (
            "the child renamed",
            |value| renamed(value, "a", "b"),
            "<r a=\"1\">\n  <b>2</b> <!-- c -->\n  <z>0</z> <!-- z -->\n</r>\n",
        ),
        (
            "the attribute removed",
            |value| {
                element(value).shift_remove("@a");
            },
            "<r>\n  <a>2</a> <!-- c -->\n  <z>0</z> <!-- z -->\n</r>\n",
        ),
        (
            "the attribute edited",
            |value| value["r"]["@a"] = json!(5),
            "<r a=\"5\">\n  <a>2</a> <!-- c -->\n  <z>0</z> <!-- z -->\n</r>\n",
        ),
        (
            "the attribute moved after the children",
            |value| {
                let object = element(value);
                let held = object.shift_remove("@a").unwrap();
                object.insert("@a".into(), held);
            },
            host,
        ),
        (
            "an attribute added",
            |value| {
                element(value).insert("@q".into(), json!(5));
            },
            "<r a=\"1\" q=\"5\">\n  <a>2</a> <!-- c -->\n  <z>0</z> <!-- z -->\n</r>\n",
        ),
        (
            "the child removed and an attribute added",
            |value| {
                element(value).shift_remove("a");
                element(value).insert("@q".into(), json!(5));
            },
            "<r a=\"1\" q=\"5\">\n  <z>0</z> <!-- z -->\n</r>\n",
        ),
    ];
    for (label, change, expected) in cases {
        from_every_view(&mut failures, label, Language::XML, host, &change, expected);
    }
    report(failures);
}

#[test]
fn an_attribute_added_beside_a_child_of_its_name_is_an_attribute() {
    let mut failures = Vec::new();
    let host = "<r>\n  <b>2</b> <!-- b -->\n</r>\n";
    let cases: [(&str, Change, &str); 2] = [
        (
            "beside a child of its name",
            |value| {
                value["r"]
                    .as_object_mut()
                    .unwrap()
                    .insert("@b".into(), json!(3));
            },
            "<r b=\"3\">\n  <b>2</b> <!-- b -->\n</r>\n",
        ),
        (
            "of another name",
            |value| {
                value["r"]
                    .as_object_mut()
                    .unwrap()
                    .insert("@q".into(), json!(3));
            },
            "<r q=\"3\">\n  <b>2</b> <!-- b -->\n</r>\n",
        ),
    ];
    // An HCL view shows `r` as a block, whose attributes are identifiers.
    for (label, change, expected) in cases {
        for view in [Language::YAML, Language::JSON, Language::TOML] {
            match save_value(Language::XML, view, host, &change) {
                Ok(saved) if saved == expected => {}
                Ok(saved) => failures.push(format!("{label} via {view}:\n{saved}")),
                Err(error) => failures.push(format!("{label} via {view}: {error}")),
            }
        }
    }
    report(failures);
}

/// One edit of the element's entries, by its key in the view.
#[derive(Clone, Copy, Debug)]
enum Op {
    Remove(&'static str),
    Rename(&'static str),
    Add(&'static str),
    MoveLast(&'static str),
    Edit(&'static str),
}

fn apply(value: &mut Value, op: Op) -> Option<()> {
    let object = element(value);
    match op {
        Op::Remove(key) => {
            object.shift_remove(key)?;
        }
        Op::Rename(key) => {
            object.get(key)?;
            let new = if key.starts_with('@') { "@n" } else { "n" };
            if object.contains_key(new) || object.contains_key("@n") || object.contains_key("n") {
                return None;
            }
            renamed(value, key, new);
        }
        Op::Add(key) => {
            if object.contains_key(key) || object.contains_key(&key[1..]) && key != "@c" {
                return None;
            }
            object.insert(key.into(), json!("7"));
        }
        Op::MoveLast(key) => {
            let held = object.shift_remove(key)?;
            object.insert(key.into(), held);
        }
        Op::Edit(key) => *object.get_mut(key)? = json!("9"),
    }
    Some(())
}

/// Every pair of edits of attributes and children sharing names, from
/// every view: each attribute of the edit is written as one, each child as
/// an element, and the comments of the children kept stay.
#[test]
fn every_edit_of_attributes_and_children_sharing_names_keeps_them_apart() {
    let host = "<r a=\"1\" b=\"2\">\n  <a>3</a> <!-- ca -->\n  <c>4</c> <!-- cc -->\n</r>\n";
    let keys = ["@a", "b", "a", "c"];
    let mut ops = vec![Op::Add("@q"), Op::Add("@c")];
    for key in keys {
        ops.extend([
            Op::Remove(key),
            Op::Rename(key),
            Op::MoveLast(key),
            Op::Edit(key),
        ]);
    }
    let mut failures = Vec::new();
    let mut tried = 0;
    for first in &ops {
        for second in &ops {
            // A plain key the host writes as an attribute: `b`, and `n`
            // when `b` was renamed.
            let renames_b = [first, second]
                .iter()
                .any(|op| matches!(op, Op::Rename("b")));
            for view in [
                Language::YAML,
                Language::JSON,
                Language::TOML,
                Language::HCL,
            ] {
                let edited = std::cell::RefCell::new(None);
                let saved = save_value(Language::XML, view, host, &|value| {
                    let mut copy = value.clone();
                    if apply(&mut copy, *first)
                        .and_then(|()| apply(&mut copy, *second))
                        .is_some()
                    {
                        *value = copy;
                        *edited.borrow_mut() = Some(value.clone());
                    }
                });
                let (Some(mut edited), Ok(saved)) = (edited.into_inner(), saved) else {
                    continue;
                };
                tried += 1;
                let start_tag = saved.lines().next().unwrap_or_default();
                let case = format!("{first:?} then {second:?} via {view}");
                for key in element(&mut edited).keys() {
                    let attribute = match key.strip_prefix('@') {
                        Some(name) => Some(name),
                        None if key == "b" || (key == "n" && renames_b) => Some(key.as_str()),
                        None => None,
                    };
                    let written = match attribute {
                        Some(name) => start_tag.contains(&format!(" {name}=\"")),
                        None => saved.contains(&format!("<{key}>")),
                    };
                    if !written {
                        failures.push(format!("{case}: {key}\n{saved}"));
                    }
                }
                for (child, comment) in [("a", "<!-- ca -->"), ("c", "<!-- cc -->")] {
                    if saved.contains(&format!("<{child}>")) && !saved.contains(comment) {
                        failures.push(format!("{case}: {comment} lost\n{saved}"));
                    }
                }
            }
        }
    }
    assert!(tried > 400, "{tried} saves tried");
    report(failures);
}

/// L24-1: the caret on an attribute's name is on the attribute's path,
/// `@a` where a child has its name.
#[test]
fn the_caret_on_an_attribute_is_on_its_path() {
    let text = "<r a=\"1\" b=\"2\">\n  <a>3</a>\n</r>\n";
    let xml = Language::XML.behavior();
    let at = |needle: &str, offset: usize| {
        let cursor = text.find(needle).unwrap() + offset;
        super::paths::path_at(xml, text, cursor).map(|path| path.to_string())
    };
    let found = [
        at(" a=", 1),
        at(" a=", 2),
        at(" b=", 1),
        at("<a>", 1),
        at("<r", 1),
    ];
    let expected = ["/r/@a", "/r/@a", "/r/b", "/r/a", "/r"].map(|path| Some(path.to_string()));
    assert_eq!(found, expected);
}

/// L24-3: a field whose value runs over lines (XML element text, a YAML
/// block scalar, an HCL heredoc) is ranged from its key through the end of
/// its value, an XML element from its `<` through its end tag.
#[test]
fn a_value_over_lines_is_ranged_through_its_last_line() {
    let cases = [
        (
            Language::XML,
            "<r>\n  <a>one\ntwo</a>\n</r>\n",
            "/r/a",
            (2, 3, 3, 8),
        ),
        (
            Language::XML,
            "<r>\n  <a>one</a>\n</r>\n",
            "/r/a",
            (2, 3, 2, 13),
        ),
        (
            Language::YAML,
            "a: |\n  one\n  two\nb: 1\n",
            "/a",
            (1, 1, 3, 6),
        ),
        (
            Language::HCL,
            "a = <<EOT\none\nEOT\nb = 1\n",
            "/a",
            (1, 1, 3, 4),
        ),
    ];
    let mut failures = Vec::new();
    for (language, text, pointer, expected) in cases {
        let found = super::paths::field_range(language.behavior(), text, pointer).map(|range| {
            (
                range.start.line,
                range.start.column,
                range.end.line,
                range.end.column,
            )
        });
        if found != Some(expected) {
            failures.push(format!(
                "{language} {text:?}: {found:?}, expected {expected:?}"
            ));
        }
    }
    report(failures);
}

/// L24-2: an element added beside prefixed siblings (`<k:a>`, read `a`)
/// goes where the view puts it, as beside unprefixed ones.
#[test]
fn an_element_added_beside_prefixed_siblings_keeps_the_views_order() {
    let mut failures = Vec::new();
    let host = "<r xmlns:k=\"urn:k\">\n  <k:a>1</k:a> <!-- a -->\n  <k:c>3</k:c>\n</r>\n";
    let cases: [(&str, Change, &str); 2] = [
        (
            "after a prefixed one",
            |value| {
                let object = element(value);
                let rest: Vec<(String, Value)> = std::mem::take(object).into_iter().collect();
                for (key, held) in rest {
                    let after = key == "a";
                    object.insert(key, held);
                    if after {
                        object.insert("b".into(), json!(2));
                    }
                }
            },
            "<r xmlns:k=\"urn:k\">\n  <k:a>1</k:a> <!-- a -->\n  <b>2</b>\n  <k:c>3</k:c>\n</r>\n",
        ),
        (
            "at the end",
            |value| {
                element(value).insert("d".into(), json!(4));
            },
            "<r xmlns:k=\"urn:k\">\n  <k:a>1</k:a> <!-- a -->\n  <k:c>3</k:c>\n  <d>4</d>\n</r>\n",
        ),
    ];
    for (label, change, expected) in cases {
        from_every_view(&mut failures, label, Language::XML, host, &change, expected);
    }
    report(failures);
}
