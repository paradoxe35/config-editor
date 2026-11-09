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

/// How many characters a long name's end keeps whole.
const END_CHARS: usize = 12;

/// `name` as the start a narrow tab cuts (with an ellipsis) and the end it
/// keeps whole, so long names sharing a start still differ on screen
/// (`service-con…production.yaml`). A short name is all start.
pub(super) fn split_end(name: &str) -> (&str, &str) {
    if name.chars().count() <= END_CHARS + 4 {
        return (name, "");
    }
    let at = name
        .char_indices()
        .rev()
        .nth(END_CHARS - 1)
        .map_or(0, |(at, _)| at);
    name.split_at(at)
}

#[cfg(test)]
mod tests {
    use super::{shared_base_names, split_end, split_name};

    /// QA 24: long names sharing a start were cut alike at phone width;
    /// the end that tells them apart stays whole.
    #[test]
    fn a_long_name_keeps_its_end_whole() {
        assert_eq!(split_end("burrow.yaml"), ("burrow.yaml", ""));
        assert_eq!(
            split_end("service-config-production.yaml"),
            ("service-config-pro", "duction.yaml")
        );
        let long = "x".repeat(40);
        for name in [
            "a-very-long-shared-start-staging.yaml",
            long.as_str(),
            "ééééééééééééééééééé",
            "dé-ünï-long-naming-ü.toml",
        ] {
            let (start, end) = split_end(name);
            assert_eq!(format!("{start}{end}"), name);
            assert_eq!(end.chars().count(), 12, "{end:?}");
        }
    }

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
