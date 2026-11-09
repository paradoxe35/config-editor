//! The values a document writes as expressions (`var.z`, `"${var.x}-y"`,
//! a heredoc holding `${…}`): read as their `${…}` text, which no other
//! language tells from the text a literal `"$${x}"` reads as, so a
//! conversion lists them as losses.

use hcl_edit::expr::Expression;
use serde_json::Value;

use super::read::MARK;

/// The JSON pointer of each value `source` writes as an expression.
pub(super) fn expression_pointers(source: &str) -> Vec<String> {
    let text = source.strip_prefix('\u{FEFF}').unwrap_or(source);
    let Ok(value) = super::read::read(text, true) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    marked(&value, &mut String::new(), &mut found);
    found
}

fn marked(value: &Value, pointer: &mut String, found: &mut Vec<String>) {
    match value {
        Value::String(text) if text == MARK => found.push(pointer.clone()),
        Value::Object(map) => {
            for (key, child) in map {
                let length = pointer.len();
                pointer.push('/');
                pointer.push_str(&key.replace('~', "~0").replace('/', "~1"));
                marked(child, pointer, found);
                pointer.truncate(length);
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                let length = pointer.len();
                pointer.push_str(&format!("/{index}"));
                marked(item, pointer, found);
                pointer.truncate(length);
            }
        }
        _ => {}
    }
}

/// Whether `written` (a value as the host writes it) is an expression that
/// reads as text: a reference, call, operation, condition or `for` (not a
/// literal, a template, or a list or object of values).
pub(super) fn is_expression(written: &str) -> bool {
    let Ok(body) = format!("v = {}\n", written.trim()).parse::<hcl_edit::structure::Body>() else {
        return false;
    };
    let Some(attribute) = body.get_attribute("v") else {
        return false;
    };
    !matches!(
        attribute.value,
        Expression::Null(_)
            | Expression::Bool(_)
            | Expression::Number(_)
            | Expression::String(_)
            | Expression::StringTemplate(_)
            | Expression::HeredocTemplate(_)
            | Expression::Array(_)
            | Expression::Object(_)
    )
}

/// The written form of the expression a `${…}` text stands for (`${var.azs}`
/// is `var.azs`, `${file("${var.dir}/x")}` is `file("${var.dir}/x")`),
/// when it reads back as that text.
pub(super) fn expression_text(text: &str) -> Option<&str> {
    let inner = text.strip_prefix("${")?.strip_suffix('}')?.trim();
    let read = hcl::from_str::<Value>(&format!("v = {inner}\n")).ok()?;
    (read.get("v")?.as_str()? == text && is_expression(inner)).then_some(inner)
}

/// The expression `written` (as the host writes it) edited to read as
/// `text`: the edit the view made to its `${…}` text applied to the host's
/// own spelling (its spaces kept: `{ for … }` where the view shows
/// `${{for …}}`), else the expression as the view spells it. `None` when
/// `written` is no expression or `text` is none.
pub(super) fn rewritten(written: &str, text: &str) -> Option<String> {
    let written = written.trim();
    if !is_expression(written) {
        return None;
    }
    let new = expression_text(text)?;
    let read = hcl::from_str::<Value>(&format!("v = {written}\n")).ok()?;
    let Some(old) = read.get("v").and_then(Value::as_str).and_then(expression_text) else {
        return Some(new.to_string());
    };
    spliced(written, old, new)
        .filter(|spliced| hcl::from_str::<Value>(&format!("v = {spliced}\n")).ok().and_then(|read| read.get("v").cloned()) == Some(Value::from(text)))
        .or_else(|| Some(new.to_string()))
}

/// `written` with the change from `old` to `new` made in it, where `written`
/// and `old` differ only in their spaces.
fn spliced(written: &str, old: &str, new: &str) -> Option<String> {
    let solid = |text: &str| text.chars().filter(|character| !character.is_whitespace()).collect::<String>();
    if solid(written) != solid(old) {
        return None;
    }
    // The common start and end of `old` and `new`, each ending (starting)
    // at a character that is no space.
    let prefix = old.chars().zip(new.chars()).take_while(|(a, b)| a == b).map(|(a, _)| a.len_utf8()).sum::<usize>();
    let prefix = old[..prefix].trim_end().len();
    let room = old.len().min(new.len()) - prefix;
    let suffix = old[prefix..]
        .chars()
        .rev()
        .zip(new[prefix..].chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(a, _)| a.len_utf8())
        .sum::<usize>()
        .min(room);
    let suffix = old[old.len() - suffix..].trim_start().len();
    // The same characters in `written`, counted without spaces.
    let (before, after) = (solid(&old[..prefix]).chars().count(), solid(&old[old.len() - suffix..]).chars().count());
    let solid_at = |count: usize| -> usize {
        written
            .char_indices()
            .filter(|(_, character)| !character.is_whitespace())
            .nth(count)
            .map_or(written.len(), |(at, _)| at)
    };
    let start = if before == 0 { 0 } else { solid_at(before - 1) + written[solid_at(before - 1)..].chars().next()?.len_utf8() };
    let total = solid(written).chars().count();
    let end = if after == 0 { written.len() } else { solid_at(total - after) };
    if start > end {
        return None;
    }
    Some(format!("{}{}{}", &written[..start], &new[prefix..new.len() - suffix], &written[end..]))
}

/// The first value `previous` writes as an expression (or template) that
/// `patched` writes as a literal string with a template sequence in its
/// text, or that `previous` writes as a literal string and `patched` as an
/// expression: they read the same, so only the written form tells them
/// apart. A literal written over an expression with no `${` (`"text"` over
/// `var.x`) is the user's literal.
pub(super) fn changed_expression(previous: &str, patched: &str) -> Option<String> {
    let before = expression_pointers(previous);
    let after = expression_pointers(patched);
    if before.is_empty() && after.is_empty() {
        return None;
    }
    let (Ok(old), Ok(new)) = (super::hcl_parse(previous), super::hcl_parse(patched)) else {
        return None;
    };
    let string = |value: &Value, pointer: &str| value.pointer(pointer).and_then(Value::as_str).map(str::to_string);
    let to_literal = before.iter().find(|pointer| {
        !after.contains(pointer)
            && string(&old, pointer).is_some()
            && string(&new, pointer).is_some_and(|text| text.contains("${") || text.contains("%{"))
    });
    let to_expression = || after.iter().find(|pointer| !before.contains(pointer) && string(&old, pointer).is_some());
    to_literal.or_else(to_expression).cloned()
}

