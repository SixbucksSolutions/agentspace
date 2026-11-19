//! agentspace-credential (docs/decisions.md, D38).
//!
//! A lease's key and certificate, and the checks of agentspace's CA, on bytes only. This crate
//! does no file I/O (E0.05-T02-S02 adds the files and the access rules).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. The rules are those of state.md, "A
//! lease's credential" and "The CA":
//!
//! - Keys and the certificate MUST be made in this program's own code, never by running
//!   `ssh-keygen`, and the random source MUST be the operating system's.
//! - A [`CredError`] message MUST be in agentspace's own words. It MUST NOT carry key material, a
//!   secret, or a library's error text.
//! - A function here MUST NOT panic on bad input, and MUST NOT write to stdout or stderr.

use ssh_key::certificate::{Builder, CertType};
use ssh_key::rand_core::OsRng;
use ssh_key::{Algorithm, Cipher, LineEnding, PrivateKey, PublicKey};

/// The first word of an ed25519 public key line, and of its certificate's signature.
const ED25519_WORD: &str = "ssh-ed25519";

/// The back-dating of a certificate's valid-after time, in seconds (D24, #0029).
const BACKDATE_SECONDS: u64 = 300;

/// An error in agentspace's own words.
///
/// The message MUST NOT hold key material, a secret, or another program's or library's error text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CredError(pub String);

impl std::fmt::Display for CredError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for CredError {}

fn err(message: &str) -> CredError {
    CredError(message.to_string())
}

/// A new lease key pair, in OpenSSH's formats.
///
/// `Debug` shows the public line only, so that the private key cannot reach a log by accident.
#[derive(Clone, PartialEq, Eq)]
pub struct LeaseKey {
    /// The private key in OpenSSH's private key format, unencrypted, with LF line endings.
    pub private_openssh: String,
    /// The public key as one line, `ssh-ed25519 <base64> agentspace lease <id>`, with no line
    /// ending.
    pub public_line: String,
}

impl std::fmt::Debug for LeaseKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LeaseKey")
            .field("private_openssh", &"<redacted>")
            .field("public_line", &self.public_line)
            .finish()
    }
}

/// Why a CA is not usable (state.md, The CA, Its checks 3 to 5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaProblem {
    /// The private key has a passphrase: its cipher or its KDF is not `none` (check 3).
    Encrypted,
    /// The public key's first two words differ from those derived from the private key (check 4).
    Mismatch,
    /// The key is not ed25519 (check 4).
    NotEd25519,
    /// A file is not an OpenSSH key at all (checks 3 and 5).
    Unreadable,
}

/// Whether `text` may stand in a one-line comment, a key ID or a principal: no control character,
/// so no line break.
fn is_one_line(text: &str) -> bool {
    !text.chars().any(char::is_control)
}

/// Makes a key's or a certificate's text from bytes, accepting LF or CR LF line endings.
fn normalized(bytes: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(bytes).ok()?;
    Some(text.replace("\r\n", "\n"))
}

/// Parses an OpenSSH private key, in whatever state of encryption, from bytes.
fn parse_private(private: &[u8]) -> Result<PrivateKey, ssh_key::Error> {
    match normalized(private) {
        Some(text) => PrivateKey::from_openssh(text.as_bytes()),
        None => Err(ssh_key::Error::FormatEncoding),
    }
}

/// Whether a private key has a passphrase: its cipher or its KDF is not `none`.
fn has_passphrase(key: &PrivateKey) -> bool {
    key.cipher() != Cipher::None || key.kdf().is_some() || key.is_encrypted()
}

/// Makes a new ed25519 key pair for the lease `id`, with the comment `agentspace lease <id>`.
///
/// The private key is unencrypted. The random source is the operating system's.
pub fn new_lease_key(id: &str) -> Result<LeaseKey, CredError> {
    if id.is_empty() || !is_one_line(id) {
        return Err(err("The lease's ID cannot be used as a key's comment."));
    }
    let mut key = PrivateKey::random(&mut OsRng, Algorithm::Ed25519)
        .map_err(|_| err("A key could not be made."))?;
    key.set_comment(format!("agentspace lease {id}"));
    let private = key
        .to_openssh(LineEnding::LF)
        .map_err(|_| err("A key could not be written."))?;
    let public_line = key
        .public_key()
        .to_openssh()
        .map_err(|_| err("A key could not be written."))?;
    Ok(LeaseKey {
        private_openssh: private.to_string(),
        public_line,
    })
}

/// Checks that a private key is unencrypted: its cipher and its KDF are both `none`.
///
/// Accepts LF or CR LF line endings.
pub fn check_unencrypted(private: &[u8]) -> Result<(), CredError> {
    let key = parse_private(private).map_err(|_| err("The private key could not be read."))?;
    if has_passphrase(&key) {
        return Err(err("The private key has a passphrase."));
    }
    Ok(())
}

