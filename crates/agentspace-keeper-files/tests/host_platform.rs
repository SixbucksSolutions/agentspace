//! A holder record and a holder's files keyed by a host's project and platform (docs/state.md, The
//! holder record, Another host's holder, A host's platform and names; docs/keeper.md, A host's key
//! and name, The hosts of a pass, Trusting the record, The holder's log; D50 under D91; E0.12-T03-S05,
//! #0258).
//!
//! RFC 2119 keywords in these comments; the capitals are load-bearing. **A host is keyed by its
//! project and its platform, and its files are named by its host name**: `<project>` for a
//! `linux/arm64` host, so every v0.11 name keeps its bytes, and `<project>.<os>-<arch>` for a host
//! of any other platform. **The record names a platform only off `linux/arm64`**, written last, and
//! **a reader takes a record with no `platform` as `linux/arm64`'s** and a record of another
//! platform than the host's as no record. Every expectation is taken from the two docs, never from
//! the crate's code; the crate is called through `tests/common/mod.rs`'s adapters.

mod common;

use agentspace_hosts::HostKey;
use common::*;

const ARM: &str = "linux/arm64";
const LIN: &str = "linux/amd64";
const WIN: &str = "windows/amd64";

fn key(project: &str, platform: &str) -> HostKey {
    HostKey::new(project, platform)
}

/// A v0.11 record, byte for byte: v0.11 wrote no `platform`.
const V011_RECORD: &str = "{\"pid\":4242,\"process_started_at\":\"2026-11-19T13:45:07.123Z\",\
\"lease_id\":\"0123abcd-4def-4abc-9def-0123456789ab\",\
\"alias\":\"agentspace-demo-0123abcd\"}\n";

/// A record's text with `platform` spliced in as given, after `alias`: JSON, so a string needs its
/// quotes.
fn with_platform(json_value: &str) -> Vec<u8> {
    format!(
        "{{\"pid\":4242,\"process_started_at\":\"{STARTED_AT}\",\"lease_id\":\"{LEASE_ID}\",\
         \"alias\":\"{ALIAS}\",\"platform\":{json_value}}}\n"
    )
    .into_bytes()
}

// ---- A host's files are named by its host name ----

/// state.md, A host's platform and names: a `linux/arm64` host's files keep every v0.11 name, byte
/// for byte, whatever the project, and the box's, swimcoach's `linux/arm64` host's, keep v0.06's.
#[test]
fn a_linux_arm64_hosts_files_keep_their_v011_names() {
    assert_eq!(
        host_files(&key("alpha", ARM)),
        [
            "board-holder.alpha.json",
            "board-holder.alpha.json.ab12.tmp",
            "board-holder-ssh.alpha.log",
            "board-holder-ssh.alpha.log.1"
        ]
    );
    assert_eq!(
        host_files(&key("my-project", ARM))[0],
        "board-holder.my-project.json"
    );
    assert_eq!(
        host_files(&key("swimcoach", ARM)),
        [
            "board-holder.json",
            "board-holder.json.ab12.tmp",
            "board-holder-ssh.log",
            "board-holder-ssh.log.1"
        ]
    );
}

/// state.md, Another host's holder: a host of another platform has its project, a `.`, and its
/// platform with the `/` written `-`, in each name: the record, its temporary file, the log and the
/// log's rotation.
#[test]
fn a_host_of_another_platform_is_named_by_its_project_and_platform() {
    assert_eq!(
        host_files(&key("alpha", WIN)),
        [
            "board-holder.alpha.windows-amd64.json",
            "board-holder.alpha.windows-amd64.json.ab12.tmp",
            "board-holder-ssh.alpha.windows-amd64.log",
            "board-holder-ssh.alpha.windows-amd64.log.1"
        ]
    );
    assert_eq!(
        host_files(&key("alpha", LIN)),
        [
            "board-holder.alpha.linux-amd64.json",
            "board-holder.alpha.linux-amd64.json.ab12.tmp",
            "board-holder-ssh.alpha.linux-amd64.log",
            "board-holder-ssh.alpha.linux-amd64.log.1"
        ]
    );
}

/// state.md, Another host's holder: the box's files, `board-holder.json` and its log, are
/// swimcoach's `linux/arm64` host's alone. Swimcoach's host on another platform is another host
/// and has `board-holder.swimcoach.windows-amd64.json`, never the box's file.
#[test]
fn the_boxs_project_on_another_platform_has_its_own_files_never_the_boxs() {
    assert_eq!(
        host_files(&key("swimcoach", WIN)),
        [
            "board-holder.swimcoach.windows-amd64.json",
            "board-holder.swimcoach.windows-amd64.json.ab12.tmp",
            "board-holder-ssh.swimcoach.windows-amd64.log",
            "board-holder-ssh.swimcoach.windows-amd64.log.1"
        ]
    );
}

