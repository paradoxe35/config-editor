//! The lines Ctrl+/ comments, widened so they never cut through something
//! written over several lines: a flow collection (`a: [1,` … `2]`), a call
//! or object over lines, a string whose text starts on its opening line
//! (`s = """multi` … `line"""`), an XML start tag, element or CDATA over
//! lines. Commenting part of one would leave an invalid document or change
//! a value; the whole of it is commented instead.

use super::keyboard::utf16_to_byte_index;
use super::language::ConfigLanguage;
use super::paths::{batch_kept, line_of_byte, line_start, split_line, units_at};
use super::work_count::{Work, note};

/// The state at each line start: the nesting depth there, and whether a
/// string (or another construct the language reads as one) is open.
#[derive(Clone, Copy, Default)]
pub(crate) struct LineState {
    pub(crate) depth: usize,
    pub(crate) open: bool,
}

/// Lines `first..=last` widened until they hold whole constructs: they do
/// not start or end inside one, and they close no more than they open nor
/// leave anything open. `starts` has one entry per line plus one for the
/// end; `lowest[i]` is the lowest depth reached on line `i`.
pub(crate) fn widen(
    starts: &[LineState],
    lowest: &[usize],
    mut first: usize,
    mut last: usize,
) -> (usize, usize) {
    let lines = lowest.len();
    for _ in 0..lines * 2 + 2 {
        let base = starts[first].depth;
        let dips = (first..=last).any(|line| lowest[line] < base);
        if (starts[first].open || dips) && first > 0 {
            first -= 1;
            continue;
        }
        let end = starts[last + 1];
        if (end.open || end.depth > base) && last + 1 < lines {
            last += 1;
            continue;
        }
        break;
    }
    (first, last)
}

/// The UTF-16 selection `start..end` over lines `first..=last` when they are
/// wider than the lines it was on.
pub(crate) fn selection_over(
    source: &str,
    (start, end): (usize, usize),
    (first, last): (usize, usize),
) -> (usize, usize) {
    if (first, last) == selected_lines(source, start, end) {
        return (start, end);
    }
    let last_end = line_start(source, last) + split_line(source, last).map_or(0, str::len);
    let new_start = units_at(source, line_start(source, first)).min(start);
    let new_end = units_at(source, last_end).max(end);
    (new_start, new_end)
}

/// The line selection of `start..end` widened over the language's brackets,
/// its quoted strings (`"…"`, `'…'`, `"""…"""`, `'''…'''`) and multi-line
/// string bodies.
pub(crate) fn over_balanced_lines(
    behavior: &(impl ConfigLanguage + ?Sized),
    source: &str,
    start: usize,
    end: usize,
) -> (usize, usize) {
    let label = behavior.info().label;
    let kept = batch_kept(source, || BalancedLines {
        label,
        states: line_states(behavior, source),
    });
    let fresh;
    let (starts, lowest) = if kept.label == label {
        (&kept.states.0, &kept.states.1)
    } else {
        fresh = line_states(behavior, source);
        (&fresh.0, &fresh.1)
    };
    let (first, last) = selected_lines(source, start, end);
    let lines_over = widen(starts, lowest, first, last);
    selection_over(source, (start, end), lines_over)
}

/// The states at each line start [`over_balanced_lines`] widens with, read
/// once for a text while a batch runs for it (each closed fold of a line
/// command asks, see `keys::line_edit::fold_units`).
struct BalancedLines {
    label: &'static str,
    states: (Vec<LineState>, Vec<usize>),
}

