//! External `$ref`s of a JSON Schema bundled from the host's resources: a
//! reference to another file (`common.json#/$defs/port`) becomes one to the
//! copy of that file under `$defs` (`#/$defs/common.json/$defs/port`), so
//! completion, which follows only `#` pointers, and the validator, which
//! fetches nothing, both find it. A file's own `#` references are rewritten
//! to point into its copy; a file referring to others brings them along. A
//! reference to a file the resources do not have is left as it is: the
//! validator refuses it, saying where it is written.

use std::collections::HashSet;

use serde_json::{Map, Value};

use super::super::language::pointer_key;
use super::super::schema::{SchemaError, each_schema, each_schema_mut};
use super::{SchemaInput, SchemaResources};

/// Where `reference` (a `$ref` written in the file at `base`) points: the
/// file's location and the fragment (`""` for the whole file); `None` for
/// a reference inside the same file.
fn target(base: Option<&str>, reference: &str) -> Option<(String, String)> {
    let (location, fragment) = reference.split_once('#').unwrap_or((reference, ""));
    if location.is_empty() {
        return None;
    }
    Some((join(base, location), fragment.to_string()))
}

/// `location` resolved against `base`'s directory (a URL or a path), as
/// RFC 3986 resolves a relative reference; an absolute one as it is. A
/// path's `.` and `..` segments are resolved without a base too, and a
/// backslash separates its folders as a slash does (Windows-style
/// `schemaLocation`s).
pub(super) fn join(base: Option<&str>, location: &str) -> String {
    if location.contains("://") || location.starts_with('/') || location.starts_with("urn:") {
        return location.to_string();
    }
    let location = location.replace('\\', "/");
    let base = base.map(|base| {
        if base.contains("://") {
            base.to_string()
        } else {
            base.replace('\\', "/")
        }
    });
    let base = base.as_deref().unwrap_or_default();
    let directory = base.rsplit_once('/').map_or("", |(directory, _)| directory);
    // A URL's scheme and host stay; its path joins as a path does.
    let (prefix, path) = match directory.split_once("://") {
        Some((scheme, rest)) => {
            let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
            (format!("{scheme}://{host}"), format!("/{path}"))
        }
        None if base.contains('/') => (String::new(), format!("{directory}/")),
        None => (String::new(), String::new()),
    };
    let absolute = path.starts_with('/') || !prefix.is_empty();
    let mut segments: Vec<&str> = Vec::new();
    for segment in path.split('/').chain(location.split('/')) {
        match segment {
            "" | "." => {}
            // Above a relative path's start, `..` stays.
            ".." if !absolute && segments.last().is_none_or(|last| *last == "..") => {
                segments.push("..");
            }
            ".." => {
                segments.pop();
            }
            other => segments.push(other),
        }
    }
    format!(
        "{prefix}{}{}",
        if absolute { "/" } else { "" },
        segments.join("/")
    )
}

/// The text of the resource at `resolved` (written `written`): looked up
/// as resolved, then as written, each also under a key naming the same
/// file (`./types.xsd`, `types\common.xsd`).
pub(super) fn resource<'a>(
    resources: &'a SchemaResources,
    resolved: &str,
    written: &str,
) -> Result<&'a str, SchemaError> {
    let named = |location: &str| {
        resources.get(location).or_else(|| {
            let wanted = join(None, location);
            resources
                .iter()
                .find(|(key, _)| join(None, key) == wanted)
                .map(|(_, text)| text)
        })
    };
    named(resolved).or_else(|| named(written)).ok_or_else(|| {
        SchemaError(format!(
            "\"{written}\" is not among the schema's resources: pass its text under \"{resolved}\""
        ))
    })
}

/// The locations (as written) `schema` refers to outside itself.
pub(super) fn external_references(schema: &Value) -> Vec<String> {
    let mut found = Vec::new();
    let mut seen = HashSet::new();
    walk_refs(schema, &mut |reference| {
        let location = reference.split('#').next().unwrap_or_default();
        if !location.is_empty() && seen.insert(location.to_string()) {
            found.push(location.to_string());
        }
    });
    found
}

/// Calls `found` with each `$ref` of a schema in `value` (the one walk
/// over a schema's schemas: data is never read as one).
fn walk_refs(value: &Value, found: &mut impl FnMut(&str)) {
    each_schema(value, &(), &mut |_, members, ()| {
        if let Some(reference) = members.get("$ref").and_then(Value::as_str) {
            found(reference);
        }
        Some(())
    });
}

