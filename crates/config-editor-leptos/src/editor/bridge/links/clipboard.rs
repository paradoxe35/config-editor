//! The editor's own record of text copied or cut with its items' links:
//! the text a copy or cut put on the clipboard, and the text an edit
//! removed with the items it covered (a cut without the clipboard: a
//! drag, a line deleted and put back). An insert of a record's text (a
//! paste, a drop) takes the record's links, each on the same line and
//! character of the text: the text matches with its lines' indents and
//! the blank lines around it written otherwise (pasted at another
//! indent, or indented after). Any other text (an external paste) takes
//! none, and its items are new.

use super::super::super::text::{TextRange, TextSize};
use super::{Link, Source};
use std::sync::Arc;

/// Records kept: the newest replace the oldest past this many, or past
/// this many bytes of text (the newest is always kept).
const MOST_RECORDS: usize = 8;
const MOST_BYTES: usize = 8 * 1024 * 1024;

/// A text's lines that are not blank, each without its indent and
/// trailing blanks: what a record is matched by.
struct Shape {
    key: String,
    /// Where each of those lines' text is in the text.
    starts: Vec<TextRange>,
}

impl Shape {
    fn of(text: &str) -> Option<Shape> {
        let mut key = String::new();
        let mut starts = Vec::new();
        let mut at = TextSize::default();
        for line in text.split('\n') {
            let content = line.trim();
            if !content.is_empty() {
                if !starts.is_empty() {
                    key.push('\n');
                }
                key.push_str(content);
                let indent = TextSize::of(line) - TextSize::of(line.trim_start());
                starts.push(TextRange::at(at + indent, TextSize::of(content)));
            }
            at += TextSize::of(line) + TextSize::from(1);
        }
        (!starts.is_empty()).then_some(Shape { key, starts })
    }

    /// `offset` of the text as a line of the shape and a byte in its
    /// text (in a blank margin: that text's nearest end).
    fn place(&self, offset: TextSize, end: bool) -> (usize, TextSize) {
        let line = self
            .starts
            .partition_point(|start| start.start() < offset + TextSize::from(u32::from(!end)))
            .saturating_sub(1);
        let text = self.starts[line];
        let column = offset.checked_sub(text.start()).unwrap_or_default();
        (line, column.min(text.len()))
    }

    /// The offset of a line of the shape's byte.
    fn offset(&self, (line, column): (usize, TextSize)) -> TextSize {
        self.starts[line].start() + column
    }
}

/// A record's link: its start's and its end's line (of the shape) and
/// byte in that line's text.
#[derive(Clone, Debug)]
struct Placed {
    start: (usize, TextSize),
    end: (usize, TextSize),
    source: Arc<Source>,
}

#[derive(Debug, Default)]
pub(crate) struct Clipboard {
    /// Each record's shape and links; newest last.
    records: Vec<(String, Vec<Placed>)>,
}

/// `links` (from the start of `text`) placed by `text`'s shape.
fn placed(text: &str, links: Vec<Link>) -> Option<(String, Vec<Placed>)> {
    let shape = Shape::of(text)?;
    let links = links
        .into_iter()
        .map(|link| Placed {
            start: shape.place(link.range.start(), false),
            end: shape.place(link.range.end(), true),
            source: link.source,
        })
        .collect();
    Some((shape.key, links))
}

/// `links` put on `text`, a text of the same shape: a link whose text
/// was only blanks there (placed on one point of the shape) is no item's.
fn put(shape: &Shape, links: &[Placed]) -> Vec<Link> {
    links
        .iter()
        .filter_map(|link| {
            let (start, end) = (shape.offset(link.start), shape.offset(link.end));
            (start < end).then(|| Link {
                range: TextRange::new(start, end),
                source: link.source.clone(),
            })
        })
        .collect()
}

/// Whether `inserted` is `removed` written again, its lines indented
/// otherwise.
pub(super) fn same_shape(removed: &str, inserted: &str) -> bool {
    match (Shape::of(removed), Shape::of(inserted)) {
        (Some(removed), Some(inserted)) => removed.key == inserted.key,
        _ => false,
    }
}

/// The links `links` of `removed` (from its start) on `inserted`, the
/// same text with its lines indented otherwise (see [`same_shape`]).
pub(super) fn reindented(removed: &str, links: Vec<Link>, inserted: &str) -> Vec<Link> {
    match (placed(removed, links), Shape::of(inserted)) {
        (Some((_, placed)), Some(shape)) => put(&shape, &placed),
        _ => Vec::new(),
    }
}

impl Clipboard {
    /// `text`, copied or removed, holding `links` (from its start).
    pub(super) fn keep(&mut self, text: &str, links: Vec<Link>) {
        let Some((key, links)) = placed(text, links) else {
            return;
        };
        self.records.retain(|(known, _)| *known != key);
        self.records.push((key, links));
        let mut bytes = 0;
        let kept = self
            .records
            .iter()
            .rev()
            .take(MOST_RECORDS)
            .take_while(|(key, links)| {
                bytes += key.len() + links.len() * std::mem::size_of::<Placed>();
                bytes <= MOST_BYTES
            })
            .count()
            .max(1);
        let dropped = self.records.len() - kept;
        self.records.drain(..dropped);
    }

    /// `text` copied with the links `links` (from its start, as
    /// [`super::Links::copied`] reads them).
    pub(crate) fn copied(&mut self, text: &str, links: Vec<(TextRange, Arc<Source>)>) {
        let links = links
            .into_iter()
            .map(|(range, source)| Link { range, source })
            .collect();
        self.keep(text, links);
    }

    /// The newest record of `text`'s shape.
    fn record(&self, shape: &Shape) -> Option<&[Placed]> {
        self.records
            .iter()
            .rev()
            .find(|(known, _)| *known == shape.key)
            .map(|(_, links)| links.as_slice())
    }

    /// Whether `text` is a record's, with links.
    pub(super) fn holds(&self, text: &str) -> bool {
        Shape::of(text)
            .is_some_and(|shape| self.record(&shape).is_some_and(|links| !links.is_empty()))
    }

    /// The links of the record of `text`, from its start (none for
    /// another text).
    pub(super) fn links_of(&self, text: &str) -> Vec<Link> {
        let Some(shape) = Shape::of(text) else {
            return Vec::new();
        };
        self.record(&shape)
            .map(|links| put(&shape, links))
            .unwrap_or_default()
    }

    /// How many records are kept (for the memory tests).
    #[cfg(all(
        test,
        feature = "yaml",
        feature = "json",
        feature = "toml",
        feature = "hcl",
        feature = "xml"
    ))]
    pub(crate) fn len(&self) -> usize {
        self.records.len()
    }
}
