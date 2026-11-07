//! Editing commands checked in every language on the same kinds of text, so
//! a fix for one language cannot leave its siblings behind. Each test lists
//! every failing case at once.

use super::Language;

const ALL: [Language; 5] = [
    Language::YAML,
    Language::JSON,
    Language::TOML,
    Language::HCL,
    Language::XML,
];

fn large(language: Language) -> &'static str {
    match language.label() {
        "YAML" => include_str!("../../../tests/fixtures/samples/burrow.large.yaml"),
        "JSON" => include_str!("../../../tests/fixtures/samples/burrow.large.json"),
        "TOML" => include_str!("../../../tests/fixtures/samples/burrow.large.toml"),
        "HCL" => include_str!("../../../tests/fixtures/samples/burrow.large.hcl"),
        _ => include_str!("../../../tests/fixtures/samples/burrow.large.xml"),
    }
}

/// Small documents holding a comment line between code lines.
fn with_comment(language: Language) -> &'static str {
    match language.label() {
        "YAML" => "listen: 1\n# note\npool:\n  min_idle: 2\n",
        "JSON" => "{\n  \"listen\": 1\n}\n",
        "TOML" => "listen = 1\n# note\n[pool]\nmin_idle = 2\n",
        "HCL" => "listen = 1\n# note\npool {\n  min_idle = 2\n}\n",
        _ => {
            "<listen>1</listen>\n<!-- note -->\n<pool>\n  <min_idle>2</min_idle>\n</pool>\n<host>a -- b</host>\n"
        }
    }
}