/// `schema` (the text of `input`, read) with the external references the
/// resources hold bundled; `parse` reads a resource's text in the same
/// format.
pub(super) fn bundle(
    mut schema: Value,
    input: &SchemaInput<'_>,
    parse: &dyn Fn(&str) -> Result<Value, SchemaError>,
) -> Result<Value, SchemaError> {
    if input.resources.is_empty() {
        return Ok(schema);
    }
    let mut bundled: Map<String, Value> = Map::new();
    let mut pending: Vec<(String, String)> = Vec::new();
    // Without a location, the schema's `$id` is its base.
    let id = schema
        .get("$id")
        .and_then(Value::as_str)
        .map(str::to_string);
    rewrite(
        &mut schema,
        input.location.or(id.as_deref()),
        None,
        input.resources,
        &mut pending,
    );
    let mut done: HashSet<String> = HashSet::new();
    while let Some((location, written)) = pending.pop() {
        if !done.insert(location.clone()) {
            continue;
        }
        let text = resource(input.resources, &location, &written)?;
        let mut document =
            parse(text).map_err(|error| SchemaError(format!("{written}: {}", error.0)))?;
        // A copy is a part of this schema now, not a resource of its own:
        // its pointers are from this schema's root.
        if let Value::Object(map) = &mut document {
            map.remove("$id");
            map.remove("$schema");
        }
        rewrite(
            &mut document,
            Some(&location),
            Some(&location),
            input.resources,
            &mut pending,
        );
        bundled.insert(location, document);
    }
    if bundled.is_empty() {
        return Ok(schema);
    }
    let Value::Object(map) = &mut schema else {
        return Err(SchemaError(
            "a schema with references is an object".to_string(),
        ));
    };
    let defs = map
        .entry("$defs")
        .or_insert_with(|| Value::Object(Map::new()));
    let Value::Object(defs) = defs else {
        return Err(SchemaError("$defs is not an object".to_string()));
    };
    for (location, document) in bundled {
        defs.insert(location, document);
    }
    Ok(schema)
}

/// The `$defs` key a bundled file is kept under, escaped for a pointer.
fn key(location: &str) -> String {
    pointer_key(location)
}

/// Rewrites the references of `value` (the file at `base`; `inside`: the
/// bundled file it is, `None` for the main one) to pointers into the
/// bundle, and lists the files they need.
fn rewrite(
    value: &mut Value,
    base: Option<&str>,
    inside: Option<&str>,
    resources: &SchemaResources,
    pending: &mut Vec<(String, String)>,
) {
    each_schema_mut(value, &mut |members| {
        let Some(Value::String(reference)) = members.get_mut("$ref") else {
            return;
        };
        let fragment = reference
            .split_once('#')
            .map_or("", |(_, fragment)| fragment);
        if !fragment.is_empty() && !fragment.starts_with('/') {
            // An anchor: the validator finds `$anchor`s wherever they are.
            return;
        }
        let rewritten = match target(base, reference) {
            Some((location, fragment)) => {
                let written = reference.split('#').next().unwrap_or_default();
                if resource(resources, &location, written).is_err() {
                    return;
                }
                pending.push((location.clone(), written.to_string()));
                format!("#/$defs/{}{}", key(&location), fragment)
            }
            // A file's own pointer, into its copy.
            None => match inside {
                Some(location) => format!("#/$defs/{}{}", key(location), fragment),
                None => return,
            },
        };
        *reference = rewritten;
    });
}

#[cfg(test)]
mod tests {
    use super::join;

    #[test]
    fn a_location_resolves_against_its_base() {
        let cases = [
            (None, "a.json", "a.json"),
            (
                Some("https://x.org/s/main.json"),
                "a.json",
                "https://x.org/s/a.json",
            ),
            (
                Some("https://x.org/s/main.json"),
                "../t/a.json",
                "https://x.org/t/a.json",
            ),
            (
                Some("https://x.org/main.json"),
                "./a.json",
                "https://x.org/a.json",
            ),
            (
                Some("schemas/main.json"),
                "types/a.json",
                "schemas/types/a.json",
            ),
            (Some("/abs/main.json"), "a.json", "/abs/a.json"),
            (
                Some("schemas/main.json"),
                "https://y.org/b.json",
                "https://y.org/b.json",
            ),
            (Some("schemas/main.json"), "urn:b", "urn:b"),
            (None, "./a.json", "a.json"),
            (None, "x/../a.json", "a.json"),
            (None, "../a.json", "../a.json"),
            (Some("schemas/main.json"), "../../a.json", "../a.json"),
            (Some("build\\main.xsd"), "core.xsd", "build/core.xsd"),
            (None, "build\\core.xsd", "build/core.xsd"),
        ];
        for (base, location, expected) in cases {
            assert_eq!(join(base, location), expected, "{base:?} {location}");
        }
    }
}
