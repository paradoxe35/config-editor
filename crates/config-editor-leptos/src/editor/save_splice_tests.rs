//! A changed string is spliced into the host in place, whatever it holds:
//! every string of a commented host in each language set to text with tabs,
//! line breaks, leading spaces, quotes, markup and Unicode line separators,
//! from every other view. A save that regenerates the whole document loses
//! the host's comments; here every comment must survive and no other line
//! may change.

use super::Language;
use super::bridge::WHOLE_RENDERS;
use super::diff::{DiffEdit, myers_edits};
use super::save_matrix::{ALL, find_slot, save_conversion, slot};
use serde_json::Value;

/// Hosts with a string in every style the language writes, and comments
/// marked `noteN` (JSON has none: its lines are checked).
fn hosts() -> Vec<(Language, &'static str)> {
    vec![
        (
            Language::YAML,
            "# note1\nplain: x # note2\nquoted: \"q\"\nsingle: 'q'\nlit: |\n  a\n  b\nfold: >-\n  a\n  b\nlist:\n  - x # note3\n  - |\n    y\nflow: [a, b] # note4\nmap:\n  # note5\n  k: v\nn: 1 # note6\n",
        ),
        (
            Language::JSON,
            "{\n    \"plain\": \"x\",\n    \"flow\": {\"a\": \"b\"},\n    \"list\": [\"x\", \"y\"],\n    \"n\": 1\n}\n",
        ),
        (
            Language::TOML,
            "# note1\nplain = \"x\" # note2\nlit = 'x'\nml = \"\"\"\na\nb\"\"\"\nlist = [\"x\", \"y\"] # note3\ninline = { k = \"v\" } # note4\nn = 1 # note5\n\n[table] # note6\nk = \"v\"\n",
        ),
        (
            Language::HCL,
            "# note1\nplain = \"x\" // note2\nheredoc = <<EOT\na\nb\nEOT\nlist = [\"x\", \"y\"] # note3\nobj = { k = \"v\" } # note4\nblock {\n  # note5\n  k = \"v\"\n}\nn = 1 /* note6 */\n",
        ),
        (
            Language::XML,
            "<!-- note1 -->\n<root>\n  <plain>x</plain> <!-- note2 -->\n  <cd><![CDATA[a < b]]></cd>\n  <at k=\"v\"/>\n  <list>x</list>\n  <list>y</list>\n  <!-- note3 -->\n  <n>1</n>\n</root>\n",
        ),
        // List items holding maps with blocks and comment lines between
        // their keys, compact nested sequences (`- - a`), a list item map
        // after a block.
        (
            Language::YAML,
            "# note1\nitems:\n  - k: |\n      a\n      b\n    # note2\n    j: x # note3\n  - - c # note4\n    - d\n  - - - e\n    - f\n  - name: n\n    tags: [t, u] # note5\n    deep:\n      - k: >-\n          folded\n        # note6\n        j: y\npct: 'p'\nn: 1 # note7\n",
        ),
        // Line-ending backslash strings, literal multi-line strings, a
        // multi-line string under a table.
        (
            Language::TOML,
            r#"# note1
bs = """\
  one \
  two"""
lit = '''
l1
l2
'''
s = "x" # note2

[t] # note3
ml = """
a
b
"""
k = 'v' # note4
"#,
        ),
        // An indented heredoc, a labelled block.
        (
            Language::HCL,
            "# note1\nh = <<-EOT\n  a\n  b\n  EOT\nblk \"lab\" {\n  # note2\n  s = \"x\" # note3\n  h = <<EOT\nc\nEOT\n}\n",
        ),
        // Comments, processing instructions and CDATA inside text, in
        // elements with and without attributes, and attribute values.
        (
            Language::XML,
            "<!-- note1 -->\n<root a=\"x\">\n  <p k=\"v\">alpha<!-- note2 -->beta</p>\n  <q k=\"v\">t<![CDATA[x < y]]>u</q>\n  <r>al<?pi note3?>be</r>\n  <c>one<![CDATA[two]]>three</c>\n  <s k=\"a &amp; b\"/>\n  <!-- note4 -->\n</root>\n",
        ),
    ]
}

