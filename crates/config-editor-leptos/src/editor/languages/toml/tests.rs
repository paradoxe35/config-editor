use super::super::super::completion::{
    Completion, accept_marked, apply_completion, completions, key_hover,
};
#[cfg(feature = "schema-validation")]
use super::super::super::diagnostics::validate;
use super::super::super::diagnostics::{SchemaBundle, build_schema};
use super::super::super::language::ConfigLanguage;
use super::super::super::highlight::{TokenKind, runs};
use super::{TomlBehavior, highlight};

fn schema() -> SchemaBundle {
    build_schema(
        r#"{"type":"object","properties":{
            "listen":{"type":"integer","description":"Port"},
            "insecure_plaintext":{"type":"boolean"},
            "server":{"type":"object","required":["host"],"properties":{
                "host":{"type":"string","default":"localhost"},
                "port":{"type":"integer","default":7835}}},
            "clients":{"type":"object","additionalProperties":{"type":"object",
                "properties":{"secret":{"type":"string"},"max_connections":{"type":"integer"}},"required":["secret"]}}
        },"additionalProperties":false}"#,
    )
    .unwrap()
}

fn complete(source: &str, schema: &SchemaBundle) -> Vec<Completion> {
    completions(
        source,
        source.encode_utf16().count(),
        false,
        Some(schema),
        true,
        &TomlBehavior,
    )
}

fn nested_schema() -> SchemaBundle {
    build_schema(
        r#"{"type":"object","properties":{"web":{"type":"object","properties":{
            "http":{"type":"integer"},
            "letsencrypt":{"type":"object","required":["email"],
                "properties":{"email":{"type":"string"}}},
            "headers":{"type":"object"}}}}}"#,
    )
    .unwrap()
}

#[test]
fn an_object_accepted_inside_an_inline_table_stays_an_inline_table() {
    let schema = nested_schema();
    let accepted = accept_marked("web = { le| }\n", "letsencrypt", &schema, &TomlBehavior);
    assert_eq!(accepted, "web = { letsencrypt = { email = \"|\" } }\n");
}

#[test]
fn an_object_without_required_children_inside_an_inline_table_opens_empty_braces() {
    let schema = nested_schema();
    let accepted = accept_marked("web = { he| }\n", "headers", &schema, &TomlBehavior);
    assert_eq!(accepted, "web = { headers = { | } }\n");
}

#[test]
fn an_object_accepted_under_a_table_header_opens_the_nested_table_path() {
    let schema = nested_schema();
    let accepted = accept_marked("[web]\nle|", "letsencrypt", &schema, &TomlBehavior);
    assert_eq!(accepted, "[web]\n[web.letsencrypt]\nemail = \"|\"\n");
    let document = TomlBehavior
        .parse(&accepted.replace('|', "a@b.c"))
        .expect("the accepted document parses");
    assert_eq!(document["web"]["letsencrypt"]["email"], "a@b.c");
}

#[test]
fn a_dotted_key_completes_only_its_last_segment() {
    let schema = nested_schema();
    let accepted = accept_marked("[web]\nletsencrypt.em|", "email", &schema, &TomlBehavior);
    assert_eq!(accepted, "[web]\nletsencrypt.email = \"|\"");
}

#[test]
fn duplicate_keys_are_a_parse_error_on_the_repeated_line() {
    let error = TomlBehavior.parse("listen = 1\nlisten = 2\n").unwrap_err();
    assert_eq!(error.line, Some(2));
    assert!(!error.message.is_empty(), "{}", error.message);
}

#[test]
fn syntax_errors_report_a_position() {
    let error = TomlBehavior.parse("listen = \n").unwrap_err();
    assert_eq!(error.line, Some(1));
    assert!(error.column.is_some());
}

#[test]
fn tables_parse_into_nested_values() {
    let document = match TomlBehavior.parse("[server]\nhost = \"example.com\"\n") {
        Ok(document) => document,
        Err(error) => panic!("parse failed: {}", error.message),
    };
    assert_eq!(
        document.get("server").and_then(|server| server.get("host")),
        Some(&serde_json::Value::String("example.com".to_string()))
    );
}

#[test]
fn keys_are_suggested_and_a_partial_key_is_replaced() {
    let source = "lis";
    let options = complete(source, &schema());
    let option = options
        .iter()
        .find(|option| option.label == "listen")
        .expect("listen should be suggested");
    let (updated, _, _) = apply_completion(source, option);
    assert_eq!(updated, "listen = ");
}

#[test]
fn table_members_are_suggested_under_their_header() {
    let source = "[server]\nho";
    let schema_bundle = schema();
    let options = completions(
        source,
        source.encode_utf16().count(),
        false,
        Some(&schema_bundle),
        true,
        &TomlBehavior,
    );
    assert!(
        options.iter().any(|option| option.label == "host"),
        "host should be suggested, have {:?}",
        options
            .iter()
            .map(|option| &option.label)
            .collect::<Vec<_>>()
    );
}

