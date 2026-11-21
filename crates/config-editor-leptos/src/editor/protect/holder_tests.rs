//! The holder query in every language and in the shapes XML makes of
//! repeats: each placeholder written in a text is held by the string that
//! holds it at that place, the k-th written by the k-th string holding it.

use serde_json::{Value, json};

use super::super::Language;
use super::super::secret_options::SecretOptions;
use super::holder::Holders;
use super::tokens::SecretMatcher;

const TOKEN: &str = "«secret 0b1c»";

/// Values holding the token: alone, beside text, repeated field, repeated
/// parent, a list in a list, copies, list items.
fn values() -> Vec<Value> {
    vec![
        json!({"db": {"user": "app", "password": TOKEN}, "port": 1}),
        json!({"db": {"password": format!("x{TOKEN}")}}),
        json!({"db": {"password": [TOKEN, "}"]}}),
        json!({"db": [{"password": TOKEN}, "2"]}),
        json!({"db": [{"password": [TOKEN, "}"]}, "2"]}),
        json!({"db": {"password": [TOKEN, TOKEN]}}),
        json!({"keys": [TOKEN, "other", TOKEN]}),
        json!({"a": {"b": {"c": TOKEN}}, "d": TOKEN}),
    ]
}

/// XML written by hand: attributes, an attribute's element copied, an
/// element given a child beside the token.
const XML: [&str; 4] = [
    "<config>\n  <db user=\"app\" password=\"«secret 0b1c»\"/>\n  <port>1</port>\n</config>\n",
    "<config>\n  <db user=\"app\" password=\"«secret 0b1c»\"/>\n  <db password=\"«secret 0b1c»\"/>\n</config>\n",
    "<config>\n  <db>\n    <password>«secret 0b1c»</password>\n    <password>}</password>\n  </db>\n  <db>2</db>\n</config>\n",
    "<config>\n  <db>\n    <password>\n      «secret 0b1c»\n    </password>\n  </db>\n</config>\n",
];

/// The pointers of the strings holding the token, in document order.
fn holding(value: &Value, at: String, out: &mut Vec<String>) {
    match value {
        Value::String(text) if text.contains(TOKEN) => out.push(at),
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                holding(item, format!("{at}/{index}"), out);
            }
        }
        Value::Object(map) => {
            for (key, item) in map {
                holding(item, format!("{at}/{key}"), out);
            }
        }
        _ => {}
    }
}

#[test]
fn each_written_placeholder_is_held_by_the_string_at_its_place_in_every_language() {
    let mut texts: Vec<(Language, String)> = Vec::new();
    for language in [
        Language::YAML,
        Language::JSON,
        Language::TOML,
        Language::HCL,
        Language::XML,
    ] {
        for value in values() {
            let Ok(text) = language.serialize(&value) else {
                continue;
            };
            texts.push((language, text));
        }
    }
    texts.extend(XML.iter().map(|text| (Language::XML, (*text).to_string())));
    let mut failures = Vec::new();
    let mut checked = 0;
    let matcher = SecretMatcher::new(&SecretOptions::default());
    for (language, text) in texts {
        let Ok(document) = language.parse(&text) else {
            failures.push(format!("{language}: does not parse: {text:?}"));
            continue;
        };
        let mut expected = Vec::new();
        holding(&document, String::new(), &mut expected);
        let written: Vec<_> = text.match_indices(TOKEN).map(|(at, _)| at).collect();
        if written.len() != expected.len() {
            // A language writing one string over several places cannot
            // be matched one to one; none here does.
            failures.push(format!(
                "{language}: {} written, {} held: {text:?}",
                written.len(),
                expected.len()
            ));
            continue;
        }
        let holders = Holders::read(language.behavior(), &text, &document, &matcher);
        let found: Vec<_> = holders.of(TOKEN).collect();
        if found.len() != expected.len() {
            failures.push(format!("{language}: {} found: {text:?}", found.len()));
            continue;
        }
        for ((at, pointer), found) in written.into_iter().zip(expected).zip(found) {
            checked += 1;
            if found.as_ref().map(|holder| holder.pointer.as_str()) != Some(pointer.as_str()) {
                failures.push(format!(
                    "{language}: the token at {at} held by {found:?}, not {pointer}: {text:?}"
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
    assert!(checked > 40, "{checked} placeholders checked");
}
