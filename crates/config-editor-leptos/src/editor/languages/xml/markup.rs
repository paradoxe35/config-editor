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
    let mut open = OpenCount::default();
    text.chars().for_each(|character| open.read(character));
    open.at_end()
}

/// The first line from `last` (of `lines`, from 0) where the lines from
/// `first` through it, joined by line breaks, leave no element open
/// ([`open_elements`]); the last line when none does. The lines
/// are read once, so the end of an element of many lines costs its lines,
/// not their square (as counting each longer joined text again would).
pub(super) fn closing_line(lines: &[&str], (first, last): (usize, usize)) -> usize {
    let mut open = OpenCount::default();
    for (index, line) in lines.iter().enumerate().skip(first) {
        if index > first {
            open.read('\n');
        }
        crate::editor::work_count::note_many(crate::editor::work_count::Work::ClosingLineBytes, line.len() + 1);
        line.chars().for_each(|character| open.read(character));
        if index >= last && (open.at_end() == 0 || index + 1 == lines.len()) {
            return index;
        }
    }
    last
}

/// [`open_element_names`]'s count, read one character at a time: after
/// each, [`OpenCount::at_end`] is what it counts for the text read so far.
#[derive(Default)]
struct OpenCount {
    /// The elements the tags read whole leave open.
    open: usize,
    /// The tag being read (from its `<`, no `>` yet): its second character
    /// once read, and its last.
    tag: Option<(Option<char>, char)>,
}

impl OpenCount {
    fn read(&mut self, character: char) {
        match self.tag {
            None if character == '<' => self.tag = Some((None, '<')),
            None => {}
            Some((second, last)) if character == '>' => {
                self.tag = None;
                // As `open_element_names` reads the tag `<…>` whole.
                let self_closing = second.is_some() && last == '/';
                self.open = self.counted(second, self_closing);
            }
            Some((second, _)) => self.tag = Some((second.or(Some(character)), character)),
        }
    }

    /// The count for a text ending here (a tag cut off is read as
    /// `open_element_names` reads the rest of a text without its `>`).
    fn at_end(&self) -> usize {
        match self.tag {
            Some((second, _)) => self.counted(second, false),
            None => self.open,
        }
    }

    /// The count once a tag whose second character is `second` is read.
    fn counted(&self, second: Option<char>, self_closing: bool) -> usize {
        match second {
            Some('/') => self.open.saturating_sub(1),
            Some('!' | '?') => self.open,
            _ if self_closing => self.open,
            Some(character) if character.is_alphabetic() || character == '_' => self.open + 1,
            _ => self.open,
        }
    }
}

/// The names of the elements `text` starts and does not end, outermost
/// first.
pub(super) fn open_element_names(text: &str) -> Vec<&str> {
    let mut open = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find('<') {
        let tag = &rest[at..];
        let end = tag.find('>').map_or(tag.len(), |end| end + 1);
        let body = &tag[..end];
        if body.starts_with("</") {
            open.pop();
        } else if body.starts_with("<!") || body.starts_with("<?") || body.ends_with("/>") {
        } else if body[1..]
            .starts_with(|character: char| character.is_alphabetic() || character == '_')
        {
            let name = &body[1..];
            let name_end = name
                .find(|character: char| character.is_whitespace() || character == '>' || character == '/')
                .unwrap_or(name.len());
            open.push(&name[..name_end]);
        }
        rest = &tag[end.max(1)..];
    }
    open
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

/// The `<` before a start tag's name, and the name's prefix (`<c:` of
/// `<c:scan>`): the key is the element's local name (a
/// prefixed element's lines were not found, so a move among its siblings
/// in one save could not be written).
pub(super) fn name_markup_len(text: &str) -> Option<usize> {
    let name = text.strip_prefix('<')?;
    let starts_name = |text: &str| text.starts_with(|character: char| character.is_alphabetic() || character == '_');
    if !starts_name(name) {
        return None;
    }
    let end = name
        .find(|character: char| !(character.is_alphanumeric() || matches!(character, '_' | '-' | '.' | ':')))
        .unwrap_or(name.len());
    match name[..end].split_once(':') {
        Some((prefix, local)) if starts_name(local) => Some(1 + prefix.len() + 1),
        _ => Some(1),
    }
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
    use super::{closing_line, element_end, element_texts, open_element_names, open_elements};

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

    /// The markup before a key, the element's local name: its `<` and its
    /// prefix, of any length; found back from the name.
    #[test]
    fn a_keys_markup_is_its_open_bracket_and_prefix() {
        use crate::editor::flow_syntax::markup_before;
        use crate::editor::languages::Language;
        for (text, length) in [("<web>", 1), ("<c:scan/>", 3), ("<context:component-scan a='1'/>", 9), ("<a.b:c-d>", 5)] {
            assert_eq!(super::name_markup_len(text), Some(length), "{text}");
            let line = format!("  {text}");
            let behavior = Language::XML.behavior();
            assert_eq!(markup_before(behavior, &line, 2 + length), Some(2), "{text}");
        }
        for text in ["<a:>", "<a:1>", "<a: b>"] {
            assert_eq!(super::name_markup_len(text), Some(1), "{text}");
        }
        for text in ["</a>", "<!-- c -->", "<1>", "a"] {
            assert_eq!(super::name_markup_len(text), None, "{text}");
        }
    }

    #[test]
    fn open_elements_count_starts_without_ends() {
        assert_eq!(open_elements("web>\n<a>1</a>\n<b/>"), 0);
        assert_eq!(open_elements("<web>\n<a>1</a>\n<!-- c -->"), 1);
    }

    /// The elements a text leaves open, counted a character at a time, are
    /// the ones it names, and the line an element closes on, read once,
    /// is the line counting each longer text again finds, whatever the lines hold: tags cut by a
    /// line break, `/>`, `</`, comments and CDATA holding `<` and `>`, a
    /// lone `<`, letters past ASCII (a seeded random sweep).
    #[test]
    fn the_closing_line_read_once_is_the_one_counted_again() {
        const PIECES: [&str; 22] = [
            "<a>", "</a>", "<b/>", "<c x=\"1\"\n/>", "<", ">", "/", "/>", "</", "\n", "\n",
            "<!-- d > <e> -->", "<?p?>", "<![CDATA[<f>]]>", "text", "é", "<_g", "<\u{e9}h>", "<!",
            "<?", " ", "\"",
        ];
        let mut seed: u64 = 0x5eed_0b1c;
        let mut next = |below: usize| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            usize::try_from(seed % below as u64).expect("below a usize")
        };
        for _ in 0..4000 {
            let text: String = (0..next(40)).map(|_| PIECES[next(PIECES.len())]).collect();
            let lines: Vec<&str> = text.split('\n').collect();
            let first = next(lines.len());
            let last = first + next(lines.len() - first);
            let mut counted = last;
            while counted + 1 < lines.len() && !open_element_names(&lines[first..=counted].join("\n")).is_empty() {
                counted += 1;
            }
            assert_eq!(closing_line(&lines, (first, last)), counted, "{text:?} from {first}, {last}");
            assert_eq!(open_elements(&text), open_element_names(&text).len(), "{text:?}");
        }
    }
}
