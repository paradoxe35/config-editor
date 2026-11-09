//! What a tab shows of its file's name. A file's own name no other tab has
//! shows with its folder (`conf.d/`), which a narrow strip leaves out; a
//! name several tabs share (`limits.toml` in three folders) shows after the
//! end of its folder that tells it apart (`eu-west-1/production/`), the
//! file's own name kept whole, and so do the first letters of that folder
//! that no other such tab starts with (`eu-…limits.toml` beside
//! `eu/limits.toml`): a narrow tab never cuts two alike. The title and
//! accessible name keep the whole path.

/// What a tab shows of its file's name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TabLabel {
    /// The folder a narrow strip leaves out.
    pub(super) folder: Option<String>,
    /// The start's first letters, which tell it from the other tabs'
    /// with the same file's name: never cut.
    pub(super) kept: String,
    /// What a narrow tab cuts (with an ellipsis).
    pub(super) start: String,
    /// What stays whole after it.
    pub(super) end: String,
}

/// The file's own name in `name`: past its last `/` or backslash.
fn base_name(name: &str) -> &str {
    name.rfind(['/', '\\']).map_or(name, |at| &name[at + 1..])
}

/// The folders of `folder` (`a/b/`), innermost first.
fn folders_inward(folder: &str) -> Vec<&str> {
    folder
        .trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .filter(|_| !folder.is_empty())
        .collect()
}

/// The end of `name`'s folder that no other of `names` with the same
/// file's own name has (`eu-west-1/production/`): as few folders as tell
/// them apart, all of them when none do.
fn telling_folder<'a>(name: &'a str, names: &[String]) -> &'a str {
    let base = base_name(name);
    let folder = &name[..name.len() - base.len()];
    let own = folders_inward(folder);
    let others: Vec<Vec<&str>> = names
        .iter()
        .filter(|other| other.as_str() != name && base_name(other) == base)
        .map(|other| folders_inward(&other[..other.len() - base.len()]))
        .collect();
    let depth = (1..=own.len())
        .find(|&depth| {
            others
                .iter()
                .all(|other| other.get(..depth) != own.get(..depth))
        })
        .unwrap_or(own.len());
    // From the start of the `depth`-th folder counted inward.
    let kept: usize = own[..depth].iter().map(|folder| folder.len() + 1).sum();
    &folder[folder.len().saturating_sub(kept)..]
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

/// What the tab of `name` shows, among the tabs of `names`.
pub(super) fn tab_label(name: &str, names: &[String]) -> TabLabel {
    let base = base_name(name);
    let folder = &name[..name.len() - base.len()];
    let shared = names
        .iter()
        .filter(|other| base_name(other) == base)
        .nth(1)
        .is_some();
    if !shared || base.is_empty() {
        let (start, end) = split_end(base);
        return TabLabel {
            folder: (!folder.is_empty() && !base.is_empty()).then(|| folder.to_string()),
            kept: String::new(),
            start: if base.is_empty() { name } else { start }.to_string(),
            end: end.to_string(),
        };
    }
    let start = shared_start(name, names);
    let others: Vec<String> = names
        .iter()
        .filter(|other| other.as_str() != name && base_name(other) == base)
        .map(|other| shared_start(other, names))
        .collect();
    let kept = telling_letters(&start, &others);
    let (kept, start) = start.split_at(kept);
    TabLabel {
        folder: None,
        kept: kept.to_string(),
        start: start.to_string(),
        end: shared_end(base).to_string(),
    }
}

/// The file's own name of a shared one, split: what a narrow tab may cut
/// (a long name's start) and what stays whole.
fn shared_split(base: &str) -> (&str, &str) {
    match split_end(base) {
        (whole, "") => ("", whole),
        split => split,
    }
}

/// What stays whole of a shared file's name: its end.
fn shared_end(base: &str) -> &str {
    shared_split(base).1
}

/// The start a tab of `name` shows, when another of `names` has the same
/// file's name: the end of its folder that tells it apart, then what may
/// be cut of the file's name.
fn shared_start(name: &str, names: &[String]) -> String {
    let base = base_name(name);
    format!("{}{}", telling_folder(name, names), shared_split(base).0)
}

/// How many bytes of `start` a narrow tab keeps whole so that it never
/// reads as another of `others` cut: one letter past the longest start
/// it shares with any (all of it at most).
fn telling_letters(start: &str, others: &[String]) -> usize {
    let shared = others
        .iter()
        .map(|other| {
            start
                .chars()
                .zip(other.chars())
                .take_while(|(a, b)| a == b)
                .count()
        })
        .max()
        .unwrap_or(0);
    start
        .char_indices()
        .nth(shared + 1)
        .map_or(start.len(), |(at, _)| at)
}

