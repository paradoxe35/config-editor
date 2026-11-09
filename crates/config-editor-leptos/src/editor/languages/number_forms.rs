//! A number written in the form of the one it replaces (`0x1F90` edited to
//! 8083 is `0x1F93`, `1_000_000` to 1000003 is `1_000_003`, `1.5e3` to
//! 1500.3 is `1.5003e3`, `1.50` to 1.8 is `1.80`): the building block of
//! [`ConfigLanguage::number_in_form`](super::super::language::ConfigLanguage::number_in_form).
//! Each language says which forms it writes; the result is a candidate the
//! language checks reads back as the value.
#![cfg_attr(
    not(all(feature = "yaml", feature = "json", feature = "toml", feature = "hcl")),
    allow(dead_code, reason = "not every language writes each form")
)]

use serde_json::Number;

/// The number forms a language writes.
pub(super) struct NumberSyntax {
    /// Integer prefixes and their radix (`0x` hexadecimal).
    pub(super) radixes: &'static [(&'static str, u32)],
    /// The digit separator (`_`), if any.
    pub(super) separator: Option<char>,
}

/// Only the decimal forms (exponent, fraction digits), no separator.
pub(super) const DECIMAL: NumberSyntax = NumberSyntax {
    radixes: &[],
    separator: None,
};

/// [`in_form`] when `reads` (the language reading a scalar) reads it back
/// as `value`.
pub(super) fn checked(
    written: &str,
    value: &Number,
    syntax: &NumberSyntax,
    reads: impl Fn(&str, &serde_json::Value) -> bool,
) -> Option<String> {
    in_form(written, value, syntax)
        .filter(|text| reads(text, &serde_json::Value::Number(value.clone())))
}

/// `value` written as `written` is: its radix, digit grouping, exponent
/// and fraction digits; `None` when `written` has no such form or `value`
/// does not fit it (a fraction where a hexadecimal integer was).
pub(super) fn in_form(written: &str, value: &Number, syntax: &NumberSyntax) -> Option<String> {
    let written = written.trim();
    let (old_sign, body) = split_sign(written);
    let new = canonical(value)?;
    let sign = if new.negative {
        "-"
    } else if old_sign == "+" {
        "+"
    } else {
        ""
    };
    if let Some((prefix, radix)) = syntax.radixes.iter().find(|(prefix, _)| {
        body.len() > prefix.len() && body[..prefix.len()].eq_ignore_ascii_case(prefix)
    }) {
        let digits = &body[prefix.len()..];
        return Some(format!(
            "{sign}{}{}",
            &body[..prefix.len()],
            radix_digits(digits, &new, *radix, syntax.separator)?
        ));
    }
    let old = Decimal::parse(body, syntax.separator)?;
    let plain = old.exponent.is_none() && old.fraction.is_none() && old.grouping.is_none();
    if plain {
        return None;
    }
    let (exponent_text, shift) = match &old.exponent {
        Some(exponent) => (Some(exponent), exponent.value),
        None => (None, 0),
    };
    // The new value's digits times 10^(scale - shift), as the mantissa.
    let scale = new.scale - shift;
    let (mut whole, mut fraction) = if scale >= 0 {
        (
            format!("{}{}", new.digits, "0".repeat(scale as usize)),
            String::new(),
        )
    } else {
        let places = scale.unsigned_abs() as usize;
        let padded = format!(
            "{}{}",
            "0".repeat((places + 1).saturating_sub(new.digits.len())),
            new.digits
        );
        let (whole, fraction) = padded.split_at(padded.len() - places);
        (
            whole.to_string(),
            fraction.trim_end_matches('0').to_string(),
        )
    };
    if let Some(places) = old.fraction
        && fraction.len() < places
    {
        fraction.push_str(&"0".repeat(places - fraction.len()));
    }
    whole = whole.trim_start_matches('0').to_string();
    if whole.is_empty() {
        whole.push('0');
    }
    if let (Some(size), Some(separator)) = (old.grouping, syntax.separator) {
        whole = grouped(&whole, size, separator);
    }
    let mut out = format!("{sign}{whole}");
    if !fraction.is_empty() {
        out.push('.');
        out.push_str(&fraction);
    }
    if let Some(exponent) = exponent_text {
        out.push_str(&exponent.marker);
        if exponent.value < 0 {
            out.push('-');
        } else {
            out.push_str(&exponent.plus);
        }
        out.push_str(&format!(
            "{:0width$}",
            exponent.value.unsigned_abs(),
            width = exponent.width
        ));
    }
    Some(out)
}

