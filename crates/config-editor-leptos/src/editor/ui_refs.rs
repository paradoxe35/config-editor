//! References by `$id` and `$anchor`, rewritten as `#` JSON pointers into
//! the schema completion and hover read.
//!
//! The validator resolves `#myAnchor`, `https://example.com/root.json#/$defs/y`
//! and a `#/…` pointer inside an embedded resource (one with its own `$id`)
//! against the base URI each one is written under. Completion and hover
//! follow only pointers from the root, so once the validator is built, each
//! reference that names a schema in the same document is rewritten to the
//! root pointer of that schema. References to other documents stay as they
//! are. A `$dynamicRef` or `$recursiveRef` reads as a `$ref` to the schema
//! it names statically (its `$dynamicAnchor`, the root for `#`), and up to
//! draft-04 `id` is `$id`.

use serde_json::Value;
use std::collections::HashMap;

/// Keywords whose value is data, not schemas: left as written.
const DATA: [&str; 5] = ["const", "enum", "default", "examples", "example"];

/// Keywords whose value maps names to schemas.
const NAMED: [&str; 6] = [
    "properties",
    "patternProperties",
    "$defs",
    "definitions",
    "dependentSchemas",
    "dependencies",
];

/// The base URI of a schema without `$id`, as the validator names it.
const DEFAULT_BASE: &str = "json-schema:///";

/// What the walk found: each resource's base URI, each anchor, and each
/// `$ref` with the base it is written under.
#[derive(Default)]
struct Index {
    resources: HashMap<String, String>,
    anchors: HashMap<(String, String), String>,
    /// Each reference: where, its base, what it names, and whether it is
    /// a `$dynamicRef` or `$recursiveRef` (written as a `$ref` beside it).
    references: Vec<(String, String, String, bool)>,
    /// The keyword naming a resource: `$id`, or `id` up to draft-04.
    id: &'static str,
}

/// Rewrites each `$ref` of `schema` that names a schema of the same
/// document to `#` and that schema's pointer from the root.
#[cfg(test)]
pub(super) fn to_pointers(schema: &mut Value) {
    let rewrites = pointer_rewrites(schema);
    rewrite(schema, rewrites);
}

/// What [`to_pointers`] changes: the pointer of each object whose `$ref`
/// it rewrites, and the new reference.
pub(super) fn pointer_rewrites(schema: &Value) -> Vec<(String, String)> {
    let old_id = schema
        .get("$schema")
        .and_then(Value::as_str)
        .is_some_and(|dialect| dialect.contains("draft-04") || dialect.contains("draft-03"));
    let mut index = Index {
        id: if old_id { "id" } else { "$id" },
        ..Index::default()
    };
    let base = schema
        .get(index.id)
        .and_then(Value::as_str)
        .filter(|id| !id.starts_with('#'))
        .map_or_else(
            || DEFAULT_BASE.to_string(),
            |id| without_fragment(&join(DEFAULT_BASE, id)),
        );
    index.resources.insert(base.clone(), String::new());
    walk(schema, String::new(), &base, &mut index);
    let mut rewrites = Vec::new();
    for (site, base, reference, dynamic) in index.references {
        let Some(target) = resolve(&index.resources, &index.anchors, &base, &reference) else {
            continue;
        };
        let rewritten = format!("#{target}");
        if dynamic || rewritten != reference {
            rewrites.push((site, rewritten));
        }
    }
    rewrites
}

/// Applies [`pointer_rewrites`] to `schema`.
pub(super) fn rewrite(schema: &mut Value, rewrites: Vec<(String, String)>) {
    for (site, rewritten) in rewrites {
        if let Some(Value::Object(members)) = schema.pointer_mut(&site) {
            members.insert("$ref".to_string(), Value::String(rewritten));
        }
    }
}