#[test]
fn a_boolean_value_is_suggested_after_its_equals() {
    let source = "insecure_plaintext = ";
    let options = complete(source, &schema());
    let labels: Vec<&str> = options.iter().map(|option| option.label.as_str()).collect();
    assert!(labels.contains(&"true"), "{labels:?}");
    assert!(labels.contains(&"false"), "{labels:?}");
}

#[test]
fn existing_table_keys_are_filtered_from_suggestions() {
    let schema_bundle = schema();
    let source = "[server]\nhost = \"x\"\npo";
    let options = completions(
        source,
        source.encode_utf16().count(),
        false,
        Some(&schema_bundle),
        true,
        &TomlBehavior,
    );
    let labels: Vec<&str> = options.iter().map(|option| option.label.as_str()).collect();
    assert!(!labels.contains(&"host"), "{labels:?}");
    assert!(labels.contains(&"port"), "{labels:?}");
}

#[cfg(feature = "schema-validation")]
#[test]
fn an_error_inside_an_inline_table_is_reported_on_its_own_line() {
    let source = "listen = 7835\n[clients]\nacme = { secret = 1 }\n";
    let diagnostics = validate(source, Some(&schema()), None, true, &TomlBehavior);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, Some(3));
    // Column of the `1` inside the inline table.
    assert_eq!(diagnostics[0].column, Some(19));
}

#[cfg(feature = "schema-validation")]
#[test]
fn an_error_inside_a_nested_inline_table_is_reported_on_its_own_line() {
    let schema_bundle = build_schema(
        r#"{"type":"object","properties":{"outer":{"type":"object","properties":{
            "inner":{"type":"object","properties":{"count":{"type":"integer"}}}}}}}"#,
    )
    .unwrap();
    let source = "outer = { inner = { count = \"x\" } }\n";
    let diagnostics = validate(source, Some(&schema_bundle), None, true, &TomlBehavior);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, Some(1));
    assert_eq!(diagnostics[0].column, Some(29));
}

#[test]
fn keys_inside_an_inline_table_are_suggested_from_the_assigned_key_schema() {
    let schema_bundle = schema();
    let source = "[clients]\nacme = { se";
    let options = completions(
        source,
        source.encode_utf16().count(),
        false,
        Some(&schema_bundle),
        true,
        &TomlBehavior,
    );
    assert!(
        options.iter().any(|option| option.label == "secret"),
        "secret should be suggested inside the inline table, have {:?}",
        options
            .iter()
            .map(|option| &option.label)
            .collect::<Vec<_>>()
    );
}

#[test]
fn keys_inside_a_nested_inline_table_are_suggested() {
    let schema_bundle = schema();
    let source = "clients = { acme = { se";
    let options = completions(
        source,
        source.encode_utf16().count(),
        false,
        Some(&schema_bundle),
        true,
        &TomlBehavior,
    );
    assert!(
        options.iter().any(|option| option.label == "secret"),
        "secret should be suggested inside the nested inline table, have {:?}",
        options
            .iter()
            .map(|option| &option.label)
            .collect::<Vec<_>>()
    );
}

#[test]
fn entries_after_comma_inside_a_nested_inline_table_complete_from_it() {
    let schema_bundle = schema();
    let source = "clients = { acme = { secret = \"x\", ma";
    let options = completions(
        source,
        source.encode_utf16().count(),
        false,
        Some(&schema_bundle),
        true,
        &TomlBehavior,
    );
    assert!(
        options
            .iter()
            .any(|option| option.label == "max_connections"),
        "max_connections should be suggested after the first entry, have {:?}",
        options
            .iter()
            .map(|option| &option.label)
            .collect::<Vec<_>>()
    );
}


#[test]
fn object_properties_expand_to_a_table_header_with_required_children() {
    let source = "";
    let schema_bundle = schema();
    let options = completions(source, 0, true, Some(&schema_bundle), true, &TomlBehavior);
    let option = options
        .iter()
        .find(|option| option.label == "server")
        .expect("server should be suggested");
    let (updated, _, _) = apply_completion(source, option);
    assert_eq!(updated, "[server]\nhost = \"localhost\"\n");
}

#[test]
fn toggle_comments_adds_and_removes_hash_prefixes() {
    let source = "listen = 7835\n";
    let (commented, _, _) = TomlBehavior
        .toggle_comments(source, 0, source.encode_utf16().count())
        .unwrap();
    assert_eq!(commented, "# listen = 7835\n");
    let (restored, _, _) = TomlBehavior
        .toggle_comments(&commented, 0, commented.encode_utf16().count())
        .unwrap();
    assert_eq!(restored, source);
}

