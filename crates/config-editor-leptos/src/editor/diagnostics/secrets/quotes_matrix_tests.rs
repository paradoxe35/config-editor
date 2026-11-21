//! P1 for syntax messages that quote the text, by class: in every
//! built-in language, an error quoting text before a secret, inside it or
//! after it, the secret a schema-secret field's value or a placeholder,
//! written in ASCII, with accents, in Japanese or with emoji. No secret is
//! ever shown, and a message whose quotes hold no secret reads exactly as
//! the letter-changing re-read (the fallback for languages that give no
//! quotes) shows it.

use crate::editor::Language;
use crate::editor::diagnostics::{Placeholders, syntax_message};
use crate::editor::language::ParseError;
use crate::editor::schema::build_schema;

/// Where the text an error quotes stands beside the secret.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Place {
    Before,
    Inside,
    After,
}

/// A broken text of `language`, `{S}` standing for the secret, at the
/// field `pointer`; `numeric`: only a number can stand there.
struct Template {
    language: Language,
    place: Place,
    text: &'static str,
    pointer: &'static str,
    numeric: bool,
}

const fn case(
    language: Language,
    place: Place,
    text: &'static str,
    pointer: &'static str,
    numeric: bool,
) -> Template {
    Template {
        language,
        place,
        text,
        pointer,
        numeric,
    }
}

fn templates() -> Vec<Template> {
    use Place::{After, Before, Inside};
    let (yaml, json, toml, hcl, xml) = (
        Language::YAML,
        Language::JSON,
        Language::TOML,
        Language::HCL,
        Language::XML,
    );
    vec![
        // A key written twice on the lines before or after the secret.
        case(
            yaml,
            Before,
            "user: app\nuser: other\npassword: {S}\n",
            "/password",
            false,
        ),
        case(
            yaml,
            After,
            "password: {S}\nuser: app\nuser: other\n",
            "/password",
            false,
        ),
        case(yaml, After, "user: app\nuser: {S}\n", "/user", false),
        // The secret read as an anchor's name, a tag's value, a number.
        case(
            yaml,
            Inside,
            "user: app\npassword: *{S}\n",
            "/password",
            false,
        ),
        case(
            yaml,
            Inside,
            "user: app\npassword: !!int {S}\n",
            "/password",
            false,
        ),
        case(
            yaml,
            Inside,
            "user: app\npassword: {S}\n",
            "/password",
            true,
        ),
        case(
            json,
            Before,
            "{\"user\": 1,\n\"user\": 2,\n\"password\": \"{S}\"}\n",
            "/password",
            false,
        ),
        case(
            json,
            After,
            "{\"password\": \"{S}\",\n\"user\": 1,\n\"user\": 2}\n",
            "/password",
            false,
        ),
        case(
            json,
            After,
            "{\"password\": \"{S}\", \"port\": 99999999999999999999}\n",
            "/password",
            false,
        ),
        case(
            json,
            Inside,
            "{\"user\": 1,\n\"password\": {S}}\n",
            "/password",
            true,
        ),
        case(
            toml,
            Before,
            "port = 99999999999999999999\npassword = \"{S}\"\n",
            "/password",
            false,
        ),
        case(
            toml,
            After,
            "password = \"{S}\"\nport = 99999999999999999999\n",
            "/password",
            false,
        ),
        case(
            toml,
            Inside,
            "user = \"app\"\npassword = {S}\n",
            "/password",
            true,
        ),
        case(
            hcl,
            Before,
            "b = 1\nb {\n}\npassword = \"{S}\"\n",
            "/password",
            false,
        ),
        case(
            hcl,
            After,
            "password = \"{S}\"\nr \"t\" {\n}\nr \"t\" \"n\" {\n}\n",
            "/password",
            false,
        ),
        case(
            hcl,
            Inside,
            "user = \"app\"\npassword = <<{S}\n",
            "/password",
            false,
        ),
        case(
            hcl,
            Inside,
            "user = \"app\"\npassword = {S}\n",
            "/password",
            true,
        ),
        case(
            xml,
            Before,
            "<config>\n<a>\n</b>\n<password>{S}</password>\n</config>\n",
            "/config/password",
            false,
        ),
        case(
            xml,
            After,
            "<config>\n<password>{S}</password>\n<a>\n</b>\n</config>\n",
            "/config/password",
            false,
        ),
        case(
            xml,
            After,
            "<config>\n<password>{S}</password>\n<x:a/>\n</config>\n",
            "/config/password",
            false,
        ),
        case(
            xml,
            Inside,
            "<config>\n  <passwo\"rd>{S}</password>\n</config>\n",
            "/config/password",
            false,
        ),
        case(
            xml,
            Inside,
            "<config>\n<password>{S}</pass>\n</config>\n",
            "/config/password",
            false,
        ),
    ]
}