/// state.md, A host's platform and names: no name of one host equals another's. A project's three
/// hosts have three record names, and a project's `linux/arm64` host does not share a name with
/// another project's host of another platform.
#[test]
fn no_two_hosts_have_one_record_name() {
    let hosts = [
        key("alpha", ARM),
        key("alpha", LIN),
        key("alpha", WIN),
        key("beta", ARM),
        key("beta", WIN),
        key("swimcoach", ARM),
        key("swimcoach", WIN),
    ];
    let mut names: Vec<String> = hosts.iter().map(|h| host_files(h)[0].clone()).collect();
    names.sort();
    names.dedup();
    assert_eq!(names.len(), hosts.len(), "{names:?}");
}

// ---- A file's name gives its host ----

/// keeper.md, The hosts of a pass, item 3: a file named `board-holder.<host name>.json` is a host's
/// record, and the host name gives its project and platform: a project alone is that project's
/// `linux/arm64` host, and a project, a `.` and `linux-amd64` or `windows-amd64` is that project's
/// host on that platform.
#[test]
fn a_record_files_name_gives_its_projects_host_and_platform() {
    assert_eq!(
        host_of_record_file("board-holder.alpha.json"),
        Some(key("alpha", ARM))
    );
    assert_eq!(
        host_of_record_file("board-holder.alpha.linux-amd64.json"),
        Some(key("alpha", LIN))
    );
    assert_eq!(
        host_of_record_file("board-holder.alpha.windows-amd64.json"),
        Some(key("alpha", WIN))
    );
    assert_eq!(
        host_of_record_file("board-holder.my-project.windows-amd64.json"),
        Some(key("my-project", WIN))
    );
}

/// keeper.md, The hosts of a pass, item 3: the name is compared without regard to case, and the
/// host is taken in lowercase.
#[test]
fn a_record_files_name_is_compared_without_regard_to_case() {
    assert_eq!(
        host_of_record_file("Board-Holder.Alpha.Windows-AMD64.JSON"),
        Some(key("alpha", WIN))
    );
    assert_eq!(
        host_of_record_file("BOARD-HOLDER.ALPHA.JSON"),
        Some(key("alpha", ARM))
    );
}

/// keeper.md, The hosts of a pass, item 3: a project of one character and of 32 are both projects,
/// with and without a platform part.
#[test]
fn a_record_files_name_is_a_host_at_the_bounds_of_a_project() {
    let longest = format!("a{}z", "b".repeat(30));
    assert_eq!(longest.len(), 32);
    assert_eq!(
        host_of_record_file(&format!("board-holder.{longest}.windows-amd64.json")),
        Some(key(&longest, WIN))
    );
    assert_eq!(
        host_of_record_file("board-holder.a.linux-amd64.json"),
        Some(key("a", LIN))
    );
}

/// keeper.md, The hosts of a pass, item 3, and H24: a name is no host's record when its platform
/// part is `linux-arm64` (never written: no `linux/arm64` name carries one), is an unsupported
/// platform, carries more after it, or is empty or malformed, and when the project is not a
/// normalized project. The box's own `board-holder.json` has no host name.
#[test]
fn a_name_that_is_no_hosts_record_gives_no_host() {
    let too_long = "a".repeat(33);
    for name in [
        "board-holder.json",
        "board-holder.alpha.linux-arm64.json",
        "board-holder.alpha.windows-arm64.json",
        "board-holder.alpha.windows-amd64.extra.json",
        "board-holder.alpha.-amd64.json",
        "board-holder.alpha..json",
        "board-holder.alpha.windows-amd64.json.ab12.tmp",
        "board-holder.alpha.windows-amd64.log",
        "board-holder..windows-amd64.json",
        "board-holder.-a.windows-amd64.json",
        "board-holder.a-.windows-amd64.json",
        "board-holder.a_b.windows-amd64.json",
        "board-holder.alpha.windows/amd64.json",
        "board-holder.alpha.linux-amd64-v3.json",
        &format!("board-holder.{too_long}.windows-amd64.json"),
        "board-holder.alpha.windows-amd64",
        "",
    ] {
        assert_eq!(host_of_record_file(name), None, "{name}");
    }
}

