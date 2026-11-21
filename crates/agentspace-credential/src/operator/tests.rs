//! Unit tests of the operator's key and certificate. Each works in a throwaway folder under the
//! temp folder, used as the home, with a CA made inside it; none reads or writes the real profile.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use ssh_key::certificate::CertType;
use ssh_key::{Certificate, PrivateKey};
use tempfile::TempDir;

use super::*;
use crate::new_lease_key;

const PROJECT: &str = "swimcoach";

fn throwaway() -> TempDir {
    tempfile::Builder::new()
        .prefix("agentspace-opcred-")
        .tempdir()
        .unwrap()
}

/// Makes a CA the way the operator's `ca` leaves it: an owner-only folder and an owner-only key.
fn make_ca(home: &Path) -> Vec<u8> {
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
    ca.public_line.into_bytes()
}

fn dir_of(home: &Path) -> PathBuf {
    host_dir(home, PROJECT)
}

fn files_of(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn read_cert(dir: &Path) -> Certificate {
    let text = fs::read_to_string(dir.join(CERT_FILE)).unwrap();
    Certificate::from_openssh(text.trim()).unwrap()
}

#[test]
fn a_project_is_valid_as_the_host_registry_names_it() {
    for ok in ["a", "swimcoach", "a-b", "a1", &"a".repeat(32)] {
        assert!(valid_project(ok), "{ok}");
    }
    for bad in [
        "",
        "-a",
        "a-",
        "A",
        "a_b",
        "a.b",
        "a/b",
        "a\\b",
        "..",
        "a b",
        "a\n",
        &"a".repeat(33),
    ] {
        assert!(!valid_project(bad), "{bad}");
    }
}

#[test]
fn ensure_makes_the_key_once_with_its_folders_rules_and_comment() {
    let home = throwaway();
    let dir = dir_of(home.path());
    assert!(!dir.exists());
    assert_eq!(ensure_operator_key(home.path(), PROJECT), Ok(true));
    assert_eq!(files_of(&dir), ["id_ed25519", "id_ed25519.pub"]);
    assert!(platform::verify_private_key(&dir.join(KEY_FILE)));

    let private = fs::read(dir.join(KEY_FILE)).unwrap();
    check_unencrypted(&private).unwrap();
    let key = PrivateKey::from_openssh(&private).unwrap();
    assert_eq!(key.comment(), "agentspace host swimcoach");
    let public = fs::read_to_string(dir.join(PUB_FILE)).unwrap();
    assert!(public.ends_with("agentspace host swimcoach\n"));
    assert!(!public.contains('\r'));
    assert_eq!(public.matches('\n').count(), 1);
    assert_eq!(public.trim_end(), key.public_key().to_openssh().unwrap());

    // The second call finds the key and changes nothing.
    assert_eq!(ensure_operator_key(home.path(), PROJECT), Ok(false));
    assert_eq!(fs::read(dir.join(KEY_FILE)).unwrap(), private);
    assert_eq!(fs::read_to_string(dir.join(PUB_FILE)).unwrap(), public);
}

#[test]
fn ensure_needs_no_ca() {
    let home = throwaway();
    assert_eq!(ensure_operator_key(home.path(), PROJECT), Ok(true));
    assert!(!home.path().join(".agentspace").join("ca").exists());
}

#[test]
fn ensure_replaces_a_leftover_public_key_and_certificate_without_a_key() {
    let home = throwaway();
    let dir = dir_of(home.path());
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(PUB_FILE), b"old public").unwrap();
    fs::write(dir.join(CERT_FILE), b"old certificate").unwrap();
    assert_eq!(ensure_operator_key(home.path(), PROJECT), Ok(true));
    assert_eq!(files_of(&dir), ["id_ed25519", "id_ed25519.pub"]);
    let public = fs::read_to_string(dir.join(PUB_FILE)).unwrap();
    assert!(public.starts_with("ssh-ed25519 "));
}

#[test]
fn ensure_never_replaces_a_key_that_does_not_load() {
    let home = throwaway();
    let dir = dir_of(home.path());
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(KEY_FILE), b"not a key").unwrap();
    fs::write(dir.join(PUB_FILE), b"keep the public key").unwrap();
    let failure = ensure_operator_key(home.path(), PROJECT).unwrap_err();
    assert!(
        failure
            .message
            .contains("~/.agentspace/hosts/swimcoach/id_ed25519"),
        "{}",
        failure.message
    );
    assert!(failure.message.starts_with("The operator's key"));
    // No path outside the home: the throwaway folder's name is nowhere in the message.
    assert!(
        !failure
            .message
            .contains(&home.path().to_string_lossy().into_owned())
    );
    assert_eq!(fs::read(dir.join(KEY_FILE)).unwrap(), b"not a key");
    assert_eq!(
        fs::read(dir.join(PUB_FILE)).unwrap(),
        b"keep the public key"
    );
}

