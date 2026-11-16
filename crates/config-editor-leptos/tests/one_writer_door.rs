//! P2: every value a language writes passes one door. The data model marks
//! the values JSON cannot hold, and only the core's writer door
//! (`src/editor/language/writer_door.rs`, the `write_…` methods) turns
//! them into what the language writes, so a language's writer never sees
//! a mark. Code outside the languages therefore never calls a writer hook
//! (`serialize`, `format_value`, `format_value_like`, `format_value_at`,
//! `one_line_body`, `serialize_over`) itself. A language's own code calls
//! its hooks on values already through the door. Test modules may call
//! hooks directly.

mod source_scan;

use std::fs;
use std::path::{Path, PathBuf};

use source_scan::{rust_files, test_cfgs, test_files, test_module};

/// The writer hooks called with any arguments.
const HOOKS: [&str; 5] = [
    ".format_value(",
    ".format_value_like(",
    ".format_value_at(",
    ".one_line_body(",
    ".serialize_over(",
];

/// The hook `serialize(value, previous)`, told from serde's
/// `serialize(serializer)` and the `Language` handle's `serialize(value)`
/// by its second argument.
const SERIALIZE: &str = ".serialize(";

/// The files and folders that may call the hooks: the door, and each
/// language's own code.
const ALLOWED: [&str; 9] = [
    "crates/config-editor-leptos/src/editor/language/writer_door.rs",
    "crates/config-editor-leptos/src/editor/languages/yaml",
    "crates/config-editor-leptos/src/editor/languages/toml",
    "crates/config-editor-leptos/src/editor/languages/json",
    "crates/config-editor-leptos/src/editor/languages/hcl",
    "crates/config-editor-leptos/src/editor/languages/xml",
    "crates/config-editor-leptos/src/editor/declared_syntax",
    "crates/config-editor-leptos/src/editor/odd_syntax",
    "js/config-editor/src/languages/js_language.rs",
];

/// Whether the call whose arguments start at `rest` has two or more.
fn two_arguments(rest: &str) -> bool {
    let mut depth = 0usize;
    for character in rest.chars() {
        match character {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' if depth == 0 => return false,
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => return true,
            _ => {}
        }
    }
    false
}

/// The calls to a writer hook in the code under `src` (test modules and
/// comments left out) outside the allowed files, as `file:line: code`.
fn hook_calls(root: &Path, src: &Path) -> Vec<String> {
    let mut files = Vec::new();
    rust_files(src, &mut files);
    let tests = test_files(&files);
    let allowed: Vec<PathBuf> = ALLOWED.iter().map(|path| root.join(path)).collect();
    let production = files.into_iter().filter(|file| {
        !tests.contains(file)
            && !file.ancestors().any(|dir| tests.contains(dir))
            && !allowed.iter().any(|allowed| file.starts_with(allowed))
    });
    let mut found = Vec::new();
    for file in production {
        let source = fs::read_to_string(&file).unwrap();
        let end = test_cfgs(&source)
            .find(|(_, rest)| matches!(test_module(rest), Some((_, true))))
            .map_or(source.len(), |(at, _)| at);
        let lines: Vec<&str> = source[..end]
            .lines()
            .map(|line| line.split("//").next().unwrap_or_default())
            .collect();
        let code = lines.join("\n");
        let calls = HOOKS
            .iter()
            .flat_map(|hook| code.match_indices(hook).map(|(at, _)| at))
            .chain(
                code.match_indices(SERIALIZE)
                    .filter(|(at, _)| two_arguments(&code[at + SERIALIZE.len()..]))
                    .map(|(at, _)| at),
            );
        for at in calls {
            let line = code[..at].matches('\n').count();
            found.push(format!(
                "{}:{}: {}",
                file.strip_prefix(root).unwrap().display(),
                line + 1,
                lines[line].trim()
            ));
        }
    }
    found.sort();
    found
}

#[test]
fn only_the_writer_door_calls_a_languages_writers() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = root.canonicalize().unwrap();
    let mut found = hook_calls(&root, &root.join("crates/config-editor-leptos/src"));
    // The JavaScript glue, where the crate is built in its workspace.
    let glue = root.join("js/config-editor/src");
    if glue.exists() {
        found.extend(hook_calls(&root, &glue));
    }
    assert!(
        found.is_empty(),
        "a writer hook is called past the writer door, so the language may be \
         given the data model's marks; call the `write_…` method instead:\n{}",
        found.join("\n")
    );
}

#[test]
fn the_check_finds_a_hook_call() {
    assert!(two_arguments("&value, None)"));
    assert!(!two_arguments("&mut serializer)"));
    assert!(!two_arguments("&json!({\"a\": 1, \"b\": 2}))"));
}
