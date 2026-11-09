//! The editor is generic and embeddable: what its users read (the READMEs,
//! the changelog, rustdoc, the shipped JS files and typings, the examples,
//! the demo and its samples, the screenshot scenes and gallery) never names
//! one particular product or person (the samples' hosts and machines are
//! RFC 2606 names and placeholders) and never assumes the host has a server
//! or a disk.
//! A server may stay as one example among several ("a service, a build
//! step"); config data (a `server:` key) and the YAML language server are
//! not about the host.

use std::fs;
use std::path::{Path, PathBuf};

/// A product the samples once came from; no user-facing place names it.
const PRODUCT: &str = "burrow";

/// People's own domains and machines the samples once used; the samples
/// use RFC 2606 names (`example.com`) and placeholder people instead.
const PERSONAL: [&str; 4] = ["pngwasi.me", "gracecarry.shop", "paradoxe-", "gaby-"];

/// Phrases that tell the reader the host keeps its files on a server or a
/// disk.
const ASSUMED: [&str; 17] = [
    "on the server",
    "on a server",
    "on disk",
    "a server's",
    "the server's",
    "your server",
    "changed-on-server",
    "from the server",
    "server pushed",
    "server answered",
    "when a server",
    "by the server",
    "server change",
    "change on server",
    "server problems",
    "server's validation",
    "to a server",
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every file under `dir` (relative to the repository) whose name ends with
/// one of `ends`, skipping installed and built folders.
fn files(dir: &str, ends: &[&str]) -> Vec<PathBuf> {
    fn walk(dir: &Path, ends: &[&str], out: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if path.is_dir() {
                if !matches!(
                    name.as_str(),
                    "node_modules" | "dist" | "pkg" | "target" | ".svelte-kit" | "tests"
                ) {
                    walk(&path, ends, out);
                }
            } else if ends.iter().any(|end| name.ends_with(end)) {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    walk(&root().join(dir), ends, &mut out);
    out.sort();
    out
}

fn file(path: &str) -> Vec<PathBuf> {
    vec![root().join(path)]
}

/// The doc comments of a Rust file (`///` and `//!` lines), other lines
/// blank so line numbers stay.
fn rustdoc(text: &str) -> String {
    text.lines()
        .map(|line| {
            let line = line.trim_start();
            if line.starts_with("///") || line.starts_with("//!") {
                line
            } else {
                ""
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The comments of a TOML file, other text blank so line numbers stay.
fn toml_comments(text: &str) -> String {
    text.lines()
        .map(|line| line.split_once('#').map_or("", |(_, comment)| comment))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The docs users read as prose: the whole file.
fn docs() -> Vec<PathBuf> {
    let mut out = Vec::new();
    for path in [
        "README.md",
        "CHANGELOG.md",
        "crates/config-editor-leptos/README.md",
        "js/README.md",
        "js/config-editor/README.md",
        "js/config-editor-react/README.md",
        "js/config-editor-vue/README.md",
        "js/config-editor-svelte/README.md",
        "docs/screenshots/README.md",
        "e2e/screenshots.mjs",
        "e2e/screenshots-gallery.mjs",
        "demo.css",
        "index.html",
    ] {
        out.extend(file(path));
    }
    out.extend(files("src", &[".rs"]));
    // What the packages ship: JS, typings and the Rust that writes typings.
    out.extend(files("js/config-editor", &[".js", ".d.ts", ".rs"]));
    for package in [
        "js/config-editor-react/src",
        "js/config-editor-vue/src",
        "js/config-editor-svelte/src",
    ] {
        out.extend(files(package, &[".ts", ".tsx", ".svelte"]));
    }
    out
}

/// Pages and data a user sees, where a server may be simulated or be config
/// data, but no product is named.
fn shown_data() -> Vec<PathBuf> {
    let mut out = files("js/examples", &[".html", ".ts", ".tsx", ".svelte"]);
    out.extend(files("js/typecheck", &[".ts", ".tsx", ".svelte", ".vue"]));
    // The strict-CSP pages, whose schema says the id is "Assigned by the server".
    out.extend(files("e2e/csp", &[".js", ".html"]));
    out.extend(file("schema.json"));
    out.extend(file("samples/generate.py"));
    out.extend(
        fs::read_dir(root().join("crates/config-editor-leptos/tests/fixtures/samples"))
            .expect("samples")
            .flatten()
            .map(|entry| entry.path()),
    );
    out
}

/// `(file, text)` for every place checked for `ASSUMED` phrases.
fn prose() -> Vec<(PathBuf, String)> {
    let mut out: Vec<(PathBuf, String)> = docs()
        .into_iter()
        .map(|path| {
            let text = fs::read_to_string(&path).expect("readable");
            (path, text)
        })
        .collect();
    for path in files("crates/config-editor-leptos/src", &[".rs"]) {
        let text = rustdoc(&fs::read_to_string(&path).expect("readable"));
        out.push((path, text));
    }
    for path in [
        "crates/config-editor-leptos/Cargo.toml",
        "js/config-editor/Cargo.toml",
    ] {
        let path = root().join(path);
        let text = toml_comments(&fs::read_to_string(&path).expect("readable"));
        out.push((path, text));
    }
    out
}

fn found(places: &[(PathBuf, String)], words: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    for (path, text) in places {
        for (number, line) in text.lines().enumerate() {
            let lower = line.to_lowercase();
            // Server rendering (SSR) is about the page, not the host's files.
            let rendering = lower.contains("ssr") || lower.contains("server render");
            if !rendering && words.iter().any(|word| lower.contains(word)) {
                let path = path.strip_prefix(root()).unwrap_or(path);
                out.push(format!(
                    "{}:{}: {}",
                    path.display(),
                    number + 1,
                    line.trim()
                ));
            }
        }
    }
    out
}

#[test]
fn no_user_facing_place_names_one_product() {
    let mut places = prose();
    for path in shown_data() {
        let text = fs::read_to_string(&path).expect("readable");
        places.push((path, text));
    }
    assert!(places.len() > 100, "only {} files read", places.len());
    let hits = found(&places, &[PRODUCT]);
    assert!(hits.is_empty(), "a product's name:\n{}", hits.join("\n"));
}

#[test]
fn no_user_facing_place_names_a_person() {
    let mut places = prose();
    for path in shown_data() {
        let text = fs::read_to_string(&path).expect("readable");
        places.push((path, text));
    }
    let hits = found(&places, &PERSONAL);
    assert!(
        hits.is_empty(),
        "a person's domain or machine (use example.com, example.net, example.org):\n{}",
        hits.join("\n")
    );
}

/// The example pages' controls, which a user reads as UI (a button that
/// moves the baseline on), say what happened, not where.
#[test]
fn no_example_page_assumes_the_host_has_a_server_or_a_disk() {
    let mut places = Vec::new();
    for path in files("js/examples", &[".html", ".ts", ".tsx", ".svelte"]) {
        let text = fs::read_to_string(&path).expect("readable");
        places.push((path, text));
    }
    assert!(places.len() > 10, "only {} pages read", places.len());
    let hits = found(&places, &ASSUMED);
    assert!(
        hits.is_empty(),
        "host-specific wording (\"External change\"):\n{}",
        hits.join("\n")
    );
}

#[test]
fn no_doc_assumes_the_host_has_a_server_or_a_disk() {
    let places = prose();
    let hits = found(&places, &ASSUMED);
    assert!(
        hits.is_empty(),
        "host-specific wording (say \"the host\", \"outside the editor\"):\n{}",
        hits.join("\n")
    );
}

#[test]
fn the_check_reads_the_places_it_names() {
    // A moved file would silently drop out of the check.
    for path in docs() {
        assert!(path.is_file(), "{} is missing", path.display());
    }
    let docs = docs();
    for expected in [
        "README.md",
        "typings.rs",
        "index.tsx",
        "ConfigEditor.ts",
        "types.ts",
        "main.rs",
    ] {
        assert!(
            docs.iter().any(|path| path.ends_with(expected)),
            "{expected} is not checked"
        );
    }
}
