//! What the shared code asks of XML's markup: where an element ends, how
//! many a text leaves open, what closes one, where a tag's name starts, and
//! which lines start repeated elements.

/// The byte just past the element that opens at `open` (its `<`): after
/// its matching end tag, or after its `/>`. Comments, CDATA, processing
/// instructions and quoted attribute values are skipped. `None` when the
/// element does not end.
pub(super) fn element_end(text: &str, open: usize) -> Option<usize> {
    if !text[open..].starts_with('<') {
        return None;
    }
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

/// Elements started and not ended in `text`.
pub(super) fn open_elements(text: &str) -> usize {
    let mut depth = 0usize;
    let mut rest = text;
    while let Some(at) = rest.find('<') {
        let tag = &rest[at..];
        let end = tag.find('>').map_or(tag.len(), |end| end + 1);
        let body = &tag[..end];
        if body.starts_with("</") {
            depth = depth.saturating_sub(1);
        } else if body.starts_with("<!") || body.starts_with("<?") || body.ends_with("/>") {
        } else if body[1..]
            .starts_with(|character: char| character.is_alphabetic() || character == '_')
        {
            depth += 1;
        }
        rest = &tag[end.max(1)..];
    }
    depth
}

/// The length of what closes an element at the start of `text`: its text
/// (over lines too) then its end tag.
pub(super) fn closer_len(text: &str) -> Option<usize> {
    let text_len = text.find('<').unwrap_or(text.len());
    let tag = &text[text_len..];
    end_tag_len(tag).map(|length| text_len + length)
}

/// The length of the end tag `text` starts with.
pub(super) fn end_tag_len(text: &str) -> Option<usize> {
    if !text.starts_with("</") {
        return None;
    }
    text.find('>').map(|end| end + 1)
}

/// The `<` before a start tag's name.
pub(super) fn name_markup_len(text: &str) -> Option<usize> {
    let name = text.strip_prefix('<')?;
    name.starts_with(|character: char| character.is_alphabetic() || character == '_')
        .then_some(1)
}

/// Whether whitespace between `before` and `after` is an element text's own
/// space (at its edges too) or would part a tag's name from its `<` or
/// `</`.
pub(super) fn space_is_content(before: &str, after: &str) -> bool {
    let Some(last) = before.chars().next_back() else {
        return false;
    };
    let opens_tag = last == '<' || before.ends_with("</");
    let in_text = after
        .chars()
        .next()
        .is_some_and(|next| (last == '>') != (next == '<'));
    in_text || opens_tag
}

/// Whether line `index` of `lines` starts an element whose name another
/// line at its indent repeats (`<s …>` twice).
pub(super) fn opens_repeated_element(
    comment_start: &dyn Fn(&str) -> usize,
    lines: &[&str],
    index: usize,
) -> bool {
    let code = |line: &str| line[..comment_start(line)].trim_end().to_string();
    let indent = |line: &str| line.len() - line.trim_start().len();
    let line = code(lines[index]);
    let at = indent(&line);
    let rest = line.trim_start();
    if !rest.starts_with('<') {
        return false;
    }
    let name_end = rest[1..]
        .find(|character: char| character.is_whitespace() || matches!(character, '>' | '/'))
        .map_or(rest.len(), |end| end + 1);
    if name_end <= 1 || rest.starts_with("</") || rest.starts_with("<!") || rest.starts_with("<?")
    {
        return false;
    }
    let head = &rest[..name_end];
    let same_head = |other: &str| {
        let other = code(other);
        indent(&other) == at
            && other.trim_start().strip_prefix(head).is_some_and(|after| {
                after.is_empty()
                    || after.starts_with(|character: char| {
                        character.is_whitespace() || matches!(character, '>' | '/')
                    })
            })
    };
    lines
        .iter()
        .enumerate()
        .any(|(other, line)| other != index && same_head(line))
}

/// The texts of elements that hold more than whitespace: from right after
/// a tag's `>` to right before the next `<` (byte ranges).
pub(super) fn element_texts(text: &str) -> Vec<std::ops::Range<usize>> {
    let mut areas = Vec::new();
    let mut in_tag = false;
    let mut start: Option<usize> = None;
    let mut blank = true;
    for (byte, character) in text.char_indices() {
        match character {
            '<' => {
                if let Some(from) = start.take()
                    && !blank
                {
                    areas.push(from..byte);
                }
                in_tag = true;
            }
            '>' if in_tag => {
                in_tag = false;
                start = Some(byte + 1);
                blank = true;
            }
            _ if !in_tag && !character.is_whitespace() => blank = false,
            _ => {}
        }
    }
    areas
}

#[cfg(test)]
mod tests {
    use super::{element_end, element_texts, open_elements};

    #[test]
    fn element_texts_are_the_texts_holding_more_than_whitespace() {
        let text = "<a>\n  <b>x y</b>\n  <c> </c>\n<d>l1\nl2</d></a>";
        let found: Vec<&str> = element_texts(text)
            .into_iter()
            .map(|range| &text[range])
            .collect();
        assert_eq!(found, ["x y", "l1\nl2"]);
    }

    #[test]
    fn an_element_ends_after_its_own_end_tag_or_its_slash() {
        let text = "<s a=\"x>\">\n  <s/>\n  <!-- </s> -->\n  <p>1</p>\n</s> tail";
        assert_eq!(element_end(text, 0), Some(text.len() - " tail".len()));
        assert_eq!(element_end("<s name='c'/> <!-- c -->", 0), Some(13));
        assert_eq!(element_end("<s>\n  <p>", 0), None);
    }

    #[test]
    fn open_elements_count_starts_without_ends() {
        assert_eq!(open_elements("web>\n<a>1</a>\n<b/>"), 0);
        assert_eq!(open_elements("<web>\n<a>1</a>\n<!-- c -->"), 1);
    }
}
