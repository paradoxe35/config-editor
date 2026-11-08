//! The lines inside element text written over several lines, read from the
//! tokens alone: asked on every edit, it never builds the paths of the
//! document's keys and values (see `scan_text`), and counts its lines in
//! one pass.

use super::*;

/// Lines wholly inside element text spanning lines: after its first line,
/// through its last unless that line goes on with markup. Text of the root
/// element itself is not a value (as in `scan_text`), and pieces of one
/// element's text around comments or processing instructions are one.
pub(super) fn text_body_lines(source: &str) -> Vec<std::ops::Range<usize>> {
    let tokens = tokenize(source);
    // A token holding the very start is partly typed (as `scan_text`
    // reads it with the caret there): its text is not recorded.
    let partial = tokens.iter().position(|token| {
        contains_caret(source, token, 0) && !matches!(token.kind, Kind::Special | Kind::Comment)
    });
    let mut open: Vec<String> = Vec::new();
    let mut texts: Vec<(usize, usize)> = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        match token.kind {
            Kind::OpenTag { .. } => {
                let name = token_name(source, token);
                if !name.is_empty() && !opens_nothing(source, token) {
                    open.push(name);
                }
            }
            Kind::EndTag => {
                let name = token_name(source, token);
                match open.iter().rposition(|open| *open == name) {
                    Some(position) => open.truncate(position),
                    None => {
                        open.pop();
                    }
                }
            }
            Kind::Text { quoted: false } | Kind::Special
                if open.len() >= 2
                    && partial != Some(index)
                    && (token.kind != Kind::Special || source[token.start..].starts_with("<![CDATA[")) =>
            {
                let raw = &source[token.start..token.end];
                let text = raw.trim();
                if text.is_empty() {
                    continue;
                }
                let start = token.start + (raw.len() - raw.trim_start().len());
                let end = start + text.len();
                match texts.last_mut() {
                    Some((_, last_end)) if only_comments(&source[*last_end..start]) => *last_end = end,
                    _ => texts.push((start, end)),
                }
            }
            _ => {}
        }
    }
    let mut counted = (0, 0);
    let mut line_of = |byte: usize| {
        let (at, line) = counted;
        counted = (byte, line + source.as_bytes()[at..byte].iter().filter(|byte| **byte == b'\n').count());
        counted.1
    };
    texts
        .into_iter()
        .filter(|(start, end)| source[*start..*end].contains('\n'))
        .map(|(start, end)| {
            let first = line_of(start);
            let rest_of_line = source[end..].split('\n').next().unwrap_or("");
            let last = line_of(end) + usize::from(rest_of_line.trim().is_empty());
            first + 1..last
        })
        .filter(|lines| !lines.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::parse::text_body_lines_from_spans;
    use super::text_body_lines;
    use crate::editor::work_count::{Work, counted};

    const DOCUMENTS: &[&str] = &[
        "<config>\n  <desc>line one\n  line two</desc>\n</config>\n",
        "<config>\n  <desc>\n    one\n    two\n  </desc>\n  <port>80</port>\n</config>\n",
        "<config>\n  <a>x<!-- c -->\n  y</a>\n  <b>p<?pi d?>\n  q</b><c>z</c>\n</config>\n",
        "<config>\n  <cdata><![CDATA[one\n  two]]></cdata>\n</config>\n",
        "<config>root text\nover lines<a>1</a>\n</config>\n",
        "<config>\n  <x:a xmlns:x=\"u\">t\n  u</x:a>\n  <e/>\n  <m>open\n  text\n</config>\n",
        "<config>\n  <a>one\n  <b>two\n  three</b>\n  four</a>\n</config>\n",
        "<?xml version=\"1.0\"?>\n<!DOCTYPE c>\n<c>\n  <list>\n    <i>a\n    b</i>\n    <i>c</i>\n  </list>\n</c>\n",
        "<c>\n  <a>1</b>\n  <d>x\n  y</d>\n</c>",
        "text\n<c>\n <a>x\ny</a>\n</c>",
        "<c><a>x\n",
    ];

    /// QA 22: XML found these lines with a full scan of every key's and
    /// value's path on each edit (long tasks at 6000 lines). The tokens
    /// alone find the same lines, with no scan.
    #[test]
    fn text_bodies_are_read_from_the_tokens_alone() {
        let large: String = std::iter::once("<config>\n".to_string())
            .chain((0..2000).map(|index| format!("  <k{index}>v{index}\n  more</k{index}>\n")))
            .chain(std::iter::once("</config>\n".to_string()))
            .collect();
        for document in DOCUMENTS.iter().copied().chain([large.as_str()]) {
            let (found, scans) = counted(Work::SpanScan, || text_body_lines(document));
            assert_eq!(scans, 0, "{document:?}");
            assert_eq!(found, text_body_lines_from_spans(document), "{document:?}");
        }
    }
}
