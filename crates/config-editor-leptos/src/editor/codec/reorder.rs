//! Lists whose items only moved (`allow` reversed): each item's lines move
//! with its comments and the comment lines above it, instead of every value
//! being rewritten in place under the comments of others.

use super::super::flow_syntax::Flow;
use super::super::language::ConfigLanguage;
use super::{byte_range, comment_start, escape, inline_location, reads_as};
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
    // Each list's old items in their new places: the host lines each new
    // place covers, and the lines it takes.
    let mut moves: Vec<(Range<usize>, Vec<String>)> = Vec::new();
    for (pointer, before, after) in lists {
        // A block list's items (`- name: a` and the lines under it) move
        // with all their lines.
        let block = block_items(language, previous, &lines, &pointer)
            .filter(|slots| slots.len() == before.len());
        let whole_items = block.is_some();
        let slots = match block {
            Some(slots) => slots,
            None => line_items(language, previous, &lines, &pointer, before)?,
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
            block.extend(lines[line..last].iter().map(|line| line.to_string()));
            if whole_items {
                block.push(lines[last].to_string());
            } else {
                block.push(with_comma_of(language, lines[last], lines[slots[slot].2]));
            }
            moves.push((slots[slot].0..slots[slot].2 + 1, block));
        }
    }
    // Lists apart from each other: no two moves cover the same line.
    moves.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    if moves.windows(2).any(|pair| pair[1].0.end > pair[0].0.start) {
        return None;
    }
    let mut patched: Vec<String> = lines.iter().map(|line| line.to_string()).collect();
    for (range, block) in moves {
        patched.splice(range, block);
    }
    let patched = patched.join("\n");
    reads_as(language, &patched, schema, new).then_some(patched)
}

/// The items of the list at `pointer`, each on lines of its own: a scalar
/// or a one-line container (a list dash before it, a comma or an XML end
/// tag after it, a comment after that), or a repeated element over lines
/// (`<s>` to its `</s>`) or the language's own item lines (a TOML `[[s]]`
/// table, an HCL repeated block). Each as its first line (the comment lines right
/// above it), its line, and its last line; `None` when an item shares a
/// line or spreads over lines another way.
fn line_items(
    language: &dyn ConfigLanguage,
    previous: &str,
    lines: &[&str],
    pointer: &str,
    before: &[Value],
) -> Option<Vec<(usize, usize, usize)>> {
    let line_of = |byte: usize| previous[..byte].matches('\n').count();
    let flow = Flow::of(language);
    let mut slots: Vec<(usize, usize, usize)> = Vec::new();
    for (index, item) in before.iter().enumerate() {
        let item_pointer = format!("{pointer}/{index}");
        let container = item.is_object() || item.is_array();
        let own_lines = language
            .item_lines(previous, &item_pointer)
            .or_else(|| element_lines(language, previous, &item_pointer));
        let (range, own) = match own_lines {
            Some(range) if container => (range, true),
            _ => {
                // A container's own span may be its opening bracket only.
                let span = if container {
                    inline_location(language, previous, &item_pointer, false)?
                } else {
                    language
                        .value_span(previous, &item_pointer)
                        .or_else(|| inline_location(language, previous, &item_pointer, false))?
                };
                (byte_range(previous, span)?, false)
            }
        };
        let (line, last) = (line_of(range.start), line_of(range.end));
        if (!own && line != last) || slots.iter().any(|(_, _, end)| *end >= line) {
            return None;
        }
        let line_start = previous[..range.start].rfind('\n').map_or(0, |at| at + 1);
        let last_start = previous[..range.end].rfind('\n').map_or(0, |at| at + 1);
        let before_value = previous[line_start..range.start].trim();
        let after_value = &lines[last][range.end - last_start..];
        let after_code = after_value[..comment_start(language, after_value)].trim();
        let marker_only = before_value.is_empty()
            || flow
                .after_item_marker(before_value)
                .is_some_and(|rest| rest.trim().is_empty());
        let separator_only = after_code.is_empty() || after_code.chars().eq(flow.separator());
        let alone = marker_only && separator_only;
        let whole = format!("{before_value}{}{after_code}", &previous[range.clone()]);
        if !alone && (own || !in_own_element(language, before_value, &whole)) {
            return None;
        }
        // Own-line comments right above it go with it, at the file's top
        // too: a file's header is the comments a blank line ends.
        let mut top = line;
        while top > 0 && comment_only(language, lines[top - 1]) {
            top -= 1;
        }
        slots.push((top, line, last));
    }
    Some(slots)
}

/// The bytes of the element at `pointer`, from the markup opening its name
/// (XML's `<`) to where the language says it ends.
fn element_lines(
    language: &dyn ConfigLanguage,
    previous: &str,
    pointer: &str,
) -> Option<Range<usize>> {
    let name = byte_range(previous, language.key_span(previous, pointer)?)?;
    let open = super::key_markup_start(language, previous, name.start)?;
    Some(open..language.element_end(previous, open)?)
}

