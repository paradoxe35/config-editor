//! Shrunk sessions the random save-promise test found, kept as regression
//! tests (those of an open finding ignored, by its name).

use super::ops::{Op, Put, Take};
use super::replay;
use crate::editor::languages::Language;

#[test]
fn fz38_1_1() {
    replay(
        Language::HCL,
        Language::HCL,
        "# note 3\nalpha {\n  k1 = [1, 2]\n  # note 1\n  k2 = <<EOT\nhello\nEOT\n}\n# note 6\nbeta {\n  # note 4\n  k3 = var.on ? 1 : 2\n  k4 = aws_subnet.main.id\n  k5 = { a = 1, b = \"x\" }\n  k6 = { a = 1, b = \"x\" } # note 5\n}\ngamma {\n  k7 = upper(var.name) # note 7\n  k8 = \"plain\" # note 8\n  k9 = \"web-${var.env}\" # note 9\n}\n# note 12\ndelta {\n  k10 = { a = 1, b = \"x\" } # note 10\n  k11 = <<EOT\nhello\nEOT\n}\n",
        &[
            Op::Type {
                line: 38,
                column: 1000,
                text: "x",
            },
            Op::Type {
                line: 10,
                column: 1000,
                text: "0",
            },
            Op::ReplaceAll {
                pick: 39,
                suffix: "_x",
            },
            Op::Switch { to: 0 },
        ],
    );
}

#[test]
fn fz38_4_1() {
    replay(
        Language::HCL,
        Language::TOML,
        "alpha {\n  # note 1\n  k5 = { a = 1, b = \"x\" } # note 6\n  k6 = true\n}\n",
        &[Op::Cut {
            line: 60,
            how: Take::Entry,
        }],
    );
}

#[test]
fn fz38_11_1() {
    replay(
        Language::HCL,
        Language::YAML,
        "alpha {\n  k1 = aws_subnet.main.id # note 1\n  k2 = 1.5\n}\nbeta {\n  k3 { one = true } # note 4\n  k4 = upper(var.name)\n}\n",
        &[
            Op::Cut {
                line: 35,
                how: Take::Entry,
            },
            Op::Pause,
            Op::Type {
                line: 42,
                column: 1000,
                text: "x",
            },
            Op::Paste {
                line: 0,
                how: Put::OverBlank,
            },
        ],
    );
}

#[test]
fn fz38_1_2() {
    replay(
        Language::TOML,
        Language::TOML,
        "[alpha]\nk1 = 0o17 # note 1\n# note 2\nk2 = true\nk3 = 1979-05-27 # note 3\n# note 4\nk4 = 1979-05-27T07:32:00Z\n\n[beta]\nk5 = 07:32:00\nk6 = 0o17 # note 5\n# note 6\nk7 = 1979-05-27T07:32:00Z # note 7\n# note 8\nk8 = 0x1F\n\n[gamma]\nk9 = 07:32:00\nk10 = true\n\n[delta]\nk11 = 1979-05-27T07:32:00\nk12 = 0x1F # note 9\nk13 = 0x1F\n# note 10\nk14 = 0x1F\n",
        &[
            Op::Type {
                line: 5,
                column: 1001,
                text: "0",
            },
            Op::Enter {
                line: 60,
                column: 1001,
            },
            Op::Type {
                line: 59,
                column: 1000,
                text: ".",
            },
            Op::Switch { to: 0 },
            Op::Type {
                line: 13,
                column: 28,
                text: ":",
            },
        ],
    );
}

#[test]
fn fz38_4_2() {
    replay(
        Language::XML,
        Language::HCL,
        "<root xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xmlns:p=\"urn:p\">\n  <alpha>\n    <k1 id=\"1\">text</k1> <!-- note 1 -->\n    <k2>text</k2> <!-- note 2 -->\n    <p:k3>v</p:k3>\n    <!-- note 4 -->\n    <p:k4>v</p:k4>\n  </alpha>\n  <beta>\n    <k5 a=\"1\" b=\"x\"/> <!-- note 6 -->\n    <k6>\n      <item>a</item>\n      <item>b</item>\n    </k6>\n  </beta>\n</root>\n",
        &[
            Op::Enter {
                line: 55,
                column: 30,
            },
            Op::MoveLine {
                line: 61,
                down: true,
            },
            Op::Pause,
            Op::DeleteLine { line: 44 },
            Op::DeleteLine { line: 39 },
            Op::CopyLine {
                line: 25,
                down: false,
            },
        ],
    );
}

#[test]
fn fz38_11_2() {
    replay(
        Language::XML,
        Language::TOML,
        "<root xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xmlns:p=\"urn:p\">\n  <alpha>\n    <p:k1>v</p:k1>\n    <k2>\n      <item>b</item>\n    </k2>\n    <k3 id=\"1\">text</k3>\n    <k4>42</k4>\n  </alpha>\n  <beta>\n    <k5 xsi:nil=\"true\"/> <!-- note 2 -->\n    <k6>text</k6>\n    <p:k7>v</p:k7>\n  </beta>\n  <gamma>\n    <k8>\n    </k8>\n    <k9>text</k9> <!-- note 3 -->\n  </gamma>\n</root>\n",
        &[
            Op::Cut {
                line: 18,
                how: Take::Line,
            },
            Op::Backspace {
                line: 10,
                column: 1000,
                count: 1,
            },
            Op::Pause,
            Op::Paste {
                line: 17,
                how: Put::Before,
            },
        ],
    );
}

