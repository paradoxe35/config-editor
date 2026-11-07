//! The structural patch: a save adding, removing or renaming entries made
//! line by line on the host's text, so its comments and formatting stay.

use super::super::language::ConfigLanguage;
use super::compare::{comment_start, comparable, trailing_comment};
use super::{empty_containers, kept_inline, read, serialize};
use serde_json::Value;

/// `previous` with a structural edit (keys or items added or removed) applied
/// line by line, keeping its comments and formatting elsewhere: the host
/// serializer renders the old and the new value, their line diff is mapped
/// onto `previous`, and the result is kept only if it reads back as `new`.
///
/// One-line containers the host wrote inline (`allow: [a, b]`) are kept as
/// written: the serializer would spread them over several lines that match
/// nothing in the host (see [`kept_inline`]).
pub(in super::super) fn patch_structure(
    language: &dyn ConfigLanguage,
    previous: &str,
    old: &Value,
    new: &Value,
    schema: Option<&Value>,
) -> Option<String> {
    // An empty host is written whole: there is nothing to keep.
    if previous.trim().is_empty() {
        return None;
    }
    let reads_as_new = |text: &str| read(language, text, schema).ok().as_ref() == Some(new);
    // Empty containers over lines (`block {` then `}`) as the serializer
    // writes them, on one line.
    if let Some(joined) = empty_containers::join(language, previous)
        && let Some(patched) = patch_kept_lines(language, &joined.host, old, new, schema)
    {
        let split = joined.split(&patched);
        if reads_as_new(&split) {
            return Some(split);
        }
    }
    patch_kept_lines(language, previous, old, new, schema)
}

/// [`patch_structure`] with the host's one-line containers kept as written.
fn patch_kept_lines(
    language: &dyn ConfigLanguage,
    previous: &str,
    old: &Value,
    new: &Value,
    schema: Option<&Value>,
) -> Option<String> {
    let reads_as_new = |text: &str| read(language, text, schema).ok().as_ref() == Some(new);
    if let Some(kept) = kept_inline::hide(language, previous, old, new)
        && let Some(patched) = patch_lines(language, &kept.host, &kept.old, &kept.new)
    {
        let restored = kept.restore(&patched);
        if reads_as_new(&restored) {
            return Some(restored);
        }
    }
    patch_lines(language, previous, old, new).filter(|patched| reads_as_new(patched))
}

