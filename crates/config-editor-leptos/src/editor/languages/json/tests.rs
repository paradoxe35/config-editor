use super::super::super::completion::{
    Completion, accept_marked, apply_completion, completions, key_hover,
};
#[cfg(feature = "schema-validation")]
use super::super::super::diagnostics::validate;
use super::super::super::diagnostics::{SchemaBundle, build_schema};
use super::super::super::language::{InsertContext, ConfigLanguage};

const INSERT_AT_ROOT: InsertContext<'static> = InsertContext {
    base_indent: "",
    unit: "  ",
    position: super::super::super::language::EntryPosition::Block,
    path: &[],
    array_item: false,
};
use super::super::super::highlight::{TokenKind, runs};
use super::{JsonBehavior, highlight};

fn schema() -> SchemaBundle {
    build_schema(
        r#"{"type":"object","properties":{
            "listen":{"type":"integer","description":"Port"},
            "insecure_plaintext":{"type":"boolean"},
            "auth":{"type":"object","required":["enabled"],"properties":{"enabled":{"type":"boolean","default":false}}},
            "clients":{"type":"object","additionalProperties":{"type":"object",
                "properties":{"secret":{"type":"string"}},"required":["secret"]}}
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
        &JsonBehavior,
    )
}

fn accept_at(marked: &str, label: &str, schema: &SchemaBundle) -> String {
    accept_marked(marked, label, schema, &JsonBehavior)
}

fn nested_schema() -> SchemaBundle {
    build_schema(
        r#"{"type":"object","properties":{"web":{"type":"object","properties":{
            "http":{"type":"integer"},
            "letsencrypt":{"type":"object","required":["email"],
                "properties":{"email":{"type":"string"}}}}}}}"#,
    )
    .unwrap()
}

#[test]
fn a_nested_object_accepted_after_an_inline_brace_is_formatted_as_a_block() {
    let schema = nested_schema();
    assert_eq!(
        accept_at("{\n  \"web\": {\"le|}\n}", "letsencrypt", &schema),
        "{\n  \"web\": {\n    \"letsencrypt\": {\n      \"email\": \"|\"\n    }\n  }\n}"
    );
}

#[test]
fn a_nested_block_before_a_following_entry_keeps_it_on_its_own_line() {
    let schema = nested_schema();
    assert_eq!(
        accept_at(
            "{\n  \"web\": {\"le|\", \"http\": 80}\n}",
            "letsencrypt",
            &schema
        ),
        "{\n  \"web\": {\n    \"letsencrypt\": {\n      \"email\": \"|\"\n    },\n    \"http\": 80}\n}"
    );
}

#[test]
fn a_scalar_accepted_after_an_inline_brace_stays_inline() {
    let schema = nested_schema();
    assert_eq!(
        accept_at("{\n  \"web\": {\"ht|}\n}", "http", &schema),
        "{\n  \"web\": {\"http\": |}\n}"
    );
}

#[test]
fn a_nested_object_on_its_own_line_is_unchanged() {
    let schema = nested_schema();
    assert_eq!(
        accept_at("{\n  \"web\": {\n    \"le|\n  }\n}", "letsencrypt", &schema),
        "{\n  \"web\": {\n    \"letsencrypt\": {\n      \"email\": \"|\"\n    }\n  }\n}"
    );
}

#[test]
fn duplicate_keys_are_a_parse_error_on_the_repeated_line() {
    let error = JsonBehavior
        .parse("{\n  \"listen\": 1,\n  \"listen\": 2\n}")
        .unwrap_err();
    assert_eq!(error.line, Some(3));
    assert!(error.message.contains("duplicate"), "{}", error.message);
}

/// A comment (a tsconfig.json as `tsc --init` writes it) is reported as a
/// comment JSON has no room for, at the comment, wherever it stands.
#[test]
fn a_comment_is_reported_as_one_json_does_not_have() {
    let cases = [
        ("{\n  // Visit the docs\n  \"a\": 1\n}\n", 2, 3),
        ("{\n  \"a\": 1, // trailing\n  \"b\": 2\n}\n", 2, 11),
        ("{\n  \"a\": 1 // no comma\n}\n", 2, 10),
        ("{\n  \"a\": [1, /* two */ 2]\n}\n", 2, 12),
        ("// first line\n{}\n", 1, 1),
        ("\u{FEFF}/* bom */ {}\n", 1, 2),
    ];
    for (text, line, column) in cases {
        let error = super::super::Language::JSON.parse(text).unwrap_err();
        assert_eq!(error.message, "JSON has no comments: remove this one", "{text:?}");
        assert_eq!((error.line, error.column), (Some(line), Some(column)), "{text:?}");
    }
    // A slash in a string is no comment.
    assert!(JsonBehavior.parse("{\"u\": \"http://x/*y\"}").is_ok());
}

