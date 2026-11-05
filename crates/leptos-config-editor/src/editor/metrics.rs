use std::cell::RefCell;
use std::collections::HashMap;
use wasm_bindgen::JsCast;

/// Fallback type size, in pixels, when `get_computed_style` is unavailable.
/// Matches the `--yaml-editor-font-size` default in `preset.css` so unit tests
/// and non-browser renders use the same baseline as the real editor.
pub(super) const FALLBACK_FONT_SIZE: f64 = 12.0;

/// Fallback line-height multiplier. Matches the `1.8` in the editor
/// `font: var(--yaml-editor-font-size)/1.8` shorthand in `preset.css`.
pub(super) const FALLBACK_LINE_HEIGHT_FACTOR: f64 = 1.8;

#[derive(Clone, Copy, Debug)]
pub(super) struct EditorMetrics {
    pub(super) char_width: f64,
    pub(super) line_height: f64,
    pub(super) padding_top: f64,
    pub(super) padding_left: f64,
}

/// Fallback advance of one monospace column relative to the font size. Used
/// only when canvas measurement is unavailable; the textarea uses a monospace
/// stack so `0.6em` tracks real glyph advances closely enough to stay usable.
const CHAR_WIDTH_FACTOR: f64 = 0.6;

impl EditorMetrics {
    /// Fallback metrics when `window.get_computed_style` is unavailable (for
    /// example in unit tests). Matches the `number()` fallbacks below.
    #[cfg(test)]
    pub(super) fn fallback() -> Self {
        Self {
            char_width: FALLBACK_FONT_SIZE * CHAR_WIDTH_FACTOR,
            line_height: FALLBACK_FONT_SIZE * FALLBACK_LINE_HEIGHT_FACTOR,
            padding_top: 0.0,
            padding_left: 0.0,
        }
    }
}

thread_local! {
    /// Canvas-measured `0`-glyph advances keyed by canvas font string, so
    /// pointer mapping pays for measurement once per font instead of once
    /// per mousemove.
    static CHAR_WIDTHS: RefCell<HashMap<String, f64>> = RefCell::new(HashMap::new());
}

/// Measures the advance of one monospace column with a canvas context, which
/// tracks the real font (including host overrides of the mono stack) instead
/// of assuming `0.6em`. Returns `None` outside a browser.
fn measured_char_width(font: &str) -> Option<f64> {
    if let Some(width) = CHAR_WIDTHS.with(|cache| cache.borrow().get(font).copied()) {
        return Some(width);
    }
    let width = (|| {
        let document = web_sys::window()?.document()?;
        let canvas: web_sys::HtmlCanvasElement =
            document.create_element("canvas").ok()?.dyn_into().ok()?;
        let context: web_sys::CanvasRenderingContext2d =
            canvas.get_context("2d").ok()??.dyn_into().ok()?;
        context.set_font(font);
        let width = context.measure_text("0000000000").ok()?.width() / 10.0;
        (width > 0.0).then_some(width)
    })();
    if let Some(width) = width {
        CHAR_WIDTHS.with(|cache| {
            cache.borrow_mut().insert(font.to_string(), width);
        });
    }
    width
}

/// Reads the editor's typography from CSS instead of hard-coding pixel values.
/// This keeps caret-anchored popups aligned when a host overrides the public
/// `--yaml-editor-font-size` or `--yaml-editor-line-height` variables.
pub(super) fn read_editor_metrics(element: &web_sys::Element) -> EditorMetrics {
    let style =
        web_sys::window().and_then(|window| window.get_computed_style(element).ok().flatten());
    let number = |name: &str, fallback: f64| {
        style
            .as_ref()
            .and_then(|style| style.get_property_value(name).ok())
            .and_then(|value| {
                let value = value.trim();
                value
                    .strip_suffix("px")
                    .unwrap_or(value)
                    .parse::<f64>()
                    .ok()
            })
            .unwrap_or(fallback)
    };
    let font_size = number("font-size", FALLBACK_FONT_SIZE);
    let line_height = number("line-height", font_size * FALLBACK_LINE_HEIGHT_FACTOR);
    let family = style
        .as_ref()
        .and_then(|style| style.get_property_value("font-family").ok())
        .map(|family| family.trim().to_string())
        .filter(|family| !family.is_empty());
    let char_width = family
        .map(|family| format!("{font_size}px {family}"))
        .and_then(|font| measured_char_width(&font))
        .unwrap_or(font_size * CHAR_WIDTH_FACTOR);
    EditorMetrics {
        char_width,
        line_height,
        padding_top: number("padding-top", 0.0),
        padding_left: number("padding-left", 0.0),
    }
}
