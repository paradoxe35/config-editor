//! Ctrl+/: the edits that comment or uncomment lines, which a language's
//! [`Typing::toggle_comments`] returns as a [`Toggled`], and the building
//! blocks the built-in languages make them with.
//!
//! [`Typing::toggle_comments`]: super::language::Typing::toggle_comments

use std::ops::Range;

use super::keyboard::utf16_to_byte_index;
use super::language::ConfigLanguage;

/// What Ctrl+/ does to a text: the replacements it makes and the selection
/// it leaves.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Toggled {
    /// Byte ranges of the text before, sorted and apart, each with the text
    /// that replaces it: a marker written or taken, a character escaped.
    /// The editor maps what it keeps by position (problem markers, folds,
    /// read-only ranges) through them, so each one says where text went.
    pub edits: Vec<(Range<usize>, String)>,
    /// The selection after, as UTF-16 offsets of the text after (in either
    /// order).
    pub selection: (usize, usize),
}

impl Toggled {
    /// The text after: `source` with the [`edits`](Self::edits) made; `None`
    /// when they do not fit it (out of order, overlapping, past its end or
    /// inside a character).
    pub fn apply(&self, source: &str) -> Option<String> {
        let mut out = String::with_capacity(source.len());
        let mut at = 0;
        for (range, text) in &self.edits {
            if range.start < at || range.end < range.start {
                return None;
            }
            out.push_str(source.get(at..range.start)?);
            source.get(range.clone())?;
            out.push_str(text);
            at = range.end;
        }
        out.push_str(source.get(at..)?);
        Some(out)
    }

    /// `edits` of `source`, leaving `start..end` (bytes of the text after)
    /// selected.
    pub(crate) fn selecting_bytes(
        source: &str,
        edits: Vec<(Range<usize>, String)>,
        start: usize,
        end: usize,
    ) -> Self {
        let mut toggled = Toggled {
            edits,
            selection: (0, 0),
        };
        let updated = toggled.apply(source).unwrap_or_default();
        let units = |byte: usize| updated[..byte.min(updated.len())].encode_utf16().count();
        toggled.selection = (units(start), units(end));
        toggled
    }
}

/// Where byte `position` of the text before lands after `edits` (sorted
/// byte ranges of it, each with its text): in place outside them, after
/// what one writes over it, and before a pure insert at its spot only with
/// `before_inserts`.
pub(crate) fn mapped_through(
    edits: &[(Range<usize>, String)],
    position: usize,
    before_inserts: bool,
) -> usize {
    let mut delta = 0isize;
    for (range, text) in edits {
        if position < range.start {
            break;
        }
        if position == range.start && range.is_empty() && before_inserts {
            return range.start.saturating_add_signed(delta);
        }
        if position <= range.end {
            return (range.start + text.len()).saturating_add_signed(delta);
        }
        delta += text.len() as isize - range.len() as isize;
    }
    position.saturating_add_signed(delta)
}

/// Ctrl+/ on lines that are each wrapped in a block comment (`/* a = 1 */`):
/// the markers removed. `None` when any selected line is not wrapped, so
/// line comments take over. Offsets are UTF-16; the selection after covers
/// the unwrapped lines.
pub fn unwrap_block_comments(
    source: &str,
    selection_start: usize,
    selection_end: usize,
    (open, close): (&str, &str),
) -> Option<Toggled> {
    let start = utf16_to_byte_index(source, selection_start);
    let end = utf16_to_byte_index(source, selection_end);
    // Inside one block comment over several lines, its markers on lines of
    // their own: they go. (One sharing its lines with code is commented
    // line by line instead: its text would become code.)
    let own_lines = |at: usize, close_end: usize| {
        let line_start = source[..at].rfind('\n').map_or(0, |newline| newline + 1);
        let line_end = source[close_end..]
            .find('\n')
            .map_or(source.len(), |offset| close_end + offset);
        source[line_start..at].trim().is_empty() && source[close_end..line_end].trim().is_empty()
    };
    if let Some(at) = source[..start].rfind(open)
        && !source[at..start].contains(close)
        && let Some(close_at) = source[end..].find(close).map(|offset| end + offset)
        && source[at..close_at].contains('\n')
        && !source[at + open.len()..close_at].contains(open)
        && own_lines(at, close_at + close.len())
    {
        let open_end = at + open.len() + usize::from(source[at + open.len()..].starts_with(' '));
        let close_start =
            close_at - usize::from(source[..close_at].ends_with(' ') && close_at > open_end);
        let edits = vec![
            (at..open_end, String::new()),
            (close_start..close_at + close.len(), String::new()),
        ];
        let removed = open_end - at;
        let map = |byte: usize| byte.clamp(open_end, close_start) - removed;
        return Some(Toggled::selecting_bytes(
            source,
            edits,
            map(start),
            map(end),
        ));
    }
    let first_start = source[..start].rfind('\n').map_or(0, |at| at + 1);
    let last_end = source[end..].find('\n').map_or(source.len(), |at| end + at);
    let block = &source[first_start..last_end];
    let wrapped = |line: &str| {
        let trimmed = line.trim();
        trimmed.len() >= open.len() + close.len()
            && trimmed.starts_with(open)
            && trimmed.ends_with(close)
    };
    let mut content = block
        .lines()
        .filter(|line| !line.trim().is_empty())
        .peekable();
    if content.peek().is_none() || !content.all(wrapped) {
        return None;
    }
    // Each wrapped line loses its markers, the blanks inside them and its
    // trailing blanks. What a position on it needs to be mapped: its start
    // before and after, its indent, and the bytes cut before the text.
    let mut edits = Vec::new();
    let mut lines: Vec<(usize, usize, usize, usize, usize)> = Vec::new();
    let mut offset = first_start;
    let mut shrunk = 0;
    for line in block.split('\n') {
        let new_start = offset - shrunk;
        if line.trim().is_empty() {
            lines.push((offset, new_start, line.len(), 0, line.len()));
        } else {
            let indent = line.len() - line.trim_start().len();
            let body = line.trim();
            let inner = &body[open.len()..body.len() - close.len()];
            let cut = open.len() + (inner.len() - inner.trim_start().len());
            let text = inner.trim();
            let text_start = offset + indent + cut;
            edits.push((offset + indent..text_start, String::new()));
            edits.push((text_start + text.len()..offset + line.len(), String::new()));
            shrunk += line.len() - indent - text.len();
            lines.push((offset, new_start, indent, cut, text.len()));
        }
        offset += line.len() + 1;
    }
    // A position keeps its place in the text; on a marker it moves to the
    // text's nearest end.
    let map = |byte: usize| {
        let (start, new_start, indent, cut, text) = lines
            .iter()
            .rev()
            .find(|(start, ..)| *start <= byte)
            .copied()
            .unwrap_or((first_start, first_start, 0, 0, 0));
        let column = byte - start;
        let mapped = if column <= indent {
            column
        } else {
            (column.saturating_sub(cut)).clamp(indent, indent + text)
        };
        new_start + mapped
    };
    Some(Toggled::selecting_bytes(
        source,
        edits,
        map(start),
        map(end),
    ))
}

