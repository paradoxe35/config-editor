//! A placeholder a host text that does not parse gives as a field's whole
//! value (read off the host's own characters) takes nothing beside it once
//! the text on screen parses: what was typed beside it while the text was
//! broken is flagged at the token, the save waits, and a further key beside
//! it is refused with the tip, however the text was repaired (typed, pasted,
//! undone back into the broken text and repaired again). A token the host
//! wrote with other text beside it stays the host's to embed.

use leptos::prelude::*;

use super::super::Language;
use super::super::host_editing::SaveRequest;
use super::super::save_matrix::report;
use super::super::secret_options::SecretOptions;
use super::super::state::EditorState;
use super::super::test_state::TestEditor;
use super::glued_tests::{paste, type_at};
use super::guarantee_tests::problems;

const TOKEN: &str = "«secret 3f2a»";
const SAYS: &str = "a secret placeholder stands for the whole secret";

/// A host text that does not parse, holding `TOKEN` as a field's whole
/// value, and the edit that repairs it (`from` replaced by `to`).
struct Broken {
    name: &'static str,
    language: Language,
    host: String,
    from: &'static str,
    to: &'static str,
}

fn hosts() -> Vec<Broken> {
    let case = |name, language, host: String, from, to| Broken {
        name,
        language,
        host,
        from,
        to,
    };
    vec![
        case(
            "YAML",
            Language::YAML,
            format!("tls:\n  key: {TOKEN}\n  cert: /etc/edge.pem\nserver:\n  port: [\n"),
            "[",
            "1",
        ),
        case(
            "YAML quoted",
            Language::YAML,
            format!("tls:\n  key: \"{TOKEN}\"\n  cert: /etc/edge.pem\nserver: [\n"),
            "[",
            "{}",
        ),
        case(
            "JSON on one line",
            Language::JSON,
            format!(
                "{{\"tls\":{{\"key\":\"{TOKEN}\",\"cert\":\"/etc/edge.pem\"}},\"server\":{{\"port\":1"
            ),
            "\"port\":1",
            "\"port\":1}}",
        ),
        case(
            "JSON array item",
            Language::JSON,
            format!("{{\"keys\":[\"{TOKEN}\"],\"server\":{{\"port\":1}}"),
            "\"port\":1}",
            "\"port\":1}}",
        ),
        case(
            "TOML",
            Language::TOML,
            format!("[tls]\nkey = \"{TOKEN}\"\ncert = \"/etc/edge.pem\"\n\n[server\nport = 1\n"),
            "[server",
            "[server]",
        ),
        case(
            "TOML array",
            Language::TOML,
            format!("[tls]\nkey = \"{TOKEN}\"\n\n[server]\nnames = [\n"),
            "names = [",
            "names = []",
        ),
        case(
            "HCL",
            Language::HCL,
            format!(
                "tls {{\n  key = \"{TOKEN}\"\n  cert = \"/etc/edge.pem\"\n}}\nserver {{\n  port = [\n}}\n"
            ),
            "[",
            "1",
        ),
        case(
            "XML element",
            Language::XML,
            format!(
                "<config>\n  <tls>\n    <key>{TOKEN}</key>\n    <cert>/etc/edge.pem</cert>\n  </tls>\n  <server>\n    <port>1</port>\n  </server\n</config>\n"
            ),
            "</server",
            "</server>",
        ),
        case(
            "XML attribute",
            Language::XML,
            format!(
                "<config>\n  <tls key=\"{TOKEN}\" cert=\"/etc/edge.pem\"/>\n  <server port=\"1\"\n</config>\n"
            ),
            "<server port=\"1\"",
            "<server port=\"1\"/>",
        ),
    ]
}

/// A live editor on `host`, with the texts it saved.
fn mount(case: &Broken, host: &str) -> (Owner, EditorState, RwSignal<Vec<String>>) {
    let saves = RwSignal::new(Vec::<String>::new());
    let mut editor = TestEditor::new(case.language, host);
    editor.secrets = Some(SecretOptions::default());
    editor.on_save = Some(Callback::new(move |request: SaveRequest| {
        saves.update(|all| all.push(request.text));
    }));
    let (owner, state) = editor.mount();
    (owner, state, saves)
}

fn text(state: EditorState) -> String {
    state.doc().text.get_untracked()
}

/// The end of the token on screen, in bytes.
fn token_end(state: EditorState) -> usize {
    text(state).find(TOKEN).expect("the token on screen") + TOKEN.len()
}

/// Repairs the text, typed (`paste: false`) or pasted over.
fn repair(state: EditorState, case: &Broken, pasted: bool) {
    let shown = text(state);
    let from = shown.rfind(case.from).expect("the broken part");
    if pasted {
        paste(state, from..from + case.from.len(), case.to);
    } else {
        paste(state, from..from + case.from.len(), "");
        type_at(state, from, case.to);
    }
}

fn parses(case: &Broken, state: EditorState) -> bool {
    case.language.parse(&text(state)).is_ok()
}

fn tip(state: EditorState) -> String {
    state
        .protect()
        .tip
        .get_untracked()
        .map(|tip| tip.message)
        .unwrap_or_default()
}

