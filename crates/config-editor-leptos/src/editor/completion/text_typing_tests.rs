//! Text scalars typed by the schema: the lexical forms each type reads,
//! every rule of `text_typing` as a matrix, and the reading with a schema
//! that says nothing being the language's own guess.

use serde_json::{Number, Value, json};

use super::text_typing::number_of;

#[test]
fn a_number_reads_from_every_decimal_form_and_nothing_else() {
    let number = |text: &str, fractions: bool| number_of(text, fractions).map(Value::Number);
    let cases: &[(&str, bool, Option<Value>)] = &[
        ("1.50", true, Some(json!(1.5))),
        ("+5", true, Some(json!(5))),
        ("+5", false, Some(json!(5))),
        ("-0", false, Some(json!(0))),
        ("007", false, Some(json!(7))),
        (".5", true, Some(json!(0.5))),
        ("5.", true, Some(json!(5.0))),
        ("1e3", true, Some(json!(1000.0))),
        ("2.0E-1", true, Some(json!(0.2))),
        ("1e3", false, Some(json!(1000.0))),
        ("1.0", false, Some(json!(1.0))),
        ("1.5", false, None),
        (
            "18446744073709551615",
            false,
            Some(json!(18_446_744_073_709_551_615_u64)),
        ),
        ("99999999999999999999", false, None),
        ("99999999999999999999", true, Some(json!(1e20))),
        ("", true, None),
        ("+", true, None),
        (".", true, None),
        ("1e", true, None),
        ("1e+", true, None),
        ("1,5", true, None),
        ("0x10", true, None),
        ("INF", true, None),
        ("NaN", true, None),
        (" 1", true, None),
        ("1e999", true, None),
    ];
    let failures: Vec<String> = cases
        .iter()
        .filter(|(text, fractions, expected)| number(text, *fractions) != *expected)
        .map(|(text, fractions, expected)| {
            format!(
                "{text:?} (fractions {fractions}): {:?}, want {expected:?}",
                number(text, *fractions)
            )
        })
        .collect();
    assert!(failures.is_empty(), "{failures:#?}");
    assert!(number_of("1", true).is_some_and(|number| number == Number::from(1)));
}

#[cfg(feature = "xml")]
mod xml {
    use serde_json::{Value, json};

    use super::super::text_typing::type_texts;
    use crate::editor::Language;
    use crate::editor::codec;

    /// `xml` read by `schema` (the property `v`'s schema, under `<r>`).
    fn read(xml: &str, property: Value) -> Value {
        let schema = json!({"properties": {"r": {"properties": {"v": property}}}});
        let value = codec::read(Language::XML.behavior(), xml, Some(&schema)).expect("parses");
        value["r"]["v"].clone()
    }

