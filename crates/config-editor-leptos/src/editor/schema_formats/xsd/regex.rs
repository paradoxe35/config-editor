//! XSD patterns as the validator's regexes. An XSD pattern matches the
//! whole value (it has no anchors: `^` and `$` are plain characters), its
//! name escapes `\i` and `\c` are XML's name characters, and a class may
//! subtract another (`[a-z-[aeiou]]`). What has no counterpart (the
//! `\p{Is…}` Unicode blocks) makes the pattern untranslatable.

/// XML's name start characters, as a class body.
const NAME_START: &str = r":A-Z_a-z\x{C0}-\x{D6}\x{D8}-\x{F6}\x{F8}-\x{2FF}\x{370}-\x{37D}\x{37F}-\x{1FFF}\x{200C}-\x{200D}\x{2070}-\x{218F}\x{2C00}-\x{2FEF}\x{3001}-\x{D7FF}\x{F900}-\x{FDCF}\x{FDF0}-\x{FFFD}\x{10000}-\x{EFFFF}";

/// XML's name characters besides the start characters, as a class body.
const NAME_MORE: &str = r"\-.0-9\x{B7}\x{300}-\x{36F}\x{203F}-\x{2040}";

/// `pattern` as an anchored regex, or why it cannot be one.
pub(super) fn translate(pattern: &str) -> Result<String, String> {
    let mut out = String::with_capacity(pattern.len() + 8);
    let characters: Vec<char> = pattern.chars().collect();
    let mut index = 0;
    // The depth of character classes open, and for each whether a `-[`
    // subtraction was opened in it.
    let mut classes: Vec<bool> = Vec::new();
    while index < characters.len() {
        let character = characters[index];
        let in_class = !classes.is_empty();
        match character {
            '\\' => {
                let Some(&next) = characters.get(index + 1) else {
                    return Err("a pattern ends with \\".to_string());
                };
                index += 2;
                match next {
                    'i' | 'c' | 'I' | 'C' => {
                        let body = if matches!(next, 'i' | 'I') {
                            NAME_START.to_string()
                        } else {
                            format!("{NAME_START}{NAME_MORE}")
                        };
                        let negated = next.is_ascii_uppercase();
                        match (in_class, negated) {
                            (true, false) => out.push_str(&body),
                            (true, true) => {
                                return Err(format!("\\{next} inside a character class"));
                            }
                            (false, false) => out.push_str(&format!("[{body}]")),
                            (false, true) => out.push_str(&format!("[^{body}]")),
                        }
                    }
                    'p' | 'P' => {
                        let rest: String = characters[index..].iter().collect();
                        let Some(end) = rest.find('}') else {
                            return Err("an unclosed \\p{…}".to_string());
                        };
                        let name = &rest[1..end];
                        if name.starts_with("Is") {
                            return Err(format!("the Unicode block \\{next}{{{name}}}"));
                        }
                        out.push('\\');
                        out.push(next);
                        out.push_str(&rest[..=end]);
                        index += end + 1;
                    }
                    // Escapes both read alike.
                    'n' | 'r' | 't' | 's' | 'S' | 'd' | 'D' | 'w' | 'W' => {
                        out.push('\\');
                        out.push(next);
                    }
                    other => {
                        // A single character escape (`\.`, `\-`, `\^`, …).
                        out.push('\\');
                        out.push(other);
                    }
                }
                continue;
            }
            '[' if !in_class => {
                classes.push(false);
                out.push('[');
                if characters.get(index + 1) == Some(&'^') {
                    out.push('^');
                    index += 1;
                }
            }
            '-' if in_class && characters.get(index + 1) == Some(&'[') => {
                // Class subtraction: `[a-z-[aeiou]]` is `[a-z--[aeiou]]`.
                out.push_str("--[");
                index += 2;
                if let Some(last) = classes.last_mut() {
                    *last = true;
                }
                classes.push(false);
                if characters.get(index) == Some(&'^') {
                    out.push('^');
                    index += 1;
                }
                continue;
            }
            '[' => out.push_str("\\["),
            ']' if in_class => {
                classes.pop();
                out.push(']');
            }
            // Plain characters in XSD, special in the regex.
            '^' | '$' if !in_class => {
                out.push('\\');
                out.push(character);
            }
            '&' | '~' if in_class => {
                // Set operators of the regex engine, plain in XSD.
                out.push('\\');
                out.push(character);
            }
            '.' if !in_class => out.push_str("[^\\n\\r]"),
            _ => out.push(character),
        }
        index += 1;
    }
    if !classes.is_empty() {
        return Err("an unclosed character class".to_string());
    }
    Ok(format!("^(?:{out})$"))
}

#[cfg(test)]
mod tests {
    use super::translate;

    #[test]
    fn xsd_patterns_become_anchored_regexes() {
        let cases: &[(&str, Result<&str, ()>)] = &[
            ("[0-9]{4}", Ok("^(?:[0-9]{4})$")),
            ("a|b", Ok("^(?:a|b)$")),
            ("$5^", Ok("^(?:\\$5\\^)$")),
            ("[^a]", Ok("^(?:[^a])$")),
            ("a.b", Ok("^(?:a[^\\n\\r]b)$")),
            ("[a-z-[aeiou]]", Ok("^(?:[a-z--[aeiou]])$")),
            ("\\d+\\.\\d+", Ok("^(?:\\d+\\.\\d+)$")),
            ("\\p{Lu}", Ok("^(?:\\p{Lu})$")),
            ("\\p{IsBasicLatin}", Err(())),
            ("[a", Err(())),
            ("a\\", Err(())),
        ];
        for (pattern, expected) in cases {
            let got = translate(pattern);
            match expected {
                Ok(regex) => assert_eq!(got.as_deref(), Ok(*regex), "{pattern}"),
                Err(()) => assert!(got.is_err(), "{pattern}: {got:?}"),
            }
        }
        let name = translate("\\i\\c*").unwrap();
        let regex = crate::editor::pattern_engine::Regex::new(&name).unwrap();
        assert!(regex.is_match("ns:item-1") && !regex.is_match("1item"));
    }
}
