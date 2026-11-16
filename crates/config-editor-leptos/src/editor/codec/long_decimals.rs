//! Decimals written with more digits than a double holds
//! (`123456789.123456789`): the data model reads the nearest double, which
//! every target writes with its own shortest digits
//! (`123456789.12345679`). The value reads back the same, but the digits
//! written are gone, so a conversion lists each one as a loss.

use crate::editor::paths::FieldPath;
use serde_json::Value;

use super::super::language::{ConfigLanguage, pointer_key};
use super::scalar_range;

/// The JSON pointers of the numbers in `value` (read from `source`) that
/// `source` writes as a decimal its double does not spell back.
pub(super) fn rounded(language: &dyn ConfigLanguage, source: &str, value: &Value) -> Vec<String> {
    let mut floats = Vec::new();
    floats_in(value, String::new(), &mut floats);
    if floats.is_empty() {
        return Vec::new();
    }
    let paths: Vec<FieldPath> = floats
        .iter()
        .map(|(pointer, _)| FieldPath::from(pointer))
        .collect();
    let spans = language.value_spans(source, &paths);
    floats
        .into_iter()
        .zip(spans)
        .filter_map(|((pointer, float), span)| {
            let range = scalar_range(language, source, span?)?;
            let written = digits(source[range].trim())?;
            (Some(written) != digits(&format!("{float:e}"))).then_some(pointer)
        })
        .collect()
}

/// The numbers `value` holds as doubles (not as integers), by pointer.
fn floats_in(value: &Value, at: String, out: &mut Vec<(String, f64)>) {
    match value {
        Value::Number(number) if !number.is_i64() && !number.is_u64() => {
            if let Some(float) = number.as_f64() {
                out.push((at, float));
            }
        }
        Value::Object(map) => {
            for (key, value) in map {
                floats_in(value, format!("{at}/{}", pointer_key(key)), out);
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                floats_in(item, format!("{at}/{index}"), out);
            }
        }
        _ => {}
    }
}

/// The decimal `text` writes, as its sign, significant digits and the
/// power of ten of the last one (`1.50` and `15e-1` are `(false, "15",
/// -1)`); `None` for text that is no plain decimal (hex, `inf`, quoted).
/// Digits may be grouped by `_` (`1_000.5`).
fn digits(text: &str) -> Option<(bool, String, i64)> {
    let text = text.replace('_', "");
    let (negative, body) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(&text)),
    };
    let (mantissa, exponent) = match body.find(['e', 'E']) {
        Some(at) => (&body[..at], body[at + 1..].parse::<i64>().ok()?),
        None => (body, 0),
    };
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    if whole.is_empty() && fraction.is_empty()
        || !whole
            .bytes()
            .chain(fraction.bytes())
            .all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let all = format!("{whole}{fraction}");
    let significant = all.trim_start_matches('0');
    if significant.is_empty() {
        return Some((false, String::new(), 0));
    }
    let trimmed = significant.trim_end_matches('0');
    let power = exponent - fraction.len() as i64 + (significant.len() - trimmed.len()) as i64;
    Some((negative, trimmed.to_string(), power))
}

#[cfg(test)]
mod tests {
    use super::digits;

    #[test]
    fn a_decimal_reads_as_its_significant_digits() {
        assert_eq!(digits("1.50"), digits("15e-1"));
        assert_eq!(digits("1_000.0"), digits("1e3"));
        assert_eq!(digits("-0.0"), digits("0"));
        assert_eq!(digits("+2.5"), Some((false, "25".into(), -1)));
        assert_ne!(
            digits("123456789.123456789"),
            digits("1.2345678912345679e8")
        );
        assert_eq!(digits("0x10"), None);
        assert_eq!(digits(".inf"), None);
        assert_eq!(digits("\"1.5\""), None);
    }
}
