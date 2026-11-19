//! The check of a document the CLI is about to print (lease-api.md, Output).

use agentspace_state::{OutputKind, validate_output};
use serde_json::{Value, json};
use std::fs;
use std::path::PathBuf;

fn sample(dir: &str, name: &str) -> Value {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../schemas/samples")
        .join(dir)
        .join(name);
    serde_json::from_str(&fs::read_to_string(p).unwrap()).unwrap()
}

fn ready() -> Value {
    sample("", "lease.output.ready.json")
}

#[test]
fn good_lease_documents_pass() {
    for n in [
        "compact",
        "expired",
        "pending",
        "ready",
        "released-from-pending",
        "released",
        "unsatisfiable",
    ] {
        let d = sample("", &format!("lease.output.{n}.json"));
        assert!(validate_output(OutputKind::Lease, &d), "{n}");
    }
}

#[test]
fn bad_lease_documents_fail() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../schemas/samples/invalid");
    let mut n = 0;
    for e in fs::read_dir(dir).unwrap() {
        let name = e.unwrap().file_name().to_string_lossy().into_owned();
        if name.starts_with("lease.output.") {
            assert!(
                !validate_output(OutputKind::Lease, &sample("invalid", &name)),
                "{name}"
            );
            n += 1;
        }
    }
    assert!(n > 10);
}

#[test]
fn a_lease_document_with_a_driver_member_fails() {
    let mut d = ready();
    d["driver"] = json!("ec2");
    assert!(!validate_output(OutputKind::Lease, &d));
}

#[test]
fn a_lease_document_with_a_null_or_trailing_lf_fails() {
    let mut d = ready();
    d["released_at"] = Value::Null;
    assert!(!validate_output(OutputKind::Lease, &d));
    let mut d = ready();
    let p = d["project"].as_str().unwrap().to_string();
    d["project"] = json!(format!("{p}\n"));
    assert!(!validate_output(OutputKind::Lease, &d));
}

#[test]
fn an_expired_document_with_ssh_fails() {
    let mut d = sample("", "lease.output.expired.json");
    d["ssh"] = ready()["ssh"].clone();
    assert!(!validate_output(OutputKind::Lease, &d));
}

#[test]
fn not_an_object_fails_for_every_kind() {
    for d in [json!(null), json!([]), json!("x"), json!(1), json!({})] {
        assert!(!validate_output(OutputKind::Lease, &d), "{d}");
        assert!(!validate_output(OutputKind::LeaseList, &d), "{d}");
        assert!(!validate_output(OutputKind::Error, &d), "{d}");
    }
}

#[test]
fn good_list_documents_pass() {
    for n in ["empty", "mixed"] {
        let d = sample("", &format!("lease-list.output.{n}.json"));
        assert!(validate_output(OutputKind::LeaseList, &d), "{n}");
    }
}

#[test]
fn bad_list_documents_fail() {
    for n in [
        "bare-array",
        "item-expired-with-ssh",
        "item-project-trailing-lf",
        "item-reason-leading-space",
        "item-without-state",
    ] {
        let d = sample("invalid", &format!("lease-list.output.{n}.json"));
        assert!(!validate_output(OutputKind::LeaseList, &d), "{n}");
    }
}

#[test]
fn a_list_of_one_lease_passes_and_a_lease_is_not_a_list() {
    assert!(validate_output(
        OutputKind::LeaseList,
        &json!({"leases": [ready()]})
    ));
    assert!(!validate_output(OutputKind::LeaseList, &ready()));
    assert!(!validate_output(OutputKind::Lease, &json!({"leases": []})));
    assert!(!validate_output(
        OutputKind::LeaseList,
        &json!({"leases": [], "extra": 1})
    ));
}

#[test]
fn good_error_documents_pass() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../schemas/samples");
    let mut n = 0;
    for e in fs::read_dir(dir).unwrap() {
        let name = e.unwrap().file_name().to_string_lossy().into_owned();
        if name.starts_with("error.output.") {
            assert!(
                validate_output(OutputKind::Error, &sample("", &name)),
                "{name}"
            );
            n += 1;
        }
    }
    assert!(n >= 8);
}

#[test]
fn bad_error_documents_fail() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../schemas/samples/invalid");
    let mut n = 0;
    for e in fs::read_dir(dir).unwrap() {
        let name = e.unwrap().file_name().to_string_lossy().into_owned();
        if name.starts_with("error.output.") {
            assert!(
                !validate_output(OutputKind::Error, &sample("invalid", &name)),
                "{name}"
            );
            n += 1;
        }
    }
    assert!(n >= 9);
}

#[test]
fn an_error_document_made_here() {
    // The shape is the schema's, so start from a sample and change one thing.
    let mut d = sample("", "error.output.not-found.json");
    assert!(validate_output(OutputKind::Error, &d));
    d["extra"] = json!(1);
    assert!(!validate_output(OutputKind::Error, &d));
}

#[test]
fn a_kind_does_not_accept_another_kinds_document() {
    let err = sample("", "error.output.not-found.json");
    assert!(!validate_output(OutputKind::Lease, &err));
    assert!(!validate_output(OutputKind::LeaseList, &err));
    assert!(!validate_output(OutputKind::Error, &ready()));
}
