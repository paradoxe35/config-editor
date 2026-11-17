//! Hover docs on an array of objects written as repeated blocks (TOML
//! `[[svc]]`, HCL `svc {`, XML `<svc>`), in every language: the name of
//! the array shows the array's own title and description (not only its
//! item's), each item's keys their own, and an XML container's closing tag
//! the container's, as its opening tag does.

use super::super::completion::{KeyHover, key_hover};
use super::super::schema::{SchemaBundle, build_schema};
use super::Language;
use super::harness::{hovering, key_place, report, text};
use serde_json::{Value, json};

fn schema() -> SchemaBundle {
    build_schema(
        r#"{"type": "object", "properties": {
            "svc": {"type": "array", "title": "Services", "description": "Services list",
                "x-docs-url": "https://example.com/svc",
                "items": {"type": "object", "properties": {
                    "name": {"type": "string", "description": "Service name"},
                    "port": {"type": "integer", "description": "Service port"}
                }}},
            "box": {"type": "object", "description": "A box",
                "properties": {"size": {"type": "integer", "description": "Its size"}}}
        }}"#,
    )
    .unwrap()
}

/// Two `svc` items and a `box` object.
fn document() -> Value {
    json!({
        "svc": [{"name": "a", "port": 1}, {"name": "b", "port": 2}],
        "box": {"size": 3}
    })
}

/// The XML document of [`document`], as written by hand.
const XML_DOCUMENT: &str = "<svc>\n  <name>a</name>\n  <port>1</port>\n</svc>\n<svc>\n  <name>b</name>\n  <port>2</port>\n</svc>\n<box>\n  <size>3</size>\n</box>\n";

/// The hover over the `nth` (zero-based) `needle` in `source`, one
/// character into it.
fn hover_at(
    language: Language,
    source: &str,
    needle: &str,
    nth: usize,
    bundle: &SchemaBundle,
) -> Option<KeyHover> {
    let (at, _) = source.match_indices(needle).nth(nth)?;
    let line = source[..at].matches('\n').count();
    let line_start = source[..at].rfind('\n').map_or(0, |index| index + 1);
    let column = source[line_start..at].chars().count() + 1;
    key_hover(source, line, column, Some(bundle), language.behavior())
}

fn described(hover: Option<KeyHover>) -> Option<(String, Option<String>, Option<String>)> {
    hover.map(|hover| (hover.key, hover.title, hover.description))
}

/// The array's own docs show on its name wherever it is written: once
/// (YAML, JSON), or on each item (TOML `[[svc]]`, HCL `svc {`, XML
/// `<svc>`), not the item's schema alone. The items over lines and on one
/// line each.
#[test]
fn an_array_of_blocks_shows_its_own_docs_on_every_item() {
    let bundle = schema();
    let mut failures = Vec::new();
    let svc = |description: &str| {
        Some((
            "svc".to_string(),
            Some("Services".to_string()),
            Some(description.to_string()),
        ))
    };
    let mut sources = Vec::new();
    for language in hovering() {
        for one_line in [&[][..], &["/svc/0", "/svc/1"][..]] {
            match text(language, &document(), one_line) {
                Ok(text) => sources.push((language, text)),
                Err(error) => failures.push(error),
            }
        }
    }
    for (language, source) in &sources {
        let (language, source) = (*language, source.as_str());
        // Each `svc` written, not in a closing tag: one character in.
        let names: Vec<usize> = source
            .match_indices("svc")
            .map(|(at, _)| at)
            .filter(|at| !source[..*at].ends_with('/'))
            .collect();
        for (nth, at) in names.into_iter().enumerate() {
            let line = source[..at].matches('\n').count();
            let line_start = source[..at].rfind('\n').map_or(0, |index| index + 1);
            let column = source[line_start..at].chars().count() + 1;
            let hover = key_hover(source, line, column, Some(&bundle), language.behavior());
            let docs = hover.as_ref().and_then(|hover| hover.docs_url.clone());
            let shown = described(hover);
            if shown != svc("Services list") || docs.as_deref() != Some("https://example.com/svc") {
                failures.push(format!(
                    "{language} svc #{nth}: {shown:?} {docs:?}\n{source}"
                ));
            }
        }
        for (needle, description) in [("name", "Service name"), ("port", "Service port")] {
            for nth in [0, 1] {
                let pointer = format!("/svc/{nth}/{needle}");
                let shown =
                    key_place(language, source, &pointer, 1)
                        .ok()
                        .and_then(|(line, column)| {
                            described(key_hover(
                                source,
                                line,
                                column,
                                Some(&bundle),
                                language.behavior(),
                            ))
                        });
                let wanted = Some((needle.to_string(), None, Some(description.to_string())));
                if shown != wanted {
                    failures.push(format!("{language} {needle} #{nth}: {shown:?}\n{source}"));
                }
            }
        }
    }
    report(&failures);
}