/// keeper.md, The holder's log: another host's log is `board-holder-ssh.<host name>.log`, found
/// as a record's name is, so the platform part is read the same way and the box's
/// `board-holder-ssh.log` has no host name.
#[test]
fn a_log_files_name_gives_its_host_as_a_records_does() {
    assert_eq!(
        host_of_log_file("board-holder-ssh.alpha.log"),
        Some(key("alpha", ARM))
    );
    assert_eq!(
        host_of_log_file("board-holder-ssh.alpha.windows-amd64.log"),
        Some(key("alpha", WIN))
    );
    assert_eq!(
        host_of_log_file("Board-Holder-SSH.Alpha.Linux-AMD64.LOG"),
        Some(key("alpha", LIN))
    );
    for name in [
        "board-holder-ssh.log",
        "board-holder-ssh.log.1",
        "board-holder-ssh.alpha.windows-amd64.log.1",
        "board-holder-ssh.alpha.linux-arm64.log",
        "board-holder-ssh.alpha.windows-arm64.log",
        "board-holder-ssh.alpha.windows-amd64.extra.log",
        "board-holder-ssh.alpha.-amd64.log",
        "board-holder.alpha.windows-amd64.json",
    ] {
        assert_eq!(host_of_log_file(name), None, "{name}");
    }
}

/// The name a host's files carry reads back as the host: each of a project's three hosts, through
/// its record's name and its log's name.
#[test]
fn each_hosts_file_names_read_back_as_that_host() {
    for host in [key("alpha", ARM), key("alpha", LIN), key("alpha", WIN)] {
        let files = host_files(&host);
        assert_eq!(
            host_of_record_file(&files[0]),
            Some(host.clone()),
            "{host:?}"
        );
        assert_eq!(host_of_log_file(&files[2]), Some(host.clone()), "{host:?}");
    }
}

// ---- The record's bytes ----

/// state.md, The holder record: a writer omits `platform` for a `linux/arm64` host, so its record
/// keeps the bytes v0.11 wrote.
#[test]
fn a_linux_arm64_hosts_record_is_v011s_bytes() {
    let bytes = holder_record_bytes_for(&good_record(), &key("demo", ARM)).unwrap();
    assert_eq!(bytes, V011_RECORD.as_bytes());
    let boxs = holder_record_bytes_for(&good_record(), &key("swimcoach", ARM)).unwrap();
    assert_eq!(boxs, V011_RECORD.as_bytes());
}

/// state.md, The holder record: a writer writes `platform` for a host of any other platform,
/// after `alias`, as the normalized platform.
#[test]
fn another_platforms_record_names_its_platform_last() {
    for platform in [WIN, LIN] {
        let bytes = holder_record_bytes_for(&good_record(), &key("demo", platform)).unwrap();
        let want = format!(
            "{{\"pid\":4242,\"process_started_at\":\"{STARTED_AT}\",\"lease_id\":\"{LEASE_ID}\",\
             \"alias\":\"{ALIAS}\",\"platform\":\"{platform}\"}}\n"
        );
        assert_eq!(String::from_utf8(bytes).unwrap(), want, "{platform}");
    }
}

/// state.md, The holder record: every record a writer writes meets the schema and the file rules,
/// the `platform` member included.
#[test]
fn every_record_a_writer_writes_meets_its_schema() {
    for platform in [ARM, LIN, WIN] {
        let bytes = holder_record_bytes_for(&good_record(), &key("demo", platform)).unwrap();
        let v = assert_valid_json_file(&bytes, "board-holder.state.schema.json");
        assert_eq!(v["pid"], 4242, "{platform}");
        assert_eq!(v.get("platform").is_some(), platform != ARM, "{platform}");
    }
}

/// state.md, The holder record: a writer is strict to the schema, whose `platform` is the
/// normalized form and never one whose architecture is `aarch64`. A host key that makes such a
/// record is refused and nothing is written.
#[test]
fn a_writer_refuses_a_platform_the_schema_refuses() {
    for bad in [
        "linux/aarch64",
        "Windows/amd64",
        "windows/amd64/v3",
        "windows",
        "",
        "windows/amd64\n",
    ] {
        assert!(
            holder_record_bytes_for(&good_record(), &key("demo", bad)).is_err(),
            "{bad:?}"
        );
    }
}

// ---- The reader: a record is a record of its own host's platform ----

