//! Tests for the lease's key, certificate and CA checks (E0.05-T01-S04, #0097).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. The OpenSSH tool proofs MUST fail, never
//! skip, when `ssh-keygen` is missing. Every file lives in a throwaway folder under the temp
//! folder, never in `~/.ssh` or `~/.agentspace`.

use agentspace_credential::{
    CaProblem, CredError, LeaseKey, check_ca_keys, check_unencrypted, new_lease_key,
    sign_certificate,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const ID: &str = "0f9c2a1e-1234-4abc-8def-0123456789ab";
// 2027-01-15 08:00:00 UTC.
const ISSUE: i64 = 1_800_000_000;

// ---------------------------------------------------------------- helpers

fn ok<T>(r: Result<T, CredError>) -> T {
    match r {
        Ok(v) => v,
        Err(e) => panic!("Expected success, got the error: {}", e.0),
    }
}

fn err<T>(r: Result<T, CredError>) -> String {
    match r {
        Ok(_) => panic!("Expected an error, got success"),
        Err(e) => e.0,
    }
}

fn run(args: &[&str], envs: &[(&str, &str)]) -> (bool, String, String) {
    let mut cmd = Command::new("ssh-keygen");
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let out = cmd
        .output()
        .expect("ssh-keygen MUST be installed for these tests; it could not be run");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn restrict(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    }
    #[cfg(windows)]
    {
        // Windows OpenSSH refuses a private key whose folder's inherited access rules reach other
        // accounts, so the key keeps only an explicit grant to the current user.
        let domain = std::env::var("USERDOMAIN")
            .expect("USERDOMAIN MUST be set to restrict a key file's access");
        let user = std::env::var("USERNAME")
            .expect("USERNAME MUST be set to restrict a key file's access");
        let out = Command::new("icacls")
            .arg(path)
            .arg("/inheritance:r")
            .arg("/grant:r")
            .arg(format!("{domain}\\{user}:F"))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .expect("icacls MUST be available to restrict a key file's access");
        assert!(
            out.status.success(),
            "icacls could not restrict a key file: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

/// Makes a key pair with ssh-keygen. Returns (private path, public path).
fn keygen(dir: &Path, name: &str, kind: &str, passphrase: &str) -> (PathBuf, PathBuf) {
    let priv_path = dir.join(name);
    let (good, _, stderr) = run(
        &[
            "-q",
            "-t",
            kind,
            "-N",
            passphrase,
            "-C",
            "test-ca",
            "-f",
            priv_path.to_str().unwrap(),
        ],
        &[],
    );
    assert!(good, "ssh-keygen could not make a key: {stderr}");
    let pub_path = dir.join(format!("{name}.pub"));
    (priv_path, pub_path)
}

fn words(line: &str) -> Vec<&str> {
    line.split_whitespace().collect()
}

fn first_two(line: &str) -> String {
    let w = words(line);
    format!("{} {}", w[0], w[1])
}

fn crlf(bytes: &[u8]) -> Vec<u8> {
    let s = String::from_utf8(bytes.to_vec()).unwrap();
    s.replace("\r\n", "\n").replace('\n', "\r\n").into_bytes()
}

fn write_key(dir: &Path, name: &str, key: &LeaseKey) -> PathBuf {
    let p = dir.join(name);
    fs::write(&p, &key.private_openssh).unwrap();
    restrict(&p);
    p
}

fn fingerprint(pub_file: &Path) -> String {
    let (good, out, err) = run(&["-l", "-f", pub_file.to_str().unwrap()], &[]);
    assert!(good, "ssh-keygen -l failed: {err}");
    out.split_whitespace().nth(1).unwrap().to_string()
}

fn describe(cert_file: &Path) -> String {
    let (good, out, err) = run(&["-L", "-f", cert_file.to_str().unwrap()], &[("TZ", "UTC")]);
    assert!(good, "ssh-keygen -L refused the certificate: {err}");
    out
}

fn value_of<'a>(listing: &'a str, label: &str) -> &'a str {
    listing
        .lines()
        .map(str::trim)
        .find_map(|l| l.strip_prefix(label))
        .unwrap_or_else(|| panic!("No `{label}` line in:\n{listing}"))
        .trim()
}

/// The lines that follow a heading (Principals, Extensions), up to the next heading.
fn list_under(listing: &str, heading: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut inside = false;
    for line in listing.lines() {
        let t = line.trim();
        if inside {
            if line.starts_with("                ") {
                out.push(t.to_string());
                continue;
            }
            break;
        }
        if let Some(rest) = t.strip_prefix(heading) {
            inside = true;
            let rest = rest.trim();
            if !rest.is_empty() {
                out.push(rest.to_string());
            }
        }
    }
    out
}

struct Signed {
    _dir: tempfile::TempDir,
    cert_line: String,
    cert_file: PathBuf,
    lease_pub: PathBuf,
    ca_pub: PathBuf,
}

fn sign_with_fresh_keys(user: &str, issue: i64, ttl: u32) -> Signed {
    let dir = tempfile::tempdir().unwrap();
    let (ca_priv, ca_pub) = keygen(dir.path(), "ca", "ed25519", "");
    let key = ok(new_lease_key(ID));
    let lease_pub = dir.path().join("lease.pub");
    fs::write(&lease_pub, &key.public_line).unwrap();
    let cert_line = ok(sign_certificate(
        &fs::read(&ca_priv).unwrap(),
        key.public_line.as_bytes(),
        ID,
        user,
        issue,
        ttl,
    ));
    let cert_file = dir.path().join("lease-cert.pub");
    fs::write(&cert_file, format!("{cert_line}\n")).unwrap();
    Signed {
        _dir: dir,
        cert_line,
        cert_file,
        lease_pub,
        ca_pub,
    }
}

// ---------------------------------------------------------------- new_lease_key

#[test]
fn new_lease_key_is_accepted_by_ssh_keygen_without_a_prompt() {
    let dir = tempfile::tempdir().unwrap();
    let key = ok(new_lease_key(ID));
    let path = write_key(dir.path(), "id_ed25519", &key);
    // An empty passphrase and a closed stdin: a key that needed one fails rather than prompts.
    let (good, out, stderr) = run(&["-y", "-P", "", "-f", path.to_str().unwrap()], &[]);
    assert!(good, "ssh-keygen -y refused the key: {stderr}");
    assert_eq!(first_two(&out), first_two(&key.public_line));
}

#[test]
fn new_lease_key_formats_and_comment() {
    let key = ok(new_lease_key(ID));
    assert!(
        key.private_openssh
            .starts_with("-----BEGIN OPENSSH PRIVATE KEY-----"),
        "The private key MUST be in OpenSSH's private key format"
    );
    assert!(
        key.private_openssh
            .trim_end()
            .ends_with("-----END OPENSSH PRIVATE KEY-----")
    );
    assert!(
        !key.public_line.ends_with(['\r', '\n']),
        "The public line MUST have no line ending"
    );
    assert!(
        !key.public_line.contains(['\r', '\n']),
        "The public key MUST be one line"
    );
    let line = key.public_line.as_str();
    let w = words(line);
    assert_eq!(w[0], "ssh-ed25519");
    assert_eq!(
        w[2..].join(" "),
        format!("agentspace lease {ID}"),
        "The comment MUST be `agentspace lease <id>`"
    );
}

#[test]
fn new_lease_key_is_ed25519_per_ssh_keygen() {
    let dir = tempfile::tempdir().unwrap();
    let key = ok(new_lease_key(ID));
    let pub_path = dir.path().join("k.pub");
    fs::write(&pub_path, &key.public_line).unwrap();
    let (good, out, stderr) = run(&["-l", "-f", pub_path.to_str().unwrap()], &[]);
    assert!(good, "ssh-keygen -l refused the public key: {stderr}");
    assert!(out.contains("(ED25519)"), "Not ed25519: {out}");
    assert!(out.contains(&format!("agentspace lease {ID}")), "{out}");
}

#[test]
fn new_lease_key_private_key_carries_the_comment() {
    // ssh-keygen -y prints the private key's own comment on the public line it derives.
    let dir = tempfile::tempdir().unwrap();
    let key = ok(new_lease_key(ID));
    let path = write_key(dir.path(), "id", &key);
    let (good, out, stderr) = run(&["-y", "-P", "", "-f", path.to_str().unwrap()], &[]);
    assert!(good, "{stderr}");
    assert_eq!(
        words(&out)[2..].join(" "),
        format!("agentspace lease {ID}"),
        "The private key's comment MUST be `agentspace lease <id>`"
    );
}

#[test]
fn new_lease_key_makes_a_different_key_each_time() {
    let a = ok(new_lease_key(ID));
    let b = ok(new_lease_key(ID));
    assert_ne!(first_two(&a.public_line), first_two(&b.public_line));
    assert_ne!(a.private_openssh, b.private_openssh);
}

#[test]
fn new_lease_key_is_unencrypted_by_its_own_check() {
    let key = ok(new_lease_key(ID));
    ok(check_unencrypted(key.private_openssh.as_bytes()));
}

#[test]
fn lease_key_debug_holds_no_private_key_body() {
    let key = ok(new_lease_key(ID));
    let shown = format!("{key:?}");
    assert!(shown.contains(&key.public_line), "{shown}");
    for line in key
        .private_openssh
        .lines()
        .filter(|l| !l.starts_with("-----"))
    {
        assert!(
            line.len() >= 12 && !shown.contains(line),
            "Debug MUST NOT show the private key: {shown}"
        );
    }
    assert!(!shown.contains("BEGIN OPENSSH PRIVATE KEY"), "{shown}");
}

#[test]
fn a_line_break_in_the_id_is_refused_without_echoing_it() {
    let dir = tempfile::tempdir().unwrap();
    let (ca_priv, _) = keygen(dir.path(), "ca", "ed25519", "");
    let ca = fs::read(ca_priv).unwrap();
    let key = ok(new_lease_key(ID));
    for bad in [
        "leak-one\nleak-two",
        "leak-one\rleak-two",
        "leak-one\r\nleak-two",
    ] {
        let m = err(new_lease_key(bad));
        assert!(!m.is_empty() && !m.contains("leak"), "ID echoed: {m}");
        let m = err(sign_certificate(
            &ca,
            key.public_line.as_bytes(),
            bad,
            "ubuntu",
            ISSUE,
            3600,
        ));
        assert!(!m.is_empty() && !m.contains("leak"), "ID echoed: {m}");
    }
}

// ---------------------------------------------------------------- check_unencrypted

#[test]
fn check_unencrypted_accepts_lf_and_crlf_forms() {
    let key = ok(new_lease_key(ID));
    let lf = key.private_openssh.replace("\r\n", "\n");
    assert!(!lf.contains('\r'));
    ok(check_unencrypted(lf.as_bytes()));
    let crlf_form = crlf(lf.as_bytes());
    assert!(String::from_utf8_lossy(&crlf_form).contains("\r\n"));
    ok(check_unencrypted(&crlf_form));
}

#[test]
fn check_unencrypted_accepts_an_ssh_keygen_key() {
    let dir = tempfile::tempdir().unwrap();
    let (priv_path, _) = keygen(dir.path(), "plain", "ed25519", "");
    ok(check_unencrypted(&fs::read(priv_path).unwrap()));
}

#[test]
fn check_unencrypted_refuses_a_key_with_a_passphrase() {
    let dir = tempfile::tempdir().unwrap();
    let (priv_path, _) = keygen(dir.path(), "locked", "ed25519", "correct horse");
    let bytes = fs::read(priv_path).unwrap();
    let msg = err(check_unencrypted(&bytes));
    assert!(!msg.is_empty());
    let msg = err(check_unencrypted(&crlf(&bytes)));
    assert!(!msg.is_empty());
}

#[test]
fn check_unencrypted_never_panics_on_garbage() {
    let key = ok(new_lease_key(ID));
    let good = key.private_openssh.clone().into_bytes();
    let mut cases: Vec<Vec<u8>> = vec![
        Vec::new(),
        b"\n".to_vec(),
        b"hello".to_vec(),
        vec![0xff, 0xfe, 0x00, 0x80],
        b"-----BEGIN OPENSSH PRIVATE KEY-----".to_vec(),
        b"-----BEGIN OPENSSH PRIVATE KEY-----\n!!!!\n-----END OPENSSH PRIVATE KEY-----\n".to_vec(),
        key.public_line.clone().into_bytes(),
        vec![b'A'; 100_000],
    ];
    for n in [10, 40, 80, good.len() / 2, good.len() - 5] {
        cases.push(good[..n].to_vec());
    }
    for bytes in cases {
        // Garbage is refused. It MUST NOT panic and MUST NOT be accepted.
        let msg = err(check_unencrypted(&bytes));
        assert!(!msg.is_empty());
    }
}

// ---------------------------------------------------------------- sign_certificate

#[test]
fn certificate_shows_every_field_in_ssh_keygen() {
    let s = sign_with_fresh_keys("ubuntu", ISSUE, 3600);
    let listing = describe(&s.cert_file);

    assert!(
        listing.contains("ssh-ed25519-cert-v01@openssh.com user certificate"),
        "Type: {listing}"
    );
    assert_eq!(value_of(&listing, "Serial:"), "0");
    assert_eq!(value_of(&listing, "Key ID:"), format!("\"{ID}\""));
    assert_eq!(
        value_of(&listing, "Valid:"),
        "from 2027-01-15T07:55:00 to 2027-01-15T09:00:00",
        "Valid from five minutes before the issue time to the issue time plus the TTL, in UTC"
    );
    assert_eq!(list_under(&listing, "Principals:"), vec!["ubuntu"]);
    assert!(
        value_of(&listing, "Critical Options:").contains("(none)"),
        "No critical options: {listing}"
    );
    let mut ext = list_under(&listing, "Extensions:");
    ext.sort();
    assert_eq!(ext, vec!["permit-port-forwarding", "permit-pty"]);
    assert!(
        value_of(&listing, "Signing CA:").contains("ED25519"),
        "{listing}"
    );
    assert!(
        value_of(&listing, "Signing CA:").contains("(using ssh-ed25519)"),
        "The signature MUST be ssh-ed25519: {listing}"
    );
}

#[test]
fn certificate_names_the_lease_key_and_the_ca() {
    let s = sign_with_fresh_keys("ubuntu", ISSUE, 3600);
    let listing = describe(&s.cert_file);
    let lease_fp = fingerprint(&s.lease_pub);
    let ca_fp = fingerprint(&s.ca_pub);
    assert!(
        value_of(&listing, "Public key:").contains(&lease_fp),
        "Certificate is not for the lease's key: {listing}"
    );
    assert!(
        value_of(&listing, "Signing CA:").contains(&ca_fp),
        "Certificate is not signed by the CA: {listing}"
    );
}

#[test]
fn certificate_window_follows_issue_time_and_ttl_exactly() {
    // 1 minute, 24 hours, and an odd number of seconds, each against a different issue time.
    let cases: [(i64, u32, &str); 3] = [
        (ISSUE, 60, "from 2027-01-15T07:55:00 to 2027-01-15T08:01:00"),
        (
            ISSUE,
            86_400,
            "from 2027-01-15T07:55:00 to 2027-01-16T08:00:00",
        ),
        (
            ISSUE + 17,
            3_723,
            "from 2027-01-15T07:55:17 to 2027-01-15T09:02:20",
        ),
    ];
    for (issue, ttl, want) in cases {
        let s = sign_with_fresh_keys("ubuntu", issue, ttl);
        assert_eq!(value_of(&describe(&s.cert_file), "Valid:"), want);
    }
}

#[test]
fn certificate_window_is_absolute_and_not_in_the_signers_zone() {
    // The same certificate read in two zones differs only by the reader's zone, and the crate
    // MUST NOT have used local time: ssh-keygen -L under UTC shows the exact UTC window above.
    let s = sign_with_fresh_keys("ubuntu", ISSUE, 3600);
    let (good, out, stderr) = run(
        &["-L", "-f", s.cert_file.to_str().unwrap()],
        &[("TZ", "NZT-13")],
    );
    assert!(good, "{stderr}");
    // The POSIX zone NZT-13 is UTC+13, as Auckland is in January: 07:55 UTC is 20:55 local.
    assert_eq!(
        value_of(&out, "Valid:"),
        "from 2027-01-15T20:55:00 to 2027-01-15T22:00:00"
    );
}

#[test]
fn certificate_uses_the_given_user_as_its_only_principal() {
    let s = sign_with_fresh_keys("agent-7", ISSUE, 3600);
    let listing = describe(&s.cert_file);
    assert_eq!(list_under(&listing, "Principals:"), vec!["agent-7"]);
}

#[test]
fn certificate_line_has_no_line_ending_and_ends_with_the_comment() {
    let s = sign_with_fresh_keys("ubuntu", ISSUE, 3600);
    assert!(!s.cert_line.contains('\n'), "Line ending found");
    assert!(!s.cert_line.contains('\r'), "Line ending found");
    assert!(s.cert_line.starts_with("ssh-ed25519-cert-v01@openssh.com "));
    let w = words(&s.cert_line);
    assert_eq!(
        w[2..].join(" "),
        format!("agentspace lease {ID}"),
        "The comment MUST be `agentspace lease <id>`"
    );
    assert!(s.cert_line.ends_with(&format!("agentspace lease {ID}")));
}

#[test]
fn sign_accepts_crlf_in_the_ca_key_and_the_public_key() {
    let dir = tempfile::tempdir().unwrap();
    let (ca_priv, _) = keygen(dir.path(), "ca", "ed25519", "");
    let key = ok(new_lease_key(ID));
    let ca_bytes = crlf(&fs::read(ca_priv).unwrap());
    let pub_bytes = format!("{}\r\n", key.public_line.trim_end()).into_bytes();
    let line = ok(sign_certificate(
        &ca_bytes, &pub_bytes, ID, "ubuntu", ISSUE, 3600,
    ));
    assert!(!line.contains('\n') && !line.contains('\r'));
    let cert = dir.path().join("c-cert.pub");
    fs::write(&cert, format!("{line}\n")).unwrap();
    assert!(describe(&cert).contains("user certificate"));
}

#[test]
fn sign_refuses_an_encrypted_ca() {
    let dir = tempfile::tempdir().unwrap();
    let (ca_priv, _) = keygen(dir.path(), "ca", "ed25519", "secret pass");
    let key = ok(new_lease_key(ID));
    let msg = err(sign_certificate(
        &fs::read(ca_priv).unwrap(),
        key.public_line.as_bytes(),
        ID,
        "ubuntu",
        ISSUE,
        3600,
    ));
    assert!(!msg.is_empty());
}

#[test]
fn sign_refuses_garbage_without_panicking() {
    let dir = tempfile::tempdir().unwrap();
    let (ca_priv, _) = keygen(dir.path(), "ca", "ed25519", "");
    let ca = fs::read(ca_priv).unwrap();
    let key = ok(new_lease_key(ID));
    let public = key.public_line.as_bytes();
    let junk: [&[u8]; 4] = [b"", b"junk", &[0xff, 0x00, 0xfe], b"ssh-ed25519 AAAA"];
    for bad in junk {
        assert!(!err(sign_certificate(bad, public, ID, "ubuntu", ISSUE, 3600)).is_empty());
        assert!(!err(sign_certificate(&ca, bad, ID, "ubuntu", ISSUE, 3600)).is_empty());
    }
}

#[test]
fn certificate_with_a_ttl_of_zero_is_valid_from_issue_minus_300_to_issue() {
    // 2026-11-25 08:00:00 UTC.
    let issue = 1_795_593_600;
    let s = sign_with_fresh_keys("ubuntu", issue, 0);
    assert_eq!(
        value_of(&describe(&s.cert_file), "Valid:"),
        "from 2026-11-25T07:55:00 to 2026-11-25T08:00:00"
    );
}

#[test]
fn sign_refuses_a_negative_issue_time() {
    let dir = tempfile::tempdir().unwrap();
    let (ca_priv, _) = keygen(dir.path(), "ca", "ed25519", "");
    let ca = fs::read(ca_priv).unwrap();
    let key = ok(new_lease_key(ID));
    for issue in [-1, i64::MIN] {
        let m = err(sign_certificate(
            &ca,
            key.public_line.as_bytes(),
            ID,
            "ubuntu",
            issue,
            3600,
        ));
        assert!(!m.is_empty());
    }
}

#[test]
fn certificate_decoded_has_empty_extension_data_and_no_critical_options() {
    let s = sign_with_fresh_keys("ubuntu", ISSUE, 3600);
    let cert = ssh_key::Certificate::from_openssh(&s.cert_line).unwrap();
    assert!(cert.critical_options().is_empty(), "A critical option");
    let ext: Vec<(&String, &String)> = cert.extensions().iter().collect();
    assert_eq!(ext.len(), 2, "{ext:?}");
    for (name, data) in &ext {
        assert!(data.is_empty(), "Extension {name} MUST have empty data");
    }
    let names: Vec<&str> = ext.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, vec!["permit-port-forwarding", "permit-pty"]);
}

// ---------------------------------------------------------------- check_ca_keys

#[test]
fn ca_made_by_ssh_keygen_passes() {
    let dir = tempfile::tempdir().unwrap();
    let (p, q) = keygen(dir.path(), "ca", "ed25519", "");
    let private = fs::read(p).unwrap();
    let public = fs::read(q).unwrap();
    assert!(check_ca_keys(&private, &public).is_ok());
}

#[test]
fn ca_accepts_crlf_and_any_comment_on_the_public_key() {
    let dir = tempfile::tempdir().unwrap();
    let (p, q) = keygen(dir.path(), "ca", "ed25519", "");
    let private = fs::read(p).unwrap();
    let public = fs::read_to_string(q).unwrap();
    let w = words(&public);
    let other_comment = format!(
        "{} {} agentspace-ca <computer name> 2026-11-18\r\n",
        w[0], w[1]
    );
    assert!(check_ca_keys(&crlf(&private), other_comment.as_bytes()).is_ok());
    let bare = format!("  {} {}  \n", w[0], w[1]);
    assert!(check_ca_keys(&private, bare.as_bytes()).is_ok());
}

#[test]
fn ca_with_a_passphrase_fails_encrypted() {
    let dir = tempfile::tempdir().unwrap();
    let (p, q) = keygen(dir.path(), "ca", "ed25519", "a passphrase");
    let r = check_ca_keys(&fs::read(p).unwrap(), &fs::read(q).unwrap());
    assert!(matches!(r, Err(CaProblem::Encrypted)));
}

#[test]
fn ca_with_a_swapped_public_key_fails_mismatch() {
    let dir = tempfile::tempdir().unwrap();
    let (p1, _) = keygen(dir.path(), "ca1", "ed25519", "");
    let (_, q2) = keygen(dir.path(), "ca2", "ed25519", "");
    let r = check_ca_keys(&fs::read(p1).unwrap(), &fs::read(q2).unwrap());
    assert!(matches!(r, Err(CaProblem::Mismatch)));
}

#[test]
fn ca_rsa_pair_fails_not_ed25519() {
    let dir = tempfile::tempdir().unwrap();
    let (p, q) = keygen(dir.path(), "rsa", "rsa", "");
    let r = check_ca_keys(&fs::read(p).unwrap(), &fs::read(q).unwrap());
    assert!(matches!(r, Err(CaProblem::NotEd25519)));
}

#[test]
fn ca_ecdsa_pair_fails_not_ed25519() {
    let dir = tempfile::tempdir().unwrap();
    let (p, q) = keygen(dir.path(), "ec", "ecdsa", "");
    let r = check_ca_keys(&fs::read(p).unwrap(), &fs::read(q).unwrap());
    assert!(matches!(r, Err(CaProblem::NotEd25519)));
}

#[test]
fn ca_with_a_truncated_public_key_fails_unreadable() {
    let dir = tempfile::tempdir().unwrap();
    let (p, q) = keygen(dir.path(), "ca", "ed25519", "");
    let private = fs::read(p).unwrap();
    let public = fs::read_to_string(q).unwrap();
    let truncated = &public[..30];
    assert!(truncated.starts_with("ssh-ed25519 AAAA"));
    let r = check_ca_keys(&private, truncated.as_bytes());
    assert!(matches!(r, Err(CaProblem::Unreadable)));
    for junk in [
        &b""[..],
        b"\n",
        b"ssh-ed25519",
        b"ssh-ed25519 !!!!",
        &[0xff, 0xfe],
    ] {
        let r = check_ca_keys(&private, junk);
        assert!(matches!(r, Err(CaProblem::Unreadable)));
    }
}

#[test]
fn ca_with_an_unreadable_private_key_never_panics() {
    let dir = tempfile::tempdir().unwrap();
    let (_, q) = keygen(dir.path(), "ca", "ed25519", "");
    let public = fs::read(q).unwrap();
    for junk in [
        &b""[..],
        b"junk",
        &[0xff, 0x00],
        b"-----BEGIN OPENSSH PRIVATE KEY-----\n",
    ] {
        assert!(check_ca_keys(junk, &public).is_err());
    }
}

#[test]
fn ca_rsa_private_key_with_an_ed25519_public_key_fails_not_ed25519() {
    let dir = tempfile::tempdir().unwrap();
    let (rsa_priv, _) = keygen(dir.path(), "rsa", "rsa", "");
    let (_, ed_pub) = keygen(dir.path(), "ed", "ed25519", "");
    let r = check_ca_keys(&fs::read(rsa_priv).unwrap(), &fs::read(ed_pub).unwrap());
    assert_eq!(r, Err(CaProblem::NotEd25519));
}

#[test]
fn ca_checks_run_in_the_rulings_order() {
    let dir = tempfile::tempdir().unwrap();
    let (rsa_priv, _) = keygen(dir.path(), "rsa", "rsa", "");
    let (rsa_enc, rsa_enc_pub) = keygen(dir.path(), "rsaenc", "rsa", "a passphrase");
    let (ed_priv, _) = keygen(dir.path(), "ed", "ed25519", "");
    let (ed_enc, _) = keygen(dir.path(), "edenc", "ed25519", "a passphrase");
    let rsa_priv = fs::read(rsa_priv).unwrap();
    let rsa_enc = fs::read(rsa_enc).unwrap();
    let rsa_enc_pub = fs::read(rsa_enc_pub).unwrap();
    let ed_priv = fs::read(ed_priv).unwrap();
    let ed_enc = fs::read(ed_enc).unwrap();

    // Encrypted comes before the key's algorithm and before the public key is read.
    assert_eq!(
        check_ca_keys(&rsa_enc, &rsa_enc_pub),
        Err(CaProblem::Encrypted)
    );
    assert_eq!(check_ca_keys(&ed_enc, b"junk"), Err(CaProblem::Encrypted));
    // A public key under two words is Unreadable before the private key's algorithm is judged.
    assert_eq!(
        check_ca_keys(&rsa_priv, b"ssh-ed25519"),
        Err(CaProblem::Unreadable)
    );
    // A wrong first word is NotEd25519 before the public key is parsed.
    assert_eq!(
        check_ca_keys(&ed_priv, b"ssh-rsa AAAAB3"),
        Err(CaProblem::NotEd25519)
    );
}

#[test]
fn ca_public_key_comparison_is_exact_not_case_folded() {
    let dir = tempfile::tempdir().unwrap();
    let (p, q) = keygen(dir.path(), "ca", "ed25519", "");
    let private = fs::read(p).unwrap();
    let public = fs::read_to_string(q).unwrap();
    let w = words(&public);
    assert_eq!(check_ca_keys(&private, public.as_bytes()), Ok(()));

    // The first word's case is part of the check.
    let upper_kind = format!("SSH-ED25519 {}", w[1]);
    assert_eq!(
        check_ca_keys(&private, upper_kind.as_bytes()),
        Err(CaProblem::NotEd25519)
    );

    // One letter of the blob in the other case, from the end, at the first position where the
    // result still parses as a key: a different key text, so a Mismatch.
    let blob: Vec<char> = w[1].chars().collect();
    let mut tried = 0;
    for i in (0..blob.len())
        .rev()
        .filter(|i| blob[*i].is_ascii_alphabetic())
    {
        let mut flipped = blob.clone();
        flipped[i] = if flipped[i].is_ascii_lowercase() {
            flipped[i].to_ascii_uppercase()
        } else {
            flipped[i].to_ascii_lowercase()
        };
        let line = format!("ssh-ed25519 {}", flipped.iter().collect::<String>());
        if ssh_key::PublicKey::from_openssh(&line).is_err() {
            continue;
        }
        tried += 1;
        assert_eq!(
            check_ca_keys(&private, line.as_bytes()),
            Err(CaProblem::Mismatch),
            "A blob that differs in the case of one letter MUST be a Mismatch"
        );
        break;
    }
    assert_eq!(tried, 1, "No case-flipped blob parsed, so none was tested");
}

// ---------------------------------------------------------------- messages

#[test]
fn error_messages_hold_no_key_material_and_no_path() {
    let dir = tempfile::tempdir().unwrap();
    let dir_text = dir.path().to_string_lossy().into_owned();
    let (locked_priv, _) = keygen(dir.path(), "locked", "ed25519", "pass phrase");
    let (ca_priv, _) = keygen(dir.path(), "ca", "ed25519", "");
    let locked = fs::read_to_string(&locked_priv).unwrap();
    let ca = fs::read_to_string(&ca_priv).unwrap();
    let key = ok(new_lease_key(ID));
    let public_b64 = words(&key.public_line)[1].to_string();

    // Pieces of key material that MUST NOT appear: bodies of the keys involved.
    let mut secrets: Vec<String> = vec![public_b64.clone()];
    for text in [&locked, &ca, &key.private_openssh] {
        for line in text.lines().filter(|l| !l.starts_with("-----")) {
            secrets.push(line.trim().to_string());
        }
    }

    let mut messages = vec![
        err(check_unencrypted(locked.as_bytes())),
        err(check_unencrypted(
            b"junk with the secret-looking text AAAAC3NzaC1lZDI1NTE5",
        )),
        err(sign_certificate(
            locked.as_bytes(),
            key.public_line.as_bytes(),
            ID,
            "ubuntu",
            ISSUE,
            3600,
        )),
        err(sign_certificate(
            ca.as_bytes(),
            b"ssh-ed25519 notbase64!!",
            ID,
            "ubuntu",
            ISSUE,
            3600,
        )),
        err(sign_certificate(
            b"junk",
            key.public_line.as_bytes(),
            ID,
            "ubuntu",
            ISSUE,
            3600,
        )),
    ];
    messages.push(err(check_unencrypted(&[])));

    for m in &messages {
        assert!(!m.is_empty());
        assert!(
            m.chars().next().unwrap().is_uppercase(),
            "A message starts with a capital: {m}"
        );
        assert!(!m.contains(&dir_text), "Path in message: {m}");
        assert!(!m.contains("/tmp"), "Path in message: {m}");
        assert!(!m.contains("BEGIN") && !m.contains("PRIVATE KEY"), "{m}");
        assert!(!m.contains("ssh-ed25519"), "{m}");
        assert!(!m.contains("AAAAC3Nza"), "Key material in message: {m}");
        for s in &secrets {
            if s.len() >= 12 {
                assert!(!m.contains(s.as_str()), "Key material in message: {m}");
            }
        }
    }
}
