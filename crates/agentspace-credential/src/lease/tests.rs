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
    // The folder of lease folders is shared, and this call made it: it MAY stay, and it is empty.
    let leases = home.path().join(".agentspace").join("leases");
    assert!(leases.is_dir(), "leases stays when this call made it");
    assert!(files_of(&leases).is_empty());
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

// ---------------------------------------------------------------------------------------------
// The delete failure's message is the `<why>` of lease-api.md's release template.
// ---------------------------------------------------------------------------------------------

/// Holds a file open without sharing, so that it cannot be deleted until the guard drops. This is
/// what a child process that opens it with `FileShare.Read` and sleeps does too; a handle of this
/// process is enough, since Windows refuses the delete either way.
fn hold(path: &Path) -> fs::File {
    fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(path)
        .unwrap()
}

/// The sentence the CLI builds around a delete failure's message (lease-api.md, "Credential or
/// alias not deleted (`release`)").
fn release_sentence(why: &str) -> String {
    format!(
        "The lease's local key, certificate or host alias could not be deleted: {why}. Run \
         release again to finish."
    )
}

#[test]
fn a_delete_failure_reads_as_the_why_of_the_release_message() {
    let home = throwaway();
    make_ca(home.path());
    issue(home.path(), ID, "ubuntu", 60).unwrap();
    let dir = lease_dir(home.path(), ID);
    let held = hold(&dir.join(KEY_FILE));
    let failure = remove_lease_folder(home.path(), ID).unwrap_err();
    // One leftover: the folder is not named, since the file's name says where it is.
    assert_eq!(
        failure.leftovers,
        [format!("~/.agentspace/leases/{ID}/id_ed25519")]
    );
    assert_eq!(
        failure.message,
        format!("~/.agentspace/leases/{ID}/id_ed25519 is still there")
    );
    assert!(!failure.message.ends_with('.'));
    assert!(!failure.message.chars().next().unwrap().is_uppercase());
    assert_eq!(
        release_sentence(&failure.message),
        format!(
            "The lease's local key, certificate or host alias could not be deleted: \
             ~/.agentspace/leases/{ID}/id_ed25519 is still there. Run release again to finish."
        )
    );
    // No absolute path and no user name.
    let lower = failure.message.to_lowercase();
    assert!(!lower.contains(&home.path().to_string_lossy().to_lowercase()));
    assert!(!lower.contains('\\'));
    drop(held);
    remove_lease_folder(home.path(), ID).unwrap();
}

#[test]
fn a_delete_failure_names_each_leftover_in_the_message() {
    let home = throwaway();
    make_ca(home.path());
    issue(home.path(), ID, "ubuntu", 60).unwrap();
    let dir = lease_dir(home.path(), ID);
    let held = (hold(&dir.join(KEY_FILE)), hold(&dir.join(CERT_FILE)));
    let failure = remove_lease_folder(home.path(), ID).unwrap_err();
    let mut leftovers = failure.leftovers.clone();
    leftovers.sort();
    assert_eq!(
        leftovers,
        [
            format!("~/.agentspace/leases/{ID}/id_ed25519"),
            format!("~/.agentspace/leases/{ID}/id_ed25519-cert.pub"),
        ]
    );
    assert_eq!(failure.message, delete_failure_why(&failure.leftovers));
    assert!(failure.message.ends_with("are still there"), "{failure:?}");
    for leftover in &failure.leftovers {
        assert!(failure.message.contains(leftover.as_str()), "{failure:?}");
    }
    assert!(!failure.message.ends_with('.'));
    drop(held);
    assert!(remove_lease_folder(home.path(), ID).is_ok());
}

// ---------------------------------------------------------------------------------------------
// A link is never followed when a lease folder is deleted.
// ---------------------------------------------------------------------------------------------

/// Makes a junction at `link` to `target`, with `mklink /J`, which needs no privilege.
fn junction(link: &Path, target: &Path) {
    let out = Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(out.status.success(), "mklink /J failed");
    assert!(
        platform::is_link(&fs::symlink_metadata(link).unwrap()),
        "the junction is a link"
    );
}

/// A folder outside every lease, holding a file that nothing here may delete.
fn outside_folder(home: &Path) -> PathBuf {
    let outside = home.join("outside");
    fs::create_dir_all(&outside).unwrap();
    fs::write(outside.join("precious.txt"), b"not agentspace's").unwrap();
    outside
}

