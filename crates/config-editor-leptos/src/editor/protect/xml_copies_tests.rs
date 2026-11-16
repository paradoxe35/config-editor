//! A secret's line copied in XML (Shift+Alt+Down, a copy and paste, or
//! Alt+Down moving it into a block holding the same field): the copies are
//! elements of the field's name, read as a list. A secret is never glued:
//! text typed right after any copy's placeholder (the first, one in the
//! middle, the last) is refused, as in the other languages, and the host
//! never hears it, in the host's own view and in an Edit-as XML view (QA
//! 36: right after the first copy it was taken, heard and saved). The
//! same for a secret attribute on a copied element. A new, different
//! element beside the secret element stays allowed (see
//! `xml_list_secret_tests`).

#![allow(clippy::string_slice, reason = "the tests cut their own texts")]

use leptos::prelude::*;

use super::super::Language;
use super::super::keys::line_edit::LineCommand;
use super::super::save_matrix::report;
use super::super::secret_options::SecretOptions;
use super::super::state::EditorState;
use super::super::sync::convert_now;
use super::super::test_state::TestEditor;
use super::glued_tests::{paste, type_at};
use super::many_copies_tests::{line_start, mask_line, on_line, shown};
use super::texts;

const MASK: &str = "«secret 0b1c»";

/// Where the secret is written.
#[derive(Clone, Copy, Debug)]
enum Form {
    Element,
    Attribute,
}

/// The XML text with the secret in block `b` (and, for a move, in block
/// `a` above it too, the same token).
fn xml(form: Form, moved: bool) -> String {
    let line = match form {
        Form::Element => format!("    <key>{MASK}</key>\n"),
        Form::Attribute => format!("    <c key=\"{MASK}\" y=\"1\"/>\n"),
    };
    let a = if moved {
        format!("  <a>\n{line}  </a>\n")
    } else {
        String::new()
    };
    format!("<config>\n{a}  <b>\n{line}    <x>1</x>\n  </b>\n</config>\n")
}

/// The same document in `host`, shown as XML.
fn host_text(host: Language, form: Form, moved: bool) -> String {
    if host == Language::XML {
        return xml(form, moved);
    }
    let value = Language::XML.parse(&xml(form, moved)).expect("XML parses");
    host.serialize(&value).expect("written in the host")
}

/// A way to copy the secret's line.
#[derive(Clone, Copy, Debug)]
enum Way {
    /// Shift+Alt+Down.
    Duplicate,
    /// The line selected, copied, and pasted below it.
    Paste,
    /// Alt+Down from block `a` into block `b` (two copies only).
    Move,
}

struct Mounted {
    _owner: Owner,
    state: EditorState,
    heard: RwSignal<Vec<String>>,
}

fn mount(host: Language, form: Form, way: Way) -> Mounted {
    let heard = RwSignal::new(Vec::<String>::new());
    let mut editor = TestEditor::new(host, &host_text(host, form, matches!(way, Way::Move)));
    if host != Language::XML {
        editor.edit_as = Some(Language::XML);
    }
    editor.secrets = Some(SecretOptions::default().with_placeholder("«secret [0-9a-f]{4}»"));
    editor.on_change = Some(Callback::new(move |text| {
        heard.update(|heard| heard.push(text));
    }));
    let (owner, state) = editor.mount();
    Mounted {
        _owner: owner,
        state,
        heard,
    }
}

/// The secret's line copied until it is written `copies` times.
fn copy(state: EditorState, way: Way, copies: usize) -> Result<(), String> {
    for _ in 1..copies {
        match way {
            Way::Duplicate => on_line(state, mask_line(state, 0), LineCommand::Copy { down: true }),
            Way::Paste => {
                let text = shown(state);
                let line = mask_line(state, 0);
                let (start, end) = (line_start(&text, line), line_start(&text, line + 1));
                let copied = text[start..end].to_string();
                paste(state, end..end, &copied);
            }
            Way::Move => {
                // From inside `<a>` past `</a>` and `<b>`.
                for _ in 0..2 {
                    on_line(state, mask_line(state, 0), LineCommand::Move { down: true });
                }
            }
        }
    }
    let written = shown(state).matches(MASK).count();
    if written != copies {
        return Err(format!("copied {written} times:\n{}", shown(state)));
    }
    Ok(())
}

