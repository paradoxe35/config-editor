//! Where a character as the user sees it (a grapheme cluster: `é` written
//! as `e` and a combining accent, `👨‍👩‍👧` joined by zero-width joiners, a
//! flag of two regional indicators) starts: a cut or a copy never ends
//! inside one. The rules of Unicode's text segmentation (UAX #29) a
//! config file meets, without its tables: combining marks, variation
//! selectors, emoji modifiers and tags extend the character before them,
//! a joiner joins an emoji to the one before it, and regional indicators
//! pair up. Hangul jamo and Indic conjuncts are left out.

/// Whether a character as the user sees it starts at byte `at` of `text`
/// (its start and end are such places; a byte inside a UTF-8 character
/// is not).
pub(crate) fn is_boundary(text: &str, at: usize) -> bool {
    if at == 0 || at >= text.len() {
        return at <= text.len();
    }
    if !text.is_char_boundary(at) {
        return false;
    }
    let (Some(before), Some(after)) = (text[..at].chars().next_back(), text[at..].chars().next())
    else {
        return true;
    };
    if before == '\r' && after == '\n' {
        return false;
    }
    if extends(after) {
        return false;
    }
    if before == JOINER && pictographic(after) {
        return false;
    }
    if regional(after) && regional(before) {
        // Pairs from the first: one after an odd count joins it.
        let count = text[..at]
            .chars()
            .rev()
            .take_while(|&c| regional(c))
            .count();
        return count % 2 == 0;
    }
    true
}

/// The last place at or before byte `at` of `text` where a character as
/// the user sees it starts (see [`is_boundary`]): where a problem's
/// quoted value is cut.
#[cfg(any(feature = "schema-validation", test))]
pub(crate) fn floor(text: &str, at: usize) -> usize {
    let mut at = at.min(text.len());
    while !is_boundary(text, at) {
        at -= 1;
    }
    at
}

const JOINER: char = '\u{200D}';

/// Whether `character` belongs to the character before it: a combining
/// mark, a variation selector, a joiner, an emoji modifier or a tag.
fn extends(character: char) -> bool {
    matches!(
        u32::from(character),
        0x0300..=0x036F
            | 0x0483..=0x0489
            | 0x0591..=0x05BD
            | 0x0610..=0x061A
            | 0x064B..=0x065F
            | 0x0900..=0x0903
            | 0x093A..=0x094F
            | 0x0E31
            | 0x0E34..=0x0E3A
            | 0x0E47..=0x0E4E
            | 0x1AB0..=0x1AFF
            | 0x1DC0..=0x1DFF
            | 0x200C..=0x200D
            | 0x20D0..=0x20FF
            | 0x3099..=0x309A
            | 0xFE00..=0xFE0F
            | 0xFE20..=0xFE2F
            | 0x1F3FB..=0x1F3FF
            | 0xE0020..=0xE007F
            | 0xE0100..=0xE01EF
    )
}

/// Whether `character` is an emoji a joiner joins to the one before it.
fn pictographic(character: char) -> bool {
    matches!(
        u32::from(character),
        0x00A9
            | 0x00AE
            | 0x203C
            | 0x2049
            | 0x2122
            | 0x2139
            | 0x2190..=0x21FF
            | 0x2300..=0x23FF
            | 0x2600..=0x27BF
            | 0x2B00..=0x2BFF
            | 0x3030
            | 0x303D
            | 0x3297
            | 0x3299
            | 0x1F000..=0x1FAFF
    )
}

fn regional(character: char) -> bool {
    matches!(u32::from(character), 0x1F1E6..=0x1F1FF)
}

#[cfg(test)]
mod tests {
    use super::{floor, is_boundary};

    /// The places inside each joined character are none; the ones between
    /// them are.
    #[test]
    fn joined_characters_have_no_place_inside() {
        let clusters = [
            "a",
            "e\u{301}",
            "👨\u{200d}👩\u{200d}👧",
            "🇫🇷",
            "🇩🇪",
            "👍🏽",
            "1\u{fe0f}\u{20e3}",
            "\r\n",
            "日",
            "🏴\u{e0067}\u{e0062}\u{e0065}\u{e006e}\u{e0067}\u{e007f}",
            "x",
        ];
        let text: String = clusters.concat();
        let mut starts = vec![0];
        for cluster in clusters {
            starts.push(starts.last().copied().unwrap_or(0) + cluster.len());
        }
        for at in 0..=text.len() {
            assert_eq!(
                is_boundary(&text, at),
                starts.contains(&at),
                "byte {at} of {text:?}"
            );
            let below = starts.iter().copied().filter(|&start| start <= at).max();
            assert_eq!(Some(floor(&text, at)), below, "floor of {at}");
        }
    }
}
