//! JSON pointers read as RFC 6901 reads them: each segment verbatim (no
//! trimming, `~1` then `~0` decoded), and only `0` or `[1-9][0-9]*` an
//! array index, resolved where the value is an array. Every consumer of a
//! path (`range_of`, `path_at`, schema lookups) in every language.

use serde_json::json;

use super::super::Language;
use super::super::language::SchemaPathSegment::{ArrayItem, Key};
use super::FieldPath;

#[track_caller]
fn report(failures: Vec<String>) {
    assert!(
        failures.is_empty(),
        "{} failing case(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn pointer_segments_are_verbatim_and_only_canonical_digits_index() {
    let cases: &[(&str, Vec<super::super::language::SchemaPathSegment>)] = &[
        ("/ a", vec![Key(" a".into())]),
        ("/a ", vec![Key("a ".into())]),
        (" /a", vec![Key(" ".into()), Key("a".into())]),
        ("/01", vec![Key("01".into())]),
        ("/+1", vec![Key("+1".into())]),
        ("/-1", vec![Key("-1".into())]),
        ("/1 ", vec![Key("1 ".into())]),
        ("/a~1b", vec![Key("a/b".into())]),
        ("/m~0n", vec![Key("m~n".into())]),
        // `~01` is `~1` decoded from `~0` once: the key `~1`, not `/`.
        ("/~01", vec![Key("~1".into())]),
        ("/0", vec![ArrayItem(0)]),
        ("/10", vec![ArrayItem(10)]),
        (
            "/list/10/x",
            vec![Key("list".into()), ArrayItem(10), Key("x".into())],
        ),
        // Too large for an index: no array has it, a key may.
        (
            "/99999999999999999999999",
            vec![Key("99999999999999999999999".into())],
        ),
    ];
    let mut failures = Vec::new();
    for (pointer, expected) in cases {
        let path = FieldPath::from_pointer(pointer);
        if path.segments() != expected.as_slice() {
            failures.push(format!(
                "{pointer:?}: expected {expected:?}, found {:?}",
                path.segments()
            ));
        }
        // Read back as written (but for the missing leading `/`).
        if pointer.starts_with('/') && path.to_pointer() != *pointer {
            failures.push(format!("{pointer:?} reads back as {:?}", path.to_pointer()));
        }
    }
    report(failures);
}

const YAML: &str = r#"m:
  " a": sp
  "01": zero-one
  "1": one
  a/b: slash
  m~n: tilde
list: [v0, v1, v2, v3, v4, v5, v6, v7, v8, v9, v10]
"#;

const JSON: &str = r#"{
  "m": {
    " a": "sp",
    "01": "zero-one",
    "1": "one",
    "a/b": "slash",
    "m~n": "tilde"
  },
  "list": ["v0", "v1", "v2", "v3", "v4", "v5", "v6", "v7", "v8", "v9", "v10"]
}
"#;

const TOML: &str = r#"list = ["v0", "v1", "v2", "v3", "v4", "v5", "v6", "v7", "v8", "v9", "v10"]

[m]
" a" = "sp"
"01" = "zero-one"
"1" = "one"
"a/b" = "slash"
"m~n" = "tilde"
"#;

const HCL: &str = r#"list = ["v0", "v1", "v2", "v3", "v4", "v5", "v6", "v7", "v8", "v9", "v10"]
m = {
  " a" = "sp"
  "01" = "zero-one"
  "1" = "one"
  "a/b" = "slash"
  "m~n" = "tilde"
}
"#;

/// XML names cannot start with a digit or hold a space, `/` or `~`: only
/// the list.
const XML: &str = "<list>v0</list>
<list>v1</list>
<list>v2</list>
<list>v3</list>
<list>v4</list>
<list>v5</list>
<list>v6</list>
<list>v7</list>
<list>v8</list>
<list>v9</list>
<list>v10</list>
";