fn walk(value: &Value, pointer: String, base: &str, index: &mut Index) {
    match value {
        Value::Object(members) => {
            let mut base = base.to_string();
            if let Some(id) = members.get(index.id).and_then(Value::as_str) {
                match id.strip_prefix('#') {
                    // Draft-07 names an anchor with a fragment-only `$id`.
                    Some(name) => {
                        index
                            .anchors
                            .insert((base.clone(), name.to_string()), pointer.clone());
                    }
                    None => {
                        let full = join(&base, id);
                        base = without_fragment(&full);
                        index
                            .resources
                            .entry(base.clone())
                            .or_insert(pointer.clone());
                        if let Some((_, name)) = full.split_once('#')
                            && !name.is_empty()
                            && !name.starts_with('/')
                        {
                            index
                                .anchors
                                .insert((base.clone(), name.to_string()), pointer.clone());
                        }
                    }
                }
            }
            for keyword in ["$anchor", "$dynamicAnchor"] {
                if let Some(name) = members.get(keyword).and_then(Value::as_str) {
                    index
                        .anchors
                        .entry((base.clone(), name.to_string()))
                        .or_insert(pointer.clone());
                }
            }
            if let Some(reference) = members.get("$ref").and_then(Value::as_str) {
                index.references.push((
                    pointer.clone(),
                    base.clone(),
                    reference.to_string(),
                    false,
                ));
            } else if let Some(reference) = ["$dynamicRef", "$recursiveRef"]
                .iter()
                .find_map(|keyword| members.get(*keyword).and_then(Value::as_str))
            {
                index
                    .references
                    .push((pointer.clone(), base.clone(), reference.to_string(), true));
            }
            for (key, member) in members {
                let at = format!("{pointer}/{}", escaped(key));
                if DATA.contains(&key.as_str()) {
                    continue;
                }
                if NAMED.contains(&key.as_str())
                    && let Value::Object(named) = member
                {
                    for (name, schema) in named {
                        walk(schema, format!("{at}/{}", escaped(name)), &base, index);
                    }
                } else {
                    walk(member, at, &base, index);
                }
            }
        }
        Value::Array(items) => {
            for (position, item) in items.iter().enumerate() {
                walk(item, format!("{pointer}/{position}"), base, index);
            }
        }
        _ => {}
    }
}

/// The root pointer of the schema `reference` names from `base`.
fn resolve(
    resources: &HashMap<String, String>,
    anchors: &HashMap<(String, String), String>,
    base: &str,
    reference: &str,
) -> Option<String> {
    let full = join(base, reference);
    let (uri, fragment) = full.split_once('#').unwrap_or((&full, ""));
    let resource = resources.get(uri)?;
    if fragment.is_empty() {
        return Some(resource.clone());
    }
    if fragment.starts_with('/') {
        return Some(format!("{resource}{}", percent_decoded(fragment)));
    }
    anchors
        .get(&(uri.to_string(), percent_decoded(fragment)))
        .cloned()
}

/// A JSON pointer token for `key` (`~0`, `~1`).
fn escaped(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

fn without_fragment(uri: &str) -> String {
    uri.split_once('#').map_or(uri, |(uri, _)| uri).to_string()
}

/// `text` with its `%XX` escapes decoded (a URI fragment's pointer).
pub(crate) fn percent_decoded(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let hex = |offset: usize| {
            bytes
                .get(at + offset)
                .and_then(|digit| char::from(*digit).to_digit(16))
        };
        if bytes[at] == b'%'
            && let (Some(high), Some(low)) = (hex(1), hex(2))
        {
            decoded.push((high * 16 + low) as u8);
            at += 3;
        } else {
            decoded.push(bytes[at]);
            at += 1;
        }
    }
    String::from_utf8(decoded).unwrap_or_else(|_| text.to_string())
}

/// `reference` resolved against `base` (RFC 3986, for the shapes schemas
/// use: absolute, network-path, absolute-path, relative-path, fragment).
fn join(base: &str, reference: &str) -> String {
    let base = without_fragment(base);
    let has_scheme = reference.split_once(':').is_some_and(|(scheme, _)| {
        scheme
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_alphabetic())
            && scheme
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || "+-.".contains(character))
    });
    if has_scheme {
        return reference.to_string();
    }
    if reference.is_empty() {
        return base;
    }
    if reference.starts_with('#') {
        return format!("{base}{reference}");
    }
    let scheme_end = base.find(':').map_or(0, |at| at + 1);
    if let Some(network) = reference.strip_prefix("//") {
        return format!("{}//{network}", &base[..scheme_end]);
    }
    // Where the path starts: after `scheme://authority`, or the scheme.
    let path_start = match base[scheme_end..].strip_prefix("//") {
        Some(rest) => scheme_end + 2 + rest.find('/').unwrap_or(rest.len()),
        None => scheme_end,
    };
    let (origin, path) = base.split_at(path_start);
    let merged = if reference.starts_with('/') {
        reference.to_string()
    } else {
        let directory = path.rfind('/').map_or("", |at| &path[..=at]);
        if directory.is_empty() && origin.ends_with(':') && !origin.is_empty() {
            // `urn:…` has no directories: a relative reference stands alone.
            return reference.to_string();
        }
        format!("{directory}{reference}")
    };
    format!("{origin}{}", without_dot_segments(&merged))
}