/// [`patch_structure`]'s line diff, unchecked.
fn patch_lines(
    language: &dyn ConfigLanguage,
    previous: &str,
    old: &Value,
    new: &Value,
) -> Option<String> {
    use super::super::diff::{DiffEdit, myers_edits};
    let before = serialize(language, old, Some(previous)).ok()?;
    let after = serialize(language, new, Some(previous)).ok()?;
    let rendered_old: Vec<&str> = before.lines().collect();
    let rendered_new: Vec<&str> = after.lines().collect();
    let host: Vec<&str> = previous.lines().collect();
    // Where each line of the old rendering sits in the host's text: a
    // forward match that skips the host's comments and blank lines, and
    // compares lines without quotes or trailing comments.
    let normalized_host: Vec<String> = host.iter().map(|line| comparable(language, line)).collect();
    // A host line whose value is an alias (`- *x`) matches the scalar it
    // reads (`- two`).
    let alias_heads: Vec<Option<String>> =
        host.iter().map(|line| alias_head(language, line)).collect();
    let mut place = vec![None; rendered_old.len()];
    let mut next = 0;
    for (index, line) in rendered_old.iter().enumerate() {
        let wanted = comparable(language, line);
        if wanted.is_empty() {
            continue;
        }
        // Look ahead a bounded distance so a missing line cannot make the
        // rest of the document unmatched. A line of only punctuation (`}`,
        // `],`) says nothing about where it is, so it matches only the next
        // line with code, never one further on.
        let punctuation = wanted
            .chars()
            .all(|character| matches!(character, '{' | '}' | '[' | ']' | '(' | ')' | ','));
        let window = if punctuation {
            (next..host.len())
                .find(|candidate| !normalized_host[*candidate].is_empty())
                .map_or(next, |candidate| candidate + 1)
        } else {
            (next + 400).min(host.len())
        };
        if let Some(found) = (next..window).find(|candidate| {
            normalized_host[*candidate] == wanted
                || alias_heads[*candidate]
                    .as_deref()
                    .is_some_and(|head| reads_alias(head, &wanted))
        }) {
            place[index] = Some(found);
            next = found + 1;
        }
    }
    // The old → new edits, applied to the host's lines.
    let mut removed = vec![false; host.len()];
    let mut inserted: Vec<Vec<&str>> = vec![Vec::new(); host.len() + 1];
    let (mut old_index, mut new_index) = (0, 0);
    // The host line a run of deletions started at: lines inserted right
    // after them replace them (a renamed table) and go in their place.
    let mut replacing: Option<usize> = None;
    let mut edits = myers_edits(&rendered_old, &rendered_new)?;
    slide_deletions_up(&mut edits, &rendered_old);
    // Each inserted line indented as the host indents (four spaces, a list
    // at its key's column, `-   item`), from the matched lines above it.
    let mut written_new: Vec<String> = rendered_new.iter().map(|line| line.to_string()).collect();
    {
        let (host_unit, rendered_unit) = (
            super::super::commands::indent_unit(previous),
            super::super::commands::indent_unit(&before),
        );
        let (mut old_at, mut new_at) = (0, 0);
        for edit in &edits {
            match edit {
                DiffEdit::Equal => {
                    old_at += 1;
                    new_at += 1;
                }
                DiffEdit::Delete => old_at += 1,
                DiffEdit::Insert => {
                    written_new[new_at] = in_host_indentation(
                        rendered_new[new_at],
                        &rendered_old[..old_at],
                        &place[..old_at],
                        &host,
                        &host_unit,
                        &rendered_unit,
                    );
                    new_at += 1;
                }
            }
        }
    }
    for edit in edits {
        match edit {
            DiffEdit::Equal => {
                old_index += 1;
                new_index += 1;
                replacing = None;
            }
            DiffEdit::Delete => {
                // A rendered line with no host line (a blank line the
                // serializer writes between tables) has nothing to remove;
                // a real mismatch is caught by the read-back below.
                if let Some(at) = place[old_index] {
                    removed[at] = true;
                    replacing.get_or_insert(at);
                }
                old_index += 1;
            }
            DiffEdit::Insert => {
                // Where the lines it replaces were; else after the host line
                // of the nearest placed rendered line above; at the top,
                // before the first placed line (after a header comment);
                // with nothing placed, at the end.
                let slot = match replacing {
                    Some(at) => at,
                    None => match place[..old_index].iter().rposition(Option::is_some) {
                        // Past the host's blank lines that match rendered
                        // ones in between, so the insert sits after them.
                        Some(placed) => {
                            let mut slot = place[placed].unwrap_or_default() + 1;
                            let blanks = rendered_old[placed + 1..old_index]
                                .iter()
                                .filter(|line| line.trim().is_empty())
                                .count();
                            for _ in 0..blanks {
                                if slot < host.len() && host[slot].trim().is_empty() {
                                    slot += 1;
                                }
                            }
                            slot
                        }
                        None => place.iter().find_map(|at| *at).unwrap_or(host.len()),
                    },
                };
                inserted[slot].push(written_new[new_index].as_str());
                new_index += 1;
            }
        }
    }
    // A removed entry takes the comment lines directly above it (`# beta`
    // above a deleted `beta { … }`); a file's header comment is never taken.
    // Blank lines are tidied so no gap is left: the one above the comments
    // goes, or, under an opener (`clients {`), the one after the entry.
    for index in 0..host.len() {
        // A replaced line keeps the comment above it.
        if !removed[index] || index == 0 || removed[index - 1] || !inserted[index].is_empty() {
            continue;
        }
        let is_comment =
            |line: &str| !line.trim().is_empty() && comparable(language, line).is_empty();
        let mut top = index;
        while top > 0 && is_comment(host[top - 1]) {
            top -= 1;
        }
        // Comments at the very top are the file's header, except the lines
        // right above the entry that name it (`# tcp` over `tcp { … }`); a
        // header of several lines keeps its first one.
        if top == 0 {
            let key: String = host[index]
                .trim_start()
                .trim_start_matches(['<', '[', '"'])
                .chars()
                .take_while(|character| {
                    character.is_alphanumeric() || matches!(character, '_' | '-')
                })
                .collect::<String>()
                .to_lowercase();
            let names_it = |line: &str| {
                !key.is_empty()
                    && line
                        .to_lowercase()
                        .split(|character: char| {
                            !(character.is_alphanumeric() || matches!(character, '_' | '-'))
                        })
                        .any(|word| word == key)
            };
            let mut first = index;
            while first > 0 && names_it(host[first - 1]) {
                first -= 1;
            }
            if first == 0 && index > 1 {
                first = 1;
            }
            for line in removed.iter_mut().take(index).skip(first) {
                *line = true;
            }
            continue;
        }
        for line in removed.iter_mut().take(index).skip(top) {
            *line = true;
        }
        if host[top - 1].trim().is_empty() {
            if top < index {
                removed[top - 1] = true;
            }
        } else {
            let mut end = index;
            while end + 1 < host.len() && removed[end + 1] {
                end += 1;
            }
            if top < index && end + 1 < host.len() && host[end + 1].trim().is_empty() {
                removed[end + 1] = true;
            }
        }
    }
    drop_doubled_blank_lines(language, &host, &mut removed, &inserted);
    follow_sibling_spacing(&host, &mut inserted);
    no_doubled_blank_after(&host, &removed, &mut inserted);
    let mut inserted: Vec<Vec<String>> = inserted
        .into_iter()
        .map(|slot| slot.into_iter().map(str::to_string).collect())
        .collect();
    // A replaced line (a renamed key) keeps its trailing comment: the
    // comment moves to the line inserted in its place at the same indent.
    for index in 0..host.len() {
        if !removed[index] {
            continue;
        }
        let comment = trailing_comment(language, host[index]);
        let indent = |line: &str| line.len() - line.trim_start().len();
        if let Some(first) = inserted[index].first_mut()
            && !comment.is_empty()
            && indent(first) == indent(host[index])
            && trailing_comment(language, first).is_empty()
        {
            first.push(' ');
            first.push_str(comment);
        }
    }
    let mut lines: Vec<&str> = Vec::with_capacity(host.len() + rendered_new.len());
    for index in 0..=host.len() {
        lines.extend(inserted[index].iter().map(String::as_str));
        if index < host.len() && !removed[index] {
            lines.push(host[index]);
        }
    }
    let mut patched = lines.join("\n");
    if previous.ends_with('\n') {
        patched.push('\n');
    }
    Some(patched)
}

