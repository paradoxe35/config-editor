use super::{BaselineDecision, SentTexts, decide_baseline};

#[test]
fn a_new_baseline_asks_only_when_it_would_mix_with_unsaved_edits() {
    use BaselineDecision::{Conflict, Take, Unchanged};
    // (in use, new, dirty, new is the text, sent by the editor) → decision
    let cases = [
        ("a: 1\n", "a: 1\n", true, false, false, Unchanged),
        ("a: 1\n", "a: 1\n", false, true, true, Unchanged),
        // Nothing unsaved: taken silently, as before.
        ("a: 1\n", "a: 2\n", false, false, false, Take),
        // The same change on both sides.
        ("a: 1\n", "a: 2\n", true, true, false, Take),
        // A save coming back while the user typed on.
        ("a: 1\n", "a: 2\n", true, false, true, Take),
        // The file changed elsewhere under unsaved edits.
        ("a: 1\n", "a: 2\n", true, false, false, Conflict),
    ];
    let failures: Vec<String> = cases
        .iter()
        .filter_map(|&(in_use, new, dirty, matches, sent, expected)| {
            let found = decide_baseline(in_use, new, dirty, matches, sent);
            (found != expected).then(|| {
                format!("{in_use:?} -> {new:?} dirty={dirty} matches={matches} sent={sent}: expected {expected:?}, found {found:?}")
            })
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The tunnel sample in each language, with a comment where it has them.
#[cfg(all(
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
fn sample(language: super::super::languages::Language) -> &'static str {
    match language.label() {
        "JSON" => "{\n  \"listen\": 7835,\n  \"public_address\": \"tunnel.example.com\"\n}\n",
        "TOML" => "# server\nlisten = 7835\npublic_address = \"tunnel.example.com\"\n",
        "HCL" => "# server\nlisten = 7835\npublic_address = \"tunnel.example.com\"\n",
        "XML" => "<listen>7835</listen>\n<public_address>tunnel.example.com</public_address>\n",
        _ => "# server\nlisten: 7835\npublic_address: tunnel.example.com\n",
    }
}

/// For every pair of languages, as the editor computes dirty (the text on
/// screen against the baseline as shown): clean on load, dirty after an
/// edit, clean again once the host takes the saved text as its baseline, and
/// dirty against a baseline changed elsewhere.
#[test]
#[cfg(all(
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
fn dirty_follows_load_edit_and_save_in_every_language_pair() {
    use super::super::bridge::Bridge;
    use super::super::languages::Language;
    use super::shown_baseline;

    let mut failures = Vec::new();
    for host in Language::ALL.iter().copied() {
        for edit in Language::ALL.iter().copied() {
            let bridge = Bridge::new(host, edit, None);
            let host_text = sample(host);
            let shown = match &bridge {
                Some(bridge) => bridge.to_edit(host_text).unwrap().text,
                None => host_text.to_string(),
            };
            let dirty = |text: &str, baseline: &str, synced: (&str, &str)| {
                let converted = || match &bridge {
                    Some(bridge) => bridge.to_edit(baseline).unwrap().text,
                    None => baseline.to_string(),
                };
                text != shown_baseline(bridge.is_some(), baseline, synced, converted)
            };
            let synced = (host_text, shown.as_str());
            if dirty(&shown, host_text, synced) {
                failures.push(format!("{host} as {edit}: dirty on load"));
            }
            let edited = shown.replace("7835", "9000");
            if !dirty(&edited, host_text, synced) {
                failures.push(format!("{host} as {edit}: clean after an edit"));
            }
            // Saved: the host text the edit converts to becomes the baseline.
            let saved = match &bridge {
                Some(bridge) => bridge.to_host(&edited, host_text).unwrap(),
                None => edited.clone(),
            };
            if dirty(&edited, &saved, (&saved, &edited)) {
                failures.push(format!(
                    "{host} as {edit}: dirty after the baseline took the save {saved:?}"
                ));
            }
            let elsewhere = host_text.replace("7835", "7000");
            if !dirty(&edited, &elsewhere, (&saved, &edited)) {
                failures.push(format!(
                    "{host} as {edit}: clean against a baseline changed elsewhere"
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn sent_texts_are_recognized_and_bounded() {
    let mut sent = SentTexts::default();
    sent.note("a: 1\n");
    assert!(sent.contains("a: 1\n"));
    assert!(!sent.contains("a: 2\n"));
    for index in 0..100 {
        sent.note(&format!("n: {index}\n"));
    }
    assert!(!sent.contains("a: 1\n"), "the oldest is forgotten");
    assert!(sent.contains("n: 99\n"));
    assert!(sent.0.len() <= super::SENT_TEXTS_KEPT);
}
