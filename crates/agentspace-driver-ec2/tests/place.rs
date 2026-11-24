//! Place through the `Driver` trait (docs/driver.md, Place a lease; state.md, Creation in order).
#![cfg(windows)]

mod common;

use agentspace_driver_api::{Driver, Placed, Reason};
use common::*;
use std::fs;

#[test]
fn the_driver_is_named_ec2() {
    assert_eq!(Fx::without_ca().driver().name(), "ec2");
}

#[test]
fn a_ready_place_leaves_the_credential_and_the_alias_and_ssh_reads_it_back() {
    let fx = Fx::with_ca();
    let before = now();
    let expires = fx.place_ready(3600);
    let after = now();
    assert!(expires >= before + 3600 && expires <= after + 3600);

    assert_eq!(
        Fx::names(&fx.lease_dir(ID)),
        ["id_ed25519", "id_ed25519-cert.pub", "id_ed25519.pub"]
    );
    assert!(fx.alias_file().is_file());
    assert_eq!(
        fs::read(fx.config()).unwrap(),
        b"Include agentspace.d/*\n",
        "The config holds the Include line"
    );
    assert_eq!(Fx::names(&fx.alias_dir()), [ALIAS]);

    // The certificate: the lease's key, the CA's signature, the user, the window.
    let info = cert_info(&fx.cert());
    assert_eq!(info.key_id, ID);
    assert_eq!(info.principals, ["ubuntu"]);
    assert_eq!(info.valid_to, expires);
    assert!(info.valid_from <= before);
    assert!(info.critical.is_empty(), "{:?}", info.critical);
    assert_eq!(info.public_fp, fingerprint(&fx.pubkey()));
    assert_eq!(
        info.ca_fp,
        fingerprint(&fx.ca_dir().join("agentspace_ca.pub"))
    );
    assert!(info.cert_type.to_lowercase().contains("user"));

    // ssh -G reads the alias back.
    let out = fx.ssh_g(ALIAS);
    assert_eq!(values(&out, "user"), ["ubuntu"], "{out}");
    let home_s = slashed(&fx.home);
    assert!(
        values(&out, "identityfile")
            .contains(&format!("{home_s}/.agentspace/leases/{ID}/id_ed25519")),
        "{out}"
    );
    assert_eq!(
        values(&out, "certificatefile"),
        [format!(
            "{home_s}/.agentspace/leases/{ID}/id_ed25519-cert.pub"
        )],
        "{out}"
    );
    let program = fx.bin.join("agentspace-ssh-proxy.exe");
    assert_eq!(
        values(&out, "proxycommand"),
        [format!("\"{}\" {ALIAS}", program.display())],
        "{out}"
    );
    assert_eq!(values(&out, "hostkeyalias"), ["agentspace"], "{out}");
}

#[test]
fn an_override_replaces_the_program_and_needs_none() {
    let mut fx = Fx::with_ca_proxy(None);
    fx.ctx.proxy_override = Some("my-transport --stdio".to_string());
    fx.place_ready(600);
    let out = fx.ssh_g(ALIAS);
    assert_eq!(
        values(&out, "proxycommand"),
        ["my-transport --stdio"],
        "{out}"
    );
}

#[test]
fn an_override_with_a_line_break_is_refused_and_leaves_no_lease() {
    let mut fx = Fx::with_ca();
    fx.ctx.proxy_override = Some("a\nProxyJump evil".to_string());
    let f = fx.place(600).unwrap_err();
    fx.assert_failure_plain(&f);
    assert!(f.leftovers.is_empty(), "{:?}", f.leftovers);
    assert!(!fx.lease_dir(ID).exists());
    assert!(!fx.alias_file().exists());
}

#[test]
fn a_missing_default_program_fails_in_plain_words_and_leaves_no_lease() {
    let fx = Fx::with_ca_proxy(None);
    let f = fx.place(600).unwrap_err();
    fx.assert_failure_plain(&f);
    assert!(f.message.contains("way to reach its host cannot be found"));
    assert!(f.leftovers.is_empty());
    assert!(!fx.lease_dir(ID).exists());
    assert!(!fx.alias_file().exists());
}

#[test]
fn a_cmd_program_is_run_through_cmd_exe_with_the_system_folder() {
    let fx = Fx::with_ca_proxy(Some("cmd"));
    fx.place_ready(600);
    let out = fx.ssh_g(ALIAS);
    let program = fx.bin.join("agentspace-ssh-proxy.cmd");
    assert_eq!(
        values(&out, "proxycommand"),
        [format!(
            "{}\\System32\\cmd.exe /d /c \"{}\" {ALIAS}",
            system_root().trim_end_matches(['\\', '/']),
            program.display()
        )],
        "{out}"
    );
}

