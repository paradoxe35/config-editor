//! How a host spells its start tags: a space before `/>` (`<add k="1" />`,
//! as many configs write it) or none, and its attributes' quotes (`"` or
//! `'`). A rewrite spells each element as the host spells that element
//! (or, for one it does not hold, as the host spells most), so a save
//! that adds or removes an item changes only that item's lines.

use super::*;

/// One element's spelling, where the host shows it.
#[derive(Clone, Copy, Default)]
struct Spelling {
    /// A space before `/>`, from a self-closing tag.
    spaced_close: Option<bool>,
    /// The attribute quote, from a tag holding an attribute.
    quote: Option<char>,
}

/// Counts for the host's most common spelling.
#[derive(Default)]
struct Tally {
    spaced: usize,
    unspaced: usize,
    single: usize,
    double: usize,
}

impl Tally {
    fn most(&self) -> Spelling {
        Spelling {
            spaced_close: (self.spaced + self.unspaced > 0).then_some(self.spaced > self.unspaced),
            quote: (self.single + self.double > 0).then_some(if self.single > self.double { '\'' } else { '"' }),
        }
    }
}

/// The attribute quotes written in start tag `tag` (`<a k="1" j='2'>`).
fn quotes(tag: &str) -> Vec<char> {
    let mut found = Vec::new();
    let mut quote: Option<char> = None;
    let mut after_equals = false;
    for character in tag.chars() {
        match (quote, character) {
            (Some(open), _) if character == open => quote = None,
            (Some(_), _) => {}
            (None, '=') => after_equals = true,
            (None, '"' | '\'') if after_equals => {
                quote = Some(character);
                found.push(character);
                after_equals = false;
            }
            (None, character) if !character.is_whitespace() => after_equals = false,
            _ => {}
        }
    }
    found
}

/// Whether self-closing tag `tag` has a space before its `/>`.
fn spaced_close(tag: &str) -> bool {
    tag.strip_suffix("/>").is_some_and(|before| before.ends_with(char::is_whitespace))
}

fn unindexed(path: &[Seg]) -> Vec<String> {
    strings(path).into_iter().filter(|segment| crate::editor::language::pointer_index(segment).is_none()).collect()
}

/// Each element path's spelling in `source`, and its most common one.
fn spellings(source: &str) -> (HashMap<Vec<String>, Spelling>, Spelling) {
    let tokens = tokenize(source);
    let plan = plan_paths(source, &tokens);
    let (mut paths, mut tally) = (HashMap::<Vec<String>, Spelling>::new(), Tally::default());
    for token in &tokens {
        let Kind::OpenTag { closing: false, .. } = token.kind else { continue };
        let tag = &source[token.start..token.end];
        let Some(path) = plan.path_of(token.start) else { continue };
        let spelling = paths.entry(unindexed(path)).or_default();
        if tag.ends_with("/>") {
            let spaced = spaced_close(tag);
            spelling.spaced_close.get_or_insert(spaced);
            if spaced { tally.spaced += 1 } else { tally.unspaced += 1 }
        }
        for quote in quotes(tag) {
            spelling.quote.get_or_insert(quote);
            if quote == '\'' { tally.single += 1 } else { tally.double += 1 }
        }
    }
    (paths, tally.most())
}

/// `tag` (as the writer writes it: `"` quotes, no space before `/>`)
/// spelled as `spelling` says.
fn respelled(tag: &str, spelling: Spelling) -> String {
    let mut out = String::with_capacity(tag.len() + 1);
    if spelling.quote == Some('\'') {
        // Each `"…"` value requoted: its `&quot;` is a `"` now, a `'` in it
        // the reference.
        let mut rest = tag;
        while let Some(open) = rest.find("=\"") {
            out.push_str(&rest[..open]);
            let value_start = open + 2;
            let Some(length) = rest[value_start..].find('"') else {
                break;
            };
            let value = &rest[value_start..value_start + length];
            out.push_str("='");
            out.push_str(&value.replace('\'', "&apos;").replace("&quot;", "\""));
            out.push('\'');
            rest = &rest[value_start + length + 1..];
        }
        out.push_str(rest);
    } else {
        out.push_str(tag);
    }
    if spelling.spaced_close == Some(true) && out.ends_with("/>") && !spaced_close(&out) {
        out.insert(out.len() - 2, ' ');
    }
    out
}

/// `rendered` with each start tag spelled as `previous` spells it.
pub(super) fn with_tag_spelling_of(rendered: &str, previous: &str) -> String {
    let (paths, most) = spellings(previous);
    let plain = |spelling: &Spelling| spelling.spaced_close != Some(true) && spelling.quote != Some('\'');
    if plain(&most) && paths.values().all(plain) {
        return rendered.to_string();
    }
    let tokens = tokenize(rendered);
    let plan = plan_paths(rendered, &tokens);
    let mut out = rendered.to_string();
    for token in tokens.iter().rev() {
        let Kind::OpenTag { closing: false, .. } = token.kind else { continue };
        let Some(path) = plan.path_of(token.start) else { continue };
        let own = paths.get(&unindexed(path)).copied().unwrap_or_default();
        let spelling = Spelling {
            spaced_close: own.spaced_close.or(most.spaced_close),
            quote: own.quote.or(most.quote),
        };
        let tag = &rendered[token.start..token.end];
        let spelled = respelled(tag, spelling);
        if spelled != tag {
            out.replace_range(token.start..token.end, &spelled);
        }
    }
    out
}
