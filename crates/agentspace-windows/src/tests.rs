//! The crate's own tests of its rows: the pure rows, and, on Windows, each script run by the
//! laptop's Windows PowerShell 5.1 in a throwaway folder under the checkout's `target`, with every
//! path that names the host's folders replaced by one in that folder first. No test runs a script
//! with a zone, so none can change the laptop's time zone, and none names a real home.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.

use super::*;

const SAMPLE_ZONE: &str = "Pacific Standard Time (Mexico)";

fn longest_zone() -> String {
    format!("A{}b", "x".repeat(62))
}

#[test]
fn every_encoded_row_fits_the_form() {
    let plan = WINDOWS.sync_plan().unwrap();
    let bootstrap = WINDOWS.bootstrap().unwrap();
    let ScriptDelivery::CopyThenRun { remote_command, .. } = bootstrap.delivery else {
        panic!("the bootstrap is copied then run");
    };
    let lines = [
        remote_command,
        plan.install_command,
        plan.clear_command.unwrap(),
        plan.localswim_install_command,
        WINDOWS.make_claude_folder_command().unwrap(),
        WINDOWS
            .remote_command(&WINDOWS.grow_script().unwrap())
            .unwrap(),
        WINDOWS
            .remote_command(&WINDOWS.push_script("").unwrap())
            .unwrap(),
        WINDOWS
            .remote_command(&WINDOWS.push_script(&longest_zone()).unwrap())
            .unwrap(),
    ];
    for line in lines {
        assert!(line.starts_with(PREFIX));
        assert!(line.len() <= MAX_LINE, "{} characters", line.len());
        assert!(!line.contains('='));
    }
}

#[test]
fn the_push_carries_its_zone_inside_the_script_and_no_zone_as_empty() {
    assert!(scripts::push(SAMPLE_ZONE).contains(&format!("AddArgument('{SAMPLE_ZONE}')")));
    assert!(!WINDOWS.push_script("").unwrap().contains("Set-TimeZone"));
    let line = WINDOWS
        .remote_command(&WINDOWS.push_script(SAMPLE_ZONE).unwrap())
        .unwrap();
    assert!(!line.contains("Pacific"), "the zone is not on the line");
}

#[test]
fn the_push_refuses_a_zone_outside_the_pattern() {
    for zone in ["It's", "America/Denver", "Zone'; Remove-Item x; '"] {
        assert_eq!(WINDOWS.push_script(zone).unwrap_err().message, NO_ZONE);
    }
}

#[test]
fn the_push_zone_is_the_windows_id_itself_never_the_iana_name() {
    assert_eq!(
        WINDOWS
            .push_zone("Mountain Standard Time", "America/Denver")
            .unwrap(),
        "Mountain Standard Time"
    );
    assert!(
        WINDOWS
            .push_zone("America/Denver", "America/Denver")
            .is_err()
    );
    assert!(WINDOWS.push_zone("", "America/Denver").is_err());
}

#[test]
fn the_horizon_command_holds_its_digits_as_one_word_and_no_cr() {
    let command = WINDOWS.horizon_command(1_795_000_000).unwrap();
    assert!(
        command.split(' ').any(|word| word == "1795000000"),
        "{command}"
    );
    assert!(command.contains("[IO.File]::WriteAllText"));
    assert!(!command.contains("Set-Content") && !command.contains("Out-File"));
    assert!(
        !command.contains('\r') && !command.contains('\n'),
        "one line"
    );
    assert!(!command.contains("`r"), "never a CR");
    assert!(!command.contains("-EncodedCommand"));
    // The largest horizon is still its digits.
    let largest = WINDOWS.horizon_command(u64::MAX).unwrap();
    assert!(largest.split(' ').any(|word| word == u64::MAX.to_string()));
}

#[test]
fn the_patch_check_is_operator_mds_command_byte_for_byte() {
    assert_eq!(
        WINDOWS.patch_check_command().unwrap(),
        r"& 'C:\Program Files\PowerShell\7\pwsh.exe' -NoProfile -NonInteractive -File C:\ProgramData\agentspace\agentspace-patch-check.ps1"
    );
}

