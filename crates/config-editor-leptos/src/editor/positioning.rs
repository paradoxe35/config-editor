use super::metrics::read_editor_metrics;
use super::source_area::SourceArea;
use super::state::EditorState;
use crate::reactive::later;
use leptos::prelude::*;
use wasm_bindgen::JsCast;

/// Hover delay in milliseconds, so moving across keys does not flash cards.
pub(super) const HOVER_DELAY_MS: i32 = 350;

/// Viewport edge margin for caret-anchored popups, in pixels.
pub(super) const POPUP_MARGIN: f64 = 8.0;

/// Gap between the anchored line and a caret-anchored popup, in pixels.
pub(super) const POPUP_GAP: f64 = 4.0;

/// Viewport size when `window.inner_width/height` is unavailable; keep in sync
/// with `diff` and `completion`.
pub(super) const FALLBACK_VIEWPORT_WIDTH: f64 = 1024.0;
pub(super) const FALLBACK_VIEWPORT_HEIGHT: f64 = 768.0;

/// Matches `max-width` on `.config-editor__hover` in `preset.css`.
pub(super) const HOVER_CARD_MAX_WIDTH: f64 = 340.0;

/// Card height assumed when deciding to flip above the line; matches the
/// editor `min-height` in `preset.css`.
pub(super) const HOVER_CARD_ESTIMATED_HEIGHT: f64 = 240.0;

/// Side of the square probe [`PopupFrame`] is measured from, in pixels.
const FRAME_PROBE_SIZE: f64 = 100.0;

/// Inline style of the probe [`PopupFrame`] is measured from: a fixed,
/// invisible square at the popups' origin (inline, so it holds without the
/// preset stylesheet too).
pub(super) const FRAME_PROBE_STYLE: &str =
    "position:fixed;left:0;top:0;width:100px;height:100px;visibility:hidden;pointer-events:none";

/// A box, in [`PopupFrame`] pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Bounds {
    pub(super) left: f64,
    pub(super) top: f64,
    pub(super) right: f64,
    pub(super) bottom: f64,
}

/// A probe's box as measured: `(left, top, width, height)` in window pixels.
pub(super) type ProbeBox = (f64, f64, f64, f64);

/// Where the editor's fixed popups (the problem tip, the hover card, the
/// suggestion list, the lock tooltip) count from, and at what size.
///
/// A host may put the editor in a transformed, filtered or zoomed container
/// (an animated dialog): `position: fixed` then counts from that container,
/// not the window, and its pixels are scaled with it. Measured from a fixed
/// probe square in the editor, the window's pixels are turned into the
/// editor's own (divided by the scale, the window's corner still at 0, 0),
/// where the editor's metrics apply and popups are kept inside the window;
/// [`PopupFrame::left`] and [`PopupFrame::top`] turn them into a fixed
/// popup's `left` and `top`. Without such a container it changes nothing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PopupFrame {
    /// Window pixels of the popups' origin (`left: 0; top: 0`).
    origin_x: f64,
    origin_y: f64,
    /// Window pixels per editor pixel.
    scale_x: f64,
    scale_y: f64,
    /// The window, in editor pixels.
    pub(super) viewport_width: f64,
    pub(super) viewport_height: f64,
}

impl PopupFrame {
    /// The frame for a probe at `probe` (`(left, top, width, height)` in
    /// window pixels, `None` without one) in a window `viewport` wide and
    /// tall.
    pub(super) fn new(probe: Option<ProbeBox>, viewport: (f64, f64)) -> Self {
        let (origin_x, origin_y, width, height) =
            probe.unwrap_or((0.0, 0.0, FRAME_PROBE_SIZE, FRAME_PROBE_SIZE));
        // A hidden editor measures nothing: no scale then.
        let scale = |size: f64| {
            if size > 0.0 && size.is_finite() {
                size / FRAME_PROBE_SIZE
            } else {
                1.0
            }
        };
        let (scale_x, scale_y) = (scale(width), scale(height));
        Self {
            origin_x,
            origin_y,
            scale_x,
            scale_y,
            viewport_width: viewport.0 / scale_x,
            viewport_height: viewport.1 / scale_y,
        }
    }

