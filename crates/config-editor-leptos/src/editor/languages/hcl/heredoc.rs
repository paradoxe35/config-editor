//! A heredoc's delimiter kept clear of its text.

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

#[cfg(test)]
mod tests {
    use super::free_delimiter;

    #[test]
    fn a_heredoc_takes_a_delimiter_its_text_does_not_hold() {
        let pair = |header: &str, terminator: &str| Some((header.to_string(), terminator.to_string()));
        assert_eq!(free_delimiter("<<EOT", "EOT", "a\n"), pair("<<EOT", "EOT"));
        assert_eq!(free_delimiter("<<EOT", "EOT", "a\nEOT\n"), pair("<<EOT_1", "EOT_1"));
        assert_eq!(free_delimiter("<<-EOT", "  EOT", "  EOT\n  EOT_1\n"), pair("<<-EOT_2", "  EOT_2"));
        assert_eq!(free_delimiter("<<", "x", "a\n"), None);
    }
}
