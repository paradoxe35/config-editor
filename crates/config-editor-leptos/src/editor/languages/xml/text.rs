//! Element text written in place of the host's: all of it between the tags
//! (its whitespace is part of the value), in CDATA where the host used it.

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

/// `text` as element content where `written` was: escaped, or as a CDATA
/// section when the host wrote one (whitespace outside it kept outside when
/// the new text starts and ends with the same).
pub(super) fn element_text_like(text: &str, written: &str) -> String {
    let (Some(open), Some(close)) = (written.find("<![CDATA["), written.rfind("]]>")) else {
        return escape_element_text(text);
    };
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
    if !code.contains("<![CDATA[") && !code.contains("]]>") && !code.contains('&') {
        return std::borrow::Cow::Borrowed(code);
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

/// `text` escaped as element content: `&`, `<`, and the `>` of a `]]>`.
pub(super) fn escape_element_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace("]]>", "]]&gt;")
}
