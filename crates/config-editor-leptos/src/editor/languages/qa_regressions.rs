//! Regressions from hands-on QA of the demo: each test is a reported case,
//! replayed against the real burrow schema.

use super::super::completion::{accept_marked, completions};
use super::super::diagnostics::{SchemaBundle, build_schema};
use super::Language;

fn burrow() -> SchemaBundle {
    build_schema(include_str!("../../../tests/fixtures/burrow.schema.json")).unwrap()
}

fn accept(language: Language, marked: &str, label: &str) -> String {
    accept_marked(marked, label, &burrow(), language.behavior())
}

fn labels(language: Language, marked: &str) -> Vec<String> {
    let at = marked.find('|').unwrap();
    let source = marked.replacen('|', "", 1);
    completions(
        &source,
        source[..at].encode_utf16().count(),
        true,
        Some(&burrow()),
        true,
        language.behavior(),
    )
    .into_iter()
    .map(|option| option.label)
    .collect()
}

#[track_caller]
fn assert_parses(language: Language, text: &str) {
    if let Err(error) = language.parse(&text.replace('|', "")) {
        panic!("{language} does not parse: {error}\n{text}");
    }
}

#[test]
fn a_key_typed_in_front_of_an_existing_entry_is_inserted_before_it() {
    let yaml = accept(Language::YAML, "lim|listen: 7835\n", "limits");
    assert!(yaml.starts_with("limits:"), "{yaml}");
    assert!(
        yaml.contains("\nlisten: 7835\n"),
        "the existing entry is kept: {yaml}"
    );
    let toml = accept(Language::TOML, "lim|listen = 7835\n", "limits");
    assert!(toml.contains("\nlisten = 7835\n"), "{toml}");
    assert!(
        !toml.contains("[limits]"),
        "no header that would capture listen: {toml}"
    );
    assert_parses(Language::TOML, &toml);
    let hcl = accept(Language::HCL, "lim|listen = 7835\n", "limits");
    assert!(hcl.contains("\nlisten = 7835\n"), "{hcl}");
    assert_parses(Language::HCL, &hcl);
}

#[test]
fn completing_the_key_the_caret_is_inside_renames_it_and_keeps_the_value() {
    assert_eq!(
        accept(
            Language::YAML,
            "clients:\n  acme:\n    max_con|nections: 5\n",
            "max_connections"
        ),
        "clients:\n  acme:\n    max_connections|: 5\n"
    );
    assert_eq!(
        accept(
            Language::TOML,
            "[clients.acme]\nmax_con|nections = 5\n",
            "max_connections"
        ),
        "[clients.acme]\nmax_connections| = 5\n"
    );
    // A typo inside the key: the suggestion ends with the word after the caret.
    assert_eq!(
        accept(
            Language::YAML,
            "inse|ecure_plaintext: false",
            "insecure_plaintext"
        ),
        "insecure_plaintext|: false"
    );
}

#[test]
fn a_toml_table_in_the_middle_of_a_table_is_written_inline() {
    let toml = accept(
        Language::TOML,
        "[web]\nhsts = true\n|\ncompress = true\n",
        "letsencrypt",
    );
    assert_eq!(
        toml,
        "[web]\nhsts = true\nletsencrypt = { email = \"|\" }\ncompress = true\n"
    );
    let value = Language::TOML.parse(&toml.replace('|', "a@b.c")).unwrap();
    assert_eq!(value["web"]["compress"], true, "compress stays in [web]");
}

#[test]
fn an_hcl_object_without_required_keys_puts_the_caret_inside() {
    assert_eq!(
        accept(Language::HCL, "lim|\n", "limits"),
        "limits {\n  |\n}\n"
    );
}

#[test]
fn the_first_key_of_an_empty_json_document_opens_the_root_object() {
    let json = accept(Language::JSON, "|", "limits");
    assert_eq!(json, "{\n  \"limits\": {|}\n}\n");
    assert_parses(Language::JSON, &json);
}

#[test]
fn string_values_start_quoted_with_the_caret_inside() {
    assert_eq!(
        accept(Language::JSON, "{\n  \"pub|\n}", "public_address"),
        "{\n  \"public_address\": \"|\"\n}"
    );
    assert_eq!(
        accept(Language::TOML, "pub|", "public_address"),
        "public_address = \"|\""
    );
    assert_eq!(
        accept(Language::HCL, "pub|", "public_address"),
        "public_address = \"|\""
    );
    // YAML needs no quotes.
    assert_eq!(
        accept(Language::YAML, "pub|", "public_address"),
        "public_address: |"
    );
}

#[test]
fn an_empty_xml_object_element_offers_its_children() {
    let offered = labels(Language::XML, "<web>|</web>");
    assert!(offered.contains(&"letsencrypt".to_string()), "{offered:?}");
}

#[test]
fn an_hcl_value_after_a_space_gets_no_second_space() {
    let hcl = accept(
        Language::HCL,
        "services {\n  web {\n    mode = |\n  }\n}\n",
        "balance",
    );
    assert!(hcl.contains("mode = \"balance\""), "{hcl}");
}

#[test]
fn a_yaml_block_insert_before_existing_content_adds_no_blank_line() {
    let yaml = accept(Language::YAML, "metr|\nlisten: 1\n", "metrics");
    assert!(!yaml.contains("\n\n"), "{yaml}");
    assert!(yaml.ends_with("\nlisten: 1\n"), "{yaml}");
}

