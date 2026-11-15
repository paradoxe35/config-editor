#[cfg(all(feature = "yaml", feature = "schema-validation"))]
mod anchor_problems;
#[cfg(all(
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod backspace_suggestions;
mod basics;
#[cfg(all(
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod bom_edits;
#[cfg(all(
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod broken_document_suggestions;
#[cfg(all(
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod convert_messages;
#[cfg(feature = "yaml")]
mod empty_schema;
#[cfg(feature = "yaml")]
mod handle;
#[cfg(feature = "yaml")]
mod hidden_suggestions;
#[cfg(all(
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod item_trail;
#[cfg(all(
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod item_trail_switches;
#[cfg(all(
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod key_trail_switches;
#[cfg(all(
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod sibling_decided_suggestions;
#[cfg(all(
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod xml_moved_namespaces;
// Reads what the view reads (folds, suggestions) on every language's sample.
#[cfg(all(
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod anchor_moves;
#[cfg(all(
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml",
    feature = "folding",
    feature = "diff",
    feature = "schema-validation"
))]
mod keystroke_work;
#[cfg(all(
    feature = "schema-validation",
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod line_breaks;
// It uses the helpers of `moved_forms`, gated the same.
#[cfg(all(
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod inline_forms_moved;
#[cfg(all(
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod inline_items_kept;
#[cfg(all(
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod large_edit_work;
#[cfg(all(
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod moved_form_checks;
#[cfg(all(
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod moved_forms;
#[cfg(all(
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod moved_forms_again;
#[cfg(all(feature = "yaml", feature = "toml"))]
mod moved_into_items;
#[cfg(all(
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod moved_maps_nested;
// Observes what every setting switches, so it needs every optional feature.
#[cfg(all(feature = "yaml", feature = "toml", feature = "schema-validation"))]
mod hover_look_back;
#[cfg(all(
    feature = "yaml",
    feature = "diff",
    feature = "search",
    feature = "folding",
    feature = "protect"
))]
mod live_settings;
#[cfg(feature = "yaml")]
mod nesting_limits;
#[cfg(all(feature = "yaml", feature = "folding"))]
mod nothing_to_fold;
#[cfg(all(feature = "yaml", feature = "schema-validation"))]
mod own_problems;
#[cfg(all(feature = "yaml", feature = "protect"))]
mod problem_cap;
#[cfg(all(
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml",
    feature = "protect",
    feature = "search",
    feature = "schema-validation"
))]
mod protect_fuzz;
#[cfg(feature = "yaml")]
mod reveal_range;
// Counts every kind of edit in every language and view, so it needs every
// optional feature the edits reach.
#[cfg(all(
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml",
    feature = "folding",
    feature = "diff",
    feature = "search",
    feature = "protect",
    feature = "schema-validation"
))]
mod linear_work;
#[cfg(all(
    feature = "schema-validation",
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl"
))]
mod schema_message;
#[cfg(all(
    feature = "schema-validation",
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl"
))]
mod schema_message_kinds;
#[cfg(all(
    feature = "schema-validation",
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod schema_option_messages;
#[cfg(all(feature = "yaml", feature = "schema-validation"))]
mod schema_text_wording;
#[cfg(all(
    feature = "schema-validation",
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml",
    feature = "protect"
))]
mod secret_inside_messages;

#[cfg(all(
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod kept_defaults;
// Random edit sequences reach every optional feature but diffs and
// schema validation.
#[cfg(all(
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml",
    feature = "folding",
    feature = "search",
    feature = "protect"
))]
mod fuzz_edit;
mod save;
#[cfg(all(feature = "yaml", feature = "schema-validation"))]
mod schema_options;
#[cfg(all(feature = "yaml", feature = "schema-validation"))]
mod schema_walk;
// Counts the suggestions' work in every language.
#[cfg(all(
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod deep_option_completion_work;
#[cfg(all(
    feature = "folding",
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod fold_line_below;
#[cfg(all(
    feature = "schema-validation",
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod list_item_problems;
#[cfg(all(
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "schema-validation"
))]
mod one_line_item_work;
#[cfg(all(
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod option_document_reads;
#[cfg(all(
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml",
    feature = "schema-validation"
))]
mod option_nested_keys_work;
#[cfg(all(feature = "json", feature = "schema-validation"))]
mod schema_error_bound;
#[cfg(all(feature = "json", feature = "schema-validation"))]
mod schema_error_collection;
#[cfg(all(feature = "json", feature = "schema-validation"))]
mod schema_error_cost;
#[cfg(all(
    feature = "schema-validation",
    feature = "protect",
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod schema_fuzz;
#[cfg(all(
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod suggestion_scale;
#[cfg(all(
    feature = "schema-validation",
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod unevaluated_beside_errors;
mod utf16_offsets;
#[cfg(all(
    feature = "schema-validation",
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod xml_applied_readings;
#[cfg(all(
    feature = "schema-validation",
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod xml_numeric_strings;
#[cfg(all(
    feature = "schema-validation",
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod xml_text_readings;
#[cfg(all(
    feature = "schema-validation",
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod xml_union_types;
