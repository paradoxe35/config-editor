#[derive(Clone, Copy, Debug)]
pub(super) struct EditorMetrics {
    pub(super) font_size: f64,
    pub(super) line_height: f64,
    pub(super) padding_top: f64,
    pub(super) padding_left: f64,
}

impl EditorMetrics {
    /// Fallback metrics when `window.get_computed_style` is unavailable (for
    /// example in unit tests). Matches the `number()` fallbacks below.
    #[cfg(test)]
    pub(super) fn fallback() -> Self {
        Self {
            font_size: 12.0,
            line_height: 12.0 * 1.8,
            padding_top: 0.0,
            padding_left: 0.0,
        }
    }
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
    let font_size = number("font-size", 12.0);
    let line_height = number("line-height", font_size * 1.8);
    EditorMetrics {
        font_size,
        line_height,
        padding_top: number("padding-top", 0.0),
        padding_left: number("padding-left", 0.0),
    }
}
