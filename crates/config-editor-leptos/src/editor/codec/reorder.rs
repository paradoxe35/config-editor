//! Lists whose items only moved (`allow` reversed): each item's line moves
//! with its trailing comment and the comment lines above it, instead of
//! every value being rewritten in place under the comments of others.

use super::super::language::ConfigLanguage;
use super::{byte_range, comment_start, escape, inline_location, read};
use serde_json::Value;

/// `previous` with the moved items' lines moved, when `new` is `old` with
/// the items of some scalar lists in another order and nothing else changed.
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
    // Each slot: the host lines it covers, and the item line in it.
    let mut moves: Vec<(std::ops::Range<usize>, Vec<String>)> = Vec::new();
    for (pointer, before, after) in lists {
        let mut slots: Vec<(usize, usize)> = Vec::new();
        for (index, item) in before.iter().enumerate() {
            let item_pointer = format!("{pointer}/{index}");
            let span = language
                .value_span(previous, &item_pointer)
                .or_else(|| inline_location(language, previous, &item_pointer, false))?;
            let range = byte_range(previous, span)?;
            let line = line_of(range.start);
            if line != line_of(range.end) || slots.iter().any(|(_, at)| *at == line) {
                return None;
            }
            // The line holds this item alone: a list dash before it, a comma
            // and a comment after it.
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
            while top > 0 && {
                let above = lines[top - 1];
                !above.trim().is_empty()
                    && above[..comment_start(language, above)].trim().is_empty()
            } {
                top -= 1;
            }
            // Not above the list's first item: those comments are the list's.
            slots.push((if index == 0 { line } else { top }, line));
        }
        // Each new position takes the block of the old item it holds.
        let mut used = vec![false; before.len()];
        for (slot, item) in after.iter().enumerate() {
            let from = (0..before.len()).find(|at| !used[*at] && before[*at] == *item)?;
            used[from] = true;
            let (top, line) = slots[from];
            let mut block: Vec<String> = lines[top..line]
                .iter()
                .map(|line| line.to_string())
                .collect();
            block.push(with_comma_of(language, lines[line], lines[slots[slot].1]));
            moves.push((slots[slot].0..slots[slot].1 + 1, block));
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

/// The lists of scalars that differ between `old` and `new` only in order
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
