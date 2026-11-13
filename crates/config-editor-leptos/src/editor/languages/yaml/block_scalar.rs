//! A new string written in place of a block scalar (`|`, `|-`, `>`, `>2-`),
//! keeping the block: its lines at the host's indentation, the host's
//! indentation indicator (or one where the text's first line starts with a
//! space), the chomping indicator its final line breaks need, and, for
//! folded text, the host's line breaks where the words around them are
//! untouched.

use super::super::super::diff::{DiffEdit, myers_edits};
use super::*;

/// `text` as the block scalar `written` (a header such as `>- # note`, then
/// its indented lines) after `prefix` on its line (its key or dash; empty
/// when unknown); `None` when no block can hold it exactly. Folded text
/// keeps folding, its more-indented lines included; text folding cannot
/// hold is written literally (`|`): the value stays exact.
pub(super) fn block_scalar(language: &YamlBehavior, prefix: &str, written: &str, text: &str) -> Option<String> {
    let (header, body) = written.split_once('\n')?;
    if text.contains(['\r', '\u{feff}']) || text.chars().any(|c| c.is_control() && !matches!(c, '\n' | '\t')) {
        return None;
    }
    // Without the line it is on, a block is read as a top-level key's.
    let prefix = if prefix.trim().is_empty() { "k: " } else { prefix };
    let reads = |candidate: &str| language.parse(&format!("{prefix}{candidate}\n")).ok().as_ref().and_then(lone_string).map(str::to_string);
    let old = reads(written)?;
    // The content's indentation: the first text line's, less the spaces its
    // text starts with (an indentation indicator lets it start with some).
    let first_line = body.split('\n').find(|line| !line.trim().is_empty())?;
    let first_text = old.split('\n').find(|line| !line.trim().is_empty())?;
    let leading = |line: &str| line.len() - line.trim_start_matches(' ').len();
    let indent = leading(first_line).checked_sub(leading(first_text))?;
    if indent == 0 {
        return None;
    }
    let indicator_end = header
        .find(|character: char| !matches!(character, '|' | '>' | '-' | '+' | '0'..='9'))
        .unwrap_or(header.len());
    let indicators = &header[..indicator_end];
    let host_digit = indicators.chars().find(char::is_ascii_digit);
    // `|-2` keeps its order.
    let digit_last = indicators.ends_with(|character: char| character.is_ascii_digit()) && indicators.contains(['-', '+']);
    // What follows the indicators (` # note`) stays on the header line.
    let after = &header[indicator_end..];
    let trailing = text.len() - text.trim_end_matches('\n').len();
    let core = &text[..text.len() - trailing];
    if core.trim().is_empty() {
        return None;
    }
    // The host's keep (`+`) stays where it reads the same as clipping (one
    // final line break): the header as written.
    let chomping = match trailing {
        0 => "-",
        1 if indicators.contains('+') => "+",
        1 => "",
        _ => "+",
    };
    let folded = header
        .starts_with('>')
        .then(|| folded_lines(core, Some(&old), body, indent))
        .flatten()
        .map(|lines| ('>', lines));
    let literal = Some(('|', core.split('\n').map(str::to_string).collect::<Vec<_>>()));
    let pad = " ".repeat(indent);
    for (style, lines) in folded.into_iter().chain(literal) {
        let mut text_lines = String::new();
        for line in lines.iter().map(String::as_str).chain(std::iter::repeat_n("", trailing.saturating_sub(1))) {
            text_lines.push('\n');
            if !line.is_empty() {
                text_lines.push_str(&pad);
                text_lines.push_str(line);
            }
        }
        // A first line starting with a space needs the indentation counted
        // from the key or dash: the host's digit, else the one that reads
        // back.
        let needs_digit = lines.iter().find(|line| !line.is_empty()).is_some_and(|line| line.starts_with(' '));
        let digits: Vec<Option<char>> = match host_digit {
            Some(digit) => vec![Some(digit)],
            None if needs_digit => ('1'..='9').map(Some).collect(),
            None => vec![None],
        };
        for digit in digits {
            let digit = digit.map(String::from).unwrap_or_default();
            let indicators = if digit_last { format!("{style}{chomping}{digit}") } else { format!("{style}{digit}{chomping}") };
            let out = format!("{indicators}{after}{text_lines}");
            if reads(&out).as_deref() == Some(text) {
                return Some(out);
            }
        }
    }
    None
}

