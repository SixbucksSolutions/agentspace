//! `sync` for a host on a platform, and two hosts of one project on two platforms (docs/operator.md,
//! sync; The platform seam, the sync row; A host's platform and its names; How the operator reaches
//! a host; Every host at once: `-All`; The patch wait; docs/state.md, A host's platform and names,
//! A host's folder, A host's operator alias; D91, D94; E0.12-T03-S03, #0256, pins P12, P17, P40 of
//! v0.12-T01).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Each test runs the crate's hidden
//! `sync_keyed_using` (or `sync_all_using`) in a child of the test binary, under a throwaway home
//! with a throwaway CA, with stand-ins for the AWS CLI, ssh, scp and go.
//!
//! - **Two hosts of one project on two platforms each get their own stack, alias, folder, record
//!   and host-key name**, as operator.md's table gives them, and share none.
//! - **The localswim CLI is built for each host's own `GOOS` and `GOARCH`**, into
//!   `localswim-<os>-<arch>` (`.exe` on Windows), once per platform among the hosts `sync -All`
//!   syncs.
//! - **`sync -All` covers every `ready` host of every platform**, naming each by its host name.
//! - **A Windows host's rows** are operator.md's, Windows hosts, "sync on a Windows host", each
//!   remote command in the encoded form. Those tests (`a_windows_...`) go through
//!   E0.12-T03-S06's rows of the seam and fail with its "not built" answer until it merges.
#![cfg(windows)]

mod common;
mod hostcalls;
mod platformcalls;
mod waitcalls;

use std::fs;

use agentspace_operator_api::Failure;
use common::expect::*;
use common::hosts::*;
use common::*;
use platformcalls::encoded::*;
use platformcalls::*;
use waitcalls::*;

// ---------------------------------------------------------------- Names

/// A path as an alias file holds it: the home's trailing separators dropped, every `\` written `/`
/// and every `%` as `%%` (state.md, The alias file).
fn path_text(path: &str) -> String {
    path.trim_end_matches(['\\', '/'])
        .replace('\\', "/")
        .replace('%', "%%")
}

fn host_file_text(rig: &Rig, host: &str, file: &str) -> String {
    path_text(&format!(
        "{}\\.agentspace\\hosts\\{host}\\{file}",
        rig.home.display()
    ))
}

/// Checks the alias file, the key folder and the certificate the operator made for `platform`'s
/// host: every name is that platform's, from operator.md's table.
fn check_access_of(rig: &Rig, platform: &str) {
    let host = host_name(platform);
    let a = alias(platform);

    // The alias file: `agentspace.d\agentspace-host.<host name>`, state.md's bytes.
    let bytes = fs::read(rig.alias_file(&host)).unwrap_or_else(|_| {
        panic!(
            "The operator's alias file of {platform}'s host MUST be written, named by its host name"
        )
    });
    rig.check(
        !bytes.starts_with(b"\xEF\xBB\xBF") && !bytes.contains(&b'\r') && bytes.ends_with(b"\n"),
        "The alias MUST be UTF-8 with no byte order mark, each line ending in one LF",
    );
    let text = String::from_utf8(bytes).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    rig.check(
        lines[0].starts_with(&format!("# agentspace host {host}; ")),
        &format!("The first line MUST be the owner mark `# agentspace host {host}; <comment>`; it is `{}`", lines[0]),
    );
    let proxy_line = lines[9];
    rig.check(
        proxy_line
            .strip_prefix("  ProxyCommand \"")
            .is_some_and(|r| r.ends_with(&format!("\" {a}"))),
        "The ProxyCommand line MUST name the program in double quotes, then the host's alias",
    );
    let known_hosts = path_text(&format!("{}\\.agentspace\\known_hosts", rig.home.display()));
    let want = vec![
        format!("Host {a}"),
        format!("  User {}", user(platform)),
        format!(
            "  IdentityFile \"{}\"",
            host_file_text(rig, &host, "id_ed25519")
        ),
        format!(
            "  CertificateFile \"{}\"",
            host_file_text(rig, &host, "id_ed25519-cert.pub")
        ),
        "  IdentitiesOnly yes".to_string(),
        format!("  HostKeyAlias {a}"),
        format!("  UserKnownHostsFile \"{known_hosts}\""),
        "  StrictHostKeyChecking accept-new".to_string(),
        proxy_line.to_string(),
        "  ServerAliveInterval 30".to_string(),
        "  ServerAliveCountMax 4".to_string(),
    ];
    let got: Vec<String> = lines[1..].iter().map(|l| l.to_string()).collect();
    rig.same_list(
        &format!("The alias file of {platform}'s host, after its owner mark"),
        &got,
        &want,
    );

    // The folder: the key pair and the certificate, in the host's own folder.
    let mut names: Vec<String> = fs::read_dir(rig.host_folder(&host))
        .unwrap_or_else(|_| panic!("The host's folder hosts\\{host} MUST exist"))
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("id_ed25519"))
        .collect();
    names.sort();
    rig.same_list(
        &format!("The key and certificate in hosts\\{host}"),
        &names,
        &svec(&["id_ed25519", "id_ed25519-cert.pub", "id_ed25519.pub"]),
    );

    // The certificate: key ID `operator-<host name>`, the host's login user alone.
    let info = rig.cert_info(&host);
    rig.same("The key ID", &info.key_id, &format!("operator-{host}"));
    rig.same_list(
        "The principals",
        &info.principals,
        &[user(platform).to_string()],
    );
    rig.check(
        info.cert_type.contains("user certificate"),
        "A user certificate",
    );
}

