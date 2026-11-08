//! Markup XML completes as it is typed: the closing quote of an attribute
//! value (also before a tag's `/>`), and the end tag `</` begins.

use super::super::super::keyboard::utf16_to_byte_index;
use super::editing::open_elements;

/// Where the caret is, read from the text before it.
struct Place {
    /// The byte the tag the caret is in starts at, if it is in one.
    tag_start: Option<usize>,
    /// Inside a comment, CDATA section, processing instruction or quoted
    /// attribute value: text, where nothing completes.
    in_text: bool,
}

fn place(before: &str) -> Place {
    let unclosed =
        |open: &str, close: &str| before.rfind(open).is_some_and(|at| !before[at..].contains(close));
    if unclosed("<!--", "-->") || unclosed("<![CDATA[", "]]>") || unclosed("<?", "?>") {
        return Place {
            tag_start: None,
            in_text: true,
        };
    }
    let mut tag_start = None;
    let mut quote: Option<char> = None;
    for (index, character) in before.char_indices() {
        match quote {
            Some(open) if character == open => quote = None,
            Some(_) => {}
            None if character == '<' => tag_start = Some(index),
            None if character == '>' => tag_start = None,
            None if matches!(character, '"' | '\'') && tag_start.is_some() => quote = Some(character),
            None => {}
        }
    }
    Place {
        tag_start,
        in_text: quote.is_some(),
    }
}

/// The closing quote for `quote` typed after `=` in a start tag, where the
/// shared pairing does not close it: before the tag's `/>` (`<r a=|/>`).
pub(super) fn attribute_quote(source: &str, cursor: usize, quote: char) -> Option<String> {
    let byte = utf16_to_byte_index(source, cursor);
    let before = &source[..byte];
    let at = place(before);
    let tag = &before[at.tag_start?..];
    let after_equals = tag.trim_end_matches([' ', '\t']).ends_with('=');
    let next = source[byte..].chars().next();
    (!at.in_text && !tag.starts_with(['/', '?', '!']) && after_equals && next == Some('/'))
        .then(|| quote.to_string())
}

/// The rest of the end tag of the innermost open element, for `/` typed
/// right after a `<` in element content (`<a>x<|` → `</a>`); `None` inside
/// a tag, comment, CDATA or attribute value, with no element open, or
/// before a name (`<|b>`, the start tag being turned into an end tag).
pub(super) fn end_tag_after_slash(source: &str, cursor: usize) -> Option<String> {
    let byte = utf16_to_byte_index(source, cursor);
    let before = source[..byte].strip_suffix('<')?;
    let at = place(before);
    if at.in_text || at.tag_start.is_some() {
        return None;
    }
    let next = source[byte..].chars().next();
    if next.is_some_and(|next| !next.is_whitespace() && next != '<') {
        return None;
    }
    let open = open_elements(before);
    let (name, _) = open.last()?;
    // Its end tag already follows (written when its start tag was).
    let rest = format!("{name}>");
    let ahead = source[byte..].trim_start();
    (!ahead.starts_with(&format!("</{rest}"))).then_some(rest)
}

#[cfg(test)]
mod tests {
    use super::{attribute_quote, end_tag_after_slash};

    fn at(marked: &str) -> (String, usize) {
        let at = marked.find('|').unwrap();
        (marked.replacen('|', "", 1), marked[..at].encode_utf16().count())
    }

    #[test]
    fn a_slash_after_a_less_than_writes_the_open_elements_end_tag() {
        for (marked, written) in [
            ("<r>\n  <a>x<|\n</r>\n", Some("a>")),
            ("<r>\n  <a>\n    <b/>\n  <|", Some("a>")),
            ("<r><a>1</a><|", Some("r>")),
            ("<r><x:a k=\"1\">t<|", Some("x:a>")),
            ("<r><!-- <a><| -->", None),
            ("<r><![CDATA[<|", None),
            ("<r a=\"<|\"", None),
            ("<r><|b>", None),
            ("<|", None),
            ("<r/><|", None),
            ("<tcp><|</tcp>", None),
            ("<tcp>\n  <|\n</tcp>", None),
        ] {
            let (source, cursor) = at(marked);
            assert_eq!(
                end_tag_after_slash(&source, cursor).as_deref(),
                written,
                "{marked:?}"
            );
        }
    }

    #[test]
    fn a_quote_after_equals_closes_before_the_tags_slash() {
        for (marked, closed) in [
            ("<r a=|/>", Some("\"")),
            ("<r a = |/>", Some("\"")),
            ("<r b=\"1\" a=|/>", Some("\"")),
            ("<r a|/>", None),
            ("<r>a=|/</r>", None),
            ("<!-- a=|/ -->", None),
        ] {
            let (source, cursor) = at(marked);
            assert_eq!(
                attribute_quote(&source, cursor, '"').as_deref(),
                closed,
                "{marked:?}"
            );
        }
    }
}
