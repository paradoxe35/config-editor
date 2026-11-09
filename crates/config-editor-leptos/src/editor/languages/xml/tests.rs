use super::super::super::completion::{
    accept_marked, apply_completion, completions, key_hover,
};
#[cfg(feature = "schema-validation")]
use super::super::super::diagnostics::validate;
use super::super::super::diagnostics::{SchemaBundle, build_schema};
use super::super::super::language::ConfigLanguage;
use super::super::super::highlight::{TokenKind, runs};
use super::XmlBehavior;

fn schema() -> SchemaBundle {
    build_schema(
        r#"{"type":"object","properties":{
            "server":{"type":"object","properties":{
                "host":{"type":"string","description":"Hostname"},
                "port":{"type":"integer","description":"Port"},
                "tls":{"type":"object","required":["enabled"],"properties":{"enabled":{"type":"boolean","default":false}}}
            }},
            "version":{"type":"string"}
        }}"#,
    )
    .unwrap()
}

fn complete(
    source: &str,
    schema: &SchemaBundle,
) -> Vec<super::super::super::completion::Completion> {
    completions(
        source,
        source.encode_utf16().count(),
        false,
        Some(schema),
        true,
        &XmlBehavior,
    )
}

fn parse_error(source: &str) -> super::super::super::language::ParseError {
    XmlBehavior
        .parse(source)
        .expect_err("the document should be rejected")
}

/// QA 24 (L24-U4): a comment holding `--` was accepted; XML forbids it
/// (and a `-` right before `-->`). The error points at the dashes; other
/// comments, dashes alone included, still parse.
#[test]
fn a_comment_holding_two_dashes_is_not_well_formed() {
    for (source, line, column) in [
        ("<r>\n  <!-- a -- comment -->\n</r>\n", 2, 10),
        ("<!-- a --->\n<r>1</r>\n", 1, 8),
        ("<r>1</r>\n<!-- end -- -->\n", 2, 10),
    ] {
        let error = parse_error(source);
        assert_eq!((error.line, error.column), (Some(line), Some(column)), "{source:?}");
        assert!(error.message.contains("--"), "{}", error.message);
    }
    // An empty comment (`<!---->`) is well-formed.
    for source in [
        "<r>\n  <!-- a - b -->\n</r>\n",
        "<!-- -a- -->\n<r>1</r>\n",
        "<r><!----><t>1</t></r>",
        "<r><!-- x --><!-- y --></r>",
    ] {
        let parsed = XmlBehavior.parse(source);
        assert!(parsed.is_ok(), "{source:?}: {parsed:?}");
    }
}

#[test]
fn mismatched_tags_are_a_parse_error_with_a_position() {
    let error = parse_error("<server>\n  <host></port></server>");
    assert_eq!(error.line, Some(2));
    assert_eq!(error.column, Some(9));
    assert!(error.message.contains("port"), "{}", error.message);
}

#[test]
fn unclosed_tags_are_a_parse_error_at_the_open_tag() {
    let error = parse_error("<server>\n  <host>");
    assert!(error.message.contains("unclosed"), "{}", error.message);
    assert_eq!(error.line, Some(2));
    assert_eq!(error.column, Some(3));
}

#[test]
fn repeated_sibling_elements_fold_into_an_array() {
    let document = XmlBehavior
        .parse("<root config-root=\"true\"><item>1</item><item>2</item></root>")
        .expect("document should parse");
    assert_eq!(document, serde_json::json!({"item": [1, 2]}));
}

fn parse_ok(source: &str) -> serde_json::Value {
    match XmlBehavior.parse(source) {
        Ok(document) => document,
        Err(error) => panic!("{}", error.message),
    }
}

#[test]
fn a_marked_config_root_is_transparent_whatever_its_name() {
    for name in ["root", "burrow", "config"] {
        let document = parse_ok(&format!(
            "<{name} config-root=\"true\"><listen>7835</listen><enabled>false</enabled></{name}>"
        ));
        assert_eq!(
            document,
            serde_json::json!({"listen": 7835, "enabled": false}),
            "<{name}>"
        );
    }
}

#[test]
fn the_config_root_marker_and_its_other_attributes_are_not_data() {
    let document =
        parse_ok("<burrow config-root=\"true\" xmlns=\"urn:x\"><listen>1</listen></burrow>");
    assert_eq!(document, serde_json::json!({"listen": 1}));
}

