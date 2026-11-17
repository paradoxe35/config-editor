//! The documents the random sessions start from: real-world samples and
//! generated ones, each rich in the forms only its host writes (TOML
//! datetimes, hex and multi-line strings, inline tables and arrays of
//! tables; HCL expressions, heredocs, one-line objects and labelled
//! blocks; YAML tags, anchors, merge keys, quote styles, block scalars and
//! flow collections; XML attributes, prefixes, `xsi:type`, CDATA), with
//! comments everywhere the host has them.

use crate::editor::languages::Language;
use crate::editor::save_properties::Random;

/// Real-world samples, by host.
pub(super) fn samples(host: Language) -> &'static [&'static str] {
    match host.label() {
        "YAML" => &[COMPOSE, K8S],
        "JSON" => &[PACKAGE, SETTINGS],
        "TOML" => &[CARGO, PYPROJECT],
        "HCL" => &[MODULE, TERRAFORM],
        _ => &[POM, NLOG, WEB_CONFIG],
    }
}

/// A document for `host`: a sample or a generated one.
pub(super) fn document(host: Language, random: &mut Random) -> String {
    let samples = samples(host);
    if random.below(3) == 0 {
        return samples[random.below(samples.len())].to_string();
    }
    generated(host, random)
}

/// The entries of one host's forms, `K` standing for the key.
fn forms(host: Language) -> &'static [&'static str] {
    match host.label() {
        "YAML" => &[
            "K: !Ref Param",
            "K: !Sub \"${AWS::StackName}-data\"",
            "K: 'single'",
            "K: \"double\"",
            "K: |\n  line one\n  line two",
            "K: >-\n  folded\n  text",
            "K: [a, 'b', 3]",
            "K: {x: 1, y: two}",
            "K: 0x1F",
            "K: 1.50",
            "K: plain words",
            "K: ~",
            "K: !!str 5",
            "K:\n  - !Ref A\n  - 'q'\n  - 3",
            "K:\n  - name: one\n    port: 80\n  - name: two\n    port: 81",
            "K:\n  base: &bK 5\n  copy: *bK",
        ],
        "JSON" => &[
            "\"K\": 1.50",
            "\"K\": 1e3",
            "\"K\": \"a\\u0041\"",
            "\"K\": {\"x\": 1, \"y\": \"two\"}",
            "\"K\": [1, 2, 3]",
            "\"K\": true",
            "\"K\": null",
            "\"K\": \"plain\"",
            "\"K\": [\n  {\"name\": \"one\"},\n  {\"name\": \"two\"}\n]",
        ],
        "TOML" => &[
            "K = 1979-05-27T07:32:00Z",
            "K = 1979-05-27",
            "K = 07:32:00",
            "K = 1979-05-27T07:32:00",
            "K = 0x1F",
            "K = 0o17",
            "K = 0b101",
            "K = 1_000",
            "K = 'C:\\dir'",
            "K = \"\"\"\nmulti\nline\"\"\"",
            "K = '''\nraw text\n'''",
            "K = { x = 1, y = \"two\" }",
            "K = [1, 0x2, 3]",
            "K = \"plain\"",
            "K = 3.5e2",
            "K = true",
        ],
        "HCL" => &[
            "K = var.name",
            "K = upper(var.name)",
            "K = \"web-${var.env}\"",
            "K = <<EOT\nhello\nEOT",
            "K = { a = 1, b = \"x\" }",
            "K = [1, 2]",
            "K = aws_subnet.main.id",
            "K = local.count + 1",
            "K = var.on ? 1 : 2",
            "K = \"plain\"",
            "K = true",
            "K = 1.5",
            "K {\n  inner = var.x\n}",
            "K \"label\" {\n  inner = 1\n}",
            "K { one = true }",
        ],
        _ => &[
            "<K>text</K>",
            "<K a=\"1\" b=\"x\"/>",
            "<K xsi:type=\"File\" name=\"f\"/>",
            "<K><![CDATA[x < y]]></K>",
            "<p:K>v</p:K>",
            "<K id=\"1\">text</K>",
            "<K xsi:nil=\"true\"/>",
            "<K>\n  <item>a</item>\n  <item>b</item>\n</K>",
            "<K>42</K>",
        ],
    }
}

