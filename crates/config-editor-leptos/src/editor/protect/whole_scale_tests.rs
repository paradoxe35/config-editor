//! One fixed mask the host writes in many fields (`«secret 0b1c0b1c»` for
//! every service's image) costs a key what it costs with one: an edit out
//! of every placeholder's reach looks up no more fields for more of them,
//! and one beside a
//! placeholder judges each field once, never each place the mask is
//! written against each field holding it (XML with 1000 such
//! fields took 1.2 s a key, YAML with 4000 1.8 s), in every language,
//! while the text parses and while it has a syntax error.

#![allow(clippy::string_slice, reason = "the tests cut their own texts")]

use leptos::prelude::*;

use super::super::Language;
use super::super::edit::Edit;
use super::super::secret_options::SecretOptions;
use super::super::state::EditorState;
use super::super::test_state::TestEditor;
use super::super::work_count::{Work, counted};
use super::glued_tests::breaking;
use super::reach_tests::services;

const MASK: &str = "«secret 0b1c0b1c»";

/// `blocks` services whose images are each the one mask, with a syntax
/// error after them when `broken`.
fn masked_services(language: Language, blocks: usize, broken: bool) -> String {
    let mut text = services(language, blocks);
    for index in 0..blocks {
        text = text.replacen(&format!("app-{index}:1.0"), MASK, 1);
    }
    if broken {
        text.push_str(breaking(language));
    }
    text
}

/// Inserts `typed` at byte `byte` of the text on screen (refused or not).
fn insert(state: EditorState, byte: usize, typed: char) {
    let before = state.doc().text.get_untracked();
    let at = before[..byte].encode_utf16().count();
    let mut text = before.clone();
    text.insert(byte, typed);
    let _ = state.commit(Edit::input_units(
        before,
        (at, at),
        text,
        (at + 1, at + 1),
        "insertText".into(),
        0.0,
    ));
}

/// The fields the placeholder check looks at (`Work::TokenScan`) typing a
/// digit in the middle service's port (far from every placeholder), and a
/// letter right after its image's placeholder (refused).
fn work(language: Language, blocks: usize, broken: bool) -> (usize, usize) {
    let text = masked_services(language, blocks, broken);
    let mut editor = TestEditor::new(language, &text);
    editor.secrets = Some(SecretOptions::default().with_placeholder("«secret [0-9a-f]{8}»"));
    let (_owner, state) = editor.mount();
    assert_eq!(
        state.protect().secrets.tokens.with_untracked(Vec::len),
        blocks,
        "{language}: the placeholders are real"
    );
    let middle = format!("svc_{}", blocks / 2);
    let service = text.find(&middle).expect("the middle service");
    let port = service + text[service..].find("port").expect("its port");
    let digits = port
        + text[port..]
            .find(|c: char| c.is_ascii_digit())
            .expect("a number");
    let ((), far) = counted(Work::TokenScan, || insert(state, digits, '7'));
    let shown = state.doc().text.get_untracked();
    let image = service + shown[service..].find(MASK).expect("its image") + MASK.len();
    let ((), beside) = counted(Work::TokenScan, || insert(state, image, 'Z'));
    assert!(
        !state
            .doc()
            .text
            .get_untracked()
            .contains(&format!("{MASK}Z")),
        "{language}: a letter beside the mask is refused"
    );
    (far, beside)
}

#[test]
fn one_mask_in_many_fields_costs_a_key_work_linear_in_them() {
    super::super::parse_cache::hold();
    let mut costly = Vec::new();
    for language in Language::ALL.iter().copied() {
        for broken in [false, true] {
            let state = if broken { " (broken)" } else { "" };
            let (small_far, small_beside) = work(language, 100, broken);
            let (large_far, large_beside) = work(language, 400, broken);
            // Out of every placeholder's reach: no more fields looked at
            // for more of them.
            if large_far > small_far {
                costly.push(format!(
                    "{language}{state}: {small_far} fields looked at for a key far from 100 masks, {large_far} from 400"
                ));
            }
            // Linear is 4 times; each place against each field is 16.
            for (what, small, large) in [
                ("far from", small_far, large_far),
                ("beside", small_beside, large_beside),
            ] {
                if large > 6 * small.max(100) {
                    costly.push(format!(
                        "{language}{state}: {small} fields looked at for a key {what} one of 100 masks, {large} of 400"
                    ));
                }
            }
        }
    }
    super::super::parse_cache::release();
    assert!(costly.is_empty(), "{costly:#?}");
}

/// The holder queries (`Work::HolderRead`, a read of the span of every
/// string holding a placeholder) a letter typed at the end of the middle
/// service's comment makes, and those of the problems of the text after
/// it: `None` in a language with no comment there.
fn holder_reads_typing_in_a_comment(language: Language, blocks: usize) -> Option<(usize, usize)> {
    let text = masked_services(language, blocks, false);
    let comment = format!("service {}", blocks / 2);
    let at = text
        .match_indices(&comment)
        .map(|(at, _)| at + comment.len())
        .find(|end| !text[*end..].starts_with(|c: char| c.is_ascii_digit()))?;
    let mut editor = TestEditor::new(language, &text);
    editor.secrets = Some(SecretOptions::default().with_placeholder("«secret [0-9a-f]{8}»"));
    editor.schema = Some(std::sync::Arc::new(
        super::super::schema::build_schema(r#"{"type": "object"}"#).expect("the schema"),
    ));
    let (_owner, state) = editor.mount();
    let (((), typed), parsed) = counted(Work::ParsedBytes, || {
        counted(Work::HolderRead, || insert(state, at, 'x'))
    });
    let shown = state.doc().text.get_untracked();
    // One parse, of the text after: the one before was the last key's.
    assert!(
        parsed <= shown.len(),
        "{language}: {parsed} bytes parsed for a key in a text of {}",
        shown.len()
    );
    assert!(
        shown.contains(&format!("{comment}x")),
        "{language}: the letter is typed in the comment"
    );
    let ((), checked) = counted(Work::HolderRead, || {
        state.problems().diagnosed_text.set(shown);
        state.problems().diagnostics.with_untracked(Vec::len);
    });
    Some((typed, checked))
}

/// A key typed in a comment, which leaves every field holding its mask as
/// it was, makes no holder query, and nor do the problems of a text where
/// no mask has text beside it: a field lookup for each mask tells that
/// none is glued (typing in a TOML comment beside 600 masks took 93 ms a
/// key at 6,800 lines, 39 without the query), at every size.
#[test]
fn a_key_typed_in_a_comment_reads_no_holder_of_a_placeholder() {
    super::super::parse_cache::hold();
    let mut costly = Vec::new();
    for language in Language::ALL.iter().copied() {
        for blocks in [50, 200, 800] {
            if let Some(reads) = holder_reads_typing_in_a_comment(language, blocks)
                && reads != (0, 0)
            {
                costly.push(format!(
                    "{language}, {blocks} masks: (key, problems) made {reads:?} holder queries"
                ));
            }
        }
    }
    super::super::parse_cache::release();
    assert!(costly.is_empty(), "{costly:#?}");
}
