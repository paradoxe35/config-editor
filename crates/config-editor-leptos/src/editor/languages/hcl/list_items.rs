//! Which lines start an item of a list written as lines of its own: a
//! labelled block (an entry of the map of its labels), or a block whose
//! type another line at its indent repeats (`s {` twice).

/// Whether line `index` of `lines` starts such an item.
pub(super) fn opens_list_item(
    comment_start: &dyn Fn(&str) -> usize,
    lines: &[&str],
    index: usize,
) -> bool {
    let code = |line: &str| line[..comment_start(line)].trim_end().to_string();
    let indent = |line: &str| line.len() - line.trim_start().len();
    let line = code(lines[index]);
    let at = indent(&line);
    let rest = line.trim_start();
    if rest.starts_with("[[") {
        return true;
    }
    let name: String = rest
        .chars()
        .take_while(|character| character.is_alphanumeric() || matches!(character, '_' | '-'))
        .collect();
    let after = rest[name.len()..].trim_start();
    if name.is_empty() {
        return false;
    }
    if after.starts_with('"') && line.ends_with('{') {
        return true;
    }
    if !after.starts_with('{') {
        return false;
    }
    let same_head = |other: &str| {
        let other = code(other);
        indent(&other) == at
            && other
                .trim_start()
                .strip_prefix(name.as_str())
                .is_some_and(|after| after.trim_start().starts_with('{'))
    };
    lines
        .iter()
        .enumerate()
        .any(|(other, line)| other != index && same_head(line))
}
