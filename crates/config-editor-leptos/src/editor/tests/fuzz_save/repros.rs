//! Shrunk sessions the random save-promise test found, kept as regression
//! tests (those of an open finding ignored, by its name).

use super::ops::{Op, Put, Take};
use super::replay;
use crate::editor::languages::Language;

/// P2: an edit made in the host's own language stays in the host text when a
/// view of another language saves after it.
#[test]
fn hcl_edits_and_a_replace_all_in_the_host_survive_a_switch_to_yaml() {
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

/// P2: values a cut TOML header leaves at the root keep the host comment above
/// them.
#[test]
fn a_cut_toml_header_keeps_the_hcl_comment_above_the_values_it_held() {
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

/// P2: a cut and paste in a view keeps the host's comments.
#[test]
fn an_hcl_entry_cut_and_pasted_back_in_yaml_keeps_the_hosts_comments() {
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

/// P2: an edit made in the host's own language stays in the host text when a
/// view of another language saves after it.
#[test]
fn toml_typing_in_the_host_survives_a_switch_and_an_edit_in_yaml() {
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

/// P2: line moves, deletes and copies in a view keep the host comment above an
/// element they leave alone.
#[test]
fn an_xml_comment_above_an_element_survives_line_moves_in_an_hcl_view() {
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

/// P2: a cut and paste in a view keeps the host's comments.
#[test]
fn an_xml_line_cut_and_pasted_back_in_toml_keeps_the_hosts_comments() {
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

/// P2: values whose block, table or element changes (a header cut or renamed,
/// a line moved across one) keep their host text.
#[test]
fn xml_values_keep_their_form_after_a_toml_line_move_and_an_undo() {
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

/// P2: values whose block, table or element changes (a header cut or renamed,
/// a line moved across one) keep their host text.
#[test]
fn yaml_values_keep_their_form_when_hcl_pastes_move_them_to_another_block() {
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

/// P2: a line deleted in a view keeps the comment of a flow map the view
/// writes as its own table.
#[test]
fn a_yaml_flow_maps_comment_survives_a_line_deleted_in_a_toml_view() {
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

/// P2: an edit made in the host's own language stays in the host text when a
/// view of another language saves after it.
#[test]
fn a_yaml_cut_made_after_leaving_an_xml_view_survives_a_switch_to_hcl() {
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

/// P2: an edit made in the host's own language stays in the host text when a
/// view of another language saves after it.
#[test]
fn a_yaml_entry_cut_in_the_host_stays_cut_after_a_switch_to_toml() {
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

/// P2: an edit made in the host's own language stays in the host text when a
/// view of another language saves after it.
#[test]
fn hcl_cuts_in_the_host_survive_cuts_and_a_paste_in_a_toml_view() {
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

/// P3: a whole-text paste in a view that renames a block's label moves its
/// values: they keep their host form.
#[test]
fn a_whole_text_paste_renaming_an_hcl_block_keeps_its_values_forms() {
    replay(
        Language::HCL,
        Language::TOML,
        "terraform {\n  required_version = \">= 1.6\"\n  backend \"s3\" {\n    bucket = \"tf-state\"\n    key    = \"app/terraform.tfstate\"\n    region = \"eu-west-1\"\n  }\n}\nprovider \"aws\" {\n  region = var.region\n}\nprovider \"aws\" {\n  alias  = \"us\"\n  region = \"us-east-1\"\n}\nvariable \"region\" {\n  type    = string\n  default = \"eu-west-1\"\n  validation {\n    condition     = can(regex(\"^eu-\", var.region))\n    error_message = \"Only EU regions.\"\n  }\n}\nvariable \"subnets\" {\n  type = map(object({\n  }))\n  default = {\n    a = { cidr = \"10.0.1.0/24\", az = \"eu-west-1a\" }\n    b = { cidr = \"10.0.2.0/24\", az = \"eu-west-1b\" }\n  }\n}\ndata \"aws_ami\" \"ubuntu\" {\n  owners      = [\"099720109477\"]\n  filter {\n  }\n}\n",
        &[Op::PasteWhole { line: 34 }],
    );
}

/// P2: values whose block, table or element changes (a header cut or renamed,
/// a line moved across one) keep their host text.
#[test]
fn an_hcl_list_left_alone_by_a_yaml_line_delete_keeps_its_one_line_form() {
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

/// P2: a whole-text paste in a view rewrites only what it changed: untouched
/// objects keep their lines.
#[test]
fn a_whole_text_paste_after_undos_in_hcl_keeps_json_objects_over_lines() {
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

/// P2: values whose block, table or element changes (a header cut or renamed,
/// a line moved across one) keep their host text.
#[test]
fn json_values_keep_their_form_when_an_hcl_line_moves_across_blocks() {
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

/// P2: values whose block, table or element changes (a header cut or renamed,
/// a line moved across one) keep their host text.
#[test]
fn a_toml_table_cut_and_pasted_elsewhere_keeps_the_json_spellings() {
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

/// P2: values whose block, table or element changes (a header cut or renamed,
/// a line moved across one) keep their host text.
#[test]
fn a_json_value_moved_out_of_its_xml_element_keeps_its_spelling() {
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

/// P2: values whose block, table or element changes (a header cut or renamed,
/// a line moved across one) keep their host text.
#[test]
fn json_values_keep_their_form_after_a_yaml_backspace_and_a_rename() {
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

/// P2: an edit made in the host's own language stays in the host text when a
/// view of another language saves after it.
#[test]
fn a_toml_paste_made_after_leaving_an_hcl_view_survives_a_switch_to_xml() {
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

/// P2: values whose block, table or element changes (a header cut or renamed,
/// a line moved across one) keep their host text.
#[test]
fn toml_values_keep_their_form_when_an_hcl_line_moves_across_blocks() {
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

/// P3: a paste over a selected line in a view keeps the forms of the host's
/// values.
#[test]
fn toml_values_keep_their_form_after_a_paste_over_a_blank_line_in_yaml() {
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

/// P2: values whose block, table or element changes (a header cut or renamed,
/// a line moved across one) keep their host text.
#[test]
fn a_toml_value_moved_out_of_its_hcl_block_keeps_its_spelling() {
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

/// P2: an edit made in the host's own language stays in the host text when a
/// view of another language saves after it.
#[test]
fn a_toml_line_moved_in_the_host_keeps_its_spelling_through_a_switch() {
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

/// P2: values whose block, table or element changes (a header cut or renamed,
/// a line moved across one) keep their host text.
#[test]
fn toml_values_keep_their_form_when_an_xml_line_moves_across_elements() {
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

/// P2: values whose block, table or element changes (a header cut or renamed,
/// a line moved across one) keep their host text.
#[test]
fn toml_values_keep_their_form_when_a_yaml_line_moves_across_maps() {
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

/// P2: values whose block, table or element changes (a header cut or renamed,
/// a line moved across one) keep their host text.
#[test]
fn xml_values_keep_their_form_after_a_toml_cut_and_a_rename() {
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

/// P2: values whose block, table or element changes (a header cut or renamed,
/// a line moved across one) keep their host text.
#[test]
fn xml_attributes_keep_their_form_when_a_toml_line_moves_across_tables() {
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

/// P2: an edit made in the host's own language stays in the host text when a
/// view of another language saves after it.
#[test]
fn xml_lines_cut_in_the_host_keep_attributes_through_a_switch_to_toml() {
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

/// P2: values whose block, table or element changes (a header cut or renamed,
/// a line moved across one) keep their host text.
#[test]
fn xml_values_keep_their_form_when_their_yaml_root_key_is_cut() {
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

/// P2: an edit made in the host's own language stays in the host text when a
/// view of another language saves after it.
#[test]
fn yaml_edits_after_leaving_an_hcl_view_and_an_undo_stay_in_the_host() {
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

/// P2: values whose block, table or element changes (a header cut or renamed,
/// a line moved across one) keep their host text.
#[test]
fn yaml_values_keep_their_form_when_an_hcl_line_moves_across_blocks() {
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

/// P3: a paste over a selected line in a view keeps the forms of the host's
/// values.
#[test]
fn yaml_values_keep_their_form_after_a_paste_over_a_line_in_toml() {
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

/// P3: a paste over a selected line in a view keeps the forms of the host's
/// values.
#[test]
fn a_toml_header_cut_and_pasted_over_a_blank_line_keeps_the_yaml_forms() {
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

/// P2: values whose block, table or element changes (a header cut or renamed,
/// a line moved across one) keep their host text.
#[test]
fn a_yaml_hex_keeps_its_spelling_when_its_toml_header_is_cut() {
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

/// P2: values whose block, table or element changes (a header cut or renamed,
/// a line moved across one) keep their host text.
#[test]
fn yaml_values_keep_their_form_when_an_xml_line_moves_across_elements() {
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

/// P2: an edit to a list item in a view keeps the item's other values as the
/// host wrote them.
#[test]
fn a_line_deleted_in_a_list_item_keeps_the_items_other_yaml_forms() {
    replay(
        Language::YAML,
        Language::XML,
        "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: web # the app\n  labels: {app: web, tier: \"front\"}\nspec:\n  replicas: 0x3\n  selector:\n    matchLabels:\n      app: web\n  template:\n    spec:\n      containers:\n        - name: web\n          image: \"nginx:1.25\"\n          args: [--port, '8080']\n          env:\n            - name: GREETING\n              value: |\n                hello\n                world\n            - name: LEVEL\n              value: !!str 3\n          resources:\n            limits: {cpu: 500m, memory: 128Mi}\n",
        &[Op::DeleteLine { line: 62 }],
    );
}

/// P2: an edit made in the host's own language stays in the host text when a
/// view of another language saves after it.
#[test]
fn a_yaml_cut_in_the_host_keeps_its_tags_through_undos_in_hcl() {
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

/// P2: an edit made in the host's own language stays in the host text when a
/// view of another language saves after it.
#[test]
fn a_yaml_line_moved_into_another_map_in_the_host_survives_a_switch() {
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

/// P2: a block a cut empties in a view stays in the host as the view reads it,
/// not dropped.
#[test]
fn an_hcl_block_emptied_by_a_cut_in_json_saves_what_the_view_reads() {
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

/// P2: a null in a view is saved as null, not as an empty block; where the
/// host cannot write one, the save waits.
#[test]
fn a_yaml_null_left_by_a_deleted_line_is_no_empty_hcl_block() {
    replay(
        Language::HCL,
        Language::YAML,
        "alpha {\n  k1 = \"web-${var.env}\" # note 1\n}\nbeta {\n}\ngamma {\n}\n",
        &[Op::DeleteLine { line: 45 }],
    );
}

/// P2: a null in a view is saved as null, not as an empty map or list.
#[test]
fn a_json_null_stays_null_after_a_yaml_line_move() {
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

/// P2: a null in a view that the host cannot write is not saved as an empty
/// list: the save waits.
#[test]
fn a_yaml_null_a_toml_host_cannot_hold_is_no_empty_list() {
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

/// P2: a `#` typed right after a TOML value starts a comment, which the
/// save does not write into the value.
#[test]
fn a_hash_typed_after_a_toml_value_starts_a_comment() {
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

/// P2: a switch after an undone edit saves, or shows why the save waits.
#[test]
fn an_undo_in_yaml_then_a_switch_saves_an_empty_hcl_block_without_waiting() {
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

/// P2: a switch after an undone edit saves, or shows why the save waits.
#[test]
fn an_undo_in_json_then_a_switch_saves_empty_xml_elements_without_waiting() {
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

/// P2: a switch after an undone edit saves, or shows why the save waits.
#[test]
fn an_undo_in_yaml_then_a_switch_saves_empty_xml_elements_without_waiting() {
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

/// P2: values a cut TOML header leaves at the root keep the host comment of a
/// flow map among them.
#[test]
fn a_cut_toml_header_keeps_the_yaml_comment_in_a_flow_map_under_it() {
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

/// P2: a whole-text paste in a view rewrites only what it changed: values it
/// moves to another block keep their host text.
#[test]
fn json_values_keep_their_form_after_a_whole_paste_and_a_move_in_xml() {
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

/// P2: a whole-text paste in a view rewrites only what it changed: values it
/// moves to another block keep their host text.
#[test]
fn toml_values_keep_their_form_after_a_whole_paste_and_a_move_in_hcl() {
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

/// P2: a whole-text paste in a view rewrites only what it changed: values it
/// moves to another block keep their host text.
#[test]
fn a_toml_inline_table_keeps_its_form_after_a_whole_paste_in_yaml() {
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

/// P2: values whose block, table or element changes (a header cut or renamed,
/// a line moved across one) keep their host text.
#[test]
fn toml_values_keep_their_form_after_a_yaml_cut_and_an_undo_past_a_save() {
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

/// P2: values whose block, table or element changes (a header cut or renamed,
/// a line moved across one) keep their host text.
#[test]
fn xml_values_keep_their_form_after_a_toml_line_cut_pasted_elsewhere() {
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

/// P2: a whole-text paste in a view rewrites only what it changed: values it
/// moves to another block keep their host text.
#[test]
fn xml_values_keep_their_form_after_a_whole_paste_and_a_cut_in_yaml() {
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

/// P2: values whose block, table or element changes (a header cut or renamed,
/// a line moved across one) keep their host text.
#[test]
fn yaml_values_keep_their_form_when_a_typed_letter_renames_their_block() {
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

/// P2: values whose block, table or element changes (a header cut or renamed,
/// a line moved across one) keep their host text.
#[test]
fn yaml_values_keep_their_form_when_a_backspace_renames_their_table() {
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

/// P2: an edit made in the host's own language stays in the host text when a
/// view of another language saves after it.
#[test]
fn a_yaml_line_deleted_in_the_host_stays_deleted_after_an_edit_in_hcl() {
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
fn a_save_that_would_drop_the_comment_above_an_array_of_tables_waits() {
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

/// P2: an undo past a save brings an item back with the comment lines its source has above it.
#[test]
fn an_undo_past_a_save_brings_a_block_back_with_its_comment() {
    replay(
        Language::HCL,
        Language::TOML,
        "# note 4\nalpha {\n  k1 \"label\" {\n    inner = 1\n  }\n  k2 = local.count + 1 # note 2\n  k3 = local.count + 1\n  k4 {\n    inner = var.x\n  }\n}\nbeta {\n  k5 = 1.5 # note 5\n  k6 = aws_subnet.main.id\n  k7 = aws_subnet.main.id # note 7\n}\n",
        &[
            Op::Cut {
                line: 14,
                how: Take::Entry,
            },
            Op::Type {
                line: 10,
                column: 1000,
                text: "0",
            },
            Op::ReplaceAll {
                pick: 55,
                suffix: "2",
            },
            Op::Pause,
            Op::Undo,
        ],
    );
}

/// P2: a table header cut and pasted back (its block empty) comes back with
/// the comment lines its source has above it.
#[test]
fn an_empty_tables_header_cut_and_pasted_back_keeps_the_comment_above() {
    replay(
        Language::HCL,
        Language::TOML,
        "alpha {\n  k1 = true # note 2\n  k2 = aws_subnet.main.id # note 4\n  k3 = \"web-${var.env}\"\n  k4 = <<EOT\nhello\nEOT\n}\nbeta {\n  k5 = aws_subnet.main.id\n  k6 = var.on ? 1 : 2\n  k7 = true\n}\n# note 9\ngamma {\n}\n",
        &[
            Op::Cut {
                line: 13,
                how: Take::Text,
            },
            Op::Pause,
            Op::Paste {
                line: 30,
                how: Put::Before,
            },
        ],
    );
}

/// P2: a key cut and pasted back in a YAML view comes back with the comment
/// lines its HCL source has above it.
#[test]
fn a_yaml_key_cut_and_pasted_back_keeps_the_hcl_comment_above_it() {
    replay(
        Language::HCL,
        Language::YAML,
        "alpha {\n  k1 = \"plain\" # note 1\n  k2 { one = true }\n}\nbeta {\n  k3 = [1, 2] # note 3\n  k4 = var.name\n  k5 = 1.5\n  k6 = var.name\n}\n# note 7\ngamma {\n  k7 = upper(var.name)\n  k8 \"label\" {\n    inner = 1\n  }\n  k9 = upper(var.name) # note 6\n}\ndelta {\n  k10 = var.name\n  k11 = var.name # note 9\n}\n",
        &[
            Op::Cut {
                line: 31,
                how: Take::Entry,
            },
            Op::Pause,
            Op::Paste {
                line: 40,
                how: Put::OverBlank,
            },
        ],
    );
}

/// P2: a table header cut and pasted back heads its item again, which keeps the comment above it.
#[test]
fn a_tables_header_cut_and_pasted_back_keeps_the_comment_above_it() {
    replay(
        Language::HCL,
        Language::TOML,
        "alpha {\n  k1 = true # note 2\n  k2 = aws_subnet.main.id # note 4\n  k3 = \"web-${var.env}\"\n  k4 = <<EOT\nhello\nEOT\n}\nbeta {\n  k5 = aws_subnet.main.id\n  k6 = var.on ? 1 : 2\n  k7 = true\n}\n# note 9\ngamma {\n  k10 = true\n}\n",
        &[
            Op::Cut {
                line: 13,
                how: Take::Text,
            },
            Op::Pause,
            Op::Paste {
                line: 30,
                how: Put::Before,
            },
        ],
    );
}

/// P2: a table header cut and pasted back heads its item again, which keeps the comment above it.
#[test]
fn a_tables_header_pasted_over_a_line_keeps_the_comment_above_it() {
    replay(
        Language::HCL,
        Language::TOML,
        "alpha {\n  k1 { one = true } # note 1\n  k2 = \"plain\"\n  k3 = var.on ? 1 : 2 # note 2\n  k4 = [1, 2] # note 3\n}\n# note 9\nbeta {\n  k7 = \"plain\" # note 8\n}\ngamma {\n  k8 {\n  }\n}\n",
        &[
            Op::Cut {
                line: 8,
                how: Take::Text,
            },
            Op::Pause,
            Op::Paste {
                line: 14,
                how: Put::OverLine,
            },
        ],
    );
}

/// P3: a map renamed and edited in one save is renamed in place, its lines kept.
#[test]
fn a_map_renamed_and_edited_in_one_save_is_renamed_in_place() {
    replay(
        Language::JSON,
        Language::YAML,
        "{\n  \"alpha\": {\n    \"k2\": [\n      {\"name\": \"two\"}\n    ],\n    \"k3\": [1, 2, 3],\n    \"k8\": \"a\\u0041\"\n  },\n  \"delta\": {\n  }\n}\n",
        &[
            Op::Type {
                line: 6,
                column: 1000,
                text: "x",
            },
            Op::ReplaceAll {
                pick: 2,
                suffix: "_x",
            },
        ],
    );
}

/// P3: a map renamed and edited in one save is renamed in place, its lines kept.
#[test]
fn a_map_renamed_by_a_whole_paste_and_edited_is_renamed_in_place() {
    replay(
        Language::JSON,
        Language::HCL,
        "{\n  \"alpha\": {\n    \"k1\": {\"x\": 1, \"y\": \"two\"},\n    \"k2\": \"a\\u0041\"\n  },\n  \"beta\": {\n    \"k3\": [\n      {\"name\": \"one\"},\n      {\"name\": \"two\"}\n    ],\n    \"k4\": 1.50\n  },\n  \"gamma\": {\n    \"k5\": 1e3,\n    \"k6\": 1.50,\n    \"k7\": true,\n    \"k8\": [\n      {\"name\": \"one\"},\n      {\"name\": \"two\"}\n    ]\n  }\n}\n",
        &[
            Op::PasteWhole { line: 55 },
            Op::Type {
                line: 58,
                column: 38,
                text: "x",
            },
        ],
    );
}

/// P2: a list item's entry the save writes after the item's marker keeps its form (a view that writes a list of one as its item).
#[test]
fn a_list_of_one_written_as_its_item_keeps_the_items_entry_forms() {
    replay(
        Language::YAML,
        Language::XML,
        "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: web # the app\n  labels: {app: web, tier: \"front\"}\nspec:\n  replicas: 0x3\n  selector:\n    matchLabels:\n      app: web\n  template:\n    spec:\n      containers:\n        - name: web\n          image: \"nginx:1.25\"\n",
        &[
            Op::Cut {
                line: 19,
                how: Take::Text,
            },
            Op::Undo,
        ],
    );
}

/// P2: a label typed onto an HCL block (`spec x{`) adds a level, not a rename: the save writes a map, not a list of one.
#[test]
fn a_label_typed_onto_an_hcl_block_adds_a_level_not_a_rename() {
    replay(
        Language::YAML,
        Language::HCL,
        "kind: Deployment\nmetadata:\n  name: web # the app\n  labels: {app: web, tier: \"front\"}\nspec:\n  replicas: 0x3\n  selector:\n    matchLabels:\n      app: web\n  template:\n    spec:\n      containers:\n            - name: GREETING\n",
        &[Op::Type {
            line: 21,
            column: 1001,
            text: "x",
        }],
    );
}

/// P3: an item of an HCL list written over lines, copied, keeps its source's form (a YAML quoted string).
#[test]
fn a_copied_item_of_an_hcl_list_over_lines_keeps_its_source_form() {
    replay(
        Language::YAML,
        Language::TOML,
        "alpha:\n  k1:\n    - !Ref A\n    - 'q'\n    - 3\n  k2: 'single'\nbeta:\n  k3: |\n  k4: {x: 1, y: two} # note 2\ngamma:\n  k5: 1.50 # note 4\n  k6: {x: 1, y: two} # note 6\n",
        &[
            Op::Switch { to: 3 },
            Op::CopyLine {
                line: 29,
                down: false,
            },
        ],
    );
}

/// P3: a block a cut leaves alone in its list is the host's block written once: its values keep their form.
#[test]
fn a_block_a_cut_leaves_alone_in_its_list_keeps_its_values_forms() {
    replay(
        Language::HCL,
        Language::TOML,
        "terraform {\n  required_version = \">= 1.6\"\n  backend \"s3\" {\n    bucket = \"tf-state\"\n    key    = \"app/terraform.tfstate\"\n    region = \"eu-west-1\"\n  }\n}\nprovider \"aws\" {\n  region = var.region\n}\nprovider \"aws\" {\n  alias  = \"us\"\n  region = \"us-east-1\"\n}\nvariable \"region\" {\n  type    = string\n  default = \"eu-west-1\"\n  validation {\n    condition     = can(regex(\"^eu-\", var.region))\n    error_message = \"Only EU regions.\"\n  }\n}\nvariable \"subnets\" {\n  type = map(object({\n  }))\n  default = {\n    a = { cidr = \"10.0.1.0/24\", az = \"eu-west-1a\" }\n    b = { cidr = \"10.0.2.0/24\", az = \"eu-west-1b\" }\n  }\n}\ndata \"aws_ami\" \"ubuntu\" {\n  most_recent = true\n  owners      = [\"099720109477\"]\n  filter {\n    values = [\"ubuntu/images/hvm-ssd/ubuntu-jammy-22.04-amd64-server-*\"]\n  }\n  filter {\n    values = [\"hvm\"]\n  }\n}\noutput \"policy\" {\n  value = jsonencode({\n    Statement = [{\n    }]\n  })\n}\n",
        &[Op::Cut {
            line: 38,
            how: Take::Text,
        }],
    );
}

/// P3: a YAML key with a quote in it (`alpha"`) is found, so its items keep their links and forms.
#[test]
fn a_yaml_key_with_a_quote_in_it_keeps_its_items_links() {
    replay(
        Language::JSON,
        Language::TOML,
        "{\n  \"alpha\": {\n    \"k1\": {\"x\": 1, \"y\": \"two\"},\n    \"k2\": [\n      {\"name\": \"one\"},\n      {\"name\": \"two\"}\n    ],\n    \"k3\": [1, 2, 3],\n    \"k4\": 1e3\n  },\n  \"beta\": {\n    \"k5\": [1, 2, 3],\n    \"k6\": 1.50,\n    \"k7\": [1, 2, 3],\n    \"k8\": \"plain\"\n  },\n  \"gamma\": {\n    \"k9\": [\n      {\"name\": \"one\"},\n      {\"name\": \"two\"}\n    ],\n    \"k10\": true,\n    \"k11\": [\n      {\"name\": \"one\"},\n      {\"name\": \"two\"}\n    ],\n    \"k12\": 1e3\n  },\n  \"delta\": {\n    \"k13\": {\"x\": 1, \"y\": \"two\"},\n    \"k14\": {\"x\": 1, \"y\": \"two\"}\n  }\n}\n",
        &[
            Op::Switch { to: 0 },
            Op::Type {
                line: 39,
                column: 5,
                text: "\"",
            },
            Op::Cut {
                line: 41,
                how: Take::Text,
            },
        ],
    );
}

/// P3: a JSON key with an escaped quote keeps its entry's lines when an edit inside it is restyled.
#[test]
fn a_json_key_with_an_escaped_quote_keeps_its_entrys_lines() {
    replay(
        Language::JSON,
        Language::TOML,
        "{\n  \"alpha\": {\n    \"k1\": {\"x\": 1, \"y\": \"two\"},\n    \"k2\": [\n      {\"name\": \"one\"},\n      {\"name\": \"two\"}\n    ],\n    \"k3\": [1, 2, 3],\n    \"k4\": 1e3\n  },\n  \"beta\": {\n    \"k5\": [1, 2, 3],\n    \"k6\": 1.50,\n    \"k7\": [1, 2, 3],\n    \"k8\": \"plain\"\n  },\n  \"gamma\": {\n    \"k9\": [\n      {\"name\": \"one\"},\n      {\"name\": \"two\"}\n    ],\n    \"k10\": true,\n    \"k11\": [\n      {\"name\": \"one\"},\n      {\"name\": \"two\"}\n    ],\n    \"k12\": 1e3\n  },\n  \"delta\": {\n    \"k13\": {\"x\": 1, \"y\": \"two\"},\n    \"k14\": {\"x\": 1, \"y\": \"two\"}\n  }\n}\n",
        &[
            Op::Switch { to: 0 },
            Op::Type {
                line: 39,
                column: 5,
                text: "\"",
            },
            Op::Cut {
                line: 41,
                how: Take::Text,
            },
            Op::DeleteLine { line: 33 },
        ],
    );
}

/// The user's own spelling: a number split by Enter and joined back as `1000.` is theirs, written in the host's spelling of it.
#[test]
fn a_number_split_and_joined_back_by_the_user_takes_the_hosts_spelling() {
    replay(
        Language::JSON,
        Language::XML,
        "{\n  \"alpha\": {\n    \"k1\": null,\n    \"k2\": true,\n    \"k3\": null\n  },\n  \"beta\": {\n    \"k4\": \"plain\",\n    \"k5\": 1e3\n  },\n  \"gamma\": {\n    \"k6\": null,\n    \"k7\": 1.50\n  },\n  \"delta\": {\n    \"k8\": 1e3,\n    \"k9\": \"plain\",\n    \"k10\": 1.50\n  }\n}\n",
        &[
            Op::Switch { to: 0 },
            Op::Enter {
                line: 25,
                column: 1001,
            },
            Op::DeleteLine { line: 62 },
            Op::Cut {
                line: 51,
                how: Take::Text,
            },
            Op::MoveLine {
                line: 22,
                down: false,
            },
        ],
    );
}

/// The user's own order: lines cut and pasted over others put a map's keys in another order, which the save keeps.
#[test]
fn keys_put_in_another_order_by_cut_and_paste_keep_that_order() {
    replay(
        Language::JSON,
        Language::YAML,
        "{\n  \"alpha\": {\n    \"k1\": [\n      {\"name\": \"one\"},\n      {\"name\": \"two\"}\n    ],\n    \"k2\": \"a\\u0041\"\n  },\n  \"beta\": {\n    \"k3\": [1, 2, 3],\n    \"k4\": {\"x\": 1, \"y\": \"two\"},\n    \"k5\": 1e3\n  },\n  \"gamma\": {\n    \"k6\": [\n      {\"name\": \"one\"},\n      {\"name\": \"two\"}\n    ],\n    \"k7\": true,\n    \"k8\": [1, 2, 3],\n    \"k9\": \"a\\u0041\"\n  }\n}\n",
        &[
            Op::Cut {
                line: 26,
                how: Take::Entry,
            },
            Op::Cut {
                line: 33,
                how: Take::Entry,
            },
            Op::Pause,
            Op::Paste {
                line: 55,
                how: Put::OverLine,
            },
        ],
    );
}

/// The user's own value: a hex number edited and typed back as `31` is theirs.
#[test]
fn a_hex_number_edited_and_typed_back_as_decimal_is_the_users_value() {
    replay(
        Language::TOML,
        Language::YAML,
        "[alpha]\nk1 = 1_000\nk2 = \"plain\"\nk3 = 0x1F # note 2\nk6 = 0b101\nk7 = 0b101\n[delta]\nk8 = 1979-05-27\nk9 = 1_000 # note 7\n",
        &[
            Op::MoveLine {
                line: 42,
                down: true,
            },
            Op::DeleteLine { line: 1 },
            Op::Type {
                line: 2,
                column: 1001,
                text: "x",
            },
            Op::Pause,
            Op::DeleteLine { line: 9 },
            Op::Backspace {
                line: 15,
                column: 1001,
                count: 1,
            },
        ],
    );
}

/// P3: a TOML header moved into a multi-line string is text there, so the keys after it keep their links and forms (a YAML `!!str` tag).
#[test]
fn a_toml_header_moved_into_a_multi_line_string_is_text_there() {
    replay(
        Language::YAML,
        Language::HCL,
        "alpha:\n  k1: |\n    line one\n    line two\n  k2: !!str 5\nbeta:\n  k3: [a, 'b', 3] # note 3\n  k4: 'single'\n",
        &[
            Op::Switch { to: 2 },
            Op::MoveLine {
                line: 30,
                down: true,
            },
        ],
    );
}

/// P3: a TOML header cut and pasted into a multi-line string is text there,
/// so the keys after it keep their links and forms (a YAML `!!str` tag).
#[test]
fn a_toml_header_pasted_into_a_multi_line_string_is_text_there() {
    replay(
        Language::YAML,
        Language::TOML,
        "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: web # the app\n  labels: {app: web, tier: \"front\"}\nspec:\n  replicas: 0x3\n  selector:\n      app: web\n      containers:\n        - name: web\n          image: \"nginx:1.25\"\n          args: [--port, '8080']\n          env:\n            - name: GREETING\n              value: |\n                world\n            - name: LEVEL\n              value: !!str 3\n          resources:\n            limits: {cpu: 500m, memory: 128Mi}\n",
        &[
            Op::Cut {
                line: 23,
                how: Take::Line,
            },
            Op::Paste {
                line: 43,
                how: Put::OverBlank,
            },
        ],
    );
}

/// P3: a TOML entry cut and pasted into a multi-line string is text there,
/// so the keys after it keep their links and their HCL forms.
#[test]
fn a_toml_table_pasted_into_a_multi_line_string_keeps_the_hcl_forms() {
    replay(
        Language::HCL,
        Language::TOML,
        "alpha {\n  k1 = <<EOT\nhello\nEOT\n  k2 = var.name\n  k3 = var.on ? 1 : 2\n}\nbeta {\n  k4 = true # note 4\n  k5 = var.name # note 5\n  k6 = \"plain\" # note 7\n  k7 = [1, 2]\n}\ngamma {\n  k8 = true\n  k9 {\n    inner = var.x\n  }\n}\n",
        &[
            Op::Cut {
                line: 3,
                how: Take::Entry,
            },
            Op::Paste {
                line: 27,
                how: Put::Before,
            },
        ],
    );
}

/// The user's own value: a key whose value a paste over another line made, cut and pasted again, is written as that save wrote it.
#[test]
fn a_value_a_paste_made_cut_and_pasted_again_keeps_how_the_save_wrote_it() {
    replay(
        Language::JSON,
        Language::YAML,
        "{\n  \"alpha\": {\n    \"k1\": \"plain\",\n    \"k2\": \"a\\u0041\"\n  },\n  \"beta\": {\n    \"k3\": true,\n    \"k4\": {\"x\": 1, \"y\": \"two\"},\n    \"k5\": true,\n    \"k6\": 1e3\n  },\n  \"gamma\": {\n    \"k7\": [\n      {\"name\": \"one\"},\n      {\"name\": \"two\"}\n    ],\n    \"k8\": 1e3,\n    \"k9\": \"plain\",\n    \"k10\": true\n  },\n  \"delta\": {\n    \"k11\": 1.50,\n    \"k12\": [1, 2, 3],\n    \"k13\": [1, 2, 3]\n  }\n}\n",
        &[
            Op::Copy {
                line: 56,
                how: Take::Entry,
            },
            Op::Paste {
                line: 46,
                how: Put::OverLine,
            },
            Op::Pause,
            Op::Cut {
                line: 46,
                how: Take::Text,
            },
            Op::Paste {
                line: 40,
                how: Put::OverBlank,
            },
        ],
    );
}

/// P2: a view undone back past a save is an edit of the host text it shows: saved over it, its comments kept.
#[test]
fn an_undo_back_past_a_save_saves_over_the_text_it_shows_keeping_its_comments() {
    replay(
        Language::TOML,
        Language::YAML,
        "[alpha]\nk1 = [1, 0x2, 3] # note 1\nk2 = 0x1F\n[beta]\nk3 = 0o17 # note 4\nk4 = true\n# note 10\n[gamma]\nk5 = 1_000 # note 7\nk6 = [1, 0x2, 3] # note 9\n",
        &[
            Op::DeleteLine { line: 12 },
            Op::Type {
                line: 42,
                column: 1001,
                text: "x",
            },
            Op::Pause,
            Op::Undo,
            Op::Type {
                line: 4,
                column: 11,
                text: "x",
            },
        ],
    );
}

/// P3: a key line moved away from its value lines and joined to them again keeps its source form.
#[test]
fn a_key_line_moved_away_from_its_value_stays_that_item() {
    replay(
        Language::HCL,
        Language::YAML,
        "alpha {\n  k1 = true\n  k2 = local.count + 1\n  k3 = upper(var.name) # note 1\n}\nbeta {\n  k4 = [1, 2] # note 3\n  k5 = 1.5\n  k6 { one = true }\n}\ngamma {\n  k7 = aws_subnet.main.id\n  k8 = var.on ? 1 : 2\n  k9 = [1, 2]\n}\n",
        &[
            Op::MoveLine {
                line: 21,
                down: true,
            },
            Op::Enter {
                line: 16,
                column: 1001,
            },
            Op::Cut {
                line: 41,
                how: Take::Line,
            },
        ],
    );
}

/// P2: what a cut in the host's own language took, pasted into a view, is the host's items: saved in their forms.
#[test]
fn a_cut_in_the_hosts_language_pasted_into_a_view_keeps_its_forms() {
    replay(
        Language::JSON,
        Language::JSON,
        "{\n  \"editor.fontSize\": 14,\n  \"files.exclude\": {\n    \"**/.git\": true,\n    \"**/node_modules\": true\n  },\n  \"servers\": [\n    {\"name\": \"one\", \"port\": 80},\n    {\"name\": \"two\", \"port\": 81}\n  ],\n  \"nothing\": null,\n  \"escaped\": \"a\\u0041b\"\n}\n",
        &[
            Op::Cut {
                line: 52,
                how: Take::Entry,
            },
            Op::Switch { to: 0 },
            Op::Paste {
                line: 48,
                how: Put::Before,
            },
        ],
    );
}

/// P2: an undo back past a saved rename, then a cut: the containers left alone keep their lines.
#[test]
fn an_undo_of_a_rename_past_a_save_keeps_untouched_containers_over_lines() {
    replay(
        Language::JSON,
        Language::YAML,
        "{\n  \"alpha\": {\n    \"k1\": [\n      {\"name\": \"one\"},\n      {\"name\": \"two\"}\n    ],\n    \"k2\": [\n      {\"name\": \"one\"},\n      {\"name\": \"two\"}\n    ],\n    \"k3\": \"plain\"\n  },\n  \"beta\": {\n    \"k4\": null,\n    \"k5\": [1, 2, 3]\n  },\n  \"gamma\": {\n    \"k6\": [\n      {\"name\": \"one\"},\n      {\"name\": \"two\"}\n    ],\n    \"k7\": null\n  },\n  \"delta\": {\n    \"k8\": 1e3,\n    \"k9\": null,\n    \"k10\": [1, 2, 3]\n  }\n}\n",
        &[
            Op::Type {
                line: 26,
                column: 1001,
                text: "x",
            },
            Op::Pause,
            Op::Undo,
            Op::Cut {
                line: 5,
                how: Take::Entry,
            },
        ],
    );
}

/// P3: an entry cut before two saves and pasted after an undo keeps its TOML date.
#[test]
fn a_paste_from_a_text_two_saves_back_keeps_its_form() {
    replay(
        Language::TOML,
        Language::JSON,
        "[alpha]\nk1 = 3.5e2\nk2 = 1979-05-27T07:32:00\nk3 = 1979-05-27\nk4 = 1979-05-27\n[beta]\nk5 = 1979-05-27T07:32:00\nk6 = true\nk7 = 0x1F\n",
        &[
            Op::ReplaceAll {
                pick: 16,
                suffix: "_x",
            },
            Op::Cut {
                line: 43,
                how: Take::Entry,
            },
            Op::Undo,
            Op::Pause,
            Op::PasteWhole { line: 47 },
            Op::Pause,
            Op::Undo,
            Op::Paste {
                line: 23,
                how: Put::Before,
            },
        ],
    );
}

/// P3: an element joined to an element pasted back from an earlier text keeps its own name, not its new siblings' prefix.
#[test]
fn an_element_moved_into_an_element_pasted_back_keeps_its_name() {
    replay(
        Language::XML,
        Language::YAML,
        "<root xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xmlns:p=\"urn:p\">\n  <alpha>\n    <k1>text</k1>\n    <k2 a=\"1\" b=\"x\"/>\n    <k3 id=\"1\">text</k3>\n    <p:k4>v</p:k4> <!-- note 3 -->\n  </alpha>\n  <beta>\n    <k5 id=\"1\">text</k5>\n    <p:k6>v</p:k6>\n  </beta>\n  <gamma>\n    <k7>42</k7>\n    <k8 xsi:type=\"File\" name=\"f\"/> <!-- note 6 -->\n  </gamma>\n  <delta>\n    <k9>42</k9>\n    <k10>text</k10>\n    <k11>\n      <item>a</item>\n      <item>b</item>\n    </k11>\n    <p:k12>v</p:k12>\n  </delta>\n</root>\n",
        &[
            Op::Cut {
                line: 36,
                how: Take::Line,
            },
            Op::Enter {
                line: 17,
                column: 37,
            },
            Op::Cut {
                line: 63,
                how: Take::Entry,
            },
            Op::Switch { to: 0 },
            Op::Paste {
                line: 28,
                how: Put::Before,
            },
        ],
    );
}

/// P3: a YAML flow map copied with its line keeps its flow form.
#[test]
fn a_flow_map_copied_by_its_line_keeps_its_form() {
    replay(
        Language::YAML,
        Language::TOML,
        "x-common: &common\n  restart: unless-stopped # always back\n  logging:\n    driver: \"json-file\"\nservices:\n  web:\n    <<: *common\n    image: 'nginx:1.25'\n    ports:\n      - \"8080:80\" # http\n      - 8443:443\n    environment:\n      MODE: production\n      DEBUG: \"false\"\n    command: >-\n      'daemon off;'\n  db:\n    image: postgres:16\n    volumes: [dbdata:/var/lib/postgresql/data]\n    healthcheck: {test: [CMD, pg_isready], interval: 10s}\n",
        &[
            Op::Backspace {
                line: 11,
                column: 8,
                count: 3,
            },
            Op::Switch { to: 1 },
            Op::Cut {
                line: 8,
                how: Take::Text,
            },
            Op::CopyLine {
                line: 25,
                down: false,
            },
        ],
    );
}

/// P2: `<4/>` is no XML; the view says so and the save waits.
#[test]
fn an_element_name_that_is_no_xml_name_is_a_syntax_error() {
    replay(
        Language::JSON,
        Language::XML,
        "{\n  \"alpha\": {\n    \"k1\": true,\n    \"k2\": \"plain\",\n    \"k3\": null\n  },\n  \"beta\": {\n    \"k4\": null,\n    \"k5\": 1e3,\n    \"k6\": \"plain\",\n    \"k7\": [1, 2, 3]\n  },\n  \"gamma\": {\n    \"k8\": \"a\\u0041\",\n    \"k9\": true,\n    \"k10\": 1e3,\n    \"k11\": null\n  }\n}\n",
        &[Op::Backspace {
            line: 63,
            column: 4,
            count: 1,
        }],
    );
}

/// P2: the whole host cut in a view and pasted back is saved as it was.
#[test]
fn a_host_emptied_by_a_cut_and_pasted_back_keeps_its_forms() {
    replay(
        Language::HCL,
        Language::JSON,
        "provider \"aws\" {\n  validation {\n  }\n}\nvariable \"subnets\" {\n  type = map(object({\n  }))\n  default = {\n    a = { cidr = \"10.0.1.0/24\", az = \"eu-west-1a\" }\n  }\n}\ndata \"aws_ami\" \"ubuntu\" {\n  tags = {\n  }\n}\nmodule \"vpc\" {\n}\nresource \"aws_security_group\" \"web\" {\n  dynamic \"ingress\" {\n    content {\n      cidr_blocks = [\"0.0.0.0/0\"]\n    }\n    cidr_blocks = [\"0.0.0.0/0\"]\n  }\n  lifecycle {\n  }\n}\noutput \"web_ips\" {\n}\noutput \"policy\" {\n  value = jsonencode({\n    Statement = [{\n    }]\n  })\n}\n",
        &[
            Op::Cut {
                line: 0,
                how: Take::Entry,
            },
            Op::Enter {
                line: 49,
                column: 1000,
            },
            Op::Pause,
            Op::Paste {
                line: 34,
                how: Put::OverBlank,
            },
        ],
    );
}

/// P3: the YAML empty key (`:` alone) is found where it is written, so the items under it keep their links.
#[test]
fn an_entry_under_an_empty_yaml_key_keeps_its_link() {
    replay(
        Language::JSON,
        Language::YAML,
        "{\n  \"alpha\": {\n    \"k1\": [\n      {\"name\": \"one\"},\n      {\"name\": \"two\"}\n    ],\n    \"k2\": \"plain\"\n  },\n  \"beta\": {\n    \"k3\": \"a\\u0041\",\n    \"k4\": 1e3,\n    \"k5\": null,\n    \"k6\": 1e3\n  }\n}\n",
        &[
            Op::Cut {
                line: 49,
                how: Take::Line,
            },
            Op::Enter {
                line: 59,
                column: 1001,
            },
            Op::Type {
                line: 35,
                column: 1000,
                text: ":",
            },
        ],
    );
}

/// P3: a container pasted into a JSON object of one entry is written on a line of its own, then in its source form.
#[test]
fn a_list_pasted_into_a_json_object_over_lines_gets_lines_of_its_own() {
    replay(
        Language::JSON,
        Language::YAML,
        "{\n  \"alpha\": {\n    \"k1\": \"a\\u0041\",\n    \"k2\": [\n      {\"name\": \"one\"},\n      {\"name\": \"two\"}\n    ],\n    \"k4\": [\n      {\"name\": \"one\"},\n      {\"name\": \"two\"}\n    ]\n  },\n  \"beta\": {\n    \"k5\": null,\n    \"k6\": \"a\\u0041\",\n    \"k7\": true\n  },\n  \"gamma\": {\n    \"k8\": [1, 2, 3],\n    \"k9\": 1.50\n  },\n  \"delta\": {\n    \"k10\": \"plain\",\n    \"k11\": \"plain\"\n  }\n}\n",
        &[
            Op::Cut {
                line: 44,
                how: Take::Entry,
            },
            Op::DeleteLine { line: 32 },
            Op::Pause,
            Op::Paste {
                line: 10,
                how: Put::OverBlank,
            },
        ],
    );
}

/// P2: a map pasted back from an earlier text with a key added keeps the forms of what it held.
#[test]
fn an_item_pasted_back_with_a_key_added_keeps_its_forms_inside() {
    replay(
        Language::JSON,
        Language::YAML,
        "{\n  \"alpha\": {\n    \"k1\": [\n      {\"name\": \"two\"}\n    ],\n    \"k2\": true\n  },\n  \"gamma\": {\n    \"k6\": [\n      {\"name\": \"two\"}\n    ]\n  },\n  \"delta\": {\n  }\n}\n",
        &[
            Op::Cut {
                line: 0,
                how: Take::Entry,
            },
            Op::Switch { to: 0 },
            Op::Enter {
                line: 12,
                column: 1000,
            },
            Op::Paste {
                line: 47,
                how: Put::Before,
            },
        ],
    );
}

/// P3: a TOML table read under the YAML empty key keeps its values' forms.
#[test]
fn a_toml_table_shown_under_an_empty_yaml_key_keeps_its_hex() {
    replay(
        Language::TOML,
        Language::YAML,
        "[alpha]\nk1 = 0o17\nk2 = { x = 1, y = \"two\" } # note 1\nk3 = '''\nraw text\n'''\nk4 = '''\nraw text\n'''\n[beta]\nk5 = [1, 0x2, 3]\nk6 = 0o17\nk7 = 0b101\nk8 = 07:32:00\n",
        &[
            Op::CopyLine {
                line: 26,
                down: false,
            },
            Op::Enter {
                line: 10,
                column: 1001,
            },
        ],
    );
}

/// P2: a quoted HCL string stays on one line; one run on over lines is an error, not a value swallowing the lines below.
#[test]
fn an_hcl_string_over_lines_is_a_syntax_error() {
    replay(
        Language::XML,
        Language::HCL,
        "<root xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xmlns:p=\"urn:p\">\n  <alpha>\n    <k1><![CDATA[x < y]]></k1> <!-- note 2 -->\n    <k2 xsi:type=\"File\" name=\"f\"/>\n  </alpha>\n  <beta>\n    <k3 id=\"1\">text</k3> <!-- note 3 -->\n    <k6 xsi:type=\"File\" name=\"f\"/>\n  </beta>\n</root>\n",
        &[Op::Backspace {
            line: 5,
            column: 1001,
            count: 3,
        }],
    );
}

/// P2: `<k=2/>` is no XML; the view says so and the save waits.
#[test]
fn an_element_name_with_an_equals_sign_is_a_syntax_error() {
    replay(
        Language::JSON,
        Language::XML,
        "{\n  \"alpha\": {\n    \"k1\": true,\n    \"k2\": null\n  },\n  \"beta\": {\n    \"k3\": 1.50,\n    \"k4\": true,\n    \"k5\": \"plain\",\n    \"k6\": [1, 2, 3]\n  },\n  \"gamma\": {\n    \"k7\": {\"x\": 1, \"y\": \"two\"},\n    \"k8\": [1, 2, 3],\n    \"k9\": 1e3,\n    \"k10\": \"plain\"\n  }\n}\n",
        &[
            Op::Enter {
                line: 5,
                column: 1000,
            },
            Op::Type {
                line: 26,
                column: 4,
                text: "=",
            },
        ],
    );
}

/// P2: a copy that the links pair with another item of its header line changes no form of what it left alone.
#[test]
fn a_value_copied_into_a_toml_array_of_tables_keeps_the_other_forms() {
    replay(
        Language::YAML,
        Language::TOML,
        "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: web # the app\n  labels: {app: web, tier: \"front\"}\nspec:\n  replicas: 0x3\n  selector:\n      app: web\n  template:\n      containers:\n        - name: web\n          image: \"nginx:1.25\"\n          args: [--port, '8080']\n          env:\n            - name: GREETING\n              value: |\n                hello\n                world\n            - name: LEVEL\n              value: !!str 3\n          resources:\n            limits: {cpu: 500m, memory: 128Mi}\n",
        &[
            Op::Copy {
                line: 51,
                how: Take::Text,
            },
            Op::Paste {
                line: 33,
                how: Put::Before,
            },
        ],
    );
}

/// P2: the same, one table deeper.
#[test]
fn a_value_copied_into_a_nested_toml_array_of_tables_keeps_the_other_forms() {
    replay(
        Language::YAML,
        Language::TOML,
        "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: web # the app\n  labels: {app: web, tier: \"front\"}\nspec:\n  replicas: 0x3\n  selector:\n      app: web\n  template:\n    spec:\n      containers:\n        - name: web\n          image: \"nginx:1.25\"\n          args: [--port, '8080']\n          env:\n            - name: GREETING\n              value: |\n                hello\n                world\n            - name: LEVEL\n              value: !!str 3\n          resources:\n            limits: {cpu: 500m, memory: 128Mi}\n",
        &[
            Op::Copy {
                line: 51,
                how: Take::Text,
            },
            Op::Paste {
                line: 33,
                how: Put::Before,
            },
        ],
    );
}
/// P2: an item pasted back into an HCL list of one item writes the list as `[1, 2]`, not `[1,2]`: a lone item tells no spacing.
#[test]
fn an_item_pasted_back_into_a_list_of_one_keeps_the_lists_spacing() {
    replay(
        Language::HCL,
        Language::YAML,
        "alpha {\n  k1 = var.name\n  k2 { one = true }\n  k3 = [1, 2]\n}\nbeta {\n  k4 = var.on ? 1 : 2\n  k5 = [1, 2] # note 2\n  k6 = { a = 1, b = \"x\" }\n}\n",
        &[
            Op::CopyLine {
                line: 36,
                down: false,
            },
            Op::Cut {
                line: 12,
                how: Take::Entry,
            },
            Op::Enter {
                line: 17,
                column: 39,
            },
            Op::Pause,
            Op::Paste {
                line: 45,
                how: Put::Before,
            },
        ],
    );
}

/// P3: the links of a list of one survive a switch through a view that writes it as its item (XML), so its values keep their forms.
#[test]
fn links_follow_a_list_of_one_through_an_xml_view() {
    replay(
        Language::YAML,
        Language::HCL,
        "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: web # the app\n  labels: {app: web, tier: \"front\"}\nspec:\n  replicas: 0x3\n  selector:\n    matchLabels:\n      app: web\n  template:\n    spec:\n      containers:\n        - name: web\n          image: \"nginx:1.25\"\n          args: [--port, '8080']\n          env:\n            - name: GREETING\n              value: |\n                world\n            - name: LEVEL\n              value: !!str 3\n",
        &[
            Op::MoveLine {
                line: 38,
                down: false,
            },
            Op::Switch { to: 4 },
            Op::Switch { to: 1 },
        ],
    );
}

/// P2: a key pasted back into a YAML flow map keeps its source spelling (`y`, not `"y"`).
#[test]
fn a_key_pasted_back_into_a_flow_map_keeps_its_spelling() {
    replay(
        Language::YAML,
        Language::TOML,
        "alpha:\n  k1: !Sub \"${AWS::StackName}-data\" # note 1\n  k2: !Ref Param # note 2\n  k3: 0x1F\nbeta:\n  k4: {x: 1, y: two} # note 3\n  k5: \"double\"\ngamma:\n  k6: 'single'\n  k7: !!str 5\n  k8:\n    - name: one\n      port: 80\n    - name: two\n      port: 81\n  k9: !Sub \"${AWS::StackName}-data\" # note 6\n",
        &[
            Op::Cut {
                line: 34,
                how: Take::Text,
            },
            Op::Undo,
            Op::Enter {
                line: 0,
                column: 1000,
            },
            Op::Pause,
            Op::Paste {
                line: 37,
                how: Put::OverLine,
            },
        ],
    );
}

/// P2: a number typed in a spelling a text host reads as other text (TOML's `47_2`) is saved in the host's own form (`472`).
#[test]
fn a_number_spelled_as_a_text_host_cannot_read_saves_in_its_own_form() {
    replay(
        Language::XML,
        Language::TOML,
        "<root xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xmlns:p=\"urn:p\">\n  <alpha>\n    <k1>\n      <item>b</item>\n    </k1>\n    <k2 xsi:type=\"File\" name=\"f\"/>\n    <k3>42</k3>\n  </alpha>\n  <beta>\n    <k5>42</k5>\n    <k6><![CDATA[x < y]]></k6> <!-- note 7 -->\n  </beta>\n  <gamma>\n    <k7>42</k7> <!-- note 9 -->\n    <k8 a=\"1\" b=\"x\"/>\n    <k9>\n      <item>b</item>\n    </k9>\n  </gamma>\n  <delta>\n    <p:k10>v</p:k10>\n    <k11><![CDATA[x < y]]></k11>\n  </delta>\n</root>\n",
        &[
            Op::Type {
                line: 40,
                column: 1001,
                text: "7",
            },
            Op::Type {
                line: 40,
                column: 1001,
                text: "_",
            },
        ],
    );
}

/// P3: a table moved by renaming its TOML header (into a parent the host
/// does not have) keeps its source's inline form.
#[test]
fn a_table_moved_by_renaming_its_header_keeps_its_inline_form() {
    replay(
        Language::HCL,
        Language::TOML,
        "terraform {\n  required_version = \">= 1.6\"\n  backend \"s3\" {\n    bucket = \"tf-state\"\n    key    = \"app/terraform.tfstate\"\n    region = \"eu-west-1\"\n  }\n}\nprovider \"aws\" {\n  region = var.region\n}\nprovider \"aws\" {\n  alias  = \"us\"\n  region = \"us-east-1\"\n}\nvariable \"region\" {\n  type    = string\n  default = \"eu-west-1\"\n  validation {\n    condition     = can(regex(\"^eu-\", var.region))\n    error_message = \"Only EU regions.\"\n  }\n}\nvariable \"subnets\" {\n  type = map(object({\n  }))\n  default = {\n    a = { cidr = \"10.0.1.0/24\", az = \"eu-west-1a\" }\n    b = { cidr = \"10.0.2.0/24\", az = \"eu-west-1b\" }\n  }\n}\nresource \"aws_security_group\" \"web\" {\n  dynamic \"ingress\" {\n    content {\n    }\n  }\n}\n",
        &[
            Op::DeleteLine { line: 19 },
            Op::Type {
                line: 25,
                column: 8,
                text: "x",
            },
        ],
    );
}

/// P3: a map moved into another parent whose keys the user also put in
/// another order is written in the order the user gave it.
#[test]
fn a_moved_map_whose_keys_the_user_reordered_keeps_their_order() {
    replay(
        Language::JSON,
        Language::YAML,
        "{\n  \"alpha\": {\n    \"k1\": null,\n    \"k2\": {\"x\": 1, \"y\": \"two\"},\n    \"k3\": [1, 2, 3],\n    \"k4\": \"plain\"\n  },\n  \"beta\": {\n    \"k5\": \"a\\u0041\",\n    \"k6\": true,\n    \"k7\": {\"x\": 1, \"y\": \"two\"},\n    \"k8\": \"a\\u0041\"\n  },\n  \"gamma\": {\n    \"k9\": {\"x\": 1, \"y\": \"two\"},\n    \"k10\": \"plain\"\n  },\n  \"delta\": {\n    \"k11\": \"a\\u0041\",\n    \"k12\": {\"x\": 1, \"y\": \"two\"},\n    \"k13\": [1, 2, 3]\n  }\n}\n",
        &[
            Op::MoveLine {
                line: 26,
                down: false,
            },
            Op::DeleteLine { line: 22 },
        ],
    );
}

/// P2: a comment belongs to its line. The `gamma:` line cut in a YAML view
/// of an XML host removes `<gamma>` (its item joins `beta`), and that save
/// drops `<!-- note 8 -->` above it; the line pasted lower down makes a
/// new `gamma` of other items, which the comment does not come back to.
#[test]
fn a_comment_goes_with_its_cut_element_though_the_name_comes_back() {
    replay(
        Language::XML,
        Language::YAML,
        "<root xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xmlns:p=\"urn:p\">\n  <alpha>\n    <k1>text</k1>\n    <k2>text</k2>\n    <k3>text</k3>\n    <p:k4>v</p:k4>\n  </alpha>\n  <beta>\n    <k5 id=\"1\">text</k5>\n    <k6 xsi:nil=\"true\"/> <!-- note 4 -->\n    <k7><![CDATA[x < y]]></k7>\n    <k8 xsi:nil=\"true\"/>\n  </beta>\n  <!-- note 8 -->\n  <gamma>\n    <p:k11>v</p:k11>\n  </gamma>\n  <delta>\n    <k12>text</k12>\n    <k13><![CDATA[x < y]]></k13> <!-- note 10 -->\n    <k14>42</k14>\n    <k15 id=\"1\">text</k15>\n  </delta>\n</root>\n",
        &[
            Op::Cut {
                line: 13,
                how: Take::Line,
            },
            Op::Pause,
            Op::Paste {
                line: 18,
                how: Put::OverBlank,
            },
        ],
    );
}

/// P2: XML elements moved into an element the user added (a cut and paste,
/// then typing in an HCL view) keep their own namespace when saved.
#[test]
fn xml_elements_moved_into_an_added_element_keep_their_namespace() {
    replay(
        Language::XML,
        Language::HCL,
        "<root xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xmlns:p=\"urn:p\">\n  <alpha>\n    <p:k1>v</p:k1>\n    <k2>42</k2>\n    <k3 id=\"1\">text</k3>\n  </alpha>\n  <beta>\n    <k4 id=\"1\">text</k4>\n    <k5>\n      <item>a</item>\n      <item>b</item>\n    </k5>\n  </beta>\n</root>\n",
        &[
            Op::Cut {
                line: 34,
                how: Take::Entry,
            },
            Op::Paste {
                line: 40,
                how: Put::OverLine,
            },
            Op::Type {
                line: 4,
                column: 8,
                text: "x",
            },
        ],
    );
}

/// P5: an item below an element the edit added, at a place its list
/// does not hold, is no host element (and no crash).
#[test]
fn an_item_below_an_added_element_past_its_list_is_new() {
    replay(
        Language::XML,
        Language::YAML,
        "<project xmlns=\"http://maven.apache.org/POM/4.0.0\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">\n  <modelVersion>4.0.0</modelVersion>\n  <groupId>com.example</groupId>\n  <artifactId>demo</artifactId>\n  <version>1.0.0</version>\n  <properties>\n    <java.version>17</java.version>\n  </properties>\n  <dependencies>\n    <dependency>\n      <groupId>org.slf4j</groupId>\n      <artifactId>slf4j-api</artifactId>\n      <version>2.0.13</version> <!-- logging -->\n    </dependency>\n    <dependency>\n      <groupId>junit</groupId>\n      <artifactId>junit</artifactId>\n      <scope>test</scope>\n    </dependency>\n  </dependencies>\n</project>\n",
        &[
            Op::Type {
                line: 8,
                column: 1001,
                text: ":",
            },
            Op::Cut {
                line: 43,
                how: Take::Text,
            },
        ],
    );
}

/// P2: after an edit, a switch, an undo and a cut in an XML view of a
/// YAML host, the saved host reads as the view.
#[test]
fn a_cut_after_an_undo_in_a_switched_view_saves_what_the_view_reads() {
    replay(
        Language::YAML,
        Language::XML,
        "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  labels: {app: web, tier: \"front\"}\nspec:\n  replicas: 0x3\n  selector:\n    matchLabels:\n      app: web\n  template:\n    spec:\n      containers:\n        - name: web\n          image: \"nginx:1.25\"\n          args: [--port, '8080']\n          env:\n            - name: GREETING\n              value: |\n                hello\n                world\n            - name: LEVEL\n              value: !!str 3\n          resources:\n            limits: {cpu: 500m, memory: 128Mi}\n",
        &[
            Op::Enter {
                line: 36,
                column: 1000,
            },
            Op::MoveLine {
                line: 47,
                down: true,
            },
            Op::Switch { to: 3 },
            Op::Type {
                line: 37,
                column: 1000,
                text: "x",
            },
            Op::Undo,
            Op::Cut {
                line: 53,
                how: Take::Entry,
            },
        ],
    );
}

/// P2: a text cut in the host's language (one that does not read, a stray
/// `x` typed) and pasted whole into a view stands for no saved host text:
/// it is read in the view's language, and the save waits as it does not
/// read there, instead of sending the host the text as it was.
#[test]
fn a_host_language_text_pasted_into_a_view_is_read_in_the_view() {
    replay(
        Language::JSON,
        Language::JSON,
        "{\n  \"name\": \"web-app\",\n  \"version\": \"1.0.0\",\n  \"private\": true,\n  \"scripts\": {\n    \"build\": \"vite build\",\n    \"test\": \"vitest run\"\n  },\n  \"dependencies\": {\"react\": \"^18.3.1\", \"react-dom\": \"^18.3.1\"},\n  \"engines\": {\n    \"node\": \">=18\"\n  },\n  \"files\": [\"dist\", \"src\"],\n  \"ratio\": 1.50,\n  \"size\": 1e3\n}\n",
        &[
            Op::Type {
                line: 24,
                column: 1000,
                text: "x",
            },
            Op::Cut {
                line: 16,
                how: Take::Entry,
            },
            Op::Switch { to: 3 },
            Op::CopyLine {
                line: 15,
                down: false,
            },
            Op::Undo,
            Op::Paste {
                line: 46,
                how: Put::OverBlank,
            },
        ],
    );
}

/// P2: an entry copied in a JSON view of a TOML host and pasted in a YAML
/// view: the original, left alone, keeps its host text (`3.5e2`).
#[test]
fn a_value_copied_across_views_leaves_the_originals_text_alone() {
    replay(
        Language::TOML,
        Language::JSON,
        "[beta]\nk5 = 3.5e2\n",
        &[
            Op::Copy {
                line: 61,
                how: Take::Entry,
            },
            Op::Switch { to: 0 },
            Op::Paste {
                line: 55,
                how: Put::Before,
            },
        ],
    );
}

/// P2: a line copied up in an XML view of a YAML host: the original list
/// item, left alone, keeps its host text (`'q'`).
#[test]
fn a_line_copied_in_a_view_leaves_the_originals_text_alone() {
    replay(
        Language::YAML,
        Language::XML,
        "alpha:\n  k1:\n    - !Ref A\n    - 'q'\n    - 3\n  k2: >-\n  k3: plain words\nbeta:\n  k4: [a, 'b', 3] # note 5\n  k5: !Ref Param\n",
        &[
            Op::MoveLine {
                line: 40,
                down: true,
            },
            Op::CopyLine {
                line: 1,
                down: false,
            },
        ],
    );
}

/// P2: a TOML host shown as HCL, then as YAML: a comment pasted over a
/// line in the view is saved, or the save waits saying why.
#[test]
fn a_comment_pasted_over_a_line_in_a_switched_view_is_saved() {
    replay(
        Language::TOML,
        Language::HCL,
        "# note 2\n[alpha]\nk1 = [1, 0x2, 3]\n# note 1\nk2 = 3.5e2\n\n# note 6\n[beta]\n# note 3\nk3 = 1_000\n# note 4\nk4 = 0x1F # note 5\nk5 = 0x1F\n\n[gamma]\nk6 = [1, 0x2, 3]\nk7 = true\nk8 = 0o17\n",
        &[
            Op::Switch { to: 2 },
            Op::Copy {
                line: 28,
                how: Take::Line,
            },
            Op::Switch { to: 3 },
            Op::Paste {
                line: 13,
                how: Put::Before,
            },
            Op::Backspace {
                line: 54,
                column: 1000,
                count: 1,
            },
            Op::Switch { to: 0 },
            Op::Paste {
                line: 0,
                how: Put::Before,
            },
            Op::Paste {
                line: 8,
                how: Put::OverLine,
            },
        ],
    );
}

/// P2: an emptied YAML value that a key line below, its colon deleted,
/// runs on into reads as that text: it is the user's value, not the
/// host's (its first line is still written as the editor wrote it).
#[test]
fn a_value_a_line_below_runs_into_is_saved_as_the_view_reads_it() {
    replay(
        Language::HCL,
        Language::YAML,
        "resource \"aws_subnet\" \"main\" {\n  vpc_id     = aws_vpc.main.id # the vpc\n  cidr_block = cidrsubnet(var.cidr, 8, 1)\n  tags = { Name = \"main-${var.env}\", Tier = \"app\" }\n}\nlocals {\n  count = 3\n  names = [\"a\", \"b\"]\n  script = <<EOT\n#!/bin/sh\necho hi\nEOT\n}\nmodule \"web\" {\n  source = \"./web\"\n  size   = var.big ? 3 : 1\n  ami = data.aws_ami.ubuntu.id\n}\n",
        &[
            Op::DeleteLine { line: 41 },
            Op::Backspace {
                line: 49,
                column: 1000,
                count: 1,
            },
            Op::Pause,
            Op::Enter {
                line: 40,
                column: 1000,
            },
            Op::Backspace {
                line: 53,
                column: 1000,
                count: 1,
            },
        ],
    );
}

/// P2: an entry cut in a JSON host, the view switched to XML and a line
/// pasted over: the host saves what the view reads.
#[test]
fn a_paste_over_a_line_after_a_switch_saves_what_the_view_reads() {
    replay(
        Language::JSON,
        Language::JSON,
        "{\n  \"alpha\": {\n    \"k1\": \"a\\u0041\",\n    \"k2\": [\n      {\"name\": \"one\"},\n      {\"name\": \"two\"}\n    ],\n    \"k3\": 1e3\n  },\n  \"beta\": {\n    \"k4\": \"plain\",\n    \"k5\": 1e3,\n    \"k6\": [\n      {\"name\": \"one\"},\n      {\"name\": \"two\"}\n    ]\n  },\n  \"gamma\": {\n    \"k7\": null,\n    \"k8\": {\"x\": 1, \"y\": \"two\"},\n    \"k9\": true,\n    \"k10\": {\"x\": 1, \"y\": \"two\"}\n  },\n  \"delta\": {\n    \"k11\": true,\n    \"k12\": 1e3,\n    \"k13\": [1, 2, 3]\n  }\n}\n",
        &[
            Op::Cut {
                line: 58,
                how: Take::Entry,
            },
            Op::Switch { to: 4 },
            Op::Paste {
                line: 57,
                how: Put::OverLine,
            },
        ],
    );
}

/// P2: in a YAML view of a TOML host, a key the session left alone keeps
/// its bare spelling (`ignore`, not `"ignore"`) after a line above it is
/// deleted and a typed character taken back.
#[test]
fn a_key_left_alone_keeps_its_spelling_after_a_line_above_goes() {
    replay(
        Language::TOML,
        Language::YAML,
        "[project]\nname = \"tool\"\nreleased = 1979-05-27T07:32:00Z # first\ndescription = \"\"\"\nA tool\nover lines\"\"\"\npaths = 'C:\\tools'\n[tool.lint]\nline-length = 1_000\nselect = [\"E\", \"F\"] # rules\nmask = 0o755\nignore = { tests = [\"E501\"] }\n[tool.cache]\nday = 1979-05-27\nat = 07:32:00\n",
        &[
            Op::Type {
                line: 54,
                column: 1001,
                text: ":",
            },
            Op::DeleteLine { line: 11 },
            Op::Pause,
            Op::Backspace {
                line: 51,
                column: 1000,
                count: 1,
            },
        ],
    );
}
