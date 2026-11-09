//! What Enter writes at a caret, before the editor applies it: the
//! language's [`newline`](ConfigLanguage::newline), except where the line
//! break is text (inside a comment or a multi-line string's body).

use super::super::keyboard::utf16_to_byte_index;
use super::super::language::{ConfigLanguage, Newline};

/// What Enter writes at `cursor` (UTF-16) in `text`; `in_body` says whether
/// a line (by index) is inside a multi-line string's body.
pub(in crate::editor) fn newline_at(
    behavior: &dyn ConfigLanguage,
    text: &str,
    cursor: usize,
    in_body: &dyn Fn(usize) -> bool,
) -> Newline {
    let byte = utf16_to_byte_index(text, cursor);
    let line_start = text[..byte].rfind('\n').map_or(0, |at| at + 1);
    let line_end = text[byte..].find('\n').map_or(text.len(), |at| byte + at);
    let line = &text[line_start..line_end];
    let index = text[..line_start].matches('\n').count();
    // In a string's body the line break is text: the line's indentation
    // and nothing more, and on a blank line the body's (a column-0 line
    // would end a YAML block scalar).
    if in_body(index) {
        let indent = if line.trim().is_empty() {
            body_indent(text, index, in_body).unwrap_or_else(|| indent_of(line))
        } else {
            indent_of(line)
        };
        return Newline::new(format!("\n{indent}"));
    }
    if behavior.lexical_state(text, cursor).in_comment
        && !after_trailing_comment(behavior, line, &text[byte..line_end])
    {
        return Newline::new(format!("\n{}", indent_of(line)));
    }
    behavior.newline(text, cursor)
}

/// Whether the caret ends `line` (`rest` is what follows it there) after a
/// comment that follows code (`a: # c`, `b { # c`): Enter there goes on
/// from the code, as at the end of a line without the comment.
fn after_trailing_comment(behavior: &dyn ConfigLanguage, line: &str, rest: &str) -> bool {
    rest.trim().is_empty()
        && behavior
            .comment_start(line)
            .is_some_and(|start| !line[..start].trim().is_empty())
}

/// The indentation of the nearest non-blank line of the body around the
/// blank line `index`: above it, else below it.
fn body_indent(text: &str, index: usize, in_body: &dyn Fn(usize) -> bool) -> Option<String> {
    let lines: Vec<&str> = text.split('\n').collect();
    let written = |at: &usize| in_body(*at) && !lines[*at].trim().is_empty();
    let above = (0..index).rev().take_while(|at| in_body(*at)).find(written);
    let below = (index + 1..lines.len())
        .take_while(|at| in_body(*at))
        .find(written);
    above.or(below).map(|at| indent_of(lines[at]))
}

fn indent_of(line: &str) -> String {
    line.chars()
        .take_while(|character| matches!(character, ' ' | '\t'))
        .collect()
}
