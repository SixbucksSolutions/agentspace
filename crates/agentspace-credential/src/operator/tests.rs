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
fn ensure_names_the_file_that_sits_where_a_folder_belongs() {
    // Each case is a file at one level of `~/.agentspace/hosts/<project>`; a person has to move it,
    // so the message says so and does not say to try again.
    for (level, name) in [
        (".agentspace", "~/.agentspace"),
        (".agentspace/hosts", "~/.agentspace/hosts"),
        (
            ".agentspace/hosts/swimcoach",
            "~/.agentspace/hosts/swimcoach",
        ),
    ] {
        let home = throwaway();
        let file = home.path().join(level);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, b"a file").unwrap();
        let failure = ensure_operator_key(home.path(), PROJECT).unwrap_err();
        assert_eq!(
            failure.message,
            FOLDER_IS_FILE.replace("{}", name),
            "{level}"
        );
        assert!(failure.message.contains("A person has to move that file"));
        assert!(!failure.message.contains("try again"), "{level}");
        assert!(failure.leftovers.is_empty());
        assert_eq!(fs::read(&file).unwrap(), b"a file");
    }
}

/// Writes a key that needs a passphrase, made by `ssh-keygen` in the throwaway home, and returns
/// it as a key pair for [`install_key`].
fn passphrase_key(home: &Path) -> LeaseKey {
    let path = home.join("locked");
    let output = ssh_keygen_in(home)
        .args([
            "-q",
            "-t",
            "ed25519",
            "-N",
            "correct horse",
            "-C",
            "x",
            "-f",
        ])
        .arg(&path)
        .output()
        .expect("ssh-keygen could not be started");
    assert!(output.status.success(), "ssh-keygen could not make a key");
    let public = fs::read_to_string(path.with_extension("pub")).unwrap();
    LeaseKey {
        private_openssh: fs::read_to_string(&path).unwrap(),
        public_line: public.trim().to_string(),
    }
}

fn plain_file(path: &Path) -> Option<fs::File> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .ok()
}

fn no_file(_: &Path) -> Option<fs::File> {
    None
}

#[test]
fn a_new_key_that_needs_a_passphrase_is_not_kept() {
    let home = throwaway();
    let key = passphrase_key(home.path());
    let dir = dir_of(home.path());
    fs::create_dir_all(&dir).unwrap();
    let failure = install_key(PROJECT, &dir, &key, &REAL_RULES).unwrap_err();
    assert_eq!(failure.message, KEY_NOT_MADE);
    assert!(failure.leftovers.is_empty(), "{:?}", failure.leftovers);
    // No file is left behind.
    assert!(files_of(&dir).is_empty(), "{:?}", files_of(&dir));
}

#[test]
fn a_private_key_whose_rules_are_wrong_is_not_kept() {
    let home = throwaway();
    let dir = dir_of(home.path());
    fs::create_dir_all(&dir).unwrap();
    // A file made with the folder's inherited rules is not a lease private key's.
    let rules = KeyRules {
        create: plain_file,
        verify: platform::verify_private_key,
    };
    let key = new_key("agentspace host swimcoach").unwrap();
    let failure = install_key(PROJECT, &dir, &key, &rules).unwrap_err();
    assert_eq!(failure.message, KEY_UNPROTECTED);
    assert!(failure.leftovers.is_empty(), "{:?}", failure.leftovers);
    assert!(files_of(&dir).is_empty(), "{:?}", files_of(&dir));

    // A file that cannot be made with its rules in place writes no byte at all.
    let rules = KeyRules {
        create: no_file,
        verify: platform::verify_private_key,
    };
    let failure = install_key(PROJECT, &dir, &key, &rules).unwrap_err();
    assert_eq!(failure.message, KEY_UNPROTECTED);
    assert!(files_of(&dir).is_empty());

    // The real rules keep the same key.
    install_key(PROJECT, &dir, &key, &REAL_RULES).unwrap();
    assert_eq!(files_of(&dir), ["id_ed25519", "id_ed25519.pub"]);
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

/// `ssh-keygen`: the one on `PATH`, as the integration tests find it, and then the one Windows
/// ships. A missing one is a failure, never a pass.
fn ssh_keygen_command() -> Command {
    let on_path = Command::new("ssh-keygen")
        .arg("-?")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    if on_path.is_ok() {
        return Command::new("ssh-keygen");
    }
    let shipped = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .map(|root| root.join("System32/OpenSSH/ssh-keygen.exe"))
        .filter(|p| p.is_file());
    match shipped {
        Some(path) => Command::new(path),
        None => panic!("ssh-keygen could not be found on PATH or in Windows' OpenSSH folder"),
    }
}

/// `ssh-keygen` run in the throwaway home: every profile variable points into it (#0137), through
/// Command::env only, with a closed stdin and the time zone UTC.
fn ssh_keygen_in(home: &Path) -> Command {
    let local = home.join("AppData").join("Local");
    let roaming = home.join("AppData").join("Roaming");
    fs::create_dir_all(&local).unwrap();
    fs::create_dir_all(&roaming).unwrap();
    let mut command = ssh_keygen_command();
    command
        .env("USERPROFILE", home)
        .env("HOME", home)
        .env("LOCALAPPDATA", &local)
        .env("APPDATA", &roaming)
        .env("TZ", "UTC")
        .stdin(Stdio::null());
    command
}

/// A Unix time as `YYYY-MM-DDTHH:MM:SS` in UTC, the form `ssh-keygen -L` prints with `TZ=UTC`.
fn utc_text(unix: i64) -> String {
    let days = unix.div_euclid(86400);
    let secs = unix.rem_euclid(86400);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}",
        secs / 3600,
        secs % 3600 / 60,
        secs % 60
    )
}

