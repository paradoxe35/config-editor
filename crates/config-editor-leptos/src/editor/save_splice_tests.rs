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
        // Indentless sequences (`l:` then `- …` at its column) holding
        // maps with blocks, compact nested sequences and a block scalar.
        (
            Language::YAML,
            "# note1\nl:\n- k: >-\n    x\n  j: y # note2\n- - a # note3\n  - b\n- |\n  lit\n- plain # note4\nm:\n  l:\n  - k: |-\n      x\n    j: z\nn: 1  # note5\n",
        ),
        // Quoted and plain scalars over several lines: folded breaks, blank
        // lines, an escaped break, a list item.
        (
            Language::YAML,
            "# note1\nml: \"l1\n\n  l2\"\nq2: \"l1\n  l2\"  # note2\nq3: 'l1\n\n  l2'\npl: plain\n  continued\nl:\n  - \"l1\n    l2\"\nq6: \"l1 \\\n  l2\"\nn: 1  # note3\n",
        ),
        // Flow collections, nested and with maps, where YAML 1.1 ends a
        // plain scalar at `?`, `,`, brackets and `:`.
        (
            Language::YAML,
            "# note1\nf: [a, b] # note2\ng: {k: v, j: w}\nh: [{k: v}, [x, y]] # note3\nn: 1\n",
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
    // What YAML 1.1 ends a plain scalar at inside a flow collection.
    "http://h/p?q=1",
    "a,b",
    "x[0]",
    "a:b",
    "x}",
    // Characters no reader takes raw: control characters, noncharacters.
    "a\u{1}b",
    "\u{0}",
    "a\u{7f}b",
    "\u{fffe}x",
    "x\u{ffff}",
    "a\u{9c}b",
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

/// Whether a list on the way to `pointer` in `document` holds a list.
fn beside_nested_list(document: &Value, pointer: &str) -> bool {
    let mut at = document;
    for segment in pointer.split('/').skip(1) {
        if let Value::Array(items) = at
            && items.iter().any(Value::is_array)
        {
            return true;
        }
        match at
            .get(segment)
            .or_else(|| at.get(segment.parse::<usize>().ok()?))
        {
            Some(next) => at = next,
            None => return false,
        }
    }
    false
}

/// Whether `host` refuses `character` written raw in its text: control
/// characters (but tab and line feed) everywhere; DEL in TOML and YAML; C1
/// controls in YAML (but its line break U+0085, checked apart); the
/// noncharacters U+FFFE and U+FFFF in YAML and XML.
pub(super) fn unreadable_raw(host: Language, character: char) -> bool {
    let c0 = character < ' ' && !matches!(character, '\t' | '\n');
    let del = character == '\u{7f}' && (host == Language::TOML || host == Language::YAML);
    let c1 = ('\u{80}'..='\u{9f}').contains(&character)
        && character != '\u{85}'
        && host == Language::YAML;
    let nonchar = matches!(character, '\u{fffe}' | '\u{ffff}')
        && (host == Language::YAML || host == Language::XML);
    c0 || del || c1 || nonchar
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
fn yaml_1_1_reads_plain_as_text(written: &str, in_flow: bool) -> bool {
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
    // Inside a flow collection a plain scalar ends at `?`, `,` and
    // brackets, and older PyYAML refuses a `:` anywhere in it.
    let flow_stop = in_flow && written.contains(['?', ',', '[', ']', '{', '}', ':']);
    !indicator
        && !flow_stop
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
/// plain scalar, and whether it is inside a flow collection.
fn yaml_plain_written<'a>(saved: &'a str, pointer: &str) -> Option<(&'a str, bool)> {
    let behavior = Language::YAML.behavior();
    let span = behavior
        .value_span(saved, pointer)
        .or_else(|| super::codec::inline_location(behavior, saved, pointer, false))?;
    let line = saved.lines().nth(span.line - 1)?;
    let start = line.char_indices().nth(span.start_column - 1)?.0;
    let end = line
        .char_indices()
        .nth(span.end_column - 1)
        .map_or(line.len(), |(at, _)| at);
    let written = line.get(start..end)?;
    let in_flow = line[..start].contains(['[', '{']);
    (!written.starts_with(['"', '\'', '|', '>', '&', '*', '!', '[', '{']))
        .then_some((written, in_flow))
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
                    // The XML view shows nested lists flat: their items,
                    // and the items after them, are not where the pointer
                    // says (the bridge tests cover them).
                    if edit == Language::XML && beside_nested_list(&old, pointer) {
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
                    // No reader takes these raw: written escaped, or refused.
                    if let Some(raw) = saved
                        .chars()
                        .find(|character| unreadable_raw(host, *character))
                    {
                        failures.push(format!("{case}: a raw {raw:?}\n--- saved\n{saved:?}"));
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
                            && let Some((written, in_flow)) = yaml_plain_written(&saved, pointer)
                            && !yaml_1_1_reads_plain_as_text(written, in_flow)
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

/// Keys a rename may write: forms YAML 1.1 types, ones no language writes
/// bare (spaces, dots, a digit first, indicators), and Unicode.
const KEYS: &[&str] = &[
    "on",
    "yes",
    "No",
    "y",
    "null",
    "~",
    "true",
    "1_000",
    "0b101",
    "12",
    "1.5",
    "2001-12-14",
    "host name",
    "1host",
    "a.b",
    "x-y",
    "a: b",
    "#k",
    "k #c",
    "-k",
    "? k",
    "*k",
    "&k",
    "tab\tk",
    "k:",
    "<<",
    "=",
    "café",
    "a\"b",
    "[k]",
    "a,b",
    "\u{fffe}k",
    // Plain where every reader takes them as text (round 19 quoted them).
    "foo bar",
    "Über",
    "a/b",
    "1.2.3",
    "-x",
    "x.y-z_1",
];

/// The JSON pointers of every key in `value` (the pointer of its value).
fn keys(value: &Value, pointer: String, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let child_pointer =
                    format!("{pointer}/{}", key.replace('~', "~0").replace('/', "~1"));
                // XML's text slot is no key the host writes.
                if key != "#text" {
                    out.push(child_pointer.clone());
                }
                keys(child, child_pointer, out);
            }
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                keys(child, format!("{pointer}/{index}"), out);
            }
        }
        _ => {}
    }
}

/// `document` with the key at `pointer` renamed `to` in its place.
fn rename(document: &mut Value, pointer: &str, to: &str) -> bool {
    let Some((parent, key)) = pointer.rsplit_once('/') else {
        return false;
    };
    let key = key.replace("~1", "/").replace("~0", "~");
    let Some(Value::Object(map)) = find_slot(document, parent) else {
        return false;
    };
    if !map.contains_key(&key) || map.contains_key(to) {
        return false;
    }
    *map = std::mem::take(map)
        .into_iter()
        .map(|(name, value)| (if name == key { to.to_string() } else { name }, value))
        .collect();
    true
}

/// HCL openers: a block (`name {`) and an object attribute (`name = {`) are
/// different things, so a rename must keep the count of each.
fn hcl_openers(text: &str) -> (usize, usize) {
    // The text before each line's first `{`, its strings left out.
    let heads = text.lines().filter_map(|line| {
        let head = line.split_once('{')?.0.trim();
        let unquoted: String = head.split('"').step_by(2).collect();
        (!head.is_empty() && !head.starts_with(['#', '/'])).then_some(unquoted)
    });
    let (attributes, blocks): (Vec<String>, Vec<String>) =
        heads.partition(|head| head.contains('='));
    (blocks.len(), attributes.len())
}

/// A renamed key is renamed where it is written, whatever it holds: every
/// key of a commented host set to a key that needs quotes (or that YAML 1.1
/// types written bare), from every other view. Comments and the other lines
/// stay, the key reads back, an HCL block stays a block, and a YAML key is
/// written plain only where YAML 1.1 reads it as text.
#[test]
fn any_key_renamed_from_any_view_is_renamed_in_place() {
    let mut failures = Vec::new();
    let (mut tried, mut saved_count) = (0, 0);
    for (host, text) in hosts() {
        let mut pointers = Vec::new();
        keys(
            &host.parse(text).expect("parses"),
            String::new(),
            &mut pointers,
        );
        let notes = text.matches("note").count();
        for pointer in &pointers {
            for to in KEYS {
                for edit in ALL.into_iter().filter(|edit| *edit != host) {
                    let shown = std::cell::Cell::new(true);
                    let set = |document: &mut Value| {
                        if !rename(document, pointer, to) {
                            shown.set(false);
                        }
                    };
                    let renders = WHOLE_RENDERS.with(std::cell::Cell::get);
                    let Ok(saved) = save_conversion(host, edit, text, &set) else {
                        continue;
                    };
                    if !shown.get() {
                        continue;
                    }
                    tried += 1;
                    let case = format!("{host} host, {pointer} renamed {to:?} via {edit}");
                    let (losses, saved) = (saved.losses, saved.text);
                    if !losses.is_empty() {
                        continue;
                    }
                    saved_count += 1;
                    if WHOLE_RENDERS.with(std::cell::Cell::get) != renders {
                        failures.push(format!(
                            "{case}: the whole document written\n--- saved\n{saved}"
                        ));
                    }
                    let kept = saved.matches("note").count();
                    let changed = changed_lines(text, &saved);
                    if kept < notes || changed > 2 {
                        failures.push(format!(
                            "{case}: {kept}/{notes} comments, {changed} lines changed\n--- saved\n{saved}"
                        ));
                    }
                    if let Some(raw) = saved
                        .chars()
                        .find(|character| unreadable_raw(host, *character))
                    {
                        failures.push(format!("{case}: a raw {raw:?}\n--- saved\n{saved:?}"));
                    }
                    let mut expected = host.parse(text).expect("parses");
                    rename(&mut expected, pointer, to);
                    if host.parse(&saved).ok().as_ref() != Some(&expected) {
                        failures.push(format!("{case}: reads back otherwise\n--- saved\n{saved}"));
                    }
                    if host == Language::HCL && hcl_openers(&saved) != hcl_openers(text) {
                        failures.push(format!(
                            "{case}: a block and an attribute switched\n--- saved\n{saved}"
                        ));
                    }
                    if host == Language::YAML
                        && let Some((written, in_flow)) = yaml_key_written(&saved, pointer, to)
                        && !yaml_1_1_reads_plain_as_text(written, in_flow)
                    {
                        failures.push(format!(
                            "{case}: key written plain as {written:?}, which YAML 1.1 reads otherwise\n--- saved\n{saved}"
                        ));
                    }
                    // A block mapping's key is quoted only where the document
                    // writer quotes it too.
                    if host == Language::YAML
                        && !in_flow_mapping(text, pointer)
                        && writer_writes_key_plain(to)
                        && yaml_key_written(&saved, pointer, to).is_none()
                    {
                        failures.push(format!(
                            "{case}: key quoted, which the writer writes plain\n--- saved\n{saved}"
                        ));
                    }
                }
            }
        }
    }
    assert!(
        saved_count * 10 > tried * 5,
        "only {saved_count} of {tried} saved"
    );
    assert!(
        failures.is_empty(),
        "{} failing:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

/// Whether the YAML writer writes `key` plain in a block mapping.
fn writer_writes_key_plain(key: &str) -> bool {
    Language::YAML
        .serialize(&serde_json::json!({ key: 0 }))
        .ok()
        .is_some_and(|whole| {
            whole.lines().count() == 1 && yaml_key_written(&whole, "/k", key).is_some()
        })
}

/// Whether the key at `pointer` of the YAML `text` is in a flow mapping.
fn in_flow_mapping(text: &str, pointer: &str) -> bool {
    let Some(span) = Language::YAML.behavior().key_span(text, pointer) else {
        return true;
    };
    let line = text.lines().nth(span.line - 1).unwrap_or("");
    let start = line
        .char_indices()
        .nth(span.start_column - 1)
        .map_or(line.len(), |(at, _)| at);
    line[..start].contains(['[', '{'])
}

/// The renamed key at `pointer` (renamed `to`) of the YAML `saved` as
/// written, when it is written bare, and whether it is in a flow mapping.
fn yaml_key_written<'a>(saved: &'a str, pointer: &str, to: &str) -> Option<(&'a str, bool)> {
    let (parent, _) = pointer.rsplit_once('/')?;
    let renamed = format!("{parent}/{}", to.replace('~', "~0").replace('/', "~1"));
    let span = Language::YAML.behavior().key_span(saved, &renamed)?;
    let line = saved.lines().nth(span.line - 1)?;
    let start = line.char_indices().nth(span.start_column - 1)?.0;
    let end = line
        .char_indices()
        .nth(span.end_column - 1)
        .map_or(line.len(), |(at, _)| at);
    let written = line.get(start..end)?;
    let in_flow = line[..start].contains(['[', '{']);
    let quoted = written.starts_with(['"', '\'']) || line[..start].ends_with(['"', '\'']);
    (!quoted).then_some((written, in_flow))
}

/// A string written over several lines (a YAML quoted or plain scalar
/// folded over lines, a TOML multi-line string, a heredoc, XML text) edited
/// a little from another view keeps its lines: a character typed at its
/// start, its end, or in the middle of one of its lines changes that line
/// only, and the value reads back (round 20: YAML joined `"l1\n  l2"` into
/// `"l1 l2Q"`).
#[test]
fn a_small_edit_of_a_string_over_lines_keeps_its_lines() {
    let mut failures = Vec::new();
    let mut tried = 0;
    for (host, text) in hosts() {
        let old = host.parse(text).expect("parses");
        let mut pointers = Vec::new();
        strings(&old, String::new(), &mut pointers);
        let behavior = host.behavior();
        for pointer in &pointers {
            let Some(span) = behavior.value_span(text, pointer) else {
                continue;
            };
            let Some(value) = old.pointer(pointer).and_then(Value::as_str) else {
                continue;
            };
            // Written over lines, its first line holding some of it.
            let first = text.lines().nth(span.line - 1).unwrap_or("");
            let opener = first
                .chars()
                .skip(span.start_column - 1)
                .collect::<String>();
            let over_lines = span.end_column - 1 > first.chars().count()
                && !opener.trim_start().starts_with(['|', '>']);
            if !over_lines || host != Language::YAML {
                continue;
            }
            // The middle of its last word.
            let middle = value.trim_end().rfind(' ').map_or(1, |at| at + 2);
            let edits = [
                format!("{value}Q"),
                format!("Q{value}"),
                format!("{}Q{}", &value[..middle], &value[middle..]),
            ];
            for edited in &edits {
                for edit in ALL.into_iter().filter(|edit| *edit != host) {
                    let set = |document: &mut Value| {
                        if let Some(slot) = find_slot(document, pointer) {
                            *slot = edited.as_str().into();
                        }
                    };
                    tried += 1;
                    let case = format!("{host} host, {pointer} = {edited:?} via {edit}");
                    let saved = match save_conversion(host, edit, text, &set) {
                        Ok(saved) => saved.text,
                        Err(error) => {
                            failures.push(format!("{case}: {error}"));
                            continue;
                        }
                    };
                    let read = host
                        .parse(&saved)
                        .ok()
                        .and_then(|read| read.pointer(pointer).cloned());
                    let changed = text
                        .lines()
                        .zip(saved.lines())
                        .filter(|(before, after)| before != after)
                        .count();
                    if read != Some(Value::from(edited.as_str()))
                        || changed != 1
                        || text.lines().count() != saved.lines().count()
                    {
                        failures.push(format!(
                            "{case}: {changed} lines changed, reads {read:?}\n--- host\n{text}--- saved\n{saved}"
                        ));
                    }
                }
            }
        }
    }
    assert!(tried > 20, "only {tried} cases");
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
        let document =
            serde_json::json!({ "k": value, "l": [value, "x"], "m": { "n": value }, *value: 1 });
        let Ok(text) = super::codec::serialize(Language::YAML.behavior(), &document, None) else {
            failures.push(format!("{value:?}: not written"));
            continue;
        };
        if Language::YAML.parse(&text).ok().as_ref() != Some(&document) {
            failures.push(format!("{value:?}: reads back otherwise\n{text}"));
        }
        for pointer in ["/k", "/l/0", "/m/n"] {
            if let Some((written, in_flow)) = yaml_plain_written(&text, pointer)
                && !yaml_1_1_reads_plain_as_text(written, in_flow)
            {
                failures.push(format!("{value:?} at {pointer}: written plain\n{text}"));
            }
        }
        if let Some((written, in_flow)) = yaml_key_written(&text, "/k", value)
            && !yaml_1_1_reads_plain_as_text(written, in_flow)
        {
            failures.push(format!("{value:?} as a key: written plain\n{text}"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} failing:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

/// Every language's writer escapes what its readers refuse raw (control
/// characters, noncharacters), or refuses the value: the headless
/// `serialize` and a conversion never write a file a reader rejects.
#[test]
fn any_string_written_whole_holds_no_raw_character_a_reader_refuses() {
    let mut failures = Vec::new();
    for host in ALL {
        for value in VALUES {
            let document =
                serde_json::json!({ "k": value, "l": [value, "x"], "m": { "n": value } });
            let Ok(text) = host.serialize(&document) else {
                continue;
            };
            if let Some(raw) = text
                .chars()
                .find(|character| unreadable_raw(host, *character))
            {
                failures.push(format!(
                    "{host}: {value:?} written with a raw {raw:?}\n{text:?}"
                ));
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
