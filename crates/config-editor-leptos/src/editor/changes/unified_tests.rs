//! `unified_diff` against what GNU `diff -u --label baseline --label current`
//! prints, and applied back as a patch.

use super::unified_diff;

#[test]
fn same_texts_have_no_diff() {
    assert_eq!(unified_diff("a\nb\n", "a\nb\n", 3), "");
    assert_eq!(unified_diff("", "", 3), "");
    // Line endings are not a change.
    assert_eq!(unified_diff("a\r\nb\r\n", "a\nb\n", 3), "");
}

#[test]
fn one_modified_line_with_context() {
    // printf 'a\nb\nc\nd\ne\nf\ng\n' / 'a\nb\nc\nD\ne\nf\ng\n' | diff -u
    assert_eq!(
        unified_diff("a\nb\nc\nd\ne\nf\ng\n", "a\nb\nc\nD\ne\nf\ng\n", 3),
        "--- baseline\n+++ current\n@@ -1,7 +1,7 @@\n a\n b\n c\n-d\n+D\n e\n f\n g\n"
    );
    assert_eq!(
        unified_diff("a\nb\nc\nd\ne\nf\ng\n", "a\nb\nc\nD\ne\nf\ng\n", 1),
        "--- baseline\n+++ current\n@@ -3,3 +3,3 @@\n c\n-d\n+D\n e\n"
    );
}

#[test]
fn distant_changes_are_separate_hunks_and_near_ones_merge() {
    let baseline: String = (1..=20).map(|n| format!("{n}\n")).collect();
    let current = baseline
        .replace("2\n", "two\n")
        .replace("19\n", "nineteen\n");
    // `replace` also hit "12\n"; diff -u of these texts:
    assert_eq!(
        unified_diff(&baseline, &current, 3),
        "--- baseline\n+++ current\n\
         @@ -1,5 +1,5 @@\n 1\n-2\n+two\n 3\n 4\n 5\n\
         @@ -9,12 +9,12 @@\n 9\n 10\n 11\n-12\n+1two\n 13\n 14\n 15\n 16\n 17\n 18\n-19\n+nineteen\n 20\n"
    );
}

#[test]
fn insertions_and_deletions_at_the_edges() {
    // Empty old side: the start is the line before (0).
    assert_eq!(
        unified_diff("", "a\nb\n", 3),
        "--- baseline\n+++ current\n@@ -0,0 +1,2 @@\n+a\n+b\n"
    );
    assert_eq!(
        unified_diff("a\nb\n", "", 3),
        "--- baseline\n+++ current\n@@ -1,2 +0,0 @@\n-a\n-b\n"
    );
    assert_eq!(
        unified_diff("a\nb\n", "a\nb\nc\n", 0),
        "--- baseline\n+++ current\n@@ -2,0 +3 @@\n+c\n"
    );
    assert_eq!(
        unified_diff("x\na\n", "a\n", 0),
        "--- baseline\n+++ current\n@@ -1 +0,0 @@\n-x\n"
    );
}

#[test]
fn a_missing_final_newline_is_marked() {
    assert_eq!(
        unified_diff("a\nb", "a\nb\n", 3),
        "--- baseline\n+++ current\n@@ -1,2 +1,2 @@\n a\n-b\n\\ No newline at end of file\n+b\n"
    );
    assert_eq!(
        unified_diff("a\n", "a\nc", 3),
        "--- baseline\n+++ current\n@@ -1 +1,2 @@\n a\n+c\n\\ No newline at end of file\n"
    );
}

/// Applies a unified diff the way `patch` does, checking every context and
/// removed line against `baseline`.
fn apply(baseline: &str, patch: &str) -> String {
    if patch.is_empty() {
        return baseline.to_string();
    }
    let old: Vec<&str> = baseline.split_inclusive('\n').collect();
    let mut out = String::new();
    let mut next_old = 0;
    let mut lines = patch.split_inclusive('\n').skip(2).peekable();
    while let Some(header) = lines.next() {
        let header = header.trim_end();
        let old_range = header
            .strip_prefix("@@ -")
            .and_then(|rest| rest.split(' ').next())
            .unwrap();
        let mut parts = old_range.split(',');
        let start: usize = parts.next().unwrap().parse().unwrap();
        let count: usize = parts.next().map_or(1, |count| count.parse().unwrap());
        let first = if count == 0 { start } else { start - 1 };
        for line in &old[next_old..first] {
            out.push_str(line);
        }
        next_old = first;
        while let Some(line) = lines.peek() {
            if line.starts_with("@@") {
                break;
            }
            let line = lines.next().unwrap();
            let mut body = line[1..].to_string();
            if lines.peek().is_some_and(|next| next.starts_with('\\')) {
                lines.next();
                body.pop();
            }
            match &line[..1] {
                " " => {
                    assert_eq!(old[next_old], body, "context line {next_old}");
                    out.push_str(&body);
                    next_old += 1;
                }
                "-" => {
                    assert_eq!(old[next_old], body, "removed line {next_old}");
                    next_old += 1;
                }
                "+" => out.push_str(&body),
                other => panic!("unexpected line {other:?}"),
            }
        }
    }
    for line in &old[next_old..] {
        out.push_str(line);
    }
    out
}

#[test]
fn the_diff_applies_back_to_the_current_text_property() {
    let mut seed = 3u64;
    let mut next = |bound: usize| {
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((seed >> 33) as usize) % bound
    };
    const WORDS: &[&str] = &["a", "b", "", "c d", "e"];
    for _ in 0..3000 {
        let mut texts = [String::new(), String::new()];
        for text in &mut texts {
            let count = next(9);
            *text = (0..count)
                .map(|_| WORDS[next(WORDS.len())])
                .collect::<Vec<_>>()
                .join("\n");
            if next(2) == 0 && !text.is_empty() {
                text.push('\n');
            }
        }
        let [baseline, current] = &texts;
        for context in [0, 1, 3] {
            let patch = unified_diff(baseline, current, context);
            assert_eq!(
                apply(baseline, &patch),
                *current,
                "{baseline:?} -> {current:?}, context {context}:\n{patch}"
            );
        }
    }
}

#[test]
fn large_documents_diff_quickly_and_correctly() {
    let baseline: String = (0..20_000).map(|n| format!("key_{n}: {n}\n")).collect();
    let current = baseline
        .replace("key_100: 100\n", "key_100: changed\n")
        .replace("key_15000: 15000\n", "");
    let patch = unified_diff(&baseline, &current, 3);
    assert_eq!(apply(&baseline, &patch), current);
    assert_eq!(patch.matches("\n@@ ").count(), 2, "{patch}");
}
