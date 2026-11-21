//! The XSD compiler, construct by construct: each mapping of the design
//! (section 3.4) compiles, reads a sample document as the type it gives
//! and gives the validator's verdict; each construct it cannot express is
//! the exact loss (construct and line); errors name where they are.

use serde_json::{Value, json};

use super::super::{LossEffect, SchemaFormat, SchemaInput};
use crate::editor::schema::Schema;

/// `body` inside an `xs:schema` element (on line 2 on: line 1 is the
/// schema's start tag).
fn xsd(body: &str) -> String {
    format!("<xs:schema xmlns:xs=\"http://www.w3.org/2001/XMLSchema\">\n{body}\n</xs:schema>\n")
}

fn compile(text: &str) -> Schema {
    compile_with(text, serde_json::Map::new())
}

fn compile_with(text: &str, options: serde_json::Map<String, Value>) -> Schema {
    Schema::compile_as(SchemaFormat::XSD, &SchemaInput::new(text).options(&options))
        .unwrap_or_else(|error| panic!("{}\n{text}", error.0))
}

/// `document` as the XML language reads it by `schema`.
#[cfg(all(feature = "xml", feature = "schema-validation"))]
fn read(schema: &Schema, document: &str) -> Value {
    crate::editor::codec::read(
        crate::editor::Language::XML.behavior(),
        document,
        Some(&schema.bundle.schema),
    )
    .expect("parses")
}

/// The validator's messages on `document`.
#[cfg(all(feature = "xml", feature = "schema-validation"))]
fn problems(schema: &Schema, document: &str) -> Vec<String> {
    schema
        .validate(crate::editor::Language::XML, document)
        .into_iter()
        .map(|problem| problem.message)
        .collect()
}

/// One row of the matrix: a schema body, a document, the value read, and
/// whether the document is valid.
#[cfg(all(feature = "xml", feature = "schema-validation"))]
struct Row {
    name: &'static str,
    body: &'static str,
    document: &'static str,
    value: Value,
    valid: bool,
}