#[test]
fn same_key_in_different_objects_is_not_a_duplicate() {
    let source = r#"{"clients": {"a": {"secret": "x"}}, "auth": {"secret": "y"}}"#;
    assert!(JsonBehavior.parse(source).is_ok());
}

#[test]
fn syntax_errors_report_a_position() {
    let error = JsonBehavior.parse("{\"listen\": }").unwrap_err();
    assert_eq!(error.line, Some(1));
    assert!(error.column.is_some());
}

#[test]
fn keys_are_suggested_and_a_partial_key_is_replaced_with_the_quoted_key() {
    let source = "{\n  \"lis";
    let options = complete(source, &schema());
    let option = options
        .iter()
        .find(|option| option.label == "listen")
        .expect("listen should be suggested");
    let (updated, _, _) = apply_completion(source, option);
    assert_eq!(updated, "{\n  \"listen\": ");
}

#[test]
fn keys_already_in_the_object_are_not_suggested_again() {
    let source = "{\n  \"listen\": 1,\n  \"li";
    assert!(
        complete(source, &schema())
            .iter()
            .all(|option| option.label != "listen")
    );
}

#[test]
fn boolean_values_are_suggested_after_the_colon() {
    let source = "{\"insecure_plaintext\": t";
    let option = complete(source, &schema())
        .into_iter()
        .find(|option| option.label == "true")
        .expect("true should be suggested");
    let (updated, _, _) = apply_completion(source, &option);
    assert_eq!(updated, "{\"insecure_plaintext\": true");
}

#[test]
fn keys_inside_an_inline_object_are_suggested_from_the_schema() {
    let source = "{\"clients\": {\"acme\": {\"sec";
    let option = complete(source, &schema())
        .into_iter()
        .find(|option| option.label == "secret")
        .expect("a key typed inside an inline object should be suggested");
    let (updated, _, _) = apply_completion(source, &option);
    assert_eq!(updated, "{\"clients\": {\"acme\": {\"secret\": \"\"");
}

#[cfg(feature = "schema-validation")]
#[test]
fn a_wrong_value_is_reported_on_the_value() {
    let source = "{\n  \"listen\": \"x\"\n}";
    let diagnostics = validate(source, Some(&schema()), None, true, &JsonBehavior);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, Some(2));
    assert_eq!(diagnostics[0].column, Some(13));
}

#[cfg(feature = "schema-validation")]
#[test]
fn an_unknown_key_is_reported_on_its_key() {
    let source = "{\n  \"bogus\": 1\n}";
    let diagnostics = validate(source, Some(&schema()), None, true, &JsonBehavior);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, Some(2));
    assert_eq!(diagnostics[0].column, Some(3));
}

#[cfg(feature = "schema-validation")]
#[test]
fn an_error_inside_an_inline_object_is_reported_on_its_own_line() {
    let source = "{\n  \"clients\": {\"acme\": {\"secret\": 1}}\n}";
    let diagnostics = validate(source, Some(&schema()), None, true, &JsonBehavior);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, Some(2));
    let line = source.lines().nth(1).unwrap();
    assert_eq!(diagnostics[0].column, Some(line.find('1').unwrap() + 1));
}

#[test]
fn hovering_a_key_returns_its_schema_documentation() {
    let source = "{\n  \"listen\": 1\n}";
    let info = key_hover(source, 1, 3, Some(&schema()), &JsonBehavior)
        .expect("listen should have docs");
    assert_eq!(info.key, "listen");
    assert_eq!(info.description.as_deref(), Some("Port"));
}

