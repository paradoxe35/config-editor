//! Random documents written with each language's rich forms (TOML
//! datetimes, inline tables and arrays of tables; HCL blocks, expressions
//! and heredocs; YAML anchors, tags, block scalars and several documents;
//! XML attributes, namespaces, mixed text and CDATA; JSON escapes, big and
//! exact numbers), as a tree rendered in one language, so a failing tree
//! shrinks by its entries.

use super::runner::Rng;
use crate::editor::languages::Language;

/// A random tree: maps, lists and leaves (an index into the language's
/// leaf forms).
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Node {
    Map(Vec<(String, Node)>),
    List(Vec<Node>),
    Leaf(usize),
}

const KEYS: [&str; 8] = ["name", "port", "web", "tls", "a", "b_2", "host", "x"];

fn leaves(language: Language) -> &'static [&'static str] {
    match language.label() {
        "YAML" => &[
            "1",
            "-0",
            "0x1F",
            "0o17",
            "1.50",
            "1e3",
            ".inf",
            "-.inf",
            ".nan",
            "~",
            "null",
            "true",
            "yes",
            "\"quoted \\u00e9 \\t\"",
            "'single ''q'''",
            "plain text",
            "2001-12-14t21:59:43.10-05:00",
            "12345678901234567890",
            "9007199254740993",
            "0.1",
            "!!str 12",
            "!Custom x",
            "|\n  line1\n  line2\n",
            ">-\n  folded\n  text\n",
            "[1, a, \"b\"]",
            "{x: 1, y: [2]}",
            "\"\"",
            "\"😀\"",
            "-9223372036854775808",
            "1.0",
            "&anchor value",
            "*anchor",
        ],
        "JSON" => &[
            "1",
            "-0",
            "1.0",
            "1.50",
            "12345678901234567890123",
            "9007199254740993",
            "-9223372036854775808",
            "18446744073709551615",
            "0.1",
            "1e-7",
            "1E+2",
            "\"\\u00e9\"",
            "\"\\ud83d\\ude00\"",
            "\"a\\\"b\\\\c\\n\"",
            "\"\\u0001\"",
            "true",
            "null",
            "[]",
            "{}",
            "[1, \"a\", null]",
            "\"\"",
            "\"8080\"",
            "\"true\"",
            "123456789.123456789",
        ],
        "TOML" => &[
            "1",
            "+1",
            "0x1F",
            "0o17",
            "0b101",
            "1_000",
            "1.50",
            "1e3",
            "inf",
            "-inf",
            "nan",
            "-0.0",
            "1979-05-27T07:32:00Z",
            "1979-05-27T00:32:00.999999-07:00",
            "1979-05-27",
            "07:32:00",
            "1979-05-27T07:32:00",
            "'literal \\n'",
            "\"basic \\u00e9 \\\\ \\\"\"",
            "\"\"\"\nmulti\nline\"\"\"",
            "'''\nraw\n'''",
            "true",
            "[1, 2]",
            "[\"a\", 'b']",
            "{ a = 1, b = \"x\" }",
            "[{ x = 1 }, { x = 2 }]",
            "9223372036854775807",
            "-9223372036854775808",
            "\"\"",
            "\"8080\"",
        ],
        "HCL" => &[
            "1",
            "1.50",
            "1e3",
            "\"str\"",
            "\"a${b}c\"",
            "var.x",
            "local.y[0]",
            "true",
            "null",
            "[1, \"a\"]",
            "{ a = 1, \"b c\" = 2 }",
            "<<EOT\nline\nEOT",
            "<<-EOT\n  ind\n  EOT",
            "\"\\u00e9\"",
            "12345678901234567890",
            "0.1",
            "upper(\"x\")",
            "a ? 1 : 2",
            "1 + 2",
            "\"%{if true}x%{endif}\"",
            "-5",
            "\"8080\"",
        ],
        _ => &[
            "1",
            "1.50",
            "text with spaces",
            " 7 ",
            "&amp;&lt;",
            "<![CDATA[a<b]]>",
            "é",
            "true",
            "",
            "12345678901234567890",
            "0.1",
            "-0",
            "x &#233; y",
            "1e3",
            "a<i>1</i>b",
        ],
    }
}

