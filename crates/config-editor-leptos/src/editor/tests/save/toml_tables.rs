//! A TOML table added from another view (a `[[bin]]` item, a new
//! `[section]`) is parted from its neighbours as the file parts its tables:
//! by a blank line where it writes one, by none where it writes none.

use crate::editor::Language;
use crate::editor::save_matrix::{report, save_steps, save_value};
use serde_json::{Map, Value, json};

const CARGO: &str = r#"[package]
name = "demo"
version = "0.1.0"
edition = "2021"
# keep in sync with CI
rust-version = "1.75"

[dependencies]
serde = { version = "1", features = ["derive"] }
tokio = { version = "1.36", features = ["full"] } # async runtime
anyhow = "1"

[dev-dependencies]
proptest = "1"

[features]
default = ["std"]
std = []

[[bin]]
name = "demo"
path = "src/main.rs"

[profile.release]
lto = true
codegen-units = 1
"#;

const TIGHT: &str = "[a]\nx = 1\n[[b]]\ny = 2\n[c]\nz = 3\n";

const VIEWS: [Language; 3] = [Language::YAML, Language::JSON, Language::HCL];

fn root(value: &mut Value) -> &mut Map<String, Value> {
    value.as_object_mut().unwrap()
}

/// `host` with `text` put before `before` (written once in it).
fn with(host: &str, before: &str, text: &str) -> String {
    assert_eq!(
        host.matches(before).count(),
        1,
        "{before:?} once in the host"
    );
    host.replacen(before, &format!("{text}{before}"), 1)
}

type Case = (&'static str, &'static str, Box<dyn Fn(&mut Value)>, String);

#[test]
fn a_table_added_is_parted_as_the_file_parts_its_tables() {
    let cases: Vec<Case> = vec![
        (
            "a [[bin]] item",
            CARGO,
            Box::new(|value| {
                value["bin"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"name": "tool", "path": "src/tool.rs"}));
            }),
            with(
                CARGO,
                "\n[profile.release]",
                "\n[[bin]]\nname = \"tool\"\npath = \"src/tool.rs\"\n",
            ),
        ),
        (
            "a table at the end",
            CARGO,
            Box::new(|value| {
                root(value).insert("lib".into(), json!({"path": "src/lib.rs"}));
            }),
            format!("{CARGO}\n[lib]\npath = \"src/lib.rs\"\n"),
        ),
        (
            "a table between two",
            CARGO,
            Box::new(|value| {
                root(value).shift_insert(2, "build-dependencies".into(), json!({"cc": "1"}));
            }),
            with(
                CARGO,
                "[dev-dependencies]",
                "[build-dependencies]\ncc = \"1\"\n\n",
            ),
        ),
        (
            "a table in a file without blank lines",
            TIGHT,
            Box::new(|value| {
                root(value).insert("d".into(), json!({"w": 4}));
            }),
            format!("{TIGHT}[d]\nw = 4\n"),
        ),
        (
            "an item in a file without blank lines",
            TIGHT,
            Box::new(|value| {
                value["b"].as_array_mut().unwrap().push(json!({"y": 3}));
            }),
            with(TIGHT, "[c]", "[[b]]\ny = 3\n"),
        ),
    ];
    let mut failures = Vec::new();
    for (label, host, change, expected) in &cases {
        for view in VIEWS {
            match save_value(Language::TOML, view, host, change) {
                Ok(saved) if saved == *expected => {}
                Ok(saved) => failures.push(format!(
                    "{label} via {view}:\n--- expected\n{expected}--- saved\n{saved}"
                )),
                Err(error) => failures.push(format!("{label} via {view}: {error}")),
            }
        }
    }
    report(failures);
}

/// Typed in the view with pauses: the item saved with its first key, then
/// with the next.
#[test]
fn an_item_typed_in_steps_is_parted_too() {
    let mut failures = Vec::new();
    let expected = with(
        CARGO,
        "\n[profile.release]",
        "\n[[bin]]\nname = \"tool\"\npath = \"src/tool.rs\"\n",
    );
    for view in VIEWS {
        let saved = save_steps(
            Language::TOML,
            view,
            CARGO,
            &[
                &|value: &mut Value| {
                    value["bin"]
                        .as_array_mut()
                        .unwrap()
                        .push(json!({"name": "tool"}));
                },
                &|value: &mut Value| value["bin"][1]["path"] = json!("src/tool.rs"),
            ],
        );
        match saved {
            Ok(saved) if saved == expected => {}
            other => failures.push(format!("via {view}: {other:?}")),
        }
    }
    report(failures);
}
