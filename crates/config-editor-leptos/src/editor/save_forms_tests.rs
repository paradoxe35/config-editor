//! Edit-as saves that keep how the host wrote a value: block scalars, long
//! strings and heredocs, XML text with its whitespace and CDATA, string
//! escapes, empty blocks over lines, anchors and merge keys. Each test edits
//! the value from every other language's view and lists every case that
//! fails.

use super::Language;
use super::bridge::Bridge;
use super::save_matrix::{ALL, from_every_view, report, save_value};
use serde_json::Value;

#[test]
fn a_block_scalar_edited_in_any_view_stays_a_block() {
    let yaml = "# c\nscript: |\n  line one\n    indented\n  line three\nkeep: |-\n  a\n  b\nfolded: >-\n  folded text\n  continues here\nz: 1 # z\n";
    let mut failures = Vec::new();
    from_every_view(
        &mut failures,
        "literal",
        Language::YAML,
        yaml,
        &|v| v["script"] = "line one\n  indented\nline 3\n".into(),
        &yaml.replace("  line three\n", "  line 3\n"),
    );
    from_every_view(
        &mut failures,
        "literal without its final line break",
        Language::YAML,
        yaml,
        &|v| v["script"] = "line one\n  indented\nline 3".into(),
        &yaml
            .replace("script: |\n", "script: |-\n")
            .replace("  line three\n", "  line 3\n"),
    );
    from_every_view(
        &mut failures,
        "strip chomping kept",
        Language::YAML,
        yaml,
        &|v| v["keep"] = "a\nc".into(),
        &yaml.replace("  a\n  b\n", "  a\n  c\n"),
    );
    // Folded text keeps the host's line break where the words around it
    // stayed.
    from_every_view(
        &mut failures,
        "folded",
        Language::YAML,
        yaml,
        &|v| v["folded"] = "folded words continues here".into(),
        &yaml.replace("  folded text\n", "  folded words\n"),
    );
    // A line break in folded text is a blank line.
    from_every_view(
        &mut failures,
        "folded with a line break",
        Language::YAML,
        yaml,
        &|v| v["folded"] = "folded text\ncontinues here".into(),
        &yaml.replace(
            "  folded text\n  continues here\n",
            "  folded text\n\n  continues here\n",
        ),
    );
    // A more-indented line keeps its line breaks in folded text.
    from_every_view(
        &mut failures,
        "folded holding an indented line",
        Language::YAML,
        yaml,
        &|v| v["folded"] = "folded\n  indented".into(),
        &yaml.replace(
            "folded: >-\n  folded text\n  continues here\n",
            "folded: >-\n  folded\n    indented\n",
        ),
    );
    // Text folding cannot hold (leading line breaks): written literally.
    from_every_view(
        &mut failures,
        "folded starting with a line break",
        Language::YAML,
        yaml,
        &|v| v["folded"] = "\nfolded".into(),
        &yaml.replace(
            "folded: >-\n  folded text\n  continues here\n",
            "folded: |-\n\n  folded\n",
        ),
    );
    report(failures);
}

#[test]
fn long_strings_and_heredocs_edited_in_any_view_keep_their_form() {
    let mut failures = Vec::new();
    let toml = "# c\nml = \"\"\"\nmulti\nline\"\"\"\nz = 1 # z\n";
    from_every_view(
        &mut failures,
        "TOML long string",
        Language::TOML,
        toml,
        &|v| v["ml"] = "many\nline".into(),
        &toml.replace("multi\n", "many\n"),
    );
    let hcl = "# c\nscript = <<EOT\nline one\n  line two\nEOT\nz = 1 # z\n";
    from_every_view(
        &mut failures,
        "HCL heredoc",
        Language::HCL,
        hcl,
        &|v| v["script"] = "line one\n  line 2\n".into(),
        &hcl.replace("  line two\n", "  line 2\n"),
    );
    // Text gaining a line equal to the delimiter takes another one.
    from_every_view(
        &mut failures,
        "HCL heredoc holding its delimiter",
        Language::HCL,
        hcl,
        &|v| v["script"] = "line one\nEOT\n".into(),
        "# c\nscript = <<EOT_1\nline one\nEOT\nEOT_1\nz = 1 # z\n",
    );
    let indented = "# c\nscript = <<-EOT\n    a\n    EOT\nz = 1 # z\n";
    from_every_view(
        &mut failures,
        "indented HCL heredoc holding its delimiter",
        Language::HCL,
        indented,
        &|v| v["script"] = "a\nEOT\n".into(),
        "# c\nscript = <<-EOT_1\n    a\n    EOT\n    EOT_1\nz = 1 # z\n",
    );
    report(failures);
}

