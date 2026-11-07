//! [`EditorSettings`]: the editor's plain settings (the theme and the on/off
//! switches), defined once. `ConfigEditor` and `ConfigDocuments` take them as
//! one signal and follow every change live; the npm package reads the same
//! struct from its options (serde, camelCase), and its TypeScript type and
//! the settings tables of the READMEs are generated from this file, doc
//! comments and defaults included (`js/config-editor/tests/settings.rs`).
//!
//! The doc comments of the fields are shown to Rust, JavaScript and React
//! users alike: they name keys and callbacks without a language's spelling.

use serde::{Deserialize, Serialize};

use crate::{EditorTheme, ThemeColors};

/// How an editor looks and which of its features are on: everything a host
/// sets once and may switch at any time, apart from the document itself.
///
/// Every field has a default ([`EditorSettings::default`]), so a host names
/// only what it changes:
///
/// ```
/// use config_editor_leptos::{EditorSettings, EditorTheme};
///
/// let settings = EditorSettings {
///     theme: EditorTheme::Nord,
///     show_footer: true,
///     folding: false,
///     ..EditorSettings::default()
/// };
/// assert!(settings.completions);
/// ```
///
/// Pass it to `ConfigEditor` (or `ConfigDocuments`) as `settings`, as a value
/// or a signal: a change applies live, keeping the caret, the undo history
/// and the folds.
///
/// It reads from and writes to JSON with camelCase keys, missing keys taking
/// their defaults (`{"showFooter": true, "theme": "nord"}`), as the npm
/// package's options do.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EditorSettings {
    /// The bundled color theme.
    pub theme: EditorTheme,
    /// Colors overriding the theme's, by token (`bg`, `key`, `string`,
    /// `selection`, …), for a custom palette.
    pub colors: ThemeColors,
    /// Inject the preset stylesheet. Off, style the `config-editor__*`
    /// markup yourself (the preset is `PRESET_CSS`, `presetCss()` in
    /// JavaScript).
    pub styled: bool,
    /// Show the built-in toolbar: the file name, its language and the
    /// schema's state.
    pub show_toolbar: bool,
    /// Show the built-in status footer: the suggestions hint, problem and
    /// change counts, and the line count.
    pub show_footer: bool,
    /// Show syntax and schema problems inline, in the footer and on hover
    /// (`F8` moves between them).
    pub show_diagnostics: bool,
    /// Show line changes against the baseline in the gutter (`Alt+F5` /
    /// `Alt+F3` move between them). Off, the change summary stays at zero.
    pub show_diff: bool,
    /// Schema completions (`Ctrl+Space`).
    pub completions: bool,
    /// Open suggestions while typing and after accepting a key. Off, only
    /// `Ctrl+Space` opens them.
    pub auto_trigger_completions: bool,
    /// Show the schema's description, type, default and examples when
    /// hovering a key.
    pub hover_docs: bool,
    /// Syntax colors.
    pub highlighting: bool,
    /// Auto-close quotes and brackets, wrap selections, and delete empty
    /// pairs on Backspace.
    pub auto_closing: bool,
    /// Find and replace (`Ctrl+F`, `Ctrl+H`, `F3`). Off, those keys are left
    /// to the browser.
    pub search: bool,
    /// Go to line (`Ctrl+G`). Off, the key is left to the browser.
    pub go_to_line: bool,
    /// Folding by structure: chevrons in the gutter, `Ctrl+Shift+[` and
    /// `Ctrl+Shift+]` at the caret, `Ctrl+K Ctrl+0` and `Ctrl+K Ctrl+J` for
    /// everything (`Cmd` on macOS). View only: the text stays whole.
    /// Switching it off opens every fold.
    pub folding: bool,
    /// Whether `Ctrl+S` / `Cmd+S` saves, when a save callback is set. Off,
    /// the key is left to the browser and only the handle's `save` saves.
    pub save_shortcut: bool,
    // Every built-in text (hints, tooltips, notices, accessible names) will
    // join as one `strings` field: a nested struct of its own with these
    // same rules (doc comments, defaults, camelCase keys), generated for
    // JavaScript the same way.
}