/// Signs a lease's certificate with the CA's private key, and returns its line, without a line
/// ending: `ssh-ed25519-cert-v01@openssh.com <base64> agentspace lease <id>`.
///
/// The fields are exactly state.md's table: a user certificate, serial 0, the key ID `id`, the one
/// principal `user`, valid from 5 minutes before `issue_unix` (or from 0, when that is sooner) to
/// `issue_unix + ttl_seconds`, no critical options, and the extensions `permit-port-forwarding`
/// and `permit-pty`, each with empty data.
pub fn sign_certificate(
    ca_private: &[u8],
    lease_public: &[u8],
    id: &str,
    user: &str,
    issue_unix: i64,
    ttl_seconds: u32,
) -> Result<String, CredError> {
    if id.is_empty() || !is_one_line(id) {
        return Err(err("The lease's ID cannot be used in a certificate."));
    }
    if user.is_empty() || !is_one_line(user) {
        return Err(err("The user cannot be used in a certificate."));
    }
    let issue = u64::try_from(issue_unix).map_err(|_| err("The issue time is not valid."))?;
    let valid_before = issue
        .checked_add(u64::from(ttl_seconds))
        .filter(|t| i64::try_from(*t).is_ok())
        .ok_or_else(|| err("The issue time and the TTL are not valid together."))?;
    let valid_after = issue.saturating_sub(BACKDATE_SECONDS);

    let ca =
        parse_private(ca_private).map_err(|_| err("The CA's private key could not be read."))?;
    if has_passphrase(&ca) {
        return Err(err("The CA's private key has a passphrase."));
    }
    if ca.algorithm() != Algorithm::Ed25519 {
        return Err(err("The CA's key is not an ed25519 key."));
    }

    let public_text =
        normalized(lease_public).ok_or_else(|| err("The lease's public key could not be read."))?;
    let public = PublicKey::from_openssh(public_text.trim())
        .map_err(|_| err("The lease's public key could not be read."))?;
    if public.algorithm() != Algorithm::Ed25519 {
        return Err(err("The lease's key is not an ed25519 key."));
    }

    let build = |_: ssh_key::Error| err("The certificate could not be made.");
    let mut builder = Builder::new_with_random_nonce(
        &mut OsRng,
        public.key_data().clone(),
        valid_after,
        valid_before,
    )
    .map_err(build)?;
    builder.serial(0).map_err(build)?;
    builder.cert_type(CertType::User).map_err(build)?;
    builder.key_id(id).map_err(build)?;
    builder.valid_principal(user).map_err(build)?;
    builder
        .extension("permit-port-forwarding", "")
        .map_err(build)?;
    builder.extension("permit-pty", "").map_err(build)?;
    builder
        .comment(format!("agentspace lease {id}"))
        .map_err(build)?;
    let certificate = builder
        .sign(&ca)
        .map_err(|_| err("The certificate could not be signed."))?;
    let line = certificate
        .to_openssh()
        .map_err(|_| err("The certificate could not be written."))?;
    if line.contains(['\r', '\n']) {
        return Err(err("The certificate could not be written."));
    }
    Ok(line)
}

/// The first two whitespace-separated words of a trimmed line.
fn first_two_words(text: &str) -> Option<(&str, &str)> {
    let mut words = text.split_whitespace();
    Some((words.next()?, words.next()?))
}

/// Checks 3 to 5 of state.md's "The CA", "Its checks", on the two files' bytes.
///
/// Checks 1 and 2 (the files exist, and are owner-only) are not made here. The architect's rulings
/// of 2026-11-19 set the order, and the first failure ends the check:
///
/// 1. The private key is not an OpenSSH private key at all: [`CaProblem::Unreadable`]. An
///    algorithm this crate does not know is [`CaProblem::NotEd25519`] instead.
/// 2. Its cipher or its KDF is not `none`: [`CaProblem::Encrypted`].
/// 3. The public key has fewer than two words: [`CaProblem::Unreadable`].
/// 4. The public key's first word is not `ssh-ed25519`, or the private key is not ed25519:
///    [`CaProblem::NotEd25519`].
/// 5. The public key does not parse: [`CaProblem::Unreadable`].
/// 6. The public key's first two words differ from those derived from the private key, compared
///    exactly: [`CaProblem::Mismatch`].
pub fn check_ca_keys(private: &[u8], public: &[u8]) -> Result<(), CaProblem> {
    // The order of the checks, and which problem each failure is, are the architect's rulings of
    // 2026-11-19 (the doc comment above lists the six steps). A private key that is not ed25519
    // is NotEd25519, and a public key that does not parse is Unreadable before Mismatch.
    let private_key = match parse_private(private) {
        Ok(key) => key,
        // An algorithm this crate does not know is not ed25519, and the file is still a key.
        Err(ssh_key::Error::AlgorithmUnknown | ssh_key::Error::AlgorithmUnsupported { .. }) => {
            return Err(CaProblem::NotEd25519);
        }
        Err(_) => return Err(CaProblem::Unreadable),
    };
    if has_passphrase(&private_key) {
        return Err(CaProblem::Encrypted);
    }

    let public_text = normalized(public).ok_or(CaProblem::Unreadable)?;
    let public_text = public_text.trim();
    let (kind, blob) = first_two_words(public_text).ok_or(CaProblem::Unreadable)?;
    if kind != ED25519_WORD || private_key.algorithm() != Algorithm::Ed25519 {
        return Err(CaProblem::NotEd25519);
    }
    PublicKey::from_openssh(public_text).map_err(|_| CaProblem::Unreadable)?;

    let derived = private_key
        .public_key()
        .to_openssh()
        .map_err(|_| CaProblem::Unreadable)?;
    let (derived_kind, derived_blob) = first_two_words(&derived).ok_or(CaProblem::Unreadable)?;
    if kind != derived_kind || blob != derived_blob {
        return Err(CaProblem::Mismatch);
    }
    Ok(())
}
