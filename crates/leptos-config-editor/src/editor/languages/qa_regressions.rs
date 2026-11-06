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
        "limits = {\n  |\n}\n"
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
    let mut text = source[..at].to_string();
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
