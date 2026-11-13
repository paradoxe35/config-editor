//! What a text that nests deeper than a language's reader goes is told:
//! one sentence for every language, at the place it goes too deep.

/// The problem for values nested deeper than the reader goes (maps and
/// lists, or elements, inside each other).
pub(super) fn too_deep(what: &str) -> String {
    format!("{what} nest too deeply here, each inside the one before: the editor reads no deeper")
}

/// What JSON, YAML, TOML and HCL values are.
#[cfg(any(feature = "json", feature = "yaml", feature = "toml", feature = "hcl"))]
pub(super) const VALUES: &str = "Maps and lists";
