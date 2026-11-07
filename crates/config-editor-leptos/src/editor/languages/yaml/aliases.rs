//! Anchors and aliases kept through a save: an alias (`udp: *p`) or merge
//! key (`<<: *client`) whose value parts from its anchor's is written out
//! in place first, so the anchor and everything else stay as written.

use super::*;
use std::collections::HashMap;

/// An anchor (`&name`) or alias (`*name`) token on a line.
struct Shared {
    name: String,
    start: usize,
    end: usize,
    pointer: String,
    merge: bool,
}

pub(super) fn detach_shared(previous: &str, old: &Value, new: &Value) -> Option<String> {
    if !previous.contains('*') {
        return None;
    }
    let mut anchors: HashMap<String, String> = HashMap::new();
    let mut aliases: Vec<Shared> = Vec::new();
    let mut offset = 0;
    for line in previous.split_inclusive('\n') {
        let code = code_before_comment(line.trim_end_matches('\n'));
        for (at, sigil) in code.char_indices().filter(|(_, character)| matches!(character, '&' | '*')) {
            let before = code[..at].chars().next_back();
            if !before.is_none_or(|character| character.is_whitespace() || "[{,:".contains(character)) {
                continue;
            }
            let name: String = code[at + 1..]
                .chars()
                .take_while(|character| character.is_alphanumeric() || matches!(character, '_' | '-'))
                .collect();
            if name.is_empty() || in_quotes(code, at) {
                continue;
            }
            let start = offset + at;
            let context = completion_context(previous, previous[..start].encode_utf16().count());
            let merge = context.value_key.as_deref() == Some("<<");
            let mut segments = context.path.clone();
            if let Some(key) = context.value_key.filter(|_| !merge) {
                segments.push(SchemaPathSegment::Key(key));
            }
            let pointer = segments
                .iter()
                .map(|segment| match segment {
                    SchemaPathSegment::Key(key) => format!("/{}", key.replace('~', "~0").replace('/', "~1")),
                    SchemaPathSegment::ArrayItem(index) => format!("/{index}"),
                })
                .collect::<String>();
            if sigil == '&' {
                anchors.insert(name, pointer);
            } else {
                aliases.push(Shared { end: start + 1 + name.len(), name, start, pointer, merge });
            }
        }
        offset += line.len();
    }
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    for alias in aliases {
        let Some(anchor) = anchors.get(&alias.name) else {
            continue;
        };
        let parts = old.pointer(&alias.pointer) != new.pointer(&alias.pointer) || old.pointer(anchor) != new.pointer(anchor);
        if !parts {
            continue;
        }
        if alias.merge {
            // The merged keys the map does not write itself, as its entries.
            let line_start = previous[..alias.start].rfind('\n').map_or(0, |at| at + 1);
            let line_end = previous[alias.start..].find('\n').map_or(previous.len(), |at| alias.start + at);
            let indent = &previous[line_start..line_start + previous[line_start..].len() - previous[line_start..].trim_start().len()];
            let Some(Value::Object(merged)) = old.pointer(anchor) else {
                return None;
            };
            let own = own_keys(previous, line_start, indent.len());
            let lines: Vec<String> = merged
                .iter()
                .filter(|(key, _)| !own.contains(*key))
                .map(|(key, value)| format!("{indent}{}: {}", yaml_key(key), yaml_value(value)))
                .collect();
            if lines.is_empty() {
                edits.push((line_start, (line_end + 1).min(previous.len()), String::new()));
            } else {
                edits.push((line_start, line_end, lines.join("\n")));
            }
        } else {
            let value = old.pointer(&alias.pointer)?;
            edits.push((alias.start, alias.end, yaml_value(value)));
        }
    }
    if edits.is_empty() {
        return None;
    }
    edits.sort_by_key(|(start, _, _)| std::cmp::Reverse(*start));
    let mut detached = previous.to_string();
    for (start, end, text) in edits {
        detached.replace_range(start..end, &text);
    }
    Some(detached)
}

/// Whether byte `at` of `code` sits inside a quoted string.
fn in_quotes(code: &str, at: usize) -> bool {
    let mut quote = None;
    for character in code[..at].chars() {
        match quote {
            Some(open) if character == open => quote = None,
            Some(_) => {}
            None if matches!(character, '"' | '\'') => quote = Some(character),
            None => {}
        }
    }
    quote.is_some()
}

/// The keys written at column `indent` in the block holding the line at
/// `line_start`, besides `<<`.
fn own_keys(source: &str, line_start: usize, indent: usize) -> Vec<String> {
    let column = |line: &str| line.len() - line.trim_start().len();
    let mut keys = Vec::new();
    let lines: Vec<&str> = source.split('\n').collect();
    let at = source[..line_start].matches('\n').count();
    let mut take = |line: &str| {
        let code = code_before_comment(line).trim();
        if let Some((key, _)) = code.split_once(':') {
            let key = key.trim().trim_matches(['"', '\'']);
            if key != "<<" {
                keys.push(key.to_string());
            }
        }
    };
    for line in lines[..at].iter().rev() {
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        if column(line) < indent {
            break;
        }
        if column(line) == indent {
            take(line);
        }
    }
    for line in &lines[at + 1..] {
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        if column(line) < indent {
            break;
        }
        if column(line) == indent {
            take(line);
        }
    }
    keys
}

/// A key as YAML writes it bare where it can.
fn yaml_key(key: &str) -> String {
    if is_plain_identifier(key) { key.to_string() } else { serde_json::to_string(key).unwrap_or_else(|_| key.to_string()) }
}