#[test]
fn an_empty_config_root_is_an_empty_document() {
    assert_eq!(
        parse_ok("<burrow config-root=\"true\"/>"),
        serde_json::json!({})
    );
    assert_eq!(
        parse_ok("<burrow config-root=\"true\">\n</burrow>"),
        serde_json::json!({})
    );
}

#[test]
fn an_unmarked_lone_container_is_configuration_data() {
    let document = parse_ok("<burrow><listen>7835</listen><enabled>false</enabled></burrow>");
    assert_eq!(
        document,
        serde_json::json!({"burrow": {"listen": 7835, "enabled": false}})
    );
}

#[test]
fn config_root_false_is_an_ordinary_attribute() {
    let document = parse_ok("<burrow config-root=\"false\"><listen>1</listen></burrow>");
    assert_eq!(
        document,
        serde_json::json!({"burrow": {"config-root": false, "listen": 1}})
    );
}

#[test]
fn a_nested_config_root_marker_is_ordinary_data() {
    let document = parse_ok("<a><b config-root=\"true\"><c>1</c></b></a>");
    assert_eq!(
        document,
        serde_json::json!({"a": {"b": {"config-root": true, "c": 1}}})
    );
}

#[test]
fn an_empty_document_is_an_empty_object() {
    assert_eq!(parse_ok(""), serde_json::json!({}));
    assert_eq!(parse_ok("<!-- only a comment -->\n"), serde_json::json!({}));
}

#[test]
fn two_config_roots_are_a_parse_error_on_the_second() {
    let error = parse_error(
        "<a config-root=\"true\"><x>1</x></a>\n<b config-root=\"true\"><y>2</y></b>",
    );
    assert_eq!(error.line, Some(2));
    assert!(error.message.contains("only one"), "{}", error.message);
}

#[test]
fn an_element_beside_the_config_root_is_a_parse_error_on_it() {
    let error = parse_error("<a config-root=\"true\"><x>1</x></a>\n<listen>1</listen>");
    assert_eq!(error.line, Some(2));
    assert!(
        error.message.contains("outside the config root"),
        "{}",
        error.message
    );
}

#[test]
fn a_config_root_holding_only_text_is_a_parse_error() {
    let error = parse_error("<a config-root=\"true\">text</a>");
    assert_eq!(error.line, Some(1));
    assert!(error.message.contains("not text"), "{}", error.message);
}

#[cfg(feature = "schema-validation")]
#[test]
fn the_config_root_marker_is_never_validated() {
    let strict = build_schema(
        r#"{"type":"object","properties":{"listen":{"type":"integer"}},"additionalProperties":false}"#,
    )
    .unwrap();
    let source = "<burrow config-root=\"true\">\n  <listen>1</listen>\n</burrow>";
    assert!(validate(source, Some(&strict), None, true, &XmlBehavior).is_empty());
    // Unmarked, the same wrapper is an unknown top-level property.
    let unmarked = "<burrow>\n  <listen>1</listen>\n</burrow>";
    let diagnostics = validate(unmarked, Some(&strict), None, true, &XmlBehavior);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, Some(1));
}

#[cfg(feature = "schema-validation")]
#[test]
fn diagnostics_under_a_config_root_point_at_their_element() {
    let strict = build_schema(
        r#"{"type":"object","properties":{"listen":{"type":"integer"}},"additionalProperties":false}"#,
    )
    .unwrap();
    let source =
        "<burrow config-root=\"true\">\n  <listen>x</listen>\n  <bogus>1</bogus>\n</burrow>";
    let mut lines: Vec<Option<usize>> =
        validate(source, Some(&strict), None, true, &XmlBehavior)
            .into_iter()
            .map(|diagnostic| diagnostic.line)
            .collect();
    lines.sort();
    assert_eq!(lines, vec![Some(2), Some(3)]);
}

#[test]
fn schema_less_parsing_handles_every_root_shape() {
    for source in [
        "<a config-root=\"true\"><x>1</x></a>",
        "<a><x>1</x></a>",
        "<x>1</x><y>2</y>",
        "<x>1</x>",
        "",
    ] {
        parse_ok(source);
    }
}

#[test]
fn nothing_is_suggested_inside_the_config_root_start_tag() {
    let source = "<burrow config-root=\"true\" se></burrow>";
    let at = source.find(" se").unwrap() + 3;
    let options = completions(source, at, true, Some(&schema()), true, &XmlBehavior);
    assert!(
        options.is_empty(),
        "{:?}",
        options.iter().map(|o| &o.label).collect::<Vec<_>>()
    );
}

