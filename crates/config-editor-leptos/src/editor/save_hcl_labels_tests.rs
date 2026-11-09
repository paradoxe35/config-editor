//! Labelled HCL blocks (`resource "aws_instance" "web" { … }`) added,
//! removed and edited from every other view: each entry under a block
//! type with labels is its own labelled block, written in place, and the
//! rest of the document stays byte for byte.

use super::Language;
use super::save_matrix::{ALL, from_every_view, report, save_value};
use serde_json::{Value, json};

/// A realistic Terraform file: comments, `terraform fmt` alignment,
/// expressions, a heredoc, nested and labelled blocks.
const MAIN_TF: &str = r#"terraform {
  required_version = ">= 1.5.0"
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 5.0"
    }
  }
}

variable "environment" {
  type        = string
  description = "Deployment environment"
  default     = "staging"
}

variable "instance_count" {
  type    = number
  default = 2
}

locals {
  name_prefix = "app-${var.environment}"
  common_tags = {
    Environment = var.environment
    ManagedBy   = "terraform"
  }
}

# The web servers
resource "aws_instance" "web" {
  count         = var.instance_count
  ami           = "ami-0abcdef1234567890"
  instance_type = "t3.micro"
  tags          = merge(local.common_tags, { Name = "${local.name_prefix}-web-${count.index}" })

  user_data = <<-EOT
    #!/bin/bash
    echo "hello ${var.environment}" > /tmp/hello
  EOT

  root_block_device {
    volume_size = 20
    encrypted   = true
  }
}

output "instance_ids" {
  value = aws_instance.web[*].id
}
"#;

const ONE: &str = "# web\nresource \"aws_instance\" \"web\" {\n  ami = \"a\"\n}\n\nvariable \"x\" {\n  default = 1\n}\n";

const TWO: &str = "resource \"aws_instance\" \"web\" {\n  ami = \"a\"\n}\n\nresource \"aws_instance\" \"db\" {\n  ami = \"b\"\n}\n";

fn add(pointer: &'static str, key: &'static str, value: Value) -> impl Fn(&mut Value) {
    move |document| {
        document
            .pointer_mut(pointer)
            .and_then(Value::as_object_mut)
            .unwrap_or_else(|| panic!("no map at {pointer}"))
            .insert(key.to_string(), value.clone());
    }
}

/// F1: a second (or third) resource of a type, a second variable or module:
/// a labelled block of its own beside the others, the rest kept.
#[test]
fn an_entry_added_under_a_labelled_block_type_is_a_labelled_block_in_place() {
    let mut failures = Vec::new();
    from_every_view(
        &mut failures,
        "second resource",
        Language::HCL,
        ONE,
        &add("/resource/aws_instance", "db", json!({"ami": "b"})),
        &ONE.replace(
            "}\n\nvariable",
            "}\n\nresource \"aws_instance\" \"db\" {\n  ami = \"b\"\n}\n\nvariable",
        ),
    );
    from_every_view(
        &mut failures,
        "third resource",
        Language::HCL,
        TWO,
        &add("/resource/aws_instance", "cache", json!({"ami": "c"})),
        &format!("{TWO}\nresource \"aws_instance\" \"cache\" {{\n  ami = \"c\"\n}}\n"),
    );
    from_every_view(
        &mut failures,
        "second variable",
        Language::HCL,
        "variable \"a\" {\n  default = 1\n}\n",
        &add("/variable", "b", json!({"default": 2})),
        "variable \"a\" {\n  default = 1\n}\n\nvariable \"b\" {\n  default = 2\n}\n",
    );
    from_every_view(
        &mut failures,
        "second module",
        Language::HCL,
        "module \"vpc\" {\n  source = \"./vpc\"\n}\n",
        &add("/module", "db", json!({"source": "./db"})),
        "module \"vpc\" {\n  source = \"./vpc\"\n}\n\nmodule \"db\" {\n  source = \"./db\"\n}\n",
    );
    from_every_view(
        &mut failures,
        "main.tf: second aws_instance",
        Language::HCL,
        MAIN_TF,
        &add("/resource/aws_instance", "db", json!({"ami": "ami-2"})),
        &MAIN_TF.replace(
            "}\n\noutput",
            "}\n\nresource \"aws_instance\" \"db\" {\n  ami = \"ami-2\"\n}\n\noutput",
        ),
    );
    from_every_view(
        &mut failures,
        "main.tf: a new resource type",
        Language::HCL,
        MAIN_TF,
        &add(
            "/resource",
            "aws_s3_bucket",
            json!({"logs": {"bucket": "my-logs"}}),
        ),
        &MAIN_TF.replace(
            "}\n\noutput",
            "}\n\nresource \"aws_s3_bucket\" \"logs\" {\n  bucket = \"my-logs\"\n}\n\noutput",
        ),
    );
    report(failures);
}

/// L2: a block type the host already writes takes the labels its blocks
/// have (`data "aws_ami" "debian"` beside `data "aws_ami" "ubuntu"`, a
/// second Nomad `job`); a new top-level type in a host writing labelled
/// blocks waits with a plain problem instead of writing nested blocks,
/// since nothing tells which of its keys are labels; a host with no
/// labelled blocks writes nested blocks.
#[test]
fn a_block_type_takes_the_labels_the_host_gives_it_or_waits() {
    let mut failures = Vec::new();
    let with_data =
        format!("{MAIN_TF}\ndata \"aws_ami\" \"ubuntu\" {{\n  most_recent = true\n}}\n");
    from_every_view(
        &mut failures,
        "main.tf: a second data block",
        Language::HCL,
        &with_data,
        &add("/data/aws_ami", "debian", json!({"most_recent": false})),
        &format!("{with_data}\ndata \"aws_ami\" \"debian\" {{\n  most_recent = false\n}}\n"),
    );
    let nomad = "# jobs\njob \"web\" {\n  datacenters = [\"dc1\"]\n}\n";
    from_every_view(
        &mut failures,
        "nomad: a second job",
        Language::HCL,
        nomad,
        &add("/job", "api", json!({"type": "service"})),
        &format!("{nomad}\njob \"api\" {{\n  type = \"service\"\n}}\n"),
    );
    // New top-level types in a host writing labelled blocks: the save waits.
    let ami = json!({"aws_ami": {"ubuntu": {"most_recent": true}}});
    for (key, value) in [
        ("data", ami.clone()),
        ("provider", json!({"aws": {"region": "eu-west-1"}})),
        ("widget", json!({"a": {"b": {"c": 1}}})),
    ] {
        for edit in ALL.into_iter().filter(|edit| *edit != Language::HCL) {
            let pointer = format!("(at /{key})");
            match save_value(Language::HCL, edit, MAIN_TF, &add("", key, value.clone())) {
                Err(error) if error.contains("labels") && error.contains(&pointer) => {}
                other => failures.push(format!("{key} via {edit}: {other:?}")),
            }
        }
    }
    // A host with no labelled blocks writes nested blocks, as before.
    from_every_view(
        &mut failures,
        "plain HCL: nested blocks",
        Language::HCL,
        "server {\n  port = 1\n}\n",
        &add("", "data", ami),
        "server {\n  port = 1\n}\n\ndata {\n  aws_ami {\n    ubuntu {\n      most_recent = true\n    }\n  }\n}\n",
    );
    report(failures);
}
