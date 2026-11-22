//! A quoted value left alone in an Edit-as view keeps its host text
//! (`driver: "json-file"`, TOML's `'literal'`, an escape) when the same
//! save moves other values next to it, out of a map a YAML merge or alias
//! reads: in every host and every other view.

use super::super::codec;
use super::super::edit::Edit;
use super::super::languages::Language;
use super::moved_forms::{pause, units};
use crate::editor::save_matrix::{ALL, report};
use crate::editor::test_state::TestEditor;
use leptos::prelude::*;
use serde_json::Value;

/// A host text, the value moved (from, to), and the host spellings the
/// save must keep: each spelling left alone, as often as the text writes
/// it before the save (a merge written out writes its scalars again, in
/// their host spelling). The value moved is written anew by the view's
/// writer, with no record of where it came from: only its neighbours are
/// promised their text.
struct Case {
    label: &'static str,
    host: Language,
    text: &'static str,
    moved: (&'static str, &'static str),
    kept: &'static [&'static str],
}

const CASES: &[Case] = &[
    Case {
        label: "YAML merge parted",
        host: Language::YAML,
        text: "x-common: &common\n  restart: unless-stopped # always back\n  logging:\n    driver: \"json-file\"\n    tag: 'edge'\nservices:\n  web:\n    <<: *common\n    image: 'nginx:1.25'\n  db:\n    image: postgres:16\n",
        moved: ("/services/web/image", "/x-common/logging/image"),
        kept: &["\"json-file\"", "'edge'"],
    },
    Case {
        label: "YAML alias parted",
        host: Language::YAML,
        text: "x-log: &log\n  driver: \"json-file\"\n  tag: 'edge'\nservices:\n  web:\n    logging: *log\n    image: 'nginx:1.25'\n",
        moved: ("/services/web/image", "/x-log/image"),
        kept: &["\"json-file\"", "'edge'"],
    },
    Case {
        label: "JSON escapes",
        host: Language::JSON,
        text: "{\n  \"logging\": {\n    \"driver\": \"json\\u002dfile\"\n  },\n  \"web\": {\n    \"image\": \"nginx\\/1.25\",\n    \"mode\": \"f\\u0061st\"\n  }\n}\n",
        moved: ("/web/image", "/logging/image"),
        kept: &["\"json\\u002dfile\"", "\"f\\u0061st\""],
    },
    Case {
        label: "TOML literal strings",
        host: Language::TOML,
        text: "[logging]\ndriver = 'json-file'\ntag = \"w\\u0065b\"\n\n[web]\nimage = 'nginx:1.25'\nmode = \"fast\"\n",
        moved: ("/web/image", "/logging/image"),
        kept: &["'json-file'", "\"w\\u0065b\""],
    },
    Case {
        label: "HCL escapes",
        host: Language::HCL,
        text: "logging {\n  driver = \"json\\u002dfile\"\n}\n\nweb {\n  image = \"nginx\\u003a1.25\"\n  mode  = \"f\\u0061st\"\n}\n",
        moved: ("/web/image", "/logging/image"),
        kept: &["\"json\\u002dfile\"", "\"f\\u0061st\""],
    },
];

/// The view of `case` with its value moved, written by the view's writer
/// over the view's text, as one edit, then saved.
fn saved(case: &Case, view: Language) -> Result<String, String> {
    let mut editor = TestEditor::new(case.host, case.text);
    editor.edit_as = Some(view);
    let (_owner, state) = editor.mount();
    let shown = state.doc().text.get_untracked();
    let mut value = view.parse(&shown).map_err(|error| error.message)?;
    let (from, to) = case.moved;
    let moved = take(&mut value, from).ok_or("nothing to move")?;
    let (map, key) = to.rsplit_once('/').ok_or("no key")?;
    value
        .pointer_mut(map)
        .and_then(Value::as_object_mut)
        .ok_or("no map to move to")?
        .insert(key.to_string(), moved);
    let text = codec::serialize(view.behavior(), &value, Some(&shown))
        .map_err(|error| error.to_string())?;
    let end = units(&shown);
    state.commit(Edit::new_units(shown, (0, end), text, 0));
    pause(state);
    if !state.sync().in_sync.get_untracked() {
        return Err(format!(
            "not saved: {:?}",
            state.sync().conversion_problems.get_untracked()
        ));
    }
    Ok(state.sync().host_text.get_value())
}

/// The value at `pointer`, taken out of its map.
fn take(value: &mut Value, pointer: &str) -> Option<Value> {
    let (map, key) = pointer.rsplit_once('/')?;
    value.pointer_mut(map)?.as_object_mut()?.shift_remove(key)
}

#[test]
fn quoted_values_left_alone_keep_their_host_text_beside_moved_values() {
    let mut failures = Vec::new();
    for case in CASES {
        for view in ALL.into_iter().filter(|view| *view != case.host) {
            match saved(case, view) {
                Ok(saved) => {
                    for spelling in case.kept {
                        // Written as often as before, and a quoted word
                        // nowhere bare (a merge written out writes it too).
                        let bare = spelling.trim_matches(['"', '\'']);
                        let unquoted = !spelling.contains('\\')
                            && saved.matches(bare).count() != saved.matches(spelling).count();
                        if unquoted
                            || saved.matches(spelling).count() < case.text.matches(spelling).count()
                        {
                            failures.push(format!(
                                "{} via {view}: {spelling} not kept:\n{saved}",
                                case.label
                            ));
                        }
                    }
                }
                Err(error) => failures.push(format!("{} via {view}: {error}", case.label)),
            }
        }
    }
    report(failures);
}
