//! A placeholder costs a keystroke away from it nothing, in every language:
//! the check that keeps it whole (`whole_guard`) judges an edit by what it
//! makes, yet typing in a value far from it parses no more of the document,
//! and reads no more of its spans, than with no placeholder (the line's
//! path is read once for all the keys typed there); and an edit of the
//! whole document checks them in work linear in the text.

use leptos::prelude::*;

use super::super::Language;
use super::super::keys::lines::{Command, run};
use super::super::secret_options::SecretOptions;
use super::super::test_state::TestEditor;
use super::super::work_count::{Work, counted, counted_all};
use super::glued_tests::breaking;
use super::reach_tests::{services, type_in_middle};
use super::whole_guard::{Placeholders, WholeIndex};

const TOKEN: &str = "«secret 3f2a91c0»";

/// The bytes parsed, the documents read for their spans and the paths
/// looked up typing three keys in the middle service, with the first
/// service's image a placeholder the host gave (`secret`) or plain text.
fn typing_work(language: Language, secret: bool) -> [usize; 3] {
    let blocks = 200;
    let text = services(language, blocks).replacen("app-0:1.0", TOKEN, 1);
    let mut editor = TestEditor::new(language, &text);
    editor.secrets = Some(SecretOptions::default().with_placeholder(if secret {
        "«secret [0-9a-f]{8}»"
    } else {
        "«nothing [0-9]»"
    }));
    let (_owner, state) = editor.mount();
    if secret {
        assert_eq!(
            state.protect().secrets.tokens.with_untracked(Vec::len),
            1,
            "{language}: the placeholder is real"
        );
    }
    let ((((), parsed), reads), paths) = counted(Work::PathLookup, || {
        counted(Work::DocumentRead, || {
            counted(Work::ParsedBytes, || type_in_middle(state, blocks))
        })
    });
    [parsed, reads, paths]
}

#[test]
fn a_placeholder_away_from_a_keystroke_costs_it_no_read_of_the_document() {
    super::super::parse_cache::hold();
    let mut costly = Vec::new();
    for language in Language::ALL.iter().copied() {
        let [parsed, reads, paths] = typing_work(language, false);
        let secret = typing_work(language, true);
        if secret != [parsed, reads, paths + 1] {
            costly.push(format!(
                "{language}: (bytes parsed, span reads, path lookups) {secret:?} with a placeholder, {:?} without",
                [parsed, reads, paths]
            ));
        }
    }
    super::super::parse_cache::release();
    assert!(costly.is_empty(), "{costly:#?}");
}

/// `blocks` services whose images are each a placeholder (`«secret
/// 0000002a»`, …), with a syntax error after them when `broken`.
fn placeholder_services(language: Language, blocks: usize, broken: bool) -> String {
    let mut text = services(language, blocks);
    for index in 0..blocks {
        for (open, close) in [("\"", "\""), (">", "<")] {
            text = text.replacen(
                &format!("{open}app-{index}:1.0{close}"),
                &format!("{open}«secret {index:08x}»{close}"),
                1,
            );
        }
    }
    if broken {
        text.push_str(breaking(language));
    }
    text
}

/// Each kind of work (by [`Work::ALL`]) that Ctrl+A Tab and its undo cost
/// on `placeholder_services`, the placeholders the host's (`secret`) or
/// plain text; and the text's length.
fn whole_edit_work(
    language: Language,
    blocks: usize,
    broken: bool,
    secret: bool,
) -> (Vec<usize>, usize) {
    let text = placeholder_services(language, blocks, broken);
    let mut editor = TestEditor::new(language, &text);
    editor.secrets = Some(SecretOptions::default().with_placeholder(if secret {
        "«secret [0-9a-f]{8}»"
    } else {
        "«nothing [0-9]»"
    }));
    let (_owner, state) = editor.mount();
    if secret {
        assert_eq!(
            state.protect().secrets.tokens.with_untracked(Vec::len),
            blocks,
            "{language}: the placeholders are real"
        );
    }
    let ((), counts) = counted_all(|| {
        let units = state
            .doc()
            .text
            .with_untracked(|text| text.encode_utf16().count());
        run(state, Command::Indent { outdent: false }, (0, units), false);
        if let Some(undo) = state.history_edit(false) {
            state.commit(undo);
        }
    });
    (counts.to_vec(), text.len())
}

/// A whole-document edit (Ctrl+A Tab and its undo) checks the host's
/// placeholders in work linear in the text, in every language, while the
/// text parses and while it has a syntax error: with none it costs the
/// check nothing; with one per service, four times the text costs about
/// four times the extra work of each kind, and parses the text a few times
/// at most (QA 31: on the large sample pasted four times, broken by its
/// repeated keys, the check read the whole text for its placeholders'
/// fields with a parse for each of dozens of lines blanked, and Ctrl+A Tab
/// took 4.5 s).
#[test]
fn a_whole_document_edit_checks_placeholders_in_linear_work() {
    super::super::parse_cache::hold();
    let ((), none) = counted_all(|| {
        let verdict = Placeholders {
            behavior: Language::YAML.behavior(),
            reading_schema: None,
            known: &[],
            index: &WholeIndex::new(&[]),
            tokens: &[],
            matcher: &super::tokens::SecretMatcher::new(&SecretOptions::default()),
        }
        .check("a: 1\nb: [\n", "  a: 1\n  b: [\n", false);
        assert!(verdict.glued.is_none());
    });
    let mut costly = Vec::new();
    if none.iter().any(|count| *count > 0) {
        costly.push(format!("no placeholder: {none:?}"));
    }
    let parsed = Work::ALL
        .iter()
        .position(|work| *work == Work::ParsedBytes)
        .unwrap_or_default();
    for language in Language::ALL.iter().copied() {
        for broken in [false, true] {
            let extra = |blocks| {
                let (plain, _) = whole_edit_work(language, blocks, broken, false);
                let (secret, bytes) = whole_edit_work(language, blocks, broken, true);
                let extra: Vec<usize> = secret
                    .iter()
                    .zip(&plain)
                    .map(|(secret, plain)| secret.saturating_sub(*plain))
                    .collect();
                (extra, bytes)
            };
            let (small, small_bytes) = extra(100);
            let (large, large_bytes) = extra(400);
            let state = if broken { " (broken)" } else { "" };
            for (index, work) in Work::ALL.iter().enumerate() {
                // Linear is 4 times; a pass over the text per placeholder
                // is 16.
                if large[index] > 6 * small[index].max(200) {
                    costly.push(format!(
                        "{language}{state}: {work:?} {} for 100 services, {} for 400",
                        small[index], large[index]
                    ));
                }
            }
            for (bytes, extra) in [(small_bytes, &small), (large_bytes, &large)] {
                if extra[parsed] > 4 * bytes {
                    costly.push(format!(
                        "{language}{state}: {} bytes parsed for a text of {bytes}",
                        extra[parsed]
                    ));
                }
            }
        }
    }
    super::super::parse_cache::release();
    assert!(costly.is_empty(), "{costly:#?}");
}
