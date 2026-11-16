//! TOML table headers: which tables a header offers and how a segment
//! completes.

use super::{accept, labels};
use crate::editor::languages::Language;

#[test]
fn toml_table_headers_complete_their_segment_with_tables_only() {
    assert_eq!(labels(Language::TOML, "[hea|]\n"), ["heartbeat"]);
    assert_eq!(labels(Language::TOML, "[web.lets|]\n"), ["letsencrypt"]);
    assert!(!labels(Language::TOML, "[|]\n").contains(&"listen".to_string()));
    assert_eq!(
        accept(Language::TOML, "[hea|]\n", "heartbeat"),
        "[heartbeat|]\n"
    );
    assert_eq!(
        accept(Language::TOML, "[[web.cert|]]\n", "certificates"),
        "[[web.certificates|]]\n"
    );
}

#[test]
fn toml_headers_offer_tables_or_arrays_of_tables_by_bracket_kind() {
    let tables = labels(Language::TOML, "[web.|]\n");
    assert!(
        tables.contains(&"letsencrypt".to_string())
            && !tables.contains(&"certificates".to_string()),
        "{tables:?}"
    );
    let arrays = labels(Language::TOML, "[[web.|]]\n");
    assert_eq!(arrays, ["certificates"]);
}

#[test]
fn toml_header_completion_with_a_prefix_leaves_out_existing_tables() {
    let offered = |marked: &str| labels(Language::TOML, marked);
    assert!(!offered("[web]\nhttp = 1\n\n[w|]").contains(&"web".to_string()));
    assert!(offered("[web]\nhttp = 1\n\n[w|]").contains(&"websocket".to_string()));
    assert!(!offered("[websocket]\nlisten = 1\n\n[webs|]").contains(&"websocket".to_string()));
    assert!(
        !offered("[web.letsencrypt]\nemail = \"a\"\n\n[web.l|]")
            .contains(&"letsencrypt".to_string())
    );
}