#[test]
fn sync_carries_box_windows_but_its_tests_folder_byte_for_byte() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../box/windows");
    let mut on_disk = Vec::new();
    collect(&root, "", &mut on_disk);
    on_disk.sort();
    let carried: Vec<(String, Vec<u8>)> = BOX_WINDOWS
        .iter()
        .map(|(name, bytes)| (name.to_string(), bytes.to_vec()))
        .collect();
    assert_eq!(carried, on_disk);
    assert!(carried.iter().any(|(name, _)| name == "install.ps1"));
    assert!(carried.iter().all(|(name, _)| !name.starts_with("tests/")));
    assert_eq!(
        WINDOWS.bootstrap().unwrap().script,
        std::fs::read(root.join("bootstrap.ps1")).unwrap()
    );
}

fn collect(dir: &std::path::Path, prefix: &str, out: &mut Vec<(String, Vec<u8>)>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().into_string().unwrap();
        let is_dir = entry.file_type().unwrap().is_dir();
        if prefix.is_empty() && name == "tests" && is_dir {
            continue;
        }
        let relative = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        if is_dir {
            collect(&entry.path(), &relative, out);
        } else {
            out.push((relative, std::fs::read(entry.path()).unwrap()));
        }
    }
}

#[cfg(windows)]
mod run {
    //! Each script run by Windows PowerShell 5.1, as a host would run it, in a throwaway folder.

