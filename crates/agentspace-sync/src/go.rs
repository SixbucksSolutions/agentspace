//! `sync`'s localswim build (docs/operator.md, sync, step 7; gap FB-14).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! - **The build MUST be exactly `go build -trimpath -o <out> ./cmd/localswim`**, run in the
//!   localswim checkout with `GOOS=<os>`, `GOARCH=<arch>` and `CGO_ENABLED=0`, the host's
//!   platform's `GOOS` and `GOARCH` from the seam's sync row (`linux` and `arm64` on
//!   `linux/arm64`, as before v0.12; D94), `<out>` the output file's full path, and no other flag
//!   (architect, 2026-11-19, FB-14).
//! - **Those three variables are set on the build's own environment alone**, so nothing else the
//!   operator starts sees them (operator.md, Environment variables).
//! - **The build's environment goes through `agentspace-run`'s [`prepare_child_env`]**: every
//!   `*_POSIX_FD_STATE` variable removed and `AGENTSPACE_NESTED=1` (architect's ruling,
//!   2026-11-19).
//! - **Its output passes through** to the operator's stdout and stderr as `go` writes it, and it
//!   gets a closed stdin (operator.md, Streams).

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use agentspace_operator_api::LocalswimTarget;
use agentspace_run::prepare_child_env;

/// The localswim checkout the CLI is built from: localswim's, fixed by operator.md.
pub(crate) const LOCALSWIM_CHECKOUT: &str = r"C:\Projects\localswim";

/// `go` on `PATH` (operator.md, Programs it starts), as `path` is that variable's value: the first
/// folder that holds a file the system can run under one of `go`'s names.
///
/// The pinned `sync` reads `PATH` through a lookup over the process's environment, as the AWS
/// CLI's fallback is found, and reads nothing else of the environment (operator.md, The test seam,
/// "Not covered"; Environment variables, the `PATH` row).
pub(crate) fn find_on_path(path: Option<OsString>) -> Option<PathBuf> {
    let path = path?;
    let names: &[&str] = if cfg!(windows) {
        &["go.exe", "go.cmd", "go.bat"]
    } else {
        &["go"]
    };
    std::env::split_paths(&path)
        .filter(|dir| !dir.as_os_str().is_empty())
        .flat_map(|dir| names.iter().map(move |name| dir.join(name)))
        .find(|candidate| is_runnable_file(candidate))
}

fn is_runnable_file(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o111 == 0 {
            return false;
        }
    }
    true
}