/// What comes before the alias a host line ends with (`- ` of `- *x`,
/// `udp:` of `udp: *p`), as the patch compares lines.
fn alias_head(language: &dyn ConfigLanguage, line: &str) -> Option<String> {
    let code = line[..comment_start(language, line)].trim();
    let (head, token) = code.rsplit_once(' ')?;
    (language.reference_len(token) == Some(token.len()) && head.ends_with([':', '-']))
        .then(|| comparable(language, head))
}

/// Whether `wanted` (a rendered line) is `head` then a one-line scalar.
fn reads_alias(head: &str, wanted: &str) -> bool {
    wanted
        .strip_prefix(head)
        .and_then(|rest| rest.strip_prefix(' '))
        .is_some_and(|rest| {
            !rest.is_empty()
                && !rest.contains(": ")
                && !rest.ends_with(':')
                && !rest.starts_with(['{', '['])
        })
}

/// `line` (rendered by the host's serializer) indented as the host would:
/// at the host indent of the nearest matched line above with the same
/// rendered indent (a sibling), else one host unit per rendered unit below
/// the nearest shallower one (its parent); a list item takes the sibling's
/// spacing after its dash.
fn in_host_indentation(
    line: &str,
    rendered_old: &[&str],
    place: &[Option<usize>],
    host: &[&str],
    host_unit: &str,
    rendered_unit: &str,
) -> String {
    let indent = |text: &str| text.len() - text.trim_start_matches([' ', '\t']).len();
    let content = line.trim_start_matches([' ', '\t']);
    if content.is_empty() {
        return line.to_string();
    }
    let wanted = indent(line);
    let placed = || {
        (0..rendered_old.len())
            .rev()
            .filter_map(|index| Some((rendered_old[index], host[place[index]?])))
            .filter(|(rendered, _)| !rendered.trim().is_empty())
    };
    let dash_gap = |text: &str| {
        let content = text.trim_start_matches([' ', '\t']);
        content
            .strip_prefix('-')
            .filter(|rest| rest.starts_with(' '))
            .map(|rest| rest.len() - rest.trim_start_matches(' ').len())
    };
    let (prefix, content) =
        if let Some((_, sibling)) = placed().find(|(rendered, _)| indent(rendered) == wanted) {
            let mut content = content.to_string();
            if let (Some(_), Some(gap)) = (dash_gap(line), dash_gap(sibling)) {
                content = format!(
                    "-{}{}",
                    " ".repeat(gap),
                    content[1..].trim_start_matches(' ')
                );
            }
            (sibling[..indent(sibling)].to_string(), content)
        } else if let Some((rendered, parent)) =
            placed().find(|(rendered, _)| indent(rendered) < wanted)
        {
            let levels = (wanted - indent(rendered)) / rendered_unit.len().max(1);
            let extra = (wanted - indent(rendered)) % rendered_unit.len().max(1);
            (
                format!(
                    "{}{}{}",
                    &parent[..indent(parent)],
                    host_unit.repeat(levels),
                    " ".repeat(extra)
                ),
                content.to_string(),
            )
        } else {
            return line.to_string();
        };
    format!("{prefix}{content}")
}

