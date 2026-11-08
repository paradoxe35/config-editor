//! Assignments aligned as formatters write them (`name = "a"` above
//! `p    = 1`): an Edit-as save compares such a line with the serializer's
//! `p = 1`, so the host's lines are found and kept.

use std::borrow::Cow;

/// `code` (a line without its comment, trimmed) with the blanks around its
/// first `=` outside quotes made one space each side; `code` itself when
/// it assigns nothing or is spaced so already.
pub(super) fn single_spaced(code: &str) -> Cow<'_, str> {
    let mut quote = None;
    let mut previous = ' ';
    for (at, character) in code.char_indices() {
        match quote {
            Some(open) if character == open && previous != '\\' => quote = None,
            Some(_) => {}
            None if matches!(character, '"' | '\'') => quote = Some(character),
            None if matches!(character, '{' | '[' | '(') => return Cow::Borrowed(code),
            None if character == '=' => {
                let rest = &code[at + 1..];
                if rest.starts_with('=') || code[..at].ends_with(['!', '<', '>', '=']) {
                    return Cow::Borrowed(code);
                }
                let (key, value) = (code[..at].trim_end(), rest.trim_start());
                if key.is_empty() {
                    return Cow::Borrowed(code);
                }
                let spaced = if value.is_empty() {
                    format!("{key} =")
                } else {
                    format!("{key} = {value}")
                };
                return if spaced == code {
                    Cow::Borrowed(code)
                } else {
                    Cow::Owned(spaced)
                };
            }
            None => {}
        }
        previous = character;
    }
    Cow::Borrowed(code)
}

#[cfg(test)]
mod tests {
    use super::single_spaced;

    #[test]
    fn aligned_assignments_compare_single_spaced() {
        assert_eq!(single_spaced("p    = 1"), "p = 1");
        assert_eq!(
            single_spaced("\"a b\"\t=\t\"x  =  y\""),
            "\"a b\" = \"x  =  y\""
        );
        assert_eq!(single_spaced("p = 1"), "p = 1");
        assert_eq!(single_spaced("a == b"), "a == b");
        assert_eq!(single_spaced("{ a    = 1 }"), "{ a    = 1 }");
        assert_eq!(single_spaced("[s]"), "[s]");
    }
}
