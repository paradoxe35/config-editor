//! The schema compiler hook: formats by name and path, a host's compiler
//! with its resources, options and losses, the built-in formats, and how
//! a compile error is worded.

use std::sync::Mutex;

use serde_json::{Value, json};

use super::{
    CompiledSchema, LossEffect, SchemaCompiler, SchemaFormat, SchemaFormatInfo, SchemaInput,
    SchemaLoss, SchemaResources,
};
use crate::editor::diagnostics::{Schema, SchemaError, SchemaSource};

/// `name: type` lines; `use: file` lines take the lines of a resource;
/// `lose: construct` lines report a loss there. Its options are copied into
/// the schema under `x-options`.
struct Fields;

impl SchemaCompiler for Fields {
    fn info(&self) -> SchemaFormatInfo {
        SchemaFormatInfo {
            name: "fields",
            label: "Fields",
            extensions: &["fields"],
        }
    }

    fn compile(&self, input: &SchemaInput<'_>) -> Result<CompiledSchema, SchemaError> {
        let mut properties = serde_json::Map::new();
        let mut losses = Vec::new();
        let mut lines: Vec<(Option<String>, usize, String)> = input
            .text
            .lines()
            .enumerate()
            .map(|(index, line)| (None, index + 1, line.to_string()))
            .collect();
        let mut index = 0;
        while index < lines.len() {
            let (file, number, line) = lines[index].clone();
            index += 1;
            let Some((name, value)) = line.split_once(':') else {
                continue;
            };
            let value = value.trim();
            match name.trim() {
                "use" => {
                    let text = input
                        .resources
                        .get(value)
                        .ok_or_else(|| SchemaError(format!("no resource \"{value}\"")))?;
                    let included = text
                        .lines()
                        .enumerate()
                        .map(|(at, line)| (Some(value.to_string()), at + 1, line.to_string()));
                    lines.splice(index..index, included);
                }
                "lose" => {
                    let mut loss = SchemaLoss::new(
                        value.to_string(),
                        LossEffect::NotChecked,
                        format!("{value} is not checked"),
                    )
                    .at(number, 1);
                    if let Some(file) = file {
                        loss = loss.in_file(file);
                    }
                    losses.push(loss);
                }
                field => {
                    properties.insert(field.to_string(), json!({ "type": value }));
                }
            }
        }
        let mut schema = json!({ "properties": properties });
        if !input.options.is_empty() {
            schema["x-options"] = Value::Object(input.options.clone());
        }
        Ok(CompiledSchema::new(schema).with_losses(losses))
    }

    fn imports(&self, text: &str) -> Vec<String> {
        text.lines()
            .filter_map(|line| line.strip_prefix("use:"))
            .map(|file| file.trim().to_string())
            .collect()
    }
}

static FIELDS: Fields = Fields;
const FIELDS_FORMAT: SchemaFormat = SchemaFormat::new(&FIELDS);

#[test]
fn a_host_compiler_gets_its_resources_and_options_and_reports_its_losses() {
    let resources = SchemaResources::new().with("common.fields", "name: string\nlose: unique\n");
    let mut options = serde_json::Map::new();
    options.insert("root".to_string(), json!("server"));
    let text = "port: integer\nuse: common.fields\nlose: order\n";
    let input = SchemaInput::new(text)
        .resources(&resources)
        .options(&options);
    let schema = Schema::compile_as(FIELDS_FORMAT, &input).expect("compiles");
    assert_eq!(
        schema.json(),
        &json!({
            "properties": {"port": {"type": "integer"}, "name": {"type": "string"}},
            "x-options": {"root": "server"},
        })
    );
    let losses: Vec<String> = schema.losses().iter().map(ToString::to_string).collect();
    assert_eq!(
        losses,
        [
            "common.fields:2:1: unique is not checked",
            "3:1: order is not checked"
        ]
    );
    assert_eq!(schema.losses()[0].construct, "unique");
    assert_eq!(schema.losses()[0].effect, LossEffect::NotChecked);
    // Losses are warnings too, so `on_schema_warning` reports them.
    assert_eq!(schema.warnings(), losses.as_slice());
    assert_eq!(
        FIELDS_FORMAT.imports(text),
        ["common.fields"],
        "the host learns what to fetch"
    );
    assert_eq!(
        SchemaFormat::JSON_SCHEMA.imports(text),
        Vec::<String>::new()
    );
}

#[test]
fn a_missing_resource_and_a_refused_text_say_their_format() {
    let error = Schema::compile_as(FIELDS_FORMAT, &SchemaInput::new("use: gone.fields\n"))
        .expect_err("refused");
    assert_eq!(error.0, "Invalid Fields: no resource \"gone.fields\"");
    let error =
        Schema::compile_as(SchemaFormat::JSON_SCHEMA, &SchemaInput::new("{")).expect_err("refused");
    assert!(error.0.starts_with("Invalid JSON Schema: "), "{}", error.0);
}