#[test]
fn fz38_4_3() {
    replay(
        Language::XML,
        Language::TOML,
        "<root xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xmlns:p=\"urn:p\">\n  <alpha>\n    <k1 id=\"1\">text</k1>\n    <k2>42</k2> <!-- note 1 -->\n    <k3 a=\"1\" b=\"x\"/>\n  </alpha>\n  <beta>\n    <k4 xsi:nil=\"true\"/>\n    <k5 a=\"1\" b=\"x\"/> <!-- note 5 -->\n  </beta>\n  <gamma>\n    <k6><![CDATA[x < y]]></k6> <!-- note 6 -->\n    <k7 xsi:nil=\"true\"/> <!-- note 7 -->\n  </gamma>\n</root>\n",
        &[
            Op::MoveLine {
                line: 28,
                down: false,
            },
            Op::DeleteLine { line: 43 },
            Op::Pause,
            Op::Undo,
            Op::Cut {
                line: 6,
                how: Take::Line,
            },
        ],
    );
}

#[test]
fn fz38_4_4() {
    replay(
        Language::YAML,
        Language::HCL,
        "alpha:\n  k1: \"double\" # note 2\n  k2: {x: 1, y: two} # note 3\n  k3: {x: 1, y: two}\n  k4: 1.50\nbeta:\n  k5: \"double\"\n  k6: 'single'\n",
        &[
            Op::Cut {
                line: 43,
                how: Take::Entry,
            },
            Op::DeleteLine { line: 40 },
            Op::Paste {
                line: 55,
                how: Put::OverBlank,
            },
            Op::Pause,
            Op::Paste {
                line: 46,
                how: Put::Before,
            },
        ],
    );
}

#[test]
fn fz38_4_5() {
    replay(
        Language::YAML,
        Language::TOML,
        "alpha:\n  k1:\n    - 3\n  k2: 'single' # note 3\nbeta:\n  k5: plain words\n  k6: {x: 1, y: two} # note 8\n",
        &[
            Op::Enter {
                line: 53,
                column: 1001,
            },
            Op::Enter {
                line: 36,
                column: 1000,
            },
            Op::DeleteLine { line: 6 },
        ],
    );
}

#[test]
fn fz38_1_3() {
    replay(
        Language::YAML,
        Language::XML,
        "alpha:\n  # note 1\n  k1: {x: 1, y: two} # note 2\n  k2: !Ref Param\nbeta:\n  # note 3\n  k3: [a, 'b', 3] # note 4\n  # note 5\n  k4: ~\n  k5: plain words\n",
        &[
            Op::Switch { to: 0 },
            Op::Cut {
                line: 52,
                how: Take::Entry,
            },
            Op::Switch { to: 3 },
        ],
    );
}

#[test]
fn fz38_1_4() {
    replay(
        Language::YAML,
        Language::YAML,
        "# compose file\nx-common: &common\n  restart: unless-stopped # always back\n  logging:\n    driver: \"json-file\"\nservices:\n  web:\n    <<: *common\n    image: 'nginx:1.25'\n    ports:\n      - \"8080:80\" # http\n      - 8443:443\n    environment:\n      MODE: production\n      DEBUG: \"false\"\n    command: >-\n      nginx -g\n      'daemon off;'\n  db:\n    image: postgres:16\n    # data lives here\n    volumes: [dbdata:/var/lib/postgresql/data]\n  dbdata: {}\n",
        &[
            Op::Cut {
                line: 21,
                how: Take::Entry,
            },
            Op::Switch { to: 2 },
        ],
    );
}

#[test]
fn fz38_1_5() {
    replay(
        Language::HCL,
        Language::HCL,
        "# note 1\nalpha {\n  k1 = var.on ? 1 : 2\n  k2 \"label\" {\n    inner = 1\n  }\n  k3 = { a = 1, b = \"x\" } # note 3\n  k4 = \"plain\"\n  k5 = 1.5 # note 4\n}\ngamma {\n  k6 = \"web-${var.env}\" # note 6\n  k7 = true\n  k8 { one = true }\n  k9 = \"plain\"\n}\n",
        &[
            Op::Cut {
                line: 2,
                how: Take::Text,
            },
            Op::Switch { to: 2 },
            Op::Cut {
                line: 0,
                how: Take::Entry,
            },
            Op::Cut {
                line: 45,
                how: Take::Text,
            },
            Op::Pause,
            Op::Paste {
                line: 4,
                how: Put::OverBlank,
            },
        ],
    );
}

