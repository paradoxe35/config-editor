//! One edit of the view's text as the trail keeps it: what it removed and
//! inserted where, placed by the caret it left (the second of two equal
//! lines deleted is told from the first), and spread over whole lines
//! when it spans lines, so an item whose lines it rewrites (two items
//! swapped, as `Alt+Up` does) is removed and inserted again whole.

use super::super::super::keyboard::utf16_to_byte_index;
use super::super::super::text_change::edit_range;

/// `removed` replaced by `inserted` at byte `at` of the text before.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Splice {
    pub(super) at: usize,
    pub(super) removed: String,
    pub(super) inserted: String,
}

impl Splice {
    /// The edit `before` → `after`, which left the caret at UTF-16
    /// `caret`; `None` when the texts do not tell one.
    pub(crate) fn of(before: &str, after: &str, caret: usize) -> Option<Splice> {
        let (start, end) = edit_range(before, after, caret);
        let (start, end) = (
            utf16_to_byte_index(before, start),
            utf16_to_byte_index(before, end),
        );
        let kept = before.len().checked_sub(end.checked_sub(start)?)?;
        let inserted_end = start + after.len().checked_sub(kept)?;
        if !after.is_char_boundary(inserted_end)
            || before.get(..start) != after.get(..start)
            || before.get(end..) != after.get(inserted_end..)
        {
            return None;
        }
        let splice = Splice {
            at: start,
            removed: before[start..end].to_string(),
            inserted: after[start..inserted_end].to_string(),
        };
        Some(splice.on_lines(before, after))
    }

    /// The same edit over whole lines where it removes or inserts whole
    /// lines one way (a line deleted, pasted, cut), and spread to whole
    /// lines where it spans lines.
    fn on_lines(self, before: &str, after: &str) -> Splice {
        let splice = match (self.removed.is_empty(), self.inserted.is_empty()) {
            (false, true) => self.slid(before, false),
            (true, false) => self.slid(after, true),
            _ => self,
        };
        if !splice.removed.contains('\n') && !splice.inserted.contains('\n') {
            return splice;
        }
        let line_start = before[..splice.at].rfind('\n').map_or(0, |at| at + 1);
        let end = splice.at + splice.removed.len();
        let line_end = if end == 0 || before.as_bytes()[end - 1] == b'\n' {
            end
        } else {
            before[end..]
                .find('\n')
                .map_or(before.len(), |at| end + at + 1)
        };
        let (head, tail) = (&before[line_start..splice.at], &before[end..line_end]);
        Splice {
            at: line_start,
            removed: format!("{head}{}{tail}", splice.removed),
            inserted: format!("{head}{}{tail}", splice.inserted),
        }
    }

    /// A deletion (of `text` before, or an insertion into `text` after)
    /// moved to the nearest place where it is whole lines, if it can be
    /// read there (`a\na\n` less one `a\n` at either place).
    fn slid(self, text: &str, inserting: bool) -> Splice {
        let bytes = text.as_bytes();
        let (at, length) = (self.at, self.removed.len().max(self.inserted.len()));
        let whole = |start: usize| {
            (start == 0 || bytes[start - 1] == b'\n') && bytes[start + length - 1] == b'\n'
        };
        let placed = |start: usize| {
            let chunk = text[start..start + length].to_string();
            if inserting {
                Splice {
                    at: start,
                    removed: String::new(),
                    inserted: chunk,
                }
            } else {
                Splice {
                    at: start,
                    removed: chunk,
                    inserted: String::new(),
                }
            }
        };
        if whole(at) {
            return self;
        }
        // Back to the start of the line the edit starts on first (the
        // caret's: the user's lines), then on.
        for shift in 1..=length.min(at) {
            if bytes[at - shift] != bytes[at + length - shift] {
                break;
            }
            if whole(at - shift) && text.is_char_boundary(at - shift) {
                return placed(at - shift);
            }
        }
        for shift in 1..=length {
            if at + length + shift > bytes.len()
                || bytes[at + length + shift - 1] != bytes[at + shift - 1]
            {
                break;
            }
            if whole(at + shift) && text.is_char_boundary(at + shift) {
                return placed(at + shift);
            }
        }
        self
    }
}
