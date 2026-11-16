//! The suggestion list's promises, checked in every language and form (block,
//! inline, XML attributes and elements) against the tunnel schema, so a fix
//! for one language cannot leave its siblings behind: each case on a document
//! every language that suggests keys writes itself, and on forms written by
//! hand. Each test lists every failing case at once. Places only one
//! language's syntax has are in that language's `completion_tests`.
//!
//! `|` marks the caret.

use super::super::completion::completions;
use super::Language;
use super::harness::completion::{
    accepted, accepted_with_selection, burrow, keys_at, offered, parsed,
};
use super::harness::{At, completing, report, write};
use serde_json::{Value, json};

/// `document` as every language that suggests keys writes it, over lines and
/// with the value at `one_line` on one line, the caret `at`: each case's
/// language, form and marked text. A language that cannot place the caret
/// is a failure.
fn every_form(
    document: &Value,
    one_line: &str,
    at: At<'_>,
    failures: &mut Vec<String>,
) -> Vec<(Language, &'static str, String)> {
    let mut cases = Vec::new();
    for language in completing() {
        for (form, one_line) in [("written", &[][..]), ("one line", &[one_line][..])] {
            match write(language, document, one_line, at) {
                Ok(written) => cases.push((language, form, written.marked())),
                Err(error) => failures.push(format!("{form} {at:?}: {error}")),
            }
        }
    }
    cases
}

