//! What Enter writes: indentation, closers moved to their own line, and lines
//! that cannot span.

use super::assert_parses;
use crate::editor::languages::Language;

/// The document after pressing Enter at `|`, with the caret marked.
fn enter(language: Language, marked: &str) -> String {
    let at = marked.find('|').unwrap();
    let source = marked.replacen('|', "", 1);
    let newline = language
        .behavior()
        .newline(&source, source[..at].encode_utf16().count());
    let mut text = source[..at - newline.replaces_before].to_string();
    text.push_str(&newline.text[..newline.caret]);
    text.push('|');
    text.push_str(&newline.text[newline.caret..]);
    text.push_str(&source[at + newline.replaces..]);
    text
}

#[test]
fn enter_between_brackets_opens_a_middle_line() {
    assert_eq!(enter(Language::JSON, "{\"a\": [|]}"), "{\"a\": [\n  |\n]}");
    assert_eq!(enter(Language::YAML, "ports: [|]\n"), "ports: [\n  |\n]\n");
    assert_eq!(enter(Language::YAML, "web: {|}\n"), "web: {\n  |\n}\n");
    assert_eq!(
        enter(Language::TOML, "ports = [|]\n"),
        "ports = [\n  |\n]\n"
    );
    assert_eq!(enter(Language::HCL, "ports = [|]\n"), "ports = [\n  |\n]\n");
    assert_eq!(enter(Language::HCL, "web {|}\n"), "web {\n  |\n}\n");
}

#[test]
fn enter_before_a_closer_aligns_it_with_its_opener() {
    assert_eq!(
        enter(Language::JSON, "{\n  \"a\": 1|}"),
        "{\n  \"a\": 1\n|}"
    );
    assert_eq!(
        enter(Language::JSON, "{\n  \"a\": [1, 2|]\n}"),
        "{\n  \"a\": [1, 2\n  |]\n}"
    );
    // A bracket inside a string does not count.
    assert_eq!(
        enter(Language::JSON, "{\n  \"a\": \"[\",\n  \"b\": 1|}"),
        "{\n  \"a\": \"[\",\n  \"b\": 1\n|}"
    );
    assert_eq!(
        enter(Language::HCL, "web {\n  a = 1|}\n"),
        "web {\n  a = 1\n|}\n"
    );
}

#[test]
fn toml_enter_steps_out_of_lines_that_cannot_span() {
    // An inline table: the next key belongs to the enclosing table.
    assert_eq!(
        enter(
            Language::TOML,
            "[[web.letsencrypt.dns]]\ncloudflare = { token = \"tok\"| }\n"
        ),
        "[[web.letsencrypt.dns]]\ncloudflare = { token = \"tok\" }\n|\n"
    );
    assert_parses(
        Language::TOML,
        &enter(Language::TOML, "a = { b = { c = 1| } }\n").replace('|', "d = 2"),
    );
    // A table header typed with its auto-closed bracket.
    assert_eq!(enter(Language::TOML, "[web|]\n"), "[web]\n|\n");
    // Inside a string Enter is literal.
    assert_eq!(
        enter(Language::TOML, "a = { b = \"x| }\" }\n"),
        "a = { b = \"x\n| }\" }\n"
    );
}

#[test]
fn enter_keeps_the_indentation_of_a_finished_line() {
    assert_eq!(
        enter(Language::YAML, "limits:\n  max_pending: 512|\n"),
        "limits:\n  max_pending: 512\n  |\n"
    );
    assert_eq!(
        enter(
            Language::XML,
            "<limits>\n  <max_pending>512</max_pending>|\n</limits>\n"
        ),
        "<limits>\n  <max_pending>512</max_pending>\n  |\n</limits>\n"
    );
    // The prolog and comments open nothing.
    assert_eq!(
        enter(Language::XML, "<?xml version=\"1.0\"?>|\n<burrow/>\n"),
        "<?xml version=\"1.0\"?>\n|\n<burrow/>\n"
    );
    assert_eq!(
        enter(Language::XML, "<!-- note -->|\n"),
        "<!-- note -->\n|\n"
    );
}