#[test]
fn hovering_a_key_reports_its_schema_type() {
    let hover = key_hover(
        "[server]\nhost = \"x\"\n",
        1,
        1,
        Some(&schema()),
        &TomlBehavior,
    )
    .expect("hover should resolve");
    assert_eq!(hover.key, "host");
    assert_eq!(hover.type_label, "string");
}

#[cfg(feature = "schema-validation")]
#[test]
fn schema_violations_point_at_the_offending_value() {
    let diagnostics = validate(
        "listen = \"not-a-port\"\n",
        Some(&schema()),
        None,
        true,
        &TomlBehavior,
    );
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, Some(1));
}

#[cfg(feature = "schema-validation")]
#[test]
fn every_caret_position_in_messy_documents_is_handled_without_panicking() {
    let schema_bundle = schema();
    for source in [
        "listen = ",
        "[server\nhost = ",
        "[[clients]]\nsecret = ",
        "a.b = ",
        "# only a comment\n",
        "",
    ] {
        for cursor in 0..=source.encode_utf16().count() {
            let _ = TomlBehavior.parse(source);
            let _ = TomlBehavior.completion_context(source, cursor);
            let _ = TomlBehavior.lexical_state(source, cursor);
            for force in [false, true] {
                for option in completions(
                    source,
                    cursor,
                    force,
                    Some(&schema_bundle),
                    true,
                    &TomlBehavior,
                ) {
                    let _ = apply_completion(source, &option);
                }
            }
        }
    }
}

fn colored(line: &str) -> Vec<(Option<TokenKind>, String)> {
    runs(line, &highlight(line))
}

fn run(kind: Option<TokenKind>, text: &str) -> (Option<TokenKind>, String) {
    (kind, text.to_string())
}

#[test]
fn table_headers_name_keys_with_a_trailing_comment() {
    assert_eq!(
        colored("  [server.http] # main"),
        vec![
            run(None, "  "),
            run(Some(TokenKind::Punctuation), "["),
            run(Some(TokenKind::Key), "server"),
            run(Some(TokenKind::Punctuation), "."),
            run(Some(TokenKind::Key), "http"),
            run(Some(TokenKind::Punctuation), "]"),
            run(None, " "),
            run(Some(TokenKind::Comment), "# main"),
        ]
    );
    assert_eq!(
        colored("[[bin]]"),
        vec![
            run(Some(TokenKind::Punctuation), "["),
            run(Some(TokenKind::Punctuation), "["),
            run(Some(TokenKind::Key), "bin"),
            run(Some(TokenKind::Punctuation), "]"),
            run(Some(TokenKind::Punctuation), "]"),
        ]
    );
}

#[test]
fn keys_and_equals_color_the_key_and_the_separator() {
    assert_eq!(
        colored("  name = plain"),
        vec![
            run(None, "  "),
            run(Some(TokenKind::Key), "name"),
            run(None, " "),
            run(Some(TokenKind::Punctuation), "="),
            run(None, " "),
            run(Some(TokenKind::Value), "plain"),
        ]
    );
    assert_eq!(
        colored("\"a=b\".c=x"),
        vec![
            run(Some(TokenKind::Key), "\"a=b\""),
            run(Some(TokenKind::Punctuation), "."),
            run(Some(TokenKind::Key), "c"),
            run(Some(TokenKind::Punctuation), "="),
            run(Some(TokenKind::Value), "x"),
        ]
    );
}

#[test]
fn quoted_values_are_strings() {
    for value in ["\"hi\"", "'hi'", "\"\"\"open"] {
        let line = format!("k = {value}");
        assert_eq!(
            colored(&line),
            vec![
                run(Some(TokenKind::Key), "k"),
                run(None, " "),
            run(Some(TokenKind::Punctuation), "="),
            run(None, " "),
                run(Some(TokenKind::String), value),
            ],
            "{line}"
        );
    }
}

/// Arrays and inline tables are colored element by element, as their lines
/// are when they go on over several (see `highlight::spanning`).
#[test]
fn arrays_and_inline_tables_are_colored_element_by_element() {
    use TokenKind::*;
    assert_eq!(
        colored("k = { a = 1 }"),
        vec![
            run(Some(Key), "k"),
            run(None, " "),
            run(Some(TokenKind::Punctuation), "="),
            run(None, " "),
            run(Some(Punctuation), "{"),
            run(None, " "),
            run(Some(Key), "a"),
            run(None, " "),
            run(Some(TokenKind::Punctuation), "="),
            run(None, " "),
            run(Some(Literal), "1"),
            run(None, " "),
            run(Some(Punctuation), "}"),
        ]
    );
    assert_eq!(
        colored("k = [1, \"x\"] # c"),
        vec![
            run(Some(Key), "k"),
            run(None, " "),
            run(Some(TokenKind::Punctuation), "="),
            run(None, " "),
            run(Some(Punctuation), "["),
            run(Some(Literal), "1"),
            run(Some(Punctuation), ","),
            run(None, " "),
            run(Some(String), "\"x\""),
            run(Some(Punctuation), "]"),
            run(None, " "),
            run(Some(Comment), "# c"),
        ]
    );
}

