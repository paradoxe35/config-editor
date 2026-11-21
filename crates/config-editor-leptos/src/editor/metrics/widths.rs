//! How wide the editor draws text: the one place where a place on a line
//! becomes an x and an x becomes a place on a line. Every overlay drawn
//! over the text (problem underlines, fold chips, the problem tip, hover
//! cards, the suggestion list) and the pointer's place in the text ask
//! here, so they agree with each other and with the text the browser
//! draws.
//!
//! A character is not one column: a CJK character or an emoji is drawn
//! about 1.7 columns wide in a monospace font (the browser takes it from
//! another font, and which one depends on the characters around it), and
//! a host may set a font of its own. So, as VS Code does, widths are
//! measured in the text area's font. A line of plain ASCII (and tabs)
//! costs no layout: its characters' widths are measured once per font on
//! a canvas. Other text is drawn, unseen, in a probe in that font and
//! measured there, as the browser lays it out (its widths kept per text,
//! a canvas cannot tell them: a Han character drawn after `é` comes out
//! wider than alone). Tabs go to their stops as the browser puts them.
//! Without a browser (unit tests) each character is one column and marks
//! drawn over a character none.

use std::cell::RefCell;
use std::collections::HashMap;

use unicode_segmentation::UnicodeSegmentation;
use wasm_bindgen::JsCast;

/// Tab width in columns; matches `tab-size` in `css/base.css`.
const TAB_SIZE: f64 = 2.0;

/// Most texts kept measured per font; past it they are measured again (a
/// document of many lines must not grow the cache without bound).
const MAX_KEPT: usize = 4_096;

/// How the editor draws text, for widths in columns (`ch`, the width of
/// `0` in its font, the unit the overlays are placed in).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::editor) struct TextWidths {
    /// The measured font, or `None` to count (no browser).
    font: Option<FontId>,
}

/// A measured font: its index in [`FONTS`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FontId(usize);

/// A font's measured widths, in columns.
struct Font {
    /// The canvas `font` string, with the letter spacing it is drawn with.
    key: (String, u64),
    /// One column (the width of `0`), in pixels.
    ch: f64,
    /// Each ASCII character's width.
    ascii: [f64; 128],
    /// The width of each text not plain ASCII, as drawn in the probe.
    drawn: HashMap<String, f64>,
}

thread_local! {
    static FONTS: RefCell<Vec<Font>> = const { RefCell::new(Vec::new()) };
    static CANVAS: RefCell<Option<Canvas>> = const { RefCell::new(None) };
    static PROBE: RefCell<Option<Probe>> = const { RefCell::new(None) };
}

/// The canvas widths are measured on, and the font it is set to.
struct Canvas {
    context: web_sys::CanvasRenderingContext2d,
    font: String,
}

impl TextWidths {
    /// Counted widths: each character one column, a tab to its stop, a
    /// mark drawn over the character before it none.
    pub(in crate::editor) const COUNTED: Self = Self { font: None };

    /// The widths of text drawn in `font` (a canvas font string), with
    /// `letter_spacing` pixels after each character, and the width of one
    /// column there, in pixels; `None` when nothing can measure.
    pub(super) fn measured(font: &str, letter_spacing: f64) -> Option<(Self, f64)> {
        let key = (font.to_string(), letter_spacing.to_bits());
        let found = FONTS.with(|fonts| {
            let fonts = fonts.borrow();
            let index = fonts.iter().position(|known| known.key == key)?;
            Some((index, fonts[index].ch))
        });
        let (index, ch) = match found {
            Some(found) => found,
            None => {
                let font = Font::new(key)?;
                let ch = font.ch;
                let index = FONTS.with(|fonts| {
                    let mut fonts = fonts.borrow_mut();
                    fonts.push(font);
                    fonts.len() - 1
                });
                (index, ch)
            }
        };
        Some((
            Self {
                font: Some(FontId(index)),
            },
            ch,
        ))
    }

