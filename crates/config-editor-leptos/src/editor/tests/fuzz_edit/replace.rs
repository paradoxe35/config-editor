//! Replace all with random patterns (plain text, regular expressions with
//! groups, match case, whole word) equals a model computed on the text:
//! the same new text and the same counts, matches in read-only text left
//! as they are and counted, and the locked value unchanged.

use crate::editor::language::pointer_key;
use leptos::prelude::*;
use serde_json::Value;

use super::actions::{QUERIES, REPLACEMENTS};
use super::docs::{CUSTOM, LANGUAGES, document, literal, smaller_documents};
use super::runner::{Case, Failure, Rng, cases, run};
use crate::editor::languages::Language;
use crate::editor::paths::FieldPath;
use crate::editor::pattern_engine::RegexBuilder;
use crate::editor::test_state::TestEditor;

/// More queries than the edits use: other cases, words in words, groups.
const MORE_QUERIES: [&str; 8] = ["PORT", "É", "é", "(a)(b)?", "\\d", "$", "o", "na"];

#[derive(Clone, Debug)]
pub(super) struct ReplaceCase {
    pub(super) language: Language,
    pub(super) text: String,
    pub(super) query: &'static str,
    pub(super) with: &'static str,
    pub(super) bits: u8,
    /// The pointer of a read-only field.
    pub(super) lock: Option<String>,
}

impl Case for ReplaceCase {
    fn code(&self) -> String {
        format!(
            "#[test]\nfn replace_repro() {{\n    super::replace::replay(super::replace::ReplaceCase {{\n        language: {},\n        text: {}.into(),\n        query: {:?},\n        with: {:?},\n        bits: {},\n        lock: {},\n    }});\n}}",
            super::docs::language_code(self.language),
            literal(&self.text),
            self.query,
            self.with,
            self.bits,
            self.lock
                .as_ref()
                .map_or("None".to_string(), |lock| format!("Some({lock:?}.into())"))
        )
    }

    fn shrinks(&self) -> Vec<Self> {
        let mut out = Vec::new();
        if self.lock.is_some() {
            out.push(Self {
                lock: None,
                ..self.clone()
            });
        }
        for text in smaller_documents(self.language, &self.text) {
            out.push(Self {
                text,
                ..self.clone()
            });
        }
        out
    }

    fn size(&self) -> usize {
        self.text.len() * 2 + usize::from(self.lock.is_some())
    }
}

fn scalar_pointers(value: &Value, at: String, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                scalar_pointers(value, format!("{at}/{}", pointer_key(key)), out);
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                scalar_pointers(item, format!("{at}/{index}"), out);
            }
        }
        _ => out.push(at),
    }
}

fn generate(rng: &mut Rng, languages: &[Language]) -> Option<ReplaceCase> {
    let language = *rng.pick(languages);
    let text = document(rng, language);
    let query = if rng.chance(30) {
        *rng.pick(&MORE_QUERIES)
    } else {
        *rng.pick(&QUERIES)
    };
    let lock = rng
        .chance(40)
        .then(|| {
            let mut pointers = Vec::new();
            scalar_pointers(&language.parse(&text).ok()?, String::new(), &mut pointers);
            (!pointers.is_empty()).then(|| rng.pick(&pointers).clone())
        })
        .flatten();
    Some(ReplaceCase {
        language,
        text,
        query,
        with: rng.pick(&REPLACEMENTS),
        bits: rng.below(8) as u8,
        lock,
    })
}