/// Strings that need escapes, quotes, a block's indentation indicator, or
/// that look like other syntax.
const VALUES: &[&str] = &[
    "x\n\tq",
    "\ttab",
    " lead",
    "trail ",
    "a\nb",
    "a\n\nb\n",
    "  x\ny\n",
    "x\n  indented",
    "#hash",
    "a: b",
    "- item",
    "\"q\" 'a'",
    "a\u{2028}b",
    "a\u{85}b",
    "a\u{2029}b\nc",
    "café",
    "back\\slash",
    "${x}",
    "<tag>&amp;",
    "]]>",
    "*alias",
    "&anchor",
    "a # not a comment",
    "1.5",
    "true",
    // Every YAML indicator first, and forms YAML 1.1 readers (PyYAML,
    // yaml.v2) take for another type.
    "%x",
    "@x",
    "`x",
    "|x",
    ">x",
    "!x",
    "?x",
    "? x",
    "-x",
    ":x",
    ": x",
    "{x",
    "}x",
    "[x",
    "]x",
    ",x",
    "'x",
    "\"x",
    "x:",
    "yes",
    "Off",
    "y",
    "N",
    "1_000",
    "1:20",
    "2001-12-14",
    "2001-12-14 21:59:43.10 -5",
    "<<",
    "=",
    "~",
    "x\ty",
    "0o17",
    ".inf",
    "-.5",
    // Carriage returns, alone and before a line break.
    "a\r\nb\r\n",
    "x\ry",
    "l1\r\nl2\n",
    "a\n\rb",
    "\r",
];

/// The JSON pointers of every string in `value`.
fn strings(value: &Value, pointer: String, out: &mut Vec<String>) {
    match value {
        Value::String(_) => out.push(pointer),
        Value::Object(map) => {
            for (key, child) in map {
                strings(child, format!("{pointer}/{key}"), out);
            }
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                strings(child, format!("{pointer}/{index}"), out);
            }
        }
        _ => {}
    }
}

/// Host lines the save dropped or rewrote.
fn changed_lines(host: &str, saved: &str) -> usize {
    let before: Vec<&str> = host.lines().collect();
    let after: Vec<&str> = saved.lines().collect();
    myers_edits(&before, &after).map_or(usize::MAX, |edits| {
        edits
            .iter()
            .filter(|edit| matches!(edit, DiffEdit::Delete))
            .count()
    })
}

/// Whether YAML 1.1 readers (PyYAML, yaml.v2) read the plain scalar
/// `written` as the string it spells: their scanner's indicators and their
/// implicit resolvers' patterns (PyYAML's `resolver.py`), written out here
/// apart from the crate's own rule.
fn yaml_1_1_reads_plain_as_text(written: &str) -> bool {
    let resolvers = [
        r"^(?:yes|Yes|YES|no|No|NO|true|True|TRUE|false|False|FALSE|on|On|ON|off|Off|OFF|y|Y|n|N)$",
        r"^(?:[-+]?(?:[0-9][0-9_]*)\.[0-9_]*(?:[eE][-+][0-9]+)?|\.[0-9][0-9_]*(?:[eE][-+][0-9]+)?|[-+]?[0-9][0-9_]*(?::[0-5]?[0-9])+\.[0-9_]*|[-+]?\.(?:inf|Inf|INF)|\.(?:nan|NaN|NAN))$",
        r"^(?:[-+]?0b[0-1_]+|[-+]?0[0-7_]+|[-+]?(?:0|[1-9][0-9_]*)|[-+]?0x[0-9a-fA-F_]+|[-+]?[1-9][0-9_]*(?::[0-5]?[0-9])+)$",
        r"^(?:<<|=|~|null|Null|NULL|)$",
        r"^(?:[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]|[0-9][0-9][0-9][0-9]-[0-9][0-9]?-[0-9][0-9]?(?:[Tt]|[ \t]+)[0-9][0-9]?:[0-9][0-9]:[0-9][0-9](?:\.[0-9]*)?(?:[ \t]*(?:Z|[-+][0-9][0-9]?(?::[0-9][0-9])?))?)$",
    ];
    let first = written.chars().next().unwrap_or(' ');
    let second = written.chars().nth(1).unwrap_or(' ');
    let indicator = "-?:,[]{}#&*!|>'\"%@`".contains(first)
        && !("-?:".contains(first) && !matches!(second, ' ' | '\t'));
    !indicator
        && !written.contains(['\t', '\r', '\n'])
        && !written.contains(": ")
        && !written.contains(" #")
        && !written.ends_with([':', ' '])
        && !written.starts_with(' ')
        && resolvers.iter().all(|pattern| {
            !regex_lite::Regex::new(pattern)
                .expect("a pattern")
                .is_match(written)
        })
}