/// A generated document: two to four blocks of two to four entries in the
/// host's forms, with comments before and after some (`note N`, each its
/// own).
pub(super) fn generated(host: Language, random: &mut Random) -> String {
    let forms = forms(host);
    let mut key = 0;
    let mut note = 0;
    let mut next_note = |random: &mut Random| -> Option<String> {
        (random.below(3) == 0).then(|| {
            note += 1;
            format!("note {note}")
        })
    };
    let names = ["alpha", "beta", "gamma", "delta"];
    let blocks = 2 + random.below(3);
    let mut out = Vec::new();
    for name in &names[..blocks] {
        let count = 2 + random.below(3);
        let mut entries = Vec::new();
        for _ in 0..count {
            key += 1;
            let entry = forms[random.below(forms.len())].replace('K', &format!("k{key}"));
            entries.push((entry, next_note(random), next_note(random)));
        }
        out.push((name.to_string(), next_note(random), entries));
    }
    write(host, &out)
}

type Block = (
    String,
    Option<String>,
    Vec<(String, Option<String>, Option<String>)>,
);

/// The blocks written in `host`: (name, comment above, entries (text,
/// comment above, comment after its first line)).
fn write(host: Language, blocks: &[Block]) -> String {
    let comment = |body: &str| match host.label() {
        "XML" => format!("<!-- {body} -->"),
        _ => format!("# {body}"),
    };
    let indent = match host.label() {
        "TOML" => "",
        "XML" | "JSON" => "    ",
        _ => "  ",
    };
    let mut text = String::new();
    if host.label() == "XML" {
        text.push_str(
            "<root xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xmlns:p=\"urn:p\">\n",
        );
    } else if host.label() == "JSON" {
        text.push_str("{\n");
    }
    for (at, (name, above, entries)) in blocks.iter().enumerate() {
        if host.label() != "JSON"
            && let Some(above) = above
        {
            let pad = if host.label() == "XML" { "  " } else { "" };
            text.push_str(&format!("{pad}{}\n", comment(above)));
        }
        match host.label() {
            "YAML" => text.push_str(&format!("{name}:\n")),
            "JSON" => text.push_str(&format!("  \"{name}\": {{\n")),
            "TOML" => text.push_str(&format!("[{name}]\n")),
            "HCL" => text.push_str(&format!("{name} {{\n")),
            _ => text.push_str(&format!("  <{name}>\n")),
        }
        for (index, (entry, above, after)) in entries.iter().enumerate() {
            if host.label() != "JSON"
                && let Some(above) = above
            {
                text.push_str(&format!("{indent}{}\n", comment(above)));
            }
            let entry = if entry.contains("<<EOT") || host.label() == "TOML" {
                entry.clone()
            } else {
                entry.replace('\n', &format!("\n{indent}"))
            };
            let mut lines: Vec<String> = entry.lines().map(str::to_string).collect();
            if host.label() == "JSON" && index + 1 < entries.len() {
                let last = lines.len() - 1;
                lines[last].push(',');
            }
            // A comment after a one-line entry's value.
            if host.label() != "JSON"
                && lines.len() == 1
                && let Some(after) = after
            {
                lines[0].push_str(&format!(" {}", comment(after)));
            }
            text.push_str(&format!("{indent}{}\n", lines.join("\n")));
        }
        match host.label() {
            "JSON" if at + 1 < blocks.len() => text.push_str("  },\n"),
            "JSON" => text.push_str("  }\n"),
            "HCL" => text.push_str("}\n"),
            "XML" => text.push_str(&format!("  </{name}>\n")),
            "TOML" if at + 1 < blocks.len() => text.push('\n'),
            _ => {}
        }
    }
    match host.label() {
        "XML" => text.push_str("</root>\n"),
        "JSON" => text.push_str("}\n"),
        _ => {}
    }
    text
}

const COMPOSE: &str = r#"# compose file
x-common: &common
  restart: unless-stopped # always back
  logging:
    driver: "json-file"
services:
  web:
    <<: *common
    image: 'nginx:1.25'
    ports:
      - "8080:80" # http
      - 8443:443
    environment:
      MODE: production
      DEBUG: "false"
    command: >-
      nginx -g
      'daemon off;'
  db:
    image: postgres:16
    # data lives here
    volumes: [dbdata:/var/lib/postgresql/data]
    healthcheck: {test: [CMD, pg_isready], interval: 10s}
