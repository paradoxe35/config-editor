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
use super::HclBehavior;
use super::super::super::highlight::{TokenKind, runs};

fn schema() -> SchemaBundle {
    build_schema(
        r#"{"type":"object","properties":{
            "region":{"type":"string","description":"AWS region","default":"us-east-1"},
            "count":{"type":"integer"},
            "enabled":{"type":"boolean"},
            "tags":{"type":"object","properties":{"Name":{"type":"string"}},"required":["Name"]},
            "ingress":{"type":"array","items":{"type":"object"}},
            "variable":{"type":"object","additionalProperties":{"type":"object",
                "properties":{"default":{"type":"string"},"description":{"type":"string"}},"required":["default"]}}
        },"additionalProperties":false}"#,
    )
    .unwrap()
}

fn parse_ok(source: &str) -> serde_json::Value {
    match HclBehavior.parse(source) {
        Ok(document) => document,
        Err(error) => panic!("{}", error.message),
    }
}

fn complete(source: &str, schema: &SchemaBundle) -> Vec<Completion> {
    completions(
        source,
        source.encode_utf16().count(),
        false,
        Some(schema),
        true,
        &HclBehavior,
    )
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
fn a_nested_object_accepted_inside_an_inline_map_is_formatted_as_a_block() {
    let schema = nested_schema();
    assert_eq!(
        accept_marked("web = {le|}\n", "letsencrypt", &schema, &HclBehavior),
        "web = {\n  letsencrypt = {\n    email = \"|\"\n  }\n}\n"
    );
}

#[test]
fn a_nested_block_before_a_following_map_entry_keeps_it_on_its_own_line() {
    let schema = nested_schema();
    assert_eq!(
        accept_marked(
            "web = { le|, http = 80 }\n",
            "letsencrypt",
            &schema,
            &HclBehavior
        ),
        "web = {\n  letsencrypt = {\n    email = \"|\"\n  },\n  http = 80 }\n"
    );
}

#[test]
fn a_scalar_accepted_inside_an_inline_map_stays_inline() {
    let schema = nested_schema();
    assert_eq!(
        accept_marked("web = { ht| }\n", "http", &schema, &HclBehavior),
        "web = { http = | }\n"
    );
}

#[test]
fn a_nested_object_inside_a_block_body_is_indented_from_its_line() {
    let schema = nested_schema();
    assert_eq!(
        accept_marked("web {\n  le|\n}\n", "letsencrypt", &schema, &HclBehavior),
        "web {\n  letsencrypt {\n    email = \"|\"\n  }\n}\n"
    );
}

#[test]
fn duplicate_attributes_are_a_parse_error_on_the_repeated_line() {
    let error = HclBehavior
        .parse("region = \"a\"\nregion = \"b\"")
        .unwrap_err();
    assert_eq!(error.line, Some(2));
    assert!(error.message.contains("unique"), "{}", error.message);
}

#[test]
fn repeated_blocks_merge_instead_of_erroring() {
    let document = parse_ok("ingress {\n  port = 1\n}\ningress {\n  port = 2\n}");
    assert_eq!(
        document["ingress"],
        serde_json::json!([{"port": 1}, {"port": 2}])
    );
}

#[test]
fn same_key_in_different_blocks_is_not_a_duplicate() {
    let source =
        "variable \"a\" {\n  default = \"x\"\n}\nvariable \"b\" {\n  default = \"y\"\n}";
    assert!(HclBehavior.parse(source).is_ok());
}

#[test]
fn syntax_errors_report_a_position() {
    let error = HclBehavior.parse("region = ").unwrap_err();
    assert_eq!(error.line, Some(1));
    assert!(error.column.is_some());
}

#[test]
fn syntax_errors_report_the_parser_line_and_column() {
    // The line comes from hcl-rs's structured error, not its message.
    let error = HclBehavior.parse("region = \"a\"\ncount = (").unwrap_err();
    assert_eq!(error.line, Some(2));
    assert!(error.column.is_some());
}

#[test]
fn references_and_interpolations_parse_as_text() {
    let document = parse_ok("a = var.foo\nb = \"hi ${var.name}\"\n");
    assert_eq!(document["a"], serde_json::json!("${var.foo}"));
    assert_eq!(document["b"], serde_json::json!("hi ${var.name}"));
}

#[test]
fn keys_are_suggested_and_a_partial_key_is_replaced() {
    let source = "reg";
    let options = complete(source, &schema());
    let option = options
        .iter()
        .find(|option| option.label == "region")
        .expect("region should be suggested");
    let (updated, _, _) = apply_completion(source, option);
    assert_eq!(updated, "region = \"us-east-1\"");
}

#[test]
fn keys_already_in_the_body_are_not_suggested_again() {
    let source = "region = \"a\"\nreg";
    assert!(
        complete(source, &schema())
            .iter()
            .all(|option| option.label != "region")
    );
}

#[test]
fn keys_inside_a_block_are_suggested_from_the_schema() {
    let source = "variable \"name\" {\ndef";
    let option = complete(source, &schema())
        .into_iter()
        .find(|option| option.label == "default")
        .expect("default should be suggested inside the block");
    let (updated, _, _) = apply_completion(source, &option);
    assert!(updated.ends_with("default = \"\""), "{updated}");
}

#[test]
fn keys_inside_a_nested_value_map_are_suggested_from_the_schema() {
    // `clients = { acme = { … } }` descends through both maps: the caret is
    // completing `ClientEntry` keys, not top-level attributes.
    let source = "variable = {\n  acme = {\n    de";
    let option = complete(source, &schema())
        .into_iter()
        .find(|option| option.label == "default")
        .expect("default should be suggested inside the nested map");
    let (updated, _, _) = apply_completion(source, &option);
    assert!(updated.ends_with("default = \"\""), "{updated}");
}

#[test]
fn an_empty_nested_value_map_suggests_its_keys() {
    let source = "variable = {\n  acme = {";
    assert!(
        complete(source, &schema())
            .iter()
            .any(|option| option.label == "default"),
        "a key should be suggested right after `{{`"
    );
}

#[cfg(feature = "schema-validation")]
#[test]
fn an_error_inside_a_nested_value_map_is_reported_on_its_own_line() {
    let source = "variable = {\n  acme = {\n    default = 1\n  }\n}";
    let diagnostics = validate(source, Some(&schema()), None, true, &HclBehavior);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, Some(3));
    assert_eq!(diagnostics[0].column, Some(15));
}

