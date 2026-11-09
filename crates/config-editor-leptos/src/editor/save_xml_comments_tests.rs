//! XML list items added and removed from every other view, in each way a
//! host spells its elements (`<add k="1"/>`, `<add k="1" />`, single
//! quotes, `<add …></add>`, tabs): only the item's line changes, a new item
//! is spelled as its siblings are, and every comment and untouched line
//! stays byte for byte.

use super::Language;
use super::save_matrix::{from_every_view, report};
use serde_json::{Value, json};

/// How a host spells an element with an attribute and no content.
struct Spelling {
    name: &'static str,
    indent: &'static str,
    element: fn(&str, &str, &str) -> String,
}

const SPELLINGS: [Spelling; 7] = [
    Spelling {
        name: "/>",
        indent: "  ",
        element: |name, attribute, value| format!("<{name} {attribute}=\"{value}\"/>"),
    },
    Spelling {
        name: "space />",
        indent: "  ",
        element: |name, attribute, value| format!("<{name} {attribute}=\"{value}\" />"),
    },
    Spelling {
        name: "single quotes",
        indent: "  ",
        element: |name, attribute, value| format!("<{name} {attribute}='{value}'/>"),
    },
    Spelling {
        name: "single quotes, space />",
        indent: "  ",
        element: |name, attribute, value| format!("<{name} {attribute}='{value}' />"),
    },
    Spelling {
        name: "end tag",
        indent: "  ",
        element: |name, attribute, value| format!("<{name} {attribute}=\"{value}\"></{name}>"),
    },
    Spelling {
        name: "tabs, space />",
        indent: "\t",
        element: |name, attribute, value| format!("<{name} {attribute}=\"{value}\" />"),
    },
    Spelling {
        name: "four spaces, space />",
        indent: "    ",
        element: |name, attribute, value| format!("<{name} {attribute}=\"{value}\" />"),
    },
];

/// The host: a comment line, two items (the second with a trailing
/// comment), and another element with a trailing comment; `items` are the
/// `k` values written.
fn host(spelling: &Spelling, items: &[&str]) -> String {
    let i = spelling.indent;
    let mut text = format!("<?xml version=\"1.0\"?>\n<r>\n{i}<!-- settings -->\n");
    for item in items {
        let comment = if *item == "2" { " <!-- two -->" } else { "" };
        text.push_str(&format!(
            "{i}{}{comment}\n",
            (spelling.element)("add", "k", item)
        ));
    }
    text.push_str(&format!(
        "{i}{} <!-- keep -->\n</r>\n",
        (spelling.element)("o", "x", "1")
    ));
    text
}

/// The `add` items, as the view shows them (a list, or a lone item).
fn items(document: &mut Value) -> &mut Vec<Value> {
    let slot = document.pointer_mut("/r/add").expect("the items");
    if !slot.is_array() {
        *slot = Value::Array(vec![slot.take()]);
    }
    slot.as_array_mut().unwrap()
}

/// `item` with its `k` (however the view spells and types it) set to `k`.
fn with_k(item: &Value, k: u64) -> Value {
    let mut item = item.clone();
    for (key, value) in item.as_object_mut().expect("an item map") {
        if key.trim_start_matches('@') == "k" {
            *value = if value.is_number() {
                json!(k)
            } else {
                json!(k.to_string())
            };
        }
    }
    item
}

#[test]
fn xml_items_added_or_removed_keep_comments_and_the_hosts_spelling() {
    let mut failures = Vec::new();
    for spelling in &SPELLINGS {
        let original = host(spelling, &["1", "2"]);
        from_every_view(
            &mut failures,
            &format!("{}: add", spelling.name),
            Language::XML,
            &original,
            &|document| {
                let items = items(document);
                let third = with_k(&items[1], 3);
                items.push(third);
            },
            &host(spelling, &["1", "2", "3"]),
        );
        from_every_view(
            &mut failures,
            &format!("{}: remove", spelling.name),
            Language::XML,
            &original,
            &|document| {
                items(document).pop();
            },
            &host(spelling, &["1"]),
        );
        // An attribute added to an item: that tag alone changes, spelled
        // as the host spells it.
        let one = (spelling.element)("add", "k", "1");
        let quote = if one.contains('\'') { '\'' } else { '"' };
        let two = one.replacen(
            &format!("k={quote}1{quote}"),
            &format!("k={quote}1{quote} v={quote}x{quote}"),
            1,
        );
        from_every_view(
            &mut failures,
            &format!("{}: attribute added", spelling.name),
            Language::XML,
            &original,
            &|document| {
                items(document)[0]
                    .as_object_mut()
                    .unwrap()
                    .insert("@v".to_string(), json!("x"));
            },
            &original.replacen(&one, &two, 1),
        );
    }
    report(failures);
}

/// L5: a key that is no XML name is refused as one, however it starts
/// (`@/*`, tsconfig's `paths`, is no attribute `/*`), whatever it holds.
#[test]
fn a_key_that_is_no_xml_name_is_refused_as_one() {
    let mut failures = Vec::new();
    for key in ["/*", "@/*", "@a b", "1x", "@1x", "@"] {
        for value in [json!(["./src/*"]), json!("x"), json!({"y": 1})] {
            let document = json!({"compilerOptions": {"paths": {key: value}}});
            match super::codec::serialize(Language::XML.behavior(), &document, None) {
                Err(error) if error.to_string().contains("is not an XML name") => {}
                other => failures.push(format!("{key:?} = {value}: {other:?}")),
            }
        }
    }
    report(failures);
}

/// A .NET web.config: an `<add … />` appended to appSettings is written
/// as its siblings are, and both comments stay.
#[test]
fn a_web_config_setting_added_in_a_view_is_written_in_place() {
    let web = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<!-- Production settings -->\n<configuration>\n  <appSettings>\n    <add key=\"Environment\" value=\"Production\" />\n    <add key=\"MaxRetries\" value=\"5\" />\n  </appSettings>\n  <system.web>\n    <!-- keep debug off in production -->\n    <compilation debug=\"false\" targetFramework=\"4.8\" />\n  </system.web>\n</configuration>\n";
    let mut failures = Vec::new();
    from_every_view(
        &mut failures,
        "web.config: add",
        Language::XML,
        web,
        &|document| {
            let items = document
                .pointer_mut("/configuration/appSettings/add")
                .and_then(Value::as_array_mut)
                .expect("the settings");
            let mut item = items[0].clone();
            for (key, value) in item.as_object_mut().unwrap() {
                *value = json!(if key.ends_with("key") { "X" } else { "y" });
            }
            items.push(item);
        },
        &web.replace(
            "value=\"5\" />\n",
            "value=\"5\" />\n    <add key=\"X\" value=\"y\" />\n",
        ),
    );
    report(failures);
}
