//! Where one text changed into another, in UTF-16 units (the textarea's
//! offsets), read off the two texts' bytes: every edit asks, so the texts
//! are compared in place instead of copied into UTF-16 first.

/// The UTF-16 length of `text`.
pub(super) fn utf16_len(text: &str) -> usize {
    // One unit per character, two for one outside the BMP (a 4-byte one).
    text.bytes()
        .map(|byte| usize::from(byte & 0xC0 != 0x80) + usize::from(byte >= 0xF0))
        .sum()
}

/// The bytes two texts share at their start, and at their end: `suffix`
/// past the shared start, `full_suffix` regardless of it (the two may
/// overlap). Each ends on a character boundary.
struct Shared {
    prefix: usize,
    suffix: usize,
    full_suffix: usize,
}

fn shared(before: &str, after: &str) -> Shared {
    let (old, new) = (before.as_bytes(), after.as_bytes());
    let mut prefix = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    while !before.is_char_boundary(prefix) {
        prefix -= 1;
    }
    let mut full_suffix = old
        .iter()
        .rev()
        .zip(new.iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    while !before.is_char_boundary(old.len() - full_suffix) {
        full_suffix -= 1;
    }
    let mut suffix = full_suffix.min(old.len() - prefix).min(new.len() - prefix);
    while !before.is_char_boundary(old.len() - suffix) {
        suffix -= 1;
    }
    Shared {
        prefix,
        suffix,
        full_suffix,
    }
}

/// The UTF-16 range of `before` that changed into `after`: the two texts
/// are the same before and after it.
pub(super) fn changed_range(before: &str, after: &str) -> (usize, usize) {
    let shared = shared(before, after);
    let start = utf16_len(&before[..shared.prefix]);
    let end = start + utf16_len(&before[shared.prefix..before.len() - shared.suffix]);
    (start, end)
}

/// The UTF-16 range of `before` the edit to `after` replaced, placed by the
/// caret it left (`caret`): inserted text ends there, which tells apart
/// places the texts alone cannot (`aa|a` + `a`).
pub(super) fn edit_range(before: &str, after: &str, caret: usize) -> (usize, usize) {
    let shared = shared(before, after);
    let start = utf16_len(&before[..shared.prefix]);
    let end = start + utf16_len(&before[shared.prefix..before.len() - shared.suffix]);
    let tail = utf16_len(&before[before.len() - shared.suffix..]);
    let full_tail =
        tail + utf16_len(&before[before.len() - shared.full_suffix..before.len() - shared.suffix]);
    let old_len = end + tail;
    let new_len = start + utf16_len(&after[shared.prefix..after.len() - shared.suffix]) + tail;
    let removed = end - start;
    let Some(inserted) = (new_len + removed).checked_sub(old_len) else {
        return (start, end);
    };
    // Inserted text ending at the caret: the texts agree before it starts
    // and after it ends.
    if let Some(from) = caret.checked_sub(inserted)
        && caret <= new_len
        && from + removed <= old_len
        && from <= start
        && new_len - caret <= full_tail
    {
        return (from, from + removed);
    }
    (start, end)
}

/// Whether `after` is `before` with the UTF-16 range `start..end` replaced.
#[cfg_attr(
    not(feature = "protect"),
    allow(dead_code, reason = "only protection asks")
)]
pub(super) fn replaces(before: &str, after: &str, (start, end): (usize, usize)) -> bool {
    let shared = shared(before, after);
    let prefix = utf16_len(&before[..shared.prefix]);
    let full_tail = utf16_len(&before[before.len() - shared.full_suffix..]);
    let old_len = prefix + utf16_len(&before[shared.prefix..]);
    let new_len = prefix + utf16_len(&after[shared.prefix..]);
    let kept = old_len.checked_sub(end).filter(|_| start <= end);
    kept.is_some_and(|kept| new_len >= start + kept && start <= prefix && kept <= full_tail)
}

/// Where UTF-16 `offset` in `old` lands in `new`, when one region between a
/// common prefix and suffix changed: before it stays, after it shifts,
/// inside it goes to the region's start.
pub(super) fn map_through_change(old: &str, new: &str, offset: usize) -> usize {
    let shared = shared(old, new);
    let start = utf16_len(&old[..shared.prefix]);
    let removed = utf16_len(&old[shared.prefix..old.len() - shared.suffix]);
    if offset <= start {
        offset
    } else if offset >= start + removed {
        // Past the change, by as much as the text grew or shrank.
        let inserted = utf16_len(&new[shared.prefix..new.len() - shared.suffix]);
        offset + inserted - removed
    } else {
        start
    }
}

#[cfg(test)]
#[path = "text_change_tests.rs"]
mod tests;