#[cfg(test)]
mod tests {
    use super::{TabLabel, split_end, tab_label};

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

    fn label(folder: Option<&str>, start: &str, end: &str) -> TabLabel {
        TabLabel {
            folder: folder.map(String::from),
            kept: String::new(),
            start: start.into(),
            end: end.into(),
        }
    }

    fn shared(kept: &str, start: &str, end: &str) -> TabLabel {
        TabLabel {
            folder: None,
            kept: kept.into(),
            start: start.into(),
            end: end.into(),
        }
    }

    /// What a tab reads cut as far as a narrow tab cuts: its kept letters,
    /// an ellipsis for the start, its end.
    fn cut(label: &TabLabel) -> String {
        let ellipsis = if label.start.is_empty() { "" } else { "…" };
        format!("{}{ellipsis}{}", label.kept, label.end)
    }

    /// QA 26 (L26-U4): at phone width `eu-west-1/production/limits.toml`
    /// and `staging/eu/limits.toml` both read "e…limits.toml". Tabs with
    /// the same file's name, cut as far as a narrow tab cuts, never read
    /// the same.
    #[test]
    fn tabs_sharing_a_name_never_read_alike_cut() {
        let sets: [&[&str]; 5] = [
            &[
                "clusters/eu-west-1/production/limits.toml",
                "clusters/us-east-1/production/limits.toml",
                "prod/limits.toml",
                "staging/eu/limits.toml",
                "limits.toml",
            ],
            &["a/config.json", "ab/config.json", "abc/config.json"],
            &[
                "x/eu-west-1a/app.yaml",
                "x/eu-west-1b/app.yaml",
                "eu/app.yaml",
            ],
            &["é/ü/a.toml", "é/üü/a.toml", "ü/a.toml"],
            &[
                "x/service-config-production.yaml",
                "y/service-config-production.yaml",
                "xx/service-config-production.yaml",
            ],
        ];
        for set in sets {
            let names: Vec<String> = set.iter().map(|name| name.to_string()).collect();
            let cuts: Vec<String> = names
                .iter()
                .map(|name| cut(&tab_label(name, &names)))
                .collect();
            for (at, one) in cuts.iter().enumerate() {
                for other in &cuts[at + 1..] {
                    assert_ne!(one, other, "{set:?}: {cuts:?}");
                }
            }
            // Nothing is lost: the parts make the shown name.
            for name in &names {
                let label = tab_label(name, &names);
                let shown = format!("{}{}{}", label.kept, label.start, label.end);
                assert!(name.ends_with(&shown), "{name}: {label:?}");
            }
        }
    }

    /// QA 23 (L23-U7): on a phone the tabs showed "conf.d/new-…", all
    /// alike. The folder is the part a narrow strip leaves out, unless
    /// another tab has the same file's name. QA 25 (U25-8): tabs sharing a
    /// file's name showed the start of their folders ("clusters…", alike,
    /// or one letter), or cut the file's name ("prod/limits…"): they show
    /// the end of the folder that tells them apart, then the whole name.
    #[test]
    fn a_tab_shows_what_tells_its_file_apart() {
        let names: Vec<String> = [
            "burrow.yaml",
            "conf.d/new-1.yaml",
            "a/config.json",
            "b/config.json",
            "etc\\x.xml",
            "dé/ünï.toml",
            "dir/",
            "clusters/eu-west-1/production/limits.toml",
            "clusters/us-east-1/production/limits.toml",
            "prod/limits.toml",
            "staging/eu/limits.toml",
            "limits.toml",
            "x/service-config-production.yaml",
            "y/service-config-production.yaml",
        ]
        .map(String::from)
        .to_vec();
        let labels: Vec<TabLabel> = names.iter().map(|name| tab_label(name, &names)).collect();
        assert_eq!(
            labels,
            [
                label(None, "burrow.yaml", ""),
                label(Some("conf.d/"), "new-1.yaml", ""),
                shared("a", "/", "config.json"),
                shared("b", "/", "config.json"),
                label(Some("etc\\"), "x.xml", ""),
                label(Some("dé/"), "ünï.toml", ""),
                label(None, "dir/", ""),
                shared("eu-", "west-1/production/", "limits.toml"),
                shared("u", "s-east-1/production/", "limits.toml"),
                shared("p", "rod/", "limits.toml"),
                shared("eu/", "", "limits.toml"),
                shared("", "", "limits.toml"),
                shared("x", "/service-config-pro", "duction.yaml"),
                shared("y", "/service-config-pro", "duction.yaml"),
            ]
        );
    }
}