fn is_word(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

fn whole_word(text: &str, start: usize, end: usize) -> bool {
    let found = &text[start..end];
    let joins = |outside: Option<char>, inside: Option<char>| {
        outside.is_some_and(is_word) && inside.is_some_and(is_word)
    };
    !joins(text[..start].chars().next_back(), found.chars().next())
        && !joins(text[end..].chars().next(), found.chars().next_back())
}

fn lower(character: char) -> char {
    let mut lowered = character.to_lowercase();
    match (lowered.next(), lowered.next()) {
        (Some(single), None) => single,
        _ => character,
    }
}

/// The plain query's match starting at byte `at`, if one does.
fn plain_at(text: &str, at: usize, query: &str, case_sensitive: bool) -> Option<usize> {
    let mut rest = text[at..].char_indices();
    let mut end = at;
    for wanted in query.chars() {
        let (offset, character) = rest.next()?;
        let same = if case_sensitive {
            character == wanted
        } else {
            lower(character) == lower(wanted)
        };
        if !same {
            return None;
        }
        end = at + offset + character.len_utf8();
    }
    Some(end)
}

/// The matches (byte ranges) the find widget should find: leftmost, not
/// overlapping, an empty match stepping one character on; with whole word
/// only those neither starting nor ending inside a word.
pub(super) fn model_matches(text: &str, query: &str, bits: u8) -> Vec<(usize, usize)> {
    let (case_sensitive, whole, regex) = (bits & 1 != 0, bits & 2 != 0, bits & 4 != 0);
    if query.is_empty() {
        return Vec::new();
    }
    let pattern = if regex {
        match RegexBuilder::new(query)
            .case_insensitive(!case_sensitive)
            .multi_line(true)
            .build()
        {
            Ok(pattern) => Some(pattern),
            Err(_) => return Vec::new(),
        }
    } else {
        None
    };
    let boundaries: Vec<usize> = text
        .char_indices()
        .map(|(at, _)| at)
        .chain([text.len()])
        .collect();
    let mut out = Vec::new();
    let mut next = 0;
    for &at in &boundaries {
        if at < next {
            continue;
        }
        let end = match &pattern {
            Some(pattern) => pattern
                .find_at(text, at)
                .filter(|found| found.start() == at)
                .map(|found| found.end()),
            None => plain_at(text, at, query, case_sensitive),
        };
        let Some(end) = end else {
            continue;
        };
        if whole && !whole_word(text, at, end) {
            continue;
        }
        out.push((at, end));
        next = if end == at {
            at + text[at..].chars().next().map_or(1, char::len_utf8)
        } else {
            end
        };
    }
    out
}

/// The replacement for a regular expression's match, VS Code's way, for
/// the references the cases use (`$0`, `$&`, `$1`–`$9`, `$$`).
fn expanded(text: &str, (start, end): (usize, usize), query: &str, bits: u8, with: &str) -> String {
    if bits & 4 == 0 {
        return with.to_string();
    }
    let pattern = RegexBuilder::new(query)
        .case_insensitive(bits & 1 == 0)
        .multi_line(true)
        .build()
        .expect("the pattern compiled for the matches");
    let Some(captures) = pattern.captures_at(text, start) else {
        return with.to_string();
    };
    if captures.get(0).map(|whole| (whole.start(), whole.end())) != Some((start, end)) {
        return with.to_string();
    }
    let mut out = String::new();
    let mut characters = with.chars().peekable();
    while let Some(character) = characters.next() {
        if character != '$' {
            out.push(character);
            continue;
        }
        match characters.peek().copied() {
            Some('$') => {
                characters.next();
                out.push('$');
            }
            Some('&' | '0') => {
                characters.next();
                out.push_str(captures.get(0).map_or("", |found| found.as_str()));
            }
            Some(digit @ '1'..='9') if (digit as usize - '0' as usize) < captures.len() => {
                characters.next();
                let group = digit as usize - '0' as usize;
                out.push_str(captures.get(group).map_or("", |found| found.as_str()));
            }
            _ => out.push('$'),
        }
    }
    out
}

pub(super) fn check(case: &ReplaceCase) -> Result<(), Failure> {
    let language = case.language;
    let mut editor = TestEditor::new(language, &case.text);
    if let Some(lock) = &case.lock {
        editor.read_only_paths = vec![FieldPath::from_pointer(lock)];
    }
    let (_owner, state) = editor.mount();
    let before = state.doc().text.get_untracked();
    let protected = state.protected_bytes(&before);
    let matches = model_matches(&before, case.query, case.bits);
    let skips = |(start, end): &(usize, usize)| {
        protected
            .iter()
            .any(|range| range.start < *end && *start < range.end)
    };
    let (mut model, mut copied, mut count, mut skipped) = (String::new(), 0, 0, 0);
    for found in &matches {
        if skips(found) {
            skipped += 1;
            continue;
        }
        model.push_str(&before[copied..found.0]);
        model.push_str(&expanded(&before, *found, case.query, case.bits, case.with));
        copied = found.1;
        count += 1;
    }
    model.push_str(&before[copied..]);
    let model = if count == 0 { before.clone() } else { model };
    state.search().query.set(case.query.to_string());
    state.search().replacement.set(case.with.to_string());
    state.search().options.update(|options| {
        options.case_sensitive = case.bits & 1 != 0;
        options.whole_word = case.bits & 2 != 0;
        options.regex = case.bits & 4 != 0;
    });
    crate::editor::search::replace_all(state);
    let after = state.doc().text.get_untracked();
    let kind = format!(
        "{language}, {}{}",
        [
            "plain",
            "case",
            "word",
            "case+word",
            "regex",
            "regex+case",
            "regex+word",
            "regex+case+word"
        ][case.bits as usize % 8],
        if case.lock.is_some() { ", locked" } else { "" }
    );
    // A Replace all a lock refuses whole is no edit; the model's text is
    // then the editor's only when nothing locked would change.
    if after != model && !(after == before && case.lock.is_some()) {
        return Err(Failure::new(
            format!("Replace all text differs from the model ({kind})"),
            format!(
                "query {:?} → {:?}\nmatches {matches:?}, protected {protected:?}\nmodel:\n{model}\neditor:\n{after}",
                case.query, case.with
            ),
        ));
    }
    let notice = format!("{:?}", state.search().notice.get_untracked());
    let wanted = if count > 0 && after == model {
        format!("Some(Replaced {{ count: {count}, skipped: {skipped} }})")
    } else if !matches.is_empty() && count == 0 {
        "Some(NothingReplaced)".to_string()
    } else {
        notice.clone()
    };
    if notice != wanted {
        return Err(Failure::new(
            format!("Replace all counts differ from the model ({kind})"),
            format!("notice {notice}, model {wanted}"),
        ));
    }
    if let Some(lock) = &case.lock {
        let read = |text: &str| {
            language
                .parse(text)
                .ok()
                .and_then(|value| value.pointer(lock).cloned())
        };
        // The locked text kept, the parse reading it otherwise (a
        // duplicate shadows it, it moved): fine while the editor holds the
        // change, saying so.
        if let Some(locked) = read(&before)
            && language.parse(&after).is_ok()
            && read(&after) != Some(locked.clone())
            && !state.changes_held()
        {
            return Err(Failure::new(
                format!("Replace all changed a read-only value ({kind})"),
                format!("{lock}: {locked} became {:?}\n{after}", read(&after)),
            ));
        }
    }
    Ok(())
}

/// Runs one case, failing with what it found (for a pasted repro).
#[allow(dead_code, reason = "called by pasted repros")]
pub(super) fn replay(case: ReplaceCase) {
    if let Err(failure) = check(&case) {
        panic!("{}\n{}", failure.class, failure.detail);
    }
}

#[test]
fn replace_all_equals_the_model_under_random_patterns() {
    run(
        "replace",
        cases(800),
        &|rng| generate(rng, &LANGUAGES),
        &check,
    );
}

#[test]
fn replace_all_equals_the_model_in_custom_languages() {
    run(
        "replace-custom",
        cases(800),
        &|rng| generate(rng, &CUSTOM),
        &check,
    );
}
