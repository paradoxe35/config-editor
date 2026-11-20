//! A placeholder pasted into a name a broken text reads cut in two (an XML
//! name ends at the token's space: `<se«secret 3f2a»ret>` reads as
//! `se«secret`) is flagged, and the warning names no piece of it: the key
//! holding one is shown as `…` (P1: the page shows no piece of a token).

#![allow(clippy::string_slice, reason = "the tests cut their own texts")]

use super::super::Language;
use super::super::secret_options::SecretOptions;
use super::problems::unknown_placeholders;
use super::texts::shown_pointer;
use super::tokens::SecretMatcher;

const TOKEN: &str = "«secret 3f2a»";
const HOST: &str = "clients:\n  acme:\n    secret: «secret 3f2a»\n";

/// The pieces of a token (a `«` or `»` not part of a whole one) in `message`.
fn pieces(message: &str) -> Vec<String> {
    let rest = message.replace(TOKEN, "");
    rest.match_indices(['«', '»'])
        .map(|(at, _)| rest[at..].chars().take(12).collect())
        .collect()
}

#[test]
fn a_token_pasted_into_a_name_is_flagged_without_a_piece_of_it() {
    let matcher = SecretMatcher::new(&SecretOptions::default());
    let known = matcher.known(Language::YAML.behavior(), HOST, None);
    let xml = Language::XML.behavior();
    let name = "secret";
    let mut failures = Vec::new();
    for at in 0..=name.len() {
        let pasted = format!("{}{TOKEN}{}", &name[..at], &name[at..]);
        let text =
            format!("<clients>\n  <acme>\n    <{pasted}>{TOKEN}</secret>\n  </acme>\n</clients>\n");
        let problems = unknown_placeholders(
            xml,
            &text,
            &matcher,
            &known,
            &[],
            None,
            crate::strings::default_strings(),
        );
        if problems.is_empty() {
            failures.push(format!("{pasted}: not flagged"));
        }
        for problem in problems {
            let found = pieces(&problem.message);
            if !found.is_empty() {
                failures.push(format!("{pasted}: {:?} shows {found:?}", problem.message));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn a_pointer_hides_only_the_keys_holding_a_piece_of_a_token() {
    let tokens = [TOKEN, "${SECRET}", "s3cr3t"];
    let cases = [
        ("/clients/acme/se«secret", "/clients/acme/…"),
        ("/a/3f2a»ret/b", "/a/…/b"),
        ("/a/x${SEC", "/a/…"),
        ("/a/RET}y", "/a/…"),
        // A key holding a token whole, and keys of letters alone.
        ("/a/«secret 3f2a»/secret", "/a/«secret 3f2a»/secret"),
        ("/pass/s3", "/pass/s3"),
        ("/~1«s", "/…"),
    ];
    let failures: Vec<String> = cases
        .iter()
        .filter(|(pointer, shown)| shown_pointer(pointer, &tokens) != *shown)
        .map(|(pointer, shown)| {
            format!(
                "{pointer}: {}, not {shown}",
                shown_pointer(pointer, &tokens)
            )
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
