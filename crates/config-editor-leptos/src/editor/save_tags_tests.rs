//! YAML tags (`!Sub`, `!GetAtt`, `!include`, `!secret`, `!reference`)
//! through Edit as: the data is the value under each tag, the host's text
//! keeps every tag through edits, and an edit that cannot keep one waits,
//! saying why. In every edit language.

use super::Language;
use super::bridge::Bridge;
use super::edit_as::SaveError;

const HOST: &str = "# stack\nres: !GetAtt [r, Arn]\nname: !Sub 'x-${y}'\nport: 1\ninc: !include\n  k: v\nkey: !secret pw\nrefs: !reference [.base, script]\n";

const TAGS: [&str; 6] = [
    "!GetAtt [r, Arn]",
    "!Sub",
    "!include",
    "!secret",
    "!reference",
    "port:",
];

fn edits() -> [Language; 4] {
    [Language::JSON, Language::TOML, Language::HCL, Language::XML]
}

#[test]
fn tagged_yaml_reads_as_the_values_under_its_tags() {
    let value = Language::YAML.parse(HOST).expect("valid YAML");
    assert_eq!(
        value,
        serde_json::json!({
            "res": ["r", "Arn"], "name": "x-${y}", "port": 1, "inc": {"k": "v"},
            "key": "pw", "refs": [".base", "script"]
        })
    );
}

