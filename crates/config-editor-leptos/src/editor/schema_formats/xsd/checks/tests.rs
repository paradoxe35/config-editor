//! The XSD checks on documents, rule by rule: each rule JSON Schema cannot
//! hold refuses what XSD refuses and takes what it takes, in an XML text
//! and in another language's value; their problems are placed on the
//! element they are about; and they cost one walk per validation.

#![cfg(all(feature = "xml", feature = "schema-validation"))]

use super::super::super::{SchemaFormat, SchemaInput};
use crate::editor::Language;
use crate::editor::diagnostics::Schema;
use crate::editor::work_count::{Work, counted};

fn xsd(body: &str) -> String {
    format!("<xs:schema xmlns:xs=\"http://www.w3.org/2001/XMLSchema\">\n{body}\n</xs:schema>\n")
}

fn compile(body: &str) -> Schema {
    let text = xsd(body);
    Schema::compile_as(SchemaFormat::XSD, &SchemaInput::new(&text))
        .unwrap_or_else(|error| panic!("{}\n{text}", error.0))
}

fn messages(schema: &Schema, language: Language, document: &str) -> Vec<String> {
    schema
        .validate(language, document)
        .into_iter()
        .map(|problem| problem.message)
        .collect()
}

/// One rule: a schema body and documents with their verdicts (each one
/// checked by `xmllint --schema`: libxml2 gives the same verdict).
struct Rule {
    name: &'static str,
    body: &'static str,
    documents: &'static [(&'static str, bool)],
}

const SEQUENCE: &str = r#"<xs:element name="r"><xs:complexType><xs:sequence>
<xs:element name="a" type="xs:string"/><xs:element name="b" type="xs:string" minOccurs="0" maxOccurs="unbounded"/><xs:element name="c" type="xs:string" minOccurs="0"/>
</xs:sequence></xs:complexType></xs:element>"#;

