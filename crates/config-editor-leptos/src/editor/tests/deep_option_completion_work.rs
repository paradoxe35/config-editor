//! Key suggestions under a recursive `anyOf` whose options share a nested
//! key: the walk down to the caret's object follows each
//! level's options once, so its work grows with the depth, not with the
//! number of option paths to it (2^depth). Counted (see `work_count`) at
//! three depths, in every language.

use serde_json::json;

use crate::editor::completion::completions_for;
use crate::editor::languages::Language;
use crate::editor::schema::{SchemaBundle, build_schema};
use crate::editor::work_count::{Work, counted};

fn schema(options: usize) -> SchemaBundle {
    let types = ["integer", "boolean", "number"];
    let options: Vec<_> = (0..options)
        .map(|index| {
            let own = format!("a{}", index + 1);
            json!({"type": "object", "properties": {
                "v": {"anyOf": [{"type": types[index]}, {"type": "string"}]},
                "c": {"$ref": "#/$defs/node"},
                own: {}}})
        })
        .collect();
    build_schema(
        &json!({"type": "object", "$defs": {"node": {"anyOf": options}},
            "properties": {"root": {"$ref": "#/$defs/node"}}})
        .to_string(),
    )
    .unwrap()
}

/// `root` holding `depth` levels of `v: q` and `c`, the deepest object
/// `v: q` too (text every option allows, so none is matched) and a key `a`
/// being typed (the caret `|` after it).
fn text(language: Language, depth: usize) -> String {
    let mut inner = match language.label() {
        "JSON" => "\"v\": \"q\", \"a|\"".to_string(),
        "XML" => "<v>q</v><a|".to_string(),
        "TOML" => String::new(),
        "HCL" => "v = \"q\"\na|".to_string(),
        _ => String::new(),
    };
    match language.label() {
        "YAML" => {
            let mut lines = vec!["root:".to_string()];
            let mut indent = "  ".to_string();
            for _ in 0..depth {
                lines.push(format!("{indent}v: q"));
                lines.push(format!("{indent}c:"));
                indent.push_str("  ");
            }
            lines.push(format!("{indent}v: q"));
            lines.push(format!("{indent}a|"));
            lines.join("\n") + "\n"
        }
        "TOML" => {
            let mut table = "root".to_string();
            let mut tables = Vec::new();
            for _ in 0..depth {
                tables.push(format!("[{table}]\nv = \"q\""));
                table.push_str(".c");
            }
            tables.push(format!("[{table}]\nv = \"q\"\na|"));
            tables.join("\n") + "\n"
        }
        label => {
            for _ in 0..depth {
                inner = match label {
                    "JSON" => format!("\"v\": \"q\", \"c\": {{{inner}}}"),
                    "XML" => format!("<v>q</v><c>{inner}</c>"),
                    _ => format!("v = \"q\"\nc = {{\n{inner}\n}}"),
                };
            }
            match label {
                "JSON" => format!("{{\"root\": {{{inner}}}}}\n"),
                "XML" => format!("<root>{inner}</root>\n"),
                _ => format!("root = {{\n{inner}\n}}\n"),
            }
        }
    }
}

/// The schema walks of one suggestion run at `depth`, and its labels.
fn walks(language: Language, options: usize, depth: usize) -> (usize, Vec<String>) {
    let marked = text(language, depth);
    let at = marked.find('|').unwrap();
    let text = marked.replacen('|', "", 1);
    let caret = text[..at].encode_utf16().count();
    let schema = schema(options);
    let (suggested, walks) = counted(Work::BranchWalk, || {
        completions_for(
            &text,
            caret,
            true,
            Some(&schema),
            true,
            language.behavior(),
            false,
        )
    });
    let labels = suggested.into_iter().map(|option| option.label).collect();
    (walks, labels)
}

#[test]
fn key_suggestions_under_recursive_options_grow_with_the_depth() {
    for &language in Language::ALL {
        for options in [2, 3] {
            let [six, eight, ten] = [6, 8, 10].map(|depth| walks(language, options, depth));
            for (depth, (_, labels)) in [(6, &six), (8, &eight), (10, &ten)] {
                for own in (1..=options).map(|index| format!("a{index}")) {
                    assert!(
                        labels.contains(&own),
                        "{language} options={options} depth={depth}: {labels:?}"
                    );
                }
            }
            // Linear work: 10/6 of the walks at depth 6 (and a few more);
            // one path per option combination is 2^4 = 16 times as many.
            let (six, ten) = (six.0, ten.0);
            assert!(
                3 * ten <= 6 * six.max(16),
                "{language} options={options}: {six} walks at depth 6, {} at 8, {ten} at 10",
                eight.0
            );
        }
    }
}