#[test]
fn sign_reads_back_with_ssh_keygen() {
    let home = throwaway();
    make_ca(home.path());
    ensure_operator_key(home.path(), PROJECT).unwrap();
    let dir = dir_of(home.path());
    let now = 1_800_000_000;
    sign_operator_certificate(home.path(), PROJECT, now).unwrap();

    let output = ssh_keygen_in(home.path())
        .args(["-L", "-f"])
        .arg(dir.join(CERT_FILE))
        .output()
        .expect("ssh-keygen could not be started");
    assert!(output.status.success(), "ssh-keygen -L failed");
    let listing = String::from_utf8_lossy(&output.stdout).to_string();

    // A failure names the field that differed and shows the certificate's listing, never an
    // environment value.
    assert!(
        listing.contains("user certificate"),
        "Type differed:\n{listing}"
    );
    assert!(
        listing.contains("Key ID: \"operator-swimcoach\""),
        "Key ID differed:\n{listing}"
    );
    let principals: Vec<&str> = listing
        .lines()
        .skip_while(|l| !l.trim().starts_with("Principals:"))
        .skip(1)
        .map(str::trim)
        .take_while(|l| !l.is_empty() && !l.contains(':'))
        .collect();
    assert_eq!(principals, ["ubuntu"], "Principals differed:\n{listing}");
    let valid = format!(
        "Valid: from {} to {}",
        utc_text(now - 300),
        utc_text(now + 3600)
    );
    assert!(
        listing.lines().any(|l| l.trim() == valid),
        "Valid differed, wanted {valid}:\n{listing}"
    );
    for extension in ["permit-pty", "permit-port-forwarding"] {
        assert!(
            listing.contains(extension),
            "Extensions differed, no {extension}:\n{listing}"
        );
    }
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
    assert_eq!(files_of(&dir), ["id_ed25519"]);
    // The message names the public key and the remedy: restore it, or remove the private key so that
    // the next command makes a new pair.
    assert!(
        failure
            .message
            .contains("~/.agentspace/hosts/swimcoach/id_ed25519.pub")
    );
    assert!(failure.message.contains("A person restores it, or removes ~/.agentspace/hosts/swimcoach/id_ed25519 so that the next command makes a new key"));
    assert!(!failure.message.contains("-----"));
    // And the remedy works: with the private key removed, the next ensure makes a new pair.
    fs::remove_file(dir.join(KEY_FILE)).unwrap();
    assert_eq!(ensure_operator_key(home.path(), PROJECT), Ok(true));
    assert_eq!(files_of(&dir), ["id_ed25519", "id_ed25519.pub"]);
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
    // The wording is true with and without leftovers: the certificate in place is not changed.
    assert_eq!(failure.message, SIGN_FAILED);
    assert!(!failure.message.contains("Nothing was changed"));
    assert!(failure.message.contains("already in place was not changed"));
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