/// The `calls` of `program` whose arguments name `alias`.
fn naming<'a>(log: &'a Log, program: &str, alias: &str) -> Vec<&'a Rec> {
    log.calls(program)
        .into_iter()
        .filter(|c| {
            c.args
                .iter()
                .any(|a| a == alias || a.starts_with(&format!("{alias}:")))
        })
        .collect()
}

/// The instance each `ec2 describe-instances` named, in order, without repeats.
fn instances_described(log: &Log) -> Vec<String> {
    log.calls("aws")
        .iter()
        .filter(|c| c.args.get(1).map(String::as_str) == Some("describe-instances"))
        .filter_map(|c| c.args.last().cloned())
        .fold(Vec::new(), |mut v, id| {
            if !v.contains(&id) {
                v.push(id);
            }
            v
        })
}

#[test]
fn two_hosts_of_one_project_on_two_platforms_each_get_their_own_names() {
    let rig = plat_rig(
        false,
        &[
            PRec::new(ARM64, "ready").id(ID2),
            PRec::new(AMD64, "ready").id(ID3),
        ],
    );
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(rig.do_sync_keyed(&say, &key(AMD64)), "sync linux/amd64");
        rig.succeeded(rig.do_sync_keyed(&say, &key(ARM64)), "sync linux/arm64");
        let log = rig.log();
        let (a64, a86) = (alias(ARM64), alias(AMD64));
        rig.same(
            "linux/arm64's alias is what it was before v0.12",
            &a64,
            "agentspace-host.demo",
        );
        rig.same(
            "linux/amd64's alias carries its platform",
            &a86,
            "agentspace-host.demo.linux-amd64",
        );

        // Every ssh and scp names one alias, and each alias is reached by its own host's calls.
        let (ssh64, ssh86) = (naming(&log, "ssh", &a64), naming(&log, "ssh", &a86));
        let (scp64, scp86) = (naming(&log, "scp", &a64), naming(&log, "scp", &a86));
        rig.check(
            !ssh64.is_empty()
                && !ssh86.is_empty()
                && ssh64.len() + ssh86.len() == log.calls("ssh").len()
                && scp64.len() + scp86.len() == log.calls("scp").len(),
            "Every ssh and scp MUST name exactly one of the two hosts' aliases",
        );

        // Each host's own instance, from its own record, and no stack is read for either.
        rig.same_list(
            "The instances started, each its own record's, linux/amd64's first",
            &instances_described(&log),
            &svec(&[ID3, ID2]),
        );
        rig.check(
            log.calls("aws")
                .iter()
                .all(|c| c.args.get(1).map(String::as_str) != Some("describe-stacks")),
            "A record that holds an instance ID MUST NOT make sync read any stack",
        );

        // The names and files of each host.
        check_access_of(rig, AMD64);
        check_access_of(rig, ARM64);
        let hosts = rig.home_path(".agentspace/hosts");
        let mut folders: Vec<String> = fs::read_dir(hosts)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        folders.sort();
        rig.same_list(
            "Each host's folder, named by its host name",
            &folders,
            &svec(&["demo", "demo.linux-amd64"]),
        );
        let public =
            |host: &str| fs::read_to_string(rig.host_folder(host).join("id_ed25519.pub")).unwrap();
        rig.check(
            public("demo").split_whitespace().nth(1)
                != public("demo.linux-amd64").split_whitespace().nth(1),
            "No two hosts share a key",
        );
        let mut aliases: Vec<String> = fs::read_dir(rig.ssh_dir().join("agentspace.d"))
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        aliases.sort();
        rig.same_list(
            "Each host's alias file, named by its host-key name",
            &aliases,
            &svec(&["agentspace-host.demo", "agentspace-host.demo.linux-amd64"]),
        );

        // The records: each its own, linux/arm64's with no platform, and the registry unchanged.
        let text = rig.registry_text();
        rig.check(
            text.contains(&PRec::new(ARM64, "ready").id(ID2).json())
                && text.contains(&PRec::new(AMD64, "ready").id(ID3).json()),
            "Each host's record MUST be as it was: sync of a record that holds its ID writes none",
        );
        rig.check(
            !text.contains(&format!(
                "\"project\":\"{PROJECT}\",\"stack\":\"agentspace-host-demo\",\"platform\""
            )),
            "A linux/arm64 record MUST hold no platform",
        );
        rig.check(
            rig.record_on(AMD64).stack == "agentspace-linux-amd64-host-demo",
            "linux/amd64's stack is agentspace-linux-amd64-host-demo",
        );
    });
}