fn split_sign(text: &str) -> (&str, &str) {
    match text.as_bytes().first() {
        Some(b'+' | b'-') => text.split_at(1),
        _ => ("", text),
    }
}

/// A value as its decimal digits (no leading or trailing zeros) times a
/// power of ten.
struct Canonical {
    negative: bool,
    digits: String,
    scale: i32,
    integer: Option<u128>,
}

fn canonical(value: &Number) -> Option<Canonical> {
    let text = value.to_string();
    let (sign, body) = split_sign(&text);
    let (mantissa, exponent) = match body.find(['e', 'E']) {
        Some(at) => (&body[..at], body[at + 1..].parse::<i32>().ok()?),
        None => (body, 0),
    };
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let all = format!("{whole}{fraction}");
    if !all.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let mut scale = exponent - fraction.len() as i32;
    let trimmed = all.trim_start_matches('0');
    let mut digits = trimmed.trim_end_matches('0').to_string();
    scale += (trimmed.len() - digits.len()) as i32;
    if digits.is_empty() {
        digits.push('0');
        scale = 0;
    }
    let integer = (value.is_i64() || value.is_u64())
        .then(|| body.parse::<u128>().ok())
        .flatten();
    Some(Canonical {
        negative: sign == "-" && digits != "0",
        digits,
        scale,
        integer,
    })
}

fn radix_digits(old: &str, new: &Canonical, radix: u32, separator: Option<char>) -> Option<String> {
    let magnitude = new.integer?;
    let bare: String = old
        .chars()
        .filter(|character| Some(*character) != separator)
        .collect();
    if bare.is_empty() || !bare.chars().all(|character| character.is_digit(radix)) {
        return None;
    }
    let mut digits = to_radix(magnitude, radix);
    if bare.chars().any(|character| character.is_ascii_uppercase()) {
        digits = digits.to_ascii_uppercase();
    }
    // Zero padding (`0x00FF`, `0o0755`) kept to its width.
    if bare.starts_with('0') && digits.len() < bare.len() {
        digits = format!("{}{digits}", "0".repeat(bare.len() - digits.len()));
    }
    match (separator, group_size(old, separator)) {
        (Some(separator), Some(Some(size))) => Some(grouped(&digits, size, separator)),
        (_, Some(None)) => None,
        _ => Some(digits),
    }
}

fn to_radix(mut value: u128, radix: u32) -> String {
    if value == 0 {
        return "0".to_string();
    }
    let mut digits = Vec::new();
    while value > 0 {
        digits
            .push(std::char::from_digit((value % u128::from(radix)) as u32, radix).unwrap_or('0'));
        value /= u128::from(radix);
    }
    digits.iter().rev().collect()
}

/// The size of the groups `digits` is written in: `None` when it has no
/// separator, `Some(None)` when its groups differ in size (no form to
/// keep).
fn group_size(digits: &str, separator: Option<char>) -> Option<Option<usize>> {
    let separator = separator?;
    if !digits.contains(separator) {
        return None;
    }
    let groups: Vec<&str> = digits.split(separator).collect();
    let size = groups.last()?.len();
    let uniform = size > 0
        && groups[1..].iter().all(|group| group.len() == size)
        && (1..=size).contains(&groups[0].len());
    Some(uniform.then_some(size))
}

