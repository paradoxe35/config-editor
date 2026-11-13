//! A placeholder the host wrote alone in one of a language's multi-line
//! string forms (a YAML block scalar of any chomping, folding and
//! indentation indicator, a TOML multi-line string, an HCL heredoc, XML
//! text on its own line) is the field's whole value, as one written on
//! one line is: its value holds only the line breaks and indentation the
//! form adds around it. Untouched, the schema check accepts it and nothing
//! flags it; a key typed at either edge is refused, shown in the host's
//! language and as every other one, and the host never hears it glued.

use leptos::prelude::*;
use serde_json::Value;

use super::super::Language;
use super::super::save_matrix::{ALL, report};
use super::super::sync::convert_now;
use super::super::test_state::TestEditor;
use super::glued_tests::{breaking, type_at};
use super::guarantee_tests::{HOSTS, Tokens};

const SAYS: &str =
    "/a: a secret placeholder stands for the whole secret, so nothing can be written beside it";

/// Each host language's multi-line forms of `a` holding `token` alone,
/// with `b` after it.
fn forms(token: &str) -> Vec<(Language, &'static str, String)> {
    let yaml =
        |header: &str, after: &str| format!("name: x\na: {header}\n  {token}\n{after}b: y\n");
    vec![
        (Language::YAML, "|", yaml("|", "")),
        (Language::YAML, ">", yaml(">", "")),
        (Language::YAML, "|2", yaml("|2", "")),
        (Language::YAML, ">2", yaml(">2", "")),
        (Language::YAML, "|-", yaml("|-", "")),
        (Language::YAML, ">-", yaml(">-", "")),
        (Language::YAML, "|+", yaml("|+", "\n")),
        (Language::YAML, ">+", yaml(">+", "\n")),
        (Language::YAML, "|2-", yaml("|2-", "")),
        (Language::YAML, "|+2", yaml("|+2", "")),
        (
            Language::TOML,
            "\"\"\"",
            format!("name = \"x\"\na = \"\"\"\n{token}\n\"\"\"\nb = \"y\"\n"),
        ),
        (
            Language::TOML,
            "'''",
            format!("name = \"x\"\na = '''\n{token}\n'''\nb = \"y\"\n"),
        ),
        (
            Language::TOML,
            "\"\"\" closed on its line",
            format!("name = \"x\"\na = \"\"\"\n{token}\"\"\"\nb = \"y\"\n"),
        ),
        (
            Language::HCL,
            "<<EOF",
            format!("name = \"x\"\na = <<EOF\n{token}\nEOF\nb = \"y\"\n"),
        ),
        (
            Language::HCL,
            "<<-EOF",
            format!("name = \"x\"\na = <<-EOF\n  {token}\n  EOF\nb = \"y\"\n"),
        ),
        (
            Language::XML,
            "text on its own line",
            format!("<name>x</name>\n<a>\n  {token}\n</a>\n<b>y</b>\n"),
        ),
        (
            Language::JSON,
            "\\n",
            format!("{{\n  \"name\": \"x\",\n  \"a\": \"{token}\\n\",\n  \"b\": \"y\"\n}}\n"),
        ),
    ]
}

/// The string `/a` reads as in `text`, in `language`.
fn field_a(language: Language, text: &str) -> Option<String> {
    language
        .parse(text)
        .ok()
        .and_then(|value| value.get("a").and_then(Value::as_str).map(str::to_string))
}

/// A live editor on the host's `text` in `host`, shown as `view`, with
/// what `on_change` heard.
fn mount(
    host: Language,
    view: Language,
    text: &str,
    tokens: &Tokens,
) -> (
    Owner,
    super::super::state::EditorState,
    RwSignal<Vec<String>>,
) {
    let heard = RwSignal::new(Vec::<String>::new());
    let mut editor = TestEditor::new(host, text);
    if view != host {
        editor.edit_as = Some(view);
    }
    editor.secrets = Some(tokens.options());
    editor.on_change = Some(Callback::new(move |text| {
        heard.update(|heard| heard.push(text));
    }));
    let (owner, state) = editor.mount();
    (owner, state, heard)
}

/// Every host form, shown in every language, for both patterns.
fn each_case(mut run: impl FnMut(&str, Language, Language, &str, &Tokens)) {
    for tokens in &HOSTS {
        for (host, form, text) in forms(tokens.same) {
            for view in ALL {
                let case = format!("{host} {form} as {view} ({})", tokens.pattern);
                run(&case, host, view, &text, tokens);
            }
        }
    }
}