/// Each line start's state over `behavior`'s brackets and strings, and the
/// lowest depth on each line.
fn line_states(
    behavior: &(impl ConfigLanguage + ?Sized),
    source: &str,
) -> (Vec<LineState>, Vec<usize>) {
    note(Work::ConstructScan);
    let lines: Vec<&str> = source.split('\n').collect();
    let mut body = vec![false; lines.len()];
    for range in super::parse_cache::string_bodies(behavior, source)
        .iter()
        .cloned()
    {
        let end = range.end.min(lines.len());
        if range.start < end {
            body[range.start..end].fill(true);
        }
    }
    let config = behavior.config();
    let brackets = config.brackets;
    // Where a value starts: a literal one-character quote opens a string
    // only there (`don't` holds none).
    let value_starts: Vec<char> = config
        .separators
        .iter()
        .filter_map(|separator| separator.chars().next_back())
        .chain(config.item_separator)
        .chain(brackets.iter().map(|(open, _)| *open))
        .collect();
    let literal = |delimiter: &str| config.literal_quotes.contains(&delimiter);
    let mut starts = Vec::with_capacity(lines.len() + 1);
    let mut lowest = Vec::with_capacity(lines.len());
    let mut depth = 0usize;
    // The open quote or block comment: its closing delimiter.
    let mut quote: Option<&'static str> = None;
    let block_comment = behavior.config().block_comment;
    let quotes = behavior.config().quotes;
    for (index, line) in lines.iter().enumerate() {
        starts.push(LineState {
            depth,
            open: quote.is_some() || body[index],
        });
        let mut low = depth;
        // A body line of a block string is text; one inside an open quote is
        // read for its closing quote.
        if body[index] && quote.is_none() {
            lowest.push(low);
            continue;
        }
        let comment = if quote.is_none() {
            behavior.comment_start(line)
        } else {
            None
        };
        let mut escaped = false;
        let mut at = 0;
        while at < line.len() {
            let rest = &line[at..];
            let character = rest.chars().next().unwrap_or(' ');
            if let Some(delimiter) = quote {
                if escaped {
                    escaped = false;
                } else if character == '\\' && !literal(delimiter) {
                    escaped = true;
                } else if rest.starts_with(delimiter) {
                    quote = None;
                    // A long quote closes after quotes of its text (TOML's
                    // `""""` is a `"` then the closer): the whole run.
                    let run = if delimiter.len() > 1 {
                        let mark = delimiter.as_bytes()[0];
                        rest.bytes()
                            .take_while(|byte| *byte == mark)
                            .count()
                            .min(delimiter.len() + 2)
                    } else {
                        delimiter.len()
                    };
                    at += run;
                    continue;
                }
                at += character.len_utf8();
                continue;
            }
            if let Some((open, close)) = block_comment
                && rest.starts_with(open)
            {
                quote = Some(close);
                at += open.len();
                continue;
            }
            if comment.is_some_and(|comment| at >= comment) {
                break;
            }
            let boundary = line[..at]
                .chars()
                .next_back()
                .is_none_or(|before| before.is_whitespace() || value_starts.contains(&before));
            // The language's string delimiters, longest first; a literal
            // one-character quote opens one only where a value starts.
            if let Some(delimiter) = quotes.iter().copied().find(|delimiter| {
                rest.starts_with(delimiter)
                    && (boundary || !literal(delimiter) || delimiter.chars().count() > 1)
            }) {
                quote = Some(delimiter);
                at += delimiter.len();
                continue;
            }
            if brackets.iter().any(|(open, _)| *open == character) {
                depth += 1;
            } else if brackets.iter().any(|(_, close)| *close == character) {
                depth = depth.saturating_sub(1);
                low = low.min(depth);
            }
            at += character.len_utf8();
        }
        lowest.push(low);
    }
    starts.push(LineState {
        depth,
        open: quote.is_some(),
    });
    (starts, lowest)
}

/// The first and last line the UTF-16 selection `start..end` covers; a
/// selection ending at a line's start leaves that line out.
pub(crate) fn selected_lines(source: &str, start: usize, end: usize) -> (usize, usize) {
    let start_byte = utf16_to_byte_index(source, start);
    let end_byte = utf16_to_byte_index(source, end);
    let first = line_of_byte(source, start_byte);
    let last = line_of_byte(source, end_byte);
    let last = if end_byte > start_byte && source[..end_byte].ends_with('\n') {
        last - 1
    } else {
        last
    };
    (first, last.max(first))
}
