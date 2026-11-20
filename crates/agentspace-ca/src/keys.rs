//! The CA's key pair: made in code, and checks 3 to 5 of docs/state.md, The CA, Its checks.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! - **The key pair MUST be ed25519 with no passphrase, made in this program's own code** from the
//!   operating system's random source, never by running `ssh-keygen` (card #0118).
//! - **Checks 3 to 5 MUST be made in their order**, each failure naming its check, so that `ca` and
//!   `sync` print that check's message.
//! - **Nothing here MUST put key material or a library's error text in a message.**

use agentspace_credential::{CaProblem, check_ca_keys, check_unencrypted};
use ssh_key::rand_core::OsRng;
use ssh_key::{Algorithm, HashAlg, LineEnding, PrivateKey, PublicKey};

/// A new CA key pair, in OpenSSH's formats.
pub(crate) struct NewCa {
    /// The private key in OpenSSH's private key format, unencrypted, LF line endings.
    pub(crate) private: String,
    /// The public key line, `ssh-ed25519 <base64> <comment>`, ending in a line feed.
    pub(crate) public: String,
}

/// Makes a new ed25519 key pair with no passphrase and the comment `comment`. The error is in
/// agentspace's own words.
pub(crate) fn new_ca(comment: &str) -> Result<NewCa, String> {
    let mut key = PrivateKey::random(&mut OsRng, Algorithm::Ed25519)
        .map_err(|_| "A key pair could not be made.".to_string())?;
    key.set_comment(comment);
    let private = key
        .to_openssh(LineEnding::LF)
        .map_err(|_| "The private key could not be written out.".to_string())?;
    let public = key
        .public_key()
        .to_openssh()
        .map_err(|_| "The public key could not be written out.".to_string())?;
    Ok(NewCa {
        private: private.to_string(),
        public: format!("{public}\n"),
    })
}

/// Which of state.md's checks 3 to 5 failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum KeyFailure {
    /// Check 3: the private key does not load with an empty passphrase.
    Passphrase,
    /// Check 4: the public key is not the private key's own, or the pair is not ed25519.
    NotItsOwn,
    /// Check 5: the public key's fingerprint cannot be read; the reason in agentspace's words.
    Fingerprint(String),
}

/// What check 5's `<tool output>` says when the public key does not parse. The rewrite reads the
/// key in code, so this is agentspace's own text (operator.md, ca, `<tool output>`).
// Since no tool runs, check 5's <tool output> is this fixed sentence (operator.md, ca, Rulings
// for the rewrite's ca: architect, 2026-11-19, for E0.06-T02-S01).
pub(crate) const NOT_A_PUBLIC_KEY: &str = "It is not an OpenSSH public key.";

/// Checks 3 to 5 on the two files' bytes, in order, and returns the public key's fingerprint,
/// `SHA256:<base64>`, when all three hold.
///
/// - **Check 3** is agentspace-credential's `check_unencrypted`. A private key of an algorithm this
///   program does not know fails check 4 instead (it may load, but it is not ed25519), as
///   `check_ca_keys` classes it.
/// - **Check 4** compares the public key's first two words, split at whitespace after trimming,
///   with those of the public key derived from the private key, exactly, and wants `ssh-ed25519`.
/// - **Check 5** parses those two words as an OpenSSH public key. So the comment, and the
///   whitespace around and between the words, never matter, as they do not to `ssh-keygen -l`.
///
/// state.md lets a build treat a public key of fewer than two words as any of these checks'
/// failures; here it is check 5's, since it has no fingerprint.
pub(crate) fn check_pair(private: &[u8], public: &[u8]) -> Result<String, KeyFailure> {
    let unreadable = || KeyFailure::Fingerprint(NOT_A_PUBLIC_KEY.to_string());
    if check_unencrypted(private).is_err() {
        return Err(match check_ca_keys(private, b"") {
            Err(CaProblem::NotEd25519) => KeyFailure::NotItsOwn,
            _ => KeyFailure::Passphrase,
        });
    }
    let private_text = std::str::from_utf8(private)
        .map_err(|_| KeyFailure::Passphrase)?
        .replace("\r\n", "\n");
    let key =
        PrivateKey::from_openssh(private_text.as_bytes()).map_err(|_| KeyFailure::Passphrase)?;

    let (kind, blob) = first_two_words(public).ok_or_else(unreadable)?;
    if kind != ED25519 || key.algorithm() != Algorithm::Ed25519 {
        return Err(KeyFailure::NotItsOwn);
    }
    let derived = key
        .public_key()
        .to_openssh()
        .map_err(|_| KeyFailure::NotItsOwn)?;
    let mut words = derived.split_whitespace();
    if words.next() != Some(kind.as_str()) || words.next() != Some(blob.as_str()) {
        return Err(KeyFailure::NotItsOwn);
    }

    fingerprint(public).ok_or_else(unreadable)
}

