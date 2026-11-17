//! Hover docs for keys, containers and segments of dotted keys.

use super::burrow;
use crate::editor::languages::Language;

#[test]
fn containers_show_hover_docs_from_their_header_or_tag() {
    use crate::editor::completion::key_hover;
    let bundle = burrow();
    let cases: &[(Language, &str, usize, &str)] = &[
        (
            Language::TOML,
            "[web.letsencrypt]\nemail = \"a@b.c\"\n",
            0,
            "letsencrypt",
        ),
        (Language::TOML, "[websocket]\nlisten = 1\n", 0, "websocket"),
        (
            Language::HCL,
            "web {\n  letsencrypt {\n    email = \"a@b.c\"\n  }\n}\n",
            1,
            "letsencrypt",
        ),
        (
            Language::XML,
            "<web><letsencrypt email=\"a@b.c\"/></web>\n",
            0,
            "letsencrypt",
        ),
    ];
    for &(language, source, line, key) in cases {
        let text = source.split('\n').nth(line).unwrap();
        let column = text.find(key).unwrap() + 1;
        let hover = key_hover(source, line, column, Some(&bundle), language.behavior());
        assert!(
            hover
                .as_ref()
                .is_some_and(|hover| hover.key == key && hover.description.is_some()),
            "{language}: no docs for `{key}` in {source:?}"
        );
    }
}

fn hover_key_at(language: Language, source: &str, line: usize, column: usize) -> Option<String> {
    crate::editor::completion::key_hover(source, line, column, Some(&burrow()), language.behavior())
        .map(|hover| hover.key)
}

#[test]
fn hovering_a_segment_of_a_dotted_key_or_header_shows_that_segment() {
    let toml = "[[web.certificates]]\nhosts = [\"a\"]\n";
    assert_eq!(
        hover_key_at(Language::TOML, toml, 0, 3).as_deref(),
        Some("web")
    );
    assert_eq!(
        hover_key_at(Language::TOML, toml, 0, 8).as_deref(),
        Some("certificates")
    );
    let services = "[services.web.health_check]\ninterval_secs = 5\n";
    assert_eq!(
        hover_key_at(Language::TOML, services, 0, 2).as_deref(),
        Some("services")
    );
    assert_eq!(
        hover_key_at(Language::TOML, services, 0, 15).as_deref(),
        Some("health_check")
    );
    let dotted = "websocket.listen = 1\n";
    assert_eq!(
        hover_key_at(Language::TOML, dotted, 0, 2).as_deref(),
        Some("websocket")
    );
    assert_eq!(
        hover_key_at(Language::TOML, dotted, 0, 12).as_deref(),
        Some("listen")
    );
    let hcl = "services \"web\" {\n  target = 80\n}\n";
    assert_eq!(
        hover_key_at(Language::HCL, hcl, 0, 2).as_deref(),
        Some("services")
    );
}

#[test]
fn hover_works_inside_an_object_missing_a_required_key() {
    // `letsencrypt` requires `email`; its other keys still show their docs.
    let json = "{\"web\": {\"letsencrypt\": {\"dns\": []}}}";
    let column = json.find("dns").unwrap() + 1;
    assert_eq!(
        hover_key_at(Language::JSON, json, 0, column).as_deref(),
        Some("dns")
    );
    let yaml = "web:\n  letsencrypt:\n    dns: []\n";
    assert_eq!(
        hover_key_at(Language::YAML, yaml, 2, 5).as_deref(),
        Some("dns")
    );
    let hcl = "web {\n  letsencrypt {\n    dns = []\n  }\n}\n";
    assert_eq!(
        hover_key_at(Language::HCL, hcl, 2, 5).as_deref(),
        Some("dns")
    );
    let toml = "[web.letsencrypt]\ndns = []\n";
    assert_eq!(
        hover_key_at(Language::TOML, toml, 1, 1).as_deref(),
        Some("dns")
    );
    let xml = "<web>\n  <letsencrypt>\n    <dns></dns>\n  </letsencrypt>\n</web>\n";
    assert_eq!(
        hover_key_at(Language::XML, xml, 2, 6).as_deref(),
        Some("dns")
    );
}
