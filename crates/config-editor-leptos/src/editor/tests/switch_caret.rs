//! An Edit-as switch rewrites the whole text. With a textarea the caret is
//! read back from it; without one (a headless editor) the caret follows
//! the change, so it is never a byte of the text before that lies inside a
//! character of the text after (P5: reading the caret cut the text there
//! and panicked).

use leptos::prelude::*;

use super::super::Language;
use super::super::sync::switch_to;
use super::super::test_state::TestEditor;
use super::super::text::TextSize;

#[test]
fn the_caret_stays_on_a_character_boundary_through_every_switch() {
    let texts = [
        // A key the view spells otherwise: the caret on it finds no field.
        "web:\n  - tls: \"true\"\n    x_1:\n      - \"\"\n      - \"true\"\n      - é\nport:\n  - web\nhos\tt:\n  - \"8080\"\n  - false\n",
        "a: é\nb: 😀x\n",
        "é: 1\n# ünï\nc: [é, 2]\n",
    ];
    let views = [Language::JSON, Language::TOML, Language::HCL, Language::XML];
    let mut failures = Vec::new();
    for text in texts {
        for view in views {
            let boundaries = (0..=text.len()).filter(|at| text.is_char_boundary(*at));
            for at in boundaries {
                let (_owner, state) = TestEditor::new(Language::YAML, text).mount();
                state.note_caret(TextSize::try_from(at).unwrap());
                for next in [view, Language::YAML] {
                    switch_to(state, next);
                    let caret = usize::from(state.doc().caret.get_untracked());
                    let shown = state.doc().text.get_untracked();
                    if caret > shown.len() || !shown.is_char_boundary(caret) {
                        failures.push(format!(
                            "{text:?} caret {at}, into {next:?}: caret {caret} in {shown:?}"
                        ));
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
