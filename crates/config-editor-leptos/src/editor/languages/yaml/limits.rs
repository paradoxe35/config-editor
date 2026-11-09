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

/// Bytes (as the parser estimates them) every alias of a document may add
/// in all, whatever its length: far beyond what real files reuse, far below
/// what would stall the page.
const EXPANSION_FLOOR: usize = 8 * 1024 * 1024;

/// What a document whose aliases expand past the limit is told.
pub(super) const EXPANSION_REFUSED: &str = "The aliases (*name) in this text expand to far more data than the text itself (each copies its anchor, and anchors alias each other): the editor does not read it";

/// The parser configuration for `source`: serde_yaml's reading, no limit
/// on the number of aliases, and their expansion bounded by the larger of
/// [`EXPANSION_FLOOR`] and the text's own length.
pub(super) fn parser_config(source: &str) -> ParserConfig {
    let mut config = ParserConfig::serde_yaml_compat();
    config.alias_anchor_ratio = None;
    config.alias_jump_event_factor = None;
    config.max_alias_expansions = usize::MAX;
    // The parser bounds the expanded bytes by the document length limit,
    // which the text itself is also checked against.
    config.max_document_length = source.len().max(EXPANSION_FLOOR);
    config
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

    #[test]
    fn a_short_chain_of_laughs_reads() {
        let value = YamlBehavior.parse(&laughs(4)).expect("1000 copies is data");
        assert_eq!(value["l3"][9][9][9], json!("lol lol lol lol lol lol lol lol"));
    }
}
