//! Shared helpers for the Edit-as save matrices: a value edited in every
//! other language's view of a host text, every failing case listed at once.

use super::Language;
use super::bridge::Bridge;
use serde_json::Value;

pub(super) const ALL: [Language; 5] = [
    Language::YAML,
    Language::JSON,
    Language::TOML,
    Language::HCL,
    Language::XML,
];

/// The host saved after the edit language's view of `host_text` is made to
/// hold `edit` (applied to the value the view shows), or why it failed.
pub(super) fn save_value(
    host: Language,
    edit: Language,
    host_text: &str,
    change: &dyn Fn(&mut Value),
) -> Result<String, String> {
    save_conversion(host, edit, host_text, change).map(|(saved, _)| saved)
}

/// [`save_value`], and whether the view itself reads the changed value
/// back otherwise (XML text `1.5` is a number).
pub(super) fn save_conversion(
    host: Language,
    edit: Language,
    host_text: &str,
    change: &dyn Fn(&mut Value),
) -> Result<(String, bool), String> {
    let bridge = Bridge::new(host, edit, None).ok_or("same language")?;
    let shown = bridge
        .to_edit(host_text)
        .map_err(|error| error.to_string())?
        .text;
    let mut value = edit.parse(&shown).map_err(|error| error.message)?;
    change(&mut value);
    let edited = super::codec::serialize(edit.behavior(), &value, Some(&shown))
        .map_err(|error| error.to_string())?;
    let saved = bridge
        .to_host(&edited, host_text)
        .map_err(|error| format!("{error:?}"))?;
    Ok((saved, edit.parse(&edited).ok().as_ref() != Some(&value)))
}

/// Saves `change` from every other view and compares with `expected`.
pub(super) fn from_every_view(
    failures: &mut Vec<String>,
    label: &str,
    host: Language,
    host_text: &str,
    change: &dyn Fn(&mut Value),
    expected: &str,
) {
    for edit in ALL.into_iter().filter(|edit| *edit != host) {
        match save_value(host, edit, host_text, change) {
            Ok(saved) if saved == expected => {}
            Ok(saved) => failures.push(format!(
                "{label}: {host} host via {edit}:\n--- expected\n{expected}--- saved\n{saved}"
            )),
            Err(error) => failures.push(format!("{label}: {host} host via {edit}: {error}")),
        }
    }
}

pub(super) fn report(failures: Vec<String>) {
    assert!(
        failures.is_empty(),
        "{} failing:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

/// The value at `pointer`, or where an XML view writes it: a one-item list
/// is a lone element there (`/l/0` is `/l`).
pub(super) fn slot<'a>(document: &'a mut Value, pointer: &str) -> &'a mut Value {
    let shown = document.to_string();
    find_slot(document, pointer).unwrap_or_else(|| panic!("no {pointer} in {shown}"))
}

/// [`slot`], or `None` where the view does not show the value (XML has no
/// nested lists).
pub(super) fn find_slot<'a>(document: &'a mut Value, pointer: &str) -> Option<&'a mut Value> {
    if document.pointer(pointer).is_some() {
        return document.pointer_mut(pointer);
    }
    let lone = pointer.replace("/0/", "/");
    let lone = lone.strip_suffix("/0").unwrap_or(&lone).to_string();
    document.pointer_mut(&lone)
}

/// The host after an edit session in the edit language's view of
/// `host_text`: each change is made to the view as the previous one left it
/// and saved over the host's last save, as the editor saves each typing
/// pause (the view is not converted again between them).
pub(super) fn save_steps(
    host: Language,
    edit: Language,
    host_text: &str,
    changes: &[&dyn Fn(&mut Value)],
) -> Result<String, String> {
    let bridge = Bridge::new(host, edit, None).ok_or("same language")?;
    let mut shown = bridge
        .to_edit(host_text)
        .map_err(|error| error.to_string())?
        .text;
    let mut saved = host_text.to_string();
    for change in changes {
        let mut value = edit.parse(&shown).map_err(|error| error.message)?;
        change(&mut value);
        shown = super::codec::serialize(edit.behavior(), &value, Some(&shown))
            .map_err(|error| error.to_string())?;
        saved = bridge
            .to_host_since(&shown, &saved, host_text)
            .map_err(|error| format!("{error:?}"))?;
    }
    Ok(saved)
}
