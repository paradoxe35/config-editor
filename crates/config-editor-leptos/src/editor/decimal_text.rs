//! Numbers written as decimal text (`2.50`, `.5`, `2e3`, `007`, `+6`):
//! how a value written as text (XML's) reads where a number is wanted.

use serde_json::Value;

/// The number `text` writes in decimal (`2.50`, `.5`, `2e3`, `007`, `+6`):
/// an integer when `integer` (the host holds one) and it has no fraction or
/// exponent, else a finite float.
pub(crate) fn decimal(text: &str, integer: bool) -> Option<Value> {
    let digits = text.strip_prefix(['+', '-']).unwrap_or(text);
    let (mantissa, exponent) = match digits.find(['e', 'E']) {
        Some(at) => (&digits[..at], Some(&digits[at + 1..])),
        None => (digits, None),
    };
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let all_digits = |part: &str| part.bytes().all(|byte| byte.is_ascii_digit());
    let exponent_ok = exponent.is_none_or(|exponent| {
        let exponent = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
        !exponent.is_empty() && all_digits(exponent)
    });
    if whole.len() + fraction.len() == 0
        || !all_digits(whole)
        || !all_digits(fraction)
        || !exponent_ok
    {
        return None;
    }
    if integer && exponent.is_none() && !mantissa.contains('.') {
        let text = text.strip_prefix('+').unwrap_or(text);
        if let Ok(value) = text.parse::<i64>() {
            return Some(Value::from(value));
        }
        if let Ok(value) = text.parse::<u64>() {
            return Some(Value::from(value));
        }
    }
    let float: f64 = text.parse().ok()?;
    serde_json::Number::from_f64(float).map(Value::Number)
}
