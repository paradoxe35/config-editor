//! Saves over one edit session: a quoted string emptied, saved, then typed
//! again keeps the quotes the host wrote before the session.

use super::Language;
use super::bridge::Bridge;
use super::codec;
use super::save_matrix::{ALL, report, slot};
use serde_json::Value;

#[test]
fn a_quoted_string_emptied_saved_and_typed_again_keeps_its_quotes_from_every_view() {
    let hosts = [
        (
            Language::YAML,
            "s: \"abc\"  # c\nn: 1\n",
            "/s",
            "s: \"xyz\"  # c\nn: 1\n",
        ),
        (Language::YAML, "s: 'abc'\nn: 1\n", "/s", "s: 'xyz'\nn: 1\n"),
        (
            Language::YAML,
            "a:\n  b:\n    - s: \"abc\"  # c\n      n: 1\n",
            "/a/b/0/s",
            "a:\n  b:\n    - s: \"xyz\"  # c\n      n: 1\n",
        ),
        (
            Language::TOML,
            "s = 'abc'  # c\nn = 1\n",
            "/s",
            "s = 'xyz'  # c\nn = 1\n",
        ),
        (
            Language::XML,
            "<c a='abc' n='1'/>\n",
            "/c/a",
            "<c a='xyz' n='1'/>\n",
        ),
    ];
    let mut failures = Vec::new();
    for (host, origin, pointer, expected) in hosts {
        for edit in ALL.into_iter().filter(|edit| *edit != host) {
            let bridge = Bridge::new(host, edit, None).unwrap();
            let case = format!("{host} host via {edit}: {pointer}");
            // The view with the string at `pointer` set to `text`.
            let typed = |text: &str| -> Option<String> {
                let shown = bridge.to_edit(origin).ok()?.text;
                let mut value = codec::read(edit.behavior(), &shown, None).ok()?;
                *slot(&mut value, pointer) = Value::String(text.to_string());
                codec::serialize(edit.behavior(), &value, Some(&shown)).ok()
            };
            let (Some(emptied), Some(filled)) = (typed(""), typed("xyz")) else {
                failures.push(format!("{case}: no view"));
                continue;
            };
            // The emptied text saved when the typing pause ends.
            let saved = match bridge.to_host_since(&emptied, origin, origin) {
                Ok(saved) => saved.text,
                // A view that reads an empty value as no string waits.
                Err(_) => origin.to_string(),
            };
            match bridge.to_host_since(&filled, &saved, origin) {
                Ok(again) if again.text == expected => {}
                Ok(again) => failures.push(format!("{case}:\n{saved}--- then\n{}", again.text)),
                Err(error) => failures.push(format!("{case}: {error:?}")),
            }
        }
    }
    report(failures);
}