#[test]
fn enum_values_are_offered_in_schema_order() {
    let offered = labels(Language::YAML, "services:\n  web:\n    mode: |\n");
    assert_eq!(
        offered[..4],
        ["failover", "balance", "least_connections", "ip_hash"]
    );
}

#[test]
fn containers_show_hover_docs_from_their_header_or_tag() {
    use super::super::completion::key_hover;
    let bundle = burrow();
    let cases: &[(Language, &str, usize, &str)] = &[
        (
            Language::TOML,
            "[web.letsencrypt]\nemail = \"a@b.c\"\n",
            0,
            "letsencrypt",
        ),
        (Language::TOML, "[websocket]\nlisten = 1\n", 0, "websocket"),
        (
            Language::HCL,
            "web {\n  letsencrypt {\n    email = \"a@b.c\"\n  }\n}\n",
            1,
            "letsencrypt",
        ),
        (
            Language::XML,
            "<web><letsencrypt email=\"a@b.c\"/></web>\n",
            0,
            "letsencrypt",
        ),
    ];
    for &(language, source, line, key) in cases {
        let text = source.split('\n').nth(line).unwrap();
        let column = text.find(key).unwrap() + 1;
        let hover = key_hover(source, line, column, Some(&bundle), language.behavior());
        assert!(
            hover
                .as_ref()
                .is_some_and(|hover| hover.key == key && hover.description.is_some()),
            "{language}: no docs for `{key}` in {source:?}"
        );
    }
}

