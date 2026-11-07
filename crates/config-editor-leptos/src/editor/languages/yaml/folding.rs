//! Fold ranges: mappings, sequences and block scalars by indentation, and
//! runs of comments.

use std::ops::Range;

use super::super::super::language::{
    FoldRange, IndentLine, comment_fold_ranges, indent_fold_ranges, merge_fold_ranges,
};

pub(super) fn yaml_fold_ranges(source: &str, bodies: &[Range<usize>]) -> Vec<FoldRange> {
    let in_body = |index: usize| bodies.iter().any(|body| body.contains(&index));
    let mut ranges = indent_fold_ranges(
        source,
        |index, line| {
            if in_body(index) {
                IndentLine::Text
            } else if line.trim_start().starts_with('#') {
                IndentLine::Skip
            } else {
                IndentLine::Code
            }
        },
        sequence_under_key,
    );
    ranges.extend(comment_fold_ranges(source, &["#"], bodies));
    merge_fold_ranges(ranges, source.split('\n').count())
}

/// A sequence written at its key's indent (`key:` then `- a`) is inside
/// the key's fold.
fn sequence_under_key(first: &str, line: &str) -> bool {
    let item = |text: &str| text == "-" || text.starts_with("- ");
    let first = first.trim_start();
    let code = first.split(" #").next().unwrap_or(first).trim_end();
    item(line.trim_start()) && !item(first) && code.ends_with(':')
}