volumes:
  dbdata: {}
"#;

const K8S: &str = r#"apiVersion: apps/v1
kind: Deployment
metadata:
  name: web # the app
  labels: {app: web, tier: "front"}
spec:
  replicas: 0x3
  selector:
    matchLabels:
      app: web
  template:
    spec:
      containers:
        - name: web
          image: "nginx:1.25"
          args: [--port, '8080']
          env:
            - name: GREETING
              value: |
                hello
                world
            - name: LEVEL
              value: !!str 3
          resources:
            limits: {cpu: 500m, memory: 128Mi}
"#;

const PACKAGE: &str = r#"{
  "name": "web-app",
  "version": "1.0.0",
  "private": true,
  "scripts": {
    "build": "vite build",
    "test": "vitest run"
  },
  "dependencies": {"react": "^18.3.1", "react-dom": "^18.3.1"},
  "engines": {
    "node": ">=18"
  },
  "files": ["dist", "src"],
  "ratio": 1.50,
  "size": 1e3
}
"#;

const SETTINGS: &str = r#"{
  "editor.fontSize": 14,
  "files.exclude": {
    "**/.git": true,
    "**/node_modules": true
  },
  "servers": [
    {"name": "one", "port": 80},
    {"name": "two", "port": 81}
  ],
  "nothing": null,
  "escaped": "a\u0041b"
}
"#;

const CARGO: &str = r#"# the package
[package]
name = "demo"
version = "0.1.0" # bump on release
edition = "2021"
authors = ['Ann <ann@example.com>']

[dependencies]
serde = { version = "1", features = ["derive"] }
tokio = { version = "1.38", features = ["rt", "macros"] } # runtime
log = "0.4"

[profile.release]
opt-level = 3
lto = true
codegen-units = 0x1

# binaries
[[bin]]
name = "cli"
path = "src/cli.rs"

[[bin]]
name = "server"
path = "src/server.rs"
"#;

const PYPROJECT: &str = r#"[project]
name = "tool"
released = 1979-05-27T07:32:00Z # first
description = """
A tool
over lines"""
paths = 'C:\tools'

[tool.lint]
line-length = 1_000
select = ["E", "F"] # rules
mask = 0o755
ignore = { tests = ["E501"] }

[tool.cache]
day = 1979-05-27
at = 07:32:00
"#;

const MODULE: &str = include_str!("../../../../tests/fixtures/real/module.tf");

const TERRAFORM: &str = r#"# network
resource "aws_subnet" "main" {
  vpc_id     = aws_vpc.main.id # the vpc
  cidr_block = cidrsubnet(var.cidr, 8, 1)
  tags = { Name = "main-${var.env}", Tier = "app" }
}

locals {
  count = 3
  names = ["a", "b"]
  script = <<EOT
#!/bin/sh
echo hi
EOT
}

module "web" {
  source = "./web"
  size   = var.big ? 3 : 1
  # pinned
  ami = data.aws_ami.ubuntu.id
}
"#;

const POM: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
  <modelVersion>4.0.0</modelVersion>
  <groupId>com.example</groupId>
  <artifactId>demo</artifactId>
  <!-- release line -->
  <version>1.0.0</version>
  <properties>
    <java.version>17</java.version>
  </properties>
  <dependencies>
    <dependency>
      <groupId>org.slf4j</groupId>
      <artifactId>slf4j-api</artifactId>
      <version>2.0.13</version> <!-- logging -->
    </dependency>
    <dependency>
      <groupId>junit</groupId>
      <artifactId>junit</artifactId>
      <scope>test</scope>
    </dependency>
  </dependencies>
</project>
"#;

const NLOG: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<nlog xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" autoReload="true">
  <!-- where logs go -->
  <targets>
    <target xsi:type="File" name="file" fileName="app.log"/>
    <target xsi:type="Console" name="console"/>
  </targets>
  <rules>
    <logger name="*" minlevel="Info" writeTo="file"/>
    <logger name="Microsoft.*" maxlevel="Info" final="true"/> <!-- quiet -->
  </rules>
  <variable name="dir"><![CDATA[C:\logs & more]]></variable>
</nlog>
"#;

const WEB_CONFIG: &str = include_str!("../../../../tests/fixtures/real/web.config");