/// The first word of an ed25519 public key line.
const ED25519: &str = "ssh-ed25519";

/// The first two words of a public key file, split at whitespace after trimming, or `None` for a
/// file that is not text or holds fewer than two words.
fn first_two_words(public: &[u8]) -> Option<(String, String)> {
    let text = std::str::from_utf8(public).ok()?;
    let mut words = text.split_whitespace();
    Some((words.next()?.to_string(), words.next()?.to_string()))
}

/// The fingerprint of a public key file's bytes, `SHA256:<base64>`, as `ssh-keygen -l` prints it:
/// its first two words parsed as an OpenSSH public key.
pub(crate) fn fingerprint(public: &[u8]) -> Option<String> {
    let (kind, blob) = first_two_words(public)?;
    let key = PublicKey::from_openssh(&format!("{kind} {blob}")).ok()?;
    Some(key.fingerprint(HashAlg::Sha256).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_ca_is_ed25519_with_no_passphrase_and_its_comment() {
        let ca = new_ca("agentspace-ca HOST 2026-11-19").unwrap();
        let key = PrivateKey::from_openssh(ca.private.as_bytes()).unwrap();
        assert_eq!(key.algorithm(), Algorithm::Ed25519);
        assert!(!key.is_encrypted());
        assert_eq!(key.cipher(), ssh_key::Cipher::None);
        assert!(key.kdf().is_none());
        assert!(ca.public.starts_with("ssh-ed25519 "));
        assert!(ca.public.ends_with(" agentspace-ca HOST 2026-11-19\n"));
        assert!(!ca.private.contains('\r'));
        assert!(check_pair(ca.private.as_bytes(), ca.public.as_bytes()).is_ok());
    }

    #[test]
    fn two_new_cas_differ() {
        let a = new_ca("c").unwrap();
        let b = new_ca("c").unwrap();
        assert_ne!(a.public, b.public);
    }

    #[test]
    fn the_fingerprint_is_sha256_of_the_key() {
        let ca = new_ca("c").unwrap();
        let fp = check_pair(ca.private.as_bytes(), ca.public.as_bytes()).unwrap();
        let key = PublicKey::from_openssh(ca.public.trim()).unwrap();
        assert_eq!(fp, key.fingerprint(HashAlg::Sha256).to_string());
        assert!(fp.starts_with("SHA256:"));
        // A CR LF file and one without its comment give the same fingerprint.
        let crlf = ca.public.replace('\n', "\r\n");
        assert_eq!(fingerprint(crlf.as_bytes()).as_deref(), Some(fp.as_str()));
    }

    #[test]
    fn a_private_key_that_is_not_a_key_fails_check_3() {
        let ca = new_ca("c").unwrap();
        assert_eq!(
            check_pair(b"not a key", ca.public.as_bytes()),
            Err(KeyFailure::Passphrase)
        );
    }

    const B64: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    fn b64_decode(text: &str) -> Vec<u8> {
        let mut bits = 0u32;
        let mut count = 0;
        let mut out = Vec::new();
        for c in text
            .bytes()
            .filter(|c| !c.is_ascii_whitespace() && *c != b'=')
        {
            let value = B64.iter().position(|b| *b == c).unwrap() as u32;
            bits = (bits << 6) | value;
            count += 6;
            if count >= 8 {
                count -= 8;
                out.push((bits >> count) as u8);
            }
        }
        out
    }

    fn b64_encode(bytes: &[u8]) -> String {
        let mut out = String::new();
        for chunk in bytes.chunks(3) {
            let n = chunk.len();
            let word = (u32::from(chunk[0]) << 16)
                | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
                | u32::from(*chunk.get(2).unwrap_or(&0));
            for i in 0..4 {
                if i <= n {
                    out.push(B64[((word >> (18 - 6 * i)) & 63) as usize] as char);
                } else {
                    out.push('=');
                }
            }
        }
        out
    }

    fn string(bytes: &[u8]) -> Vec<u8> {
        let mut out = (bytes.len() as u32).to_be_bytes().to_vec();
        out.extend_from_slice(bytes);
        out
    }

    /// Reads one length-prefixed string at `at`, and moves `at` past it.
    fn take(blob: &[u8], at: &mut usize) -> Vec<u8> {
        let len = u32::from_be_bytes(blob[*at..*at + 4].try_into().unwrap()) as usize;
        let value = blob[*at + 4..*at + 4 + len].to_vec();
        *at += 4 + len;
        value
    }

    /// The same key's file as one that claims a passphrase: cipher `aes256-ctr`, KDF `bcrypt`.
    fn as_if_encrypted(private: &str) -> String {
        let body: String = private
            .lines()
            .filter(|line| !line.starts_with("-----"))
            .collect();
        let blob = b64_decode(&body);
        let magic = b"openssh-key-v1\0";
        let mut at = magic.len();
        for _ in 0..3 {
            take(&blob, &mut at); // The cipher, the KDF and its options.
        }
        at += 4; // The number of keys.
        let public = take(&blob, &mut at);
        let mut secret = take(&blob, &mut at);
        secret.resize(secret.len().div_ceil(16) * 16, 0);
        let mut out = magic.to_vec();
        out.extend(string(b"aes256-ctr"));
        out.extend(string(b"bcrypt"));
        let mut options = string(&[7u8; 16]);
        options.extend_from_slice(&16u32.to_be_bytes());
        out.extend(string(&options));
        out.extend_from_slice(&1u32.to_be_bytes());
        out.extend(string(&public));
        out.extend(string(&secret));
        let encoded = b64_encode(&out);
        let mut text = "-----BEGIN OPENSSH PRIVATE KEY-----\n".to_string();
        for line in encoded.as_bytes().chunks(70) {
            text.push_str(std::str::from_utf8(line).unwrap());
            text.push('\n');
        }
        text.push_str("-----END OPENSSH PRIVATE KEY-----\n");
        text
    }

    #[test]
    fn an_encrypted_private_key_fails_check_3() {
        let ca = new_ca("c").unwrap();
        let locked = as_if_encrypted(&ca.private);
        let parsed = PrivateKey::from_openssh(locked.as_bytes()).expect("still a key file");
        assert!(parsed.is_encrypted());
        assert_eq!(
            check_pair(locked.as_bytes(), ca.public.as_bytes()),
            Err(KeyFailure::Passphrase)
        );
    }

    #[test]
    fn another_public_key_fails_check_4() {
        let a = new_ca("c").unwrap();
        let b = new_ca("c").unwrap();
        assert_eq!(
            check_pair(a.private.as_bytes(), b.public.as_bytes()),
            Err(KeyFailure::NotItsOwn)
        );
    }

    #[test]
    fn a_public_key_of_another_type_fails_check_4() {
        let a = new_ca("c").unwrap();
        let other = a.public.replacen("ssh-ed25519", "ssh-rsa", 1);
        assert_eq!(
            check_pair(a.private.as_bytes(), other.as_bytes()),
            Err(KeyFailure::NotItsOwn)
        );
    }

    #[test]
    fn a_public_key_of_fewer_than_two_words_fails_check_5() {
        let a = new_ca("c").unwrap();
        for public in [&b""[..], b"  \r\n", b"ssh-ed25519", b"\xff\xfe ssh-ed25519"] {
            assert_eq!(
                check_pair(a.private.as_bytes(), public),
                Err(KeyFailure::Fingerprint(NOT_A_PUBLIC_KEY.to_string())),
                "public key {public:?}"
            );
        }
    }

    #[test]
    fn a_second_word_that_is_not_the_keys_fails_check_4_before_check_5() {
        let a = new_ca("c").unwrap();
        assert_eq!(
            check_pair(a.private.as_bytes(), b"ssh-ed25519 !!!notbase64!!! c"),
            Err(KeyFailure::NotItsOwn)
        );
        // The first word is compared exactly, case included.
        let shouted = a.public.replacen("ssh-ed25519", "SSH-ED25519", 1);
        assert_eq!(
            check_pair(a.private.as_bytes(), shouted.as_bytes()),
            Err(KeyFailure::NotItsOwn)
        );
    }

    #[test]
    fn the_comment_and_the_whitespace_are_not_compared() {
        let a = new_ca("one").unwrap();
        let fp = check_pair(a.private.as_bytes(), a.public.as_bytes()).unwrap();
        let words: Vec<&str> = a.public.split_whitespace().collect();
        for public in [
            a.public.replace(" one\n", " two words\n"),
            format!(
                "  {}\t{}    any comment at all \r\n\r\n",
                words[0], words[1]
            ),
            format!("{} {}", words[0], words[1]),
        ] {
            assert_eq!(
                check_pair(a.private.as_bytes(), public.as_bytes()).as_deref(),
                Ok(fp.as_str()),
                "public key {public:?}"
            );
        }
        // A private key with CR LF line endings loads the same.
        let crlf = a.private.replace('\n', "\r\n");
        assert!(check_pair(crlf.as_bytes(), a.public.as_bytes()).is_ok());
    }
}
