//! How `sync -Host` reaches a host: the operator's key, its one-hour certificate, and its alias
//! (docs/operator.md, How the operator reaches a host; docs/state.md, A host's folder, The
//! operator's key and certificate, A host's operator alias, The `Include` line).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Each test runs the crate's hidden
//! `sync_target_using` for `Target::Host` in a child of the test binary, under a throwaway home with
//! a throwaway CA. The certificate is read with the real `ssh-keygen -L` (operator.md, The test
//! seam, "Not covered"), which reads one file of the throwaway home and reaches no host. What goes
//! wrong is in `host_access_failures`.
#![cfg(windows)]

mod common;
mod hostcalls;

use std::fs;

use agentspace_sync::Target;
use common::expect::*;
use common::hosts::*;
use common::*;
use hostcalls::setup::*;

/// A path as an alias file holds it: the home's trailing separators dropped, every `\` written `/`
/// and every `%` as `%%` (state.md, The alias file).
fn path_text(path: &str) -> String {
    path.trim_end_matches(['\\', '/'])
        .replace('\\', "/")
        .replace('%', "%%")
}

fn host_file_text(rig: &Rig, file: &str) -> String {
    path_text(&format!(
        "{}\\.agentspace\\hosts\\{PROJECT}\\{file}",
        rig.home.display()
    ))
}

#[test]
fn sync_makes_the_operators_key_certificate_and_alias_before_it_connects() {
    let rig = sync_rig(false);
    rig.watch_file(rig.alias_file(PROJECT));
    rig.watch_file(agentspace_hosts::host_cert_path(&rig.home, PROJECT));
    rig.probe_lock(agentspace_hosts::hosts_lock_path(&rig.home));
    rig.run(|rig| {
        let fp = fingerprint(rig);
        let registry_before = rig.hosts_bytes();
        let say = rig.say();
        rig.succeeded(
            rig.do_sync_target(&say, Target::Host(PROJECT)),
            "sync -Host",
        );
        let log = rig.log();
        let alias = alias_of(PROJECT);

        // Between steps 1 and 2: before the box is started, so before the first call of any kind.
        let first = log.all_calls()[0];
        rig.check(
            log.watched(&first.name, first.n, &alias)
                .is_some_and(|c| c.starts_with(&format!("# agentspace host {PROJECT}; ")))
                && log
                    .watched(&first.name, first.n, "id_ed25519-cert.pub")
                    .is_some_and(|c| c != "<missing>"),
            "The alias and the certificate MUST be written before the command's first program starts",
        );

        // The folder holds the key pair and the certificate, and nothing else.
        let mut names: Vec<String> = fs::read_dir(rig.host_folder(PROJECT))
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        rig.same_list(
            "The host's folder",
            &names,
            &svec(&["id_ed25519", "id_ed25519-cert.pub", "id_ed25519.pub"]),
        );

        // The certificate: a user certificate for that key, key ID, one principal, one hour.
        let info = rig.cert_info(PROJECT);
        rig.check(
            info.cert_type.contains("user certificate"),
            "The operator's certificate MUST be a user certificate",
        );
        rig.same("The key ID", &info.key_id, &format!("operator-{PROJECT}"));
        rig.same_list("The principals", &info.principals, &svec(&["ubuntu"]));
        rig.check(
            info.signing_ca.contains(&fp),
            "The certificate MUST be signed by the agentspace CA",
        );
        let window = info.valid_to - info.valid_from;
        rig.check(
            (3600..=3900).contains(&window),
            &format!("The certificate MUST be valid for one hour from the issue time, back-dated by at most 5 minutes; its window is {window} seconds"),
        );
        for ext in ["permit-port-forwarding", "permit-pty"] {
            rig.check(
                info.extensions.iter().any(|e| e == ext),
                &format!("The certificate MUST carry the extension {ext}"),
            );
        }

        // The Include line, as the first line of a config that was not there.
        let config = fs::read_to_string(rig.ssh_dir().join("config")).unwrap();
        rig.check(
            config.lines().next() == Some("Include agentspace.d/*"),
            "The Include line MUST be the first line of the new ssh config",
        );

        // Nothing of the key reaches a program's arguments, and the registry is not written.
        rig.check(
            !log.all_calls().iter().any(|c| {
                c.args
                    .iter()
                    .any(|a| a.contains("id_ed25519") || a.contains("PRIVATE KEY"))
            }),
            "No program MUST be given the operator's key or its path: the alias holds it",
        );
        rig.check(
            rig.hosts_bytes() == registry_before,
            "sync of a host whose record holds its instance ID MUST NOT write the registry",
        );
        let probes = log.probed("hosts.lock");
        rig.check(
            !probes.is_empty() && probes.iter().all(|p| !p.2),
            "The operator MUST NOT hold hosts.lock while a program runs",
        );
    });
}

