//! Every built-in language says what its syntax messages quote of the
//! text (P1: the editor hides exactly those parts where a secret may be).
//! Checked by class over broken texts of every language (cut off, a
//! character typed in, a line written twice, with accents, Japanese and
//! emoji): each error gives its quotes, each a part of its message and of
//! the text, and changing every letter and digit of the text changes the
//! message only inside them.

use super::Language;

/// `character` moved `shift` along its alphabet (a letter or a digit
/// other than zero; any other letter or digit becomes an ASCII one).
fn moved(character: char, shift: u8) -> char {
    let rotate =
        |first: u8, count: u8| char::from(first + (character as u8 - first + shift) % count);
    match character {
        'a'..='z' => rotate(b'a', 26),
        'A'..='Z' => rotate(b'A', 26),
        '1'..='9' => rotate(b'1', 9),
        other if other.is_alphanumeric() && !other.is_ascii() => char::from(b'a' + shift),
        other => other,
    }
}

/// The languages' own words, kept: moved, they would be no value, an
/// error of another kind elsewhere.
const KEYWORDS: &[&str] = &[
    "true", "false", "null", "nil", "yes", "no", "inf", "nan", "int",
];

/// `text` with every letter and digit moved `shift` along, but in a
/// number (a word starting with a digit: `1e999` moved would be no
/// number; a message quoting a number is checked by the secrets matrix)
/// and in a keyword.
fn masked(text: &str, shift: u8) -> String {
    let mut out = String::with_capacity(text.len());
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String| {
        let kept = word.starts_with(|character: char| character.is_ascii_digit())
            || KEYWORDS.contains(&word.as_str());
        if kept {
            out.push_str(word);
        } else {
            out.extend(word.chars().map(|character| moved(character, shift)));
        }
        word.clear();
    };
    for character in text.chars() {
        if character.is_alphanumeric() {
            word.push(character);
        } else {
            flush(&mut word, &mut out);
            out.push(character);
        }
    }
    flush(&mut word, &mut out);
    out
}

/// Broken texts of `language`.
fn broken(language: Language) -> Vec<String> {
    let documents = [
        serde_json::json!({
            "name": "web",
            "spec": {"port": 80, "tags": ["a", "b"], "limits": {"cpu": 1.5, "on": true}},
            "items": [{"id": 1}, {"id": 2}]
        }),
        serde_json::json!({"päss": "s3cr3té", "日本": {"x": "日本語"}, "emo": "🔑key", "n": null}),
    ];
    let typed = [
        "\"",
        "'",
        "<",
        ">",
        ":",
        "=",
        "{",
        "[",
        "]",
        "}",
        "&",
        "*",
        "!",
        "\\",
        "#",
        "@",
        " ",
        "\n",
        "\t",
        "x y",
        "</q>",
        "<q>",
        "&b;",
        "&#1;",
        "!!int ",
        "*al",
        "${",
        "<<E\n",
        ",",
        "1e999",
        "99999999999999999999",
        "x:y",
        "\\q",
    ];
    let mut texts = Vec::new();
    for document in &documents {
        let Ok(text) = super::harness::text(language, document, &[]) else {
            continue;
        };
        let cuts: Vec<usize> = (0..=text.len())
            .filter(|at| text.is_char_boundary(*at))
            .collect();
        for (index, &cut) in cuts.iter().enumerate() {
            texts.push(text[..cut].to_string());
            for insert in typed {
                texts.push(format!("{}{insert}{}", &text[..cut], &text[cut..]));
            }
            if let Some(next) = cuts.get(index + 1) {
                texts.push(format!("{}{}", &text[..cut], &text[*next..]));
            }
        }
        let lines: Vec<&str> = text.lines().collect();
        for index in 0..lines.len() {
            let mut copy = lines.clone();
            copy.insert(index, lines[index]);
            texts.push(copy.join("\n") + "\n");
        }
    }
    texts
}

/// Whether each character of `message` differs in `other`: position by
/// position when they are as long, else what lies between what they start
/// and end with alike.
fn differing(message: &[char], other: &[char]) -> Vec<bool> {
    if message.len() == other.len() {
        return message
            .iter()
            .zip(other)
            .map(|(one, two)| one != two)
            .collect();
    }
    let start = message
        .iter()
        .zip(other)
        .take_while(|(one, two)| one == two)
        .count();
    let end = message.len()
        - message
            .iter()
            .rev()
            .zip(other.iter().rev())
            .take_while(|(one, two)| one == two)
            .count()
            .min(message.len() - start);
    (0..message.len())
        .map(|at| start <= at && at < end)
        .collect()
}

