//! Suggestions at places only JSON's syntax has (a key's quotes), against
//! the tunnel schema. The suggestion promises every language shares are in
//! `completion_matrix`.
//!
//! `|` marks the caret.

use super::super::Language;
use super::super::harness::completion::{accepted, offered, parsed};
use super::super::harness::report;

/// JSON types in front of a key like the other languages: at a key's
/// opening quote (`"|tcp"`) or before it (`hea|"tcp"`), a new entry goes
/// before it on its own line, and the key is not offered.
#[test]
fn a_json_key_typed_in_front_of_a_key_goes_before_it_on_its_own_line() {
    let mut failures = Vec::new();
    let marked = "{\n  \"|tcp\": {}\n}\n";
    if offered(Language::JSON, marked).contains(&"tcp".to_string()) {
        failures.push(format!("{marked:?} offers tcp"));
    }
    for (marked, label, expected) in [
        (
            "{\n  \"|tcp\": {}\n}\n",
            "clients",
            "{\n  \"clients\": {},\n  \"tcp\": {}\n}\n",
        ),
        (
            "{\n  hea|\"tcp\": {}\n}\n",
            "heartbeat",
            "{\n  \"heartbeat\": {},\n  \"tcp\": {}\n}\n",
        ),
    ] {
        match accepted(Language::JSON, marked, label) {
            Ok(text)
                if parsed(Language::JSON, &text).is_ok()
                    && text.contains(&expected[4..expected.find(",").unwrap()])
                    && text.contains(",\n  \"tcp\"") => {}
            Ok(text) => failures.push(format!("{marked:?} → {text:?}")),
            Err(error) => failures.push(format!("{marked:?} → {error}")),
        }
    }
    report(&failures);
}
