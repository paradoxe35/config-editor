//! A placeholder is opaque: the editor matches the host's pattern and keeps
//! each token whole, as written; it reads nothing from inside one. The
//! same token in two fields and two different tokens each stay where they
//! were, byte for byte, through edits, line moves, list reorders and
//! Edit-as saves, in every language and for two unrelated patterns; a text
//! the pattern does not match exactly is plain text.

use leptos::prelude::*;

use super::super::Language;
use super::super::save_matrix::{ALL, report, save_value};
use super::guarantee_tests::{
    HOSTS, has_comments, mount, nth, problems, real, real_texts, replace, sample,
};

/// The tokens the host wrote, in document order, the comment's first.
fn written(language: Language, tokens: &super::guarantee_tests::Tokens) -> Vec<String> {
    let mut all: Vec<String> = Vec::new();
    if has_comments(language) {
        all.push(tokens.note.to_string());
    }
    all.extend(tokens.in_fields().iter().map(|token| token.to_string()));
    all
}

#[test]
fn every_token_stays_whole_and_in_place_through_edits_and_moves() {
    let mut failures = Vec::new();
    for tokens in &HOSTS {
        for language in ALL {
            let case = format!("{language} ({})", tokens.pattern);
            let text = sample(language, tokens);
            let (_owner, state) = mount(language, &text, tokens);
            let expect = |failures: &mut Vec<String>, step: &str, shown: &str| {
                let now = state.doc().text.get_untracked();
                if now != shown {
                    failures.push(format!("{case}, {step}: the text is\n{now}"));
                }
                // Each real one sits where its text is written.
                let at: Vec<(usize, String)> = real(state);
                for (start, token) in &at {
                    if !now[*start..].starts_with(token.as_str()) {
                        failures.push(format!("{case}, {step}: {token} is not at {start}"));
                    }
                }
                if real_texts(state) != written(language, tokens) {
                    failures.push(format!("{case}, {step}: real {:?}", real_texts(state)));
                }
                let problems = problems(state, language, None);
                if !problems.is_empty() {
                    failures.push(format!("{case}, {step}: problems {problems:?}"));
                }
            };
            expect(&mut failures, "on load", &text);

            // Typing elsewhere moves them with the text.
            let name = text.find("name").unwrap();
            replace(state, name..name, "x");
            let typed = format!("{}x{}", &text[..name], &text[name..]);
            expect(&mut failures, "typed", &typed);

            // The line holding `a` moved below the one holding `b`: both
            // hold the same token, each still in a field the host gave it.
            let shown = state.doc().text.get_untracked();
            let a_line = nth(&shown, tokens.same, 0).start;
            let a_line = shown[..a_line].rfind('\n').unwrap() + 1;
            let a_end = shown[a_line..].find('\n').unwrap() + a_line + 1;
            let b_end = shown[a_end..].find('\n').unwrap() + a_end + 1;
            let moved = format!(
                "{}{}{}{}",
                &shown[..a_line],
                &shown[a_end..b_end],
                &shown[a_line..a_end],
                &shown[b_end..]
            );
            replace(state, a_line..b_end, &moved[a_line..b_end]);
            expect(&mut failures, "a line moved", &moved);
        }
    }
    report(failures);
}

/// A list's items swapped on screen: each token goes with its item, as
/// written; each now stands in the other item's field, so each is plain
/// text there and said so (a host restoring by field would give each item
/// the other's secret).
#[test]
fn reordered_list_items_keep_their_tokens_and_are_flagged_in_the_other_item() {
    let mut failures = Vec::new();
    for tokens in &HOSTS {
        for language in ALL {
            let case = format!("{language} ({})", tokens.pattern);
            let text = sample(language, tokens);
            let (_owner, state) = mount(language, &text, tokens);
            let first = nth(&text, tokens.first, 0);
            let second = nth(&text, tokens.second, 0);
            let swapped = format!(
                "{}{}{}{}{}",
                &text[..first.start],
                tokens.second,
                &text[first.end..second.start],
                tokens.first,
                &text[second.end..]
            );
            replace(
                state,
                first.start..second.end,
                &swapped[first.start..second.end],
            );
            if state.doc().text.get_untracked() != swapped {
                failures.push(format!(
                    "{case}: swapped to\n{}",
                    state.doc().text.get_untracked()
                ));
                continue;
            }
            let real = real_texts(state);
            if real
                .iter()
                .any(|token| token == tokens.first || token == tokens.second)
            {
                failures.push(format!("{case}: real after the swap {real:?}"));
            }
            let found = problems(state, language, None);
            let moved = found
                .iter()
                .filter(|problem| problem.contains("stands for the secret of /l/"))
                .count();
            if moved != 2 || found.len() != 2 {
                failures.push(format!("{case}: problems {found:?}"));
            }
            // Swapped back: real again, nothing flagged.
            let back = nth(&swapped, tokens.second, 0).start..nth(&swapped, tokens.first, 0).end;
            replace(state, back.clone(), &text[first.start..second.end]);
            if state.doc().text.get_untracked() != text
                || real_texts(state).len() != real.len() + 2
                || !problems(state, language, None).is_empty()
            {
                failures.push(format!(
                    "{case}: swapped back, real {:?} problems {:?}",
                    real_texts(state),
                    problems(state, language, None)
                ));
            }
        }
    }
    report(failures);
}

