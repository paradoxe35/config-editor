use leptos::prelude::*;

pub(crate) fn highlight_line(line: &str) -> AnyView {
    let indent_len = line.len() - line.trim_start().len();
    let (indent, content) = line.split_at(indent_len);
    if content.starts_with('#') {
        return view! {
            <span>{indent}<span class="yaml-editor__comment">{content.to_string()}</span></span>
        }
        .into_any();
    }

    let (dash, content) = content
        .strip_prefix("- ")
        .map_or(("", content), |content| ("- ", content));
    if let Some((key, value)) = content.split_once(':')
        && !key.is_empty()
        && key
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
        && (value.is_empty() || value.starts_with(' ') || value.starts_with('\t'))
    {
        return view! {
            <span>{indent}{dash}<span class="yaml-editor__key">{key.to_string()}</span><span class="yaml-editor__punctuation">:</span>{highlight_value(value)}</span>
        }
        .into_any();
    }

    if !dash.is_empty() {
        return view! {
            <span>{indent}<span class="yaml-editor__punctuation">"- "</span>{highlight_value(content)}</span>
        }
        .into_any();
    }
    view! { <span>{line.to_string()}</span> }.into_any()
}

fn highlight_value(value: &str) -> AnyView {
    let whitespace_len = value.len() - value.trim_start().len();
    let (whitespace, value) = value.split_at(whitespace_len);
    let (scalar, comment) = split_yaml_comment(value);
    let class = scalar_class(scalar);
    view! {
        <span>
            {whitespace}
            <span class=class>{scalar.to_string()}</span>
            {comment.map(|comment| view! { <span class="yaml-editor__comment">{comment.to_string()}</span> })}
        </span>
    }
    .into_any()
}

fn scalar_class(scalar: &str) -> &'static str {
    let scalar = scalar.trim_end();
    if matches!(scalar, "true" | "false" | "null" | "~") || scalar.parse::<f64>().is_ok() {
        "yaml-editor__literal"
    } else if scalar.starts_with('\'') || scalar.starts_with('"') {
        "yaml-editor__string"
    } else {
        "yaml-editor__value"
    }
}

fn split_yaml_comment(value: &str) -> (&str, Option<&str>) {
    let characters: Vec<_> = value.char_indices().collect();
    let mut quote = None;
    let mut escaped = false;
    let mut position = 0;
    while position < characters.len() {
        let (index, character) = characters[position];
        if let Some(active_quote) = quote {
            if active_quote == '\'' && character == '\'' {
                if characters
                    .get(position + 1)
                    .is_some_and(|(_, next)| *next == '\'')
                {
                    position += 2;
                    continue;
                }
                quote = None;
            } else if active_quote == '"' && escaped {
                escaped = false;
            } else if active_quote == '"' && character == '\\' {
                escaped = true;
            } else if character == active_quote {
                quote = None;
            }
        } else if matches!(character, '\'' | '"') {
            quote = Some(character);
        } else if character == '#'
            && (index == 0
                || value[..index]
                    .chars()
                    .next_back()
                    .is_some_and(char::is_whitespace))
        {
            return (&value[..index], Some(&value[index..]));
        }
        position += 1;
    }
    (value, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_scalars_are_colored_as_strings_and_yaml_literals_stay_distinct() {
        assert_eq!(scalar_class("unquoted value"), "yaml-editor__value");
        assert_eq!(scalar_class("\"quoted value\""), "yaml-editor__string");
        assert_eq!(scalar_class("false"), "yaml-editor__literal");
        assert_eq!(scalar_class("42"), "yaml-editor__literal");
    }

    #[test]
    fn comments_are_split_only_outside_yaml_quotes() {
        assert_eq!(
            split_yaml_comment("plain value # note"),
            ("plain value ", Some("# note"))
        );
        assert_eq!(
            split_yaml_comment("\"value # not a comment\""),
            ("\"value # not a comment\"", None)
        );
        assert_eq!(
            split_yaml_comment("'it''s # still quoted'"),
            ("'it''s # still quoted'", None)
        );
    }
}
