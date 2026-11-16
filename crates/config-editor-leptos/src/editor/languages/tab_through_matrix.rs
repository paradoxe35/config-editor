//! Typing a value at each stop of an accepted block and pressing Tab
//! between them fills each required value in turn, in every language
//! alike: a nested required object's own key (`limits:`) is no stop, so
//! no later value lands one stop early.

use super::super::completion::{apply_completion, completions, next_stop, stop_ranges};
use super::super::schema::build_schema;
use super::Language;
use serde_json::{Value, json};

const LANGUAGES: [Language; 5] = [
    Language::YAML,
    Language::JSON,
    Language::TOML,
    Language::HCL,
    Language::XML,
];

/// Where a new top-level key is typed, per language.
fn root(language: Language) -> &'static str {
    match language.label() {
        "JSON" => "{\n  |\n}\n",
        "XML" => "<|\n",
        _ => "|\n",
    }
}

/// `svc` accepted at the root, then each of `typed` typed at the caret
/// (over the selected value there) with Tab after it, as the editor walks
/// its stops: the slots left empty and the values written. All ASCII.
fn typed_through(language: Language, schema: &str, typed: &[&str]) -> String {
    let bundle = build_schema(schema).unwrap();
    let marked = root(language);
    let at = marked.find('|').unwrap();
    let source = marked.replacen('|', "", 1);
    let options = completions(&source, at, true, Some(&bundle), true, language.behavior());
    let option = options
        .iter()
        .find(|option| option.label == "svc")
        .expect("svc offered");
    let (mut text, mut anchor, mut caret) = apply_completion(&source, option);
    let insert_start = caret - option.cursor_offset;
    let insert_end = insert_start + option.insert.len();
    // Kept from the end, as the editor keeps them.
    let total = text.len();
    let stops: Vec<(usize, usize)> =
        stop_ranges(&option.insert, option.cursor_offset, &option.fills)
            .into_iter()
            .map(|(start, end)| (total - (insert_start + start), total - (insert_start + end)))
            .collect();
    let after_region = total - insert_end;
    for value in typed {
        let start = anchor.min(caret);
        let end = anchor.max(caret);
        text.replace_range(start..end, value);
        caret = start + value.len();
        let length = text.len();
        let region_end = length - after_region;
        let here: Vec<(usize, usize)> = stops
            .iter()
            .filter(|(start, _)| *start <= length)
            .map(|(start, end)| (length - start, length - end))
            .collect();
        match next_stop(&text, caret, region_end, &here, language.behavior()) {
            Some((from, to)) => (anchor, caret) = (from, to),
            None => break,
        }
    }
    text
}

fn read(language: Language, schema: &str, text: &str) -> Option<Value> {
    let bundle = build_schema(schema).unwrap();
    super::super::codec::read(language.behavior(), text, Some(&bundle.schema)).ok()
}

