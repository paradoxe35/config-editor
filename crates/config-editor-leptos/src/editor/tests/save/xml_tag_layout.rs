//! XML start tags written over several lines (a Spring `<bean>` with an
//! attribute per line, a `pom.xml` root with its declarations), attributes
//! aligned in columns, and `=` with spaces around it: an attribute
//! removed, added or inserted from another view changes only that
//! attribute's text (its line, when it has its own), and every other
//! attribute, comment, indent and blank line stays as written.

use crate::editor::Language;
use crate::editor::save_matrix::{report, save_value};
use serde_json::{Map, Value, json};

const BEANS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<beans xmlns="http://www.springframework.org/schema/beans"
       xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
       xsi:schemaLocation="http://www.springframework.org/schema/beans http://www.springframework.org/schema/beans/spring-beans.xsd">

    <!-- the pool -->
    <bean id="dataSource"
          class="org.apache.commons.dbcp2.BasicDataSource"
          destroy-method="close">
        <property name="url"      value="jdbc:h2:mem:test"/>
        <property name="username" value="sa"/>
    </bean>

    <bean id="service" class="com.example.Service"/>

    <bean id="cache"
          class="com.example.Cache"
          scope="singleton"/>
</beans>
"#;

const POM: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0"
         xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
         xsi:schemaLocation="http://maven.apache.org/POM/4.0.0 https://maven.apache.org/xsd/maven-4.0.0.xsd">
  <modelVersion>4.0.0</modelVersion>

  <!-- coordinates -->
  <groupId>com.example</groupId>
  <artifactId>demo</artifactId>
  <version>1.0.0</version>
</project>
"#;

const SERVER: &str = "<!-- c -->\n<server\n    host=\"a\"\n    port=\"80\"\n    tls=\"true\">\n  <path>/x</path>\n</server>\n";

const SPACED: &str =
    "<!-- c -->\n<server host=\"a\" port=\"80\" tls = \"true\">\n  <path>/x</path>\n</server>\n";

fn object<'a>(value: &'a mut Value, pointer: &str) -> &'a mut Map<String, Value> {
    let shown = value.to_string();
    value
        .pointer_mut(pointer)
        .and_then(Value::as_object_mut)
        .unwrap_or_else(|| panic!("no object at {pointer} in {shown}"))
}

fn removed(pointer: &'static str, key: &'static str) -> Box<dyn Fn(&mut Value)> {
    Box::new(move |value| {
        object(value, pointer).shift_remove(key);
    })
}

fn inserted(
    pointer: &'static str,
    at: usize,
    key: &'static str,
    held: Value,
) -> Box<dyn Fn(&mut Value)> {
    Box::new(move |value| {
        object(value, pointer).shift_insert(at, key.to_string(), held.clone());
    })
}

/// `host` with `from` (written once in it) replaced by `to`.
fn with(host: &str, from: &str, to: &str) -> String {
    assert_eq!(host.matches(from).count(), 1, "{from:?} once in the host");
    host.replacen(from, to, 1)
}

type Case<'a> = (&'a str, &'a str, Box<dyn Fn(&mut Value)>, String);

/// The views that write a key as an XML attribute (`@a`): HCL names no
/// block attribute so.
const ALL_VIEWS: [Language; 4] = [
    Language::YAML,
    Language::JSON,
    Language::TOML,
    Language::HCL,
];

const ATTRIBUTE_VIEWS: [Language; 3] = [Language::YAML, Language::JSON, Language::TOML];

/// Each case saved from each of `views`, with the host's comments and
/// without them (a host without comments may be written whole: it must
/// not be).
fn check(views: &[Language], cases: Vec<Case<'_>>) {
    let mut failures = Vec::new();
    let bare = |text: &str| {
        text.lines()
            .filter(|line| !line.trim_start().starts_with("<!--"))
            .map(|line| format!("{line}\n"))
            .collect::<String>()
    };
    for (label, host, change, expected) in &cases {
        let mut forms = vec![(label.to_string(), host.to_string(), expected.clone())];
        if host.contains("<!--") {
            forms.push((format!("{label} (no comments)"), bare(host), bare(expected)));
        }
        for (label, host, expected) in forms {
            for view in views {
                match save_value(Language::XML, *view, &host, change) {
                    Ok(saved) if saved == expected => {}
                    Ok(saved) => failures.push(format!(
                        "{label} via {view}:\n--- expected\n{expected}--- saved\n{saved}"
                    )),
                    Err(error) => failures.push(format!("{label} via {view}: {error}")),
                }
            }
        }
    }
    report(failures);
}

