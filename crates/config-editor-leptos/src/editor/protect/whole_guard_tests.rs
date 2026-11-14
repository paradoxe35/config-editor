//! A placeholder costs a keystroke away from it nothing, in every language:
//! the check that keeps it whole (`whole_guard`) judges an edit by what it
//! makes, yet typing in a value far from it parses no more of the document,
//! and reads no more of its spans, than with no placeholder (the line's
//! path is read once for all the keys typed there).

use leptos::prelude::*;

use super::super::Language;
use super::super::secret_options::SecretOptions;
use super::super::test_state::TestEditor;
use super::super::work_count::{Work, counted};
use super::reach_tests::{services, type_in_middle};

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
