//! Whether a string may be written as a plain (unquoted) YAML scalar: one
//! rule for a value spliced into the host and for a document written whole,
//! so the string reads back the same in YAML 1.2 readers and in YAML 1.1
//! ones (PyYAML, yaml.v2), which also take `yes`, `on`, `y`, `1_000`, `1:20`,
//! dates, `<<` and `=` for other types.

use serde_json::Value;
use std::sync::LazyLock;

/// The plain forms YAML 1.1 or 1.2 resolves to a type other than a string:
/// booleans, nulls, integers (with `_`, octal, hexadecimal, binary,
/// sexagesimal), floats (with exponents, sexagesimal, `.inf`, `.nan`),
/// timestamps, the merge key and the value key.
static TYPED: LazyLock<regex_lite::Regex> = LazyLock::new(|| {
    regex_lite::Regex::new(concat!(
        r"^(?:",
        r"(?i:y|n|yes|no|true|false|on|off|null|~|<<|=|[-+]?\.inf|\.nan)",
        r"|[-+]?(?:0b[01_]+|0o?[0-7_]+|0x[0-9a-fA-F_]+|[0-9][0-9_]*(?::[0-5]?[0-9])*)",
        r"|[-+]?(?:[0-9][0-9_]*(?::[0-5]?[0-9])*)?\.[0-9_]*(?:[eE][-+]?[0-9]+)?",
        r"|[-+]?[0-9][0-9_]*[eE][-+]?[0-9]+",
        r"|[0-9]{4}-[0-9]{1,2}-[0-9]{1,2}(?:[Tt ].*)?",
        r")$"
    ))
    .expect("the typed scalar pattern")
});

/// Whether `text` reads back as itself written plain, in a block mapping
/// value or sequence item: not empty, no indicator first (a `-` before a
/// non-space is fine: `-Xmx512m`), no `: ` or ` #` inside, no space or tab
/// at its ends, no tab, control character or line break anywhere, not a
/// document marker, and not a form either YAML version types.
pub(super) fn can_be_plain(text: &str) -> bool {
    let mut characters = text.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    let second = characters.next();
    let indicator = "-?:,[]{}#&*!|>'\"%@`".contains(first)
        && !(first == '-' && second.is_some_and(|second| !matches!(second, ' ' | '\t')));
    !indicator
        && !text.chars().any(|character| {
            character.is_control() || character == '\u{feff}' || super::suggestions::needs_yaml_escape(character)
        })
        && !text.starts_with(' ')
        && !text.ends_with([' ', ':'])
        && !text.contains(": ")
        && !text.contains(" #")
        && !text.starts_with("---")
        && !text.starts_with("...")
        && !TYPED.is_match(text)
}

/// [`can_be_plain`] inside a flow collection (`[a, b]`, `{k: v}`), where
/// YAML 1.1 readers end a plain scalar at `?`, `,` and brackets, and older
/// ones refuse a `:` in it (PyYAML before 5.1).
pub(super) fn can_be_plain_in_flow(text: &str) -> bool {
    can_be_plain(text) && !text.contains(['?', ',', '[', ']', '{', '}', ':'])
}

/// `text` (a document the YAML writer wrote for `value`) with each string
/// value it wrote plain that [`can_be_plain`] refuses written in double
/// quotes instead: the writer follows YAML 1.2 alone and leaves `yes`,
/// `1_000` and dates bare.
pub(super) fn quote_typed_plain(text: &str, value: &Value) -> String {
    let mut edits: Vec<(std::ops::Range<usize>, String)> = Vec::new();
    let line_starts: Vec<usize> = std::iter::once(0)
        .chain(text.match_indices('\n').map(|(at, _)| at + 1))
        .collect();
    for (path, line_number, column, end_column) in super::parse::yaml_path_lines(text) {
        let pointer: String = path.iter().map(|segment| format!("/{}", segment.replace('~', "~0").replace('/', "~1"))).collect();
        // A key follows the same rule (`"yes": 1`).
        if let (Some(key), Some(&start)) = (path.last(), line_starts.get(line_number - 1)) {
            let line = &text[start..text[start..].find('\n').map_or(text.len(), |end| start + end)];
            let byte = |column: usize| line.char_indices().nth(column - 1).map_or(line.len(), |(at, _)| at);
            let (from, to) = (byte(column), byte(end_column));
            if line.get(from..to) == Some(key.as_str()) && line[to..].starts_with(':') && !can_be_plain(key) {
                edits.push((start + from..start + to, super::suggestions::yaml_quoted(key)));
            }
        }
        let Some(Value::String(string)) = value.pointer(&pointer) else {
            continue;
        };
        let Some(&start) = line_starts.get(line_number - 1) else {
            continue;
        };
        let line = &text[start..text[start..].find('\n').map_or(text.len(), |end| start + end)];
        if can_be_plain(string) {
            // One the writer quoted that the rule writes plain (`-Xmx512m`,
            // `--flag`): plain, as a spliced value is.
            let quoted = [format!("'{}'", string.replace('\'', "''")), serde_json::to_string(string).unwrap_or_default()];
            if let Some(before) = quoted.iter().find_map(|quoted| line.strip_suffix(quoted.as_str()))
                && (before.ends_with(": ") || before.trim_start() == "- ")
                && super::parse::yaml_parse(&format!("k: {string}\n")).ok() == Some(serde_json::json!({ "k": string }))
            {
                edits.push((start + before.len()..start + line.len(), string.clone()));
            }
            continue;
        }
        // The line ends with the value, after its key's `: ` or its item's
        // `- ` (the writer adds no comments).
        if let Some(before) = line.strip_suffix(string.as_str())
            && (before.ends_with(": ") || before.trim_start() == "- ")
        {
            let quoted = super::suggestions::yaml_quoted(string);
            edits.push((start + before.len()..start + line.len(), quoted));
        }
    }
    let mut out = text.to_string();
    for (range, quoted) in edits.into_iter().rev() {
        out.replace_range(range, &quoted);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_is_kept_and_text_another_type_reads_is_not() {
        for text in ["udp", "a b", "1.2.3", "10m", "-Xmx512m", "x,y", "a:b", "a#b", "café", "v1", "e5", "0.0.0.0", "yesterday", "~x"] {
            assert!(can_be_plain(text), "{text:?} written plain");
        }
        for text in [
            "", "%x", "@x", "`x", "|x", ">x", "!x", "&x", "*x", "?x", "? x", ":x", "#x", "{x", "}x", "[x", "]x", ",x", "'x", "\"x", "- x", "-", "a: b", "a #b", " x", "x ", "x:", "yes",
            "No", "ON", "off", "y", "N", "true", "Null", "~", "1_000", "1:20", "-1:20", "0x1F", "0o7", "017", "0b101", "+1", "-5", ".5", "1.", "1e3", "1E+3", "6.8523015e+5", "190:20:30.15", ".inf",
            "-.Inf", ".NaN", "2001-12-14", "2001-12-14t21:59:43.10-05:00", "2001-12-14 21:59:43.10 -5", "<<", "=", "a\tb", "a\nb", "a\rb", "a\u{2028}b", "---", "...x",
        ] {
            assert!(!can_be_plain(text), "{text:?} quoted");
        }
    }
}
