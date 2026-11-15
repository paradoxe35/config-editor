//! Heredocs: a delimiter kept clear of the text, and the template escapes
//! (`$${`, `%%{`) read and written as a quoted string's are.

/// The heredoc `header` (`<<EOT`, `<<-EOT`) and `terminator` line (`EOT`,
/// `  EOT`) for `text`: as written while no line of `text` is the
/// delimiter, else with the first free one (`EOT_1`, `EOT_2`, …). `None`
/// when the header names no delimiter.
pub(super) fn free_delimiter(header: &str, terminator: &str, text: &str) -> Option<(String, String)> {
    let name = header.trim_start_matches("<<").trim_start_matches('-').trim();
    if name.is_empty() || terminator.trim() != name {
        return None;
    }
    let taken = |candidate: &str| text.lines().any(|line| line.trim() == candidate);
    let free = std::iter::once(name.to_string())
        .chain((1..).map(|serial| format!("{name}_{serial}")))
        .find(|candidate| !taken(candidate))?;
    let mark = &header[..header.len() - header.trim_start_matches("<<").trim_start_matches('-').len()];
    let indent = &terminator[..terminator.len() - terminator.trim_start().len()];
    Some((format!("{mark}{free}"), format!("{indent}{free}")))
}

/// Whether `character` continues an HCL name (a heredoc's delimiter).
pub(super) fn continues_name(character: char) -> bool {
    character.is_alphanumeric() || matches!(character, '_' | '-')
}

/// Where the heredoc terminator `line` ends a heredoc delimited by
/// `marker`: the byte offset just past the marker, when the line holds,
/// after spaces and tabs, the marker with no name character after it. The
/// parser ends a heredoc there and reads the rest of the line on as code
/// (`  EOT]` closes a list, `EOT,` goes on to the next item).
pub(super) fn terminator_end(line: &str, marker: &str) -> Option<usize> {
    let after = line.trim_start_matches([' ', '\t']).strip_prefix(marker)?;
    (!after.starts_with(continues_name)).then_some(line.len() - after.len())
}

/// Whether `template` (a string's raw template text) holds a template
/// sequence: an interpolation or directive (`${`, `%{`) not escaped.
pub(super) fn has_sequence(template: &str) -> bool {
    let plain = template.replace("$${", "").replace("%%{", "");
    plain.contains("${") || plain.contains("%{")
}

/// The text a heredoc value `written` (as the host writes it: `<<-EOT`,
/// its lines, the terminator) that holds a template sequence reads as, and
/// that heredoc, its lines as written; `None` for any other value.
pub(super) fn written_heredoc(written: &str) -> Option<(String, hcl::Heredoc)> {
    let (header, rest) = written.split_once('\n')?;
    let name = header.strip_prefix("<<")?;
    let (strip, name) = match name.strip_prefix('-') {
        Some(name) => (hcl::expr::HeredocStripMode::Indent, name),
        None => (hcl::expr::HeredocStripMode::None, name),
    };
    let body = &rest[..rest.trim_end().rfind('\n').map_or(0, |at| at + 1)];
    if !has_sequence(body) {
        return None;
    }
    let read = hcl::from_str::<serde_json::Value>(&format!("v = {written}\n")).ok()?;
    let text = read.get("v")?.as_str()?.to_string();
    let delimiter = hcl::Identifier::new(name.trim()).ok()?;
    Some((text, hcl::Heredoc::new(delimiter, body).with_strip_mode(strip)))
}

/// `text` written in a template: `${` as `$${`, `%{` as `%%{`.
pub(super) fn escaped(text: &str) -> String {
    text.replace("${", "$${").replace("%{", "%%{")
}

/// `text` as a quoted string (`"a \"b\" $${c}"`): HCL's escapes, and the
/// template markers escaped. `hcl-rs` writes a marker after a `$` or `%`
/// unescaped (`$${x}` as `$${x}`, which reads as `${x}`), so strings are
/// not left to it.
pub(super) fn quoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for character in escaped(text).chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            control if control.is_control() => out.push_str(&format!("\\u{:04x}", control as u32)),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// `text` as an expression that writes it as [`quoted`] does.
pub(super) fn string_expression(text: &str) -> hcl::Expression {
    let raw = quoted(text);
    hcl::Expression::TemplateExpr(Box::new(hcl::TemplateExpr::QuotedString(raw[1..raw.len() - 1].to_string())))
}

/// `source` parsed, each heredoc value that holds no template sequence read
/// as the text it stands for, as a quoted string is (and each whole float
/// marked, see [`super::floats::read_whole_floats`]). `hcl-rs` keeps a
/// heredoc as its template written again, with [`quoted`]'s marker bug, so
/// its text is taken from the parser's literal (escapes read, indentation
/// stripped). (`hcl-rs`'s reading, which `super::read` reads as.)
#[cfg(test)]
pub(super) fn parse_body(source: &str) -> Result<hcl::Body, hcl::Error> {
    let mut body: hcl_edit::structure::Body = source.parse()?;
    read_heredocs(&mut body);
    Ok(body.into())
}

