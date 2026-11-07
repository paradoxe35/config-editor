//! Secret placeholders across every language: tokens surviving every
//! Edit-as conversion and save (by property, under random edits), the
//! schema check accepting an untouched placeholder, unknown ones flagged,
//! and reference completions in a secret field.

use serde_json::Value;

use super::super::Language;
use super::super::bridge::Bridge;
use super::super::codec;
use super::super::diagnostics::build_schema;
use super::locks::{Sources, protect};
use super::problems::unknown_placeholders;
use super::suggest::{Around, adjust};
use super::tokens::{SecretMatcher, SecretOptions, locate, widen};

const SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "name": {"type": "string"},
    "secret": {"type": "string", "x-secret": true, "minLength": 40, "pattern": "^-----BEGIN"},
    "pin": {"type": "integer", "writeOnly": true},
    "id": {"type": "string", "format": "uuid", "x-secret": true, "maxLength": 36}
  }
}"#;

const SECRET: &str = "«secret 3f2a»";
const PIN: &str = "«secret 0b1c»";
const ID: &str = "«secret 9d»";

/// The same document in every language, holding three placeholders.
fn documents() -> Vec<(Language, String)> {
    vec![
        (
            Language::YAML,
            format!("name: a\nsecret: {SECRET}\npin: {PIN}\nid: {ID}\n"),
        ),
        (
            Language::JSON,
            format!(
                "{{\n  \"name\": \"a\",\n  \"secret\": \"{SECRET}\",\n  \"pin\": \"{PIN}\",\n  \"id\": \"{ID}\"\n}}\n"
            ),
        ),
        (
            Language::TOML,
            format!("name = \"a\"\nsecret = \"{SECRET}\"\npin = \"{PIN}\"\nid = \"{ID}\"\n"),
        ),
        (
            Language::HCL,
            format!("name = \"a\"\nsecret = \"{SECRET}\"\npin = \"{PIN}\"\nid = \"{ID}\"\n"),
        ),
        (
            Language::XML,
            format!("<name>a</name>\n<secret>{SECRET}</secret>\n<pin>{PIN}</pin>\n<id>{ID}</id>\n"),
        ),
    ]
}

fn matcher() -> SecretMatcher {
    SecretMatcher::new(&SecretOptions::default())
}

fn schema() -> Value {
    serde_json::from_str(SCHEMA).unwrap()
}

/// Every `«` of `text` starts a whole token the host gave.
fn no_cut_tokens(text: &str) -> bool {
    text.match_indices('«').all(|(at, _)| {
        [SECRET, PIN, ID]
            .iter()
            .any(|token| text[at..].starts_with(token))
    })
}

