//! Suggestions at places only TOML's syntax has (table headers, dotted
//! keys), against the tunnel schema. The suggestion promises every
//! language shares are in `completion_matrix`.
//!
//! `|` marks the caret.

use super::super::Language;
use super::super::harness::completion::{accepted, parsed, keys_at};

/// A key typed in front of a TOML `[table]` header goes on its own line,
/// before the table, and the document still parses.
#[test]
fn a_toml_key_typed_in_front_of_a_header_goes_before_it() {
    let text = accepted(Language::TOML, "hea|[tcp]\nnodelay = true\n", "heartbeat").unwrap();
    let value = parsed(Language::TOML, &text).unwrap_or_else(|error| panic!("{error}: {text:?}"));
    assert_eq!(keys_at(&value, &[]), ["heartbeat", "tcp"], "{text:?}");
    assert_eq!(keys_at(&value, &["tcp"]), ["nodelay"], "{text:?}");
    // A scalar waits for its value on its own line.
    let text = accepted(Language::TOML, "lis|[tcp]\nnodelay = true\n", "listen").unwrap();
    assert!(
        text.starts_with("listen = ") && text.contains("\n[tcp]\nnodelay = true"),
        "{text:?}"
    );
}

/// The head of a TOML dotted key completed in place keeps the rest
/// of the key and its value (`ud|.nodelay = true` + udp → `udp.nodelay`).
#[test]
fn a_toml_dotted_key_head_completes_in_place() {
    let text = accepted(
        Language::TOML,
        "ud|.nodelay = true\ntcp.keepalive_secs = 3\n",
        "udp",
    )
    .unwrap();
    assert_eq!(text, "udp.nodelay = true\ntcp.keepalive_secs = 3\n");
}

/// A TOML table renamed by a completion takes its sub-tables and
/// dotted keys along, so its data stays one table.
#[test]
fn a_toml_table_renamed_by_completion_renames_its_sub_tables_and_dotted_keys() {
    let marked = "[t|cp]\nnodelay = true\n[tcp.x]\na = 1\n";
    let text = accepted(Language::TOML, marked, "metrics").unwrap();
    assert!(!text.contains("tcp"), "{marked:?} → {text:?}");
    assert!(parsed(Language::TOML, &text).is_ok(), "{text:?}");
    let text = accepted(
        Language::TOML,
        "[t|cp]\nnodelay = true\n[tcp.x]\na = 1\n",
        "metrics",
    )
    .unwrap();
    assert_eq!(text, "[metrics]\nnodelay = true\n[metrics.x]\na = 1\n");
}

/// A key typed in front of a dotted key's last segment keeps the
/// dotted head on both entries.
#[test]
fn a_key_typed_inside_a_dotted_toml_key_keeps_the_head_on_both() {
    let text = accepted(Language::TOML, "websocket.ho|path = \"/x\"\n", "host").unwrap();
    let value = parsed(Language::TOML, &text).unwrap();
    assert_eq!(keys_at(&value, &["websocket"]), ["host", "path"], "{text}");
}
