//! The one door for cutting text at typed positions: each cut is checked
//! (an end inside a character or past the text gives `None`), so no cut
//! panics on user text.

use text_size::{TextRange, TextSize};

/// `range` of `text`; `None` when an end is not on a character boundary
/// or is past the text.
pub(crate) fn slice(text: &str, range: TextRange) -> Option<&str> {
    text.get(usize::from(range.start())..usize::from(range.end()))
}

/// `text` up to byte `at`; `None` as [`slice`].
pub(crate) fn before(text: &str, at: TextSize) -> Option<&str> {
    slice(text, TextRange::up_to(at))
}

/// `text` from byte `at`; `None` as [`slice`].
pub(crate) fn after(text: &str, at: TextSize) -> Option<&str> {
    let end = TextSize::try_from(text.len()).ok()?;
    (at <= end)
        .then(|| slice(text, TextRange::new(at, end)))
        .flatten()
}

/// Writes `with` over `range` of `text`; `None` (and `text` as it was)
/// when `range` is no cut of it (see [`slice`]).
pub(crate) fn replace(text: &mut String, range: TextRange, with: &str) -> Option<()> {
    slice(text, range)?;
    text.replace_range(usize::from(range.start())..usize::from(range.end()), with);
    Some(())
}

/// Byte `at` of `text` on a character boundary: the start of the character
/// it lies inside, the text's end past it.
pub(crate) fn char_start(text: &str, at: TextSize) -> TextSize {
    let mut at = usize::from(at.min(TextSize::of(text)));
    // A character is at most four bytes: three steps back at most.
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    super::offset::text_size(at)
}

/// `text` up to byte `at`, or up to the start of the character `at` lies
/// inside; the whole text past its end. For counting what lies before a
/// position (its line, its column, its UTF-16 offset).
pub(crate) fn up_to(text: &str, at: TextSize) -> &str {
    before(text, char_start(text, at)).unwrap_or(text)
}

/// `range` of `text`, each end moved back to a character boundary (see
/// [`char_start`]); empty when the end comes before the start.
pub(crate) fn within(text: &str, range: TextRange) -> &str {
    let end = char_start(text, range.end());
    let start = char_start(text, range.start()).min(end);
    slice(text, TextRange::new(start, end)).unwrap_or_default()
}

/// The start of the character before byte `at` of `text`; `at` at the
/// text's start (or off a boundary).
pub(crate) fn step_back(text: &str, at: TextSize) -> TextSize {
    before(text, at)
        .and_then(|head| head.chars().next_back())
        .map_or(at, |character| at - TextSize::of(character))
}

/// The end of the character after byte `at` of `text`; `at` at the text's
/// end (or off a boundary).
pub(crate) fn step_forward(text: &str, at: TextSize) -> TextSize {
    after(text, at)
        .and_then(|tail| tail.chars().next())
        .map_or(at, |character| at + TextSize::of(character))
}

/// The start of the line holding byte `at` of `text`: just after the line
/// break before it, or the text's start.
pub(crate) fn line_start_of(text: &str, at: TextSize) -> TextSize {
    before(text, at)
        .and_then(|head| head.rfind('\n'))
        .map_or_else(TextSize::default, |newline| {
            super::offset::text_size(newline + 1)
        })
}

/// The end of the line holding byte `at` of `text`: its line break, or the
/// text's end.
pub(crate) fn line_end_of(text: &str, at: TextSize) -> TextSize {
    after(text, at)
        .and_then(|tail| tail.find('\n'))
        .map_or_else(
            || TextSize::of(text),
            |newline| at + super::offset::text_size(newline),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cut_off_a_character_boundary_or_past_the_text_is_none() {
        let text = "aé";
        assert_eq!(slice(text, TextRange::new(0.into(), 1.into())), Some("a"));
        assert_eq!(slice(text, TextRange::new(1.into(), 2.into())), None);
        assert_eq!(slice(text, TextRange::new(1.into(), 9.into())), None);
        assert_eq!(before(text, 3.into()), Some("aé"));
        assert_eq!(after(text, 1.into()), Some("é"));
        assert_eq!(after(text, 4.into()), None);
    }

    #[test]
    fn a_position_inside_a_character_counts_from_its_start() {
        let text = "aé😀";
        assert_eq!(char_start(text, 2.into()), TextSize::from(1));
        assert_eq!(char_start(text, 6.into()), TextSize::from(3));
        assert_eq!(char_start(text, 99.into()), TextSize::of(text));
        assert_eq!(up_to(text, 2.into()), "a");
        assert_eq!(up_to(text, 3.into()), "aé");
        assert_eq!(up_to(text, 99.into()), text);
        assert_eq!(within(text, TextRange::new(2.into(), 5.into())), "é");
        assert_eq!(within(text, TextRange::new(1.into(), 99.into())), "é😀");
    }

    #[test]
    fn a_position_finds_the_ends_of_its_line() {
        let text = "a\nbc\n";
        assert_eq!(line_start_of(text, 3.into()), TextSize::from(2));
        assert_eq!(line_end_of(text, 3.into()), TextSize::from(4));
        assert_eq!(line_start_of(text, 0.into()), TextSize::from(0));
        assert_eq!(line_end_of(text, 5.into()), TextSize::from(5));
    }
}