#[test]
fn a_windows_hosts_alias_key_and_certificate_are_its_own_and_name_agent() {
    // The operator writes them between sync's steps 1 and 2, before any program starts, so they
    // are there whatever the Windows rows of the seam do after (E0.12-T03-S06).
    let rig = plat_rig(
        false,
        &[
            PRec::new(AMD64, "ready").id(ID3),
            PRec::new(WINDOWS, "ready").id(ID4),
        ],
    );
    rig.run(|rig| {
        let say = rig.say();
        let _ = rig.do_sync_keyed(&say, &key(WINDOWS));
        check_access_of(rig, WINDOWS);
        let log = rig.log();
        rig.check(
            log.all_calls().iter().all(|c| {
                !c.args
                    .iter()
                    .any(|a| a.contains("agentspace-host.demo.linux-amd64"))
            }),
            "A Windows host's run MUST NOT touch linux/amd64's alias",
        );
        rig.check(
            !rig.alias_file(&host_name(AMD64)).exists(),
            "No alias is written for a host the command did not name",
        );
    });
}

#[test]
fn a_platform_with_no_record_is_refused_naming_the_host_and_the_platform_before_any_program() {
    let rig = plat_rig(false, &[PRec::new(ARM64, "ready").id(ID2)]);
    rig.run(|rig| {
        let say = rig.say();
        for platform in [AMD64, WINDOWS] {
            let f = rig.failure(
                rig.do_sync_keyed(&say, &key(platform)),
                "sync -Host -Platform",
            );
            rig.same(
                "The failure message",
                &f.message,
                &format!(
                    "agentspace: Project '{PROJECT}.{}' has no host; provision -Host {PROJECT} -Platform {platform} makes one.",
                    name_part(platform).unwrap()
                ),
            );
        }
        rig.check(
            rig.log().all_calls().is_empty() && !rig.has_alias_files(),
            "No program MUST start and no alias be written for a platform with no record",
        );
    });
}

// ---------------------------------------------------------------- A Linux platform's rows

#[test]
fn a_linux_amd64_sync_makes_the_calls_of_the_box_in_order_over_its_alias() {
    let rig = plat_rig(false, &[PRec::new(AMD64, "ready").id(ID3)]);
    rig.with_claude_md();
    rig.run(|rig| {
        let fp = fingerprint(rig);
        let say = rig.say();
        rig.succeeded(rig.do_sync_keyed(&say, &key(AMD64)), "sync linux/amd64");
        let log = rig.log();
        rig.same_list(
            "The order of sync's messages and calls",
            &timeline_of(&log, |_| true),
            &expected_timeline(&fp, true, false),
        );
        check_calls_with(rig, &log, true, false, &alias(AMD64), Folder::PerCommand);
        rig.same_list(
            "The first AWS call: the host's own instance",
            &log.calls("aws")[0].args,
            &describe_instances_args(ID3),
        );
        rig.check(
            say.warnings().is_empty(),
            &format!(
                "sync MUST print no warning here; it warned {:?}",
                say.warnings()
            ),
        );
        check_environment(rig, &log);
        let snap = log.dir_snap("scp", 1);
        rig.check(
            matches!(&snap, Some((_, entries)) if *entries == listing(&repo_root().join("box").join("system"))),
            "The staging copy MUST hold exactly the repository's box/system files, byte for byte",
        );
        rig.check(
            rig.command_folders().is_empty(),
            "The command's folder MUST be removed as the command ends",
        );
    });
}

