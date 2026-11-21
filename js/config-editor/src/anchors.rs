//! The texts a host's lists name places in, from JavaScript: `rangesFor` and
//! `diagnosticsFor` (a revision `onChange` gave, or a text), and the lists
//! read in the text the core says they name (see the core's
//! `host_inputs`), so a list is never read in a text it was not given for.

use leptos_config_editor::RangesFor;
use leptos_config_editor::js_support::{
    HostChannel, HostUpdate, List, Listed, Reading, statement_of,
};
use wasm_bindgen::prelude::*;

use crate::host_diagnostics::host_diagnostics_from;
use crate::protect::ranges_from;
use crate::update::given;

/// The anchor `name` of `options` (`rangesFor`, `diagnosticsFor`): a
/// revision (a number `onChange` gave) or a text; `None` left out or
/// `null`.
pub(crate) fn anchor_of(options: &JsValue, name: &str) -> Result<Option<RangesFor>, JsError> {
    let Some(anchor) = given(options, name).filter(|anchor| !anchor.is_null()) else {
        return Ok(None);
    };
    if let Some(text) = anchor.as_string() {
        return Ok(Some(RangesFor::Text(text.into())));
    }
    match anchor.as_f64() {
        Some(revision) if revision >= 0.0 && revision.fract() == 0.0 && revision.is_finite() => {
            Ok(Some(RangesFor::Revision(revision as u64)))
        }
        _ => Err(JsError::new(&format!(
            "{name} must be a revision (the number onChange gave with a text) or a text"
        ))),
    }
}

/// The host's change from JavaScript (`[{ from, to, insert }]`, string
/// indices of the text it was made on).
pub(crate) fn changes_from(list: &JsValue) -> Result<Vec<(usize, usize, String)>, JsError> {
    let bad = || {
        JsError::new(
            "applyChanges takes an array of { from, to, insert } (string indices and a text)",
        )
    };
    if !js_sys::Array::is_array(list) {
        return Err(bad());
    }
    js_sys::Array::from(list)
        .iter()
        .map(|change| {
            let index = |name: &str| {
                crate::values::field(&change, name)
                    .as_f64()
                    .filter(|at| *at >= 0.0 && at.fract() == 0.0)
                    .map(|at| at as usize)
            };
            let insert = crate::values::field(&change, "insert").as_string();
            match (index("from"), index("to"), insert) {
                (Some(from), Some(to), Some(insert)) => Ok((from, to, insert)),
                _ => Err(bad()),
            }
        })
        .collect()
}

/// `name` of `options`, a revision, when it is one.
pub(crate) fn revision_named(options: &JsValue, name: &str) -> Option<u64> {
    given(options, name)
        .and_then(|revision| revision.as_f64())
        .filter(|revision| *revision >= 0.0 && revision.is_finite())
        .map(|revision| revision as u64)
}

/// `valueRevision` of `options`, when it is a revision.
pub(crate) fn revision_of(options: &JsValue) -> Option<u64> {
    given(options, "valueRevision")
        .and_then(|revision| revision.as_f64())
        .filter(|revision| *revision >= 0.0 && revision.is_finite())
        .map(|revision| revision as u64)
}

/// What a list is compared by when given again: its JSON (`null` for a
/// list cleared).
fn statement(list: &JsValue) -> u64 {
    let json = js_sys::JSON::stringify(list)
        .ok()
        .and_then(|text| text.as_string())
        .unwrap_or_default();
    statement_of(&json)
}

/// `text` as the editor reads it: LF breaks, no leading byte order mark.
fn editor_form(text: &str) -> String {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// `head` with the `readOnlyRanges` and `hostDiagnostics` of `options`
/// (as `prefix` names them in errors), each read in the text it names
/// places in, the host's text now being `current`. A list in force given
/// again is not read (its numbers may not fit the text it comes with). A
/// list naming a revision the editor never sent throws, and so does one
/// that does not fit the text it names; one the editor can no longer name
/// is passed on unread (the core says so and keeps the locks).
pub(crate) fn with_lists(
    channel: &HostChannel,
    options: &JsValue,
    mut head: HostUpdate,
    current: &str,
    prefix: &str,
) -> Result<HostUpdate, JsError> {
    for (list, key) in [
        (List::Ranges, "readOnlyRanges"),
        (List::Diagnostics, "hostDiagnostics"),
    ] {
        let Some(given) = given(options, key) else {
            continue;
        };
        let stated = statement(&given);
        let probe = match list {
            List::Ranges => {
                head.ranges = Some(Listed::stated(Vec::new(), stated));
                &head
            }
            List::Diagnostics => {
                head.diagnostics = Some(Listed::stated(Vec::new(), stated));
                &head
            }
        };
        let text = match channel.reading(list, probe, current) {
            // The host's text now, as the host writes it (a byte order
            // mark, CRLF): string indices count those.
            Ok(Reading::In(text)) if *text == *editor_form(current) => Some(current.into()),
            Ok(Reading::In(text)) => Some(text),
            Ok(Reading::Again) => None,
            Err(error) if error.is_warning() => None,
            Err(error) => return Err(JsError::new(&format!("{prefix}{error}"))),
        };
        let Some(text) = text else {
            continue;
        };
        let name = format!("{prefix}{key}");
        match list {
            List::Ranges => match ranges_from(&given, &text, &name) {
                Ok(items) => head.ranges = Some(Listed::stated(items, stated)),
                // The list whose locks a text of the host's own carried,
                // rendered again for the text before it: no news.
                Err(_) if channel.carried_list() == Some(stated) => head.ranges = None,
                Err(error) => return Err(error),
            },
            List::Diagnostics => {
                let items = if given.is_null() {
                    Vec::new()
                } else {
                    host_diagnostics_from(&given, &text).map_err(|error| {
                        let message = js_sys::Error::from(JsValue::from(error)).message();
                        JsError::new(&format!("{prefix}{}", String::from(message)))
                    })?
                };
                head.diagnostics = Some(Listed::stated(items, stated));
            }
        }
    }
    Ok(head)
}