/// What the glued text on screen does: flagged at the token, the save
/// waiting with the message, a further key beside the token refused.
fn check_glued(
    case: &Broken,
    state: EditorState,
    saves: RwSignal<Vec<String>>,
    step: &str,
) -> Vec<String> {
    let mut failures = Vec::new();
    let name = format!("{}, {step}", case.name);
    if !parses(case, state) {
        return vec![format!("{name}: not repaired: {:?}", text(state))];
    }
    let found = problems(state, case.language, None);
    if !found.iter().any(|problem| problem.contains(SAYS)) {
        failures.push(format!("{name}: not flagged: {found:?}"));
    }
    if state.save() || !saves.get_untracked().is_empty() {
        failures.push(format!("{name}: saved {:?}", saves.get_untracked()));
    } else if !tip(state).contains(SAYS) {
        failures.push(format!("{name}: the save says {:?}", tip(state)));
    }
    for (at, label) in [
        (token_end(state), "beside the token"),
        (token_end(state) + 1, "after the text beside it"),
    ] {
        let before = text(state);
        state.protect().tip.set(None);
        type_at(state, at, "Z");
        if text(state) != before || !tip(state).contains(SAYS) {
            failures.push(format!(
                "{name}: a key {label} let through: {:?}, tip {:?}",
                text(state),
                tip(state)
            ));
        }
    }
    failures
}

#[test]
fn text_typed_beside_a_whole_placeholder_of_a_broken_host_is_flagged_once_repaired() {
    let mut failures = Vec::new();
    for case in hosts() {
        for pasted in [false, true] {
            let (_owner, state, saves) = mount(&case, &case.host);
            type_at(state, token_end(state), "Y");
            if !text(state).contains(&format!("{TOKEN}Y")) {
                failures.push(format!("{}: typing while broken refused", case.name));
                continue;
            }
            repair(state, &case, pasted);
            let step = if pasted {
                "pasted repair"
            } else {
                "typed repair"
            };
            failures.extend(check_glued(&case, state, saves, step));
            // Taking the text beside it away is free, and then it saves.
            let end = token_end(state);
            if !paste(state, end..end + 1, "") {
                failures.push(format!(
                    "{}, {step}: taking the glued text away refused",
                    case.name
                ));
            }
            if !problems(state, case.language, None)
                .iter()
                .all(|problem| !problem.contains(SAYS))
                || !state.save()
            {
                failures.push(format!(
                    "{}, {step}: unglued text not saved: {:?}",
                    case.name,
                    text(state)
                ));
            }
        }
    }
    report(failures);
}

#[test]
fn undoing_back_into_the_broken_glued_text_and_repairing_again_flags_it() {
    let mut failures = Vec::new();
    for case in hosts() {
        let (_owner, state, saves) = mount(&case, &case.host);
        type_at(state, token_end(state), "Y");
        repair(state, &case, true);
        let undo = state.history_edit(false).expect("the repair to undo");
        if !state.commit(undo) || parses(&case, state) {
            failures.push(format!(
                "{}: not undone into the broken text: {:?}",
                case.name,
                text(state)
            ));
            continue;
        }
        repair(state, &case, true);
        failures.extend(check_glued(&case, state, saves, "repaired after undo"));
    }
    report(failures);
}

/// The host's own text beside a token is the host's: repaired, nothing is
/// flagged, typing beside it is free and it saves.
#[test]
fn a_token_a_broken_host_wrote_with_text_beside_it_stays_free() {
    let mut failures = Vec::new();
    for case in hosts() {
        let host = case.host.replacen(TOKEN, &format!("{TOKEN}YZ"), 1);
        let (_owner, state, saves) = mount(&case, &host);
        repair(state, &case, true);
        if !parses(&case, state) {
            failures.push(format!("{}: not repaired: {:?}", case.name, text(state)));
            continue;
        }
        let found = problems(state, case.language, None);
        if found.iter().any(|problem| problem.contains(SAYS)) {
            failures.push(format!("{}: flagged: {found:?}", case.name));
        }
        let at = token_end(state);
        type_at(state, at, "Q");
        if !text(state).contains(&format!("{TOKEN}QYZ")) {
            failures.push(format!(
                "{}: typing beside it refused: {:?}",
                case.name,
                text(state)
            ));
        }
        if !state.save() || saves.get_untracked().len() != 1 {
            failures.push(format!("{}: not saved: {:?}", case.name, tip(state)));
        }
    }
    report(failures);
}

/// Reading the fields' values of a text that does not parse, to tell which
/// tokens it gives whole, reads the text a fixed number of times, however
/// many tokens it holds.
#[test]
fn whole_values_of_a_text_that_does_not_parse_cost_one_reading() {
    use super::super::work_count::{Work, counted};
    use super::tokens::SecretMatcher;
    let matcher = SecretMatcher::new(&SecretOptions::default());
    let mut failures = Vec::new();
    for case in hosts() {
        let reads = |count: usize| {
            let mut text = case.host.clone();
            let at = text.find(TOKEN).unwrap();
            let line_start = text[..at].rfind('\n').map_or(0, |found| found + 1);
            let line_end = text[at..]
                .find('\n')
                .map_or(text.len(), |found| at + found + 1);
            let line = text[line_start..line_end].to_string();
            if !line.ends_with('\n') || line.trim_start().starts_with('{') {
                return None;
            }
            let lines: String = (0..count)
                .map(|index| line.replacen("key", &format!("key{index}"), 1))
                .collect();
            text.replace_range(line_start..line_end, &lines);
            let ((known, documents), spans) = counted(Work::SpanLookup, || {
                counted(Work::DocumentRead, || {
                    matcher.known(case.language.behavior(), &text)
                })
            });
            let whole = known.iter().filter(|known| known.whole.is_some()).count();
            Some((whole == count, documents, spans))
        };
        let (few, many) = (reads(20), reads(200));
        if let (Some(few), Some(many)) = (few, many)
            && (!few.0 || !many.0 || few.1 != many.1)
        {
            failures.push(format!(
                "{}: (all whole, reads, lookups) {few:?} at 20, {many:?} at 200",
                case.name
            ));
        }
    }
    report(failures);
}
