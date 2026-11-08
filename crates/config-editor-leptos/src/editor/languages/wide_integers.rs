//! Integers past 64 bits, which a parser without big integers reads as a
//! float (silently not the number written): a syntax error, as YAML and
//! TOML make it.

use serde_json::Value;

/// The pointer of the first number of `value` read as a float past the
/// 64-bit integers that `written` (the text at a pointer) shows as an
/// integer literal, with that text.
pub(super) fn first_past_64_bits(
    value: &Value,
    written: &dyn Fn(&str) -> Option<String>,
) -> Option<(String, String)> {
    let mut wide = Vec::new();
    floats_past_64_bits(value, String::new(), &mut wide);
    wide.into_iter().find_map(|pointer| {
        let text = written(&pointer)?;
        let digits = text.trim().strip_prefix('-').unwrap_or(text.trim());
        (!digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit()))
            .then(|| (pointer, text.trim().to_string()))
    })
}

/// The message of the syntax error.
pub(super) fn message(written: &str) -> String {
    format!("the integer {written} does not fit in 64 bits")
}

fn floats_past_64_bits(value: &Value, pointer: String, found: &mut Vec<String>) {
    match value {
        Value::Number(number) if number.is_f64() => {
            if number
                .as_f64()
                .is_some_and(|float| float.fract() == 0.0 && float.abs() >= 9.2e18)
            {
                found.push(pointer);
            }
        }
        Value::Object(map) => {
            for (key, child) in map {
                let key = key.replace('~', "~0").replace('/', "~1");
                floats_past_64_bits(child, format!("{pointer}/{key}"), found);
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                floats_past_64_bits(item, format!("{pointer}/{index}"), found);
            }
        }
        _ => {}
    }
}