#[test]
fn hover_on_the_config_root_marker_has_no_documentation() {
    let source = "<burrow config-root=\"true\">\n  <server/>\n</burrow>";
    assert!(key_hover(source, 0, 10, Some(&schema()), &XmlBehavior).is_none());
}

#[test]
fn completion_under_a_config_root_addresses_the_document_root() {
    assert_eq!(
        accept_marked(
            "<burrow config-root=\"true\">\n  <ver|\n</burrow>",
            "version",
            &schema(),
            &XmlBehavior
        ),
        "<burrow config-root=\"true\">\n  <version>|</version>\n</burrow>"
    );
}

#[test]
fn a_new_element_typed_on_its_own_line_after_a_closing_tag_stays_on_that_line() {
    // The replaced range must not reach back to the previous line's `>`.
    let marked = "<listen>7835</listen>\n<server>\n  <host>x</host>\n</server>\nver|";
    assert_eq!(
        accept_marked(marked, "version", &schema(), &XmlBehavior),
        "<listen>7835</listen>\n<server>\n  <host>x</host>\n</server>\n<version>|</version>"
    );
    let indented = "<server>\n  <host>x</host>\n  po|\n</server>";
    assert_eq!(
        accept_marked(indented, "port", &schema(), &XmlBehavior),
        "<server>\n  <host>x</host>\n  <port>|</port>\n</server>"
    );
}

#[test]
fn a_block_element_accepted_after_an_open_tag_on_its_line_is_formatted() {
    assert_eq!(
        accept_marked("<server><tl|</server>", "tls", &schema(), &XmlBehavior),
        "<server>\n  <tls>\n    <enabled>|false</enabled>\n  </tls>\n</server>"
    );
}

#[test]
fn a_scalar_element_between_tags_stays_compact() {
    assert_eq!(
        accept_marked("<server><ho|</server>", "host", &schema(), &XmlBehavior),
        "<server><host>|</host></server>"
    );
}

#[test]
fn a_lone_scalar_element_is_a_document_property() {
    let document = XmlBehavior
        .parse("<listen>7835</listen>")
        .expect("document should parse");
    assert_eq!(document, serde_json::json!({"listen": 7835}));
}

#[test]
fn several_top_level_elements_form_an_implicit_root() {
    let document = XmlBehavior
        .parse("<listen>1</listen>\n<enabled>true</enabled>")
        .expect("document should parse");
    assert_eq!(document, serde_json::json!({"listen": 1, "enabled": true}));
}

#[test]
fn element_text_is_typed_against_the_schema() {
    let document = XmlBehavior
        .parse("<root config-root=\"true\"><port>80</port><tls><enabled>true</enabled></tls></root>")
        .expect("document should parse");
    assert_eq!(
        document,
        serde_json::json!({"port": 80, "tls": {"enabled": true}})
    );
}

#[test]
fn same_names_in_different_elements_are_not_duplicates() {
    let source = "<a><host>x</host><b><host>y</host></b></a>";
    assert!(XmlBehavior.parse(source).is_ok());
}

#[test]
fn duplicate_attributes_are_a_parse_error() {
    let error = parse_error("<server version=\"1\" version=\"2\"/>");
    assert!(error.message.contains("duplicate"), "{}", error.message);
    assert_eq!(error.line, Some(1));
}

#[test]
fn attributes_and_text_make_an_object() {
    let document = XmlBehavior
        .parse("<server version=\"1\">up</server>")
        .expect("document should parse");
    assert_eq!(
        document,
        serde_json::json!({"server": {"version": 1, "#text": "up"}})
    );
}

#[test]
fn stray_text_outside_the_root_is_a_parse_error() {
    let error = parse_error("<server/>\nstray");
    assert_eq!(error.line, Some(2));
    assert_eq!(error.column, Some(1));
}

#[test]
fn syntax_errors_report_a_position() {
    let error = parse_error("<server><host></server>");
    assert_eq!(error.line, Some(1));
    assert!(error.column.is_some());
}