/// A sync with a `go` that builds: the build's environment, arguments, file and messages.
fn builds_for(
    platform: &'static str,
    goos: &'static str,
    goarch: &'static str,
    file: &'static str,
) {
    let rig = plat_rig(true, &[PRec::new(platform, "ready").id(ID3)]);
    rig.with_claude_md();
    rig.run(move |rig| {
        let a = alias(platform);
        let say = rig.say();
        let result = rig.do_sync_keyed(&say, &key(platform));
        if platform != WINDOWS {
            rig.succeeded(result, "sync -Host -Platform");
        }
        let log = rig.log();
        let go = log.calls("go");
        rig.check(go.len() == 1, "The localswim CLI MUST be built once");
        let g = go[0];
        rig.same("GOOS", &g.goos, goos);
        rig.same("GOARCH", &g.goarch, goarch);
        rig.same("CGO_ENABLED", &g.cgo, "0");
        rig.same_path("The build runs in the localswim checkout", &g.cwd, &rig.localswim());
        rig.check(
            g.args.len() == 5
                && g.args[0] == "build"
                && g.args[1] == "-trimpath"
                && g.args[2] == "-o"
                && g.args[4] == "./cmd/localswim",
            &format!("The build MUST be exactly `go build -trimpath -o <out> ./cmd/localswim`; it was {:?}", g.args),
        );
        let out = g.args[3].clone();
        let folder = command_folder_of(&out);
        rig.check(
            folder.is_some()
                && out.replace('/', "\\").ends_with(&format!("\\{file}")),
            &format!("The CLI MUST be built into {file} in the command's own folder: {out}"),
        );
        let progress = say.progress_lines();
        rig.check(
            progress.contains(&format!("Building the localswim CLI for {platform}...")),
            &format!("The message MUST be `Building the localswim CLI for {platform}...`"),
        );
        // The copy names the built file, and the install is the platform's.
        let cli: Vec<&Rec> = log
            .calls("scp")
            .into_iter()
            .filter(|c| c.args.iter().any(|a| a.replace('/', "\\").ends_with(&format!("\\{file}"))))
            .collect();
        rig.check(
            cli.len() == 1,
            &format!("Exactly one scp MUST copy {file}"),
        );
        if platform != WINDOWS {
            rig.same_list(
                "The CLI's copy",
                &cli[0].args[..3].iter().cloned().chain([cli[0].args[4].clone()]).collect::<Vec<_>>(),
                &svec(&["-o", "ClearAllForwardings=yes", "-q", &format!("{a}:/tmp/localswim")]),
            );
            let snap = log.file_snap("scp", cli[0].n);
            rig.check(
                snap.len() == 1 && snap[0].2 == fnv(b"FAKE-LOCALSWIM-BINARY\n"),
                "The copy MUST be the file the build wrote",
            );
        }
    });
}

#[test]
fn a_linux_arm64_host_builds_the_cli_for_arm64_as_before() {
    builds_for(ARM64, "linux", "arm64", "localswim-linux-arm64");
}

#[test]
fn a_linux_amd64_host_builds_the_cli_for_amd64_into_its_own_file() {
    builds_for(AMD64, "linux", "amd64", "localswim-linux-amd64");
}

#[test]
fn a_windows_host_builds_the_cli_for_windows_amd64_into_an_exe() {
    builds_for(WINDOWS, "windows", "amd64", "localswim-windows-amd64.exe");
}

fn skips_when_go_is_missing(platform: &'static str) {
    let rig = plat_rig(false, &[PRec::new(platform, "ready").id(ID3)]);
    rig.run(move |rig| {
        let say = rig.say();
        rig.succeeded(
            rig.do_sync_keyed(&say, &key(platform)),
            "sync -Host -Platform",
        );
        rig.check(
            say.progress_lines().iter().any(|l| l == SKIP_LINE),
            "The skip message MUST print when go is missing, whatever the platform",
        );
        rig.check(rig.log().calls("go").is_empty(), "No build");
    });
}

#[test]
fn the_skip_message_prints_for_a_linux_arm64_host_when_go_is_missing() {
    skips_when_go_is_missing(ARM64);
}

#[test]
fn the_skip_message_prints_for_a_linux_amd64_host_when_go_is_missing() {
    skips_when_go_is_missing(AMD64);
}

// ---------------------------------------------------------------- The patch wait, by platform

#[test]
fn a_linux_amd64_hosts_patch_wait_is_the_shell_documents_with_the_shorthand() {
    let rig = plat_rig(false, &[PRec::new(AMD64, "ready").id(ID3)]);
    rig.with_claude_md();
    let checks = [Check::Answer("pending"), Check::Answer("patched")];
    rig.script_checks(ID3, &checks);
    rig.run(|rig| {
        let say = rig.say();
        let clock = FakeClock::new();
        rig.succeeded(
            rig.do_sync_keyed_clocked(&say, &key(AMD64), &clock),
            "sync linux/amd64",
        );
        let log = rig.log();
        let want = with_wait(
            expected_timeline(&fingerprint(rig), true, false),
            Want::new()
                .line(checking(ID3))
                .check(&checks[0])
                .line(PATCHING)
                .check(&checks[1])
                .line(patched(ID3)),
        );
        rig.same_list("The wait", &wait_timeline(&log), &want);
        rig.check_ssm_args(&log, ID3);
    });
}