fn assert_untouched(outside: &Path) {
    assert!(outside.is_dir(), "the target folder MUST still exist");
    assert_eq!(
        fs::read(outside.join("precious.txt")).unwrap(),
        b"not agentspace's",
        "the target's file MUST still exist, unchanged"
    );
}

#[test]
fn a_junction_where_the_lease_folder_should_be_is_removed_and_not_followed() {
    let home = throwaway();
    let outside = outside_folder(home.path());
    let leases = home.path().join(".agentspace").join("leases");
    fs::create_dir_all(&leases).unwrap();
    junction(&leases.join(ID), &outside);
    remove_lease_folder(home.path(), ID).unwrap();
    assert!(
        fs::symlink_metadata(leases.join(ID)).is_err(),
        "the link goes"
    );
    assert_untouched(&outside);
}

#[test]
fn a_junction_inside_the_lease_folder_is_removed_and_not_followed() {
    let home = throwaway();
    let outside = outside_folder(home.path());
    let dir = lease_dir(home.path(), ID);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(KEY_FILE), b"k").unwrap();
    junction(&dir.join("inner"), &outside);
    remove_lease_folder(home.path(), ID).unwrap();
    assert!(!dir.exists(), "the lease's own folder goes, link and all");
    assert_untouched(&outside);
}

#[test]
fn a_directory_symlink_is_removed_and_not_followed() {
    let home = throwaway();
    let outside = outside_folder(home.path());
    let leases = home.path().join(".agentspace").join("leases");
    fs::create_dir_all(&leases).unwrap();
    if std::os::windows::fs::symlink_dir(&outside, leases.join(ID)).is_err() {
        // Creating a symbolic link needs a privilege (or Developer Mode) that this run lacks.
        eprintln!("skipped: the process may not create symbolic links");
        return;
    }
    let dir = lease_dir(home.path(), ID);
    remove_lease_folder(home.path(), ID).unwrap();
    assert!(fs::symlink_metadata(&dir).is_err());
    assert_untouched(&outside);
}

// ---------------------------------------------------------------------------------------------
// The private key's rules are verified, and each departure is refused (state.md, "The lease
// folder").
// ---------------------------------------------------------------------------------------------

/// A folder that hands nothing down to what is made in it: its one entry, for the user, is not
/// inheritable. Inheritance switched on for a file in it adds no entry, so a test can tell the
/// protected flag apart from the entries.
fn bare_folder(home: &Path, name: &str) -> PathBuf {
    let dir = home.join(name);
    fs::create_dir_all(&dir).unwrap();
    let grant = format!("*{}:F", platform::user_sid_for_tests().unwrap());
    icacls(&[
        dir.as_os_str(),
        "/inheritance:r".as_ref(),
        "/grant:r".as_ref(),
        grant.as_ref(),
    ]);
    dir
}

#[test]
fn a_private_key_made_by_the_program_has_exactly_the_rules() {
    let home = throwaway();
    let dir = bare_folder(home.path(), "keys");
    let key = dir.join("id_ed25519");
    let file = platform::create_private_file(&key).expect("the key's file is made");
    // Before a byte is written: protected, with the three entries.
    assert_eq!(platform::protected_and_entries(&key), Some((true, 3)));
    assert!(platform::verify_private_key(&key));
    drop(file);
    // It never replaces a file.
    fs::write(dir.join("other"), b"mine").unwrap();
    assert!(platform::create_private_file(&dir.join("other")).is_none());
    assert_eq!(fs::read(dir.join("other")).unwrap(), b"mine");
}