/// The UTF-16 selection Ctrl+/ toggles: a selection's lines as they are
/// (what the user chose, broken text included, is theirs to see a problem
/// on); a caret's line widened over the whole constructs it is in (see
/// [`Typing::whole_constructs`]), so one key press never cuts a string or
/// a block in half.
///
/// [`Typing::whole_constructs`]: super::language::Typing::whole_constructs
pub(crate) fn comment_target(
    behavior: &dyn ConfigLanguage,
    source: &str,
    start: usize,
    end: usize,
) -> (usize, usize) {
    if start == end {
        behavior.whole_constructs(source, start, end)
    } else {
        (start, end)
    }
}

/// Ctrl+/ for line-comment languages: the selected lines (UTF-16 offsets)
/// commented at the block's smallest indent with `markers[0]`, or, when every
/// non-blank line already starts with one of `markers`, uncommented.
///
/// ```
/// use leptos_config_editor::language::toggle_line_comments;
/// let source = "a:\n  b: 1\n";
/// let toggled = toggle_line_comments(source, 0, 9, &["#"]).unwrap();
/// assert_eq!(toggled.edits, [(0..0, "# ".to_string()), (3..3, "# ".to_string())]);
/// assert_eq!(toggled.apply(source).unwrap(), "# a:\n#   b: 1\n");
/// ```
pub fn toggle_line_comments(
    source: &str,
    selection_start: usize,
    selection_end: usize,
    markers: &[&str],
) -> Option<Toggled> {
    let insert = *markers.first()?;
    let added = format!("{insert} ");
    let start = utf16_to_byte_index(source, selection_start);
    let end = utf16_to_byte_index(source, selection_end);
    let first_line = source[..start].matches('\n').count();
    let last_line = source[..end]
        .matches('\n')
        .count()
        .saturating_sub(usize::from(start < end && source[..end].ends_with('\n')));
    let lines: Vec<&str> = source.split_inclusive('\n').collect();
    if first_line >= lines.len() {
        // The caret is on the empty last line: start a comment there.
        if start != end || start != source.len() {
            return None;
        }
        let caret = source.len() + added.len();
        let edits = vec![(source.len()..source.len(), added)];
        return Some(Toggled::selecting_bytes(source, edits, caret, caret));
    }
    let last_line = last_line.min(lines.len() - 1);
    let body = |line: &str| {
        line.trim_end_matches('\n')
            .trim_end_matches('\r')
            .to_string()
    };
    let indent_of = |line: &str| line.len() - line.trim_start_matches([' ', '\t']).len();
    let selected: Vec<String> = lines[first_line..=last_line]
        .iter()
        .map(|line| body(line))
        .collect();
    let content_lines = selected.iter().filter(|line| !line.trim().is_empty());
    let marker_of = |content: &str| {
        markers
            .iter()
            .copied()
            .find(|marker| content.starts_with(marker))
    };
    let uncomment = selected
        .iter()
        .filter(|line| !line.trim().is_empty())
        .all(|line| marker_of(&line[indent_of(line)..]).is_some());
    let column = content_lines.map(|line| indent_of(line)).min().unwrap_or(0);
    let first_start: usize = lines[..first_line].iter().map(|line| line.len()).sum();
    // On a blank line, start a comment there, after its indentation.
    if first_line == last_line && selected[0].trim().is_empty() {
        let at = first_start + selected[0].len();
        let caret = at + added.len();
        return Some(Toggled::selecting_bytes(
            source,
            vec![(at..at, added)],
            caret,
            caret,
        ));
    }

    let mut edits: Vec<(Range<usize>, String)> = Vec::new();
    let mut offset = first_start;
    for (line, text) in lines[first_line..=last_line].iter().zip(&selected) {
        if !text.trim().is_empty() {
            if uncomment {
                let indent = indent_of(text);
                let marker = marker_of(&text[indent..]).unwrap_or_default();
                let removed =
                    marker.len() + usize::from(text[indent + marker.len()..].starts_with(' '));
                edits.push((offset + indent..offset + indent + removed, String::new()));
            } else {
                edits.push((offset + column..offset + column, added.clone()));
            }
        }
        offset += line.len();
    }
    let new_start = mapped_through(&edits, start, start != end);
    let new_end = mapped_through(&edits, end, false);
    Some(Toggled::selecting_bytes(source, edits, new_start, new_end))
}