#[test]
fn elements_are_suggested_and_a_partial_name_is_replaced() {
    let source = "<root config-root=\"true\"><server><ho";
    let option = complete(source, &schema())
        .into_iter()
        .find(|option| option.label == "host")
        .expect("host should be suggested");
    let (updated, _, _) = apply_completion(source, &option);
    assert_eq!(updated, "<root config-root=\"true\"><server><host></host>");
}

#[test]
fn top_level_elements_are_suggested_in_an_implicit_root() {
    // Several top-level elements: nothing is a wrapper, both are document
    // properties, so a new top-level element completes from the root.
    let source = "<server><host>x</host></server><ver";
    let option = complete(source, &schema())
        .into_iter()
        .find(|option| option.label == "version")
        .expect("version should be suggested");
    let (updated, _, _) = apply_completion(source, &option);
    assert_eq!(
        updated,
        "<server><host>x</host></server><version></version>"
    );
}

#[test]
fn values_inside_an_element_are_suggested_from_its_schema() {
    let source = "<root config-root=\"true\"><server><tls><enabled>tr";
    let option = complete(source, &schema())
        .into_iter()
        .find(|option| option.label == "true")
        .expect("true should be suggested for a boolean");
    let (updated, _, _) = apply_completion(source, &option);
    assert_eq!(
        updated,
        "<root config-root=\"true\"><server><tls><enabled>true"
    );
}

#[test]
fn attribute_values_are_suggested_inside_quotes() {
    let local_schema = build_schema(
        r#"{"type":"object","properties":{"server":{"type":"object","properties":{"mode":{"type":"string","enum":["fast","slow"]}}}}}"#,
    )
    .unwrap();
    let source = "<server mode=\"f";
    let option = complete(source, &local_schema)
        .into_iter()
        .find(|option| option.label == "fast")
        .expect("fast should be suggested");
    let (updated, _, _) = apply_completion(source, &option);
    assert_eq!(updated, "<server mode=\"fast");
}

#[test]
fn attributes_are_suggested_from_their_element() {
    let source = "<server po";
    let option = complete(source, &schema())
        .into_iter()
        .find(|option| option.label == "port")
        .expect("port should be suggested as an attribute");
    let (updated, _, _) = apply_completion(source, &option);
    assert_eq!(updated, "<server port=\"\"");
}

#[test]
fn an_element_already_present_is_not_offered_again() {
    // `server` is an object, not an array, so a second one would be data
    // the schema does not describe.
    let source = "<root config-root=\"true\"><server><host>x</host></server><ver";
    let labels: Vec<String> = complete(source, &schema())
        .into_iter()
        .map(|option| option.label)
        .collect();
    assert!(labels.iter().any(|label| label == "version"), "{labels:?}");
    let again = "<root config-root=\"true\"><server><host>x</host></server><ser";
    let labels: Vec<String> = complete(again, &schema())
        .into_iter()
        .map(|option| option.label)
        .collect();
    assert!(!labels.iter().any(|label| label == "server"), "{labels:?}");
}

#[test]
fn hover_over_an_element_returns_its_schema_documentation() {
    let source =
        "<root config-root=\"true\">\n  <server>\n    <host>x</host>\n  </server>\n</root>";
    let info =
        key_hover(source, 2, 5, Some(&schema()), &XmlBehavior).expect("host should have docs");
    assert_eq!(info.key, "host");
    assert_eq!(info.description.as_deref(), Some("Hostname"));
}

#[test]
fn toggle_comments_wraps_lines_in_xml_comments() {
    let (updated, _, _) = XmlBehavior
        .toggle_comments("<server>\n  <host>x</host>\n</server>", 0, 8)
        .unwrap();
    assert!(updated.starts_with("<!-- <server> -->"), "{updated}");
}

#[test]
fn toggle_comments_unwraps_xml_comments() {
    let (updated, _, _) = XmlBehavior
        .toggle_comments("<!-- <server> -->", 0, 17)
        .unwrap();
    assert_eq!(updated, "<server>");
}

#[test]
fn enter_between_tags_puts_the_caret_on_an_indented_middle_line() {
    let super::Newline { text, caret, .. } = XmlBehavior.newline("<a></a>", 3);
    assert_eq!(text, "\n  \n");
    assert_eq!(caret, 3);
}

#[cfg(feature = "schema-validation")]
#[test]
fn a_wrong_value_is_reported_on_the_value() {
    let source =
        "<root config-root=\"true\">\n  <server>\n    <port>x</port>\n  </server>\n</root>";
    let diagnostics = validate(source, Some(&schema()), None, true, &XmlBehavior);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, Some(3));
    assert_eq!(diagnostics[0].column, Some(11));
}

