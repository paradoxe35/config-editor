//! A number changed in another view is saved in the form the host wrote
//! the one it replaces in: its radix (`0x1F90`), digit separators
//! (TOML `1_000_000`), exponent (`1.5e3`, `1E+3`) and fraction digits
//! (`1.50`), where the new value fits it; every host and view. (XML reads a
//! number only written plainly: `1.50` there is text, saved as typed.)

use super::Language;
use super::save_matrix::{from_every_view, report, slot};
use serde_json::{Value, json};

/// A host text and its numbers: each field's pointer, the value it is
/// changed to, and how the host writes it then.
struct Host {
    language: Language,
    text: &'static str,
    numbers: &'static [(&'static str, &'static str, &'static str, &'static str)],
}

const HOSTS: [Host; 4] = [
    Host {
        language: Language::YAML,
        text: "a: 0x1F90 # hex\nc: 1.5e3\nd: 1.50\ne: 1E+3\nf: 0o755\n",
        numbers: &[
            ("/a", "8083", "0x1F90", "0x1F93"),
            ("/c", "1500.3", "1.5e3", "1.5003e3"),
            ("/d", "1.8", "1.50", "1.80"),
            ("/e", "1000.3", "1E+3", "1.0003E+3"),
            ("/f", "420", "0o755", "0o644"),
        ],
    },
    Host {
        language: Language::TOML,
        text: "a = 0x1F90 # hex\nb = 1_000_000\nc = 1.5e3\nd = 1.50\ne = 1E+3\nf = 0b1010\n",
        numbers: &[
            ("/a", "8083", "0x1F90", "0x1F93"),
            ("/b", "1000003", "1_000_000", "1_000_003"),
            ("/c", "1500.3", "1.5e3", "1.5003e3"),
            ("/d", "1.8", "1.50", "1.80"),
            ("/e", "1000.3", "1E+3", "1.0003E+3"),
            ("/f", "5", "0b1010", "0b101"),
        ],
    },
    Host {
        language: Language::JSON,
        text: "{\n  \"c\": 1.5e3,\n  \"d\": 1.50,\n  \"e\": 1E+3\n}\n",
        numbers: &[
            ("/c", "1500.3", "1.5e3", "1.5003e3"),
            ("/d", "1.8", "1.50", "1.80"),
            ("/e", "1000.3", "1E+3", "1.0003E+3"),
        ],
    },
    Host {
        language: Language::HCL,
        text: "c = 1.5e3\nd = 1.50\ne = 1E+3\n",
        numbers: &[
            ("/c", "1500.3", "1.5e3", "1.5003e3"),
            ("/d", "1.8", "1.50", "1.80"),
            ("/e", "1000.3", "1E+3", "1.0003E+3"),
        ],
    },
];

#[test]
fn a_changed_number_keeps_the_form_of_the_one_it_replaces() {
    let mut failures = Vec::new();
    for host in &HOSTS {
        for (pointer, new, was, now) in host.numbers {
            let value: Value = serde_json::from_str(new).unwrap();
            let change = |document: &mut Value| *slot(document, pointer) = value.clone();
            let expected = host.text.replacen(was, now, 1);
            from_every_view(
                &mut failures,
                pointer,
                host.language,
                host.text,
                &change,
                &expected,
            );
        }
    }
    report(failures);
}

/// What does not fit the form is written in the host writer's own: a
/// fraction where a hexadecimal integer was, a negative one.
#[test]
fn a_number_that_does_not_fit_the_form_is_written_plainly() {
    let mut failures = Vec::new();
    let host = "a: 0x1F90\nb: 2\n";
    let cases: [(Value, &str); 2] = [
        (json!(8083.5), "a: 8083.5\nb: 2\n"),
        (json!(-3), "a: -3\nb: 2\n"),
    ];
    for (value, expected) in cases {
        let change = |document: &mut Value| document["a"] = value.clone();
        from_every_view(&mut failures, "a", Language::YAML, host, &change, expected);
    }
    report(failures);
}
