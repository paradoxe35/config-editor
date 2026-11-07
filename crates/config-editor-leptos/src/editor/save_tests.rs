//! Edit-as saves that must change only the edited lines of the host.

use super::Language;
use super::bridge::Bridge;

/// The host text saved after `change` is made to the shown text.
fn save(
    host: Language,
    edit: Language,
    host_text: &str,
    change: impl Fn(&str) -> String,
) -> String {
    let bridge = Bridge::new(host, edit, None).unwrap();
    let shown = bridge.to_edit(host_text).unwrap().text;
    let changed = change(&shown);
    assert_ne!(changed, shown, "the change applies to:\n{shown}");
    bridge.to_host(&changed, host_text).unwrap().text
}

#[test]
fn deleting_a_toml_inline_table_removes_only_its_line() {
    let toml = "# Burrow config\nlisten = 1 # bind\nheartbeat = { interval_secs = 15 }\npublic_address = \"x\" # addr\n";
    let saved = save(Language::TOML, Language::YAML, toml, |shown| {
        shown.replace("heartbeat:\n  interval_secs: 15\n", "")
    });
    assert_eq!(
        saved,
        "# Burrow config\nlisten = 1 # bind\npublic_address = \"x\" # addr\n"
    );
}

#[test]
fn inserting_next_to_a_yaml_flow_sequence_keeps_it_and_the_comments() {
    let yaml = "# Burrow config\nlisten: 1 # bind\nself:\n  allow: [a, b] # cidrs\nlimits:\n  max_pending: 512\n";
    let saved = save(Language::YAML, Language::JSON, yaml, |shown| {
        shown.replace(
            "  \"limits\"",
            "  \"insecure_plaintext\": true,\n  \"limits\"",
        )
    });
    assert_eq!(
        saved,
        "# Burrow config\nlisten: 1 # bind\nself:\n  allow: [a, b] # cidrs\ninsecure_plaintext: true\nlimits:\n  max_pending: 512\n"
    );
}

#[test]
fn deleting_a_block_after_an_hcl_inline_array_keeps_the_rest() {
    let hcl = "# head\nself {\n  allow = [\"10.0.0.0/8\"] # lan\n}\n\npool {\n  min_idle = 1\n}\n";
    let saved = save(Language::HCL, Language::YAML, hcl, |shown| {
        shown.replace("pool:\n  min_idle: 1\n", "")
    });
    assert_eq!(
        saved,
        "# head\nself {\n  allow = [\"10.0.0.0/8\"] # lan\n}\n"
    );
}

#[test]
fn a_flow_list_replaced_by_a_scalar_changes_only_that_line() {
    let yaml = "# head\nclients:\n  acme:\n    secret: [k1, k2] # keys\nlisten: 1 # port\n";
    let saved = save(Language::YAML, Language::JSON, yaml, |shown| {
        shown.replace(
            "\"secret\": [\n        \"k1\",\n        \"k2\"\n      ]",
            "\"secret\": \"only\"",
        )
    });
    assert!(
        saved.starts_with("# head\nclients:\n  acme:\n    secret: only"),
        "{saved}"
    );
    assert!(saved.ends_with("listen: 1 # port\n"), "{saved}");
}

#[test]
fn a_json_edit_keeps_an_unrelated_one_line_object() {
    let json = "{\n  \"listen\": 1,\n  \"heartbeat\": {\"interval_secs\": 15},\n  \"pool\": {\n    \"min_idle\": 1\n  }\n}\n";
    let saved = save(Language::JSON, Language::YAML, json, |shown| {
        shown.replace("pool:\n  min_idle: 1\n", "")
    });
    assert_eq!(
        saved,
        "{\n  \"listen\": 1,\n  \"heartbeat\": {\"interval_secs\": 15}\n}\n"
    );
}

#[test]
fn deleting_a_middle_hcl_block_leaves_the_block_above_whole() {
    let hcl = "heartbeat {\n  interval_secs = 1\n}\n\npool {\n  min_idle = 1\n}\n\nlimits {\n  max_pending = 5\n}\n";
    let saved = save(Language::HCL, Language::YAML, hcl, |shown| {
        shown.replace("pool:\n  min_idle: 1\n", "")
    });
    assert_eq!(
        saved,
        "heartbeat {\n  interval_secs = 1\n}\n\nlimits {\n  max_pending = 5\n}\n"
    );
}

#[test]
fn deleting_a_section_between_blank_lines_leaves_one_blank_line() {
    let toml = "listen = 1\n\n[pool]\nmin_idle = 1\n\n[limits]\nmax_pending = 5\n";
    let saved = save(Language::TOML, Language::YAML, toml, |shown| {
        shown.replace("pool:\n  min_idle: 1\n", "")
    });
    assert_eq!(saved, "listen = 1\n\n[limits]\nmax_pending = 5\n");
    let saved = save(Language::TOML, Language::YAML, toml, |shown| {
        shown.replace("limits:\n  max_pending: 5\n", "")
    });
    assert_eq!(saved, "listen = 1\n\n[pool]\nmin_idle = 1\n");
}

