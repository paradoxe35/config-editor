//! A strict Content-Security-Policy (`style-src 'self'`, no
//! `'unsafe-inline'`) refuses `style` attributes set as text: every inline
//! style goes through the CSSOM (`style=cssom(…)` or `style:<name>=`), and
//! the preset ships as a file a host can serve itself.

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

/// The `style=` attributes in `text` not written as `style=cssom(`, and any
/// `style` attribute set by name, as `line: text`.
fn style_attribute_writes(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    for (number, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") {
            continue;
        }
        let bytes = line.as_bytes();
        let mut from = 0;
        while let Some(at) = line[from..].find("style=") {
            let at = from + at;
            from = at + "style=".len();
            let before = at.checked_sub(1).map(|index| bytes[index]);
            let attribute = before.is_none_or(|byte| byte == b' ' || byte == b'\t' || byte == b'<');
            let comparison = bytes.get(from) == Some(&b'=');
            if attribute && !comparison && !line[from..].starts_with("cssom(") {
                found.push(format!("{}: {}", number + 1, trimmed));
            }
        }
        if line.contains("set_attribute(\"style\"") || line.contains("setAttribute(\"style\"") {
            found.push(format!("{}: {}", number + 1, trimmed));
        }
    }
    found
}

#[test]
fn no_style_attribute_is_written_as_text() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    rust_files(&root.join("src"), &mut files);
    // The JS package's own Rust, when built from the repository.
    let js = root.join("../../js/config-editor/src");
    if js.is_dir() {
        rust_files(&js, &mut files);
    }
    let failures: Vec<String> = files
        .iter()
        .flat_map(|path| {
            let text = std::fs::read_to_string(path).expect("read source file");
            style_attribute_writes(&text)
                .into_iter()
                .map(move |found| format!("{}:{found}", path.display()))
        })
        .collect();
    assert!(
        failures.is_empty(),
        "style attributes written as text (a strict CSP refuses them; use style=cssom(Declarations) or style:<name>=):\n{}",
        failures.join("\n")
    );
}

#[test]
fn the_scan_finds_text_styles_and_lets_cssom_through() {
    let found = style_attribute_writes(concat!(
        "<div style=format!(\"left: 1px\")>\n",
        "    style=move || shown.color_style()\n",
        "    style=cssom(move || shown.colors())\n",
        "    style:left=\"1px\"\n",
        "let style = 1; if style==2 {}\n",
        "el.set_attribute(\"style\", \"x\");\n",
        "// style=\"a comment\"\n",
    ));
    assert_eq!(found.len(), 3, "{found:#?}");
}

#[test]
fn the_shipped_preset_css_matches_preset_css() {
    let shipped = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("preset.css"))
        .expect("preset.css next to Cargo.toml (`make docs-generate` writes it)");
    assert!(
        shipped == config_editor_leptos::PRESET_CSS,
        "preset.css is stale: run `make docs-generate`"
    );
}

/// `default-src 'self'` also covers images: a `url()` in the preset (a
/// `data:` icon) would be refused, so its icons are inline SVG markup.
#[test]
fn the_preset_loads_nothing() {
    let urls: Vec<&str> = config_editor_leptos::PRESET_CSS
        .lines()
        .filter(|line| line.contains("url(") || line.contains("@import"))
        .collect();
    assert!(
        urls.is_empty(),
        "the preset loads files a strict CSP may refuse:\n{}",
        urls.join("\n")
    );
}
