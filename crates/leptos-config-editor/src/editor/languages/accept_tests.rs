//! The accept invariant: in every built-in language, accepting any schema
//! property at any path, then typing a value at the caret, leaves a document
//! that still parses. Documents are built by each language's own serializer
//! around a marker entry, and the property is accepted on a fresh line next to
//! it, so top-level, nested, map-entry, and array-item containers are covered.

use super::super::completion::{apply_completion, completions};
use super::super::diagnostics::build_schema;
use super::super::language::SchemaPathSegment;
use super::Language;
use serde_json::{Map, Value, json};

/// A real config schema, and one covering every nesting shape (arrays of
/// arrays, matrices of objects, objects in arrays in objects).
const SCHEMAS: [&str; 2] = [
    include_str!("../../../tests/fixtures/burrow.schema.json"),
    include_str!("../../../tests/fixtures/shapes.schema.json"),
];
const MARKER: &str = "zz_marker";

/// Resolves `$ref`, `anyOf`, `oneOf`, and `allOf` to the branches with
/// properties (enough to walk the burrow schema's containers).
fn object_branches<'a>(schema: &'a Value, root: &'a Value, out: &mut Vec<&'a Value>, depth: usize) {
    if depth > 8 {
        return;
    }
    if let Some(target) = schema
        .get("$ref")
        .and_then(Value::as_str)
        .and_then(|reference| root.pointer(reference.trim_start_matches('#')))
    {
        object_branches(target, root, out, depth + 1);
    }
    if schema.get("properties").is_some()
        || schema
            .get("additionalProperties")
            .is_some_and(Value::is_object)
    {
        out.push(schema);
    }
    for keyword in ["anyOf", "oneOf", "allOf"] {
        for option in schema
            .get(keyword)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            object_branches(option, root, out, depth + 1);
        }
    }
}

fn array_items<'a>(schema: &'a Value, root: &'a Value, depth: usize) -> Option<&'a Value> {
    if depth > 8 {
        return None;
    }
    if let Some(items) = schema.get("items") {
        return Some(items);
    }
    if let Some(target) = schema
        .get("$ref")
        .and_then(Value::as_str)
        .and_then(|reference| root.pointer(reference.trim_start_matches('#')))
        && let Some(items) = array_items(target, root, depth + 1)
    {
        return Some(items);
    }
    ["anyOf", "oneOf"].iter().find_map(|keyword| {
        schema
            .get(keyword)
            .and_then(Value::as_array)?
            .iter()
            .find_map(|option| array_items(option, root, depth + 1))
    })
}

/// Every container path (with its object schemas) up to `max_depth` levels.
fn containers(root: &Value, max_depth: usize) -> Vec<(Vec<SchemaPathSegment>, &Value)> {
    let mut found = Vec::new();
    let mut queue = vec![(Vec::new(), root)];
    while let Some((path, schema)) = queue.pop() {
        // An array of arrays: descend to its items.
        if path.len() < max_depth + 2
            && let Some(items) = array_items(schema, root, 0)
                .filter(|_| matches!(path.last(), Some(SchemaPathSegment::ArrayItem(_))))
        {
            let mut item = path.clone();
            item.push(SchemaPathSegment::ArrayItem(0));
            queue.push((item, items));
        }
        let mut branches = Vec::new();
        object_branches(schema, root, &mut branches, 0);
        if branches.is_empty() {
            continue;
        }
        found.push((path.clone(), schema));
        if path.len() >= max_depth {
            continue;
        }
        for branch in branches {
            for (key, property) in branch
                .get("properties")
                .and_then(Value::as_object)
                .into_iter()
                .flatten()
            {
                let mut child = path.clone();
                child.push(SchemaPathSegment::Key(key.clone()));
                queue.push((child.clone(), property));
                if let Some(items) = array_items(property, root, 0) {
                    let mut item = child;
                    item.push(SchemaPathSegment::ArrayItem(0));
                    queue.push((item, items));
                }
            }
            if let Some(additional) = branch
                .get("additionalProperties")
                .filter(|value| value.is_object())
            {
                let mut child = path.clone();
                child.push(SchemaPathSegment::Key("sample".to_string()));
                queue.push((child, additional));
            }
        }
    }
    found
}