#[test]
fn xml_text_keeps_its_whitespace_and_cdata_through_any_view() {
    let xml = "<!-- c -->\n<config>\n  <spaced>  keep  </spaced>\n  <lines>a\nb\n</lines>\n  <script><![CDATA[if a < b then\n  echo \"x\"]]></script>\n  <n>1</n>\n</config>\n";
    let mut failures = Vec::new();
    // Shown exactly in every view.
    for edit in ALL.into_iter().filter(|edit| *edit != Language::XML) {
        let bridge = Bridge::new(Language::XML, edit, None).unwrap();
        let shown = bridge.to_edit(xml).unwrap().text;
        let value = edit.parse(&shown).unwrap();
        for (pointer, expected) in [
            ("/config/spaced", "  keep  "),
            ("/config/lines", "a\nb\n"),
            ("/config/script", "if a < b then\n  echo \"x\""),
        ] {
            if value.pointer(pointer).and_then(Value::as_str) != Some(expected) {
                failures.push(format!(
                    "{edit} shows {pointer} as {:?}",
                    value.pointer(pointer)
                ));
            }
        }
    }
    from_every_view(
        &mut failures,
        "spaced text",
        Language::XML,
        xml,
        &|v| v["config"]["spaced"] = "  kept  ".into(),
        &xml.replace("  keep  ", "  kept  "),
    );
    from_every_view(
        &mut failures,
        "text ending in a line break",
        Language::XML,
        xml,
        &|v| v["config"]["lines"] = "a\nc\n".into(),
        &xml.replace("a\nb\n</lines>", "a\nc\n</lines>"),
    );
    from_every_view(
        &mut failures,
        "CDATA",
        Language::XML,
        xml,
        &|v| v["config"]["script"] = "if a < b then\n  print \"x\"".into(),
        &xml.replace("echo", "print"),
    );
    // Another value edited: the whitespace and CDATA stay as written.
    from_every_view(
        &mut failures,
        "a sibling",
        Language::XML,
        xml,
        &|v| v["config"]["n"] = 2.into(),
        &xml.replace("<n>1</n>", "<n>2</n>"),
    );
    // Values with whitespace from other hosts reach XML and come back.
    let yaml = "# c\npadded: \"  padded  \"\ntail: |\n  tail\nz: 1\n";
    let saved = save_value(Language::YAML, Language::XML, yaml, &|v| v["z"] = 2.into());
    if saved.as_deref() != Ok(yaml.replace("z: 1", "z: 2").as_str()) {
        failures.push(format!("YAML host via XML: {saved:?}"));
    }
    report(failures);
}

#[test]
fn an_edited_string_keeps_the_hosts_escapes_and_others_stay_byte_exact() {
    let json = "{\n  \"s\": \"caf\\u00e9\",\n  \"t\": \"tab\\there\",\n  \"u\": \"a\\/b\",\n  \"flow\": {\"x\": \"\\u00e9\", \"y\": 1},\n  \"n\": 1\n}\n";
    let mut failures = Vec::new();
    from_every_view(
        &mut failures,
        "a number edited",
        Language::JSON,
        json,
        &|v| v["n"] = 2.into(),
        &json.replace("\"n\": 1", "\"n\": 2"),
    );
    from_every_view(
        &mut failures,
        "an escaped string edited",
        Language::JSON,
        json,
        &|v| v["s"] = "cafés".into(),
        &json.replace("caf\\u00e9", "caf\\u00e9s"),
    );
    from_every_view(
        &mut failures,
        "a string with an escaped slash edited",
        Language::JSON,
        json,
        &|v| v["u"] = "a/c".into(),
        &json.replace("a\\/b", "a\\/c"),
    );
    from_every_view(
        &mut failures,
        "a key added to an object holding an escaped string",
        Language::JSON,
        json,
        &|v| v["flow"]["z"] = 3.into(),
        &json.replace("\"y\": 1}", "\"y\": 1, \"z\": 3}"),
    );
    // The other languages with `\u` escapes in double-quoted strings.
    for (host, text) in [
        (Language::YAML, "s: \"caf\\u00e9\"\nn: 1\n"),
        (Language::TOML, "s = \"caf\\u00e9\"\nn = 1\n"),
        (Language::HCL, "s = \"caf\\u00e9\"\nn = 1\n"),
    ] {
        from_every_view(
            &mut failures,
            "an escaped string edited",
            host,
            text,
            &|v| v["s"] = "cafés".into(),
            &text.replace("caf\\u00e9", "caf\\u00e9s"),
        );
        from_every_view(
            &mut failures,
            "a sibling edited",
            host,
            text,
            &|v| v["n"] = 2.into(),
            &text.replace('1', "2"),
        );
    }
    report(failures);
}