#[test]
fn every_unknown_key_gets_its_own_marker() {
    use super::super::diagnostics::validate;
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
fn an_xml_start_tag_missing_its_bracket_is_reported_on_its_own_line() {
    let source = "<listen>1</listen>\n<insecure_plaintextfalse</insecure_plaintext>\n<pool>\n  <min_idle>4</min_idle>\n</pool>\n";
    let error = Language::XML.parse(source).unwrap_err();
    assert_eq!(error.line, Some(2), "{error}");
}

#[test]
fn hcl_syntax_errors_read_as_a_sentence() {
    let error = Language::HCL
        .parse("listen = 1\nhealth_check : true\n")
        .unwrap_err();
    assert!(
        !error.message.contains("-->") && !error.message.contains('|'),
        "{}",
        error.message
    );
    assert_eq!(error.line, Some(2));
}

#[test]
fn an_xml_array_key_writes_one_item_that_validates_as_a_one_item_array() {
    let marked =
        "<web>\n  <letsencrypt>\n    <email>a@b.c</email>\n    <dn|\n  </letsencrypt>\n</web>\n";
    let xml = accept(Language::XML, marked, "dns");
    assert!(xml.contains("    <dns>|</dns>\n"), "{xml}");
    // The element is one item: the item's keys are offered inside it.
    assert!(labels(Language::XML, &xml).contains(&"cloudflare".to_string()));
    let done = accept(Language::XML, &xml, "cloudflare").replace('|', "x");
    let schema =
        crate::Schema::compile(include_str!("../../../tests/fixtures/burrow.schema.json")).unwrap();
    assert_eq!(schema.validate(Language::XML, &done), [], "{done}");
    // A second item makes a two-item array.
    let two = done.replace("</dns>", "</dns>\n    <dns><domains>b.example</domains><cloudflare><token>y</token></cloudflare></dns>");
    assert_eq!(schema.validate(Language::XML, &two), [], "{two}");
}

#[test]
fn an_xml_client_secret_list_accepts_one_secret() {
    let marked = "<clients>\n  <bob>\n    <sec|\n  </bob>\n</clients>\n";
    let xml = accept(Language::XML, marked, "secret");
    assert!(xml.contains("    <secret>|</secret>\n"), "{xml}");
    let done = xml.replace('|', "s3cret");
    let schema =
        crate::Schema::compile(include_str!("../../../tests/fixtures/burrow.schema.json")).unwrap();
    assert_eq!(schema.validate(Language::XML, &done), [], "{done}");
}

/// The text left selected after accepting `label` at the caret.
fn selected_after_accept(language: Language, marked: &str, label: &str) -> String {
    let at = marked.find('|').unwrap();
    let source = marked.replacen('|', "", 1);
    let options = completions(
        &source,
        source[..at].encode_utf16().count(),
        true,
        Some(&burrow()),
        true,
        language.behavior(),
    );
    let option = options.iter().find(|option| option.label == label).unwrap();
    let (updated, anchor, caret) = super::super::completion::apply_completion(&source, option);
    String::from_utf16(&updated.encode_utf16().collect::<Vec<_>>()[anchor..caret]).unwrap()
}

#[test]
fn an_inserted_default_is_selected_so_typing_replaces_it() {
    for (language, marked) in [
        (Language::YAML, "limits:\n  max_p|\n"),
        (Language::TOML, "[limits]\nmax_p|\n"),
        (Language::HCL, "limits {\n  max_p|\n}\n"),
        (Language::JSON, "{\n  \"limits\": {\n    \"max_p|\n  }\n}\n"),
        (Language::XML, "<limits>\n  <max_p|\n</limits>\n"),
    ] {
        assert_eq!(
            selected_after_accept(language, marked, "max_pending"),
            "512",
            "{language}"
        );
    }
    // A quoted default is selected inside its quotes.
    for (language, marked) in [
        (Language::TOML, "[websocket]\npa|\n"),
        (Language::JSON, "{\n  \"websocket\": {\n    \"pa|\n  }\n}\n"),
        (Language::HCL, "websocket {\n  pa|\n}\n"),
        (Language::YAML, "websocket:\n  pa|\n"),
        (Language::XML, "<websocket>\n  <pa|\n</websocket>\n"),
    ] {
        assert_eq!(
            selected_after_accept(language, marked, "path"),
            "/burrow",
            "{language}"
        );
    }
    // A key without a default leaves nothing selected.
    assert_eq!(
        selected_after_accept(Language::YAML, "web:\n  hst|\n", "hsts"),
        ""
    );
}

#[test]
fn typing_a_number_does_not_open_value_suggestions_unless_asked() {
    let auto = |language: Language, marked: &str| {
        let at = marked.find('|').unwrap();
        let source = marked.replacen('|', "", 1);
        completions(
            &source,
            source[..at].encode_utf16().count(),
            false,
            Some(&burrow()),
            true,
            language.behavior(),
        )
        .len()
    };
    assert_eq!(auto(Language::YAML, "limits:\n  max_pending: 5|\n"), 0);
    assert_eq!(auto(Language::TOML, "[limits]\nmax_pending = 5|\n"), 0);
    // Asked for, they are listed.
    assert!(!labels(Language::YAML, "limits:\n  max_pending: 5|\n").is_empty());
}

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
fn hcl_keys_are_offered_after_a_sibling_in_a_multi_line_object_value() {
    let offered = labels(Language::HCL, "web = {\n  hsts = true\n  le|\n}\n");
    assert!(offered.contains(&"letsencrypt".to_string()), "{offered:?}");
    assert!(
        !labels(Language::HCL, "web = {\n  hsts = true\n  |\n}\n").contains(&"hsts".to_string())
    );
}

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
fn yaml_tab_indentation_is_a_syntax_error_outside_block_scalars() {
    let error = Language::YAML
        .parse("regions:\n  - a\n\t  - b\n")
        .unwrap_err();
    assert_eq!((error.line, error.column), (Some(3), Some(1)));
    assert!(error.message.contains("tabs"), "{}", error.message);
    // Tabs inside a block scalar are text.
    assert!(
        Language::YAML
            .parse("note: |\n  a\n  \tb\nnext: 1\n")
            .is_ok()
    );
    // A tab after the indentation is fine.
    assert!(Language::YAML.parse("a:\t1\n").is_ok());
}

#[test]
fn toml_table_accepted_before_another_table_starts_its_own_lines() {
    let toml = accept(
        Language::TOML,
        "listen = 1\n|\n[tcp]\nnodelay = true\n",
        "pool",
    );
    assert_eq!(toml, "listen = 1\n[pool]\n|\n[tcp]\nnodelay = true\n");
    // The next accept lands on its own line under the header.
    let next = accept(Language::TOML, &toml, "max_idle");
    assert!(next.contains("[pool]\nmax_idle = "), "{next}");
    assert_parses(Language::TOML, &next);
}

#[test]
fn a_json_list_of_strings_starts_with_a_quoted_item() {
    let json = accept(
        Language::JSON,
        "{\n  \"clients\": {\n    \"bob\": {\n      \"sec|\n    }\n  }\n}\n",
        "secret",
    );
    assert!(
        json.contains("\"secret\": [\"|\"]") || json.contains("\"secret\": \"|\""),
        "{json}"
    );
    assert_parses(Language::JSON, &json.replace('|', "s"));
}

#[test]
fn a_yaml_file_of_only_comments_is_an_empty_document() {
    for source in ["# just a note\n", "---\n# note\n", "---\n...\n"] {
        assert_eq!(
            Language::YAML.parse(source).unwrap(),
            serde_json::json!({}),
            "{source:?}"
        );
    }
}

#[test]
fn every_yaml_syntax_error_has_a_place() {
    let problems = crate::check_syntax(Language::YAML, "listen: 1\n---\nlisten: 2\n");
    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].line, Some(2), "{}", problems[0].message);
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
fn a_word_typed_in_an_empty_xml_object_element_completes_a_child() {
    let marked = "<web>certif|</web>";
    assert!(labels(Language::XML, marked).contains(&"certificates".to_string()));
    let xml = accept(Language::XML, marked, "certificates");
    assert!(xml.contains("<certificates>"), "{xml}");
    assert!(
        !xml.contains("certif<"),
        "the typed word is replaced: {xml}"
    );
    assert_parses(Language::XML, &xml);
    // A scalar element keeps its value completions.
    assert!(
        !labels(
            Language::XML,
            "<services><web><mode>bal|</mode></web></services>"
        )
        .is_empty()
    );
}
#[test]
fn keys_already_set_in_list_items_are_not_offered_again() {
    let hcl_multi = labels(
        Language::HCL,
        "web {\n  certificates = [\n    {\n      hosts = [\"a\"]\n      |\n    }\n  ]\n}\n",
    );
    assert!(hcl_multi.contains(&"cert".to_string()), "{hcl_multi:?}");
    assert!(!hcl_multi.contains(&"hosts".to_string()), "{hcl_multi:?}");
    let hcl_inline = labels(
        Language::HCL,
        "web {\n  certificates = [{ hosts = [\"a\"], | }]\n}\n",
    );
    assert!(!hcl_inline.contains(&"hosts".to_string()), "{hcl_inline:?}");
    let hcl_dns = labels(
        Language::HCL,
        "web {\n  letsencrypt {\n    email = \"a\"\n    dns = [\n      {\n        domains = [\"a.com\"]\n        wait_secs = 5\n        |\n      },\n    ]\n  }\n}\n",
    );
    assert!(
        !hcl_dns.contains(&"domains".to_string()) && !hcl_dns.contains(&"wait_secs".to_string()),
        "{hcl_dns:?}"
    );
    let toml = labels(
        Language::TOML,
        "[[web.letsencrypt.dns]]\ndomains = [\"x\"]\n|\n\n[web.letsencrypt.dns.cloudflare]\ntoken = \"t\"\n",
    );
    assert!(!toml.contains(&"cloudflare".to_string()), "{toml:?}");
    assert!(toml.contains(&"rfc2136".to_string()), "{toml:?}");
}

