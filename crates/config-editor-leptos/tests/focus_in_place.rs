//! A plain `element.focus()` scrolls the host page to the element. Every
//! focus the editor gives itself goes through `crate::focus::in_place`
//! (preventScroll): clippy bans the browser's `focus()` (which a text area's,
//! input's or button's `focus()` resolves to), and no `.focus()` call in the
//! sources reads as one: the editor's wrappers are named `focus_in_place`,
//! and the only `.focus()` left is the public handles' own.

use std::path::{Path, PathBuf};

fn rust_files(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read source folder") {
        let path = entry.expect("read source entry").path();
        if path.is_dir() {
            rust_files(&path, found);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
}

/// The `.focus()` and `.focus_with_options(` calls in `text` not made on a
/// handle (`handle.focus()`, `self.state.handle.focus()`), as `line: text`.
fn plain_focus_calls(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    for (number, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") {
            continue;
        }
        for call in [".focus()", ".focus_with_options("] {
            let mut from = 0;
            while let Some(at) = line[from..].find(call) {
                let at = from + at;
                from = at + call.len();
                let on_handle = call == ".focus()" && line[..at].ends_with("handle");
                if !on_handle {
                    found.push(format!("{}: {}", number + 1, trimmed));
                }
            }
        }
    }
    found
}

#[test]
fn the_check_catches_a_plain_focus() {
    assert_eq!(plain_focus_calls("    let _ = element.focus();").len(), 1);
    assert_eq!(
        plain_focus_calls("let _ = input.focus_with_options(&o);").len(),
        1
    );
    assert!(plain_focus_calls("    handle.focus();").is_empty());
    assert!(plain_focus_calls("        self.state.handle.focus();").is_empty());
    assert!(plain_focus_calls("    let _ = element.focus_in_place();").is_empty());
    assert!(plain_focus_calls("//! `element.focus()` scrolls").is_empty());
}

#[test]
fn every_focus_goes_through_focus_in_place() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    rust_files(&root.join("src"), &mut files);
    let js = root.join("../../js/config-editor/src");
    if js.is_dir() {
        rust_files(&js, &mut files);
    }
    let failures: Vec<String> = files
        .iter()
        .filter(|path| !path.ends_with("src/focus/mod.rs"))
        .flat_map(|path| {
            let text = std::fs::read_to_string(path).expect("read source file");
            plain_focus_calls(&text)
                .into_iter()
                .map(move |line| format!("{}:{line}", path.display()))
        })
        .collect();
    assert!(
        failures.is_empty(),
        "focus with crate::focus::in_place (or a `focus_in_place` wrapper), which keeps the page still:\n{}",
        failures.join("\n")
    );
}

#[test]
fn clippy_bans_the_browsers_focus() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let config = std::fs::read_to_string(root.join("clippy.toml")).expect("read clippy.toml");
    for banned in [
        "path = \"web_sys::HtmlElement::focus\"",
        "path = \"web_sys::HtmlElement::focus_with_options\"",
    ] {
        assert!(config.contains(banned), "clippy.toml bans {banned}");
    }
}
