//! Folds through an Edit-as switch, between every pair of languages: the
//! fields folded before are folded after, and nothing else is.

use super::super::Language;
use super::switch::{folded_fields, folds_of_fields};

const YAML: &str = "\
listen: 7835
web:
  http: 80
  hosts:
    - name: a
      port: 1
    - name: b
      port: 2
db:
  user: u
  pool:
    min: 1
    max: 2
";

const ALL: [Language; 5] = [
    Language::YAML,
    Language::JSON,
    Language::TOML,
    Language::HCL,
    Language::XML,
];

#[test]
fn folded_fields_stay_folded_through_a_switch_between_every_pair() {
    let mut failures = Vec::new();
    for from in ALL {
        let before = Language::YAML.convert(YAML, from).unwrap().text;
        for wanted in [
            vec!["/web"],
            vec!["/db/pool"],
            vec!["/web", "/db/pool"],
            vec!["/web/hosts/1"],
        ] {
            // The folds of `from` starting on the wanted fields.
            let wanted: Vec<String> = wanted.into_iter().map(str::to_string).collect();
            let folds = folds_of_fields(&wanted, &before, from);
            if folds.len() != wanted.len() {
                failures.push(format!("{from}: {wanted:?} fold as {folds:?} in\n{before}"));
                continue;
            }
            let fields = folded_fields(&folds, &before, from);
            for to in ALL.into_iter().filter(|to| *to != from) {
                let after = from.convert(&before, to).unwrap().text;
                let carried = folds_of_fields(&fields, &after, to);
                let back = folded_fields(&carried, &after, to);
                if back != wanted {
                    failures.push(format!(
                        "{from} to {to}: {wanted:?} folded as {back:?} in\n{after}"
                    ));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} failing:\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}