/// A document whose container at `path` holds only the marker entry; arrays
/// hold two items so XML reads them as arrays.
fn document_value(path: &[SchemaPathSegment]) -> Value {
    let mut value = json!({ MARKER: 1 });
    for (index, segment) in path.iter().enumerate().rev() {
        value = match segment {
            SchemaPathSegment::Key(key) => {
                let mut map = Map::new();
                map.insert(key.clone(), value);
                Value::Object(map)
            }
            SchemaPathSegment::ArrayItem(_) => {
                let _ = index;
                Value::Array(vec![value, json!({ MARKER: 2 })])
            }
        };
    }
    value
}

/// Properties of the container schemas, by name.
fn property_names(schema: &Value, root: &Value) -> Vec<String> {
    let mut branches = Vec::new();
    object_branches(schema, root, &mut branches, 0);
    let mut names: Vec<String> = branches
        .iter()
        .flat_map(|branch| {
            branch
                .get("properties")
                .and_then(Value::as_object)
                .into_iter()
                .flatten()
        })
        .map(|(key, _)| key.clone())
        .collect();
    names.sort();
    names.dedup();
    names
}

/// The text typed at the caret after accepting: a value where the caret waits
/// for one (after a separator, in empty quotes or brackets, between XML
/// tags), nothing where the insert is already complete.
fn fill(text: &str, caret: usize) -> &'static str {
    let before = text[..caret].trim_end_matches([' ', '\t']);
    let after = &text[caret..];
    let waits = before.ends_with([':', '=', '['])
        || (before.ends_with('"') && after.starts_with('"'))
        || (before.ends_with('>') && after.starts_with("</"));
    if waits { "1" } else { "" }
}

#[test]
fn accepting_any_property_anywhere_keeps_the_document_valid() {
    for (schema, minimum) in SCHEMAS.into_iter().zip([300, 60]) {
        check_accepts(schema, minimum);
    }
}

fn check_accepts(schema_text: &str, minimum: usize) {
    let bundle = build_schema(schema_text).unwrap();
    let root = &bundle.schema;
    let mut failures = Vec::new();
    let mut checked = 0;
    for &language in Language::ALL {
        for (path, schema) in containers(root, 3) {
            // Skip shapes the language cannot write (XML names starting
            // with a digit, HCL top-level keys that are not identifiers).
            let base = match language.serialize(&document_value(&path)) {
                Ok(text) if language.parse(&text).is_ok() => text,
                _ => continue,
            };
            // A fresh line before the marker entry, at its indentation.
            let marker = base.find(MARKER).expect("marker in the document");
            let line_start = base[..marker].rfind('\n').map_or(0, |index| index + 1);
            let indent: String = base[line_start..]
                .chars()
                .take_while(|c| *c == ' ')
                .collect();
            let mut source = base.clone();
            source.insert_str(line_start, &format!("{indent}\n"));
            let caret_byte = line_start + indent.len();
            let caret = source[..caret_byte].encode_utf16().count();
            let options = completions(
                &source,
                caret,
                true,
                Some(&bundle),
                true,
                language.behavior(),
            );
            for name in property_names(schema, root) {
                let Some(option) = options.iter().find(|option| option.label == name) else {
                    continue;
                };
                checked += 1;
                let (updated, new_caret) = apply_completion(&source, option);
                let caret_at = updated
                    .char_indices()
                    .map(|(index, _)| index)
                    .chain([updated.len()])
                    .find(|index| updated[..*index].encode_utf16().count() == new_caret)
                    .unwrap();
                let mut filled = updated.clone();
                filled.insert_str(caret_at, fill(&updated, caret_at));
                // The entry lands on the caret's own line: nothing above it moves.
                let above = &source[..line_start + indent.len()];
                if !updated.starts_with(above)
                    || !updated[above.len()..]
                        .split('\n')
                        .next()
                        .unwrap_or("")
                        .contains(name.as_str())
                {
                    failures.push(format!("{language} {path:?} `{name}`: not inserted on the caret's line\n---\n{updated}---"));
                }
                if let Err(error) = language.parse(&filled) {
                    failures.push(format!(
                        "{language} {path:?} `{name}`: {error}\n---\n{filled}---"
                    ));
                }
            }
        }
    }
    assert!(checked > minimum, "only {checked} accepts checked");
    assert!(
        failures.is_empty(),
        "{} of {checked} accepts left an invalid document:\n\n{}",
        failures.len(),
        failures
            .iter()
            .take(60)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n\n")
    );
}

