//! Names for an object's keys (`x-config-editor-keys` in its
//! `propertyNames`), in every built-in language and through Edit-as: a new
//! key, and a key typed over, is offered the document's own keys at the
//! pointer, then the host's names, then the `propertyNames` `enum`/
//! `examples`, each once with its source, but not the keys the object has
//! already; the `propertyNames` is found through `$ref`, `allOf`, `anyOf`
//! and `oneOf`; and an unknown key is no problem because of the keyword,
//! while a `propertyNames` `enum` still judges the keys.

use serde_json::{Value, json};

use super::super::completion::{Completion, apply_completion, completions_in, word_key_origins};
use super::super::key_names::{KeyName, KeyNames};
use super::super::schema::build_schema;
use super::Language;
use super::harness::{At, BUILT_IN, report, write};
use crate::editor::text::text_size;

/// The `propertyNames` of `/reach`, naming the keys of `/clients` here
/// and in the schema `https://x/server`.
fn key_schema() -> Value {
    json!({
        "x-config-editor-keys": {"pointer": "/clients", "from": ["https://x/server"]},
        "examples": ["self"]
    })
}

/// The ways a schema may hold `/reach`'s `propertyNames`: written on it,
/// through a `$ref`, an `allOf`, an `anyOf` inside it, a `oneOf` the
/// object is in, and the object itself a `$ref`.
fn forms() -> Vec<(&'static str, Value)> {
    let values = json!({"type": "integer", "default": 1});
    vec![
        (
            "plain",
            json!({"type": "object", "propertyNames": key_schema(), "additionalProperties": values}),
        ),
        (
            "$ref",
            json!({"type": "object", "propertyNames": {"$ref": "#/$defs/client"}, "additionalProperties": values}),
        ),
        (
            "allOf",
            json!({"type": "object", "allOf": [{"propertyNames": key_schema()}], "additionalProperties": values}),
        ),
        (
            "anyOf inside",
            json!({"type": "object", "propertyNames": {"anyOf": [key_schema()]}, "additionalProperties": values}),
        ),
        (
            "oneOf",
            json!({"oneOf": [{"type": "object", "propertyNames": key_schema(), "additionalProperties": values}, {"type": "string"}]}),
        ),
        ("object $ref", json!({"$ref": "#/$defs/reach"})),
    ]
}

fn schema(reach: &Value) -> String {
    json!({
        "type": "object",
        "$defs": {
            "client": key_schema(),
            "reach": {"type": "object", "propertyNames": key_schema(), "additionalProperties": {"type": "integer", "default": 1}}
        },
        "properties": {
            "clients": {"type": "object", "additionalProperties": {"type": "object"}},
            "reach": reach
        }
    })
    .to_string()
}

fn document() -> Value {
    json!({
        "clients": {"home": {"port": 1}, "work": {"port": 2}},
        "reach": {"home": 1}
    })
}

fn host() -> Vec<KeyNames> {
    vec![
        KeyNames::new(
            "/clients",
            [KeyName::new("remote").with_source("server.d/remote.yaml")],
        ),
        KeyNames::new("/clients", ["fromserver"]).for_schema("https://x/server"),
        KeyNames::new("/clients", ["foreign"]).for_schema("https://x/other"),
    ]
}

/// The suggestions at `byte` of `text` in `language`, worded.
fn offered(
    language: Language,
    text: &str,
    byte: usize,
    schema: &str,
    replace_word: bool,
) -> Vec<Completion> {
    let bundle = build_schema(schema).expect("the schema compiles");
    let behavior = language.behavior();
    let mut options = completions_in(
        text,
        text_size(byte),
        true,
        Some(&bundle),
        true,
        behavior,
        replace_word,
        &behavior.string_body_lines(text),
        (&host(), None),
    );
    word_key_origins(&mut options, crate::strings::default_strings());
    options
}

/// `(label, detail)` of each key expected: `home` only where it is the
/// key typed over.
fn expected(retyped: bool) -> Vec<(String, String)> {
    let mut names = Vec::new();
    if retyped {
        names.push(("home", "this document"));
    }
    names.extend([
        ("work", "this document"),
        ("remote", "server.d/remote.yaml"),
        ("fromserver", ""),
        ("self", ""),
    ]);
    names
        .into_iter()
        .map(|(label, detail)| (label.to_string(), detail.to_string()))
        .collect()
}

/// A text and its caret, and whether the key there is typed over.
type Place = dyn Fn() -> Result<(String, usize, bool), String>;

/// The places of each case: a new key of `/reach`, and its key `home`
/// typed over, in each language and in YAML edited as TOML and as JSON.
fn cases() -> Vec<(String, Language, Box<Place>)> {
    let mut cases: Vec<(String, Language, Box<Place>)> = Vec::new();
    for language in BUILT_IN {
        cases.push((
            format!("{language} new key"),
            language,
            Box::new(move || {
                write(
                    language,
                    &document(),
                    &[],
                    At::NewKey {
                        object: "/reach",
                        typed: "",
                    },
                )
                .map(|written| (written.text, written.byte, false))
            }),
        ));
        cases.push((
            format!("{language} retyped key"),
            language,
            Box::new(move || {
                write(
                    language,
                    &document(),
                    &[],
                    At::InKey {
                        pointer: "/reach/home",
                        offset: 2,
                    },
                )
                .map(|written| (written.text, written.byte, true))
            }),
        ));
    }
    for shown in [Language::TOML, Language::JSON] {
        cases.push((
            format!("YAML edited as {shown}, retyped key"),
            shown,
            Box::new(move || {
                let written = Language::YAML
                    .serialize(&document())
                    .map_err(|error| error.to_string())?;
                let text = Language::YAML
                    .convert(&written, shown)
                    .map_err(|error| error.to_string())?
                    .text;
                // `/reach` comes after `/clients`: its `home` is the last.
                let at = text.rfind("home").ok_or("no home")? + 2;
                Ok((text, at, true))
            }),
        ));
    }
    cases
}

