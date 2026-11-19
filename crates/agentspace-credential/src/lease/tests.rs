//! Unit tests of the Windows half. Each works in a throwaway folder under the temp folder, used as
//! the home; none reads or writes the real profile.

use std::fs;
use std::os::windows::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use tempfile::TempDir;

use super::*;
use crate::new_lease_key;
use crate::platform;

const ID: &str = "0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d";

fn throwaway() -> TempDir {
    tempfile::Builder::new()
        .prefix("agentspace-cred-")
        .tempdir()
        .unwrap()
}

/// Makes a CA the way the operator's `ca` leaves it: an owner-only folder and an owner-only key.
fn make_ca(home: &Path) -> PathBuf {
    let dir = home.join(".agentspace").join("ca");
    fs::create_dir_all(&dir).unwrap();
    assert!(platform::make_owner_only(&dir, true));
    let ca = new_lease_key("ca").unwrap();
    fs::write(dir.join("agentspace_ca"), &ca.private_openssh).unwrap();
    assert!(platform::make_owner_only(&dir.join("agentspace_ca"), false));
    fs::write(
        dir.join("agentspace_ca.pub"),
        format!("{}\r\n", ca.public_line),
    )
    .unwrap();
    dir
}

fn icacls(args: &[&std::ffi::OsStr]) {
    let out = Command::new("icacls")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(out.status.success(), "icacls failed");
}

fn ssh_keygen() -> Option<PathBuf> {
    let path = PathBuf::from(r"C:\Windows\System32\OpenSSH\ssh-keygen.exe");
    path.exists().then_some(path)
}