#[test]
fn boolean_values_are_suggested_after_equals() {
    let source = "enabled = t";
    let option = complete(source, &schema())
        .into_iter()
        .find(|option| option.label == "true")
        .expect("true should be suggested");
    let (updated, _, _) = apply_completion(source, &option);
    assert_eq!(updated, "enabled = true");
}

#[cfg(feature = "schema-validation")]
#[test]
fn a_wrong_value_is_reported_on_the_value() {
    let source = "count = \"x\"";
    let diagnostics = validate(source, Some(&schema()), None, true, &HclBehavior);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, Some(1));
    assert_eq!(diagnostics[0].column, Some(9));
}

#[cfg(feature = "schema-validation")]
#[test]
fn an_unknown_key_is_reported_on_its_key() {
    let source = "bogus = 1";
    let diagnostics = validate(source, Some(&schema()), None, true, &HclBehavior);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, Some(1));
    assert_eq!(diagnostics[0].column, Some(1));
}

#[cfg(feature = "schema-validation")]
#[test]
fn an_error_inside_a_block_is_reported_on_its_own_line() {
    let source = "variable \"name\" {\n  default = 1\n}";
    let diagnostics = validate(source, Some(&schema()), None, true, &HclBehavior);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, Some(2));
}

#[test]
fn hovering_a_key_returns_its_schema_documentation() {
    let source = "region = \"a\"";
    let info = key_hover(source, 0, 1, Some(&schema()), &HclBehavior)
        .expect("region should have docs");
    assert_eq!(info.key, "region");
    assert_eq!(info.description.as_deref(), Some("AWS region"));
}

