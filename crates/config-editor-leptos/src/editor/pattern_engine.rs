//! The one regular-expression engine of a build. With `schema-validation`
//! the validator links `regex`, so every pattern (search, `patternProperties`,
//! secret placeholders, the YAML typed-scalar rule) uses it too; without it,
//! the much smaller `regex-lite`. Linking both would cost the second engine
//! for nothing. Code names these, never either crate
//! (`tests/one_regex_engine.rs`).

// Each name is used by the features that need it.
#![allow(unused_imports)]

#[cfg(feature = "schema-validation")]
pub(crate) use regex::{Captures, Regex, RegexBuilder};
#[cfg(not(feature = "schema-validation"))]
pub(crate) use regex_lite::{Captures, Regex, RegexBuilder};

#[cfg(test)]
mod tests {
    use crate::SecretOptions;

    /// `regex` reads Unicode classes and `regex-lite` refuses them: the
    /// placeholder pattern (as every other) is read by the build's one
    /// engine, the validator's when it is linked.
    #[test]
    fn every_pattern_uses_the_one_engine_of_the_build() {
        let full = cfg!(feature = "schema-validation");
        assert_eq!(super::Regex::new(r"\p{L}+").is_ok(), full);
        let secrets = SecretOptions::default().with_placeholder(r"«secret \p{L}+»");
        assert_eq!(secrets.check_placeholder().is_ok(), full);
    }
}