    /// The editor's frame now (measured from its probe).
    pub(super) fn measure(state: EditorState) -> Self {
        let window = web_sys::window();
        let size = |value: Option<Result<wasm_bindgen::JsValue, wasm_bindgen::JsValue>>,
                    fallback: f64| {
            value
                .and_then(Result::ok)
                .and_then(|value| value.as_f64())
                .unwrap_or(fallback)
        };
        let viewport = (
            size(
                window.as_ref().map(|w| w.inner_width()),
                FALLBACK_VIEWPORT_WIDTH,
            ),
            size(
                window.as_ref().map(|w| w.inner_height()),
                FALLBACK_VIEWPORT_HEIGHT,
            ),
        );
        let probe = state.view().frame_probe_ref.get_untracked().map(|probe| {
            let rect = probe.get_bounding_client_rect();
            (rect.left(), rect.top(), rect.width(), rect.height())
        });
        Self::new(probe, viewport)
    }

    /// Window x `x` in editor pixels.
    pub(super) fn x(&self, x: f64) -> f64 {
        x / self.scale_x
    }

    /// Window y `y` in editor pixels.
    pub(super) fn y(&self, y: f64) -> f64 {
        y / self.scale_y
    }

    /// A box measured in window pixels, in editor pixels.
    pub(super) fn bounds(&self, rect: &web_sys::DomRect) -> Bounds {
        Bounds {
            left: self.x(rect.left()),
            top: self.y(rect.top()),
            right: self.x(rect.right()),
            bottom: self.y(rect.bottom()),
        }
    }

    /// A fixed popup's `left` for editor x `x`.
    pub(super) fn left(&self, x: f64) -> f64 {
        x - self.x(self.origin_x)
    }

    /// A fixed popup's `top` for editor y `y`.
    pub(super) fn top(&self, y: f64) -> f64 {
        y - self.y(self.origin_y)
    }
}

/// Zero-based document line and column of the textarea cell under a
/// pointer position.
pub(super) fn pointer_text_position(element: &SourceArea, x: f64, y: f64) -> (usize, usize) {
    let target: &web_sys::Element = element.element().unchecked_ref();
    let frame = PopupFrame::measure(element.state());
    let rect = frame.bounds(&target.get_bounding_client_rect());
    let (x, y) = (frame.x(x), frame.y(y));
    let metrics = read_editor_metrics(target);
    let line =
        (y - rect.top - metrics.padding_top + element.scroll_top() as f64) / metrics.line_height;
    let column =
        (x - rect.left - metrics.padding_left + element.scroll_left() as f64) / metrics.char_width;
    (
        element.line_of_row(line.max(0.0).floor() as usize),
        column.max(0.0).floor() as usize,
    )
}

/// Fixed-position style for the hover card; flips above the line when space
/// below is short.
pub(super) fn hover_card_style(element: &SourceArea, line: usize, column: usize) -> String {
    let target: &web_sys::Element = element.element().unchecked_ref();
    let line = element.row_of_line(line);
    let frame = PopupFrame::measure(element.state());
    let rect = frame.bounds(&target.get_bounding_client_rect());
    let metrics = read_editor_metrics(target);
    let x = rect.left + metrics.padding_left + column as f64 * metrics.char_width
        - element.scroll_left() as f64;
    let line_top = rect.top + metrics.padding_top + line as f64 * metrics.line_height
        - element.scroll_top() as f64;
    hover_card_geometry(&frame, rect, x, line_top, metrics.line_height)
}

