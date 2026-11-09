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
        // Only JSON of the views shows a top-level list.
        let shown = match bridge.to_edit(host_text) {
            Ok(shown) => shown.text,
            Err(_) if pointer.is_empty() && view.label() != "JSON" => continue,
            Err(error) => panic!("{name} via {}: {error:?}", view.label()),
        };
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

/// H23-E1: a list that starts the document. The comments right above its
/// first item are the item's (they move and go with it); a file's header
/// is the comments a blank line ends, and stays.
#[test]
fn comments_follow_the_items_of_a_list_that_starts_the_document() {
    let mut failures = Vec::new();
    let play = "# Install nginx\n- hosts: web\n  become: true\n\n# Deploy the app\n- hosts: app\n  serial: 2\n";
    let swapped = r#"[{"hosts": "app", "serial": 2}, {"hosts": "web", "become": true}]"#;
    let first_gone = r#"[{"hosts": "app", "serial": 2}]"#;
    let cases: [(&str, Language, String, &str, &str, String); 12] = [
        (
            "playbook moved",
            Language::YAML,
            play.to_string(),
            "",
            swapped,
            "# Deploy the app\n- hosts: app\n  serial: 2\n\n# Install nginx\n- hosts: web\n  become: true\n".to_string(),
        ),
        (
            "playbook, first play removed",
            Language::YAML,
            play.to_string(),
            "",
            first_gone,
            "# Deploy the app\n- hosts: app\n  serial: 2\n".to_string(),
        ),
        (
            "playbook after ---, moved",
            Language::YAML,
            format!("---\n{play}"),
            "",
            swapped,
            "---\n# Deploy the app\n- hosts: app\n  serial: 2\n\n# Install nginx\n- hosts: web\n  become: true\n".to_string(),
        ),
        (
            "playbook with a header, moved",
            Language::YAML,
            format!("# playbook\n\n{play}"),
            "",
            swapped,
            "# playbook\n\n# Deploy the app\n- hosts: app\n  serial: 2\n\n# Install nginx\n- hosts: web\n  become: true\n".to_string(),
        ),
        (
            "playbook with a header, first play removed",
            Language::YAML,
            format!("# playbook\n\n{play}"),
            "",
            first_gone,
            "# playbook\n\n# Deploy the app\n- hosts: app\n  serial: 2\n".to_string(),
        ),
        (
            "scalars moved",
            Language::YAML,
            "# A\n- a\n# B\n- b\n".to_string(),
            "",
            r#"["b", "a"]"#,
            "# B\n- b\n# A\n- a\n".to_string(),
        ),
        (
            "scalars, first removed",
            Language::YAML,
            "# A\n- a\n# B\n- b\n".to_string(),
            "",
            r#"["b"]"#,
            "# B\n- b\n".to_string(),
        ),
        (
            "TOML tables moved",
            Language::TOML,
            "# A\n[[s]]\nn = \"a\"\n\n# B\n[[s]]\nn = \"b\"\n".to_string(),
            "/s",
            r#"[{"n": "b"}, {"n": "a"}]"#,
            "# B\n[[s]]\nn = \"b\"\n\n# A\n[[s]]\nn = \"a\"\n".to_string(),
        ),
        (
            "TOML tables, first removed",
            Language::TOML,
            "# A\n[[s]]\nn = \"a\"\n\n# B\n[[s]]\nn = \"b\"\n".to_string(),
            "/s",
            r#"[{"n": "b"}]"#,
            "# B\n[[s]]\nn = \"b\"\n".to_string(),
        ),
        (
            "HCL blocks moved",
            Language::HCL,
            "# A\ns {\n  n = \"a\"\n}\n\n# B\ns {\n  n = \"b\"\n}\n".to_string(),
            "/s",
            r#"[{"n": "b"}, {"n": "a"}]"#,
            "# B\ns {\n  n = \"b\"\n}\n\n# A\ns {\n  n = \"a\"\n}\n".to_string(),
        ),
        (
            "HCL blocks, first removed",
            Language::HCL,
            "# A\ns {\n  n = \"a\"\n}\n\n# B\ns {\n  n = \"b\"\n}\n".to_string(),
            "/s",
            r#"[{"n": "b"}]"#,
            "# B\ns {\n  n = \"b\"\n}\n".to_string(),
        ),
        (
            "HCL blocks with a header, first removed",
            Language::HCL,
            "# head\n\n# A\ns {\n  n = \"a\"\n}\n\n# B\ns {\n  n = \"b\"\n}\n".to_string(),
            "/s",
            r#"[{"n": "b"}]"#,
            "# head\n\n# B\ns {\n  n = \"b\"\n}\n".to_string(),
        ),
    ];
    for (name, host, text, pointer, list, expected) in cases {
        check(&mut failures, name, host, &text, pointer, list, &expected);
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// M23-E2: every repeated HCL block or TOML `[[table]]` removed (the list
/// emptied) goes with its comments, as XML's repeated elements do: no
/// `tag = []` attribute for a block type, no comment left behind.
#[test]
fn emptying_a_list_written_as_blocks_or_tables_removes_them() {
    let mut failures = Vec::new();
    let cases: [(&str, Language, &str, &str, &str); 5] = [
        (
            "HCL blocks",
            Language::HCL,
            "x = 1\n\n# T1\ntag {\n  k = \"a\"\n}\n# T2\ntag {\n  k = \"b\"\n}\n",
            "/tag",
            "x = 1\n",
        ),
        (
            "HCL blocks in a block",
            Language::HCL,
            "resource \"x\" \"y\" {\n  name = \"a\" # n\n  port = 80\n\n  # T1\n  tag {\n    k = \"a\"\n  }\n\n  # T2\n  tag {\n    k = \"b\"\n  }\n}\n",
            "/resource/x/y/tag",
            "resource \"x\" \"y\" {\n  name = \"a\" # n\n  port = 80\n}\n",
        ),
        (
            "HCL blocks before an attribute",
            Language::HCL,
            "# T1\ntag {\n  k = \"a\"\n}\n\n# T2\ntag {\n  k = \"b\"\n}\n\nx = 1 # x\n",
            "/tag",
            "x = 1 # x\n",
        ),
        (
            "TOML tables",
            Language::TOML,
            "x = 1\n\n# A\n[[s]]\nn = 1\n",
            "/s",
            "x = 1\n",
        ),
        (
            "TOML tables before a table",
            Language::TOML,
            "x = 1\n\n# A\n[[s]]\nn = 1\n\n# B\n[[s]]\nn = 2\n\n# T\n[t]\ny = 2\n",
            "/s",
            "x = 1\n\n# T\n[t]\ny = 2\n",
        ),
    ];
    for (name, host, text, pointer, expected) in cases {
        check(&mut failures, name, host, text, pointer, "[]", expected);
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// M24-E1: every labelled HCL block removed (the map of its labels
/// emptied, at any of its labels) goes with its comments, as unlabelled
/// ones do: no `svc {}` or `resource "aws_s3" {}` left, which Terraform
/// refuses.
#[test]
fn emptying_the_labels_of_blocks_removes_them() {
    let mut failures = Vec::new();
    let services = "x = 1\n\n# A\nsvc \"a\" {\n  p = 1\n}\n\n# B\nsvc \"b\" {\n  p = 2\n}\n";
    let resource = "# R\nresource \"aws_s3\" \"b\" {\n  acl = \"x\"\n}\n\nx = 1\n";
    let nested = "top {\n  x = 1\n\n  # A\n  svc \"a\" {\n    p = 1\n  }\n}\n";
    let cases = [
        ("one label", services, "/svc", "x = 1\n"),
        ("two labels", resource, "/resource", "x = 1\n"),
        ("the inner label", resource, "/resource/aws_s3", "x = 1\n"),
        ("in a block", nested, "/top/svc", "top {\n  x = 1\n}\n"),
    ];
    for (name, text, pointer, expected) in cases {
        check(
            &mut failures,
            name,
            Language::HCL,
            text,
            pointer,
            "{}",
            expected,
        );
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
