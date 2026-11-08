//! Tagged nodes (`!Sub x`, `!GetAtt [a, b]`, `!include`, `!!binary`):
//! the data is the value under the tag (a tagged scalar is its text, as
//! YAML leaves its type to the application; see `parse`), and the tag
//! stays in the text. Where the tags are, so an Edit-as save can keep each
//! one.

use noyalib::Value as Yaml;

/// The JSON pointer of each tagged value of `source`, with its tag as
/// written; empty when the text has none or does not parse.
pub(super) fn yaml_tags(source: &str) -> Vec<(String, String)> {
    if !source.contains('!') {
        return Vec::new();
    }
    let config = noyalib::ParserConfig::serde_yaml_compat();
    let Ok(document) = noyalib::from_str_with_config::<Yaml>(source, &config) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    let mut stack = vec![(String::new(), &document)];
    while let Some((pointer, node)) = stack.pop() {
        match node {
            Yaml::Tagged(tagged) => {
                stack.push((pointer.clone(), tagged.value()));
                found.push((pointer, tagged.tag().to_string()));
            }
            Yaml::Sequence(items) => stack.extend(
                items
                    .iter()
                    .enumerate()
                    .rev()
                    .map(|(index, item)| (format!("{pointer}/{index}"), item)),
            ),
            Yaml::Mapping(entries) => stack.extend(
                entries
                    .iter()
                    .rev()
                    .map(|(key, item)| (format!("{pointer}/{}", escape(key)), item)),
            ),
            _ => {}
        }
    }
    found
}

/// `key` as a JSON pointer segment.
fn escape(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

#[cfg(test)]
mod tests {
    use super::yaml_tags;

    #[test]
    fn every_tagged_value_is_found_with_its_tag() {
        let text =
            "a: !Sub 'x-${y}'\nb:\n  - !Ref r\n  - 2\nc: !include\n  k: !secret pw\n  n: !!str 1\nd: plain\n";
        let found = yaml_tags(text);
        let pointers: Vec<&str> = found.iter().map(|(pointer, _)| pointer.as_str()).collect();
        assert_eq!(pointers, ["/a", "/b/0", "/c", "/c/k"], "{found:?}");
        assert!(found[0].1.contains("Sub"), "{found:?}");
    }
}