/// Each heredoc value of `body` holding no template sequence read as its
/// text, and each whole float marked.
pub(super) fn read_heredocs(body: &mut hcl_edit::structure::Body) {
    for mut structure in body.iter_mut() {
        if let Some(mut attribute) = structure.as_attribute_mut() {
            read_in(attribute.value_mut());
        } else if let Some(block) = structure.as_block_mut() {
            read_heredocs(&mut block.body);
        }
    }
}

/// Only values: a heredoc inside a call or an operation stays in its
/// expression's source text.
fn read_in(expression: &mut hcl_edit::expr::Expression) {
    use hcl_edit::expr::Expression;
    use hcl_edit::template::Element;
    match expression {
        Expression::HeredocTemplate(heredoc) => {
            let mut text = String::new();
            for element in heredoc.template.iter() {
                match element {
                    Element::Literal(literal) => text.push_str(literal.value()),
                    _ => return,
                }
            }
            *expression = Expression::String(hcl_edit::Decorated::new(text));
        }
        Expression::Array(items) => items.iter_mut().for_each(read_in),
        Expression::Object(object) => object.iter_mut().for_each(|(_, value)| read_in(value.expr_mut())),
        // A whole float written so (`1.0`) is read as one.
        Expression::Number(_) => super::floats::mark_whole_float(expression),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::{escaped, free_delimiter, has_sequence};

    #[test]
    fn template_escapes_read_and_write_back() {
        for text in ["${x}", "a ${x} b", "%{y} and ${x}${x}", "$${x}", "%${x}", "$ { } %", "end ${"] {
            assert!(!has_sequence(&escaped(text)), "{text}");
            let source = format!("v = <<EOT\n{}\nEOT\n", escaped(text));
            let value: serde_json::Value = hcl::from_body(super::parse_body(&source).unwrap()).unwrap();
            assert_eq!(value["v"], format!("{text}\n"), "{source}");
        }
        assert!(has_sequence("a ${var.x}") && has_sequence("%{ if x }"));
        assert!(!has_sequence("a $${x} %%{y}"));
    }

    /// Every text a quoted string writes reads back as itself, markers
    /// after `$` and `%` included.
    #[test]
    fn a_quoted_string_reads_back_as_its_text() {
        for text in ["$${x}", "%${x}", "$%{y}", "a \"b\" \\ c\n\t\u{1}", "$$$${x} %%%{y}", "café ${x}"] {
            let source = format!("v = {}\n", super::quoted(text));
            let value: serde_json::Value = hcl::from_str(&source).unwrap();
            assert_eq!(value["v"], text, "{source}");
        }
    }

    /// A heredoc's terminator may share its line with code (`EOT]`,
    /// `EOT,`), as the parser reads it: the values after it, items of
    /// repeated blocks included, are found where they are written (P1:
    /// read-only locks and problems are placed on these spans).
    #[test]
    fn values_after_a_heredoc_ending_mid_line_are_found_where_written() {
        use super::super::parse::{hcl_parse, hcl_value_span, written_at};
        let heredocs = [
            "web = [<<-EOT\n  ind\n  EOT]\n",
            "web = [<<EOT\nind\nEOT ]\n",
            "web = [<<EOT\nind\nEOT, \"b\"]\n",
            "web = [\n  <<EOT\nind\nEOT\n  ,\"b\"\n]\n",
            "web = upper(<<EOT\nind\nEOT)\n",
            "web = { k = <<EOT\nind\nEOT }\n",
            "web = <<EOT\nEOT is not the end\nEOT\n",
            "web = <<EOT-X\nEOT\nEOT-X\n",
        ];
        let blocks = "port {\n  b_2 = 1\n  x = <<EOT\nline\nEOT\n}\nport {\n  b_2 = -5\n}\nlast = 7\n";
        for heredoc in heredocs {
            for nested in [false, true] {
                let body = format!("{heredoc}{blocks}");
                let (source, prefix) = if nested { (format!("a {{\n{body}}}\n"), "/a") } else { (body, "") };
                hcl_parse(&source).unwrap_or_else(|error| panic!("{source}: {error:?}"));
                for (pointer, written) in [("port/0/b_2", "1"), ("port/1/b_2", "-5"), ("last", "7")] {
                    let pointer = format!("{prefix}/{pointer}");
                    let span = hcl_value_span(&source, &pointer).unwrap_or_else(|| panic!("{pointer} in {source}"));
                    assert_eq!(written_at(&source, span).as_deref(), Some(written), "{pointer} in {source}");
                }
            }
        }
    }

    #[test]
    fn a_heredoc_takes_a_delimiter_its_text_does_not_hold() {
        let pair = |header: &str, terminator: &str| Some((header.to_string(), terminator.to_string()));
        assert_eq!(free_delimiter("<<EOT", "EOT", "a\n"), pair("<<EOT", "EOT"));
        assert_eq!(free_delimiter("<<EOT", "EOT", "a\nEOT\n"), pair("<<EOT_1", "EOT_1"));
        assert_eq!(free_delimiter("<<-EOT", "  EOT", "  EOT\n  EOT_1\n"), pair("<<-EOT_2", "  EOT_2"));
        assert_eq!(free_delimiter("<<", "x", "a\n"), None);
    }
}
