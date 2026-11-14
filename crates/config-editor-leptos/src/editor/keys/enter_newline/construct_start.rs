//! Where a construct written over lines starts when the caret's line ends
//! it before the caret: a bracket (`2]`), a string (`"""`, a heredoc's
//! `EOT`) or a block comment (`b */`) begun on a line above (QA `E35-2`).

use super::super::super::comment_scope::over_balanced_lines;
use super::super::super::keyboard::utf16_to_byte_index;
use super::super::super::language::{ConfigLanguage, over_whole_strings};

/// The start of the line the constructs that the text from `line_start` to
/// `byte` closes (and those that line closes in turn) begin on; `None`
/// when that text closes none, or one that goes on past the caret's line.
pub(super) fn line_of_start(
    behavior: &dyn ConfigLanguage,
    text: &str,
    line_start: usize,
    byte: usize,
) -> Option<usize> {
    let line_end = text[byte..].find('\n').map_or(text.len(), |at| byte + at);
    let utf16 = |at: usize| text[..at].encode_utf16().count();
    let block_open = behavior.config().block_comment.map(|(open, _)| open);
    let mut from = line_start;
    loop {
        let mut start = from;
        // A block comment the line begins in: its opener.
        if let Some(open) = block_open
            && behavior.lexical_state(text, utf16(from)).in_comment
            && let Some(at) = text[..from].rfind(open)
        {
            start = at;
        }
        // The strings and brackets the text up to the caret cuts through.
        let widened = over_whole_strings(behavior, text, utf16(start), utf16(byte));
        let (first, last) = over_balanced_lines(behavior, text, widened.0, widened.1);
        if utf16_to_byte_index(text, last) > line_end + 1 {
            break;
        }
        let start = start.min(utf16_to_byte_index(text, first));
        let start_line = text[..start].rfind('\n').map_or(0, |at| at + 1);
        if start_line >= from {
            break;
        }
        from = start_line;
    }
    (from < line_start).then_some(from)
}