#[test]
fn ensure_never_replaces_something_that_is_not_a_file_at_the_key() {
    let home = throwaway();
    let dir = dir_of(home.path());
    fs::create_dir_all(dir.join(KEY_FILE)).unwrap();
    let failure = ensure_operator_key(home.path(), PROJECT).unwrap_err();
    assert!(failure.message.contains("id_ed25519"));
    assert!(dir.join(KEY_FILE).is_dir());
}

#[test]
fn ensure_fails_for_a_project_that_cannot_name_a_folder() {
    let home = throwaway();
    for bad in ["..", "a/b", "A", ""] {
        assert!(ensure_operator_key(home.path(), bad).is_err(), "{bad}");
    }
    assert!(!home.path().join(".agentspace").exists());
}

#[test]
fn ensure_fails_when_the_hosts_folder_is_a_file() {
    let home = throwaway();
    let agentspace = home.path().join(".agentspace");
    fs::create_dir_all(&agentspace).unwrap();
    fs::write(agentspace.join("hosts"), b"a file").unwrap();
    assert!(ensure_operator_key(home.path(), PROJECT).is_err());
    assert_eq!(fs::read(agentspace.join("hosts")).unwrap(), b"a file");
}

#[test]
fn sign_makes_the_certificate_the_state_md_table_gives() {
    let home = throwaway();
    let ca_public = make_ca(home.path());
    ensure_operator_key(home.path(), PROJECT).unwrap();
    let dir = dir_of(home.path());
    let now = 1_800_000_000;
    assert_eq!(
        sign_operator_certificate(home.path(), PROJECT, now),
        Ok(now + 3600)
    );
    assert_eq!(
        files_of(&dir),
        ["id_ed25519", "id_ed25519-cert.pub", "id_ed25519.pub"]
    );

    let text = fs::read_to_string(dir.join(CERT_FILE)).unwrap();
    assert!(text.ends_with('\n') && !text.ends_with("\r\n"));
    assert_eq!(text.matches('\n').count(), 1);
    let cert = read_cert(&dir);
    assert_eq!(cert.cert_type(), CertType::User);
    assert_eq!(cert.key_id(), "operator-swimcoach");
    assert_eq!(cert.valid_principals(), ["ubuntu"]);
    assert_eq!(cert.valid_after(), (now - 300) as u64);
    assert_eq!(cert.valid_before(), (now + 3600) as u64);
    assert_eq!(cert.serial(), 0);
    assert!(cert.critical_options().is_empty());
    let extensions: Vec<&str> = cert.extensions().keys().map(String::as_str).collect();
    assert_eq!(extensions, ["permit-port-forwarding", "permit-pty"]);
    assert!(cert.extensions().values().all(String::is_empty));
    assert_eq!(cert.comment(), "agentspace host swimcoach");

    // For the operator's key, signed by the CA.
    let public =
        ssh_key::PublicKey::from_openssh(fs::read_to_string(dir.join(PUB_FILE)).unwrap().trim())
            .unwrap();
    assert_eq!(cert.public_key(), public.key_data());
    let ca =
        ssh_key::PublicKey::from_openssh(String::from_utf8(ca_public).unwrap().trim()).unwrap();
    cert.verify_signature().unwrap();
    assert_eq!(cert.signature_key(), ca.key_data());
}

#[test]
fn sign_reads_back_with_ssh_keygen() {
    let keygen = PathBuf::from(r"C:\Windows\System32\OpenSSH\ssh-keygen.exe");
    if !keygen.exists() {
        return;
    }
    let home = throwaway();
    make_ca(home.path());
    ensure_operator_key(home.path(), PROJECT).unwrap();
    let dir = dir_of(home.path());
    sign_operator_certificate(home.path(), PROJECT, 1_800_000_000).unwrap();
    let listing = Command::new(&keygen)
        .args(["-L", "-f"])
        .arg(dir.join(CERT_FILE))
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let listing = String::from_utf8_lossy(&listing.stdout).to_string();
    assert!(listing.contains("user certificate"), "{listing}");
    assert!(
        listing.contains("Key ID: \"operator-swimcoach\""),
        "{listing}"
    );
    assert!(listing.contains("ubuntu"), "{listing}");
    assert!(listing.contains("permit-pty"), "{listing}");
    assert!(listing.contains("permit-port-forwarding"), "{listing}");
}