#[test]
fn a_linux_amd64_failed_patch_run_names_the_hosts_alias_in_its_warning() {
    let rig = plat_rig(false, &[PRec::new(AMD64, "ready").id(ID3)]);
    let checks = [Check::Answer("failed")];
    rig.script_checks(ID3, &checks);
    rig.run(|rig| {
        let a = alias(AMD64);
        let say = rig.say();
        let clock = FakeClock::new();
        rig.succeeded(
            rig.do_sync_keyed_clocked(&say, &key(AMD64), &clock),
            "A failed patch run MUST NOT fail sync",
        );
        rig.same_list(
            "The warning: `agentspace` in the command is the host's alias",
            &say.warnings(),
            &[failed_warning_text(ID3, &a)],
        );
        rig.same(
            "The warning's alias",
            &a,
            "agentspace-host.demo.linux-amd64",
        );
    });
}

#[test]
fn a_linux_amd64_patch_run_that_never_finishes_fails_at_30_minutes_naming_the_alias() {
    let rig = plat_rig(false, &[PRec::new(AMD64, "ready").id(ID3)]);
    rig.script_checks(ID3, &[Check::Answer("pending")]);
    rig.run(|rig| {
        let say = rig.say();
        let clock = FakeClock::new();
        let f: Failure = rig.failure(
            rig.do_sync_keyed_clocked(&say, &key(AMD64), &clock),
            "sync linux/amd64",
        );
        rig.same(
            "The failure message",
            &f.message,
            &deadline_message(ID3, &alias(AMD64)),
        );
    });
}

// ---------------------------------------------------------------- sync -All

/// The five records of the Linux platforms: the box, the project's `linux/arm64` host, its
/// `linux/amd64` host, its Windows host not yet `ready`.
fn all_rig(go: bool, windows: &'static str) -> Rig {
    plat_rig(
        go,
        &[
            PRec::new(ARM64, "ready").id(ID2),
            PRec::new(AMD64, "ready").id(ID3),
            PRec::new(WINDOWS, windows),
        ],
    )
}

fn host_lines_in(stderr: &str) -> Vec<String> {
    stderr
        .lines()
        .filter(|l| l.starts_with("Host      "))
        .map(str::to_string)
        .collect()
}

#[test]
fn sync_all_covers_every_ready_host_of_every_linux_platform_naming_each_by_its_host_name() {
    let rig = all_rig(true, "creating");
    let Some(out) = rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(rig.do_sync_all(&say), "sync -All");
        let log = rig.log();
        let reached: Vec<String> = log
            .calls("ssh")
            .iter()
            .filter_map(|c| {
                c.args
                    .iter()
                    .find(|a| *a == "agentspace" || a.starts_with("agentspace-host."))
                    .cloned()
            })
            .fold(Vec::new(), |mut v, a| {
                if !v.contains(&a) {
                    v.push(a);
                }
                v
            });
        rig.same_list(
            "The hosts reached, in the file's order, each over its own alias",
            &reached,
            &svec(&[
                "agentspace",
                "agentspace-host.demo",
                "agentspace-host.demo.linux-amd64",
            ]),
        );
        rig.same_list(
            "The instances started, each its record's",
            &instances_described(&log),
            &svec(&[ID, ID2, ID3]),
        );
        rig.same_list(
            "The skip line of the host that is not ready",
            &say.progress_lines()
                .into_iter()
                .filter(|l| l.starts_with("Skipping: the host is"))
                .collect::<Vec<_>>(),
            &["Skipping: the host is creating; its next provisioning run syncs it.".to_string()],
        );
        rig.check(
            !log.all_calls().iter().any(|c| {
                c.args
                    .iter()
                    .any(|a| a.contains("agentspace-host.demo.windows-amd64"))
            }),
            "A host that is not ready MUST NOT be reached",
        );

        // The CLI is built once per platform among the hosts it syncs, in the part of the first
        // host of each platform, and every later host of that platform copies that build.
        let go = log.calls("go");
        let built: Vec<(String, String)> = go
            .iter()
            .map(|g| (g.goos.clone(), g.goarch.clone()))
            .collect();
        rig.check(
            built
                == [
                    ("linux".to_string(), "arm64".to_string()),
                    ("linux".to_string(), "amd64".to_string()),
                ],
            &format!("The CLI MUST be built once per platform, linux/arm64 for the first part and linux/amd64 for the next: {built:?}"),
        );
        let progress = say.progress_lines();
        let count = |line: &str| progress.iter().filter(|l| *l == line).count();
        rig.check(
            count("Building the localswim CLI for linux/arm64...") == 1
                && count("Building the localswim CLI for linux/amd64...") == 1
                && count("Copying the localswim CLI to the instance...") == 3,
            "Each build's message MUST print once, in the part of the first host of its platform, and each part copies the CLI",
        );
        let clis: Vec<String> = log
            .calls("scp")
            .iter()
            .filter(|c| c.args.last().is_some_and(|a| a.ends_with(":/tmp/localswim")))
            .filter_map(|c| c.args.get(3).cloned())
            .collect();
        rig.check(
            clis.len() == 3
                && clis[0] == clis[1]
                && clis[0] != clis[2]
                && clis[0].ends_with("localswim-linux-arm64")
                && clis[2].ends_with("localswim-linux-amd64"),
            &format!("The two linux/arm64 hosts MUST copy one build and linux/amd64's another: {clis:?}"),
        );
        let systems: Vec<String> = log
            .calls("scp")
            .iter()
            .filter(|c| c.args.iter().any(|a| a == "-r"))
            .filter_map(|c| c.args.get(4).cloned())
            .collect();
        rig.check(
            systems.len() == 3 && systems.iter().all(|s| *s == systems[0]),
            "box/system MUST be written once, and every part copies the same folder",
        );
        rig.check(
            rig.command_folders().is_empty(),
            "The command's one folder MUST be removed as it ends",
        );
    }) else {
        return;
    };
    assert_eq!(
        host_lines_in(&out.stderr),
        vec![
            "Host      swimcoach (agentbox), ready".to_string(),
            "Host      demo (agentspace-host-demo), ready".to_string(),
            "Host      demo.linux-amd64 (agentspace-linux-amd64-host-demo), ready".to_string(),
            "Host      demo.windows-amd64 (agentspace-windows-amd64-host-demo), creating"
                .to_string(),
        ],
        "A Host line MUST be printed before each record, in the file's order, each host by its host name and its stack"
    );
}

