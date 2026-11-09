//! Elements with children the host writes on one line
//! (`<s n="a"><p>1</p></s>`), written on one line again by a rewrite, so
//! a save adding, removing or moving them changes only their lines.

use super::*;
use std::collections::HashSet;

/// The element paths (indices left out) of `source`'s elements that hold
/// child elements and open and close on one line.
fn one_line_paths(source: &str) -> HashSet<Vec<String>> {
    let tokens = tokenize(source);
    let plan = plan_paths(source, &tokens);
    let mut found = HashSet::new();
    for index in 0..tokens.len() {
        if let Some((end, path)) = element_with_children(source, &tokens, index)
            && !source[tokens[index].start..tokens[end].end].contains('\n')
            && let Some(path) = path.then(|| plan.path_of(tokens[index].start)).flatten()
        {
            found.insert(unindexed(path));
        }
    }
    found
}

fn unindexed(path: &[Seg]) -> Vec<String> {
    strings(path).into_iter().filter(|segment| crate::editor::language::pointer_index(segment).is_none()).collect()
}

/// The end tag's token of the element whose start tag is token `index`,
/// when it holds a child element (and `true`, so the path is read).
fn element_with_children(source: &str, tokens: &[Token], index: usize) -> Option<(usize, bool)> {
    let token = tokens[index];
    if !matches!(token.kind, Kind::OpenTag { closing: false, .. }) || source[..token.end].ends_with("/>") {
        return None;
    }
    let (_, end) = element_from(source, tokens, index)?;
    let end = end?;
    let children = tokens[index + 1..end].iter().any(|inner| matches!(inner.kind, Kind::OpenTag { closing: false, .. }));
    children.then_some((end, true))
}

/// `rendered` with each element `previous` writes on one line (by path)
/// that the serializer spread over lines joined onto one, when each of its
/// lines holds whole tags.
pub(super) fn with_one_line_elements_of(rendered: &str, previous: &str) -> String {
    let wanted = one_line_paths(previous);
    if wanted.is_empty() {
        return rendered.to_string();
    }
    let tokens = tokenize(rendered);
    let plan = plan_paths(rendered, &tokens);
    let mut out = String::with_capacity(rendered.len());
    let mut copied = 0;
    for index in 0..tokens.len() {
        let start = tokens[index].start;
        if start < copied {
            continue;
        }
        let Some((end, _)) = element_with_children(rendered, &tokens, index) else {
            continue;
        };
        let range = start..tokens[end].end;
        let spread = &rendered[range.clone()];
        let whole_tags = spread.lines().all(|line| {
            let line = line.trim();
            line.starts_with('<') && line.ends_with('>')
        });
        if !spread.contains('\n')
            || !whole_tags
            || !plan.path_of(start).is_some_and(|path| wanted.contains(&unindexed(path)))
        {
            continue;
        }
        out.push_str(&rendered[copied..start]);
        out.extend(spread.lines().map(str::trim));
        copied = range.end;
    }
    out.push_str(&rendered[copied..]);
    out
}

#[cfg(test)]
mod tests {
    use super::with_one_line_elements_of;

    #[test]
    fn elements_the_host_writes_on_one_line_are_joined() {
        let previous = "<r>\n  <s n=\"a\"><p>1</p></s>\n  <t>\n    <p>1</p>\n  </t>\n</r>\n";
        let rendered = "<r>\n  <s n=\"b\">\n    <p>2</p>\n  </s>\n  <s n=\"a\">\n    <p>1</p>\n    <q>x</q>\n  </s>\n  <t>\n    <p>1</p>\n  </t>\n</r>\n";
        assert_eq!(
            with_one_line_elements_of(rendered, previous),
            "<r>\n  <s n=\"b\"><p>2</p></s>\n  <s n=\"a\"><p>1</p><q>x</q></s>\n  <t>\n    <p>1</p>\n  </t>\n</r>\n"
        );
        // Text over lines stays as written.
        let rendered = "<r>\n  <s>\n    <p>a\nb</p>\n  </s>\n</r>\n";
        assert_eq!(with_one_line_elements_of(rendered, "<r>\n  <s><p>1</p></s>\n</r>\n"), rendered);
    }
}
