//! Where else the key a suggestion replaces is written, so renaming it
//! renames them too. The suggestions of a list replace the same few
//! ranges: each range's places are found once (a scan of the whole text),
//! not once per suggestion.

use std::collections::HashMap;

use super::offsets::ByteOffsets;
use super::*;

/// A key written in the text and its other places (UTF-16 ranges).
type Written = (String, Vec<(usize, usize)>);

/// The other places of the keys a list's suggestions replace.
pub(super) struct OtherPlaces<'a> {
    source: &'a str,
    context: &'a CompletionContext,
    behavior: &'a dyn ConfigLanguage,
    /// By replaced UTF-16 range: the key written there and its other
    /// places, or nothing when no key is.
    found: HashMap<(usize, usize), Option<Written>>,
}

impl<'a> OtherPlaces<'a> {
    pub(super) fn new(
        source: &'a str,
        context: &'a CompletionContext,
        behavior: &'a dyn ConfigLanguage,
    ) -> Self {
        Self {
            source,
            context,
            behavior,
            found: HashMap::new(),
        }
    }

    /// Where else the key `option` replaces is written (UTF-16 ranges), per
    /// the language's [`ConfigLanguage::key_renames`]: other dotted keys
    /// and headers through it, other blocks or elements of its name. Empty
    /// unless the replaced text is a whole key written in the document,
    /// other than the option's.
    pub(super) fn of(
        &mut self,
        option: &Completion,
        offsets: &mut ByteOffsets<'_>,
    ) -> Vec<(usize, usize)> {
        let (source, context, behavior) = (self.source, self.context, self.behavior);
        let found = self
            .found
            .entry((option.start, option.end))
            .or_insert_with(|| {
                let start = offsets.byte(option.start);
                let end = offsets.byte(option.end);
                written_places(source, context, behavior, start..end)
            });
        match found {
            Some((old, places)) if *old != option.label => places.clone(),
            _ => Vec::new(),
        }
    }
}

/// The key written in `replaced` (bytes) and its other places.
fn written_places(
    source: &str,
    context: &CompletionContext,
    behavior: &dyn ConfigLanguage,
    replaced: std::ops::Range<usize>,
) -> Option<Written> {
    const STAND_IN: &str = "configeditorrenamed";
    let stop = |character: char| character.is_whitespace() || "\"'.=:[]{}<>/,#".contains(character);
    let end = replaced.end;
    let replaced = source[replaced].trim_start_matches(|character: char| stop(character));
    let word_start = end - replaced.len();
    let word_len = replaced.find(stop).unwrap_or(replaced.len());
    let old = &replaced[..word_len];
    if old.is_empty() {
        return None;
    }
    let mut pointer = String::new();
    for segment in &context.path {
        pointer.push('/');
        match segment {
            SchemaPathSegment::Key(key) => {
                pointer.push_str(&key.replace('~', "~0").replace('/', "~1"))
            }
            SchemaPathSegment::ArrayItem(index) => pointer.push_str(&index.to_string()),
        }
    }
    pointer.push('/');
    pointer.push_str(&old.replace('~', "~0").replace('/', "~1"));
    let none = || Some((old.to_string(), Vec::new()));
    let Some(renamed) = behavior
        .key_renames(source, &pointer, STAND_IN)
        .map(|edits| crate::editor::language::with_edits(source, edits))
    else {
        return none();
    };
    // Each stand-in maps back to the word it replaced.
    let mut places = Vec::new();
    let mut delta: isize = 0;
    for (at, _) in renamed.match_indices(STAND_IN) {
        let Some(place) = at.checked_add_signed(-delta) else {
            return none();
        };
        let length = source
            .get(place..)
            .map_or(0, |rest| rest.find(stop).unwrap_or(rest.len()));
        if source.get(place..place + length) != Some(old) {
            return none();
        }
        delta += STAND_IN.len() as isize - length as isize;
        places.push((place, place + length));
    }
    // Only a key written where the caret is: a new key is not a rename.
    if !places.iter().any(|(place, _)| *place == word_start) {
        return none();
    }
    let utf16 = |byte: usize| source[..byte].encode_utf16().count();
    let places = places
        .into_iter()
        .filter(|(place, _)| *place != word_start)
        .map(|(place, place_end)| (utf16(place), utf16(place_end)))
        .collect();
    Some((old.to_string(), places))
}