#[test]
fn structural_edits_on_an_xml_element_written_with_attributes_keep_the_host() {
    let xml = "<!-- t -->\n<listen>1</listen> <!-- l -->\n<clients>\n  <acme secret=\"x\"/>\n</clients>\n";
    let saved = save(Language::XML, Language::YAML, xml, |shown| {
        shown.replace("    secret: x\n", "    secret: x\n    max_connections: 3\n")
    });
    assert_eq!(
        saved,
        "<!-- t -->\n<listen>1</listen> <!-- l -->\n<clients>\n  <acme secret=\"x\" max_connections=\"3\"/>\n</clients>\n"
    );
    let pool = "<!-- t -->\n<pool min_idle=\"1\" max_idle=\"2\"/>\n<listen>1</listen>\n";
    let saved = save(Language::XML, Language::YAML, pool, |shown| {
        shown.replace("  max_idle: 2\n", "")
    });
    assert_eq!(
        saved,
        "<!-- t -->\n<pool min_idle=\"1\"/>\n<listen>1</listen>\n"
    );
    let heartbeat = "<!-- t -->\n<heartbeat interval_secs=\"15\"/>\n<listen>1</listen>\n";
    let saved = save(Language::XML, Language::YAML, heartbeat, |shown| {
        shown.replace("heartbeat:\n  interval_secs: 15\n", "")
    });
    assert_eq!(saved, "<!-- t -->\n<listen>1</listen>\n");
}

#[test]
fn renaming_an_entry_and_editing_its_value_keeps_its_comment_and_place() {
    let toml = "listen = 1\n\n# acme secret\n[clients.acme]\nsecret = \"s\"\nmax_connections = 1\n\n# beta\n[clients.beta]\nsecret = \"b\"\n";
    let saved = save(Language::TOML, Language::YAML, toml, |shown| {
        shown
            .replace("  acme:\n", "  acme2:\n")
            .replace("max_connections: 1", "max_connections: 2")
    });
    assert_eq!(
        saved,
        "listen = 1\n\n# acme secret\n[clients.acme2]\nsecret = \"s\"\nmax_connections = 2\n\n# beta\n[clients.beta]\nsecret = \"b\"\n"
    );
}

#[test]
fn renaming_an_xml_entry_and_editing_its_value_keeps_its_comment_and_place() {
    let xml = "<clients>\n  <!-- acme secret -->\n  <acme>\n    <secret>s</secret>\n    <max_connections>1</max_connections>\n  </acme>\n  <beta>\n    <secret>b</secret>\n  </beta>\n</clients>\n";
    let saved = save(Language::XML, Language::YAML, xml, |shown| {
        shown
            .replace("  acme:\n", "  acme2:\n")
            .replace("max_connections: 1", "max_connections: 2")
    });
    assert_eq!(
        saved,
        "<clients>\n  <!-- acme secret -->\n  <acme2>\n    <secret>s</secret>\n    <max_connections>2</max_connections>\n  </acme2>\n  <beta>\n    <secret>b</secret>\n  </beta>\n</clients>\n"
    );
}

#[test]
fn deleting_a_one_line_xml_element_removes_only_its_line() {
    let xml = "<!-- head -->\n<listen>1</listen> <!-- port -->\n<heartbeat><interval_secs>15</interval_secs></heartbeat>\n<public_address>x</public_address>\n";
    let saved = save(Language::XML, Language::YAML, xml, |shown| {
        shown.replace("heartbeat:\n  interval_secs: 15\n", "")
    });
    assert_eq!(
        saved,
        "<!-- head -->\n<listen>1</listen> <!-- port -->\n<public_address>x</public_address>\n"
    );
}

#[test]
fn deleting_a_client_of_the_large_hcl_sample_removes_only_its_block() {
    let hcl = include_str!("../../tests/fixtures/samples/burrow.large.hcl");
    let block =
        "  etiolles-pve-ai {\n    secret = \"«secret 705fde3f»\"\n    max_connections = 264\n  }\n";
    assert!(hcl.contains(block));
    let saved = save(Language::HCL, Language::YAML, hcl, |shown| {
        let start = shown
            .find("\n  etiolles-pve-ai:\n")
            .expect("the client in YAML")
            + 1;
        let end = start + shown[start..].find("\n  etiolles-pve-stagging:").unwrap() + 1;
        format!("{}{}", &shown[..start], &shown[end..])
    });
    assert_eq!(saved, hcl.replacen(block, "", 1));
}

#[test]
fn a_one_line_hcl_block_keeps_the_host_when_deleted_or_extended() {
    let hcl = "# c\ntcp { nodelay = true }\nlisten = \"a\" # p\n";
    let saved = save(Language::HCL, Language::YAML, hcl, |shown| {
        shown.replace("tcp:\n  nodelay: true\n", "")
    });
    assert_eq!(saved, "# c\nlisten = \"a\" # p\n");
    let saved = save(Language::HCL, Language::YAML, hcl, |shown| {
        shown.replace(
            "  nodelay: true\n",
            "  nodelay: true\n  keepalive_secs: 3\n",
        )
    });
    assert_eq!(
        saved,
        "# c\ntcp {\n  nodelay = true\n  keepalive_secs = 3\n}\nlisten = \"a\" # p\n"
    );
    let nested = "# c\nclients {\n  # a\n  acme { secret = \"x\" }\n  # b\n  beta {\n    secret = \"y\"\n  }\n}\n";
    let saved = save(Language::HCL, Language::YAML, nested, |shown| {
        shown.replace("  acme:\n    secret: x\n", "")
    });
    assert_eq!(
        saved,
        "# c\nclients {\n  # b\n  beta {\n    secret = \"y\"\n  }\n}\n"
    );
}