#[test]
fn a_windows_host_that_is_ready_is_synced_by_sync_all_with_its_own_build() {
    // Depends on E0.12-T03-S06's rows. The Windows host is the third `ready` host: the CLI is
    // built a third time, for windows/amd64, as localswim-windows-amd64.exe.
    let rig = all_rig(true, "ready");
    let rig_id = ID4;
    rig.write_home(
        ".agentspace/hosts.json",
        format!(
            "{{\"version\":0,\"hosts\":[{BOX_RECORD},{},{},{}]}}\n",
            PRec::new(ARM64, "ready").id(ID2).json(),
            PRec::new(AMD64, "ready").id(ID3).json(),
            PRec::new(WINDOWS, "ready").id(rig_id).json()
        )
        .as_bytes(),
    );
    let Some(out) = rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(rig.do_sync_all(&say), "sync -All");
        let log = rig.log();
        let built: Vec<(String, String)> = log
            .calls("go")
            .iter()
            .map(|g| (g.goos.clone(), g.goarch.clone()))
            .collect();
        rig.check(
            built.len() == 3 && built[2] == ("windows".to_string(), "amd64".to_string()),
            &format!("The CLI MUST be built for each platform present: {built:?}"),
        );
        rig.check(
            say.progress_lines()
                .contains(&"Building the localswim CLI for windows/amd64...".to_string()),
            "The Windows build's message MUST print, in the Windows host's part",
        );
    }) else {
        return;
    };
    assert!(host_lines_in(&out.stderr).contains(
        &"Host      demo.windows-amd64 (agentspace-windows-amd64-host-demo), ready".to_string()
    ));
}

// ---------------------------------------------------------------- A Windows host's rows