#[test]
fn fz38_4_6() {
    replay(
        Language::HCL,
        Language::TOML,
        "terraform {\n  required_version = \">= 1.6\"\n  backend \"s3\" {\n    bucket = \"tf-state\"\n    key    = \"app/terraform.tfstate\"\n    region = \"eu-west-1\"\n  }\n}\nprovider \"aws\" {\n  region = var.region\n}\nprovider \"aws\" {\n  alias  = \"us\"\n  region = \"us-east-1\"\n}\nvariable \"region\" {\n  type    = string\n  default = \"eu-west-1\"\n  validation {\n    condition     = can(regex(\"^eu-\", var.region))\n    error_message = \"Only EU regions.\"\n  }\n}\nvariable \"subnets\" {\n  type = map(object({\n  }))\n  default = {\n    a = { cidr = \"10.0.1.0/24\", az = \"eu-west-1a\" }\n    b = { cidr = \"10.0.2.0/24\", az = \"eu-west-1b\" }\n  }\n}\ndata \"aws_ami\" \"ubuntu\" {\n  owners      = [\"099720109477\"]\n  filter {\n  }\n}\n",
        &[Op::PasteWhole { line: 34 }],
    );
}

#[test]
fn fz38_4_7() {
    replay(
        Language::HCL,
        Language::YAML,
        "alpha {\n  k1 = 1.5\n  k2 = { a = 1, b = \"x\" }\n}\nbeta {\n  k3 = [1, 2] # note 1\n  k4 = [1, 2]\n  k5 = true # note 2\n  k6 = [1, 2]\n}\ngamma {\n  k7 = <<EOT\nhello\nEOT\n  k8 = \"web-${var.env}\" # note 5\n  k9 = 1.5\n}\n",
        &[
            Op::DeleteLine { line: 15 },
            Op::Cut {
                line: 45,
                how: Take::Text,
            },
        ],
    );
}

#[test]
fn fz38_12_1() {
    replay(
        Language::JSON,
        Language::HCL,
        "{\n  \"name\": \"web-app\",\n  \"version\": \"1.0.0\",\n  \"private\": true,\n  \"scripts\": {\n    \"test\": \"vitest run\"\n  },\n  \"files\": [\"dist\", \"src\"],\n  \"size\": 1e3\n}\n",
        &[
            Op::Backspace {
                line: 3,
                column: 3,
                count: 2,
            },
            Op::Pause,
            Op::Type {
                line: 38,
                column: 1001,
                text: "x",
            },
            Op::Undo,
            Op::Undo,
            Op::PasteWhole { line: 37 },
        ],
    );
}

#[test]
fn fz38_4_8() {
    replay(
        Language::JSON,
        Language::HCL,
        "{\n  \"alpha\": {\n    \"k1\": [\n      {\"name\": \"one\"},\n      {\"name\": \"two\"}\n    ],\n    \"k2\": \"plain\"\n  },\n  \"beta\": {\n    \"k3\": \"plain\",\n    \"k4\": \"a\\u0041\",\n    \"k5\": 1.50,\n    \"k6\": [1, 2, 3]\n  },\n  \"gamma\": {\n    \"k7\": 1e3,\n    \"k8\": true\n  },\n  \"delta\": {\n    \"k9\": 1.50,\n    \"k10\": {\"x\": 1, \"y\": \"two\"},\n    \"k11\": \"plain\"\n  }\n}\n",
        &[Op::MoveLine {
            line: 61,
            down: true,
        }],
    );
}

#[test]
fn fz38_4_9() {
    replay(
        Language::JSON,
        Language::TOML,
        "{\n  \"name\": \"web-app\",\n  \"version\": \"1.0.0\",\n  \"private\": true,\n  \"scripts\": {\n    \"build\": \"vite build\",\n    \"test\": \"vitest run\"\n  },\n  \"dependencies\": {\"react\": \"^18.3.1\", \"react-dom\": \"^18.3.1\"},\n  \"engines\": {\n    \"node\": \">=18\"\n  },\n  \"files\": [\"dist\", \"src\"],\n  \"ratio\": 1.50,\n  \"size\": 1e3\n}\n",
        &[
            Op::Cut {
                line: 32,
                how: Take::Entry,
            },
            Op::Pause,
            Op::Paste {
                line: 1,
                how: Put::Before,
            },
        ],
    );
}

#[test]
fn fz38_4_10() {
    replay(
        Language::JSON,
        Language::XML,
        "{\n  \"alpha\": {\n    \"k7\": 1e3,\n    \"k8\": true\n  }\n}\n",
        &[Op::MoveLine {
            line: 1,
            down: false,
        }],
    );
}

#[test]
fn fz38_4_11() {
    replay(
        Language::JSON,
        Language::YAML,
        "{\n  \"alpha\": {\n    \"k1\": \"plain\",\n    \"k2\": 1.50\n  },\n  \"beta\": {\n    \"k3\": 1.50,\n    \"k4\": true,\n    \"k5\": true\n  },\n  \"gamma\": {\n    \"k6\": \"a\\u0041\",\n    \"k7\": null,\n    \"k8\": \"plain\"\n  },\n  \"delta\": {\n    \"k9\": [\n      {\"name\": \"one\"},\n      {\"name\": \"two\"}\n    ],\n    \"k10\": \"plain\",\n    \"k11\": [\n      {\"name\": \"one\"},\n      {\"name\": \"two\"}\n    ],\n    \"k12\": true\n  }\n}\n",
        &[
            Op::Backspace {
                line: 19,
                column: 1000,
                count: 3,
            },
            Op::ReplaceAll {
                pick: 48,
                suffix: "_x",
            },
        ],
    );
}