/// Secret values: ASCII, accents, Japanese, emoji, and a number.
const SECRETS: &[&str] = &[
    "s3cr3tZ",
    "pässé",
    "日本秘密",
    "🔑k🗝",
    "99999999999999999999",
];

/// Placeholders a host gives, each with the pieces no message may show.
const TOKENS: &[(&str, &[&str])] = &[
    ("«secret 0b1c»", &["0b1c", "«", "»"]),
    ("«secret é日本🔑»", &["é日本", "🔑", "«", "»"]),
];

/// A schema marking the field at `pointer` secret (`writeOnly`) or not.
fn schema(pointer: &str, secret: bool) -> crate::editor::schema::SchemaBundle {
    let mut field = serde_json::json!({});
    if secret {
        field["writeOnly"] = serde_json::Value::Bool(true);
    }
    for key in pointer.rsplit('/').filter(|key| !key.is_empty()) {
        field = serde_json::json!({"properties": {key: field}});
    }
    build_schema(&field.to_string()).expect("compiles")
}

/// The error's message as shown, by its quotes and by the re-read of a
/// language that gives none.
fn shown(
    language: Language,
    text: &str,
    schema: Option<&crate::editor::schema::SchemaBundle>,
    accepted: &[(String, String)],
) -> Option<(ParseError, String, String)> {
    let behavior = language.behavior();
    let error = behavior.parse(text).err()?;
    let placeholders = Placeholders {
        accepted,
        patterns: &[],
    };
    let new = syntax_message(text, &error, schema, behavior, placeholders);
    let unsaid = ParseError {
        quotes: None,
        ..error.clone()
    };
    let old = syntax_message(text, &unsaid, schema, behavior, placeholders);
    Some((error, new, old))
}