#[test]
fn toml_saves_keep_dotted_keys_inline_tables_and_comments() {
    let toml = "# h\nlisten = 1 # port\nlimits.max_pending = 1 # mp\ntcp.nodelay = true # t\n\n[visitors.a]\nsecret = \"s\"\nreach = {}\nadmin_from = [\"x\"]\n\n[pool]\nmin_idle = 1\n";
    // Renaming a dotted prefix keeps the line and its comment.
    let saved = save(Language::TOML, Language::YAML, toml, |shown| {
        shown.replace("tcp:", "udp:")
    });
    assert_eq!(saved, toml.replace("tcp.nodelay", "udp.nodelay"));
    // A scalar becoming a table is written in its place.
    let saved = save(Language::TOML, Language::YAML, toml, |shown| {
        shown.replace("listen: 1\n", "listen:\n  port: 1\n")
    });
    assert_eq!(
        saved,
        toml.replace("listen = 1 # port", "listen = { port = 1 } # port")
    );
    // An edit elsewhere leaves the inline tables of other tables in place.
    let saved = save(Language::TOML, Language::YAML, toml, |shown| {
        shown.replace("  min_idle: 1\n", "  min_idle: 1\n  max_idle: 2\n")
    });
    assert_eq!(
        saved,
        toml.replace("min_idle = 1\n", "min_idle = 1\nmax_idle = 2\n")
    );
    // The last dotted key deleted leaves its table, empty, in its place.
    let saved = save(Language::TOML, Language::YAML, toml, |shown| {
        shown.replace("tcp:\n  nodelay: true\n", "tcp: {}\n")
    });
    assert_eq!(
        saved,
        toml.replace("tcp.nodelay = true # t", "tcp = {} # t")
    );
}

#[test]
fn renaming_an_xml_element_keeps_its_attributes_and_comments() {
    let xml =
        "<clients>\n  <!-- acme -->\n  <acme secret=\"x\" max_connections=\"3\"/>\n</clients>\n";
    let saved = save(Language::XML, Language::YAML, xml, |shown| {
        shown.replace("acme:", "acme2:")
    });
    assert_eq!(saved, xml.replace("<acme ", "<acme2 "));
    let saved = save(Language::XML, Language::YAML, xml, |shown| {
        shown.replace("clients:", "visitors:")
    });
    assert_eq!(saved, xml.replace("clients>", "visitors>"));
}

#[test]
fn hcl_saves_follow_the_hosts_blank_lines() {
    let packed =
        "clients {\n  acme {\n    secret = \"a\"\n  }\n  beta {\n    secret = \"b\"\n  }\n}\n";
    let saved = save(Language::HCL, Language::YAML, packed, |shown| {
        format!("{shown}  gamma:\n    secret: c\n")
    });
    assert_eq!(
        saved,
        packed.replace("  }\n}\n", "  }\n  gamma {\n    secret = \"c\"\n  }\n}\n")
    );
    let spaced =
        "clients {\n  acme {\n    secret = \"a\"\n  }\n\n  beta {\n    secret = \"b\"\n  }\n}\n";
    let saved = save(Language::HCL, Language::YAML, spaced, |shown| {
        shown.replace("  beta:\n    secret: b\n", "")
    });
    assert_eq!(saved, "clients {\n  acme {\n    secret = \"a\"\n  }\n}\n");
}

#[test]
fn a_multi_line_string_under_a_dotted_key_keeps_its_form_and_the_comments() {
    let toml = "# c\nweb.error_pages = \"\"\"\nmulti\nline\"\"\"\nlisten = 1 # l\n";
    let saved = save(Language::TOML, Language::YAML, toml, |shown| {
        shown.replace("multi", "MULTI")
    });
    assert_eq!(saved, toml.replace("multi", "MULTI"));
    let saved = save(Language::TOML, Language::YAML, toml, |shown| {
        shown.replace("listen: 1", "listen: 2")
    });
    assert_eq!(saved, toml.replace("listen = 1", "listen = 2"));
}

