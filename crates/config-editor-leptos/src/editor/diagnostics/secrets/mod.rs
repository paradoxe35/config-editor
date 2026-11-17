//! What a problem message must not quote: a secret's value or a
//! placeholder. Schema messages quote values of a parsed document, so
//! their secrets are found by the schema and by their texts (`by_schema`);
//! a syntax message quotes the text itself where no parse says which part
//! is a secret's, so what it quotes of the text a secret may be in is
//! found by reading that text with its letters and digits changed
//! (`in_syntax`).

#[cfg(feature = "schema-validation")]
mod by_schema;
mod in_syntax;

#[cfg(feature = "schema-validation")]
pub(super) use by_schema::Secrets;
pub(super) use in_syntax::hide_quoted_text;

// Without protection no schema marks a secret.
#[cfg(all(test, feature = "protect"))]
mod in_syntax_tests;