fn grouped(digits: &str, size: usize, separator: char) -> String {
    let mut out = String::new();
    for (index, character) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(size) {
            out.push(separator);
        }
        out.push(character);
    }
    out
}

/// A decimal as written: its whole part's grouping, its fraction's
/// places and its exponent.
struct Decimal {
    grouping: Option<usize>,
    fraction: Option<usize>,
    exponent: Option<Exponent>,
}

struct Exponent {
    marker: String,
    plus: String,
    value: i32,
    width: usize,
}

impl Decimal {
    fn parse(text: &str, separator: Option<char>) -> Option<Self> {
        let (mantissa, exponent) = match text.find(['e', 'E']) {
            Some(at) => {
                let rest = &text[at + 1..];
                let (sign, digits) = split_sign(rest);
                if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
                    return None;
                }
                let magnitude: i32 = digits.parse().ok()?;
                let exponent = Exponent {
                    marker: text[at..at + 1].to_string(),
                    plus: if sign == "+" {
                        "+".to_string()
                    } else {
                        String::new()
                    },
                    value: if sign == "-" { -magnitude } else { magnitude },
                    width: if digits.starts_with('0') {
                        digits.len()
                    } else {
                        1
                    },
                };
                (&text[..at], Some(exponent))
            }
            None => (text, None),
        };
        let (whole, fraction) = match mantissa.split_once('.') {
            Some((whole, fraction)) => (whole, Some(fraction)),
            None => (mantissa, None),
        };
        let digit = |character: char| character.is_ascii_digit() || Some(character) == separator;
        if whole.is_empty()
            || !whole.chars().all(digit)
            || !fraction
                .unwrap_or("")
                .bytes()
                .all(|byte| byte.is_ascii_digit())
        {
            return None;
        }
        let grouping = match group_size(whole, separator) {
            None => None,
            Some(Some(size)) => Some(size),
            Some(None) => return None,
        };
        Some(Decimal {
            grouping,
            fraction: fraction.map(str::len),
            exponent,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SYNTAX: NumberSyntax = NumberSyntax {
        radixes: &[("0x", 16), ("0o", 8), ("0b", 2)],
        separator: Some('_'),
    };

    fn written(old: &str, new: &str) -> Option<String> {
        let value: serde_json::Value = serde_json::from_str(new).unwrap();
        in_form(old, value.as_number().unwrap(), &SYNTAX)
    }

    #[test]
    fn each_form_is_kept_where_the_new_value_fits_it() {
        let cases: [(&str, &str, Option<&str>); 19] = [
            ("0x1F90", "8083", Some("0x1F93")),
            ("0x1f90", "8083", Some("0x1f93")),
            ("0x00FF", "16", Some("0x0010")),
            ("0o755", "420", Some("0o644")),
            ("0b1010", "5", Some("0b101")),
            ("0xFF_FF", "65534", Some("0xFF_FE")),
            ("0x1F90", "8083.5", None),
            ("1_000_000", "1000003", Some("1_000_003")),
            ("1_000_000", "999", Some("999")),
            ("10_00_000", "1", None),
            ("1.5e3", "1500.3", Some("1.5003e3")),
            ("1E+3", "1000.3", Some("1.0003E+3")),
            ("2.5e-3", "0.004", Some("4.0e-3")),
            ("1.50", "1.8", Some("1.80")),
            ("1.50", "1.875", Some("1.875")),
            ("-0.50", "0.25", Some("0.25")),
            ("+1.50", "2", Some("+2.00")),
            ("1.0", "-3", Some("-3.0")),
            ("8080", "8083", None),
        ];
        let failures: Vec<String> = cases
            .iter()
            .filter_map(|(old, new, expected)| {
                let got = written(old, new);
                (got.as_deref() != *expected)
                    .then(|| format!("{old} -> {new}: {got:?}, expected {expected:?}"))
            })
            .collect();
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}
