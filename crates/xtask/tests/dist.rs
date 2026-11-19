//! The bytes and the folder of `cargo xtask dist`. Every test works in a throwaway folder under the
//! temp folder.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use xtask::{
    DIST_FILES, DistError, FORWARDER_FILE, PROGRAM_FILE, Request, WRAPPER_FILE, choose_checkout,
    choose_profile, forwarder_bytes, parse_args, wrapper_bytes, write_dist,
};

/// A throwaway folder that removes itself.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Scratch {
        let path =
            std::env::temp_dir().join(format!("agentspace-xtask-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("A throwaway folder is made");
        Scratch(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A checkout that holds `scripts\ssh-proxy.cmd`.
fn checkout_with_target(scratch: &Scratch, folder: &str) -> PathBuf {
    let checkout = scratch.path().join(folder);
    fs::create_dir_all(checkout.join("scripts")).unwrap();
    fs::write(
        checkout.join("scripts").join("ssh-proxy.cmd"),
        "@echo off\r\n",
    )
    .unwrap();
    checkout
}

fn fake_program(scratch: &Scratch) -> PathBuf {
    let program = scratch.path().join("built.exe");
    fs::write(&program, b"program bytes").unwrap();
    program
}

fn names(folder: &Path) -> Vec<String> {
    let mut found: Vec<String> = fs::read_dir(folder)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    found.sort();
    found
}

// The wrapper.

#[test]
fn wrapper_uses_lf_only_and_starts_with_the_sh_line() {
    let bytes = wrapper_bytes();
    assert!(
        !bytes.contains(&b'\r'),
        "A carriage return is in the wrapper"
    );
    assert!(bytes.starts_with(b"#!/bin/sh\n"));
    assert!(bytes.ends_with(b"\n"));
}

#[test]
fn wrapper_turns_msys_path_conversion_off() {
    let text = String::from_utf8(wrapper_bytes()).unwrap();
    assert!(text.contains("MSYS_NO_PATHCONV=1\nexport MSYS_NO_PATHCONV\n"));
    assert!(text.contains("MSYS2_ARG_CONV_EXCL='*'\nexport MSYS2_ARG_CONV_EXCL\n"));
}

#[test]
fn wrapper_runs_the_program_from_its_own_folder_with_arguments_unchanged() {
    let text = String::from_utf8(wrapper_bytes()).unwrap();
    // The folder comes from the wrapper's own path, never from PATH.
    assert!(text.contains("dirname -- \"$0\""));
    assert!(text.contains("exec \"$here/agentspace.exe\" \"$@\"\n"));
    // A bare program name would be found through PATH, and an unquoted $@ would split arguments.
    assert!(!text.contains("exec agentspace"));
    assert!(!text.contains(" $@"));
    assert!(!text.contains("$*"));
}

/// The sh to run the wrapper with, when the machine has one.
fn find_sh() -> Option<PathBuf> {
    let mut candidates = vec![
        PathBuf::from(r"C:\Program Files\Git\bin\sh.exe"),
        PathBuf::from(r"C:\Program Files\Git\usr\bin\sh.exe"),
        PathBuf::from(r"C:\Program Files (x86)\Git\bin\sh.exe"),
        PathBuf::from("/bin/sh"),
    ];
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        candidates.push(PathBuf::from(local).join(r"Programs\Git\bin\sh.exe"));
    }
    if let Some(path) = std::env::var_os("PATH") {
        for folder in std::env::split_paths(&path) {
            candidates.push(folder.join("sh.exe"));
        }
    }
    candidates.into_iter().find(|c| c.is_file())
}

#[test]
fn wrapper_passes_arguments_and_the_exit_code_through_a_real_sh() {
    let Some(sh) = find_sh() else {
        // Git for Windows is expected on the laptop, so a missing sh there is a failure, not a pass.
        if cfg!(windows) {
            panic!(
                "No sh was found: Git for Windows is expected on this machine, so the wrapper \
                 could not be run."
            );
        }
        eprintln!("No sh was found, so the wrapper was not run.");
        return;
    };
    let scratch = Scratch::new("wrapper-run");
    // The xtask program stands in for agentspace.exe: an unknown command exits 2 and quotes it.
    let program = PathBuf::from(env!("CARGO_BIN_EXE_xtask"));
    let checkout = checkout_with_target(&scratch, "checkout");
    let dist = scratch.path().join("dist");
    write_dist(&dist, &program, &checkout).unwrap();

    let wrapper = dist.join(WRAPPER_FILE).to_string_lossy().replace('\\', "/");
    let out = Command::new(&sh)
        .arg(&wrapper)
        .arg("/x")
        .env_remove("MSYS_NO_PATHCONV")
        .env_remove("MSYS2_ARG_CONV_EXCL")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("'/x'"), "The argument changed: {stderr}");

    let out = Command::new(&sh)
        .arg(&wrapper)
        .arg("--help")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
}

// The forwarder.

#[test]
fn forwarder_is_one_line_that_runs_the_target_by_full_path_with_its_arguments() {
    let bytes = forwarder_bytes(r"C:\Projects\agentspace\scripts\ssh-proxy.cmd").unwrap();
    assert_eq!(
        bytes,
        b"@\"C:\\Projects\\agentspace\\scripts\\ssh-proxy.cmd\" %*\r\n".to_vec()
    );
    let text = String::from_utf8(bytes).unwrap();
    assert_eq!(text.lines().count(), 1, "More than one line");
    assert_eq!(text.matches('\n').count(), 1);
}

#[test]
fn forwarder_quotes_a_path_with_spaces() {
    let text =
        String::from_utf8(forwarder_bytes(r"C:\My Projects\a b\scripts\ssh-proxy.cmd").unwrap())
            .unwrap();
    assert!(text.starts_with("@\"C:\\My Projects\\a b\\scripts\\ssh-proxy.cmd\" %*"));
}

#[test]
fn forwarder_doubles_every_percent_of_the_path() {
    let text = String::from_utf8(forwarder_bytes(r"C:\100%\%PATH%\scripts\ssh-proxy.cmd").unwrap())
        .unwrap();
    assert!(text.contains(r"C:\100%%\%%PATH%%\scripts\ssh-proxy.cmd"));
    // The one `%*` after the path is the arguments, and is not doubled.
    assert!(text.trim_end().ends_with("\" %*"));
}

#[test]
fn forwarder_runs_nothing_after_the_target_and_has_no_call() {
    let text = String::from_utf8(forwarder_bytes(r"C:\c\scripts\ssh-proxy.cmd").unwrap()).unwrap();
    // `call` would return to the forwarder, which would then read its next line.
    assert!(!text.to_ascii_lowercase().contains("call"));
    // Nothing follows the arguments on the line.
    assert!(text.ends_with("%*\r\n"));
    assert!(!text.contains("&&") && !text.contains('|') && !text.contains("exit"));
}

#[test]
fn forwarder_refuses_a_path_it_cannot_quote() {
    assert_eq!(
        forwarder_bytes("C:\\a\"b\\scripts\\ssh-proxy.cmd"),
        Err(DistError::ProxyPathUnusable)
    );
    assert_eq!(
        forwarder_bytes("C:\\a\r\n&calc"),
        Err(DistError::ProxyPathUnusable)
    );
    assert_eq!(forwarder_bytes(""), Err(DistError::ProxyPathUnusable));
}

#[test]
fn forwarder_refuses_a_path_with_a_character_outside_ascii() {
    // cmd reads a batch file in the console's code page, so it cannot follow such a path.
    for target in [
        "C:\\Projets\\caf\u{e9}\\scripts\\ssh-proxy.cmd",
        "C:\\\u{4e2d}\\scripts\\ssh-proxy.cmd",
        "C:\\a\u{200b}b\\scripts\\ssh-proxy.cmd",
    ] {
        assert_eq!(
            forwarder_bytes(target),
            Err(DistError::ProxyPathUnusable),
            "{target:?}"
        );
    }
    // The same path in ASCII is fine.
    assert!(forwarder_bytes("C:\\Projets\\cafe\\scripts\\ssh-proxy.cmd").is_ok());
}

#[test]
fn forwarder_refuses_a_path_in_the_extended_length_form() {
    assert_eq!(
        forwarder_bytes(r"\\?\C:\c\scripts\ssh-proxy.cmd"),
        Err(DistError::ProxyPathUnusable)
    );
    assert_eq!(
        forwarder_bytes(r"\\?\UNC\server\share\scripts\ssh-proxy.cmd"),
        Err(DistError::ProxyPathUnusable)
    );
    // An ordinary UNC path is not that form.
    assert!(forwarder_bytes(r"\\server\share\scripts\ssh-proxy.cmd").is_ok());
}

#[test]
fn a_checkout_with_a_non_ascii_path_writes_no_forwarder() {
    let scratch = Scratch::new("non-ascii");
    let program = fake_program(&scratch);
    let checkout = checkout_with_target(&scratch, "caf\u{e9}");
    let dist = scratch.path().join("dist");
    assert_eq!(
        write_dist(&dist, &program, &checkout),
        Err(DistError::ProxyPathUnusable)
    );
    assert!(!dist.exists());
}

/// A generated forwarder, run the way a ProxyCommand runs it, against a fake target.
#[cfg(windows)]
#[test]
fn forwarder_carries_arguments_input_and_the_exit_code_through_cmd() {
    use std::io::Write;
    use std::os::windows::process::CommandExt;
    use std::process::Stdio;

    let scratch = Scratch::new("forwarder-run");
    // A space and a percent sign in the path, which a batch file reads specially.
    let checkout = scratch.path().join("my checkout%PATH%x");
    fs::create_dir_all(checkout.join("scripts")).unwrap();
    fs::write(
        checkout.join("scripts").join("ssh-proxy.cmd"),
        "@echo off\r\necho args:[%*]\r\nset /p line=\r\necho line:[%line%]\r\nexit /b 7\r\n",
    )
    .unwrap();
    let dist = scratch.path().join("dist");
    write_dist(&dist, &fake_program(&scratch), &checkout).unwrap();
    let forwarder = dist.join(FORWARDER_FILE);

    // cmd /c with a line that starts with a quote: its outer pair of quotes is removed.
    let mut child = Command::new("cmd.exe")
        .args(["/d", "/c"])
        .raw_arg(format!(
            "\"\"{}\" i-0abc \"two words\" 22\"",
            forwarder.display()
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"hello proxy\r\n")
        .unwrap();
    let out = child.wait_with_output().unwrap();

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stdout.contains("args:[i-0abc \"two words\" 22]"),
        "Arguments changed: {stdout} {stderr}"
    );
    assert!(
        stdout.contains("line:[hello proxy]"),
        "Input did not arrive: {stdout} {stderr}"
    );
    assert_eq!(out.status.code(), Some(7), "{stdout} {stderr}");
}

// The output folder.

#[test]
fn dist_holds_exactly_the_three_files_and_removes_anything_else() {
    let scratch = Scratch::new("exactly-three");
    let checkout = checkout_with_target(&scratch, "checkout");
    let program = fake_program(&scratch);
    let dist = scratch.path().join("dist");
    fs::create_dir_all(dist.join("old-folder").join("deeper")).unwrap();
    fs::write(dist.join("old-folder").join("deeper").join("x.txt"), "x").unwrap();
    fs::write(dist.join("agentspace-operator.exe"), "stale").unwrap();
    fs::write(dist.join("agentspace.exe.old-1"), "stale").unwrap();
    // A file that is one of the three is replaced, not kept.
    fs::write(dist.join(PROGRAM_FILE), "stale").unwrap();

    write_dist(&dist, &program, &checkout).unwrap();

    let mut expected: Vec<String> = DIST_FILES.iter().map(|s| s.to_string()).collect();
    expected.sort();
    assert_eq!(names(&dist), expected);
    assert_eq!(fs::read(dist.join(PROGRAM_FILE)).unwrap(), b"program bytes");
    assert_eq!(fs::read(dist.join(WRAPPER_FILE)).unwrap(), wrapper_bytes());
    let forwarder = String::from_utf8(fs::read(dist.join(FORWARDER_FILE)).unwrap()).unwrap();
    let target = checkout.join("scripts").join("ssh-proxy.cmd");
    assert_eq!(
        forwarder,
        format!(
            "@\"{}\" %*\r\n",
            target.to_str().unwrap().replace('%', "%%")
        )
    );
}

#[test]
fn dist_creates_the_folder_when_it_is_missing() {
    let scratch = Scratch::new("creates");
    let checkout = checkout_with_target(&scratch, "checkout");
    let program = fake_program(&scratch);
    let dist = scratch.path().join("target").join("dist");
    write_dist(&dist, &program, &checkout).unwrap();
    assert_eq!(names(&dist).len(), 3);
}

#[test]
fn a_checkout_without_the_target_writes_no_forwarder() {
    let scratch = Scratch::new("no-target");
    let program = fake_program(&scratch);
    let empty = scratch.path().join("empty-checkout");
    fs::create_dir_all(empty.join("scripts")).unwrap();
    let dist = scratch.path().join("dist");

    assert_eq!(
        write_dist(&dist, &program, &empty),
        Err(DistError::ProxyTargetMissing)
    );
    assert!(!dist.join(FORWARDER_FILE).exists());
    // Nothing else of the output folder is written either.
    assert!(!dist.exists() || names(&dist).is_empty());
}

#[test]
fn a_checkout_that_does_not_exist_writes_no_forwarder() {
    let scratch = Scratch::new("no-checkout");
    let program = fake_program(&scratch);
    let dist = scratch.path().join("dist");
    let result = write_dist(&dist, &program, &scratch.path().join("nowhere"));
    assert_eq!(result, Err(DistError::ProxyTargetMissing));
    assert!(!dist.join(FORWARDER_FILE).exists());
}

#[test]
fn a_target_that_is_a_folder_is_not_a_target() {
    let scratch = Scratch::new("target-is-folder");
    let program = fake_program(&scratch);
    let checkout = scratch.path().join("checkout");
    fs::create_dir_all(checkout.join("scripts").join("ssh-proxy.cmd")).unwrap();
    let dist = scratch.path().join("dist");
    assert_eq!(
        write_dist(&dist, &program, &checkout),
        Err(DistError::ProxyTargetMissing)
    );
    assert!(!dist.join(FORWARDER_FILE).exists());
}

#[test]
fn a_refusal_leaves_an_earlier_output_folder_as_it_was() {
    let scratch = Scratch::new("refusal-keeps");
    let program = fake_program(&scratch);
    let dist = scratch.path().join("dist");
    fs::create_dir_all(&dist).unwrap();
    fs::write(dist.join("leftover.txt"), "kept").unwrap();
    let empty = scratch.path().join("empty");
    fs::create_dir_all(&empty).unwrap();
    assert!(write_dist(&dist, &program, &empty).is_err());
    assert_eq!(names(&dist), vec!["leftover.txt".to_string()]);
}

#[test]
fn a_missing_build_writes_nothing() {
    let scratch = Scratch::new("no-build");
    let checkout = checkout_with_target(&scratch, "checkout");
    let dist = scratch.path().join("dist");
    let result = write_dist(&dist, &scratch.path().join("absent.exe"), &checkout);
    assert_eq!(result, Err(DistError::ProgramMissing));
    assert!(!dist.exists());
}

// The command line and its messages.

#[test]
fn the_command_line_reads_its_flags() {
    let args = |a: &[&str]| parse_args(a.iter().map(|s| s.to_string()));
    assert_eq!(
        args(&["dist"]),
        Ok(Request::Dist {
            release: false,
            proxy_checkout: None
        })
    );
    assert_eq!(
        args(&["dist", "--release", "--proxy-checkout", "C:\\x y"]),
        Ok(Request::Dist {
            release: true,
            proxy_checkout: Some(PathBuf::from("C:\\x y"))
        })
    );
    assert_eq!(
        args(&["dist", "--proxy-checkout=a"]),
        Ok(Request::Dist {
            release: false,
            proxy_checkout: Some(PathBuf::from("a"))
        })
    );
    assert_eq!(args(&["--help"]), Ok(Request::Help));
    assert!(args(&[]).is_err());
    assert!(args(&["build"]).is_err());
    assert!(args(&["dist", "--bogus"]).is_err());
    assert!(args(&["dist", "--proxy-checkout"]).is_err());
    assert!(args(&["dist", "--proxy-checkout="]).is_err());
    assert!(args(&["dist", "--proxy-checkout", ""]).is_err());
    assert!(args(&["dist", "--proxy-checkout", "a", "--proxy-checkout", "b"]).is_err());
}

#[test]
fn the_program_refuses_a_checkout_without_the_target_with_its_own_words() {
    let scratch = Scratch::new("cli-refusal");
    let empty = scratch.path().join("empty-checkout");
    fs::create_dir_all(&empty).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["dist", "--proxy-checkout"])
        .arg(&empty)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.starts_with("xtask: The proxy checkout has no scripts\\ssh-proxy.cmd"));
    // The message names no path of the machine.
    assert!(!stderr.contains(scratch.path().to_str().unwrap()));
}

