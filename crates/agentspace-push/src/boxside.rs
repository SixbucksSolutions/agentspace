//! The ssh's arguments, the box's half as a remote command, and what the laptop checks in the
//! box's answer (operator.md, What the laptop sends, What the box does, What the laptop checks).

/// `-Alias`'s rule: `^[A-Za-z0-9._-]+\z`, case-sensitively, not starting with `-`.
pub fn alias_is_valid(alias: &str) -> bool {
    !alias.is_empty()
        && !alias.starts_with('-')
        && alias
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// The remote command that makes the login user's POSIX shell on today's box carry out the box's
/// half of the protocol (What the box does), naming `zone` when it is not empty. `zone` MUST be a
/// zone [`crate::check_zone`] returned; one that fails that check is never put into the command,
/// which is then the command for the empty zone. The command carries no part of the content, and
/// holds no `%`, no double quote and no control character, and does not end in a backslash.
///
/// Step by step:
/// - `umask 077`, so the folders `mkdir -p` creates are 0700 and every file 0600 from the moment
///   it exists; a folder that already exists keeps its mode. The folder is entered by a tilde
///   path, which the shell does not split, so a home with a space in it works without quotes.
/// - `mktemp` makes the temporary file with a name no other run uses, and the `EXIT` trap removes
///   it on every exit but the rename, which consumes it; a hang-up, an interrupt, a termination or
///   a broken pipe exits through that trap.
/// - `timeout 30 head -n 2` reads at most two lines and at most 30 seconds; then the CRs go, then
///   a UTF-8 BOM at the start of the first line, then every empty line.
/// - Exactly two lines (each ending in LF), one starting `LOCALSWIM_URL=` and one
///   `LOCALSWIM_TOKEN=`, each with at least one character after it, or exit 1.
/// - The rename over `bot.env`, then `sha256sum` of the installed file, `<hash>  -`.
/// - With a zone, `timedatectl` through `sudo -n`, given up after 5 seconds and killed 1 second
///   later, with no input and its output discarded; then `zone <zone>` or `zone-failed`, and
///   exit 0 whatever the zone did.
pub fn remote_command(zone: &str) -> String {
    let mut cmd = String::from(
        "umask 077; export LC_ALL=C; t=; trap 'rm -f -- $t' EXIT; trap 'exit 1' HUP INT TERM PIPE; \
         mkdir -p ~/.config/localswim && cd ~/.config/localswim || exit 1; \
         t=$(mktemp bot.env.XXXXXXXXXX) || exit 1; \
         timeout 30 head -n 2 | tr -d '\\r' | sed '1s/^\\xEF\\xBB\\xBF//' | sed '/^$/d' >$t || exit 1; \
         test $(wc -l <$t) -eq 2 || exit 1; \
         grep -q '^LOCALSWIM_URL=.' $t || exit 1; \
         grep -q '^LOCALSWIM_TOKEN=.' $t || exit 1; \
         mv -f -- $t bot.env || exit 1; t=; \
         sha256sum <bot.env || exit 1;",
    );
    // The last guard: only a zone that passes step 3's check is ever put on the box's command
    // line (operator.md, The time zone), whoever calls this.
    if !agentspace_zone::is_valid_zone(zone) {
        cmd.push_str(" exit 0");
    } else {
        cmd.push_str(&format!(
            " if timeout -k 1 5 sudo -n timedatectl set-timezone {zone} </dev/null >/dev/null 2>&1; \
             then echo zone {zone}; else echo zone-failed; fi; exit 0"
        ));
    }
    cmd
}

/// The push's ssh arguments: `-o ClearAllForwardings=yes -o BatchMode=yes <alias> <remote
/// command>`, the remote command one argument and the last.
pub fn ssh_args(alias: &str, remote_command: &str) -> Vec<String> {
    vec![
        "-o".to_owned(),
        "ClearAllForwardings=yes".to_owned(),
        "-o".to_owned(),
        "BatchMode=yes".to_owned(),
        alias.to_owned(),
        remote_command.to_owned(),
    ]
}

/// The ssh's stdout split at each LF: each line without its LF, and whether an LF ended it. A CR
/// just before an LF stays on the line here; the callers below say what it means to each.
fn lines(stdout: &[u8]) -> impl Iterator<Item = (&[u8], bool)> {
    let ends_in_lf = stdout.last() == Some(&b'\n');
    let mut parts: Vec<&[u8]> = stdout.split(|b| *b == b'\n').collect();
    if ends_in_lf {
        // The empty piece after the last LF is not a line.
        parts.pop();
    }
    let count = parts.len();
    parts
        .into_iter()
        .enumerate()
        .map(move |(i, line)| (line, i + 1 < count || ends_in_lf))
}

/// A hash line: its first 64 characters are hex digits, and its 65th is a space, a tab, a CR, or
/// the LF that ends it. Returns the 64 digits.
///
/// The box's last stdout line counts as a line even with no LF after it, so 64 hex digits that
/// end the output are a hash line too. A line cut short by the 10-second bound is not 64 hex
/// digits, so it is no hash line (operator.md, What the laptop checks; architect, 2026-11-19, for
/// E0.06-T01-S04).
fn hash_of(line: &[u8]) -> Option<&[u8]> {
    let digits = line.get(..64)?;
    if !digits.iter().all(u8::is_ascii_hexdigit) {
        return None;
    }
    match line.get(64) {
        // The line ends after the digits: at its LF, or at the end of the output.
        None | Some(b' ' | b'\t' | b'\r') => Some(digits),
        Some(_) => None,
    }
}

/// What the box's answer confirmed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Answer {
    /// No confirmed token: a non-zero exit, or no first hash line carrying the expected hash.
    NotConfirmed,
    /// The token, but not the zone that was sent.
    ZoneNotTaken,
    /// The token and, when one was sent, the zone.
    Confirmed,
}

