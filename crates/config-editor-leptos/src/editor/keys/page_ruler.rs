//! Where on a row the page keys find the on-screen column, as `ArrowDown`
//! does: by the pixels the text is drawn at. A count of columns cannot
//! find it: the host's font, or the machine's (its emoji and wide
//! characters), draws characters at widths of their own, and a
//! proportional font no two lines alike. So the line is drawn, unseen, in
//! the highlighted copy (in the textarea's font), and measured: not its
//! own row there, which a page away is not drawn yet (only the rows in
//! view are). Columns are counted only where nothing can be drawn.

use wasm_bindgen::JsCast;

use super::super::completion::{visual_column, zero_width};
use super::super::source_area::SourceArea;
use crate::page_scroll::scale_of;

/// Measures x on a line of text, in the text area's pixels.
pub(super) enum Ruler {
    /// The line drawn in the highlighted copy and measured.
    Drawn(Drawn),
    /// Columns of `width` pixels: tabs to their stops, marks taking none,
    /// every other character one. Only where nothing can be drawn (no
    /// highlighted copy, or no browser, as in unit tests): right in a
    /// monospace font without emoji or wide characters, near otherwise.
    Columns { width: f64 },
}

impl Ruler {
    /// The ruler for `area`, whose columns, when nothing can be drawn, are
    /// `char_width` pixels wide.
    pub(super) fn new(area: &SourceArea, char_width: f64) -> Self {
        Drawn::new(area).map_or(
            Self::Columns {
                width: char_width.max(f64::EPSILON),
            },
            Self::Drawn,
        )
    }

    /// The x a caret at UTF-16 offset `units` into `line` goes up and
    /// down for, from the line's start. A text area's caret goes for
    /// where it is drawn, in whole pixels of the window (rounded down),
    /// and lands, a row away, on the character boundary nearest that.
    pub(super) fn goal(&self, line: &str, units: usize) -> f64 {
        match self {
            Self::Drawn(drawn) => {
                drawn.show(line);
                drawn.snapped(drawn.x_at(units))
            }
            Self::Columns { width } => columns_x(line, units) * width,
        }
    }

    /// The UTF-16 offset into `line` whose x is nearest `goal`: between
    /// characters (never between one and the marks drawn over it), the
    /// later of two as near, as a text area's caret lands.
    pub(super) fn nearest(&self, line: &str, goal: f64) -> usize {
        match self {
            Self::Drawn(drawn) => {
                drawn.show(line);
                nearest(line, goal, |units| drawn.x_at(units))
            }
            Self::Columns { width } => nearest(line, goal, |units| columns_x(line, units) * width),
        }
    }
}

/// The on-screen column of UTF-16 offset `units` into `line`.
fn columns_x(line: &str, units: usize) -> f64 {
    let mut taken = 0;
    let mut end = line.len();
    for (at, character) in line.char_indices() {
        if taken >= units {
            end = at;
            break;
        }
        taken += character.len_utf16();
    }
    visual_column(&line[..end])
}

/// The UTF-16 offsets into `line` a caret can stand at: between
/// characters, never before a mark (it is drawn over the character
/// before it).
fn boundaries(line: &str) -> Vec<usize> {
    let mut at = vec![0];
    let mut units = 0;
    let mut characters = line.chars().peekable();
    while let Some(character) = characters.next() {
        units += character.len_utf16();
        if !characters.peek().copied().is_some_and(zero_width) {
            at.push(units);
        }
    }
    at
}

/// The boundary of `line` whose `x` is nearest `goal`, the later of two as
/// near. Halves the boundaries: x grows along a line (left-to-right text),
/// and a long line takes a few measures, not one per character.
fn nearest(line: &str, goal: f64, x: impl Fn(usize) -> f64) -> usize {
    let at = boundaries(line);
    let after = at.partition_point(|units| x(*units) < goal);
    match (after.checked_sub(1).map(|before| at[before]), at.get(after)) {
        (Some(before), Some(&next)) if goal - x(before) < x(next) - goal => before,
        (_, Some(&next)) => next,
        (before, None) => before.unwrap_or(0),
    }
}

