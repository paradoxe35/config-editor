//! Tab stops of an accepted block: the values it wrote for required
//! children (defaults, examples, placeholders), which Tab selects in turn
//! along with the empty values it left.

use super::*;
use crate::editor::text::byte_range;

/// The first `examples` entry (or `example`) of a property schema, when its
/// types allow it; a numeric string example is a number where only numbers
/// are allowed.
pub fn schema_example(schema: &Value, root: &Value) -> Option<Value> {
    let here = Here::of(schema, root);
    let example = here
        .branches()
        .into_iter()
        .find_map(|branch| match branch.get("examples") {
            Some(Value::Array(examples)) => examples.first(),
            Some(example) => Some(example),
            None => branch.get("example"),
        })?;
    let types = here.types();
    let numeric_only = types.allows_type("number") && !types.allows_type("string");
    let example = numeric_example_as_number(example.clone(), numeric_only);
    types.allows(&example).then_some(example)
}

/// `(key, value text)` of each scalar a block insert for `property` writes
/// for its required children, depth first in schema order: what
/// [`required_children`] has the language write, as found later by
/// [`stop_ranges`]. The child the caret goes to is written empty, and so
/// is none.
#[cfg(test)]
pub(crate) fn required_fills(
    behavior: &dyn ConfigLanguage,
    property: &Value,
    root: &Value,
) -> Vec<(String, String)> {
    fills_of(&required_children(behavior, property, root))
}

