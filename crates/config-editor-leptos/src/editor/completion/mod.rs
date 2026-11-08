//! Schema-driven suggestions: what to offer at the caret, how it ranks,
//! how accepting writes it, hover docs, and the popup's placement.

mod accept;
mod candidates;
mod choices;
mod docs;
mod headless;
mod hover;
mod live;
mod matching;
mod patterns;
mod popup;
mod schema;
mod stops;

pub(crate) use accept::*;
pub(crate) use candidates::*;
pub(crate) use choices::*;
pub(crate) use docs::*;
pub use headless::{HoverDocs, Suggestion};
pub(crate) use headless::{schema_hover, schema_suggestions};
pub(crate) use hover::*;
pub(crate) use live::{CompletionState, Sources, SuggestionPlace};
pub(crate) use matching::*;
pub(crate) use patterns::unsupported_patterns;
pub(crate) use popup::*;
pub(crate) use schema::*;
pub(crate) use stops::*;
// Public for languages written outside the crate (see `language`).
pub use schema::{array_branch, object_branch, placeholder, schema_const, schema_default};
pub use stops::{fill_value, schema_example};

use super::commands::indent_unit;
use super::diagnostics::SchemaBundle;
use super::keyboard::{replace_selection, utf16_to_byte_index};
use super::language::{
    CompletionContext, ConfigLanguage, EntryPosition, InsertContext, Repetition, SchemaPathSegment,
};
use super::metrics::read_editor_metrics;
use super::positioning::{FALLBACK_VIEWPORT_HEIGHT, FALLBACK_VIEWPORT_WIDTH, POPUP_MARGIN};
use super::typing::Closer;
use serde_json::Value;
use std::collections::HashSet;
use wasm_bindgen::JsCast;

#[derive(Clone, PartialEq)]
pub(super) struct Completion {
    pub(super) label: String,
    pub(super) detail: String,
    pub(super) type_label: String,
    pub(super) insert: String,
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) cursor_offset: usize,
    /// UTF-16 length of the text before the caret that is left selected (an
    /// inserted default), so typing replaces it.
    pub(super) selected: usize,
    pub(super) is_value: bool,
    /// Renames a key that already has its value (`ht|: 80`): accepting it
    /// completes the entry, so the list does not reopen.
    pub(super) renames: bool,
    /// Character ranges of `label` matched by the query, for highlighting.
    pub(super) matched: Vec<(usize, usize)>,
    /// The schema marks the key deprecated: it is listed last, struck through.
    pub(super) deprecated: bool,
    /// `(key, value text)` of the values `insert` writes for required
    /// children (defaults, examples, placeholders): accepting it finds them
    /// after the caret ([`stop_ranges`]) and Tab selects each in turn, as a
    /// snippet's placeholders.
    pub(super) fills: Vec<(String, String)>,
    /// The end tag XML suggests for the element `label` names: its detail
    /// and type are worded when shown (the `closesTag` and `endTag` texts).
    pub(super) closes_tag: bool,
}

#[cfg(test)]
mod tests {
    use super::{
        COMPLETION_LIST_PADDING, COMPLETION_POPUP_MAX_HEIGHT, COMPLETION_POPUP_WIDTH,
        COMPLETION_ROW_HEIGHT, MATCH_SUBSTRING_INDEX_PENALTY, MATCH_SUBSTRING_LENGTH_DIVISOR,
        MATCH_TIER_CASE_INSENSITIVE_PREFIX, MATCH_TIER_EXACT_PREFIX, MATCH_TIER_INITIALS,
        MATCH_TIER_SUBSEQUENCE, MATCH_TIER_SUBSTRING, completion_popup_geometry,
    };