#[test]
fn an_attribute_removed_from_a_tag_over_lines_takes_only_its_own_text() {
    check(
        &ALL_VIEWS,
        vec![
            (
                "the last, closing the tag",
                SERVER,
                removed("/server", "tls"),
                with(SERVER, "\n    tls=\"true\"", ""),
            ),
            (
                "one between",
                SERVER,
                removed("/server", "port"),
                with(SERVER, "\n    port=\"80\"", ""),
            ),
            (
                "the first",
                SERVER,
                removed("/server", "host"),
                with(SERVER, "\n    host=\"a\"", ""),
            ),
            (
                "a bean's last",
                BEANS,
                removed("/beans/bean/0", "destroy-method"),
                with(BEANS, "\n          destroy-method=\"close\"", ""),
            ),
            (
                "a bean's first, on the name's line",
                BEANS,
                removed("/beans/bean/0", "id"),
                with(BEANS, " id=\"dataSource\"", ""),
            ),
            (
                "a self-closing bean's last",
                BEANS,
                removed("/beans/bean/2", "scope"),
                with(BEANS, "\n          scope=\"singleton\"", ""),
            ),
            (
                "an aligned one",
                BEANS,
                removed("/beans/bean/0/property/1", "value"),
                with(BEANS, " value=\"sa\"", ""),
            ),
            (
                "one beside an `=` with spaces",
                SPACED,
                removed("/server", "port"),
                with(SPACED, " port=\"80\"", ""),
            ),
            (
                "the one with an `=` with spaces",
                SPACED,
                removed("/server", "tls"),
                with(SPACED, " tls = \"true\"", ""),
            ),
        ],
    );
}

#[test]
fn an_attribute_added_to_a_tag_over_lines_gets_a_line_of_its_own() {
    check(
        &ATTRIBUTE_VIEWS,
        vec![
            (
                "at the end",
                SERVER,
                inserted("/server", 3, "@extra", json!("x")),
                with(SERVER, "tls=\"true\">", "tls=\"true\"\n    extra=\"x\">"),
            ),
            (
                "between two",
                SERVER,
                inserted("/server", 1, "@extra", json!("x")),
                with(SERVER, "\n    port=", "\n    extra=\"x\"\n    port="),
            ),
            (
                "at the end of a bean",
                BEANS,
                inserted("/beans/bean/0", 3, "@lazy-init", json!(true)),
                with(
                    BEANS,
                    "destroy-method=\"close\">",
                    "destroy-method=\"close\"\n          lazy-init=\"true\">",
                ),
            ),
            (
                "between a bean's",
                BEANS,
                inserted("/beans/bean/0", 2, "@scope", json!("prototype")),
                with(
                    BEANS,
                    "\n          destroy-method",
                    "\n          scope=\"prototype\"\n          destroy-method",
                ),
            ),
            (
                "to a self-closing bean",
                BEANS,
                inserted("/beans/bean/2", 3, "@lazy-init", json!(true)),
                with(
                    BEANS,
                    "scope=\"singleton\"/>",
                    "scope=\"singleton\"\n          lazy-init=\"true\"/>",
                ),
            ),
            (
                "to a one-line bean",
                BEANS,
                inserted("/beans/bean/1", 2, "@scope", json!("prototype")),
                with(
                    BEANS,
                    "class=\"com.example.Service\"/>",
                    "class=\"com.example.Service\" scope=\"prototype\"/>",
                ),
            ),
            (
                "to a root over lines with declarations",
                POM,
                inserted(
                    "/project",
                    0,
                    "@child.project.url.inherit.append.path",
                    json!(false),
                ),
                with(
                    POM,
                    "maven-4.0.0.xsd\">",
                    "maven-4.0.0.xsd\"\n         child.project.url.inherit.append.path=\"false\">",
                ),
            ),
            (
                "beside an `=` with spaces",
                SPACED,
                inserted("/server", 3, "@extra", json!("x")),
                with(SPACED, "tls = \"true\">", "tls = \"true\" extra=\"x\">"),
            ),
        ],
    );
}

#[test]
fn an_element_edited_beside_tags_over_lines_leaves_them_as_written() {
    check(
        &ALL_VIEWS,
        vec![
            (
                "a value of an aligned attribute",
                BEANS,
                Box::new(|value: &mut Value| {
                    value["beans"]["bean"][0]["property"][1]["value"] = json!("admin")
                }),
                with(BEANS, "value=\"sa\"", "value=\"admin\""),
            ),
            (
                "a child of a self-closing bean over lines",
                BEANS,
                Box::new(|value: &mut Value| {
                    object(value, "/beans/bean/2").insert("description".into(), json!("cache"));
                }),
                with(
                    BEANS,
                    "scope=\"singleton\"/>",
                    "scope=\"singleton\">\n        <description>cache</description>\n    </bean>",
                ),
            ),
            (
                "a pom element",
                POM,
                Box::new(|value: &mut Value| value["project"]["version"] = json!("1.0.1")),
                with(POM, "1.0.0</version>", "1.0.1</version>"),
            ),
            (
                "a pom element removed",
                POM,
                Box::new(|value: &mut Value| {
                    object(value, "/project").shift_remove("version");
                }),
                with(POM, "  <version>1.0.0</version>\n", ""),
            ),
            (
                "a bean removed",
                BEANS,
                Box::new(|value: &mut Value| {
                    value["beans"]["bean"].as_array_mut().unwrap().remove(1);
                }),
                with(
                    BEANS,
                    "    <bean id=\"service\" class=\"com.example.Service\"/>\n\n",
                    "",
                ),
            ),
        ],
    );
}
