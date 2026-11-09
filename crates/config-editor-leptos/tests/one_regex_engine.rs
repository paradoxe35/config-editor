//! One regular-expression engine per build: `regex` when
//! `schema-validation` links it (its validator needs it), else the small
//! `regex-lite`. Every pattern (search, `patternProperties`, secret
//! placeholders, the YAML typed-scalar rule) goes through
//! `src/editor/pattern_engine.rs`, so no build links both: no file names
//! either crate but that one.

use std::fs;
use std::path::{Path, PathBuf};

fn rust_files(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, found);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
}

#[test]
fn only_the_pattern_engine_module_names_a_regex_crate() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    rust_files(&root.join("src"), &mut files);
    let engine = root.join("src/editor/pattern_engine.rs");
    assert!(engine.exists(), "the engine module is missing");
    let mut failures = Vec::new();
    for file in files.iter().filter(|file| **file != engine) {
        let source = fs::read_to_string(file).unwrap();
        for (number, line) in source.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            let names = code
                .split(|c: char| !c.is_alphanumeric() && c != '_' && c != ':')
                .any(|word| word.starts_with("regex::") || word.starts_with("regex_lite::"));
            if names {
                failures.push(format!(
                    "{}:{}: {}",
                    file.strip_prefix(root).unwrap().display(),
                    number + 1,
                    line.trim()
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "use `pattern_engine` instead of naming a regex crate:\n{}",
        failures.join("\n")
    );
}
