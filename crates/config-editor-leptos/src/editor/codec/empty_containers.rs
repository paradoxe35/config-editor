//! Empty containers the host wrote over lines (`block {` then `}`) seen as
//! one line by the structural patch.
//!
//! The serializer writes an empty block or object on one line (`block {}`),
//! which matches no host line, so a key added to it failed the line patch
//! and the whole host was rewritten. Each one is joined onto its opening
//! line for the patch, then split back where the patch left it as it was.

use super::super::language::ConfigLanguage;
use super::comment_start;

/// The host with its empty multi-line containers joined, and each joined
/// line with the lines it stands for.
pub(super) struct Joined {
    pub host: String,
    lines: Vec<(String, String)>,
}

impl Joined {
    /// `patched` with each joined line the patch kept written over its
    /// lines again, in order.
    pub fn split(&self, patched: &str) -> String {
        let mut lines: Vec<String> = patched.split('\n').map(str::to_string).collect();
        let mut from = 0;
        for (joined, original) in &self.lines {
            if let Some(at) = (from..lines.len()).find(|index| lines[*index] == *joined) {
                lines[at] = original.clone();
                from = at + 1;
            }
        }
        lines.join("\n")
    }
}

/// `previous` with each line opening a container (`… {`, `… [`) followed by
/// blank lines and its closer alone on a line joined into one; `None` when
/// it has none.
pub(super) fn join(language: &dyn ConfigLanguage, previous: &str) -> Option<Joined> {
    let host: Vec<&str> = previous.split('\n').collect();
    let code = |line: &str| line[..comment_start(language, line)].trim_end().to_string();
    let brackets = language.config().brackets;
    let separator = language.config().item_separator;
    let mut out: Vec<String> = Vec::with_capacity(host.len());
    let mut lines = Vec::new();
    let mut index = 0;
    while index < host.len() {
        let opener = code(host[index]);
        let close: String = opener
            .chars()
            .last()
            .and_then(|last| {
                brackets
                    .iter()
                    .find_map(|(open, close)| (*open == last).then_some(*close))
            })
            .map(String::from)
            .unwrap_or_default();
        let mut end = index + 1;
        while end < host.len() && host[end].trim().is_empty() {
            end += 1;
        }
        let closer = host.get(end).map(|line| line.trim());
        if close.is_empty()
            || closer.is_none_or(|closer| {
                separator.map_or(closer, |separator| closer.trim_end_matches(separator)) != close
            })
            || opener.len() != host[index].trim_end().len()
        {
            out.push(host[index].to_string());
            index += 1;
            continue;
        }
        let closer = closer.unwrap_or_default();
        let joined = format!("{opener}{closer}");
        lines.push((joined.clone(), host[index..=end].join("\n")));
        out.push(joined);
        index = end + 1;
    }
    (!lines.is_empty()).then(|| Joined {
        host: out.join("\n"),
        lines,
    })
}
