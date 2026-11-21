//! Every operator command given `-Host` for a host that is not the box signs a fresh one-hour
//! certificate for the host's own key and writes the host's operator alias (docs/operator.md: How
//! the operator reaches a host; docs/state.md: A host's folder, The operator's key and
//! certificate, A host's operator alias). `ssh-keygen` has no seam variable (operator.md, The test
//! seam, "Not covered"), so these runs use the real `SystemRoot` and a throwaway CA that the real
//! `ca` makes in the rig's throwaway home: nothing real is read, signed or reached. A test process
//! that is elevated makes no CA (`ca` refuses), and each test then says so and returns.
//!
//! The operator runs from a copy in the rig's own folder with a stand-in file named
//! `agentspace-ssh-proxy.exe` beside it, which is all the alias's ProxyCommand line needs to name.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
#![cfg(windows)]

#[path = "common/rig.rs"]
mod rig;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use rig::*;

const ZONE: &str = "America/New_York";

struct Signing {
    rig: Rig,
    operator: PathBuf,
}

impl Signing {
    /// A rig holding the box and the project `agentspace` (ready, `OTHER_ID`), a CA, and an
    /// operator with its proxy beside it. `None` when the test process is elevated.
    fn new() -> Option<Signing> {
        let rig = Rig::standard();
        rig.write_box_and("agentspace", "ready", ID, Some(OTHER_ID));
        let out = rig.op(&["ca"]).real_windows().run();
        if out.err().contains("elevated shell") {
            eprintln!("skipped: the test process is elevated, so ca refuses to create a CA");
            return None;
        }
        assert_eq!(out.code, Some(0), "ca: {:?}", out.err());
        let operator = rig.operator_with_proxy();
        Some(Signing { rig, operator })
    }

    fn run(&self, args: &[&str]) -> Out {
        self.rig.op(args).exe(&self.operator).real_windows().run()
    }

    fn dir(&self) -> PathBuf {
        self.rig.host_dir("agentspace")
    }

    fn key(&self) -> PathBuf {
        self.dir().join("id_ed25519")
    }

    fn cert(&self) -> PathBuf {
        self.dir().join("id_ed25519-cert.pub")
    }

    fn alias(&self) -> PathBuf {
        self.rig.host_alias_path("agentspace")
    }

    /// Forgets what a command made, so that the next one is shown to make it.
    fn clear(&self) {
        let _ = fs::remove_dir_all(self.rig.state_dir().join("hosts"));
        let _ = fs::remove_dir_all(self.rig.profile.join(".ssh"));
    }

    fn made_anything(&self) -> bool {
        self.rig.state_dir().join("hosts").exists()
            || self.rig.profile.join(".ssh").join("agentspace.d").exists()
    }
}

fn ssh_keygen() -> PathBuf {
    Path::new(&std::env::var_os("SystemRoot").expect("SystemRoot"))
        .join("System32")
        .join("OpenSSH")
        .join("ssh-keygen.exe")
}