#[cfg(feature = "schema-validation")]
#[test]
fn an_untouched_placeholder_alone_in_a_multi_line_form_passes_the_schema_check() {
    use super::super::diagnostics::{build_schema, validate, validate_with};
    use super::guarantee_tests::problems;
    let bundle = build_schema(
        r#"{"type": "object", "properties": {
            "a": {"type": "string", "x-secret": true, "minLength": 40, "pattern": "^-----BEGIN"}
        }}"#,
    )
    .unwrap();
    let mut failures = Vec::new();
    each_case(|case, host, view, text, tokens| {
        let (_owner, state, _) = mount(host, view, text, tokens);
        let shown = state.doc().text.get_untracked();
        let accepted = untrack(|| state.protect().accepted_tokens());
        let strings = crate::strings::default_strings();
        let found = validate_with(
            &shown,
            Some(&bundle),
            None,
            true,
            view.behavior(),
            &accepted,
            strings,
        );
        if !found.is_empty() {
            failures.push(format!("{case}: {found:?} in\n{shown}"));
        }
        // The check is there: the same value is flagged without the
        // host's placeholders.
        if validate(&shown, Some(&bundle), None, true, view.behavior()).is_empty() {
            failures.push(format!("{case}: no problem without the placeholders"));
        }
        let flagged = problems(state, view, None);
        if !flagged.is_empty() {
            failures.push(format!("{case}: flagged {flagged:?}"));
        }
    });
    report(failures);
}

#[test]
fn a_key_typed_at_either_edge_of_a_placeholder_alone_in_a_multi_line_form_is_refused() {
    let mut failures = Vec::new();
    each_case(|case, host, view, text, tokens| {
        let Some(given) = field_a(host, text) else {
            failures.push(format!("{case}: the host text does not parse"));
            return;
        };
        for (step, after, typed) in [
            ("Z after it", true, "Z"),
            ("Z before it", false, "Z"),
            ("a space and a word after it", true, " x"),
        ] {
            let (_owner, state, heard) = mount(host, view, text, tokens);
            let shown = state.doc().text.get_untracked();
            let Some(start) = shown.find(tokens.same) else {
                failures.push(format!("{case}: no token in\n{shown}"));
                return;
            };
            let at = if after {
                start + tokens.same.len()
            } else {
                start
            };
            type_at(state, at, typed);
            convert_now(
                state,
                state.doc().text.get_untracked(),
                state.sync().edit_language.get_untracked(),
            );
            let now = state.doc().text.get_untracked();
            if field_a(view, &now).as_deref() != Some(given.as_str()) {
                failures.push(format!("{case}, {step}: shown {now:?}"));
            }
            if !state
                .protect()
                .tip
                .get_untracked()
                .is_some_and(|tip| tip.message.starts_with(SAYS))
            {
                failures.push(format!(
                    "{case}, {step}: refusal {:?}",
                    state.protect().tip.get_untracked()
                ));
            }
            if let Some(glued) = heard
                .get_untracked()
                .iter()
                .find(|text| field_a(host, text).as_deref() != Some(given.as_str()))
            {
                failures.push(format!("{case}, {step}: the host heard {glued:?}"));
            }
        }
    });
    report(failures);
}

/// A host text that does not parse: the language finds `a` written
/// around the token and its value is taken from that writing, as the
/// parsed text gives it.
#[test]
fn a_placeholder_alone_in_a_multi_line_form_of_a_broken_host_is_whole() {
    use super::tokens::SecretMatcher;
    let mut failures = Vec::new();
    for tokens in &HOSTS {
        // A TOML token closing its string on its line is not placed in its
        // field while the text does not parse (a lookup by place, not by
        // what the value is), so is not one to ask about here.
        for (host, form, text) in forms(tokens.same)
            .into_iter()
            .filter(|(_, form, _)| !form.ends_with("closed on its line"))
        {
            let broken = format!("{text}{}", breaking(host));
            let known = SecretMatcher::new(&tokens.options()).known(host.behavior(), &broken);
            let whole: Vec<_> = known
                .iter()
                .filter(|known| known.field.as_deref() == Some("/a"))
                .map(|known| known.whole.clone())
                .collect();
            let given = field_a(host, &text);
            if whole != vec![given.clone()] {
                failures.push(format!("{host} {form}: {whole:?}, not {given:?}"));
            }
        }
    }
    report(failures);
}
