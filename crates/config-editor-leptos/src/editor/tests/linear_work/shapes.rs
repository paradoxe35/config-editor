//! The documents the matrix edits, at a size: written as data, then in the
//! host's language by its own writer (so every host holds them in its
//! usual form), with what a writer cannot write (a key written many
//! times) put in by hand.

use serde_json::{Value, json};

use crate::editor::codec;
use crate::editor::languages::Language;
use crate::editor::paths::{TextPosition, TextRange};
use crate::editor::schema::{SchemaBundle, build_schema};

/// What a document is made of, `size` times over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Shape {
    /// `size` items with the same data.
    Same,
    /// `size` items each with its own data (the last one invalid).
    Distinct,
    /// [`Shape::Same`]'s items, each holding a secret placeholder, and in a
    /// host's own language a key holding one written `size` times (a line
    /// copied again and again).
    Secrets,
    /// [`Shape::Distinct`]'s items beside a map of `size` keys, each line
    /// locked by a range of its own (`read_only_ranges`).
    Locks,
    /// One value nested `size` deep under a recursive `anyOf` of the
    /// schema, wrong at every level: only validated.
    Deep,
}

/// The secret placeholder written in [`Shape::Secrets`].
pub(super) const MASK: &str = "«secret 0b1c»";

/// The pattern the host's secret placeholders match.
pub(super) const PLACEHOLDER: &str = "«secret [0-9a-f]{4}»";

/// The data of an item (`index` of a [`Shape::Distinct`] list when
/// `distinct`).
fn item(index: usize, distinct: bool, secret: bool) -> Value {
    let mut item = if distinct {
        json!({"name": format!("a{index}"), "port": index + 1, "host": format!("h{index}"), "mode": format!("x{index}")})
    } else {
        json!({"name": "a", "port": 1, "host": "h", "mode": "x"})
    };
    if secret {
        item["token"] = json!(MASK);
    }
    item
}

/// The data of `shape` at `size`, less what its host's writer cannot
/// write (see [`text`]).
fn data(shape: Shape, size: usize) -> Value {
    let distinct = matches!(shape, Shape::Distinct | Shape::Locks);
    let mut items: Vec<Value> = (0..size)
        .map(|index| item(index, distinct, shape == Shape::Secrets))
        .collect();
    if distinct && let Some(last) = items.last_mut() {
        last["port"] = json!("none");
    }
    match shape {
        Shape::Deep => {
            let mut node = json!({"v": 3});
            for level in 0..size {
                let mark = if level % 3 == 0 {
                    json!("q")
                } else {
                    json!(true)
                };
                // A list where every option wants a lone value.
                let mut next = node;
                next["v"] = json!([1, mark]);
                node = json!({ "c": next });
            }
            json!({"other": 1, "root": node})
        }
        Shape::Locks => {
            let locked: serde_json::Map<String, Value> = (0..size)
                .map(|index| (format!("lock{index}"), json!(index)))
                .collect();
            json!({"other": 1, "locked": locked, "items": items})
        }
        _ => json!({"other": 1, "items": items}),
    }
}

/// The document of `shape` at `size`, as `host` writes it.
pub(super) fn text(host: Language, shape: Shape, size: usize) -> String {
    codec::serialize(host.behavior(), &data(shape, size), None)
        .unwrap_or_else(|error| panic!("{host} writes {shape:?}: {error:?}"))
}

/// [`text`], with the key holding the placeholder written `size` times
/// more (a host's own language only: no view could convert it).
pub(super) fn with_copies(host: Language, text: &str, size: usize) -> String {
    let (head, tail) = match host.label() {
        "JSON" => {
            let end = text.trim_end().strip_suffix('}').expect("a JSON object");
            let copies = format!("    \"key\": \"{MASK}\",\n").repeat(size);
            return format!(
                "{},\n  \"b\": {{\n{copies}    \"x\": 1\n  }}\n}}\n",
                end.trim_end()
            );
        }
        "XML" => {
            let close = text.trim_end().rfind('\n').expect("a root element") + 1;
            (&text[..close], &text[close..])
        }
        _ => (text, ""),
    };
    let block = match host.label() {
        "YAML" => format!("b:\n{}  x: 1\n", format!("  key: {MASK}\n").repeat(size)),
        "TOML" => format!("[b]\n{}x = 1\n", format!("key = \"{MASK}\"\n").repeat(size)),
        "HCL" => format!(
            "b {{\n{}  x = 1\n}}\n",
            format!("  key = \"{MASK}\"\n").repeat(size)
        ),
        _ => format!(
            "  <b>\n{}    <x>1</x>\n  </b>\n",
            format!("    <key>{MASK}</key>\n").repeat(size)
        ),
    };
    format!("{head}{block}{tail}")
}

/// A range for each line of `text` writing a key `lock…`: each locked on
/// its own.
pub(super) fn locks(text: &str) -> Vec<TextRange> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| line.contains("lock"))
        .map(|(index, line)| {
            let end = line.chars().count() + 1;
            TextRange::new(
                TextPosition::new(index + 1, 1),
                TextPosition::new(index + 1, end),
            )
        })
        .collect()
}

/// The schema of every shape: the items an `anyOf` of an object and a
/// text, their mode a `oneOf`, and the nested value a recursive `anyOf`
/// (the schema that once made the error path exponential).
pub(super) fn schema() -> std::sync::Arc<SchemaBundle> {
    let node = json!({"anyOf": [
        {"type": "object", "properties": {
            "v": {"anyOf": [{"type": "integer"}, {"type": "string"}]},
            "c": {"$ref": "#/$defs/node"}}},
        {"type": "object", "properties": {
            "v": {"anyOf": [{"type": "boolean"}, {"type": "string"}]},
            "c": {"$ref": "#/$defs/node"}}},
        {"type": "object", "properties": {
            "v": {"oneOf": [{"type": "number"}, {"const": "n"}]},
            "c": {"anyOf": [{"$ref": "#/$defs/node"}, {"type": "integer"}]}}}
    ]});
    let item = json!({"anyOf": [
        {"type": "object", "properties": {
            "name": {"type": "string", "description": "The item's name."},
            "port": {"type": "integer", "description": "The port."},
            "host": {"type": "string"},
            "mode": {"oneOf": [{"enum": ["x", "y"]}, {"type": "string", "pattern": "^x"}]},
            "token": {"type": "string", "writeOnly": true}},
         "additionalProperties": false},
        {"type": "string"}
    ]});
    let schema = json!({
        "type": "object",
        "$defs": {"node": node, "item": item},
        "properties": {
            "other": {"type": "integer"},
            "items": {"type": "array", "items": {"$ref": "#/$defs/item"}},
            "locked": {"type": "object", "additionalProperties": {"type": "integer"}},
            "root": {"$ref": "#/$defs/node"},
            "b": {"type": "object"}
        }
    });
    std::sync::Arc::new(build_schema(&schema.to_string()).expect("the schema"))
}
