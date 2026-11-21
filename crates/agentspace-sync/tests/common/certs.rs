//! The operator's certificate for a host, read with the real `ssh-keygen -L` (E0.07-T03-S03).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. operator.md, The test seam, "Not
//! covered": `ssh-keygen` has no variable, so a test of a host that is not the box runs the real
//! `ssh-keygen.exe`, against the throwaway CA in the throwaway home. Here it only reads a file of
//! the throwaway home; it signs nothing and reaches no host.
#![allow(dead_code)]

use std::process::{Command, Stdio};

use super::times::parse_time;
use super::{Rig, real_system_root};

/// What `ssh-keygen -L` shows of a certificate.
#[derive(Debug)]
pub struct CertInfo {
    pub cert_type: String,
    pub key_id: String,
    pub principals: Vec<String>,
    /// The valid-after and valid-before times as printed, read as if they were UTC: only their
    /// difference means anything.
    pub valid_from: i64,
    pub valid_to: i64,
    pub signing_ca: String,
    pub extensions: Vec<String>,
}

impl Rig {
    /// The host's operator certificate as the real `ssh-keygen -L` shows it (operator.md, Which
    /// checks show it).
    pub fn cert_info(&self, project: &str) -> CertInfo {
        let cert = agentspace_hosts::host_cert_path(&self.home, project);
        let keygen = real_system_root()
            .join("System32")
            .join("OpenSSH")
            .join("ssh-keygen.exe");
        let out = Command::new(keygen)
            .args(["-L", "-f"])
            .arg(&cert)
            .env_clear()
            .env("SystemRoot", real_system_root())
            // ssh looks for its system-wide configuration in `%ProgramData%\ssh`, and fails
            // without the variable; the throwaway folder holds none.
            .env("ProgramData", self.root.join("appdata"))
            .env("USERPROFILE", &self.home)
            .env("HOME", &self.home)
            .stdin(Stdio::null())
            .output()
            .expect("ssh-keygen could not be started");
        assert!(
            out.status.success(),
            "ssh-keygen -L could not read the operator's certificate: {}",
            self.mask(&String::from_utf8_lossy(&out.stderr))
        );
        parse_keygen_l(&String::from_utf8_lossy(&out.stdout))
    }
}

fn parse_keygen_l(text: &str) -> CertInfo {
    let mut info = CertInfo {
        cert_type: String::new(),
        key_id: String::new(),
        principals: Vec::new(),
        valid_from: 0,
        valid_to: 0,
        signing_ca: String::new(),
        extensions: Vec::new(),
    };
    let mut section = "";
    for raw in text.lines() {
        let line = raw.trim();
        if let Some(v) = line.strip_prefix("Type: ") {
            info.cert_type = v.to_string();
            section = "";
        } else if let Some(v) = line.strip_prefix("Signing CA: ") {
            info.signing_ca = v.to_string();
            section = "";
        } else if let Some(v) = line.strip_prefix("Key ID: ") {
            info.key_id = v.trim_matches('"').to_string();
            section = "";
        } else if let Some(v) = line.strip_prefix("Valid: from ") {
            if let Some((from, to)) = v.split_once(" to ") {
                info.valid_from = parse_time(from, "").unwrap_or(0);
                info.valid_to = parse_time(to, "").unwrap_or(0);
            }
            section = "";
        } else if line.starts_with("Principals:") {
            section = "principals";
        } else if line.starts_with("Extensions:") {
            section = "extensions";
        } else if line.ends_with(':') || line.contains(": ") {
            section = "";
        } else if !line.is_empty() {
            match section {
                "principals" => info.principals.push(line.to_string()),
                "extensions" => info.extensions.push(line.to_string()),
                _ => {}
            }
        }
    }
    info
}