#[test]
fn tab_walks_the_values_of_an_accepted_block_with_a_nested_required_object() {
    let schema = r#"{"type":"object","properties":{"svc":{"type":"object",
        "required":["name","port","limits","mode"],
        "properties":{
            "name":{"type":"string"},
            "port":{"type":"integer"},
            "limits":{"type":"object","required":["cpu","mem"],"properties":{
                "cpu":{"type":"integer","default":2},
                "mem":{"type":"string"}}},
            "mode":{"enum":["a","b"]}
        }}}}"#;
    let wanted =
        json!({"svc": {"name": "web", "port": 80, "limits": {"cpu": 3, "mem": "1G"}, "mode": "b"}});
    let mut failures = Vec::new();
    for language in LANGUAGES {
        let text = typed_through(language, schema, &["web", "80", "3", "1G", "b"]);
        if read(language, schema, &text).as_ref() != Some(&wanted) {
            failures.push(format!("{language}:\n{text}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn tab_walks_the_values_of_an_accepted_block_nested_two_deep() {
    let schema = r#"{"type":"object","properties":{"svc":{"type":"object",
        "required":["name","limits","mode"],
        "properties":{
            "name":{"type":"string"},
            "limits":{"type":"object","required":["io","mem"],"properties":{
                "io":{"type":"object","required":["read","write"],"properties":{
                    "read":{"type":"integer","default":5},
                    "write":{"type":"integer"}}},
                "mem":{"type":"string"}}},
            "mode":{"type":"string"}
        }}}}"#;
    let wanted = json!({"svc": {"name": "web", "limits": {"io": {"read": 6, "write": 7}, "mem": "1G"}, "mode": "b"}});
    let mut failures = Vec::new();
    for language in LANGUAGES {
        let text = typed_through(language, schema, &["web", "6", "7", "1G", "b"]);
        if read(language, schema, &text).as_ref() != Some(&wanted) {
            failures.push(format!("{language}:\n{text}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// A required list with `minItems` is written with its items, and Tab
/// walks each item's value in order, the caret starting in the first
/// item when the list comes first: a list of strings, of numbers, of
/// objects, first, after a string field, or alone.
#[test]
fn tab_walks_the_items_a_required_list_is_written_with() {
    let lists = [
        (
            r#""list":{"type":"array","minItems":2,"items":{"type":"string"}}"#,
            &["T1", "T2"][..],
            json!(["T1", "T2"]),
        ),
        (
            r#""list":{"type":"array","minItems":2,"items":{"type":"integer"}}"#,
            &["7", "8"][..],
            json!([7, 8]),
        ),
        (
            r#""list":{"type":"array","minItems":1,"items":{"type":"object","required":["h"],
                "properties":{"h":{"type":"string"}}}}"#,
            &["H"][..],
            json!([{"h": "H"}]),
        ),
    ];
    let orders: [&[&str]; 3] = [&["list", "name"], &["name", "list"], &["list"]];
    let mut failures = Vec::new();
    for (list, items, wanted_list) in &lists {
        for order in orders {
            let schema = format!(
                r#"{{"type":"object","properties":{{"svc":{{"type":"object","required":{},
                    "properties":{{"name":{{"type":"string"}},{list}}}}}}}}}"#,
                serde_json::to_string(order).unwrap()
            );
            let mut typed = Vec::new();
            let mut wanted = serde_json::Map::new();
            for key in order {
                if *key == "list" {
                    typed.extend_from_slice(items);
                    wanted.insert("list".into(), wanted_list.clone());
                } else {
                    typed.push("N");
                    wanted.insert("name".into(), json!("N"));
                }
            }
            let wanted = json!({ "svc": wanted });
            for language in LANGUAGES {
                let text = typed_through(language, &schema, &typed);
                let read = read(language, &schema, &text);
                if read.as_ref() != Some(&wanted) {
                    failures.push(format!(
                        "{language} {order:?} {items:?}:\n{text}\nreads {read:?}"
                    ));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

/// A list's `minItems` is filled at every depth, not only for a list the
/// block requires itself: a required list inside a list's item, a list of
/// lists, a list inside a required object, a list of objects with two
/// fields. Tab walks every value written, in order, and the block reads
/// back as typed.
#[test]
fn tab_walks_the_items_of_lists_filled_at_every_depth() {
    let cases = [
        (
            r#"{"type":"object","required":["rules","name"],"properties":{"name":{"type":"string"},
                "rules":{"type":"array","minItems":1,"items":{"type":"object","required":["ports","proto"],
                "properties":{"proto":{"type":"string"},
                    "ports":{"type":"array","minItems":2,"items":{"type":"integer"}}}}}}}"#,
            &["80", "443", "tcp", "N"][..],
            json!({"rules": [{"ports": [80, 443], "proto": "tcp"}], "name": "N"}),
        ),
        (
            r#"{"type":"object","required":["grid"],"properties":{"grid":{"type":"array","minItems":1,
                "items":{"type":"array","minItems":2,"items":{"type":"integer"}}}}}"#,
            &["1", "2"][..],
            json!({"grid": [[1, 2]]}),
        ),
        (
            r#"{"type":"object","required":["inner","last"],"properties":{"last":{"type":"string"},
                "inner":{"type":"object","required":["ids"],"properties":{
                    "ids":{"type":"array","minItems":2,"items":{"type":"integer"}}}}}}"#,
            &["5", "6", "L"][..],
            json!({"inner": {"ids": [5, 6]}, "last": "L"}),
        ),
        (
            r#"{"type":"object","required":["hosts","name"],"properties":{"name":{"type":"string"},
                "hosts":{"type":"array","minItems":2,"items":{"type":"object","required":["h","p"],
                "properties":{"h":{"type":"string"},"p":{"type":"integer"}}}}}}"#,
            &["a", "1", "b", "2", "N"][..],
            json!({"hosts": [{"h": "a", "p": 1}, {"h": "b", "p": 2}], "name": "N"}),
        ),
        // QA 36: a list of objects holding a required list of objects of
        // two fields: Tab goes from `from` to `to`, then to `proto`.
        (
            r#"{"type":"object","required":["rules","name"],"properties":{"name":{"type":"string"},
                "rules":{"type":"array","minItems":2,"items":{"type":"object","required":["ports","proto"],
                "properties":{"proto":{"type":"string"},
                    "ports":{"type":"array","minItems":1,"items":{"type":"object","required":["from","to"],
                    "properties":{"from":{"type":"integer"},"to":{"type":"integer"}}}}}}}}}"#,
            &["1", "2", "t", "3", "4", "u", "N"][..],
            json!({"rules": [
                {"ports": [{"from": 1, "to": 2}], "proto": "t"},
                {"ports": [{"from": 3, "to": 4}], "proto": "u"}
            ], "name": "N"}),
        ),
    ];
    let mut failures = Vec::new();
    for (svc, typed, wanted) in &cases {
        let schema = format!(r#"{{"type":"object","properties":{{"svc":{svc}}}}}"#);
        let wanted = json!({ "svc": wanted });
        for language in LANGUAGES {
            // XML writes a list as its key's element repeated, one per
            // item: an item that is a list itself has no element of its
            // own to repeat, so the block leaves such a list out.
            if language == Language::XML && svc.contains("grid") {
                continue;
            }
            let text = typed_through(language, &schema, typed);
            let read = read(language, &schema, &text);
            if read.as_ref() != Some(&wanted) {
                failures.push(format!("{language} {typed:?}:\n{text}\nreads {read:?}"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}
