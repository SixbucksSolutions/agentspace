//! Helpers for the on-disk tests: throwaway folders, a child process that holds a file open, and a
//! guard that restores an access rule. Windows only; every file lives under the temp folder.
#![cfg(windows)]
#![allow(dead_code)]

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use agentspace_state::{StateFile, parse};
use tempfile::TempDir;

/// A throwaway folder under the temp folder.
pub fn sandbox() -> TempDir {
    tempfile::Builder::new()
        .prefix("agentspace-disk-test-")
        .tempdir()
        .expect("A throwaway folder must be made")
}

/// A state file of the given records, built through `parse` like any reader's.
pub fn state_of(records: Vec<serde_json::Value>) -> StateFile {
    parse(&serde_json::to_vec(&serde_json::json!({"version": 0, "leases": records})).unwrap())
        .expect("Fixture must parse")
}

/// A child process that is killed when this is dropped, even if the test fails.
pub struct Holder {
    child: Child,
}

impl Drop for Holder {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Holder {
    /// Whether the child has exited.
    pub fn exited(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(Some(_)))
    }
}

/// Starts a PowerShell child that opens `path` with the given FileMode, FileAccess and FileShare
/// names, prints a ready line, then holds the handle for `seconds` seconds. Returns once the child
/// has the file open. The path travels in an environment variable, never in the command line.
pub fn hold(path: &Path, mode: &str, access: &str, share: &str, seconds: u32) -> Holder {
    let script = format!(
        "$f = [IO.File]::Open($env:AS_HOLD_PATH, '{mode}', '{access}', '{share}'); \
         [Console]::Out.WriteLine('READY'); [Console]::Out.Flush(); Start-Sleep -Seconds {seconds}"
    );
    let mut child = Command::new("pwsh")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .env("AS_HOLD_PATH", path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .spawn()
        .expect("pwsh must start");
    let out = child.stdout.take().unwrap();
    let holder = Holder { child };
    let mut line = String::new();
    BufReader::new(out)
        .read_line(&mut line)
        .expect("The child must print a line");
    assert_eq!(line.trim(), "READY", "The child must open the file");
    holder
}

/// Opens `path` for reading, sharing read, write and delete, as a reader of the state file does.
pub fn open_sharing_everything(path: &Path) -> std::fs::File {
    use std::os::windows::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .read(true)
        .share_mode(7)
        .open(path)
        .expect("The file must open")
}

/// Every `leases.json.*.tmp` left in the folder.
pub fn temp_files_in(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with("leases.json.") && n.ends_with(".tmp"))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// A deny rule for read and write data on a file for Everyone, removed again on drop so the
/// throwaway folder can be deleted.
pub struct Deny {
    path: PathBuf,
}

impl Deny {
    pub fn on(path: &Path) -> Deny {
        let ok = Command::new("icacls")
            .arg(path)
            .args(["/deny", "*S-1-1-0:(RD,WD)"])
            .stdout(Stdio::null())
            .status()
            .expect("icacls must run")
            .success();
        assert!(ok, "icacls must set the deny rule");
        Deny {
            path: path.to_path_buf(),
        }
    }
}

impl Drop for Deny {
    fn drop(&mut self) {
        let _ = Command::new("icacls")
            .arg(&self.path)
            .args(["/remove:d", "*S-1-1-0"])
            .stdout(Stdio::null())
            .status();
    }
}
