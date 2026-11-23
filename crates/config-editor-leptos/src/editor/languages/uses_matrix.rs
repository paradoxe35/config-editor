//! Find uses (the engine behind `Shift+F12` and `tools::uses_at`) in every
//! built-in language and through Edit-as: a name is used by a value, a
//! list's items, a property of an object, an `anyOf` option and a key an
//! object's `propertyNames` reads, wherever the keyword names its map (and
//! the schema of its documents, through `from`); never by a free text
//! field, a field whose `examples` have the word, or a field naming another
//! map. Each use's range covers the name as written.

use serde_json::{Value, json};

use super::super::completion::field_at;
use super::super::key_names::{
    NameRef, NoKeyNames, SchemaIdentity, UsePart, name_at, searched_at, uses_in,
};
use super::super::schema::build_schema;
use super::Language;
use super::harness::{BUILT_IN, report};

const KEYWORD: &str = "x-config-editor-keys";
const SERVER: &str = "https://x/server";

fn schema(keyword: &Value) -> String {
    let named = json!({"type": "string", KEYWORD: keyword});
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
            "fallback": {"anyOf": [{"type": "integer"}, {"type": "string", KEYWORD: keyword}]},
            "note": {"type": "string"},
            "mode": {"type": "string", "examples": ["home"]},
            "group": {"type": "string", KEYWORD: "/groups"},
            "reach": {
                "type": "object",
                "propertyNames": {KEYWORD: keyword},
                "additionalProperties": {"type": "array"}
            }
        }
    })
    .to_string()
}

fn document() -> Value {
    json!({
        "clients": {"home": {"port": 1}, "work": {"port": 2}},
        "default_client": "home",
        "members": ["home", "work", "home"],
        "route": {"name": "home", "weight": 2},
        "fallback": "home",
        "note": "home",
        "mode": "home",
        "group": "home",
        "reach": {"home": [1], "work": [2]}
    })
}

/// The uses of `home`.
const USES: [(&str, UsePart); 6] = [
    ("/default_client", UsePart::Value),
    ("/members/0", UsePart::Value),
    ("/members/2", UsePart::Value),
    ("/route/name", UsePart::Value),
    ("/fallback", UsePart::Value),
    ("/reach/home", UsePart::Key),
];

/// The uses of `home` at `/clients` in `text`, named for the schema `of`
/// (`None`: the document's own): each one's pointer and part, and the text
/// its range covers.
fn found(
    language: Language,
    text: &str,
    keyword: &Value,
    of: Option<&str>,
) -> Vec<(String, UsePart, String)> {
    let bundle = build_schema(&schema(keyword)).expect("the schema compiles");
    let name = NameRef {
        schema: of.map_or_else(
            || bundle.identity.clone(),
            |id| SchemaIdentity::Id(id.to_string()),
        ),
        pointer: "/clients".into(),
        name: "home".into(),
    };
    uses_in(
        text,
        language.behavior(),
        &bundle.schema,
        &bundle.identity,
        &name,
    )
    .into_iter()
    .map(|hit| {
        let written = text[hit.range.start().into()..hit.range.end().into()].to_string();
        (hit.path.to_pointer(), hit.part, written)
    })
    .collect()
}

/// Each case: a name and the text of the document in a language.
fn cases() -> Vec<(String, Language, String)> {
    let mut cases = Vec::new();
    for language in BUILT_IN {
        let text = super::harness::text(language, &document(), &[]).expect("written");
        cases.push((language.label().to_string(), language, text));
    }
    for shown in [Language::TOML, Language::JSON] {
        let yaml = Language::YAML.serialize(&document()).expect("written");
        let text = Language::YAML.convert(&yaml, shown).expect("converts").text;
        cases.push((format!("YAML edited as {shown}"), shown, text));
    }
    cases
}

/// Whether `written` is the name, as a language writes it (in quotes or
/// not).
fn is_name(written: &str) -> bool {
    written.trim_matches(|c| c == '"' || c == '\'') == "home"
}

#[test]
fn find_uses_lists_every_value_item_field_option_and_key_that_names_the_key() {
    let mut failures = Vec::new();
    for (name, language, text) in cases() {
        for keyword in [
            json!("/clients"),
            json!(["/groups", "/clients"]),
            json!({"pointer": "/clients", "from": SERVER}),
        ] {
            let got = found(language, &text, &keyword, None);
            // In the order written, which differs by language.
            let mut paths: Vec<(String, UsePart)> = got
                .iter()
                .map(|(pointer, part, _)| (pointer.clone(), *part))
                .collect();
            let mut want: Vec<(String, UsePart)> = USES
                .iter()
                .map(|(pointer, part)| (pointer.to_string(), *part))
                .collect();
            paths.sort_by(|one, other| one.0.cmp(&other.0));
            want.sort_by(|one, other| one.0.cmp(&other.0));
            if paths != want {
                failures.push(format!(
                    "{name} {keyword}:\n  got  {paths:?}\n  want {want:?}\n{text}"
                ));
            }
            for (pointer, _, written) in &got {
                if !is_name(written) {
                    failures.push(format!("{name} {keyword} {pointer}: covers {written:?}"));
                }
            }
        }
    }
    report(&failures);
}

