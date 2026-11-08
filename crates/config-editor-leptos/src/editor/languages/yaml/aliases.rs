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
        let parts = if alias.merge {
            // A merge parts only when an inherited key no longer reads the
            // anchor's value: the map's own keys change freely.
            let line_start = previous[..alias.start].rfind('\n').map_or(0, |at| at + 1);
            let (_, column) = merge_layout(previous, line_start);
            let own = own_keys(previous, line_start, column);
            match (new.pointer(&alias.pointer), new.pointer(anchor), old.pointer(anchor)) {
                // The anchor's keys before and after: one it gains would be
                // inherited too.
                (Some(Value::Object(map)), Some(Value::Object(source)), Some(Value::Object(was))) => was
                    .keys()
                    .chain(source.keys())
                    // An own key gone (deleted, renamed) lets the anchor's
                    // show through; an inherited one must still match.
                    .any(|key| if own.contains(key) { !map.contains_key(key) } else { map.get(key) != source.get(key) }),
                _ => true,
            }
        } else {
            // An alias edited as its anchor was still reads it.
            let (alias_now, anchor_now) = (new.pointer(&alias.pointer), new.pointer(anchor));
            (old.pointer(&alias.pointer) != alias_now || old.pointer(anchor) != anchor_now)
                && !(alias_now.is_some() && alias_now == anchor_now)
        };
        if !parts {
            continue;
        }
        if alias.merge {
            // The merged keys the map does not write itself, as its entries.
            let line_start = previous[..alias.start].rfind('\n').map_or(0, |at| at + 1);
            let line_end = previous[alias.start..].find('\n').map_or(previous.len(), |at| alias.start + at);
            // On a list item's line (`- <<: *p`) the map starts after the
            // dash: the first entry takes its place, the others its column.
            let (prefix, column) = merge_layout(previous, line_start);
            let Some(Value::Object(merged)) = old.pointer(anchor) else {
                return None;
            };
            let own = own_keys(previous, line_start, column);
            let lines: Vec<String> = merged
                .iter()
                .filter(|(key, _)| !own.contains(*key))
                .enumerate()
                .map(|(index, (key, value))| {
                    let lead = if index == 0 { prefix.clone() } else { " ".repeat(column) };
                    let pointer = format!("{anchor}/{}", key.replace('~', "~0").replace('/', "~1"));
                    format!("{lead}{}: {}", yaml_key(key), written_value(previous, &pointer, value))
                })
                .collect();
            if lines.is_empty() {
                // The item's dash would go with the line.
                if prefix.trim_start().starts_with('-') {
                    return None;
                }
                edits.push((line_start, (line_end + 1).min(previous.len()), String::new()));
            } else {
                edits.push((line_start, line_end, lines.join("\n")));
            }
        } else {
            let value = old.pointer(&alias.pointer)?;
            edits.push((alias.start, alias.end, written_value(previous, anchor, value)));
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

/// `value` as `previous` writes it at `pointer` (`0x1F`, `'quoted'`) when
/// that is one line reading as it, else as YAML writes it.
fn written_value(previous: &str, pointer: &str, value: &Value) -> String {
    let written = || {
        let (line, start, end) = yaml_value_span_for_path(previous, pointer)?;
        let text = previous.lines().nth(line.checked_sub(1)?)?;
        let characters: Vec<char> = text.chars().collect();
        let text: String = characters.get(start.checked_sub(1)?..end.checked_sub(1)?)?.iter().collect();
        let text = text[node_properties_len(&text)..].trim().to_string();
        let reads = YamlBehavior.parse(&format!("k: {text}\n")).ok()?;
        (!text.is_empty() && !text.starts_with(['|', '>', '*']) && reads.get("k") == Some(value)).then_some(text)
    };
    if value.is_object() || value.is_array() {
        return yaml_value(value);
    }
    written().unwrap_or_else(|| yaml_value(value))
}

/// What comes before `<<` on the line at `line_start` (its indentation and
/// any list dashes, `    - `) and the column the merged map's keys sit at.
fn merge_layout(source: &str, line_start: usize) -> (String, usize) {
    let line = source[line_start..].split('\n').next().unwrap_or("");
    let at = line.find("<<").unwrap_or(line.len() - line.trim_start().len());
    (line[..at].to_string(), line[..at].chars().count())
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
    if is_plain_identifier(key) { key.to_string() } else { super::suggestions::yaml_quoted(key) }
}
