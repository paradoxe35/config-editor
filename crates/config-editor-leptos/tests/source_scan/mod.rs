//! Reading the crate's own sources, for the tests that check how the
//! code is built: its Rust files, and which of them are test modules.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

pub fn rust_files(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, found);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
}

/// Where `#[cfg(… test …)]` (with any further attributes) starts, if the
/// item after it at `rest` is a `mod`: its name, and whether it is inline.
pub fn test_module(attribute_end: &str) -> Option<(String, bool)> {
    let mut rest = attribute_end.trim_start();
    while rest.starts_with("#[") {
        rest = rest[rest.find(']')? + 1..].trim_start();
    }
    for visibility in ["pub(crate) ", "pub(super) ", "pub "] {
        rest = rest.strip_prefix(visibility).unwrap_or(rest);
    }
    let rest = rest.strip_prefix("mod ")?;
    let name: String = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    let after = rest[name.len()..].trim_start();
    Some((name.clone(), after.starts_with('{')))
}

/// The `#[cfg(…)]` attributes of `source` that mention `test`, with the
/// text after each.
pub fn test_cfgs(source: &str) -> impl Iterator<Item = (usize, &str)> {
    source.match_indices("#[cfg(").filter_map(move |(at, _)| {
        let end = at + source[at..].find(")]")? + 2;
        let words = &source[at..end];
        words
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .any(|word| word == "test")
            .then_some((at, &source[end..]))
    })
}

/// The `#[path = "…"]` among the attributes that start `rest`, if any.
fn path_attribute(rest: &str) -> Option<&str> {
    let mut rest = rest.trim_start();
    while rest.starts_with("#[") {
        let end = rest.find(']')?;
        if let Some(value) = rest[2..end].trim().strip_prefix("path") {
            let value = value.trim_start().strip_prefix('=')?.trim();
            return value.strip_prefix('"')?.strip_suffix('"');
        }
        rest = rest[end + 1..].trim_start();
    }
    None
}

/// A file declared as `mod name;` under a test cfg: the path it lives at.
pub fn test_files(files: &[PathBuf]) -> HashSet<PathBuf> {
    let mut found = HashSet::new();
    for file in files {
        let source = fs::read_to_string(file).unwrap();
        let stem = file.file_stem().unwrap().to_str().unwrap();
        let base = if ["mod", "lib", "main"].contains(&stem) {
            file.parent().unwrap().to_path_buf()
        } else {
            file.with_extension("")
        };
        for (_, rest) in test_cfgs(&source) {
            if let Some((name, false)) = test_module(rest) {
                // `#[path = "…"]` names the file outright, relative to the
                // declaring file's folder.
                if let Some(path) = path_attribute(rest) {
                    found.insert(file.parent().unwrap().join(path));
                    continue;
                }
                found.insert(base.join(format!("{name}.rs")));
                found.insert(base.join(&name));
            }
        }
    }
    found
}