    use super::super::*;
    use super::SAMPLE_ZONE;
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Output, Stdio};
    use std::sync::atomic::{AtomicU32, Ordering};

    static NEXT: AtomicU32 = AtomicU32::new(0);

    /// A new, empty folder under the checkout's `target`, never the system's temporary folder.
    fn scratch(name: &str) -> PathBuf {
        let n = NEXT.fetch_add(1, Ordering::SeqCst);
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/s06impl-unit")
            .join(format!("{name}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("home")).unwrap();
        std::fs::canonicalize(&dir).unwrap()
    }

    /// A path as the scripts' PowerShell reads it: no `\\?\` prefix.
    fn plain(path: &Path) -> String {
        let text = path.display().to_string();
        text.strip_prefix(r"\\?\").unwrap_or(&text).to_string()
    }

    fn powershell() -> PathBuf {
        let root = std::env::var_os("SystemRoot").expect("SystemRoot");
        PathBuf::from(root).join(r"System32\WindowsPowerShell\v1.0\powershell.exe")
    }

    /// Windows PowerShell with the throwaway home in place of every home variable, and `TMP` and
    /// `TEMP` kept as they are.
    fn command(dir: &Path) -> Command {
        let home = dir.join("home");
        let mut command = Command::new(powershell());
        command
            .env("USERPROFILE", &home)
            .env("HOME", &home)
            .env("LOCALAPPDATA", &home)
            .env("APPDATA", &home)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }

    /// Runs one encoded remote command line, its program and arguments split at its spaces as the
    /// login shell splits them, with `stdin` on its stdin.
    fn run_line(dir: &Path, line: &str, stdin: &[u8]) -> Output {
        let mut words = line.split(' ');
        assert_eq!(words.next(), Some("powershell"));
        let mut child = command(dir)
            .args(words)
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(stdin).unwrap();
        child.wait_with_output().unwrap()
    }

    /// A script with `$HOME` replaced by the throwaway home, run through the encoded form.
    fn run_at_home(dir: &Path, script: &str, stdin: &[u8]) -> Output {
        let home = plain(&dir.join("home"));
        let script = script.replace("$HOME", &format!("'{home}'"));
        let line = WINDOWS.remote_command(&script).unwrap();
        run_line(dir, &line, stdin)
    }

    fn code(output: &Output) -> Option<i32> {
        output.status.code()
    }

    fn stdout(output: &Output) -> String {
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    #[test]
    fn every_script_parses_under_windows_powershell() {
        let dir = scratch("parse");
        let scripts = [
            scripts::bootstrap_run(),
            scripts::install(),
            scripts::CLEAR_STAGING.to_string(),
            scripts::MAKE_CLAUDE_FOLDER.to_string(),
            scripts::INSTALL_LOCALSWIM.to_string(),
            scripts::GROW.to_string(),
            scripts::push(""),
            scripts::push(SAMPLE_ZONE),
            scripts::horizon(1_795_000_000),
            scripts::PATCH_CHECK.to_string(),
        ];
        for (n, script) in scripts.iter().enumerate() {
            std::fs::write(dir.join(format!("s{n}.txt")), script).unwrap();
        }
        let check = format!(
            "$bad = 0; foreach ($n in 0..{last}) {{ $e = $null; $t = $null; \
             [void][System.Management.Automation.Language.Parser]::ParseInput(\
             [IO.File]::ReadAllText('{dir}\\s' + $n + '.txt'), [ref]$t, [ref]$e); \
             if ($e.Count -gt 0) {{ [Console]::Out.Write([string]$n + ' ') ; $bad++ }} }}; exit $bad",
            last = scripts.len() - 1,
            dir = plain(&dir),
        );
        let output = command(&dir)
            .args(["-NoProfile", "-NonInteractive", "-Command", &check])
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert_eq!(
            code(&output),
            Some(0),
            "scripts that do not parse: {}",
            stdout(&output)
        );
    }

    #[test]
    fn the_horizon_command_writes_the_digits_and_one_lf_over_the_old_file() {
        let dir = scratch("horizon");
        let folder = dir.join("agentspace");
        let run = |horizon: u64| {
            let text = WINDOWS
                .horizon_command(horizon)
                .unwrap()
                .replace(PROGRAM_DATA, &plain(&folder));
            command(&dir)
                .args(["-NoProfile", "-NonInteractive", "-Command", &text])
                .stdin(Stdio::null())
                .output()
                .unwrap()
        };
        let first = run(1_795_000_000);
        assert_eq!(code(&first), Some(0));
        assert_eq!(
            std::fs::read(folder.join("lease-horizon")).unwrap(),
            b"1795000000\n"
        );
        let second = run(7);
        assert_eq!(code(&second), Some(0));
        assert_eq!(std::fs::read(folder.join("lease-horizon")).unwrap(), b"7\n");
        let mut left: Vec<String> = std::fs::read_dir(&folder)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        left.sort();
        assert_eq!(left, ["lease-horizon"], "no temporary file is left");
    }

    #[test]
    fn the_horizon_command_exits_non_zero_when_its_folder_cannot_be_made() {
        let dir = scratch("horizon-fails");
        let blocker = dir.join("a-file");
        std::fs::write(&blocker, b"x").unwrap();
        let text = WINDOWS
            .horizon_command(5)
            .unwrap()
            .replace(PROGRAM_DATA, &plain(&blocker.join("agentspace")));
        let output = command(&dir)
            .args(["-NoProfile", "-NonInteractive", "-Command", &text])
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert_eq!(code(&output), Some(1));
    }

    #[test]
    fn the_bootstrap_runs_the_copy_removes_it_and_exits_with_its_status() {
        let dir = scratch("bootstrap");
        let copy = dir.join("home").join(BOOTSTRAP_COPY);
        std::fs::write(&copy, "Write-Output 'ran'\nexit 5\n").unwrap();
        let output = run_at_home(&dir, &scripts::bootstrap_run(), b"");
        assert_eq!(code(&output), Some(5));
        assert!(stdout(&output).contains("ran"));
        assert!(!copy.exists(), "the copy is removed");
    }

    #[test]
    fn the_install_removes_the_staging_folder_only_after_it_exits_zero() {
        let dir = scratch("install");
        let staging = dir.join("home").join(STAGING);
        std::fs::create_dir_all(&staging).unwrap();
        std::fs::write(staging.join("install.ps1"), "exit 3\n").unwrap();
        let failed = run_at_home(&dir, &scripts::install(), b"");
        assert_eq!(code(&failed), Some(3));
        assert!(staging.exists(), "kept after a failed install");

        std::fs::write(
            staging.join("install.ps1"),
            "Write-Output 'Installed: x'\nexit 0\n",
        )
        .unwrap();
        let passed = run_at_home(&dir, &scripts::install(), b"");
        assert_eq!(code(&passed), Some(0));
        assert!(stdout(&passed).contains("Installed: x"));
        assert!(!staging.exists(), "removed after the install exited 0");
    }

    #[test]
    fn the_clear_step_removes_the_staging_folder_and_a_missing_one_is_no_error() {
        let dir = scratch("clear");
        let staging = dir.join("home").join(STAGING);
        std::fs::create_dir_all(staging.join("sub")).unwrap();
        std::fs::write(staging.join("sub").join("f"), b"x").unwrap();
        assert_eq!(
            code(&run_at_home(&dir, scripts::CLEAR_STAGING, b"")),
            Some(0)
        );
        assert!(!staging.exists());
        assert_eq!(
            code(&run_at_home(&dir, scripts::CLEAR_STAGING, b"")),
            Some(0)
        );
    }

    #[test]
    fn the_claude_folder_step_makes_the_folder_and_keeps_one_already_there() {
        let dir = scratch("claude");
        let folder = dir.join("home").join(".claude");
        assert_eq!(
            code(&run_at_home(&dir, scripts::MAKE_CLAUDE_FOLDER, b"")),
            Some(0)
        );
        assert!(folder.is_dir());
        std::fs::write(folder.join("keep"), b"x").unwrap();
        assert_eq!(
            code(&run_at_home(&dir, scripts::MAKE_CLAUDE_FOLDER, b"")),
            Some(0)
        );
        assert!(folder.join("keep").exists());
    }

    #[test]
    fn the_localswim_step_moves_the_old_copy_aside_installs_and_removes_the_staged_one() {
        let dir = scratch("localswim");
        let folder = dir.join("localswim");
        let script = scripts::INSTALL_LOCALSWIM
            .replace(r"C:\ProgramData\agentspace\localswim", &plain(&folder));
        let staged = dir.join("home").join(LOCALSWIM_STAGED);
        std::fs::write(&staged, b"new").unwrap();
        assert_eq!(code(&run_at_home(&dir, &script, b"")), Some(0));
        assert_eq!(std::fs::read(folder.join("localswim.exe")).unwrap(), b"new");
        assert!(!staged.exists());

        std::fs::write(&staged, b"newer").unwrap();
        assert_eq!(code(&run_at_home(&dir, &script, b"")), Some(0));
        assert_eq!(
            std::fs::read(folder.join("localswim.exe")).unwrap(),
            b"newer"
        );
        assert!(
            !folder.join("localswim.exe.old").exists(),
            "removed when it can be"
        );
        assert!(!staged.exists());

        // No staged copy: the install fails, and the installed copy is put back.
        assert_eq!(code(&run_at_home(&dir, &script, b"")), Some(1));
        assert_eq!(
            std::fs::read(folder.join("localswim.exe")).unwrap(),
            b"newer"
        );
    }

    const URL: &str = "LOCALSWIM_URL=http://127.0.0.1:9";
    const TOKEN: &str = "LOCALSWIM_TOKEN=planted-not-a-token";

    fn credential(dir: &Path) -> PathBuf {
        dir.join("home")
            .join(".config")
            .join("localswim")
            .join("bot.env")
    }

    fn files_beside(path: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn the_push_writes_the_credential_normalized_and_prints_its_hash() {
        let dir = scratch("push");
        let content = format!("\u{feff}{URL}\r\n{TOKEN}\r\n");
        let output = run_at_home(&dir, &WINDOWS.push_script("").unwrap(), content.as_bytes());
        assert_eq!(code(&output), Some(0));
        let want = format!("{URL}\n{TOKEN}\n");
        let written = std::fs::read(credential(&dir)).unwrap();
        assert_eq!(written, want.as_bytes());
        let printed = stdout(&output);
        assert!(
            printed
                .lines()
                .any(|line| line == format!("{}  -", sha256_hex(&written))),
            "the hash line: {printed}"
        );
        assert!(!printed.contains("zone"), "no zone line without a zone");
        assert!(!printed.contains("planted-not-a-token"));
        assert!(!String::from_utf8_lossy(&output.stderr).contains("planted-not-a-token"));
        assert_eq!(files_beside(&credential(&dir)), ["bot.env"]);

        // The folder the push made, and the file, hold exactly the two rules, inheritance off.
        for path in [
            credential(&dir),
            credential(&dir).parent().unwrap().to_path_buf(),
        ] {
            let check = format!(
                "$n = '{}'; $a = if ([IO.Directory]::Exists($n)) {{ [IO.Directory]::GetAccessControl($n) }} else {{ [IO.File]::GetAccessControl($n) }}; \
                 $r = $a.GetAccessRules($true, $true, [Security.Principal.SecurityIdentifier]); \
                 $me = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value; \
                 $ok = ($r | Where-Object {{ $_.IdentityReference.Value -eq $me -or $_.IdentityReference.Value -eq 'S-1-5-32-544' }}).Count; \
                 [Console]::Out.Write([string]$a.AreAccessRulesProtected + ' ' + $r.Count + ' ' + $ok)",
                plain(&path)
            );
            let acl = command(&dir)
                .args(["-NoProfile", "-NonInteractive", "-Command", &check])
                .stdin(Stdio::null())
                .output()
                .unwrap();
            assert_eq!(stdout(&acl), "True 2 2");
        }
    }

    #[test]
    fn the_push_refuses_content_that_is_not_the_two_lines_and_keeps_the_old_credential() {
        let dir = scratch("push-refused");
        let first = run_at_home(
            &dir,
            &WINDOWS.push_script("").unwrap(),
            format!("{URL}\n{TOKEN}\n").as_bytes(),
        );
        assert_eq!(code(&first), Some(0));
        let before = std::fs::read(credential(&dir)).unwrap();
        for content in [
            format!("{URL}\n"),
            format!("{URL}\n{URL}\n"),
            "LOCALSWIM_URL=\nLOCALSWIM_TOKEN=x\n".to_string(),
            format!("\n{URL}\n{TOKEN}\n"),
            String::new(),
        ] {
            let output = run_at_home(&dir, &WINDOWS.push_script("").unwrap(), content.as_bytes());
            assert_eq!(code(&output), Some(1), "{content:?}");
            assert!(
                stdout(&output).trim().is_empty(),
                "no hash line: {content:?}"
            );
            assert_eq!(std::fs::read(credential(&dir)).unwrap(), before);
            assert_eq!(
                files_beside(&credential(&dir)),
                ["bot.env"],
                "no temporary file"
            );
        }
    }

    #[test]
    fn the_push_takes_the_lines_in_either_order_and_reads_no_more_than_two() {
        let dir = scratch("push-order");
        let content = format!("{TOKEN}\n{URL}\nLOCALSWIM_EXTRA=never-read\n");
        let output = run_at_home(&dir, &WINDOWS.push_script("").unwrap(), content.as_bytes());
        assert_eq!(code(&output), Some(0));
        assert_eq!(
            std::fs::read(credential(&dir)).unwrap(),
            format!("{TOKEN}\n{URL}\n").as_bytes()
        );
    }

    /// SHA-256, the test's own (FIPS 180-4), so the hash line is checked against no code of the
    /// script's.
    #[allow(clippy::needless_range_loop)]
    fn sha256_hex(data: &[u8]) -> String {
        const K: [u32; 64] = [
            0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
            0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
            0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
            0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
            0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
            0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
            0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
            0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
            0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
            0xc67178f2,
        ];
        let mut h: [u32; 8] = [
            0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
            0x5be0cd19,
        ];
        let mut message = data.to_vec();
        message.push(0x80);
        while message.len() % 64 != 56 {
            message.push(0);
        }
        message.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
        for block in message.chunks(64) {
            let mut w = [0u32; 64];
            for (i, word) in block.chunks(4).enumerate() {
                w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
            }
            for i in 16..64 {
                let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
                let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
                w[i] = w[i - 16]
                    .wrapping_add(s0)
                    .wrapping_add(w[i - 7])
                    .wrapping_add(s1);
            }
            let mut v = h;
            for i in 0..64 {
                let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
                let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
                let t1 = v[7]
                    .wrapping_add(s1)
                    .wrapping_add(ch)
                    .wrapping_add(K[i])
                    .wrapping_add(w[i]);
                let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
                let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
                let t2 = s0.wrapping_add(maj);
                v = [
                    t1.wrapping_add(t2),
                    v[0],
                    v[1],
                    v[2],
                    v[3].wrapping_add(t1),
                    v[4],
                    v[5],
                    v[6],
                ];
            }
            for (a, b) in h.iter_mut().zip(v) {
                *a = a.wrapping_add(b);
            }
        }
        h.iter().map(|word| format!("{word:08x}")).collect()
    }

    #[test]
    fn the_tests_sha256_is_right() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