#[test]
fn the_program_names_itself_xtask_in_every_error() {
    let out = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .arg("bogus")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.starts_with("xtask: Unknown command 'bogus'."),
        "{stderr}"
    );
    assert!(!stderr.contains("agentspace:"));
}

#[test]
fn an_empty_proxy_checkout_is_a_usage_error_with_exit_2() {
    for args in [
        vec!["dist", "--proxy-checkout="],
        vec!["dist", "--proxy-checkout", ""],
    ] {
        let out = Command::new(env!("CARGO_BIN_EXE_xtask"))
            .args(&args)
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert!(out.stdout.is_empty());
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.starts_with("xtask: --proxy-checkout needs a folder."),
            "{stderr}"
        );
        assert!(stderr.contains("Usage:"));
    }
}

// Choosing the checkout and the profile.

#[test]
fn the_checkout_is_the_workspace_root_when_none_is_given() {
    let root = Path::new("workspace-root");
    assert_eq!(choose_checkout(None, root), Ok(root.to_path_buf()));
}

#[test]
fn a_given_checkout_wins_over_the_workspace_root_and_becomes_a_full_path() {
    let scratch = Scratch::new("choose-checkout");
    let given = scratch.path().join("main checkout");
    let chosen = choose_checkout(Some(&given), Path::new("workspace-root")).unwrap();
    assert_eq!(chosen, given);

    let relative = choose_checkout(Some(Path::new("some-folder")), Path::new("workspace-root"));
    let relative = relative.unwrap();
    assert!(relative.is_absolute(), "{relative:?}");
    assert!(relative.ends_with("some-folder"));
}

#[test]
fn a_checkout_path_that_cannot_be_made_full_is_refused() {
    assert_eq!(
        choose_checkout(Some(Path::new("")), Path::new("workspace-root")),
        Err(DistError::CheckoutPathUnreadable)
    );
}

#[test]
fn the_profile_is_debug_unless_release_is_asked_for() {
    assert_eq!(choose_profile(false), "debug");
    assert_eq!(choose_profile(true), "release");
}
