//! Names for fields that name a key defined elsewhere
//! (`x-config-editor-keys`), in every built-in language and through Edit-as:
//! at each place a string schema sits (a value, a list's item, a property
//! of an object, an `anyOf` option) and with each written form of the
//! keyword, the suggestions are the document's own keys at the pointer,
//! then the host's names, then the schema's `enum`/`examples`, each once
//! with its source; a list's item skips the names the list holds; and no
//! name, known or not, is ever a problem.

use serde_json::{Value, json};

use super::super::completion::{Completion, completions_in, word_key_origins};
use super::super::key_names::{KeyName, KeyNames};
use super::super::schema::build_schema;
use super::Language;
use super::harness::{At, BUILT_IN, report, write};
use crate::editor::text::text_size;

const KEYWORD: &str = "x-config-editor-keys";

/// The keyword's written forms, and whether each names `/groups` too and
/// the schema `https://x/server` in `from`.
fn forms() -> Vec<(Value, bool, bool)> {
    vec![
        (json!("/clients"), false, false),
        (json!(["/clients", "/groups"]), true, false),
        (
            json!({"pointer": "/clients", "from": "https://x/server"}),
            false,
            true,
        ),
        (
            json!([{"pointer": "/clients", "from": ["https://x/server"]}]),
            false,
            true,
        ),
    ]
}

fn schema(keyword: &Value) -> String {
    let named = json!({"type": "string", KEYWORD: keyword, "examples": ["self"]});
    json!({
        "type": "object",
        "properties": {
            "clients": {"type": "object", "additionalProperties": {"type": "object"}},
            "default_client": named,
            "members": {"type": "array", "items": named},
            "route": {
                "type": "object",
                "properties": {"name": named, "weight": {"type": "integer"}}
            },
            "fallback": {"anyOf": [{"enum": ["none"]}, {"type": "string", KEYWORD: keyword}]}
        }
    })
    .to_string()
}

fn document() -> Value {
    json!({
        "clients": {"home": {"port": 1}, "work": {"port": 2}},
        "default_client": "x",
        "members": ["home", "x"],
        "route": {"name": "x", "weight": 2},
        "fallback": "x"
    })
}

/// The host's names: for this schema (no `$id`), for the schema `from`
/// names, for `/groups`, and for a schema no form names.
fn host() -> Vec<KeyNames> {
    vec![
        KeyNames::new(
            "/clients",
            [KeyName::new("remote").with_source("server.d/remote.yaml")],
        ),
        KeyNames::new("/clients", ["fromserver"]).for_schema("https://x/server"),
        KeyNames::new("/groups", ["admins"]),
        KeyNames::new("/clients", ["foreign"]).for_schema("https://x/other"),
        // A host name the document has too is offered once, as its own.
        KeyNames::new(
            "/clients",
            [KeyName::new("work").with_source("elsewhere.yaml")],
        ),
    ]
}

/// `(label, detail)` of each suggestion expected at the value `pointer`.
fn expected(pointer: &str, groups: bool, from: bool) -> Vec<(String, String)> {
    let mut names = Vec::new();
    if pointer != "/members/1" {
        names.push(("home", "this document"));
    }
    names.push(("work", "this document"));
    names.push(("remote", "server.d/remote.yaml"));
    if groups {
        names.push(("admins", ""));
    }
    if from {
        names.push(("fromserver", ""));
    }
    names.push(if pointer == "/fallback" {
        ("none", "")
    } else {
        ("self", "")
    });
    names
        .into_iter()
        .map(|(label, detail)| (label.to_string(), detail.to_string()))
        .collect()
}

/// The suggestions at `byte` of `text` in `language`, worded.
fn offered(language: Language, text: &str, byte: usize, schema: &str) -> Vec<Completion> {
    let bundle = build_schema(schema).expect("the schema compiles");
    let behavior = language.behavior();
    let mut options = completions_in(
        text,
        text_size(byte),
        true,
        Some(&bundle),
        true,
        behavior,
        false,
        &behavior.string_body_lines(text),
        (&host(), None),
    );
    word_key_origins(&mut options, crate::strings::default_strings());
    options
}

/// `document` written by `host` with `slot` at `pointer`, converted to
/// `shown` (Edit-as), and the caret where the slot was.
fn edited_as(host: Language, shown: Language, pointer: &str) -> Result<(String, usize), String> {
    const SLOT: &str = "editasslotvalue";
    let mut held = document();
    *held.pointer_mut(pointer).expect("a value") = Value::String(SLOT.into());
    let written = host.serialize(&held).map_err(|error| error.to_string())?;
    let mut text = host
        .convert(&written, shown)
        .map_err(|error| error.to_string())?
        .text;
    let at = text.find(SLOT).ok_or("no slot")?;
    let quotes = shown.behavior().config().quotes;
    let (mut start, mut end) = (at, at + SLOT.len());
    if let (Some(open), Some(close)) = (text[..start].chars().last(), text[end..].chars().next())
        && open == close
        && quotes.contains(&open.to_string().as_str())
    {
        start -= 1;
        end += 1;
    }
    text.replace_range(start..end, "");
    Ok((text, start))
}