/// A random tree of at most `depth` levels.
pub(super) fn random_tree(rng: &mut Rng, language: Language, depth: usize) -> Node {
    let mut entries = Vec::new();
    for _ in 0..1 + rng.below(4) {
        let key = rng.pick(&KEYS).to_string();
        if entries.iter().any(|(existing, _)| *existing == key) {
            continue;
        }
        entries.push((key, random_node(rng, language, depth)));
    }
    Node::Map(entries)
}

fn random_node(rng: &mut Rng, language: Language, depth: usize) -> Node {
    let leaf = |rng: &mut Rng| Node::Leaf(rng.below(leaves(language).len()));
    if depth == 0 {
        return leaf(rng);
    }
    match rng.below(6) {
        0..=2 => leaf(rng),
        3 => random_tree(rng, language, depth - 1),
        4 => Node::List((0..1 + rng.below(3)).map(|_| leaf(rng)).collect()),
        _ => Node::List(
            (0..1 + rng.below(3))
                .map(|_| random_tree(rng, language, depth - 1))
                .collect(),
        ),
    }
}

/// Smaller trees: an entry or item left out, a branch made a leaf.
pub(super) fn smaller_trees(node: &Node) -> Vec<Node> {
    let mut out = Vec::new();
    match node {
        Node::Leaf(index) => {
            if *index > 0 {
                out.push(Node::Leaf(0));
            }
        }
        Node::Map(entries) => {
            for index in 0..entries.len() {
                let mut fewer = entries.clone();
                fewer.remove(index);
                out.push(Node::Map(fewer));
            }
            for (index, (key, value)) in entries.iter().enumerate() {
                for smaller in smaller_trees(value) {
                    let mut changed = entries.clone();
                    changed[index] = (key.clone(), smaller);
                    out.push(Node::Map(changed));
                }
                if !matches!(value, Node::Leaf(_)) {
                    let mut changed = entries.clone();
                    changed[index] = (key.clone(), Node::Leaf(0));
                    out.push(Node::Map(changed));
                }
            }
        }
        Node::List(items) => {
            for index in 0..items.len() {
                let mut fewer = items.clone();
                fewer.remove(index);
                if !fewer.is_empty() {
                    out.push(Node::List(fewer));
                }
            }
            for (index, item) in items.iter().enumerate() {
                for smaller in smaller_trees(item) {
                    let mut changed = items.clone();
                    changed[index] = smaller;
                    out.push(Node::List(changed));
                }
            }
        }
    }
    out
}

pub(super) fn size(node: &Node) -> usize {
    match node {
        Node::Leaf(index) => 1 + usize::from(*index > 0),
        Node::Map(entries) => {
            1 + entries
                .iter()
                .map(|(_, value)| 2 + size(value))
                .sum::<usize>()
        }
        Node::List(items) => 1 + items.iter().map(size).sum::<usize>(),
    }
}

/// `node` (a map) written in `language`; `extra` adds the language's
/// document-level forms (a second YAML document, an XML namespace).
pub(super) fn render(language: Language, node: &Node, extra: bool) -> String {
    let Node::Map(entries) = node else {
        return String::new();
    };
    let mut out = String::new();
    match language.label() {
        "YAML" => {
            yaml_map(language, entries, 0, &mut out);
            if extra {
                out.push_str("---\nother: 1\n");
            }
        }
        "JSON" => {
            json_node(language, node, 0, &mut out);
            out.push('\n');
        }
        "TOML" => toml_table(language, entries, &[], &mut out),
        "HCL" => hcl_body(language, entries, 0, &mut out),
        _ => {
            if extra {
                out.push_str("<c:meta xmlns:c=\"urn:c\" c:id=\"7\">\n  <c:k>v</c:k>\n</c:meta>\n");
            }
            xml_body(language, entries, 0, &mut out);
        }
    }
    out
}

