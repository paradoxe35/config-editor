use crate::editor::paths::FieldPath;
use super::super::super::completion::{
    Completion, accept_marked, apply_completion, completions, key_hover,
};
#[cfg(feature = "schema-validation")]
use super::super::super::diagnostics::validate;
use super::super::super::schema::{SchemaBundle, build_schema};
use super::super::super::language::ConfigLanguage;
use super::super::super::highlight::{TokenKind, runs};
use super::{TomlBehavior, highlight};

/// The language under test, read as the editor reads one.
const LANGUAGE: &dyn ConfigLanguage = &TomlBehavior;

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
    let document = LANGUAGE
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
    let error = LANGUAGE.parse("listen = 1\nlisten = 2\n").unwrap_err();
    assert_eq!(error.line, Some(2));
    assert!(!error.message.is_empty(), "{}", error.message);
}

#[test]
fn syntax_errors_report_a_position() {
    let error = LANGUAGE.parse("listen = \n").unwrap_err();
    assert_eq!(error.line, Some(1));
    assert!(error.column.is_some());
}

#[test]
fn tables_parse_into_nested_values() {
    let document = match LANGUAGE.parse("[server]\nhost = \"example.com\"\n") {
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
    let (commented, _, _) = LANGUAGE
        .toggled_text(source, 0, source.encode_utf16().count())
        .unwrap();
    assert_eq!(commented, "# listen = 7835\n");
    let (restored, _, _) = LANGUAGE
        .toggled_text(&commented, 0, commented.encode_utf16().count())
        .unwrap();
    assert_eq!(restored, source);
}

/// A selected commented line is uncommented as it is, even into text
/// that does not read (the problem shows): no refusal.
#[test]
fn toggle_comments_uncomments_a_selected_line_into_broken_text() {
    let source = "# a = [1,\nb = 2\n";
    let end = "# a = [1,".encode_utf16().count();
    let (uncommented, _, _) = LANGUAGE.toggled_text(source, 0, end).unwrap();
    assert_eq!(uncommented, "a = [1,\nb = 2\n");
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
            let _ = LANGUAGE.parse(source);
            let _ = LANGUAGE.completion_context(source, cursor);
            let _ = LANGUAGE.lexical_state(source, cursor);
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
    let value = LANGUAGE
        .parse("day = 1979-05-27\nat = 07:32:00\nstamp = 1979-05-27T07:32:00Z\n")
        .unwrap();
    assert_eq!(value["day"], "1979-05-27");
    assert_eq!(value["at"], "07:32:00");
    assert_eq!(value["stamp"], "1979-05-27T07:32:00Z");
}

#[test]
fn a_dotted_keys_span_is_its_last_segment() {
    let source = "websocket.bogus = 2\n\"a.b\".c = 1\n";
    let span = LANGUAGE.key_span(source, &FieldPath::from("/websocket/bogus")).unwrap();
    assert_eq!((span.start_column, span.end_column), (11, 16));
    let span = LANGUAGE.key_span(source, &FieldPath::from("/a.b/c")).unwrap();
    assert_eq!((span.line, span.start_column, span.end_column), (2, 7, 8));
}


#[test]
fn an_integer_past_the_signed_64_bit_range_is_named_in_the_error() {
    let value = serde_json::json!({"big": 9_223_372_036_854_775_808_u64});
    let error = LANGUAGE.serialize(&value, None).unwrap_err();
    assert_eq!(error, "TOML integers end at 9223372036854775807 (at /big)");
}

#[test]
fn a_header_in_a_later_array_entry_suggests_the_tables_that_entry_lacks() {
    let schema_bundle = build_schema(
        r#"{"type":"object","properties":{"r":{"type":"array","items":{"type":"object","properties":{
            "s":{"type":"object"},"u":{"type":"object"}}}}}}"#,
    )
    .unwrap();
    // `s` is taken in the first `[[r]]` entry, not in the second.
    let source = "[[r]]\n[r.s]\n[[r]]\n[r.";
    let options = completions(
        source,
        source.encode_utf16().count(),
        false,
        Some(&schema_bundle),
        true,
        &TomlBehavior,
    );
    let labels: Vec<&str> = options.iter().map(|option| option.label.as_str()).collect();
    assert!(labels.contains(&"s") && labels.contains(&"u"), "{labels:?}");
}