fn files_of(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn a_ca_made_as_the_operator_makes_it_passes_and_is_not_changed() {
    let home = throwaway();
    let dir = make_ca(home.path());
    let before = (
        fs::read(dir.join("agentspace_ca")).unwrap(),
        fs::read(dir.join("agentspace_ca.pub")).unwrap(),
    );
    assert_eq!(check_ca(home.path()), Ok(()));
    let after = (
        fs::read(dir.join("agentspace_ca")).unwrap(),
        fs::read(dir.join("agentspace_ca.pub")).unwrap(),
    );
    assert_eq!(before, after);
}

#[test]
fn a_ca_with_a_file_missing_is_missing() {
    let home = throwaway();
    assert_eq!(check_ca(home.path()), Err(CaFailure::Missing));
    let dir = make_ca(home.path());
    fs::remove_file(dir.join("agentspace_ca.pub")).unwrap();
    assert_eq!(check_ca(home.path()), Err(CaFailure::Missing));
    let home = throwaway();
    let dir = make_ca(home.path());
    fs::remove_file(dir.join("agentspace_ca")).unwrap();
    assert_eq!(check_ca(home.path()), Err(CaFailure::Missing));
}

#[test]
fn a_ca_key_with_one_more_entry_is_not_usable() {
    let home = throwaway();
    let dir = make_ca(home.path());
    let user = platform::user_sid_for_tests().unwrap();
    assert!(platform::set_sddl_rules(
        &dir.join("agentspace_ca"),
        &format!("D:P(A;;FA;;;{user})(A;;FA;;;SY)")
    ));
    assert_eq!(check_ca(home.path()), Err(CaFailure::NotUsable));
}

#[test]
fn a_ca_key_with_less_than_full_control_is_not_usable() {
    let home = throwaway();
    let dir = make_ca(home.path());
    let user = platform::user_sid_for_tests().unwrap();
    assert!(platform::set_sddl_rules(
        &dir.join("agentspace_ca"),
        &format!("D:P(A;;FR;;;{user})")
    ));
    assert_eq!(check_ca(home.path()), Err(CaFailure::NotUsable));
}

#[test]
fn a_ca_key_with_a_deny_entry_is_not_usable() {
    let home = throwaway();
    let dir = make_ca(home.path());
    let user = platform::user_sid_for_tests().unwrap();
    assert!(platform::set_sddl_rules(
        &dir.join("agentspace_ca"),
        &format!("D:P(D;;FX;;;BG)(A;;FA;;;{user})")
    ));
    assert_eq!(check_ca(home.path()), Err(CaFailure::NotUsable));
}

#[test]
fn a_ca_key_with_inheritance_on_is_not_usable() {
    let home = throwaway();
    let dir = make_ca(home.path());
    icacls(&[
        dir.join("agentspace_ca").as_os_str(),
        "/inheritance:e".as_ref(),
    ]);
    assert_eq!(check_ca(home.path()), Err(CaFailure::NotUsable));
}

#[test]
fn a_ca_folder_that_is_not_owner_only_is_not_usable() {
    let home = throwaway();
    let dir = make_ca(home.path());
    icacls(&[dir.as_os_str(), "/inheritance:e".as_ref()]);
    assert_eq!(check_ca(home.path()), Err(CaFailure::NotUsable));
}

#[test]
fn a_ca_whose_public_key_is_another_key_is_not_usable() {
    let home = throwaway();
    let dir = make_ca(home.path());
    let other = new_lease_key("other").unwrap();
    fs::write(dir.join("agentspace_ca.pub"), other.public_line).unwrap();
    assert_eq!(check_ca(home.path()), Err(CaFailure::NotUsable));
}

#[test]
fn the_failure_messages_are_lease_apis_wording() {
    assert!(ca_failure_message(CaFailure::Missing).starts_with("agentspace's certificate"));
    assert!(ca_failure_message(CaFailure::Missing).ends_with("the operator sets it up."));
    assert_eq!(
        ca_failure_message(CaFailure::NotUsable),
        "agentspace's certificate authority is not usable. Nothing was changed; the operator's ca \
         command says why."
    );
}

#[test]
fn issue_without_a_ca_changes_nothing() {
    let home = throwaway();
    let failure = issue(home.path(), ID, "ubuntu", 3600).unwrap_err();
    assert_eq!(failure.message, ca_failure_message(CaFailure::Missing));
    assert!(failure.leftovers.is_empty());
    assert!(!home.path().join(".agentspace").exists());
}

#[test]
fn an_issued_credential_is_three_files_with_the_keys_rules() {
    let home = throwaway();
    make_ca(home.path());
    let issued = issue(home.path(), ID, "ubuntu", 3600).unwrap();
    let dir = lease_dir(home.path(), ID);
    assert_eq!(
        files_of(&dir),
        ["id_ed25519", "id_ed25519-cert.pub", "id_ed25519.pub"]
    );
    assert!(platform::verify_private_key(&dir.join(KEY_FILE)));
    let public = fs::read_to_string(dir.join(PUB_FILE)).unwrap();
    assert!(public.ends_with('\n') && !public.ends_with("\r\n"));
    assert_eq!(public.matches('\n').count(), 1);
    let cert = fs::read_to_string(dir.join(CERT_FILE)).unwrap();
    assert!(cert.ends_with('\n') && !cert.ends_with("\r\n"));
    assert_eq!(cert.matches('\n').count(), 1);
    let now = now_unix().unwrap();
    assert!((issued.expires_at - (now + 3600)).abs() <= 5);

    if let Some(keygen) = ssh_keygen() {
        let loaded = Command::new(&keygen)
            .args(["-y", "-P", "", "-f"])
            .arg(dir.join(KEY_FILE))
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert!(loaded.status.success(), "ssh-keygen -y refused the key");
        let derived = String::from_utf8_lossy(&loaded.stdout).to_string();
        let derived: Vec<&str> = derived.split_whitespace().collect();
        let stored: Vec<&str> = public.split_whitespace().collect();
        assert_eq!(derived[..2], stored[..2]);

        let listing = Command::new(&keygen)
            .args(["-L", "-f"])
            .arg(dir.join(CERT_FILE))
            .stdin(Stdio::null())
            .output()
            .unwrap();
        let listing = String::from_utf8_lossy(&listing.stdout).to_string();
        assert!(listing.contains("user certificate"), "{listing}");
        assert!(listing.contains(&format!("Key ID: \"{ID}\"")), "{listing}");
        assert!(listing.contains("ubuntu"), "{listing}");
        assert!(listing.contains("permit-pty"), "{listing}");
        assert!(listing.contains("permit-port-forwarding"), "{listing}");
    }
}

#[test]
fn issue_refuses_an_existing_key_and_changes_nothing() {
    let home = throwaway();
    make_ca(home.path());
    let dir = lease_dir(home.path(), ID);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(KEY_FILE), b"keep me").unwrap();
    assert!(issue(home.path(), ID, "ubuntu", 3600).is_err());
    assert_eq!(files_of(&dir), ["id_ed25519"]);
    assert_eq!(fs::read(dir.join(KEY_FILE)).unwrap(), b"keep me");

    fs::remove_file(dir.join(KEY_FILE)).unwrap();
    fs::write(dir.join(CERT_FILE), b"keep me too").unwrap();
    assert!(issue(home.path(), ID, "ubuntu", 3600).is_err());
    assert_eq!(files_of(&dir), ["id_ed25519-cert.pub"]);
}