#[test]
fn every_tag_is_kept_through_edits_in_every_language() {
    let mut failures = Vec::new();
    for edit in edits() {
        let bridge = Bridge::new(Language::YAML, edit, None).unwrap();
        let shown = match bridge.to_edit(HOST) {
            Ok(shown) => shown.text,
            Err(error) => {
                failures.push(format!("{edit}: not shown: {error}"));
                continue;
            }
        };
        for (change, from, to) in [
            ("another field", "1", "2"),
            ("a tagged scalar", "pw", "pw2"),
            ("a tagged list's item", "Arn", "Id"),
            ("a tagged map's field", ">v<", ">w<"),
        ] {
            let (bare_from, bare_to) = (from.trim_matches(['>', '<']), to.trim_matches(['>', '<']));
            let changed = if edit == Language::XML {
                shown.replacen(from, to, 1)
            } else {
                shown.replacen(bare_from, bare_to, 1)
            };
            match bridge.to_host(&changed, HOST) {
                Ok(saved) => {
                    let lost: Vec<&str> = TAGS
                        .iter()
                        .copied()
                        .filter(|tag| !saved.text.contains(tag.split(' ').next().unwrap()))
                        .collect();
                    if !lost.is_empty() {
                        failures.push(format!(
                            "{edit}, {change}: lost {lost:?} in {:?}",
                            saved.text
                        ));
                    }
                    let read = Language::YAML.parse(&saved.text).unwrap();
                    let expected = Language::YAML
                        .parse(&HOST.replacen(bare_from, bare_to, 1))
                        .unwrap();
                    if read != expected {
                        failures.push(format!("{edit}, {change}: saved {:?}", saved.text));
                    }
                }
                Err(error) => failures.push(format!("{edit}, {change}: waits: {error:?}")),
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn an_edit_that_cannot_keep_a_tag_waits_saying_why() {
    // The `!include`d map replaced by a number: the tag would be lost.
    let mut failures = Vec::new();
    for edit in edits() {
        let bridge = Bridge::new(Language::YAML, edit, None).unwrap();
        let shown = bridge.to_edit(HOST).unwrap().text;
        let changed = if edit == Language::JSON {
            shown.replace("\"inc\": {\n    \"k\": \"v\"\n  }", "\"inc\": 5")
        } else if edit == Language::TOML {
            shown
                .replace("\n[inc]\nk = \"v\"\n", "")
                .replace("port = 1", "port = 1\ninc = 5")
        } else if edit == Language::HCL {
            shown.replace("inc {\n  k = \"v\"\n}", "inc = 5")
        } else {
            shown.replace("<inc>\n  <k>v</k>\n</inc>", "<inc>5</inc>")
        };
        if changed == shown {
            failures.push(format!(
                "{edit}: the test's change does not apply to {shown:?}"
            ));
            continue;
        }
        match bridge.to_host(&changed, HOST) {
            Ok(saved) if saved.text.contains("!include") => {}
            Ok(saved) => failures.push(format!("{edit}: saved without the tag: {:?}", saved.text)),
            Err(error) => {
                let message = match &error {
                    SaveError::Convert(error) => error.to_string(),
                    other => format!("{other:?}"),
                };
                if !message.contains("!include") || error.pointer() != "/inc" {
                    failures.push(format!("{edit}: unclear reason: {message}"));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn edit_as_on_several_yaml_documents_is_refused_saying_why() {
    for edit in edits() {
        let bridge = Bridge::new(Language::YAML, edit, None).unwrap();
        let error = bridge
            .to_edit("a: 1\n---\nb: 2\n")
            .expect_err("several documents do not convert");
        let message = error.to_string();
        assert!(
            message.contains("more than one YAML document") && !message.contains("deserializ"),
            "{edit}: {message}"
        );
    }
}

/// M24-E3: tagged list items (flow and block maps, lists, scalars) moved
/// or removed from every view carry their tags and comments, as a pure
/// reorder or deletion can.
#[test]
fn tagged_items_moved_or_removed_keep_their_tags() {
    let mut failures = Vec::new();
    let documents = [
        (
            "l:\n  # A\n  - !A {x: 1}\n  # B\n  - !B {x: 2}\n",
            "l:\n  # B\n  - !B {x: 2}\n  # A\n  - !A {x: 1}\n",
            "l:\n  # B\n  - !B {x: 2}\n",
        ),
        (
            "l:\n  - !A\n    x: 1\n  - !B\n    x: 2\n",
            "l:\n  - !B\n    x: 2\n  - !A\n    x: 1\n",
            "l:\n  - !B\n    x: 2\n",
        ),
        (
            "l:\n  - !A [1, 2]\n  - !B [3]\n",
            "l:\n  - !B [3]\n  - !A [1, 2]\n",
            "l:\n  - !B [3]\n",
        ),
        (
            "l:\n  - !Ref a\n  - !Ref b\n",
            "l:\n  - !Ref b\n  - !Ref a\n",
            "l:\n  - !Ref b\n",
        ),
    ];
    type Change = fn(&mut serde_json::Value);
    for (host, reversed, first_removed) in documents {
        let cases: [(&str, Change, &str); 2] = [
            (
                "reversed",
                |value| value["l"].as_array_mut().unwrap().reverse(),
                reversed,
            ),
            (
                "first removed",
                |value| {
                    value["l"].as_array_mut().unwrap().remove(0);
                },
                first_removed,
            ),
        ];
        for (label, change, expected) in cases {
            // XML has no nested lists: its view of a list of lists is
            // another value (see `bridge::merge`).
            let views = edits()
                .into_iter()
                .filter(|view| *view != Language::XML || !host.contains("[1, 2]"));
            for view in views {
                match super::save_matrix::save_value(Language::YAML, view, host, &change) {
                    Ok(saved) if saved == expected => {}
                    Ok(saved) => failures.push(format!("{host:?} {label} via {view}: {saved:?}")),
                    Err(error) => failures.push(format!("{host:?} {label} via {view}: {error}")),
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// L6: a conversion to a language without tags lists every tagged value in
/// its losses (the text it writes reads back the same values, untagged).
#[test]
fn a_conversion_lists_the_tagged_values_among_its_losses() {
    let mut failures = Vec::new();
    for target in edits() {
        let losses = match Language::YAML.convert(HOST, target) {
            Ok(conversion) => conversion.losses,
            Err(error) => {
                failures.push(format!("{target}: {error}"));
                continue;
            }
        };
        for pointer in ["/res", "/name", "/inc", "/key", "/refs"] {
            if !losses.iter().any(|loss| loss == pointer) {
                failures.push(format!("{target}: {pointer} not in {losses:?}"));
            }
        }
        if losses.iter().any(|loss| loss == "/port") {
            failures.push(format!("{target}: the untagged /port in {losses:?}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