/// Writes the document with the caret at the value at a pointer.
type Place = dyn Fn(&str) -> Result<(String, usize), String>;

#[test]
fn a_key_names_field_suggests_the_documents_keys_then_the_hosts_then_the_schemas_values() {
    let mut failures = Vec::new();
    let places = ["/default_client", "/members/1", "/route/name", "/fallback"];
    let mut cases: Vec<(String, Language, Box<Place>)> = Vec::new();
    for language in BUILT_IN {
        cases.push((
            language.label().to_string(),
            language,
            Box::new(move |pointer| {
                write(language, &document(), &[], At::EmptyValue(pointer))
                    .map(|written| (written.text, written.byte))
            }),
        ));
    }
    for shown in [Language::TOML, Language::JSON] {
        cases.push((
            format!("YAML edited as {shown}"),
            shown,
            Box::new(move |pointer| edited_as(Language::YAML, shown, pointer)),
        ));
    }
    for (name, language, place) in &cases {
        for (keyword, groups, from) in forms() {
            let schema = schema(&keyword);
            for pointer in places {
                let (text, byte) = match place(pointer) {
                    Ok(placed) => placed,
                    Err(error) => {
                        failures.push(format!("{name} {pointer}: {error}"));
                        continue;
                    }
                };
                let got: Vec<(String, String)> = offered(*language, &text, byte, &schema)
                    .into_iter()
                    .map(|option| (option.label, option.detail))
                    .collect();
                let want = expected(pointer, groups, from);
                if got != want {
                    failures.push(format!(
                        "{name} {pointer} {keyword}:\n  got  {got:?}\n  want {want:?}\n{}|{}",
                        &text[..byte],
                        &text[byte..]
                    ));
                }
            }
        }
    }
    report(&failures);
}

/// Each name is written by the language's writer: accepting one gives a
/// document that reads the name back at its place.
#[test]
fn an_accepted_name_is_written_by_the_language() {
    let mut failures = Vec::new();
    let schema = schema(&json!("/clients"));
    for language in BUILT_IN {
        for pointer in ["/default_client", "/route/name", "/fallback"] {
            let written = match write(language, &document(), &[], At::EmptyValue(pointer)) {
                Ok(written) => written,
                Err(error) => {
                    failures.push(format!("{language} {pointer}: {error}"));
                    continue;
                }
            };
            let options = offered(language, &written.text, written.byte, &schema);
            let Some(work) = options.iter().find(|option| option.label == "work") else {
                failures.push(format!("{language} {pointer}: no work offered"));
                continue;
            };
            let (text, _, _) = super::super::completion::apply_completion(&written.text, work);
            let read = language.parse(&text).ok();
            let value = read.as_ref().and_then(|read| read.pointer(pointer));
            if value != Some(&json!("work")) {
                failures.push(format!("{language} {pointer}: reads {value:?}\n{text}"));
            }
        }
    }
    report(&failures);
}

/// The keyword is a suggestion only: no schema warning, and no problem for
/// a name that is not a key anywhere.
#[cfg(feature = "schema-validation")]
#[test]
fn a_name_that_is_no_key_is_never_a_problem() {
    let mut failures = Vec::new();
    for (keyword, _, _) in forms() {
        let schema = crate::Schema::compile(&schema(&keyword)).expect("the schema compiles");
        if !schema.warnings().is_empty() {
            failures.push(format!("{keyword}: warnings {:?}", schema.warnings()));
        }
        let mut unknown = document();
        for pointer in ["/default_client", "/members/1", "/route/name", "/fallback"] {
            *unknown.pointer_mut(pointer).expect("a value") = json!("nosuchname");
        }
        for language in BUILT_IN {
            let text = language.serialize(&unknown).expect("written");
            let problems = schema.validate(language, &text);
            if !problems.is_empty() {
                failures.push(format!("{language} {keyword}: {problems:?}"));
            }
        }
    }
    report(&failures);
}

/// `tools::keys_at` reads the keys at a pointer in every language, in the
/// order written; no map there is none, and a text that does not parse is
/// `None` (the host keeps its last good names).
#[test]
fn keys_at_reads_the_map_at_a_pointer_in_every_language() {
    use crate::tools::keys_at;
    let mut failures = Vec::new();
    let broken = [
        (Language::YAML, "clients: ["),
        (Language::JSON, "{\"clients\": {"),
        (Language::TOML, "clients = ["),
        (Language::HCL, "clients = {"),
        (Language::XML, "<config><clients>"),
    ];
    for (language, broken) in broken {
        let text = super::harness::text(language, &document(), &[]).expect("written");
        let got = (
            keys_at(&text, language, "/clients"),
            keys_at(&text, language, "/route"),
            keys_at(&text, language, "/nothing"),
            keys_at(broken, language, "/clients"),
        );
        let want = (
            Some(vec!["home".to_string(), "work".to_string()]),
            Some(vec!["name".to_string(), "weight".to_string()]),
            Some(Vec::new()),
            None,
        );
        if got != want {
            failures.push(format!("{language}: {got:?}\n{text}"));
        }
    }
    report(&failures);
}
