//! P1: every change to the document passes one gate. The edit pipeline
//! (`src/editor/edit/pipeline.rs`) is the only code that writes the
//! document's text, so every edit, whatever made it (a key, a command, a
//! paste, a drag, an undo, another document's paste, an Edit-as view),
//! is checked by the same span rule before it is made. Test modules may
//! set a text directly.

mod source_scan;

use std::fs;
use std::path::{Path, PathBuf};

use source_scan::{rust_files, test_cfgs, test_files, test_module};

/// The calls that write a signal.
const WRITES: [&str; 8] = [
    "set(",
    "update(",
    "write(",
    "try_set(",
    "try_update(",
    "set_untracked(",
    "update_untracked(",
    "maybe_update(",
];

/// The one file that may write the document.
const PIPELINE: &str = "editor/edit/pipeline.rs";

/// The lines of the code under `src` (test modules left out) that write
/// the document's text outside the pipeline.
fn writers(src: &Path) -> Vec<String> {
    let mut files = Vec::new();
    rust_files(src, &mut files);
    let tests = test_files(&files);
    let production: Vec<PathBuf> = files
        .into_iter()
        .filter(|file| {
            *file != src.join(PIPELINE)
                && !tests.contains(file)
                && !file.ancestors().any(|dir| tests.contains(dir))
        })
        .collect();
    let mut found = Vec::new();
    for file in production {
        let source = fs::read_to_string(&file).unwrap();
        let end = test_cfgs(&source)
            .find(|(_, rest)| matches!(test_module(rest), Some((_, true))))
            .map_or(source.len(), |(at, _)| at);
        for (index, line) in source[..end].lines().enumerate() {
            let code = line.split("//").next().unwrap_or_default();
            let writes = code.match_indices("doc().text.").any(|(at, matched)| {
                let call = &code[at + matched.len()..];
                WRITES.iter().any(|write| call.starts_with(write))
            });
            if writes {
                found.push(format!(
                    "{}:{}: {}",
                    file.strip_prefix(src).unwrap().display(),
                    index + 1,
                    line.trim()
                ));
            }
        }
    }
    found
}

#[test]
fn only_the_edit_pipeline_writes_the_document() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let found = writers(&src);
    assert!(
        found.is_empty(),
        "the document is written outside the edit pipeline, past the span check; \
         make the change an `Edit` and `commit` it:\n{}",
        found.join("\n")
    );
}

/// The check itself catches a write outside the pipeline, and lets the
/// pipeline, test modules, reads and comments through.
#[test]
fn the_check_catches_a_write_past_the_pipeline() {
    let dir = std::env::temp_dir().join(format!("one-edit-gate-{}", std::process::id()));
    let edit = dir.join("editor").join("edit");
    fs::create_dir_all(&edit).unwrap();
    fs::write(
        dir.join("lib.rs"),
        "#[cfg(test)]\nmod checks;\nfn f() { let t = state.doc().text.get(); } // doc().text.set(x)\n",
    )
    .unwrap();
    fs::write(
        dir.join("checks.rs"),
        "fn t() { state.doc().text.set(x); }\n",
    )
    .unwrap();
    fs::write(
        dir.join("editor").join("lines.rs"),
        "fn g() {\n    self.doc().text.update(|text| text.push('x'));\n}\n#[cfg(test)]\nmod tests { fn h() { state.doc().text.set(y); } }\n",
    )
    .unwrap();
    fs::write(
        edit.join("pipeline.rs"),
        "fn w() { self.doc().text.set(text); }\n",
    )
    .unwrap();
    let found = writers(&dir);
    fs::remove_dir_all(&dir).unwrap();
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(found[0].starts_with("editor/lines.rs:2:"), "{found:#?}");
}