#[cfg(feature = "schema-validation")]
#[test]
fn an_attribute_value_error_is_reported_on_the_attribute() {
    let source = "<root config-root=\"true\"><server port=\"x\"/></root>";
    let diagnostics = validate(source, Some(&schema()), None, true, &XmlBehavior);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, Some(1));
    // The span starts at the opening quote of `port="x"`.
    assert_eq!(diagnostics[0].column, Some(39));
}

#[cfg(feature = "schema-validation")]
#[test]
fn a_nested_element_error_is_reported_on_its_own_line() {
    let local_schema = build_schema(
        r#"{"type":"object","properties":{
            "clients":{"type":"object","additionalProperties":{
                "type":"object","properties":{"secret":{"type":"integer"}}
            }}
        }}"#,
    )
    .unwrap();
    let source = "<root config-root=\"true\">\n  <clients>\n    <acme>\n      <secret>x</secret>\n    </acme>\n  </clients>\n</root>";
    let diagnostics = validate(source, Some(&local_schema), None, true, &XmlBehavior);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].line, Some(4));
    assert_eq!(diagnostics[0].column, Some(15));
}

#[test]
fn comments_do_not_confuse_the_structure_scan() {
    let source = "<root config-root=\"true\"><server><!-- <host> --><po";
    let option = complete(source, &schema())
        .into_iter()
        .find(|option| option.label == "port")
        .expect("port should be suggested despite the comment");
    let (updated, _, _) = apply_completion(source, &option);
    assert_eq!(
        updated,
        "<root config-root=\"true\"><server><!-- <host> --><port></port>"
    );
}

fn highlighted(line: &str) -> Vec<(Option<TokenKind>, String)> {
    runs(line, &super::highlight(line))
}

fn run(kind: TokenKind, text: &str) -> (Option<TokenKind>, String) {
    (Some(kind), text.to_string())
}

fn plain(text: &str) -> (Option<TokenKind>, String) {
    (None, text.to_string())
}

#[test]
fn highlights_an_open_tag_with_attributes() {
    use TokenKind::*;
    assert_eq!(
        highlighted("<a:b x=\"1\" y='2' flag>"),
        vec![
            run(Punctuation, "<"),
            run(Key, "a:b"),
            plain(" "),
            run(Key, "x"),
            run(Punctuation, "="),
            run(String, "\"1\""),
            plain(" "),
            run(Key, "y"),
            run(Punctuation, "="),
            run(String, "'2'"),
            plain(" "),
            run(Value, "flag"),
            run(Punctuation, ">"),
        ]
    );
}

#[test]
fn highlights_the_close_of_open_tags() {
    use TokenKind::*;
    assert_eq!(
        highlighted("<a>"),
        vec![run(Punctuation, "<"), run(Key, "a"), run(Punctuation, ">")]
    );
    assert_eq!(
        highlighted("<a/>"),
        vec![run(Punctuation, "<"), run(Key, "a"), run(Punctuation, "/>")]
    );
    assert_eq!(
        highlighted("<a x=\"1\">"),
        vec![
            run(Punctuation, "<"),
            run(Key, "a"),
            plain(" "),
            run(Key, "x"),
            run(Punctuation, "="),
            run(String, "\"1\""),
            run(Punctuation, ">"),
        ]
    );
}

#[test]
fn highlights_an_end_tag() {
    use TokenKind::*;
    assert_eq!(
        highlighted("</p:a >"),
        vec![run(Punctuation, "</"), run(Key, "p:a"), run(Punctuation, " >")]
    );
}

#[test]
fn highlights_element_text_as_values() {
    use TokenKind::*;
    assert_eq!(
        highlighted("<n>12 true</n>"),
        vec![
            run(Punctuation, "<"),
            run(Key, "n"),
            run(Punctuation, ">"),
            // The words of a text, one run.
            run(Value, "12 true"),
            run(Punctuation, "</"),
            run(Key, "n"),
            run(Punctuation, ">"),
        ]
    );
}

#[test]
fn highlights_comments_whole() {
    use TokenKind::*;
    assert_eq!(
        highlighted("  <!-- <a> --> x"),
        vec![
            plain("  "),
            run(Comment, "<!-- <a> -->"),
            plain(" "),
            run(Value, "x"),
        ]
    );
}