fn leaf(language: Language, index: usize) -> &'static str {
    let all = leaves(language);
    all[index % all.len()]
}

fn indent(depth: usize) -> String {
    "  ".repeat(depth)
}

fn yaml_scalar(language: Language, index: usize, depth: usize) -> String {
    let text = leaf(language, index);
    // A block scalar's lines are indented under its key.
    if text.starts_with(['|', '>']) {
        let mut lines = text.lines();
        let head = lines.next().unwrap_or_default();
        let body: Vec<String> = lines
            .map(|line| format!("{}{}", indent(depth), line.trim_start()))
            .map(|line| format!("  {line}"))
            .collect();
        return format!("{head}\n{}", body.join("\n"));
    }
    text.to_string()
}

fn yaml_map(language: Language, entries: &[(String, Node)], depth: usize, out: &mut String) {
    for (key, value) in entries {
        out.push_str(&format!("{}{key}:", indent(depth)));
        yaml_value(language, value, depth, out);
    }
}

fn yaml_value(language: Language, value: &Node, depth: usize, out: &mut String) {
    match value {
        Node::Leaf(index) => {
            let scalar = yaml_scalar(language, *index, depth);
            out.push_str(&format!(" {scalar}\n"));
        }
        Node::Map(entries) if entries.is_empty() => out.push_str(" {}\n"),
        Node::Map(entries) => {
            out.push('\n');
            yaml_map(language, entries, depth + 1, out);
        }
        Node::List(items) => {
            out.push('\n');
            for item in items {
                match item {
                    Node::Map(entries) if !entries.is_empty() => {
                        out.push_str(&format!("{}-\n", indent(depth + 1)));
                        yaml_map(language, entries, depth + 2, out);
                    }
                    Node::List(_) | Node::Map(_) => {
                        out.push_str(&format!("{}- []\n", indent(depth + 1)));
                    }
                    Node::Leaf(index) => {
                        let scalar = yaml_scalar(language, *index, depth + 1);
                        out.push_str(&format!("{}- {scalar}\n", indent(depth + 1)));
                    }
                }
            }
        }
    }
}

fn json_node(language: Language, node: &Node, depth: usize, out: &mut String) {
    match node {
        Node::Leaf(index) => out.push_str(leaf(language, *index)),
        Node::Map(entries) => {
            if entries.is_empty() {
                out.push_str("{}");
                return;
            }
            out.push_str("{\n");
            for (position, (key, value)) in entries.iter().enumerate() {
                out.push_str(&format!("{}\"{key}\": ", indent(depth + 1)));
                json_node(language, value, depth + 1, out);
                out.push_str(if position + 1 < entries.len() {
                    ",\n"
                } else {
                    "\n"
                });
            }
            out.push_str(&format!("{}}}", indent(depth)));
        }
        Node::List(items) => {
            out.push('[');
            for (position, item) in items.iter().enumerate() {
                json_node(language, item, depth + 1, out);
                if position + 1 < items.len() {
                    out.push_str(", ");
                }
            }
            out.push(']');
        }
    }
}