#[test]
fn a_windows_sync_makes_operator_mds_rows_in_order_each_remote_command_encoded() {
    // Depends on E0.12-T03-S06's rows (sync_plan and the encoded remote form).
    let rig = plat_rig(true, &[PRec::new(WINDOWS, "ready").id(ID4)]);
    rig.with_claude_md();
    rig.run(|rig| {
        let fp = fingerprint(rig);
        let a = alias(WINDOWS);
        let say = rig.say();
        rig.succeeded(rig.do_sync_keyed(&say, &key(WINDOWS)), "sync windows/amd64");
        let log = rig.log();

        // The order: the same nine rows, with the same messages, but for the build's platform.
        let want: Vec<String> = expected_timeline(&fp, true, true)
            .into_iter()
            .map(|l| l.replace("for linux/arm64", "for windows/amd64"))
            .collect();
        rig.same_list(
            "The order of sync's messages and calls",
            &timeline_of(&log, |_| true),
            &want,
        );

        let ssh = log.calls("ssh");
        let scp = log.calls("scp");
        rig.check(
            ssh.len() == 4 && scp.len() == 4,
            &format!("Four ssh and four scp (the push is off); it was {} and {}", ssh.len(), scp.len()),
        );
        let operator = rig.home.join(".agentspace").join("operator");
        let ca_pub = rig.home.join(".agentspace").join("ca").join("agentspace_ca.pub");
        let flags = svec(&["-o", "ClearAllForwardings=yes"]);

        // Every ssh: `-o ClearAllForwardings=yes <alias> <encoded remote command>`.
        let script = |n: usize| -> String {
            let args = &ssh[n].args;
            rig.same_list(
                &format!("ssh #{}'s arguments before its remote command", n + 1),
                &args[..3],
                &[flags[0].clone(), flags[1].clone(), a.clone()],
            );
            rig.check(args.len() == 4, "The remote command is one argument, the last");
            decode_encoded(&args[3])
        };
        // Row 1: the staging folder removed.
        let s1 = script(0);
        rig.check(s1.contains(".agentspace-system"), "Row 1 MUST remove .agentspace-system in the login user's home");
        // Row 2: box/windows, whole and recursively, to the staging folder by a relative path.
        let c1 = scp[0];
        rig.same_list(
            "Row 2's arguments but its source",
            &[c1.args[..4].to_vec(), vec![c1.args[5].clone()]].concat(),
            &svec(&["-o", "ClearAllForwardings=yes", "-q", "-r", &format!("{a}:.agentspace-system")]),
        );
        rig.same_path(
            "Row 2's source: the windows folder of the command's own",
            &resolve(rig, &c1.args[4], Folder::PerCommand, "Row 2's source"),
            &operator.join("windows"),
        );
        rig.check(
            matches!(log.dir_snap("scp", 1), Some((_, entries)) if entries == listing(&repo_root().join("box").join("windows"))),
            "The staging copy MUST hold exactly the repository's box/windows files, byte for byte",
        );
        // Row 3: the CA's public key.
        rig.same_list(
            "Row 3's arguments but its source",
            &[scp[1].args[..3].to_vec(), vec![scp[1].args[4].clone()]].concat(),
            &svec(&[
                "-o",
                "ClearAllForwardings=yes",
                "-q",
                &format!("{a}:.agentspace-system/agentspace_ca.pub"),
            ]),
        );
        rig.same_path("Row 3's source", &scp[1].args[3], &ca_pub);
        // Row 4: the install, under Windows PowerShell 5.1 with -File, then the staging removed.
        let s4 = script(1);
        rig.check(
            s4.contains("install.ps1") && s4.contains(".agentspace-system") && s4.contains("-File"),
            "Row 4 MUST run the staged install.ps1 with -File, then remove the staging folder",
        );
        // Row 5: the folder CLAUDE.md goes into.
        let s5 = script(2);
        rig.check(s5.contains(".claude"), "Row 5 MUST make <home>\\.claude");
        // Row 6: CLAUDE.md, relative to the home.
        rig.same_list(
            "Row 6",
            &[scp[2].args[..3].to_vec(), vec![scp[2].args[4].clone()]].concat(),
            &svec(&["-o", "ClearAllForwardings=yes", "-q", &format!("{a}:.claude/CLAUDE.md")]),
        );
        rig.same_path(
            "Row 6's source",
            &scp[2].args[3],
            &rig.home.join(".claude").join("CLAUDE.md"),
        );
        // Row 7: the build, to a staged name in the home.
        rig.same_list(
            "Row 7",
            &[scp[3].args[..3].to_vec(), vec![scp[3].args[4].clone()]].concat(),
            &svec(&["-o", "ClearAllForwardings=yes", "-q", &format!("{a}:.agentspace-localswim.exe")]),
        );
        rig.same_path(
            "Row 7's source",
            &resolve(rig, &scp[3].args[3], Folder::PerCommand, "Row 7's source"),
            &operator.join("localswim-windows-amd64.exe"),
        );
        // Row 8: the install at C:\ProgramData\agentspace\localswim\localswim.exe.
        let s8 = script(3);
        rig.check(
            s8.contains(".agentspace-localswim.exe")
                && s8.contains(r"C:\ProgramData\agentspace\localswim\localswim.exe"),
            "Row 8 MUST install the staged copy at C:\\ProgramData\\agentspace\\localswim\\localswim.exe",
        );
        rig.check(
            rig.command_folders().is_empty(),
            "The command's folder MUST be removed as the command ends",
        );
        check_environment(rig, &log);
    });
}

