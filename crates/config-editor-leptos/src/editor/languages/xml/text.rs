//! Element text written in place of the host's: all of it between the tags
//! (its whitespace is part of the value), in CDATA where the host used it,
//! and what did not change kept as written (its references and CDATA
//! sections).

use super::{named_entity, numeric_entity};

/// `range` (an element text's span, trimmed) widened to the element's whole
/// content when the element holds nothing but that text: from its start
/// tag's `>` to its end tag's `<`.
pub(super) fn written_text_range(
    source: &str,
    range: std::ops::Range<usize>,
) -> std::ops::Range<usize> {
    let before = &source[..range.start];
    let after = &source[range.end..];
    let lead = before.len() - before.trim_end().len();
    let trail = after.len() - after.trim_start().len();
    let opened_by_start_tag = before.trim_end().ends_with('>') && {
        let tag_end = before.trim_end().len();
        before[..tag_end].rfind('<').is_some_and(|open| {
            let tag = &before[open..tag_end];
            !tag.starts_with("</")
                && !tag.starts_with("<!")
                && !tag.starts_with("<?")
                && !tag.ends_with("/>")
        })
    };
    if opened_by_start_tag && after[trail..].starts_with("</") {
        range.start - lead..range.end + trail
    } else {
        range
    }
}

/// `text` as element content where `written` was: the unchanged start and
/// end as written, the change escaped or in CDATA beside CDATA (see
/// [`spliced`]); text that is only whitespace, which reads as nothing
/// outside CDATA, as a CDATA section when the host wrote one.
pub(super) fn element_text_like(text: &str, written: &str) -> String {
    if !text.trim().is_empty()
        && let Some(out) = spliced(text, written, false, &escape_element_text)
    {
        return out;
    }
    let (Some(open), Some(close)) = (written.find("<![CDATA["), written.rfind("]]>")) else {
        return escape_element_text(text);
    };
    if text.contains('\r') {
        return escape_element_text(text);
    }
    let lead = &written[..open];
    let trail = &written[close + 3..];
    if lead.trim().is_empty()
        && trail.trim().is_empty()
        && text.len() >= lead.len() + trail.len()
        && text.starts_with(lead)
        && text[lead.len()..].ends_with(trail)
    {
        let middle = &text[lead.len()..text.len() - trail.len()];
        format!("{lead}{}{trail}", cdata(middle))
    } else {
        cdata(text)
    }
}

/// `text` in a CDATA section; a `]]>` in it is split across two sections.
fn cdata(text: &str) -> String {
    format!("<![CDATA[{}]]>", text.replace("]]>", "]]]]><![CDATA[>"))
}

/// `code` (one line) with CDATA markers dropped and the predefined entity
/// references read, so text compares the same however it is written.
pub(super) fn comparable_text(code: &str) -> std::borrow::Cow<'_, str> {
    let code = without_prefixes(code);
    if !code.contains("<![CDATA[") && !code.contains("]]>") && !code.contains('&') {
        return code;
    }
    let text = code
        .replace("<![CDATA[", "")
        .replace("]]>", "")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&");
    std::borrow::Cow::Owned(text)
}

/// `code` with the namespace prefixes of its tag names left out
/// (`<k:a>1</k:a>` is `<a>1</a>`): the model keys an element by its local
/// name, as the serializer writes it.
fn without_prefixes(code: &str) -> std::borrow::Cow<'_, str> {
    let name_start = |character: char| character.is_alphabetic() || character == '_';
    let name_part =
        |character: char| character.is_alphanumeric() || matches!(character, '_' | '-' | '.');
    let mut out = String::new();
    let mut rest = code;
    let mut changed = false;
    while let Some(open) = rest.find('<') {
        let after = &rest[open + 1..];
        let tag = after.strip_prefix('/').unwrap_or(after);
        let head = rest.len() - tag.len();
        let prefix_len = tag
            .find(|character: char| !name_part(character))
            .unwrap_or(tag.len());
        let prefixed = tag.starts_with(name_start)
            && tag[prefix_len..].starts_with(':')
            && tag[prefix_len + 1..].starts_with(name_start);
        out.push_str(&rest[..head]);
        rest = &rest[head..];
        if prefixed {
            rest = &rest[prefix_len + 1..];
            changed = true;
        }
    }
    if !changed {
        return std::borrow::Cow::Borrowed(code);
    }
    out.push_str(rest);
    std::borrow::Cow::Owned(out)
}

/// `text` escaped as element content: `&`, `<`, the `>` of a `]]>`, and a
/// carriage return (readers read a raw one before a line break as nothing).
pub(super) fn escape_element_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace("]]>", "]]&gt;")
        .replace('\r', "&#13;")
}

/// One piece of written text: the character it reads as (none for a
/// comment or processing instruction, which read as nothing), how it is
/// written (a reference such as `&#233;`, or itself), and the CDATA section
/// holding it.
struct Unit<'a> {
    read: Option<char>,
    written: &'a str,
    section: Option<usize>,
}

