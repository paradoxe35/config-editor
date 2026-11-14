//! A key renamed in many places at once in a view (a replace-all renaming
//! `target`), saved to a host that writes it inside inline containers
//! (`services = [{ target = 22 }]`, in each of many blocks or tables): the rename is
//! made where the host writes each key, the host text read from its top a
//! few times whatever the number of renames (HCL read it again for each
//! renamed key's statement: 3 s for 900 renames in 10,000 lines).

use serde_json::Value;

use super::super::languages::Language;
use super::super::work_count::{Work, counted_all};
use super::Bridge;

/// A host text of `count` services written in a list of inline maps and
/// `count` more in blocks or tables, each holding `key`.
fn host_text(host: Language, count: usize, key: &str) -> String {
    let lines = |item: &dyn Fn(usize) -> String| (0..count).map(item).collect::<Vec<_>>();
    match host.label() {
        "HCL" => format!(
            "services = [\n{}\n]\n{}",
            lines(&|index| format!("  {{ {key} = {index}, name = \"s{index}\" }}")).join(",\n"),
            lines(&|index| format!(
                "site_{index} {{\n  {key} = {index}\n  opts = [{{ {key} = {index} }}]\n}}\n"
            ))
            .concat()
        ),
        "TOML" => format!(
            "services = [\n{}\n]\n{}",
            lines(&|index| format!("  {{ {key} = {index}, name = \"s{index}\" }}")).join(",\n"),
            lines(&|index| format!(
                "\n[site_{index}]\n{key} = {index}\nopts = [{{ {key} = {index} }}]\n"
            ))
            .concat()
        ),
        "YAML" => format!(
            "services: [\n{}\n]\n{}",
            lines(&|index| format!("  {{{key}: {index}, name: s{index}}}")).join(",\n"),
            lines(&|index| format!("site_{index}:\n  {key}: {index}\n  opts: [{{{key}: {index}}}]\n"))
                .concat()
        ),
        "JSON" => format!(
            "{{\n  \"services\": [\n{}\n  ],\n{}\n}}\n",
            lines(&|index| format!("    {{\"{key}\": {index}, \"name\": \"s{index}\"}}"))
                .join(",\n"),
            lines(&|index| format!(
                "  \"site_{index}\": {{\"{key}\": {index}, \"opts\": [{{\"{key}\": {index}}}]}}"
            ))
            .join(",\n")
        ),
        _ => format!(
            "<root>\n{}</root>\n",
            lines(&|index| format!(
                "  <site_{index}><{key}>{index}</{key}><opts><{key}>{index}</{key}></opts></site_{index}>\n"
            ))
            .concat()
        ),
    }
}

/// The kinds of work that read a whole text from its top.
const WHOLE_READS: [Work; 12] = [
    Work::DocumentRead,
    Work::PathLookup,
    Work::LineSplit,
    Work::SpanScan,
    Work::KeyScan,
    Work::Statements,
    Work::Spanning,
    Work::ConstructScan,
    Work::LineSeek,
    Work::CompletionContext,
    Work::LexicalState,
    Work::UngroupedParse,
];

/// The whole reads (by kind) and bytes parsed (per byte of the host text)
/// saving `count` renames of `target` made in a `view` of a `host` text.
fn work((host, view): (Language, Language), count: usize) -> (Vec<(Work, usize)>, f64, bool) {
    let bridge = Bridge::new(host, view, None).expect("two languages");
    let before = host_text(host, count, "target");
    let after = host_text(host, count, "goal");
    let wanted: Value = host.behavior().parse(&after).expect("reads");
    let edited = bridge
        .to_edit(&host.serialize(&wanted).expect("writes"))
        .expect("shows")
        .text;
    let (saved, done) = counted_all(|| bridge.to_host(&edited, &before));
    let saved = saved.unwrap_or_else(|error| panic!("{host} as {view}: {error:?}"));
    let reads = WHOLE_READS
        .into_iter()
        .map(|kind| {
            (
                kind,
                done[Work::ALL.iter().position(|it| *it == kind).unwrap()],
            )
        })
        .collect();
    let parsed = done[Work::ALL
        .iter()
        .position(|it| *it == Work::ParsedBytes)
        .unwrap()];
    (reads, parsed as f64 / before.len() as f64, saved == after)
}

#[test]
fn a_key_renamed_in_many_places_reads_the_host_text_a_few_times() {
    let mut grew = Vec::new();
    for &host in Language::ALL {
        for &view in Language::ALL {
            if host == view {
                continue;
            }
            let (few, few_parsed, kept) = work((host, view), 10);
            if !kept {
                grew.push(format!(
                    "{host} as {view}: 10 renames not made where written"
                ));
            }
            for count in [40, 200] {
                let (many, many_parsed, kept) = work((host, view), count);
                if !kept {
                    grew.push(format!(
                        "{host} as {view}: {count} renames not made where written"
                    ));
                }
                for ((kind, few), (_, many)) in few.iter().zip(many) {
                    if many > *few {
                        grew.push(format!(
                            "{host} as {view}: {kind:?} {few} for 10 renames, {many} for {count}"
                        ));
                    }
                }
                if many_parsed > 1.5 * few_parsed.max(1.0) {
                    grew.push(format!(
                        "{host} as {view}: {few_parsed:.1} parses of the text for 10 renames, {many_parsed:.1} for {count}"
                    ));
                }
            }
        }
    }
    assert!(grew.is_empty(), "{}", grew.join("\n"));
}