/// `from` names the schema of the documents defining the names: a name of
/// that schema is used here, a name of a schema no keyword names is not.
#[test]
fn find_uses_honors_the_schemas_from_names() {
    let mut failures = Vec::new();
    for (name, language, text) in cases() {
        let from = json!({"pointer": "/clients", "from": [SERVER]});
        let of_server = found(language, &text, &from, Some(SERVER)).len();
        let plain_of_server = found(language, &text, &json!("/clients"), Some(SERVER)).len();
        let of_other = found(language, &text, &from, Some("https://x/other")).len();
        if (of_server, plain_of_server, of_other) != (USES.len(), 0, 0) {
            failures.push(format!(
                "{name}: from {of_server}, without from {plain_of_server}, other {of_other}"
            ));
        }
    }
    report(&failures);
}

/// `tools::uses_at` finds the same uses with their lines; a text that does
/// not parse finds none (`None`).
#[test]
fn uses_at_finds_the_uses_in_every_language() {
    use crate::tools::uses_at;
    let mut failures = Vec::new();
    let schema = crate::Schema::compile(&schema(&json!("/clients"))).expect("compiles");
    for language in BUILT_IN {
        let text = super::harness::text(language, &document(), &[]).expect("written");
        let found = uses_at(&text, language, &schema, "/clients", "home", None).unwrap_or_default();
        let mut paths: Vec<String> = found.iter().map(|found| found.path.to_pointer()).collect();
        let mut want: Vec<&str> = USES.iter().map(|(pointer, _)| *pointer).collect();
        paths.sort();
        want.sort();
        if paths != want {
            failures.push(format!("{language}: {paths:?}\n{text}"));
        }
        for found in &found {
            let line = text.lines().nth(found.line - 1).unwrap_or_default();
            if line != found.text || !is_name(&found.range.slice(&text)) {
                failures.push(format!("{language}: {found:?}"));
            }
        }
        if uses_at("{[<", language, &schema, "/clients", "home", None).is_some() {
            failures.push(format!("{language}: a broken text finds uses"));
        }
    }
    report(&failures);
}

/// The document with its lists, objects and maps written on one line each
/// (a YAML flow map, a TOML inline table), or whole on one line (JSON,
/// XML).
fn one_line(language: Language) -> String {
    match language.label() {
        "YAML" => concat!(
            "clients:\n  home: {port: 1}\n  work: {port: 2}\ndefault_client: home\n",
            "members: [home, work, home]\nroute: {name: home, weight: 2}\nfallback: home\n",
            "note: home\nmode: home\ngroup: home\nreach: {home: [1], work: [2]}\n",
        )
        .to_string(),
        "TOML" => concat!(
            "default_client = \"home\"\nmembers = [\"home\", \"work\", \"home\"]\n",
            "route = {name = \"home\", weight = 2}\nfallback = \"home\"\nnote = \"home\"\n",
            "mode = \"home\"\ngroup = \"home\"\nreach = {home = [1], work = [2]}\n\n",
            "[clients.home]\nport = 1\n\n[clients.work]\nport = 2\n",
        )
        .to_string(),
        "HCL" => concat!(
            "clients = { home = { port = 1 }, work = { port = 2 } }\n",
            "default_client = \"home\"\nmembers = [\"home\", \"work\", \"home\"]\n",
            "route = { name = \"home\", weight = 2 }\nfallback = \"home\"\nnote = \"home\"\n",
            "mode = \"home\"\ngroup = \"home\"\nreach = { home = [1], work = [2] }\n",
        )
        .to_string(),
        "JSON" => format!("{}\n", document()),
        _ => {
            let text = super::harness::text(language, &document(), &[]).expect("written");
            let joined: String = text.lines().map(str::trim).collect();
            format!("{joined}\n")
        }
    }
}

/// Every use, written in block form or on one line, is reached by the one
/// lookup hover, go to definition and Find uses share (`field_at`): on its
/// first, middle and last character, the name there is the known name
/// `home` (the hover card's "defined in", the definition to go to) and what
/// Find uses searches for.
#[test]
fn every_use_is_a_known_name_to_hover_definition_and_find_uses() {
    let mut failures = Vec::new();
    let keyword = json!("/clients");
    let bundle = build_schema(&schema(&keyword)).expect("the schema compiles");
    let name = NameRef {
        schema: bundle.identity.clone(),
        pointer: "/clients".into(),
        name: "home".into(),
    };
    let mut all = cases();
    for language in BUILT_IN {
        all.push((
            format!("{language} on one line"),
            language,
            one_line(language),
        ));
    }
    for (label, language, text) in all {
        let behavior = language.behavior();
        let hits = uses_in(&text, behavior, &bundle.schema, &bundle.identity, &name);
        if hits.len() != USES.len() {
            failures.push(format!("{label}: {} uses found\n{text}", hits.len()));
        }
        for hit in hits {
            let start = usize::from(hit.range.start());
            let end = usize::from(hit.range.end());
            let line_start = text[..start].rfind('\n').map_or(0, |at| at + 1);
            let first = text[line_start..start].chars().count();
            let last = first + text[start..end].chars().count() - 1;
            for column in [first, usize::midpoint(first, last), last] {
                let place = format!("{label} {} column {column}", hit.path.to_pointer());
                let Some(field) = field_at(&text, hit.line, column, behavior) else {
                    failures.push(format!("{place}: no field\n{text}"));
                    continue;
                };
                let known = name_at(&text, &field, &bundle, behavior, &NoKeyNames);
                if known.map(|known| known.name).as_deref() != Some("home") {
                    failures.push(format!("{place}: no known name at {field:?}"));
                }
                let searched =
                    searched_at(&text, &field, &bundle, behavior, &NoKeyNames, &|_| false);
                if searched.map(|searched| searched.name.name).as_deref() != Some("home") {
                    failures.push(format!("{place}: Find uses searches for nothing"));
                }
            }
        }
    }
    report(&failures);
}