#[test]
fn verify_private_key_refuses_each_departure_from_the_rules() {
    let home = throwaway();
    let dir = bare_folder(home.path(), "keys");
    let key = dir.join("id_ed25519");
    fs::write(&key, b"not a real key").unwrap();
    let user = platform::user_sid_for_tests().unwrap();
    let good = format!("(A;;FA;;;SY)(A;;FA;;;BA)(A;;FA;;;{user})");
    let set = |entries: &str| {
        assert!(platform::set_sddl_rules(&key, &format!("D:P{entries}")));
    };

    // The control: the rules as the program makes them are accepted.
    set(&good);
    assert!(platform::verify_private_key(&key), "the control");

    let refused = [
        ("an extra Everyone read", format!("{good}(A;;FR;;;WD)")),
        (
            "an extra entry for another group",
            format!("{good}(A;;FR;;;BU)"),
        ),
        ("a deny entry", format!("(D;;FR;;;WD){good}")),
        (
            "the user with read only",
            "(A;;FA;;;SY)(A;;FA;;;BA)".to_string() + &format!("(A;;FR;;;{user})"),
        ),
        ("SYSTEM missing", format!("(A;;FA;;;BA)(A;;FA;;;{user})")),
        (
            "Administrators missing",
            format!("(A;;FA;;;SY)(A;;FA;;;{user})"),
        ),
        (
            "SYSTEM without full control",
            format!("(A;;FR;;;SY)(A;;FA;;;BA)(A;;FA;;;{user})"),
        ),
        ("the user missing", "(A;;FA;;;SY)(A;;FA;;;BA)".to_string()),
    ];
    for (what, entries) in refused {
        set(&entries);
        assert!(!platform::verify_private_key(&key), "refused: {what}");
        set(&good);
        assert!(
            platform::verify_private_key(&key),
            "accepted again after {what}"
        );
    }

    // Inheritance on (the DACL not protected) with the same three entries and nothing inherited.
    icacls(&[key.as_os_str(), "/inheritance:e".as_ref()]);
    assert_eq!(platform::protected_and_entries(&key), Some((false, 3)));
    assert!(
        !platform::verify_private_key(&key),
        "refused: inheritance on"
    );
    set(&good);
    assert!(platform::verify_private_key(&key));

    // An owner that is not the user, when this process may give one.
    if platform::set_owner_and_rules(&key, "S-1-5-32-544", &format!("D:P{good}")) {
        assert!(
            !platform::verify_private_key(&key),
            "refused: the owner is not the user"
        );
    } else {
        // Giving a file an owner other than the user needs a privilege that a process that is not
        // elevated lacks, and no other file on this machine has such an owner to borrow.
        eprintln!("skipped the owner case: the process may not give another owner");
    }
}

// ---------------------------------------------------------------------------------------------
// The CA's "protected" check, alone (state.md, "The CA", "Its checks", 2).
// ---------------------------------------------------------------------------------------------

#[test]
fn a_ca_folder_that_is_unprotected_and_otherwise_perfect_is_not_usable() {
    let home = throwaway();
    let dir = make_ca(home.path());
    assert_eq!(check_ca(home.path()), Ok(()), "the control");
    // Nothing inheritable reaches below `.agentspace`, so turning inheritance on adds no entry.
    let user = platform::user_sid_for_tests().unwrap();
    let grant = format!("*{user}:F");
    icacls(&[
        home.path().join(".agentspace").as_os_str(),
        "/inheritance:r".as_ref(),
        "/grant:r".as_ref(),
        grant.as_ref(),
    ]);
    icacls(&[dir.as_os_str(), "/inheritance:e".as_ref()]);
    // Still one entry, for the user: only the protected flag is wrong.
    assert_eq!(platform::protected_and_entries(&dir), Some((false, 1)));
    assert_eq!(check_ca(home.path()), Err(CaFailure::NotUsable));
}

#[test]
fn a_ca_key_that_is_unprotected_and_otherwise_perfect_is_not_usable() {
    let home = throwaway();
    let dir = make_ca(home.path());
    // The folder's entry is made not inheritable, so the key inherits nothing, and the folder
    // still passes its own check.
    let user = platform::user_sid_for_tests().unwrap();
    let mine = format!("D:P(A;;FA;;;{user})");
    assert!(platform::set_sddl_rules(&dir, &mine));
    // The public key had its entry from the folder, which the change took away: give it its own.
    assert!(platform::set_sddl_rules(
        &dir.join("agentspace_ca.pub"),
        &mine
    ));
    let key = dir.join("agentspace_ca");
    assert_eq!(check_ca(home.path()), Ok(()), "the control");
    icacls(&[key.as_os_str(), "/inheritance:e".as_ref()]);
    assert_eq!(platform::protected_and_entries(&key), Some((false, 1)));
    assert_eq!(check_ca(home.path()), Err(CaFailure::NotUsable));
}

