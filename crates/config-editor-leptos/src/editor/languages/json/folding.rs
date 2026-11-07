//! Fold ranges: objects and arrays over several lines.

use super::super::super::language::{
    FoldRange, LanguageConfig, bracket_fold_ranges, merge_fold_ranges,
};

pub(super) fn json_fold_ranges(source: &str, config: &LanguageConfig) -> Vec<FoldRange> {
    merge_fold_ranges(
        bracket_fold_ranges(source, config, &[]),
        source.split('\n').count(),
    )
}