/// Accepts `option` and types a value at the caret, returning the result.
fn accept_and_fill(source: &str, option: &super::super::completion::Completion) -> String {
    let (updated, new_caret) = apply_completion(source, option);
    let caret_at = updated
        .char_indices()
        .map(|(index, _)| index)
        .chain([updated.len()])
        .find(|index| updated[..*index].encode_utf16().count() == new_caret)
        .unwrap();
    let mut filled = updated.clone();
    filled.insert_str(caret_at, fill(&updated, caret_at));
    filled
}

#[test]
fn a_key_typed_directly_in_an_array_of_objects_starts_a_valid_item() {
    for (schema, minimum) in SCHEMAS.into_iter().zip([40, 8]) {
        check_array_accepts(schema, minimum);
    }
}

fn check_array_accepts(schema_text: &str, minimum: usize) {
    let bundle = build_schema(schema_text).unwrap();
    let root = &bundle.schema;
    let mut failures = Vec::new();
    let mut checked = 0;
    // XML has no array syntax; repeated elements are covered above.
    for &language in Language::ALL
        .iter()
        .filter(|language| **language != Language::XML)
    {
        for (path, schema) in containers(root, 3) {
            let mut branches = Vec::new();
            object_branches(schema, root, &mut branches, 0);
            for branch in &branches {
                for (key, property) in branch
                    .get("properties")
                    .and_then(Value::as_object)
                    .into_iter()
                    .flatten()
                {
                    let Some(items) = array_items(property, root, 0) else {
                        continue;
                    };
                    let mut item_branches = Vec::new();
                    object_branches(items, root, &mut item_branches, 0);
                    if item_branches.is_empty() {
                        continue;
                    }
                    let mut value = document_value(&path);
                    let container = path
                        .iter()
                        .fold(&mut value, |value, segment| match segment {
                            SchemaPathSegment::Key(key) => &mut value[key.as_str()],
                            SchemaPathSegment::ArrayItem(index) => &mut value[*index],
                        });
                    container[key.as_str()] = json!([]);
                    let Ok(base) = language.serialize(&value) else {
                        continue;
                    };
                    if language.parse(&base).is_err() {
                        continue;
                    }
                    let Some(at) = base.find(&key.to_string()).and_then(|start| {
                        base[start..].find("[]").map(|offset| start + offset + 1)
                    }) else {
                        continue;
                    };
                    let line_start = base[..at].rfind('\n').map_or(0, |index| index + 1);
                    let indent: String = base[line_start..]
                        .chars()
                        .take_while(|c| *c == ' ')
                        .collect();
                    let multi_line =
                        format!("{}\n{indent}  \n{indent}{}", &base[..at], &base[at..]);
                    let multi_caret = at + 1 + indent.len() + 2;
                    for (source, caret_byte) in [(base.clone(), at), (multi_line, multi_caret)] {
                        let caret = source[..caret_byte].encode_utf16().count();
                        let options = completions(
                            &source,
                            caret,
                            true,
                            Some(&bundle),
                            true,
                            language.behavior(),
                        );
                        for name in property_names(items, root) {
                            let Some(option) = options.iter().find(|option| option.label == name)
                            else {
                                failures.push(format!("{language} {path:?} `{key}[]`: `{name}` not offered\n---\n{source}---"));
                                continue;
                            };
                            checked += 1;
                            let filled = accept_and_fill(&source, option);
                            if let Err(error) = language.parse(&filled) {
                                failures.push(format!("{language} {path:?} `{key}[]` `{name}`: {error}\n---\n{filled}---"));
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(checked > minimum, "only {checked} accepts checked");
    assert!(
        failures.is_empty(),
        "{} of {checked} array accepts failed:\n\n{}",
        failures.len(),
        failures
            .iter()
            .take(80)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n\n")
    );
}

#[test]
fn the_shapes_schema_reaches_every_nesting_shape() {
    let bundle = build_schema(SCHEMAS[1]).unwrap();
    let paths: Vec<Vec<SchemaPathSegment>> = containers(&bundle.schema, 3)
        .into_iter()
        .map(|(path, _)| path)
        .collect();
    let key = |name: &str| SchemaPathSegment::Key(name.to_string());
    let matrix = vec![
        key("6_deeply_nested_combinations"),
        key("matrix_of_objects"),
        SchemaPathSegment::ArrayItem(0),
        SchemaPathSegment::ArrayItem(0),
    ];
    assert!(
        paths.contains(&matrix),
        "matrix of objects not reached: {paths:?}"
    );
    assert!(paths.contains(&vec![
        key("4_array_of_objects"),
        SchemaPathSegment::ArrayItem(0)
    ]));
}