/// `path` without its `.` and `..` segments.
fn without_dot_segments(path: &str) -> String {
    let (path, rest) = path
        .find(['?', '#'])
        .map_or((path, ""), |at| path.split_at(at));
    let mut kept: Vec<&str> = Vec::new();
    let segments: Vec<&str> = path.split('/').collect();
    for (position, segment) in segments.iter().enumerate() {
        let last = position + 1 == segments.len();
        match *segment {
            "." => {
                if last {
                    kept.push("");
                }
            }
            ".." => {
                if kept.len() > 1 {
                    kept.pop();
                }
                if last {
                    kept.push("");
                }
            }
            other => kept.push(other),
        }
    }
    format!("{}{rest}", kept.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn references_are_joined_to_their_base() {
        let base = "https://example.com/a/b.json";
        assert_eq!(join(base, "c.json"), "https://example.com/a/c.json");
        assert_eq!(join(base, "../c.json#x"), "https://example.com/c.json#x");
        assert_eq!(join(base, "/c.json"), "https://example.com/c.json");
        assert_eq!(join(base, "#/x"), "https://example.com/a/b.json#/x");
        assert_eq!(join(base, "urn:x"), "urn:x");
        assert_eq!(join(DEFAULT_BASE, "#a"), "json-schema:///#a");
        assert_eq!(join(DEFAULT_BASE, "sub.json"), "json-schema:///sub.json");
    }

    #[test]
    fn anchors_ids_and_embedded_pointers_become_root_pointers() {
        let mut schema = json!({
            "$id": "https://example.com/root.json",
            "$defs": {
                "x": {"$anchor": "myAnchor", "type": "integer"},
                "y": {"type": "string"},
                "sub": {
                    "$id": "sub.json",
                    "$defs": {"inner": {"type": "boolean"}},
                    "properties": {"p": {"$ref": "#/$defs/inner"}, "q": {"$ref": "#"}}
                },
                "legacy": {"$id": "#legacy", "type": "null"}
            },
            "properties": {
                "a": {"$ref": "#myAnchor"},
                "b": {"$ref": "https://example.com/root.json#/$defs/y"},
                "c": {"$ref": "sub.json"},
                "d": {"$ref": "#legacy"},
                "e": {"$ref": "#/$defs/y"},
                "f": {"$ref": "https://elsewhere.example/other.json"},
                "g": {"$ref": "#/$defs/a%20b"}
            },
            "default": {"$ref": "#myAnchor"}
        });
        to_pointers(&mut schema);
        let reference = |pointer: &str| schema.pointer(pointer).cloned().unwrap();
        assert_eq!(reference("/properties/a/$ref"), "#/$defs/x");
        assert_eq!(reference("/properties/b/$ref"), "#/$defs/y");
        assert_eq!(reference("/properties/c/$ref"), "#/$defs/sub");
        assert_eq!(reference("/properties/d/$ref"), "#/$defs/legacy");
        assert_eq!(reference("/properties/e/$ref"), "#/$defs/y");
        assert_eq!(
            reference("/properties/f/$ref"),
            "https://elsewhere.example/other.json"
        );
        assert_eq!(reference("/properties/g/$ref"), "#/$defs/a b");
        assert_eq!(
            reference("/$defs/sub/properties/p/$ref"),
            "#/$defs/sub/$defs/inner"
        );
        assert_eq!(reference("/$defs/sub/properties/q/$ref"), "#/$defs/sub");
        assert_eq!(reference("/default/$ref"), "#myAnchor", "data stays");
    }

    #[test]
    fn dynamic_and_recursive_references_and_draft_four_ids_become_root_pointers() {
        let mut dynamic = json!({
            "$id": "https://example.com/tree",
            "$dynamicAnchor": "node",
            "properties": {"children": {"items": {"$dynamicRef": "#node"}}}
        });
        to_pointers(&mut dynamic);
        assert_eq!(
            dynamic.pointer("/properties/children/items/$ref"),
            Some(&json!("#"))
        );
        let mut recursive = json!({
            "$recursiveAnchor": true,
            "$defs": {"leaf": {"properties": {"kids": {"items": {"$recursiveRef": "#"}}}}}
        });
        to_pointers(&mut recursive);
        assert_eq!(
            recursive.pointer("/$defs/leaf/properties/kids/items/$ref"),
            Some(&json!("#"))
        );
        let mut draft_four = json!({
            "$schema": "http://json-schema.org/draft-04/schema#",
            "definitions": {"a": {"id": "#foo", "type": "object"}},
            "properties": {"a": {"$ref": "#foo"}, "id": {"type": "string"}}
        });
        to_pointers(&mut draft_four);
        assert_eq!(
            draft_four.pointer("/properties/a/$ref"),
            Some(&json!("#/definitions/a"))
        );
        // From draft-06 on, `id` names nothing.
        let mut draft_six = draft_four.clone();
        draft_six["$schema"] = json!("http://json-schema.org/draft-06/schema#");
        draft_six["properties"]["a"]["$ref"] = json!("#foo");
        to_pointers(&mut draft_six);
        assert_eq!(
            draft_six.pointer("/properties/a/$ref"),
            Some(&json!("#foo"))
        );
    }
}
