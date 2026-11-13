//! What the YAML parser is allowed to build: one configuration for every
//! place the editor parses YAML (the data, the tags, the keys a completion
//! leaves out), so a document reads the same everywhere.
//!
//! An alias (`*name`) copies its anchor's value, so a short text can stand
//! for a huge one ("billion laughs": each anchor aliases the one before ten
//! times). The guard is on what the aliases expand to, never on how many
//! there are: one `x-defaults: &d` merged into every service of a Compose
//! file, or a GitLab CI template used by every job, is ordinary YAML.

use noyalib::{Error, ParserConfig};

/// Bytes of text every alias of a document may add in all, whatever its
/// length (see `super::expansion`: a copy of its anchor's node as
/// written): far beyond what real files reuse, far below what would stall
/// the page.
pub(super) const EXPANSION_FLOOR: usize = 8 * 1024 * 1024;

/// What a document whose aliases expand past the limit is told.
pub(super) const EXPANSION_REFUSED: &str = "The aliases (*name) in this text expand to far more data than the text itself (each copies its anchor, and anchors alias each other): the editor does not read it";

/// The parser configuration for `source`: serde_yaml's reading, no limit
/// on the number of aliases, and what they add bounded by the larger of
/// [`EXPANSION_FLOOR`] and the text's own length, in bytes of text. `Err`
/// with the byte of the alias taking it past that.
pub(super) fn parser_config(source: &str) -> Result<ParserConfig, usize> {
    if let Some(alias) = super::expansion::past(source, source.len().max(EXPANSION_FLOOR)) {
        return Err(alias);
    }
    let mut config = ParserConfig::serde_yaml_compat();
    config.alias_anchor_ratio = None;
    config.alias_jump_event_factor = None;
    config.max_alias_expansions = usize::MAX;
    // The parser bounds the bytes it allocates for the copies by the
    // document length limit (and by 32 MB whatever it is): left at its own
    // bound, a backstop for copies the text measure does not see (an
    // anchor aliased on its own line). Many small values copied reach it
    // before the text limit (it counts each value at the size of one in
    // memory).
    config.max_document_length = usize::MAX;
    // The counts the parser bounds on its own (keys of one map, items of
    // one list, documents of a stream, nodes, events, scalar bytes, merge
    // keys) are bounded by the text's length already, and what aliases add
    // by the guard above: a large file reads as it is written. Only the
    // nesting depth stays, which bounds the parser's stack.
    config.max_stream_bytes = usize::MAX;
    config.max_mapping_keys = usize::MAX;
    config.max_sequence_length = usize::MAX;
    config.max_events = usize::MAX;
    config.max_nodes = usize::MAX;
    config.max_total_scalar_bytes = usize::MAX;
    config.max_documents = usize::MAX;
    config.max_merge_keys = usize::MAX;
    Ok(config)
}

/// Whether `error` is the parser refusing to nest deeper.
pub(super) fn is_too_deep(error: &Error) -> bool {
    matches!(error, Error::RecursionLimitExceeded { .. })
}

/// Where `source` first nests deeper than `read` goes: the end of the
/// shortest prefix whose reading is refused for its depth (the parser
/// reads in order, so it is refused there before anything later is seen).
/// Read only when the whole text was refused so.
pub(super) fn too_deep_at(source: &str, read: impl Fn(&str) -> Result<(), Error>) -> Option<usize> {
    let ends: Vec<usize> = source
        .char_indices()
        .map(|(at, character)| at + character.len_utf8())
        .collect();
    let refused = |end: usize| read(&source[..end]).is_err_and(|error| is_too_deep(&error));
    let first = ends.partition_point(|&end| !refused(end));
    let end = *ends.get(first)?;
    // The character that went one level too deep.
    Some(source[..end].char_indices().next_back().map_or(0, |(at, _)| at))
}

/// Whether `error` is the expansion guard refusing a document.
pub(super) fn is_expansion_refused(error: &Error) -> bool {
    matches!(error, Error::RepetitionLimitExceeded)
}

#[cfg(test)]
mod tests {
    use super::super::YamlBehavior;
    use super::EXPANSION_REFUSED;
    use crate::editor::language::ConfigLanguage;
    use serde_json::json;

    fn compose(services: usize) -> String {
        let mut text = String::from("x-defaults: &defaults\n  restart: unless-stopped\n  logging:\n    driver: json-file\nservices:\n");
        for index in 0..services {
            text.push_str(&format!("  s{index}:\n    <<: *defaults\n    image: app{index}\n"));
        }
        text
    }

    #[test]
    fn an_anchor_used_by_many_aliases_reads() {
        for services in [10, 11, 50, 500, 5_000] {
            let value = YamlBehavior
                .parse(&compose(services))
                .unwrap_or_else(|error| panic!("{services} aliases: {}", error.message));
            let last = &value["services"][format!("s{}", services - 1)];
            assert_eq!(last["restart"], json!("unless-stopped"), "{services}");
            assert_eq!(last["logging"]["driver"], json!("json-file"), "{services}");
        }
    }

