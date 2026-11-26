//! The lease state file's `ssh.user` (docs/state.md, The lease state file; D95; E0.12-T02-S01,
//! card #0250): the record's pattern, `^[a-z_][a-z0-9_-]{0,31}$(?!\n)`, binds, so a lease on
//! `windows/amd64` can hold `agent`, and a user that broke the pattern before still breaks it.
//!
//! RFC 2119 keywords, and the capitals are load-bearing. state.md: "`agent` matches both
//! patterns, so the state schema's pattern does not change, and a program built before v0.12
//! reads a record of either. The schema does not check that a record's user goes with its
//! platform; the driver writes it so."

mod common;

use agentspace_state::{OutputKind, StateFile, lease_document, parse, serialize, validate_output};
use common::*;
use serde_json::{Value, json};

const SAMPLE_WINDOWS_AGENT: &str =
    include_str!("../../../schemas/state/samples/leases.state.windows-agent.json");

fn ready_for(id: &str, project: &str, platform: &str, user: &str) -> Value {
    let rec = with(ready(id, project), "platform", Some(json!(platform)));
    let alias = format!("agentspace-{project}-{}", &id[..8]);
    with(rec, "ssh", Some(json!({"host_alias": alias, "user": user})))
}

fn with_user(user: &str) -> Value {
    ready_for(ID_A, "demo", "windows/amd64", user)
}

fn parses(user: &str) -> bool {
    parse(&bytes_of(vec![with_user(user)])).is_ok()
}

fn parsed(records: Vec<Value>) -> StateFile {
    parse(&bytes_of(records)).expect("The fixture must parse")
}

// --- agent is a valid user --------------------------------------------------------------------

#[test]
fn a_ready_lease_on_windows_amd64_may_hold_the_user_agent() {
    let f = parsed(vec![with_user("agent")]);
    let ssh = f.leases[0].ssh.as_ref().expect("A ready record has ssh");
    assert_eq!(ssh.user, "agent");
    assert_eq!(f.leases[0].platform, "windows/amd64");
}

#[test]
fn the_user_agent_is_written_back_as_it_was_read() {
    let f = parsed(vec![with_user("agent")]);
    let out = serialize(&f).expect("A record holding agent is valid to write");
    let text = String::from_utf8(out).unwrap();
    assert!(
        text.contains("\"ssh\":{\"host_alias\":\"agentspace-demo-3f2a9c1e\",\"user\":\"agent\"}"),
        "{text}"
    );
    let back = parse(text.as_bytes()).unwrap();
    assert_eq!(back.leases[0].ssh.as_ref().unwrap().user, "agent");
}

#[test]
fn a_user_set_to_agent_in_a_record_already_read_is_accepted_by_serialize() {
    let mut f = parsed(vec![ready(ID_A, "demo")]);
    f.leases[0].platform = "windows/amd64".to_string();
    f.leases[0].ssh.as_mut().unwrap().user = "agent".to_string();
    assert!(serialize(&f).is_ok());
}

#[test]
fn the_lease_document_shows_the_user_agent_and_validates() {
    let f = parsed(vec![with_user("agent")]);
    let d = lease_document(&f.leases[0], EXPIRES_UNIX - 1);
    assert_eq!(d["ssh"]["user"], "agent");
    assert_eq!(d["platform"], "windows/amd64");
    assert!(validate_output(OutputKind::Lease, &d), "{d}");
}

#[test]
fn the_windows_agent_sample_reads_with_each_leases_user() {
    let f = parse(SAMPLE_WINDOWS_AGENT.as_bytes()).expect("The sample is valid");
    assert_eq!(f.leases.len(), 2);
    assert_eq!(f.leases[0].platform, "windows/amd64");
    assert_eq!(f.leases[0].ssh.as_ref().unwrap().user, "agent");
    assert_eq!(f.leases[1].platform, "linux/amd64");
    assert_eq!(f.leases[1].ssh.as_ref().unwrap().user, "ubuntu");
}

#[test]
fn the_windows_agent_sample_is_written_back_and_reads_the_same() {
    let f = parse(SAMPLE_WINDOWS_AGENT.as_bytes()).unwrap();
    let once = serialize(&f).unwrap();
    let again = serialize(&parse(&once).unwrap()).unwrap();
    assert_eq!(once, again);
    let text = String::from_utf8(once).unwrap();
    assert!(text.contains("\"user\":\"agent\""), "{text}");
    assert!(text.contains("\"user\":\"ubuntu\""), "{text}");
}

