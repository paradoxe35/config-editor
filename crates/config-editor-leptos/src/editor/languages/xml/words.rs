//! The XML reader's errors in plain words: what is missing or wrong and
//! how to write it. A message that is plain already passes unchanged.

/// The reader's `message` in plain words.
pub(super) fn plain(message: &str) -> String {
    let message = message
        .strip_prefix("syntax error: ")
        .or_else(|| message.strip_prefix("ill-formed document: "))
        .unwrap_or(message);
    // `position 4: …`: the place is reported on its own.
    let message = match message.strip_prefix("position ") {
        Some(rest) => rest.split_once(": ").map_or(message, |(_, said)| said),
        None => message,
    };
    if message.starts_with("entity or character reference not closed") {
        return "an & starts a reference that no ; ends: write a plain & as &amp;".to_string();
    }
    if let Some((what, ending)) = unclosed(message) {
        let what = match what {
            "tag" => "a tag",
            "attribute value" => "an attribute value",
            "comment" => "a comment",
            "CDATA" => "a CDATA section",
            "XML declaration" => "the XML declaration",
            "DOCTYPE" => "the DOCTYPE",
            "processing instruction" => "a processing instruction",
            other => return format!("{other} is never closed: {ending} is missing"),
        };
        return format!("{what} is never closed: {ending} is missing");
    }
    if let Some(open) = between(message, "expected `</", ">`, but `</")
        && let Some(found) = message.split("but `</").nth(1).and_then(|rest| rest.split(">`").next())
    {
        return format!("</{found}> does not match the open <{open}>: close <{open}> first, or fix the name");
    }
    if let Some(tag) = between(message, "close tag `</", ">` does not match any open tag") {
        return format!("</{tag}> closes an element that was never opened");
    }
    if message.starts_with("attribute value must be enclosed") {
        return "an attribute value must be in quotes, as in name=\"value\"".to_string();
    }
    if message.starts_with("attribute key must be directly followed") {
        return "an attribute needs = and a value in quotes, as in name=\"value\"".to_string();
    }
    message.to_string()
}

/// What `X not closed: \`Y\` not found before end of input` names: `X` and
/// `Y`.
fn unclosed(message: &str) -> Option<(&str, String)> {
    let (what, rest) = message.split_once(" not closed: `")?;
    let ending = rest.strip_suffix("` not found before end of input")?;
    Some((what, ending.to_string()))
}

/// The text of `message` between `start` and `end`.
fn between<'a>(message: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let rest = message.split_once(start)?.1;
    rest.split(end).next()
}
