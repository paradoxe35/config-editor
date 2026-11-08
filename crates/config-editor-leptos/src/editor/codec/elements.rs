//! Where a markup element written over lines ends, for moving a repeated
//! element (`<s name="a">` … `</s>`) with all its lines.

/// The byte just past the element that opens at `open` (its `<`): after
/// its matching end tag, or after its `/>`. Comments, CDATA, processing
/// instructions and quoted attribute values are skipped. `None` when the
/// element does not end.
pub(super) fn element_end(text: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut at = open;
    while at < text.len() {
        let rest = &text[at..];
        if !rest.starts_with('<') {
            at += rest.find('<')?;
            continue;
        }
        let skipped = [
            ("<!--", "-->"),
            ("<![CDATA[", "]]>"),
            ("<?", "?>"),
            ("<!", ">"),
        ]
        .into_iter()
        .find(|(start, _)| rest.starts_with(start));
        if let Some((_, end)) = skipped {
            at += rest.find(end)? + end.len();
            continue;
        }
        let close = tag_close(rest)?;
        let tag = &rest[..close];
        at += close + 1;
        if tag.starts_with("</") {
            depth = depth.checked_sub(1)?;
        } else if !tag.ends_with('/') {
            depth += 1;
        }
        if depth == 0 {
            return Some(at);
        }
    }
    None
}

/// The offset of the `>` closing the tag `tag` starts with, outside quoted
/// attribute values.
fn tag_close(tag: &str) -> Option<usize> {
    let mut quote = None;
    for (index, character) in tag.char_indices() {
        match (quote, character) {
            (Some(open), _) if open == character => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(character),
            (None, '>') => return Some(index),
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::element_end;

    #[test]
    fn an_element_ends_after_its_own_end_tag_or_its_slash() {
        let text = "<s a=\"x>\">\n  <s/>\n  <!-- </s> -->\n  <p>1</p>\n</s> tail";
        assert_eq!(element_end(text, 0), Some(text.len() - " tail".len()));
        assert_eq!(element_end("<s name='c'/> <!-- c -->", 0), Some(13));
        assert_eq!(element_end("<s>\n  <p>", 0), None);
    }
}
