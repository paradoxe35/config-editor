//! The lines Ctrl+/ comments, widened so they never cut through something
//! written over several lines: a flow collection (`a: [1,` … `2]`), a call
//! or object over lines, a string whose text starts on its opening line
//! (`s = """multi` … `line"""`), an XML start tag, element or CDATA over
//! lines. Commenting part of one would leave an invalid document or change
//! a value; the whole of it is commented instead.

use super::keyboard::utf16_to_byte_index;
use super::language::ConfigLanguage;

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
pub(crate) fn widen(starts: &[LineState], lowest: &[usize], mut first: usize, mut last: usize) -> (usize, usize) {
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
    let lines: Vec<&str> = source.split('\n').collect();
    let line_start = |line: usize| lines[..line].iter().map(|text| text.len() + 1).sum::<usize>();
    let to_utf16 = |byte: usize| source[..byte].encode_utf16().count();
    let new_start = to_utf16(line_start(first)).min(start);
    let new_end = to_utf16(line_start(last) + lines[last].len()).max(end);
    (new_start, new_end)
}

/// The line selection of `start..end` widened over brackets (the
/// language's and parentheses), quoted strings (`"…"`, `'…'`, `"""…"""`,
/// `'''…'''`) and multi-line string bodies.
pub(crate) fn over_balanced_lines(
    behavior: &(impl ConfigLanguage + ?Sized),
    source: &str,
    start: usize,
    end: usize,
) -> (usize, usize) {
    let lines: Vec<&str> = source.split('\n').collect();
    let mut body = vec![false; lines.len()];
    for range in behavior.string_body_lines(source) {
        let end = range.end.min(lines.len());
        if range.start < end {
            body[range.start..end].fill(true);
        }
    }
    let mut brackets: Vec<(char, char)> = behavior.config().brackets.to_vec();
    if !brackets.contains(&('(', ')')) {
        brackets.push(('(', ')'));
    }
    let mut starts = Vec::with_capacity(lines.len() + 1);
    let mut lowest = Vec::with_capacity(lines.len());
    let mut depth = 0usize;
    // The open quote or block comment: its closing delimiter.
    let mut quote: Option<&'static str> = None;
    let block_comment = behavior.config().block_comment;
    for (index, line) in lines.iter().enumerate() {
        starts.push(LineState { depth, open: quote.is_some() || body[index] });
        let mut low = depth;
        // A body line of a block string is text; one inside an open quote is
        // read for its closing quote.
        if body[index] && quote.is_none() {
            lowest.push(low);
            continue;
        }
        let comment = if quote.is_none() { behavior.comment_start(line) } else { None };
        let mut escaped = false;
        let mut at = 0;
        while at < line.len() {
            let rest = &line[at..];
            let character = rest.chars().next().unwrap_or(' ');
            if let Some(delimiter) = quote {
                if escaped {
                    escaped = false;
                } else if character == '\\' && delimiter.starts_with('"') {
                    escaped = true;
                } else if rest.starts_with(delimiter) {
                    quote = None;
                    at += delimiter.len();
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
                .is_none_or(|before| before.is_whitespace() || "[{(,:=".contains(before));
            if let Some(delimiter) = ["\"\"\"", "'''", "\""].into_iter().find(|delimiter| rest.starts_with(delimiter)) {
                quote = Some(delimiter);
                at += delimiter.len();
                continue;
            }
            if character == '\'' && boundary {
                quote = Some("'");
            } else if brackets.iter().any(|(open, _)| *open == character) {
                depth += 1;
            } else if brackets.iter().any(|(_, close)| *close == character) {
                depth = depth.saturating_sub(1);
                low = low.min(depth);
            }
            at += character.len_utf8();
        }
        lowest.push(low);
    }
    starts.push(LineState { depth, open: quote.is_some() });
    let (first, last) = selected_lines(source, start, end);
    let lines_over = widen(&starts, &lowest, first, last);
    selection_over(source, (start, end), lines_over)
}

/// The first and last line the UTF-16 selection `start..end` covers; a
/// selection ending at a line's start leaves that line out.
pub(crate) fn selected_lines(source: &str, start: usize, end: usize) -> (usize, usize) {
    let start_byte = utf16_to_byte_index(source, start);
    let end_byte = utf16_to_byte_index(source, end);
    let first = source[..start_byte].matches('\n').count();
    let last = source[..end_byte].matches('\n').count();
    let last = if end_byte > start_byte && source[..end_byte].ends_with('\n') { last - 1 } else { last };
    (first, last.max(first))
}