/// A removed run of lines between two blank lines (or after a blank line at
/// the end) would leave the blank lines doubled: the one above goes.
fn drop_doubled_blank_lines(
    language: &dyn ConfigLanguage,
    host: &[&str],
    removed: &mut [bool],
    inserted: &[Vec<&str>],
) {
    let blank = |index: usize| host[index].trim().is_empty();
    let mut index = 0;
    while index < host.len() {
        if !removed[index] {
            index += 1;
            continue;
        }
        let start = index;
        while index < host.len() && removed[index] {
            index += 1;
        }
        let untouched = (start..=index).all(|slot| inserted[slot].is_empty());
        // A closer (`}`, `]`, `</a>`) after the run ends the body: no blank
        // line belongs before it either.
        let closes = |line: &str| {
            let trimmed = line.trim();
            trimmed.starts_with(['}', ']', ')']) || language.closing_markup_len(trimmed).is_some()
        };
        let after_blank = index == host.len() || blank(index) || closes(host[index]);
        if untouched && start > 0 && !removed[start - 1] && blank(start - 1) && after_blank {
            removed[start - 1] = true;
        }
        // Removed from the top of the file: no blank line left to start it.
        if untouched && start == 0 && index < host.len() && blank(index) {
            removed[index] = true;
        }
    }
}

/// An inserted entry starting with a blank line (as the serializer separates
/// blocks) keeps it only where the host separates siblings that way: the
/// earlier sibling at the slot's indent ending its block is followed by a
/// blank line there.
fn follow_sibling_spacing(host: &[&str], inserted: &mut [Vec<&str>]) {
    let indent = |line: &str| line.len() - line.trim_start().len();
    for slot in 1..inserted.len() {
        let starts_blank = inserted[slot]
            .first()
            .is_some_and(|line| line.trim().is_empty());
        let before = host[slot - 1];
        if !starts_blank || before.trim().is_empty() {
            continue;
        }
        let level = indent(before);
        // The closer of the sibling before `before`'s block, if any.
        let mut earlier = None;
        for index in (0..slot - 1).rev() {
            let line = host[index];
            if line.trim().is_empty() {
                continue;
            }
            if indent(line) < level {
                break;
            }
            if indent(line) == level && line.trim().starts_with(['}', ']']) {
                earlier = Some(index);
                break;
            }
        }
        if let Some(index) = earlier
            && host
                .get(index + 1)
                .is_some_and(|line| !line.trim().is_empty())
        {
            inserted[slot].remove(0);
        }
    }
}

/// An inserted entry ending with a blank line (as the serializer separates
/// a block from what follows) drops it where the host line after it is
/// blank already or closes the body.
fn no_doubled_blank_after(host: &[&str], removed: &[bool], inserted: &mut [Vec<&str>]) {
    for slot in 0..inserted.len() {
        let ends_blank = inserted[slot].len() > 1
            && inserted[slot]
                .last()
                .is_some_and(|line| line.trim().is_empty());
        let next = (slot..host.len()).find(|index| !removed[*index]);
        let blank_after = next.is_none_or(|index| {
            let line = host[index].trim();
            line.is_empty() || line.starts_with(['}', ']', ')'])
        });
        if ends_blank && blank_after && inserted.get(slot + 1).is_none_or(Vec::is_empty) {
            inserted[slot].pop();
        }
    }
}

/// Shifts each run of deleted lines up while the line before it equals its
/// last line, so a repeated block (`[[web.certificates]]` items) is deleted
/// whole from its first line rather than straddling two items. Closing
/// brackets stay where the diff put them: they end the block above.
fn slide_deletions_up(edits: &mut [super::super::diff::DiffEdit], old: &[&str]) {
    use super::super::diff::DiffEdit;
    let mut changed = true;
    while changed {
        changed = false;
        let mut old_index = 0;
        let mut index = 0;
        while index < edits.len() {
            if edits[index] == DiffEdit::Equal {
                let run = edits[index + 1..]
                    .iter()
                    .take_while(|edit| **edit == DiffEdit::Delete)
                    .count();
                // A closer (`}`) ends the block above; sliding past it would
                // delete that block's end and keep the deleted one's.
                let closer = old[old_index]
                    .trim()
                    .chars()
                    .all(|character| matches!(character, '}' | ']' | ')' | ','));
                if run > 0 && !closer && old[old_index] == old[old_index + run] {
                    edits[index] = DiffEdit::Delete;
                    edits[index + run] = DiffEdit::Equal;
                    changed = true;
                }
            }
            if edits[index] != DiffEdit::Insert {
                old_index += 1;
            }
            index += 1;
        }
    }
}