#[cfg(feature = "schema-validation")]
#[test]
fn a_new_entry_after_a_value_without_a_comma_gets_one() {
    let source = r#"{"listen": 1"ins"#;
    let option = complete(source, &schema())
        .into_iter()
        .find(|option| option.label == "insecure_plaintext")
        .expect("insecure_plaintext should be suggested");
    let (updated, _, _) = apply_completion(source, &option);
    assert_eq!(updated, r#"{"listen": 1, "insecure_plaintext": "#);
}

#[cfg(feature = "schema-validation")]
#[test]
fn typing_an_opening_quote_in_an_object_lists_every_key() {
    let options = complete("{\n  \"", &schema());
    assert!(options.iter().any(|option| option.label == "listen"));
    assert!(options.iter().any(|option| option.label == "clients"));
}

#[test]
fn enter_between_braces_puts_the_caret_on_an_indented_middle_line() {
    let super::super::super::language::Newline { text, caret, .. } = JsonBehavior.newline("{}", 1);
    assert_eq!(text, "\n  \n");
    assert_eq!(caret, 3);
}

#[cfg(feature = "schema-validation")]
#[test]
fn a_key_inserted_before_an_existing_key_gets_a_comma_after_it() {
    let source = r#"{"listen": 1}"#;
    let option = completions(source, 1, true, Some(&schema()), true, &JsonBehavior)
        .into_iter()
        .find(|option| option.label == "insecure_plaintext")
        .expect("insecure_plaintext should be suggested");
    let (updated, _, caret) = apply_completion(source, &option);
    assert_eq!(updated, r#"{"insecure_plaintext": , "listen": 1}"#);
    assert_eq!(caret, r#"{"insecure_plaintext": "#.encode_utf16().count());
}

#[cfg(feature = "schema-validation")]
#[test]
fn completing_a_key_before_its_existing_colon_swallows_the_colon() {
    let source = r#"{"insecure_pla": false}"#;
    let cursor = r#"{"insecure_pla"#.encode_utf16().count();
    let context = JsonBehavior.completion_context(source, cursor);
    assert!(context.replace_start <= context.replace_end);
    let option = completions(source, cursor, false, Some(&schema()), true, &JsonBehavior)
        .into_iter()
        .find(|option| option.label == "insecure_plaintext")
        .expect("insecure_plaintext should be suggested");
    let (updated, _, _) = apply_completion(source, &option);
    assert_eq!(updated, r#"{"insecure_plaintext": false}"#);
}

#[cfg(feature = "schema-validation")]
#[test]
fn the_reported_text_is_flagged_and_its_keys_get_separators() {
    let source =
        "{\n  \"listen\": 7835,\n  \"clients\": {\"secrets\": []\"max_connections\":324}\n}";
    let error = JsonBehavior.parse(source).unwrap_err();
    assert_eq!(error.line, Some(3));
    let cursor = "{\n  \"listen\": 7835,\n  \"clients\": {\"secrets\": []\"max"
        .encode_utf16()
        .count();
    let context = JsonBehavior.completion_context(source, cursor);
    assert_eq!(context.entry_prefix, ", ");
}

#[cfg(feature = "schema-validation")]
#[test]
fn every_caret_position_in_messy_documents_is_handled_without_panicking() {
    let schema = schema();
    let samples = [
        "",
        "{",
        "}",
        "]]",
        "{\"a\": [1, {\"b\": ",
        "{\n  \"héllo\": \"wörld\" \"x\"\n}",
        "[[[]]]",
        "{\"a\":1,,\"b\"::}",
        "{\"clients\": {\"secrets\": []\"max_connections\":324}}",
        "\"unterminated\n{\"a\": \"",
        "{\"k\": tru\n",
        "{\n  \"listen\": 7835,\n  \"clients\": {\"alex\": {\"secret\": \"x\"}\n}",
        "{\"emoji🙂\": [\"🙂\", 1 2 3]}",
    ];
    for source in samples {
        let caret_bytes = source
            .char_indices()
            .map(|(index, _)| index)
            .chain(std::iter::once(source.len()));
        for byte in caret_bytes {
            let cursor = source[..byte].encode_utf16().count();
            let _ = JsonBehavior.completion_context(source, cursor);
            let _ = JsonBehavior.newline(source, cursor);
            let _ = JsonBehavior.lexical_state(source, cursor);
            for force in [false, true] {
                for option in
                    completions(source, cursor, force, Some(&schema), true, &JsonBehavior)
                {
                    let _ = apply_completion(source, &option);
                }
            }
        }
        let _ = JsonBehavior.parse(source);
        let _ = validate(source, Some(&schema), None, true, &JsonBehavior);
        for (line_index, line) in source.split('\n').enumerate() {
            for column in 0..12 {
                let _ = JsonBehavior.hover_cursor(line, column);
                let _ = key_hover(source, line_index, column, Some(&schema), &JsonBehavior);
            }
        }
    }
}

#[test]
fn enter_inside_braces_indents_one_level() {
    // Between `{}` the closer moves down and the caret sits on the middle line.
    assert_eq!(JsonBehavior.newline("{}", 1), super::super::super::language::Newline::with_caret("\n  \n", 3));
    assert_eq!(
        JsonBehavior.newline("{\n  \"a\": 1,", 12).text,
        "\n  "
    );
}

#[test]
fn json_has_no_comments_so_toggling_is_unsupported() {
    assert!(JsonBehavior.toggle_comments("{}", 0, 2).is_none());
}

#[test]
fn an_open_string_is_reported_to_the_pairing_guard() {
    assert_eq!(JsonBehavior.lexical_state("{\"a\": \"x", 8).quote, Some('"'));
    assert_eq!(JsonBehavior.lexical_state("{\"a\": \"x\"", 9).quote, None);
}

#[test]
fn an_object_with_required_children_expands_with_the_caret_on_the_first_value() {
    let schema = schema();
    let auth = &schema.schema["properties"]["auth"];
    let (insert, caret) = JsonBehavior
        .property_insert("auth", auth, &schema.schema, &INSERT_AT_ROOT)
        .unwrap();
    assert_eq!(insert, "\"auth\": {\n  \"enabled\": false\n}");
    assert_eq!(caret, "\"auth\": {\n  \"enabled\": ".encode_utf16().count());
}

fn hl(line: &str) -> Vec<(Option<TokenKind>, String)> {
    runs(line, &highlight(line))
}

fn run(kind: Option<TokenKind>, text: &str) -> (Option<TokenKind>, String) {
    (kind, text.to_string())
}

#[test]
fn a_string_followed_by_a_colon_is_a_key() {
    assert_eq!(
        hl(r#"  "name" : "x","#),
        vec![
            run(None, "  "),
            run(Some(TokenKind::Key), r#""name""#),
            run(None, " "),
            run(Some(TokenKind::Punctuation), ":"),
            run(None, " "),
            run(Some(TokenKind::String), r#""x""#),
            run(Some(TokenKind::Punctuation), ","),
        ]
    );
}

#[test]
fn numbers_booleans_and_null_are_literals_and_other_words_are_values() {
    assert_eq!(
        hl("[1, -2.5e3, true, false, null, nope]"),
        vec![
            run(Some(TokenKind::Punctuation), "["),
            run(Some(TokenKind::Literal), "1"),
            run(Some(TokenKind::Punctuation), ","),
            run(None, " "),
            run(Some(TokenKind::Literal), "-2.5e3"),
            run(Some(TokenKind::Punctuation), ","),
            run(None, " "),
            run(Some(TokenKind::Literal), "true"),
            run(Some(TokenKind::Punctuation), ","),
            run(None, " "),
            run(Some(TokenKind::Literal), "false"),
            run(Some(TokenKind::Punctuation), ","),
            run(None, " "),
            run(Some(TokenKind::Literal), "null"),
            run(Some(TokenKind::Punctuation), ","),
            run(None, " "),
            run(Some(TokenKind::Value), "nope"),
            run(Some(TokenKind::Punctuation), "]"),
        ]
    );
}

#[test]
fn every_bracket_and_separator_is_punctuation() {
    assert_eq!(
        hl("{}[],:"),
        "{}[],:"
            .chars()
            .map(|c| run(Some(TokenKind::Punctuation), &c.to_string()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn an_escaped_quote_does_not_end_the_string() {
    assert_eq!(
        hl(r#""a\"b": "c\"""#),
        vec![
            run(Some(TokenKind::Key), r#""a\"b""#),
            run(Some(TokenKind::Punctuation), ":"),
            run(None, " "),
            run(Some(TokenKind::String), r#""c\"""#),
        ]
    );
}

#[test]
fn an_unterminated_string_runs_to_the_line_end() {
    assert_eq!(
        hl(r#""k": "open, [1"#),
        vec![
            run(Some(TokenKind::Key), r#""k""#),
            run(Some(TokenKind::Punctuation), ":"),
            run(None, " "),
            run(Some(TokenKind::String), r#""open, [1"#),
        ]
    );
}

#[test]
fn highlighting_covers_every_character_of_tricky_lines() {
    for line in [
        "",
        "   ",
        r#"{"clé": "日本語 🎉"}  "#,
        r#""unterminated \""#,
        r#""esc\\": "q\"x""#,
        "\t1,\t",
        "🎉: wörd",
    ] {
        let text: String = hl(line).into_iter().map(|(_, text)| text).collect();
        assert_eq!(text, line);
    }
}

/// QA 21 (U21-6): the caret's place, read without the spans of every key
/// and value (each keystroke reads it), is the one the full scan finds, at
/// every place of a document written in JSON's forms.
#[test]
fn the_caret_scan_without_spans_finds_the_place_the_full_scan_does() {
    use super::scan::{analyze, analyze_caret};
    let texts = [
        "{\n  \"listen\": 7835,\n  \"web\": {\n    \"http\": 80,\n    \"hosts\": [\"a\", {\"b\": [1, {}]}],\n    \"e\": {}\n  },\n  \"pub\n  \"pool\": {}\n}\n",
        "[1, \"two\", {\"k\": [true, null]}, [[]]]",
        "{\"a\": , \"b\" \"c\": 1,, }",
    ];
    let mut failures = Vec::new();
    for text in texts {
        for cursor in (0..=text.len()).filter(|at| text.is_char_boundary(*at)) {
            let (full, caret) = (analyze(text, cursor), analyze_caret(text, cursor));
            let found = |scan: &super::scan::Scan| format!("{:?} {:?}", scan.snapshot, scan.keys);
            if found(&full) != found(&caret) {
                failures.push(format!("{text:?} at {cursor}: {} vs {}", found(&full), found(&caret)));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