impl Default for EditorSettings {
    fn default() -> Self {
        Self {
            theme: EditorTheme::default(),
            colors: ThemeColors::new(),
            styled: true,
            show_toolbar: false,
            show_footer: false,
            show_diagnostics: true,
            show_diff: true,
            completions: true,
            auto_trigger_completions: true,
            hover_docs: true,
            highlighting: true,
            auto_closing: true,
            search: true,
            go_to_line: true,
            folding: true,
            save_shortcut: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ThemeColor;

    #[test]
    fn the_defaults_are_the_documented_ones() {
        let settings = EditorSettings::default();
        assert_eq!(settings.theme, EditorTheme::VsCodeDark);
        assert!(settings.colors.is_empty());
        let off = [settings.show_toolbar, settings.show_footer];
        assert_eq!(off, [false, false]);
        let on = [
            settings.styled,
            settings.show_diagnostics,
            settings.show_diff,
            settings.completions,
            settings.auto_trigger_completions,
            settings.hover_docs,
            settings.highlighting,
            settings.auto_closing,
            settings.search,
            settings.go_to_line,
            settings.folding,
            settings.save_shortcut,
        ];
        assert!(on.iter().all(|on| *on), "{on:?}");
    }

    #[test]
    fn missing_keys_take_their_defaults() {
        let read: EditorSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(read, EditorSettings::default());
        let read: EditorSettings =
            serde_json::from_str(r#"{"showFooter": true, "theme": "nord"}"#).unwrap();
        assert_eq!(
            read,
            EditorSettings {
                show_footer: true,
                theme: EditorTheme::Nord,
                ..EditorSettings::default()
            }
        );
    }

    /// Every key, as JavaScript spells it, reads into its field: a matrix
    /// listing every key that does not.
    #[test]
    fn every_key_reads_from_camel_case_json() {
        let defaults = serde_json::to_value(EditorSettings::default()).unwrap();
        let keys: Vec<String> = defaults.as_object().unwrap().keys().cloned().collect();
        let failures: Vec<String> = keys
            .iter()
            .filter_map(|key| {
                let flipped = match &defaults[key] {
                    serde_json::Value::Bool(on) => serde_json::Value::Bool(!on),
                    serde_json::Value::String(_) => "dracula".into(),
                    serde_json::Value::Object(_) => serde_json::json!({"key": "#ffcc66"}),
                    other => return Some(format!("{key}: unexpected default {other}")),
                };
                let read: EditorSettings =
                    match serde_json::from_value(serde_json::json!({ key: flipped })) {
                        Ok(read) => read,
                        Err(error) => return Some(format!("{key}: {error}")),
                    };
                let written = serde_json::to_value(&read).unwrap();
                (written[key] != flipped || read == EditorSettings::default())
                    .then(|| format!("{key}: read {written} from {flipped}"))
            })
            .collect();
        assert!(failures.is_empty(), "{failures:#?}");
        assert!(
            keys.contains(&"autoTriggerCompletions".to_string()),
            "{keys:?}"
        );
    }

    #[test]
    fn settings_round_trip_through_json() {
        let settings = EditorSettings {
            theme: EditorTheme::GitHubLight,
            colors: ThemeColors::new()
                .set(ThemeColor::Key, "#ffcc66")
                .set(ThemeColor::TabBar, "rgb(0 0 0 / 40%)"),
            show_toolbar: true,
            go_to_line: false,
            ..EditorSettings::default()
        };
        let json = serde_json::to_string(&settings).unwrap();
        assert!(json.contains(r##""colors":{"key":"#ffcc66","tab-bar":"rgb(0 0 0 / 40%)"}"##));
        assert!(json.contains(r#""theme":"github-light""#));
        assert_eq!(
            serde_json::from_str::<EditorSettings>(&json).unwrap(),
            settings
        );
    }

    #[test]
    fn a_wrong_value_is_an_error_naming_what_was_expected() {
        let cases = [
            (r#"{"theme": "no-such-theme"}"#, "unknown editor theme"),
            (
                r#"{"colors": {"keys": "red"}}"#,
                "unknown color token \"keys\"",
            ),
            (r#"{"colors": {"key": 7}}"#, "a CSS color string"),
            (r#"{"folding": "yes"}"#, "expected a boolean"),
            (r#"{"colors": []}"#, "an object of token: CSS color"),
        ];
        let failures: Vec<String> = cases
            .iter()
            .filter_map(
                |(json, wanted)| match serde_json::from_str::<EditorSettings>(json) {
                    Ok(read) => Some(format!("{json} read as {read:?}")),
                    Err(error) if error.to_string().contains(wanted) => None,
                    Err(error) => Some(format!("{json}: {error}, wanted {wanted:?}")),
                },
            )
            .collect();
        assert!(failures.is_empty(), "{failures:#?}");
    }
}
