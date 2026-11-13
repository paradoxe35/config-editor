//! `utf16_to_byte_index` passes ASCII a block at a time: the same byte as
//! reading character by character, on random texts of every kind of
//! character and every offset (inside a character outside the BMP, past
//! the end).

use crate::editor::keyboard::utf16_to_byte_index;

/// Character by character (the reference).
fn by_character(source: &str, offset: usize) -> usize {
    let mut unit = 0;
    for (byte, character) in source.char_indices() {
        if unit >= offset {
            return byte;
        }
        unit += character.len_utf16();
    }
    source.len()
}

#[test]
fn places_every_offset_as_reading_by_character_does() {
    const CHARS: [char; 7] = ['a', ' ', '\n', 'é', '€', '😀', 'z'];
    let mut seed: u64 = 0xb10c;
    let mut below = |bound: usize| {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        ((seed >> 33) % bound as u64) as usize
    };
    for _ in 0..400 {
        // Long ASCII runs (whole blocks and parts of one) between others.
        let mut text = String::new();
        for _ in 0..below(8) {
            text.extend(std::iter::repeat_n('x', below(200)));
            text.push(CHARS[below(CHARS.len())]);
        }
        let units = text.encode_utf16().count();
        for offset in 0..=units + 2 {
            assert_eq!(
                utf16_to_byte_index(&text, offset),
                by_character(&text, offset),
                "{text:?} at {offset}"
            );
        }
    }
}

/// A long document is mostly ASCII: placing the caret in the middle of one
/// reads only its other characters one at a time, not every character
/// before the caret.
#[test]
fn an_offset_in_a_long_text_reads_only_its_other_characters_one_at_a_time() {
    use crate::editor::work_count::{Work, counted};
    let text = "  secret: «secret 3f2a» # é\n  name: plain ascii line\n".repeat(20_000);
    let middle = text.encode_utf16().count() / 2;
    let (byte, steps) = counted(Work::CharacterStep, || utf16_to_byte_index(&text, middle));
    assert_eq!(byte, by_character(&text, middle));
    // Three other characters a line pair, half the pairs before the middle.
    assert!(
        steps <= 3 * 10_000 + 3,
        "{steps} characters read one at a time"
    );
}
