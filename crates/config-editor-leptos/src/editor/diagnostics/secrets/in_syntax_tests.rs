//! P1: a syntax message never quotes a secret, in every language: the
//! built-in ones, the custom language shipped with the editor and a
//! host's own whose messages quote whole lines.

use serde_json::Value;

use crate::editor::language::{ConfigLanguage, LanguageInfo, ParseError};
use crate::editor::schema::build_schema;
use crate::strings::default_strings;

const SECRET: &str = "s3cr3tZ";
const TOKEN: &str = "«secret 0b1c»";

/// A host's language whose messages quote the line they stop on: each
/// line is `key = value`.
struct Quoting;

impl ConfigLanguage for Quoting {
    fn info(&self) -> LanguageInfo {
        LanguageInfo {
            label: "Quoting",
            icon: "Q",
            extensions: &["quoting"],
        }
    }

    fn parse(&self, source: &str) -> Result<Value, ParseError> {
        let mut map = serde_json::Map::new();
        for (index, line) in source.lines().enumerate() {
            let Some((key, value)) = line.split_once(" = ").filter(|(key, _)| {
                !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric())
            }) else {
                return Err(ParseError {
                    message: format!("`{line}` is no `key = value` line"),
                    line: Some(index + 1),
                    column: Some(1),
                });
            };
            map.insert(key.to_string(), Value::from(value));
        }
        Ok(Value::Object(map))
    }
}

/// The messages of `text`'s problems, its schema marking the field at
/// `pointer` secret or not, the host having given `accepted`.
fn messages(
    behavior: &dyn ConfigLanguage,
    text: &str,
    pointer: &str,
    secret: bool,
    accepted: &[(String, String)],
) -> Vec<String> {
    let mut field = serde_json::json!({"type": "string"});
    if secret {
        field["writeOnly"] = Value::Bool(true);
    }
    let mut schema = field;
    for key in pointer.rsplit('/').filter(|key| !key.is_empty()) {
        schema = serde_json::json!({"properties": {key: schema}});
    }
    let bundle = build_schema(&schema.to_string()).expect("compiles");
    super::super::validate_with(
        text,
        Some(&bundle),
        None,
        true,
        behavior,
        accepted,
        default_strings(),
    )
    .into_iter()
    .map(|problem| problem.message)
    .collect()
}

/// A broken text in one language, with the secret field it was written
/// from.
struct Case {
    label: &'static str,
    behavior: &'static dyn ConfigLanguage,
    text: &'static str,
    pointer: &'static str,
    /// Whether the language's message quotes the broken text.
    quotes: bool,
}

/// In each language, a text whose error is on the line holding a secret's
/// value (a key half typed into it, a value read as a name).
fn cases() -> Vec<Case> {
    // The custom language is pushed last whatever the features.
    let mut cases = vec![Case {
        label: "a host's language",
        behavior: &Quoting,
        text: "user = app\npasswo rd = s3cr3tZ\n",
        pointer: "/password",
        quotes: true,
    }];
    #[cfg(feature = "xml")]
    cases.push(Case {
        label: "XML",
        behavior: crate::editor::Language::XML.behavior(),
        text: "<config>\n  <passwo\"rd>s3cr3tZ</password>\n  <port>\"1</port>\n</config>\n",
        pointer: "/config/password",
        quotes: true,
    });
    #[cfg(feature = "yaml")]
    cases.push(Case {
        label: "YAML",
        behavior: crate::editor::Language::YAML.behavior(),
        text: "user: app\npassword: *s3cr3tZ\n",
        pointer: "/password",
        quotes: true,
    });
    #[cfg(feature = "hcl")]
    cases.push(Case {
        label: "HCL",
        behavior: crate::editor::Language::HCL.behavior(),
        text: "user = \"app\"\npassword = <<s3cr3tZ\n",
        pointer: "/password",
        quotes: true,
    });
    #[cfg(feature = "toml")]
    cases.push(Case {
        label: "TOML",
        behavior: crate::editor::Language::TOML.behavior(),
        text: "user = \"app\"\npassword = s3cr3tZ\n",
        pointer: "/password",
        quotes: false,
    });
    #[cfg(feature = "json")]
    cases.push(Case {
        label: "JSON",
        behavior: crate::editor::Language::JSON.behavior(),
        text: "{\"user\": \"app\", \"password\": s3cr3tZ}\n",
        pointer: "/password",
        quotes: false,
    });
    cases.push(Case {
        label: "the custom language",
        behavior: &crate::editor::odd_syntax::ODD,
        text: "user := app\ns3cr3tZ := 1\ns3cr3tZ := 2\n",
        pointer: "/s3cr3tZ",
        quotes: true,
    });
    cases
}

