//! What the tests of the host's guarantees share (`opaque_tests`,
//! `bound_tests`, the save and output tests): one document in every
//! language holding the host's tokens of two unrelated patterns, and a
//! live editor driven through edits.
//!
//! Each pattern has the same token in two fields (`a`, `b`), another token
//! (`c`), two list items (`l`) and, where the language has comments, one in
//! a comment. Nothing in the editor may assume a token's format: the two
//! patterns share nothing.

use leptos::prelude::*;

use super::super::Language;
use super::super::edit::Edit;
use super::super::secret_options::SecretOptions;
use super::super::state::EditorState;
use super::super::test_state::TestEditor;

/// A host's placeholder pattern and tokens it matches.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Tokens {
    pub(crate) pattern: &'static str,
    /// In `a` and `b`.
    pub(crate) same: &'static str,
    /// In `c`.
    pub(crate) other: &'static str,
    /// The items of `l`.
    pub(crate) first: &'static str,
    pub(crate) second: &'static str,
    /// In a comment.
    pub(crate) note: &'static str,
    /// Texts like a token the pattern does not match.
    pub(crate) near_misses: [&'static str; 3],
    /// YAML writes the tokens quoted (a plain scalar cannot start with
    /// `{`).
    pub(crate) quoted_in_yaml: bool,
}

/// Two hosts' patterns, unrelated in every character.
pub(crate) const HOSTS: [Tokens; 2] = [
    Tokens {
        pattern: "«secret [0-9a-f]{8}»",
        same: "«secret 3f2a91c0»",
        other: "«secret 0b1c2d3e»",
        first: "«secret 11aa22bb»",
        second: "«secret 33cc44dd»",
        note: "«secret 5e5e5e5e»",
        near_misses: ["«hidden»", "«secret 3f2a»", "«secret 3F2A91C0»"],
        quoted_in_yaml: false,
    },
    Tokens {
        pattern: r"\{\{vault:[a-z0-9-]+\}\}",
        same: "{{vault:db-pass}}",
        other: "{{vault:api-key}}",
        first: "{{vault:k-1}}",
        second: "{{vault:k-2}}",
        note: "{{vault:old-key}}",
        near_misses: ["{{hidden}}", "{{vault:DB}}", "{{ vault:db }}"],
        quoted_in_yaml: true,
    },
];

impl Tokens {
    pub(crate) fn options(&self) -> SecretOptions {
        SecretOptions::default().with_placeholder(self.pattern)
    }

    /// Every token the host wrote in a field, in document order.
    pub(crate) fn in_fields(&self) -> [&'static str; 5] {
        [self.same, self.same, self.other, self.first, self.second]
    }
}

/// The host's document in `language`, holding `tokens`.
pub(crate) fn sample(language: Language, tokens: &Tokens) -> String {
    let Tokens {
        same,
        other,
        first,
        second,
        note,
        ..
    } = *tokens;
    match language.label() {
        "YAML" if tokens.quoted_in_yaml => format!(
            "# note {note}\nname: a\na: \"{same}\"\nb: \"{same}\"\nc: \"{other}\"\nl:\n  - \"{first}\"\n  - \"{second}\"\n"
        ),
        "YAML" => format!(
            "# note {note}\nname: a\na: {same}\nb: {same}\nc: {other}\nl:\n  - {first}\n  - {second}\n"
        ),
        "JSON" => format!(
            "{{\n  \"name\": \"a\",\n  \"a\": \"{same}\",\n  \"b\": \"{same}\",\n  \"c\": \"{other}\",\n  \"l\": [\n    \"{first}\",\n    \"{second}\"\n  ]\n}}\n"
        ),
        "TOML" | "HCL" => format!(
            "# note {note}\nname = \"a\"\na = \"{same}\"\nb = \"{same}\"\nc = \"{other}\"\nl = [\n  \"{first}\",\n  \"{second}\",\n]\n"
        ),
        "XML" => format!(
            "<!-- note {note} -->\n<name>a</name>\n<a>{same}</a>\n<b>{same}</b>\n<c>{other}</c>\n<l>{first}</l>\n<l>{second}</l>\n"
        ),
        other => panic!("no sample in {other}"),
    }
}

/// Whether `language` writes comments (JSON does not).
pub(crate) fn has_comments(language: Language) -> bool {
    language.label() != "JSON"
}

/// A live editor on `text` in `language`, with the host's `tokens`.
pub(crate) fn mount(language: Language, text: &str, tokens: &Tokens) -> (Owner, EditorState) {
    let mut editor = TestEditor::new(language, text);
    editor.secrets = Some(tokens.options());
    editor.mount()
}

pub(crate) fn utf16(text: &str, byte: usize) -> usize {
    text[..byte].encode_utf16().count()
}

/// Replaces `range` (bytes) of the shown text with `with`, as typing or a
/// paste would, then lets the typing pause check the placeholders.
pub(crate) fn replace(state: EditorState, range: std::ops::Range<usize>, with: &str) {
    let before = state.doc().text.get_untracked();
    let after = format!("{}{with}{}", &before[..range.start], &before[range.end..]);
    let (start, end) = (utf16(&before, range.start), utf16(&before, range.end));
    let caret = start + with.encode_utf16().count();
    state.commit(Edit::new(before, (start, end), after, caret));
    state.recheck_tokens();
}

/// The real placeholders on screen, each with the byte it starts at.
pub(crate) fn real(state: EditorState) -> Vec<(usize, String)> {
    let text = state.doc().text.get_untracked();
    let units: Vec<u16> = text.encode_utf16().collect();
    state.protect().secrets.tokens.with_untracked(|tokens| {
        tokens
            .iter()
            .map(|token| {
                let start = String::from_utf16_lossy(&units[..token.start]).len();
                (start, String::from_utf16_lossy(&units[token.clone()]))
            })
            .collect()
    })
}

/// The real placeholders' texts, in order.
pub(crate) fn real_texts(state: EditorState) -> Vec<String> {
    real(state).into_iter().map(|(_, token)| token).collect()
}

/// The placeholder problems of the text on screen, as `line:column
/// message`, with `schema` (or none).
pub(crate) fn problems(
    state: EditorState,
    language: Language,
    schema: Option<&serde_json::Value>,
) -> Vec<String> {
    let text = state.doc().text.get_untracked();
    untrack(|| {
        state.protect().placeholder_problems(
            language.behavior(),
            &text,
            schema,
            crate::strings::default_strings(),
            true,
        )
    })
    .into_iter()
    .map(|problem| {
        format!(
            "{}:{}-{} {}",
            problem.line.unwrap_or(0),
            problem.column.unwrap_or(0),
            problem.end_column.unwrap_or(0),
            problem.message
        )
    })
    .collect()
}

/// The byte range of the `nth` (zero-based) `token` in `text`.
pub(crate) fn nth(text: &str, token: &str, nth: usize) -> std::ops::Range<usize> {
    let start = text
        .match_indices(token)
        .nth(nth)
        .unwrap_or_else(|| panic!("{token} #{nth} in {text}"))
        .0;
    start..start + token.len()
}

/// The one-based line and column of byte `at`, and the column just past
/// `token` there.
pub(crate) fn place(text: &str, at: usize, token: &str) -> String {
    let line = text[..at].matches('\n').count() + 1;
    let start = text[..at].rfind('\n').map_or(0, |at| at + 1);
    let column = text[start..at].chars().count() + 1;
    format!("{line}:{column}-{}", column + token.chars().count())
}