/// state.md, The holder record, and keeper.md, A host's key and name: a record written before
/// v0.12 has no `platform`, and is read as `linux/arm64`'s. It is a record for the box and for
/// every `linux/arm64` host, and for no host of another platform.
#[test]
fn a_record_written_before_v012_is_read_as_linux_arm64s() {
    assert_eq!(
        parse_holder_record_for(V011_RECORD.as_bytes(), &key("demo", ARM)),
        Some(good_record())
    );
    assert_eq!(
        parse_holder_record_for(V011_RECORD.as_bytes(), &key("swimcoach", ARM)),
        Some(good_record())
    );
}

/// The other half of the old record: with no `platform` it is not the record of a host of any
/// other platform (H25).
#[test]
fn a_record_written_before_v012_is_no_record_of_another_platforms_host() {
    for platform in [WIN, LIN] {
        assert_eq!(
            parse_holder_record_for(V011_RECORD.as_bytes(), &key("demo", platform)),
            None,
            "{platform}"
        );
    }
}

/// state.md, The holder record: a record that names its platform is a record of a host of that
/// platform alone.
#[test]
fn a_record_that_names_its_platform_is_that_platforms_hosts_alone() {
    let hosts = [ARM, LIN, WIN];
    for named in [LIN, WIN] {
        let bytes = with_platform(&format!("\"{named}\""));
        for host_platform in hosts {
            let want = (host_platform == named).then(good_record);
            assert_eq!(
                parse_holder_record_for(&bytes, &key("demo", host_platform)),
                want,
                "A record of {named} for a host of {host_platform}"
            );
        }
    }
}

/// state.md, The holder record (pin P16): a reader takes an explicit `"platform":"linux/arm64"` as
/// equal to an absent one, so it is a record of a `linux/arm64` host and of no other.
#[test]
fn an_explicit_linux_arm64_reads_as_an_absent_platform() {
    let bytes = with_platform("\"linux/arm64\"");
    assert_eq!(
        parse_holder_record_for(&bytes, &key("demo", ARM)),
        Some(good_record())
    );
    assert_eq!(parse_holder_record_for(&bytes, &key("demo", WIN)), None);
    assert_eq!(parse_holder_record_for(&bytes, &key("demo", LIN)), None);
}

/// state.md, The holder record: `null` is never accepted for `platform`, and a member that is not
/// the normalized platform is no record: a reader MUST NOT normalize a stored platform.
#[test]
fn a_platform_that_is_null_or_not_normalized_is_no_record() {
    for bad in [
        "null",
        "7",
        "[]",
        "{}",
        "true",
        "\"Windows/amd64\"",
        "\" windows/amd64\"",
        "\"windows/amd64\\n\"",
        "\"windows/amd64/v3\"",
        "\"windows\"",
        "\"\"",
    ] {
        let bytes = with_platform(bad);
        for platform in [ARM, LIN, WIN] {
            assert_eq!(
                parse_holder_record_for(&bytes, &key("demo", platform)),
                None,
                "A record whose platform is {bad} for a host of {platform}"
            );
        }
    }
}

/// A record the writer wrote for a host reads back, for that host, the same: each platform.
#[test]
fn a_written_record_reads_back_for_its_host_and_no_other() {
    let hosts = [key("demo", ARM), key("demo", LIN), key("demo", WIN)];
    for written_for in &hosts {
        let bytes = holder_record_bytes_for(&good_record(), written_for).unwrap();
        for read_for in &hosts {
            let want = (read_for == written_for).then(good_record);
            assert_eq!(
                parse_holder_record_for(&bytes, read_for),
                want,
                "Written for {written_for:?}, read for {read_for:?}"
            );
        }
    }
}

/// The reader's other rules hold with a `platform` present: a member missing, or a PID out of
/// range, is still no record.
#[test]
fn the_other_reader_rules_still_hold_beside_a_platform() {
    let host = key("demo", WIN);
    let text = String::from_utf8(with_platform("\"windows/amd64\"")).unwrap();
    assert_eq!(
        parse_holder_record_for(text.as_bytes(), &host),
        Some(good_record())
    );
    let no_alias = text.replace(&format!("\"alias\":\"{ALIAS}\","), "");
    assert_ne!(no_alias, text);
    assert_eq!(parse_holder_record_for(no_alias.as_bytes(), &host), None);
    let bad_pid = text.replace("\"pid\":4242", "\"pid\":0");
    assert_ne!(bad_pid, text);
    assert_eq!(parse_holder_record_for(bad_pid.as_bytes(), &host), None);
}
