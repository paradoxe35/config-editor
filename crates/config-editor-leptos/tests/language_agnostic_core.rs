//! The editor's core is language-agnostic: code outside
//! `src/editor/languages/` never names a built-in language. What differs by
//! syntax is asked of the `ConfigLanguage` trait (add a hook with a default
//! when none fits), so a custom language gets every feature too. Tests,
//! doc comments and the demo (`main.rs`) may name languages.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

const LANGUAGES: [&str; 5] = ["YAML", "JSON", "TOML", "HCL", "XML"];

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

/// Where `#[cfg(… test …)]` (with any further attributes) starts, if the
/// item after it at `rest` is a `mod`: its name, and whether it is inline.
fn test_module(attribute_end: &str) -> Option<(String, bool)> {
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
fn test_cfgs(source: &str) -> impl Iterator<Item = (usize, &str)> {
    source.match_indices("#[cfg(").filter_map(move |(at, _)| {
        let end = at + source[at..].find(")]")? + 2;
        let words = &source[at..end];
        words
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .any(|word| word == "test")
            .then_some((at, &source[end..]))
    })
}

/// A file declared as `mod name;` under a test cfg: the path it lives at.
fn test_files(files: &[PathBuf]) -> HashSet<PathBuf> {
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
                found.insert(base.join(format!("{name}.rs")));
                found.insert(base.join(&name));
            }
        }
    }
    found
}

/// The lines of `file` (before its first inline test module, without
/// comments) that name a built-in language.
fn naming_lines(file: &Path) -> Vec<(usize, String)> {
    let source = fs::read_to_string(file).unwrap();
    let end = test_cfgs(&source)
        .find(|(_, rest)| matches!(test_module(rest), Some((_, true))))
        .map_or(source.len(), |(at, _)| at);
    source[..end]
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let code = line.split("//").next().unwrap();
            LANGUAGES
                .iter()
                .any(|name| {
                    code.match_indices(&format!("Language::{name}"))
                        .any(|(at, matched)| {
                            !code[at + matched.len()..]
                                .starts_with(|c: char| c.is_alphanumeric() || c == '_')
                        })
                })
                .then(|| (index + 1, line.trim().to_string()))
        })
        .collect()
}

fn offenders(src: &Path) -> Vec<String> {
    let mut files = Vec::new();
    rust_files(src, &mut files);
    let tests = test_files(&files);
    let languages = src.join("editor").join("languages");
    files
        .iter()
        .filter(|file| {
            !file.starts_with(&languages)
                && *file != &src.join("main.rs")
                && !tests.contains(*file)
                && !file.ancestors().any(|dir| tests.contains(dir))
        })
        .flat_map(|file| {
            naming_lines(file).into_iter().map(move |(line, text)| {
                format!(
                    "{}:{line}: {text}",
                    file.strip_prefix(src).unwrap().display()
                )
            })
        })
        .collect()
}

#[test]
fn the_core_never_names_a_built_in_language() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let found = offenders(&src);
    assert!(
        found.is_empty(),
        "the core names a language; ask the ConfigLanguage trait instead \
         (a hook with a default, mapped in e2e/tests/api-parity.spec.mjs):\n{}",
        found.join("\n")
    );
}

/// The check itself catches a core file that names a language, and lets
/// test modules and comments through.
#[test]
fn the_check_catches_a_named_language() {
    let dir = std::env::temp_dir().join(format!("agnostic-core-{}", std::process::id()));
    let editor = dir.join("editor");
    fs::create_dir_all(editor.join("languages")).unwrap();
    fs::write(
        dir.join("lib.rs"),
        "#[cfg(test)]\nmod checks;\nfn f() {} // Language::YAML in a comment\n",
    )
    .unwrap();
    fs::write(dir.join("checks.rs"), "fn t() { Language::JSON; }\n").unwrap();
    fs::write(
        editor.join("hover.rs"),
        "fn g() { if language == Language::YAML { } }\n#[cfg(all(test, feature = \"x\"))]\nmod tests { fn h() { Language::TOML; } }\n",
    )
    .unwrap();
    fs::write(
        editor.join("languages").join("yaml.rs"),
        "fn y() { Language::YAML; }\n",
    )
    .unwrap();
    let found = offenders(&dir);
    fs::remove_dir_all(&dir).unwrap();
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(found[0].starts_with("editor/hover.rs:1:"), "{found:#?}");
}