/// The hover card's style for text at editor `x` on a line at `line_top`
/// (`line_height` tall) in the text area at `rect`: below the line, or
/// above when the room below is short, inside the window.
pub(super) fn hover_card_geometry(
    frame: &PopupFrame,
    rect: Bounds,
    x: f64,
    line_top: f64,
    line_height: f64,
) -> String {
    let (viewport_width, viewport_height) = (frame.viewport_width, frame.viewport_height);
    // At most the window's width, so narrow screens do not clip the text.
    let width = HOVER_CARD_MAX_WIDTH.min(viewport_width - 2.0 * POPUP_MARGIN);
    let left = x.clamp(
        POPUP_MARGIN,
        (viewport_width - width - POPUP_MARGIN).max(POPUP_MARGIN),
    );
    // Room inside the visible part of the editor, not the whole window.
    let room_below = rect.bottom.min(viewport_height) - (line_top + line_height);
    let room_above = line_top - rect.top.max(0.0);
    let left = frame.left(left);
    if room_below < HOVER_CARD_ESTIMATED_HEIGHT && room_above > room_below {
        format!(
            "left:{left:.1}px;top:{:.1}px;max-width:{width:.1}px;transform:translateY(calc(-100% - {POPUP_GAP}px))",
            frame.top(line_top)
        )
    } else {
        format!(
            "left:{left:.1}px;top:{:.1}px;max-width:{width:.1}px",
            frame.top(line_top + line_height + POPUP_GAP)
        )
    }
}

/// Places the caret and scrolls it into view, mirroring the scroll into
/// `scroll_y` for the highlight overlay.
///
/// `set_selection_range` alone does not reliably scroll the textarea.
pub(super) fn set_caret_visible(
    editor: &SourceArea,
    text: &str,
    offset: usize,
    scroll_y: RwSignal<f64>,
) {
    let offset = offset.min(text.encode_utf16().count());
    let _ = editor.set_selection_range(offset as u32, offset as u32);
    reveal_offset(editor, text, offset, scroll_y);
}

/// Scrolls UTF-16 `offset` into view without touching the selection.
pub(super) fn reveal_offset(
    editor: &SourceArea,
    text: &str,
    offset: usize,
    scroll_y: RwSignal<f64>,
) {
    use super::keyboard::utf16_to_byte_index;

    let offset = offset.min(text.encode_utf16().count());
    let byte_offset = utf16_to_byte_index(text, offset);
    let line = text[..byte_offset]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count();
    let line = editor.row_of_line(line) as f64;
    let target: &web_sys::Element = editor.element().unchecked_ref();
    let metrics = read_editor_metrics(target);
    let style =
        web_sys::window().and_then(|window| window.get_computed_style(target).ok().flatten());
    let padding_bottom = style
        .as_ref()
        .and_then(|style| style.get_property_value("padding-bottom").ok())
        .and_then(|value| {
            value
                .trim()
                .strip_suffix("px")
                .unwrap_or(&value)
                .parse::<f64>()
                .ok()
        })
        .unwrap_or(0.0);
    let current_top = editor.scroll_top() as f64;
    let client_height = editor.client_height() as f64;
    let caret_top = metrics.padding_top + line * metrics.line_height;
    let caret_bottom = caret_top + metrics.line_height;
    let visible_top = current_top + metrics.padding_top;
    let visible_bottom = current_top + client_height - padding_bottom;
    let next_top = if caret_bottom > visible_bottom {
        caret_bottom - client_height + padding_bottom
    } else if caret_top < visible_top {
        caret_top - metrics.padding_top
    } else {
        current_top
    }
    .clamp(
        0.0,
        (editor.scroll_height() as f64 - client_height).max(0.0),
    );
    editor.set_scroll_top(next_top as i32);
    scroll_y.set(next_top);
    // Horizontally too: a caret back at the start of a line after a long one
    // must not stay scrolled off to the left.
    let line_start = text[..byte_offset].rfind('\n').map_or(0, |index| index + 1);
    let caret_x =
        super::completion::visual_column(&text[line_start..byte_offset]) * metrics.char_width;
    let current_left = editor.scroll_left() as f64;
    let client_width = editor.client_width() as f64 - metrics.padding_left;
    if caret_x < current_left {
        editor.set_scroll_left((caret_x - 4.0 * metrics.char_width).max(0.0) as i32);
    } else if caret_x > current_left + client_width - 2.0 * metrics.char_width {
        editor.set_scroll_left((caret_x - client_width + 4.0 * metrics.char_width) as i32);
    }
}