/// `written` (element text, or an attribute value without its quotes) read
/// one piece at a time; `None` when it holds a child element. An attribute
/// value reads its own line breaks and tabs as spaces.
fn units(written: &str, attribute: bool) -> Option<Vec<Unit<'_>>> {
    let mut out = Vec::new();
    let mut sections = 0;
    let mut at = 0;
    while at < written.len() {
        let rest = &written[at..];
        if !attribute && let Some(body) = rest.strip_prefix("<![CDATA[") {
            let close = body.find("]]>")?;
            let start = at + "<![CDATA[".len();
            for (offset, read) in body[..close].char_indices() {
                let from = start + offset;
                out.push(Unit { read: Some(read), written: &written[from..from + read.len_utf8()], section: Some(sections) });
            }
            sections += 1;
            at = start + close + "]]>".len();
            continue;
        }
        // A comment or processing instruction among the text: kept, read
        // as nothing.
        if !attribute
            && let Some((open, close)) = [("<!--", "-->"), ("<?", "?>")].into_iter().find(|(open, _)| rest.starts_with(open))
        {
            let length = rest[open.len()..].find(close)? + open.len() + close.len();
            out.push(Unit { read: None, written: &rest[..length], section: None });
            at += length;
            continue;
        }
        if rest.starts_with('<') {
            return None;
        }
        if rest.starts_with('&')
            && let Some(semi) = rest[1..rest.len().min(12)].find(';')
            && let Some(read) = named_entity(&rest[1..1 + semi]).or_else(|| numeric_entity(&rest[1..1 + semi]))
        {
            out.push(Unit { read: Some(read), written: &rest[..semi + 2], section: None });
            at += semi + 2;
            continue;
        }
        let read = rest.chars().next()?;
        let length = read.len_utf8();
        let shown = if attribute && matches!(read, '\n' | '\t' | '\r') { ' ' } else { read };
        out.push(Unit { read: Some(shown), written: &rest[..length], section: None });
        at += length;
    }
    Some(out)
}

/// `text` written over `written` keeping what did not change as the host
/// wrote it: the common start and end keep their references, CDATA
/// sections, comments and processing instructions, and the changed middle
/// is written by `escape`, or inside CDATA where it replaces or extends
/// CDATA text; a comment inside the change stays, after it. `None` when
/// `written` holds a child element.
fn spliced(text: &str, written: &str, attribute: bool, escape: &dyn Fn(&str) -> String) -> Option<String> {
    let units = units(written, attribute)?;
    // The units that read as a character.
    let chars: Vec<usize> = units.iter().enumerate().filter(|(_, unit)| unit.read.is_some()).map(|(index, _)| index).collect();
    let read = |index: usize| units[chars[index]].read;
    let new: Vec<char> = text.chars().collect();
    let prefix = (0..chars.len().min(new.len())).take_while(|&index| read(index) == Some(new[index])).count();
    let room = (chars.len() - prefix).min(new.len() - prefix);
    let suffix = (0..room).take_while(|&back| read(chars.len() - 1 - back) == Some(new[new.len() - 1 - back])).count();
    // The kept start runs through its last character (or the comments
    // before the first), the kept end from its first (or the comments
    // after the last).
    let keep_start = match prefix {
        0 => chars.first().copied().unwrap_or(units.len()),
        _ => chars[prefix - 1] + 1,
    };
    let keep_end = match suffix {
        0 => chars.last().map_or(units.len(), |last| last + 1).max(keep_start),
        _ => chars[chars.len() - suffix],
    };
    let removed: Vec<&Unit<'_>> = units[keep_start..keep_end].iter().filter(|unit| unit.read.is_some()).collect();
    let kept_markers: Vec<&Unit<'_>> = units[keep_start..keep_end].iter().filter(|unit| unit.read.is_none()).collect();
    let middle: String = new[prefix..new.len() - suffix].iter().collect();
    // What the middle sits beside: the text's edge, plain text, or a CDATA
    // section.
    let side = |unit: Option<&Unit<'_>>| unit.map(|unit| unit.section.ok_or(()));
    let before = side(prefix.checked_sub(1).map(|index| &units[chars[index]]));
    let after = side((suffix > 0).then(|| &units[chars[chars.len() - suffix]]));
    let section = match removed.first().map(|unit| unit.section) {
        Some(Some(first)) if removed.iter().all(|unit| unit.section == Some(first)) => Some(first),
        _ => match (before, after) {
            (Some(Ok(left)), Some(Ok(right))) if left == right => Some(left),
            (Some(Ok(edge)), None) | (None, Some(Ok(edge))) => Some(edge),
            _ => None,
        },
    };
    let middle_written = match section {
        // CDATA has no escapes: a carriage return in it would be read as
        // nothing.
        Some(_) if middle.contains("]]>") || middle.contains('\r') => return None,
        Some(_) => middle,
        None => escape(&middle),
    };
    let mut out = String::with_capacity(written.len() + middle_written.len());
    let mut open: Option<usize> = None;
    let mut push = |out: &mut String, section: Option<usize>, piece: &str| {
        if piece.is_empty() {
            return;
        }
        if section != open {
            if open.is_some() {
                out.push_str("]]>");
            }
            if section.is_some() {
                out.push_str("<![CDATA[");
            }
            open = section;
        }
        out.push_str(piece);
    };
    for unit in &units[..keep_start] {
        push(&mut out, unit.section, unit.written);
    }
    push(&mut out, section, &middle_written);
    for unit in kept_markers.into_iter().chain(&units[keep_end..]) {
        push(&mut out, unit.section, unit.written);
    }
    if open.is_some() {
        out.push_str("]]>");
    }
    Some(out)
}

/// `text` as an attribute value where `inner` (the value without its
/// quotes) was: the unchanged start and end as written, the change by
/// `escape`.
pub(super) fn attribute_like(text: &str, inner: &str, escape: &dyn Fn(&str) -> String) -> String {
    spliced(text, inner, true, escape).unwrap_or_else(|| escape(text))
}