/// A line drawn, unseen, in the highlighted copy of the text (whose font,
/// tab stops and spacing are the textarea's), for its x's to be measured;
/// taken out again when dropped.
pub(super) struct Drawn {
    span: web_sys::HtmlElement,
    text: web_sys::Text,
    range: web_sys::Range,
    /// The text area's scale on the screen, to measure in its own pixels.
    scale: f64,
}

impl Drawn {
    fn new(area: &SourceArea) -> Option<Self> {
        let element: &web_sys::Element = area.element();
        let copy = element
            .parent_element()?
            .query_selector(".config-editor__highlight")
            .ok()??;
        let document = element.owner_document()?;
        let span: web_sys::HtmlElement = document.create_element("span").ok()?.dyn_into().ok()?;
        span.set_attribute(
            "style",
            "position:absolute;left:0;top:0;visibility:hidden;white-space:pre",
        )
        .ok()?;
        let text = document.create_text_node("");
        span.append_child(&text).ok()?;
        copy.append_child(&span).ok()?;
        let range = document.create_range().ok()?;
        Some(Self {
            span,
            text,
            range,
            scale: scale_of(element).max(f64::EPSILON),
        })
    }

    /// Draws `line`, unless it is drawn already.
    fn show(&self, line: &str) {
        if self.text.data() != line {
            self.text.set_data(line);
        }
    }

    /// `x` on the line drawn as the window's whole pixel it falls in.
    fn snapped(&self, x: f64) -> f64 {
        let left = self.span.get_bounding_client_rect().left();
        ((left + x * self.scale).floor() - left) / self.scale
    }

    /// The x of UTF-16 offset `units` into the line drawn.
    fn x_at(&self, units: usize) -> f64 {
        if units == 0 {
            return 0.0;
        }
        let drawn = self.range.set_start(&self.text, 0).is_ok()
            && self.range.set_end(&self.text, units as u32).is_ok();
        if !drawn {
            return 0.0;
        }
        let right = self.range.get_bounding_client_rect().right();
        (right - self.span.get_bounding_client_rect().left()) / self.scale
    }
}

impl Drop for Drawn {
    fn drop(&mut self) {
        self.span.remove();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const COLUMNS: Ruler = Ruler::Columns { width: 1.0 };

    #[test]
    fn a_column_inside_a_tab_lands_on_its_nearer_side_never_inside_a_mark() {
        // Tabs at columns 0..2, 2..4: column 1 is as near either side.
        assert_eq!(COLUMNS.nearest("\t\tx", 1.0), 1);
        assert_eq!(COLUMNS.nearest("a\tx", 1.0), 1);
        // `é` as `e` and a combining mark: after both, or before both.
        assert_eq!(COLUMNS.nearest("e\u{301}x", 1.0), 2);
        assert_eq!(COLUMNS.nearest("e\u{301}x", 0.0), 0);
        assert_eq!(COLUMNS.nearest("e\u{301}x", 0.4), 0);
        // Past the end: the end.
        assert_eq!(COLUMNS.nearest("ab", 9.0), 2);
        assert_eq!(COLUMNS.nearest("", 9.0), 0);
        assert_eq!(COLUMNS.nearest("ab", -1.0), 0);
    }

    #[test]
    fn columns_count_tabs_to_their_stops_and_marks_as_none() {
        assert_eq!(COLUMNS.goal("a\tx", 2), 2.0);
        assert_eq!(COLUMNS.goal("e\u{301}x", 2), 1.0);
        assert_eq!(COLUMNS.goal("😀x", 2), visual_column("😀"));
        assert_eq!(Ruler::Columns { width: 7.5 }.goal("ab", 2), 15.0);
        assert_eq!(COLUMNS.goal("ab", 9), 2.0);
    }

    #[test]
    fn a_long_line_is_measured_a_few_times_not_once_a_character() {
        let line = "y".repeat(10_000);
        let measures = std::cell::Cell::new(0);
        let at = nearest(&line, 4321.0, |units| {
            measures.set(measures.get() + 1);
            units as f64
        });
        assert_eq!(at, 4321);
        assert!(measures.get() < 40, "{} measures", measures.get());
    }
}