fn keygen_text(args: &[&Path]) -> String {
    let out = Command::new(ssh_keygen())
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("ssh-keygen runs");
    assert!(out.status.success(), "ssh-keygen {args:?}");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn status_lines(id: &str) -> String {
    format!(
        "Instance  {id}\nState     running\nType      m9g.xlarge\nDisk      8 GiB gp3 ({VOL})\nSSM       online\n"
    )
}

/// Days since 1970-01-01 of a civil date (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// `2026-11-21T10:55:00` as seconds, with no zone: only a difference of two such times is used.
fn seconds_of(text: &str) -> i64 {
    let (date, time) = text.split_once('T').expect("a date and a time");
    let mut d = date.split('-').map(|p| p.parse::<i64>().expect("a number"));
    let mut t = time.split(':').map(|p| p.parse::<i64>().expect("a number"));
    days_from_civil(d.next().unwrap(), d.next().unwrap(), d.next().unwrap()) * 86_400
        + t.next().unwrap() * 3600
        + t.next().unwrap() * 60
        + t.next().unwrap()
}

/// The two times of `ssh-keygen -L`'s `Valid: from <a> to <b>` line.
fn validity(listing: &str) -> (i64, i64) {
    let line = listing
        .lines()
        .find(|l| l.trim_start().starts_with("Valid:"))
        .expect("a Valid line");
    let words: Vec<&str> = line.split_whitespace().collect();
    let from = words
        .iter()
        .position(|w| *w == "from")
        .expect("a from time");
    let to = words.iter().position(|w| *w == "to").expect("a to time");
    (seconds_of(words[from + 1]), seconds_of(words[to + 1]))
}

// ---- What a command given -Host makes ----

#[test]
fn a_command_given_a_host_makes_the_key_signs_the_certificate_and_writes_the_alias() {
    let Some(s) = Signing::new() else { return };
    let out = s.run(&["status", "-Host", "agentspace"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(out.err(), status_lines(OTHER_ID));

    // The key: ed25519, no passphrase, in OpenSSH's format, with its public half beside it.
    let private = fs::read_to_string(s.key()).expect("the operator's key");
    assert!(
        private.starts_with("-----BEGIN OPENSSH PRIVATE KEY-----"),
        "an OpenSSH private key"
    );
    let public_path = s.dir().join("id_ed25519.pub");
    let public = fs::read_to_string(&public_path).expect("its public half");
    assert!(public.starts_with("ssh-ed25519 "), "{public:?}");

    // The certificate: signed by the CA, principal ubuntu, key ID operator-agentspace, one hour.
    let listing = keygen_text(&[Path::new("-L"), Path::new("-f"), &s.cert()]);
    assert!(
        listing.contains("user certificate"),
        "a user certificate: {listing}"
    );
    assert!(
        listing.contains("Key ID: \"operator-agentspace\""),
        "{listing}"
    );
    let principals: Vec<&str> = listing
        .lines()
        .skip_while(|l| !l.trim_start().starts_with("Principals:"))
        .skip(1)
        .take_while(|l| {
            let t = l.trim();
            !t.is_empty() && !t.contains(':')
        })
        .map(str::trim)
        .collect();
    assert_eq!(principals, ["ubuntu"], "exactly one principal: {listing}");
    let (from, to) = validity(&listing);
    assert!(
        (3600..=3600 + 600).contains(&(to - from)),
        "valid for one hour, back-dated by a few minutes at most: {} seconds",
        to - from
    );
    let ca_pub = s.rig.state_dir().join("ca").join("agentspace_ca.pub");
    let ca_fp = keygen_text(&[Path::new("-l"), Path::new("-f"), &ca_pub]);
    let ca_fp = ca_fp
        .split_whitespace()
        .nth(1)
        .expect("a fingerprint")
        .to_string();
    assert!(
        listing.contains(&ca_fp),
        "signed by the CA ({ca_fp}): {listing}"
    );

    // The alias: the owner mark, the host's lines, no RemoteForward, LF line ends.
    let alias = fs::read(s.alias()).expect("the host's operator alias");
    let text = String::from_utf8(alias).expect("UTF-8");
    assert!(
        !text.starts_with('\u{feff}') && !text.contains('\r'),
        "{text:?}"
    );
    assert!(text.ends_with('\n'), "{text:?}");
    let lines: Vec<&str> = text.lines().collect();
    assert!(
        lines[0].starts_with("# agentspace host agentspace; "),
        "the owner mark: {:?}",
        lines[0]
    );
    assert_eq!(lines[1], "Host agentspace-host.agentspace");
    for wanted in [
        "  User ubuntu",
        "  IdentitiesOnly yes",
        "  HostKeyAlias agentspace-host.agentspace",
        "  StrictHostKeyChecking accept-new",
        "  ServerAliveInterval 30",
        "  ServerAliveCountMax 4",
    ] {
        assert!(lines.contains(&wanted), "{wanted:?} in {text:?}");
    }
    for (name, target) in [
        ("  IdentityFile \"", "id_ed25519\""),
        ("  CertificateFile \"", "id_ed25519-cert.pub\""),
        ("  UserKnownHostsFile \"", "known_hosts\""),
    ] {
        let line = lines
            .iter()
            .find(|l| l.starts_with(name))
            .unwrap_or_else(|| panic!("{name:?} in {text:?}"));
        assert!(line.ends_with(target), "{line:?}");
    }
    let proxy = lines
        .iter()
        .find(|l| l.starts_with("  ProxyCommand "))
        .expect("a ProxyCommand line");
    assert!(
        proxy.contains("agentspace-ssh-proxy.exe")
            && proxy.ends_with(" agentspace-host.agentspace"),
        "the program beside the operator, then the alias: {proxy:?}"
    );
    assert!(
        !text.contains("RemoteForward"),
        "the operator holds no board: {text:?}"
    );
    assert!(
        !s.rig.state_dir().join("known_hosts").exists(),
        "the operator never writes agentspace's known_hosts"
    );

    // The Include line, above any Host line.
    let config = fs::read_to_string(s.rig.profile.join(".ssh").join("config"))
        .expect("the ssh config, with the Include line added");
    assert_eq!(
        config.lines().map(str::trim).find(|l| !l.is_empty()),
        Some("Include agentspace.d/*")
    );
}

#[test]
fn the_key_is_made_once_and_every_command_signs_a_fresh_certificate_and_leaves_no_scratch_file() {
    let Some(s) = Signing::new() else { return };
    assert_eq!(s.run(&["status", "-Host", "agentspace"]).code, Some(0));
    let key = fs::read(s.key()).unwrap();
    let cert = fs::read(s.cert()).unwrap();
    let alias = fs::read(s.alias()).unwrap();
    // A certificate's times are whole seconds, so wait for the clock to move on.
    std::thread::sleep(std::time::Duration::from_millis(2100));
    assert_eq!(s.run(&["status", "-Host", "agentspace"]).code, Some(0));
    assert_eq!(
        fs::read(s.key()).unwrap(),
        key,
        "an existing key is never replaced"
    );
    assert_ne!(
        fs::read(s.cert()).unwrap(),
        cert,
        "a fresh certificate replaces the last one"
    );
    assert_eq!(
        fs::read(s.alias()).unwrap(),
        alias,
        "the alias is written again only when its bytes differ, and these do not"
    );
    let mut names: Vec<String> = fs::read_dir(s.dir())
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(
        names,
        ["id_ed25519", "id_ed25519-cert.pub", "id_ed25519.pub"],
        "the host's folder holds the key, its public half and the certificate alone"
    );
}

#[test]
fn every_command_given_a_host_signs_and_none_of_the_box_or_all_does() {
    let Some(s) = Signing::new() else { return };
    // Commands that sign for a host that is not the box.
    s.rig.instance("stopped");
    for line in [
        vec!["down", "-Host", "agentspace"],
        vec!["status", "-Host", "agentspace"],
        vec!["up", "-Host", "agentspace"],
        vec!["grow", "-Host", "agentspace"],
    ] {
        s.clear();
        let out = s.run(&line);
        assert_eq!(out.code, Some(0), "{line:?}: {:?}", out.err());
        assert!(s.key().is_file(), "{line:?}: the key");
        assert!(s.cert().is_file(), "{line:?}: the certificate");
        assert!(s.alias().is_file(), "{line:?}: the alias");
    }
    // Commands that never sign.
    for line in [
        vec!["status", "-All"],
        vec!["down", "-All"],
        vec!["status", "-Host", "swimcoach"],
        vec!["down", "-Host", "swimcoach"],
        vec!["status"],
        vec!["down"],
    ] {
        s.clear();
        let out = s.run(&line);
        assert_eq!(out.code, Some(0), "{line:?}: {:?}", out.err());
        assert!(
            !s.made_anything(),
            "{line:?}: no key, certificate or alias, though a CA exists and could sign"
        );
    }
    // A nested `up` never signs either: it never pushes.
    s.clear();
    let out = s
        .rig
        .op(&["up", "-Host", "agentspace"])
        .exe(&s.operator)
        .real_windows()
        .env("AGENTSPACE_NESTED", "1")
        .run();
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert!(!s.made_anything());
}

// ---- Failures in the alias ----

#[test]
fn a_file_at_the_aliass_path_that_is_not_the_hosts_own_is_left_alone() {
    for first_line in [
        "# a person's own entry",
        "Host agentspace-host.agentspace",
        "# agentspace host other; written by agentspace",
        "# agentspace lease abc; written by agentspace",
    ] {
        let Some(s) = Signing::new() else { return };
        fs::create_dir_all(s.alias().parent().unwrap()).unwrap();
        let theirs = format!("{first_line}\nHost x\n  HostName y\n");
        fs::write(s.alias(), &theirs).unwrap();
        let out = s.run(&["status", "-Host", "agentspace"]);
        let err = out.err();
        assert_eq!(out.code, Some(1), "{first_line:?}: {err:?}");
        assert!(
            err.starts_with(&status_lines(OTHER_ID)),
            "status prints its report first: {err:?}"
        );
        let last = err.lines().last().unwrap();
        assert!(
            last.starts_with("agentspace: ")
                && last.ends_with(
                    " is not agentspace's alias for 'agentspace'; it was left as it is."
                )
                && last.contains("agentspace-host.agentspace"),
            "{first_line:?}: {last:?}"
        );
        assert_eq!(
            fs::read_to_string(s.alias()).unwrap(),
            theirs,
            "{first_line:?}: the file is left as it was"
        );
    }
}

#[test]
fn a_proxy_program_that_is_not_beside_the_operator_fails_and_writes_no_alias() {
    let Some(s) = Signing::new() else { return };
    let bare = s.rig.operator_without_proxy();
    let out = s
        .rig
        .op(&["status", "-Host", "agentspace"])
        .exe(&bare)
        .real_windows()
        .run();
    let err = out.err();
    assert_eq!(out.code, Some(1), "{err:?}");
    assert!(err.starts_with(&status_lines(OTHER_ID)), "{err:?}");
    let last = err.lines().last().unwrap();
    assert!(
        last.starts_with("agentspace: ")
            && last.ends_with(
                "agentspace-ssh-proxy.exe is missing; the operator's alias for a host needs it beside the operator."
            ),
        "{last:?}"
    );
    assert!(
        !s.alias().exists(),
        "no alias is written for a proxy that is not there"
    );
    assert!(s.rig.calls_of("ssh").is_empty(), "and no ssh is started");
}

#[test]
fn down_for_a_host_stops_it_before_signing_and_the_stop_stands_when_signing_fails() {
    let Some(s) = Signing::new() else { return };
    fs::create_dir_all(s.alias().parent().unwrap()).unwrap();
    fs::write(s.alias(), "# a person's own entry\n").unwrap();
    let out = s.run(&["down", "-Host", "agentspace"]);
    let lines = out.err_lines();
    assert_eq!(out.code, Some(1), "{lines:?}");
    assert_eq!(lines[0], format!("Stopping {OTHER_ID}."));
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert_eq!(
        s.rig.keys(),
        ["ec2-stop-instances"],
        "the stop was requested"
    );
}

#[test]
fn status_all_and_down_all_never_fail_on_the_ca_even_when_the_hosts_alias_is_not_the_operators() {
    // The cost guard's two tools: no signing, no alias, so nothing there can fail them.
    let Some(s) = Signing::new() else { return };
    fs::create_dir_all(s.alias().parent().unwrap()).unwrap();
    fs::write(s.alias(), "# a person's own entry\n").unwrap();
    for line in [vec!["status", "-All"], vec!["down", "-All"]] {
        let out = s.run(&line);
        assert_eq!(out.code, Some(0), "{line:?}: {:?}", out.err());
    }
    assert!(!s.dir().exists(), "no host folder was made");
}

// ---- The push and grow, over the host's alias ----

#[test]
fn up_for_a_host_pushes_over_the_hosts_alias_and_keeps_the_hosts_own_stamp() {
    let Some(s) = Signing::new() else { return };
    s.rig.localswim();
    let hash = sha256_hex(s.rig.content().as_bytes());
    s.rig.answer(
        "ssh",
        None,
        Some(&format!("{hash}  -\nzone {ZONE}\n")),
        None,
        Some(0),
    );
    let out = s.run(&["up", "-Host", "agentspace"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    let ssh = s.rig.calls_of("ssh");
    assert_eq!(ssh.len(), 1, "one push: {:?}", out.err());
    assert_eq!(
        &ssh[0].args[..5],
        [
            "-o",
            "ClearAllForwardings=yes",
            "-o",
            "BatchMode=yes",
            "agentspace-host.agentspace"
        ],
        "the host's own alias, not `agentspace`"
    );
    assert_eq!(
        fs::read(s.dir().join("localswim-pushed.json")).ok(),
        Some(
            format!(
                "{{\"version\":0,\"stamp\":\"{}\"}}",
                expected_stamp(OTHER_ID, ZONE, &s.rig.content())
            )
            .into_bytes()
        ),
        "the host's stamp covers the host's own instance ID"
    );
    assert!(
        !s.rig.stamp_path().exists(),
        "the box's stamp is never written for another host"
    );

    // The stamp now matches: a second `up` sends nothing.
    let again = s.run(&["up", "-Host", "agentspace"]);
    assert_eq!(again.code, Some(0), "{:?}", again.err());
    assert_eq!(s.rig.calls_of("ssh").len(), 1, "nothing is sent twice");
}

#[test]
fn up_for_a_host_with_an_alias_pushes_over_that_alias_and_still_stamps_the_host() {
    let Some(s) = Signing::new() else { return };
    s.rig.localswim();
    let hash = sha256_hex(s.rig.content().as_bytes());
    s.rig.answer(
        "ssh",
        None,
        Some(&format!("{hash}  -\nzone {ZONE}\n")),
        None,
        Some(0),
    );
    let out = s.run(&["up", "-Host", "agentspace", "-Alias", "custom.alias"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    let ssh = s.rig.calls_of("ssh");
    assert_eq!(ssh.len(), 1);
    assert_eq!(ssh[0].args[4], "custom.alias");
    assert!(
        s.dir().join("localswim-pushed.json").is_file(),
        "-Host picks the host's stamp; -Alias picks only the alias"
    );
    for c in s.rig.calls() {
        if c.role == "aws" {
            assert!(
                c.args.iter().all(|a| !a.contains(ID)),
                "the box's instance is never named for a host's command"
            );
        }
    }
}

#[test]
fn grow_for_a_running_host_goes_through_the_hosts_alias_after_signing() {
    let Some(s) = Signing::new() else { return };
    s.rig
        .answer("ssh", None, Some("DF-OUTPUT\n"), None, Some(0));
    let out = s.run(&["grow", "-Host", "agentspace", "-GiB", "3"]);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(out.out(), "DF-OUTPUT\n");
    assert_eq!(
        s.rig.keys(),
        [
            "ec2-describe-instances",
            "ec2-describe-volumes",
            "ec2-modify-volume",
            "ssh"
        ]
    );
    let ssh = &s.rig.calls_of("ssh")[0];
    assert_eq!(
        &ssh.args[..3],
        [
            "-o",
            "ClearAllForwardings=yes",
            "agentspace-host.agentspace"
        ],
        "one ssh through the host's alias with forwards cleared"
    );
    assert_eq!(
        ssh.args.len(),
        4,
        "the remote command is one argument, the last"
    );
    assert!(s.cert().is_file(), "signed before the online step");
    assert!(
        s.rig.calls()[2].args.contains(&"11".to_string()),
        "8 GiB plus 3"
    );
}

// ---- The same sign-in through agentspace-sync and the operator ----

/// The Include line below a Host block, which agentspace-alias refuses as a `Config` failure.
const INCLUDE_BELOW_A_HOST: &str = "Host other\n  User someone\nInclude agentspace.d/*\n";

#[test]
fn an_include_line_that_cannot_be_added_gives_sync_and_status_the_same_line() {
    // `sync -Host` signs through agentspace-sync, `status -Host` through the operator's own sign-in;
    // the failure is the same one line (operator.md, How the operator reaches a host, step 4;
    // architect, 2026-11-21, for v0.07-T03).
    let Some(s) = Signing::new() else { return };
    let config = s.rig.profile.join(".ssh").join("config");
    fs::create_dir_all(config.parent().unwrap()).unwrap();
    fs::write(&config, INCLUDE_BELOW_A_HOST).unwrap();

    let synced = s.run(&["sync", "-Host", "agentspace"]);
    assert_eq!(synced.code, Some(1), "{:?}", synced.err());
    assert_eq!(synced.out(), "");
    let sync_line = synced.err_lines().last().cloned().unwrap_or_default();
    assert!(
        s.rig.calls().is_empty(),
        "sync signs before its start-and-wait, so nothing was started"
    );

    let status = s.run(&["status", "-Host", "agentspace"]);
    assert_eq!(status.code, Some(1), "{:?}", status.err());
    let err = status.err();
    assert!(err.starts_with(&status_lines(OTHER_ID)), "{err:?}");
    let status_line = status.err_lines().last().cloned().unwrap_or_default();

    assert_eq!(sync_line, status_line, "one failure, one line");
    let prefix = format!(
        "agentspace: {} cannot take agentspace's Include line: ",
        config.display()
    );
    assert!(sync_line.starts_with(&prefix), "{sync_line:?}");
    assert!(
        sync_line.ends_with('.') && sync_line.len() > prefix.len() + 1,
        "the reason, then a period: {sync_line:?}"
    );
    assert_eq!(
        fs::read_to_string(&config).unwrap(),
        INCLUDE_BELOW_A_HOST,
        "the config is left as it was"
    );
    assert!(!s.alias().exists(), "no alias without its Include line");
}

#[test]
fn sync_and_status_write_the_same_alias_bytes_for_a_host() {
    // Two commands for one host write the same bytes, so neither rewrites the other's alias
    // (operator.md, How the operator reaches a host).
    let Some(s) = Signing::new() else { return };
    assert_eq!(s.run(&["status", "-Host", "agentspace"]).code, Some(0));
    let by_status = fs::read(s.alias()).expect("the alias status wrote");
    s.clear();

    // `sync -Host` writes the alias after its CA check and before its start-and-wait, which this
    // rig fails, so the run reaches nothing past the host's own instance.
    s.rig.fail(
        "ec2-describe-instances",
        "An error occurred (InvalidInstanceID.NotFound).",
        254,
    );
    let out = s.run(&["sync", "-Host", "agentspace"]);
    assert_eq!(out.code, Some(1), "{:?}", out.err());
    let described = s.rig.calls_of("ec2-describe-instances");
    assert!(
        !described.is_empty()
            && described
                .iter()
                .all(|c| c.args.contains(&OTHER_ID.to_string())),
        "the host's own instance is started and waited for"
    );
    assert!(s.rig.calls_of("ssh").is_empty() && s.rig.calls_of("scp").is_empty());
    assert!(
        s.key().is_file() && s.cert().is_file(),
        "sync signed for the host"
    );
    assert_eq!(
        fs::read(s.alias()).expect("the alias sync wrote"),
        by_status,
        "the same bytes"
    );
}

// ---- The operator's key and hosts.lock ----

#[test]
fn a_command_whose_key_exists_does_not_wait_for_a_held_hosts_lock() {
    let Some(s) = Signing::new() else { return };
    assert_eq!(s.run(&["status", "-Host", "agentspace"]).code, Some(0));
    assert!(s.key().is_file());
    // Held for the whole run: a command that took it would wait its 30 seconds and fail with the
    // lock's message, so exit 0 and the report alone show it was never taken. No time is asserted,
    // since a loaded machine makes a run slow.
    let held = agentspace_hosts::lock(&s.rig.profile).expect("hosts.lock");
    let out = s.run(&["status", "-Host", "agentspace"]);
    drop(held);
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert_eq!(out.err(), status_lines(OTHER_ID));
}

#[test]
fn a_command_whose_key_is_missing_waits_for_hosts_lock_before_making_it() {
    let Some(s) = Signing::new() else { return };
    assert!(!s.key().exists());
    let hold = std::time::Duration::from_secs(4);
    let held = agentspace_hosts::lock(&s.rig.profile).expect("hosts.lock");
    let taken = std::time::Instant::now();
    let releaser = std::thread::spawn(move || {
        std::thread::sleep(hold);
        drop(held);
    });
    let out = s.run(&["status", "-Host", "agentspace"]);
    let took = taken.elapsed();
    releaser.join().unwrap();
    assert_eq!(out.code, Some(0), "{:?}", out.err());
    assert!(
        took >= hold,
        "the key is made only once hosts.lock is free: {took:?}"
    );
    assert!(s.key().is_file(), "the key was made");
}