#[test]
fn a_cmd_program_with_no_system_folder_fails_and_leaves_no_lease() {
    let mut fx = Fx::with_ca_proxy(Some("cmd"));
    fx.ctx.system_root = None;
    let f = fx.place(600).unwrap_err();
    fx.assert_failure_plain(&f);
    assert!(
        f.message.contains("system folder is not known"),
        "{}",
        f.message
    );
    assert!(f.leftovers.is_empty());
    assert!(!fx.lease_dir(ID).exists());
    assert!(!fx.alias_file().exists());
}

#[test]
fn an_exe_program_needs_no_system_folder() {
    let mut fx = Fx::with_ca();
    fx.ctx.system_root = None;
    fx.place_ready(600);
}

#[test]
fn the_alias_may_live_outside_the_home() {
    let fx = Fx::with_ca_ssh_elsewhere();
    fx.place_ready(600);
    assert!(fx.alias_file().is_file());
    assert_eq!(values(&fx.ssh_g(ALIAS), "user"), ["ubuntu"]);
}

fn reasons_for(platform: &str, profile: &str, size: &str) -> Vec<(String, String)> {
    let mut v = Vec::new();
    if platform != "linux/arm64" {
        v.push((
            "unsupported_platform".to_string(),
            format!("Platform '{platform}' is not supported; v0 supports linux/arm64 only."),
        ));
    }
    if profile != "agent-linux" {
        v.push((
            "unsupported_profile".to_string(),
            format!(
                "Profile '{profile}' is not supported; v0 supports agent-linux with no version only."
            ),
        ));
    }
    if !["s", "m", "l", "xl"].contains(&size) {
        v.push((
            "unsupported_size".to_string(),
            format!("Size '{size}' is not supported; v0 supports sizes s, m, l and xl."),
        ));
    }
    v
}

