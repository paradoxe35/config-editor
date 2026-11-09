//! What the aliases of a text add to it, measured as the text is: in bytes
//! of text. An alias (`*name`) stands for a copy of its anchor's node as
//! written (`&name` and what follows it: the rest of its line and the
//! lines below it indented deeper), each alias inside that node counted as
//! its own copy; a merge key's alias (`<<: *name`) as any other.

use std::collections::HashMap;

use super::aliases::in_quotes;
use super::editing::code_before_comment;

/// An anchor whose node is being read: its name, where its node ends
/// (bytes), and the bytes its copies take.
struct Open<'a> {
    name: &'a str,
    end: usize,
    size: usize,
}

/// Where the alias is (bytes) that takes what the aliases of `source` add
/// past `limit` bytes; `None` when they add no more.
pub(super) fn past(source: &str, limit: usize) -> Option<usize> {
    if !source.contains('*') {
        return None;
    }
    let lines: Vec<&str> = source.split_inclusive('\n').collect();
    let mut sizes: HashMap<&str, usize> = HashMap::new();
    let mut open: Vec<Open<'_>> = Vec::new();
    let mut added = 0usize;
    let mut offset = 0;
    for (index, line) in lines.iter().enumerate() {
        let code = code_before_comment(line.trim_end_matches('\n'));
        for (at, sigil, name) in sigils(code) {
            let start = offset + at;
            close_before(&mut open, &mut sizes, start);
            if sigil == '&' {
                let end = node_end(&lines, index, offset);
                open.push(Open { name, end, size: end - start });
                continue;
            }
            // An alias inside its own anchor's node (a cycle the parser
            // refuses) copies nothing measured.
            let copy = sizes.get(name).copied().unwrap_or(0);
            added = added.saturating_add(copy);
            if added > limit {
                return Some(start);
            }
            for anchor in &mut open {
                anchor.size = anchor.size.saturating_add(copy).saturating_sub(1 + name.len());
            }
        }
        offset += line.len();
    }
    None
}

/// Each anchor (`&`) and alias (`*`) of a line's code, with its byte, as
/// `aliases` reads them: after a space, a bracket, a comma or a colon, and
/// outside quotes.
fn sigils(code: &str) -> impl Iterator<Item = (usize, char, &str)> {
    code.char_indices().filter_map(move |(at, sigil)| {
        if !matches!(sigil, '&' | '*') {
            return None;
        }
        let before = code[..at].chars().next_back();
        if !before.is_none_or(|character| character.is_whitespace() || "[{,:".contains(character)) {
            return None;
        }
        let rest = &code[at + 1..];
        let length = rest
            .find(|character: char| !(character.is_alphanumeric() || matches!(character, '_' | '-')))
            .unwrap_or(rest.len());
        let name = &rest[..length];
        (!name.is_empty() && !in_quotes(code, at)).then_some((at, sigil, name))
    })
}

/// The anchors whose nodes end at or before byte `at`, each closed with
/// the size of its copies.
fn close_before<'a>(open: &mut Vec<Open<'a>>, sizes: &mut HashMap<&'a str, usize>, at: usize) {
    let mut index = 0;
    while index < open.len() {
        if open[index].end <= at {
            let closed = open.remove(index);
            sizes.insert(closed.name, closed.size);
        } else {
            index += 1;
        }
    }
}

/// Where the node of an anchor on line `index` (starting at byte `offset`)
/// ends: past its line and the lines below indented deeper than it (blank
/// ones and comments between them included).
fn node_end(lines: &[&str], index: usize, offset: usize) -> usize {
    let indent = |line: &str| line.len() - line.trim_start_matches(' ').len();
    let own = indent(lines[index]);
    let mut end = offset + lines[index].len();
    let mut at = end;
    for line in &lines[index + 1..] {
        at += line.len();
        // Blank lines and comments do not end a node.
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        if indent(line) <= own {
            break;
        }
        end = at;
    }
    end
}
