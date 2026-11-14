//! Elements with children the host writes on one line
//! (`<s n="a"><p>1</p></s>`), written on one line again by a rewrite, so
//! a save adding, removing or moving them changes only their lines; and
//! spread over lines for a structural save (see [`children_apart`]), so
//! the comments and processing instructions between their children stay.

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
        // Each line's indent dropped, what ends it kept (the spaces a
        // host wrote between two children, see `children_apart`).
        out.push_str(&rendered[copied..start]);
        out.extend(spread.lines().map(str::trim_start));
        copied = range.end;
    }
    out.push_str(&rendered[copied..]);
    out
}

/// `source` with the children of each element it writes on one line, and
/// the comments and processing instructions between them, on lines of
/// their own, indented by depth (which reads the same: the whitespace
/// between child elements is no text). A structural save patches that
/// line by line, keeping what it does not touch, and
/// [`with_one_line_elements_of`] joins them back. `None` when `source`
/// writes no such element.
pub(super) fn children_apart(source: &str) -> Option<String> {
    let tokens = tokenize(source);
    let unit = indent_unit(source);
    // Where a line break and indent go: byte, depth below the outermost
    // one-line element.
    let mut breaks: Vec<(usize, usize)> = Vec::new();
    // The end tokens of the one-line elements open around the next one.
    let mut open: Vec<usize> = Vec::new();
    for index in 0..tokens.len() {
        while open.last().is_some_and(|end| *end < index) {
            open.pop();
        }
        let Some((end, _)) = element_with_children(source, &tokens, index) else {
            continue;
        };
        if source[tokens[index].start..tokens[end].end].contains('\n') {
            continue;
        }
        let Some(children) = child_starts(source, &tokens, index, end) else {
            continue;
        };
        let depth = open.len();
        breaks.extend(children.into_iter().map(|at| (at, depth + 1)));
        breaks.push((tokens[end].start, depth));
        open.push(end);
    }
    if breaks.is_empty() {
        return None;
    }
    breaks.sort_unstable();
    let mut out = String::with_capacity(source.len() + breaks.len() * 8);
    let mut copied = 0;
    for (at, depth) in breaks {
        let line_start = source[..at].rfind('\n').map_or(0, |found| found + 1);
        let indent: String = source[line_start..].chars().take_while(|character| *character == ' ' || *character == '\t').collect();
        out.push_str(&source[copied..at]);
        out.push('\n');
        out.push_str(&indent);
        out.push_str(&unit.repeat(depth));
        copied = at;
    }
    out.push_str(&source[copied..]);
    Some(out)
}

/// Where each child of the element whose start tag is token `index` and end
/// tag token `end` starts (elements, comments, processing instructions);
/// `None` when text sits beside them (mixed content, or a CDATA section).
fn child_starts(source: &str, tokens: &[Token], index: usize, end: usize) -> Option<Vec<usize>> {
    let mut starts = Vec::new();
    let mut depth = 0usize;
    for token in &tokens[index + 1..end] {
        let self_closing = source[..token.end].ends_with("/>");
        match token.kind {
            Kind::OpenTag { closing: false, .. } => {
                if depth == 0 {
                    starts.push(token.start);
                }
                if !self_closing {
                    depth += 1;
                }
            }
            Kind::EndTag => depth = depth.checked_sub(1)?,
            Kind::Comment if depth == 0 => starts.push(token.start),
            Kind::Special if depth == 0 && source[token.start..].starts_with("<?") => starts.push(token.start),
            _ if depth == 0 => return None,
            _ => {}
        }
    }
    Some(starts)
}

#[cfg(test)]
mod tests {
    use super::{children_apart, with_one_line_elements_of};

    #[test]
    fn children_on_one_line_are_spread_and_joined_back() {
        let host = "<r a=\"1\"><!-- c --><?pi x?><s n=\"1\"><p>1</p> <q/></s><b><![CDATA[<x>]]></b></r>\n";
        let apart = children_apart(host).expect("spread");
        assert_eq!(
            apart,
            "<r a=\"1\">\n  <!-- c -->\n  <?pi x?>\n  <s n=\"1\">\n    <p>1</p> \n    <q/>\n  </s>\n  <b><![CDATA[<x>]]></b>\n</r>\n"
        );
        assert_eq!(super::super::xml_parse(&apart).ok(), super::super::xml_parse(host).ok());
        assert_eq!(with_one_line_elements_of(&apart, host), host);
        // Nested in a document over lines, indented from its line.
        let host = "<r>\n  <s><p>1</p><!-- c --></s>\n</r>\n";
        assert_eq!(children_apart(host).as_deref(), Some("<r>\n  <s>\n    <p>1</p>\n    <!-- c -->\n  </s>\n</r>\n"));
        // Text beside children is left on its line; nothing on one line,
        // nothing spread.
        assert_eq!(children_apart("<r>t<a/></r>\n"), None);
        assert_eq!(children_apart("<r>\n  <a>1</a>\n</r>\n"), None);
    }

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
