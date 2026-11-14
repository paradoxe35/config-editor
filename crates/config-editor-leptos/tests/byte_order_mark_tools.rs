//! A leading byte order mark through every headless entry point, in every
//! built-in language: each answers for the marked text what it answers for
//! the text without it, positions on the first line one column on (the mark
//! is a character of the text given), and none panics. Astral, other
//! non-ASCII and ASCII characters sit in each kind of place (keys, values,
//! element text, attributes, comments), on the first line too, in documents
//! that read and in ones that do not (QA 31: an XML text with a mark and an
//! emoji in an element's text panicked `convert`).
#![cfg(all(
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml",
    feature = "schema-validation",
    feature = "folding"
))]

use std::panic::{AssertUnwindSafe, catch_unwind};

use config_editor_leptos::{
    ConvertError, Diagnostic, FieldPath, Language, ParseError, Schema, TextPosition, TextRange,
    check_syntax,
};
use serde_json::Value;

const MARK: char = '\u{FEFF}';

/// Documents with `@` where a character goes, each language's places.
fn templates(language: Language) -> &'static [&'static str] {
    match language.label() {
        "YAML" => &[
            "k@: v@ # c@\n",
            "# c@\nk@: v@\nlist:\n  - \"q@\"\n  - 'r@'\nn: 1\nblock: |\n  t@\n",
            "{a@: b@, n: [1, \"x@\"]}\n",
            "k@: [v@\n",
            "k@: v@\n  bad: [\n",
        ],
        "JSON" => &[
            "{\"k@\": \"v@\", \"n\": [1, \"x@\"]}",
            "{\n  \"k@\": \"v@\",\n  \"o\": {\"a\": \"@\"}\n}\n",
            "{\"k@\": }",
            "{\"k@\": \"v@\",\n  \"x\": tru }\n",
        ],
        "TOML" => &[
            "\"k@\" = \"v@\" # c@\n",
            "# c@\nk = 'v@'\n[\"t@\"]\na = \"\"\"\nx@\"\"\"\n[[l]]\nb = \"y@\"\n",
            "k = \"v@\n",
            "\"k@\" = \"v@\"\nk = = 1\n",
        ],
        "HCL" => &[
            "k = \"v@\" # c@\n",
            "// c@\nb \"l@\" {\n  a = \"x@\"\n  n = [1, \"y@\"]\n}\nh = <<EOT\nt@\nEOT\n",
            "k = \"v@\n",
            "k = \"v@\"\nb {\n  = 1\n}\n",
        ],
        "XML" => &[
            "<r k=\"v@\"><e>t@</e><!-- c@ --></r>\n",
            "<?xml version=\"1.0\"?>\n<r>\n  <a>@</a>\n  <b x=\"@\">y@</b>\n  <!-- @ -->\n</r>\n",
            "<a>@</a>\n",
            "<r><e@>t@</e@></r>\n",
            "<r>@<e>t</e></r>\n",
            "<r><e>t@</x></r>\n",
            "<r>\n  <e>t@</e>\n  <e k=\"@\">\n</r>\n",
        ],
        other => panic!("no documents for {other}"),
    }
}

const CHARACTERS: [&str; 4] = ["x", "é", "😀", "😀é😀"];

const SCHEMA: &str = r#"{
  "type": "object",
  "properties": {"n": {"type": "string"}, "list": {"type": "array", "items": {"type": "number"}}},
  "additionalProperties": {"type": "number", "description": "a number"}
}"#;

/// A first-line column moved one on, past the mark.
fn on(line: Option<usize>, column: Option<usize>) -> Option<usize> {
    match line {
        Some(1) => column.map(|column| column + 1),
        _ => column,
    }
}

fn shifted_position(position: TextPosition) -> TextPosition {
    TextPosition::new(
        position.line,
        on(Some(position.line), Some(position.column)).unwrap(),
    )
}

fn shifted_range(range: TextRange) -> TextRange {
    TextRange::new(shifted_position(range.start), shifted_position(range.end))
}

fn shifted_error(error: ParseError) -> ParseError {
    ParseError {
        column: on(error.line, error.column),
        ..error
    }
}

fn shifted_diagnostic(mut diagnostic: Diagnostic) -> Diagnostic {
    diagnostic.end_column = on(
        diagnostic.end_line.or(diagnostic.line),
        diagnostic.end_column,
    );
    diagnostic.column = on(diagnostic.line, diagnostic.column);
    diagnostic
}

fn shifted_conversion(
    result: Result<config_editor_leptos::Conversion, ConvertError>,
) -> Result<config_editor_leptos::Conversion, ConvertError> {
    result.map_err(|error| match error {
        ConvertError::Parse(error) => ConvertError::Parse(shifted_error(error)),
        other => other,
    })
}

/// Every pointer in `value`, the root first.
fn pointers(value: &Value, at: String, out: &mut Vec<String>) {
    out.push(at.clone());
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let key = key.replace('~', "~0").replace('/', "~1");
                pointers(child, format!("{at}/{key}"), out);
            }
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                pointers(child, format!("{at}/{index}"), out);
            }
        }
        _ => {}
    }
}

