//! A YAML merge (`<<: *base`) or alias (`logging: *log`) an Edit-as edit
//! parts from its anchor is written out with each map or list it held in
//! the form its anchor writes it: a block on lines of its own, a flow on
//! one line, from every other view.

use super::super::codec;
use super::super::edit::Edit;
use super::super::languages::Language;
use super::moved_forms::{pause, units};
use crate::editor::save_matrix::{ALL, report};
use crate::editor::test_state::TestEditor;
use leptos::prelude::*;
use serde_json::Value;

/// A YAML host text, the edit made to the view's value, and the text the
/// save must write where the merge or alias was.
struct Case {
    label: &'static str,
    text: &'static str,
    edit: fn(&mut Value) -> Option<()>,
    written: &'static str,
}

/// The merged key `keep` gone from `/web`: the merge is written out.
fn drop_keep(value: &mut Value) -> Option<()> {
    value
        .pointer_mut("/web")?
        .as_object_mut()?
        .shift_remove("keep")?;
    Some(())
}

/// `/web/image` moved into the anchor `/log`: the alias parts from it.
fn image_into_log(value: &mut Value) -> Option<()> {
    let image = value
        .pointer_mut("/web")?
        .as_object_mut()?
        .shift_remove("image")?;
    value
        .pointer_mut("/log")?
        .as_object_mut()?
        .insert("image".into(), image);
    Some(())
}

const CASES: &[Case] = &[
    Case {
        label: "merged block map",
        text: "base: &b\n  keep: 1\n  log:\n    driver: \"json-file\"\n    tag: edge\nweb:\n  <<: *b\n  image: nginx\n",
        edit: drop_keep,
        written: "web:\n  log:\n    driver: \"json-file\"\n    tag: edge\n  image: nginx\n",
    },
    Case {
        label: "merged flow map",
        text: "base: &b\n  keep: 1\n  log: {driver: \"json-file\", tag: edge}\nweb:\n  <<: *b\n  image: nginx\n",
        edit: drop_keep,
        written: "web:\n  log: {driver: \"json-file\", tag: edge}\n  image: nginx\n",
    },
    Case {
        label: "merged block list",
        text: "base: &b\n  keep: 1\n  ports:\n    - 'a'\n    - b\nweb:\n  <<: *b\n  image: nginx\n",
        edit: drop_keep,
        written: "web:\n  ports:\n    - 'a'\n    - b\n  image: nginx\n",
    },
    Case {
        label: "merged flow list",
        text: "base: &b\n  keep: 1\n  ports: ['a', b]\nweb:\n  <<: *b\n  image: nginx\n",
        edit: drop_keep,
        written: "web:\n  ports: ['a', b]\n  image: nginx\n",
    },
    Case {
        label: "merged block map in a list item",
        text: "base: &b\n  a: 1\n  keep: 1\n  log:\n    driver: \"json-file\"\nweb:\n  - <<: *b # shared\n    image: nginx\n  - image: redis\n",
        edit: |value| {
            *value.pointer_mut("/web/0/keep")? = Value::from(2);
            Some(())
        },
        written: "web:\n  - a: 1 # shared\n    keep: 2\n    log:\n      driver: \"json-file\"\n    image: nginx\n  - image: redis\n",
    },
    Case {
        label: "aliased block map",
        text: "log: &log\n  driver: \"json-file\"\n  tag: edge\nweb:\n  logging: *log # shared\n  image: nginx\n",
        edit: image_into_log,
        written: "web:\n  logging: # shared\n    driver: \"json-file\"\n    tag: edge\n",
    },
    Case {
        label: "aliased flow map",
        text: "log: &log {driver: \"json-file\", tag: edge}\nweb:\n  logging: *log\n  image: nginx\n",
        edit: image_into_log,
        written: "web:\n  logging: {driver: \"json-file\", tag: edge}\n",
    },
];

/// The view of `case.text` with `case.edit` made, written by the view's
/// writer over the view's text as one edit, then saved.
fn saved(case: &Case, view: Language) -> Result<String, String> {
    let mut editor = TestEditor::new(Language::YAML, case.text);
    editor.edit_as = Some(view);
    let (_owner, state) = editor.mount();
    let shown = state.doc().text.get_untracked();
    let mut value = view.parse(&shown).map_err(|error| error.message)?;
    (case.edit)(&mut value).ok_or("the edit does not apply")?;
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

#[test]
fn a_merge_or_alias_written_out_keeps_the_form_of_each_container_it_held() {
    let mut failures = Vec::new();
    for case in CASES {
        for view in ALL.into_iter().filter(|view| *view != Language::YAML) {
            match saved(case, view) {
                Ok(saved) if saved.ends_with(case.written) => {}
                Ok(saved) => failures.push(format!(
                    "{} via {view}: not written as\n{}\n--- saved:\n{saved}",
                    case.label, case.written
                )),
                Err(error) => failures.push(format!("{} via {view}: {error}", case.label)),
            }
        }
    }
    report(failures);
}
