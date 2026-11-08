//! Which host lines start an item of a list (written as lines of its own),
//! for the comments at a file's top: above a list's first item they are
//! the item's, above a key the file's header.

use super::super::language::{ConfigLanguage, Repetition};
use super::compare::comment_start;

/// Whether line `index` of `host` starts a list item: a YAML `- ` item, a
/// TOML `[[array]]` table, or an XML element or HCL unlabeled block whose
/// name another line at its indent repeats (`<s …>` twice, `s {` twice).
pub(super) fn opens_list_item(language: &dyn ConfigLanguage, host: &[&str], index: usize) -> bool {
    let code = |line: &str| line[..comment_start(language, line)].trim_end().to_string();
    let indent = |line: &str| line.len() - line.trim_start().len();
    let line = code(host[index]);
    let at = indent(&line);
    let rest = line.trim_start();
    if (language.indent_is_structure() && (rest == "-" || rest.starts_with("- ")))
        || rest.starts_with("[[")
    {
        return true;
    }
    // The item's head: `<s` with what ends its name, or `s {`.
    let head = match language.repetition() {
        Repetition::Elements if rest.starts_with('<') => {
            let name_end = rest[1..]
                .find(|character: char| character.is_whitespace() || matches!(character, '>' | '/'))
                .map_or(rest.len(), |end| end + 1);
            (name_end > 1
                && !rest.starts_with("</")
                && !rest.starts_with("<!")
                && !rest.starts_with("<?"))
            .then(|| rest[..name_end].to_string())
        }
        Repetition::Blocks => {
            let name: String = rest
                .chars()
                .take_while(|character| {
                    character.is_alphanumeric() || matches!(character, '_' | '-')
                })
                .collect();
            let after = rest[name.len()..].trim_start();
            (!name.is_empty() && after.starts_with('{')).then(|| format!("{name} {{"))
        }
        _ => None,
    };
    let Some(head) = head else {
        return false;
    };
    let same_head = |other: &str| {
        let other = code(other);
        if indent(&other) != at {
            return false;
        }
        let other = other.trim_start();
        match language.repetition() {
            Repetition::Blocks => {
                let name = &head[..head.len() - 2];
                other
                    .strip_prefix(name)
                    .is_some_and(|after| after.trim_start().starts_with('{'))
            }
            _ => other.strip_prefix(head.as_str()).is_some_and(|after| {
                after.is_empty()
                    || after.starts_with(|character: char| {
                        character.is_whitespace() || matches!(character, '>' | '/')
                    })
            }),
        }
    };
    host.iter()
        .enumerate()
        .any(|(other, line)| other != index && same_head(line))
}