/// QA round 8 Edit-as findings, as reported.
#[test]
fn round_8_saves_change_only_the_edit() {
    let mut failures = Vec::new();
    let mut check = |name: &str,
                     host: Language,
                     edit: Language,
                     text: &str,
                     change: &dyn Fn(&str) -> String,
                     expected: &str| {
        let bridge = Bridge::new(host, edit, None).unwrap();
        let shown = bridge.to_edit(text).unwrap().text;
        let changed = change(&shown);
        if changed == shown {
            failures.push(format!("{name}: the change does not apply to\n{shown}"));
            return;
        }
        let saved = bridge.to_host(&changed, text).unwrap().text;
        if saved != expected {
            failures.push(format!(
                "{name}:\n--- expected\n{expected}--- saved\n{saved}"
            ));
        }
    };
    // H1: a one-line block deleted, its next sibling holding the same value.
    let hcl = "# top\nlisten = 1\n\n# tcp opts\ntcp { nodelay = true } # inline\n\n# udp\nudp {\n  nodelay = true\n}\n";
    check(
        "H1",
        Language::HCL,
        Language::YAML,
        hcl,
        &|s| s.replace("tcp:\n  nodelay: true\n", ""),
        "# top\nlisten = 1\n\n# udp\nudp {\n  nodelay = true\n}\n",
    );
    // H2: a block-style sibling equal to a one-line container stays block style.
    let yaml = "# c\nlisten: 1\ntcp: {nodelay: true}\nudp:\n  nodelay: true # u\n";
    check(
        "H2",
        Language::YAML,
        Language::TOML,
        yaml,
        &|s| format!("insecure_plaintext = true\n{s}"),
        "# c\ninsecure_plaintext: true\nlisten: 1\ntcp: {nodelay: true}\nudp:\n  nodelay: true # u\n",
    );
    // H3: a comment in an untouched block whose lists equal other blocks'.
    let nested = "x {\n  s {\n    secret = \"a\"\n  }\n  n {\n    v = [\"p\"]\n  }\n  m {\n    c = [\"e\"]\n  }\n  w {\n    c = [\"e\"]\n    # note\n    v = [\"p\"]\n  }\n}\n";
    check(
        "H3",
        Language::HCL,
        Language::YAML,
        nested,
        &|s| s.replace("    secret: a\n", "    secret: a\n    max_connections: 7\n"),
        &nested.replace(
            "    secret = \"a\"\n",
            "    secret = \"a\"\n    max_connections = 7\n",
        ),
    );
    // H4: an item added to a multi-line list.
    let toml = "# c\nallow = [\n  \"x\", # x\n  \"y\",\n]\nlisten = 1 # l\n";
    check(
        "H4 add",
        Language::TOML,
        Language::YAML,
        toml,
        &|s| s.replace("  - y\n", "  - y\n  - z\n"),
        "# c\nallow = [\n  \"x\", # x\n  \"y\",\n  \"z\",\n]\nlisten = 1 # l\n",
    );
    check(
        "H4 delete",
        Language::TOML,
        Language::YAML,
        toml,
        &|s| s.replace("  - x\n", ""),
        "# c\nallow = [\n  \"y\",\n]\nlisten = 1 # l\n",
    );
    // H6: dotted prefixes renamed everywhere, three segments deep too.
    let dotted = "web.http = 80 # h\nweb.https = 443 # s\nlisten = 1 # l\n";
    check(
        "H6 shared",
        Language::TOML,
        Language::YAML,
        dotted,
        &|s| s.replace("web:", "www:"),
        &dotted.replace("web.", "www."),
    );
    let deep = "# top\nservices.web.health_check.interval_secs = 2 # h\nservices.web.target = 80 # t\n\n[pool]\nmin_idle = 1 # m\n";
    check(
        "H6 deep",
        Language::TOML,
        Language::YAML,
        deep,
        &|s| s.replace("  web:", "  www:"),
        &deep.replace("services.web.", "services.www."),
    );
    // H7: a table renamed in the TOML view keeps its place.
    let blocks = "# c\ntcp {\n  nodelay = true # n\n}\nlisten = 1 # l\n";
    check(
        "H7",
        Language::HCL,
        Language::TOML,
        blocks,
        &|s| s.replace("[tcp]", "[pool]"),
        &blocks.replace("tcp {", "pool {"),
    );
    // MEDIUM: a deleted one-line block takes its own comment with it.
    let comment = "# tcp\ntcp { nodelay = true }\n# pool\npool {\n  min_idle = 1\n}\n";
    check(
        "M comment",
        Language::HCL,
        Language::YAML,
        comment,
        &|s| s.replace("tcp:\n  nodelay: true\n", ""),
        "# pool\npool {\n  min_idle = 1\n}\n",
    );
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// QA round 9 Edit-as findings, as reported.
#[test]
fn round_9_saves_change_only_the_edit() {
    let mut failures = Vec::new();
    let mut check = |name: &str,
                     host: Language,
                     edit: Language,
                     text: &str,
                     change: &dyn Fn(&str) -> String,
                     expected: &str| {
        let bridge = Bridge::new(host, edit, None).unwrap();
        let shown = bridge.to_edit(text).unwrap().text;
        let changed = change(&shown);
        if changed == shown {
            failures.push(format!("{name}: the change does not apply to\n{shown}"));
            return;
        }
        let saved = bridge.to_host(&changed, text).unwrap().text;
        if saved != expected {
            failures.push(format!(
                "{name}:\n--- expected\n{expected}--- saved\n{saved}"
            ));
        }
    };
    // H1: an own-line comment inside a multi-line list.
    let toml = "# c\nlisten = 1\n\n[self]\nallow = [\n  \"127.0.0.1/32\",\n  # private\n  \"10.0.0.0/8\"\n]\n# tail\n";
    check(
        "H1 add",
        Language::TOML,
        Language::YAML,
        toml,
        &|s| {
            s.replace(
                "    - 10.0.0.0/8\n",
                "    - 10.0.0.0/8\n    - 192.168.0.0/16\n",
            )
        },
        "# c\nlisten = 1\n\n[self]\nallow = [\n  \"127.0.0.1/32\",\n  # private\n  \"10.0.0.0/8\",\n  \"192.168.0.0/16\"\n]\n# tail\n",
    );
    check(
        "H1 delete",
        Language::TOML,
        Language::YAML,
        toml,
        &|s| s.replace("    - 127.0.0.1/32\n", ""),
        "# c\nlisten = 1\n\n[self]\nallow = [\n  # private\n  \"10.0.0.0/8\"\n]\n# tail\n",
    );
    // H2: a dotted key deleted with another key after its group.
    let dotted = "# top\nweb.http = \"a\"\nweb.https = \"b\"\nlisten = 1 # l\n";
    check(
        "H2 one",
        Language::TOML,
        Language::YAML,
        dotted,
        &|s| s.replace("  http: a\n", ""),
        "# top\nweb.https = \"b\"\nlisten = 1 # l\n",
    );
    check(
        "H2 group",
        Language::TOML,
        Language::YAML,
        dotted,
        &|s| s.replace("web:\n  http: a\n  https: b\n", ""),
        "# top\nlisten = 1 # l\n",
    );
    let deep = "# top\nservices.api.target.address = \"x\"\nservices.api.target.port = 1\nservices.api.mode = \"tcp\" # m\n";
    check(
        "H2 deep",
        Language::TOML,
        Language::YAML,
        deep,
        &|s| s.replace("      port: 1\n", ""),
        "# top\nservices.api.target.address = \"x\"\nservices.api.mode = \"tcp\" # m\n",
    );
    // H3: a dotted multi-line string deleted.
    let string = "# top\nweb.hsts = true # s\nweb.error_pages = \"\"\"\n<h1>x</h1>\n\"\"\"\n";
    check(
        "H3",
        Language::TOML,
        Language::YAML,
        string,
        &|s| {
            let start = s.find("  error_pages:").unwrap();
            s[..start].to_string()
        },
        "# top\nweb.hsts = true # s\n",
    );
    // M4: a compact JSON host keeps its spacing.
    let compact = "{\"listen\":1,\"tcp\":{\"nodelay\":true}}\n";
    check(
        "M4",
        Language::JSON,
        Language::YAML,
        compact,
        &|s| s.replace("listen: 1\n", "listen: 1\ninsecure_plaintext: true\n"),
        "{\"listen\":1,\"insecure_plaintext\":true,\"tcp\":{\"nodelay\":true}}\n",
    );
    // LOW: deleting the first table with its own comment leaves no blank line.
    let first = "# tcp settings\n[tcp]\nnodelay = true\n\n[udp]\nmax_peers = 1\n";
    check(
        "first table",
        Language::TOML,
        Language::YAML,
        first,
        &|s| s.replace("tcp:\n  nodelay: true\n", ""),
        "[udp]\nmax_peers = 1\n",
    );
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn round_10_saves_change_only_the_edit() {
    use serde_json::{Value, json};
    let mut failures = Vec::new();
    // The edit is made on the value (through a JSON view); the host must
    // change only where the value did.
    let mut check = |name: &str, host: Language, text: &str, change: &dyn Fn(&mut Value), expected: &str| {
        let bridge = Bridge::new(host, Language::JSON, None).unwrap();
        let shown = bridge.to_edit(text).unwrap().text;
        let mut value: Value = serde_json::from_str(&shown).unwrap();
        change(&mut value);
        let changed = serde_json::to_string_pretty(&value).unwrap();
        match bridge.to_host(&changed, text) {
            Ok(saved) if saved.text == expected => {}
            Ok(saved) => failures.push(format!("{name}:\n--- expected\n{expected}--- saved\n{}\n{:?}", saved.text, saved.text)),
            Err(error) => failures.push(format!("{name}: {error:?}")),
        }
    };
    let object = |value: &mut Value, path: &[&str]| -> serde_json::Map<String, Value> {
        let mut at = &*value;
        for key in path {
            at = &at[*key];
        }
        at.as_object().unwrap().clone()
    };
    let set = |value: &mut Value, path: &[&str], map: serde_json::Map<String, Value>| {
        let mut at = value;
        for key in path {
            at = &mut at[*key];
        }
        *at = Value::Object(map);
    };
    // H1: a TOML table with its own header, next to dotted keys of its parent.
    let toml = "# head\nlisten = 1\nservices.api.target = 81\n\n[services.web]\ntarget = 80 # t\nclient = \"acme\" # c\n";
    check("H1 add", Language::TOML, toml, &|v| v["services"]["web"]["bind"] = json!(5),
        "# head\nlisten = 1\nservices.api.target = 81\n\n[services.web]\ntarget = 80 # t\nclient = \"acme\" # c\nbind = 5\n");
    check("H1 delete", Language::TOML, toml, &|v| {
        let mut web = object(v, &["services", "web"]);
        web.remove("client");
        set(v, &["services", "web"], web);
    }, "# head\nlisten = 1\nservices.api.target = 81\n\n[services.web]\ntarget = 80 # t\n");
    // H2: one-line labelled HCL blocks.
    let hcl = "# head\nlisten = 1 # l\nclients \"beta\" { secret = \"s2\" }\nudp = { max_peers = 3 }\n";
    check("H2 add", Language::HCL, hcl, &|v| v["clients"]["beta"]["max_connections"] = json!(2),
        "# head\nlisten = 1 # l\nclients \"beta\" {\n  secret = \"s2\"\n  max_connections = 2\n}\nudp = { max_peers = 3 }\n");
    let hcl = "# h\nclients \"a\" { secret = \"s\" }\nclients \"b\" { secret = \"t\" }\nlisten = 1 # l\n";
    let drop_a = |v: &mut Value| {
        let mut clients = object(v, &["clients"]);
        clients.remove("a");
        set(v, &["clients"], clients);
    };
    check("H2 delete", Language::HCL, hcl, &drop_a, "# h\nclients \"b\" { secret = \"t\" }\nlisten = 1 # l\n");
    let hcl = "clients \"a\" { secret = \"s\" } # a\nclients \"b\" {\n  secret = \"t\" # b\n}\n";
    check("H2 delete before multi-line", Language::HCL, hcl, &drop_a, "clients \"b\" {\n  secret = \"t\" # b\n}\n");
    // H4: YAML scalars and flow collections keep their style.
    let yaml = "allow:\n  - a/8 # A\n  - 'c/8'\n  - \"e/8\"\nsecret: >-\n  s1\nhealth_check: {path: /h, interval_secs: 3}\nflow: [ 'a/8', \"b/8\" ]   # flow\n";
    check("H4 plain", Language::YAML, yaml, &|v| v["allow"][0] = json!("b/8"),
        &yaml.replace("- a/8 # A", "- b/8 # A"));
    check("H4 single", Language::YAML, yaml, &|v| v["allow"][1] = json!("d/8"),
        &yaml.replace("'c/8'", "'d/8'"));
    check("H4 block", Language::YAML, yaml, &|v| v["secret"] = json!("s1x"),
        &yaml.replace("  s1\n", "  s1x\n"));
    check("H4 flow add", Language::YAML, yaml, &|v| v["health_check"]["failures"] = json!(2),
        &yaml.replace("interval_secs: 3}", "interval_secs: 3, failures: 2}"));
    check("H4 flow delete", Language::YAML, yaml, &|v| {
        let mut health = object(v, &["health_check"]);
        health.remove("interval_secs");
        set(v, &["health_check"], health);
    }, &yaml.replace("{path: /h, interval_secs: 3}", "{path: /h}"));
    check("H4 flow append", Language::YAML, yaml, &|v| v["flow"].as_array_mut().unwrap().push(json!("c/8")),
        &yaml.replace("\"b/8\" ]", "\"b/8\", \"c/8\" ]"));
    check("H4 block delete", Language::YAML, yaml, &|v| {
        v.as_object_mut().unwrap().remove("secret");
    }, &yaml.replace("secret: >-\n  s1\n", ""));
    let hcl = "clients \"alpha\" { secret = \"s1\" } # a\nclients \"beta\" {\n  secret = \"s2\" # b\n}\n";
    check("H2 rename", Language::HCL, hcl, &|v| {
        let clients = v.as_object_mut().unwrap().remove("clients").unwrap();
        v["peers"] = clients;
    }, &hcl.replace("clients", "peers"));
    // M1: a header of several comment lines above the first entry keeps
    // the lines that do not name the entry.
    for (host, text, expected) in [
        (Language::YAML, "# Header\n# about tcp\ntcp:\n  nodelay: true\nlisten: 1\n", "# Header\nlisten: 1\n"),
        (Language::YAML, "# Burrow\n# settings\ntcp:\n  nodelay: true\nlisten: 1\n", "# Burrow\n# settings\nlisten: 1\n"),
        (Language::TOML, "# Header\n# about tcp\n[tcp]\nnodelay = true\n\n[udp]\nmax_peers = 1\n", "# Header\n\n[udp]\nmax_peers = 1\n"),
        (Language::HCL, "# Header\n# about tcp\ntcp {\n  nodelay = true\n}\nlisten = 1\n", "# Header\nlisten = 1\n"),
    ] {
        check(&format!("M1 {host:?}"), host, text, &|v| {
            v.as_object_mut().unwrap().remove("tcp");
        }, expected);
    }
    // M2: a reorder takes each item's comments along.
    for (host, text, expected) in [
        (Language::YAML, "allow:\n  - 127.0.0.1/32 # loopback\n  # private\n  - 10.0.0.0/8\n", "allow:\n  # private\n  - 10.0.0.0/8\n  - 127.0.0.1/32 # loopback\n"),
        (Language::TOML, "allow = [\n  \"127.0.0.1/32\", # loopback\n  \"10.0.0.0/8\",\n]\n", "allow = [\n  \"10.0.0.0/8\",\n  \"127.0.0.1/32\", # loopback\n]\n"),
        (Language::HCL, "allow = [\n  \"127.0.0.1/32\", # loopback\n  \"10.0.0.0/8\",\n]\n", "allow = [\n  \"10.0.0.0/8\",\n  \"127.0.0.1/32\", # loopback\n]\n"),
    ] {
        check(&format!("M2 {host:?}"), host, text, &|v| v["allow"].as_array_mut().unwrap().reverse(), expected);
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn round_11_saves_change_only_the_edit() {
    use serde_json::{Value, json};
    let mut failures = Vec::new();
    // The edits are made on the value (through a JSON view), all before one
    // save; the host must change only where the value did.
    let mut check = |name: &str, host: Language, text: &str, change: &dyn Fn(&mut Value), expected: &str| {
        // JSON text is YAML too: a JSON host is edited through a YAML view.
        let edit = if host == Language::JSON { Language::YAML } else { Language::JSON };
        let bridge = Bridge::new(host, edit, None).unwrap();
        let shown = bridge.to_edit(text).unwrap().text;
        let mut value: Value = edit.parse(&shown).unwrap();
        change(&mut value);
        let changed = serde_json::to_string_pretty(&value).unwrap();
        match bridge.to_host(&changed, text) {
            Ok(saved) if saved.text == expected => {}
            Ok(saved) => failures.push(format!("{name}:\n--- expected\n{expected}--- saved\n{}\n{:?}", saved.text, saved.text)),
            Err(error) => failures.push(format!("{name}: {error:?}")),
        }
    };
    let remove = |value: &mut Value, path: &[&str], key: &str| {
        let mut at = value;
        for segment in path {
            at = &mut at[*segment];
        }
        let map = at.as_object_mut().unwrap();
        let rebuilt: serde_json::Map<String, Value> = std::mem::take(map).into_iter().filter(|(name, _)| name != key).collect();
        *map = rebuilt;
    };
    // H1: YAML hosts indented other than by two spaces.
    check("H1 indentless", Language::YAML, "# head\nlisten: 1 # port\nself:\n  allow:\n  - a\n",
        &|v| v["self"]["allow"].as_array_mut().unwrap().push(json!("b")),
        "# head\nlisten: 1 # port\nself:\n  allow:\n  - a\n  - b\n");
    check("H1 four-space map", Language::YAML, "# h\npool:\n    min_idle: 4\n\"listen\": 1\n",
        &|v| v["pool"]["max_idle"] = json!(1),
        "# h\npool:\n    min_idle: 4\n    max_idle: 1\n\"listen\": 1\n");
    check("H1 four-space list", Language::YAML, "# h\nallow:\n    - a\n",
        &|v| v["allow"].as_array_mut().unwrap().push(json!("b")),
        "# h\nallow:\n    - a\n    - b\n");
    check("H1 spaced dashes", Language::YAML, "# h\nallow:\n-   a\n",
        &|v| v["allow"].as_array_mut().unwrap().push(json!("b")),
        "# h\nallow:\n-   a\n-   b\n");
    // H2: two edits in one save, one of them structural.
    check("H2 flow kept", Language::YAML, "listen: 1\nudp: {max_peers: 2}\npool:\n  min_idle: 1\n",
        &|v| {
            v["udp"]["max_peers"] = json!(3);
            v["pool"]["max_idle"] = json!(1);
        },
        "listen: 1\nudp: {max_peers: 3}\npool:\n  min_idle: 1\n  max_idle: 1\n");
    check("H2 delete and edit", Language::YAML, "listen: 1 # l\nudp: {max_peers: 2}\n",
        &|v| {
            remove(v, &[], "listen");
            v["udp"]["max_peers"] = json!(3);
        },
        "udp: {max_peers: 3}\n");
    // H3: a list append and another edit in one save.
    check("H3 TOML", Language::TOML, "# head\nlisten = 1 # l\nudp = { max_peers = 2 }\n\n[self]\nallow = [\n  \"a\", # only\n]\n",
        &|v| {
            v["listen"] = json!(2);
            v["self"]["allow"].as_array_mut().unwrap().push(json!("b"));
        },
        "# head\nlisten = 2 # l\nudp = { max_peers = 2 }\n\n[self]\nallow = [\n  \"a\", # only\n  \"b\",\n]\n");
    check("H3 HCL", Language::HCL, "# head\nlisten = 1 # l\nudp = { max_peers = 2 }\n\nself {\n  allow = [\n    \"a\", # only\n  ]\n}\n",
        &|v| {
            v["listen"] = json!(2);
            v["self"]["allow"].as_array_mut().unwrap().push(json!("b"));
        },
        "# head\nlisten = 2 # l\nudp = { max_peers = 2 }\n\nself {\n  allow = [\n    \"a\", # only\n    \"b\",\n  ]\n}\n");
    // H4: a multi-line string in a table under a parent with dotted keys.
    let toml = "listen = 1\nservices.web.target = 80\n\n[services.db]\nhealth_check.path = '''\n/lit\n'''\nclient = \"acme\"\n";
    check("H4", Language::TOML, toml, &|v| v["listen"] = json!(2), &toml.replace("listen = 1", "listen = 2"));
    // H5, H6: XML attribute values that need escaping.
    let xml = "<!-- head -->\n<beta secret=\"s2\" max_connections=\"2\"></beta>\n<!-- tail -->\n";
    for (value, written) in [("a&b", "a&amp;b"), ("a\"b", "a&quot;b"), ("a<b", "a&lt;b"), ("a\nb", "a&#10;b")] {
        check(&format!("H5 {value:?}"), Language::XML, xml, &|v| v["beta"]["secret"] = json!(value),
            &xml.replace("secret=\"s2\"", &format!("secret=\"{written}\"")));
    }
    // H7: XML text ending in a line break.
    let xml = "<!-- c -->\n<beta>\n  <secret>k1\nk2\n</secret>\n</beta>\n<listen>1</listen>\n";
    check("H7 edit", Language::XML, xml, &|v| v["beta"]["secret"] = json!("k3"),
        "<!-- c -->\n<beta>\n  <secret>k3\n</secret>\n</beta>\n<listen>1</listen>\n");
    check("H7 delete", Language::XML, xml, &|v| remove(v, &[], "beta"), "<!-- c -->\n<listen>1</listen>\n");
    // MEDIUM: an HCL heredoc keeps its form; a YAML flow list its spacing.
    check("M heredoc", Language::HCL, "# c\nsecret = <<EOT\nabc\nEOT\nlisten = 1\n", &|v| v["secret"] = json!("abd\n"),
        "# c\nsecret = <<EOT\nabd\nEOT\nlisten = 1\n");
    let yaml = "allow: [ 10.0.0.0/8 ,\"192.168.0.0/16\",   '172.16.0.0/12' ]\n";
    check("M flow spacing", Language::YAML, yaml, &|v| v["allow"].as_array_mut().unwrap().push(json!("1.1.1.1/32")),
        "allow: [ 10.0.0.0/8 ,\"192.168.0.0/16\",   '172.16.0.0/12',   '1.1.1.1/32' ]\n");
    // LOW: compact JSON stays compact.
    check("L compact", Language::JSON, "{\"tcp\":{\"nodelay\":true}}\n", &|v| v["tcp"]["keepalive"] = json!({"secs": 1}),
        "{\"tcp\":{\"nodelay\":true,\"keepalive\":{\"secs\":1}}}\n");
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// Round 11: XML element text decodes character references, as attribute
/// values do, and a null an XML host cannot hold is refused, not saved as an
/// empty element.
#[test]
fn xml_text_decodes_references_and_refuses_null() {
    let value = Language::XML.parse("<secret>a&#10;b&#65;&#x42;</secret>\n").unwrap();
    assert_eq!(value["secret"], "a\nbAB");
    let bridge = Bridge::new(Language::XML, Language::YAML, None).unwrap();
    let host = "<public_address>x</public_address>\n<listen>1</listen>\n";
    assert!(bridge.to_host("public_address: null\nlisten: 1\n", host).is_err());
}

/// Round 11: `<` in an XML attribute value is not well-formed XML, so it is
/// a syntax error, never read (or saved) as if it were.
#[test]
fn a_raw_less_than_in_an_xml_attribute_is_a_syntax_error() {
    assert!(Language::XML.parse("<beta secret=\"a<b\"/>\n").is_err());
    assert!(Language::XML.parse("<beta secret=\"a&lt;b\"/>\n").is_ok());
}

#[test]
fn round_12_saves_change_only_the_edit() {
    use serde_json::{Value, json};
    let mut failures = Vec::new();
    let mut check = |name: &str, host: Language, edit: Language, text: &str, change: &dyn Fn(&mut Value), expected: &str| {
        let bridge = Bridge::new(host, edit, None).unwrap();
        let shown = bridge.to_edit(text).unwrap().text;
        let mut value: Value = edit.parse(&shown).unwrap();
        change(&mut value);
        let changed = edit.behavior().serialize(&value, Some(&shown)).unwrap();
        match bridge.to_host(&changed, text) {
            Ok(saved) if saved.text == expected => {}
            Ok(saved) => failures.push(format!("{name}:\n--- expected\n{expected}--- saved\n{}\n{:?}", saved.text, saved.text)),
            Err(error) => failures.push(format!("{name}: {error:?}")),
        }
    };
    // H1: anchors and aliases; an alias whose value parts from its anchor's
    // is written out, everything else stays.
    let yaml = "pool: &p\n  min_idle: 1\nudp: *p\nlisten: 1 # keep\n";
    check("H1 anchor edited", Language::YAML, Language::JSON, yaml, &|v| v["pool"]["min_idle"] = json!(5),
        "pool: &p\n  min_idle: 5\nudp: {min_idle: 1}\nlisten: 1 # keep\n");
    check("H1 alias edited", Language::YAML, Language::JSON, yaml, &|v| v["udp"]["min_idle"] = json!(7),
        "pool: &p\n  min_idle: 1\nudp: {min_idle: 7}\nlisten: 1 # keep\n");
    let yaml = "# shared\nclients:\n  acme: &client\n    secret: s1\n    max_connections: 2\n  gamma:\n    <<: *client\n    secret: s3\n";
    check("H1 merge, anchor edited", Language::YAML, Language::JSON, yaml, &|v| v["clients"]["acme"]["max_connections"] = json!(5),
        "# shared\nclients:\n  acme: &client\n    secret: s1\n    max_connections: 5\n  gamma:\n    max_connections: 2\n    secret: s3\n");
    check("H1 merge, other edit", Language::YAML, Language::JSON, &format!("{yaml}listen: 1\n"), &|v| v["listen"] = json!(2),
        &format!("{yaml}listen: 2\n"));
    // H2: a YAML empty value filled.
    check("H2 filled", Language::YAML, Language::JSON, "listen: 1 # l\nservices:\n", &|v| v["services"] = json!({"web": {"target": 80}}),
        "listen: 1 # l\nservices:\n  web:\n    target: 80\n");
    check("H2 next comment", Language::YAML, Language::JSON, "services:\nvisitors: {} # none yet\n", &|v| v["services"] = json!({"web": 1}),
        "services:\n  web: 1\nvisitors: {} # none yet\n");
    // H3: values the edit language cannot show exactly stay as written.
    let yaml = "listen: 0x10\nudp: {max_peers: 1}\nheartbeat:\n  interval_secs: 1.5e1\n";
    for edit in [Language::XML, Language::HCL] {
        check(&format!("H3 {edit:?}"), Language::YAML, edit, yaml, &|v| v["pool"] = json!({"min_idle": 1}),
            &format!("{yaml}pool:\n  min_idle: 1\n"));
    }
    // H4: a TOML datetime stays a datetime.
    check("H4", Language::TOML, Language::JSON, "started = 1979-05-27T07:32:00Z # when\n", &|v| v["started"] = json!("1980-01-01T00:00:00Z"),
        "started = 1980-01-01T00:00:00Z # when\n");
    // H5, M6: a YAML flow map over lines keeps its form and comments.
    let yaml = "tcp: {\n  nodelay: true,   # a\n  keepalive_secs: 30 # b\n}\nlisten: 1\n";
    check("H5 add", Language::YAML, Language::JSON, yaml, &|v| v["tcp"]["nodelay_x"] = json!(1),
        "tcp: {\n  nodelay: true,   # a\n  keepalive_secs: 30, # b\n  nodelay_x: 1\n}\nlisten: 1\n");
    check("M6 edit", Language::YAML, Language::JSON, yaml, &|v| v["tcp"]["keepalive_secs"] = json!(31),
        &yaml.replace("keepalive_secs: 30", "keepalive_secs: 31"));
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// Round 12: YAML merge keys (`<<: *defaults`) merge the anchored map in,
/// keys written beside them winning.
#[test]
fn yaml_merge_keys_merge_the_anchored_map() {
    let value = Language::YAML.parse("base: &b\n  a: 1\n  b: 2\nmine:\n  <<: *b\n  b: 3\n").unwrap();
    assert_eq!(value["mine"], serde_json::json!({"a": 1, "b": 3}));
}