    /// Changing these must be deliberate: the sizes mirror `preset.css`.
    #[test]
    fn ranking_tiers_and_popup_sizing_match_their_documentation() {
        assert_eq!(MATCH_TIER_EXACT_PREFIX, 1000);
        assert_eq!(MATCH_TIER_CASE_INSENSITIVE_PREFIX, 900);
        assert_eq!(MATCH_TIER_INITIALS, 800);
        assert_eq!(MATCH_TIER_SUBSTRING, 700);
        assert_eq!(MATCH_TIER_SUBSEQUENCE, 500);
        assert_eq!(MATCH_SUBSTRING_INDEX_PENALTY, 2);
        assert_eq!(MATCH_SUBSTRING_LENGTH_DIVISOR, 4);
        assert_eq!(COMPLETION_POPUP_WIDTH, 420.0);
        assert_eq!(COMPLETION_ROW_HEIGHT, 44.0);
        assert_eq!(COMPLETION_LIST_PADDING, 2.0);
        assert_eq!(COMPLETION_POPUP_MAX_HEIGHT, 320.0);
    }

    #[test]
    fn completion_popup_opens_below_caret_when_space_allows() {
        let style = completion_popup_geometry(1024.0, 768.0, 100.0, 200.0, 21.6, 3);
        // 3 options: ideal height = 3*44+2 = 134. Below: 768-200-21.6-8 = 538.4.
        assert!(
            style.contains("top:221.6px"),
            "popup should sit just below the caret line, got: {style}"
        );
        assert!(
            style.contains("left:100.0px"),
            "popup should anchor at the caret x, got: {style}"
        );
        assert!(
            style.contains("max-height:134.0px"),
            "popup should fit all options, got: {style}"
        );
    }

    #[test]
    fn completion_popup_flips_above_caret_near_viewport_bottom() {
        let style = completion_popup_geometry(1024.0, 768.0, 100.0, 700.0, 21.6, 3);
        // Below: 768-700-21.6-8 = 38.4 < 134; above: 700-8 = 692. So flip.
        assert!(
            style.contains("top:566.0px"),
            "popup should flip above the caret (700-134), got: {style}"
        );
        assert!(
            style.contains("max-height:134.0px"),
            "popup keeps full height when there is room above, got: {style}"
        );
    }

    #[test]
    fn completion_popup_clamps_to_viewport_right_edge() {
        let style = completion_popup_geometry(1024.0, 768.0, 1000.0, 200.0, 21.6, 3);
        // width = min(420, 1024-16) = 420; left clamped to 1024-420-8 = 596.
        assert!(
            style.contains("left:596.0px"),
            "popup should clamp to the right edge, got: {style}"
        );
        assert!(
            style.contains("width:420.0px"),
            "popup keeps full width when clamped, got: {style}"
        );
    }
}

#[cfg(all(test, feature = "yaml", feature = "json", feature = "xml"))]
mod slot_tests {
    use super::super::Language;
    use super::next_slot;

    #[test]
    fn slots_are_empty_values_after_the_caret() {
        let yaml_behavior = Language::YAML.behavior();
        let yaml = "rfc2136:\n  server: \n  key_name: \n  secret: \n";
        let first = yaml.find("server: ").unwrap() + "server: ".len();
        let second = next_slot(yaml, first, yaml.len(), yaml_behavior).unwrap();
        assert_eq!(&yaml[..second], "rfc2136:\n  server: \n  key_name: ");
        let json = "{\"a\": \"\", \"b\": \"\"}";
        assert_eq!(
            next_slot(json, 7, json.len(), Language::JSON.behavior()),
            Some(16)
        );
        let xml = "<x>\n  <a></a>\n  <b></b>\n</x>";
        let after_a = xml.find("<a>").unwrap() + 3;
        assert_eq!(
            next_slot(xml, after_a, xml.len(), Language::XML.behavior()),
            Some(xml.find("<b>").unwrap() + 3)
        );
        assert_eq!(
            next_slot(yaml, yaml.len() - 1, yaml.len(), yaml_behavior),
            None
        );
    }
}