/// `range` (a value's text) run through the blank lines after it when it is
/// a block scalar that keeps its final line breaks (`|+`, `>2+`): they are
/// its text, so a new value written in its place brings its own.
pub(super) fn with_kept_blank_lines(source: &str, mut range: std::ops::Range<usize>) -> std::ops::Range<usize> {
    let written = &source[range.clone()];
    let header = written.split('\n').next().unwrap_or_default();
    let header = &header[node_properties_len(header)..];
    let indicators = header.split([' ', '\t']).next().unwrap_or_default();
    if !is_block_scalar(indicators) || !indicators.contains('+') || !written.contains('\n') {
        return range;
    }
    while let Some(rest) = source[range.end..].strip_prefix('\n') {
        let blank = rest.len() - rest.trim_start_matches([' ', '\t']).len();
        if !rest[blank..].starts_with('\n') {
            break;
        }
        range.end += 1 + blank;
    }
    range
}

/// The string a document holding one value reads as (`k: …`, `- …`,
/// `- k: …`).
fn lone_string(value: &Value) -> Option<&str> {
    match value {
        Value::String(text) => Some(text),
        Value::Object(map) if map.len() == 1 => lone_string(map.values().next()?),
        Value::Array(items) if items.len() == 1 => lone_string(&items[0]),
        _ => None,
    }
}

/// Whether a folded line keeps its line breaks (it starts with a space or
/// tab: more indented than the others).
fn spaced(line: &str) -> bool {
    line.starts_with([' ', '\t'])
}

/// `core` as folded block lines: a line break between two plain lines is
/// one blank line, one beside a more-indented line is the line break
/// itself, and a space becomes a line break where the host broke the line
/// at that same space (the words on both sides unchanged). `None` when the
/// text starts with a line break.
fn folded_lines(core: &str, old: Option<&str>, body: &str, indent: usize) -> Option<Vec<String>> {
    if core.starts_with('\n') {
        return None;
    }
    let breaks = kept_breaks(core, old, body, indent);
    let mut lines = Vec::new();
    let mut offset = 0;
    let mut previous: Option<&str> = None;
    let mut empty_run = 0;
    for segment in core.split('\n') {
        if segment.is_empty() {
            empty_run += 1;
            offset += 1;
            continue;
        }
        if let Some(previous) = previous {
            let blanks = if spaced(previous) || spaced(segment) { empty_run } else { empty_run + 1 };
            lines.extend(std::iter::repeat_n(String::new(), blanks));
        }
        let mut start = 0;
        if !spaced(segment) {
            for (at, _) in segment.match_indices(' ') {
                if breaks.contains(&(offset + at)) && at > start {
                    lines.push(segment[start..at].to_string());
                    start = at + 1;
                }
            }
        }
        lines.push(segment[start..].to_string());
        previous = Some(segment);
        empty_run = 0;
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
    // The host's lines, read the way folding joins them: a line break
    // between plain lines is a space (a fold) or the blank lines after it,
    // one beside a more-indented line is kept.
    let mut rebuilt = String::new();
    let mut folds = Vec::new();
    let mut blank_run = 0;
    let mut previous_spaced: Option<bool> = None;
    for line in body.split('\n') {
        if line.trim().is_empty() {
            blank_run += 1;
            continue;
        }
        let Some(content) = line.get(indent..) else {
            return Vec::new();
        };
        match previous_spaced {
            Some(false) if !spaced(content) && blank_run == 0 => {
                folds.push(rebuilt.len());
                rebuilt.push(' ');
            }
            Some(false) if !spaced(content) => rebuilt.push_str(&"\n".repeat(blank_run)),
            Some(_) => rebuilt.push_str(&"\n".repeat(blank_run + 1)),
            None => {}
        }
        rebuilt.push_str(content);
        previous_spaced = Some(spaced(content));
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
