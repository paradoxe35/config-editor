//! The suggestion list's promises, checked in every language and form (block,
//! inline, XML attributes and elements) against the burrow schema, so a fix
//! for one language cannot leave its siblings behind. Each test lists every
//! failing case at once.
//!
//! `|` marks the caret.

use super::super::completion::{apply_completion, completions};
use super::super::diagnostics::{SchemaBundle, build_schema};
use super::Language;
use serde_json::Value;

fn burrow() -> SchemaBundle {
    build_schema(include_str!("../../../tests/fixtures/burrow.schema.json")).unwrap()
}

/// The labels offered at the caret (as after Ctrl+Space).
fn offered(language: Language, marked: &str) -> Vec<String> {
    let at = marked.find('|').expect("a | caret marker");
    let source = marked.replacen('|', "", 1);
    completions(
        &source,
        source[..at].encode_utf16().count(),
        true,
        Some(&burrow()),
        true,
        language.behavior(),
    )
    .into_iter()
    .map(|option| option.label)
    .collect()
}

/// The document after accepting `label` at the caret, or why it could not be.
fn accepted(language: Language, marked: &str, label: &str) -> Result<String, String> {
    let at = marked.find('|').expect("a | caret marker");
    let source = marked.replacen('|', "", 1);
    let options = completions(
        &source,
        source[..at].encode_utf16().count(),
        true,
        Some(&burrow()),
        true,
        language.behavior(),
    );
    let option = options
        .iter()
        .find(|option| option.label == label)
        .ok_or_else(|| {
            format!(
                "{label} not offered; offered {:?}",
                options.iter().map(|o| &o.label).collect::<Vec<_>>()
            )
        })?;
    Ok(apply_completion(&source, option).0)
}

fn parsed(language: Language, text: &str) -> Result<Value, String> {
    language
        .parse(text)
        .map_err(|error| format!("does not parse: {error}"))
}

/// The keys of the object at `path` (keys only) in `value`.
fn keys_at(value: &Value, path: &[&str]) -> Vec<String> {
    let mut current = value;
    for segment in path {
        current = match current.get(segment) {
            Some(next) => next,
            None => return Vec::new(),
        };
    }
    current
        .as_object()
        .map(|map| map.keys().cloned().collect())
        .unwrap_or_default()
}

