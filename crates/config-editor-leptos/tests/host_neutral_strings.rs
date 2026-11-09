//! The editor's default strings never assume where the host keeps its
//! files: a new baseline may come from a server, the disk, another tab or
//! another user, and the editor cannot tell which. A host that knows says so
//! through `strings` ("The file changed on the server.").

use config_editor_leptos::EditorStrings;

/// Words that name one kind of host's storage.
const ASSUMED: [&str; 4] = ["server", "disk", "backend", "cloud"];

fn texts(value: &serde_json::Value, key: &str, out: &mut Vec<(String, String)>) {
    match value {
        serde_json::Value::String(text) => out.push((key.to_string(), text.clone())),
        serde_json::Value::Object(map) => {
            for (name, item) in map {
                texts(item, name, out);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                texts(item, key, out);
            }
        }
        _ => {}
    }
}

#[test]
fn no_default_string_names_where_the_host_keeps_its_files() {
    let strings = serde_json::to_value(EditorStrings::default()).expect("serializes");
    let mut all = Vec::new();
    texts(&strings, "", &mut all);
    assert!(all.len() > 100, "only {} strings read", all.len());
    let found: Vec<String> = all
        .iter()
        .filter(|(_, text)| {
            let words = text.to_lowercase();
            ASSUMED.iter().any(|word| {
                words
                    .split(|c: char| !c.is_alphanumeric())
                    .any(|part| part == *word)
            })
        })
        .map(|(key, text)| format!("{key}: {text:?}"))
        .collect();
    assert!(
        found.is_empty(),
        "host-specific wording in defaults:\n{}",
        found.join("\n")
    );
}