#[test]
fn a_key_names_object_suggests_the_documents_keys_then_the_hosts_then_the_schemas_values() {
    let mut failures = Vec::new();
    for (name, language, place) in cases() {
        let (text, byte, retyped) = match place() {
            Ok(placed) => placed,
            Err(error) => {
                failures.push(format!("{name}: {error}"));
                continue;
            }
        };
        for (form, reach) in forms() {
            let got: Vec<(String, String)> =
                offered(language, &text, byte, &schema(&reach), retyped)
                    .into_iter()
                    .map(|option| (option.label, option.detail))
                    .collect();
            let want = expected(retyped);
            if got != want {
                failures.push(format!(
                    "{name} {form}:\n  got  {got:?}\n  want {want:?}\n{}|{}",
                    &text[..byte],
                    &text[byte..]
                ));
            }
        }
    }
    report(&failures);
}

/// Each name is written by the language's writer as a key: accepting one
/// gives a document with that key in the object.
#[test]
fn an_accepted_key_name_is_written_by_the_language() {
    let mut failures = Vec::new();
    let schema = schema(&forms()[0].1);
    for language in BUILT_IN {
        let at = At::NewKey {
            object: "/reach",
            typed: "",
        };
        let written = match write(language, &document(), &[], at) {
            Ok(written) => written,
            Err(error) => {
                failures.push(format!("{language}: {error}"));
                continue;
            }
        };
        let options = offered(language, &written.text, written.byte, &schema, false);
        let Some(work) = options.iter().find(|option| option.label == "work") else {
            failures.push(format!("{language}: no work offered"));
            continue;
        };
        let (text, _, _) = apply_completion(&written.text, work);
        let read = language.parse(&text).ok();
        let value = read.as_ref().and_then(|read| read.pointer("/reach/work"));
        if value.is_none_or(|value| value != 1 && value != "1") {
            failures.push(format!("{language}: no /reach/work in\n{text}"));
        }
    }
    report(&failures);
}

/// A `propertyNames` with only an `enum` or `examples` offers them as
/// keys; a name its `enum` leaves out is not offered.
#[test]
fn a_property_names_enum_or_examples_names_the_keys() {
    let mut failures = Vec::new();
    let cases = [
        (json!({"enum": ["alpha", "beta"]}), vec!["alpha", "beta"]),
        (
            json!({"examples": ["alpha", "beta"]}),
            vec!["alpha", "beta"],
        ),
        (
            json!({"enum": ["alpha", "gamma"], "examples": ["beta", "gamma"]}),
            vec!["alpha", "gamma"],
        ),
    ];
    for (names, want) in cases {
        let reach = json!({"type": "object", "propertyNames": names, "additionalProperties": {"type": "integer"}});
        let schema = schema(&reach);
        for language in [Language::YAML, Language::JSON, Language::TOML] {
            let at = At::NewKey {
                object: "/reach",
                typed: "",
            };
            let written = write(language, &document(), &[], at).expect("written");
            let got: Vec<String> = offered(language, &written.text, written.byte, &schema, false)
                .into_iter()
                .map(|option| option.label)
                .collect();
            if got != want {
                failures.push(format!("{language} {names}: got {got:?}, want {want:?}"));
            }
        }
    }
    report(&failures);
}

/// The keyword is a suggestion only: no schema warning, and no problem for
/// a key that names no key anywhere; a `propertyNames` `enum` still
/// judges the keys, as JSON Schema says.
#[cfg(feature = "schema-validation")]
#[test]
fn a_key_that_is_no_name_is_never_a_problem_but_property_names_still_judges() {
    let mut failures = Vec::new();
    let mut unknown = document();
    unknown["reach"] = json!({"nosuchname": 1});
    for (form, reach) in forms() {
        let schema = crate::Schema::compile(&schema(&reach)).expect("the schema compiles");
        if !schema.warnings().is_empty() {
            failures.push(format!("{form}: warnings {:?}", schema.warnings()));
        }
        for language in BUILT_IN {
            let text = language.serialize(&unknown).expect("written");
            let problems = schema.validate(language, &text);
            if !problems.is_empty() {
                failures.push(format!("{language} {form}: {problems:?}"));
            }
        }
    }
    let mut named = key_schema();
    named["enum"] = json!(["home", "work"]);
    let reach = json!({"type": "object", "propertyNames": named, "additionalProperties": {"type": "integer"}});
    let schema = crate::Schema::compile(&schema(&reach)).expect("the schema compiles");
    for language in BUILT_IN {
        let text = language.serialize(&unknown).expect("written");
        if schema.validate(language, &text).is_empty() {
            failures.push(format!("{language}: a key the enum leaves out passes"));
        }
        let text = language.serialize(&document()).expect("written");
        let problems = schema.validate(language, &text);
        if !problems.is_empty() {
            failures.push(format!("{language}: a key the enum names: {problems:?}"));
        }
    }
    report(&failures);
}
