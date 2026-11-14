//! A key deleted, renamed or moved in another language's view of a host
//! without comments, for every form of value the host writes: the save
//! patches only that key's lines (no whole document written anew), and
//! no other value is ever written in another form, reading the same (a
//! TOML hex number as decimal, a datetime as a string, an HCL expression
//! as text, a YAML tag or quoting dropped, XML markup lost: QA `E37-2`).
//! A save that cannot keep every form waits; here only for what the view
//! or host cannot hold at all, never for want of a patch.

use crate::editor::Language;
use crate::editor::bridge::WHOLE_RENDERS;
use crate::editor::codec::written_range;
use crate::editor::save_matrix::{ALL, find_slot, report, save_conversion};
use serde_json::{Map, Value};

/// Hosts without comments, each holding every form of value its language
/// writes in a map `a`, and the pointer of another map `b`.
fn hosts() -> Vec<(Language, &'static str, &'static str)> {
    vec![
        (
            Language::TOML,
            "top = 1\n\n[a]\nmask = 0xff\noct = 0o17\nbin = 0b11\nund = 1_000\nexp = 1e3\nwhen = 1979-05-27T07:32:00Z\nday = 1979-05-27\ntime = 07:32:00\nlit = 'x\\y'\nml = \"\"\"\nx\"\"\"\nmll = '''\nx'''\nml1 = \"\"\"x\"\"\"\nml2 = \"\"\"\na\nb\"\"\"\nesc = \"\\u0041\"\narr = [0x1, 'q']\ninl = { k = 0x2 }\n\n[b]\nz = 1\n",
            "/b",
        ),
        (
            Language::TOML,
            "[a]\nmask = 0xff\nwhen = 1979-05-27T07:32:00Z\nml = '''\nx'''\n\n[b]\nz = 1\nml = \"\"\"\ny\"\"\"\n",
            "/b",
        ),
        (
            Language::HCL,
            "top = 1\n\na {\n  ex   = var.x\n  tpl  = \"${var.y}-z\"\n  doc  = <<EOT\nx\nEOT\n  docs = <<-EOT\n    a\n    EOT\n  num  = 1e3\n  list = [var.a, 1]\n  obj  = { k = var.b }\n  fn   = upper(\"x\")\n}\n\nb {\n  z = 1\n}\n",
            "/b",
        ),
        (
            Language::YAML,
            "top: 1\na:\n  tag: !!str 1\n  custom: !Ref x\n  sq: 'x'\n  dq: \"x\"\n  lit: |\n    x\n  fold: >-\n    x\n  hex: 0x1f\n  oct: 0o17\n  nul: ~\n  exp: 1e3\n  flow: [1, 'q']\n  fmap: {k: 'v'}\n  date: 2001-12-14\nb:\n  z: 1\n",
            "/b",
        ),
        (
            Language::JSON,
            "{\n  \"top\": 1,\n  \"a\": {\n    \"f\": 1.0,\n    \"e\": 1e3,\n    \"esc\": \"\\u0041\",\n    \"arr\": [1, 2],\n    \"obj\": {\"k\": 1.50}\n  },\n  \"b\": {\"z\": 1}\n}\n",
            "/b",
        ),
        (
            Language::XML,
            "<r>\n  <top>1</top>\n  <a at=\"1\">\n    <cd><![CDATA[x]]></cd>\n    <ent>a &amp; b</ent>\n    <empty/>\n    <n>1</n>\n    <s>x</s>\n  </a>\n  <b>\n    <z>1</z>\n  </b>\n</r>\n",
            "/r/b",
        ),
    ]
}

#[derive(Clone, Copy, Debug)]
enum Op {
    Delete,
    Rename,
    /// To the end of its own map.
    MoveLast,
    /// To the end of the host's other map.
    MoveOut,
}

const OPS: [Op; 4] = [Op::Delete, Op::Rename, Op::MoveLast, Op::MoveOut];

fn escaped(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

fn renamed(key: &str) -> String {
    if key.starts_with('@') {
        "@renamed".to_string()
    } else {
        "renamed".to_string()
    }
}

/// `document` with `op` made to the key `key` of the map at `parent`
/// (`dest`: the other map); `false` where the view does not show them.
fn apply(document: &mut Value, parent: &str, key: &str, dest: &str, op: Op) -> bool {
    let Some(Value::Object(map)) = find_slot(document, parent) else {
        return false;
    };
    let Some(value) = map.get(key).cloned() else {
        return false;
    };
    match op {
        Op::Delete => {
            map.shift_remove(key);
        }
        Op::Rename => {
            let to = renamed(key);
            if map.contains_key(&to) {
                return false;
            }
            *map = std::mem::take(map)
                .into_iter()
                .map(|(name, value)| (if name == key { to.clone() } else { name }, value))
                .collect::<Map<_, _>>();
        }
        Op::MoveLast => {
            map.shift_remove(key);
            map.insert(key.to_string(), value);
        }
        Op::MoveOut => {
            map.shift_remove(key);
            let Some(Value::Object(other)) = find_slot(document, dest) else {
                return false;
            };
            if other.contains_key(key) {
                return false;
            }
            other.insert(key.to_string(), value);
        }
    }
    true
}

/// The pointers of the scalars under `value`.
fn scalars(value: &Value, pointer: String, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                scalars(child, format!("{pointer}/{}", escaped(key)), out);
            }
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                scalars(child, format!("{pointer}/{index}"), out);
            }
        }
        _ => out.push(pointer),
    }
}

