//! What a problem message must not quote: a secret's value or a
//! placeholder. Schema messages quote values of a parsed document, so
//! their secrets are found by the schema and by their texts (`by_schema`);
//! a syntax message quotes the text itself where no parse says which part
//! is a secret's: the language says which parts of its message quote the
//! text, and those quoting a placeholder or the text from where the
//! parser stopped are hidden (`diagnostics::syntax_message`). For a
//! language that does not say, what its message quotes is found by
//! reading the text with its letters and digits changed (`in_syntax`).

#[cfg(feature = "schema-validation")]
mod by_schema;
mod in_syntax;

#[cfg(feature = "schema-validation")]
pub(super) use by_schema::Secrets;
pub(super) use in_syntax::{hide_quoted_text, unread_from};

// Without protection no schema marks a secret.
#[cfg(all(test, feature = "protect"))]
mod in_syntax_tests;
#[cfg(all(
    test,
    feature = "protect",
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod quotes_matrix_tests;
