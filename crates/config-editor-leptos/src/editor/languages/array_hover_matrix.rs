//! Hover docs on an array of objects written as repeated blocks (TOML
//! `[[svc]]`, HCL `svc {`, XML `<svc>`), in every language: the name of
//! the array shows the array's own title and description (not only its
//! item's), each item's keys their own, and an XML container's closing tag
//! the container's, as its opening tag does.

use super::super::completion::{KeyHover, key_hover};
use super::super::diagnostics::{SchemaBundle, build_schema};
use super::Language;

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

/// Each language's document with two `svc` items and a `box` object.
const DOCUMENTS: [(Language, &str); 5] = [
    (
        Language::YAML,
        "svc:\n  - name: a\n    port: 1\n  - name: b\n    port: 2\nbox:\n  size: 3\n",
    ),
    (
        Language::JSON,
        "{\n  \"svc\": [\n    { \"name\": \"a\", \"port\": 1 },\n    { \"name\": \"b\", \"port\": 2 }\n  ],\n  \"box\": {\n    \"size\": 3\n  }\n}\n",
    ),
    (
        Language::TOML,
        "[[svc]]\nname = \"a\"\nport = 1\n\n[[svc]]\nname = \"b\"\nport = 2\n\n[box]\nsize = 3\n",
    ),
    (
        Language::HCL,
        "svc {\n  name = \"a\"\n  port = 1\n}\nsvc {\n  name = \"b\"\n  port = 2\n}\nbox {\n  size = 3\n}\n",
    ),
    (
        Language::XML,
        "<svc>\n  <name>a</name>\n  <port>1</port>\n</svc>\n<svc>\n  <name>b</name>\n  <port>2</port>\n</svc>\n<box>\n  <size>3</size>\n</box>\n",
    ),
];

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

/// QA 24 (L24-U3): TOML `[[svc]]`, HCL `svc {` and XML `<svc>` showed the
/// item's schema alone (no description), so an array's own docs never
/// showed there; YAML and JSON show them on `svc:`.
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
    for (language, source) in DOCUMENTS {
        // Each `svc` that opens an item (not XML's closing tags).
        let opening = source.matches("svc").count() - source.matches("/svc").count();
        let needle = if language == Language::XML {
            "<svc"
        } else {
            "svc"
        };
        for nth in 0..opening {
            let hover = hover_at(language, source, needle, nth, &bundle);
            let docs = hover.as_ref().and_then(|hover| hover.docs_url.clone());
            let shown = described(hover);
            if shown != svc("Services list") || docs.as_deref() != Some("https://example.com/svc") {
                failures.push(format!("{language:?} svc #{nth}: {shown:?} {docs:?}"));
            }
        }
        for (needle, description) in [("name", "Service name"), ("port", "Service port")] {
            for nth in [0, 1] {
                let shown = described(hover_at(language, source, needle, nth, &bundle));
                let wanted = Some((needle.to_string(), None, Some(description.to_string())));
                if shown != wanted {
                    failures.push(format!("{language:?} {needle} #{nth}: {shown:?}"));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// QA 24 (L24-U3): an XML container's closing tag (`</svc>`, `</box>`)
/// showed nothing; it shows the container's docs, as its opening tag.
#[test]
fn an_xml_containers_closing_tag_shows_its_docs() {
    let bundle = schema();
    let (_, source) = DOCUMENTS[4];
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