#[test]
fn issue_uses_a_folder_that_exists_without_a_key_and_keeps_it_on_failure() {
    let home = throwaway();
    make_ca(home.path());
    let dir = lease_dir(home.path(), ID);
    fs::create_dir_all(&dir).unwrap();
    // A user with a line break fails at signing, after the key has been made.
    let failure = issue(home.path(), ID, "bad\nuser", 3600).unwrap_err();
    assert!(failure.leftovers.is_empty(), "{failure:?}");
    assert!(dir.is_dir(), "a folder this call did not make stays");
    assert!(files_of(&dir).is_empty());
}

#[test]
fn issue_that_fails_at_signing_leaves_no_file_and_no_folder_it_made() {
    let home = throwaway();
    make_ca(home.path());
    let failure = issue(home.path(), ID, "bad\nuser", 3600).unwrap_err();
    assert!(failure.leftovers.is_empty());
    assert!(!lease_dir(home.path(), ID).exists());
    assert!(!failure.message.contains(home.path().to_str().unwrap()));
}

#[test]
fn issue_names_what_it_could_not_remove_home_relative() {
    let home = throwaway();
    let dir = lease_dir(home.path(), ID);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(KEY_FILE), b"a").unwrap();
    fs::write(dir.join(PUB_FILE), b"b").unwrap();
    let held = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(dir.join(KEY_FILE))
        .unwrap();
    let made = Made {
        id: ID.to_string(),
        dir: dir.clone(),
        folder: true,
        files: vec![
            (dir.join(KEY_FILE), KEY_FILE.to_string()),
            (dir.join(PUB_FILE), PUB_FILE.to_string()),
        ],
    };
    // The key is held, so it stays, and so does the folder; the folder is not named twice.
    assert_eq!(
        made.undo(),
        [format!("~/.agentspace/leases/{ID}/id_ed25519")]
    );
    assert!(!dir.join(PUB_FILE).exists());
    drop(held);
}

#[test]
fn issue_with_a_bad_id_changes_nothing() {
    let home = throwaway();
    make_ca(home.path());
    for id in ["", "..", "a/b", "a\\b", "c:x", "x\ny"] {
        assert!(issue(home.path(), id, "ubuntu", 60).is_err(), "{id:?}");
    }
    assert!(!home.path().join(".agentspace").join("leases").exists());
}

#[test]
fn a_committed_renewal_leaves_the_three_files_only() {
    let home = throwaway();
    make_ca(home.path());
    let first = issue(home.path(), ID, "ubuntu", 60).unwrap();
    let dir = lease_dir(home.path(), ID);
    let key = fs::read(dir.join(KEY_FILE)).unwrap();
    let public = fs::read(dir.join(PUB_FILE)).unwrap();
    let old = fs::read(dir.join(CERT_FILE)).unwrap();

    let guard = renew(home.path(), ID, "ubuntu", 7200).unwrap();
    assert!(guard.expires_at() > first.expires_at);
    let during = files_of(&dir);
    assert!(during.iter().any(|n| n.starts_with("renew-")));
    assert_ne!(fs::read(dir.join(CERT_FILE)).unwrap(), old);
    guard.commit();

    assert_eq!(
        files_of(&dir),
        ["id_ed25519", "id_ed25519-cert.pub", "id_ed25519.pub"]
    );
    assert_eq!(fs::read(dir.join(KEY_FILE)).unwrap(), key);
    assert_eq!(fs::read(dir.join(PUB_FILE)).unwrap(), public);
    assert!(platform::verify_private_key(&dir.join(KEY_FILE)));
}