/// The texts holding the mask with a letter right after it.
fn glued(texts: &[String]) -> Vec<String> {
    texts
        .iter()
        .filter(|text| text.contains(&format!("{MASK}M")))
        .cloned()
        .collect()
}

#[test]
fn text_beside_any_copy_of_a_secret_element_or_attribute_is_refused() {
    let mut failures = Vec::new();
    for host in [Language::XML, Language::YAML, Language::JSON] {
        for form in [Form::Element, Form::Attribute] {
            for (way, counts) in [
                (Way::Duplicate, &[2, 3, 5][..]),
                (Way::Paste, &[2, 3][..]),
                (Way::Move, &[2][..]),
            ] {
                for &copies in counts {
                    let mut tried = vec![0, copies / 2, copies - 1];
                    tried.dedup();
                    for nth in tried {
                        let case = format!("{host} {form:?} {way:?} {copies} copies, copy {nth}");
                        let mounted = mount(host, form, way);
                        let state = mounted.state;
                        if let Err(error) = copy(state, way, copies) {
                            failures.push(format!("{case}: {error}"));
                            continue;
                        }
                        let text = shown(state);
                        let start = text.match_indices(MASK).nth(nth).expect("the mask").0;
                        type_at(state, start + MASK.len(), "MY");
                        convert_now(state, shown(state), Language::XML);
                        let mut found = glued(&[shown(state)]);
                        found.extend(glued(&mounted.heard.get_untracked()));
                        if !found.is_empty() {
                            failures.push(format!("{case}: {found:?}"));
                        }
                    }
                }
            }
        }
    }
    report(failures);
}

/// The secret's block in `language`, as `xml` writes it in XML.
fn native(language: Language, form: Form) -> String {
    match language.label() {
        "XML" => xml(form, false),
        "YAML" => format!("config:\n  b:\n    key: {MASK}\n    x: 1\n"),
        "JSON" => format!(
            "{{\n  \"config\": {{\n    \"b\": {{\n      \"key\": \"{MASK}\",\n      \"x\": 1\n    }}\n  }}\n}}\n"
        ),
        "TOML" => format!("[config.b]\nkey = \"{MASK}\"\nx = 1\n"),
        _ => format!("config {{\n  b {{\n    key = \"{MASK}\"\n    x = 1\n  }}\n}}\n"),
    }
}

/// A copy of a secret's line is said to be one, the same way in every
/// language and however many copies there are, and the host's own line,
/// left as it was, is not flagged (QA 36: XML flagged the original as
/// standing for another field's secret; YAML said nothing of one copy but
/// "duplicate key").
#[test]
fn copies_of_a_secret_line_are_flagged_alike_in_every_language() {
    let strings = crate::strings::default_strings();
    let mut failures = Vec::new();
    for language in [
        Language::YAML,
        Language::JSON,
        Language::TOML,
        Language::HCL,
        Language::XML,
    ] {
        let forms: &[Form] = if language == Language::XML {
            &[Form::Element, Form::Attribute]
        } else {
            &[Form::Element]
        };
        for &form in forms {
            for copies in [2, 3, 7] {
                let mut editor = TestEditor::new(language, &native(language, form));
                editor.secrets =
                    Some(SecretOptions::default().with_placeholder("«secret [0-9a-f]{4}»"));
                let (_owner, state) = editor.mount();
                if let Err(error) = copy(state, Way::Duplicate, copies) {
                    failures.push(format!("{language} {form:?} {copies}: {error}"));
                    continue;
                }
                let text = shown(state);
                let lines: Vec<usize> = text
                    .lines()
                    .enumerate()
                    .filter(|(_, line)| line.contains(MASK))
                    .map(|(index, _)| index + 1)
                    .collect();
                let found = super::guarantee_tests::problems(state, language, None);
                let pointer = match form {
                    Form::Attribute => "/config/b/c/key",
                    Form::Element => "/config/b/key",
                };
                let extra = texts::extra_placeholder(strings, pointer);
                let flagged: Vec<&String> = found
                    .iter()
                    .filter(|problem| problem.contains("placeholder"))
                    .collect();
                let right = flagged.len() == copies - 1
                    && lines[1..].iter().zip(&flagged).all(|(line, problem)| {
                        problem.starts_with(&format!("{line}:")) && problem.ends_with(&extra)
                    });
                if !right {
                    failures.push(format!(
                        "{language} {form:?} {copies} copies:\n{text}{found:#?}"
                    ));
                }
            }
        }
    }
    report(failures);
}
