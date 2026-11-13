//! Tabs' labels (see `tab_names`): what tells tabs sharing a file's name
//! apart, cut or whole, and what working them out costs.

use super::{TabLabel, TabNames, folder_end, split_end, tab_label};

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
    // QA 30: a folder written with two separators at its end
    // (`é//a.toml`) cut a letter in two and panicked.
    let sets: [&[&str]; 6] = [
        &[
            "é//a.toml",
            "x/a.toml",
            "ab//a.toml",
            "b//a.toml",
            "ü\\\\a.toml",
        ],
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
            .map(|name| cut(&tab_label(name, &TabNames::new(names.clone()))))
            .collect();
        for (at, one) in cuts.iter().enumerate() {
            for other in &cuts[at + 1..] {
                assert_ne!(one, other, "{set:?}: {cuts:?}");
            }
        }
        // Nothing is lost: the parts make the shown name.
        for name in &names {
            let label = tab_label(name, &TabNames::new(names.clone()));
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
    let labels: Vec<TabLabel> = names
        .iter()
        .map(|name| tab_label(name, &TabNames::new(names.clone())))
        .collect();
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

/// Labelling every tab reads each name a few times, however many tabs
/// (round 27's `D27.memory-1`: each tab read every other name, a re-list
/// of 1,000 renamed files froze the page for 45 s).
#[test]
fn labelling_every_tab_reads_each_name_a_few_times() {
    use crate::editor::work_count::{Work, counted};
    let visits = |count: usize| {
        // Every name its own but five that share one.
        let names: Vec<String> = (0..count)
            .map(|index| {
                if index < 5 {
                    format!("dir{index}/shared.yaml")
                } else {
                    format!("dir{index}/file{index}.yaml")
                }
            })
            .collect();
        counted(Work::TabNameVisit, || {
            let index = TabNames::new(names.clone());
            for name in &names {
                let _ = tab_label(name, &index);
            }
        })
        .1
    };
    let (few, many) = (visits(100), visits(1_000));
    assert!(
        many <= 12 * few,
        "{few} visits for 100 tabs, {many} for 1,000"
    );
}

/// Where `name`'s own file name starts.
fn base_at(name: &str) -> usize {
    name.rfind(['/', '\\']).map_or(0, |at| at + 1)
}

/// The folders of `folder`, innermost first.
fn inward(folder: &str) -> Vec<&str> {
    folder
        .trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .filter(|_| !folder.is_empty())
        .collect()
}

/// The other names of `names` with `name`'s own file name.
fn alike<'a>(name: &str, names: &'a [String]) -> Vec<&'a String> {
    names
        .iter()
        .filter(|other| other.as_str() != name && other[base_at(other)..] == name[base_at(name)..])
        .collect()
}

/// The start of `name`'s label among `names`, worked out against every
/// other name: the end of its folder no other alike has, then what may be
/// cut of its file's name.
fn reference_start(name: &str, names: &[String]) -> String {
    let (folder, base) = name.split_at(base_at(name));
    let own = inward(folder);
    let others: Vec<Vec<&str>> = alike(name, names)
        .into_iter()
        .map(|other| inward(&other[..base_at(other)]))
        .collect();
    let depth = (1..=own.len())
        .find(|&depth| {
            others
                .iter()
                .all(|other| other.get(..depth) != own.get(..depth))
        })
        .unwrap_or(own.len());
    let cut = match split_end(base) {
        (_, "") => "",
        (start, _) => start,
    };
    format!("{}{cut}", folder_end(folder, depth))
}

/// The label of `name` worked out tab by tab, each looking through every
/// other name (how labels were worked out until QA 30): what labelling a
/// group at a time must match.
fn reference(name: &str, names: &[String]) -> TabLabel {
    let base = &name[base_at(name)..];
    let others = alike(name, names);
    if others.is_empty() || base.is_empty() {
        return tab_label(name, &TabNames::default());
    }
    let start = reference_start(name, names);
    let shared = others
        .iter()
        .map(|other| {
            let other = reference_start(other, names);
            start
                .chars()
                .zip(other.chars())
                .take_while(|(a, b)| a == b)
                .count()
        })
        .max()
        .unwrap_or(0);
    let kept = start
        .char_indices()
        .nth(shared + 1)
        .map_or(start.len(), |(at, _)| at);
    let end = match split_end(base) {
        (whole, "") => whole,
        (_, end) => end,
    };
    TabLabel {
        folder: None,
        kept: start[..kept].to_string(),
        start: start[kept..].to_string(),
        end: end.to_string(),
    }
}

struct Lcg(u64);

impl Lcg {
    fn next(&mut self, bound: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 33) as usize) % bound.max(1)
    }
}

/// Labels worked out a group at a time, and relabelled as names come, go
/// and change, are the labels worked out tab by tab against every other
/// name: random names over few folders and file names (folders alike
/// nested, backslashes, non-ASCII, long names, duplicates).
#[test]
fn labels_per_group_match_labels_tab_by_tab() {
    const FOLDERS: &[&str] = &["a", "ab", "eu", "eu-west-1", "é", "ü", "x", "prod", ""];
    const FILES: &[&str] = &[
        "values.yaml",
        "a.toml",
        "service-config-production.yaml",
        "",
    ];
    let mut random = Lcg(30);
    let mut random_name = move || {
        let mut name = String::new();
        for _ in 0..random.next(4) {
            name.push_str(FOLDERS[random.next(FOLDERS.len())]);
            name.push(if random.next(6) == 0 { '\\' } else { '/' });
        }
        name.push_str(FILES[random.next(FILES.len())]);
        (name, random.next(4), random.next(16))
    };
    let mut names = TabNames::default();
    let mut list: Vec<String> = Vec::new();
    for _ in 0..1_500 {
        // One name added, closed or renamed, or a new list.
        let (name, change, at) = random_name();
        match change {
            0 if !list.is_empty() => {
                list.remove(at % list.len());
            }
            1 if !list.is_empty() => {
                let at = at % list.len();
                list[at] = name;
            }
            2 if list.len() > 12 => list.clear(),
            _ => list.push(name),
        }
        if let Some(next) = names.relabel(&list) {
            names = next;
        }
        for name in &list {
            assert_eq!(
                tab_label(name, &names),
                reference(name, &list),
                "{name} in {list:?}"
            );
        }
    }
}
