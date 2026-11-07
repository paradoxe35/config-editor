//! Fold ranges: tables and arrays of tables from their header to the next
//! header, multi-line arrays, inline tables and strings, and runs of
//! comments.

use std::ops::Range;

use super::super::super::folding::scan_brackets;
use super::super::super::language::{
    FoldRange, LanguageConfig, comment_fold_ranges, merge_fold_ranges, string_body_fold_ranges,
};

pub(super) fn toml_fold_ranges(
    source: &str,
    config: &LanguageConfig,
    bodies: &[Range<usize>],
) -> Vec<FoldRange> {
    let scan = scan_brackets(source, config, &[]);
    let lines: Vec<&str> = source.split('\n').collect();
    let headers: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(index, line)| scan.at_top[*index] && line.trim_start().starts_with('['))
        .map(|(index, _)| index)
        .collect();
    let mut ranges = scan.ranges;
    for (position, &header) in headers.iter().enumerate() {
        let next = headers.get(position + 1).copied().unwrap_or(lines.len());
        // Blank lines and the comments that lead into the next table stay
        // out of the fold.
        let last = (header + 1..next).rev().find(|&index| {
            let trimmed = lines[index].trim_start();
            !trimmed.is_empty() && !trimmed.starts_with('#')
        });
        if let Some(last) = last {
            ranges.push(FoldRange::new(header, last));
        }
    }
    ranges.extend(comment_fold_ranges(source, &["#"], bodies));
    ranges.extend(string_body_fold_ranges(bodies));
    merge_fold_ranges(ranges, lines.len())
}