#[test]
fn xml_enter_before_an_outer_end_tag_outdents_it() {
    assert_eq!(
        enter(Language::XML, "<clients>\n  <acme>\n  </acme>|</clients>"),
        "<clients>\n  <acme>\n  </acme>\n|</clients>"
    );
    assert_eq!(
        enter(Language::XML, "<a>\n  <b>1</b>|</a>"),
        "<a>\n  <b>1</b>\n|</a>"
    );
}

#[test]
fn json_enter_at_the_start_of_a_line_keeps_its_indentation() {
    assert_eq!(
        enter(Language::JSON, "{\n|  \"listen\": 1\n}"),
        "{\n\n|  \"listen\": 1\n}"
    );
}

#[test]
fn yaml_enter_ends_an_empty_item_and_indents_inside_flow_collections_and_node_properties() {
    // An empty item ends the list at the column of what holds it: at the
    // dash's column, under the list, a key is invalid.
    for (marked, ended) in [
        ("hosts:\n  - a\n  - |\n", "hosts:\n  - a\n|\n"),
        ("hosts:\n- a\n- |\n", "hosts:\n- a\n|\n"),
        (
            "m:\n  hosts:\n    - a\n    - |\nn: 1\n",
            "m:\n  hosts:\n    - a\n  |\nn: 1\n",
        ),
        (
            "- name: a\n  hosts:\n    - x\n    - |\n",
            "- name: a\n  hosts:\n    - x\n  |\n",
        ),
        ("- a\n- |\n", "- a\n|\n"),
    ] {
        assert_eq!(enter(Language::YAML, marked), ended, "{marked:?}");
    }
    // Inside a flow collection opened on the line: one level deeper, since
    // YAML refuses the key's own indentation there.
    for (marked, entered) in [
        ("l: [a,| b]\n", "l: [a,\n  | b]\n"),
        ("m:\n  l: [a,|\n", "m:\n  l: [a,\n    |\n"),
        ("- {a: 1,| b: 2}\n", "- {a: 1,\n  | b: 2}\n"),
        ("l: [a, b|]\n", "l: [a, b\n|]\n"),
        ("a: b[1|\n", "a: b[1\n|\n"),
        ("l: [\"a,|\n", "l: [\"a,\n|\n"),
    ] {
        assert_eq!(enter(Language::YAML, marked), entered, "{marked:?}");
    }
    assert_eq!(enter(Language::YAML, "- - a|\n"), "- - a\n  - |\n");
    assert_eq!(enter(Language::YAML, "list: [|\n"), "list: [\n  |\n");
    assert_eq!(
        enter(Language::YAML, "base: &defaults|\n"),
        "base: &defaults\n  |\n"
    );
    assert_eq!(enter(Language::YAML, "a: !!map|\n"), "a: !!map\n  |\n");
}

#[test]
fn enter_between_brackets_inside_a_string_is_a_plain_newline() {
    assert_eq!(
        enter(Language::JSON, "{\"p\": \"[|]\"}"),
        "{\"p\": \"[\n|]\"}"
    );
    assert_eq!(enter(Language::TOML, "re = \"[|]\"\n"), "re = \"[\n|]\"\n");
    assert_eq!(enter(Language::HCL, "# block {|\n"), "# block {\n|\n");
}

#[test]
fn xml_enter_before_an_end_tag_in_text_splits_the_line() {
    // Stepping past an end tag is the editor's, only for tags it wrote.
    assert_eq!(
        enter(
            Language::XML,
            "<services>\n  <mode>balance|</mode>\n</services>"
        ),
        "<services>\n  <mode>balance\n  |</mode>\n</services>"
    );
    // An empty element still opens a block.
    assert_eq!(enter(Language::XML, "<web>|</web>"), "<web>\n  |\n</web>");
}

#[test]
fn enter_inside_a_one_line_bracket_indents_one_level() {
    assert_eq!(
        enter(Language::JSON, "{\n  \"listen\": [1,| 2]\n}"),
        "{\n  \"listen\": [1,\n    | 2]\n}"
    );
    assert_eq!(enter(Language::TOML, "a = [1,| 2]\n"), "a = [1,\n  | 2]\n");
    assert_eq!(
        enter(Language::HCL, "a = {x = 1,| y = 2}\n"),
        "a = {x = 1,\n  | y = 2}\n"
    );
}