/// In every language, a syntax message quoting text that holds a
/// secret's value (a broken key run into it, a value read as a name) says
/// what is wrong without it, where a schema marks a field secret.
#[test]
fn a_syntax_message_never_quotes_a_secret_in_any_language() {
    for Case {
        label,
        behavior,
        text,
        pointer,
        quotes,
    } in cases()
    {
        let plain = messages(behavior, text, pointer, false, &[]);
        let hidden = messages(behavior, text, pointer, true, &[]);
        assert_eq!(hidden.len(), 1, "{label}: {hidden:?}");
        assert!(!hidden[0].contains(SECRET), "{label}: {hidden:?}");
        // Only what is quoted goes: the message says the same otherwise
        // (JSON and TOML word every syntax error without quoting).
        if quotes {
            assert!(plain[0].contains(SECRET), "{label}: {plain:?}");
            assert!(hidden[0].contains('…'), "{label}: {hidden:?}");
        } else {
            assert_eq!(plain, hidden, "{label}");
        }
    }
}

/// A syntax message never quotes a host's placeholder, nor a piece of
/// one, with no schema marking a secret.
#[test]
fn a_syntax_message_never_quotes_a_placeholder_in_any_language() {
    for Case {
        label,
        behavior,
        text,
        pointer,
        ..
    } in cases()
    {
        let text = text.replace(SECRET, TOKEN);
        let accepted = [(TOKEN.to_string(), pointer.to_string())];
        let hidden = messages(behavior, &text, pointer, false, &accepted);
        assert_eq!(hidden.len(), 1, "{label}: {hidden:?}");
        for piece in ["secret", "0b1c", "«", "»"] {
            assert!(!hidden[0].contains(piece), "{label}: {hidden:?}");
        }
    }
}

/// A switch to another language waiting for a broken text to parse says
/// why without quoting a secret: its message repeats the syntax error as
/// the syntax problem words it.
#[cfg(all(feature = "xml", feature = "yaml"))]
#[test]
fn a_switch_waiting_on_a_broken_text_never_quotes_a_secret() {
    use crate::editor::Language;
    use leptos::prelude::GetUntracked;

    let text = "<config>\n  <passwo\"rd>s3cr3tZ</password>\n  <port>\"1</port>\n</config>\n";
    let schema = serde_json::json!({"properties": {"config": {"properties": {
        "password": {"type": "string", "writeOnly": true}
    }}}});
    let mut editor = crate::editor::test_state::TestEditor::new(Language::XML, text);
    editor.schema = Some(std::sync::Arc::new(
        build_schema(&schema.to_string()).expect("compiles"),
    ));
    let (_owner, state) = editor.mount();
    crate::editor::sync::switch_to(state, Language::YAML);
    assert!(state.sync().pending_switch.get_untracked().is_some());
    state.check_text_now();
    let messages: Vec<String> = state
        .problems()
        .own
        .get_untracked()
        .into_iter()
        .map(|problem| problem.message)
        .collect();
    assert!(
        messages.iter().any(|message| message.contains("YAML")),
        "{messages:?}"
    );
    assert!(
        messages.iter().all(|message| !message.contains(SECRET)),
        "{messages:?}"
    );
}

/// A message about a line holding no secret is not changed when the text
/// holds none.
#[test]
fn a_syntax_message_without_secrets_keeps_its_quotes() {
    let text = "user = app\nnot a line\n";
    let plain = messages(&Quoting, text, "/password", false, &[]);
    assert_eq!(plain, ["`not a line` is no `key = value` line"]);
}
