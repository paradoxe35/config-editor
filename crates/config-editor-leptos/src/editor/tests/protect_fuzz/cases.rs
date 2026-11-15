//! The documents the random protect runs edit: one protected field each
//! (its value written `{v}`), the places ordinary new entries go (`after`
//! a text, an entry made from a name), in every language: blocks, lists,
//! XML elements and attributes, HCL labelled blocks, TOML tables.

use crate::editor::Language;

/// A secret placeholder of the default pattern.
pub(super) const TOKEN: &str = "«secret 0b1c»";
/// A read-only field's value.
pub(super) const LOCKED: &str = "hunter2";
/// A new value the user writes over a secret: no message may quote it.
pub(super) const NEW_SECRET: &str = "s3cr3tZ";

/// Where an entry goes: after `after` (its first match), the entry made
/// from a name.
pub(super) struct Anchor {
    pub(super) after: &'static str,
    pub(super) entry: fn(&str) -> String,
}

pub(super) struct Case {
    pub(super) label: &'static str,
    pub(super) language: Language,
    pub(super) text: &'static str,
    /// The protected field.
    pub(super) pointer: &'static str,
    pub(super) anchors: &'static [Anchor],
}

/// Names new entries take: ordinary ones, and ones passing through or
/// starting with the protected names (`db`, `password`, `keys`, `web`).
pub(super) const NAMES: &[&str] = &[
    "dbname",
    "db_host",
    "d",
    "pass",
    "password2",
    "passwordx",
    "user2",
    "keys2",
    "k",
    "ami2",
    "webapp",
    "web2",
    "portx",
    "p",
    "zz",
    "db",
    "password",
    "key",
];