/// [`required_fills`] of `children` found already.
pub(crate) fn fills_of(children: &[RequiredChild<'_>]) -> Vec<(String, String)> {
    let mut fills = Vec::new();
    collect_fills(children, &mut fills);
    fills
}

fn collect_fills(children: &[RequiredChild<'_>], fills: &mut Vec<(String, String)>) {
    for child in children {
        match &child.fill {
            Fill::Children(nested) => collect_fills(nested, fills),
            Fill::Value(value) | Fill::CaretAt(value) | Fill::Empty(Some(value)) => {
                collect_values(child.key, value, fills);
            }
            list @ Fill::List { .. } => {
                collect_values(child.key, &list.value().unwrap_or_default(), fills);
            }
            Fill::Caret { .. } | Fill::Empty(None) => {}
        }
    }
}

/// The scalars of `value` written for `key`: itself, an object's members
/// under their own keys, or a list's items in order, each under the
/// list's key.
fn collect_values(key: &str, value: &Value, fills: &mut Vec<(String, String)>) {
    match value {
        Value::Object(members) => {
            for (key, member) in members {
                collect_values(key, member, fills);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_values(key, item, fills);
            }
        }
        Value::Null => {}
        Value::String(text) if text.is_empty() => {}
        value => fills.push((key.to_string(), value_label(value))),
    }
}

/// Byte ranges in `insert`, after `cursor_offset`, of the `fills` it
/// wrote: each value is looked for after its key on the key's line (the
/// first such line from where the fill before it ended), in order; the
/// next item of a list (a fill under the key of the fill before) first
/// right after the item before it, on its line, as a list written on one
/// line has its key once, and the first fill at the caret (the caret
/// moved into a list's first item) or also on the caret's line (its key
/// before the caret). A fill not written that way (left empty)
/// is skipped.
pub(crate) fn stop_ranges(
    insert: &str,
    cursor_offset: TextSize,
    fills: &[(String, String)],
) -> Vec<TextRange> {
    let mut from = usize::from(cursor_offset).min(insert.len());
    let mut ranges = Vec::new();
    let line_end = |at: usize| insert[at..].find('\n').map_or(insert.len(), |end| at + end);
    let after_key = |from: usize, key: &str, text: &str| {
        let mut search = from;
        loop {
            let key_end = find_word(insert, search, key)? + key.len();
            if let Some(start) = find_word(&insert[..line_end(key_end)], key_end, text) {
                return Some(start);
            }
            search = key_end;
        }
    };
    // The value the caret is at, or selected before it.
    let at_caret = |from: usize, text: &str| {
        let starts =
            |start: usize| (find_word(insert, start, text) == Some(start)).then_some(start);
        starts(from).or_else(|| {
            from.checked_sub(text.len())
                .filter(|start| insert.is_char_boundary(*start))
                .and_then(starts)
        })
    };
    let mut previous: Option<&str> = None;
    for (key, text) in fills {
        let on_line = || find_word(&insert[..line_end(from)], from, text);
        let start = match previous {
            Some(before) if before == key => on_line().or_else(|| after_key(from, key, text)),
            Some(_) => after_key(from, key, text),
            // The first: the value the caret is at (a list's first item,
            // the caret moved into it), not the same key's in the next
            // item; else after its key, which may sit before the caret.
            None => at_caret(from, text)
                .or_else(|| after_key(from, key, text))
                .or_else(on_line),
        };
        previous = Some(key);
        let Some(start) = start else {
            continue;
        };
        let end = start + text.len();
        ranges.push(byte_range(start..end));
        from = end;
    }
    ranges
}

/// The first byte index at or after `from` where `word` stands on its own
/// (no letter, digit, `_` or `-` right before or after it).
fn find_word(text: &str, from: usize, word: &str) -> Option<usize> {
    if word.is_empty() {
        return None;
    }
    let part_of_word =
        |character: char| character.is_alphanumeric() || matches!(character, '_' | '-');
    let mut search = from;
    while let Some(offset) = text.get(search..)?.find(word) {
        let at = search + offset;
        let before = text[..at].chars().next_back();
        let after = text[at + word.len()..].chars().next();
        if !before.is_some_and(part_of_word) && !after.is_some_and(part_of_word) {
            return Some(at);
        }
        search = at + word.len();
    }
    None
}

/// Where Tab goes next in an accepted block, as `(anchor, caret)` bytes:
/// the nearest empty value slot after byte `caret` (an empty selection),
/// or written value (selected, so typing replaces it), before byte
/// `region_end`. `stops` are byte ranges of `source`.
pub(crate) fn next_stop(
    source: &str,
    caret: TextSize,
    region_end: TextSize,
    stops: &[TextRange],
    behavior: &dyn ConfigLanguage,
) -> Option<(TextSize, TextSize)> {
    let slot = next_slot(source, caret, region_end, behavior).map(|slot| (slot, slot));
    let stop = stops
        .iter()
        .map(|stop| (stop.start(), stop.end()))
        .filter(|(start, end)| *start >= caret && *end <= region_end)
        .min();
    match (slot, stop) {
        (Some(slot), Some(stop)) => Some(if stop.0 < slot.0 { stop } else { slot }),
        (slot, stop) => slot.or(stop),
    }
}

/// `(cursor_offset, selected)` (bytes of `insert`) for a block whose caret
/// goes to a list written with items ([`caret_at_list`]): in its first
/// item's value, as Tab would select it (its empty slot, or the value
/// written selected).
pub(crate) fn caret_in_list(
    insert: &str,
    cursor_offset: TextSize,
    fills: &[(String, String)],
    behavior: &dyn ConfigLanguage,
) -> Option<(TextSize, TextSize)> {
    let end = TextSize::of(insert);
    let stops = stop_ranges(insert, cursor_offset, fills);
    let (anchor, caret) = next_stop(insert, cursor_offset, end, &stops, behavior)?;
    Some((caret, caret - anchor))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::text::text_size;
    use serde_json::json;

    #[test]
    fn examples_fill_in_only_where_the_types_allow_them() {
        let root = json!({});
        assert_eq!(
            schema_example(&json!({"type": "integer", "examples": ["8080"]}), &root),
            Some(json!(8080))
        );
        assert_eq!(
            schema_example(&json!({"type": "string", "examples": ["8080"]}), &root),
            Some(json!("8080"))
        );
        assert_eq!(
            schema_example(&json!({"type": "boolean", "examples": ["yes"]}), &root),
            None
        );
        assert_eq!(
            schema_example(&json!({"example": 3}), &root),
            Some(json!(3))
        );
    }

    #[test]
    fn written_values_are_found_after_their_keys_in_every_shape() {
        let fills = vec![
            ("port".to_string(), "8080".to_string()),
            ("scheme".to_string(), "https".to_string()),
        ];
        let inserts = [
            "server:\n  host: \n  port: 8080\n  scheme: https\n",
            "\"server\": {\n  \"host\": \"\",\n  \"port\": 8080,\n  \"scheme\": \"https\"\n}",
            "[server]\nhost = \"\"\nport = 8080\nscheme = \"https\"\n",
            "server {\n  host = \"\"\n  port = 8080\n  scheme = \"https\"\n}",
            "<server>\n  <host></host>\n  <port>8080</port>\n  <scheme>https</scheme>\n</server>",
        ];
        for insert in inserts {
            let caret = insert.find("host").unwrap() + 4;
            let ranges = stop_ranges(insert, text_size(caret), &fills);
            let texts: Vec<&str> = ranges.iter().map(|range| &insert[*range]).collect();
            assert_eq!(texts, ["8080", "https"], "{insert:?}");
        }
        // the caret moved into a list's first item is at the first
        // fill: that one, not the same key's in the next item.
        let fill = |key: &str| (key.to_string(), "0".to_string());
        let list_fills = [fill("from"), fill("to"), fill("from"), fill("to")];
        for insert in [
            "rules: [{ports: [{from: 0, to: 0}], proto: \"\"}, {ports: [{from: 0, to: 0}], proto: \"\"}]",
            "\"rules\": [{\"ports\": [{\"from\": 0, \"to\": 0}]}, {\"ports\": [{\"from\": 0, \"to\": 0}]}]",
        ] {
            let zero = insert.find('0').unwrap();
            // The caret before the value, or after it with it selected.
            for caret in [zero, zero + 1] {
                let ranges = stop_ranges(insert, text_size(caret), &list_fills);
                let starts: Vec<usize> = ranges.iter().map(|range| range.start().into()).collect();
                let zeros: Vec<usize> = insert.match_indices('0').map(|(at, _)| at).collect();
                assert_eq!(starts, zeros, "{insert} {caret}");
            }
        }
        // A key never written, or a value left empty, is skipped.
        assert_eq!(stop_ranges("server:\n  port: \n", 0.into(), &fills), vec![]);
    }

    #[test]
    fn fills_follow_the_required_children_in_order() {
        let root = json!({});
        let property = json!({
            "type": "object",
            "required": ["host", "port", "tls", "scheme"],
            "properties": {
                "host": {"type": "string"},
                "port": {"type": "integer", "examples": [8080]},
                "tls": {"type": "object", "required": ["verify"], "properties": {"verify": {"type": "boolean"}}},
                "scheme": {"type": "string", "default": "https"}
            }
        });
        assert_eq!(
            required_fills(&Plain, &property, &root),
            vec![
                ("port".to_string(), "8080".to_string()),
                ("verify".to_string(), "false".to_string()),
                ("scheme".to_string(), "https".to_string()),
            ]
        );
        // A required name is the property's own unless the language
        // spells it otherwise: `@port` names no property here.
        let mut spelled = property.clone();
        spelled["required"] = json!(["@port"]);
        assert_eq!(required_fills(&Plain, &spelled, &root), vec![]);
    }

    /// A language that spells required properties as they are.
    struct Plain;

    impl crate::editor::language::ConfigLanguage for Plain {
        fn info(&self) -> crate::editor::language::LanguageInfo {
            crate::editor::language::LanguageInfo {
                label: "Plain",
                icon: "P",
                extensions: &[],
            }
        }
        fn parse(&self, _source: &str) -> Result<Value, crate::editor::language::ParseError> {
            Ok(json!({}))
        }
    }
}