#[cfg(all(feature = "xml", feature = "schema-validation"))]
#[test]
fn each_construct_reads_and_validates_as_xsd_says() {
    let rows = vec![
        Row {
            name: "built-in integer, typed",
            body: r#"<xs:element name="port" type="xs:int"/>"#,
            document: "<port> 0080 </port>",
            value: json!({"port": 80}),
            valid: true,
        },
        Row {
            name: "int bounds",
            body: r#"<xs:element name="port" type="xs:unsignedShort"/>"#,
            document: "<port>70000</port>",
            value: json!({"port": 70000}),
            valid: false,
        },
        Row {
            name: "decimal keeps its value",
            body: r#"<xs:element name="price" type="xs:decimal"/>"#,
            document: "<price>1.50</price>",
            value: json!({"price": 1.5}),
            valid: true,
        },
        Row {
            name: "boolean in XSD spellings",
            body: r#"<xs:element name="on" type="xs:boolean"/>"#,
            document: "<on>1</on>",
            value: json!({"on": true}),
            valid: true,
        },
        Row {
            name: "boolean refused",
            body: r#"<xs:element name="on" type="xs:boolean"/>"#,
            document: "<on>yes</on>",
            value: json!({"on": "yes"}),
            valid: false,
        },
        Row {
            name: "string keeps text that looks like a number",
            body: r#"<xs:element name="v" type="xs:string"/>"#,
            document: "<v>007</v>",
            value: json!({"v": "007"}),
            valid: true,
        },
        Row {
            name: "token collapses whitespace",
            body: r#"<xs:element name="v" type="xs:token"/>"#,
            document: "<v>  a   b </v>",
            value: json!({"v": "a b"}),
            valid: true,
        },
        Row {
            name: "enumeration",
            body: r#"<xs:element name="mode"><xs:simpleType><xs:restriction base="xs:string"><xs:enumeration value="tcp"/><xs:enumeration value="udp"/></xs:restriction></xs:simpleType></xs:element>"#,
            document: "<mode>http</mode>",
            value: json!({"mode": "http"}),
            valid: false,
        },
        Row {
            name: "numeric enumeration typed",
            body: r#"<xs:element name="level"><xs:simpleType><xs:restriction base="xs:integer"><xs:enumeration value="1"/><xs:enumeration value="2"/></xs:restriction></xs:simpleType></xs:element>"#,
            document: "<level>02</level>",
            value: json!({"level": 2}),
            valid: true,
        },
        Row {
            name: "pattern is anchored",
            body: r#"<xs:element name="code"><xs:simpleType><xs:restriction base="xs:string"><xs:pattern value="[A-Z]{2}"/></xs:restriction></xs:simpleType></xs:element>"#,
            document: "<code>XYZ</code>",
            value: json!({"code": "XYZ"}),
            valid: false,
        },
        Row {
            name: "length facets",
            body: r#"<xs:element name="pin"><xs:simpleType><xs:restriction base="xs:string"><xs:length value="4"/></xs:restriction></xs:simpleType></xs:element>"#,
            document: "<pin>12345</pin>",
            value: json!({"pin": "12345"}),
            valid: false,
        },
        Row {
            name: "inclusive bounds on a named type",
            body: r#"<xs:simpleType name="percent"><xs:restriction base="xs:decimal"><xs:minInclusive value="0"/><xs:maxInclusive value="100"/></xs:restriction></xs:simpleType><xs:element name="load" type="percent"/>"#,
            document: "<load>100.5</load>",
            value: json!({"load": 100.5}),
            valid: false,
        },
        Row {
            name: "union",
            body: r#"<xs:element name="size"><xs:simpleType><xs:union memberTypes="xs:integer"><xs:simpleType><xs:restriction base="xs:string"><xs:enumeration value="auto"/></xs:restriction></xs:simpleType></xs:union></xs:simpleType></xs:element>"#,
            document: "<size>auto</size>",
            value: json!({"size": "auto"}),
            valid: true,
        },
        Row {
            name: "sequence: required and optional children",
            body: r#"<xs:element name="server"><xs:complexType><xs:sequence><xs:element name="host" type="xs:string"/><xs:element name="port" type="xs:int" minOccurs="0"/></xs:sequence></xs:complexType></xs:element>"#,
            document: "<server><port>1</port></server>",
            value: json!({"server": {"port": 1}}),
            valid: false,
        },
        Row {
            name: "closed content model",
            body: r#"<xs:element name="server"><xs:complexType><xs:sequence><xs:element name="host" type="xs:string"/></xs:sequence></xs:complexType></xs:element>"#,
            document: "<server><host>h</host><hots>x</hots></server>",
            value: json!({"server": {"host": "h", "hots": "x"}}),
            valid: false,
        },
        Row {
            name: "a lone repeated element is a one-item list",
            body: r#"<xs:element name="servers"><xs:complexType><xs:sequence><xs:element name="server" type="xs:string" maxOccurs="unbounded"/></xs:sequence></xs:complexType></xs:element>"#,
            document: "<servers><server>a</server></servers>",
            value: json!({"servers": {"server": ["a"]}}),
            valid: true,
        },
        Row {
            name: "maxOccurs counts",
            body: r#"<xs:element name="l"><xs:complexType><xs:sequence><xs:element name="i" type="xs:int" maxOccurs="2"/></xs:sequence></xs:complexType></xs:element>"#,
            document: "<l><i>1</i><i>2</i><i>3</i></l>",
            value: json!({"l": {"i": [1, 2, 3]}}),
            valid: false,
        },
        Row {
            name: "attributes are properties, typed, required and defaulted",
            body: r#"<xs:element name="server"><xs:complexType><xs:attribute name="port" type="xs:int" use="required"/><xs:attribute name="tls" type="xs:boolean" default="false"/></xs:complexType></xs:element>"#,
            document: "<server port=\"08\" tls=\"0\"/>",
            value: json!({"server": {"port": 8, "tls": false}}),
            valid: true,
        },
        Row {
            name: "a required attribute missing",
            body: r#"<xs:element name="server"><xs:complexType><xs:attribute name="port" type="xs:int" use="required"/></xs:complexType></xs:element>"#,
            document: "<server/>",
            value: json!({"server": {}}),
            valid: false,
        },
        Row {
            name: "an attribute clashing with a child element is @name",
            body: r#"<xs:element name="r"><xs:complexType><xs:sequence><xs:element name="a" type="xs:string"/></xs:sequence><xs:attribute name="a" type="xs:int"/></xs:complexType></xs:element>"#,
            document: "<r a=\"1\"><a>x</a></r>",
            value: json!({"r": {"@a": 1, "a": "x"}}),
            valid: true,
        },
        Row {
            name: "simple content with attributes: the text alone",
            body: r#"<xs:element name="price"><xs:complexType><xs:simpleContent><xs:extension base="xs:decimal"><xs:attribute name="currency" type="xs:string"/></xs:extension></xs:simpleContent></xs:complexType></xs:element>"#,
            document: "<price>1.50</price>",
            value: json!({"price": 1.5}),
            valid: true,
        },
        Row {
            name: "simple content with attributes: text and attribute",
            body: r#"<xs:element name="price"><xs:complexType><xs:simpleContent><xs:extension base="xs:decimal"><xs:attribute name="currency" type="xs:string"/></xs:extension></xs:simpleContent></xs:complexType></xs:element>"#,
            document: "<price currency=\"EUR\">1.50</price>",
            value: json!({"price": {"currency": "EUR", "#text": 1.5}}),
            valid: true,
        },
        Row {
            name: "an empty element of optional children",
            body: r#"<xs:element name="opts"><xs:complexType><xs:sequence><xs:element name="a" type="xs:int" minOccurs="0"/></xs:sequence></xs:complexType></xs:element>"#,
            document: "<opts/>",
            value: json!({"opts": {}}),
            valid: true,
        },
        Row {
            name: "extension flattens the base's children and attributes",
            body: r#"<xs:complexType name="base"><xs:sequence><xs:element name="id" type="xs:int"/></xs:sequence><xs:attribute name="v" type="xs:int"/></xs:complexType><xs:element name="item"><xs:complexType><xs:complexContent><xs:extension base="base"><xs:sequence><xs:element name="name" type="xs:string"/></xs:sequence></xs:extension></xs:complexContent></xs:complexType></xs:element>"#,
            document: "<item v=\"2\"><id>1</id><name>n</name></item>",
            value: json!({"item": {"v": 2, "id": 1, "name": "n"}}),
            valid: true,
        },
        Row {
            name: "groups and attribute groups",
            body: r#"<xs:group name="g"><xs:sequence><xs:element name="a" type="xs:int"/></xs:sequence></xs:group><xs:attributeGroup name="ag"><xs:attribute name="b" type="xs:int"/></xs:attributeGroup><xs:element name="r"><xs:complexType><xs:sequence><xs:group ref="g"/></xs:sequence><xs:attributeGroup ref="ag"/></xs:complexType></xs:element>"#,
            document: "<r b=\"2\"><a>1</a></r>",
            value: json!({"r": {"b": 2, "a": 1}}),
            valid: true,
        },
        Row {
            name: "element references and recursion",
            body: r#"<xs:element name="node"><xs:complexType><xs:sequence><xs:element ref="node" minOccurs="0" maxOccurs="unbounded"/></xs:sequence><xs:attribute name="id" type="xs:int"/></xs:complexType></xs:element>"#,
            document: "<node id=\"1\"><node id=\"2\"/></node>",
            value: json!({"node": {"id": 1, "node": [{"id": 2}]}}),
            valid: true,
        },
        Row {
            name: "a single choice: two options refused",
            body: r#"<xs:element name="auth"><xs:complexType><xs:choice><xs:element name="token" type="xs:string"/><xs:element name="password" type="xs:string"/></xs:choice></xs:complexType></xs:element>"#,
            document: "<auth><token>t</token><password>p</password></auth>",
            value: json!({"auth": {"token": "t", "password": "p"}}),
            valid: false,
        },
        Row {
            name: "a single choice: one option",
            body: r#"<xs:element name="auth"><xs:complexType><xs:choice><xs:element name="token" type="xs:string"/><xs:element name="password" type="xs:string"/></xs:choice></xs:complexType></xs:element>"#,
            document: "<auth><password>p</password></auth>",
            value: json!({"auth": {"password": "p"}}),
            valid: true,
        },
        Row {
            name: "element default and fixed are typed",
            body: r#"<xs:element name="r"><xs:complexType><xs:sequence><xs:element name="n" type="xs:int" default="5"/><xs:element name="v" type="xs:string" fixed="1.0"/></xs:sequence></xs:complexType></xs:element>"#,
            document: "<r><n>5</n><v>2.0</v></r>",
            value: json!({"r": {"n": 5, "v": "2.0"}}),
            valid: false,
        },
        Row {
            name: "every global element may start a document (a fragment)",
            body: r#"<xs:element name="a" type="xs:int"/><xs:element name="b" type="xs:int"/>"#,
            document: "<a>1</a>\n<b>2</b>",
            value: json!({"a": 1, "b": 2}),
            valid: true,
        },
        Row {
            name: "anyAttribute ##other: a prefixed attribute only",
            body: r###"<xs:element name="bean"><xs:complexType><xs:attribute name="id" type="xs:string"/><xs:anyAttribute namespace="##other"/></xs:complexType></xs:element>"###,
            document: "<bean xmlns:p=\"urn:p\" id=\"a\" p:x=\"1\"/>",
            value: json!({"bean": {"id": "a", "p:x": 1}}),
            valid: true,
        },
        Row {
            name: "anyAttribute ##other: an unprefixed attribute refused",
            body: r###"<xs:element name="bean"><xs:complexType><xs:attribute name="id" type="xs:string"/><xs:anyAttribute namespace="##other"/></xs:complexType></xs:element>"###,
            document: "<bean klass=\"a\"/>",
            value: json!({"bean": {"klass": "a"}}),
            valid: false,
        },
        Row {
            name: "namespace declarations and xsi attributes are markup",
            body: r#"<xs:element name="project"><xs:complexType><xs:sequence><xs:element name="v" type="xs:int"/></xs:sequence></xs:complexType></xs:element>"#,
            document: "<project xmlns=\"urn:p\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xsi:schemaLocation=\"urn:p p.xsd\"><v>1</v></project>",
            value: json!({"project": {"v": 1}}),
            valid: true,
        },
    ];
    let mut failures = Vec::new();
    for row in rows {
        let text = xsd(row.body);
        let schema = match Schema::compile_as(SchemaFormat::XSD, &SchemaInput::new(&text)) {
            Ok(schema) => schema,
            Err(error) => {
                failures.push(format!("{}: {}", row.name, error.0));
                continue;
            }
        };
        let value = read(&schema, row.document);
        if value != row.value {
            failures.push(format!("{}: reads {value}, want {}", row.name, row.value));
        }
        let found = problems(&schema, row.document);
        if found.is_empty() != row.valid {
            failures.push(format!(
                "{}: problems {found:?}, want valid = {}\n{}",
                row.name,
                row.valid,
                schema.json()
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// What XSD says that JSON Schema does not: one loss each, with its
/// construct, effect and line.
#[test]
fn each_construct_with_no_json_schema_form_is_its_loss() {
    let cases: &[(&str, &str, LossEffect, usize)] = &[
        (
            r#"<xs:element name="r"><xs:complexType><xs:sequence><xs:element name="a" type="xs:string"/></xs:sequence></xs:complexType>
<xs:key name="k"><xs:selector xpath="a[1]"/><xs:field xpath="."/></xs:key></xs:element>"#,
            "xs:key",
            LossEffect::NotChecked,
            3,
        ),
        (
            r#"<xs:element name="r"><xs:complexType><xs:sequence><xs:element name="a" type="xs:string"/><xs:element name="b" type="xs:string"/></xs:sequence></xs:complexType></xs:element>"#,
            "xs:sequence order",
            LossEffect::NotChecked,
            2,
        ),
        (
            r#"<xs:element name="r"><xs:complexType><xs:sequence>
<xs:any processContents="lax"/></xs:sequence></xs:complexType></xs:element>"#,
            "xs:any",
            LossEffect::Loosened,
            3,
        ),
        (
            r#"<xs:element name="r"><xs:complexType>
<xs:anyAttribute/></xs:complexType></xs:element>"#,
            "xs:anyAttribute",
            LossEffect::Loosened,
            3,
        ),
        (
            r#"<xs:element name="v"><xs:simpleType><xs:restriction base="xs:decimal">
<xs:totalDigits value="5"/></xs:restriction></xs:simpleType></xs:element>"#,
            "xs:totalDigits",
            LossEffect::NotChecked,
            3,
        ),
        (
            r#"<xs:element name="v"><xs:simpleType><xs:restriction base="xs:string">
<xs:pattern value="\p{IsBasicLatin}+"/></xs:restriction></xs:simpleType></xs:element>"#,
            "xs:pattern",
            LossEffect::NotChecked,
            3,
        ),
        (
            r#"<xs:element name="v">
<xs:simpleType><xs:list itemType="xs:int"/></xs:simpleType></xs:element>"#,
            "xs:list",
            LossEffect::Loosened,
            3,
        ),
        (
            r#"
<xs:element name="v" type="xs:double"/>"#,
            "xs:float INF/NaN",
            LossEffect::Approximated,
            3,
        ),
        (
            r#"
<xs:element name="v" type="xs:string" nillable="true"/>"#,
            "nillable",
            LossEffect::NotChecked,
            3,
        ),
        (
            r#"
<xs:notation name="gif" public="image/gif"/>"#,
            "xs:notation",
            LossEffect::NotChecked,
            3,
        ),
    ];
    let mut failures = Vec::new();
    for (body, construct, effect, line) in cases {
        let schema = compile(&xsd(body));
        let found: Vec<_> = schema
            .losses()
            .iter()
            .filter(|loss| loss.construct == *construct)
            .collect();
        match found.as_slice() {
            [loss] if loss.effect == *effect && loss.line == Some(*line) => {}
            other => failures.push(format!(
                "{construct}: {other:?}, want one {effect:?} on line {line}"
            )),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn the_options_choose_the_root_and_the_closedness() {
    let body = r#"<xs:element name="a"><xs:complexType><xs:sequence><xs:element name="x" type="xs:int"/></xs:sequence></xs:complexType></xs:element><xs:element name="b" type="xs:int"/>"#;
    let text = xsd(body);
    let all = compile(&text);
    let properties = |schema: &Schema| -> Vec<String> {
        schema.json()["properties"]
            .as_object()
            .map(|map| map.keys().cloned().collect())
            .unwrap_or_default()
    };
    assert_eq!(properties(&all), ["a", "b"]);
    assert_eq!(all.json()["additionalProperties"], json!(false));
    let mut options = serde_json::Map::new();
    options.insert("root".to_string(), json!("a"));
    let root = compile_with(&text, options);
    assert_eq!(properties(&root), ["a"]);
    assert_eq!(root.json()["required"], json!(["a"]));
    let mut options = serde_json::Map::new();
    options.insert("lax".to_string(), json!(true));
    let lax = compile_with(&text, options);
    assert!(lax.json().get("additionalProperties").is_none());
    assert!(
        lax.json()["$defs"]["element.a"]
            .get("additionalProperties")
            .is_none()
    );
    let mut options = serde_json::Map::new();
    options.insert("configRoot".to_string(), json!("a"));
    let config = compile_with(&text, options);
    assert_eq!(config.json()["properties"]["x"]["type"], json!("integer"));
    for (key, value, message) in [
        (
            "root",
            json!("zz"),
            "root: the schema declares no global element \"zz\"",
        ),
        (
            "configRoot",
            json!("zz"),
            "configRoot: the schema declares no global element \"zz\"",
        ),
        (
            "lax",
            json!("yes"),
            "the option \"lax\" must be true or false",
        ),
        (
            "rooot",
            json!("a"),
            "unknown option \"rooot\" (expected root, configRoot, lax)",
        ),
    ] {
        let mut options = serde_json::Map::new();
        options.insert(key.to_string(), value);
        let error = Schema::compile_as(
            SchemaFormat::XSD,
            &SchemaInput::new(&text).options(&options),
        )
        .expect_err(key);
        assert_eq!(error.0, format!("Invalid XSD: {message}"));
    }
}

#[test]
fn errors_say_where_they_are() {
    let cases = [
        ("<xs:schema", "Invalid XSD: "),
        ("<root/>", "the document is no XML Schema"),
        (
            &*xsd(r#"<xs:element name="a" type="nope"/>"#),
            "the type \"nope\" is not defined (line 2, column 1)",
        ),
        (
            &*xsd(
                r#"<xs:element name="a"><xs:complexType><xs:sequence><xs:element ref="b"/></xs:sequence></xs:complexType></xs:element>"#,
            ),
            "the element \"b\" is not declared (line 2, column",
        ),
        (
            &*xsd(
                r#"<xs:complexType name="t"><xs:complexContent><xs:extension base="t"/></xs:complexContent></xs:complexType><xs:element name="a" type="t"/>"#,
            ),
            "derives from itself",
        ),
    ];
    for (text, expected) in cases {
        let error = Schema::compile_as(SchemaFormat::XSD, &SchemaInput::new(text)).expect_err(text);
        assert!(error.0.contains(expected), "{text}: {}", error.0);
    }
}

#[test]
fn imports_lists_the_files_it_refers_to() {
    let text = xsd(
        r#"<xs:include schemaLocation="common.xsd"/><xs:import namespace="urn:x" schemaLocation="x.xsd"/><xs:import namespace="urn:y"/>"#,
    );
    assert_eq!(SchemaFormat::XSD.imports(&text), ["common.xsd", "x.xsd"]);
    assert_eq!(SchemaFormat::XSD.imports("not xml <"), Vec::<String>::new());
    assert_eq!(
        SchemaFormat::from_path("schemas/pom.xsd"),
        Some(SchemaFormat::XSD)
    );
    assert_eq!("xsd".parse::<SchemaFormat>(), Ok(SchemaFormat::XSD));
}

/// Count: a named type every element uses, and a base several types
/// extend, are each read once.
#[test]
fn each_named_type_is_read_once() {
    let mut body = String::from(
        r#"<xs:complexType name="base"><xs:sequence><xs:element name="id" type="xs:int"/></xs:sequence></xs:complexType>"#,
    );
    for index in 0..20 {
        body.push_str(&format!(
            r#"<xs:complexType name="t{index}"><xs:complexContent><xs:extension base="base"><xs:sequence><xs:element name="e{index}" type="base"/></xs:sequence></xs:extension></xs:complexContent></xs:complexType><xs:element name="g{index}" type="t{index}"/>"#
        ));
    }
    let text = xsd(&body);
    let root = super::tree::read(&text).unwrap();
    let mut compiler =
        super::compile::Compiler::new(&root, super::compile::Settings { lax: false });
    for element in compiler.global_elements(&[&root]) {
        let name = element.attribute("name").unwrap().to_string();
        compiler.global_element(&name, element).unwrap();
    }
    compiler.finish(&root).unwrap();
    assert_eq!(compiler.models_read, 21, "base once, each t once");
}

/// An element that may repeat is suggested again where it is written
/// already, as long as its maximum leaves room: the written items are
/// read as the schema reads them (as validation does), so an XSD boolean
/// spelled `1` counts as a valid item.
#[cfg(feature = "xml")]
#[test]
fn a_repeated_element_written_once_is_suggested_again() {
    for (max, offered_again) in [("unbounded", true), ("2", true), ("1", false)] {
        let body = format!(
            r#"<xs:element name="config"><xs:complexType><xs:sequence><xs:element name="host" type="xs:string"/><xs:element name="mode" maxOccurs="{max}" type="xs:boolean"/></xs:sequence></xs:complexType></xs:element>"#
        );
        let schema = compile(&xsd(&body));
        // `1` is an XSD boolean: the list written so far is valid.
        let text = "<config>\n  <host>x</host>\n  <mode>1</mode>\n  <\n</config>\n";
        let caret = text.find("  <\n").unwrap() + 3;
        let labels: Vec<String> = schema
            .suggestions(crate::editor::Language::XML, text, caret)
            .iter()
            .map(|option| option.label().to_string())
            .collect();
        assert_eq!(
            labels.iter().any(|label| label == "mode"),
            offered_again,
            "maxOccurs {max}: {labels:?}"
        );
    }
}

/// A fill writes a required attribute as an attribute, beside required
/// children or alone, at the root or deeper, with a named type or not.
#[cfg(feature = "xml")]
#[test]
fn a_fill_writes_a_required_attribute_as_an_attribute() {
    let attribute = r#"<xs:attribute name="on" type="xs:int" use="required"/>"#;
    let children = r#"<xs:sequence><xs:element name="item" type="xs:string"/></xs:sequence>"#;
    let mut failures = Vec::new();
    for with_children in [false, true] {
        let content = if with_children { children } else { "" };
        for named in [false, true] {
            let (inline, globals) = if named {
                (
                    r#" type="t""#.to_string(),
                    format!(r#"<xs:complexType name="t">{content}{attribute}</xs:complexType>"#),
                )
            } else {
                (String::new(), String::new())
            };
            let mode = if named {
                format!(r#"<xs:element name="mode"{inline}/>"#)
            } else {
                format!(
                    r#"<xs:element name="mode"><xs:complexType>{content}{attribute}</xs:complexType></xs:element>"#
                )
            };
            for deep in [false, true] {
                let body = if deep {
                    format!(
                        r#"<xs:element name="config"><xs:complexType><xs:sequence>{}</xs:sequence></xs:complexType></xs:element>{globals}"#,
                        mode.replace(
                            "<xs:element name=\"mode\"",
                            "<xs:element name=\"mode\" minOccurs=\"0\""
                        )
                    )
                } else {
                    format!("{mode}{globals}")
                };
                let schema = compile(&xsd(&body));
                let (text, caret) = if deep {
                    ("<config>\n  <\n</config>\n", 12)
                } else {
                    ("<", 1)
                };
                let offered = schema.suggestions(crate::editor::Language::XML, text, caret);
                let Some(option) = offered.iter().find(|option| option.label() == "mode") else {
                    failures.push(format!(
                        "children {with_children} named {named} deep {deep}: mode not offered"
                    ));
                    continue;
                };
                let (accepted, _) = option.accept(text);
                if !accepted.contains("<mode on=") || accepted.contains("<on>") {
                    failures.push(format!(
                        "children {with_children} named {named} deep {deep}:\n{accepted}"
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// A repeated element that must occur asks for its first item: a list with
/// none is refused in every language, and a fill writes one item.
#[cfg(feature = "yaml")]
#[test]
fn a_required_repeated_element_asks_for_an_item() {
    let schema = compile(&xsd(
        r#"<xs:element name="h"><xs:complexType><xs:sequence><xs:element name="u" maxOccurs="unbounded" type="xs:boolean"/></xs:sequence></xs:complexType></xs:element>"#,
    ));
    let errors = schema
        .validate(crate::editor::Language::YAML, "h:\n  u: []\n")
        .into_iter()
        .filter(|problem| problem.severity == crate::editor::diagnostics::Severity::Error)
        .count();
    assert!(errors > 0, "{}", schema.json());
    let offered = schema.suggestions(crate::editor::Language::XML, "<", 1);
    let option = offered
        .iter()
        .find(|option| option.label() == "h")
        .expect("h offered");
    let (accepted, _) = option.accept("<");
    assert!(accepted.contains("<u>"), "{accepted}");
}

/// A choice of one option is that option: what the option requires is
/// required, so a fill writes it (and an optional choice requires nothing).
#[cfg(feature = "xml")]
#[test]
fn a_choice_of_one_option_requires_what_the_option_requires() {
    for (occurs, required) in [("", true), (r#" minOccurs="0""#, false)] {
        let body = format!(
            r#"<xs:element name="auth"><xs:complexType><xs:choice{occurs}><xs:element name="token" type="xs:string"/></xs:choice></xs:complexType></xs:element>"#
        );
        let schema = compile(&xsd(&body));
        let offered = schema.suggestions(crate::editor::Language::XML, "<", 1);
        let option = offered
            .iter()
            .find(|option| option.label() == "auth")
            .unwrap_or_else(|| panic!("{occurs}: auth offered"));
        let (accepted, _) = option.accept("<");
        assert_eq!(
            accepted.contains("<token>"),
            required,
            "{occurs}: {accepted}\n{}",
            schema.json()
        );
    }
}
