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
struct Attribute {
    /// The blanks before it.
    blanks: Range<usize>,
    name: Range<usize>,
    /// Its value, quotes included.
    value: Range<usize>,
    markup: bool,
}

impl Attribute {
    /// What is between its name and its value (`=`, ` = `).
    fn equals(&self) -> Range<usize> {
        self.name.end..self.value.start
    }
}

/// A start tag's attributes and where its name ends and its closing text
/// (the blanks before `>` or `/>`, and that) begins.
struct Tag {
    name_end: usize,
    attributes: Vec<Attribute>,
    close: Range<usize>,
}

impl Tag {
    fn of(source: &str, token: &Token, markup: &declarations::Markup) -> Self {
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

    fn self_closing(&self, source: &str) -> bool {
        source[self.close.clone()].ends_with("/>")
    }

    /// The `name=value` texts of its data attributes.
    fn pairs<'a>(&self, source: &'a str) -> Vec<(&'a str, &'a str)> {
        self.attributes
            .iter()
            .filter(|attribute| !attribute.markup)
            .map(|attribute| (&source[attribute.name.clone()], &source[attribute.value.clone()]))
            .collect()
    }
}

/// A host start tag, by its element's path without indices.
struct HostTag {
    tag: Tag,
    pairs: Vec<(String, String)>,
}

fn unindexed(path: &[Seg]) -> Vec<String> {
    strings(path).into_iter().filter(|segment| crate::editor::language::pointer_index(segment).is_none()).collect()
}

/// `written` (a rewrite of `previous`) with each start tag laid out as
/// `previous` writes the same element's (see the module documentation);
/// the attributes keep the order, names and values of `written`.
pub(super) fn with_layout_of(written: &str, previous: &str) -> String {
    let markup = declarations::Markup::of(previous);
    let host_tokens = tokenize(previous);
    let host_plan = plan_paths(previous, &host_tokens);
    let mut host: HashMap<Vec<String>, Vec<HostTag>> = HashMap::new();
    for token in &host_tokens {
        if !matches!(token.kind, Kind::OpenTag { closing: false, .. }) {
            continue;
        }
        let Some(path) = host_plan.path_of(token.start) else { continue };
        let tag = Tag::of(previous, token, &markup);
        let pairs = tag.pairs(previous).into_iter().map(|(name, value)| (name.to_string(), value.to_string())).collect();
        host.entry(unindexed(path)).or_default().push(HostTag { tag, pairs });
    }
    let tokens = tokenize(written);
    let plan = plan_paths(written, &tokens);
    let mut seen: HashMap<Vec<String>, usize> = HashMap::new();
    let mut out = String::with_capacity(written.len() + previous.len() / 8);
    let mut copied = 0;
    for token in &tokens {
        if !matches!(token.kind, Kind::OpenTag { closing: false, .. }) {
            continue;
        }
        let Some(path) = plan.path_of(token.start) else { continue };
        let path = unindexed(path);
        let ordinal = {
            let count = seen.entry(path.clone()).or_default();
            *count += 1;
            *count - 1
        };
        let tag = Tag::of(written, token, &markup);
        let Some(like) = host.get(&path).and_then(|tags| matched(tags, ordinal, &tag.pairs(written))) else {
            continue;
        };
        out.push_str(&written[copied..tag.name_end]);
        laid_out(&mut out, (written, &tag), (previous, &like.tag));
        copied = token.end;
    }
    out.push_str(&written[copied..]);
    out
}

/// The host tag of `tags` (one element path's, in order) a written tag,
/// the `ordinal`-th of that path, stands for: of those a few places
/// around it (an item added or removed before it shifts it), the one
/// sharing the most attributes with it, then the nearest.
fn matched<'a>(tags: &'a [HostTag], ordinal: usize, pairs: &[(&str, &str)]) -> Option<&'a HostTag> {
    let shared = |host: &HostTag| {
        pairs.iter().filter(|(name, value)| host.pairs.iter().any(|(other, held)| other == name && held == value)).count()
    };
    (ordinal.saturating_sub(2)..tags.len().min(ordinal + 3))
        .max_by_key(|index| (shared(&tags[*index]), std::cmp::Reverse(index.abs_diff(ordinal))))
        .map(|index| &tags[index])
}

/// Writes the attributes and close of the written tag `tag` laid out as
/// `host` is.
fn laid_out(out: &mut String, (written, tag): (&str, &Tag), (previous, host): (&str, &Tag)) {
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
    use super::with_layout_of;

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
