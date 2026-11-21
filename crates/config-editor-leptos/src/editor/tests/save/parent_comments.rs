//! P2: a comment above the line that opens a map belongs to the outermost
//! map that line opens (a TOML `[beta.x]` header opens `beta` where no
//! `[beta]` stands), and stays above whatever line the save writes for
//! that map when a key under it is removed, renamed or added; it goes only
//! with the map.

use crate::editor::Language;
use crate::editor::bridge::Bridge;
use crate::editor::codec;
use serde_json::{Value, json};

/// A host text with `# note` above the line opening `beta` (which holds
/// only `x`, holding `k3`).
fn host(language: Language) -> &'static str {
    match language.label() {
        "TOML" => "[alpha]\nk1 = 1\n# note\n[beta.x]\nk3 = 1\n",
        "YAML" => "alpha:\n  k1: 1\n# note\nbeta:\n  x:\n    k3: 1\n",
        "HCL" => "alpha {\n  k1 = 1\n}\n# note\nbeta {\n  x {\n    k3 = 1\n  }\n}\n",
        other => panic!("no host text for {other}"),
    }
}

/// The edits of `beta`, by name.
fn edits() -> Vec<(&'static str, Value)> {
    vec![
        ("x removed, its keys lifted", json!({"k3": 1})),
        ("x renamed", json!({"y": {"k3": 1}})),
        ("a key added beside x", json!({"x": {"k3": 1}, "k4": 2})),
        ("x emptied of k3, k4 added", json!({"x": {"k4": 2}})),
    ]
}

#[test]
fn a_comment_above_a_map_s_opening_line_stays_when_a_key_under_it_changes() {
    let mut failures = Vec::new();
    for host_language in [Language::TOML, Language::YAML, Language::HCL] {
        let text = host(host_language);
        for view in [Language::JSON, Language::YAML] {
            let Some(bridge) = Bridge::new(host_language, view, None) else {
                continue;
            };
            let shown = bridge.to_edit(text).unwrap().text;
            for (name, beta) in edits() {
                let mut value = codec::read(view.behavior(), &shown, None).unwrap();
                value["beta"] = beta;
                let edited = codec::serialize(view.behavior(), &value, Some(&shown)).unwrap();
                // Right above the line opening `beta`.
                let above_beta = |saved: &str| {
                    let mut lines = saved.lines().skip_while(|line| *line != "# note");
                    lines.next().is_some()
                        && lines
                            .next()
                            .is_some_and(|line| line.trim_start_matches('[').starts_with("beta"))
                };
                match bridge.to_host(&edited, text) {
                    Ok(saved) if above_beta(&saved) => {}
                    other => failures.push(format!(
                        "{} via {}, {name}: {other:?}",
                        host_language.label(),
                        view.label()
                    )),
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