#[test]
fn every_built_in_syntax_message_says_what_it_quotes() {
    let mut failures = std::collections::BTreeSet::new();
    let mut errors = 0;
    let mut checked = std::collections::BTreeMap::<String, usize>::new();
    for language in [
        Language::YAML,
        Language::JSON,
        Language::TOML,
        Language::HCL,
        Language::XML,
    ] {
        let behavior = language.behavior();
        for text in broken(language) {
            let Err(error) = behavior.parse(&text) else {
                continue;
            };
            errors += 1;
            let Some(quotes) = &error.quotes else {
                failures.insert(format!("{language}: no quotes for {:?}", error.message));
                continue;
            };
            for quote in quotes {
                if error.message.get(quote.said.clone()).is_none()
                    || text.get(quote.of.clone()).is_none()
                {
                    failures.insert(format!(
                        "{language}: {quote:?} is no cut of {:?} in {text:?}",
                        error.message
                    ));
                }
            }
            // The characters of the message every reading with changed
            // letters changes, at the same place.
            let message: Vec<char> = error.message.chars().collect();
            let mut changed: Option<Vec<bool>> = None;
            let mut readings = Vec::new();
            for shift in [1, 2, 3] {
                let Err(other) = behavior.parse(&masked(&text, shift)) else {
                    continue;
                };
                if (other.line, other.column) != (error.line, error.column) {
                    continue;
                }
                let here = differing(&message, &other.message.chars().collect::<Vec<_>>());
                readings.push((shift, other.message));
                changed = Some(match changed {
                    Some(before) => before
                        .iter()
                        .zip(&here)
                        .map(|(one, two)| *one && *two)
                        .collect(),
                    None => here,
                });
            }
            // A changed word no quote holds that the text writes is text
            // the message quotes unsaid; a changed word the text does not
            // write is the message worded otherwise (`t` read as a broken
            // `true` once changed reads as a word).
            let Some(changed) = changed else {
                continue;
            };
            *checked.entry(language.to_string()).or_insert(0) += 1;
            let unsaid: Vec<bool> = changed
                .into_iter()
                .zip(error.message.char_indices())
                .map(|(changed, (at, _))| {
                    changed && !quotes.iter().any(|quote| quote.said.contains(&at))
                })
                .collect();
            let words = message.split(|character| !character.is_alphanumeric());
            let mut at = 0;
            for word in words {
                let span = at..at + word.len();
                at = span.end + 1;
                let word: String = word.iter().collect();
                // Quoted, the word reads changed in each other message (a
                // word the masking keeps is the language's own).
                let followed = readings.iter().all(|(shift, other)| {
                    let moved = masked(&word, *shift);
                    moved != word && other.contains(&moved)
                });
                if !word.is_empty()
                    && unsaid[span].iter().any(|unsaid| *unsaid)
                    && text.contains(&word)
                    && followed
                {
                    failures.insert(format!(
                        "{language}: {:?} quotes {word:?} unsaid ({quotes:?}) in {text:?}",
                        error.message
                    ));
                }
            }
        }
    }
    assert!(errors > 10_000, "only {errors} errors");
    // Most errors read again at the same place: the check is not blind.
    for language in ["YAML", "JSON", "TOML", "HCL", "XML"] {
        let count = checked.get(language).copied().unwrap_or(0);
        assert!(
            count > 1_000,
            "{language}: {count} errors read again ({checked:?})"
        );
    }
    let failures: Vec<String> = failures.into_iter().take(40).collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// A quote's bytes are in the text given to `Language::parse`, a file's
/// CRLF line breaks and byte order mark counted.
#[test]
fn quotes_are_placed_in_the_text_given() {
    let cases = [
        (Language::YAML, "a: 1\nuser: app\nuser: other\n"),
        (Language::JSON, "{\"a\": 1,\n\"user\": 1,\n\"user\": 2}\n"),
        (Language::TOML, "a = 1\nport = 99999999999999999999\n"),
        (Language::HCL, "a = 1\nb = <<EOT\nx\n"),
        (Language::XML, "<config>\n<a>\n</b>\n</config>\n"),
    ];
    for (language, text) in cases {
        let quoted = |given: &str| {
            let error = language.parse(given).unwrap_err();
            let quotes = error.quotes.expect("built-in quotes");
            assert!(!quotes.is_empty(), "{language} {text:?}");
            quotes
                .iter()
                .map(|quote| given[quote.of.clone()].replace(['\r', '\u{feff}'], ""))
                .collect::<Vec<_>>()
        };
        let plain = quoted(text);
        assert_eq!(
            quoted(&text.replace('\n', "\r\n")),
            plain,
            "{language} CRLF"
        );
        assert_eq!(quoted(&format!("\u{feff}{text}")), plain, "{language} BOM");
    }
}
