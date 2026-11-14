//! How a host lays out its start tags' attributes: an attribute per line
//! (a Spring `<bean>`, a `pom.xml` root), columns of aligned attributes,
//! `=` with spaces around it, and the markup the reader leaves out
//! (namespace declarations, `xsi:*`, see `declarations`). A rewrite writes
//! each start tag as the host wrote it: every attribute it keeps with the
//! blanks before it and its `=` as written, markup back in its place, an
//! attribute added with the blanks of its neighbour (on a line of its own
//! where they have theirs), so a save adding or removing one changes only
//! that attribute's text.

use super::*;
use std::ops::Range;

/// One attribute in a start tag.
pub(super) struct Attribute {
    /// The blanks before it.
    blanks: Range<usize>,
    pub(super) name: Range<usize>,
    /// Its value, quotes included.
    pub(super) value: Range<usize>,
    pub(super) markup: bool,
}

impl Attribute {
    /// What is between its name and its value (`=`, ` = `).
    fn equals(&self) -> Range<usize> {
        self.name.end..self.value.start
    }
}

/// A start tag's attributes and where its name ends and its closing text
/// (the blanks before `>` or `/>`, and that) begins.
pub(super) struct Tag {
    pub(super) name_end: usize,
    pub(super) attributes: Vec<Attribute>,
    close: Range<usize>,
}

impl Tag {
    pub(super) fn of(source: &str, token: &Token, markup: &declarations::Markup) -> Self {
        let name_end = local_name_start(source, token) + name_len(source, token);
        let mut after = name_end;
        let mut attributes = Vec::new();
        let inner = tokenize_tag_inside(source, token);
        for window in inner.windows(3) {
            if let [name, Token { kind: Kind::Equals, .. }, value] = window
                && matches!(name.kind, Kind::Text { quoted: false })
                && matches!(value.kind, Kind::Text { quoted: true })
                && name.start >= after
            {
                attributes.push(Attribute {
                    blanks: after..name.start,
                    name: name.start..name.end,
                    value: value.start..value.end,
                    markup: markup.holds(&source[name.start..name.end]),
                });
                after = value.end;
            }
        }
        Tag {
            name_end,
            attributes,
            close: after..token.end,
        }
    }

    /// The tag with only the attributes `keep` keeps (each keeps the
    /// blanks before it).
    pub(super) fn keeping(&self, keep: impl Fn(&Attribute) -> bool) -> Tag {
        Tag {
            name_end: self.name_end,
            attributes: self
                .attributes
                .iter()
                .filter(|attribute| keep(attribute))
                .map(|attribute| Attribute {
                    blanks: attribute.blanks.clone(),
                    name: attribute.name.clone(),
                    value: attribute.value.clone(),
                    markup: attribute.markup,
                })
                .collect(),
            close: self.close.clone(),
        }
    }

    fn self_closing(&self, source: &str) -> bool {
        source[self.close.clone()].ends_with("/>")
    }

    /// The `name=value` texts of its data attributes.
    pub(super) fn pairs<'a>(&self, source: &'a str) -> Vec<(&'a str, &'a str)> {
        self.attributes
            .iter()
            .filter(|attribute| !attribute.markup)
            .map(|attribute| (&source[attribute.name.clone()], &source[attribute.value.clone()]))
            .collect()
    }
}

/// The lines (0-based) of `text` a tag continues on: its attributes over
/// lines, as the host aligns them.
pub(super) fn attribute_lines(text: &str) -> Vec<usize> {
    let mut lines = Vec::new();
    let (mut line, mut counted) = (0, 0);
    for token in tokenize(text).iter() {
        if !matches!(token.kind, Kind::OpenTag { .. }) {
            continue;
        }
        line += text[counted..token.start].matches('\n').count();
        let inside = text[token.start..token.end].matches('\n').count();
        lines.extend(line + 1..=line + inside);
        line += inside;
        counted = token.end;
    }
    lines
}

/// Whether the element whose start tag is `token` holds anything.
pub(super) fn filled(source: &str, token: &Token) -> bool {
    !source[..token.end].ends_with("/>") && !source[token.end..].starts_with("</")
}

/// The markup attributes of the start tag `token` of `source` (see
/// `declarations`), by name, each with its value as written, unquoted.
pub(super) fn markup_attributes(source: &str, token: &Token, markup: &declarations::Markup) -> Vec<(String, String)> {
    let mut found: Vec<(String, String)> = Tag::of(source, token, markup)
        .attributes
        .iter()
        .filter(|attribute| attribute.markup)
        .map(|attribute| (source[attribute.name.clone()].to_string(), source[attribute.value.clone()].to_string()))
        .map(|(name, value)| (name, value[1..value.len() - 1].to_string()))
        .collect();
    found.sort();
    found
}