/// A `"""` or `'''` string whose closing quotes share its only body line:
/// a place on that line is in the string's field (it was the root).
#[test]
fn a_place_on_a_string_s_closing_line_is_in_its_field() {
    use crate::TextPosition;
    use crate::editor::languages::Language;
    for text in ["a = \"\"\"\nxyz\"\"\"\n", "a = '''\nxyz'''\n", "b = 1\na = \"\"\"\nxyz\"\"\"\n"] {
        let line = text.lines().position(|line| line.starts_with("xyz")).expect("a body") + 1;
        for column in [1, 2, 4] {
            let path = Language::TOML.path_at(text, TextPosition { line, column });
            assert_eq!(path.map(|path| path.to_string()), Some("/a".to_string()), "{text:?} {line}:{column}");
        }
    }
}

/// The key under the pointer on a long line of inline keys
/// (`t = { k0 = 1, k1 = 1, … }`) costs the line once: the pointer's byte
/// was found again for each key (the line's square, P5). Timed, four times
/// the keys: about four times as long, sixteen when quadratic.
#[test]
fn the_key_under_the_pointer_on_a_long_inline_table_costs_the_line_once() {
    let line = |keys: usize| {
        let entries: Vec<String> = (0..keys).map(|key| format!("k{key} = 1")).collect();
        format!("t = {{ {} }}", entries.join(", "))
    };
    let (small, large) = (line(2500), line(10_000));
    let last = |line: &String| line.chars().count() - 8;
    let growth = crate::editor::test_timing::growth(5, (&small, &large), |line: &String| {
        LANGUAGE.hover_key(line, last(line))
    });
    assert_eq!(
        LANGUAGE.hover_key(&large, last(&large)).map(|(key, _)| key),
        Some("k9999".to_string())
    );
    assert!(growth.ratio() < 8.0, "2500 keys, then 10000: {growth}");
}

/// A `#` right after a value starts a comment (TOML reads `n = 1#c` as
/// `1`): every reading of the line agrees, the comment hook, the comments
/// and the caret's lexical state.
#[test]
fn a_hash_right_after_a_value_starts_a_comment_in_every_reading() {
    let line = "n = 1#c";
    assert_eq!(LANGUAGE.comment_start(line), Some(5));
    assert_eq!(LANGUAGE.comments(&format!("{line}\n")), vec![5..7]);
    let state = LANGUAGE.lexical_state(line, line.len());
    assert_eq!((state.quote, state.in_comment), (None, true));
    assert_eq!(LANGUAGE.parse(line).unwrap(), serde_json::json!({"n": 1}));
}

/// A list key quoted for its characters (`"é" = []`) leaves the caret
/// between the brackets: the caret is counted in UTF-16 units, not bytes.
#[test]
fn a_list_written_for_a_quoted_key_takes_the_caret_inside() {
    use super::super::super::language::{EntryPosition, InsertContext, InsertPlan};
    let root = serde_json::json!({});
    let list = serde_json::json!({"type": "array", "items": {"type": "integer"}});
    let context = InsertContext {
        base_indent: "",
        unit: "  ",
        position: EntryPosition::Block,
        path: &[],
        array_item: false,
        another_item: false,
        attributes_marked: false,
    };
    let toml: &dyn ConfigLanguage = &TomlBehavior;
    let (insert, caret) = toml.write_insert("é", &InsertPlan::of(toml, &list, &root), &context).unwrap();
    assert_eq!(insert, "\"é\" = []");
    assert_eq!(caret, "\"é\" = [".encode_utf16().count());
}