#[test]
fn the_alias_file_is_the_documented_bytes() {
    let rig = sync_rig(false);
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(
            rig.do_sync_target(&say, Target::Host(PROJECT)),
            "sync -Host",
        );
        let bytes = fs::read(rig.alias_file(PROJECT)).unwrap();
        rig.check(
            !bytes.starts_with(b"\xEF\xBB\xBF")
                && !bytes.contains(&b'\r')
                && bytes.ends_with(b"\n"),
            "The alias MUST be UTF-8 with no byte order mark, each line ending in one LF",
        );
        let text = String::from_utf8(bytes).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        let alias = alias_of(PROJECT);
        rig.check(
            lines[0].starts_with(&format!("# agentspace host {PROJECT}; ")),
            "The first line MUST be the owner mark `# agentspace host <project>; <comment>`",
        );
        let proxy_line = lines[9];
        let proxy = proxy_line
            .strip_prefix("  ProxyCommand \"")
            .and_then(|r| r.strip_suffix(&format!("\" {alias}")));
        rig.check(
            proxy.is_some(),
            "The ProxyCommand line MUST name the program in double quotes, then the alias",
        );
        rig.same_path(
            "The ProxyCommand program",
            proxy.unwrap_or(""),
            &rig.operator_bin().join("agentspace-ssh-proxy.exe"),
        );
        let known_hosts = path_text(&format!("{}\\.agentspace\\known_hosts", rig.home.display()));
        let want = vec![
            format!("Host {alias}"),
            "  User ubuntu".to_string(),
            format!("  IdentityFile \"{}\"", host_file_text(rig, "id_ed25519")),
            format!(
                "  CertificateFile \"{}\"",
                host_file_text(rig, "id_ed25519-cert.pub")
            ),
            "  IdentitiesOnly yes".to_string(),
            format!("  HostKeyAlias {alias}"),
            format!("  UserKnownHostsFile \"{known_hosts}\""),
            "  StrictHostKeyChecking accept-new".to_string(),
            proxy_line.to_string(),
            "  ServerAliveInterval 30".to_string(),
            "  ServerAliveCountMax 4".to_string(),
        ];
        let got: Vec<String> = lines[1..].iter().map(|l| l.to_string()).collect();
        rig.same_list("The alias file, after its owner mark", &got, &want);
        rig.check(
            !rig.home_path(".agentspace/known_hosts").exists()
                && !rig.ssh_dir().join("known_hosts").exists(),
            "The operator MUST NOT write a known_hosts file: only ssh does",
        );
    });
}

#[test]
fn an_existing_key_is_kept_and_the_alias_comes_out_the_same_bytes() {
    let rig = sync_rig(false);
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(
            rig.do_sync_target(&say, Target::Host(PROJECT)),
            "The first sync",
        );
        let folder = rig.host_folder(PROJECT);
        let key = fs::read(folder.join("id_ed25519")).unwrap();
        let public = fs::read(folder.join("id_ed25519.pub")).unwrap();
        let alias = fs::read(rig.alias_file(PROJECT)).unwrap();
        rig.succeeded(
            rig.do_sync_target(&say, Target::Host(PROJECT)),
            "The second sync",
        );
        rig.check(
            fs::read(folder.join("id_ed25519")).unwrap() == key
                && fs::read(folder.join("id_ed25519.pub")).unwrap() == public,
            "An existing key MUST never be replaced",
        );
        rig.check(
            fs::read(rig.alias_file(PROJECT)).unwrap() == alias,
            "The alias MUST come out the same bytes at the second command",
        );
        let info = rig.cert_info(PROJECT);
        rig.same("The key ID", &info.key_id, &format!("operator-{PROJECT}"));
    });
}

#[test]
fn a_stale_certificate_with_no_key_is_replaced_by_a_new_one() {
    let rig = sync_rig(false);
    rig.write_home(
        ".agentspace/hosts/demo/id_ed25519-cert.pub",
        b"a stale certificate\n",
    );
    rig.write_home(
        ".agentspace/hosts/demo/id_ed25519.pub",
        b"a stale public key\n",
    );
    rig.run(|rig| {
        let say = rig.say();
        rig.succeeded(
            rig.do_sync_target(&say, Target::Host(PROJECT)),
            "sync -Host",
        );
        let info = rig.cert_info(PROJECT);
        rig.same("The key ID", &info.key_id, &format!("operator-{PROJECT}"));
        let leftovers = fs::read_dir(rig.host_folder(PROJECT))
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("sign-"))
            .count();
        rig.check(
            leftovers == 0,
            "The command MUST delete the sign-<hex> files it made, whether or not anything failed",
        );
    });
}
