//! A host token copied into another field by a reference (a YAML alias,
//! a merge key) stands for another field's secret, as a pasted one does,
//! and is flagged the same way, on the field the copy is in; the host's
//! own copies (its text giving one secret to two fields by an alias) are
//! real. (An alias copying the key's token into `note` was not flagged.)

use super::super::Language;
use super::super::secret_options::SecretOptions;
use super::problems::unknown_placeholders;
use super::tokens::SecretMatcher;

const KEY: &str = "«secret 3f2a»";
const PIN: &str = "«secret 0b1c»";

/// A warning: its line and message.
type Flag = (Option<usize>, String);

/// The warnings on `text` (line and message) for the tokens the host
/// gave in `host`, both YAML.
fn flagged(host: &str, text: &str) -> Vec<Flag> {
    let yaml = Language::YAML.behavior();
    let matcher = SecretMatcher::new(&SecretOptions::default());
    let known = matcher.known(yaml, host);
    unknown_placeholders(
        yaml,
        text,
        &matcher,
        &known,
        &[],
        None,
        crate::strings::default_strings(),
    )
    .into_iter()
    .map(|problem| (problem.line, problem.message))
    .collect()
}

const HOST: &str = "id: srv-42\ntls:\n  key: «secret 3f2a»\n  pin: «secret 0b1c»\nnote: x\n";

#[test]
fn a_token_copied_by_an_alias_is_flagged_like_a_pasted_one() {
    let says = |pointer: &str| {
        super::texts::moved_placeholder(crate::strings::default_strings(), pointer, "/tls/key")
    };
    let anchored = HOST.replace(&format!("key: {KEY}"), &format!("key: &k {KEY}"));
    let cases: Vec<(&str, String, Vec<Flag>)> = vec![
        (
            "the key's token anchored, the note its alias",
            anchored.replace("note: x", "note: *k"),
            vec![(Some(5), says("/note"))],
        ),
        (
            "the key's token anchored, the pin its alias",
            anchored.replace(&format!("pin: {PIN}"), "pin: *k"),
            vec![(Some(4), says("/tls/pin"))],
        ),
        (
            "tls anchored, merged into another map",
            HOST.replace("tls:", "tls: &t") + "tls2:\n  <<: *t\n",
            vec![(Some(6), says("/tls2/key"))],
        ),
    ];
    let mut failures = Vec::new();
    for (case, text, expected) in cases {
        let found: Vec<_> = flagged(HOST, &text)
            .into_iter()
            .filter(|(_, message)| message.contains("/tls/key"))
            .collect();
        if found != expected {
            failures.push(format!("{case}: {found:?}, not {expected:?}\n{text}"));
        }
    }
    // The host's own alias gives the one secret to both fields: real.
    let host = format!("id: srv-42\nbase: &k {KEY}\ntls:\n  key: *k\n");
    let found = flagged(&host, &host);
    if !found.is_empty() {
        failures.push(format!("the host's alias: {found:?}"));
    }
    // Untouched, nothing is flagged.
    let found = flagged(HOST, &anchored);
    if !found.is_empty() {
        failures.push(format!("only anchored: {found:?}"));
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