#[test]
fn numbers_and_booleans_are_literals_including_whitespace_before_a_comment() {
    for value in ["true", "false", "8080", "-1.5e3", "inf"] {
        let line = format!("k = {value}");
        assert_eq!(
            colored(&line),
            vec![
                run(Some(TokenKind::Key), "k"),
                run(None, " "),
            run(Some(TokenKind::Punctuation), "="),
            run(None, " "),
                run(Some(TokenKind::Literal), value),
            ],
            "{line}"
        );
    }
    assert_eq!(
        colored("port = 80  # http"),
        vec![
            run(Some(TokenKind::Key), "port"),
            run(None, " "),
            run(Some(TokenKind::Punctuation), "="),
            run(None, " "),
            run(Some(TokenKind::Literal), "80  "),
            run(Some(TokenKind::Comment), "# http"),
        ]
    );
}

#[test]
fn a_hash_right_after_equals_is_a_comment_and_an_empty_value_is_uncolored() {
    assert_eq!(
        colored("k =#x"),
        vec![
            run(Some(TokenKind::Key), "k"),
            run(None, " "),
            run(Some(TokenKind::Punctuation), "="),
            run(Some(TokenKind::Comment), "#x"),
        ]
    );
    assert_eq!(
        colored("k = "),
        vec![
            run(Some(TokenKind::Key), "k"),
            run(None, " "),
            run(Some(TokenKind::Punctuation), "="),
            run(None, " "),
        ]
    );
}

#[test]
fn comment_lines_and_unrecognized_lines() {
    assert_eq!(
        colored("   # note"),
        vec![run(None, "   "), run(Some(TokenKind::Comment), "# note")]
    );
    assert_eq!(colored("no equals here"), vec![run(None, "no equals here")]);
    assert_eq!(colored("= 1"), vec![run(None, "= 1")]);
    assert_eq!(colored(""), vec![]);
    assert_eq!(colored("   "), vec![run(None, "   ")]);
}

#[test]
fn a_hash_inside_a_string_is_part_of_the_string() {
    assert_eq!(
        colored("k = \"a # b\" # c"),
        vec![
            run(Some(TokenKind::Key), "k"),
            run(None, " "),
            run(Some(TokenKind::Punctuation), "="),
            run(None, " "),
            run(Some(TokenKind::String), "\"a # b\" "),
            run(Some(TokenKind::Comment), "# c"),
        ]
    );
}

#[test]
fn tricky_lines_are_covered_exactly_once() {
    for line in [
        "",
        "# only a comment",
        "\t# indented comment",
        "名前 = \"値 ✓\" # コメント",
        "k = \"unterminated # not a comment",
        "k = 'unterminated",
        "k = \"\"\"multi # still string",
        "k = \"a # b\"",
        "[table]   ",
        "[[arr]] # c  ",
        "k = 1   ",
        "k = { a = \"#\", b = [1, 2] }  # x",
        "\"quoted key\" = true",
        "[unclosed",
        "  = ",
        "é=ü",
    ] {
        let text: String = colored(line).into_iter().map(|(_, text)| text).collect();
        assert_eq!(text, line);
    }
}

#[test]
fn dates_and_times_read_as_their_text() {
    let value = TomlBehavior
        .parse("day = 1979-05-27\nat = 07:32:00\nstamp = 1979-05-27T07:32:00Z\n")
        .unwrap();
    assert_eq!(value["day"], "1979-05-27");
    assert_eq!(value["at"], "07:32:00");
    assert_eq!(value["stamp"], "1979-05-27T07:32:00Z");
}

#[test]
fn a_dotted_keys_span_is_its_last_segment() {
    let source = "websocket.bogus = 2\n\"a.b\".c = 1\n";
    let span = TomlBehavior.key_span(source, "/websocket/bogus").unwrap();
    assert_eq!((span.start_column, span.end_column), (11, 16));
    let span = TomlBehavior.key_span(source, "/a.b/c").unwrap();
    assert_eq!((span.line, span.start_column, span.end_column), (2, 7, 8));
}


#[test]
fn an_integer_past_the_signed_64_bit_range_is_named_in_the_error() {
    let value = serde_json::json!({"big": 9_223_372_036_854_775_808_u64});
    let error = TomlBehavior.serialize(&value, None).unwrap_err();
    assert_eq!(error, "TOML integers end at 9223372036854775807 (at /big)");
}