    /// How many columns `text` (the start of a line) is drawn wide.
    pub(in crate::editor) fn columns(self, text: &str) -> f64 {
        if let Some(font) = self.font.filter(|_| !text.is_ascii())
            && let Some(width) = FONTS.with(|fonts| fonts.borrow_mut().get_mut(font.0)?.drawn(text))
        {
            return width;
        }
        let mut end = 0.0;
        self.walk(text, |_, _, to| {
            end = to;
            false
        });
        end
    }

    /// The zero-based character of `line` drawn at `x` columns from its
    /// start (the first of a cluster drawn as one); past its end, one
    /// more per column.
    pub(in crate::editor) fn column_at(self, line: &str, x: f64) -> usize {
        let mut found = None;
        let mut count = 0;
        let mut end = 0.0;
        self.walk(line, |characters, from, to| {
            if to > from && x < to {
                found = Some(count);
                return true;
            }
            count += characters;
            end = to;
            false
        });
        found.unwrap_or_else(|| {
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "whole columns past a line's end, at least zero"
            )]
            let past = (x - end).max(0.0).floor() as usize;
            count + past
        })
    }

    /// Calls `piece(characters, from, to)` for each piece of `text` drawn
    /// as one, in order: how many characters it is and the columns it is
    /// drawn between; stops when it answers `true`.
    fn walk(self, text: &str, mut piece: impl FnMut(usize, f64, f64) -> bool) {
        let mut x = 0.0;
        let Some(font) = self.font else {
            for character in text.chars() {
                let to = advance(x, character);
                if piece(1, x, to) {
                    return;
                }
                x = to;
            }
            return;
        };
        let drawn = !text.is_ascii();
        FONTS.with(|fonts| {
            let fonts = fonts.borrow();
            let Some(font) = fonts.get(font.0) else {
                return;
            };
            if drawn
                && let Some(()) = Probe::with(font, text, |probe| {
                    let ch = font.ch;
                    let mut units = 0;
                    for cluster in text.graphemes(true) {
                        units += cluster.encode_utf16().count();
                        let to = probe.x_at(units) / ch;
                        if piece(cluster.chars().count(), x, to) {
                            return;
                        }
                        x = to;
                    }
                })
            {
                return;
            }
            for cluster in text.graphemes(true) {
                let to = match cluster.as_bytes() {
                    [b'\t'] => tab_stop(x, font.ascii[usize::from(b' ')]),
                    [byte] if byte.is_ascii() => x + font.ascii[usize::from(*byte)],
                    _ => cluster.chars().fold(x, advance),
                };
                if piece(cluster.chars().count(), x, to) {
                    return;
                }
                x = to;
            }
        });
    }
}

impl Font {
    /// The font `key` measured: one column (the width of `0`) and the
    /// ASCII characters; `None` when nothing can measure.
    fn new(key: (String, u64)) -> Option<Self> {
        let name = &key.0;
        // Ten in a row, as the browser's `ch` is one: in a monospace font
        // every ASCII character then comes out exactly one column.
        let ten = |character: char| measure(name, &character.to_string().repeat(10));
        let ch = ten('0')? / 10.0;
        if ch <= 0.0 || !ch.is_finite() {
            return None;
        }
        let letter_spacing = f64::from_bits(key.1) / ch;
        let mut ascii = [1.0; 128];
        for (code, width) in (0u8..).zip(ascii.iter_mut()) {
            let character = char::from(code);
            if !character.is_ascii_control() {
                *width = ten(character).map_or(1.0, |ten| ten / 10.0 / ch) + letter_spacing;
            }
        }
        Some(Self {
            key,
            ch,
            ascii,
            drawn: HashMap::new(),
        })
    }

    /// The width of `text` (not plain ASCII) as drawn in the probe, in
    /// columns, measured once; `None` without a page to draw it in.
    fn drawn(&mut self, text: &str) -> Option<f64> {
        if let Some(width) = self.drawn.get(text) {
            return Some(*width);
        }
        let width = Probe::with(self, text, |probe| probe.width())? / self.ch;
        if self.drawn.len() >= MAX_KEPT {
            self.drawn.clear();
        }
        self.drawn.insert(text.to_string(), width);
        Some(width)
    }
}

