//! Lists whose items only moved (`allow` reversed): each item's lines move
//! with its comments and the comment lines above it, instead of every value
//! being rewritten in place under the comments of others.

use super::super::language::ConfigLanguage;
use super::{byte_range, comment_start, escape, inline_location, read};
use serde_json::Value;
use std::ops::Range;

/// `previous` with the moved items' lines moved, when `new` is `old` with
/// the items of some lists in another order and nothing else changed: each
/// on a line of its own, or block list items (maps, block scalars) taken
/// with every line under them.
pub(in super::super) fn patch_reorder(
    language: &dyn ConfigLanguage,
    previous: &str,
    old: &Value,
    new: &Value,
    schema: Option<&Value>,
) -> Option<String> {
    let mut lists = Vec::new();
    permuted_lists(old, new, String::new(), &mut lists)?;
    if lists.is_empty() {
        return None;
    }
    let lines: Vec<&str> = previous.split('\n').collect();
    let line_of = |byte: usize| previous[..byte].matches('\n').count();
    // Each slot: its first host line (comments above the item), the item's
    // first line, and its last line.
    let mut moves: Vec<(std::ops::Range<usize>, Vec<String>)> = Vec::new();
    for (pointer, before, after) in lists {
        // A block list's items (`- name: a` and the lines under it) move
        // with all their lines.
        let block = block_items(language, previous, &lines, &pointer)
            .filter(|slots| slots.len() == before.len());
        let whole_items = block.is_some();
        let slots = match block {
            Some(slots) => slots,
            None => {
                let mut slots: Vec<(usize, usize, usize)> = Vec::new();
                for (index, item) in before.iter().enumerate() {
                    let item_pointer = format!("{pointer}/{index}");
                    let span = language
                        .value_span(previous, &item_pointer)
                        .or_else(|| inline_location(language, previous, &item_pointer, false))?;
                    let range = byte_range(previous, span)?;
                    let line = line_of(range.start);
                    if line != line_of(range.end) || slots.iter().any(|(_, at, _)| *at == line) {
                        return None;
                    }
                    // The line holds this item alone: a list dash before it,
                    // a comma and a comment after it.
                    let line_start = previous[..range.start].rfind('\n').map_or(0, |at| at + 1);
                    let text = lines[line];
                    let before_value = previous[line_start..range.start].trim();
                    let after_value = &text[range.end - line_start..];
                    let after_code = after_value[..comment_start(language, after_value)].trim();
                    if !matches!(before_value, "" | "-")
                        || !matches!(after_code, "" | ",")
                        || item.is_object()
                        || item.is_array()
                    {
                        return None;
                    }
                    // Own-line comments right above it go with it.
                    let mut top = line;
                    while top > 0 && comment_only(language, lines[top - 1]) {
                        top -= 1;
                    }
                    // Not above the list's first item: those comments are the
                    // list's.
                    slots.push((if index == 0 { line } else { top }, line, line));
                }
                slots
            }
        };
        // Each new position takes the block of the old item it holds.
        let mut used = vec![false; before.len()];
        for (slot, item) in after.iter().enumerate() {
            let from = (0..before.len()).find(|at| !used[*at] && before[*at] == *item)?;
            used[from] = true;
            let (top, line, last) = slots[from];
            let mut block: Vec<String> = lines[top..line]
                .iter()
                .map(|line| line.to_string())
                .collect();
            if whole_items {
                block.extend(lines[line..=last].iter().map(|line| line.to_string()));
            } else {
                block.push(with_comma_of(language, lines[line], lines[slots[slot].1]));
            }
            moves.push((slots[slot].0..slots[slot].2 + 1, block));
        }
    }
    moves.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    let mut patched: Vec<String> = lines.iter().map(|line| line.to_string()).collect();
    for (range, block) in moves {
        patched.splice(range, block);
    }
    let patched = patched.join("\n");
    (read(language, &patched, schema).ok().as_ref() == Some(new)).then_some(patched)
}

/// The item line `moved`, with a comma after its value exactly when the
/// line of the place it moves to (`slot`) has one.
fn with_comma_of(language: &dyn ConfigLanguage, moved: &str, slot: &str) -> String {
    let has_comma = |line: &str| {
        line[..comment_start(language, line)]
            .trim_end()
            .ends_with(',')
    };
    let code_end = comment_start(language, moved);
    let code = moved[..code_end].trim_end();
    let rest = &moved[code.len()..];
    let code = code.strip_suffix(',').unwrap_or(code);
    format!("{code}{}{rest}", if has_comma(slot) { "," } else { "" })
}