#[test]
fn completing_a_key_before_its_existing_equals_keeps_the_value() {
    let source = "reg = \"a\"";
    let cursor = "reg".encode_utf16().count();
    let option = completions(source, cursor, false, Some(&schema()), true, &HclBehavior)
        .into_iter()
        .find(|option| option.label == "region")
        .expect("region should be suggested");
    let (updated, _, _) = apply_completion(source, &option);
    assert_eq!(updated, "region = \"a\"");
}

/// Every caret position in messy and malformed documents must produce
/// completions, hovers, and edits without panicking or slicing mid-character.
#[cfg(feature = "schema-validation")]
#[test]
fn every_caret_position_in_messy_documents_is_handled_without_panicking() {
    let schema = schema();
    let samples = [
        "",
        "{",
        "}",
        "[[ ]]",
        "region = ",
        "variable \"na",
        "variable \"name\" {\n  def",
        "a = \"${unclosed}",
        "/* unclosed comment",
        "a = <<EOT\nhello",
        "resource \"aws_x\" \"y\" {\n  count = 2",
        "région = \"wörld\" co",
        "# comment\nregion = \"a\" # trailing",
        "a = [1, {\"b\" = ",
        "a = 1 b = 2",
    ];
    for source in samples {
        let caret_bytes = source
            .char_indices()
            .map(|(index, _)| index)
            .chain(std::iter::once(source.len()));
        for byte in caret_bytes {
            let cursor = source[..byte].encode_utf16().count();
            let _ = HclBehavior.completion_context(source, cursor);
            let _ = HclBehavior.newline(source, cursor);
            let _ = HclBehavior.lexical_state(source, cursor);
            for force in [false, true] {
                for option in
                    completions(source, cursor, force, Some(&schema), true, &HclBehavior)
                {
                    let _ = apply_completion(source, &option);
                }
            }
        }
        let _ = HclBehavior.parse(source);
        let _ = validate(source, Some(&schema), None, true, &HclBehavior);
        for (line_index, line) in source.split('\n').enumerate() {
            for column in 0..12 {
                let _ = HclBehavior.hover_cursor(line, column);
                let _ = key_hover(source, line_index, column, Some(&schema), &HclBehavior);
            }
        }
    }
}

#[test]
fn enter_inside_a_block_indents_one_level() {
    assert_eq!(
        HclBehavior.newline("variable \"a\" {", 15).text,
        "\n  "
    );
    assert_eq!(HclBehavior.newline("region = 1", 10).text, "\n");
}

#[test]
fn enter_between_braces_puts_the_caret_on_an_indented_middle_line() {
    let super::Newline { text, caret, .. } = HclBehavior.newline("{}", 1);
    assert_eq!(text, "\n  \n");
    assert_eq!(caret, 3);
}

#[test]
fn hash_comments_toggle_over_the_selection() {
    let (updated, _, _) = HclBehavior.toggle_comments("region = 1\n", 0, 10).unwrap();
    assert_eq!(updated, "# region = 1\n");
    let (restored, _, _) = HclBehavior
        .toggle_comments(&updated, 0, updated.encode_utf16().count())
        .unwrap();
    assert_eq!(restored, "region = 1\n");
}

#[test]
fn an_open_string_is_reported_to_the_pairing_guard() {
    assert_eq!(HclBehavior.lexical_state("region = \"x", 11).quote, Some('"'));
    assert_eq!(HclBehavior.lexical_state("region = \"x\"", 12).quote, None);
    assert!(HclBehavior.lexical_state("# region", 4).in_comment);
    assert!(HclBehavior.lexical_state("/* region", 9).in_comment);
}