/// Text drawn, unseen, in a font, to be measured as the browser lays it
/// out: a span in a box of no size, fixed at the window's corner and
/// contained, so drawing in it lays out nothing else of the page.
struct Probe {
    span: web_sys::HtmlElement,
    text: web_sys::Text,
    range: web_sys::Range,
    /// The font it draws in now.
    font: (String, u64),
}

impl Probe {
    /// Calls `measure` with `text` drawn in `font`; `None` without a page.
    fn with<T>(font: &Font, text: &str, measure: impl FnOnce(&Probe) -> T) -> Option<T> {
        PROBE.with(|probe| {
            let mut probe = probe.borrow_mut();
            if probe.is_none() {
                *probe = Probe::new();
            }
            let probe = probe.as_mut()?;
            if !probe.span.is_connected() {
                *probe = Probe::new()?;
            }
            if probe.font != font.key {
                let style = probe.span.style();
                style.set_property("font", &font.key.0).ok()?;
                let spacing = format!("{}px", f64::from_bits(font.key.1));
                style.set_property("letter-spacing", &spacing).ok()?;
                probe.font = font.key.clone();
            }
            if probe.text.data() != text {
                probe.text.set_data(text);
            }
            Some(measure(probe))
        })
    }

    fn new() -> Option<Self> {
        let document = web_sys::window()?.document()?;
        let body = document.body()?;
        let element = |tag: &str| -> Option<web_sys::HtmlElement> {
            document.create_element(tag).ok()?.dyn_into().ok()
        };
        let (frame, span) = (element("div")?, element("span")?);
        let set = |element: &web_sys::HtmlElement, rules: &[(&str, &str)]| {
            let style = element.style();
            rules
                .iter()
                .all(|(name, value)| style.set_property(name, value).is_ok())
        };
        let placed = set(
            &frame,
            &[
                ("position", "fixed"),
                ("left", "0"),
                ("top", "0"),
                ("width", "0"),
                ("height", "0"),
                ("overflow", "hidden"),
                ("visibility", "hidden"),
                ("pointer-events", "none"),
                ("contain", "strict"),
            ],
        ) && set(
            &span,
            &[
                ("position", "absolute"),
                ("left", "0"),
                ("top", "0"),
                ("white-space", "pre"),
                ("tab-size", "2"),
            ],
        );
        if !placed {
            return None;
        }
        frame.set_attribute("aria-hidden", "true").ok()?;
        let text = document.create_text_node("");
        span.append_child(&text).ok()?;
        frame.append_child(&span).ok()?;
        body.append_child(&frame).ok()?;
        Some(Self {
            span,
            text,
            range: document.create_range().ok()?,
            font: (String::new(), 0),
        })
    }

    /// The width of the text drawn, in pixels.
    fn width(&self) -> f64 {
        self.span.get_bounding_client_rect().width()
    }

    /// The x of UTF-16 offset `units` into the text drawn, in pixels.
    fn x_at(&self, units: usize) -> f64 {
        let Ok(end) = u32::try_from(units) else {
            return self.width();
        };
        let set = self.range.set_start(&self.text, 0).is_ok()
            && self.range.set_end(&self.text, end).is_ok();
        if !set {
            return self.width();
        }
        // The range's width: its right edge is unreliable where the text
        // runs right to left.
        self.range.get_bounding_client_rect().width()
    }
}

/// The width of `text` in `font`, in pixels, measured on the canvas.
fn measure(font: &str, text: &str) -> Option<f64> {
    CANVAS.with(|canvas| {
        let mut canvas = canvas.borrow_mut();
        if canvas.is_none() {
            *canvas = new_canvas();
        }
        let canvas = canvas.as_mut()?;
        if canvas.font != font {
            canvas.context.set_font(font);
            canvas.font = font.to_string();
        }
        canvas
            .context
            .measure_text(text)
            .ok()
            .map(|metrics| metrics.width())
    })
}

/// A canvas to measure on; `None` outside a browser.
fn new_canvas() -> Option<Canvas> {
    let document = web_sys::window()?.document()?;
    let canvas: web_sys::HtmlCanvasElement =
        document.create_element("canvas").ok()?.dyn_into().ok()?;
    let context: web_sys::CanvasRenderingContext2d =
        canvas.get_context("2d").ok()??.dyn_into().ok()?;
    Some(Canvas {
        context,
        font: String::new(),
    })
}

