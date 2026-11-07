//! Every text of `EditorStrings` is shown somewhere.

/// Every text is read somewhere outside its definition and the tests
/// (`strings.find`, `|strings| &strings.find`): a text nothing shows is a
/// text a host would translate for nothing. Lists each unused one.
#[test]
fn every_text_is_shown_somewhere() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut code = String::new();
    let mut folders = vec![root];
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(&folder).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            if path.is_dir() {
                folders.push(path);
            } else if name.ends_with(".rs") && !name.contains("test") && name != "strings.rs" {
                let source = std::fs::read_to_string(&path).unwrap();
                code.push_str(before_tests(&source));
            }
        }
    }
    let defaults = serde_json::to_value(super::EditorStrings::default()).unwrap();
    let unused: Vec<String> = defaults
        .as_object()
        .unwrap()
        .keys()
        .map(|key| snake_case(key))
        .filter(|field| !code.contains(&format!("strings.{field}")))
        .collect();
    assert!(unused.is_empty(), "texts nothing shows: {unused:#?}");
}

/// `source` up to its first block of tests (`#[cfg(test)]` before a line
/// opening a block); a test module declared on one line
/// (`#[cfg(test)] mod test_timing;`) does not end it.
fn before_tests(source: &str) -> &str {
    let mut from = 0;
    while let Some(found) = source[from..].find("#[cfg(test)]") {
        let at = from + found;
        let rest = &source[at..];
        let next = rest.lines().nth(1).unwrap_or_default();
        if next.trim_end().ends_with('{') {
            return &source[..at];
        }
        from = at + "#[cfg(test)]".len();
    }
    source
}

fn snake_case(key: &str) -> String {
    let mut out = String::new();
    for character in key.chars() {
        if character.is_ascii_uppercase() {
            out.push('_');
            out.push(character.to_ascii_lowercase());
        } else {
            out.push(character);
        }
    }
    out
}
