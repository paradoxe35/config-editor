//! P1: a placeholder stays whole in a text that does not parse: one pasted
//! into its field's key (the key split, the text broken) leaves the
//! placeholder of the value a span no paste may enter.

use leptos::prelude::*;

use super::super::Language;
use super::super::secret_options::SecretOptions;
use super::super::test_state::TestEditor;
use super::glued_tests::paste;

const TOKEN: &str = "«secret 3f2a»";

#[test]
fn a_paste_inside_a_placeholder_is_refused_after_its_key_was_split() {
    let text = format!("[clients.acme]\nsecret = \"{TOKEN}\"\n");
    let mut editor = TestEditor::new(Language::TOML, &text);
    editor.secrets = Some(SecretOptions::default());
    let (_owner, state) = editor.mount();
    // The placeholder and its closing quote copied into the key.
    let key = text.find("secret =").expect("the key") + "sec".len();
    let copied = format!("{TOKEN}\"\n");
    assert!(paste(state, key..key, &copied), "the copy is taken");
    let shown = state.doc().text.get_untracked();
    assert_eq!(
        shown,
        format!("[clients.acme]\nsec{TOKEN}\"\nret = \"{TOKEN}\"\n")
    );
    // Another placeholder pasted inside the field's own: refused, or taken
    // over the whole placeholder (the same token), never inside it.
    let inside = shown.rfind(TOKEN).expect("the field's placeholder") + "«s".len();
    paste(state, inside..inside, TOKEN);
    assert_eq!(state.doc().text.get_untracked(), shown);
    let start = shown.rfind(TOKEN).expect("the placeholder");
    assert_eq!(
        state.protect().secrets.tokens.get_untracked(),
        vec![super::super::text::TextRange::at(
            super::super::text::text_size(start),
            super::super::text::TextSize::of(TOKEN)
        )],
        "the field's placeholder stays one"
    );
}