/// An XML container's closing tag (`</svc>`, `</box>`) shows the
/// container's docs, as its opening tag does: markup of XML's own.
#[test]
fn an_xml_containers_closing_tag_shows_its_docs() {
    let bundle = schema();
    let source = XML_DOCUMENT;
    let mut failures = Vec::new();
    for (needle, nth, wanted) in [
        ("/svc", 0, ("svc", "Services list")),
        ("/svc", 1, ("svc", "Services list")),
        ("/box", 0, ("box", "A box")),
        ("/size", 0, ("size", "Its size")),
    ] {
        let shown = hover_at(Language::XML, source, needle, nth, &bundle)
            .map(|hover| (hover.key, hover.description));
        if shown != Some((wanted.0.to_string(), Some(wanted.1.to_string()))) {
            failures.push(format!("{needle} #{nth}: {shown:?}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// Each item, by its key or what opens it (a YAML `- `, a JSON item's `{`,
/// a repeated block's name), shows the list's docs with the item's own
/// under them: the item's title and description, not only its type. What
/// opens an item is each language's own syntax, so the cases are written
/// by hand.
#[test]
fn a_list_item_shows_the_lists_docs_and_its_own() {
    let bundle = build_schema(
        r#"{"type": "object", "properties": {
            "svc": {"type": "array", "description": "Services list",
                "items": {"type": "object", "title": "Service",
                    "description": "One service entry",
                    "properties": {"name": {"type": "string", "description": "Service name"}}}},
            "ports": {"type": "array", "description": "Ports",
                "items": {"type": "integer", "description": "One port"}}
        }}"#,
    )
    .unwrap();
    let documents = [
        (
            Language::YAML,
            "svc:\n  - name: a\n  - name: b\nports:\n  - 80\n  -\n",
            vec![("- name", 0), ("- name", 1)],
            vec![("- 80", 0), ("-\n", 0)],
        ),
        (
            Language::JSON,
            "{\n  \"svc\": [\n    { \"name\": \"a\" },\n    {\n      \"name\": \"b\"\n    }\n  ],\n  \"ports\": [[80]]\n}\n",
            vec![("{ \"name", 0), ("{\n      \"name", 0)],
            vec![],
        ),
        (
            Language::TOML,
            "[[svc]]\nname = \"a\"\n\n[[svc]]\nname = \"b\"\n",
            vec![("svc", 0), ("svc", 1)],
            vec![],
        ),
        (
            Language::HCL,
            "svc {\n  name = \"a\"\n}\nsvc {\n  name = \"b\"\n}\n",
            vec![("svc", 0), ("svc", 1)],
            vec![],
        ),
        (
            Language::XML,
            "<svc>\n  <name>a</name>\n</svc>\n<svc>\n  <name>b</name>\n</svc>\n",
            vec![("<svc", 0), ("<svc", 1)],
            vec![],
        ),
    ];
    // (key, type, description, item title, item description)
    type Shown = (
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
    );
    let shown = |hover: Option<KeyHover>| -> Option<Shown> {
        hover.map(|hover| {
            let item = hover.item.unwrap_or_default();
            (
                hover.key,
                hover.type_label,
                hover.description,
                item.title,
                item.description,
            )
        })
    };
    let wanted = |key: &str, description: &str, title: Option<&str>, own: &str| {
        Some((
            key.to_string(),
            "array".to_string(),
            Some(description.to_string()),
            title.map(str::to_string),
            Some(own.to_string()),
        ))
    };
    let mut failures = Vec::new();
    for (language, source, services, ports) in documents {
        // On the first character of each: the dash, the brace, the key.
        let at = |needle: &str, nth: usize| {
            let (at, _) = source.match_indices(needle).nth(nth).unwrap();
            let line = source[..at].matches('\n').count();
            let line_start = source[..at].rfind('\n').map_or(0, |index| index + 1);
            let column = source[line_start..at].chars().count();
            let column = if needle.starts_with('<') {
                column + 1
            } else {
                column
            };
            key_hover(source, line, column, Some(&bundle), language.behavior())
        };
        for (needle, nth) in services {
            let got = shown(at(needle, nth));
            if got != wanted("svc", "Services list", Some("Service"), "One service entry") {
                failures.push(format!("{language:?} {needle:?} #{nth}: {got:?}"));
            }
        }
        for (needle, nth) in ports {
            let got = shown(at(needle, nth));
            if got != wanted("ports", "Ports", None, "One port") {
                failures.push(format!("{language:?} {needle:?} #{nth}: {got:?}"));
            }
        }
        // A list's own key still shows the list alone; its item's key, its.
        let key = shown(at("name", 0)).map(|(key, _, description, ..)| (key, description));
        if key != Some(("name".to_string(), Some("Service name".to_string()))) {
            failures.push(format!("{language:?} name: {key:?}"));
        }
    }
    // A bracket that opens a list, not an item of one, is no item.
    let json = "{\n  \"ports\": [\n    80\n  ]\n}\n";
    let bracket = json.find('[').unwrap();
    let column = bracket - json[..bracket].rfind('\n').unwrap() - 1;
    let on = key_hover(json, 1, column, Some(&bundle), Language::JSON.behavior());
    if on.as_ref().is_some_and(|hover| hover.item.is_some()) {
        failures.push(format!("JSON list bracket: {:?}", shown(on)));
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// Hovering any character of lists holding wide characters (a secret's
/// `•` placeholder, accents, a no-break space after a dash) finds docs or
/// nothing, in every language, and never stops the editor: the lists as
/// each language writes them, then broken forms written by hand.
#[test]
fn hovering_any_character_of_a_list_with_wide_characters_never_fails() {
    let bundle = schema();
    let wide = json!({"svc": [{"name": "é•••"}, {"name": "•\u{a0}x"}], "box": {"size": 3}});
    for language in hovering() {
        for one_line in [&[][..], &["/svc"][..]] {
            let Ok(source) = text(language, &wide, one_line) else {
                continue;
            };
            for (line, text) in source.split('\n').enumerate() {
                for column in 0..=text.chars().count() {
                    let _ = key_hover(&source, line, column, Some(&bundle), language.behavior());
                }
            }
        }
    }
    let documents = [
        (
            Language::YAML,
            "svc:\n  - name: é•••\n  -\u{a0}name: x\n  -•\n- •\n",
        ),
        (
            Language::JSON,
            "{\"svc\": [{\"name\": \"é•\"}, [\"•\"], {•}]}\n",
        ),
        (
            Language::TOML,
            "svc = [{name = \"é•\"}, [\"•\"]]\n[[svc]]\nname = \"•\"\n",
        ),
        (
            Language::HCL,
            "svc = [{name = \"é•\"}, [\"•\"]]\nsvc {\n  name = \"•\"\n}\n",
        ),
        (
            Language::XML,
            "<svc>\n  <name>é•</name>\n</svc>\n<svc>•</svc>\n",
        ),
    ];
    for (language, source) in documents {
        for (line, text) in source.split('\n').enumerate() {
            for column in 0..=text.chars().count() {
                let _ = key_hover(source, line, column, Some(&bundle), language.behavior());
            }
        }
    }
}
