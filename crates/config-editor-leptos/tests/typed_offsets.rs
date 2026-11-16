//! Text offsets are typed: a byte offset is a `TextSize` (a range a
//! `TextRange`), a UTF-16 one a `Utf16`, so the two are never mixed. A
//! function signature or struct field of the crate's own code (not its
//! tests) that names an offset (`offset`, `at`, `pos`, `start`, `end`,
//! `caret`, `anchor`, `head`, `*_at`, `*_offset`) as a plain `usize` or
//! `Range<usize>` is counted per file; `typed_offsets_allowed.txt` lists
//! the files still allowed some, with why. A file may not gain any, and a
//! file that lost some lowers its number there, so the list only shrinks.

mod source_scan;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use source_scan::{rust_files, test_cfgs, test_files, test_module};

/// Whether `name` names an offset.
fn offset_name(name: &str) -> bool {
    matches!(
        name,
        "offset" | "at" | "pos" | "start" | "end" | "caret" | "anchor" | "head"
    ) || name.ends_with("_at")
        || name.ends_with("_offset")
}

/// The plain types an offset may not have.
const PLAIN: [&str; 4] = [
    "usize",
    "Range<usize>",
    "std::ops::Range<usize>",
    "Option<usize>",
];

/// The places in `code` (the crate's own, its test modules cut off) where a
/// signature or a field names an offset as a plain type: `(line, name)`.
/// A `let` and a closure's parameters are local, not counted.
fn plain_offsets(code: &str) -> Vec<(usize, String)> {
    let mut found = Vec::new();
    for (number, line) in code.lines().enumerate() {
        let line = line.split("//").next().unwrap_or("");
        let trimmed = line.trim_start();
        if trimmed.starts_with("let ") || trimmed.starts_with("///") {
            continue;
        }
        let mut rest = line;
        let mut before = 0;
        while let Some(colon) = rest.find(':') {
            let (head, tail) = rest.split_at(colon);
            let tail = &tail[1..];
            // `::` is a path, not a type.
            if tail.starts_with(':') || head.ends_with(':') {
                before += colon + 2;
                rest = tail.get(1..).unwrap_or("");
                continue;
            }
            let name: String = head
                .chars()
                .rev()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            let ty = tail.trim_start();
            let plain = PLAIN.iter().any(|plain| {
                ty.strip_prefix(plain).is_some_and(|after| {
                    !after.starts_with(|c: char| c.is_alphanumeric() || c == '_' || c == '<')
                })
            });
            let in_closure = line[..before + colon].matches('|').count() % 2 == 1;
            if plain && offset_name(&name) && !in_closure {
                found.push((number + 1, name));
            }
            before += colon + 1;
            rest = tail;
        }
    }
    found
}

/// `source` up to its first inline test module.
fn without_tests(source: &str) -> &str {
    test_cfgs(source)
        .find(|(_, rest)| matches!(test_module(rest), Some((_, true))))
        .map_or(source, |(at, _)| &source[..at])
}

/// Plain offsets per file of the crate's own code, by path from `src`.
fn counts(root: &Path) -> BTreeMap<String, usize> {
    let mut files = Vec::new();
    rust_files(&root.join("src"), &mut files);
    let tests = test_files(&files);
    let is_test = |file: &PathBuf| file.ancestors().any(|dir| tests.contains(dir));
    let mut counts = BTreeMap::new();
    for file in files.iter().filter(|file| !is_test(file)) {
        let source = fs::read_to_string(file).unwrap();
        let found = plain_offsets(without_tests(&source)).len();
        if found > 0 {
            let path = file.strip_prefix(root.join("src")).unwrap();
            counts.insert(path.display().to_string(), found);
        }
    }
    counts
}

/// The allowed counts: `path count why` per line, `#` comments.
fn allowed(root: &Path) -> BTreeMap<String, usize> {
    let list = fs::read_to_string(root.join("tests/typed_offsets_allowed.txt")).unwrap();
    list.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let mut words = line.split_whitespace();
            let path = words.next().unwrap().to_string();
            let count = words.next().and_then(|count| count.parse().ok()).unwrap();
            assert!(words.next().is_some(), "{path}: say why it is allowed");
            (path, count)
        })
        .collect()
}

#[test]
fn no_file_gains_a_plain_offset_and_the_allowed_list_only_shrinks() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let (found, allowed) = (counts(root), allowed(root));
    let mut failures = Vec::new();
    for (path, count) in &found {
        match allowed.get(path) {
            Some(allowed) if count <= allowed => {}
            Some(allowed) => failures.push(format!(
                "{path}: {count} plain offsets, {allowed} allowed: type the new ones (TextSize, TextRange, Utf16)"
            )),
            None => failures.push(format!(
                "{path}: {count} plain offsets: type them (TextSize, TextRange, Utf16)"
            )),
        }
    }
    for (path, allowed) in &allowed {
        let count = found.get(path).copied().unwrap_or(0);
        if count < *allowed {
            failures.push(format!(
                "{path}: {count} plain offsets now, {allowed} allowed: lower its number"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The check finds a planted plain offset in a signature and a field, and
/// leaves typed ones, locals, closures and other names alone.
#[test]
fn the_check_finds_a_planted_plain_offset() {
    let planted = "\
pub(crate) fn place(text: &str, caret: usize, count: usize) -> bool {
    let start: usize = 0;
    let typed = |at: usize| at;
    true
}
struct Span {
    line_offset: usize,
    range: Range<usize>,
    end: TextSize,
    path: crate::editor::paths::FieldPath,
}
fn cut(text: &str, at: Range<usize>, head_at: Option<usize>) {}
";
    let names: Vec<String> = plain_offsets(planted)
        .into_iter()
        .map(|(_, name)| name)
        .collect();
    assert_eq!(names, ["caret", "line_offset", "at", "head_at"]);
}