pub(super) const CASES: &[Case] = &[
    Case {
        label: "YAML block",
        language: Language::YAML,
        text: "db:\n  user: app\n  password: {v}\nport: 1\n",
        pointer: "/db/password",
        anchors: &[
            Anchor {
                after: "user: app",
                entry: |k| format!("\n  {k}: x"),
            },
            Anchor {
                after: "port: 1",
                entry: |k| format!("\n{k}: 1"),
            },
            Anchor {
                after: "db:",
                entry: |k| format!("\n  {k}: y"),
            },
        ],
    },
    Case {
        label: "YAML list",
        language: Language::YAML,
        text: "keys:\n  - {v}\n  - other\nport: 1\n",
        pointer: "/keys/0",
        anchors: &[
            Anchor {
                after: "keys:",
                entry: |k| format!("\n  - {k}"),
            },
            Anchor {
                after: "- other",
                entry: |k| format!("\n  - {k}"),
            },
            Anchor {
                after: "port: 1",
                entry: |k| format!("\n{k}: 2"),
            },
        ],
    },
    Case {
        label: "JSON object",
        language: Language::JSON,
        text: "{\n  \"db\": {\n    \"user\": \"app\",\n    \"password\": \"{v}\"\n  },\n  \"port\": 1\n}\n",
        pointer: "/db/password",
        anchors: &[
            Anchor {
                after: "\"user\": \"app\",",
                entry: |k| format!("\n    \"{k}\": \"x\","),
            },
            Anchor {
                after: "\"port\": 1",
                entry: |k| format!(",\n  \"{k}\": 2"),
            },
        ],
    },
    Case {
        label: "JSON list",
        language: Language::JSON,
        text: "{\n  \"keys\": [\n    \"{v}\",\n    \"other\"\n  ],\n  \"port\": 1\n}\n",
        pointer: "/keys/0",
        anchors: &[
            Anchor {
                after: "[",
                entry: |k| format!("\n    \"{k}\","),
            },
            Anchor {
                after: "\"other\"",
                entry: |k| format!(",\n    \"{k}\""),
            },
        ],
    },
    Case {
        label: "TOML table",
        language: Language::TOML,
        text: "port = 1\n\n[db]\nuser = \"app\"\npassword = \"{v}\"\n",
        pointer: "/db/password",
        anchors: &[
            Anchor {
                after: "user = \"app\"",
                entry: |k| format!("\n{k} = \"x\""),
            },
            Anchor {
                after: "port = 1",
                entry: |k| format!("\n{k} = 2"),
            },
            Anchor {
                after: "password = \"{v}\"",
                entry: |k| format!("\n\n[{k}]\na = 1"),
            },
        ],
    },
    Case {
        label: "HCL labelled block",
        language: Language::HCL,
        text: "resource \"aws_instance\" \"web\" {\n  ami = \"{v}\"\n  type = \"t3\"\n}\nport = 1\n",
        pointer: "/resource/aws_instance/web/ami",
        anchors: &[
            Anchor {
                after: "type = \"t3\"",
                entry: |k| format!("\n  {k} = \"x\""),
            },
            Anchor {
                after: "port = 1",
                entry: |k| format!("\nresource \"aws_instance\" \"{k}\" {{\n  ami = \"x\"\n}}"),
            },
            Anchor {
                after: "port = 1",
                entry: |k| format!("\n{k} {{\n  from = 1\n}}"),
            },
        ],
    },
    Case {
        label: "XML element",
        language: Language::XML,
        text: "<config>\n  <db>\n    <user>app</user>\n    <password>{v}</password>\n  </db>\n  <port>1</port>\n</config>\n",
        pointer: "/config/db/password",
        anchors: &[
            Anchor {
                after: "<user>app</user>",
                entry: |k| format!("\n    <{k}>x</{k}>"),
            },
            Anchor {
                after: "<port>1</port>",
                entry: |k| format!("\n  <{k}>2</{k}>"),
            },
        ],
    },
    Case {
        label: "XML list",
        language: Language::XML,
        text: "<config>\n  <keys>\n    <key>{v}</key>\n    <key>other</key>\n  </keys>\n</config>\n",
        pointer: "/config/keys/key/0",
        anchors: &[
            Anchor {
                after: "<keys>",
                entry: |k| format!("\n    <key>{k}</key>"),
            },
            Anchor {
                after: "<key>other</key>",
                entry: |k| format!("\n    <key>{k}</key>"),
            },
        ],
    },
    Case {
        label: "XML attribute",
        language: Language::XML,
        text: "<config>\n  <db user=\"app\" password=\"{v}\"/>\n  <port>1</port>\n</config>\n",
        pointer: "/config/db/password",
        anchors: &[
            Anchor {
                after: "<port>1</port>",
                entry: |k| format!("\n  <{k}>x</{k}>"),
            },
            Anchor {
                after: "user=\"app\"",
                entry: |k| format!(" {k}=\"5\""),
            },
        ],
    },
];

/// How the field is protected.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) enum Lock {
    /// A secret placeholder (`x-secret`), the host's secrets on.
    Secret,
    /// `read_only_paths`.
    Path,
    /// `read_only_ranges`: the field's whole line.
    Range,
    /// The schema's `readOnly`.
    Schema,
}

pub(super) const LOCKS: [Lock; 4] = [Lock::Secret, Lock::Path, Lock::Range, Lock::Schema];

/// The protected value for `lock`.
pub(super) fn value(lock: Lock) -> &'static str {
    if lock == Lock::Secret { TOKEN } else { LOCKED }
}

/// A schema marking `pointer` with `mark` (a `0` segment is a list's
/// items); a secret also has a pattern its new value breaks.
pub(super) fn schema(pointer: &str, lock: Lock) -> String {
    let mut schema = if lock == Lock::Secret {
        r#"{"type": "string", "x-secret": true, "pattern": "^[0-9]+$"}"#.to_string()
    } else {
        r#"{"type": "string", "readOnly": true}"#.to_string()
    };
    let keys: Vec<&str> = pointer.split('/').skip(1).collect();
    for key in keys.into_iter().rev() {
        schema = if key == "0" {
            format!(r#"{{"type": "array", "items": {schema}}}"#)
        } else {
            format!(r#"{{"type": "object", "properties": {{"{key}": {schema}}}}}"#)
        };
    }
    schema
}
