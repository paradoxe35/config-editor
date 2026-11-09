//! The keys already written, read around the caret's element
//! ([`existing_elements`]), are those every span of the document gives
//! (the reading they replaced), at every place of documents valid and not.

use super::*;

/// The keys already written, off every span the whole document's scan
/// records: what [`existing_elements`] answered before it read only around
/// the caret's element.
fn from_every_span(source: &str, cursor_byte: usize) -> Option<HashSet<String>> {
    let scan = analyze(source, cursor_byte);
    let snapshot = scan.snapshot.clone()?;
    let naming_element = snapshot
        .open_tag
        .is_some_and(|tag| is_element_name_position(&tag, cursor_byte));
    if snapshot.in_tag && !naming_element {
        if snapshot.pending_attr.is_some() {
            return None;
        }
        let mut names = existing_attributes(source, &snapshot, cursor_byte);
        if let Some(tag) = snapshot.open_tag {
            let mut element = strings(&snapshot.path);
            let name = token_name(source, &tag);
            if element.last() != Some(&name) {
                element.push(name);
            }
            names.extend(
                scan.key_spans
                    .iter()
                    .filter(|(segments, ..)| segments.len() == element.len() + 1 && segments.starts_with(&element))
                    .map(|(segments, ..)| segments[element.len()].clone()),
            );
        }
        return Some(names);
    }
    let holder = strings(&snapshot.path);
    Some(
        scan.key_spans
            .iter()
            .filter(|(_, start, end)| !(*start <= cursor_byte && cursor_byte <= *end))
            .filter(|(segments, ..)| segments.len() > holder.len() && segments.starts_with(&holder))
            .map(|(segments, ..)| segments[holder.len()].clone())
            .collect(),
    )
}

/// Documents whose elements nest, repeat, misnest, stay open, share names
/// with attributes, sit beside a config root or are named like indexes.
const DOCUMENTS: &[&str] = &[
    "<root config-root=\"true\">\n  <a k=\"1\">\n    <b>1</b>\n    <b>2</b>\n    <c x=\"y\"/>\n  </a>\n  <d>t</d>\n</root>\n",
    "<a>1</a>\n<b><c>2</c><c>3</c></b>\n<a>4</a>\n",
    "<r a=\"1\"><a>2</a><s a=\"3\" b='4'><a/></s></r>",
    "<r config-root=\"true\"><a><b>1</b></a></r>\n<a><b>2</b></a>\n",
    "<r config-root=\"true\"><a/></r><r config-root=\"true\"><b/></r>",
    "<r><0><a>1</a></0><x><0>2</0></x></r>",
    "<r><a><b>1</a><c>2</c></r>",
    "<r><a><b>1</bb><c>2</c></a></r>",
    "<r><a><b k=\"v\" <c>2</c></a></r>",
    "<r><a>\n  <b>1</b>\n  <",
    "<r>\n  <a>1</a>\n  <a>2</a>\n  ve\n</r>",
    "<r><!-- <a>x</a> --><b>1</b><![CDATA[<c>]]><?pi <d>?></r>",
    "<r><ns:a ns:k=\"1\"><ns:b>2</ns:b></ns:a></r>",
    "<r><a></a><a/><a>3</a></r>",
    "<r><a k=\"1\"/><a k=\"2\"><k>3</k></a></r>",
];

/// Every place of `source`, or (in a long one) `count` places spread over it.
fn places(source: &str, count: usize) -> Vec<usize> {
    let boundaries: Vec<usize> = (0..=source.len()).filter(|at| source.is_char_boundary(*at)).collect();
    let step = (boundaries.len() / count).max(1);
    boundaries.into_iter().step_by(step).collect()
}

/// `source` broken `count` ways: a character removed, or a `<`, `>`, `/`
/// or `"` added, at places spread over it.
fn broken(source: &str, count: usize) -> Vec<String> {
    let mut seed = 0x2545_f491_u64;
    (0..count)
        .map(|_| {
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
            let mut at = usize::try_from(seed >> 33).unwrap_or(0) % (source.len() + 1);
            while !source.is_char_boundary(at) {
                at -= 1;
            }
            let mut text = source.to_string();
            match (seed >> 20) % 5 {
                0 if at < source.len() => {
                    text.remove(at);
                }
                kind => text.insert(at, ['<', '>', '/', '"', '<'][usize::try_from(kind).unwrap_or(0)]),
            }
            text
        })
        .collect()
}

fn assert_same(source: &str, places: &[usize]) {
    for &cursor in places {
        assert_eq!(
            existing_elements(source, cursor),
            from_every_span(source, cursor),
            "at {cursor} of {source:?}"
        );
    }
}

#[test]
fn the_keys_around_the_caret_s_element_are_those_every_span_gives() {
    for document in DOCUMENTS {
        assert_same(document, &places(document, usize::MAX));
        for text in broken(document, 12) {
            assert_same(&text, &places(&text, usize::MAX));
        }
    }
}

#[test]
fn the_keys_around_the_caret_s_element_of_a_long_document_are_those_every_span_gives() {
    let long = include_str!("../../../../tests/fixtures/samples/tunnel.large.xml");
    assert_same(long, &places(long, 150));
    for text in broken(long, 4) {
        assert_same(&text, &places(&text, 40));
    }
}
