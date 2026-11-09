//! The history carried over a change the host made (a new controlled
//! `value`): each step's text gets the host's change too, so undo takes
//! back only the user's own edits and keeps the host's, as VS Code does
//! after a file changed outside the editor. A step whose edit overlaps the host's
//! change cannot be carried over: it and every older one go.

use super::super::sync::map_through_change;

/// The bytes `a` and `b` share at their start and, after that, at their
/// end (on character boundaries).
fn common(a: &str, b: &str) -> (usize, usize) {
    let prefix = a
        .char_indices()
        .zip(b.chars())
        .find(|((_, left), right)| left != right)
        .map_or(a.len().min(b.len()), |((at, _), _)| at);
    let (a_rest, b_rest) = (&a[prefix..], &b[prefix..]);
    let suffix = a_rest
        .char_indices()
        .rev()
        .zip(b_rest.chars().rev())
        .take_while(|((_, left), right)| left == right)
        .last()
        .map_or(0, |((at, _), _)| a_rest.len() - at);
    (prefix, suffix)
}

/// The host's change: `old` (the text it replaced) read as `new`.
pub(super) struct HostChange<'a> {
    old: &'a str,
    new: &'a str,
    /// The bytes before and after the region the host changed.
    prefix: usize,
    suffix: usize,
}

impl<'a> HostChange<'a> {
    pub(super) fn new(old: &'a str, new: &'a str) -> Self {
        let (prefix, suffix) = common(old, new);
        HostChange {
            old,
            new,
            prefix,
            suffix,
        }
    }

    /// `text` (an earlier state of `old`) with the host's change made, when
    /// what turns it into `old` lies apart from what the host changed.
    pub(super) fn applied(&self, text: &str) -> Option<String> {
        let (old, new) = (self.old, self.new);
        let (prefix, suffix) = common(text, old);
        // In `old`, the user's region is `prefix..old.len() - suffix`.
        if old.len() - suffix <= self.prefix {
            // Before the host's region: `text` ends with all of it.
            let keep = text.len() - (old.len() - self.prefix);
            return Some(format!("{}{}", &text[..keep], &new[self.prefix..]));
        }
        if prefix >= old.len() - self.suffix {
            // After it: `text` starts with all of it.
            let from = old.len() - self.suffix;
            return Some(format!(
                "{}{}",
                &new[..new.len() - self.suffix],
                &text[from..]
            ));
        }
        None
    }
}

/// UTF-16 `offset` in `text` where it lands in `rebased` (the host's
/// change made to it).
pub(super) fn mapped(text: &str, rebased: &str, offset: usize) -> usize {
    map_through_change(text, rebased, offset)
}

#[cfg(test)]
mod tests {
    use super::HostChange;

    #[test]
    fn a_state_takes_the_hosts_change_apart_from_its_own() {
        let change = HostChange::new("a = \"xZ\"\nb = 2\n", "a = \"xZ\"\nb = 3\n");
        assert_eq!(
            change.applied("a = \"x\"\nb = 2\n").as_deref(),
            Some("a = \"x\"\nb = 3\n")
        );
        let change = HostChange::new("a = 1\nb = \"xZ\"\n", "a = 9\nb = \"xZ\"\n");
        assert_eq!(
            change.applied("a = 1\nb = \"x\"\n").as_deref(),
            Some("a = 9\nb = \"x\"\n")
        );
        // Over the user's own edit: not carried over.
        let change = HostChange::new("a = \"xZ\"\n", "a = \"host\"\n");
        assert_eq!(change.applied("a = \"x\"\n"), None);
        assert_eq!(
            HostChange::new("é1", "é2").applied("é1").as_deref(),
            Some("é2")
        );
    }
}
