//! A changed string is spliced into the host in place, whatever it holds:
//! every string of a commented host in each language set to text with tabs,
//! line breaks, leading spaces, quotes, markup and Unicode line separators,
//! from every other view. A save that regenerates the whole document loses
//! the host's comments; here every comment must survive and no other line
//! may change.

use super::Language;
use super::diff::{DiffEdit, myers_edits};
use super::save_matrix::{ALL, save_value, slot};
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

#[test]
fn any_string_edited_from_any_view_is_spliced_in_place() {
    let mut failures = Vec::new();
    let (mut tried, mut saved_count) = (0, 0);
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
                    let set = |document: &mut Value| *slot(document, pointer) = (*value).into();
                    tried += 1;
                    // A value the view or the host cannot carry is reported,
                    // not saved: nothing to check.
                    let Ok(saved) = save_value(host, edit, text, &set) else {
                        continue;
                    };
                    saved_count += 1;
                    let kept = saved.matches("note").count();
                    // The value's own lines, and a block's header and
                    // terminator, may change.
                    let allowed = old_lines + value.lines().count() + 2;
                    let changed = changed_lines(text, &saved);
                    if kept < notes || changed > allowed {
                        failures.push(format!(
                            "{host} host, {pointer} = {value:?} via {edit}: {kept}/{notes} comments, {changed} lines changed\n--- saved\n{saved}"
                        ));
                    }
                    // YAML 1.1 readers break lines at these: written escaped.
                    if host == Language::YAML && saved.contains(['\u{85}', '\u{2028}', '\u{2029}'])
                    {
                        failures.push(format!("{host} host, {pointer} = {value:?} via {edit}: a raw line separator\n--- saved\n{saved}"));
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
        failures.is_empty(),
        "{} failing:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}
