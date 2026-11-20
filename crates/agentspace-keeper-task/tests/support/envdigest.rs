//! Shared by the test-only stub keeper and the tests, through `#[path]`: a count and a hash of an
//! environment, so a test can show a child's environment equals the one it gave the launcher
//! without ever printing a value.

use std::ffi::OsString;

/// The number of variables and an FNV-1a hash of the sorted `NAME=value` lines, names in upper
/// case since Windows compares them so. The per-drive entries whose names start with `=` are left
/// out: the operating system makes them, they are not the caller's.
pub fn digest(vars: impl Iterator<Item = (OsString, OsString)>) -> (usize, u64) {
    let mut lines: Vec<String> = vars
        .filter(|(name, _)| !name.to_string_lossy().starts_with('='))
        .map(|(name, value)| {
            format!(
                "{}={}",
                name.to_string_lossy().to_uppercase(),
                value.to_string_lossy()
            )
        })
        .collect();
    lines.sort();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for line in &lines {
        for byte in line.bytes().chain(std::iter::once(b'\n')) {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    }
    (lines.len(), hash)
}