#[test]
fn fz38_1_6() {
    replay(
        Language::TOML,
        Language::HCL,
        "[project]\nname = \"tool\"\nreleased = 1979-05-27T07:32:00Z # first\ndescription = \"\"\"\nA tool\nover lines\"\"\"\npaths = 'C:\\tools'\nat = 07:32:00\n",
        &[
            Op::Cut {
                line: 31,
                how: Take::Entry,
            },
            Op::Switch { to: 2 },
            Op::Paste {
                line: 29,
                how: Put::OverLine,
            },
            Op::Switch { to: 4 },
        ],
    );
}

#[test]
fn fz38_4_12() {
    replay(
        Language::TOML,
        Language::HCL,
        "[alpha]\nk1 = 07:32:00\nk2 = \"plain\" # note 1\nk3 = 1979-05-27T07:32:00\n[beta]\nk4 = 'C:\\dir'\nk5 = 1979-05-27\n[gamma]\nk6 = { x = 1, y = \"two\" }\nk7 = 1979-05-27T07:32:00Z\nk8 = { x = 1, y = \"two\" } # note 6\n[delta]\nk9 = true # note 8\nk10 = 'C:\\dir'\nk11 = 0o17\nk12 = 0b101\n",
        &[Op::MoveLine {
            line: 31,
            down: true,
        }],
    );
}

#[test]
fn e38_1_1() {
    replay(
        Language::TOML,
        Language::JSON,
        "[alpha]\nk1 = \"\"\"\nmulti\nline\"\"\"\nk2 = 0x1F # note 1\n\n[beta]\nk3 = 1979-05-27 # note 3\n[gamma]\n",
        &[
            Op::Switch { to: 0 },
            Op::Paste {
                line: 3,
                how: Put::OverBlank,
            },
            Op::Cut {
                line: 5,
                how: Take::Text,
            },
        ],
    );
}

#[test]
fn fz38_4_13() {
    replay(
        Language::TOML,
        Language::JSON,
        "[alpha]\nk1 = 1_000\nk3 = [1, 0x2, 3]\n",
        &[
            Op::Switch { to: 3 },
            Op::MoveLine {
                line: 1,
                down: false,
            },
        ],
    );
}

#[test]
fn fz38_1_7() {
    replay(
        Language::TOML,
        Language::TOML,
        "\n[beta]\nk4 = 0o17 # note 2\nk5 = 0x1F\n",
        &[
            Op::MoveLine {
                line: 54,
                down: false,
            },
            Op::Switch { to: 1 },
        ],
    );
}

#[test]
fn fz38_4_14() {
    replay(
        Language::TOML,
        Language::XML,
        "[alpha]\nk1 = 1_000\nk2 = 'C:\\dir' # note 2\nk3 = [1, 0x2, 3]\n[beta]\nk4 = true # note 5\nk5 = 1979-05-27\nk6 = 07:32:00\nk7 = true\n[gamma]\nk8 = { x = 1, y = \"two\" }\nk9 = 0b101\n[delta]\nk10 = \"plain\"\nk11 = 1979-05-27T07:32:00 # note 7\nk12 = 1979-05-27T07:32:00 # note 9\n",
        &[Op::MoveLine {
            line: 50,
            down: true,
        }],
    );
}

#[test]
fn fz38_4_15() {
    replay(
        Language::TOML,
        Language::YAML,
        "[alpha]\nk1 = 1_000 # note 1\nk2 = \"\"\"\nmulti\nline\"\"\"\n[beta]\nk3 = 07:32:00 # note 4\nk5 = true # note 5\n",
        &[Op::MoveLine {
            line: 6,
            down: false,
        }],
    );
}

#[test]
fn fz38_4_16() {
    replay(
        Language::XML,
        Language::HCL,
        "<root xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xmlns:p=\"urn:p\">\n  <alpha>\n    <k1>text</k1> <!-- note 2 -->\n    <k2 a=\"1\" b=\"x\"/>\n    <k3><![CDATA[x < y]]></k3>\n    <k4><![CDATA[x < y]]></k4> <!-- note 5 -->\n  </alpha>\n  <beta>\n    <k5>text</k5>\n    <k6 xsi:nil=\"true\"/>\n  </beta>\n  <gamma>\n    <k7 a=\"1\" b=\"x\"/>\n    <k8 xsi:nil=\"true\"/>\n    <k9 a=\"1\" b=\"x\"/>\n    <k10 xsi:nil=\"true\"/>\n  </gamma>\n</root>\n",
        &[
            Op::Switch { to: 2 },
            Op::Cut {
                line: 30,
                how: Take::Entry,
            },
            Op::ReplaceAll {
                pick: 27,
                suffix: "Y",
            },
        ],
    );
}

