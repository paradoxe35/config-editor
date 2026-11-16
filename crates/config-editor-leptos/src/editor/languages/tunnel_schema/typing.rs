//! Typed characters: auto-closed tags and quotes, and the indent unit read
//! while a line is typed.

use crate::editor::languages::Language;

#[test]
fn typing_the_end_of_an_xml_start_tag_writes_its_end_tag() {
    let close = |marked: &str| {
        let at = marked.find('|').unwrap();
        let source = marked.replacen('|', "", 1);
        Language::XML
            .behavior()
            .auto_close(&source, source[..at].encode_utf16().count(), '>')
    };
    assert_eq!(
        close("<clients>\n  <bob|\n</clients>"),
        Some("</bob>".into())
    );
    assert_eq!(close("<web enabled=\"true\"|"), Some("</web>".into()));
    // Nothing to close: end tags, self-closing tags, declarations, comments,
    // `>` inside an attribute value, or the end tag already there.
    for marked in [
        "</bob|",
        "<bob/|",
        "<?xml version=\"1.0\"?|",
        "<!-- a -|",
        "<a b=\"x|",
        "<bob|</bob>",
    ] {
        assert_eq!(close(marked), None, "{marked}");
    }
}

#[test]
fn toml_triple_quotes_close_as_a_multi_line_string() {
    let close = |source: &str, typed: char| {
        Language::TOML
            .behavior()
            .auto_close(source, source.encode_utf16().count(), typed)
    };
    assert_eq!(close("a = \"\"", '"'), Some("\"\"\"".into()));
    assert_eq!(close("a = ''", '\''), Some("'''".into()));
    assert_eq!(close("a = \"", '"'), None);
    assert_eq!(close("a = \"\"\"", '"'), None);
}

#[test]
fn an_xml_end_tag_typed_on_its_own_line_moves_out_one_level() {
    let source = "<r>\n  <a>\n    x\n    </a";
    assert_eq!(
        Language::XML
            .behavior()
            .reindent_on_type(source, source.encode_utf16().count(), '>')
            .as_deref(),
        Some("  ")
    );
    let inline = "<r>\n  <a>x</a";
    assert_eq!(
        Language::XML
            .behavior()
            .reindent_on_type(inline, inline.encode_utf16().count(), '>'),
        None
    );
}

#[test]
fn xml_auto_close_skips_comments_and_reads_quoted_attributes() {
    let close = |marked: &str| {
        let at = marked.find('|').unwrap();
        let source = marked.replacen('|', "", 1);
        Language::XML
            .behavior()
            .auto_close(&source, source[..at].encode_utf16().count(), '>')
    };
    assert_eq!(close("<!-- <b| -->"), None);
    assert_eq!(close("<![CDATA[ <b| ]]>"), None);
    assert_eq!(close("<a b=\"x>\"|"), Some("</a>".into()));
}

#[test]
fn a_line_being_typed_does_not_set_the_indent_unit() {
    assert_eq!(crate::editor::commands::indent_unit("root:\n    "), "  ");
}
