//! Secret placeholders: tokens such as `«secret 3f2a»` the host writes in
//! place of real secrets. Only the tokens the host gave are real; each is
//! atomic (the caret never lands inside one, and an edit touching one
//! replaces it whole), and its place is followed through every edit.

use std::ops::Range;

use leptos::prelude::Callable;
use leptos::prelude::Callback;

use super::texts;

/// How the editor treats secret placeholders, for the `secrets` prop.
///
/// ```
/// use config_editor_leptos::SecretOptions;
/// let options = SecretOptions::default()
///     .with_placeholder(r"<redacted:[a-z0-9]+>")
///     .with_hint("Leave as is to keep the stored secret")
///     .with_reference_prefixes(["${", "vault:"]);
/// assert_eq!(options.reference_prefixes, vec!["${", "vault:"]);
/// ```
#[derive(Clone)]
#[non_exhaustive]
pub struct SecretOptions {
    /// A regular expression matching placeholder tokens (default
    /// `«secret [0-9a-f]+»`). A token is real only when the host's own
    /// value holds it: typed or pasted look-alikes stay plain text.
    pub placeholder: String,
    /// Decides which matched tokens are real placeholders, after the
    /// pattern (and before the check that the host's value holds them).
    pub is_placeholder: Option<Callback<String, bool>>,
    /// The hover text of a placeholder; unset, the `secretHint` text of
    /// the `strings` setting.
    pub hint: Option<String>,
    /// The reference forms offered in a secret field's value (`${` opens
    /// `${VAR}`, `file:` writes `file:`).
    pub reference_prefixes: Vec<String>,
}

impl Default for SecretOptions {
    fn default() -> Self {
        SecretOptions {
            placeholder: texts::SECRET_PATTERN.to_string(),
            is_placeholder: None,
            hint: None,
            reference_prefixes: texts::REFERENCE_PREFIXES
                .iter()
                .map(|prefix| prefix.to_string())
                .collect(),
        }
    }
}

impl std::fmt::Debug for SecretOptions {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SecretOptions")
            .field("placeholder", &self.placeholder)
            .field("is_placeholder", &self.is_placeholder.is_some())
            .field("hint", &self.hint)
            .field("reference_prefixes", &self.reference_prefixes)
            .finish()
    }
}

impl SecretOptions {
    /// The placeholder pattern (a regular expression).
    #[must_use]
    pub fn with_placeholder(mut self, pattern: impl Into<String>) -> Self {
        self.placeholder = pattern.into();
        self
    }

    /// A host check deciding which matched tokens are real placeholders.
    #[must_use]
    pub fn with_matcher(
        mut self,
        is_placeholder: impl Fn(String) -> bool + Send + Sync + 'static,
    ) -> Self {
        self.is_placeholder = Some(Callback::new(is_placeholder));
        self
    }

    /// The hover text of a placeholder.
    #[must_use]
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    /// The reference forms offered in a secret field.
    #[must_use]
    pub fn with_reference_prefixes<T: Into<String>>(
        mut self,
        prefixes: impl IntoIterator<Item = T>,
    ) -> Self {
        self.reference_prefixes = prefixes.into_iter().map(Into::into).collect();
        self
    }
}

/// [`SecretOptions`] made ready to match.
pub(crate) struct SecretMatcher {
    pattern: Option<regex_lite::Regex>,
    is_placeholder: Option<Callback<String, bool>>,
}

impl SecretMatcher {
    /// `None` pattern when it does not compile: no token is ever real then.
    pub(crate) fn new(options: &SecretOptions) -> Self {
        SecretMatcher {
            pattern: regex_lite::Regex::new(&options.placeholder).ok(),
            is_placeholder: options.is_placeholder,
        }
    }