#[test]
fn no_syntax_message_shows_a_secret_and_other_quotes_read_as_before() {
    let mut failures = Vec::new();
    let mut quoted = std::collections::BTreeMap::<String, usize>::new();
    let mut changed = std::collections::BTreeSet::new();
    for template in templates() {
        let label = format!(
            "{} {:?} {:?}",
            template.language, template.place, template.text
        );
        let mut fillings: Vec<(String, Vec<String>, bool)> = SECRETS
            .iter()
            .filter(|secret| !template.numeric || secret.bytes().all(|byte| byte.is_ascii_digit()))
            .map(|secret| (secret.to_string(), vec![secret.to_string()], false))
            .collect();
        if !template.numeric {
            fillings.extend(TOKENS.iter().map(|(token, pieces)| {
                (
                    token.to_string(),
                    pieces.iter().map(|piece| piece.to_string()).collect(),
                    true,
                )
            }));
        }
        for (secret, pieces, placeholder) in fillings {
            let text = template.text.replace("{S}", &secret);
            let marked = schema(template.pointer, !placeholder);
            let accepted: Vec<(String, String)> = if placeholder {
                vec![(secret.clone(), template.pointer.to_string())]
            } else {
                Vec::new()
            };
            let Some((error, new, old)) = shown(template.language, &text, Some(&marked), &accepted)
            else {
                failures.push(format!("{label} with {secret:?}: reads"));
                continue;
            };
            let said = |message: &str| {
                format!(
                    "{label} with {secret:?}: {message:?} (plain {:?})",
                    error.message
                )
            };
            if let Some(piece) = pieces.iter().find(|piece| new.contains(piece.as_str())) {
                failures.push(format!("shows {piece:?}: {}", said(&new)));
            }
            if error.message.contains(secret.as_str()) {
                *quoted.entry(template.language.to_string()).or_default() += 1;
            }
            // A message holding no secret: as the re-read shows it where
            // that hides nothing, else every word the re-read shows is
            // still shown (the re-read hides the words a changed letter
            // reaches, `<a>` whole where the quote is `a`).
            let holds = pieces
                .iter()
                .any(|piece| error.message.contains(piece.as_str()));
            let unchanged = old == error.message;
            if !holds
                && (if unchanged {
                    new != old
                } else {
                    !shows_all(&new, &old)
                })
            {
                failures.push(format!(
                    "reads {new:?}, the re-read {old:?}: {}",
                    said(&new)
                ));
            }
            if new != old {
                changed.insert(format!("{label}: {old:?} -> {new:?}"));
            }
            // Each part not hidden reads as the parser wrote it.
            if !reads_as_written(&new, &error) {
                failures.push(format!("a part not hidden changed: {}", said(&new)));
            }
            // Without a secret marked, the message is the parser's.
            let unmarked = schema(template.pointer, false);
            if !placeholder
                && let Some((error, new, _)) = shown(template.language, &text, Some(&unmarked), &[])
                && new != error.message
            {
                failures.push(format!("changed without secrets: {}", said(&new)));
            }
        }
    }
    // Every language has a message quoting the secret itself.
    for language in ["YAML", "JSON", "TOML", "HCL"] {
        if quoted.get(language).copied().unwrap_or(0) == 0 {
            failures.push(format!(
                "{language}: no message quotes a secret ({quoted:?})"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    // What reads otherwise than the re-read showed it (more words kept).
    for line in changed {
        println!("{line}");
    }
}

/// An XML element pasted above a secret's, its end tag cut off: the
/// message names the open element as written and hides only the name
/// the reader stopped in (it reads on to the next `>`), never the secret.
/// The re-read hid every name: it changes the letters from the line on.
#[test]
fn an_xml_syntax_message_beside_a_secret_keeps_the_names_read_before_it() {
    let marked = schema("/db/password", true);
    let token = "«secret 1a1a»";
    let accepted = vec![(token.to_string(), "/db/password".to_string())];
    for (line, name) in [
        ("  <password>x</password\n", "password"),
        ("  <pass>x</pass\n", "pass"),
    ] {
        let text = format!("<db>\n{line}  <password>{token}</password>\n</db>\n");
        let (_, new, old) = shown(Language::XML, &text, Some(&marked), &accepted).expect("broken");
        let wanted =
            format!("</…> does not match the open <{name}>: close <{name}> first, or fix the name");
        assert_eq!(new, wanted, "{text:?} (the re-read: {old:?})");
        assert_ne!(old, wanted, "the re-read hides the names");
    }
}

/// Whether `new` shows every character `old` shows, in order (each
/// without its `…`).
fn shows_all(new: &str, old: &str) -> bool {
    let mut new = new.chars().filter(|character| *character != '…');
    old.chars()
        .filter(|character| *character != '…')
        .all(|wanted| new.any(|character| character == wanted))
}

/// Whether `shown` is `error`'s message with some of its quotes as `…`
/// and every other part as written.
fn reads_as_written(shown: &str, error: &ParseError) -> bool {
    let quotes = error.quotes.as_deref().unwrap_or_default();
    let mut said: Vec<std::ops::Range<usize>> =
        quotes.iter().map(|quote| quote.said.clone()).collect();
    said.sort_by_key(|range| range.start);
    // The words between the quotes, each found in order.
    let mut rest = shown;
    let mut at = 0;
    for range in said
        .iter()
        .chain(std::iter::once(&(error.message.len()..error.message.len())))
    {
        if range.start < at {
            continue;
        }
        let words = &error.message[at..range.start];
        let Some(found) = rest.strip_prefix(words) else {
            return false;
        };
        rest = found;
        if range.end > range.start {
            let quote = &error.message[range.clone()];
            rest = rest
                .strip_prefix(quote)
                .or_else(|| rest.strip_prefix('…'))
                .unwrap_or(rest);
        }
        at = range.end;
    }
    rest.is_empty()
}