#[test]
fn fz38_4_17() {
    replay(
        Language::XML,
        Language::TOML,
        "<nlog xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" autoReload=\"true\">\n  <targets>\n    <target xsi:type=\"File\" name=\"file\" fileName=\"app.log\"/>\n  </targets>\n</nlog>\n",
        &[Op::MoveLine {
            line: 3,
            down: true,
        }],
    );
}

#[test]
fn fz38_1_8() {
    replay(
        Language::XML,
        Language::XML,
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<nlog xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" autoReload=\"true\">\n  <!-- where logs go -->\n  <targets>\n    <target xsi:type=\"File\" name=\"file\" fileName=\"app.log\"/>\n    <target xsi:type=\"Console\" name=\"console\"/>\n  </targets>\n  <rules>\n    <logger name=\"*\" minlevel=\"Info\" writeTo=\"file\"/>\n    <logger name=\"Microsoft.*\" maxlevel=\"Info\" final=\"true\"/> <!-- quiet -->\n  </rules>\n  <variable name=\"dir\"><![CDATA[C:\\logs & more]]></variable>\n</nlog>\n",
        &[
            Op::Cut {
                line: 6,
                how: Take::Line,
            },
            Op::Cut {
                line: 27,
                how: Take::Line,
            },
            Op::Switch { to: 2 },
        ],
    );
}

#[test]
fn fz38_4_18() {
    replay(
        Language::XML,
        Language::YAML,
        "<root xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xmlns:p=\"urn:p\">\n  <alpha>\n    <k1 a=\"1\" b=\"x\"/>\n  </alpha>\n  <beta>\n    <k7>\n      <item>b</item>\n    </k7>\n  </beta>\n  <gamma>\n    <k10>\n      <item>b</item>\n    </k10>\n    <k11>\n      <item>b</item>\n    </k11>\n  </gamma>\n</root>\n",
        &[Op::Cut {
            line: 0,
            how: Take::Text,
        }],
    );
}

#[test]
fn fz38_1_9() {
    replay(
        Language::YAML,
        Language::HCL,
        "alpha:\n  k1: {x: 1, y: two}\n  k2: !Ref Param\n",
        &[
            Op::Switch { to: 0 },
            Op::Undo,
            Op::Cut {
                line: 0,
                how: Take::Line,
            },
            Op::Backspace {
                line: 11,
                column: 10,
                count: 3,
            },
        ],
    );
}

#[test]
fn fz38_4_19() {
    replay(
        Language::YAML,
        Language::HCL,
        "alpha:\n  k1: 1.50\n  k2: 'single' # note 2\n  k3:\n    - !Ref A\n    - 'q'\n    - 3\nbeta:\n  k4: 0x1F\n  k5: 0x1F\ngamma:\n  k6: !!str 5\n  k7:\n    base: &bk7 5\n    copy: *bk7\n  k8: plain words\n",
        &[Op::MoveLine {
            line: 61,
            down: false,
        }],
    );
}

#[test]
fn e38_1_2() {
    replay(
        Language::YAML,
        Language::JSON,
        "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: web # the app\n  labels: {app: web, tier: \"front\"}\nspec:\n  replicas: 0x3\n  selector:\n      app: web\n      containers:\n        - name: web\n          image: \"nginx:1.25\"\n          args: [--port, '8080']\n          env:\n            - name: GREETING\n              value: |\n                hello\n                world\n            - name: LEVEL\n              value: !!str 3\n          resources:\n            limits: {cpu: 500m, memory: 128Mi}\n",
        &[
            Op::Switch { to: 2 },
            Op::Paste {
                line: 25,
                how: Put::OverLine,
            },
            Op::Type {
                line: 63,
                column: 8,
                text: "x",
            },
        ],
    );
}

#[test]
fn e38_1_3() {
    replay(
        Language::YAML,
        Language::TOML,
        "alpha:\n  k1: >-\n    text\n  k2: 0x1F # note 2\n  k3: 1.50 # note 4\n  k4: {x: 1, y: two} # note 5\nbeta:\n  k5: \"double\" # note 6\n  k6: 1.50\n  k7: 1.50\n  k8: {x: 1, y: two}\n",
        &[
            Op::Cut {
                line: 51,
                how: Take::Entry,
            },
            Op::Paste {
                line: 55,
                how: Put::OverBlank,
            },
            Op::Switch { to: 1 },
        ],
    );
}

#[test]
fn fz38_4_20() {
    replay(
        Language::YAML,
        Language::TOML,
        "alpha:\n  k1: 0x1F\n",
        &[Op::Cut {
            line: 0,
            how: Take::Line,
        }],
    );
}

#[test]
fn fz38_4_21() {
    replay(
        Language::YAML,
        Language::XML,
        "alpha:\n  k1: 1.50 # note 1\n  k2:\n    base: &bk2 5\n    copy: *bk2\n  k3: ~\n  k4:\n    base: &bk4 5\n    copy: *bk4\nbeta:\n  k5: [a, 'b', 3] # note 4\n  k6: \"double\" # note 6\n  k7: !Sub \"${AWS::StackName}-data\"\n  k8: {x: 1, y: two}\ngamma:\n  k9: \"double\" # note 8\n  k10: plain words\n  k11:\n    - !Ref A\n    - 'q'\n    - 3\n  k12: >-\n",
        &[Op::MoveLine {
            line: 32,
            down: false,
        }],
    );
}

