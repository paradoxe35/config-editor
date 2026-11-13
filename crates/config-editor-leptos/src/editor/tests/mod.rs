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
mod convert_messages;
#[cfg(feature = "yaml")]
mod handle;
#[cfg(feature = "yaml")]
mod hidden_suggestions;
// Reads what the view reads (folds, suggestions) on every language's sample.
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
#[cfg(all(feature = "yaml", feature = "schema-validation"))]
mod own_problems;
#[cfg(all(feature = "yaml", feature = "protect"))]
mod problem_cap;
#[cfg(feature = "yaml")]
mod reveal_range;
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
    feature = "hcl",
    feature = "xml"
))]
mod schema_option_messages;

#[cfg(all(
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod kept_defaults;
mod save;
#[cfg(all(feature = "yaml", feature = "schema-validation"))]
mod schema_options;
#[cfg(all(feature = "yaml", feature = "schema-validation"))]
mod schema_walk;
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
mod xml_numeric_strings;
