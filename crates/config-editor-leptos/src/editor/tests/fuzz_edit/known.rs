//! Failure classes the random tests know of, each naming its finding:
//! reported (with its smallest case), not failed. A fix removes its entry
//! (and the run shows the class gone); an entry matching nothing costs
//! nothing, but goes when its finding is fixed.

/// A known class: the properties it shows in and a part of its class
/// name. Each has its smallest case in `repros` (ignored while open).
struct Known {
    properties: &'static [&'static str],
    class: &'static str,
    finding: &'static str,
}

const FE38_5: &str = "FE38-5 (LOW edge): line moves at the end of a text without a last newline (caret past the text, a blank line lost into it)";

const KNOWN: &[Known] = &[
    // `123456789.123456789` (more digits than a double holds) converts
    // to `123456789.12345679` with no loss listed: the same double, but
    // the written value changed (headless convert and its round trip).
    Known {
        properties: &["convert"],
        class: "a decimal with more digits than a double holds is rounded",
        finding: "FE38-1 (LOW edge): long decimals rounded to a double on conversion",
    },
    // A replace (one or all) of a match by the same text, or an accepted
    // suggestion writing what is there, records an undo step that changes
    // nothing: one Ctrl+Z seems to do nothing.
    Known {
        properties: &["undo-grouping", "undo-interleaved"],
        class: "an undo step changes nothing",
        finding: "FE38-2 (LOW edge): an edit changing nothing is an undo step",
    },
    // Ctrl+/ on an XML element holding a comment comments it whole (the
    // inner comment escaped as `&#45;&#45;`); Ctrl+/ again comments each
    // line again instead of taking the comment back.
    Known {
        properties: &["valid"],
        class: "comment twice does not give the text back (XML)",
        finding: "FE38-3 (MEDIUM normal use): an XML block holding a comment cannot be uncommented",
    },
    // Ctrl+/ on a YAML comment line of prose (`# about 1` above a list
    // item) uncomments it into the item's text; Ctrl+/ again comments
    // the construct it now reads as (two lines), not the line.
    Known {
        properties: &["valid"],
        class: "comment twice does not give the text back (YAML)",
        finding: "FE38-4 (LOW edge): uncommented YAML prose joins the next item",
    },
    // Moving the last lines down in a text without a last newline leaves
    // the caret one past the text's end (`a\nb`, line 1 down); a blank
    // first line moved down there becomes the last newline, and moving it
    // back up does nothing (`\na = ""`).
    Known {
        properties: &["undo-grouping", "undo-interleaved", "valid"],
        class: "caret past the text's end after",
        finding: FE38_5,
    },
    Known {
        properties: &["commands"],
        class: "leaves the caret past the text's end",
        finding: FE38_5,
    },
    Known {
        properties: &["valid"],
        class: "move down then up does not give the text back",
        finding: FE38_5,
    },
    // Enter after a YAML key whose entries are indented more than the
    // file's unit (`pool:` over `    max: 2` in a two-space file) indents
    // by the unit: the new entry breaks the block.
    Known {
        properties: &["valid"],
        class: "Enter after a block's opening line, then a new entry, does not read (YAML)",
        finding: "FE38-6 (MEDIUM normal use?): Enter under a YAML key indents by the file's unit, not by its entries",
    },
    // Ctrl+/ on a line of overlapping comment markers (`<!--->`) panics
    // (`xml::editing::toggle_xml_comments` slices past the markers).
    Known {
        properties: &["undo-grouping", "undo-interleaved", "commands"],
        class: "panic at editor/languages/xml/editing.rs",
        finding: "FE38-7 (HIGH promise, crash): Ctrl+/ on `<!--->` panics",
    },
    // Suggestions in a TOML text whose header ends with a character of
    // several bytes (`[clients.acmé]`, typed in an Edit-as TOML view)
    // panic (`toml::style::key_places` cuts the character).
    Known {
        properties: &["undo-grouping", "undo-interleaved", "commands"],
        class: "panic at editor/languages/toml/style.rs",
        finding: "FE38-8 (HIGH promise, crash): a TOML header ending in a non-ASCII character panics the suggestions",
    },
    // Enter after a top-level YAML entry below a block indented by eight
    // spaces indents the new line by eight.
    Known {
        properties: &["valid"],
        class: "Enter after an entry, then a new entry, does not read (YAML)",
        finding: "FE38-9 (LOW edge): Enter after a YAML entry below an eight-space block indents by eight",
    },
    // Suggestions accepted in an Edit-as TOML view of an HCL host, after
    // a Backspace in a table's header line, panic
    // (`toml::suggestions` slices the header backwards).
    Known {
        properties: &["undo-grouping", "undo-interleaved", "commands"],
        class: "panic at editor/languages/toml/suggestions.rs",
        finding: "FE38-10 (HIGH promise, crash): suggestions in a half-typed TOML header panic",
    },
    // Suggestions accepted one after another on a YAML key line panic in
    // the renamed-key lookup (`completion::elsewhere::written_places`
    // gets a backward range).
    Known {
        properties: &["undo-grouping", "undo-interleaved", "commands"],
        class: "panic at editor/completion/elsewhere.rs",
        finding: "FE38-11 (HIGH promise, crash): accepting suggestions in a row on a YAML key line panics",
    },
];

/// The finding excusing `class` in `property`, if one does.
pub(super) fn excused(property: &str, class: &str) -> Option<&'static str> {
    KNOWN
        .iter()
        .find(|known| known.properties.contains(&property) && class.contains(known.class))
        .map(|known| known.finding)
}