/// Whether a value is alone in its element on its line: `whole` (the
/// line's code from the element's start) is one element, opened by the one
/// start before the value (`before`, as `<l>` before `1` in `<l>1</l>`).
fn in_own_element(language: &dyn ConfigLanguage, before: &str, whole: &str) -> bool {
    language.key_markup_len(before).is_some()
        && language.unclosed_containers(before) == 1
        && language.element_end(whole, 0) == Some(whole.len())
}

/// The item line `moved`, with a comma after its value exactly when the
/// line of the place it moves to (`slot`) has one.
fn with_comma_of(language: &dyn ConfigLanguage, moved: &str, slot: &str) -> String {
    let Some(separator) = Flow::of(language).separator() else {
        return moved.to_string();
    };
    let has_comma = |line: &str| {
        line[..comment_start(language, line)]
            .trim_end()
            .ends_with(separator)
    };
    let code_end = comment_start(language, moved);
    let code = moved[..code_end].trim_end();
    let rest = &moved[code.len()..];
    let code = code.strip_suffix(separator).unwrap_or(code);
    let comma = if has_comma(slot) {
        separator.to_string()
    } else {
        String::new()
    };
    format!("{code}{comma}{rest}")
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
            let permuted = left.iter().all(|item| {
                rest.iter()
                    .position(|other| *other == item)
                    .map(|at| rest.swap_remove(at))
                    .is_some()
            });
            if permuted {
                out.push((pointer, left, right));
                return Some(());
            }
            // Not moved itself: lists inside its items may be (`sub` of a
            // list item reordered).
            for (index, (item, other)) in left.iter().zip(right).enumerate() {
                permuted_lists(item, other, format!("{pointer}/{index}"), out)?;
            }
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
/// structure), each as its first line (the comment lines right above it at
/// its dash's indent or further out, at exactly its indent above the first
/// item), its dash line, and its last line (deeper comment lines end the
/// item before).
pub(super) fn block_items(
    language: &dyn ConfigLanguage,
    previous: &str,
    lines: &[&str],
    pointer: &str,
) -> Option<Vec<(usize, usize, usize)>> {
    let flow = Flow::of(language);
    if !language.indent_is_structure() || flow.item_marker().is_none() {
        return None;
    }
    let code = |line: &str| line[..comment_start(language, line)].trim_end().to_string();
    // Where the list starts: after its key's line, which ends at its `:`;
    // the document's own list after the document's start (`---`, a
    // directive) when it has one.
    let (mut at, key_indent) = if pointer.is_empty() {
        let start = lines
            .iter()
            .position(|line| {
                let code = code(line);
                !code.trim().is_empty() && !language.document_marker(code.trim())
            })
            .unwrap_or(lines.len());
        (start, 0)
    } else {
        let key = language.key_span(previous, pointer)?;
        let key_line = key.line.checked_sub(1)?;
        let key_indent = indent_of(lines.get(key_line)?);
        let key_code = code(lines[key_line]);
        if !language
            .config()
            .separators
            .iter()
            .any(|separator| key_code.ends_with(separator))
        {
            return None;
        }
        (key_line + 1, key_indent)
    };
    let mut items: Vec<(usize, usize, usize)> = Vec::new();
    let mut dash: Option<usize> = None;
    while at < lines.len() {
        let line = lines[at];
        if line.trim().is_empty() || comment_only(language, line) {
            at += 1;
            continue;
        }
        let indent = indent_of(line);
        let starts_item = flow.after_item_marker(code(line).trim_start()).is_some();
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
        // Above the first item, only comments at its dash's column are its
        // own (the key's line stops the walk); ones further out are the
        // list's.
        let owned = |line: &str| {
            let indent = indent_of(line);
            if index == 0 {
                indent == column
            } else {
                indent <= column
            }
        };
        while item.0 > 0 && comment_only(language, lines[item.0 - 1]) && owned(lines[item.0 - 1]) {
            item.0 -= 1;
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
    let line_of = |byte: usize| previous[..byte].matches('\n').count();
    lists.into_iter().find_map(|(pointer, before, _)| {
        // The lines from the first item to the end of the last.
        let span: Range<usize> = match block_items(language, previous, &lines, &pointer) {
            Some(items) => items.first()?.1..items.last()?.2 + 1,
            None => {
                let range = |index: usize| {
                    let item = format!("{pointer}/{index}");
                    let span = language
                        .value_span(previous, &item)
                        .or_else(|| inline_location(language, previous, &item, false))?;
                    byte_range(previous, span)
                };
                let first = range(0)?;
                let last = range(before.len().checked_sub(1)?)?;
                line_of(first.start)..line_of(last.end.max(first.start)) + 1
            }
        };
        // A list on one line: a comment after it is the list's, not an
        // item's (`allow: [a, b] # cidrs`).
        if span.len() == 1 {
            return None;
        }
        lines[span]
            .iter()
            .any(|line| comment_start(language, line) < line.len())
            .then_some(pointer)
    })
}