/// Every place in `text`: its line, column and UTF-16 offset.
fn places(text: &str) -> Vec<(TextPosition, usize)> {
    let mut out = Vec::new();
    let (mut line, mut column, mut units) = (1, 1, 0);
    for character in text.chars() {
        out.push((TextPosition::new(line, column), units));
        units += character.len_utf16();
        if character == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    out.push((TextPosition::new(line, column), units));
    out
}

/// What each tool answers for `plain` and for it with a mark, the second
/// moved back where the first is, as text; a panic is an answer too.
fn compare(language: Language, plain: &str, schema: &Schema, failures: &mut Vec<String>) {
    let marked = format!("{MARK}{plain}");
    let mut check = |tool: &str, run: &dyn Fn() -> (String, String)| match catch_unwind(
        AssertUnwindSafe(run),
    ) {
        Ok((without, with)) if without == with => {}
        Ok((without, with)) => failures.push(format!(
            "{} {tool} {plain:?}\n  without the mark: {without}\n  with it:          {with}",
            language.label()
        )),
        Err(_) => failures.push(format!("{} {tool} {plain:?}: panicked", language.label())),
    };
    check("parse", &|| {
        (
            format!("{:?}", language.parse(plain).map_err(shifted_error)),
            format!("{:?}", language.parse(&marked)),
        )
    });
    check("check_syntax", &|| {
        let without: Vec<_> = check_syntax(language, plain)
            .into_iter()
            .map(shifted_diagnostic)
            .collect();
        (
            format!("{without:?}"),
            format!("{:?}", check_syntax(language, &marked)),
        )
    });
    check("Schema::validate", &|| {
        let without: Vec<_> = schema
            .validate(language, plain)
            .into_iter()
            .map(shifted_diagnostic)
            .collect();
        (
            format!("{without:?}"),
            format!("{:?}", schema.validate(language, &marked)),
        )
    });
    for target in Language::ALL {
        check(&format!("convert to {}", target.label()), &|| {
            (
                format!("{:?}", shifted_conversion(language.convert(plain, *target))),
                format!("{:?}", language.convert(&marked, *target)),
            )
        });
        check(&format!("Schema::convert to {}", target.label()), &|| {
            (
                format!(
                    "{:?}",
                    shifted_conversion(schema.convert(language, plain, *target))
                ),
                format!("{:?}", schema.convert(language, &marked, *target)),
            )
        });
    }
    check("fold_ranges", &|| {
        (
            format!("{:?}", language.fold_ranges(plain)),
            format!("{:?}", language.fold_ranges(&marked)),
        )
    });
    let mut all = Vec::new();
    if let Ok(value) = language.parse(plain) {
        pointers(&value, String::new(), &mut all);
    }
    check("range_of", &|| {
        let without: Vec<_> = all
            .iter()
            .map(|pointer| {
                language
                    .range_of(plain, FieldPath::from(pointer.as_str()))
                    .map(shifted_range)
            })
            .collect();
        let with: Vec<_> = all
            .iter()
            .map(|pointer| language.range_of(&marked, FieldPath::from(pointer.as_str())))
            .collect();
        (format!("{without:?}"), format!("{with:?}"))
    });
    check("ranges_of", &|| {
        let without: Vec<_> = language
            .ranges_of(plain, all.iter().map(String::as_str))
            .into_iter()
            .map(|range| range.map(|range| (shifted_range(range), range.slice(plain).to_string())))
            .collect();
        let with: Vec<_> = language
            .ranges_of(&marked, all.iter().map(String::as_str))
            .into_iter()
            .map(|range| range.map(|range| (range, range.slice(&marked).to_string())))
            .collect();
        (format!("{without:?}"), format!("{with:?}"))
    });
    let at = places(plain);
    check("path_at", &|| {
        let without: Vec<_> = at
            .iter()
            .map(|(position, _)| language.path_at(plain, *position))
            .collect();
        let with: Vec<_> = at
            .iter()
            .map(|(position, _)| language.path_at(&marked, shifted_position(*position)))
            .collect();
        (format!("{without:?}"), format!("{with:?}"))
    });
    check("Schema::hover", &|| {
        let without: Vec<_> = at
            .iter()
            .map(|(position, _)| schema.hover(language, plain, *position))
            .collect();
        let with: Vec<_> = at
            .iter()
            .map(|(position, _)| schema.hover(language, &marked, shifted_position(*position)))
            .collect();
        (format!("{without:?}"), format!("{with:?}"))
    });
    check("Schema::suggestions", &|| {
        let offered = |text: &str, cursor: usize, mark: &str| -> Vec<(String, String, String)> {
            schema
                .suggestions(language, text, cursor)
                .iter()
                .map(|suggestion| {
                    let accepted = suggestion.accept(text).0;
                    let accepted = accepted
                        .strip_prefix(mark)
                        .map_or(accepted.clone(), str::to_string);
                    (
                        suggestion.label().to_string(),
                        suggestion.detail().to_string(),
                        accepted,
                    )
                })
                .collect()
        };
        let mark = MARK.to_string();
        let without: Vec<_> = at
            .iter()
            .map(|(_, units)| offered(plain, *units, ""))
            .collect();
        let with: Vec<_> = at
            .iter()
            .map(|(_, units)| offered(&marked, units + 1, &mark))
            .collect();
        (format!("{without:?}"), format!("{with:?}"))
    });
}

#[test]
fn every_tool_reads_a_marked_text_as_the_text_after_its_mark() {
    let schema = Schema::compile(SCHEMA).unwrap();
    let mut failures = Vec::new();
    for language in Language::ALL {
        for template in templates(*language) {
            for character in CHARACTERS {
                compare(
                    *language,
                    &template.replace('@', character),
                    &schema,
                    &mut failures,
                );
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} answers differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