/// Writes the attributes and close of the written tag `tag` laid out as
/// `host` is.
pub(super) fn laid_out(out: &mut String, (written, tag): (&str, &Tag), (previous, host): (&str, &Tag)) {
    let slots = &host.attributes;
    let mut emitted = vec![false; slots.len()];
    // Where the last attribute written stands among the host's.
    let mut last = None::<usize>;
    let emit_slot = |out: &mut String, index: usize, value: Option<(&str, &str)>, last: &mut Option<usize>| {
        let slot = &slots[index];
        out.push_str(&previous[slot.blanks.clone()]);
        match value {
            Some((name, value)) => {
                out.push_str(name);
                out.push_str(&previous[slot.equals()]);
                out.push_str(value);
            }
            None => out.push_str(&previous[slot.name.start..slot.value.end]),
        }
        *last = Some(index);
    };
    // The host's most common `=` spelling, for an attribute it lacks.
    let equals = {
        let mut counts: Vec<(&str, usize)> = Vec::new();
        for slot in slots.iter().filter(|slot| !slot.markup) {
            let text = &previous[slot.equals()];
            match counts.iter_mut().find(|(known, _)| *known == text) {
                Some((_, count)) => *count += 1,
                None => counts.push((text, 1)),
            }
        }
        counts.into_iter().rev().max_by_key(|(_, count)| *count).map(|(text, _)| text)
    };
    // The host's data attribute written `name`, not written yet.
    let own = |emitted: &[bool], name: &str| {
        (0..slots.len()).find(|index| {
            !slots[*index].markup && !emitted[*index] && previous[slots[*index].name.clone()] == *name
        })
    };
    for (at, attribute) in tag.attributes.iter().enumerate() {
        let name = &written[attribute.name.clone()];
        let value = &written[attribute.value.clone()];
        // The markup written before it (or before the next attribute the
        // host holds, for one it lacks) first, in its place.
        let anchor = own(&emitted, name).or_else(|| {
            tag.attributes[at + 1..].iter().find_map(|after| own(&emitted, &written[after.name.clone()]))
        });
        for before in 0..anchor.unwrap_or(slots.len()) {
            if slots[before].markup && !emitted[before] {
                emitted[before] = true;
                emit_slot(out, before, None, &mut last);
            }
        }
        match own(&emitted, name) {
            Some(index) => {
                emitted[index] = true;
                emit_slot(out, index, Some((name, value)), &mut last);
            }
            None => {
                // Spaced as the attribute after it is, or as the last one.
                let next = last.map_or(0, |index| index + 1);
                let blanks = slots
                    .get(next)
                    .or(slots.last())
                    .map_or(&written[attribute.blanks.clone()], |slot| &previous[slot.blanks.clone()]);
                out.push_str(blanks);
                out.push_str(name);
                out.push_str(equals.unwrap_or(&written[attribute.equals()]));
                out.push_str(value);
            }
        }
    }
    for index in 0..slots.len() {
        if slots[index].markup && !emitted[index] {
            emitted[index] = true;
            emit_slot(out, index, None, &mut last);
        }
    }
    // The host's blanks before `>` or `/>` where it closes alike.
    if host.self_closing(previous) == tag.self_closing(written) {
        out.push_str(&previous[host.close.clone()]);
    } else {
        out.push_str(&written[tag.close.clone()]);
    }
}

#[cfg(test)]
mod tests {
    use super::super::identity::Identity;
    use super::super::kept_elements;
    use super::super::parse::xml_parse;

    /// `written` laid out as `previous`, each element by the item it is.
    fn with_layout_of(written: &str, previous: &str) -> String {
        let (old, new) = (xml_parse(previous).expect("reads"), xml_parse(written).expect("reads"));
        kept_elements::with_layout_of(written, (previous, None), &Identity::of(Some(&old), &new))
    }

    #[test]
    fn declarations_go_back_in_their_places_as_written() {
        let host = "<r a='1'\n   xmlns:x=\"urn:x\"  b=\"2\"><s xmlns=\"u\">1</s></r>";
        assert_eq!(
            with_layout_of("<r a=\"3\" b=\"2\" c=\"4\"><s>1</s></r>", host),
            "<r a=\"3\"\n   xmlns:x=\"urn:x\"  b=\"2\"  c=\"4\"><s xmlns=\"u\">1</s></r>"
        );
        // An attribute removed takes its blanks; the markup stays.
        assert_eq!(
            with_layout_of("<r b=\"2\"><s>1</s></r>", host),
            "<r\n   xmlns:x=\"urn:x\"  b=\"2\"><s xmlns=\"u\">1</s></r>"
        );
        assert_eq!(with_layout_of("<r/>", "<r xmlns=\"u\"/>"), "<r xmlns=\"u\"/>");
    }

    #[test]
    fn each_item_takes_the_layout_of_its_own_element() {
        let host = "<l>\n  <i n=\"1\"\n     k=\"a\"/>\n  <i n=\"2\" k=\"b\"/>\n</l>";
        // An attribute added to a tag holding only markup: after it.
        assert_eq!(
            with_layout_of("<r a=\"1\"/>", "<r xmlns=\"u\"\n   xmlns:x=\"v\"/>"),
            "<r xmlns=\"u\"\n   xmlns:x=\"v\"\n   a=\"1\"/>"
        );
        // The first item removed: the second keeps its one line.
        assert_eq!(with_layout_of("<l>\n  <i n=\"2\" k=\"b\"/>\n</l>", host), "<l>\n  <i n=\"2\" k=\"b\"/>\n</l>");
        // An item added before: the others keep theirs.
        assert_eq!(
            with_layout_of("<l>\n  <i n=\"0\"/>\n  <i n=\"1\" k=\"a\"/>\n  <i n=\"2\" k=\"b\"/>\n</l>", host),
            "<l>\n  <i n=\"0\"/>\n  <i n=\"1\"\n     k=\"a\"/>\n  <i n=\"2\" k=\"b\"/>\n</l>"
        );
    }
}
