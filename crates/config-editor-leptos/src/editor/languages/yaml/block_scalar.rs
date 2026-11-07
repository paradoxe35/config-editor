//! A new string written in place of a block scalar (`|`, `|-`, `>`, `>-`),
//! keeping the block: its lines at the host's indentation, the chomping
//! indicator its final line breaks need, and, for folded text, the host's
//! line breaks where the words around them are untouched.

use super::super::super::diff::{DiffEdit, myers_edits};
use super::*;

/// `text` as the block scalar `written` (a header such as `>- # note`, then
/// its indented lines); `None` when no block can hold it exactly. Folded
/// text the folded form cannot hold (a line indented more than the others)
/// is written literally (`|`): the value stays exact.
pub(super) fn block_scalar(language: &YamlBehavior, written: &str, text: &str) -> Option<String> {
    let (header, body) = written.split_once('\n')?;
    let indent = body
        .split('\n')
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start_matches(' ').len())
        .min()?;
    if indent == 0 || text.contains(['\r', '\u{feff}']) || text.chars().any(|c| c.is_control() && !matches!(c, '\n' | '\t')) {
        return None;
    }
    let indicator_end = header
        .find(|character: char| !matches!(character, '|' | '>' | '-' | '+' | '0'..='9'))
        .unwrap_or(header.len());
    // What follows the indicators (` # note`) stays on the header line.
    let after = &header[indicator_end..];
    let trailing = text.len() - text.trim_end_matches('\n').len();
    let core = &text[..text.len() - trailing];
    if core.trim().is_empty() {
        return None;
    }
    let chomping = match trailing {
        0 => "-",
        1 => "",
        _ => "+",
    };
    let old = language
        .parse(&format!("k: {written}\n"))
        .ok()
        .and_then(|value| value.get("k").and_then(Value::as_str).map(str::to_string));
    let folded = header
        .starts_with('>')
        .then(|| folded_lines(core, old.as_deref(), body, indent))
        .flatten()
        .map(|lines| (">", lines));
    let literal = || literal_lines(core).map(|lines| ("|", lines));
    for (style, lines) in folded.into_iter().chain(literal()) {
        let pad = " ".repeat(indent);
        let mut out = format!("{style}{chomping}{after}");
        for line in lines.iter().map(String::as_str).chain(std::iter::repeat_n("", trailing.saturating_sub(1))) {
            out.push('\n');
            if !line.is_empty() {
                out.push_str(&pad);
                out.push_str(line);
            }
        }
        let reads = language.parse(&format!("k: {out}\n")).ok();
        if reads.as_ref().and_then(|value| value.get("k")).and_then(Value::as_str) == Some(text) {
            return Some(out);
        }
    }
    None
}

/// `core` (no final line breaks) as literal block lines; `None` when its
/// first line starts with a space, which would need an indentation indicator.
fn literal_lines(core: &str) -> Option<Vec<String>> {
    let first = core.split('\n').find(|line| !line.is_empty())?;
    if first.starts_with([' ', '\t']) {
        return None;
    }
    Some(core.split('\n').map(str::to_string).collect())
}

/// `core` as folded block lines: each line break of the value is one blank
/// line, and a space becomes a line break where the host broke the line at
/// that same space (the words on both sides unchanged). `None` when a line
/// of the value starts with a space or tab (folding would keep its break).
fn folded_lines(core: &str, old: Option<&str>, body: &str, indent: usize) -> Option<Vec<String>> {
    if core.starts_with('\n') || core.split('\n').any(|line| line.starts_with([' ', '\t'])) {
        return None;
    }
    let breaks = kept_breaks(core, old, body, indent);
    let mut lines = Vec::new();
    let mut offset = 0;
    for (index, segment) in core.split('\n').enumerate() {
        if index > 0 {
            lines.push(String::new());
        }
        let mut start = 0;
        for (at, _) in segment.match_indices(' ') {
            if breaks.contains(&(offset + at)) && at > start {
                lines.push(segment[start..at].to_string());
                start = at + 1;
            }
        }
        if !segment.is_empty() {
            lines.push(segment[start..].to_string());
        }
        offset += segment.len() + 1;
    }
    Some(lines)
}

/// The byte offsets in `core` of the spaces to write as line breaks: the
/// spaces the host's folded `body` broke at, mapped through a word diff of
/// the old value and `core`, where both neighbours are plain words.
fn kept_breaks(core: &str, old: Option<&str>, body: &str, indent: usize) -> Vec<usize> {
    let Some(old) = old.map(|old| old.trim_end_matches('\n')) else {
        return Vec::new();
    };
    // The host's lines, read the way folding joins them; a line indented
    // more than the others keeps its breaks, so nothing is mapped then.
    let mut rebuilt = String::new();
    let mut folds = Vec::new();
    let mut blank_run = 0;
    let mut started = false;
    for line in body.split('\n') {
        if line.trim().is_empty() {
            blank_run += 1;
            continue;
        }
        let Some(content) = line.get(indent..) else {
            return Vec::new();
        };
        if content.starts_with([' ', '\t']) {
            return Vec::new();
        }
        if started {
            if blank_run == 0 {
                folds.push(rebuilt.len());
                rebuilt.push(' ');
            } else {
                rebuilt.push_str(&"\n".repeat(blank_run));
            }
        }
        rebuilt.push_str(content);
        started = true;
        blank_run = 0;
    }
    if rebuilt != old {
        return Vec::new();
    }
    let tokens = |text: &str| -> Vec<(usize, String)> {
        let mut out = Vec::new();
        let mut word_start = 0;
        for (at, character) in text.char_indices() {
            if matches!(character, ' ' | '\n') {
                if at > word_start {
                    out.push((word_start, text[word_start..at].to_string()));
                }
                out.push((at, character.to_string()));
                word_start = at + 1;
            }
        }
        if word_start < text.len() {
            out.push((word_start, text[word_start..].to_string()));
        }
        out
    };
    let (before, after) = (tokens(old), tokens(core));
    let before_text: Vec<&str> = before.iter().map(|(_, token)| token.as_str()).collect();
    let after_text: Vec<&str> = after.iter().map(|(_, token)| token.as_str()).collect();
    let Some(edits) = myers_edits(&before_text, &after_text) else {
        return Vec::new();
    };
    let word = |tokens: &[(usize, String)], index: Option<usize>| {
        index
            .and_then(|index| tokens.get(index))
            .is_some_and(|(_, token)| token != " " && token != "\n")
    };
    let (mut old_at, mut new_at) = (0usize, 0usize);
    let mut kept = Vec::new();
    for edit in edits {
        match edit {
            DiffEdit::Equal => {
                if folds.contains(&before[old_at].0)
                    && word(&after, new_at.checked_sub(1))
                    && word(&after, Some(new_at + 1))
                {
                    kept.push(after[new_at].0);
                }
                old_at += 1;
                new_at += 1;
            }
            DiffEdit::Delete => old_at += 1,
            DiffEdit::Insert => new_at += 1,
        }
    }
    kept
}