/// Where a scalar at `pointer` of the host is after `op` on `touched`
/// (`None`: deleted, or moved to another map, whose forms the item trail
/// keeps (`moved_forms`), which a value-level save has not).
fn moved_to(pointer: &str, touched: &str, op: Op) -> Option<String> {
    let inside = pointer == touched || pointer.starts_with(&format!("{touched}/"));
    if !inside {
        return Some(pointer.to_string());
    }
    let (parent, key) = touched.rsplit_once('/')?;
    let rest = &pointer[touched.len()..];
    match op {
        Op::Delete | Op::MoveOut => None,
        Op::Rename => Some(format!(
            "{parent}/{}{rest}",
            escaped(&renamed(&key.replace("~1", "/").replace("~0", "~")))
        )),
        Op::MoveLast => Some(pointer.to_string()),
    }
}

/// The host's written text of the scalar at `pointer` of `text`.
fn written(host: Language, text: &str, pointer: &str) -> Option<String> {
    let behavior = host.behavior();
    let span = behavior.value_span(text, pointer)?;
    let range = written_range(behavior, text, span)?;
    Some(text[range].to_string())
}

/// Whether a save waited on what the view or the host holds no form for:
/// a TOML view of a null, an XML attribute moved among elements, or a
/// key moved to another map, whose forms (a YAML tag, an HCL expression)
/// only its item trail keeps (`moved_forms`).
fn holds_no(error: &str, op: Op) -> bool {
    error.contains("no null value")
        || error.contains("in another place than the view moved it to")
        || matches!(op, Op::MoveOut)
}

/// Each key of the map `a` (`/r/a` in XML) under each change from each
/// other view: a save keeps the written form of every value it does not
/// change (or waits), and patches only the key's lines.
#[test]
fn a_key_deleted_renamed_or_moved_keeps_every_form_and_is_patched() {
    let mut failures = Vec::new();
    let mut whole = Vec::new();
    let mut waited = Vec::new();
    let (mut tried, mut saved_count) = (0, 0);
    for (host, text, dest) in hosts() {
        let old = host.parse(text).expect("parses");
        let parent = if host == Language::XML { "/r/a" } else { "/a" };
        let keys: Vec<String> = old
            .pointer(parent)
            .and_then(Value::as_object)
            .expect("a map a")
            .keys()
            .filter(|key| *key != "#text")
            .cloned()
            .collect();
        let mut others = Vec::new();
        scalars(&old, String::new(), &mut others);
        for key in &keys {
            let touched = format!("{parent}/{}", escaped(key));
            for op in OPS {
                for edit in ALL.into_iter().filter(|edit| *edit != host) {
                    let shown = std::cell::Cell::new(true);
                    let change = |document: &mut Value| {
                        if !apply(document, parent, key, dest, op) {
                            shown.set(false);
                        }
                    };
                    let renders = WHOLE_RENDERS.with(std::cell::Cell::get);
                    let result = save_conversion(host, edit, text, &change);
                    if !shown.get() {
                        continue;
                    }
                    tried += 1;
                    let case = format!("{host} host, {touched} {op:?} via {edit}");
                    let (saved, _) = match result {
                        Ok(saved) => saved,
                        Err(error) => {
                            if !holds_no(&error, op) {
                                waited.push(format!("{case}: {error}"));
                            }
                            continue;
                        }
                    };
                    saved_count += 1;
                    if WHOLE_RENDERS.with(std::cell::Cell::get) != renders {
                        whole.push(format!("{case}\n--- saved\n{saved}"));
                    }
                    for pointer in &others {
                        let Some(now) = moved_to(pointer, &touched, op) else {
                            continue;
                        };
                        let (Some(was), is) =
                            (written(host, text, pointer), written(host, &saved, &now))
                        else {
                            continue;
                        };
                        if is.as_deref() != Some(was.as_str()) {
                            failures.push(format!(
                                "{case}: {pointer} written {was:?}, now {is:?} at {now}\n--- saved\n{saved}"
                            ));
                        }
                    }
                }
            }
        }
    }
    assert!(
        saved_count * 10 > tried * 6,
        "only {saved_count} of {tried} saved"
    );
    failures.extend(waited.into_iter().map(|case| format!("waited: {case}")));
    failures.extend(
        whole
            .into_iter()
            .map(|case| format!("written whole: {case}")),
    );
    report(failures);
}
