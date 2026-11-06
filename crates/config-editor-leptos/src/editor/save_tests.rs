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
    // The last dotted key deleted leaves its table, empty; the rest stays.
    let saved = save(Language::TOML, Language::YAML, toml, |shown| {
        shown.replace("tcp:\n  nodelay: true\n", "tcp: {}\n")
    });
    assert!(saved.contains("tcp = {}"), "{saved}");
    assert_eq!(
        saved.replace("tcp = {}\n", ""),
        toml.replace("tcp.nodelay = true # t\n", "")
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