/// Ctrl+/ over a selection twice gives the text back, and the commented
/// text is still a valid document (XML comments cannot nest or hold `--`).
#[test]
fn toggling_comments_twice_restores_the_selection() {
    let mut failures = Vec::new();
    for language in ALL {
        for (name, text) in [
            ("small", with_comment(language)),
            ("large", large(language)),
        ] {
            let behavior = language.behavior();
            let all = text.encode_utf16().count();
            let Some((once, start, end)) = behavior.toggle_comments(text, 0, all) else {
                continue;
            };
            if language.label() == "XML" && language.parse(&once).is_err() {
                failures.push(format!(
                    "XML {name}: commented text does not parse:\n{once}"
                ));
            }
            match behavior.toggle_comments(&once, start, end) {
                Some((twice, _, _)) if twice == text => {}
                Some((twice, _, _)) => {
                    failures.push(format!(
                        "{} {name}: toggled twice gives\n{twice}\ninstead of\n{text}",
                        language.label()
                    ));
                }
                None => failures.push(format!(
                    "{} {name}: the second toggle does nothing",
                    language.label()
                )),
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} failing case(s):\n{}",
        failures.len(),
        failures.join("\n---\n")
    );
}

/// Ctrl+/ on lines each wrapped in the language's block comment unwraps
/// them instead of adding a line comment.
#[test]
fn toggling_comments_on_block_commented_lines_unwraps_them() {
    let text = "/* a = 1 */\n  /* b = 2 */\n";
    let (unwrapped, _, _) = Language::HCL
        .behavior()
        .toggle_comments(text, 0, text.encode_utf16().count())
        .unwrap();
    assert_eq!(unwrapped, "a = 1\n  b = 2\n");
    // A mixed selection is line-commented as before.
    let mixed = "/* a = 1 */\nb = 2\n";
    let (commented, _, _) = Language::HCL
        .behavior()
        .toggle_comments(mixed, 0, mixed.encode_utf16().count())
        .unwrap();
    assert_eq!(commented, "# /* a = 1 */\n# b = 2\n");
}

/// The caret keeps its place in the text through Ctrl+/ and back, also on an
/// XML line whose `--` the comment escapes.
#[test]
fn the_caret_keeps_its_place_through_toggling_comments() {
    let mut failures = Vec::new();
    for (language, text, at) in [
        (Language::YAML, "listen: 1\n", "1"),
        (Language::TOML, "listen = 1\n", "1"),
        (Language::HCL, "listen = 1\n", "1"),
        (Language::XML, "<listen>a--b</listen>\n", "</listen>"),
    ] {
        let behavior = language.behavior();
        let caret = text[..text.find(at).unwrap()].encode_utf16().count();
        let Some((once, start, _)) = behavior.toggle_comments(text, caret, caret) else {
            continue;
        };
        let expected_once = once
            .find(at)
            .map(|byte| once[..byte].encode_utf16().count());
        if Some(start) != expected_once {
            failures.push(format!(
                "{}: caret at {start} in {once:?}, expected before {at:?}",
                language.label()
            ));
        }
        let Some((twice, back, _)) = behavior.toggle_comments(&once, start, start) else {
            failures.push(format!("{}: second toggle does nothing", language.label()));
            continue;
        };
        if twice != text || back != caret {
            failures.push(format!(
                "{}: back to {back} in {twice:?}, expected {caret}",
                language.label()
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Ctrl+/ twice over a selection that starts or ends mid-line gives the text
/// back (XML comments each line separately, so they must unwrap the same
/// way), and a block comment unwrapped keeps the caret where it was.
#[test]
fn toggling_comments_twice_from_mid_line_restores_the_text() {
    let mut failures = Vec::new();
    for (language, text, from, to) in [
        (
            Language::XML,
            "<web>\n  <path>a</path>\n  <host>b</host>\n</web>\n",
            "a</path>",
            "</host>",
        ),
        (
            Language::XML,
            "<web>\n  <path>a--b</path>\n  <host>c</host>\n</web>\n",
            "--b",
            "c</host>",
        ),
        (Language::YAML, "web:\n  path: a\n  host: b\n", "a\n", "b\n"),
        (Language::TOML, "path = 1\nhost = 2\n", "1", "2"),
        (Language::HCL, "path = 1\nhost = 2\n", "1", "2"),
    ] {
        let behavior = language.behavior();
        let utf16 = |byte: usize| text[..byte].encode_utf16().count();
        let (start, end) = (
            utf16(text.find(from).unwrap()),
            utf16(text.find(to).unwrap()),
        );
        let Some((once, a, b)) = behavior.toggle_comments(text, start, end) else {
            continue;
        };
        match behavior.toggle_comments(&once, a, b) {
            Some((twice, _, _)) if twice == text => {}
            Some((twice, _, _)) => failures.push(format!("{}: {twice:?}", language.label())),
            None => failures.push(format!("{}: second toggle does nothing", language.label())),
        }
    }
    // HCL block comment: the caret stays on the same text.
    let text = "tcp {\n  /* nodelay = true */\n}\n";
    let caret = text.find(" = true").unwrap();
    if let Some((unwrapped, start, end)) =
        Language::HCL.behavior().toggle_comments(text, caret, caret)
    {
        let expected = unwrapped.find(" = true").unwrap();
        if (start, end) != (expected, expected) {
            failures.push(format!("HCL block: selection {start}..{end} in {unwrapped:?}, expected the caret at {expected}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Round 10: Ctrl+/ never writes a comment marker into a multi-line
/// string's text: a selection touching one covers the whole entry, so the
/// document stays the same document, commented out or back.
#[test]
fn toggling_comments_never_writes_into_a_multi_line_string() {
    let mut failures = Vec::new();
    for (language, text, at) in [
        (
            Language::YAML,
            "listen: 1\nservices:\n  web:\n    target: 1\n    note: |\n      line1\n      line2\n    bind: x\n",
            "line1",
        ),
        (Language::YAML, "a: \"x\n  b\n  c\"\nd: 1\n", "  b"),
        (Language::TOML, "a = \"\"\"\nline\n\"\"\"\nb = 1\n", "line"),
        (Language::HCL, "a = <<EOT\nline\nEOT\nb = 1\n", "line"),
    ] {
        let behavior = language.behavior();
        let caret = text[..text.find(at).unwrap()].encode_utf16().count();
        let Some((once, start, end)) = behavior.toggle_comments(text, caret, caret) else {
            continue;
        };
        // Commented out: whatever stays is still a document, and no line of
        // the string's text has a marker inside it.
        match language.parse(&once) {
            Err(_) => failures.push(format!(
                "{}: commented text does not parse:\n{once}",
                language.label()
            )),
            // A string the comment went into holds the marker now.
            Ok(value) if strings_of(&value).iter().any(|text| text.contains('#')) => {
                failures.push(format!(
                    "{}: a marker went into a string:\n{once}",
                    language.label()
                ));
            }
            Ok(_) => {}
        }
        match behavior.toggle_comments(&once, start, end) {
            Some((twice, _, _)) if twice == text => {}
            Some((twice, _, _)) => {
                failures.push(format!("{}: twice gives\n{twice}", language.label()))
            }
            None => failures.push(format!(
                "{}: the second toggle does nothing",
                language.label()
            )),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n---\n"));
}

fn strings_of(value: &serde_json::Value) -> Vec<String> {
    match value {
        serde_json::Value::String(text) => vec![text.clone()],
        serde_json::Value::Array(items) => items.iter().flat_map(strings_of).collect(),
        serde_json::Value::Object(map) => map.values().flat_map(strings_of).collect(),
        _ => Vec::new(),
    }
}
#[test]
fn probe_bodies() {
    for (language, text) in [
        (
            Language::YAML,
            "listen: 1\nservices:\n  web:\n    target: 1\n    note: |\n      line1\n      line2\n    bind: x\n",
        ),
        (Language::YAML, "a: \"x\n  b\n  c\"\nd: 1\n"),
        (Language::TOML, "a = \"\"\"\nline\n\"\"\"\nb = 1\n"),
        (Language::HCL, "a = <<EOT\nline\nEOT\nb = 1\n"),
    ] {
        println!(
            "{} {:?}",
            language.label(),
            language.behavior().string_body_lines(text)
        );
    }
}

/// Round 10: Ctrl+/ inside an HCL block comment over several lines removes
/// that comment, wherever the caret is in it.
#[test]
fn toggling_comments_inside_a_multi_line_block_comment_unwraps_it() {
    let text = "tcp {\n  /* nodelay = true\n  keepalive_secs = 30 */\n}\n";
    let caret = text.find(" = 30").unwrap();
    let (unwrapped, start, _) = Language::HCL
        .behavior()
        .toggle_comments(text, caret, caret)
        .unwrap();
    assert_eq!(
        unwrapped,
        "tcp {\n  nodelay = true\n  keepalive_secs = 30\n}\n"
    );
    assert_eq!(start, unwrapped.find(" = 30").unwrap());
}

/// Round 11: Ctrl+/ on one line of something written over several lines
/// comments all of it, so the document still reads and other values keep
/// theirs; a second Ctrl+/ gives the text back. A block comment sharing its
/// lines with code is never unwrapped into code.
#[test]
fn ctrl_slash_never_cuts_through_something_written_over_lines() {
    let mut failures = Vec::new();
    for (language, marked) in [
        (Language::TOML, "a = 1\ns = \"\"\"mu|lti\nline\"\"\"\nb = 2\n"),
        (Language::TOML, "a = 1\ns = \"\"\"multi\nli|ne\"\"\"\nb = 2\n"),
        (Language::TOML, "a = [1,|\n  2]\nb = 2\n"),
        (Language::YAML, "a: [1,|\n  2]\nb: 2\n"),
        (Language::YAML, "a: [1,\n  2|]\nb: 2\n"),
        (Language::YAML, "a: {x: 1,|\n  y: 2}\nb: 2\n"),
        (Language::HCL, "a = [1,|\n  2]\nb = 2\n"),
        (Language::HCL, "a = max(1,|\n  2)\nb = 2\n"),
        (Language::XML, "<a>line1\nli|ne2</a>\n<b>2</b>\n"),
        (Language::XML, "<a\n  x=\"1\"|\n  y=\"2\">t</a>\n<b>2</b>\n"),
        (Language::XML, "<a x=\"1\n|2\">t</a>\n<b>2</b>\n"),
        (Language::XML, "<a><![CDATA[x\n  y|]]></a>\n<b>2</b>\n"),
    ] {
        let at = marked.find('|').unwrap();
        let text = marked.replacen('|', "", 1);
        let caret = text[..at].encode_utf16().count();
        let behavior = language.behavior();
        let Some((once, start, end)) = behavior.toggle_comments(&text, caret, caret) else {
            failures.push(format!("{}: nothing toggled in {text:?}", language.label()));
            continue;
        };
        let b = |source: &str| language.parse(source).ok().and_then(|value| value.get("b").cloned());
        if language.parse(&once).is_err() || b(&once) != b(&text) {
            failures.push(format!("{}: {text:?} commented as {once:?}", language.label()));
        }
        match behavior.toggle_comments(&once, start, end) {
            Some((twice, _, _)) if twice == text => {}
            other => failures.push(format!("{}: {text:?} toggled back as {:?}", language.label(), other.map(|o| o.0))),
        }
    }
    for (language, marked, comment) in [
        (Language::HCL, "a = 1 /* multi\nli|ne */\nb = 2\n", "/*"),
        (Language::XML, "<a>1</a> <!-- multi\nli|ne --> <b>2</b>\n", "<!--"),
    ] {
        let at = marked.find('|').unwrap();
        let text = marked.replacen('|', "", 1);
        let caret = text[..at].encode_utf16().count();
        let once = language.behavior().toggle_comments(&text, caret, caret).map(|o| o.0).unwrap_or_default();
        if !once.contains(comment) || language.parse(&once).is_err() {
            failures.push(format!("{}: {text:?} became {once:?}", language.label()));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Round 12: Ctrl+/ comments exactly the construct the caret is in: a quoted
/// string over lines (no further), a heredoc from its closing line, a YAML
/// parent with its children, lines inside a block comment shared with code.
#[test]
fn ctrl_slash_takes_exactly_the_construct_under_the_caret() {
    let mut failures = Vec::new();
    for (language, marked, expected) in [
        (Language::YAML, "motd: \"abc\n  de|f\"\nlisten: 1\npath: 2\n", "# motd: \"abc\n#   def\"\nlisten: 1\npath: 2\n"),
        (Language::YAML, "motd: \"abc\n  def\"\nli|sten: 1\npath: 2\n", "motd: \"abc\n  def\"\n# listen: 1\npath: 2\n"),
        (Language::YAML, "- 'a\n  b|'\n- c\n", "# - 'a\n#   b'\n- c\n"),
        (Language::YAML, "webs|ocket:\n  path: /x\nlisten: 1\n", "# websocket:\n#   path: /x\nlisten: 1\n"),
        (Language::HCL, "a = <<EOF\nx\nEOF|\nb = 1\n", "# a = <<EOF\n# x\n# EOF\nb = 1\n"),
        (Language::HCL, "x {\n  a = <<-EOF\n    x\n    EOF|\n}\nb = 1\n", "x {\n  # a = <<-EOF\n  #   x\n  #   EOF\n}\nb = 1\n"),
        (Language::HCL, "listen = 1 /* c\n  m|ore */ path = 2\nb = 1\n", "# listen = 1 /* c\n#   more */ path = 2\nb = 1\n"),
    ] {
        let at = marked.find('|').unwrap();
        let text = marked.replacen('|', "", 1);
        let caret = text[..at].encode_utf16().count();
        match language.behavior().toggle_comments(&text, caret, caret) {
            Some((once, _, _)) if once == expected => {}
            other => failures.push(format!("{}: {text:?} became {:?}", language.label(), other.map(|o| o.0))),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Round 12: Enter inside a TOML inline table leaves it whole on its line
/// (TOML 1.0 keeps inline tables on one line) and starts the next line.
#[test]
fn enter_inside_a_toml_inline_table_keeps_it_on_one_line() {
    let source = "ws = { path = \"/x\", host = \"h\" }\n";
    let caret = source.find(" host").unwrap();
    let newline = Language::TOML.behavior().newline(source, caret);
    let rest = &source[caret..source.find('\n').unwrap()];
    assert_eq!(newline.replaces, rest.len());
    assert_eq!(newline.text, format!("{rest}\n"));
}

/// Round 13: Ctrl+/ on a parent takes what belongs to it (YAML parents with
/// anchors, tags, list items with maps, comments among the children; TOML
/// tables), and Enter anywhere in a one-line HCL block opens it validly.
#[test]
fn ctrl_slash_takes_parents_and_enter_opens_one_line_blocks() {
    let mut failures = Vec::new();
    for (language, marked, expected) in [
        (Language::YAML, "- a: 1|\n  b: 2\n- c: 3\n", "# - a: 1\n#   b: 2\n- c: 3\n"),
        (Language::YAML, "a: &x|\n  b: 1\nc: 2\n", "# a: &x\n#   b: 1\nc: 2\n"),
        (Language::YAML, "a: !!map|\n  b: 1\nc: 2\n", "# a: !!map\n#   b: 1\nc: 2\n"),
        (Language::YAML, "ws:|\n  path: 1\n# comment\n  host: 2\nl: 1\n", "# ws:\n#   path: 1\n# # comment\n#   host: 2\nl: 1\n"),
        (Language::TOML, "[webs|ocket]\npath = \"/x\"\nhost = \"h\"\n\n[x]\na = 1\n", "# [websocket]\n# path = \"/x\"\n# host = \"h\"\n\n[x]\na = 1\n"),
    ] {
        let at = marked.find('|').unwrap();
        let text = marked.replacen('|', "", 1);
        let caret = text[..at].encode_utf16().count();
        match language.behavior().toggle_comments(&text, caret, caret) {
            Some((once, _, _)) if once == expected && language.parse(&once).is_ok() => {}
            other => failures.push(format!("{}: {text:?} became {:?}", language.label(), other.map(|o| o.0))),
        }
    }
    let source = "websocket { path = \"/x\" }\n";
    let caret = source.find('{').unwrap() + 1;
    let newline = Language::HCL.behavior().newline(source, caret);
    let mut after = source.to_string();
    after.replace_range(caret - newline.replaces_before..caret + newline.replaces, &newline.text);
    if Language::HCL.parse(&after).is_err() {
        failures.push(format!("HCL Enter after brace: {after:?}"));
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