#[test]
fn a_loss_is_worded_with_as_much_of_its_place_as_is_known() {
    let loss = || SchemaLoss::new("xs:key", LossEffect::Loosened, "xs:key is not checked");
    let cases = [
        (loss(), "xs:key is not checked"),
        (loss().at(4, 9), "4:9: xs:key is not checked"),
        (loss().in_file("a.xsd"), "a.xsd: xs:key is not checked"),
        (
            loss().in_file("a.xsd").at(4, 9),
            "a.xsd:4:9: xs:key is not checked",
        ),
    ];
    for (loss, expected) in cases {
        assert_eq!(loss.to_string(), expected);
    }
    for effect in [
        LossEffect::NotChecked,
        LossEffect::Loosened,
        LossEffect::Approximated,
    ] {
        assert_eq!(effect.as_str().parse::<LossEffect>(), Ok(effect));
    }
    assert!("lost".parse::<LossEffect>().is_err());
}

#[test]
fn a_built_in_format_is_found_by_name_and_by_the_longest_extension() {
    assert_eq!(
        "json-schema".parse::<SchemaFormat>(),
        Ok(SchemaFormat::JSON_SCHEMA)
    );
    assert_eq!(
        " JSON-Schema ".parse::<SchemaFormat>(),
        Ok(SchemaFormat::JSON_SCHEMA)
    );
    let unknown = "jtd".parse::<SchemaFormat>().expect_err("not built in");
    assert!(
        unknown
            .to_string()
            .starts_with("unknown schema format \"jtd\" (expected json-schema"),
        "{unknown}"
    );
    let cases: &[(&str, Option<SchemaFormat>)] = &[
        ("schemas/app.json", Some(SchemaFormat::JSON_SCHEMA)),
        ("APP.SCHEMA.JSON", Some(SchemaFormat::JSON_SCHEMA)),
        ("json", None),
        (".json", None),
        ("dir.json/app", None),
        ("app.jsonc", None),
        ("app.toml", None),
    ];
    for (path, expected) in cases {
        assert_eq!(SchemaFormat::from_path(path), *expected, "{path}");
    }
    for format in SchemaFormat::ALL {
        let info = format.info();
        assert!(!info.name.is_empty() && !info.label.is_empty());
        for extension in info.extensions {
            assert_eq!(*extension, extension.to_ascii_lowercase(), "{format:?}");
            assert!(!extension.starts_with('.'), "{format:?}");
            assert_eq!(
                SchemaFormat::from_path(&format!("a.{extension}")),
                Some(*format)
            );
        }
        assert_eq!(format.name().parse::<SchemaFormat>(), Ok(*format));
    }
    assert_eq!(SchemaFormat::default(), SchemaFormat::JSON_SCHEMA);
    assert_ne!(FIELDS_FORMAT, SchemaFormat::JSON_SCHEMA);
    assert_eq!(FIELDS_FORMAT.to_string(), "Fields");
}

#[cfg(feature = "yaml")]
#[test]
fn json_schema_written_in_yaml_compiles_and_a_yaml_error_says_where() {
    let yaml = "properties:\n  listen: {type: integer}\nrequired: [listen]\n";
    let schema = Schema::compile_as(SchemaFormat::JSON_SCHEMA_YAML, &SchemaInput::new(yaml))
        .expect("compiles");
    assert_eq!(
        schema.json(),
        &json!({"properties": {"listen": {"type": "integer"}}, "required": ["listen"]})
    );
    assert!(schema.losses().is_empty());
    let cases: &[(&str, Option<SchemaFormat>)] = &[
        ("k8s/crd.yaml", Some(SchemaFormat::JSON_SCHEMA_YAML)),
        ("app.schema.yml", Some(SchemaFormat::JSON_SCHEMA_YAML)),
    ];
    for (path, expected) in cases {
        assert_eq!(SchemaFormat::from_path(path), *expected, "{path}");
    }
    assert_eq!(
        "json-schema-yaml".parse::<SchemaFormat>(),
        Ok(SchemaFormat::JSON_SCHEMA_YAML)
    );
    let error = Schema::compile_as(
        SchemaFormat::JSON_SCHEMA_YAML,
        &SchemaInput::new("properties:\n  a: {type: [\n"),
    )
    .expect_err("refused");
    assert!(
        error.0.starts_with("Invalid JSON Schema (YAML): ") && error.0.contains("(line "),
        "{}",
        error.0
    );
}