#[test]
fn placeholders_survive_every_conversion_and_save() {
    let bundle = std::sync::Arc::new(build_schema(SCHEMA).unwrap());
    let schema = schema();
    let mut failures = Vec::new();
    for (host, host_text) in documents() {
        let known = matcher().known(&host_text);
        if known.len() != 3 {
            failures.push(format!("{host}: known {known:?}"));
            continue;
        }
        for (edit, _) in documents() {
            let Some(bridge) = Bridge::new(host, edit, Some(bundle.clone())) else {
                continue;
            };
            let shown = match bridge.to_edit(&host_text) {
                Ok(conversion) => conversion.text,
                Err(error) => {
                    failures.push(format!("{host} as {edit}: {error}"));
                    continue;
                }
            };
            if locate(&shown, &known).len() != 3 {
                failures.push(format!("{host} as {edit}: tokens lost in {shown:?}"));
                continue;
            }
            // An edit elsewhere, saved back: every token as it was.
            let edited = [("\"a\"", "\"b\""), (">a<", ">b<"), (": a", ": b")]
                .iter()
                .find(|(from, _)| shown.contains(from))
                .map_or_else(|| shown.clone(), |(from, to)| shown.replacen(from, to, 1));
            let saved = match bridge.to_host(&edited, &host_text) {
                Ok(conversion) => conversion.text,
                Err(error) => {
                    failures.push(format!("{host} as {edit}: save failed: {error}"));
                    continue;
                }
            };
            let read = codec::read(host.behavior(), &saved, Some(&schema)).ok();
            for (pointer, token) in [("/secret", SECRET), ("/pin", PIN), ("/id", ID)] {
                if read.as_ref().and_then(|value| value.pointer(pointer))
                    != Some(&Value::from(token))
                {
                    failures.push(format!("{host} as {edit}: {pointer} saved as {saved:?}"));
                }
                if !saved.contains(token) {
                    failures.push(format!(
                        "{host} as {edit}: {token} not byte-for-byte in {saved:?}"
                    ));
                }
            }
            // And back, unedited: the host's text exactly.
            if bridge
                .to_host(&shown, &host_text)
                .map(|conversion| conversion.text)
                .ok()
                != Some(host_text.clone())
            {
                failures.push(format!(
                    "{host} as {edit}: an unedited round trip changed the host"
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// A small fixed-seed generator, so a failure always reproduces.
struct Random(u64);

impl Random {
    fn below(&mut self, bound: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % bound.max(1) as u64) as usize
    }
}

const PIECES: &[&str] = &[
    "x", "1", " ", "\n", "\"", ":", "=", "<", ">", "", "", "secret", "zz",
];

/// Random edits in every language pair, each widened over the placeholders
/// it touches as the editor does: no placeholder is ever cut, in the text
/// on screen or in the saved host, and every saved one is the host's token
/// byte-for-byte.
#[test]
fn random_edits_never_cut_a_placeholder() {
    let bundle = std::sync::Arc::new(build_schema(SCHEMA).unwrap());
    let mut failures = Vec::new();
    let mut random = Random(0x2545_f491_4f6c_dd1d);
    let mut saves = 0;
    for (host, host_text) in documents() {
        let known = matcher().known(&host_text);
        for (edit, _) in documents() {
            let bridge = Bridge::new(host, edit, Some(bundle.clone()));
            let mut shown = match &bridge {
                Some(bridge) => bridge.to_edit(&host_text).unwrap().text,
                None => host_text.clone(),
            };
            let mut saved_host = host_text.clone();
            let mut tokens = locate(&shown, &known);
            for step in 0..120 {
                let units: Vec<u16> = shown.encode_utf16().collect();
                let start = random.below(units.len() + 1);
                let end = (start + random.below(5)).min(units.len());
                let piece = PIECES[random.below(PIECES.len())];
                let mut edited = String::from_utf16_lossy(&units[..start]);
                edited.push_str(piece);
                edited.push_str(&String::from_utf16_lossy(&units[end..]));
                let edited = match widen(&tokens, &shown, &edited, start..end) {
                    Some((widened, _, _)) => widened,
                    None => edited,
                };
                if !no_cut_tokens(&edited) {
                    failures.push(format!(
                        "{host} as {edit}, step {step}: {piece:?} at {start}..{end} cut a token:\n{shown}\n→\n{edited}"
                    ));
                    break;
                }
                // Text that no longer parses is fixed by the user first.
                if codec::read(edit.behavior(), &edited, Some(&bundle.schema)).is_err() {
                    continue;
                }
                shown = edited;
                tokens = locate(&shown, &known);
                let converted = match &bridge {
                    Some(bridge) => bridge
                        .to_host(&shown, &saved_host)
                        .map(|conversion| conversion.text),
                    None => Ok(shown.clone()),
                };
                if let Ok(converted) = converted {
                    if !no_cut_tokens(&converted) {
                        failures.push(format!(
                            "{host} as {edit}, step {step}: saved a cut token: {converted}"
                        ));
                    }
                    saved_host = converted;
                    saves += 1;
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n\n")
    );
    assert!(saves > 500, "only {saves} saves");
}

#[cfg(feature = "schema-validation")]
#[test]
fn an_untouched_placeholder_passes_the_schema_check_in_every_language() {
    use super::super::diagnostics::{validate, validate_with};
    let bundle = build_schema(SCHEMA).unwrap();
    let mut failures = Vec::new();
    for (language, text) in documents() {
        let accepted: Vec<String> = matcher()
            .known(&text)
            .into_iter()
            .map(|(token, _)| token)
            .collect();
        let problems = validate_with(
            &text,
            Some(&bundle),
            None,
            true,
            language.behavior(),
            &accepted,
        );
        if !problems.is_empty() {
            failures.push(format!("{language}: {problems:#?}"));
        }
        // Without them, the same values fail: pattern, length, type, format.
        let unaccepted = validate(&text, Some(&bundle), None, true, language.behavior());
        if unaccepted.len() < 2 {
            failures.push(format!(
                "{language}: only {unaccepted:?} without the placeholders"
            ));
        }
        // A typed value is checked, and never quoted in the message.
        let typed = text.replacen(SECRET, "hunter2", 1);
        let problems = validate_with(
            &typed,
            Some(&bundle),
            None,
            true,
            language.behavior(),
            &accepted,
        );
        if !problems
            .iter()
            .any(|problem| problem.message.starts_with("/secret"))
        {
            failures.push(format!(
                "{language}: a typed secret was not checked: {problems:?}"
            ));
        }
        if problems
            .iter()
            .any(|problem| problem.message.contains("hunter2"))
        {
            failures.push(format!(
                "{language}: a message quotes the secret: {problems:?}"
            ));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn unknown_placeholders_in_secret_fields_are_flagged() {
    let schema = schema();
    let mut failures = Vec::new();
    for (language, text) in documents() {
        let known = matcher().known(&text);
        let none = unknown_placeholders(language.behavior(), &text, &matcher(), &known, &schema);
        if !none.is_empty() {
            failures.push(format!("{language}: flagged the host's own: {none:?}"));
        }
        let typed = text
            .replacen(SECRET, "«secret beef»", 1)
            .replacen('a', "«secret 77»", 1);
        let flagged =
            unknown_placeholders(language.behavior(), &typed, &matcher(), &known, &schema);
        // The one in `secret`, not the look-alike in `name`.
        if flagged.len() != 1 || !flagged[0].message.starts_with("/secret") {
            failures.push(format!("{language}: flagged {flagged:?}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn a_secret_field_offers_reference_forms_in_every_language() {
    let schema = schema();
    let cases = [
        (Language::YAML, "name: a\nsecret: ", "\n"),
        (Language::JSON, "{\n  \"secret\": ", "\n}\n"),
        (Language::TOML, "secret = ", "\n"),
        (Language::HCL, "secret = ", "\n"),
        (Language::XML, "<secret>", "</secret>\n"),
    ];
    let prefixes = vec!["${".to_string(), "file:".to_string()];
    let mut failures = Vec::new();
    for (language, before, after) in cases {
        let text = format!("{before}{after}");
        let cursor = before.encode_utf16().count();
        let protected = protect(language.behavior(), &text, Sources::default());
        let options = adjust(
            Vec::new(),
            &Around {
                behavior: language.behavior(),
                source: &text,
                cursor,
                explicit: true,
                schema: Some(&schema),
                host_paths: &[],
                protected: &protected,
                prefixes: &prefixes,
            },
        );
        let labels: Vec<&str> = options.iter().map(|option| option.label.as_str()).collect();
        if labels != ["${VAR}", "file:path"] {
            failures.push(format!("{language}: {labels:?}"));
            continue;
        }
        let variable = &options[0];
        let caret: String = String::from_utf16_lossy(
            &variable.insert.encode_utf16().collect::<Vec<_>>()[..variable.cursor_offset],
        );
        if !caret.ends_with("${") {
            failures.push(format!(
                "{language}: caret after {caret:?} in {:?}",
                variable.insert
            ));
        }
        if language == Language::HCL && !variable.insert.contains("$${") {
            failures.push(format!("HCL: `${{` not escaped: {:?}", variable.insert));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