/// A step: its name, the change made in the view, the host's text expected.
type Case<'a> = (&'a str, &'a dyn Fn(&mut serde_json::Value), &'a str);

/// Edit-as saves of a value elsewhere, of the list reordered and of a key
/// moved: each token reaches the host's text as written, with its item or
/// field, in every pair of languages.
#[test]
fn edit_as_saves_keep_every_token_byte_for_byte_with_its_field() {
    let mut failures = Vec::new();
    for tokens in &HOSTS {
        for host in ALL {
            let text = sample(host, tokens);
            let first = nth(&text, tokens.first, 0);
            let second = nth(&text, tokens.second, 0);
            let swapped = format!(
                "{}{}{}{}{}",
                &text[..first.start],
                tokens.second,
                &text[first.end..second.start],
                tokens.first,
                &text[second.end..]
            );
            let renamed = text.replacen(">a<", ">z<", 1).replacen(": a\n", ": z\n", 1);
            let renamed = renamed
                .replacen("\"name\": \"a\"", "\"name\": \"z\"", 1)
                .replacen("name = \"a\"", "name = \"z\"", 1);
            for view in ALL.into_iter().filter(|view| *view != host) {
                let case = format!("{host} host via {view} ({})", tokens.pattern);
                let cases: [Case<'_>; 2] = [
                    (
                        "a value elsewhere",
                        &|value| value["name"] = "z".into(),
                        &renamed,
                    ),
                    (
                        "the list reordered",
                        &|value| {
                            if let Some(items) = value["l"].as_array_mut() {
                                items.reverse();
                            }
                        },
                        &swapped,
                    ),
                ];
                for (step, change, expected) in cases {
                    match save_value(host, view, &text, change) {
                        Ok(saved) if saved == expected => {}
                        Ok(saved) => failures.push(format!(
                            "{case}, {step}:\n--- expected\n{expected}--- saved\n{saved}"
                        )),
                        Err(error) => failures.push(format!("{case}, {step}: {error}")),
                    }
                }
                // A key moved first: each token still with its field, as
                // written (the host's text keeps its order or follows the
                // view's; either way no token changes).
                let moved = save_value(host, view, &text, &|value| {
                    if let Some(map) = value.as_object_mut()
                        && let Some(c) = map.remove("c")
                    {
                        let rest = std::mem::take(map);
                        map.insert("c".to_string(), c);
                        map.extend(rest);
                    }
                });
                match moved {
                    Ok(saved) => {
                        let parsed = host.parse(&saved).map_err(|error| error.message);
                        let fields = parsed.as_ref().map(|value| {
                            ["a", "b", "c"].map(|key| value[key].as_str().unwrap_or("").to_string())
                        });
                        let want = [tokens.same, tokens.same, tokens.other].map(str::to_string);
                        if fields.as_ref() != Ok(&want)
                            || saved.matches(tokens.same).count() != 2
                            || saved.matches(tokens.other).count() != 1
                        {
                            failures.push(format!("{case}, a key moved: saved\n{saved}"));
                        }
                    }
                    Err(error) => failures.push(format!("{case}, a key moved: {error}")),
                }
            }
        }
    }
    report(failures);
}

/// A text like a token that the host's pattern does not match exactly
/// (another word, too short, another case, spaces) is plain text: no chip,
/// no atomic editing, no problem, also in a field the schema marks secret.
#[test]
fn near_misses_of_the_pattern_are_plain_text() {
    let schema: serde_json::Value = serde_json::json!({
        "type": "object",
        "properties": {"c": {"type": "string", "x-secret": true}}
    });
    let mut failures = Vec::new();
    for tokens in &HOSTS {
        for language in ALL {
            let case = format!("{language} ({})", tokens.pattern);
            for miss in tokens.near_misses {
                let text = sample(language, tokens).replace(tokens.other, miss);
                let (_owner, state) = mount(language, &text, tokens);
                if real_texts(state).iter().any(|token| token == miss) {
                    failures.push(format!("{case}: {miss} is a placeholder"));
                }
                let found = problems(state, language, Some(&schema));
                if !found.is_empty() {
                    failures.push(format!("{case}: {miss} flagged {found:?}"));
                }
                // Deleting its last character deletes just that.
                let at = nth(&text, miss, 0);
                let last = miss.chars().last().unwrap().len_utf8();
                replace(state, at.end - last..at.end, "");
                let want = format!("{}{}", &text[..at.end - last], &text[at.end..]);
                if state.doc().text.get_untracked() != want {
                    failures.push(format!("{case}: Backspace in {miss} took more"));
                }
            }
        }
    }
    report(failures);
}