#[test]
fn a_windows_sync_writes_box_windows_beside_nothing_of_the_linux_system_folder() {
    let rig = plat_rig(false, &[PRec::new(WINDOWS, "ready").id(ID4)]);
    rig.run(|rig| {
        let say = rig.say();
        let _ = rig.do_sync_keyed(&say, &key(WINDOWS));
        let log = rig.log();
        let copied: Vec<String> = log
            .calls("scp")
            .iter()
            .filter(|c| c.args.iter().any(|a| a == "-r"))
            .filter_map(|c| c.args.get(4).cloned())
            .collect();
        rig.check(
            copied.len() == 1 && copied[0].replace('/', "\\").ends_with("\\windows"),
            &format!("A Windows sync MUST copy the windows folder alone: {copied:?}"),
        );
    });
}

#[test]
fn a_windows_hosts_patch_wait_goes_through_run_powershell_script_with_a_json_parameters_value() {
    let rig = plat_rig(false, &[PRec::new(WINDOWS, "ready").id(ID4)]);
    rig.with_claude_md();
    rig.script_checks(ID4, &[Check::Answer("pending"), Check::Answer("patched")]);
    rig.run(|rig| {
        let say = rig.say();
        let clock = FakeClock::new();
        rig.succeeded(
            rig.do_sync_keyed_clocked(&say, &key(WINDOWS), &clock),
            "sync windows/amd64",
        );
        let log = rig.log();
        let sends = ssm_calls(&log, "send-command");
        rig.check(sends.len() == 2, "Two checks, two questions");
        for r in sends {
            let before = vec![
                "ssm",
                "send-command",
                "--region",
                "us-west-2",
                "--output",
                "json",
                "--instance-ids",
                ID4,
                "--document-name",
                "AWS-RunPowerShellScript",
                "--comment",
                COMMENT,
                "--parameters",
            ];
            rig.same_list(
                "The send call but its parameters",
                &r.args[..r.args.len() - 1],
                &svec(&before),
            );
            let parameters = r.args.last().unwrap();
            rig.check(
                !parameters.starts_with("commands="),
                "The shorthand MUST NOT be used on a Windows host",
            );
            let value: serde_json::Value =
                serde_json::from_str(parameters).expect("--parameters is a JSON value");
            let commands = value["commands"].as_array().map(Vec::len);
            rig.check(
                value.as_object().is_some_and(|o| o.len() == 1) && commands == Some(1),
                &format!("The value MUST be {{\"commands\":[\"<command>\"]}}: {parameters}"),
            );
            let command = value["commands"][0].as_str().unwrap_or("");
            rig.check(
                command.contains(r"C:\Program Files\PowerShell\7\pwsh.exe")
                    && command.contains("agentspace-patch-check.ps1"),
                "The question MUST run the patch check under PowerShell 7 by its full path",
            );
        }
        let lines = say.progress_lines();
        rig.check(
            lines.contains(
                &"Patching Windows (Windows Update, restart if an update needs one)...".to_string(),
            ) && !lines.iter().any(|l| l.starts_with("Patching Ubuntu")),
            "The phase message MUST be the Windows one",
        );
    });
}

#[test]
fn a_windows_failed_patch_run_names_the_patch_log_not_journalctl() {
    let rig = plat_rig(false, &[PRec::new(WINDOWS, "ready").id(ID4)]);
    rig.script_checks(ID4, &[Check::Answer("failed")]);
    rig.run(|rig| {
        let a = alias(WINDOWS);
        let say = rig.say();
        let clock = FakeClock::new();
        rig.succeeded(
            rig.do_sync_keyed_clocked(&say, &key(WINDOWS), &clock),
            "A failed patch run MUST NOT fail sync",
        );
        rig.same_list(
            "The warning",
            &say.warnings(),
            &[format!(
                "agentspace: The patch run on {ID4} failed; sync goes on without it. Check: ssh {a} cat C:/ProgramData/agentspace/patch.log"
            )],
        );
    });
}

#[test]
fn a_windows_patch_run_that_never_finishes_names_the_patch_log_in_the_deadline() {
    let rig = plat_rig(false, &[PRec::new(WINDOWS, "ready").id(ID4)]);
    rig.script_checks(ID4, &[Check::Answer("pending")]);
    rig.run(|rig| {
        let say = rig.say();
        let clock = FakeClock::new();
        let f = rig.failure(
            rig.do_sync_keyed_clocked(&say, &key(WINDOWS), &clock),
            "sync windows/amd64",
        );
        rig.same(
            "The deadline's failure",
            &f.message,
            &format!(
                "agentspace: {ID4} did not finish its patch run within 30 minutes; check: ssh {} cat C:/ProgramData/agentspace/patch.log",
                alias(WINDOWS)
            ),
        );
    });
}
