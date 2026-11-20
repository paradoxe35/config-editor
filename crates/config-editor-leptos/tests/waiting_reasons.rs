//! A save that waits says why in plain words and what to do, so its
//! reasons read as sentences: "make this edit while editing as YAML",
//! never "make it editing as YAML".

use std::fs;
use std::path::{Path, PathBuf};

fn rust_files(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("reads the folder") {
        let path = entry.expect("reads an entry").path();
        if path.is_dir() {
            rust_files(&path, found);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
}

/// Every "editing as {language}" in a message the crate writes (a format
/// string's `editing as {}` or `editing as {edit}`) follows "while": the
/// edit is made while editing as that language.
#[test]
fn every_reason_to_edit_as_another_language_reads_while_editing_as() {
    let mut files = Vec::new();
    rust_files(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut files,
    );
    assert!(files.len() > 100, "only {} files read", files.len());
    let mut found = Vec::new();
    for file in files {
        let source = fs::read_to_string(&file).expect("reads");
        for (index, line) in source.lines().enumerate() {
            let mut rest = line;
            while let Some(at) = rest.find("editing as {") {
                if !rest[..at].ends_with("while ") {
                    found.push(format!("{}:{}: {}", file.display(), index + 1, line.trim()));
                }
                rest = &rest[at + 1..];
            }
        }
    }
    assert!(
        found.is_empty(),
        "reasons without \"while\":\n{}",
        found.join("\n")
    );
}
