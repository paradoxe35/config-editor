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
        if !alias.merge {
            // An alias edited as its anchor was still reads it.
            let (alias_now, anchor_now) = (new.pointer(&alias.pointer), new.pointer(anchor));
            let parts = (old.pointer(&alias.pointer) != alias_now || old.pointer(anchor) != anchor_now)
                && !(alias_now.is_some() && alias_now == anchor_now);
            if parts {
                let value = old.pointer(&alias.pointer)?;
                edits.push((alias.start, alias.end, written_value(previous, anchor, value)));
            }
            continue;
        }
        let line_start = previous[..alias.start].rfind('\n').map_or(0, |at| at + 1);
        let line_end = previous[alias.start..].find('\n').map_or(previous.len(), |at| alias.start + at);
        // On a list item's line (`- <<: *p`) the map starts after the dash.
        let (prefix, column) = merge_layout(previous, line_start);
        let own = own_keys(previous, line_start, column);
        let (Some(Value::Object(was)), Some(Value::Object(was_map))) = (old.pointer(anchor), old.pointer(&alias.pointer)) else {
            return None;
        };
        let entry = |key: &String, value: &Value| {
            let pointer = format!("{anchor}/{}", key.replace('~', "~0").replace('/', "~1"));
            format!("{}: {}", yaml_key(key), written_value(previous, &pointer, value))
        };
        let lone = lone_merge(&previous[line_start..line_end], &alias.name);
        match merge_parting(new.pointer(&alias.pointer), new.pointer(anchor), was, &own) {
            Parting::No => {}
            // The merge stays: each inherited key the map now holds
            // otherwise becomes its own, as it read before the edit, on
            // the merge's side where it reads in the same place.
            Parting::Overrides(keys) if lone && let Some((before, after)) = overrides_around(was, &own, &keys) => {
                let lines = |keys: &[String]| -> Option<Vec<String>> { keys.iter().map(|key| Some(entry(key, was_map.get(key)?))).collect() };
                let (before, after) = (lines(&before)?, lines(&after)?);
                let indent = " ".repeat(column);
                if !before.is_empty() {
                    let at = line_start + prefix.len();
                    let text: String = before.iter().map(|line| format!("{line}\n{indent}")).collect();
                    edits.push((at, at, text));
                }
                if !after.is_empty() {
                    let text: String = after.iter().map(|line| format!("\n{indent}{line}")).collect();
                    edits.push((line_end, line_end, text));
                }
            }
            // No override reads as the edit (an inherited key gone): the
            // merged keys the map does not write itself, as its entries,
            // the first in the merge's place, the others at its column.
            // Those the edit keeps come first, so the merge's comments stay
            // with one of them.
            Parting::Overrides(_) | Parting::WrittenOut => {
                let kept = |key: &String| new.pointer(&alias.pointer).and_then(|map| map.get(key)).is_some();
                let (mut entries, gone): (Vec<_>, Vec<_>) = was.iter().filter(|(key, _)| !own.contains(*key)).partition(|(key, _)| kept(key));
                entries.extend(gone);
                let lines: Vec<String> = entries
                    .into_iter()
                    .enumerate()
                    .map(|(index, (key, value))| {
                        let lead = if index == 0 { prefix.clone() } else { " ".repeat(column) };
                        format!("{lead}{}", entry(key, value))
                    })
                    .collect();
                if lines.is_empty() {
                    // The item's dash would go with the line.
                    if prefix.trim_start().starts_with('-') {
                        return None;
                    }
                    edits.push((line_start, (line_end + 1).min(previous.len()), String::new()));
                } else {
                    // The merge line's comment stays, on the first entry.
                    let line = &previous[line_start..line_end];
                    let comment = &line[code_before_comment(line).trim_end().len()..];
                    let mut lines = lines;
                    lines[0].push_str(comment);
                    edits.push((line_start, line_end, lines.join("\n")));
                }
            }
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

/// How a merge (`<<: *base`) reads the edit.
enum Parting {
    /// As the anchor's keys still: it stays.
    No,
    /// Otherwise for these inherited keys, which the map still holds: it
    /// stays, and they become the map's own.
    Overrides(Vec<String>),
    /// Without an inherited key, or without its anchor: written out.
    WrittenOut,
}

/// How the merged map `map` reads the edit beside its anchor `source`
/// (`was` before it), `own` the keys the map writes itself.
fn merge_parting(map: Option<&Value>, source: Option<&Value>, was: &serde_json::Map<String, Value>, own: &[String]) -> Parting {
    let (Some(Value::Object(map)), Some(Value::Object(source))) = (map, source) else {
        return Parting::WrittenOut;
    };
    // An anchor emptied writes no entries for the merge to read.
    if source.is_empty() && !was.is_empty() {
        return Parting::WrittenOut;
    }
    let mut overrides: Vec<String> = Vec::new();
    // The anchor's keys before and after: one it gains would be inherited
    // too.
    for key in was.keys().chain(source.keys()) {
        if own.contains(key) {
            // An own key gone (deleted, renamed) lets the anchor's show
            // through.
            if !map.contains_key(key) {
                return Parting::WrittenOut;
            }
        } else if map.get(key) != source.get(key) && !overrides.contains(key) {
            // An override reads as the map did: only for a key it held.
            if !map.contains_key(key) || !was.contains_key(key) {
                return Parting::WrittenOut;
            }
            overrides.push(key.clone());
        }
    }
    if overrides.is_empty() { Parting::No } else { Parting::Overrides(overrides) }
}

/// Whether `line` merges the one alias `*name` (`  <<: *name  # c`), not a
/// list of them.
fn lone_merge(line: &str, name: &str) -> bool {
    let code = code_before_comment(line);
    code.split_once("<<")
        .and_then(|(_, rest)| rest.trim_start().strip_prefix(':'))
        .is_some_and(|rest| rest.trim() == format!("*{name}"))
}

/// The overridden `keys` written before the merge and after it, so the map
/// reads its keys in the order it did (the merged ones where the merge
/// is): those leading the inherited keys go before it, those ending them
/// after; `None` when some sit between keys still inherited.
fn overrides_around(was: &serde_json::Map<String, Value>, own: &[String], keys: &[String]) -> Option<(Vec<String>, Vec<String>)> {
    let inherited: Vec<&String> = was.keys().filter(|key| !own.contains(key)).collect();
    let leading = inherited.iter().take_while(|key| keys.contains(key)).count();
    let trailing = inherited[leading..].iter().rev().take_while(|key| keys.contains(key)).count();
    if leading + trailing != keys.len() {
        return None;
    }
    let before = inherited[..leading].iter().map(|key| (*key).clone()).collect();
    let after = inherited[inherited.len() - trailing..].iter().map(|key| (*key).clone()).collect();
    Some((before, after))
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