#[test]
#[ignore = "open: FZ38-8"]
fn fz38_8_1() {
    replay(
        Language::YAML,
        Language::XML,
        "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: web # the app\n  labels: {app: web, tier: \"front\"}\nspec:\n  replicas: 0x3\n  selector:\n    matchLabels:\n      app: web\n  template:\n    spec:\n      containers:\n        - name: web\n          image: \"nginx:1.25\"\n          args: [--port, '8080']\n          env:\n            - name: GREETING\n              value: |\n                hello\n                world\n            - name: LEVEL\n              value: !!str 3\n          resources:\n            limits: {cpu: 500m, memory: 128Mi}\n",
        &[Op::DeleteLine { line: 62 }],
    );
}

#[test]
fn fz38_1_10() {
    replay(
        Language::YAML,
        Language::YAML,
        "alpha:\n  k1: ~ # note 1\n  k2: 0x1F\n# note 4\nbeta:\n  k3: 0x1F\n  # note 2\n  k4: 0x1F # note 3\ngamma:\n  k5: !Ref Param\n  k6: 0x1F\n  k7: 'single'\n# note 7\ndelta:\n  k8: !Sub \"${AWS::StackName}-data\"\n  # note 5\n  k9: >-\n    folded\n    text\n  k10: |\n    line one\n    line two\n  k11:\n    - name: one\n      port: 80\n    - name: two\n      port: 81\n",
        &[
            Op::Cut {
                line: 35,
                how: Take::Text,
            },
            Op::Switch { to: 3 },
            Op::Cut {
                line: 21,
                how: Take::Line,
            },
            Op::Type {
                line: 30,
                column: 1000,
                text: "x",
            },
            Op::Undo,
            Op::Undo,
        ],
    );
}

#[test]
fn fz38_16_1() {
    replay(
        Language::YAML,
        Language::YAML,
        "# compose file\nx-common: &common\n  restart: unless-stopped # always back\n  logging:\n    driver: \"json-file\"\nservices:\n  web:\n    <<: *common\n    image: 'nginx:1.25'\n    ports:\n      - \"8080:80\" # http\n      - 8443:443\n    environment:\n      MODE: production\n      DEBUG: \"false\"\n    command: >-\n      nginx -g\n      'daemon off;'\n  db:\n    image: postgres:16\n    # data lives here\n    volumes: [dbdata:/var/lib/postgresql/data]\n    healthcheck: {test: [CMD, pg_isready], interval: 10s}\nvolumes:\n  dbdata: {}\n",
        &[
            Op::MoveLine {
                line: 24,
                down: false,
            },
            Op::Switch { to: 1 },
        ],
    );
}

#[test]
fn fz38_3_1() {
    replay(
        Language::HCL,
        Language::JSON,
        "resource \"aws_subnet\" \"main\" {\n  script = <<EOT\nEOT\n}\n",
        &[Op::Cut {
            line: 3,
            how: Take::Entry,
        }],
    );
}

#[test]
fn fz38_2_1() {
    replay(
        Language::HCL,
        Language::YAML,
        "alpha {\n  k1 = \"web-${var.env}\" # note 1\n}\nbeta {\n}\ngamma {\n}\n",
        &[Op::DeleteLine { line: 45 }],
    );
}

#[test]
fn fz38_2_2() {
    replay(
        Language::JSON,
        Language::YAML,
        "{\n  \"alpha\": {\n    \"k1\": 1.50,\n    \"k2\": [1, 2, 3],\n    \"k3\": null,\n    \"k4\": 1e3\n  },\n  \"beta\": {\n    \"k5\": true,\n    \"k6\": \"a\\u0041\"\n  },\n  \"gamma\": {\n    \"k7\": \"plain\",\n    \"k8\": \"plain\",\n    \"k9\": [1, 2, 3]\n  },\n  \"delta\": {\n    \"k10\": 1e3,\n    \"k11\": [\n      {\"name\": \"one\"},\n      {\"name\": \"two\"}\n    ],\n    \"k12\": true\n  }\n}\n",
        &[Op::MoveLine {
            line: 26,
            down: false,
        }],
    );
}

#[test]
fn fz38_2_3() {
    replay(
        Language::TOML,
        Language::YAML,
        "[alpha]\nk1 = 'C:\\dir'\nk2 = [1, 0x2, 3]\nk3 = 0o17\n[beta]\nk4 = 'C:\\dir'\nk5 = 07:32:00\nk6 = \"\"\"\nmulti\nline\"\"\"\n",
        &[Op::MoveLine {
            line: 54,
            down: false,
        }],
    );
}

