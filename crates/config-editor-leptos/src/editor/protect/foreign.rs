//! Placeholders of other editors: in a documents view, each document's
//! host gives its own tokens, and only its editor knows them (`tokens`).
//! The view owns which placeholders exist across its documents and gives
//! every editor the list; one of another document's tokens written here
//! stands for that document's secret, never this one's: plain text here,
//! flagged wherever it is written, as a token moved out of its own field
//! is.

use std::collections::HashSet;
use std::ops::Range;
use std::sync::Arc;

use super::super::diagnostics::{Diagnostic, Severity};
use super::texts;
use super::tokens::{Known, SecretMatcher};
use crate::EditorStrings;

/// One editor's real placeholders, as another editor reads them: the
/// document they belong to (its file name), the tokens its host gave, and
/// the pattern finding them.
#[derive(Clone)]
pub(crate) struct DocumentPlaceholders {
    source: String,
    matcher: Arc<SecretMatcher>,
    tokens: Arc<HashSet<String>>,
}

impl PartialEq for DocumentPlaceholders {
    fn eq(&self, other: &Self) -> bool {
        self.source == other.source
            && Arc::ptr_eq(&self.matcher, &other.matcher)
            && self.tokens == other.tokens
    }
}

impl DocumentPlaceholders {
    /// `tokens` (those an editor's host gave), from `source`, found by
    /// `matcher`; `None` when the host gave none.
    pub(crate) fn new(
        source: String,
        matcher: Arc<SecretMatcher>,
        tokens: HashSet<String>,
    ) -> Option<Self> {
        if tokens.is_empty() {
            return None;
        }
        Some(DocumentPlaceholders {
            source,
            matcher,
            tokens: Arc::new(tokens),
        })
    }

    /// The pattern finding its document's placeholders.
    pub(crate) fn matcher(&self) -> &Arc<SecretMatcher> {
        &self.matcher
    }
}

/// The tokens of `others` written in `text` (byte ranges, in order, apart)
/// that this editor's host did not give (`known`), with the document each
/// is from. One pass of each pattern over the text (one pass for every
/// list found by a pattern like `own`'s), whatever the number of tokens.
pub(super) fn foreign_tokens<'o>(
    text: &str,
    own: &SecretMatcher,
    known: &[Known],
    others: &'o [DocumentPlaceholders],
) -> Vec<(Range<usize>, &'o str)> {
    let own_tokens: HashSet<&str> = known.iter().map(|known| known.token.as_str()).collect();
    let mut found: Vec<(Range<usize>, &str)> = Vec::new();
    let mut own_found: Option<Vec<Range<usize>>> = None;
    for other in others {
        let ranges = if own.finds_as(&other.matcher) {
            own_found.get_or_insert_with(|| own.find(text)).clone()
        } else {
            other.matcher.find_in(text)
        };
        for range in ranges {
            let token = text.get(range.clone()).unwrap_or_default();
            if !own_tokens.contains(token) && other.tokens.contains(token) {
                found.push((range, other.source.as_str()));
            }
        }
    }
    found.sort_by_key(|(range, _)| range.start);
    found.dedup_by(|later, earlier| later.0.start < earlier.0.end);
    found
}

/// A warning on each token of `others` written in `text` that this
/// editor's host did not give (`known`): another document's placeholder.
pub(super) fn foreign_placeholders(
    text: &str,
    own: &SecretMatcher,
    known: &[Known],
    others: &[DocumentPlaceholders],
    strings: &EditorStrings,
) -> Vec<Diagnostic> {
    let found = foreign_tokens(text, own, known, others);
    // Their lines and columns, in one pass of the text.
    let mut problems = Vec::new();
    let (mut line, mut line_start, mut byte) = (1, 0, 0);
    for (range, source) in found {
        let passed = text.get(byte..range.start).unwrap_or_default();
        if let Some(last) = passed.rfind('\n') {
            line += passed.matches('\n').count();
            line_start = byte + last + 1;
        }
        byte = range.start;
        let column = text
            .get(line_start..range.start)
            .unwrap_or_default()
            .chars()
            .count()
            + 1;
        problems.push(Diagnostic {
            line: Some(line),
            column: Some(column),
            end_column: Some(column + text.get(range).unwrap_or_default().chars().count()),
            message: texts::foreign_placeholder(strings, source),
            severity: Severity::Warning,
            ..Diagnostic::default()
        });
    }
    problems
}

#[cfg(all(test, feature = "yaml"))]
mod tests {
    use super::*;
    use crate::SecretOptions;
    use crate::editor::Language;

    fn placeholders(source: &str, options: &SecretOptions, host: &str) -> DocumentPlaceholders {
        let matcher = SecretMatcher::new(options);
        let known = matcher.known(Language::YAML.behavior(), host, None);
        let tokens = known.iter().map(|known| known.token.clone()).collect();
        DocumentPlaceholders::new(source.to_string(), Arc::new(matcher), tokens)
            .expect("the host gave a token")
    }

    #[test]
    fn another_documents_placeholder_is_flagged_wherever_it_is_written() {
        let strings = EditorStrings::default();
        let options = SecretOptions::default();
        let tls = placeholders("tls.yaml", &options, "key: «secret 9b1c»\n");
        let own = SecretMatcher::new(&options);
        let host = "password: «secret 3f2a»\n";
        let known = own.known(Language::YAML.behavior(), host, None);
        // Glued into a value, alone in a field, in a comment; this
        // document's own token is not flagged.
        let text = "password: «secret 3f2a»\nhost: tunnel.exam«secret 9b1c»ple.com\n\
                    other: «secret 9b1c»\n# «secret 9b1c»\n";
        let problems =
            foreign_placeholders(text, &own, &known, std::slice::from_ref(&tls), &strings);
        let places: Vec<(Option<usize>, Option<usize>)> = problems
            .iter()
            .map(|problem| (problem.line, problem.column))
            .collect();
        assert_eq!(
            places,
            vec![(Some(2), Some(18)), (Some(3), Some(8)), (Some(4), Some(3))]
        );
        assert!(problems[0].message.contains("tls.yaml"), "{problems:?}");
        // A token both hosts gave is this document's own.
        let shared = own.known(Language::YAML.behavior(), "a: «secret 9b1c»\n", None);
        assert!(foreign_placeholders(text, &own, &shared, &[tls], &strings).is_empty());
    }

    #[test]
    fn another_documents_pattern_finds_its_placeholders() {
        let strings = EditorStrings::default();
        let theirs = SecretOptions::default().with_placeholder("<redacted:[a-z0-9]+>");
        let other = placeholders("db.yaml", &theirs, "key: <redacted:ab12>\n");
        let own = SecretMatcher::new(&SecretOptions::default());
        let problems = foreign_placeholders("a: x<redacted:ab12>\n", &own, &[], &[other], &strings);
        assert_eq!(problems.len(), 1);
        assert_eq!((problems[0].line, problems[0].column), (Some(1), Some(5)));
    }
}
