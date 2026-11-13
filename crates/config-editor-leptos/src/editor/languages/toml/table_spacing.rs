//! How a host parts its tables: the writer puts a blank line before each
//! table header (`[package]`, `[[bin]]`); a host that writes its headers
//! right under the line before has them written so again, so a table added
//! from another view is parted as the file parts the others.

use super::editing::multi_line_string_bodies;

/// The lines of `source` that are table headers (outside multi-line
/// strings), by index.
fn headers(lines: &[&str], source: &str) -> Vec<usize> {
    let bodies = multi_line_string_bodies(source);
    lines
        .iter()
        .enumerate()
        .filter(|(index, line)| {
            line.trim_start().starts_with('[') && !bodies.iter().any(|body| body.contains(index))
        })
        .map(|(index, _)| index)
        .collect()
}

/// Whether `previous` writes most of its table headers (those after other
/// lines) with no blank line before them, nor before the comments above
/// them.
fn tight(previous: &str) -> bool {
    let lines: Vec<&str> = previous.split('\n').collect();
    let (mut spaced, mut unspaced) = (0, 0);
    for header in headers(&lines, previous) {
        // Above the comments written over it.
        let mut above = header;
        while above > 0 && lines[above - 1].trim_start().starts_with('#') {
            above -= 1;
        }
        if above == 0 {
            continue;
        }
        if lines[above - 1].trim().is_empty() {
            spaced += 1;
        } else {
            unspaced += 1;
        }
    }
    unspaced > spaced
}

/// `rendered` (as the writer writes it) with the blank lines before its
/// table headers dropped where `previous` writes none.
pub(super) fn spaced_like(rendered: String, previous: &str) -> String {
    if !tight(previous) {
        return rendered;
    }
    let lines: Vec<&str> = rendered.split('\n').collect();
    let headers = headers(&lines, &rendered);
    let dropped = |index: usize| {
        lines[index].trim().is_empty() && headers.binary_search(&(index + 1)).is_ok()
    };
    lines
        .iter()
        .enumerate()
        .filter(|(index, _)| !dropped(*index))
        .map(|(_, line)| *line)
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::spaced_like;

    #[test]
    fn headers_keep_the_host_s_spacing() {
        let rendered = "[a]\nx = 1\n\n[b]\ny = \"\"\"\n\n[s]\n\"\"\"\n".to_string();
        // A host writing its headers right under the line before.
        assert_eq!(
            spaced_like(rendered.clone(), "[a]\nx = 1\n# b\n[b]\ny = 2\n"),
            "[a]\nx = 1\n[b]\ny = \"\"\"\n\n[s]\n\"\"\"\n"
        );
        // One with blank lines between them, or with one table.
        assert_eq!(spaced_like(rendered.clone(), "[a]\nx = 1\n\n# b\n[b]\ny = 2\n"), rendered);
        assert_eq!(spaced_like(rendered.clone(), "[a]\nx = 1\n"), rendered);
    }
}