#[test]
#[ignore = "open: FZ38-3"]
fn fz38_3_2() {
    replay(
        Language::XML,
        Language::TOML,
        "<root xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xmlns:p=\"urn:p\">\n  <alpha>\n    <k1>text</k1>\n    <k2 xsi:type=\"File\" name=\"f\"/>\n    <k3 id=\"1\">text</k3>\n  </alpha>\n  <beta>\n    <k4 a=\"1\" b=\"x\"/> <!-- note 4 -->\n    <k5 xsi:nil=\"true\"/>\n    <p:k6>v</p:k6> <!-- note 6 -->\n    <k7 a=\"1\" b=\"x\"/>\n  </beta>\n  <gamma>\n    <k8>42</k8> <!-- note 7 -->\n    <k9 xsi:type=\"File\" name=\"f\"/> <!-- note 9 -->\n    <k10>text</k10>\n    <k11>\n      <item>b</item>\n    </k11>\n  </gamma>\n  <delta>\n    <k12 xsi:type=\"File\" name=\"f\"/>\n    <k13 id=\"1\">text</k13>\n    <p:k14>v</p:k14>\n  </delta>\n</root>\n",
        &[
            Op::Type {
                line: 49,
                column: 25,
                text: "#",
            },
            Op::Cut {
                line: 54,
                how: Take::Entry,
            },
        ],
    );
}

#[test]
fn fz38_10_1() {
    replay(
        Language::HCL,
        Language::YAML,
        "beta {\n  k13 {\n  }\n}\n",
        &[
            Op::Type {
                line: 44,
                column: 1000,
                text: "x",
            },
            Op::Pause,
            Op::Undo,
            Op::Switch { to: 1 },
        ],
    );
}

#[test]
fn fz38_10_2() {
    replay(
        Language::XML,
        Language::JSON,
        "<configuration>\n  <appSettings>\n  </appSettings>\n  <connectionStrings>\n  </connectionStrings>\n  <system.web>\n  </system.web>\n</configuration>\n",
        &[
            Op::Type {
                line: 4,
                column: 1001,
                text: "x",
            },
            Op::Pause,
            Op::Undo,
            Op::Switch { to: 2 },
        ],
    );
}

#[test]
fn fz38_10_3() {
    replay(
        Language::XML,
        Language::YAML,
        "<configuration>\n  <appSettings>\n  </appSettings>\n  <connectionStrings>\n  </connectionStrings>\n  <system.web>\n  </system.web>\n</configuration>\n",
        &[
            Op::Switch { to: 2 },
            Op::Type {
                line: 14,
                column: 1000,
                text: "x",
            },
            Op::Pause,
            Op::Undo,
            Op::Switch { to: 0 },
        ],
    );
}

#[test]
fn fz38_4_22() {
    replay(
        Language::YAML,
        Language::TOML,
        "alpha:\n  k1: >-\n    text\n  k2: 0x1F # note 2\n  k3: 1.50 # note 4\n  k4: {x: 1, y: two} # note 5\nbeta:\n  k5: \"double\" # note 6\n  k6: 1.50\n  k7: 1.50\n  k8: {x: 1, y: two}\n",
        &[Op::Cut {
            line: 51,
            how: Take::Entry,
        }],
    );
}

#[test]
fn fz38_4_23() {
    replay(
        Language::JSON,
        Language::XML,
        "{\n  \"alpha\": {\n    \"k7\": 1e3,\n    \"k8\": true\n  }\n}\n",
        &[
            Op::PasteWhole { line: 30 },
            Op::MoveLine {
                line: 1,
                down: false,
            },
            Op::Cut {
                line: 1,
                how: Take::Entry,
            },
        ],
    );
}

#[test]
fn fz38_4_24() {
    replay(
        Language::TOML,
        Language::HCL,
        "[alpha]\nk1 = 1979-05-27T07:32:00Z # note 2\nk2 = 1979-05-27T07:32:00Z # note 4\n[beta]\nk4 = 1_000 # note 6\nk5 = 1979-05-27T07:32:00Z # note 7\n[gamma]\nk6 = 0o17 # note 10\nk7 = '''\n'''\n[delta]\nk8 = '''\nraw text\n'''\n",
        &[
            Op::PasteWhole { line: 38 },
            Op::MoveLine {
                line: 16,
                down: true,
            },
        ],
    );
}

#[test]
fn fz38_4_25() {
    replay(
        Language::TOML,
        Language::XML,
        "[project]\nname = \"tool\"\nreleased = 1979-05-27T07:32:00Z # first\ndescription = \"\"\"\nA tool\nover lines\"\"\"\npaths = 'C:\\tools'\n[tool.lint]\nline-length = 1_000\nselect = [\"E\", \"F\"] # rules\nmask = 0o755\nignore = { tests = [\"E501\"] }\n[tool.cache]\nday = 1979-05-27\nat = 07:32:00\n",
        &[
            Op::Switch { to: 0 },
            Op::PasteWhole { line: 4 },
            Op::DeleteLine { line: 47 },
        ],
    );
}

