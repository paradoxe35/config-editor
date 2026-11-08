//! Items moved in an Edit-as view while edited, added or removed: the
//! round 22 cases, as reported (the property test in
//! `save_moves_properties` covers the class).

use super::Language;
use super::bridge::Bridge;
use super::codec;
use serde_json::Value;

/// `host_text` saved after its list at `pointer` is set to `list` in every
/// other view.
fn check(
    failures: &mut Vec<String>,
    name: &str,
    host: Language,
    host_text: &str,
    pointer: &str,
    list: &str,
    expected: &str,
) {
    for view in [
        Language::YAML,
        Language::JSON,
        Language::TOML,
        Language::HCL,
        Language::XML,
    ] {
        let Some(bridge) = Bridge::new(host, view, None) else {
            continue;
        };
        let shown = bridge.to_edit(host_text).unwrap().text;
        let mut value = codec::read(view.behavior(), &shown, None).unwrap();
        *value.pointer_mut(pointer).unwrap() = serde_json::from_str::<Value>(list).unwrap();
        let edited = codec::serialize(view.behavior(), &value, Some(&shown)).unwrap();
        match bridge.to_host(&edited, host_text) {
            Ok(saved) if saved.text == expected => {}
            Ok(saved) => failures.push(format!(
                "{name} via {}:\n--- expected\n{expected}--- saved\n{}",
                view.label(),
                saved.text
            )),
            Err(error) => failures.push(format!("{name} via {}: {error:?}", view.label())),
        }
    }
}

#[test]
fn comments_follow_items_moved_while_edited_added_or_removed() {
    let mut failures = Vec::new();
    // H22-E2: a moved item also edited.
    let yaml = "l:\n  # A\n  - n: a\n    p: 1\n  # B\n  - n: b\n    p: 2\n";
    check(
        &mut failures,
        "YAML maps",
        Language::YAML,
        yaml,
        "/l",
        r#"[{"n": "b", "p": 20}, {"n": "a", "p": 1}]"#,
        "l:\n  # B\n  - n: b\n    p: 20\n  # A\n  - n: a\n    p: 1\n",
    );
    let yaml = "l:\n  # A\n  - a\n  # B\n  - b\n  # C\n  - c\n";
    check(
        &mut failures,
        "YAML scalars",
        Language::YAML,
        yaml,
        "/l",
        r#"["c", "a", "bb"]"#,
        "l:\n  # C\n  - c\n  # A\n  - a\n  # B\n  - bb\n",
    );
    check(
        &mut failures,
        "YAML scalars, one added",
        Language::YAML,
        yaml,
        "/l",
        r#"["c", "new", "a", "bb"]"#,
        "l:\n  # C\n  - c\n  - new\n  # A\n  - a\n  # B\n  - bb\n",
    );
    // H22-E2 (XML) and M22-E2: repeated one-line elements with children.
    let xml = "<r>\n  <!-- A -->\n  <s n=\"a\"><p>1</p></s>\n  <!-- B -->\n  <s n=\"b\"><p>2</p></s>\n</r>\n";
    check(
        &mut failures,
        "XML edited and moved",
        Language::XML,
        xml,
        "/r/s",
        r#"[{"n": "b", "p": 20}, {"n": "a", "p": 1}]"#,
        "<r>\n  <!-- B -->\n  <s n=\"b\"><p>20</p></s>\n  <!-- A -->\n  <s n=\"a\"><p>1</p></s>\n</r>\n",
    );
    check(
        &mut failures,
        "XML moved, one added",
        Language::XML,
        xml,
        "/r/s",
        r#"[{"n": "b", "p": 2}, {"n": "c", "p": 3}, {"n": "a", "p": 1}]"#,
        "<r>\n  <!-- B -->\n  <s n=\"b\"><p>2</p></s>\n  <s n=\"c\"><p>3</p></s>\n  <!-- A -->\n  <s n=\"a\"><p>1</p></s>\n</r>\n",
    );
    let xml = "<r>\n  <!-- A -->\n  <s n=\"a\"><p>1</p></s>\n  <!-- B -->\n  <s n=\"b\"><p>2</p></s>\n  <s n=\"c\"/>\n</r>\n";
    check(
        &mut failures,
        "XML moved, one removed",
        Language::XML,
        xml,
        "/r/s",
        r#"[{"n": "c"}, {"n": "b", "p": 2}]"#,
        "<r>\n  <s n=\"c\"/>\n  <!-- B -->\n  <s n=\"b\"><p>2</p></s>\n</r>\n",
    );
    // M22-E1: TOML arrays of tables and HCL repeated blocks (aligned as
    // `terraform fmt` writes them), moved whole with their comments.
    let toml = "n = 1\n\n# A\n[[s]]\nn = \"a\"\np = 1 # pa\n\n# B\n[[s]]\nn = \"b\" # tb\n";
    check(
        &mut failures,
        "TOML tables",
        Language::TOML,
        toml,
        "/s",
        r#"[{"n": "b"}, {"n": "a", "p": 1}]"#,
        "n = 1\n\n# B\n[[s]]\nn = \"b\" # tb\n\n# A\n[[s]]\nn = \"a\"\np = 1 # pa\n",
    );
    check(
        &mut failures,
        "TOML tables, one added",
        Language::TOML,
        toml,
        "/s",
        r#"[{"n": "b"}, {"n": "c"}, {"n": "a", "p": 1}]"#,
        "n = 1\n\n# B\n[[s]]\nn = \"b\" # tb\n\n[[s]]\nn = \"c\"\n\n# A\n[[s]]\nn = \"a\"\np = 1 # pa\n",
    );
    let hcl = "n = 1\n\n# A\nsvc {\n  name = \"a\"\n  p    = 1 # pa\n}\n\n# B\nsvc {\n  name = \"b\"\n  p    = 2\n}\n";
    check(
        &mut failures,
        "HCL blocks",
        Language::HCL,
        hcl,
        "/svc",
        r#"[{"name": "b", "p": 2}, {"name": "a", "p": 1}]"#,
        "n = 1\n\n# B\nsvc {\n  name = \"b\"\n  p    = 2\n}\n\n# A\nsvc {\n  name = \"a\"\n  p    = 1 # pa\n}\n",
    );
    check(
        &mut failures,
        "HCL blocks, one added, one edited",
        Language::HCL,
        hcl,
        "/svc",
        r#"[{"name": "b", "p": 20}, {"name": "c", "p": 3}, {"name": "a", "p": 1}]"#,
        "n = 1\n\n# B\nsvc {\n  name = \"b\"\n  p    = 20\n}\n\nsvc {\n  name = \"c\"\n  p = 3\n}\n\n# A\nsvc {\n  name = \"a\"\n  p    = 1 # pa\n}\n",
    );
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