/// The value at `pointer` of the YAML `saved` as written, when it is a
/// plain scalar.
fn yaml_plain_written<'a>(saved: &'a str, pointer: &str) -> Option<&'a str> {
    let span = Language::YAML.behavior().value_span(saved, pointer)?;
    let line = saved.lines().nth(span.line - 1)?;
    let start = line.char_indices().nth(span.start_column - 1)?.0;
    let end = line
        .char_indices()
        .nth(span.end_column - 1)
        .map_or(line.len(), |(at, _)| at);
    let written = line.get(start..end)?;
    (!written.starts_with(['"', '\'', '|', '>', '&', '*', '!', '[', '{'])).then_some(written)
}

#[test]
fn any_string_edited_from_any_view_is_spliced_in_place() {
    let mut failures = Vec::new();
    let (mut tried, mut saved_count, mut lossy_whole) = (0, 0, 0);
    for (host, text) in hosts() {
        let mut pointers = Vec::new();
        strings(
            &host.parse(text).expect("parses"),
            String::new(),
            &mut pointers,
        );
        let notes = text.matches("note").count();
        for pointer in &pointers {
            let old = host.parse(text).expect("parses");
            let old_lines = old
                .pointer(pointer)
                .and_then(Value::as_str)
                .map_or(1, |old| old.trim_end_matches('\n').lines().count());
            for value in VALUES {
                for edit in ALL.into_iter().filter(|edit| *edit != host) {
                    // The XML view shows nested lists flat: their items
                    // are not where the pointer says (the bridge tests
                    // cover them).
                    let nested = pointer
                        .split('/')
                        .collect::<Vec<_>>()
                        .windows(2)
                        .any(|pair| pair.iter().all(|segment| segment.parse::<usize>().is_ok()));
                    if edit == Language::XML && nested {
                        continue;
                    }
                    let shown = std::cell::Cell::new(true);
                    let set = |document: &mut Value| match find_slot(document, pointer) {
                        Some(slot) => *slot = (*value).into(),
                        None => shown.set(false),
                    };
                    tried += 1;
                    let renders = WHOLE_RENDERS.with(std::cell::Cell::get);
                    // A value the view or the host cannot carry is reported,
                    // not saved: nothing to check.
                    let Ok(saved) = save_conversion(host, edit, text, &set) else {
                        continue;
                    };
                    if !shown.get() {
                        tried -= 1;
                        continue;
                    }
                    let case = format!("{host} host, {pointer} = {value:?} via {edit}");
                    let (losses, saved) = (saved.losses, saved.text);
                    saved_count += 1;
                    // A value the host cannot hold exactly (XML text of a
                    // lone carriage return reads as nothing) may be written
                    // whole; any other save is a splice.
                    let whole = WHOLE_RENDERS.with(std::cell::Cell::get) != renders;
                    lossy_whole += usize::from(whole && !losses.is_empty());
                    if whole && losses.is_empty() {
                        failures.push(format!(
                            "{case}: the whole document written\n--- saved\n{saved}"
                        ));
                    }
                    let kept = saved.matches("note").count();
                    // The value's own lines, and a block's header and
                    // terminator, may change.
                    let allowed = old_lines + value.lines().count() + 2;
                    let changed = changed_lines(text, &saved);
                    if (kept < notes || changed > allowed) && !(whole && !losses.is_empty()) {
                        failures.push(format!(
                            "{case}: {kept}/{notes} comments, {changed} lines changed\n--- saved\n{saved}"
                        ));
                    }
                    // The editor holds LF text: a carriage return is written
                    // as an escape (readers drop a raw one before a break).
                    if saved.contains('\r') {
                        failures.push(format!(
                            "{case}: a raw carriage return\n--- saved\n{saved:?}"
                        ));
                    }
                    // Read back the same, when the host carries it exactly.
                    if losses.is_empty() {
                        let read = host
                            .parse(&saved)
                            .map(|mut read| slot(&mut read, pointer).clone());
                        if read.as_ref().ok() != Some(&Value::from(*value)) {
                            failures.push(format!(
                                "{case}: reads back as {read:?}\n--- saved\n{saved}"
                            ));
                        }
                    }
                    if host == Language::YAML {
                        // YAML 1.1 readers break lines at these: written escaped.
                        if saved.contains(['\u{85}', '\u{2028}', '\u{2029}']) {
                            failures
                                .push(format!("{case}: a raw line separator\n--- saved\n{saved}"));
                        }
                        let read_as_text = host
                            .parse(&saved)
                            .ok()
                            .and_then(|read| read.pointer(pointer).cloned())
                            .is_some_and(|read| read.is_string());
                        // The host's own text, when the value did not change.
                        let changed = old.pointer(pointer).and_then(Value::as_str) != Some(*value);
                        if read_as_text
                            && changed
                            && let Some(written) = yaml_plain_written(&saved, pointer)
                            && !yaml_1_1_reads_plain_as_text(written)
                        {
                            failures.push(format!(
                                "{case}: written plain as {written:?}, which YAML 1.1 reads otherwise\n--- saved\n{saved}"
                            ));
                        }
                    }
                }
            }
        }
    }
    // Most of them save (TOML has no `\u{85}` escape, XML no tab-only
    // text, …): the matrix is not vacuous.
    assert!(
        saved_count * 10 > tried * 8,
        "only {saved_count} of {tried} saved"
    );
    assert!(
        lossy_whole * 200 < saved_count,
        "{lossy_whole} lossy saves written whole"
    );
    assert!(
        failures.is_empty(),
        "{} failing:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

/// A document written whole follows the same plain-scalar rule as a
/// spliced value: every string reads back the same, and none is written
/// bare where a YAML 1.1 reader would take it for another type.
#[test]
fn any_string_written_whole_reads_back_in_yaml_1_1_and_1_2() {
    let mut failures = Vec::new();
    for value in VALUES {
        let document = serde_json::json!({ "k": value, "l": [value, "x"], "m": { "n": value } });
        let Ok(text) = super::codec::serialize(Language::YAML.behavior(), &document, None) else {
            failures.push(format!("{value:?}: not written"));
            continue;
        };
        if Language::YAML.parse(&text).ok().as_ref() != Some(&document) {
            failures.push(format!("{value:?}: reads back otherwise\n{text}"));
        }
        for pointer in ["/k", "/l/0", "/m/n"] {
            if let Some(written) = yaml_plain_written(&text, pointer)
                && !yaml_1_1_reads_plain_as_text(written)
            {
                failures.push(format!("{value:?} at {pointer}: written plain\n{text}"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} failing:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

/// A file's CRLF line endings read as LF inside multi-line values, in the
/// headless parse as in the editor (round 17: the parse read `\r\n`).
#[test]
fn crlf_inside_multi_line_values_reads_as_lf() {
    let mut failures = Vec::new();
    for (host, text) in [
        (Language::TOML, "ml = \"\"\"\r\na\r\nb\r\n\"\"\"\r\n"),
        (Language::TOML, "ml = '''\r\na\r\nb\r\n'''\r\n"),
        (Language::YAML, "ml: |\r\n  a\r\n  b\r\n"),
        (Language::HCL, "ml = <<EOT\r\na\r\nb\r\nEOT\r\n"),
        (Language::XML, "<r>\r\n<ml>a\r\nb\r\n</ml>\r\n</r>\r\n"),
    ] {
        let read = host.parse(text).map(|value| {
            value
                .pointer("/ml")
                .or_else(|| value.pointer("/r/ml"))
                .cloned()
        });
        if read.as_ref().ok().cloned().flatten() != Some(Value::from("a\nb\n")) {
            failures.push(format!("{host}: {read:?}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