/// Checks the box's answer: the token, by the exit code and the first hash line; then the zone,
/// when one was sent, by a line that is exactly `zone <zone>` once its LF or CRLF is removed.
pub(crate) fn check_answer(code: i32, stdout: &[u8], expected_hash: &str, zone: &str) -> Answer {
    if code != 0 {
        return Answer::NotConfirmed;
    }
    let first = lines(stdout).find_map(|(line, _)| hash_of(line));
    match first {
        Some(h) if h.eq_ignore_ascii_case(expected_hash.as_bytes()) => {}
        _ => return Answer::NotConfirmed,
    }
    if zone.is_empty() {
        return Answer::Confirmed;
    }
    let wanted = format!("zone {zone}");
    let taken = lines(stdout).any(|(line, lf)| {
        let line = if lf {
            line.strip_suffix(b"\r").unwrap_or(line)
        } else {
            line
        };
        line == wanted.as_bytes()
    });
    if taken {
        Answer::Confirmed
    } else {
        Answer::ZoneNotTaken
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: &str = "1d171cf9f0a8d9965c97eb40f0ae2d7ee8b8afdd5486b25857521a4958f3666c";

    #[test]
    fn alias_rule() {
        for ok in ["agentspace", "a.b_c-d", "A1"] {
            assert!(alias_is_valid(ok), "{ok}");
        }
        for bad in ["", "-x", "a b", "a\n", "a;b", "ä", "a/b"] {
            assert!(!alias_is_valid(bad), "{bad:?}");
        }
    }

    #[test]
    fn remote_command_survives_one_pair_of_double_quotes() {
        for zone in ["", "America/New_York", "Etc/GMT+5"] {
            let c = remote_command(zone);
            assert!(!c.contains('%'), "{c}");
            assert!(!c.contains('"'), "{c}");
            assert!(!c.chars().any(char::is_control), "{c}");
            assert!(!c.ends_with('\\'), "{c}");
            assert_eq!(
                c.contains(&format!("zone {zone};")),
                !zone.is_empty(),
                "{c}"
            );
        }
        assert!(!remote_command("").contains("timedatectl"));
        // A zone that fails step 3's check is never put into the command, whoever asks.
        for bad in [
            "-UTC",
            "UTC; reboot",
            "a b",
            "a'b",
            "$(x)",
            "a\"b",
            "a%b",
            "UTC\n",
        ] {
            assert_eq!(remote_command(bad), remote_command(""), "{bad:?}");
        }
    }

    #[test]
    fn args() {
        assert_eq!(
            ssh_args("agentspace", "cmd x"),
            [
                "-o",
                "ClearAllForwardings=yes",
                "-o",
                "BatchMode=yes",
                "agentspace",
                "cmd x"
            ]
        );
    }

    #[test]
    fn hash_lines() {
        let ok = |s: &str| check_answer(0, s.as_bytes(), H, "");
        assert_eq!(ok(&format!("{H}  -\n")), Answer::Confirmed);
        assert_eq!(ok(&format!("{H}\n")), Answer::Confirmed);
        assert_eq!(ok(&format!("{H}\r\n")), Answer::Confirmed);
        assert_eq!(ok(&format!("{H}\t-")), Answer::Confirmed);
        // The last line counts as a line even with no LF after it (architect, 2026-11-19).
        assert_eq!(
            ok(&H.to_uppercase()),
            Answer::Confirmed,
            "A hash line that ends the output with no LF"
        );
        assert_eq!(ok(&format!("Welcome\n{H}")), Answer::Confirmed);
        assert_eq!(ok(&format!("{H}  -")), Answer::Confirmed);
        // A line cut short is not 64 hex digits, so it is no hash line.
        assert_eq!(ok(&H[..63]), Answer::NotConfirmed);
        assert_eq!(ok(&format!("Welcome\n{}", &H[..40])), Answer::NotConfirmed);
        // An earlier whole hash line is still the first one; a cut one after it changes nothing.
        assert_eq!(ok(&format!("{H}  -\n{}", &H[..10])), Answer::Confirmed);
        assert_eq!(
            ok(&format!("Welcome\n{}  -\n", H.to_uppercase())),
            Answer::Confirmed
        );
        assert_eq!(ok(&format!("{H}x\n")), Answer::NotConfirmed);
        assert_eq!(
            ok(&format!("{}  -\n{H}  -\n", "0".repeat(64))),
            Answer::NotConfirmed,
            "Only the first hash line counts"
        );
        assert_eq!(ok(""), Answer::NotConfirmed);
        assert_eq!(
            check_answer(1, format!("{H}  -\n").as_bytes(), H, ""),
            Answer::NotConfirmed
        );
        assert_eq!(check_answer(255, b"", H, "UTC"), Answer::NotConfirmed);
    }

    #[test]
    fn zone_lines() {
        let z = |s: &str| check_answer(0, format!("{H}  -\n{s}").as_bytes(), H, "America/New_York");
        assert_eq!(z("zone America/New_York\n"), Answer::Confirmed);
        assert_eq!(z("zone America/New_York\r\n"), Answer::Confirmed);
        assert_eq!(z("zone America/New_York"), Answer::Confirmed);
        assert_eq!(z("zone-failed\n"), Answer::ZoneNotTaken);
        assert_eq!(z(""), Answer::ZoneNotTaken);
        assert_eq!(z("zone america/new_york\n"), Answer::ZoneNotTaken);
        assert_eq!(z("zone America/New_York \n"), Answer::ZoneNotTaken);
        assert_eq!(z("zone America/New_York\r"), Answer::ZoneNotTaken);
    }

    /// The box's half, carried out for real by a POSIX shell in a throwaway home (operator.md,
    /// What the box does). Linux alone, where today's box's tools are.
    ///
    /// The shell's `PATH` is a throwaway folder holding links to the tools the command uses and a
    /// stand-in `sudo`, and nothing else. So the real `sudo` and `timedatectl` cannot be reached,
    /// and the system's zone is never touched. `HOME`, `USERPROFILE`, `LOCALAPPDATA` and
    /// `APPDATA` point into the throwaway folder, and every value is a dummy.
    #[cfg(target_os = "linux")]
    mod on_a_shell {
        use super::super::remote_command;
        use crate::Content;
        use crate::stamp::box_hash;
        use std::io::Write;
        use std::os::unix::fs::PermissionsExt;
        use std::path::{Path, PathBuf};
        use std::process::{Command, Stdio};
        use zeroize::Zeroizing;

        const TOOLS: [&str; 12] = [
            "mkdir",
            "mktemp",
            "timeout",
            "head",
            "tr",
            "sed",
            "wc",
            "grep",
            "mv",
            "sha256sum",
            "rm",
            "sleep",
        ];
        const URL_LINE: &str = "LOCALSWIM_URL=http://127.0.0.1:8811";
        const TOKEN_LINE: &str = "LOCALSWIM_TOKEN=dummy-token-not-real";
        const ZONE: &str = "America/Los_Angeles";

        /// The stand-in `sudo`: it records its arguments, records whether it was given any input,
        /// prints to both streams, sleeps and exits as it is told.
        const SUDO: &str = "#!/bin/sh\n\
            echo \"$*\" >>\"$STUB_LOG\"\n\
            if read -r line; then echo got-input >>\"$STUB_LOG\"; fi\n\
            echo noise-on-stdout\n\
            echo noise-on-stderr >&2\n\
            if [ \"$STUB_SLEEP\" != 0 ]; then sleep \"$STUB_SLEEP\"; fi\n\
            exit \"$STUB_EXIT\"\n";

        fn content() -> String {
            format!("{URL_LINE}\n{TOKEN_LINE}\n")
        }

        fn hash_line(installed: &str) -> String {
            let hash = box_hash(&Content(Zeroizing::new(installed.as_bytes().to_vec())));
            format!("{hash}  -\n")
        }

        fn mode(path: &Path) -> u32 {
            std::fs::metadata(path).unwrap().permissions().mode() & 0o777
        }

        /// A tool's file, found on the test's own `PATH`.
        fn find(tool: &str) -> PathBuf {
            let path = std::env::var_os("PATH").expect("PATH is set");
            std::env::split_paths(&path)
                .map(|dir| dir.join(tool))
                .find(|file| file.is_file())
                .unwrap_or_else(|| panic!("{tool} is not on PATH"))
        }

        /// The shells to run the command with: `sh`, and `bash` where there is one.
        fn shells() -> Vec<&'static str> {
            ["/bin/sh", "/bin/bash"]
                .into_iter()
                .filter(|shell| Path::new(shell).is_file())
                .collect()
        }

        /// How one run ended.
        struct Ended {
            code: Option<i32>,
            stdout: String,
            stderr: String,
        }

        /// A throwaway box: a home, and the folder of tools that is its whole `PATH`.
        struct ThrowawayBox {
            root: PathBuf,
        }

        impl ThrowawayBox {
            fn new(name: &str) -> Self {
                let root = std::env::temp_dir().join(format!(
                    "agentspace-push-box-{name}-{}-{}",
                    std::process::id(),
                    crate::unique_hex()
                ));
                let b = ThrowawayBox { root };
                std::fs::create_dir_all(b.home()).unwrap();
                std::fs::create_dir_all(b.bin()).unwrap();
                for tool in TOOLS {
                    std::os::unix::fs::symlink(find(tool), b.bin().join(tool)).unwrap();
                }
                let sudo = b.bin().join("sudo");
                std::fs::write(&sudo, SUDO).unwrap();
                std::fs::set_permissions(&sudo, std::fs::Permissions::from_mode(0o755)).unwrap();
                b
            }
            fn home(&self) -> PathBuf {
                self.root.join("home")
            }
            fn bin(&self) -> PathBuf {
                self.root.join("bin")
            }
            fn folder(&self) -> PathBuf {
                self.home().join(".config").join("localswim")
            }
            fn bot_env(&self) -> PathBuf {
                self.folder().join("bot.env")
            }
            fn sudo_log(&self) -> PathBuf {
                self.root.join("sudo.log")
            }
            /// The names in the credential file's folder, sorted.
            fn names(&self) -> Vec<String> {
                let mut names: Vec<String> = std::fs::read_dir(self.folder())
                    .unwrap()
                    .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                    .collect();
                names.sort();
                names
            }

            fn run(&self, shell: &str, zone: &str, stdin: &[u8], sudo: (i32, u32)) -> Ended {
                let home = self.home();
                let mut child = Command::new(shell)
                    .arg("-c")
                    .arg(remote_command(zone))
                    .env_clear()
                    .env("PATH", self.bin())
                    .env("HOME", &home)
                    .env("USERPROFILE", &home)
                    .env("LOCALAPPDATA", home.join("lad"))
                    .env("APPDATA", home.join("ad"))
                    .env("STUB_LOG", self.sudo_log())
                    .env("STUB_EXIT", sudo.0.to_string())
                    .env("STUB_SLEEP", sudo.1.to_string())
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .unwrap();
                // A box that stops reading early closes the pipe; that is not this test's failure.
                let _ = child.stdin.take().unwrap().write_all(stdin);
                let out = child.wait_with_output().unwrap();
                Ended {
                    code: out.status.code(),
                    stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
                    stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
                }
            }
        }

        impl Drop for ThrowawayBox {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.root);
            }
        }

        #[test]
        fn the_token_is_installed_and_its_hash_printed() {
            for shell in shells() {
                let b = ThrowawayBox::new("install");
                // Steps 1 to 6 and 8: the two lines installed, readable by the login user alone,
                // in folders the box created for that user alone; the hash on a line of its own.
                let r = b.run(shell, "", content().as_bytes(), (0, 0));
                assert_eq!(r.code, Some(0), "{shell}");
                assert_eq!(r.stdout, hash_line(&content()), "{shell}");
                assert_eq!(r.stderr, "", "{shell}");
                assert_eq!(std::fs::read_to_string(b.bot_env()).unwrap(), content());
                assert_eq!(mode(&b.bot_env()), 0o600, "{shell}");
                assert_eq!(mode(&b.folder()), 0o700, "{shell}");
                assert_eq!(mode(&b.home().join(".config")), 0o700, "{shell}");
                assert_eq!(b.names(), ["bot.env"], "{shell}: no temporary file");

                // Step 2: every CR removed, a BOM at the start of the first line removed. The
                // hash is the installed file's, which is what the laptop expects.
                let other = format!("{URL_LINE}/other\n{TOKEN_LINE}-2\n");
                let sent = format!("\u{FEFF}{}", other.replace('\n', "\r\n"));
                let r = b.run(shell, "", sent.as_bytes(), (0, 0));
                assert_eq!(r.code, Some(0), "{shell}");
                assert_eq!(r.stdout, hash_line(&other), "{shell}");
                assert_eq!(std::fs::read_to_string(b.bot_env()).unwrap(), other);
                assert_eq!(b.names(), ["bot.env"], "{shell}");

                // Step 1: at most two lines are read. Step 4: the count and each prefix are
                // checked, not their order.
                let swapped = format!("{TOKEN_LINE}\n{URL_LINE}\n");
                let sent = format!("{swapped}LOCALSWIM_EXTRA=never-read\n");
                let r = b.run(shell, "", sent.as_bytes(), (0, 0));
                assert_eq!(r.code, Some(0), "{shell}");
                assert_eq!(r.stdout, hash_line(&swapped), "{shell}");
                assert_eq!(std::fs::read_to_string(b.bot_env()).unwrap(), swapped);
                assert!(!b.sudo_log().exists(), "{shell}: no zone, so no setter");
            }
        }

        #[test]
        fn refused_content_leaves_the_old_file_and_no_temporary_file_and_exits_non_zero() {
            let refused = [
                String::new(),
                format!("{URL_LINE}\n"),
                format!("{TOKEN_LINE}\n"),
                // The two lines are taken before empty lines are dropped, so a blank line ahead
                // of them leaves one line.
                format!("\n{}", content()),
                format!("\r\n{}", content()),
                format!("LOCALSWIM_URL=\n{TOKEN_LINE}\n"),
                format!("{URL_LINE}\nLOCALSWIM_TOKEN=\n"),
                format!("{URL_LINE}\n{URL_LINE}\n"),
                format!("{TOKEN_LINE}\n{TOKEN_LINE}\n"),
                format!("X{URL_LINE}\n{TOKEN_LINE}\n"),
                format!("{URL_LINE}\n {TOKEN_LINE}\n"),
                // A second line with no LF after it is not known to be whole, so it is refused.
                format!("{URL_LINE}\n{TOKEN_LINE}"),
            ];
            for shell in shells() {
                let b = ThrowawayBox::new("refuse");
                // Before any credential file exists, and then with one in place.
                for installed in [false, true] {
                    if installed {
                        let r = b.run(shell, "", content().as_bytes(), (0, 0));
                        assert_eq!(r.code, Some(0), "{shell}");
                    }
                    for sent in &refused {
                        // With a zone named, so that a refusal is seen to stop before the zone.
                        let r = b.run(shell, ZONE, sent.as_bytes(), (0, 0));
                        assert_eq!(r.code, Some(1), "{shell} {sent:?}");
                        assert_eq!(r.stdout, "", "{shell} {sent:?}: no hash line");
                        assert!(!r.stderr.contains("dummy-token"), "{shell} {sent:?}");
                        assert!(!b.sudo_log().exists(), "{shell} {sent:?}: no setter");
                        if installed {
                            assert_eq!(std::fs::read_to_string(b.bot_env()).unwrap(), content());
                            assert_eq!(b.names(), ["bot.env"], "{shell} {sent:?}");
                        } else {
                            assert!(b.names().is_empty(), "{shell} {sent:?}");
                        }
                    }
                }
            }
        }

        #[test]
        fn a_folder_that_already_exists_keeps_its_mode() {
            let b = ThrowawayBox::new("folder");
            std::fs::create_dir_all(b.folder()).unwrap();
            std::fs::set_permissions(b.folder(), std::fs::Permissions::from_mode(0o755)).unwrap();
            let r = b.run("/bin/sh", "", content().as_bytes(), (0, 0));
            assert_eq!(r.code, Some(0));
            assert_eq!(mode(&b.folder()), 0o755);
            assert_eq!(mode(&b.bot_env()), 0o600);
        }

        #[test]
        fn the_zone_is_set_after_the_hash_and_answered_on_a_line_of_its_own() {
            for shell in shells() {
                // Step 7: the setter through `sudo -n`, with no input and its output discarded;
                // then `zone <zone>`, and exit 0.
                let b = ThrowawayBox::new("zone");
                let r = b.run(shell, ZONE, content().as_bytes(), (0, 0));
                assert_eq!(r.code, Some(0), "{shell}");
                assert_eq!(
                    r.stdout,
                    format!("{}zone {ZONE}\n", hash_line(&content())),
                    "{shell}"
                );
                assert_eq!(r.stderr, "", "{shell}");
                assert_eq!(
                    std::fs::read_to_string(b.sudo_log()).unwrap(),
                    format!("-n timedatectl set-timezone {ZONE}\n"),
                    "{shell}: the setter's arguments, and no input"
                );
                assert_eq!(std::fs::read_to_string(b.bot_env()).unwrap(), content());

                // A setter that fails: `zone-failed`, the token stays installed, and exit 0.
                let b = ThrowawayBox::new("zone-failed");
                let r = b.run(shell, ZONE, content().as_bytes(), (1, 0));
                assert_eq!(r.code, Some(0), "{shell}");
                assert_eq!(
                    r.stdout,
                    format!("{}zone-failed\n", hash_line(&content())),
                    "{shell}"
                );
                assert_eq!(r.stderr, "", "{shell}");
                assert_eq!(std::fs::read_to_string(b.bot_env()).unwrap(), content());
            }
        }

        #[test]
        fn a_setter_that_hangs_is_given_up_after_five_seconds() {
            let b = ThrowawayBox::new("zone-hangs");
            let started = std::time::Instant::now();
            let r = b.run("/bin/sh", ZONE, content().as_bytes(), (0, 60));
            let took = started.elapsed();
            assert_eq!(r.code, Some(0));
            assert_eq!(r.stdout, format!("{}zone-failed\n", hash_line(&content())));
            assert!(took.as_secs() >= 4 && took.as_secs() < 30, "{took:?}");
        }
    }
}
