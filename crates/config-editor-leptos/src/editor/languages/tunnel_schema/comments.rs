//! Comment toggling: where comment markers go and where the caret and
//! selection end up.

use crate::editor::languages::Language;

/// Toggles comments over the selection marked by `[` and `]` (or the caret
/// `|`) and returns the result marked the same way.
fn toggle(language: Language, marked: &str) -> String {
    let (start, end) = match marked.find('|') {
        Some(at) => (at, at),
        None => (marked.find('[').unwrap(), marked.find(']').unwrap() - 1),
    };
    let source = marked
        .replacen('|', "", 1)
        .replacen('[', "", 1)
        .replacen(']', "", 1);
    let utf16 = |byte: usize| source[..byte].encode_utf16().count();
    let (updated, from, to) = language
        .behavior()
        .toggle_comments(&source, utf16(start), utf16(end))
        .unwrap();
    let byte = |offset: usize| {
        updated
            .char_indices()
            .map(|(index, _)| index)
            .chain([updated.len()])
            .find(|index| updated[..*index].encode_utf16().count() == offset)
            .unwrap()
    };
    let (from, to) = (byte(from), byte(to));
    if from == to {
        format!("{}|{}", &updated[..from], &updated[from..])
    } else {
        format!(
            "{}[{}]{}",
            &updated[..from],
            &updated[from..to],
            &updated[to..]
        )
    }
}

#[test]
fn xml_comment_toggling_keeps_the_caret_and_selection_on_the_text() {
    assert_eq!(
        toggle(Language::XML, "<a>\n  <b>|1</b>\n</a>"),
        "<a>\n  <!-- <b>|1</b> -->\n</a>"
    );
    assert_eq!(
        toggle(Language::XML, "<a>\n  <!-- <b>|1</b> -->\n</a>"),
        "<a>\n  <b>|1</b>\n</a>"
    );
    let commented = toggle(Language::XML, "<a>\n[  <b>1</b>\n  <c>2</c>\n]</a>");
    assert_eq!(
        commented,
        "<a>\n[  <!-- <b>1</b> -->\n  <!-- <c>2</c> -->\n]</a>"
    );
    assert_eq!(
        toggle(Language::XML, &commented),
        "<a>\n[  <b>1</b>\n  <c>2</c>\n]</a>"
    );
}

#[test]
fn comments_go_at_the_blocks_smallest_indent_and_round_trip() {
    let commented = toggle(Language::YAML, "[clients:\n  acme:\n    secret: x\n]");
    assert_eq!(commented, "[# clients:\n#   acme:\n#     secret: x\n]");
    assert_eq!(
        toggle(Language::YAML, &commented),
        "[clients:\n  acme:\n    secret: x\n]"
    );
    assert_eq!(
        toggle(Language::TOML, "x = 0\n[  a = 1\n    b = 2\n]"),
        "x = 0\n[  # a = 1\n  #   b = 2\n]"
    );
}

#[test]
fn hcl_slash_comments_uncomment() {
    assert_eq!(toggle(Language::HCL, "// x|"), "x|");
    assert_eq!(toggle(Language::HCL, "x = 1|"), "# x = 1|");
}

#[test]
fn comment_toggling_on_a_blank_line_starts_a_comment() {
    assert_eq!(toggle(Language::YAML, "a: 1\n  |\n"), "a: 1\n  # |\n");
    assert_eq!(toggle(Language::TOML, "|"), "# |");
}

#[test]
fn xml_comment_toggling_inside_a_multi_line_comment_removes_it() {
    assert_eq!(
        toggle(Language::XML, "<a>\n<!-- <b/>\n  <c|/> -->\n</a>"),
        "<a>\n<b/>\n  <c|/>\n</a>"
    );
    assert_eq!(toggle(Language::XML, "<!--\n  <a/>|\n  -->\n"), "  <a/>|\n");
}
