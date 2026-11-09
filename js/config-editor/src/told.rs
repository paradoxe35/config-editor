//! What the host was last told of an editor's state (the language on
//! screen, whether it is in sync, its problems), so a callback given later
//! through `update` hears it at once instead of waiting for the next change.

use config_editor_leptos::{Diagnostic, Language};
use js_sys::Function;
use wasm_bindgen::prelude::*;

use crate::catalog::language_name;
use crate::values::diagnostics_to_js;

/// The last of each state an editor reported.
#[derive(Clone, Default)]
pub(crate) struct Told {
    pub(crate) language: Option<Language>,
    pub(crate) in_sync: Option<bool>,
    pub(crate) problems: Option<Vec<Diagnostic>>,
}

/// The callbacks reporting a state, by option name.
pub(crate) const STATE_CALLBACKS: [&str; 3] = ["onEditAsChange", "onSyncChange", "onProblems"];

impl Told {
    /// The state `name` (one of [`STATE_CALLBACKS`]) reports, as its
    /// argument; `None` before the editor reported it.
    pub(crate) fn argument(&self, name: &str) -> Option<JsValue> {
        match name {
            "onEditAsChange" => self
                .language
                .map(|language| JsValue::from_str(&language_name(language))),
            "onSyncChange" => self.in_sync.map(JsValue::from_bool),
            "onProblems" => self.problems.clone().map(diagnostics_to_js),
            _ => None,
        }
    }
}

/// The state callbacks an update adds (none before, a function now), with
/// the function: `changes` pairs each option name with its change (see
/// `update::callback_change`) and whether one was set before. In the order
/// of [`STATE_CALLBACKS`]: the language first, as on mount.
pub(crate) fn added<'a>(
    changes: impl IntoIterator<Item = (&'static str, Option<&'a Option<Function>>, bool)>,
) -> Vec<(&'static str, Function)> {
    let mut added: Vec<(&'static str, Function)> = changes
        .into_iter()
        .filter_map(|(name, change, before)| match change {
            Some(Some(callback)) if !before && STATE_CALLBACKS.contains(&name) => {
                Some((name, callback.clone()))
            }
            _ => None,
        })
        .collect();
    added.sort_by_key(|(name, _)| STATE_CALLBACKS.iter().position(|known| known == name));
    added
}
