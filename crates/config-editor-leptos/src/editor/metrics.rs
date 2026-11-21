//! The editor's typography as the browser computes it: the column, the
//! line and the paddings overlays and popups are placed by, and how wide
//! text is drawn ([`TextWidths`]).

mod widths;

pub(super) use widths::{TextWidths, zero_width};

/// Font size in pixels when computed style is unavailable; matches
/// `--config-editor-font-size` in `css/base.css`.
pub(super) const FALLBACK_FONT_SIZE: f64 = 12.0;

/// Line-height multiplier fallback; matches the `font` shorthand in `css/base.css`.
pub(super) const FALLBACK_LINE_HEIGHT_FACTOR: f64 = 1.8;

#[derive(Clone, Copy, Debug)]
pub(super) struct EditorMetrics {
    pub(super) char_width: f64,
    pub(super) line_height: f64,
    pub(super) padding_top: f64,
    pub(super) padding_left: f64,
    /// How wide text is drawn, in `char_width` columns.
    pub(super) widths: TextWidths,
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
            widths: TextWidths::COUNTED,
        }
    }
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
    let text = |name: &str| {
        style
            .as_ref()
            .and_then(|style| style.get_property_value(name).ok())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "normal".to_string())
    };
    // `normal` reads as no number: no spacing.
    let letter_spacing = number("letter-spacing", 0.0);
    let (widths, char_width) = family
        .map(|family| {
            format!(
                "{} {} {font_size}px {family}",
                text("font-style"),
                text("font-weight")
            )
        })
        .and_then(|font| TextWidths::measured(&font, letter_spacing))
        .unwrap_or((TextWidths::COUNTED, font_size * CHAR_WIDTH_FACTOR));
    EditorMetrics {
        char_width,
        line_height,
        padding_top: number("padding-top", 0.0),
        padding_left: number("padding-left", 0.0),
        widths,
    }
}
