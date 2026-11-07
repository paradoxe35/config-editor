use std::cell::RefCell;
use std::collections::HashMap;
use wasm_bindgen::JsCast;

/// Font size in pixels when computed style is unavailable; matches
/// `--config-editor-font-size` in `preset.css`.
pub(super) const FALLBACK_FONT_SIZE: f64 = 12.0;

/// Line-height multiplier fallback; matches the `font` shorthand in `preset.css`.
pub(super) const FALLBACK_LINE_HEIGHT_FACTOR: f64 = 1.8;

#[derive(Clone, Copy, Debug)]
pub(super) struct EditorMetrics {
    pub(super) char_width: f64,
    pub(super) line_height: f64,
    pub(super) padding_top: f64,
    pub(super) padding_left: f64,
}

/// Monospace column width in `em`, used when canvas measurement is unavailable.
const CHAR_WIDTH_FACTOR: f64 = 0.6;

impl EditorMetrics {
    #[cfg(all(test, feature = "diff"))]
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
    /// Measured column widths per canvas font string, so mousemove does not
    /// re-measure.
    static CHAR_WIDTHS: RefCell<HashMap<String, f64>> = RefCell::new(HashMap::new());
}

/// Measures one monospace column with a canvas so host font overrides are
/// honored. Returns `None` outside a browser.
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

/// Reads typography from computed CSS so popups stay aligned when a host
/// overrides the font-size or line-height variables.
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