    /// Byte ranges of the tokens the pattern (and the host check) accept in
    /// `text`.
    pub(crate) fn find(&self, text: &str) -> Vec<Range<usize>> {
        let Some(pattern) = &self.pattern else {
            return Vec::new();
        };
        pattern
            .find_iter(text)
            .filter(|found| !found.as_str().is_empty())
            .filter(|found| {
                self.is_placeholder
                    .is_none_or(|check| check.run(found.as_str().to_string()))
            })
            .map(|found| found.range())
            .collect()
    }

    /// The tokens of the host's `text`, each once, with how often it holds
    /// it.
    pub(crate) fn known(&self, text: &str) -> Vec<(String, usize)> {
        let mut known: Vec<(String, usize)> = Vec::new();
        for range in self.find(text) {
            let token = &text[range];
            match known.iter_mut().find(|(seen, _)| seen == token) {
                Some((_, count)) => *count += 1,
                None => known.push((token.to_string(), 1)),
            }
        }
        known
    }
}

/// UTF-16 ranges of the real tokens in `text`: each `known` token where it
/// is written, at most as often as the host wrote it, first ones first.
pub(crate) fn locate(text: &str, known: &[(String, usize)]) -> Vec<Range<usize>> {
    let mut found: Vec<Range<usize>> = Vec::new();
    for (token, count) in known {
        found.extend(
            text.match_indices(token.as_str())
                .take(*count)
                .map(|(byte, token)| {
                    let start = text[..byte].encode_utf16().count();
                    start..start + token.encode_utf16().count()
                }),
        );
    }
    found.sort_by_key(|range| range.start);
    found.dedup_by(|later, earlier| later.start < earlier.end);
    found
}

/// The edit turning `before` into `after` (whose replaced region is
/// `region`, UTF-16 in `before`), widened so it never cuts a token: a
/// token it touches goes whole. `None` when it touches none.
pub(crate) fn widen(
    tokens: &[Range<usize>],
    before: &str,
    after: &str,
    region: Range<usize>,
) -> Option<(String, Range<usize>, usize)> {
    let old: Vec<u16> = before.encode_utf16().collect();
    let new: Vec<u16> = after.encode_utf16().collect();
    let inserted_len = (new.len() + region.len()).checked_sub(old.len())?;
    let inserted = &new[region.start..region.start + inserted_len];
    let mut wide = region.clone();
    loop {
        let grown = tokens.iter().fold(wide.clone(), |wide, token| {
            if cuts(token, &wide) {
                wide.start.min(token.start)..wide.end.max(token.end)
            } else {
                wide
            }
        });
        if grown == wide {
            break;
        }
        wide = grown;
    }
    if wide == region {
        return None;
    }
    let mut units = old[..wide.start].to_vec();
    units.extend_from_slice(inserted);
    units.extend_from_slice(&old[wide.end..]);
    Some((
        String::from_utf16_lossy(&units),
        wide.clone(),
        wide.start + inserted_len,
    ))
}

/// Whether an edit replacing `region` cuts `token`: it removes part of it,
/// or inserts inside it.
fn cuts(token: &Range<usize>, region: &Range<usize>) -> bool {
    if region.is_empty() {
        return token.start < region.start && region.start < token.end;
    }
    let overlaps = region.start < token.end && token.start < region.end;
    let covers = region.start <= token.start && token.end <= region.end;
    overlaps && !covers
}

/// The tokens after an edit replaced `region` (UTF-16 in the old text)
/// with `inserted_len` units: those before stay, those after shift, those
/// it touched are gone.
pub(crate) fn follow(
    tokens: &[Range<usize>],
    region: &Range<usize>,
    inserted_len: usize,
) -> Vec<Range<usize>> {
    tokens
        .iter()
        .filter_map(|token| {
            if token.end <= region.start {
                Some(token.clone())
            } else if token.start >= region.end {
                let shift = |at: usize| at + inserted_len - region.len();
                Some(shift(token.start)..shift(token.end))
            } else {
                None
            }
        })
        .collect()
}