#[test]
fn the_list_document_of_a_windows_lease_validates() {
    let f = parse(SAMPLE_WINDOWS_AGENT.as_bytes()).unwrap();
    let l = agentspace_state::list_document(&f, None, &[], EXPIRES_UNIX - 1);
    assert!(validate_output(OutputKind::LeaseList, &l), "{l}");
}

// --- What the pattern still allows and refuses ------------------------------------------------

#[test]
fn every_user_the_pattern_allows_is_accepted() {
    let thirty_two = format!("a{}", "b".repeat(31));
    for user in [
        "agent",
        "ubuntu",
        "a",
        "_",
        "_agent",
        "agent-2",
        "agent_2",
        "a-b_c9",
        thirty_two.as_str(),
    ] {
        assert!(parses(user), "{user}");
    }
}

#[test]
fn a_user_the_pattern_refuses_is_still_refused() {
    let thirty_three = format!("a{}", "b".repeat(32));
    for user in [
        "Agent",
        "AGENT",
        "Ubuntu",
        "",
        "1agent",
        "-agent",
        "agent user",
        " agent",
        "agent ",
        "agent.",
        "ag.ent",
        "agent\n",
        "agent\\n",
        "agént",
        "agent@host",
        "agent/",
        thirty_three.as_str(),
    ] {
        assert!(!parses(user), "{user:?}");
    }
}

#[test]
fn a_user_with_a_line_feed_at_its_end_is_refused() {
    let doc = bytes_of(vec![with_user("agent")]);
    let text = String::from_utf8(doc).unwrap();
    let from = "\"user\":\"agent\"";
    assert_eq!(text.matches(from).count(), 1);
    let bad = text.replace(from, "\"user\":\"agent\\n\"");
    assert!(parse(bad.as_bytes()).is_err());
}

#[test]
fn serialize_refuses_a_user_that_breaks_the_pattern() {
    for user in ["Agent", "", "1agent", "agent user", "agent\n"] {
        let mut f = parsed(vec![ready(ID_A, "demo")]);
        f.leases[0].ssh.as_mut().unwrap().user = user.to_string();
        assert!(serialize(&f).is_err(), "{user:?}");
    }
}

#[test]
fn a_user_given_twice_is_still_not_valid() {
    let text = String::from_utf8(bytes_of(vec![with_user("agent")])).unwrap();
    let from = "\"user\":\"agent\"";
    let bad = text.replace(from, "\"user\":\"agent\",\"user\":\"agent\"");
    assert!(parse(bad.as_bytes()).is_err());
}

// --- The user is not compared with the platform -----------------------------------------------

#[test]
fn the_schema_does_not_check_that_a_user_goes_with_its_platform() {
    // state.md: the driver writes the user; the schema does not check it against the platform.
    assert!(
        parse(&bytes_of(vec![ready_for(
            ID_A,
            "demo",
            "linux/arm64",
            "agent"
        )]))
        .is_ok()
    );
    assert!(
        parse(&bytes_of(vec![ready_for(
            ID_A,
            "demo",
            "linux/amd64",
            "agent"
        )]))
        .is_ok()
    );
    assert!(
        parse(&bytes_of(vec![ready_for(
            ID_A,
            "demo",
            "windows/amd64",
            "ubuntu"
        )]))
        .is_ok()
    );
}

// --- Files v0.11 wrote read unchanged ---------------------------------------------------------

#[test]
fn a_v0_11_ready_record_with_ubuntu_reads_and_is_written_back_unchanged() {
    let bytes = bytes_of(vec![ready(ID_A, "demo"), pending(ID_B, "demo")]);
    let f = parse(&bytes).unwrap();
    assert_eq!(f.leases[0].ssh.as_ref().unwrap().user, "ubuntu");
    let out = serialize(&f).unwrap();
    let again = parse(&out).unwrap();
    assert_eq!(serialize(&again).unwrap(), out);
    assert!(!String::from_utf8(out).unwrap().contains("agent\""));
}
