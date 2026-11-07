//! Ranking suggestions against what was typed.

use super::*;

/// Base scores of the `match_score` tiers, highest first. Within a tier,
/// shorter labels win.
pub(crate) const MATCH_TIER_EXACT_PREFIX: i64 = 1000;
pub(crate) const MATCH_TIER_CASE_INSENSITIVE_PREFIX: i64 = 900;
pub(crate) const MATCH_TIER_INITIALS: i64 = 800;
pub(crate) const MATCH_TIER_SUBSTRING: i64 = 700;
pub(crate) const MATCH_TIER_SUBSEQUENCE: i64 = 500;

/// Substring tier: earlier hits win (`- 2 * index`) and length counts only a
/// quarter, so a short late match does not beat a long early one.
pub(crate) const MATCH_SUBSTRING_INDEX_PENALTY: i64 = 2;
pub(crate) const MATCH_SUBSTRING_LENGTH_DIVISOR: i64 = 4;

/// Scores `label` against `typed`: exact prefix, case-insensitive prefix, word
/// initials (`mc` for `max_connections`), substring, then subsequence.
pub(super) fn match_score(label: &str, typed: &str) -> Option<i64> {
    if typed.is_empty() {
        return Some(0);
    }
    let label_length = label.chars().count() as i64;
    if label.starts_with(typed) {
        return Some(MATCH_TIER_EXACT_PREFIX - label_length);
    }
    let label_lower = label.to_lowercase();
    let typed_lower = typed.to_lowercase();
    if label_lower.starts_with(&typed_lower) {
        return Some(MATCH_TIER_CASE_INSENSITIVE_PREFIX - label_length);
    }
    let initials: String = label
        .split(['_', '-', '.', ' '])
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.chars().next())
        .flat_map(char::to_lowercase)
        .collect();
    if !initials.is_empty() && initials.starts_with(&typed_lower) {
        return Some(MATCH_TIER_INITIALS - label_length);
    }
    if let Some(index) = label_lower.find(&typed_lower) {
        return Some(
            MATCH_TIER_SUBSTRING
                - index as i64 * MATCH_SUBSTRING_INDEX_PENALTY
                - label_length / MATCH_SUBSTRING_LENGTH_DIVISOR,
        );
    }
    let mut remaining = label_lower.chars();
    let mut matched = 0usize;
    for wanted in typed_lower.chars() {
        if remaining.any(|candidate| candidate == wanted) {
            matched += 1;
        } else {
            return None;
        }
    }
    (matched == typed_lower.chars().count()).then_some(MATCH_TIER_SUBSEQUENCE - label_length)
}

/// Character ranges of `label` matched by `typed`, following the
/// `match_score` tiers; empty when nothing matches.
pub(crate) fn match_ranges(label: &str, typed: &str) -> Vec<(usize, usize)> {
    if typed.is_empty() {
        return Vec::new();
    }
    let typed_len = typed.chars().count();
    if label.starts_with(typed) {
        return vec![(0, typed_len)];
    }
    let label_lower = label.to_lowercase();
    let typed_lower = typed.to_lowercase();
    if label_lower.starts_with(&typed_lower) {
        return vec![(0, typed_len)];
    }
    let mut initials: Vec<(usize, char)> = Vec::new();
    // Separators are one ASCII byte, so this tracks each part's start exactly.
    let mut byte_offset = 0;
    for part in label.split(['_', '-', '.', ' ']) {
        if let Some(first) = part.chars().next() {
            let char_offset = label[..byte_offset].chars().count();
            initials.extend(first.to_lowercase().map(|lower| (char_offset, lower)));
        }
        byte_offset += part.len() + 1;
    }
    if !initials.is_empty()
        && initials
            .iter()
            .map(|(_, initial)| *initial)
            .collect::<String>()
            .starts_with(&typed_lower)
    {
        return initials
            .into_iter()
            .take(typed_len)
            .map(|(offset, _)| (offset, offset + 1))
            .collect();
    }
    if let Some(byte_index) = label_lower.find(&typed_lower) {
        let start = label_lower[..byte_index].chars().count();
        return vec![(start, start + typed_len)];
    }
    let mut ranges = Vec::new();
    let mut remaining = label_lower.chars().enumerate();
    for wanted in typed_lower.chars() {
        let mut hit = None;
        for (index, candidate) in remaining.by_ref() {
            if candidate == wanted {
                hit = Some(index);
                break;
            }
        }
        match hit {
            Some(index) => ranges.push((index, index + 1)),
            None => return Vec::new(),
        }
    }
    ranges
}

/// Lifts required keys within a match tier, never above the next tier.
pub(super) const REQUIRED_BONUS: i64 = 40;

/// Best match first, deprecated keys after all others. Ties keep schema
/// order for values (enum order, so a default listed first stays first) and
/// sort alphabetically for properties.
pub(super) fn sorted_completions(mut scored: Vec<(i64, Completion)>) -> Vec<Completion> {
    scored.sort_by(|left, right| {
        left.1
            .deprecated
            .cmp(&right.1.deprecated)
            .then_with(|| right.0.cmp(&left.0))
            .then_with(|| {
                if left.1.is_value && right.1.is_value {
                    std::cmp::Ordering::Equal
                } else {
                    left.1.label.cmp(&right.1.label)
                }
            })
    });
    scored.dedup_by(|left, right| left.1.label == right.1.label);
    scored
        .into_iter()
        .map(|(_, completion)| completion)
        .collect()
}