#[test]
fn a_key_added_to_an_empty_hcl_block_over_lines_keeps_the_rest() {
    let hcl = "# top\nblock {\n}\n\n# mid\nempty = {}\nobj = {\n  a = 1\n}\n";
    let mut failures = Vec::new();
    from_every_view(
        &mut failures,
        "empty block",
        Language::HCL,
        hcl,
        &|v| v["block"] = serde_json::json!({"k": 1}),
        &hcl.replace("block {\n}", "block {\n  k = 1\n}"),
    );
    from_every_view(
        &mut failures,
        "multi-line object attribute",
        Language::HCL,
        hcl,
        &|v| v["obj"]["b"] = 2.into(),
        &hcl.replace("  a = 1\n", "  a = 1\n  b = 2\n"),
    );
    let json = "{\n  \"block\": {\n  },\n  \"n\": 1\n}\n";
    from_every_view(
        &mut failures,
        "empty JSON object over lines",
        Language::JSON,
        json,
        &|v| v["block"] = serde_json::json!({"k": 1}),
        "{\n  \"block\": {\n    \"k\": 1\n  },\n  \"n\": 1\n}\n",
    );
    report(failures);
}

#[test]
fn yaml_anchors_aliases_and_merges_survive_an_edit_from_any_view() {
    let yaml = "# c\ndefaults: &base   # anchor\n  timeout: 30\n  retries: 0x1F\nservice:\n  <<: *base\n  name: web\nlist:\n  - one\n  - &x two\n  - *x\n";
    let mut failures = Vec::new();
    // The anchored item edited: its anchor stays, the alias is written out.
    from_every_view(
        &mut failures,
        "anchored item",
        Language::YAML,
        yaml,
        &|v| v["list"][1] = "three".into(),
        &yaml.replace("  - &x two\n  - *x\n", "  - &x three\n  - two\n"),
    );
    // Both edited alike: the alias stays.
    from_every_view(
        &mut failures,
        "anchored item and its alias",
        Language::YAML,
        yaml,
        &|v| {
            v["list"][1] = "three".into();
            v["list"][2] = "three".into();
        },
        &yaml.replace("&x two", "&x three"),
    );
    // The anchor's value and the merged copy edited alike: the merge stays.
    from_every_view(
        &mut failures,
        "merged value",
        Language::YAML,
        yaml,
        &|v| {
            v["defaults"]["timeout"] = 31.into();
            v["service"]["timeout"] = 31.into();
        },
        &yaml.replace("timeout: 30", "timeout: 31"),
    );
    // Only the anchor's: the merge is written out, numbers as the host
    // wrote them.
    from_every_view(
        &mut failures,
        "anchor parted from its merge",
        Language::YAML,
        yaml,
        &|v| v["defaults"]["timeout"] = 31.into(),
        &yaml
            .replace("  timeout: 30\n", "  timeout: 31\n")
            .replace("  <<: *base\n", "  timeout: 30\n  retries: 0x1F\n"),
    );
    report(failures);
}

#[test]
fn new_yaml_strings_are_plain_like_their_neighbours() {
    let yaml = "a: hello\nb: some words\n";
    let mut failures = Vec::new();
    from_every_view(
        &mut failures,
        "new strings",
        Language::YAML,
        yaml,
        &|v| {
            v["c"] = "more words".into();
            v["d"] = "/path".into();
        },
        "a: hello\nb: some words\nc: more words\nd: /path\n",
    );
    report(failures);
}