/// `tcp` holding `nodelay = true`.
fn tcp() -> Value {
    json!({"tcp": {"nodelay": true}})
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
    let mut cases = Vec::new();
    for language in ALL
        .into_iter()
        .filter(|language| language.label() != "JSON")
    {
        for (form, marked) in tcp_forms(language, "k|nodelay") {
            cases.push((language, form, marked));
        }
    }
    let at = At::BeforeKey {
        pointer: "/tcp/nodelay",
        typed: "k",
    };
    // Not inside a key's quotes (see above).
    cases.extend(
        every_form(&tcp(), "/tcp", at, &mut failures)
            .into_iter()
            .filter(|(_, _, marked)| !marked.contains("\"k|")),
    );
    for (language, form, marked) in cases {
        let case = format!("{} {form}: {marked:?}", language.label());
        match accepted(language, &marked, "keepalive_secs").and_then(|text| {
            let value = parsed(language, &text).map_err(|error| format!("{error}: {text:?}"))?;
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
    report(&failures);
}

/// The key the caret is inside is offered (it is not a sibling of itself),
/// and accepting it changes nothing.
#[test]
fn the_key_being_edited_is_offered_and_accepting_it_changes_nothing() {
    let mut failures = Vec::new();
    let mut cases = Vec::new();
    for language in ALL {
        for (form, marked) in tcp_forms(language, "nodela|y") {
            cases.push((language, form, marked));
        }
    }
    let at = At::InKey {
        pointer: "/tcp/nodelay",
        offset: 6,
    };
    cases.extend(every_form(&tcp(), "/tcp", at, &mut failures));
    for (language, form, marked) in cases {
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
    report(&failures);
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
    let mut cases: Vec<(Language, String)> = cases
        .into_iter()
        .map(|(language, marked)| (language, marked.to_string()))
        .collect();
    let at = At::NewKey {
        object: "/tcp",
        typed: "",
    };
    for (language, _, marked) in every_form(&tcp(), "/tcp", at, &mut failures) {
        cases.push((language, marked));
    }
    for (language, marked) in &cases {
        let (language, marked) = (*language, marked.as_str());
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
    report(&failures);
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
    let mut cases = Vec::new();
    for language in ALL {
        for (form, marked) in nested_forms(language, "k|delay", "kdelay") {
            cases.push((language, form, marked));
        }
    }
    let at = At::InKey {
        pointer: "/tcp/kdelay",
        offset: 1,
    };
    let document = json!({"tcp": {"kdelay": true}});
    cases.extend(every_form(&document, "/tcp", at, &mut failures));
    for (language, form, marked) in cases {
        let case = format!("{} {form}: {marked:?}", language.label());
        match accepted(language, &marked, "keepalive_secs").and_then(|text| {
            let value = parsed(language, &text).map_err(|error| format!("{error}: {text:?}"))?;
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
    report(&failures);
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
    // As each language writes the container: renamed from inside its name,
    // a new entry before it from in front (not inside a key's quotes).
    let rename = At::InKey {
        pointer: "/tcp",
        offset: 1,
    };
    for (language, form, marked) in every_form(&tcp(), "/tcp", rename, &mut failures) {
        match accepted(language, &marked, "metrics")
            .and_then(|text| parsed(language, &text).map(|value| (text, value)))
        {
            Ok((_, value)) if keys_at(&value, &[]) == ["metrics"] => {}
            other => failures.push(format!("{language} {form} rename {marked:?}: {other:?}")),
        }
    }
    let in_front = At::BeforeKey {
        pointer: "/tcp",
        typed: "hea",
    };
    for (language, form, marked) in every_form(&tcp(), "/tcp", in_front, &mut failures) {
        if marked.contains("\"hea|") {
            continue;
        }
        match accepted(language, &marked, "heartbeat")
            .and_then(|text| parsed(language, &text).map(|value| (text, value)))
        {
            Ok((_, value))
                if keys_at(&value, &[]).contains(&"heartbeat".to_string())
                    && keys_at(&value, &["tcp"]) == ["nodelay"] => {}
            other => failures.push(format!("{language} {form} in front {marked:?}: {other:?}")),
        }
    }
    report(&failures);
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
    let mut cases: Vec<(Language, String)> = cases
        .into_iter()
        .map(|(language, marked)| (language, marked.to_string()))
        .collect();
    let at = At::InKey {
        pointer: "/tcp",
        offset: 0,
    };
    for (language, _, marked) in every_form(&tcp(), "/tcp", at, &mut failures) {
        cases.push((language, marked));
    }
    for (language, marked) in &cases {
        let (language, marked) = (*language, marked.as_str());
        let labels = offered(language, marked);
        if labels.contains(&"tcp".to_string()) {
            failures.push(format!("{} {marked:?} offers tcp", language.label()));
        }
    }
    report(&failures);
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
    let mut cases: Vec<(Language, String, &str)> = cases
        .into_iter()
        .map(|(language, marked, label)| (language, marked.to_string(), label))
        .collect();
    for (document, container, pointer, offset, label) in [
        (
            json!({"limits": {"max_hpending": 1}}),
            "/limits",
            "/limits/max_hpending",
            5,
            "max_handshakes",
        ),
        (
            json!({"tcp": {"keepx": 5}}),
            "/tcp",
            "/tcp/keepx",
            4,
            "keepalive_secs",
        ),
    ] {
        let at = At::InKey { pointer, offset };
        for (language, _, marked) in every_form(&document, container, at, &mut failures) {
            cases.push((language, marked, label));
        }
    }
    for (language, marked, label) in &cases {
        let (language, marked, label) = (*language, marked.as_str(), *label);
        match accepted_with_selection(language, marked, label) {
            Ok((text, anchor, caret)) => {
                let before: String = text
                    .encode_utf16()
                    .take(caret)
                    .collect::<Vec<u16>>()
                    .iter()
                    .map(|unit| char::from_u32(u32::from(*unit)).unwrap_or('?'))
                    .collect();
                // After the key as written: a quoted key's closing quote too.
                let key_end = before.strip_suffix('"').unwrap_or(&before);
                if anchor != caret || !key_end.ends_with(label) {
                    failures.push(format!(
                        "{} {marked:?} → {text:?} selection {anchor}..{caret}",
                        language.label()
                    ));
                }
            }
            Err(error) => failures.push(format!("{} {marked:?} → {error}", language.label())),
        }
    }
    report(&failures);
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
    let mut cases: Vec<(Language, String, &str)> = cases
        .into_iter()
        .map(|(language, marked, label)| (language, marked.to_string(), label))
        .collect();
    for (pointer, label) in [("/tcp", "tcp"), ("/tcp/nodelay", "nodelay")] {
        for (language, _, marked) in every_form(&tcp(), "/tcp", At::KeyEnd(pointer), &mut failures)
        {
            cases.push((language, marked, label));
        }
    }
    for (language, marked, label) in &cases {
        let (language, marked, label) = (*language, marked.as_str(), *label);
        let original = marked.replacen('|', "", 1);
        match accepted(language, marked, label) {
            Ok(text) if text == original => {}
            Ok(text) => failures.push(format!("{} {marked:?} → {text:?}", language.label())),
            Err(error) => failures.push(format!("{} {marked:?} → {error}", language.label())),
        }
    }
    report(&failures);
}

/// The shared suggestion cases run in Dot, a language as a JavaScript host
/// declares it without `contextAt` (its caret read from the brackets,
/// item separator and item marker it declares, as the bridge reads one),
/// so each case here, such as the list not reopening after a rename and
/// the keys offered in a repeated item, checks that reading too.
#[test]
fn the_suggestion_cases_run_in_a_language_declared_from_javascript() {
    let labels: Vec<&str> = completing()
        .iter()
        .map(|language| language.label())
        .collect();
    assert!(labels.contains(&"Dot"), "{labels:?}");
}

/// After a rename the list does not reopen on the renamed key (it would
/// take the next Enter).
#[test]
fn a_rename_does_not_reopen_the_list() {
    use super::super::completion::accept_reopens_suggestions;
    let mut failures = Vec::new();
    let at = At::InKey {
        pointer: "/tcp",
        offset: 1,
    };
    let written: Vec<(Language, String, &str)> = every_form(&tcp(), "/tcp", at, &mut failures)
        .into_iter()
        .map(|(language, _, marked)| (language, marked, "metrics"))
        .collect();
    let by_hand = [
        (Language::YAML, "t|cp:\n  nodelay: true\n", "metrics"),
        (Language::HCL, "t|cp {\n  nodelay = true\n}\n", "metrics"),
        (Language::TOML, "t|cp = { nodelay = true }\n", "metrics"),
        (
            Language::XML,
            "<burrow config-root=\"true\">\n  <t|cp/>\n</burrow>\n",
            "metrics",
        ),
        (Language::TOML, "tcp|.nodelay = true\n", "tcp"),
    ];
    let cases = by_hand
        .into_iter()
        .map(|(language, marked, label)| (language, marked.to_string(), label))
        .chain(written);
    for (language, marked, label) in cases {
        let marked = marked.as_str();
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
    report(&failures);
}

/// Typing in front of a key inserts before it in every form,
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
    report(&failures);
}

/// With the caret inside or before a line's indentation, a
/// suggestion goes where the line's text starts, as at the end of the
/// indentation.
#[test]
fn a_suggestion_accepted_in_the_indentation_goes_where_the_text_starts() {
    let mut failures = Vec::new();
    let mut written = Vec::new();
    for offset in [0, 1] {
        let at = At::LineOfKey {
            pointer: "/tcp/nodelay",
            offset,
        };
        for language in completing() {
            match write(language, &tcp(), &[], at) {
                // Only where the line has that much indentation.
                Ok(written_at)
                    if written_at.text[written_at.byte..].starts_with(' ') || offset == 0 =>
                {
                    written.push((language, written_at.marked(), "keepalive_secs"));
                }
                Ok(_) => {}
                Err(error) => failures.push(error),
            }
        }
    }
    let by_hand = [
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
    ];
    let cases = by_hand
        .into_iter()
        .map(|(language, marked, label)| (language, marked.to_string(), label))
        .chain(written);
    for (language, marked, label) in cases {
        let marked = marked.as_str();
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
    report(&failures);
}

/// Ctrl+Space over a selected key offers every key, and accepting
/// one renames the key, in every language.
#[test]
fn ctrl_space_over_a_selected_key_offers_every_key_and_renames_it() {
    use super::super::completion::{apply_completion, completions_for};
    let mut failures = Vec::new();
    let written: Vec<(Language, String, &str)> = completing()
        .into_iter()
        .filter_map(|language| {
            Some((
                language,
                super::harness::text(language, &tcp(), &[]).ok()?,
                "tcp",
            ))
        })
        .collect();
    let by_hand = [
        (Language::YAML, "tcp:\n  nodelay: true\n", "tcp"),
        (Language::JSON, "{\"tcp\": {\"nodelay\": true}}\n", "tcp"),
        (Language::TOML, "[tcp]\nnodelay = true\n", "tcp"),
        (Language::HCL, "tcp {\n  nodelay = true\n}\n", "tcp"),
        (
            Language::XML,
            "<burrow config-root=\"true\">\n  <tcp/>\n</burrow>\n",
            "tcp",
        ),
    ];
    let cases = by_hand
        .into_iter()
        .map(|(language, text, key)| (language, text.to_string(), key))
        .chain(written);
    for (language, text, key) in cases {
        let text = text.as_str();
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
    report(&failures);
}

/// Ctrl+Space over a selection covering the start of a key
/// replaces the whole key, in every language and form.
#[test]
fn ctrl_space_over_part_of_a_key_replaces_all_of_it() {
    use super::super::completion::{apply_completion, completions_for};
    let mut failures = Vec::new();
    let websocket = json!({"websocket": {"path": "/x"}});
    let mut written = Vec::new();
    for (pointer, offset, label, rest) in [
        ("/websocket/path", 2, "host", "th"),
        ("/websocket", 3, "metrics", "socket"),
    ] {
        let at = At::InKey { pointer, offset };
        for (language, _, marked) in every_form(&websocket, "/websocket", at, &mut failures) {
            written.push((language, marked, label, rest));
        }
    }
    let by_hand = [
        (Language::YAML, "websocket:\n  pa|th: /x\n", "host", "th"),
        (
            Language::JSON,
            "{\"websocket\": {\"pa|th\": \"/x\"}}\n",
            "host",
            "th",
        ),
        (
            Language::TOML,
            "[websocket]\npa|th = \"/x\"\n",
            "host",
            "th",
        ),
        (
            Language::HCL,
            "websocket {\n  pa|th = \"/x\"\n}\n",
            "host",
            "th",
        ),
        (
            Language::XML,
            "<websocket>\n  <pa|th>/x</path>\n</websocket>\n",
            "host",
            "th",
        ),
        (
            Language::XML,
            "<websocket>\n  <path>/x</pa|th>\n</websocket>\n",
            "host",
            "th",
        ),
        (
            Language::TOML,
            "[web|socket]\npath = \"/x\"\n",
            "metrics",
            "socket",
        ),
        (
            Language::HCL,
            "web|socket {\n  path = \"/x\"\n}\n",
            "metrics",
            "socket",
        ),
        (
            Language::YAML,
            "web|socket:\n  path: /x\n",
            "metrics",
            "socket",
        ),
    ];
    let cases = by_hand
        .into_iter()
        .map(|(language, marked, label, rest)| (language, marked.to_string(), label, rest))
        .chain(written);
    for (language, marked, label, rest) in cases {
        let marked = marked.as_str();
        let at = marked.find('|').unwrap();
        let text = marked.replacen('|', "", 1);
        let options = completions_for(
            &text,
            text[..at].encode_utf16().count(),
            true,
            Some(&burrow()),
            true,
            language.behavior(),
            true,
        );
        let Some(option) = options.iter().find(|option| option.label == label) else {
            failures.push(format!(
                "{}: {label} not offered in {marked:?}",
                language.label()
            ));
            continue;
        };
        let renamed = apply_completion(&text, option).0;
        let split = renamed.contains(&format!("{rest}:"))
            || renamed.contains(&format!("{rest} "))
            || renamed.contains(&format!("<{rest}"))
            || renamed.contains(&format!("[{rest}"))
            || renamed.contains(&format!("\"{rest}\""));
        if split || !renamed.contains(label) || parsed(language, &renamed).is_err() {
            failures.push(format!(
                "{}: {marked:?} became {renamed:?}",
                language.label()
            ));
        }
    }
    report(&failures);
}

/// A key accepted where the caret sits off its siblings' column
/// (deeper than they can be, or between its parent's column and theirs in
/// YAML) is written at their column, with no trailing spaces left.
#[test]
fn a_key_accepted_off_its_siblings_column_lines_up_with_them() {
    let mut failures = Vec::new();
    for (language, marked) in [
        (Language::YAML, "websocket:\n  path: /x\n ho| host2: h\n"),
        (Language::YAML, "websocket:\n  path: /x\n    |\n"),
        (Language::YAML, "websocket:\n  path: /x\n   ho|\n"),
        (
            Language::JSON,
            "{\n  \"websocket\": {\n    \"path\": \"/x\",\n        \"|\n  }\n}\n",
        ),
        (Language::TOML, "[websocket]\npath = \"/x\"\n    |\n"),
        (Language::HCL, "websocket {\n  path = \"/x\"\n      |\n}\n"),
        (
            Language::XML,
            "<websocket>\n  <path>/x</path>\n      |\n</websocket>\n",
        ),
    ] {
        let text = match accepted(language, marked, "host") {
            Ok(text) => text,
            Err(error) => {
                failures.push(format!("{}: {marked:?}: {error}", language.label()));
                continue;
            }
        };
        let indent_of = |word: &str| {
            text.lines()
                .find(|line| line.contains(word))
                .map(|line| line.len() - line.trim_start().len())
        };
        let parses =
            language.label() == "YAML" && marked.contains("host2") && text.contains("host2")
                || parsed(language, &text).is_ok();
        if !parses
            || indent_of("host") != indent_of("path")
            || text
                .lines()
                .any(|line| line.ends_with([' ', '\t']) && !line.trim_end().ends_with(':'))
        {
            failures.push(format!("{}: {marked:?} became {text:?}", language.label()));
        }
    }
    report(&failures);
}

/// A key renamed by completion is renamed everywhere it is
/// written: every dotted key and header through it in TOML, every block of
/// that name in HCL, every element of that name in XML.
#[test]
fn a_key_renamed_by_completion_is_renamed_everywhere_it_is_written() {
    let mut failures = Vec::new();
    for (language, marked, label, old) in [
        (
            Language::TOML,
            "t|cp.nodelay = true\ntcp.keepalive_secs = 1\n",
            "metrics",
            "tcp",
        ),
        (
            Language::TOML,
            "[clien|.a]\nsecret = \"s\"\n\n[clien.b]\nsecret = \"t\"\n",
            "clients",
            "clien.",
        ),
        (
            Language::HCL,
            "clien| \"a\" {\n  secret = \"s\"\n}\nclien \"b\" {\n  secret = \"t\"\n}\n",
            "clients",
            "clien ",
        ),
        (
            Language::HCL,
            "tc|p {\n  nodelay = true\n}\ntcp {\n  nodelay = false\n}\n",
            "metrics",
            "tcp",
        ),
        (
            Language::XML,
            "<clien|><a/></clien>\n<clien><b/></clien>\n",
            "clients",
            "clien>",
        ),
    ] {
        match accepted(language, marked, label) {
            Ok(text) if !text.contains(old) && text.matches(label).count() >= 2 => {}
            other => failures.push(format!("{}: {marked:?} became {other:?}", language.label())),
        }
    }
    report(&failures);
}

/// A key typed in an HCL block written on one line opens the
/// block over lines, each attribute on its own; a header typed in front of
/// a TOML table leaves the caret in the new table; a TOML table made only by
/// its sub-tables can still be named in a header.
#[test]
fn entries_typed_in_one_line_blocks_and_tables_stay_valid() {
    let mut failures = Vec::new();
    for (marked, expected) in [
        (
            "websocket { ho|path = \"/x\" }\n",
            "websocket {\n  host = \"\"\n  path = \"/x\"\n}\n",
        ),
        (
            "websocket { path = \"/x\" ho| }\n",
            "websocket {\n  path = \"/x\"\n  host = \"\"\n}\n",
        ),
        (
            "websocket { path = \"/x\" | }\n",
            "websocket {\n  path = \"/x\"\n  host = \"\"\n}\n",
        ),
    ] {
        match accepted(Language::HCL, marked, "host") {
            Ok(text) if text == expected => {}
            other => failures.push(format!("HCL {marked:?}: {other:?}")),
        }
    }
    match accepted_with_selection(
        Language::TOML,
        "[heart|websocket]\npath = \"/x\"\n",
        "heartbeat",
    ) {
        Ok((text, _, caret))
            if text == "[heartbeat]\n\n[websocket]\npath = \"/x\"\n"
                && caret == "[heartbeat]\n".len() => {}
        other => failures.push(format!("TOML header in front: {other:?}")),
    }
    for marked in [
        "[websock|et]\n\n[websocket.tls]\ncert = \"c\"\n",
        "[websockt|]\n\n[websocket.tls]\ncert = \"c\"\n",
    ] {
        if !offered(Language::TOML, marked).contains(&"websocket".to_string()) {
            failures.push(format!("TOML {marked:?}: websocket not offered"));
        }
    }
    report(&failures);
}
/// Past the most problems shown, one more without a position says
/// that more follow, so a cut list is never taken for the whole one.
#[cfg(feature = "schema-validation")]
#[test]
fn a_cut_problem_list_says_more_follow() {
    let document: serde_json::Map<String, Value> = (0..50)
        .map(|index| (format!("unknown_{index}"), json!(1)))
        .collect();
    let mut failures = Vec::new();
    for language in super::harness::every_language() {
        let text = match super::harness::text(language, &Value::Object(document.clone()), &[]) {
            Ok(text) => text,
            Err(error) => {
                failures.push(error);
                continue;
            }
        };
        let problems = super::super::diagnostics::validate(
            &text,
            Some(&burrow()),
            None,
            true,
            language.behavior(),
        );
        let last = problems.get(20);
        if problems.len() != 21
            || !last.is_some_and(|last| last.line.is_none() && last.message.contains("more follow"))
        {
            failures.push(format!(
                "{language}: {} problems, the last {last:?}",
                problems.len()
            ));
        }
    }
    report(&failures);
}

/// A name completed in a block or end tag renames it; a dotted
/// key typed in front of an existing entry leaves that entry as it was.
#[test]
fn names_complete_in_place_and_entries_in_front_stay() {
    use super::super::completion::{apply_completion, completions_for};
    let mut failures = Vec::new();
    for (language, marked, label, expected) in [
        (
            Language::HCL,
            "heart| { path = \"/x\" }\n",
            "heartbeat",
            "heartbeat { path = \"/x\" }\n",
        ),
        (Language::HCL, "heart| {}\n", "heartbeat", "heartbeat {}\n"),
        (
            Language::XML,
            "<listen>1</|listen>\n",
            "public_address",
            "<public_address>1</public_address>\n",
        ),
        (
            Language::XML,
            "<websocket><path>/x</path></p|websocket>\n",
            "pool",
            "<pool><path>/x</path></pool>\n",
        ),
    ] {
        match accepted(language, marked, label) {
            Ok(text) if text == expected => {}
            other => failures.push(format!("{}: {marked:?} became {other:?}", language.label())),
        }
    }
    for marked in [
        "[websocket]\ntls.|path = \"/x\"\n",
        "[websocket]\ntls.c|path = \"/x\"\n",
    ] {
        match accepted(Language::TOML, marked, "cert") {
            Ok(text) => {
                let value = parsed(Language::TOML, &text);
                let path = value
                    .as_ref()
                    .ok()
                    .and_then(|value| value.pointer("/websocket/path").cloned());
                if path != Some(serde_json::json!("/x")) {
                    failures.push(format!("TOML {marked:?} became {text:?}"));
                }
            }
            Err(error) => failures.push(format!("TOML {marked:?}: {error}")),
        }
    }
    // The namespaced element is closed with its prefix.
    let at = "<a:b>\n  </".len();
    let text = "<a:b>\n  </";
    let options = completions_for(
        text,
        at,
        true,
        Some(&burrow()),
        true,
        Language::XML.behavior(),
        false,
    );
    match options.first() {
        Some(option) if apply_completion(text, option).0 == "<a:b>\n  </a:b>" => {}
        other => failures.push(format!(
            "XML prefix: {:?}",
            other.map(|o| (o.label.clone(), o.insert.clone()))
        )),
    }
    report(&failures);
}

/// Inside a repeated HCL block or XML element the keys are offered
/// as in the first.
#[test]
fn a_repeated_block_or_element_offers_its_keys() {
    let mut failures = Vec::new();
    let at = At::NewKey {
        object: "/websocket/1",
        typed: "",
    };
    let document = json!({"websocket": [{"path": "/x"}, {"path": "/y"}]});
    let written: Vec<(Language, String)> = every_form(&document, "/websocket/1", at, &mut failures)
        .into_iter()
        .map(|(language, _, marked)| (language, marked))
        .collect();
    let by_hand = [
        (
            Language::HCL,
            "websocket {\n  path = \"/x\"\n}\nwebsocket {\n  |\n}\n",
        ),
        (
            Language::XML,
            "<websocket>\n  <path>/x</path>\n</websocket>\n<websocket>\n  |\n</websocket>\n",
        ),
    ];
    let cases = by_hand
        .into_iter()
        .map(|(language, marked)| (language, marked.to_string()))
        .chain(written);
    for (language, marked) in cases {
        let marked = marked.as_str();
        if !offered(language, marked).contains(&"host".to_string()) {
            failures.push(format!(
                "{}: {:?}",
                language.label(),
                offered(language, marked)
            ));
        }
    }
    report(&failures);
}

/// A table key after a dotted head is written inline; a prefixed
/// XML name is renamed keeping its prefix, in both tags; a TOML header with
/// keys is not renamed onto a table other headers make.
#[test]
fn dotted_heads_prefixed_names_and_header_renames_stay_valid() {
    use super::super::completion::{apply_completion, completions_for};
    let mut failures = Vec::new();
    match accepted(Language::TOML, "websocket.t|", "tls") {
        Ok(text)
            if parsed(Language::TOML, &text)
                .is_ok_and(|value| value.pointer("/websocket/tls").is_some()) => {}
        other => failures.push(format!("TOML dotted head: {other:?}")),
    }
    for (marked, label, expected) in [
        (
            "<ns:websocket>\n  <ns:path>/x</ns:path>\n</ns:websocket|>\n",
            "heartbeat",
            "<ns:heartbeat>\n  <ns:path>/x</ns:path>\n</ns:heartbeat>\n",
        ),
        (
            "<ns:websocket|>\n  <ns:path>/x</ns:path>\n</ns:websocket>\n",
            "heartbeat",
            "<ns:heartbeat>\n  <ns:path>/x</ns:path>\n</ns:heartbeat>\n",
        ),
    ] {
        let at = marked.find('|').unwrap();
        let text = marked.replacen('|', "", 1);
        let options = completions_for(
            &text,
            at,
            true,
            Some(&burrow()),
            true,
            Language::XML.behavior(),
            true,
        );
        match options.iter().find(|option| option.label == label) {
            Some(option) if apply_completion(&text, option).0 == expected => {}
            Some(option) => failures.push(format!(
                "XML {marked:?} became {:?}",
                apply_completion(&text, option).0
            )),
            None => failures.push(format!("XML {marked:?}: {label} not offered")),
        }
    }
    let labels = offered(
        Language::TOML,
        "[web|socket]\npath = \"/x\"\n\n[clients.acme]\nsecret = \"s\"\n",
    );
    if labels.contains(&"clients".to_string()) {
        failures.push(format!("TOML rename offers clients: {labels:?}"));
    }
    report(&failures);
}

/// Quoted keys (TOML dotted, YAML single-quoted) are renamed as
/// keys; prefixed XML attributes keep their prefix; a one-line HCL block
/// keeps its spacing; nothing is offered right after an HCL block's `}` or
/// on a YAML explicit key.
#[test]
fn quoted_and_prefixed_names_rename_as_names() {
    use super::super::completion::{apply_completion, completions_for};
    let mut failures = Vec::new();
    for (language, marked, label, check) in [
        (
            Language::TOML,
            "\"websock|\".path = \"/x\"\n",
            "heartbeat",
            "\"heartbeat\".path = \"/x\"\n",
        ),
        (
            Language::TOML,
            "'websock|'.path = \"/x\"\n",
            "heartbeat",
            "'heartbeat'.path = \"/x\"\n",
        ),
        (
            Language::YAML,
            "'websock|':\n  path: /x\n",
            "heartbeat",
            "'heartbeat':\n  path: /x\n",
        ),
        (
            Language::YAML,
            "websocket: {'pa|': /x}\n",
            "host",
            "websocket: {'host': /x}\n",
        ),
        (
            Language::XML,
            "<websocket ns:pa|=\"/x\"/>\n",
            "host",
            "<websocket ns:host=\"/x\"/>\n",
        ),
        (
            Language::HCL,
            "websocket { pa| = \"/x\" }\n",
            "host",
            "websocket { host = \"/x\" }\n",
        ),
    ] {
        let at = marked.find('|').unwrap();
        let text = marked.replacen('|', "", 1);
        let word_start = text[..at]
            .rfind(|character: char| !(character.is_alphanumeric() || character == '_'))
            .map_or(0, |index| index + 1);
        let word_end = at
            + text[at..]
                .find(|character: char| !(character.is_alphanumeric() || character == '_'))
                .unwrap_or(0);
        // Ctrl+Space over the selected name: the caret at its end.
        let _ = word_start;
        let options = completions_for(
            &text,
            word_end,
            true,
            Some(&burrow()),
            true,
            language.behavior(),
            true,
        );
        match options.iter().find(|option| option.label == label) {
            Some(option) if apply_completion(&text, option).0 == check => {}
            Some(option) => failures.push(format!(
                "{}: {marked:?} became {:?}",
                language.label(),
                apply_completion(&text, option).0
            )),
            None => failures.push(format!(
                "{}: {marked:?}: {label} not offered",
                language.label()
            )),
        }
    }
    for (language, marked) in [
        (Language::HCL, "websocket {\n  path = \"/x\"\n}|"),
        (Language::HCL, "websocket { path = \"/x\" }|"),
        (Language::YAML, "? websock|\n: path: /x\n"),
    ] {
        let labels = offered(language, marked);
        if !labels.is_empty() {
            failures.push(format!(
                "{}: {marked:?} offered {labels:?}",
                language.label()
            ));
        }
    }
    report(&failures);
}
