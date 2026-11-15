//! Some fields of a document by pointer (the secret ones, the host's
//! placeholders), looked up rather than compared with each problem.

use std::collections::BTreeSet;

/// Some fields of a document, by pointer: whether a path is one of them
/// or inside one is a lookup of each of its ancestors, and the ones inside
/// a path are a range of the sorted pointers, not a pass over them all
/// (thousands of secret placeholders, checked for each problem).
pub(super) struct Fields(BTreeSet<String>);

impl Fields {
    pub(super) fn new(pointers: Vec<String>) -> Self {
        super::super::work_count::note(super::super::work_count::Work::TokenScan);
        Self(pointers.into_iter().collect())
    }

    /// Whether `path` is one of the fields or inside one.
    pub(super) fn holds(&self, path: &str) -> bool {
        if self.0.is_empty() {
            return false;
        }
        let mut end = path.len();
        loop {
            if self.0.contains(&path[..end]) {
                return true;
            }
            if end == 0 {
                return false;
            }
            end = path[..end].rfind('/').unwrap_or(0);
        }
    }

    /// The fields strictly inside `path`, by pointer.
    pub(super) fn within<'s>(&'s self, path: &str) -> impl Iterator<Item = &'s str> + 's {
        let prefix = format!("{path}/");
        self.0
            .range(prefix.clone()..)
            .take_while(move |pointer| pointer.starts_with(&prefix))
            .map(String::as_str)
    }

    /// Whether some field is strictly inside `path`.
    pub(super) fn any_within(&self, path: &str) -> bool {
        !self.0.is_empty() && self.within(path).next().is_some()
    }
}
