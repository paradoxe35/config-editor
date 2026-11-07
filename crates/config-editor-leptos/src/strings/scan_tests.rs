//! No view writes a text of its own: a scan of the crate's `view!` macros
//! for literals a person reads.

/// Literals a view may show: symbols, not words.
const ALLOWED_LITERALS: &[&str] = &["Aa", "ab"];

/// The attributes whose values people read.
const TEXT_ATTRIBUTES: &[&str] = &["title", "aria-label", "placeholder", "label", "alt"];

/// The string literals of `source`'s `view!` macros that a person reads:
/// text nodes and text attributes (doc comments and tests left out).
fn view_texts(source: &str) -> Vec<String> {
    let code: String = source
        .split("#[cfg(test)]")
        .next()
        .unwrap_or_default()
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut found = Vec::new();
    let mut rest = code.as_str();
    while let Some(at) = rest.find("view! {") {
        let body = &rest[at + "view! {".len()..];
        let mut depth = 1usize;
        let end = body
            .char_indices()
            .find(|(_, character)| {
                match character {
                    '{' => depth += 1,
                    '}' => depth -= 1,
                    _ => {}
                }
                depth == 0
            })
            .map_or(body.len(), |(index, _)| index);
        found.extend(texts_in(&body[..end]));
        rest = &body[end..];
    }
    found
}

/// The read literals of one `view!` body (nested ones included).
fn texts_in(body: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut index = 0;
    while let Some(open) = body[index..].find('"') {
        let start = index + open;
        let Some(length) = body[start + 1..].find('"') else {
            break;
        };
        let literal = &body[start + 1..start + 1 + length];
        index = start + length + 2;
        let before = body[..start].trim_end();
        // `format!("…")` as an attribute value: its text outside `{…}`.
        let (before, literal) = match before.strip_suffix("format!(") {
            Some(before) => (
                before.trim_end().trim_end_matches("move ||").trim_end(),
                literal
                    .split('{')
                    .map(|piece| piece.split_once('}').map_or(piece, |(_, after)| after))
                    .collect::<String>(),
            ),
            None => (before, literal.to_string()),
        };
        let text_node = before.ends_with('>');
        let text_attribute = TEXT_ATTRIBUTES
            .iter()
            .any(|name| before.ends_with(&format!(" {name}=")));
        let has_words = literal.chars().any(char::is_alphabetic);
        if (text_node || text_attribute) && has_words && !ALLOWED_LITERALS.contains(&&*literal) {
            found.push(literal);
        }
    }
    found
}

#[test]
fn the_scan_finds_view_texts_and_skips_the_rest() {
    let source = r#"
        /// view! { <p>"In a doc comment"</p> }
        fn a() -> impl IntoView {
            view! {
                <div class="config-editor__x" role="status" title="A tooltip">
                    <span>"Shown text"</span>
                    "×"
                    <button aria-label=move || format!("{} source", label())>"Aa"</button>
                    {move || view! { <b>"Nested"</b> }}
                </div>
            }
        }
        #[cfg(test)]
        mod tests { fn b() { view! { <p>"Test only"</p> } } }
    "#;
    assert_eq!(
        view_texts(source),
        ["A tooltip", "Shown text", " source", "Nested"]
    );
}

/// Every text a view shows comes from the `strings` setting: lists each
/// file and literal that does not.
#[test]
fn views_take_every_text_from_the_strings_setting() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    let mut folders = vec![root.clone()];
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(&folder).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                folders.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                files.push(path);
            }
        }
    }
    files.sort();
    let mut failures = Vec::new();
    for path in &files {
        let source = std::fs::read_to_string(path).unwrap();
        for text in view_texts(&source) {
            let file = path.strip_prefix(&root).unwrap().display();
            failures.push(format!("{file}: {text:?}"));
        }
    }
    assert!(files.len() > 50, "{} files", files.len());
    assert!(
        failures.is_empty(),
        "texts written in views instead of `EditorStrings`: {failures:#?}"
    );
}
