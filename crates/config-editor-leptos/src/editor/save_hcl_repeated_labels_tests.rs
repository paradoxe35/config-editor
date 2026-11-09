//! Labelled HCL blocks written more than once with the same type and
//! labels (`provider "aws" { … }` twice, the second with an alias; two
//! `provisioner "local-exec"`; two `dynamic "ingress"`): read as a list
//! under their last label, as `hcl-rs` reads them, and edited from every
//! other view in place, the rest of the document byte for byte.

use super::Language;
use super::save_matrix::{from_every_view, report, slot};
use serde_json::{Value, json};

const PROVIDERS: &str = "provider \"aws\" {\n  region = \"eu-west-1\"\n}\n\nprovider \"aws\" {\n  alias  = \"us\"\n  region = \"us-east-1\"\n}\n\nvariable \"x\" {\n  default = 1\n}\n";

const PROVISIONERS: &str = "resource \"null_resource\" \"a\" {\n  triggers = { v = 1 }\n\n  provisioner \"local-exec\" {\n    command = \"echo one\"\n  }\n\n  provisioner \"local-exec\" {\n    command = \"echo two\"\n  }\n}\n";

const DYNAMIC: &str = "resource \"aws_security_group\" \"s\" {\n  name = \"s\"\n\n  dynamic \"ingress\" {\n    for_each = var.a\n    content {\n      from_port = ingress.value\n    }\n  }\n\n  dynamic \"ingress\" {\n    for_each = var.b\n    content {\n      from_port = ingress.value\n    }\n  }\n}\n";

/// A Terraform module with two `provider "aws"` blocks.
const MODULE: &str = include_str!("../../tests/fixtures/real/module.tf");

fn set(pointer: &'static str, value: Value) -> impl Fn(&mut Value) {
    move |document| *slot(document, pointer) = value.clone()
}

#[test]
fn repeated_labelled_blocks_read_as_a_list_under_their_last_label() {
    let read = |text: &str| Language::HCL.parse(text).unwrap();
    assert_eq!(
        read(PROVIDERS)["provider"],
        json!({"aws": [{"region": "eu-west-1"}, {"alias": "us", "region": "us-east-1"}]})
    );
    assert_eq!(
        read(PROVISIONERS)["resource"]["null_resource"]["a"]["provisioner"],
        json!({"local-exec": [{"command": "echo one"}, {"command": "echo two"}]})
    );
}

#[test]
fn a_value_in_or_beside_repeated_labelled_blocks_is_saved_in_place_from_every_view() {
    let mut failures = Vec::new();
    let cases: [(&str, &str, &str, Value, &str, &str); 9] = [
        (
            "providers: variable",
            PROVIDERS,
            "/variable/x/default",
            json!(12),
            "default = 1",
            "default = 12",
        ),
        (
            "providers: first region",
            PROVIDERS,
            "/provider/aws/0/region",
            json!("eu-west-2"),
            "\"eu-west-1\"",
            "\"eu-west-2\"",
        ),
        (
            "providers: second region",
            PROVIDERS,
            "/provider/aws/1/region",
            json!("us-east-1X"),
            "\"us-east-1\"",
            "\"us-east-1X\"",
        ),
        (
            "provisioners: triggers",
            PROVISIONERS,
            "/resource/null_resource/a/triggers/v",
            json!(12),
            "v = 1",
            "v = 12",
        ),
        (
            "provisioners: second command",
            PROVISIONERS,
            "/resource/null_resource/a/provisioner/local-exec/1/command",
            json!("echo twoX"),
            "echo two",
            "echo twoX",
        ),
        (
            "dynamic: name",
            DYNAMIC,
            "/resource/aws_security_group/s/name",
            json!("sX"),
            "name = \"s\"",
            "name = \"sX\"",
        ),
        (
            "module: variable default",
            MODULE,
            "/variable/region/default",
            json!("eu-west-2"),
            "default = \"eu-west-1\"",
            "default = \"eu-west-2\"",
        ),
        (
            "module: aliased provider",
            MODULE,
            "/provider/aws/1/region",
            json!("us-west-2"),
            "\"us-east-1\"",
            "\"us-west-2\"",
        ),
        (
            "module: instance type",
            MODULE,
            "/resource/aws_instance/worker/instance_type",
            json!("t3.large"),
            "\"t3.medium\"",
            "\"t3.large\"",
        ),
    ];
    for (label, host, pointer, value, was, now) in cases {
        from_every_view(
            &mut failures,
            label,
            Language::HCL,
            host,
            &set(pointer, value),
            &host.replacen(was, now, 1),
        );
    }
    report(failures);
}

#[test]
fn an_attribute_added_to_one_of_repeated_labelled_blocks_goes_in_that_block() {
    let mut failures = Vec::new();
    from_every_view(
        &mut failures,
        "providers: profile",
        Language::HCL,
        PROVIDERS,
        &|document| {
            slot(document, "/provider/aws/1")
                .as_object_mut()
                .unwrap()
                .insert("profile".into(), json!("ops"));
        },
        &PROVIDERS.replace(
            "region = \"us-east-1\"\n",
            "region = \"us-east-1\"\n  profile = \"ops\"\n",
        ),
    );
    report(failures);
}

#[test]
fn one_of_repeated_labelled_blocks_removed_or_added_from_every_view() {
    let mut failures = Vec::new();
    from_every_view(
        &mut failures,
        "providers: the aliased one removed",
        Language::HCL,
        PROVIDERS,
        &|document| {
            let aws = document.pointer_mut("/provider/aws").unwrap();
            *aws = aws[0].clone();
        },
        &PROVIDERS.replace(
            "provider \"aws\" {\n  alias  = \"us\"\n  region = \"us-east-1\"\n}\n\n",
            "",
        ),
    );
    from_every_view(
        &mut failures,
        "provisioners: a third",
        Language::HCL,
        PROVISIONERS,
        &|document| {
            document
                .pointer_mut("/resource/null_resource/a/provisioner/local-exec")
                .and_then(Value::as_array_mut)
                .unwrap()
                .push(json!({"command": "echo three"}));
        },
        &PROVISIONERS.replace(
            "echo two\"\n  }\n",
            "echo two\"\n  }\n\n  provisioner \"local-exec\" {\n    command = \"echo three\"\n  }\n",
        ),
    );
    report(failures);
}

/// A repeated labelled block made something no block holds waits, with
/// the problem at that block.
#[test]
fn a_repeated_labelled_block_made_a_scalar_waits_with_the_problem_at_it() {
    for edit in super::save_matrix::ALL
        .into_iter()
        .filter(|edit| *edit != Language::HCL)
    {
        let saved = super::save_matrix::save_value(Language::HCL, edit, PROVIDERS, &|document| {
            document["provider"]["aws"] = json!([{"region": "a"}, "b"]);
        });
        match saved {
            Err(error) if error.contains("(at /provider/aws") => {}
            other => panic!("via {edit}: {other:?}"),
        }
    }
}