/// Each pointer, and the value its range holds (`None`: no such field).
const FIELDS: &[(&str, Option<&str>)] = &[
    ("/m/ a", Some("sp")),
    ("/m/01", Some("zero-one")),
    ("/m/1", Some("one")),
    ("/m/a~1b", Some("slash")),
    ("/m/m~0n", Some("tilde")),
    ("/m/ 1", None),
    ("/m/+1", None),
    ("/list/0", Some("v0")),
    ("/list/10", Some("v10")),
    ("/list/01", None),
    ("/list/+1", None),
    ("/list/ 1", None),
];

#[test]
fn range_of_resolves_index_like_keys_and_indices_in_every_language() {
    let documents = [
        (Language::YAML, YAML),
        (Language::JSON, JSON),
        (Language::TOML, TOML),
        (Language::HCL, HCL),
        (Language::XML, XML),
    ];
    let mut failures = Vec::new();
    for (language, text) in documents {
        for (pointer, value) in FIELDS {
            if language == Language::XML && pointer.starts_with("/m") {
                continue;
            }
            let found = language
                .range_of(text, *pointer)
                .map(|range| range.slice(text));
            let ok = match (value, &found) {
                (None, None) => true,
                (Some(value), Some(found)) => {
                    let unquoted = |text: &str| text.replace('"', "");
                    // The field's own value, and not a longer one (`v10`
                    // for `v1`).
                    unquoted(found).ends_with(value)
                        && !unquoted(found).ends_with(&format!("0{value}"))
                        || unquoted(found).contains(&format!(">{value}<"))
                }
                _ => false,
            };
            if !ok {
                failures.push(format!(
                    "{language} {pointer}: expected {value:?}, found {found:?}"
                ));
            }
            // And the caret on that field names it again.
            if let Some(range) = language.range_of(text, *pointer)
                && value.is_some()
            {
                let back = language.path_at(text, range.start);
                if back.as_ref().map(FieldPath::to_pointer).as_deref() != Some(*pointer) {
                    failures.push(format!(
                        "{language} {pointer}: path_at its start gives {back:?}"
                    ));
                }
            }
        }
    }
    report(failures);
}

#[cfg(feature = "schema-validation")]
#[test]
fn schema_lookups_resolve_index_like_keys_and_indices() {
    use super::super::completion::docs_url_at;
    let schema = json!({
        "type": "object",
        "properties": {
            "m": {
                "type": "object",
                "properties": {
                    "1": {"x-docs-url": "https://docs.example/one"},
                    "01": {"x-docs-url": "https://docs.example/zero-one"}
                }
            },
            "list": {
                "type": "array",
                "items": {"x-docs-url": "https://docs.example/item"}
            }
        }
    });
    let cases: &[(&str, Option<&str>)] = &[
        ("/m/1", Some("https://docs.example/one")),
        ("/m/01", Some("https://docs.example/zero-one")),
        ("/list/0", Some("https://docs.example/item")),
        ("/list/10", Some("https://docs.example/item")),
        ("/list/01", None),
    ];
    let mut failures = Vec::new();
    for (pointer, expected) in cases {
        let found = docs_url_at(&schema, pointer, false);
        if found.as_deref() != *expected {
            failures.push(format!("{pointer}: expected {expected:?}, found {found:?}"));
        }
    }
    report(failures);
}

#[test]
fn values_resolve_index_like_keys_and_indices() {
    use super::super::completion::value_at;
    let document = json!({"m": {"1": "one", "01": "zero-one"}, "list": ["v0", "v1"]});
    let at = |pointer: &str| value_at(&document, FieldPath::from(pointer).segments()).cloned();
    let cases = [
        ("/m/1", Some(json!("one"))),
        ("/m/01", Some(json!("zero-one"))),
        ("/list/1", Some(json!("v1"))),
        ("/list/01", None),
        ("/m/+1", None),
    ];
    let mut failures = Vec::new();
    for (pointer, expected) in cases {
        let found = at(pointer);
        if found != expected {
            failures.push(format!("{pointer}: expected {expected:?}, found {found:?}"));
        }
    }
    report(failures);
}
