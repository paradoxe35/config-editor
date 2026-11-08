//! A tab's name in two parts: its folder (`conf.d/`) and the file's own
//! name. A narrow tab strip leaves the folder out, so the names that tell
//! the tabs apart show (the title and accessible name keep the whole); a
//! file's name that two tabs share keeps its folder.

/// The file's own name in `name`: past its last `/` or backslash.
fn base_name(name: &str) -> &str {
    name.rfind(['/', '\\']).map_or(name, |at| &name[at + 1..])
}

/// The files' own names more than one of `names` have, sorted.
pub(super) fn shared_base_names(names: &[String]) -> Vec<String> {
    let mut bases: Vec<&str> = names.iter().map(|name| base_name(name)).collect();
    bases.sort_unstable();
    let mut shared: Vec<String> = bases
        .windows(2)
        .filter(|pair| pair[0] == pair[1])
        .map(|pair| pair[0].to_string())
        .collect();
    shared.dedup();
    shared
}

/// `name` as its folder (when one may be left out: a file's own name no
/// other tab has) and the rest.
pub(super) fn split_name<'a>(name: &'a str, shared: &[String]) -> (Option<&'a str>, &'a str) {
    let base = base_name(name);
    let folder = &name[..name.len() - base.len()];
    if folder.is_empty() || base.is_empty() || shared.iter().any(|other| other == base) {
        return (None, name);
    }
    (Some(folder), base)
}

#[cfg(test)]
mod tests {
    use super::{shared_base_names, split_name};

    /// QA 23 (L23-U7): on a phone the tabs showed "conf.d/new-…", all
    /// alike. The folder is the part a narrow strip leaves out, unless
    /// another tab has the same file's name.
    #[test]
    fn a_tab_name_splits_off_its_folder_unless_another_tab_shares_the_rest() {
        let names: Vec<String> = [
            "burrow.yaml",
            "conf.d/new-1.yaml",
            "conf.d/new-2.yaml",
            "a/config.json",
            "b/config.json",
            "etc\\x.xml",
            "dé/ünï.toml",
            "dir/",
        ]
        .map(String::from)
        .to_vec();
        let shared = shared_base_names(&names);
        assert_eq!(shared, ["config.json"]);
        let split: Vec<(Option<&str>, &str)> =
            names.iter().map(|name| split_name(name, &shared)).collect();
        assert_eq!(
            split,
            [
                (None, "burrow.yaml"),
                (Some("conf.d/"), "new-1.yaml"),
                (Some("conf.d/"), "new-2.yaml"),
                (None, "a/config.json"),
                (None, "b/config.json"),
                (Some("etc\\"), "x.xml"),
                (Some("dé/"), "ünï.toml"),
                (None, "dir/"),
            ]
        );
    }
}