    #[test]
    fn each_text_reads_as_the_type_the_schema_gives_its_place() {
        let lexical =
            json!({"type": "boolean", "x-true": ["true", "1"], "x-false": ["false", "0"]});
        let yes_no = json!({"type": "boolean", "x-true": ["yes", "on"], "x-false": ["no", "off"]});
        let collapse = json!({"type": "string", "x-whitespace": "collapse"});
        let replace = json!({"type": "string", "x-whitespace": "replace"});
        let cases: Vec<(&str, Value, Value)> = vec![
            // Only a string: the exact text.
            (
                "<r><v>  80 </v></r>",
                json!({"type": "string"}),
                json!("  80 "),
            ),
            (
                "<r><v>true</v></r>",
                json!({"type": "string"}),
                json!("true"),
            ),
            (
                "<r><v>null</v></r>",
                json!({"type": "string"}),
                json!("null"),
            ),
            (
                "<r><v>1.50</v></r>",
                json!({"type": "string"}),
                json!("1.50"),
            ),
            ("<r v=\"007\"/>", json!({"type": "string"}), json!("007")),
            ("<r><v> a \n  b </v></r>", collapse, json!("a b")),
            ("<r><v>a\tb\n</v></r>", replace, json!("a b ")),
            // A number: the trimmed text, in any decimal form.
            ("<r><v>1.50</v></r>", json!({"type": "number"}), json!(1.5)),
            ("<r><v> 80 </v></r>", json!({"type": "integer"}), json!(80)),
            ("<r><v>+6</v></r>", json!({"type": "integer"}), json!(6)),
            (
                "<r><v>2e3</v></r>",
                json!({"type": "number"}),
                json!(2000.0),
            ),
            ("<r v=\"007\"/>", json!({"type": "integer"}), json!(7)),
            ("<r><v>4.10</v></r>", json!({"type": "number"}), json!(4.1)),
            // Not a number: the text, for validation to report.
            (
                "<r><v>eighty</v></r>",
                json!({"type": "integer"}),
                json!("eighty"),
            ),
            (
                "<r><v>1.5</v></r>",
                json!({"type": "integer"}),
                json!("1.5"),
            ),
            // A boolean: `true` and `false`, or the spellings the schema
            // lists (`x-true`, `x-false`: XSD's `1` and `0`).
            (
                "<r><v> false </v></r>",
                json!({"type": "boolean"}),
                json!(false),
            ),
            ("<r><v>1</v></r>", json!({"type": "boolean"}), json!("1")),
            ("<r><v>1</v></r>", lexical.clone(), json!(true)),
            ("<r><v>0</v></r>", lexical, json!(false)),
            (
                "<r><v>False</v></r>",
                json!({"type": "boolean"}),
                json!("False"),
            ),
            // Spellings any schema lists, not XSD's only.
            ("<r><v> on </v></r>", yes_no.clone(), json!(true)),
            ("<r><v>no</v></r>", yes_no.clone(), json!(false)),
            ("<r><v>true</v></r>", yes_no, json!("true")),
            // Null.
            ("<r><v>null</v></r>", json!({"type": "null"}), json!(null)),
            // Types without a string: each allowed reading in turn.
            (
                "<r><v>null</v></r>",
                json!({"type": ["integer", "null"]}),
                json!(null),
            ),
            (
                "<r><v>007</v></r>",
                json!({"type": ["integer", "null"]}),
                json!(7),
            ),
            (
                "<r><v>true</v></r>",
                json!({"type": ["integer", "boolean"]}),
                json!(true),
            ),
            // An enum without a type: the types of its values.
            ("<r><v>01</v></r>", json!({"enum": [1, 2]}), json!(1)),
            (
                "<r><v>01</v></r>",
                json!({"enum": ["01", "02"]}),
                json!("01"),
            ),
            ("<r><v>2.50</v></r>", json!({"const": 2.5}), json!(2.5)),
            // A string among other types: the guess, unless only the
            // string allows it.
            (
                "<r><v>5</v></r>",
                json!({"type": ["string", "integer"]}),
                json!(5),
            ),
            (
                "<r><v>5.0</v></r>",
                json!({"type": ["string", "integer"]}),
                json!("5.0"),
            ),
            (
                "<r><v>true</v></r>",
                json!({"anyOf": [{"type": "string"}, {"type": "integer"}]}),
                json!("true"),
            ),
            (
                "<r><v>auto</v></r>",
                json!({"anyOf": [{"type": "integer"}, {"enum": ["auto"]}]}),
                json!("auto"),
            ),
            (
                "<r><v>12</v></r>",
                json!({"anyOf": [{"type": "integer"}, {"enum": ["auto"]}]}),
                json!(12),
            ),
            // A schema saying nothing of the type: the guess.
            ("<r><v>5</v></r>", json!({}), json!(5)),
            (
                "<r><v>1.50</v></r>",
                json!({"description": "d"}),
                json!("1.50"),
            ),
            // Through references and combinators.
            (
                "<r><v>1.50</v></r>",
                json!({"allOf": [{"type": "number"}, {"minimum": 0}]}),
                json!(1.5),
            ),
            // A scalar or a list of them: a lone text reads as the scalar.
            (
                "<r><v>5</v></r>",
                json!({"anyOf": [{"type": "integer"}, {"type": "array", "items": {"type": "integer"}}]}),
                json!(5),
            ),
            // Only a list: the lone text is its item, typed as one.
            (
                "<r><v>1.50</v></r>",
                json!({"type": "array", "items": {"type": "number"}}),
                json!([1.5]),
            ),
            (
                "<r><v>true</v></r>",
                json!({"type": "array", "items": {"type": "string"}}),
                json!(["true"]),
            ),
            (
                "<r><v>1</v><v>2.0</v></r>",
                json!({"type": "array", "items": {"type": "number"}}),
                json!([1, 2.0]),
            ),
            // Inside an element with attributes: each by its own schema;
            // the text beside them by `#text`, or as text.
            (
                "<r><v port=\"080\" name=\"7\">1.50</v></r>",
                json!({"properties": {"port": {"type": "integer"}, "name": {"type": "string"}, "#text": {"type": "number"}}}),
                json!({"port": 80, "name": "7", "#text": 1.5}),
            ),
            (
                "<r><v port=\"080\" flag=\"true\">5</v></r>",
                json!({"properties": {"port": {"type": "string"}}}),
                json!({"port": "080", "flag": true, "#text": "5"}),
            ),
        ];
        let failures: Vec<String> = cases
            .iter()
            .filter_map(|(xml, schema, expected)| {
                let got = read(xml, schema.clone());
                (got != *expected).then(|| format!("{xml} by {schema}: {got}, want {expected}"))
            })
            .collect();
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    /// Round trip by property: values generated with the types a schema
    /// gives (strings spelled like numbers, booleans and `null` included,
    /// lists of one item and more, attributes' and elements' values), written
    /// as XML and read back by the schema, are the values again.
    #[test]
    fn generated_values_written_as_xml_read_back_by_the_schema() {
        struct Random(u64);
        impl Random {
            fn below(&mut self, bound: usize) -> usize {
                self.0 = self
                    .0
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                ((self.0 >> 33) as usize) % bound
            }
        }
        const STRINGS: [&str; 10] = [
            "x", "true", "null", "1.50", "007", " a b ", "-0", "1e3", "false ", "a\nb",
        ];
        const KINDS: [&str; 5] = ["string", "integer", "number", "boolean", "object"];
        /// A schema of `depth` levels and a value of it.
        fn generate(random: &mut Random, depth: usize) -> (Value, Value) {
            let kind = KINDS[random.below(if depth == 0 { 4 } else { 5 })];
            match kind {
                "string" => (
                    json!({"type": "string"}),
                    json!(STRINGS[random.below(STRINGS.len())]),
                ),
                "integer" => (
                    json!({"type": "integer"}),
                    json!(random.below(2000) as i64 - 1000),
                ),
                "number" => (
                    json!({"type": "number"}),
                    json!((random.below(20000) as f64 - 10000.0) / 8.0),
                ),
                "boolean" => (json!({"type": "boolean"}), json!(random.below(2) == 0)),
                _ => {
                    let mut properties = serde_json::Map::new();
                    let mut value = serde_json::Map::new();
                    for index in 0..1 + random.below(4) {
                        let key = format!("k{index}");
                        let (schema, item) = generate(random, depth - 1);
                        if random.below(3) == 0 && !item.is_object() {
                            let count = 1 + random.below(3);
                            let items: Vec<Value> = (0..count).map(|_| item.clone()).collect();
                            properties
                                .insert(key.clone(), json!({"type": "array", "items": schema}));
                            value.insert(key, Value::Array(items));
                        } else {
                            properties.insert(key.clone(), schema);
                            value.insert(key, item);
                        }
                    }
                    (
                        json!({"type": "object", "properties": properties}),
                        Value::Object(value),
                    )
                }
            }
        }
        let language = Language::XML.behavior();
        let mut random = Random(7);
        let mut failures = Vec::new();
        for _ in 0..300 {
            let (schema, value) = generate(&mut random, 3);
            let schema = json!({"properties": {"r": schema}});
            let document = json!({"r": value});
            let Ok(text) = codec::serialize(language, &document, None) else {
                failures.push(format!("not written: {document}"));
                continue;
            };
            match codec::read(language, &text, Some(&schema)) {
                Ok(read) if read == document => {}
                other => failures.push(format!("{document}\nwritten {text}\nread {other:?}")),
            }
        }
        assert!(
            failures.is_empty(),
            "{} failing:\n{}",
            failures.len(),
            failures[..failures.len().min(5)].join("\n\n")
        );
    }

    /// Count: a text read by a schema again and again (each reader at a
    /// pause in typing) is parsed once while an editor is mounted, its
    /// text-scalar reading kept beside the guessed one.
    #[test]
    fn a_text_read_by_the_schema_again_is_parsed_once() {
        use crate::editor::parse_cache;
        use crate::editor::work_count::{Work, counted};
        let schema = json!({"properties": {"r": {"properties": {"v": {"type": "number"}}}}});
        let text = "<r><v>1.50</v><w>2</w></r>";
        let language = Language::XML.behavior();
        parse_cache::hold();
        let ((), bytes) = counted(Work::ParsedBytes, || {
            for _ in 0..5 {
                let read = codec::read(language, text, Some(&schema)).unwrap();
                assert_eq!(read, json!({"r": {"v": 1.5, "w": 2}}));
                let guessed = codec::read(language, text, None).unwrap();
                assert_eq!(guessed, json!({"r": {"v": "1.50", "w": 2}}));
            }
        });
        parse_cache::release();
        assert_eq!(bytes, 2 * text.len(), "one parse each way");
    }

    /// The `$ref`-reached schema of a key, and a schema whose root is the
    /// definitions' container, both type.
    #[test]
    fn references_reach_the_type() {
        let schema = json!({
            "$defs": {"port": {"type": "integer"}, "server": {"properties": {"port": {"$ref": "#/$defs/port"}}}},
            "properties": {"server": {"$ref": "#/$defs/server"}},
        });
        let value = codec::read(
            Language::XML.behavior(),
            "<server><port>0080</port></server>",
            Some(&schema),
        )
        .unwrap();
        assert_eq!(value, json!({"server": {"port": 80}}));
    }

    /// Where the schema says nothing of a type (no schema below a key, or
    /// one without types), a text reads as the language guesses it: the
    /// reading with such a schema is the reading without one, on every XML
    /// sample the save tests use.
    #[test]
    fn a_schema_saying_nothing_reads_as_the_language_guesses() {
        #[cfg(all(feature = "yaml", feature = "json", feature = "toml", feature = "hcl"))]
        let mut samples: Vec<&str> = crate::editor::save_properties::hosts()
            .into_iter()
            .filter(|(language, _)| *language == Language::XML)
            .map(|(_, text)| text)
            .collect();
        #[cfg(not(all(feature = "yaml", feature = "json", feature = "toml", feature = "hcl")))]
        let mut samples: Vec<&str> = Vec::new();
        samples.extend([
            "<r a=\"1\" b=\"x\" c=\"true\" d=\" 2 \">1.50</r>",
            "<r><n>null</n><e/><e2></e2><t> text </t><f>-0</f><g>1e3</g></r>",
            "<r config-root=\"true\"><a>1</a></r>",
            "<r><![CDATA[5]]></r>",
            "<r><a>1</a>tail<b>true</b></r>",
            "<a>1</a><a>2</a><b x=\"3\"><x>4</x></b>",
        ]);
        let language = Language::XML.behavior();
        for schema in [
            json!({}),
            json!({"description": "none"}),
            json!({"properties": {"zzz": {"type": "integer"}}}),
        ] {
            for sample in &samples {
                let guessed = language.parse(sample).expect("parses");
                let mut typed = language.parse_text(sample).expect("parses");
                type_texts(&mut typed, &schema, &schema, language);
                assert_eq!(typed, guessed, "{sample} by {schema}");
            }
        }
    }
}