#[test]
fn a_rolled_back_renewal_puts_the_old_certificate_back_byte_for_byte() {
    let home = throwaway();
    make_ca(home.path());
    issue(home.path(), ID, "ubuntu", 60).unwrap();
    let dir = lease_dir(home.path(), ID);
    let old = fs::read(dir.join(CERT_FILE)).unwrap();
    let guard = renew(home.path(), ID, "ubuntu", 7200).unwrap();
    assert_ne!(fs::read(dir.join(CERT_FILE)).unwrap(), old);
    guard.rollback().unwrap();
    assert_eq!(fs::read(dir.join(CERT_FILE)).unwrap(), old);
    assert_eq!(
        files_of(&dir),
        ["id_ed25519", "id_ed25519-cert.pub", "id_ed25519.pub"]
    );
}

#[test]
fn a_dropped_renewal_rolls_back() {
    let home = throwaway();
    make_ca(home.path());
    issue(home.path(), ID, "ubuntu", 60).unwrap();
    let dir = lease_dir(home.path(), ID);
    let old = fs::read(dir.join(CERT_FILE)).unwrap();
    drop(renew(home.path(), ID, "ubuntu", 7200).unwrap());
    assert_eq!(fs::read(dir.join(CERT_FILE)).unwrap(), old);
    assert_eq!(files_of(&dir).len(), 3);
}

#[test]
fn rolling_back_a_renewal_with_no_previous_certificate_deletes_the_new_one() {
    let home = throwaway();
    make_ca(home.path());
    issue(home.path(), ID, "ubuntu", 60).unwrap();
    let dir = lease_dir(home.path(), ID);
    fs::remove_file(dir.join(CERT_FILE)).unwrap();
    let guard = renew(home.path(), ID, "ubuntu", 7200).unwrap();
    assert!(dir.join(CERT_FILE).is_file());
    guard.rollback().unwrap();
    assert_eq!(files_of(&dir), ["id_ed25519", "id_ed25519.pub"]);
}

#[test]
fn renew_needs_the_key_and_the_public_key_and_changes_nothing() {
    let home = throwaway();
    make_ca(home.path());
    let failure = renew(home.path(), ID, "ubuntu", 60).unwrap_err();
    assert_eq!(failure.message, KEY_MISSING);
    issue(home.path(), ID, "ubuntu", 60).unwrap();
    let dir = lease_dir(home.path(), ID);
    fs::remove_file(dir.join(PUB_FILE)).unwrap();
    assert_eq!(
        renew(home.path(), ID, "ubuntu", 60).unwrap_err().message,
        KEY_MISSING
    );
    assert_eq!(files_of(&dir), ["id_ed25519", "id_ed25519-cert.pub"]);
}

#[test]
fn a_renewal_that_fails_at_signing_changes_nothing() {
    let home = throwaway();
    make_ca(home.path());
    issue(home.path(), ID, "ubuntu", 60).unwrap();
    let dir = lease_dir(home.path(), ID);
    let old = fs::read(dir.join(CERT_FILE)).unwrap();
    let failure = renew(home.path(), ID, "bad\nuser", 60).unwrap_err();
    assert!(failure.leftovers.is_empty());
    assert_eq!(fs::read(dir.join(CERT_FILE)).unwrap(), old);
    assert_eq!(files_of(&dir).len(), 3);
}

#[test]
fn remove_lease_folder_deletes_everything_and_a_missing_folder_is_fine() {
    let home = throwaway();
    make_ca(home.path());
    assert!(remove_lease_folder(home.path(), ID).is_ok());
    issue(home.path(), ID, "ubuntu", 60).unwrap();
    let dir = lease_dir(home.path(), ID);
    fs::write(dir.join("renew-ab-previous-cert.pub"), b"x").unwrap();
    remove_lease_folder(home.path(), ID).unwrap();
    assert!(!dir.exists());
    assert!(home.path().join(".agentspace").join("leases").is_dir());
}

#[test]
fn remove_lease_folder_names_what_it_could_not_delete() {
    let home = throwaway();
    make_ca(home.path());
    issue(home.path(), ID, "ubuntu", 60).unwrap();
    let dir = lease_dir(home.path(), ID);
    let held = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(dir.join(CERT_FILE))
        .unwrap();
    let failure = remove_lease_folder(home.path(), ID).unwrap_err();
    assert_eq!(
        failure.leftovers,
        [format!("~/.agentspace/leases/{ID}/id_ed25519-cert.pub")]
    );
    assert!(!failure.message.contains(home.path().to_str().unwrap()));
    drop(held);
    remove_lease_folder(home.path(), ID).unwrap();
    assert!(!dir.exists());
}
