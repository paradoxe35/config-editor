//! Schema problems: one marker per problem, placed on the key or value at
//! fault, and found in time linear in the document.

use super::burrow;
use crate::editor::languages::Language;
use crate::editor::paths::FieldPath;

#[test]
fn every_unknown_key_gets_its_own_marker() {
    use crate::editor::diagnostics::validate;
    let bundle = burrow();
    let source = "{\"listen\": 1, \"foo\": 1, \"bar\": 2, \"baz\": 3}";
    let columns: Vec<Option<usize>> =
        validate(source, Some(&bundle), None, true, Language::JSON.behavior())
            .into_iter()
            .map(|diagnostic| diagnostic.column)
            .collect();
    assert_eq!(columns.len(), 3, "{columns:?}");
    let mut unique = columns.clone();
    unique.dedup();
    assert_eq!(unique.len(), 3, "markers on different keys: {columns:?}");
}

#[test]
fn errors_inside_inline_containers_point_at_the_value() {
    let schema = crate::Schema::compile(include_str!(
        "../../../../tests/fixtures/tunnel.schema.json"
    ))
    .unwrap();
    let place = |language: Language, source: &str| -> (usize, String) {
        let problems = schema.validate(language, source);
        assert_eq!(problems.len(), 1, "{language}: {problems:?}");
        let problem = &problems[0];
        let line = source.lines().nth(problem.line.unwrap() - 1).unwrap();
        let marked: String = line
            .chars()
            .skip(problem.column.unwrap() - 1)
            .take(problem.end_column.unwrap() - problem.column.unwrap())
            .collect();
        (problem.line.unwrap(), marked)
    };
    assert_eq!(
        place(Language::YAML, "self: {allow: [a, 5]}\n"),
        (1, "5".into())
    );
    assert_eq!(
        place(
            Language::YAML,
            "web:\n  certificates:\n    - {hosts: [a], cert: c, key: k, bogus: 1}\n"
        ),
        (3, "bogus".into())
    );
    assert_eq!(
        place(Language::YAML, "websocket: {listen: 1, challenge: ftp}\n"),
        (1, "ftp".into())
    );
    assert_eq!(
        place(Language::YAML, "self:\n  allow:\n    - a\n    - 5\n"),
        (4, "5".into())
    );
    assert_eq!(
        place(
            Language::TOML,
            "[web]\ncertificates = [\n  { hosts = [\"a\"], cert = \"c\", key = \"k\", bogus = 1 },\n]\n"
        ),
        (3, "bogus".into())
    );
    assert_eq!(
        place(Language::TOML, "[self]\nallow = [\"a\", 1]\n"),
        (2, "1".into())
    );
    assert_eq!(
        place(Language::TOML, "self = { allow = [\"a\", 5] }\n"),
        (1, "5".into())
    );
    assert_eq!(
        place(Language::HCL, "self {\n  allow = [\"a\", 5]\n}\n"),
        (2, "5".into())
    );
}

#[test]
fn an_error_in_a_single_hcl_block_points_inside_it() {
    let schema = crate::Schema::compile(include_str!(
        "../../../../tests/fixtures/tunnel.schema.json"
    ))
    .unwrap();
    let problems = schema.validate(
        Language::HCL,
        "web {\n  certificates {\n    hosts = 1\n  }\n}\n",
    );
    assert!(
        problems.iter().any(|problem| problem.line == Some(3)),
        "{problems:?}"
    );
}

#[test]
fn a_wrong_type_value_written_as_a_block_underlines_its_key() {
    let schema = burrow();
    for (source, key) in [
        ("tcp:\n  nodelay:\n    a: 1\n", "nodelay"),
        ("listen:\n  port: 1\n", "listen"),
    ] {
        let problems = crate::editor::diagnostics::validate(
            source,
            Some(&schema),
            None,
            true,
            Language::YAML.behavior(),
        );
        let problem = problems
            .first()
            .unwrap_or_else(|| panic!("no problem in {source:?}"));
        let line = source.lines().nth(problem.line.unwrap() - 1).unwrap();
        let start = problem.column.unwrap() - 1;
        let end = problem.end_column.unwrap() - 1;
        assert_eq!(&line[start..end], key, "{source:?}: {}", problem.message);
    }
}

#[test]
fn a_deep_toml_dotted_unknown_key_is_marked_on_its_segment() {
    use crate::editor::diagnostics::validate;
    let bundle = burrow();
    for (source, line, column) in [
        ("listen = 1\n[limits]\nbogus.deep = 1\n", 3, 1),
        ("listen = 1\nlimits.bogus.deep = 1\n", 2, 8),
    ] {
        let markers: Vec<(Option<usize>, Option<usize>)> =
            validate(source, Some(&bundle), None, true, Language::TOML.behavior())
                .into_iter()
                .map(|diagnostic| (diagnostic.line, diagnostic.column))
                .collect();
        assert_eq!(markers, vec![(Some(line), Some(column))], "{source}");
    }
}

/// Span lookups run per diagnostic and per ancestor of it; on a 2000-line
/// document each grows with the document only (four times the clients
/// about four times as long), in every language.
#[test]
fn span_lookups_grow_with_the_document_only() {
    let document = |count: usize| {
        let mut clients = serde_json::Map::new();
        for index in 0..count {
            clients.insert(
                format!("client-{index}"),
                serde_json::json!({"secret": format!("s{index}"), "max_connections": index % 50}),
            );
        }
        serde_json::json!({
            "clients": clients,
            "web": {"letsencrypt": {"email": "a@b.c", "dns": [
                {"domains": ["a.example", "c.example"], "wait_secs": 60},
                {"domains": ["b.example", "d.example"], "wait_secs": 120}
            ]}}
        })
    };
    let (small, large) = (document(375), document(1500));
    for language in Language::ALL {
        let small = language.serialize(&small).unwrap();
        let large = language.serialize(&large).unwrap();
        assert!(large.lines().count() > 2000, "{language}");
        for pointer in [
            "/web/letsencrypt/dns/1/wait_secs",
            "/web/letsencrypt/dns/1",
            "/web",
            "/missing/key",
        ] {
            // Timed: the lookups read the text with the language's parser.
            let growth =
                crate::editor::test_timing::growth(5, (&small, &large), |text: &String| {
                    let _ = language
                        .behavior()
                        .value_span(text, &FieldPath::from(pointer));
                    let _ = language
                        .behavior()
                        .key_span(text, &FieldPath::from(pointer));
                });
            assert!(growth.ratio() < 10.0, "{language} {pointer}: {growth}");
        }
    }
}