const RULES: &[Rule] = &[
    Rule {
        name: "xs:sequence order",
        body: SEQUENCE,
        documents: &[
            ("<r><a>1</a><b>2</b><b>3</b><c>4</c></r>", true),
            ("<r><b>2</b><a>1</a></r>", false),
            ("<r><a>1</a><c>4</c><b>2</b></r>", false),
            // Repeated elements apart: one list in the value, apart in the
            // text.
            ("<r><a>1</a><b>2</b><c>4</c><b>3</b></r>", false),
        ],
    },
    Rule {
        name: "xs:choice taken once",
        body: r#"<xs:element name="r"><xs:complexType><xs:choice>
<xs:element name="a" type="xs:string" minOccurs="0"/><xs:element name="b" type="xs:string"/>
</xs:choice></xs:complexType></xs:element>"#,
        documents: &[
            ("<r><a>1</a></r>", true),
            ("<r><b>1</b></r>", true),
            ("<r><a>1</a><b>2</b></r>", false),
        ],
    },
    Rule {
        name: "xs:choice repeated in a sequence",
        body: r#"<xs:element name="r"><xs:complexType><xs:sequence maxOccurs="2">
<xs:element name="k" type="xs:string"/><xs:choice><xs:element name="a" type="xs:string"/><xs:element name="b" type="xs:string"/></xs:choice>
</xs:sequence></xs:complexType></xs:element>"#,
        documents: &[
            ("<r><k>1</k><a>2</a><k>3</k><b>4</b></r>", true),
            ("<r><k>1</k><a>2</a><b>4</b></r>", false),
            ("<r><k>1</k><k>3</k><a>2</a><b>4</b></r>", false),
        ],
    },
    Rule {
        name: "substitution group",
        body: r#"<xs:element name="head" type="xs:string" abstract="true"/>
<xs:element name="one" type="xs:string" substitutionGroup="head"/>
<xs:element name="two" type="xs:string" substitutionGroup="head"/>
<xs:element name="r"><xs:complexType><xs:sequence><xs:element ref="head"/><xs:element name="z" type="xs:string"/></xs:sequence></xs:complexType></xs:element>"#,
        documents: &[
            ("<r><one>1</one><z>2</z></r>", true),
            ("<r><two>1</two><z>2</z></r>", true),
            ("<r><one>1</one><two>1</two><z>2</z></r>", false),
            ("<r><z>2</z><one>1</one></r>", false),
        ],
    },
    Rule {
        name: "mixed content",
        body: r#"<xs:element name="p"><xs:complexType mixed="true"><xs:sequence>
<xs:element name="b" type="xs:string"/><xs:element name="i" type="xs:string" minOccurs="0"/>
</xs:sequence></xs:complexType></xs:element>"#,
        documents: &[
            ("<p>a <b>bold</b> and <i>it</i>.</p>", true),
            ("<p>a <i>it</i> and <b>bold</b>.</p>", false),
        ],
    },
    Rule {
        name: "xs:any namespace=\"##other\"",
        body: r###"<xs:element name="r"><xs:complexType><xs:sequence>
<xs:element name="a" type="xs:string"/><xs:any namespace="##other" processContents="skip" minOccurs="0" maxOccurs="unbounded"/>
</xs:sequence></xs:complexType></xs:element>"###,
        documents: &[
            (r#"<r><a>1</a><x:z xmlns:x="urn:x">2</x:z></r>"#, true),
            ("<r><a>1</a><z>2</z></r>", false),
        ],
    },
    Rule {
        name: "xs:anyAttribute namespace=\"##other\" beside xs:any",
        body: r###"<xs:element name="r"><xs:complexType><xs:sequence>
<xs:any namespace="##other" processContents="skip" minOccurs="0"/>
</xs:sequence><xs:attribute name="id" type="xs:string"/><xs:anyAttribute namespace="##other" processContents="skip"/></xs:complexType></xs:element>"###,
        documents: &[
            (r#"<r id="1" xmlns:x="urn:x" x:extra="2"/>"#, true),
            (r#"<r id="1" klass="2"/>"#, false),
        ],
    },
    Rule {
        name: "xs:key and xs:keyref",
        body: r#"<xs:element name="r"><xs:complexType><xs:sequence>
<xs:element name="item" maxOccurs="unbounded"><xs:complexType><xs:attribute name="id" type="xs:string"/></xs:complexType></xs:element>
<xs:element name="use" minOccurs="0" maxOccurs="unbounded"><xs:complexType><xs:attribute name="ref" type="xs:string"/></xs:complexType></xs:element>
</xs:sequence></xs:complexType>
<xs:key name="ids"><xs:selector xpath="item"/><xs:field xpath="@id"/></xs:key>
<xs:keyref name="uses" refer="ids"><xs:selector xpath="use"/><xs:field xpath="@ref"/></xs:keyref>
</xs:element>"#,
        documents: &[
            (r#"<r><item id="a"/><item id="b"/><use ref="b"/></r>"#, true),
            (r#"<r><item id="a"/><item id="a"/></r>"#, false),
            (r#"<r><item id="a"/><item/></r>"#, false),
            (r#"<r><item id="a"/><use ref="c"/></r>"#, false),
        ],
    },
    Rule {
        name: "xs:unique",
        body: r#"<xs:element name="r"><xs:complexType><xs:sequence>
<xs:element name="v" type="xs:string" maxOccurs="unbounded"/>
</xs:sequence></xs:complexType>
<xs:unique name="once"><xs:selector xpath="v"/><xs:field xpath="."/></xs:unique>
</xs:element>"#,
        documents: &[
            ("<r><v>1</v><v>2</v></r>", true),
            ("<r><v>1</v><v> 1 </v></r>", true),
            ("<r><v>1</v><v>2</v><v>1</v></r>", false),
        ],
    },
];

#[test]
fn each_rule_json_schema_cannot_hold_refuses_what_xsd_refuses() {
    let mut failures = Vec::new();
    for rule in RULES {
        let schema = compile(rule.body);
        for (document, valid) in rule.documents {
            let found = messages(&schema, Language::XML, document);
            if found.is_empty() != *valid {
                failures.push(format!(
                    "{}: {document}: want valid {valid}, got {found:?}",
                    rule.name
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn the_rules_it_checks_are_no_losses() {
    // Only the order, which another language's value does not keep, is
    // told: checked in an XML text alone.
    for rule in RULES {
        let schema = compile(rule.body);
        let losses: Vec<String> = schema
            .losses()
            .iter()
            .filter(|loss| loss.construct != "xs:sequence order")
            .map(ToString::to_string)
            .collect();
        assert!(losses.is_empty(), "{}: {losses:?}", rule.name);
    }
    let order = compile(SEQUENCE);
    let losses: Vec<String> = order
        .losses()
        .iter()
        .map(|loss| loss.message.clone())
        .collect();
    assert_eq!(losses.len(), 1, "{losses:?}");
    assert!(
        losses[0].contains("checked in an XML text only"),
        "{losses:?}"
    );
}

#[test]
fn a_problem_names_what_is_expected_on_the_element_it_is_about() {
    let schema = compile(SEQUENCE);
    let problems = schema.validate(
        Language::XML,
        "<r>\n  <a>1</a>\n  <c>4</c>\n  <b>2</b>\n</r>\n",
    );
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert_eq!(
        problems[0].message,
        "/r/b/0: <b> is not expected here: no more elements are allowed"
    );
    assert_eq!(problems[0].line, Some(4));
    let problems = schema.validate(Language::XML, "<r>\n  <b>2</b>\n  <a>1</a>\n</r>\n");
    assert_eq!(
        problems[0].message,
        "/r/b/0: <b> is not expected here; expected <a>"
    );
    assert_eq!(problems[0].line, Some(2));
    // A required element missing is the JSON Schema's problem, told once.
    let problems = messages(&schema, Language::XML, "<r><b>2</b></r>");
    assert_eq!(problems.len(), 1, "{problems:?}");
}

#[cfg(feature = "yaml")]
#[test]
fn another_languages_value_is_checked_but_for_the_order() {
    // TOML and HCL write tables after values: the children are taken in
    // the model's order.
    let schema = compile(SEQUENCE);
    for document in [
        "r:\n  a: '1'\n  b: ['2', '3']\n  c: '4'\n",
        "r:\n  c: '4'\n  b: ['2', '3']\n  a: '1'\n",
    ] {
        assert!(
            messages(&schema, Language::YAML, document).is_empty(),
            "{document}"
        );
    }
    // A choice taken twice is refused in every language.
    let choice = compile(RULES[1].body);
    let found = messages(&choice, Language::YAML, "r:\n  b: '2'\n  a: '1'\n");
    assert_eq!(
        found,
        ["/r/b: <b> is not expected here: no more elements are allowed"]
    );
    // A key no wildcard allows: written without a prefix, an element of
    // the target namespace (here none), which `##other` refuses as in XML.
    let open = compile(RULES[5].body);
    let found = messages(&open, Language::YAML, "r:\n  a: '1'\n  z: '2'\n");
    assert_eq!(
        found,
        ["/r/z: \"z\" is neither declared here nor allowed by a wildcard"]
    );
    let other = compile(
        r###"<xs:element name="r"><xs:complexType><xs:sequence>
<xs:any namespace="##any" processContents="skip" minOccurs="0" maxOccurs="unbounded"/>
</xs:sequence></xs:complexType></xs:element>"###,
    );
    assert!(messages(&other, Language::YAML, "r:\n  z: '2'\n").is_empty());
    let keys = compile(RULES[7].body);
    let found = messages(
        &keys,
        Language::YAML,
        "r:\n  item:\n    - id: a\n    - id: a\n",
    );
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(
        found[0].starts_with("/r/item/1: \"a\" is already the value of the xs:key \"ids\""),
        "{found:?}"
    );
}

#[test]
fn the_compiled_schema_carries_only_the_checks_it_needs() {
    let names = |body: &str| -> Vec<String> {
        let text = xsd(body);
        let compiled = SchemaFormat::XSD.compile(&SchemaInput::new(&text)).unwrap();
        compiled
            .checks
            .iter()
            .map(|check| check.name().into_owned())
            .collect()
    };
    assert!(names(r#"<xs:element name="v" type="xs:string"/>"#).is_empty());
    assert_eq!(names(SEQUENCE).len(), 1);
    assert_eq!(names(RULES[7].body).len(), 2);
}

#[test]
fn the_checks_walk_the_document_once_per_validation() {
    let schema = compile(RULES[7].body);
    let document = |items: usize| {
        let mut text = String::from("<r>");
        for index in 0..items {
            text.push_str(&format!("<item id=\"i{index}\"/>"));
        }
        for index in 0..items {
            text.push_str(&format!("<use ref=\"i{index}\"/>"));
        }
        text.push_str("</r>");
        text
    };
    let small = document(10);
    let large = document(100);
    let (found, checks) = counted(Work::SchemaCheck, || schema.validate(Language::XML, &small));
    assert!(found.is_empty(), "{found:?}");
    assert_eq!(checks, 2, "each check once");
    let (_, branch_small) = counted(Work::BranchWalk, || schema.validate(Language::XML, &small));
    let (_, branch_large) = counted(Work::BranchWalk, || schema.validate(Language::XML, &large));
    // Linear in the document: ten times the elements, at most ten times
    // the schema walks (and some more for the fixed part).
    assert!(
        branch_large <= branch_small * 10 + 50,
        "{branch_small} -> {branch_large}"
    );
}

#[test]
fn a_model_too_large_to_check_is_a_loss() {
    let schema = compile(
        r#"<xs:element name="r"><xs:complexType><xs:sequence minOccurs="60" maxOccurs="60"><xs:sequence minOccurs="60" maxOccurs="60">
<xs:element name="a" type="xs:string" minOccurs="10" maxOccurs="10"/></xs:sequence></xs:sequence></xs:complexType></xs:element>"#,
    );
    let losses: Vec<&str> = schema
        .losses()
        .iter()
        .map(|loss| loss.construct.as_ref())
        .collect();
    assert_eq!(losses, ["content model"]);
}

#[test]
fn a_constraint_beyond_the_xpath_subset_is_a_loss() {
    let schema = compile(
        r#"<xs:element name="r"><xs:complexType><xs:sequence><xs:element name="a" type="xs:string"/></xs:sequence></xs:complexType>
<xs:key name="k"><xs:selector xpath="a[1]"/><xs:field xpath="."/></xs:key></xs:element>"#,
    );
    let losses: Vec<String> = schema
        .losses()
        .iter()
        .map(|loss| loss.construct.to_string())
        .collect();
    assert_eq!(losses, ["xs:key"]);
    assert!(!schema.json().to_string().contains("x-xsd-identity"));
}