#[track_caller]
fn report(failures: Vec<String>) {
    assert!(
        failures.is_empty(),
        "{} failing case(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

const ALL: [Language; 5] = [
    Language::YAML,
    Language::JSON,
    Language::TOML,
    Language::HCL,
    Language::XML,
];

/// `tcp` holding `nodelay = true`, in each language and form, with the caret
/// where `{caret}` is: the documents the cases below edit.
fn tcp_forms(language: Language, entry: &str) -> Vec<(&'static str, String)> {
    let entry_json = format!("\"{entry}");
    match language.label() {
        "YAML" => vec![
            ("block", format!("tcp:\n  {entry}: true\n")),
            ("flow", format!("tcp: {{{entry}: true}}\n")),
        ],
        "JSON" => vec![(
            "object",
            format!("{{\n  \"tcp\": {{\n    {entry_json}\": true\n  }}\n}}\n"),
        )],
        "TOML" => vec![
            ("table", format!("[tcp]\n{entry} = true\n")),
            ("inline", format!("tcp = {{{entry} = true}}\n")),
        ],
        "HCL" => vec![
            ("block", format!("tcp {{\n  {entry} = true\n}}\n")),
            ("inline", format!("tcp = {{{entry} = true}}\n")),
        ],
        _ => vec![
            ("attribute", format!("<tcp {entry}=\"true\"/>\n")),
            (
                "element",
                format!("<tcp>\n  <{entry}>true</nodelay>\n</tcp>\n"),
            ),
        ],
    }
}

/// A key typed in front of an existing one (`k|nodelay`) and accepted is a
/// new entry before it: both keys are there and the document still parses.
/// (In JSON the caret is inside the key's quotes there, and a suggestion
/// replaces the quoted key, as in VS Code.)
#[test]
fn a_key_accepted_in_front_of_an_entry_is_a_new_entry() {
    let mut failures = Vec::new();
    for language in ALL
        .into_iter()
        .filter(|language| language.label() != "JSON")
    {
        for (form, marked) in tcp_forms(language, "k|nodelay") {
            let case = format!("{} {form}: {marked:?}", language.label());
            match accepted(language, &marked, "keepalive_secs").and_then(|text| {
                let value =
                    parsed(language, &text).map_err(|error| format!("{error}: {text:?}"))?;
                Ok((text, value))
            }) {
                Ok((text, value)) => {
                    let keys = keys_at(&value, &["tcp"]);
                    if !(keys.contains(&"nodelay".to_string())
                        && keys.contains(&"keepalive_secs".to_string()))
                    {
                        failures.push(format!("{case} → {text:?} has keys {keys:?}"));
                    }
                }
                Err(error) => failures.push(format!("{case} → {error}")),
            }
        }
    }
    report(failures);
}

/// The key the caret is inside is offered (it is not a sibling of itself),
/// and accepting it changes nothing.
#[test]
fn the_key_being_edited_is_offered_and_accepting_it_changes_nothing() {
    let mut failures = Vec::new();
    for language in ALL {
        for (form, marked) in tcp_forms(language, "nodela|y") {
            let case = format!("{} {form}: {marked:?}", language.label());
            let original = marked.replacen('|', "", 1);
            match accepted(language, &marked, "nodelay") {
                Ok(text)
                    if parsed(language, &text) == parsed(language, &original)
                        && parsed(language, &text).is_ok() => {}
                Ok(text) => failures.push(format!("{case} → {text:?}")),
                Err(error) => failures.push(format!("{case} → {error}")),
            }
        }
    }
    report(failures);
}

/// A key already in the object is not offered again, whether it comes
/// before or after the caret.
#[test]
fn keys_already_present_before_or_after_the_caret_are_not_offered() {
    let cases: Vec<(Language, &str)> = vec![
        (Language::YAML, "tcp:\n  |\n  nodelay: true\n"),
        (Language::YAML, "tcp:\n  nodelay: true\n  |\n"),
        (Language::YAML, "tcp: {| nodelay: true}\n"),
        (Language::YAML, "tcp: {nodelay: true, |}\n"),
        (
            Language::JSON,
            "{\n  \"tcp\": {\n    \"|\"\n    \"nodelay\": true\n  }\n}\n",
        ),
        (
            Language::JSON,
            "{\n  \"tcp\": {\n    \"|\",\n    \"nodelay\": true\n  }\n}\n",
        ),
        (
            Language::JSON,
            "{\n  \"tcp\": {\n    \"nodelay\": true,\n    \"|\"\n  }\n}\n",
        ),
        (Language::TOML, "[tcp]\n|\nnodelay = true\n"),
        (Language::TOML, "[tcp]\nnodelay = true\n|\n"),
        (Language::TOML, "tcp = {nodelay = true, |}\n"),
        (Language::HCL, "tcp {\n  |\n  nodelay = true\n}\n"),
        (Language::HCL, "tcp {\n  nodelay = true\n  |\n}\n"),
        (Language::HCL, "tcp = {nodelay = true, |}\n"),
        (
            Language::XML,
            "<tcp>\n  <|\n  <nodelay>true</nodelay>\n</tcp>\n",
        ),
        (
            Language::XML,
            "<tcp>\n  <nodelay>true</nodelay>\n  <|\n</tcp>\n",
        ),
        (Language::XML, "<tcp | nodelay=\"true\"/>\n"),
        (Language::XML, "<tcp nodelay=\"true\" |/>\n"),
    ];
    let mut failures = Vec::new();
    for (language, marked) in cases {
        let labels = offered(language, marked);
        if labels.contains(&"nodelay".to_string()) {
            failures.push(format!(
                "{} {marked:?} offers nodelay: {labels:?}",
                language.label()
            ));
        }
        if !labels.contains(&"keepalive_secs".to_string()) {
            failures.push(format!(
                "{} {marked:?} lacks keepalive_secs: {labels:?}",
                language.label()
            ));
        }
    }
    report(failures);
}

/// Where a name is not a property (an HCL block label, quoted or not),
/// nothing is offered.
#[test]
fn block_labels_get_no_property_suggestions() {
    let cases = [
        "services \"|\" {\n}\n",
        "services |",
        "services we|",
        "services w| {\n}\n",
        "clients m|",
        "clients \"a\" \"|\" {\n}\n",
    ];
    let mut failures = Vec::new();
    for marked in cases {
        let labels = offered(Language::HCL, marked);
        if !labels.is_empty() {
            failures.push(format!("{marked:?} offers {labels:?}"));
        }
    }
    report(failures);
}

/// An XML element name typed in front of, or inside, an existing tag never
/// breaks that element: it becomes its own element or renames the tag.
#[test]
fn an_xml_element_name_typed_at_an_existing_tag_keeps_it_an_element() {
    let root = |body: &str| format!("<burrow config-root=\"true\">\n  {body}\n</burrow>\n");
    let cases = [
        (root("<pu|tcp/>"), "public_address"),
        (
            root("<li|tcp>\n    <nodelay>true</nodelay>\n  </tcp>"),
            "limits",
        ),
        (root("<tc|p/>"), "tcp"),
    ];
    let mut failures = Vec::new();
    for (marked, label) in cases {
        match accepted(Language::XML, &marked, label).and_then(|text| {
            let value =
                parsed(Language::XML, &text).map_err(|error| format!("{error}: {text:?}"))?;
            Ok((text, value))
        }) {
            Ok((text, value)) => {
                let keys = keys_at(&value, &[]);
                let text_leak = text.contains(">tcp") || text.contains(">p/>");
                if !keys.contains(&label.to_string())
                    || text_leak
                    || (label != "tcp" && !keys.contains(&"tcp".to_string()))
                {
                    failures.push(format!("{marked:?} + {label} → {text:?} (keys {keys:?})"));
                }
            }
            Err(error) => failures.push(format!("{marked:?} + {label} → {error}")),
        }
    }
    report(failures);
}