/// Quotes and comment markers inside a template sequence of a string
/// (`"${ "#" }"`, nested ones, `%{ … }`) neither close the string nor open
/// a comment, for every reader of the text: the comments, the comment
/// start Edit-as matches lines by, and the pairing guard.
#[test]
fn markers_inside_template_sequences_are_string_text() {
    let lines = [
        "a = \"${\"/*\"}\"",
        "a = \"a${ \"*/\" }b\"",
        "a = \"a${ x /* c */ }b\"",
        "a = \"a${\"#\"}b\"",
        "a = \"a${ \" #\" }b\"",
        "a = \"a${ \" // x\" }b\"",
        "a = \"%{ if true }/*%{ endif }\"",
        "a = \"${\"${\" #\"}\"}\"",
    ];
    let mut failures = Vec::new();
    for line in lines {
        let source = format!("{line} # c\nb = 1\n");
        let comments: Vec<&str> = HclBehavior
            .comments(&source)
            .into_iter()
            .map(|range| &source[range])
            .collect();
        if comments != ["# c"] {
            failures.push(format!("{line}: comments {comments:?}"));
        }
        let with_comment = format!("{line} # c");
        if HclBehavior.comment_start(&with_comment) != Some(line.len() + 1) {
            failures.push(format!(
                "{line}: comment start {:?}",
                HclBehavior.comment_start(&with_comment)
            ));
        }
        if HclBehavior.comment_start(line).is_some() {
            failures.push(format!("{line}: a comment start without a comment"));
        }
        let end = line.encode_utf16().count();
        let state = HclBehavior.lexical_state(line, end);
        if state.quote.is_some() || state.in_comment {
            failures.push(format!("{line}: after it {state:?}"));
        }
        let next = format!("{line}\nb = 1");
        let state = HclBehavior.lexical_state(&next, next.encode_utf16().count());
        if state.quote.is_some() || state.in_comment {
            failures.push(format!("{line}: on the next line {state:?}"));
        }
        // Inside the template's string, the caret is in a string.
        if let Some(at) = line.find("${\"").map(|at| at + 3) {
            let state = HclBehavior.lexical_state(line, at);
            if state.quote != Some('"') || state.in_comment {
                failures.push(format!("{line}: inside {state:?}"));
            }
        }
    }
    // A value replaced by a suggestion runs through its template sequences.
    let value = "\"${ \"}\" }\" # c";
    if super::value_token_len(value) != value.len() - 4 {
        failures.push(format!("{value}: value length {}", super::value_token_len(value)));
    }
    // `$${` is a literal `${`: the string closes at the next quote.
    let literal = "a = \"$${\" # c";
    if HclBehavior.comment_start(literal) != Some(literal.len() - 3) {
        failures.push(format!("{literal}: {:?}", HclBehavior.comment_start(literal)));
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}


#[test]
fn an_object_property_expands_to_a_block_with_the_caret_on_the_first_value() {
    let schema = schema();
    let tags = &schema.schema["properties"]["tags"];
    let (insert, caret) = HclBehavior
        .property_insert("tags", tags, &schema.schema, &INSERT_AT_ROOT)
        .unwrap();
    assert_eq!(insert, "tags {\n  Name = \"\"\n}");
    // Inside the string's quotes.
    assert_eq!(caret, "tags {\n  Name = \"".encode_utf16().count());
}

fn highlighted(line: &str) -> Vec<(Option<TokenKind>, String)> {
    runs(line, &super::highlight(line))
}

fn run(kind: Option<TokenKind>, text: &str) -> (Option<TokenKind>, String) {
    (kind, text.to_string())
}

#[test]
fn block_headers_color_the_type_as_key_and_labels_as_strings() {
    use TokenKind::*;
    assert_eq!(
        highlighted(r#"resource "aws_x" "y" {"#),
        vec![
            run(Some(Key), "resource"),
            run(None, " "),
            run(Some(String), r#""aws_x""#),
            run(None, " "),
            run(Some(String), r#""y""#),
            run(None, " "),
            run(Some(Punctuation), "{"),
        ]
    );
    assert_eq!(
        highlighted("  ingress {"),
        vec![
            run(None, "  "),
            run(Some(Key), "ingress"),
            run(None, " "),
            run(Some(Punctuation), "{"),
        ]
    );
    assert_eq!(
        highlighted("module web {"),
        vec![
            run(Some(Key), "module"),
            run(None, " "),
            run(Some(Value), "web"),
            run(None, " "),
            run(Some(Punctuation), "{"),
        ]
    );
}

#[test]
fn attributes_color_key_equals_and_value() {
    use TokenKind::*;
    assert_eq!(
        highlighted("  name = var.name"),
        vec![
            run(None, "  "),
            run(Some(Key), "name"),
            run(None, " "),
            run(Some(Punctuation), "="),
            run(None, " "),
            run(Some(Value), "var"),
            run(Some(Punctuation), "."),
            run(Some(Value), "name"),
        ]
    );
}

#[test]
fn a_leading_word_without_equals_or_brace_is_a_value() {
    use TokenKind::*;
    assert_eq!(
        highlighted("name"),
        vec![run(Some(Value), "name")]
    );
}

#[test]
fn strings_heredocs_and_templates_color_as_strings() {
    use TokenKind::*;
    assert_eq!(
        highlighted(r#"a = "x\"y""#),
        vec![
            run(Some(Key), "a"),
            run(None, " "),
            run(Some(Punctuation), "="),
            run(None, " "),
            run(Some(String), r#""x\"y""#),
        ]
    );
    assert_eq!(
        highlighted("a = <<EOT"),
        vec![
            run(Some(Key), "a"),
            run(None, " "),
            run(Some(Punctuation), "="),
            run(None, " "),
            run(Some(String), "<<EOT"),
        ]
    );
    assert_eq!(
        highlighted("a = ${var.x}"),
        vec![
            run(Some(Key), "a"),
            run(None, " "),
            run(Some(Punctuation), "="),
            run(None, " "),
            run(Some(String), "${var.x}"),
        ]
    );
}

#[test]
fn numbers_booleans_and_null_are_literals() {
    use TokenKind::*;
    assert_eq!(
        highlighted("x = [1.5, true, null, false]"),
        vec![
            run(Some(Key), "x"),
            run(None, " "),
            run(Some(Punctuation), "="),
            run(None, " "),
            run(Some(Punctuation), "["),
            run(Some(Literal), "1.5"),
            run(Some(Punctuation), ","),
            run(None, " "),
            run(Some(Literal), "true"),
            run(Some(Punctuation), ","),
            run(None, " "),
            run(Some(Literal), "null"),
            run(Some(Punctuation), ","),
            run(None, " "),
            run(Some(Literal), "false"),
            run(Some(Punctuation), "]"),
        ]
    );
}

#[test]
fn braces_and_brackets_are_punctuation() {
    use TokenKind::*;
    assert_eq!(
        highlighted("  }]"),
        vec![
            run(None, "  "),
            run(Some(Punctuation), "}"),
            run(Some(Punctuation), "]"),
        ]
    );
}

#[test]
fn a_comment_only_line_is_one_comment_after_the_indent() {
    use TokenKind::*;
    for comment in ["# note \"x\" { = }", "// note"] {
        let line = format!("  {comment}");
        assert_eq!(
            highlighted(&line),
            vec![run(None, "  "), run(Some(Comment), comment)]
        );
    }
}

#[test]
fn trailing_and_block_comments_are_comments() {
    use TokenKind::*;
    assert_eq!(
        highlighted("a = 1 # one"),
        vec![
            run(Some(Key), "a"),
            run(None, " "),
            run(Some(Punctuation), "="),
            run(None, " "),
            run(Some(Literal), "1"),
            run(None, " "),
            run(Some(Comment), "# one"),
        ]
    );
    assert_eq!(
        highlighted("a /* k */ = 1 // one"),
        vec![
            run(Some(Key), "a"),
            run(None, " "),
            run(Some(Comment), "/* k */"),
            run(None, " "),
            run(Some(Punctuation), "="),
            run(None, " "),
            run(Some(Literal), "1"),
            run(None, " "),
            run(Some(Comment), "// one"),
        ]
    );
    assert_eq!(
        highlighted("/* open"),
        vec![run(Some(Comment), "/* open")]
    );
}

#[test]
fn a_hash_inside_a_string_is_not_a_comment() {
    use TokenKind::*;
    assert_eq!(
        highlighted(r#"a = "x # y""#),
        vec![
            run(Some(Key), "a"),
            run(None, " "),
            run(Some(Punctuation), "="),
            run(None, " "),
            run(Some(String), r#""x # y""#),
        ]
    );
}

#[test]
fn tricky_lines_cover_every_character() {
    for line in [
        "",
        "   ",
        "# only a comment",
        "  // only a comment",
        r#"name = "héllo ✓""#,
        r#"name = "unterminated"#,
        r#"a = "x # y" # z"#,
        "a = 1   ",
        "ключ = \"значение\" # ✓",
        "a = ${ \"}\" ",
        "<< = é",
    ] {
        let text: std::string::String =
            highlighted(line).into_iter().map(|(_, text)| text).collect();
        assert_eq!(text, line);
    }
}

#[test]
fn a_name_written_in_two_forms_is_reported_not_dropped() {
    for (source, line) in [
        ("services \"web\" { target = 80 }\nservices {\n  api { target = 81 }\n}\n", 2),
        ("services {\n  api { target = 81 }\n}\nservices \"web\" { target = 80 }\n", 4),
        ("x = 1\nx { v = 2 }\n", 2),
        ("x { v = 1 }\nx = { w = 2 }\n", 2),
        ("outer {\n  x = 1\n  x \"a\" { v = 2 }\n}\n", 3),
    ] {
        let error = super::parse::hcl_parse(source).expect_err(source);
        assert_eq!(error.line, Some(line), "{source}: {}", error.message);
        assert!(error.message.contains("both as"), "{}", error.message);
    }
    // Forms the parser keeps: repeated blocks, labels of any depth, a name
    // reused in another body.
    for source in [
        "x { v = 1 }\nx { v = 2 }\n",
        "a \"x\" \"y\" { v = 1 }\na \"z\" { v = 2 }\n",
        "x = 1\ny {\n  x { v = 2 }\n}\n",
    ] {
        assert!(super::parse::hcl_parse(source).is_ok(), "{source}: {:?}", super::parse::hcl_parse(source));
    }
}

/// A schema problem's key in a large document: the path of each body is
/// read through the header that opens it, not a search through every
/// header for each attribute (quadratic: 150 ms of a 300 ms task over 24
/// problems at 8000 lines).
#[test]
fn key_spans_in_a_large_document_take_no_search_per_header() {
    let source: String = (0..3_000)
        .map(|index| format!("client_{index} {{\n  secret = \"s{index}\"\n  max = {index}\n}}\n"))
        .collect();
    let pointers: Vec<String> = (2_976..3_000).map(|index| format!("/client_{index}/max")).collect();
    let (took, spans) = crate::editor::test_timing::best_of(3, || {
        pointers
            .iter()
            .map(|pointer| HclBehavior.key_span(&source, pointer))
            .collect::<Vec<_>>()
    });
    assert!(spans.iter().all(Option::is_some), "{spans:?}");
    assert_eq!(spans[0].map(|span| span.line), Some(2_976 * 4 + 3));
    assert!(took.as_millis() < 200, "24 key spans in {took:?}");
}

#[test]
fn whole_floats_are_written_with_a_fraction() {
    let value = serde_json::json!({"f": 1.0, "i": 1, "l": [2.0, 3], "b": {"x": -4.0, "y": 0.5}});
    let text = HclBehavior.serialize(&value, None).unwrap();
    assert!(text.contains("f = 1.0\n") && text.contains("i = 1\n"), "{text}");
    assert!(text.contains("  2.0,\n") && text.contains("x = -4.0") && text.contains("y = 0.5"), "{text}");
    // Read back, they are the numbers they were.
    assert_eq!(HclBehavior.parse(&text).unwrap()["l"][0].as_f64(), Some(2.0));
}