    #[test]
    fn plain_aliases_of_a_scalar_and_a_list_read() {
        let mut text = String::from("base: &b [1, 2, 3]\nname: &n x\nitems:\n");
        for _ in 0..2_000 {
            text.push_str("  - [*b, *n]\n");
        }
        let value = YamlBehavior.parse(&text).expect("2000 aliases");
        assert_eq!(value["items"][1999], json!([[1, 2, 3], "x"]));
    }

    #[test]
    fn the_tags_of_a_many_alias_document_read() {
        let text = format!("tagged: !Ref x\n{}", compose(50));
        assert_eq!(YamlBehavior.tags(&text), vec![("/tagged".to_string(), "!Ref".to_string())]);
    }

    fn laughs(levels: usize) -> String {
        let mut text = String::from("l0: &l0 \"lol lol lol lol lol lol lol lol\"\n");
        for level in 1..levels {
            let aliases = vec![format!("*l{}", level - 1); 10].join(", ");
            text.push_str(&format!("l{level}: &l{level} [{aliases}]\n"));
        }
        text
    }

    #[test]
    fn a_billion_laughs_is_refused_plainly_and_promptly() {
        for levels in [9, 12, 20] {
            let started = std::time::Instant::now();
            let error = YamlBehavior.parse(&laughs(levels)).expect_err("a bomb is refused");
            assert_eq!(error.message, EXPANSION_REFUSED, "{levels} levels");
            assert!(started.elapsed().as_secs() < 5, "{levels} levels: {:?}", started.elapsed());
        }
    }

    /// A Compose file whose `x-defaults` anchor of `options` lines of
    /// `width` characters is merged into `services` services; and the
    /// bytes of text one copy of the anchor takes.
    fn merged(options: usize, width: usize, services: usize) -> (String, usize) {
        let mut text = String::from("x-defaults: &defaults\n");
        for index in 0..options {
            text.push_str(&format!("  opt_{index}: value-{index}-{}\n", "x".repeat(width)));
        }
        let copy = text.len() - text.find("&defaults").expect("anchored");
        text.push_str("services:\n");
        for index in 0..services {
            text.push_str(&format!("  s{index}:\n    <<: *defaults\n    image: app{index}\n"));
        }
        (text, copy)
    }

    #[test]
    fn aliases_adding_up_to_the_limit_read_and_past_it_are_refused() {
        let limit = super::EXPANSION_FLOOR;
        let (_, copy) = merged(40, 200, 0);
        let under = limit / copy;
        let (text, _) = merged(40, 200, under);
        assert!(under * copy > limit - copy, "{under} copies of {copy} bytes");
        let value = YamlBehavior.parse(&text).unwrap_or_else(|error| panic!("{} below the limit: {}", under * copy, error.message));
        assert_eq!(value["services"][format!("s{}", under - 1)]["opt_39"].as_str().map(str::len), Some(209));
        let (text, _) = merged(40, 200, under + 1);
        let error = YamlBehavior.parse(&text).expect_err("past the limit");
        assert_eq!(error.message, EXPANSION_REFUSED);
        // At the alias taking it past.
        let alias_line = text.lines().position(|line| line == format!("  s{under}:")).expect("written") + 2;
        assert_eq!(error.line, Some(alias_line));
    }

    #[test]
    fn a_long_anchor_merged_into_hundreds_of_services_reads() {
        let (text, copy) = merged(100, 20, 634);
        assert!(copy * 634 > 2_000_000, "{copy}");
        let value = YamlBehavior.parse(&text).unwrap_or_else(|error| panic!("{}", error.message));
        assert_eq!(value["services"]["s633"]["opt_99"], json!(format!("value-99-{}", "x".repeat(20))));
    }

    #[test]
    fn what_the_aliases_add_is_measured_in_bytes_of_the_anchors_text() {
        let (text, copy) = merged(3, 10, 5);
        assert_eq!(super::super::expansion::past(&text, 5 * copy), None);
        let first_past = super::super::expansion::past(&text, 5 * copy - 1).expect("past");
        assert_eq!(&text[first_past..first_past + 9], "*defaults");
        assert!(text[first_past..].starts_with("*defaults\n    image: app4"));
        // Nested: an anchor holding aliases copies them too.
        let nested = "a: &a [1, 2, 3]\nb: &b\n  - *a\n  - *a\nc: *b\n";
        let a = "&a [1, 2, 3]\n".len();
        let b = "&b\n  - *a\n  - *a\n".len() + 2 * (a - 2);
        assert_eq!(super::super::expansion::past(nested, 2 * a + b), None);
        assert!(super::super::expansion::past(nested, 2 * a + b - 1).is_some());
    }

    #[test]
    fn a_short_chain_of_laughs_reads() {
        let value = YamlBehavior.parse(&laughs(4)).expect("1000 copies is data");
        assert_eq!(value["l3"][9][9][9], json!("lol lol lol lol lol lol lol lol"));
    }
}