// ---------------------------------------------------------------------------------------------
// Renew, and the issue messages.
// ---------------------------------------------------------------------------------------------

#[test]
fn a_renew_whose_certificate_cannot_be_taken_as_the_previous_one_changes_nothing() {
    let home = throwaway();
    make_ca(home.path());
    issue(home.path(), ID, "ubuntu", 60).unwrap();
    let dir = lease_dir(home.path(), ID);
    // The certificate is replaced by a folder: it is there, and it is not a certificate.
    fs::remove_file(dir.join(CERT_FILE)).unwrap();
    fs::create_dir(dir.join(CERT_FILE)).unwrap();
    fs::write(dir.join(CERT_FILE).join("inside"), b"x").unwrap();
    let before = files_of(&dir);
    let failure = renew(home.path(), ID, "ubuntu", 60).unwrap_err();
    assert_eq!(failure.message, RENEW_FAILED);
    assert!(failure.leftovers.is_empty());
    assert_eq!(files_of(&dir), before, "no renew file was made");
    assert!(dir.join(CERT_FILE).is_dir());
}

#[test]
fn an_issue_failure_that_left_files_says_so_for_either_cause() {
    assert_eq!(issue_message(ISSUE_FAILED, false), ISSUE_FAILED);
    assert_eq!(issue_message(ISSUE_FAILED, true), ISSUE_FAILED_LEFT);
    assert_eq!(issue_message(KEY_UNPROTECTED, false), KEY_UNPROTECTED);
    assert_eq!(issue_message(KEY_UNPROTECTED, true), KEY_UNPROTECTED_LEFT);
    for message in [KEY_UNPROTECTED_LEFT, ISSUE_FAILED_LEFT] {
        assert!(!message.contains('\\') && !message.contains('~'));
    }
}

#[test]
fn a_failure_at_signing_comes_after_the_key_and_the_public_key_were_made() {
    // This and the `bad\nuser` and empty-principal tests depend on `sign_certificate` checking the
    // user (one line, not empty) at signing, the step after the key and the public key are made.
    // If that check moved earlier, those tests would still pass; this one would not.
    let home = throwaway();
    make_ca(home.path());
    let ca_private = fs::read(home.path().join(".agentspace/ca/agentspace_ca")).unwrap();
    let dir = lease_dir(home.path(), ID);
    fs::create_dir_all(&dir).unwrap();
    for user in ["", "bad\nuser"] {
        let mut made = Made {
            id: ID.to_string(),
            dir: dir.clone(),
            folder: false,
            files: Vec::new(),
        };
        let message = build(&mut made, &ca_private, user, 3600).unwrap_err();
        assert_eq!(message, ISSUE_FAILED);
        let names: Vec<&str> = made.files.iter().map(|(_, name)| name.as_str()).collect();
        assert_eq!(names, [KEY_FILE, PUB_FILE], "failed after both were made");
        assert!(dir.join(KEY_FILE).is_file() && dir.join(PUB_FILE).is_file());
        assert!(!dir.join(CERT_FILE).exists());
        assert_eq!(made.undo(), Vec::<String>::new());
        assert!(files_of(&dir).is_empty());
    }
}

// ---------------------------------------------------------------------------------------------
// A file where a folder has to be (architect, 2026-11-19; state.md, "The CA" and "Release").
// ---------------------------------------------------------------------------------------------

/// The three places a file can stand where a folder of the CA has to be, each as a home with the
/// file there and nothing below it: the home folder itself, `.agentspace`, and the CA's folder.
fn homes_with_a_file_for_the_ca() -> Vec<(&'static str, TempDir, PathBuf)> {
    let mut homes = Vec::new();

    // The home folder is a file: the "home" is a path to a file in a throwaway folder.
    let outer = throwaway();
    let home = outer.path().join("home-file");
    fs::write(&home, b"not a folder").unwrap();
    homes.push(("home", outer, home));

    // `.agentspace` is a file.
    let outer = throwaway();
    let home = outer.path().to_path_buf();
    fs::write(home.join(".agentspace"), b"not a folder").unwrap();
    homes.push((".agentspace", outer, home));

    // The CA's folder is a file.
    let outer = throwaway();
    let home = outer.path().to_path_buf();
    fs::create_dir_all(home.join(".agentspace")).unwrap();
    fs::write(home.join(".agentspace").join("ca"), b"not a folder").unwrap();
    homes.push(("ca", outer, home));
    homes
}

