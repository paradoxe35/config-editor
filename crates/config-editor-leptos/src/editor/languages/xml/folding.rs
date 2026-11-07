//! Fold ranges: elements, comments, CDATA sections and start tags written
//! over several lines.

use super::super::super::language::{FoldRange, merge_fold_ranges};

pub(super) fn xml_fold_ranges(source: &str) -> Vec<FoldRange> {
    let bytes = source.as_bytes();
    // Line of each byte, by the line starts before it.
    let starts: Vec<usize> = std::iter::once(0)
        .chain(source.match_indices('\n').map(|(at, _)| at + 1))
        .collect();
    let line_of = |at: usize| starts.partition_point(|start| *start <= at) - 1;
    // Whether only blanks precede `at` on its line.
    let first_on_line = |at: usize| source[starts[line_of(at)]..at].trim().is_empty();
    // A fold from `open` to the line of `close`; a closer first on its line
    // stays visible.
    let span = |open: usize, close: usize, keep_closer: bool| {
        let (start, end) = (line_of(open), line_of(close));
        let end = if keep_closer && first_on_line(close) {
            end.saturating_sub(1)
        } else {
            end
        };
        FoldRange::new(start, end)
    };
    let mut ranges = Vec::new();
    // Open elements: (name, offset of `<`).
    let mut open: Vec<(&str, usize)> = Vec::new();
    let mut at = 0;
    while let Some(found) = source[at..].find('<') {
        let tag = at + found;
        let rest = &source[tag..];
        let delimited = |opener: &str, closer: &str| {
            rest.starts_with(opener)
                .then(|| rest.find(closer).map(|end| tag + end + closer.len()))
        };
        if let Some(end) = delimited("<!--", "-->").or_else(|| delimited("<![CDATA[", "]]>")) {
            let Some(end) = end else {
                break;
            };
            ranges.push(span(tag, end - 1, false));
            at = end;
            continue;
        }
        if rest.starts_with("<?") || rest.starts_with("<!") {
            at = rest.find('>').map_or(source.len(), |end| tag + end + 1);
            continue;
        }
        // A `<` that starts no complete tag is text.
        let Some(end) = tag_end(bytes, tag) else {
            at = tag + 1;
            continue;
        };
        let closing = rest.starts_with("</");
        let name_start = tag + if closing { 2 } else { 1 };
        let name_len = source[name_start..end]
            .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
            .unwrap_or(end - name_start);
        let name = &source[name_start..name_start + name_len];
        if closing {
            if let Some(depth) = open.iter().rposition(|(open_name, _)| *open_name == name) {
                let (_, opened) = open[depth];
                open.truncate(depth);
                ranges.push(span(opened, tag, true));
            }
        } else if bytes[end - 2] == b'/' {
            // A self-closing tag over several lines folds its attributes.
            ranges.push(span(tag, end - 2, true));
        } else if !name.is_empty() {
            open.push((name, tag));
        }
        at = end;
    }
    merge_fold_ranges(ranges, starts.len())
}

/// The offset just past the `>` ending the tag at `tag`, skipping quoted
/// attribute values.
fn tag_end(bytes: &[u8], tag: usize) -> Option<usize> {
    let mut quote = None;
    for (index, byte) in bytes.iter().enumerate().skip(tag + 1) {
        match (quote, byte) {
            (Some(open), _) if *byte == open => quote = None,
            (Some(_), _) => {}
            (None, b'"' | b'\'') => quote = Some(*byte),
            (None, b'>') => return Some(index + 1),
            (None, b'<') => return None,
            _ => {}
        }
    }
    None
}
