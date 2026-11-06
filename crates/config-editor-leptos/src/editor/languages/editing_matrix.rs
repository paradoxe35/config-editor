//! Editing commands checked in every language on the same kinds of text, so
//! a fix for one language cannot leave its siblings behind. Each test lists
//! every failing case at once.

use super::Language;

const ALL: [Language; 5] = [
    Language::YAML,
    Language::JSON,
    Language::TOML,
    Language::HCL,
    Language::XML,
];

fn large(language: Language) -> &'static str {
    match language.label() {
        "YAML" => include_str!("../../../tests/fixtures/samples/burrow.large.yaml"),
        "JSON" => include_str!("../../../tests/fixtures/samples/burrow.large.json"),
        "TOML" => include_str!("../../../tests/fixtures/samples/burrow.large.toml"),
        "HCL" => include_str!("../../../tests/fixtures/samples/burrow.large.hcl"),
        _ => include_str!("../../../tests/fixtures/samples/burrow.large.xml"),
    }
}

/// Small documents holding a comment line between code lines.
fn with_comment(language: Language) -> &'static str {
    match language.label() {
        "YAML" => "listen: 1\n# note\npool:\n  min_idle: 2\n",
        "JSON" => "{\n  \"listen\": 1\n}\n",
        "TOML" => "listen = 1\n# note\n[pool]\nmin_idle = 2\n",
        "HCL" => "listen = 1\n# note\npool {\n  min_idle = 2\n}\n",
        _ => {
            "<listen>1</listen>\n<!-- note -->\n<pool>\n  <min_idle>2</min_idle>\n</pool>\n<host>a -- b</host>\n"
        }
    }
}

/// Ctrl+/ over a selection twice gives the text back, and the commented
/// text is still a valid document (XML comments cannot nest or hold `--`).
#[test]
fn toggling_comments_twice_restores_the_selection() {
    let mut failures = Vec::new();
    for language in ALL {
        for (name, text) in [
            ("small", with_comment(language)),
            ("large", large(language)),
        ] {
            let behavior = language.behavior();
            let all = text.encode_utf16().count();
            let Some((once, start, end)) = behavior.toggle_comments(text, 0, all) else {
                continue;
            };
            if language.label() == "XML" && language.parse(&once).is_err() {
                failures.push(format!(
                    "XML {name}: commented text does not parse:\n{once}"
                ));
            }
            match behavior.toggle_comments(&once, start, end) {
                Some((twice, _, _)) if twice == text => {}
                Some((twice, _, _)) => {
                    failures.push(format!(
                        "{} {name}: toggled twice gives\n{twice}\ninstead of\n{text}",
                        language.label()
                    ));
                }
                None => failures.push(format!(
                    "{} {name}: the second toggle does nothing",
                    language.label()
                )),
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} failing case(s):\n{}",
        failures.len(),
        failures.join("\n---\n")
    );
}