#[test]
fn a_single_hcl_block_for_a_list_is_a_one_item_list() {
    let schema =
        crate::Schema::compile(include_str!("../../../tests/fixtures/burrow.schema.json")).unwrap();
    let one =
        "web {\n  certificates {\n    hosts = [\"a\"]\n    cert = \"c\"\n    key = \"k\"\n  }\n}\n";
    assert_eq!(schema.validate(Language::HCL, one), []);
    // Inside the block, the item's keys are offered.
    let inside = "web {\n  certificates {\n    hosts = [\"a\"]\n    |\n  }\n}\n";
    assert!(labels(Language::HCL, inside).contains(&"cert".to_string()));
    // A scalar where a list belongs is still an error.
    assert_eq!(
        schema
            .validate(Language::HCL, "self {\n  allow = \"a\"\n}\n")
            .len(),
        1
    );
}
#[test]
fn errors_inside_inline_containers_point_at_the_value() {
    let schema =
        crate::Schema::compile(include_str!("../../../tests/fixtures/burrow.schema.json")).unwrap();
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

fn hover_key_at(language: Language, source: &str, line: usize, column: usize) -> Option<String> {
    super::super::completion::key_hover(source, line, column, Some(&burrow()), language.behavior())
        .map(|hover| hover.key)
}

#[test]
fn hovering_a_segment_of_a_dotted_key_or_header_shows_that_segment() {
    let toml = "[[web.certificates]]\nhosts = [\"a\"]\n";
    assert_eq!(
        hover_key_at(Language::TOML, toml, 0, 3).as_deref(),
        Some("web")
    );
    assert_eq!(
        hover_key_at(Language::TOML, toml, 0, 8).as_deref(),
        Some("certificates")
    );
    let services = "[services.web.health_check]\ninterval_secs = 5\n";
    assert_eq!(
        hover_key_at(Language::TOML, services, 0, 2).as_deref(),
        Some("services")
    );
    assert_eq!(
        hover_key_at(Language::TOML, services, 0, 15).as_deref(),
        Some("health_check")
    );
    let dotted = "websocket.listen = 1\n";
    assert_eq!(
        hover_key_at(Language::TOML, dotted, 0, 2).as_deref(),
        Some("websocket")
    );
    assert_eq!(
        hover_key_at(Language::TOML, dotted, 0, 12).as_deref(),
        Some("listen")
    );
    let hcl = "services \"web\" {\n  target = 80\n}\n";
    assert_eq!(
        hover_key_at(Language::HCL, hcl, 0, 2).as_deref(),
        Some("services")
    );
}

/// Span lookups run per diagnostic and per ancestor of it; on a 2000-line
/// document each must stay far below a frame budget, in every language.
#[test]
fn span_lookups_on_large_documents_are_fast() {
    let mut clients = serde_json::Map::new();
    for index in 0..1500 {
        clients.insert(
            format!("client-{index}"),
            serde_json::json!({"secret": format!("s{index}"), "max_connections": index % 50}),
        );
    }
    let document = serde_json::json!({
        "clients": clients,
        "web": {"letsencrypt": {"email": "a@b.c", "dns": [
            {"domains": ["a.example", "c.example"], "wait_secs": 60},
            {"domains": ["b.example", "d.example"], "wait_secs": 120}
        ]}}
    });
    for language in Language::ALL {
        let text = language.serialize(&document).unwrap();
        assert!(text.lines().count() > 2000, "{language}");
        for pointer in [
            "/web/letsencrypt/dns/1/wait_secs",
            "/web/letsencrypt/dns/1",
            "/web",
            "/missing/key",
        ] {
            let (elapsed, ()) = crate::editor::test_timing::best_of(5, || {
                let _ = language.behavior().value_span(&text, pointer);
                let _ = language.behavior().key_span(&text, pointer);
            });
            assert!(
                elapsed < std::time::Duration::from_millis(150),
                "{language} {pointer}: {elapsed:?}"
            );
        }
    }
}

#[test]
fn json_errors_point_at_the_problem_without_repeating_the_position() {
    let error = Language::JSON
        .parse("{\n  \"listen\": 7835,\n}")
        .unwrap_err();
    assert_eq!(
        (error.line, error.column),
        (Some(2), Some(17)),
        "{}",
        error.message
    );
    assert!(!error.message.contains(" at line "), "{}", error.message);
}

#[test]
fn an_empty_xml_element_is_empty_text_where_text_is_expected() {
    let schema =
        crate::Schema::compile(include_str!("../../../tests/fixtures/burrow.schema.json")).unwrap();
    assert_eq!(
        schema.validate(Language::XML, "<public_address></public_address>"),
        []
    );
    let mixed = schema.validate(Language::XML, "<web>stray<hsts>true</hsts></web>");
    assert!(
        mixed
            .iter()
            .any(|problem| problem.message.contains("text is not allowed")),
        "{mixed:?}"
    );
}

#[test]
fn comment_toggling_on_a_blank_line_starts_a_comment() {
    assert_eq!(toggle(Language::YAML, "a: 1\n  |\n"), "a: 1\n  # |\n");
    assert_eq!(toggle(Language::TOML, "|"), "# |");
}

#[test]
fn yaml_enter_rules_for_lists_flow_and_node_properties() {
    // An empty item ends the list at the column of what holds it (round 21:
    // it stayed at the dash's, under the list, where a key is invalid).
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
    // Inside a flow collection opened on the line: one level deeper (round
    // 21: the new line had the key's indentation, which YAML refuses).
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
fn a_line_being_typed_does_not_set_the_indent_unit() {
    assert_eq!(crate::editor::commands::indent_unit("root:\n    "), "  ");
}

#[test]
fn xml_comment_toggling_inside_a_multi_line_comment_removes_it() {
    assert_eq!(
        toggle(Language::XML, "<a>\n<!-- <b/>\n  <c|/> -->\n</a>"),
        "<a>\n<b/>\n  <c|/>\n</a>"
    );
    assert_eq!(toggle(Language::XML, "<!--\n  <a/>|\n  -->\n"), "  <a/>|\n");
}

#[test]
fn nothing_is_suggested_in_comments_and_toml_gets_no_null() {
    assert!(labels(Language::YAML, "# po|\n").is_empty());
    assert!(labels(Language::TOML, "# po|\n").is_empty());
    assert!(labels(Language::HCL, "# po|\n").is_empty());
    assert!(!labels(Language::TOML, "public_address = |\n").contains(&"null".to_string()));
    assert!(labels(Language::YAML, "public_address: |\n").contains(&"null".to_string()));
}

#[test]
fn completing_a_key_that_has_a_value_renames_only_the_key() {
    assert_eq!(
        accept(Language::YAML, "tcp:\n  keep|: 99\n", "keepalive_secs"),
        "tcp:\n  keepalive_secs|: 99\n"
    );
    assert_eq!(
        accept(
            Language::JSON,
            "{\n  \"publ|\": \"my.host\"\n}",
            "public_address"
        ),
        "{\n  \"public_address\"|: \"my.host\"\n}"
    );
    assert_eq!(
        accept(Language::TOML, "[tcp]\nkeep| = 99\n", "keepalive_secs"),
        "[tcp]\nkeepalive_secs| = 99\n"
    );
    assert_eq!(
        accept(
            Language::TOML,
            "hea| = { interval_secs = 5 }\n",
            "heartbeat"
        ),
        "heartbeat| = { interval_secs = 5 }\n"
    );
    assert_eq!(
        accept(Language::HCL, "tcp {\n  keep| = 99\n}\n", "keepalive_secs"),
        "tcp {\n  keepalive_secs| = 99\n}\n"
    );
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
fn an_xml_value_accepted_mid_word_replaces_the_whole_value() {
    assert_eq!(
        accept(
            Language::XML,
            "<services><web><mode>fail|over</mode></web></services>",
            "failover"
        ),
        "<services><web><mode>failover|</mode></web></services>"
    );
}

#[test]
fn nothing_is_offered_after_the_json_root_closes() {
    assert!(labels(Language::JSON, "{\n  \"listen\": 1\n}|").is_empty());
    assert!(labels(Language::JSON, "{\n  \"listen\": 1\n}\nli|").is_empty());
    assert!(!labels(Language::JSON, "{\n  |\n}").is_empty());
}
#[test]
fn keys_not_values_on_a_new_line_after_the_last_hcl_attribute() {
    let offered = labels(Language::HCL, "listen = 7835\n|");
    assert!(offered.contains(&"limits".to_string()), "{offered:?}");
    assert!(!offered.contains(&"7835".to_string()));
}

#[test]
fn after_an_xml_end_tag_the_parent_is_completed() {
    let offered = labels(Language::XML, "<pool>\n  <min_idle>4</min_idle>\n</pool>|");
    assert!(offered.contains(&"limits".to_string()), "{offered:?}");
    assert!(!offered.contains(&"max_idle".to_string()));
}
#[test]
fn toml_table_headers_complete_their_segment_with_tables_only() {
    assert_eq!(labels(Language::TOML, "[hea|]\n"), ["heartbeat"]);
    assert_eq!(labels(Language::TOML, "[web.lets|]\n"), ["letsencrypt"]);
    assert!(!labels(Language::TOML, "[|]\n").contains(&"listen".to_string()));
    assert_eq!(
        accept(Language::TOML, "[hea|]\n", "heartbeat"),
        "[heartbeat|]\n"
    );
    assert_eq!(
        accept(Language::TOML, "[[web.cert|]]\n", "certificates"),
        "[[web.certificates|]]\n"
    );
}

#[test]
fn a_key_after_a_value_without_a_comma_in_a_one_line_object_gets_one() {
    assert!(
        labels(Language::TOML, "pool = { min_idle = 4 | }\n").contains(&"max_idle".to_string())
    );
    let toml = accept(Language::TOML, "pool = { min_idle = 4 | }\n", "max_idle");
    assert!(
        toml.starts_with("pool = { min_idle = 4, max_idle = "),
        "{toml}"
    );
    assert_parses(Language::TOML, &toml);
    let hcl = accept(Language::HCL, "pool = { min_idle = 4 | }\n", "max_idle");
    assert!(
        hcl.starts_with("pool = { min_idle = 4, max_idle = "),
        "{hcl}"
    );
    assert_parses(Language::HCL, &hcl);
    let yaml = accept(Language::YAML, "pool: {min_idle: 4 |}\n", "max_idle");
    assert!(yaml.starts_with("pool: {min_idle: 4, max_idle: "), "{yaml}");
    assert_parses(Language::YAML, &yaml);
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
fn a_space_after_a_complete_word_offers_nothing() {
    let auto = |language: Language, marked: &str| {
        let at = marked.find('|').unwrap();
        let source = marked.replacen('|', "", 1);
        completions(
            &source,
            source[..at].encode_utf16().count(),
            false,
            Some(&burrow()),
            true,
            language.behavior(),
        )
        .len()
    };
    assert_eq!(auto(Language::HCL, "insecure_plaintext = false |"), 0);
    assert_eq!(auto(Language::HCL, "listen = 7835 |"), 0);
    assert_eq!(auto(Language::TOML, "insecure_plaintext = f |"), 0);
    assert_eq!(auto(Language::YAML, "insecure_plaintext: false |"), 0);
    assert_eq!(auto(Language::YAML, "pu |"), 0);
    // Right after the separator, values are still offered.
    assert!(
        auto(Language::YAML, "insecure_plaintext: |") > 0
            || auto(Language::HCL, "insecure_plaintext = |") > 0
    );
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
fn renaming_hcl_blocks_and_quoted_keys_keeps_the_rest() {
    assert_eq!(
        accept(Language::HCL, "pol| {\n  max_idle = 1\n}\n", "pool"),
        "pool| {\n  max_idle = 1\n}\n"
    );
    assert_eq!(
        accept(
            Language::HCL,
            "servi| \"ssh\" {\n  target = 1\n}\n",
            "services"
        ),
        "services| \"ssh\" {\n  target = 1\n}\n"
    );
    assert_eq!(
        accept(Language::JSON, "{\"pub|lic\": 1}", "public_address"),
        "{\"public_address\"|: 1}"
    );
}

#[test]
fn a_toml_key_typed_after_a_quote_takes_the_closing_quote() {
    let toml = accept(Language::TOML, "listen = 1\n\"pub|\"", "public_address");
    assert!(!toml.contains("\"\"\""), "{toml}");
    assert!(toml.starts_with("listen = 1\npublic_address = "), "{toml}");
}

#[test]
fn toml_headers_offer_tables_or_arrays_of_tables_by_bracket_kind() {
    let tables = labels(Language::TOML, "[web.|]\n");
    assert!(
        tables.contains(&"letsencrypt".to_string())
            && !tables.contains(&"certificates".to_string()),
        "{tables:?}"
    );
    let arrays = labels(Language::TOML, "[[web.|]]\n");
    assert_eq!(arrays, ["certificates"]);
}
#[test]
fn yaml_indentless_sequences_complete_their_items() {
    assert_eq!(
        labels(Language::YAML, "web:\n  certificates:\n  - |"),
        ["hosts", "cert", "key", "upload"]
    );
    let inside = labels(
        Language::YAML,
        "web:\n  certificates:\n  - hosts: [a]\n    |",
    );
    assert!(
        inside.contains(&"cert".to_string()) && !inside.contains(&"hosts".to_string()),
        "{inside:?}"
    );
    // A key at the list's indentation ends it.
    assert!(
        labels(Language::YAML, "web:\n  certificates:\n  - hosts: [a]\n  |")
            .contains(&"hsts".to_string())
    );
}

#[test]
fn xml_text_is_typed_by_the_schema_and_kept_as_written() {
    let schema =
        crate::Schema::compile(include_str!("../../../tests/fixtures/burrow.schema.json")).unwrap();
    for text in ["123", "true", "1.5"] {
        let document = format!("<public_address>{text}</public_address>");
        assert_eq!(schema.validate(Language::XML, &document), [], "{text}");
    }
    assert_eq!(
        Language::XML.parse("<a>1e3</a>").unwrap(),
        serde_json::json!({"a": "1e3"})
    );
    assert_eq!(
        Language::XML.parse("<a>15</a>").unwrap(),
        serde_json::json!({"a": 15})
    );
}
#[test]
fn hcl_values_written_in_place_escape_templates() {
    let bridge_host = "listen = 7835\npublic_address = \"$${HOST}\"\n";
    let value = Language::HCL.parse(bridge_host).unwrap();
    assert_eq!(value["public_address"], "${HOST}");
    assert_eq!(
        Language::HCL
            .behavior()
            .format_value(&serde_json::json!("${HOST}.example.com")),
        "\"$${HOST}.example.com\""
    );
}

#[test]
fn an_error_in_a_single_hcl_block_points_inside_it() {
    let schema =
        crate::Schema::compile(include_str!("../../../tests/fixtures/burrow.schema.json")).unwrap();
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
fn xml_refuses_empty_element_names() {
    assert!(
        Language::XML
            .parse("<services>\n  <>\n  </>\n</services>")
            .is_err()
    );
    assert!(
        Language::YAML
            .convert("services: {\"\": {target: 80}}\n", Language::XML)
            .is_err()
    );
}

#[test]
fn hover_works_inside_an_object_missing_a_required_key() {
    // `letsencrypt` requires `email`; its other keys still show their docs.
    let json = "{\"web\": {\"letsencrypt\": {\"dns\": []}}}";
    let column = json.find("dns").unwrap() + 1;
    assert_eq!(
        hover_key_at(Language::JSON, json, 0, column).as_deref(),
        Some("dns")
    );
    let yaml = "web:\n  letsencrypt:\n    dns: []\n";
    assert_eq!(
        hover_key_at(Language::YAML, yaml, 2, 5).as_deref(),
        Some("dns")
    );
    let hcl = "web {\n  letsencrypt {\n    dns = []\n  }\n}\n";
    assert_eq!(
        hover_key_at(Language::HCL, hcl, 2, 5).as_deref(),
        Some("dns")
    );
    let toml = "[web.letsencrypt]\ndns = []\n";
    assert_eq!(
        hover_key_at(Language::TOML, toml, 1, 1).as_deref(),
        Some("dns")
    );
    let xml = "<web>\n  <letsencrypt>\n    <dns></dns>\n  </letsencrypt>\n</web>\n";
    assert_eq!(
        hover_key_at(Language::XML, xml, 2, 6).as_deref(),
        Some("dns")
    );
}

#[test]
fn toml_header_completion_with_a_prefix_leaves_out_existing_tables() {
    let offered = |marked: &str| labels(Language::TOML, marked);
    assert!(!offered("[web]\nhttp = 1\n\n[w|]").contains(&"web".to_string()));
    assert!(offered("[web]\nhttp = 1\n\n[w|]").contains(&"websocket".to_string()));
    assert!(!offered("[websocket]\nlisten = 1\n\n[webs|]").contains(&"websocket".to_string()));
    assert!(
        !offered("[web.letsencrypt]\nemail = \"a\"\n\n[web.l|]")
            .contains(&"letsencrypt".to_string())
    );
}

#[test]
fn a_wrong_type_value_written_as_a_block_underlines_its_key() {
    let schema = burrow();
    for (source, key) in [
        ("tcp:\n  nodelay:\n    a: 1\n", "nodelay"),
        ("listen:\n  port: 1\n", "listen"),
    ] {
        let problems = super::super::diagnostics::validate(
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
fn accepting_a_value_inside_existing_quotes_keeps_the_rest_of_the_line() {
    // TOML inline table.
    let toml = accept(
        Language::TOML,
        "[services.s]\ntarget = 1\nweb = { hosts = [\"a\"], tls = \"|\", hsts = true }\n",
        "auto",
    );
    assert!(toml.contains("tls = \"auto\"|, hsts = true }"), "{toml}");
    // TOML with a trailing comment.
    let toml = accept(
        Language::TOML,
        "[services.s]\ntarget = 1\nmode = \"|\" # pick one\n",
        "failover",
    );
    assert!(toml.contains("mode = \"failover\"| # pick one"), "{toml}");
    // HCL inline object.
    let hcl = accept(
        Language::HCL,
        "services {\n  s {\n    target = 1\n    web = { hosts = [\"a\"], tls = \"|\", hsts = true }\n  }\n}\n",
        "auto",
    );
    assert!(hcl.contains("tls = \"auto\"|, hsts = true }"), "{hcl}");
    // YAML flow map.
    let yaml = accept(
        Language::YAML,
        "services: {s: {target: 1, mode: \"|\", bandwidth: 1m}}\n",
        "failover",
    );
    assert!(yaml.contains(", bandwidth: 1m}}"), "{yaml}");
    assert_parses(Language::YAML, &yaml);
    // The value after an accepted key, in nested inline tables.
    let toml = accept(
        Language::TOML,
        "services = { s = { target = 1, mode = \"|\" } }\n",
        "failover",
    );
    assert!(toml.contains("mode = \"failover\"| } }"), "{toml}");
}

#[test]
fn an_xml_attribute_value_accepted_inside_its_quotes_leaves_the_caret_after_them() {
    let xml = accept(
        Language::XML,
        "<burrow config-root=\"true\">\n  <services>\n    <s target=\"1\" mode=\"|\"/>\n  </services>\n</burrow>\n",
        "balance",
    );
    assert!(xml.contains("mode=\"balance\"|/>"), "{xml}");
}

#[test]
fn hcl_leaves_out_an_existing_key_even_when_its_whole_name_is_typed() {
    let nested = labels(Language::HCL, "web {\n  http2 = true\n  http2|\n}\n");
    assert!(!nested.contains(&"http2".to_string()), "{nested:?}");
    let root = labels(Language::HCL, "listen = 1\nlisten|\n");
    assert!(!root.contains(&"listen".to_string()), "{root:?}");
    let block = labels(Language::HCL, "tcp {\n}\ntcp|\n");
    assert!(!block.contains(&"tcp".to_string()), "{block:?}");
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
fn xml_leaves_out_children_that_come_after_the_caret() {
    let offered = labels(
        Language::XML,
        "<websocket><listen>1</listen>\n  <t|\n  <tls><cert>a</cert><key>b</key></tls></websocket>\n",
    );
    assert!(!offered.contains(&"tls".to_string()), "{offered:?}");
}

#[test]
fn accepting_the_key_already_typed_in_front_of_its_value_changes_nothing() {
    assert_eq!(
        accept(Language::TOML, "[tcp]\nnodelay| = true\n", "nodelay"),
        "[tcp]\nnodelay| = true\n"
    );
    assert_eq!(
        accept(Language::YAML, "tcp:\n  nodelay|: true\n", "nodelay"),
        "tcp:\n  nodelay|: true\n"
    );
    assert_eq!(accept(Language::HCL, "tcp| {\n}\n", "tcp"), "tcp| {\n}\n");
}

#[test]
fn a_key_typed_in_front_of_an_inline_entry_goes_before_it() {
    assert_eq!(
        accept(Language::XML, "<tcp k|nodelay=\"true\"/>", "keepalive_secs"),
        "<tcp keepalive_secs=\"30|\" nodelay=\"true\"/>"
    );
    assert_eq!(
        accept(Language::YAML, "tcp: {k|nodelay: true}\n", "keepalive_secs"),
        "tcp: {keepalive_secs: 30|, nodelay: true}\n"
    );
    assert_eq!(
        accept(
            Language::TOML,
            "tcp = {k|nodelay = true}\n",
            "keepalive_secs"
        ),
        "tcp = {keepalive_secs = 30|, nodelay = true}\n"
    );
    // A suggestion completing the key renames it.
    assert_eq!(
        accept(Language::YAML, "tcp: {node|lay: true}\n", "nodelay"),
        "tcp: {nodelay|: true}\n"
    );
}

#[test]
fn an_hcl_block_label_gets_no_property_suggestions() {
    let offered = labels(Language::HCL, "services \"|\" {\n}\n");
    assert!(offered.is_empty(), "{offered:?}");
    let offered = labels(Language::HCL, "clients \"a\" \"|\" {\n}\n");
    assert!(offered.is_empty(), "{offered:?}");
    // A string value is still a value.
    assert!(!labels(Language::HCL, "public_address = \"|\"\n").contains(&"listen".to_string()));
}

#[test]
fn a_deep_toml_dotted_unknown_key_is_marked_on_its_segment() {
    use super::super::diagnostics::validate;
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

#[test]
fn an_hcl_map_accepted_in_a_body_is_a_block_as_documents_write_it() {
    let hcl = accept(Language::HCL, "cli|\n", "clients");
    assert!(hcl.starts_with("clients {\n"), "{hcl}");
    // In an object expression it stays an attribute.
    let inline = accept(Language::HCL, "web = { le| }\n", "letsencrypt");
    assert!(inline.contains("letsencrypt = {"), "{inline}");
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
        // it: at that line's indent (round 20: at column 0).
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
        // In a flow collection, one level deeper (round 21).
        ("key: [a, |b]\n", "key: [a, \n  |b]\n"),
    ] {
        let actual = enter(Language::YAML, marked);
        if actual != expected {
            failures.push(format!("{marked:?}: {actual:?}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// QA 23: a leading byte order mark was a syntax error at 1:1 in JSON and
/// HCL (YAML, TOML and XML read past it). Every language reads the
/// document after it, and an error on its line is placed where it is.
#[test]
fn a_leading_byte_order_mark_is_no_part_of_the_document() {
    let mut failures = Vec::new();
    for (language, text, broken) in [
        (Language::JSON, "{\"a\": 1}\n", "{\"a\": }\n"),
        (Language::HCL, "a = 1\n", "a = \n"),
        (Language::YAML, "a: 1\n", "a: [\n"),
        (Language::TOML, "a = 1\n", "a = \n"),
        (Language::XML, "<a>1</a>\n", "<a>1</b>\n"),
    ] {
        let marked = format!("\u{FEFF}{text}");
        if language.parse(&marked).ok() != language.parse(text).ok()
            || language.parse(text).is_err()
        {
            failures.push(format!("{language}: {:?}", language.parse(&marked)));
        }
        let plain = language
            .parse(broken)
            .err()
            .map(|error| (error.line, error.column));
        let with_mark = language
            .parse(&format!("\u{FEFF}{broken}"))
            .err()
            .map(|error| {
                let on_its_line = usize::from(error.line == Some(1));
                (error.line, error.column.map(|column| column - on_its_line))
            });
        if language != Language::XML && (plain.is_none() || plain != with_mark) {
            failures.push(format!("{language} error: {plain:?} vs {with_mark:?}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
