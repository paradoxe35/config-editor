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

/// The document after accepting `label` at the caret, with the selection
/// left (anchor, caret) as UTF-16 offsets, or why it could not be.
fn accepted_with_selection(
    language: Language,
    marked: &str,
    label: &str,
) -> Result<(String, usize, usize), String> {
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
    Ok(apply_completion(&source, option))
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

/// `tcp` holding a nested object entry written `entry` (caret inside), whose
/// XML end tag is `end`.
fn nested_forms(language: Language, entry: &str, end: &str) -> Vec<(&'static str, String)> {
    match language.label() {
        "YAML" => vec![
            ("block", format!("tcp:\n  {entry}: true\n")),
            ("flow", format!("tcp: {{{entry}: true}}\n")),
        ],
        "JSON" => vec![(
            "object",
            format!("{{\n  \"tcp\": {{\n    \"{entry}\": true\n  }}\n}}\n"),
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
                format!("<tcp>\n  <{entry}>true</{end}>\n</tcp>\n"),
            ),
        ],
    }
}

/// With the caret inside a key that is not a known one (`k|delay`), a
/// suggestion replaces the whole key and keeps its value, as in VS Code.
#[test]
fn a_suggestion_accepted_inside_an_unknown_key_replaces_the_whole_key() {
    let mut failures = Vec::new();
    for language in ALL {
        for (form, marked) in nested_forms(language, "k|delay", "kdelay") {
            let case = format!("{} {form}: {marked:?}", language.label());
            match accepted(language, &marked, "keepalive_secs").and_then(|text| {
                let value =
                    parsed(language, &text).map_err(|error| format!("{error}: {text:?}"))?;
                Ok((text, value))
            }) {
                Ok((text, value)) => {
                    let keys = keys_at(&value, &["tcp"]);
                    if keys != ["keepalive_secs"] {
                        failures.push(format!("{case} → {text:?} has keys {keys:?}"));
                    }
                }
                Err(error) => failures.push(format!("{case} → {error}")),
            }
        }
    }
    report(failures);
}

/// A container's own name: the caret inside it (`t|cp`) and a suggestion
/// accepted renames it, children kept; typed in front of it (`hea|tcp`), a
/// new entry goes before it. Either way the document still parses.
#[test]
fn a_suggestion_accepted_at_a_container_name_renames_it_or_goes_before_it() {
    let containers = |language: Language, name: &str, end: &str| -> Vec<(&'static str, String)> {
        match language.label() {
            "YAML" => vec![("block", format!("{name}:\n  nodelay: true\n"))],
            "JSON" => vec![(
                "object",
                format!("{{\n  \"{name}\": {{\n    \"nodelay\": true\n  }}\n}}\n"),
            )],
            "TOML" => vec![("inline", format!("{name} = {{ nodelay = true }}\n"))],
            "HCL" => vec![
                ("block", format!("{name} {{\n  nodelay = true\n}}\n")),
                (
                    "labelled",
                    format!("{name} \"web\" {{\n  mode = \"balance\"\n}}\n"),
                ),
            ],
            _ => vec![(
                "element",
                format!(
                    "<burrow config-root=\"true\">\n  <{name}>\n    <nodelay>true</nodelay>\n  </{end}>\n</burrow>\n"
                ),
            )],
        }
    };
    let mut failures = Vec::new();
    for language in ALL {
        // Inside the name: renamed. (JSON keys sit inside quotes, as above.)
        for (form, marked) in containers(language, "t|cp", "tcp") {
            let case = format!("{} {form} rename: {marked:?}", language.label());
            match accepted(language, &marked, "metrics")
                .and_then(|text| parsed(language, &text).map(|value| (text, value)))
            {
                Ok((text, value)) => {
                    let keys = keys_at(&value, &[]);
                    if !(keys.contains(&"metrics".to_string())
                        && !keys.contains(&"tcp".to_string())
                        && !keys.iter().any(|key| key.contains("cp")))
                    {
                        failures.push(format!("{case} → {text:?} keys {keys:?}"));
                    }
                }
                Err(error) => failures.push(format!("{case} → {error}")),
            }
        }
        if language.label() == "JSON" || language.label() == "TOML" {
            continue;
        }
        // In front of the name: a new entry, the container kept.
        for (form, marked) in containers(language, "hea|tcp", "tcp") {
            let case = format!("{} {form} in front: {marked:?}", language.label());
            match accepted(language, &marked, "heartbeat")
                .and_then(|text| parsed(language, &text).map(|value| (text, value)))
            {
                Ok((text, value)) => {
                    let keys = keys_at(&value, &[]);
                    let kept = keys.contains(&"tcp".to_string())
                        || keys.contains(&"services".to_string())
                        || form == "labelled";
                    if !(keys.contains(&"heartbeat".to_string()) && kept && text.contains("tcp")) {
                        failures.push(format!("{case} → {text:?} keys {keys:?}"));
                    }
                }
                Err(error) => failures.push(format!("{case} → {error}")),
            }
        }
    }
    report(failures);
}

/// The key the caret stands at the start of is already in the document: it
/// is not offered again.
#[test]
fn the_key_at_the_caret_is_not_offered_again() {
    let cases: Vec<(Language, &str)> = vec![
        (Language::YAML, "|tcp:\n  nodelay: true\n"),
        (Language::HCL, "|tcp {\n  nodelay = true\n}\n"),
        (Language::TOML, "|tcp = { nodelay = true }\n"),
        (
            Language::XML,
            "<burrow config-root=\"true\">\n  <|tcp/>\n</burrow>\n",
        ),
    ];
    let mut failures = Vec::new();
    for (language, marked) in cases {
        let labels = offered(language, marked);
        if labels.contains(&"tcp".to_string()) {
            failures.push(format!("{} {marked:?} offers tcp", language.label()));
        }
    }
    report(failures);
}

/// A key typed in front of a TOML `[table]` header goes on its own line,
/// before the table, and the document still parses.
#[test]
fn a_toml_key_typed_in_front_of_a_header_goes_before_it() {
    let text = accepted(Language::TOML, "hea|[tcp]\nnodelay = true\n", "heartbeat").unwrap();
    let value = parsed(Language::TOML, &text).unwrap_or_else(|error| panic!("{error}: {text:?}"));
    assert_eq!(keys_at(&value, &[]), ["heartbeat", "tcp"], "{text:?}");
    assert_eq!(keys_at(&value, &["tcp"]), ["nodelay"], "{text:?}");
    // A scalar waits for its value on its own line.
    let text = accepted(Language::TOML, "lis|[tcp]\nnodelay = true\n", "listen").unwrap();
    assert!(
        text.starts_with("listen = ") && text.contains("\n[tcp]\nnodelay = true"),
        "{text:?}"
    );
}

/// An XML element name typed right before the tag's end (`<hea|>`,
/// `<hea|/>`) gets name suggestions like anywhere else in the name.
#[test]
fn an_xml_name_before_the_tag_end_gets_suggestions() {
    let root = |body: &str| format!("<burrow config-root=\"true\">\n  {body}\n</burrow>\n");
    let mut failures = Vec::new();
    for marked in [root("<hea|/>"), root("<hea|></hea>"), root("<hea|>")] {
        let labels = offered(Language::XML, &marked);
        if !labels.contains(&"heartbeat".to_string()) {
            failures.push(format!("{marked:?} offers {labels:?}"));
        }
    }
    report(failures);
}

/// Accepting a name at the end of a complete tag's name renames the
/// element, end tag included.
#[test]
fn an_xml_name_accepted_at_the_end_of_a_complete_tag_renames_it() {
    let root = |body: &str| format!("<burrow config-root=\"true\">\n  {body}\n</burrow>\n");
    let mut failures = Vec::new();
    for (marked, expected) in [
        (root("<hea|></hea>"), root("<heartbeat></heartbeat>")),
        (root("<hea|/>"), root("<heartbeat/>")),
        (
            root("<hea|>\n    <interval_secs>5</interval_secs>\n  </hea>"),
            root("<heartbeat>\n    <interval_secs>5</interval_secs>\n  </heartbeat>"),
        ),
    ] {
        match accepted(Language::XML, &marked, "heartbeat") {
            Ok(text) if text == expected => {}
            Ok(text) => failures.push(format!("{marked:?} → {text:?}")),
            Err(error) => failures.push(format!("{marked:?} → {error}")),
        }
    }
    report(failures);
}

/// A rename leaves the caret right after the new key, with nothing selected,
/// so the next keystroke cannot replace part of the key.
#[test]
fn a_rename_leaves_the_caret_after_the_new_key() {
    let cases = [
        (
            Language::YAML,
            "limits:\n  max_h|pending: 1\n",
            "max_handshakes",
        ),
        (Language::YAML, "tcp:\n  keep|x: 5\n", "keepalive_secs"),
        (Language::YAML, "tcp: {keep|x: 5}\n", "keepalive_secs"),
        (
            Language::TOML,
            "[limits]\nmax_h|pending = 1\n",
            "max_handshakes",
        ),
        (
            Language::HCL,
            "limits {\n  max_h|pending = 1\n}\n",
            "max_handshakes",
        ),
        (
            Language::XML,
            "<tcp>\n  <keep|>5</keep>\n</tcp>\n",
            "keepalive_secs",
        ),
        (Language::XML, "<tcp keep|x=\"5\"/>\n", "keepalive_secs"),
    ];
    let mut failures = Vec::new();
    for (language, marked, label) in cases {
        match accepted_with_selection(language, marked, label) {
            Ok((text, anchor, caret)) => {
                let before: String = text
                    .encode_utf16()
                    .take(caret)
                    .collect::<Vec<u16>>()
                    .iter()
                    .map(|unit| char::from_u32(u32::from(*unit)).unwrap_or('?'))
                    .collect();
                if anchor != caret || !before.ends_with(label) {
                    failures.push(format!(
                        "{} {marked:?} → {text:?} selection {anchor}..{caret}",
                        language.label()
                    ));
                }
            }
            Err(error) => failures.push(format!("{} {marked:?} → {error}", language.label())),
        }
    }
    report(failures);
}

/// Accepting the key already written, with the caret at its end, changes
/// nothing, whatever follows it (`tcp|.nodelay`, `tcp|: 1`, `<tcp|>`).
#[test]
fn accepting_the_key_written_at_its_end_changes_nothing() {
    let cases = [
        (Language::TOML, "tcp|.nodelay = true\n", "tcp"),
        (Language::TOML, "[tcp]\nnodelay| = true\n", "nodelay"),
        (Language::YAML, "tcp|:\n  nodelay: true\n", "tcp"),
        (Language::HCL, "tcp| {\n  nodelay = true\n}\n", "tcp"),
        (
            Language::XML,
            "<burrow config-root=\"true\">\n  <tcp|/>\n</burrow>\n",
            "tcp",
        ),
    ];
    let mut failures = Vec::new();
    for (language, marked, label) in cases {
        let original = marked.replacen('|', "", 1);
        match accepted(language, marked, label) {
            Ok(text) if text == original => {}
            Ok(text) => failures.push(format!("{} {marked:?} → {text:?}", language.label())),
            Err(error) => failures.push(format!("{} {marked:?} → {error}", language.label())),
        }
    }
    report(failures);
}

/// An XML attribute accepted right before the tag's end keeps the end and
/// gets its `="…"`.
#[test]
fn an_xml_attribute_accepted_before_the_tag_end_is_whole() {
    let mut failures = Vec::new();
    for (marked, expected) in [
        ("<tcp no|>\n</tcp>\n", "<tcp nodelay=\"true\">\n</tcp>\n"),
        ("<tcp no|></tcp>\n", "<tcp nodelay=\"true\"></tcp>\n"),
        ("<tcp no|/>\n", "<tcp nodelay=\"true\"/>\n"),
    ] {
        match accepted(Language::XML, marked, "nodelay") {
            Ok(text) if text.replace("\"\"", "\"true\"") == expected || text == expected => {}
            Ok(text) => failures.push(format!("{marked:?} → {text:?}")),
            Err(error) => failures.push(format!("{marked:?} → {error}")),
        }
    }
    report(failures);
}

/// JSON types in front of a key like the other languages: at a key's
/// opening quote (`"|tcp"`) or before it (`hea|"tcp"`), a new entry goes
/// before it on its own line, and the key is not offered.
#[test]
fn a_json_key_typed_in_front_of_a_key_goes_before_it_on_its_own_line() {
    let mut failures = Vec::new();
    let marked = "{\n  \"|tcp\": {}\n}\n";
    if offered(Language::JSON, marked).contains(&"tcp".to_string()) {
        failures.push(format!("{marked:?} offers tcp"));
    }
    for (marked, label, expected) in [
        (
            "{\n  \"|tcp\": {}\n}\n",
            "clients",
            "{\n  \"clients\": {},\n  \"tcp\": {}\n}\n",
        ),
        (
            "{\n  hea|\"tcp\": {}\n}\n",
            "heartbeat",
            "{\n  \"heartbeat\": {},\n  \"tcp\": {}\n}\n",
        ),
    ] {
        match accepted(Language::JSON, marked, label) {
            Ok(text)
                if parsed(Language::JSON, &text).is_ok()
                    && text.contains(&expected[4..expected.find(",").unwrap()])
                    && text.contains(",\n  \"tcp\"") => {}
            Ok(text) => failures.push(format!("{marked:?} → {text:?}")),
            Err(error) => failures.push(format!("{marked:?} → {error}")),
        }
    }
    report(failures);
}

/// After a rename the list does not reopen on the renamed key (it would
/// take the next Enter).
#[test]
fn a_rename_does_not_reopen_the_list() {
    use super::super::completion::accept_reopens_suggestions;
    let mut failures = Vec::new();
    for (language, marked, label) in [
        (Language::YAML, "t|cp:\n  nodelay: true\n", "metrics"),
        (Language::HCL, "t|cp {\n  nodelay = true\n}\n", "metrics"),
        (Language::TOML, "t|cp = { nodelay = true }\n", "metrics"),
        (
            Language::XML,
            "<burrow config-root=\"true\">\n  <t|cp/>\n</burrow>\n",
            "metrics",
        ),
        (Language::TOML, "tcp|.nodelay = true\n", "tcp"),
    ] {
        let at = marked.find('|').unwrap();
        let source = marked.replacen('|', "", 1);
        let options = completions(
            &source,
            source[..at].encode_utf16().count(),
            true,
            Some(&burrow()),
            true,
            language.behavior(),
        );
        match options.iter().find(|option| option.label == label) {
            Some(option) if !accept_reopens_suggestions(option, true) => {}
            Some(_) => failures.push(format!("{} {marked:?}: the list reopens", language.label())),
            None => failures.push(format!(
                "{} {marked:?}: {label} not offered",
                language.label()
            )),
        }
    }
    report(failures);
}

/// Round 10: typing in front of a key inserts before it in every form,
/// including text typed after a JSON key's opening quote and TOML table
/// headers (`[ud|tcp]`).
#[test]
fn typing_in_front_of_a_key_inserts_before_it_in_every_form() {
    let mut failures = Vec::new();
    for (language, marked, label, keys) in [
        (
            Language::JSON,
            "{\"listen\": 7835, \"ud|tcp\": {\"nodelay\": true}}\n",
            "udp",
            vec!["listen", "udp", "tcp"],
        ),
        (
            Language::JSON,
            "{\"listen\": 7835, \"|tcp\": {\"nodelay\": true}}\n",
            "udp",
            vec!["listen", "udp", "tcp"],
        ),
        (
            Language::TOML,
            "listen = 7835\n[ud|tcp]\nnodelay = true\n",
            "udp",
            vec!["listen", "udp", "tcp"],
        ),
        (
            Language::TOML,
            "listen = 7835\n[|tcp]\nnodelay = true\n",
            "udp",
            vec!["listen", "udp", "tcp"],
        ),
    ] {
        match accepted(language, marked, label)
            .and_then(|text| parsed(language, &text).map(|value| (text, value)))
        {
            Ok((text, value)) => {
                let mut found = keys_at(&value, &[]);
                found.sort();
                let mut wanted: Vec<String> = keys.iter().map(|key| key.to_string()).collect();
                wanted.sort();
                if found != wanted || keys_at(&value, &["tcp"]) != ["nodelay"] {
                    failures.push(format!("{} {marked:?} → {text:?}", language.label()));
                }
            }
            Err(error) => failures.push(format!("{} {marked:?} → {error}", language.label())),
        }
    }
    report(failures);
}

/// Round 10: with the caret inside or before a line's indentation, a
/// suggestion goes where the line's text starts, as at the end of the
/// indentation.
#[test]
fn a_suggestion_accepted_in_the_indentation_goes_where_the_text_starts() {
    let mut failures = Vec::new();
    for (language, marked, label) in [
        (Language::YAML, "tcp:\n|  nodelay: true\n", "keepalive_secs"),
        (Language::YAML, "tcp:\n | nodelay: true\n", "keepalive_secs"),
        (
            Language::HCL,
            "tcp {\n|  nodelay = true\n}\n",
            "keepalive_secs",
        ),
        (
            Language::HCL,
            "tcp {\n | nodelay = true\n}\n",
            "keepalive_secs",
        ),
        (Language::TOML, "[tcp]\n|nodelay = true\n", "keepalive_secs"),
        (
            Language::JSON,
            "{\n  \"tcp\": {\n|    \"nodelay\": true\n  }\n}\n",
            "keepalive_secs",
        ),
        (
            Language::XML,
            "<tcp>\n|  <nodelay>true</nodelay>\n</tcp>\n",
            "keepalive_secs",
        ),
    ] {
        // Not offering the key already on the line.
        if offered(language, marked).contains(&"nodelay".to_string()) {
            failures.push(format!("{} {marked:?} offers nodelay", language.label()));
        }
        match accepted(language, marked, label)
            .and_then(|text| parsed(language, &text).map(|value| (text, value)))
        {
            Ok((text, value)) => {
                let mut keys = keys_at(&value, &["tcp"]);
                keys.sort();
                // The new line takes the indentation of the line it goes before.
                let indented = text
                    .lines()
                    .filter(|line| line.contains("keepalive_secs") || line.contains("nodelay"))
                    .map(|line| line.len() - line.trim_start().len())
                    .collect::<Vec<_>>();
                if keys != ["keepalive_secs", "nodelay"]
                    || indented.windows(2).any(|pair| pair[0] != pair[1])
                {
                    failures.push(format!("{} {marked:?} → {text:?}", language.label()));
                }
            }
            Err(error) => failures.push(format!("{} {marked:?} → {error}", language.label())),
        }
    }
    report(failures);
}

/// Round 10: the head of a TOML dotted key completed in place keeps the rest
/// of the key and its value (`ud|.nodelay = true` + udp → `udp.nodelay`).
#[test]
fn a_toml_dotted_key_head_completes_in_place() {
    let text = accepted(
        Language::TOML,
        "ud|.nodelay = true\ntcp.keepalive_secs = 3\n",
        "udp",
    )
    .unwrap();
    assert_eq!(text, "udp.nodelay = true\ntcp.keepalive_secs = 3\n");
}

/// Round 10: a TOML table renamed by a completion takes its sub-tables and
/// dotted keys along, so its data stays one table.
#[test]
fn a_toml_table_renamed_by_completion_renames_its_sub_tables_and_dotted_keys() {
    let marked = "[t|cp]\nnodelay = true\n[tcp.x]\na = 1\n";
    let text = accepted(Language::TOML, marked, "metrics").unwrap();
    assert!(!text.contains("tcp"), "{marked:?} → {text:?}");
    assert!(parsed(Language::TOML, &text).is_ok(), "{text:?}");
    let text = accepted(
        Language::TOML,
        "[t|cp]\nnodelay = true\n[tcp.x]\na = 1\n",
        "metrics",
    )
    .unwrap();
    assert_eq!(text, "[metrics]\nnodelay = true\n[metrics.x]\na = 1\n");
}

/// Round 10: an XML attribute is not offered when the element already has
/// that key as a child element, and the reverse.
#[test]
fn an_xml_key_is_one_key_whether_attribute_or_child() {
    let attributes = offered(
        Language::XML,
        "<tcp |>\n  <nodelay>true</nodelay>\n</tcp>\n",
    );
    assert!(
        !attributes.contains(&"nodelay".to_string()),
        "{attributes:?}"
    );
    assert!(
        attributes.contains(&"keepalive_secs".to_string()),
        "{attributes:?}"
    );
    let children = offered(Language::XML, "<tcp nodelay=\"true\">\n  <|\n</tcp>\n");
    assert!(!children.contains(&"nodelay".to_string()), "{children:?}");
}

/// Round 10: Ctrl+Space over a selected key offers every key, and accepting
/// one renames the key, in every language.
#[test]
fn ctrl_space_over_a_selected_key_offers_every_key_and_renames_it() {
    use super::super::completion::{apply_completion, completions_for};
    let mut failures = Vec::new();
    for (language, text, key) in [
        (Language::YAML, "tcp:\n  nodelay: true\n", "tcp"),
        (Language::JSON, "{\"tcp\": {\"nodelay\": true}}\n", "tcp"),
        (Language::TOML, "[tcp]\nnodelay = true\n", "tcp"),
        (Language::HCL, "tcp {\n  nodelay = true\n}\n", "tcp"),
        (
            Language::XML,
            "<burrow config-root=\"true\">\n  <tcp/>\n</burrow>\n",
            "tcp",
        ),
    ] {
        let end = text.find(key).unwrap() + key.len();
        let options = completions_for(
            text,
            end,
            true,
            Some(&burrow()),
            true,
            language.behavior(),
            true,
        );
        let Some(option) = options.iter().find(|option| option.label == "metrics") else {
            failures.push(format!(
                "{}: metrics not offered: {:?}",
                language.label(),
                options.iter().map(|o| &o.label).collect::<Vec<_>>()
            ));
            continue;
        };
        let renamed = apply_completion(text, option).0;
        if renamed.contains("tcp") || parsed(language, &renamed).is_err() {
            failures.push(format!("{}: {renamed:?}", language.label()));
        }
    }
    report(failures);
}

/// Round 10: completing in an XML end tag's name renames the element, its
/// start tag included.
#[test]
fn an_xml_end_tag_name_completes_and_renames_both_tags() {
    let marked = "<burrow config-root=\"true\">\n  <publ>x</pu|bl>\n</burrow>\n";
    let text = accepted(Language::XML, marked, "public_address").unwrap();
    assert_eq!(
        text,
        "<burrow config-root=\"true\">\n  <public_address>x</public_address>\n</burrow>\n"
    );
}