/// The lists that differ between `old` and `new` only in order
/// (with their pointer and both item orders); `None` when anything else
/// differs.
fn permuted_lists<'a>(
    old: &'a Value,
    new: &'a Value,
    pointer: String,
    out: &mut Vec<(String, &'a [Value], &'a [Value])>,
) -> Option<()> {
    if old == new {
        return Some(());
    }
    match (old, new) {
        (Value::Object(left), Value::Object(right)) => {
            if left.len() != right.len() || !left.keys().zip(right.keys()).all(|(a, b)| a == b) {
                return None;
            }
            for (key, value) in left {
                permuted_lists(
                    value,
                    &right[key],
                    format!("{pointer}/{}", escape(key)),
                    out,
                )?;
            }
            Some(())
        }
        (Value::Array(left), Value::Array(right)) if left.len() == right.len() => {
            let mut rest: Vec<&Value> = right.iter().collect();
            for item in left {
                let at = rest.iter().position(|other| *other == item)?;
                rest.swap_remove(at);
            }
            out.push((pointer, left, right));
            Some(())
        }
        _ => None,
    }
}

/// Whether `line` holds only a comment.
fn comment_only(language: &dyn ConfigLanguage, line: &str) -> bool {
    !line.trim().is_empty() && line[..comment_start(language, line)].trim().is_empty()
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start_matches([' ', '\t']).len()
}

/// The items of the block list at `pointer` (a key's value written `key:`
/// then one `- ` line per item, in a language whose indentation is
/// structure), each as its first line (the comment lines above it at its
/// dash's indent, but above the first item: those are the list's), its
/// dash line, and its last line (deeper comment lines end the item before).
fn block_items(
    language: &dyn ConfigLanguage,
    previous: &str,
    lines: &[&str],
    pointer: &str,
) -> Option<Vec<(usize, usize, usize)>> {
    if !language.indent_is_structure() || pointer.is_empty() {
        return None;
    }
    let key = language.key_span(previous, pointer)?;
    let key_line = key.line.checked_sub(1)?;
    let key_indent = indent_of(lines.get(key_line)?);
    let code = |line: &str| line[..comment_start(language, line)].trim_end().to_string();
    // The key's line ends at its `:`: the list starts on a line of its own.
    let opener = code(lines[key_line]);
    if !opener.ends_with(':') {
        return None;
    }
    let mut items: Vec<(usize, usize, usize)> = Vec::new();
    let mut dash: Option<usize> = None;
    let mut at = key_line + 1;
    while at < lines.len() {
        let line = lines[at];
        if line.trim().is_empty() || comment_only(language, line) {
            at += 1;
            continue;
        }
        let indent = indent_of(line);
        let starts_item = {
            let code = code(line);
            let rest = code.trim_start();
            rest == "-" || rest.starts_with("- ")
        };
        match dash {
            None if starts_item && indent >= key_indent => dash = Some(indent),
            None => return None,
            Some(column) if indent < column || (indent == column && !starts_item) => break,
            Some(_) => {}
        }
        let column = dash?;
        if indent == column {
            items.push((at, at, at));
        } else if let Some(item) = items.last_mut() {
            item.2 = at;
        }
        at += 1;
    }
    let column = dash?;
    for (index, item) in items.iter_mut().enumerate() {
        if index > 0 {
            while item.0 > 0
                && comment_only(language, lines[item.0 - 1])
                && indent_of(lines[item.0 - 1]) <= column
            {
                item.0 -= 1;
            }
        }
    }
    Some(items)
}

/// Where the lists `new` reorders hold comments their items' lines would
/// leave behind if written in place: the pointer of the first such list,
/// when [`patch_reorder`] cannot move its items. The save then waits.
pub(in super::super) fn unmoved_comments(
    language: &dyn ConfigLanguage,
    previous: &str,
    old: &Value,
    new: &Value,
) -> Option<String> {
    let mut lists = Vec::new();
    permuted_lists(old, new, String::new(), &mut lists)?;
    if lists.is_empty() || patch_reorder(language, previous, old, new, None).is_some() {
        return None;
    }
    let lines: Vec<&str> = previous.split('\n').collect();
    lists.into_iter().find_map(|(pointer, _, _)| {
        let items = block_items(language, previous, &lines, &pointer)?;
        let span: Range<usize> = items.first()?.1..items.last()?.2 + 1;
        lines[span]
            .iter()
            .any(|line| comment_start(language, line) < line.len())
            .then_some(pointer)
    })
}