fn toml_inline(language: Language, node: &Node) -> String {
    match node {
        Node::Leaf(index) => leaf(language, *index).to_string(),
        Node::Map(entries) => format!(
            "{{ {} }}",
            entries
                .iter()
                .map(|(key, value)| format!("{key} = {}", toml_inline(language, value)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Node::List(items) => format!(
            "[{}]",
            items
                .iter()
                .map(|item| toml_inline(language, item))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

fn toml_table(language: Language, entries: &[(String, Node)], path: &[String], out: &mut String) {
    let is_table_list = |value: &Node| matches!(value, Node::List(items) if !items.is_empty() && items.iter().all(|item| matches!(item, Node::Map(_))));
    for (key, value) in entries {
        let inline = match value {
            Node::Map(_) => false,
            _ => !is_table_list(value),
        };
        if inline {
            out.push_str(&format!("{key} = {}\n", toml_inline(language, value)));
        }
    }
    for (key, value) in entries {
        let mut child = path.to_vec();
        child.push(key.clone());
        match value {
            Node::Map(inner) => {
                out.push_str(&format!("\n[{}]\n", child.join(".")));
                toml_table(language, inner, &child, out);
            }
            Node::List(items) if is_table_list(value) => {
                for item in items {
                    if let Node::Map(inner) = item {
                        out.push_str(&format!("\n[[{}]]\n", child.join(".")));
                        toml_table(language, inner, &child, out);
                    }
                }
            }
            _ => {}
        }
    }
}

fn hcl_inline(language: Language, node: &Node) -> String {
    match node {
        Node::Leaf(index) => leaf(language, *index).to_string(),
        Node::Map(entries) => format!(
            "{{ {} }}",
            entries
                .iter()
                .map(|(key, value)| format!("{key} = {}", hcl_inline(language, value)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Node::List(items) => format!(
            "[{}]",
            items
                .iter()
                .map(|item| hcl_inline(language, item))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

fn hcl_body(language: Language, entries: &[(String, Node)], depth: usize, out: &mut String) {
    for (key, value) in entries {
        let pad = indent(depth);
        match value {
            Node::Map(inner) => {
                // Some blocks carry a label.
                let label = if key.len() % 2 == 0 { " \"main\"" } else { "" };
                out.push_str(&format!("{pad}{key}{label} {{\n"));
                hcl_body(language, inner, depth + 1, out);
                out.push_str(&format!("{pad}}}\n"));
            }
            Node::List(items)
                if !items.is_empty() && items.iter().all(|item| matches!(item, Node::Map(_))) =>
            {
                for item in items {
                    if let Node::Map(inner) = item {
                        out.push_str(&format!("{pad}{key} {{\n"));
                        hcl_body(language, inner, depth + 1, out);
                        out.push_str(&format!("{pad}}}\n"));
                    }
                }
            }
            _ => {
                let written = hcl_inline(language, value);
                out.push_str(&format!("{pad}{key} = {written}\n"));
            }
        }
    }
}

fn xml_body(language: Language, entries: &[(String, Node)], depth: usize, out: &mut String) {
    for (key, value) in entries {
        let pad = indent(depth);
        match value {
            Node::Leaf(index) => {
                let text = leaf(language, *index);
                if text.is_empty() {
                    out.push_str(&format!("{pad}<{key}/>\n"));
                } else {
                    out.push_str(&format!("{pad}<{key}>{text}</{key}>\n"));
                }
            }
            Node::Map(inner) => {
                // Leaves of a map become attributes when the key is short.
                let (attributes, children): (Vec<_>, Vec<_>) = inner.iter().cloned().partition(
                    |(name, value)| {
                        name.len() <= 2
                            && matches!(value, Node::Leaf(index) if !leaf(language, *index).contains(['<', '&']))
                    },
                );
                let written: String = attributes
                    .iter()
                    .map(|(name, value)| match value {
                        Node::Leaf(index) => format!(" {name}=\"{}\"", leaf(language, *index)),
                        _ => String::new(),
                    })
                    .collect();
                if children.is_empty() {
                    out.push_str(&format!("{pad}<{key}{written}/>\n"));
                } else {
                    out.push_str(&format!("{pad}<{key}{written}>\n"));
                    xml_body(language, &children, depth + 1, out);
                    out.push_str(&format!("{pad}</{key}>\n"));
                }
            }
            Node::List(items) => {
                for item in items {
                    xml_body(language, &[(key.clone(), item.clone())], depth, out);
                }
            }
        }
    }
}