#[test]
fn highlights_declarations_as_punctuation_and_cdata_text_as_a_string() {
    use TokenKind::*;
    assert_eq!(
        highlighted("<?xml version=\"1.0\"?>"),
        vec![run(Punctuation, "<?xml version=\"1.0\"?>")]
    );
    assert_eq!(
        highlighted("<!DOCTYPE x>"),
        vec![run(Punctuation, "<!DOCTYPE x>")]
    );
    assert_eq!(
        highlighted("<![CDATA[<a>]]>"),
        vec![
            run(Punctuation, "<![CDATA["),
            run(String, "<a>"),
            run(Punctuation, "]]>")
        ]
    );
}

#[test]
fn highlighting_covers_every_character_of_tricky_lines() {
    for line in [
        "",
        "   ",
        "<",
        "</",
        "<a",
        "<a x=\"unterminated",
        "<a x='",
        "<!-- open",
        "<![CDATA[ open",
        "<?xml",
        "<a>text</a>   ",
        "<a><b>1</b></a>",
        "<é ü=\"ñ\">日本</é>",
        "<a =\"v\" / >",
        "text < more",
    ] {
        let text: std::string::String =
            highlighted(line).into_iter().map(|(_, text)| text).collect();
        assert_eq!(text, line);
    }
    assert!(highlighted("").is_empty());
}

#[test]
fn nesting_deeper_than_the_limit_is_an_error_not_a_crash() {
    let deep = "<a>".repeat(20_000) + &"</a>".repeat(20_000);
    let error = XmlBehavior.parse(&deep).unwrap_err();
    assert!(error.message.contains("deeper than 128"), "{}", error.message);
    assert_eq!(error.line, Some(1));
    let fine = "<a>".repeat(100) + "1" + &"</a>".repeat(100);
    assert!(XmlBehavior.parse(&fine).is_ok());
}

#[test]
fn an_element_with_text_beside_its_children_writes_back() {
    let value = XmlBehavior.parse("<listen port=\"1\">7835</listen>\n").unwrap();
    assert_eq!(value["listen"]["#text"], "7835");
    let written = XmlBehavior.serialize(&value, None).unwrap();
    assert_eq!(XmlBehavior.parse(&written).unwrap(), value, "{written}");
}


#[test]
fn a_tag_ending_right_after_a_name_splits_into_finite_tokens() {
    let source = "<tcp no></tcp>";
    let tokens = super::tokens::tokenize(source);
    let open = tokens.first().unwrap();
    let inner = super::tokens::tokenize_tag_inside(source, open);
    assert_eq!(inner.len(), 1, "{inner:?}");
    assert_eq!(&source[inner[0].start..inner[0].end], "no");
}

/// Round 11: a document nested far deeper than the parser reads is planned
/// in linear memory (its lines are looked up for string bodies on every
/// render).
#[test]
fn a_very_deep_document_is_planned_without_quadratic_memory() {
    let source = format!("{}{}", "<a>".repeat(20000), "</a>".repeat(20000));
    assert!(XmlBehavior.string_body_lines(&source).is_empty());
}

/// QA 21 (U21-6): the caret's place, read without the spans of every name
/// and value (each keystroke reads it), is the one the full scan finds, at
/// every place of a document written in XML's forms.
#[test]
fn the_caret_scan_without_spans_finds_the_place_the_full_scan_does() {
    use super::scan::scan_text;
    let texts = [
        "<?xml version=\"1.0\"?>\n<config config-root=\"true\" v=\"2\">\n  <listen>7835</listen>\n  <!-- c -->\n  <web secure=\"true\">\n    <http>80</http>\n    <hosts>a</hosts>\n    <hosts>b</hosts>\n    <t><![CDATA[x<y]]></t>\n    <e/>\n  </web>\n  <half a=\n</config>\n",
        "<a><b>1</b><b>2<c x='1'/></b></a>",
        "<a>\n  text\n  <!-- c -->\n  more\n</a\n",
    ];
    let mut failures = Vec::new();
    for text in texts {
        for cursor in (0..=text.len()).filter(|at| text.is_char_boundary(*at)) {
            let full = format!("{:?}", scan_text(text, cursor, true).snapshot);
            let caret = format!("{:?}", scan_text(text, cursor, false).snapshot);
            if full != caret {
                failures.push(format!("{text:?} at {cursor}: {full} vs {caret}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