/// UTF-16 `offset` moved out of any token: to the edge it was moving
/// towards (`forward`), or the nearest one.
pub(crate) fn snap(tokens: &[Range<usize>], offset: usize, forward: Option<bool>) -> usize {
    let Some(token) = tokens
        .iter()
        .find(|token| token.start < offset && offset < token.end)
    else {
        return offset;
    };
    match forward {
        Some(true) => token.end,
        Some(false) => token.start,
        None if offset - token.start <= token.end - offset => token.start,
        None => token.end,
    }
}

/// A selection grown to take whole every token it cuts.
pub(crate) fn snap_selection(tokens: &[Range<usize>], start: usize, end: usize) -> (usize, usize) {
    if start == end {
        let at = snap(tokens, start, None);
        return (at, at);
    }
    (
        snap(tokens, start, Some(false)),
        snap(tokens, end, Some(true)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "«secret 3f2a»";

    #[test]
    fn only_host_tokens_are_real_and_at_most_as_often() {
        let matcher = SecretMatcher::new(&SecretOptions::default());
        let host = format!("a: {TOKEN}\nb: «secret zz»\n");
        let known = matcher.known(&host);
        assert_eq!(known, vec![(TOKEN.to_string(), 1)]);
        let shown = format!("{{\"a\": \"{TOKEN}\", \"c\": \"{TOKEN}\"}}");
        let start = shown
            .find(TOKEN)
            .map(|at| shown[..at].encode_utf16().count())
            .unwrap();
        assert_eq!(locate(&shown, &known), vec![start..start + 13]);
        let custom = SecretMatcher::new(
            &SecretOptions::default().with_matcher(|token| token.ends_with("2a»")),
        );
        assert_eq!(custom.known("x: «secret 11»\ny: «secret 2a»").len(), 1);
    }

    #[test]
    fn edits_touching_a_token_take_it_whole() {
        let text = format!("k: {TOKEN} x");
        let token = 3..16;
        let tokens = [token.clone()];
        // Backspace after it, Delete before it.
        let backspace = format!("k: {} x", &TOKEN[..TOKEN.len() - '»'.len_utf8()]);
        assert_eq!(
            widen(&tokens, &text, &backspace, 15..16),
            Some(("k:  x".to_string(), token.clone(), 3))
        );
        let delete = format!("k: {} x", &TOKEN['«'.len_utf8()..]);
        assert_eq!(
            widen(&tokens, &text, &delete, 3..4).map(|(text, ..)| text),
            Some("k:  x".to_string())
        );
        // Typing over a selection that ends inside it.
        let units: Vec<u16> = text.encode_utf16().collect();
        let typed = format!("k:Z{}", String::from_utf16_lossy(&units[6..]));
        assert_eq!(
            widen(&tokens, &text, &typed, 2..6).map(|(text, ..)| text),
            Some("k:Z x".to_string())
        );
        // Typing at its edges, or deleting it whole, cuts nothing.
        assert_eq!(widen(&tokens, &text, &format!("k: Q{TOKEN} x"), 3..3), None);
        assert_eq!(
            widen(&tokens, &text, &format!("k: {TOKEN}Q x"), 16..16),
            None
        );
        assert_eq!(widen(&tokens, &text, "k:  x", 3..16), None);
    }

    #[test]
    fn tokens_follow_edits_and_the_caret_snaps_out() {
        let tokens = vec![3..16, 20..33];
        assert_eq!(follow(&tokens, &(0..0), 2), vec![5..18, 22..35]);
        assert_eq!(follow(&tokens, &(16..16), 1), vec![3..16, 21..34]);
        assert_eq!(follow(&tokens, &(3..3), 1), vec![4..17, 21..34]);
        assert_eq!(follow(&tokens, &(10..12), 0), vec![18..31]);
        assert_eq!(snap(&tokens, 5, None), 3);
        assert_eq!(snap(&tokens, 14, None), 16);
        assert_eq!(snap(&tokens, 5, Some(true)), 16);
        assert_eq!(snap(&tokens, 3, Some(true)), 3);
        assert_eq!(snap_selection(&tokens, 5, 25), (3, 33));
    }
}
