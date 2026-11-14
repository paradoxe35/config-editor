//! A block holding a secret that becomes a list: another block, below or
//! above it, renamed to its name (`<d>` to `<db>`, `d {` to `db {`). The
//! field the host gave the placeholder for (`/config/db/password`) is now
//! an item's (`/config/db/0/password`): the placeholder there is flagged
//! as standing for the host's field, in XML as in HCL (QA 37: XML took it
//! for a copy of its own line, said nothing, and the save went through).
//! A copy of the secret's own line stays the field's first copy (see
//! `xml_copies_tests`).

use super::super::Language;
use super::super::diagnostics::build_schema;
use super::super::save_matrix::report;
use super::super::secret_options::SecretOptions;
use super::guarantee_tests::problems;
use super::many_copies_tests::shown;
use super::texts;
use super::xml_list_secret_tests::write;

const MASK: &str = "«secret 0b1c»";

struct Case {
    language: Language,
    text: String,
    /// The host's field of the placeholder, and where it is once renamed.
    field: &'static str,
    item: &'static str,
    /// The names to rename, each with the byte past which a letter goes.
    renames: &'static [(&'static str, usize)],
    schema: &'static str,
}

const XML_SCHEMA: &str = r#"{"type": "object", "properties": {"config": {"type": "object",
  "properties": {"db": {"type": "object", "properties": {"user": {"type": "string"},
  "password": {"type": "string", "x-secret": true}}}}}}}"#;
const HCL_SCHEMA: &str = r#"{"type": "object", "properties": {"db": {"type": "object",
  "properties": {"user": {"type": "string"},
  "password": {"type": "string", "x-secret": true}}}}}"#;

fn cases() -> Vec<Case> {
    let db = format!("  <db>\n    <user>app</user>\n    <password>{MASK}</password>\n  </db>\n");
    let other = "  <d>\n    <password>evil</password>\n  </d>\n";
    let hcl_db = format!("db {{\n  user = \"app\"\n  password = \"{MASK}\"\n}}\n");
    let hcl_other = "d {\n  password = \"evil\"\n}\n";
    vec![
        Case {
            language: Language::XML,
            text: format!("<config>\n{db}{other}</config>\n"),
            field: "/config/db/password",
            item: "/config/db/0/password",
            renames: &[("<d>", 2), ("</d>", 3)],
            schema: XML_SCHEMA,
        },
        Case {
            language: Language::XML,
            text: format!("<config>\n{other}{db}</config>\n"),
            field: "/config/db/password",
            item: "/config/db/1/password",
            renames: &[("<d>", 2), ("</d>", 3)],
            schema: XML_SCHEMA,
        },
        Case {
            language: Language::HCL,
            text: format!("{hcl_db}{hcl_other}"),
            field: "/db/password",
            item: "/db/0/password",
            renames: &[("d {", 1)],
            schema: HCL_SCHEMA,
        },
        Case {
            language: Language::HCL,
            text: format!("{hcl_other}{hcl_db}"),
            field: "/db/password",
            item: "/db/1/password",
            renames: &[("d {", 1)],
            schema: HCL_SCHEMA,
        },
    ]
}

#[test]
fn a_secret_whose_block_became_a_list_is_flagged_in_every_language() {
    let strings = crate::strings::default_strings();
    let mut failures = Vec::new();
    for case in cases() {
        for with_schema in [false, true] {
            let mut editor = super::super::test_state::TestEditor::new(case.language, &case.text);
            editor.secrets = Some(SecretOptions::default());
            let schema = std::sync::Arc::new(build_schema(case.schema).unwrap());
            if with_schema {
                editor.schema = Some(schema.clone());
            }
            let (_owner, state) = editor.mount();
            for (name, skip) in case.renames {
                // The block renamed is the one not holding the secret: the
                // first `name` past the start of a line.
                let text = shown(state);
                let at = text
                    .match_indices(name)
                    .map(|(at, _)| at)
                    .find(|at| *at == 0 || text[..*at].ends_with([' ', '\n']))
                    .expect("the other block");
                if !write(state, at + skip..at + skip, "b", true) {
                    failures.push(format!("{} renaming {name} refused", case.language));
                }
            }
            let text = shown(state);
            let line = text
                .lines()
                .position(|line| line.contains(MASK))
                .expect("the placeholder")
                + 1;
            let found = problems(state, case.language, with_schema.then_some(&schema.schema));
            let wanted = texts::moved_placeholder(strings, case.item, case.field);
            let flagged = found.iter().any(|problem| {
                problem.starts_with(&format!("{line}:")) && problem.ends_with(&wanted)
            });
            if !flagged {
                failures.push(format!(
                    "{} (schema {with_schema}): the placeholder at {} not flagged:\n{text}{found:#?}",
                    case.language, case.item
                ));
            }
        }
    }
    report(failures);
}