#[cfg(all(feature = "yaml", feature = "schema-validation"))]
#[test]
fn a_yaml_schema_validates_documents_like_its_json_twin() {
    use crate::editor::Language;
    let yaml = "properties:\n  listen: {type: integer}\nadditionalProperties: false\n";
    let json = r#"{"properties":{"listen":{"type":"integer"}},"additionalProperties":false}"#;
    let from_yaml =
        Schema::compile_as(SchemaFormat::JSON_SCHEMA_YAML, &SchemaInput::new(yaml)).unwrap();
    let from_json = Schema::compile(json).unwrap();
    for document in ["listen: 80\n", "listen: x\n", "other: 1\n"] {
        assert_eq!(
            from_yaml.validate(Language::YAML, document),
            from_json.validate(Language::YAML, document),
            "{document}"
        );
    }
}

/// What a compiler gives must still be a valid JSON Schema: that error is
/// worded as the JSON Schema's.
#[cfg(feature = "schema-validation")]
#[test]
fn a_compiler_giving_an_invalid_json_schema_is_refused_as_json_schema() {
    let error = Schema::compile_as(FIELDS_FORMAT, &SchemaInput::new("port: 5\n"))
        .expect_err("type 5 is no type");
    assert!(error.0.starts_with("Invalid JSON Schema: "), "{}", error.0);
}

/// `json()` gives the schema as the compiler gave it, even where the editor
/// rewrote a reference for completion (an `$anchor`).
#[test]
fn the_json_is_the_compilers_even_where_references_are_rewritten() {
    let text = r##"{"$defs":{"port":{"$anchor":"port","type":"integer"}},"properties":{"p":{"$ref":"#port"}}}"##;
    let schema = Schema::compile(text).unwrap();
    assert_eq!(schema.json(), &serde_json::from_str::<Value>(text).unwrap());
    assert_eq!(
        schema.bundle.schema["properties"]["p"]["$ref"], "#/$defs/port",
        "completion reads the pointer"
    );
}

/// A `Formatted` source is compiled when the editor mounts; a compile
/// error is shown inline, worded with the format.
#[test]
fn a_formatted_source_resolves_and_its_error_names_the_format() {
    let source = SchemaSource::formatted(FIELDS_FORMAT, "port: integer\nlose: order\n");
    let bundle = source.resolve().expect("compiles");
    assert_eq!(bundle.warnings, ["2:1: order is not checked"]);
    let failure = SchemaSource::formatted(FIELDS_FORMAT, "use: gone\n")
        .resolve()
        .err()
        .expect("refused");
    assert_eq!(
        failure.message(crate::strings::default_strings()),
        "Invalid Fields: no resource \"gone\""
    );
    let failure = SchemaSource::Json("{".to_string())
        .resolve()
        .err()
        .expect("refused");
    assert!(
        failure
            .message(crate::strings::default_strings())
            .starts_with("Invalid JSON Schema: ")
    );
}

/// Two sources compile to one schema exactly when their format, text,
/// resources and options are the same; JSON text is JSON Schema.
#[test]
fn sources_share_a_key_only_when_everything_they_compile_from_is_the_same() {
    let plain = SchemaSource::formatted(FIELDS_FORMAT, "a: string\n");
    let with_resources = SchemaSource::Formatted {
        format: FIELDS_FORMAT,
        text: "a: string\n".to_string(),
        resources: SchemaResources::new().with("x", "b: string\n"),
        options: serde_json::Map::new(),
    };
    let mut options = serde_json::Map::new();
    options.insert("root".to_string(), json!("a"));
    let with_options = SchemaSource::Formatted {
        format: FIELDS_FORMAT,
        text: "a: string\n".to_string(),
        resources: SchemaResources::new(),
        options,
    };
    assert_eq!(plain.key(), plain.clone().key());
    assert_ne!(plain.key(), with_resources.key());
    assert_ne!(plain.key(), with_options.key());
    assert_ne!(
        plain.key(),
        SchemaSource::formatted(SchemaFormat::JSON_SCHEMA, "a: string\n").key()
    );
    assert_eq!(
        SchemaSource::Json("{}".into()).key(),
        SchemaSource::formatted(SchemaFormat::JSON_SCHEMA, "{}").key()
    );
    let compiled = Schema::compile("{}").unwrap();
    assert_eq!(SchemaSource::Compiled(compiled).key(), None);
}

/// The doc example's compiler, run by many threads at once (the trait is
/// `Send + Sync`): each compile sees only its own input.
#[test]
fn a_compiler_is_shared_between_threads() {
    let results = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for index in 0..4 {
            let results = &results;
            scope.spawn(move || {
                let text = format!("f{index}: integer\n");
                let schema = Schema::compile_as(FIELDS_FORMAT, &SchemaInput::new(&text)).unwrap();
                let names: Vec<String> = schema.json()["properties"]
                    .as_object()
                    .unwrap()
                    .keys()
                    .cloned()
                    .collect();
                results.lock().unwrap().push((index, names));
            });
        }
    });
    for (index, names) in results.into_inner().unwrap() {
        assert_eq!(names, [format!("f{index}")]);
    }
}