/// Runs `callback` once the pointer has rested, if the editor is still
/// mounted.
pub(super) fn schedule_hover(state: EditorState, callback: impl FnOnce(EditorState) + 'static) {
    later::after(state, HOVER_DELAY_MS, callback);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn popup_geometry_constants_match_the_preset_stylesheet() {
        assert_eq!(POPUP_MARGIN, 8.0);
        assert_eq!(POPUP_GAP, 4.0);
        assert_eq!(FALLBACK_VIEWPORT_WIDTH, 1024.0);
        assert_eq!(FALLBACK_VIEWPORT_HEIGHT, 768.0);
        assert_eq!(HOVER_CARD_MAX_WIDTH, 340.0);
        assert_eq!(HOVER_CARD_ESTIMATED_HEIGHT, 240.0);
    }

    /// The probe of each container a host may put the editor in (as the
    /// browser measures it in a 1100 × 700 window).
    fn containers() -> Vec<(&'static str, ProbeBox)> {
        vec![
            ("none", (0.0, 0.0, 100.0, 100.0)),
            ("translate + margin", (88.0, 60.0, 100.0, 100.0)),
            ("scale 0.9", (48.0, 40.0, 90.0, 90.0)),
            ("zoom 1.5", (0.0, 0.0, 150.0, 150.0)),
            ("filter + margin", (108.0, 8.0, 100.0, 100.0)),
        ]
    }

    /// QA 20: in a transformed, filtered or zoomed container the popups
    /// landed 80 to 150 px off. A point measured in the window comes back to
    /// the same point once placed, in every container.
    #[test]
    fn a_popup_lands_on_its_window_point_in_every_container() {
        let mut failures = Vec::new();
        for (name, probe) in containers() {
            let frame = PopupFrame::new(Some(probe), (1100.0, 700.0));
            let (left, top) = (frame.left(frame.x(500.0)), frame.top(frame.y(300.0)));
            // Where the browser draws `left`/`top` in this container.
            let drawn = (
                probe.0 + left * probe.2 / 100.0,
                probe.1 + top * probe.3 / 100.0,
            );
            if (drawn.0 - 500.0).abs() > 0.01 || (drawn.1 - 300.0).abs() > 0.01 {
                failures.push(format!("{name}: drawn at {drawn:?}"));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    /// The hover card near the window's right and bottom edges stays inside
    /// the window, drawn in every container.
    #[test]
    fn the_hover_card_stays_inside_the_window_in_every_container() {
        let mut failures = Vec::new();
        for (name, probe) in containers() {
            let frame = PopupFrame::new(Some(probe), (1100.0, 700.0));
            // A text area filling the window; text at its right edge.
            let rect = Bounds {
                left: 0.0,
                top: 0.0,
                right: frame.viewport_width,
                bottom: frame.viewport_height,
            };
            let style = hover_card_geometry(&frame, rect, frame.viewport_width - 10.0, 20.0, 20.0);
            let number = |key: &str| -> f64 {
                let from = style.find(key).unwrap() + key.len();
                style[from..].split("px").next().unwrap().parse().unwrap()
            };
            let (left, width) = (number("left:"), number("max-width:"));
            let drawn_left = probe.0 + left * probe.2 / 100.0;
            let drawn_right = drawn_left + width * probe.2 / 100.0;
            if drawn_left < 0.0 || drawn_right > 1100.0 {
                failures.push(format!("{name}: {style} drawn {drawn_left}..{drawn_right}"));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    /// A hidden editor (its probe measured empty) scales nothing.
    #[test]
    fn a_hidden_editor_scales_nothing() {
        let frame = PopupFrame::new(Some((5.0, 5.0, 0.0, 0.0)), (800.0, 600.0));
        assert_eq!(
            (frame.viewport_width, frame.viewport_height),
            (800.0, 600.0)
        );
        assert_eq!(frame.x(42.0), 42.0);
    }
}