#[test]
fn yaml_enter_after_block_scalars_anchors_and_tags_indents_their_content() {
    // (line typed, text Enter writes): the caret is at the end of `source`.
    let cases = [
        ("args:\n  - |", "\n    "),
        ("args:\n  - >", "\n    "),
        ("args:\n  - |-", "\n    "),
        ("args:\n  - &x", "\n    "),
        ("args:\n  - !!map", "\n    "),
        ("script: &run |", "\n  "),
        ("a: !!str |", "\n  "),
        ("a: &x >-", "\n  "),
        ("list:\n  - a: &x |", "\n      "),
        // Unchanged: a plain block scalar.
        ("a: |", "\n  "),
        ("list:\n  - a: |", "\n      "),
    ];
    for (source, expected) in cases {
        let newline = Language::YAML
            .behavior()
            .newline(source, source.encode_utf16().count());
        assert_eq!(newline.text, expected, "Enter after {source:?}");
    }
}

#[test]
fn enter_in_a_one_line_hcl_block_opens_it_over_lines() {
    let source = "tcp { nodelay = true }\n";
    let caret = source.find(" }").unwrap();
    let newline = Language::HCL.behavior().newline(source, caret);
    let byte = caret;
    let mut updated = source.to_string();
    updated.replace_range(
        byte - newline.replaces_before..byte + newline.replaces,
        &newline.text,
    );
    assert_eq!(updated, "tcp {\n  nodelay = true\n  \n}\n");
}

/// Enter inside a YAML scalar that may run over lines (quoted, or plain
/// with text after the caret) indents the new line so it continues the
/// scalar and the document stays valid; elsewhere Enter is unchanged.
#[test]
fn enter_inside_a_yaml_scalar_continues_it() {
    let cases = [
        (
            "key: \"abc|def\"\n",
            "key: \"abc\n  |def\"\n",
            "/key",
            "abc def",
        ),
        (
            "outer:\n  key: 'ab|c'\n",
            "outer:\n  key: 'ab\n    |c'\n",
            "/outer/key",
            "ab c",
        ),
        (
            "key: \"abc\n  de|f\"\n",
            "key: \"abc\n  de\n  |f\"\n",
            "/key",
            "abc de f",
        ),
        (
            "key: hello wo|rld\n",
            "key: hello wo\n  |rld\n",
            "/key",
            "hello wo rld",
        ),
        (
            "list:\n  - \"a|b\"\n",
            "list:\n  - \"a\n    |b\"\n",
            "/list/0",
            "a b",
        ),
        (
            "- key: hel|lo\n",
            "- key: hel\n    |lo\n",
            "/0/key",
            "hel lo",
        ),
        ("- - a|b\n", "- - a\n    |b\n", "/0/0", "a b"),
        // At the end of a plain value's line when the next line continues
        // it: at that line's indent, not at column 0.
        (
            "ml: l1|\n  l2  # mc\nn: 1\n",
            "ml: l1\n  |\n  l2  # mc\nn: 1\n",
            "/ml",
            "l1\nl2",
        ),
        (
            "m:\n  ml: l1|\n      l2\n",
            "m:\n  ml: l1\n      |\n      l2\n",
            "/m/ml",
            "l1\nl2",
        ),
    ];
    let mut failures = Vec::new();
    for (marked, expected, pointer, value) in cases {
        let actual = enter(Language::YAML, marked);
        let parsed = Language::YAML.parse(&actual.replace('|', ""));
        let got = parsed
            .as_ref()
            .ok()
            .and_then(|parsed| parsed.pointer(pointer).cloned());
        if actual != expected || got != Some(serde_json::json!(value)) {
            failures.push(format!("{marked:?}: {actual:?} -> {parsed:?}"));
        }
    }
    // Outside a scalar, Enter is unchanged.
    for (marked, expected) in [
        ("key: value|\n", "key: value\n|\n"),
        ("key: v|  # c\n", "key: v\n|  # c\n"),
        ("key: \"done\"|\n", "key: \"done\"\n|\n"),
        // In a flow collection, one level deeper, no spacing
        // left at the line's end.
        ("key: [a, |b]\n", "key: [a,\n  |b]\n"),
    ] {
        let actual = enter(Language::YAML, marked);
        if actual != expected {
            failures.push(format!("{marked:?}: {actual:?}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