/// FB-14's build, made from `cmd` (a [`Command`] for `go`), for `target`'s `GOOS` and `GOARCH`: its
/// arguments, working folder, environment and streams.
pub(crate) fn build_command(
    mut cmd: Command,
    checkout: &Path,
    out: &Path,
    target: &LocalswimTarget,
) -> Command {
    cmd.arg("build")
        .arg("-trimpath")
        .arg("-o")
        .arg(out)
        .arg("./cmd/localswim")
        .current_dir(checkout)
        .env("GOOS", target.goos)
        .env("GOARCH", target.goarch)
        .env("CGO_ENABLED", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    prepare_child_env(&mut cmd);
    cmd
}

/// Runs the build with `go` for `target`. True when it started and exited 0.
pub(crate) fn build(go: &Path, checkout: &Path, out: &Path, target: &LocalswimTarget) -> bool {
    build_command(Command::new(go), checkout, out, target)
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{Scratch, exit_stand_in};
    use agentspace_operator_api::HostPlatform;

    /// The localswim target of `platform`'s sync row.
    fn target_of(platform: &dyn HostPlatform) -> LocalswimTarget {
        platform.sync_plan().unwrap().localswim
    }

    /// `linux/arm64`'s target, every host's before v0.12.
    fn arm64() -> LocalswimTarget {
        target_of(&agentspace_platforms::LINUX_ARM64)
    }

    #[test]
    fn go_is_found_on_path_only_as_a_runnable_file() {
        let dir = Scratch::new("go-path");
        let empty = dir.path().join("empty");
        let with_go = dir.path().join("bin");
        std::fs::create_dir_all(&empty).unwrap();
        std::fs::create_dir_all(&with_go).unwrap();
        // A folder named like go is not go.
        std::fs::create_dir_all(empty.join(if cfg!(windows) { "go.exe" } else { "go" })).unwrap();
        let go = exit_stand_in(&with_go, "go", 0);
        let path = std::env::join_paths([&empty, &with_go]).unwrap();
        assert_eq!(find_on_path(Some(path)), Some(go));
        let only_empty = std::env::join_paths([&empty]).unwrap();
        assert_eq!(find_on_path(Some(only_empty)), None);
        assert_eq!(find_on_path(None), None);
    }

    /// FB-14's command line, folder and variables, and the environment rules: a `go` stand-in
    /// records what it saw. A `*_POSIX_FD_STATE` variable set on the command is gone.
    #[test]
    fn the_build_is_exactly_fb14s_with_the_operators_environment() {
        let dir = Scratch::new("go-build");
        let checkout = dir.path().join("localswim");
        std::fs::create_dir_all(&checkout).unwrap();
        let record = dir.path().join("record.txt");
        let body = if cfg!(windows) {
            format!(
                "@echo off\r\n\
                 > \"{r}\" echo cwd=%CD%\r\n\
                 >> \"{r}\" echo args=%*\r\n\
                 >> \"{r}\" echo goos=%GOOS% goarch=%GOARCH% cgo=%CGO_ENABLED%\r\n\
                 >> \"{r}\" echo nested=%AGENTSPACE_NESTED%\r\n\
                 if defined ABC_POSIX_FD_STATE (>> \"{r}\" echo fd=set) else (>> \"{r}\" echo fd=unset)\r\n\
                 exit /b 0",
                r = record.display()
            )
        } else {
            format!(
                "{{ echo \"cwd=$(pwd)\"; echo \"args=$*\"; \
                 echo \"goos=$GOOS goarch=$GOARCH cgo=$CGO_ENABLED\"; \
                 echo \"nested=$AGENTSPACE_NESTED\"; \
                 if [ -n \"${{ABC_POSIX_FD_STATE+x}}\" ]; then echo fd=set; else echo fd=unset; fi; }} > '{}'",
                record.display()
            )
        };
        let go = crate::tests::raw_stand_in(dir.path(), "go", &body);
        let out = dir.path().join("out").join("localswim-linux-arm64");
        let mut cmd = Command::new(&go);
        cmd.env("ABC_POSIX_FD_STATE", "x");
        let status = build_command(cmd, &checkout, &out, &arm64())
            .stdout(Stdio::null())
            .status()
            .unwrap();
        assert!(status.success());
        let got = std::fs::read_to_string(&record).unwrap();
        let lines: Vec<&str> = got.lines().map(str::trim_end).collect();
        let cwd = lines[0].strip_prefix("cwd=").unwrap();
        assert_eq!(
            std::fs::canonicalize(cwd).unwrap(),
            std::fs::canonicalize(&checkout).unwrap()
        );
        // cmd.exe's `%*` shows the quotes Rust puts around a batch file's argument; a program
        // receives the path whole without them.
        assert_eq!(
            lines[1].replace('"', ""),
            format!("args=build -trimpath -o {} ./cmd/localswim", out.display())
        );
        assert_eq!(lines[2], "goos=linux goarch=arm64 cgo=0");
        assert_eq!(lines[3], "nested=1");
        assert_eq!(lines[4], "fd=unset");
    }

    #[test]
    fn a_failed_or_unstartable_build_is_false() {
        let dir = Scratch::new("go-fail");
        let go = exit_stand_in(dir.path(), "go", 3);
        assert!(!build(&go, dir.path(), &dir.path().join("o"), &arm64()));
        assert!(!build(
            &dir.path().join("missing"),
            dir.path(),
            &dir.path().join("o"),
            &arm64()
        ));
        let ok = exit_stand_in(dir.path(), "go2", 0);
        assert!(build(&ok, dir.path(), &dir.path().join("o"), &arm64()));
    }

    /// The build takes its target's `GOOS` and `GOARCH`, each host's own (D94): `linux/amd64`'s
    /// and `windows/amd64`'s, beside `linux/arm64`'s above. A stand-in `go` records them.
    #[test]
    fn the_build_takes_the_targets_goos_and_goarch() {
        let dir = Scratch::new("go-targets");
        let checkout = dir.path().join("localswim");
        std::fs::create_dir_all(&checkout).unwrap();
        let record = dir.path().join("record.txt");
        let body = if cfg!(windows) {
            format!(
                "@echo off\r\n> \"{r}\" echo goos=%GOOS% goarch=%GOARCH% cgo=%CGO_ENABLED%\r\nexit /b 0",
                r = record.display()
            )
        } else {
            format!(
                "echo \"goos=$GOOS goarch=$GOARCH cgo=$CGO_ENABLED\" > '{}'",
                record.display()
            )
        };
        let go = crate::tests::raw_stand_in(dir.path(), "go", &body);
        let windows = LocalswimTarget {
            goos: "windows",
            goarch: "amd64",
            file_name: "localswim-windows-amd64.exe",
        };
        for (target, want) in [
            (
                target_of(&agentspace_platforms::LINUX_AMD64),
                "goos=linux goarch=amd64 cgo=0",
            ),
            (windows, "goos=windows goarch=amd64 cgo=0"),
            (arm64(), "goos=linux goarch=arm64 cgo=0"),
        ] {
            let out = dir.path().join(target.file_name);
            let status = build_command(Command::new(&go), &checkout, &out, &target)
                .stdout(Stdio::null())
                .status()
                .unwrap();
            assert!(status.success());
            let got = std::fs::read_to_string(&record).unwrap();
            assert_eq!(got.trim_end(), want);
        }
    }
}