#[test]
fn fz38_4_26() {
    replay(
        Language::TOML,
        Language::YAML,
        "[alpha]\nk1 = 3.5e2 # note 2\nk2 = 'C:\\dir'\nk3 = 0o17 # note 4\n[beta]\nk4 = '''\nraw text\n'''\nk5 = 3.5e2 # note 6\nk6 = 0o17\n[gamma]\nk7 = 07:32:00 # note 7\nk11 = 0x1F\n",
        &[
            Op::Cut {
                line: 52,
                how: Take::Line,
            },
            Op::Type {
                line: 7,
                column: 11,
                text: "x",
            },
            Op::Pause,
            Op::Undo,
        ],
    );
}

#[test]
fn fz38_4_27() {
    replay(
        Language::XML,
        Language::TOML,
        "<root xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xmlns:p=\"urn:p\">\n  <alpha>\n    <k1 xsi:nil=\"true\"/>\n    <k2 id=\"1\">text</k2>\n    <k3><![CDATA[x < y]]></k3>\n  </alpha>\n  <beta>\n    <k4>\n      <item>b</item>\n    </k4>\n    <k5>text</k5> <!-- note 3 -->\n    <k6 a=\"1\" b=\"x\"/>\n  </beta>\n  <gamma>\n    <k7><![CDATA[x < y]]></k7>\n    <k8 xsi:type=\"File\" name=\"f\"/>\n    <k9>text</k9>\n    <k10>42</k10>\n  </gamma>\n</root>\n",
        &[
            Op::Cut {
                line: 27,
                how: Take::Line,
            },
            Op::Paste {
                line: 13,
                how: Put::Before,
            },
            Op::PasteWhole { line: 9 },
        ],
    );
}

#[test]
fn fz38_4_28() {
    replay(
        Language::XML,
        Language::YAML,
        "<root xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xmlns:p=\"urn:p\">\n  <alpha>\n    <k1>42</k1> <!-- note 1 -->\n    <k2>\n      <item>a</item>\n      <item>b</item>\n    </k2>\n    <p:k3>v</p:k3>\n    <p:k4>v</p:k4>\n  </alpha>\n  <beta>\n    <k5 xsi:type=\"File\" name=\"f\"/> <!-- note 2 -->\n    <k6><![CDATA[x < y]]></k6>\n  </beta>\n  <gamma>\n    <k7>text</k7> <!-- note 5 -->\n    <k8 id=\"1\">text</k8> <!-- note 7 -->\n    <k9 a=\"1\" b=\"x\"/> <!-- note 9 -->\n    <k10><![CDATA[x < y]]></k10>\n  </gamma>\n</root>\n",
        &[
            Op::PasteWhole { line: 9 },
            Op::Cut {
                line: 22,
                how: Take::Text,
            },
        ],
    );
}

#[test]
fn fz38_4_29() {
    replay(
        Language::YAML,
        Language::HCL,
        "alpha:\n  k1: >-\n  k3:\n    - !Ref A\n  k5: !Sub \"${AWS::StackName}-data\"\n  k6:\n    base: &bk6 5\n",
        &[Op::Type {
            line: 55,
            column: 1001,
            text: "x",
        }],
    );
}

#[test]
fn fz38_4_30() {
    replay(
        Language::YAML,
        Language::TOML,
        "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: web # the app\n  labels: {app: web, tier: \"front\"}\nspec:\n  replicas: 0x3\n  selector:\n      app: web\n      containers:\n        - name: web\n          image: \"nginx:1.25\"\n          args: [--port, '8080']\n          env:\n            - name: GREETING\n              value: |\n                hello\n                world\n            - name: LEVEL\n              value: !!str 3\n          resources:\n            limits: {cpu: 500m, memory: 128Mi}\n",
        &[Op::Backspace {
            line: 45,
            column: 1001,
            count: 1,
        }],
    );
}

#[test]
fn fz38_1_11() {
    replay(
        Language::YAML,
        Language::YAML,
        "alpha:\n  k1:\n    - name: two\n  # note 1\n  k2: >-\n    folded\n  # note 10\n",
        &[
            Op::DeleteLine { line: 24 },
            Op::Switch { to: 3 },
            Op::Type {
                line: 0,
                column: 1001,
                text: "x",
            },
        ],
    );
}

/// P2: a `-` typed before a list item's `{` in an HCL view makes the
/// item a string, which a TOML array of tables cannot hold: the save
/// that writes the array inline would drop the comment above its header,
/// so it waits.
#[test]
fn sv38_18_1() {
    replay(
        Language::TOML,
        Language::HCL,
        "[package]\nname = \"demo\"\nversion = \"0.1.0\" # bump on release\nedition = \"2021\"\nauthors = ['Ann <ann@example.com>']\n[dependencies]\nserde = { version = \"1\", features = [\"derive\"] }\ntokio = { version = \"1.38\", features = [\"rt\", \"macros\"] } # runtime\nlog = \"0.4\"\n[profile.release]\nopt-level = 3\nlto = true\ncodegen-units = 0x1\n# binaries\n[[bin]]\nname = \"cli\"\npath = \"src/cli.rs\"\n[[bin]]\nname = \"server\"\n",
        &[
            Op::Enter {
                line: 0,
                column: 1000,
            },
            Op::Type {
                line: 42,
                column: 1001,
                text: "-",
            },
        ],
    );
}