fn run_combinations(fx: &Fx) {
    let before = fx.snapshot();
    let mut checked = 0;
    for platform in ["linux/arm64", "windows/amd64", "linux/amd64"] {
        for profile in ["agent-linux", "agent-linux@1", "agent-linux@12", "other"] {
            for size in ["s", "m", "l", "xl", "xxl"] {
                let want = reasons_for(platform, profile, size);
                if want.is_empty() {
                    continue;
                }
                let got = fx
                    .driver()
                    .place(&fx.ctx, &fx.request(platform, profile, size, 600))
                    .unwrap_or_else(|f| panic!("{platform} {profile} {size}: {f:?}"));
                let Placed::Unsatisfiable { reasons } = got else {
                    panic!("{platform} {profile} {size}: not unsatisfiable: {got:?}");
                };
                let got: Vec<(String, String)> = reasons
                    .iter()
                    .map(|Reason { code, message }| (code.to_string(), message.clone()))
                    .collect();
                assert_eq!(got, want, "{platform} {profile} {size}");
                for r in &reasons {
                    fx.assert_plain(&r.message);
                }
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 3 * 4 * 5 - 4);
    assert_eq!(fx.snapshot(), before, "An unsupported place left a file");
}

#[test]
fn every_unsupported_combination_gives_its_reasons_in_order_and_leaves_nothing() {
    run_combinations(&Fx::with_ca());
}

#[test]
fn unsupported_values_are_answered_without_a_ca_too() {
    let fx = Fx::without_ca();
    run_combinations(&fx);
    assert!(!fx.home.join(".agentspace").exists());
}

#[test]
fn no_ca_fails_with_the_no_ca_wording_and_leaves_nothing() {
    for with_agentspace_folder in [false, true] {
        let fx = Fx::without_ca();
        if with_agentspace_folder {
            fs::create_dir_all(fx.home.join(".agentspace")).unwrap();
        }
        let before = fx.snapshot();
        let f = fx.place(600).unwrap_err();
        assert_eq!(f.message, NO_CA);
        assert!(f.leftovers.is_empty());
        assert_eq!(fx.snapshot(), before);
        assert!(!fx.ca_dir().exists(), "Place created a CA");
    }
}

#[test]
fn a_ca_with_either_file_missing_is_no_ca() {
    for file in ["agentspace_ca", "agentspace_ca.pub"] {
        let fx = Fx::with_ca();
        fs::remove_file(fx.ca_dir().join(file)).unwrap();
        let before = fx.snapshot();
        let f = fx.place(600).unwrap_err();
        assert_eq!(f.message, NO_CA, "{file}");
        assert_eq!(fx.snapshot(), before);
    }
}

#[test]
fn an_unusable_ca_gets_its_own_wording_and_leaves_nothing() {
    // The public key is not a key.
    let fx = Fx::with_ca();
    fs::write(fx.ca_dir().join("agentspace_ca.pub"), b"not a key\n").unwrap();
    let before = fx.snapshot();
    let f = fx.place(600).unwrap_err();
    assert_eq!(f.message, CA_UNUSABLE);
    assert!(f.leftovers.is_empty());
    assert_eq!(fx.snapshot(), before);

    // The private key is readable by another principal.
    let fx = Fx::with_ca();
    icacls(
        &fx.ca_dir().join("agentspace_ca"),
        &["/grant", "*S-1-1-0:R"],
    );
    let before = fx.snapshot();
    let f = fx.place(600).unwrap_err();
    assert_eq!(f.message, CA_UNUSABLE);
    assert_eq!(fx.snapshot(), before);

    // The CA's folder is a file.
    let fx = Fx::with_ca();
    fs::remove_dir_all(fx.ca_dir()).unwrap();
    fs::write(fx.ca_dir(), b"x").unwrap();
    let before = fx.snapshot();
    let f = fx.place(600).unwrap_err();
    assert_eq!(f.message, CA_UNUSABLE);
    assert_eq!(fx.snapshot(), before);
}

#[test]
fn a_file_where_agentspace_d_has_to_be_fails_the_place_and_keeps_it() {
    let fx = Fx::with_ca();
    fs::write(fx.alias_dir(), b"mine\n").unwrap();
    let f = fx.place(600).unwrap_err();
    fx.assert_failure_plain(&f);
    assert!(
        f.message.contains("OpenSSH client configuration"),
        "{}",
        f.message
    );
    assert!(f.message.contains("No lease was recorded"), "{}", f.message);
    assert!(f.leftovers.is_empty(), "{:?}", f.leftovers);
    assert_eq!(fs::read(fx.alias_dir()).unwrap(), b"mine\n");
    assert!(!fx.lease_dir(ID).exists(), "The credential was not removed");
}

#[test]
fn a_foreign_alias_file_fails_the_place_and_is_kept_with_the_include_line() {
    for foreign in [
        format!("# agentspace lease {OTHER_ID}; written by agentspace\nHost {ALIAS}\n"),
        format!("Host {ALIAS}\n  User someone\n"),
    ] {
        let fx = Fx::with_ca();
        fs::write(fx.config(), b"Host mine\n  User me\n").unwrap();
        fs::create_dir_all(fx.alias_dir()).unwrap();
        fs::write(fx.alias_file(), foreign.as_bytes()).unwrap();
        let f = fx.place(600).unwrap_err();
        fx.assert_failure_plain(&f);
        assert!(f.leftovers.is_empty(), "{:?}", f.leftovers);
        assert_eq!(fs::read(fx.alias_file()).unwrap(), foreign.as_bytes());
        assert!(!fx.lease_dir(ID).exists(), "The credential was not removed");
        // The Include line, a backup and agentspace.d stay.
        let config = fs::read(fx.config()).unwrap();
        assert!(config.starts_with(b"Include agentspace.d/*"));
        assert!(
            Fx::names(&fx.ssh_dir)
                .iter()
                .any(|n| n.starts_with("config.agentspace-backup-"))
        );
        assert!(fx.alias_dir().is_dir());
    }
}

#[test]
fn an_existing_key_or_certificate_is_refused_and_left_alone() {
    for file in ["id_ed25519", "id_ed25519-cert.pub"] {
        let fx = Fx::with_ca();
        fs::create_dir_all(fx.lease_dir(ID)).unwrap();
        fs::write(fx.lease_dir(ID).join(file), b"existing").unwrap();
        let f = fx.place(600).unwrap_err();
        fx.assert_failure_plain(&f);
        assert_eq!(Fx::names(&fx.lease_dir(ID)), [file]);
        assert_eq!(fs::read(fx.lease_dir(ID).join(file)).unwrap(), b"existing");
        assert!(!fx.alias_file().exists());
    }
}

#[test]
fn a_failed_place_keeps_a_lease_folder_that_was_there_and_removes_what_it_made() {
    let fx = Fx::with_ca();
    fs::create_dir_all(fx.lease_dir(ID)).unwrap();
    fs::write(fx.lease_dir(ID).join("notes.txt"), b"keep").unwrap();
    fs::write(fx.alias_dir(), b"mine\n").unwrap();
    let f = fx.place(600).unwrap_err();
    fx.assert_failure_plain(&f);
    assert_eq!(Fx::names(&fx.lease_dir(ID)), ["notes.txt"]);
    assert_eq!(
        fs::read(fx.lease_dir(ID).join("notes.txt")).unwrap(),
        b"keep"
    );
}

#[test]
fn a_failed_place_does_not_touch_another_leases_files() {
    let fx = Fx::with_ca();
    fs::create_dir_all(fx.lease_dir(OTHER_ID)).unwrap();
    fs::write(fx.lease_dir(OTHER_ID).join("id_ed25519"), b"theirs").unwrap();
    fs::write(fx.alias_dir(), b"mine\n").unwrap();
    assert!(fx.place(600).is_err());
    assert_eq!(
        fs::read(fx.lease_dir(OTHER_ID).join("id_ed25519")).unwrap(),
        b"theirs"
    );
}

#[test]
fn a_place_after_a_failed_one_works() {
    let fx = Fx::with_ca();
    fs::write(fx.alias_dir(), b"mine\n").unwrap();
    assert!(fx.place(600).is_err());
    fs::remove_file(fx.alias_dir()).unwrap();
    fx.place_ready(600);
}