#[test]
fn sign_replaces_the_old_certificate_and_leaves_no_sign_files() {
    let home = throwaway();
    make_ca(home.path());
    ensure_operator_key(home.path(), PROJECT).unwrap();
    let dir = dir_of(home.path());
    sign_operator_certificate(home.path(), PROJECT, 1_800_000_000).unwrap();
    let first = fs::read(dir.join(CERT_FILE)).unwrap();
    assert_eq!(
        sign_operator_certificate(home.path(), PROJECT, 1_800_003_600),
        Ok(1_800_007_200)
    );
    let second = fs::read(dir.join(CERT_FILE)).unwrap();
    assert_ne!(first, second);
    assert_eq!(read_cert(&dir).valid_before(), 1_800_007_200);
    assert!(files_of(&dir).iter().all(|n| !n.starts_with("sign-")));
}

#[test]
fn sign_with_no_usable_ca_writes_nothing() {
    let home = throwaway();
    ensure_operator_key(home.path(), PROJECT).unwrap();
    let dir = dir_of(home.path());
    let before = files_of(&dir);

    // No CA at all.
    let failure = sign_operator_certificate(home.path(), PROJECT, 1_800_000_000).unwrap_err();
    assert_eq!(
        failure.message,
        crate::ca_failure_message(crate::CaFailure::Missing)
    );
    assert_eq!(files_of(&dir), before);

    // A CA whose private key is not owner-only.
    let ca = make_ca(home.path());
    drop(ca);
    let user = platform::user_sid_for_tests().unwrap();
    let key = home.path().join(".agentspace/ca/agentspace_ca");
    assert!(platform::set_sddl_rules(
        &key,
        &format!("D:P(A;;FA;;;{user})(A;;FA;;;SY)")
    ));
    let failure = sign_operator_certificate(home.path(), PROJECT, 1_800_000_000).unwrap_err();
    assert_eq!(
        failure.message,
        crate::ca_failure_message(crate::CaFailure::NotUsable)
    );
    assert_eq!(files_of(&dir), before);
}

#[test]
fn sign_with_a_missing_key_names_it_and_writes_nothing() {
    let home = throwaway();
    make_ca(home.path());
    let dir = dir_of(home.path());
    fs::create_dir_all(&dir).unwrap();
    let failure = sign_operator_certificate(home.path(), PROJECT, 1_800_000_000).unwrap_err();
    assert!(
        failure
            .message
            .contains("~/.agentspace/hosts/swimcoach/id_ed25519"),
        "{}",
        failure.message
    );
    assert!(files_of(&dir).is_empty());

    // A key without its public key is missing too, and nothing makes it here.
    ensure_operator_key(home.path(), PROJECT).unwrap();
    fs::remove_file(dir.join(PUB_FILE)).unwrap();
    let failure = sign_operator_certificate(home.path(), PROJECT, 1_800_000_000).unwrap_err();
    assert!(failure.message.contains("id_ed25519.pub"));
    assert_eq!(files_of(&dir), ["id_ed25519"]);
}

#[test]
fn a_failed_sign_deletes_its_sign_files_and_keeps_the_old_certificate_place() {
    let home = throwaway();
    make_ca(home.path());
    ensure_operator_key(home.path(), PROJECT).unwrap();
    let dir = dir_of(home.path());
    // A folder where the certificate goes makes the rename fail.
    fs::create_dir_all(dir.join(CERT_FILE)).unwrap();
    let failure = sign_operator_certificate(home.path(), PROJECT, 1_800_000_000).unwrap_err();
    assert!(failure.leftovers.is_empty(), "{:?}", failure.leftovers);
    assert_eq!(
        files_of(&dir),
        ["id_ed25519", "id_ed25519-cert.pub", "id_ed25519.pub"]
    );
    assert!(dir.join(CERT_FILE).is_dir());
}

#[test]
fn a_certificate_in_flight_is_never_part_of_one() {
    // The old certificate stays whole until the new one replaces it: after a failed signing (a key
    // that is not ed25519 cannot be signed), the certificate is as it was.
    let home = throwaway();
    make_ca(home.path());
    ensure_operator_key(home.path(), PROJECT).unwrap();
    let dir = dir_of(home.path());
    sign_operator_certificate(home.path(), PROJECT, 1_800_000_000).unwrap();
    let before = fs::read(dir.join(CERT_FILE)).unwrap();
    fs::write(dir.join(PUB_FILE), b"not a public key\n").unwrap();
    assert!(sign_operator_certificate(home.path(), PROJECT, 1_800_003_600).is_err());
    assert_eq!(fs::read(dir.join(CERT_FILE)).unwrap(), before);
    assert!(files_of(&dir).iter().all(|n| !n.starts_with("sign-")));
}
