//! A language whose scalars are text (XML) is parsed once per text: its
//! guessed reading is its text reading with each scalar guessed, taken
//! from the parse cache, and is exactly what `parse` gives.

use crate::editor::Language;
use crate::editor::parse_cache;
use crate::editor::work_count::{Work, counted};

/// Documents covering what the two readings could differ on.
const DOCUMENTS: &[&str] = &[
    "<r a=\"1.50\" b=\"007\" c=\"true\" d=\" 5 \"><n>80</n><s>null</s><f>1e3</f></r>",
    "<r><p>text <b>5</b> 7</p><e/><e></e><c><![CDATA[12]]></c></r>",
    "<r><i>inf</i><i>-inf</i><i>nan</i><i>\u{FDD0}inf</i><j>-0</j><k>false</k></r>",
    "<r config-root=\"true\"><a>1</a><a>2</a><a>x</a></r>",
    "<x a=\"1\">2</x><y>3</y>",
    include_str!("../../../tests/fixtures/samples/tunnel.large.xml"),
];

#[test]
fn the_guessed_reading_of_a_text_language_is_its_text_reading_guessed() {
    let xml = Language::XML.behavior();
    parse_cache::hold();
    for document in DOCUMENTS {
        let direct = xml.parse(document);
        // The text reading first (as a schema's read takes it), then the
        // guessed one from it: no second parse.
        let text = parse_cache::parse_text(xml, document);
        let (guessed, parsed) = counted(Work::ParsedBytes, || parse_cache::parse(xml, document));
        assert_eq!(guessed, direct, "{document}");
        assert_eq!(parsed, 0, "parsed again: {document}");
        assert!(text.is_ok() == direct.is_ok());
    }
    // The guessed reading asked first parses once, for both.
    let document = "<r><n>1</n></r>";
    let (_, parsed) = counted(Work::ParsedBytes, || {
        let _ = parse_cache::parse(xml, document);
        parse_cache::parse_text(xml, document)
    });
    assert_eq!(parsed, document.len());
    parse_cache::release();
}
