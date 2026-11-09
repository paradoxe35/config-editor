//! Fold ranges: blocks, objects, lists and calls over several lines,
//! heredocs, block comments and runs of line comments.

use std::ops::Range;

use super::super::super::language::{
    FoldRange, LanguageConfig, bracket_fold_ranges, comment_fold_ranges, merge_fold_ranges,
    string_body_fold_ranges,
};

/// The brackets that fold, function calls' parentheses included.
const FOLD_SYNTAX: LanguageConfig = LanguageConfig {
    line_comments: &["#", "//"],
    block_comment: Some(("/*", "*/")),
    brackets: &[('{', '}'), ('[', ']'), ('(', ')')],
    separators: &[],
    quotes: &["\""],
    literal_quotes: &[],
        map_brackets: None,
    list_brackets: None,
    item_separator: None,
    item_marker: None,
        written_separator: None,
};

/// `heredocs` are the heredoc bodies, skipped by the bracket scan.
pub(super) fn hcl_fold_ranges(source: &str, heredocs: &[Range<usize>]) -> Vec<FoldRange> {
    let mut ranges = bracket_fold_ranges(source, &FOLD_SYNTAX, heredocs);
    ranges.extend(comment_fold_ranges(
        source,
        FOLD_SYNTAX.line_comments,
        heredocs,
    ));
    ranges.extend(string_body_fold_ranges(heredocs));
    merge_fold_ranges(ranges, source.split('\n').count())
}