/// Where a tab drawn at `x` ends, `space` columns being a space's width:
/// at the next stop, or the one after when the next is under half a space
/// away, as the browser draws it.
fn tab_stop(x: f64, space: f64) -> f64 {
    let stop = TAB_SIZE * space;
    if stop <= 0.0 {
        return x;
    }
    let mut to = x + stop - x % stop;
    if to - x < space / 2.0 {
        to += stop;
    }
    to
}

/// The counted column after `character` drawn at `column`: a tab to its
/// stop, a mark drawn over the character before it none, else one.
fn advance(column: f64, character: char) -> f64 {
    if character == '\t' {
        tab_stop(column, 1.0)
    } else if zero_width(character) {
        column
    } else {
        column + 1.0
    }
}

/// Whether `character` is drawn without a width of its own: a combining
/// mark, a variation selector, or a zero-width joiner or space.
pub(in crate::editor) fn zero_width(character: char) -> bool {
    matches!(
        u32::from(character),
        0x0300..=0x036F
            | 0x0483..=0x0489
            | 0x0591..=0x05BD
            | 0x0610..=0x061A
            | 0x064B..=0x065F
            | 0x0E31
            | 0x0E34..=0x0E3A
            | 0x0E47..=0x0E4E
            | 0x1AB0..=0x1AFF
            | 0x1DC0..=0x1DFF
            | 0x200B..=0x200F
            | 0x20D0..=0x20FF
            | 0xFE00..=0xFE0F
            | 0xFE20..=0xFE2F
            | 0xE0100..=0xE01EF
    )
}

#[cfg(test)]
mod tests {
    use super::{TextWidths, tab_stop};

    const COUNTED: TextWidths = TextWidths::COUNTED;

    /// An underline after `é` written as `e` + U+0301 sat one column to the
    /// right: the mark takes no column of its own, in every overlay that
    /// places itself by column (problems, locks, fold chips).
    #[test]
    fn marks_over_a_character_take_no_column() {
        let cases = [
            ("e\u{301}x", 2.0),
            ("caf\u{e9}", 4.0),
            ("cafe\u{301}", 4.0),
            ("a\u{20dd}b", 2.0),
            ("\u{2764}\u{fe0f}", 1.0),
            ("a\u{200d}b", 2.0),
            ("\tx", 3.0),
        ];
        let failures: Vec<String> = cases
            .iter()
            .filter(|(text, expected)| COUNTED.columns(text) != *expected)
            .map(|(text, expected)| format!("{text:?}: {} not {expected}", COUNTED.columns(text)))
            .collect();
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    /// A place found from an x is the character drawn there: a tab is two
    /// columns, a joined accent none; past the end, one per column.
    #[test]
    fn an_x_lands_on_the_character_drawn_there() {
        assert_eq!(COUNTED.column_at("abc", 0.0), 0);
        assert_eq!(COUNTED.column_at("abc", 2.7), 2);
        assert_eq!(COUNTED.column_at("abc", 3.0), 3);
        assert_eq!(COUNTED.column_at("abc", 5.2), 5);
        assert_eq!(COUNTED.column_at("\tab", 1.0), 0);
        assert_eq!(COUNTED.column_at("\tab", 2.0), 1);
        assert_eq!(COUNTED.column_at("e\u{301}x", 1.0), 2);
        // A tab before a secret placeholder: on its opening mark.
        assert_eq!(COUNTED.column_at("a:\t\u{ab}secret\u{bb}", 4.0), 3);
    }

    /// A tab goes to the next stop, or the one after when the next is
    /// under half a space away (after a wide character), as the browser
    /// draws it.
    #[test]
    fn a_tab_goes_to_its_stop_as_the_browser_draws_it() {
        assert_eq!(tab_stop(0.0, 1.0), 2.0);
        assert_eq!(tab_stop(1.0, 1.0), 2.0);
        assert_eq!(tab_stop(2.0, 1.0), 4.0);
        assert_eq!(tab_stop(1.7, 1.0), 4.0);
        assert_eq!(tab_stop(1.4, 1.0), 2.0);
    }
}