#[test]
fn a_file_where_a_folder_of_the_ca_has_to_be_is_not_usable_and_not_missing() {
    for (what, _outer, home) in homes_with_a_file_for_the_ca() {
        assert_eq!(check_ca(&home), Err(CaFailure::NotUsable), "{what}");
    }
}

#[test]
fn issue_and_renew_with_a_file_in_the_cas_path_say_not_usable_and_make_nothing() {
    for (what, _outer, home) in homes_with_a_file_for_the_ca() {
        let issued = issue(&home, ID, "ubuntu", 60).unwrap_err();
        assert_eq!(
            issued.message,
            ca_failure_message(CaFailure::NotUsable),
            "{what}"
        );
        assert!(issued.leftovers.is_empty(), "{what}");
        let renewed = renew(&home, ID, "ubuntu", 60).unwrap_err();
        assert_eq!(
            renewed.message,
            ca_failure_message(CaFailure::NotUsable),
            "{what}"
        );
        assert!(renewed.leftovers.is_empty(), "{what}");
        // Nothing was created: the file is still a file, and no `leases` folder exists.
        match what {
            "home" => assert!(home.is_file(), "{what}"),
            ".agentspace" => assert!(home.join(".agentspace").is_file(), "{what}"),
            _ => {
                assert!(home.join(".agentspace").join("ca").is_file(), "{what}");
                assert!(!home.join(".agentspace").join("leases").exists(), "{what}");
            }
        }
    }
}

#[test]
fn a_missing_home_or_agentspace_is_still_a_missing_ca() {
    let outer = throwaway();
    assert_eq!(
        check_ca(&outer.path().join("no-such-home")),
        Err(CaFailure::Missing)
    );
    assert_eq!(check_ca(outer.path()), Err(CaFailure::Missing));
    fs::create_dir_all(outer.path().join(".agentspace")).unwrap();
    assert_eq!(check_ca(outer.path()), Err(CaFailure::Missing));
}

#[test]
fn remove_lease_folder_fails_when_a_file_stands_where_a_folder_for_leases_has_to_be() {
    // Each entry has a file where `<home>`, `.agentspace` or `leases` has to be.
    let mut homes = Vec::new();
    let outer = throwaway();
    let home = outer.path().join("home-file");
    fs::write(&home, b"keep me").unwrap();
    homes.push((outer, home.clone(), home));

    let outer = throwaway();
    let home = outer.path().to_path_buf();
    let file = home.join(".agentspace");
    fs::write(&file, b"keep me").unwrap();
    homes.push((outer, home, file));

    let outer = throwaway();
    let home = outer.path().to_path_buf();
    fs::create_dir_all(home.join(".agentspace")).unwrap();
    let file = home.join(".agentspace").join("leases");
    fs::write(&file, b"keep me").unwrap();
    homes.push((outer, home, file));

    for (_outer, home, file) in homes {
        let failure = remove_lease_folder(&home, ID).unwrap_err();
        assert_eq!(
            failure.message, "the folder for leases is a file, not a folder",
            "{file:?}"
        );
        assert!(!failure.message.ends_with('.'));
        assert!(!failure.message.chars().next().unwrap().is_uppercase());
        assert_eq!(
            release_sentence(&failure.message),
            "The lease's local key, certificate or host alias could not be deleted: the folder \
             for leases is a file, not a folder. Run release again to finish."
        );
        // Nothing was deleted: the file is still there, with what it held.
        assert_eq!(fs::read(&file).unwrap(), b"keep me");
    }
}

#[test]
fn remove_lease_folder_still_treats_a_missing_leases_folder_as_nothing_to_delete() {
    let outer = throwaway();
    assert!(remove_lease_folder(&outer.path().join("no-such-home"), ID).is_ok());
    assert!(remove_lease_folder(outer.path(), ID).is_ok());
    fs::create_dir_all(outer.path().join(".agentspace")).unwrap();
    assert!(remove_lease_folder(outer.path(), ID).is_ok());
}
