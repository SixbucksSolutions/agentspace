//! The bytes and the folder of `cargo xtask dist`. Every test works in a throwaway folder under the
//! temp folder, and every child process gets a throwaway home.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use xtask::{
    DIST_FILES, DistError, FORWARDER_FILE, KEEPER_TASK_FILE, PROGRAM_FILE, PROGRAMS, Request,
    WRAPPER_FILE, choose_profile, forwarder_bytes, parse_args, pe_subsystem, wrapper_bytes,
    write_dist,
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

/// Bytes that hold just enough of a PE file to carry a Subsystem field.
fn fake_pe(subsystem: u16) -> Vec<u8> {
    let pe = 0x80usize;
    let mut bytes = vec![0u8; pe + 24 + 70];
    bytes[0x3C..0x40].copy_from_slice(&(pe as u32).to_le_bytes());
    bytes[pe..pe + 4].copy_from_slice(b"PE\0\0");
    bytes[pe + 24 + 68..pe + 24 + 70].copy_from_slice(&subsystem.to_le_bytes());
    bytes
}

/// A build profile's folder with the six programs, a console stand-in for each but the launcher,
/// and files that are not in the table.
fn fake_built(scratch: &Scratch, name: &str) -> PathBuf {
    let built = scratch.path().join(name);
    fs::create_dir_all(&built).unwrap();
    for program in PROGRAMS {
        let bytes = if program.file == KEEPER_TASK_FILE {
            fake_pe(2)
        } else {
            let mut bytes = fake_pe(3);
            bytes.extend_from_slice(program.file.as_bytes());
            bytes
        };
        fs::write(built.join(program.file), bytes).unwrap();
    }
    for extra in [
        "agentspace-launch-testhost.exe",
        "agentspace-keeper-task-stub-keeper.exe",
        "xtask.exe",
        "agentspace.pdb",
        "agentspace-cli.d",
    ] {
        fs::write(built.join(extra), "not shipped").unwrap();
    }
    fs::create_dir_all(built.join("deps")).unwrap();
    fs::write(built.join("deps").join("agentspace.exe"), "not shipped").unwrap();
    built
}

fn names(folder: &Path) -> Vec<String> {
    let mut found: Vec<String> = fs::read_dir(folder)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    found.sort();
    found
}

/// A throwaway home on a child's environment.
fn with_home<'a>(command: &'a mut Command, home: &Path) -> &'a mut Command {
    command
        .env("USERPROFILE", home)
        .env("HOME", home)
        .env("LOCALAPPDATA", home.join("local"))
        .env("APPDATA", home.join("roaming"))
}

// The file list.

#[test]
fn the_file_list_is_the_programs_table_and_the_forwarder() {
    // operator.md, "The programs": seven files, and v0.06's forwarder.
    let mut expected = vec![
        "agentspace.exe",
        "agentspace",
        "agentspace-operator.exe",
        "agentspace-ssh-proxy.exe",
        "agentspace-keeper.exe",
        "agentspace-keeper-task.exe",
        "agentspace-keeper-install.exe",
        "agentspace-ssh-proxy.cmd",
    ];
    expected.sort();
    let mut actual = DIST_FILES.to_vec();
    actual.sort();
    assert_eq!(actual, expected);
    // Each program of the table is one of them, built from its own package and binary.
    let mut files: Vec<&str> = PROGRAMS.iter().map(|p| p.file).collect();
    files.sort();
    let mut programs: Vec<&str> = DIST_FILES
        .iter()
        .copied()
        .filter(|f| f.ends_with(".exe"))
        .collect();
    programs.sort();
    assert_eq!(files, programs);
    for program in PROGRAMS {
        assert_eq!(program.file, format!("{}.exe", program.binary));
        assert!(program.package.starts_with("agentspace-") || program.package == "agentspace-cli");
    }
}

#[test]
fn dist_writes_exactly_the_eight_files_and_nothing_extra_is_copied() {
    let scratch = Scratch::new("exactly-eight");
    let built = fake_built(&scratch, "debug");
    let dist = scratch.path().join("dist");
    fs::create_dir_all(&dist).unwrap();
    fs::write(dist.join("agentspace.exe.old-1"), "stale").unwrap();
    fs::write(dist.join("xtask.exe"), "stale").unwrap();
    // A file that is one of the eight is replaced, not kept.
    fs::write(dist.join(PROGRAM_FILE), "stale").unwrap();

    write_dist(&dist, &built).unwrap();

    let mut expected: Vec<String> = DIST_FILES.iter().map(|s| s.to_string()).collect();
    expected.sort();
    assert_eq!(names(&dist), expected);
    for program in PROGRAMS {
        assert_eq!(
            fs::read(dist.join(program.file)).unwrap(),
            fs::read(built.join(program.file)).unwrap(),
            "{}",
            program.file
        );
    }
    assert_eq!(fs::read(dist.join(WRAPPER_FILE)).unwrap(), wrapper_bytes());
    assert_eq!(
        fs::read(dist.join(FORWARDER_FILE)).unwrap(),
        forwarder_bytes()
    );
}

#[test]
fn dist_creates_the_folder_when_it_is_missing() {
    let scratch = Scratch::new("creates");
    let built = fake_built(&scratch, "debug");
    let dist = scratch.path().join("target").join("dist");
    write_dist(&dist, &built).unwrap();
    assert_eq!(names(&dist).len(), 8);
}

#[test]
fn a_missing_program_writes_nothing() {
    for missing in PROGRAMS {
        let scratch = Scratch::new("missing-program");
        let built = fake_built(&scratch, "debug");
        fs::remove_file(built.join(missing.file)).unwrap();
        let dist = scratch.path().join("dist");
        assert_eq!(
            write_dist(&dist, &built),
            Err(DistError::ProgramMissing(missing.file))
        );
        assert!(!dist.exists(), "{}", missing.file);
    }
}

#[test]
fn a_refusal_leaves_an_earlier_output_folder_as_it_was() {
    let scratch = Scratch::new("refusal-keeps");
    let built = fake_built(&scratch, "debug");
    fs::remove_file(built.join("agentspace-operator.exe")).unwrap();
    let dist = scratch.path().join("dist");
    fs::create_dir_all(&dist).unwrap();
    fs::write(dist.join("leftover.txt"), "kept").unwrap();
    assert!(write_dist(&dist, &built).is_err());
    assert_eq!(names(&dist), vec!["leftover.txt".to_string()]);
}

// Clearing the output folder reaches nothing outside it.

/// What an earlier output folder holds, by name and bytes, for a before-and-after comparison.
fn contents(folder: &Path) -> Vec<(String, Vec<u8>)> {
    let mut found: Vec<(String, Vec<u8>)> = fs::read_dir(folder)
        .unwrap()
        .map(|e| {
            let e = e.unwrap();
            let bytes = if e.file_type().unwrap().is_file() {
                fs::read(e.path()).unwrap()
            } else {
                Vec::new()
            };
            (e.file_name().into_string().unwrap(), bytes)
        })
        .collect();
    found.sort();
    found
}

#[test]
fn a_folder_in_the_output_folder_is_refused_and_nothing_is_removed() {
    let scratch = Scratch::new("folder-inside");
    let built = fake_built(&scratch, "debug");
    let dist = scratch.path().join("dist");
    fs::create_dir_all(dist.join("old-folder")).unwrap();
    fs::write(dist.join("old-folder").join("x.txt"), "kept").unwrap();
    fs::write(dist.join("stale.txt"), "kept").unwrap();
    fs::write(dist.join(PROGRAM_FILE), "kept").unwrap();
    let before = contents(&dist);
    assert_eq!(
        write_dist(&dist, &built),
        Err(DistError::OutputHoldsNonFile)
    );
    assert_eq!(contents(&dist), before);
    assert_eq!(
        fs::read(dist.join("old-folder").join("x.txt")).unwrap(),
        b"kept"
    );
}

#[test]
fn a_file_where_the_output_folder_goes_is_refused_and_kept() {
    let scratch = Scratch::new("file-as-dist");
    let built = fake_built(&scratch, "debug");
    let dist = scratch.path().join("dist");
    fs::write(&dist, "kept").unwrap();
    assert_eq!(write_dist(&dist, &built), Err(DistError::OutputNotAFolder));
    assert_eq!(fs::read(&dist).unwrap(), b"kept");
}

#[test]
fn a_hard_link_in_the_output_folder_is_never_written_through() {
    let scratch = Scratch::new("hard-link");
    let built = fake_built(&scratch, "debug");
    let outside = scratch.path().join("outside");
    fs::create_dir_all(&outside).unwrap();
    let precious = outside.join("precious.exe");
    fs::write(&precious, "precious").unwrap();
    let dist = scratch.path().join("dist");
    fs::create_dir_all(&dist).unwrap();
    // One of the eight names, and another name, both sharing the outside file's bytes.
    fs::hard_link(&precious, dist.join(PROGRAM_FILE)).unwrap();
    fs::hard_link(&precious, dist.join(FORWARDER_FILE)).unwrap();
    fs::hard_link(&precious, dist.join("stale.exe")).unwrap();

    write_dist(&dist, &built).unwrap();

    assert_eq!(fs::read(&precious).unwrap(), b"precious");
    assert_eq!(names(&dist).len(), 8);
    assert_eq!(
        fs::read(dist.join(PROGRAM_FILE)).unwrap(),
        fs::read(built.join(PROGRAM_FILE)).unwrap()
    );
}

/// Makes a junction with cmd's `mklink /J`, which needs no privilege, with a throwaway home.
#[cfg(windows)]
fn junction(link: &Path, target: &Path, home: &Path) {
    fs::create_dir_all(home).unwrap();
    let mut command = Command::new("cmd.exe");
    let out = with_home(&mut command, home)
        .current_dir(home)
        .args(["/d", "/c", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .output()
        .unwrap();
    assert!(out.status.success(), "mklink /J failed");
    assert!(
        fs::symlink_metadata(link).unwrap().file_type().is_symlink(),
        "The junction is not seen as a link"
    );
}

/// A folder outside the output folder, with a file in it that must survive.
#[cfg(windows)]
fn outside_folder(scratch: &Scratch) -> (PathBuf, PathBuf) {
    let outside = scratch.path().join("outside");
    fs::create_dir_all(&outside).unwrap();
    let precious = outside.join("precious.txt");
    fs::write(&precious, "precious").unwrap();
    (outside, precious)
}

#[cfg(windows)]
#[test]
fn a_junction_as_the_output_folder_is_refused_and_its_target_is_untouched() {
    let scratch = Scratch::new("junction-as-dist");
    let built = fake_built(&scratch, "debug");
    let (outside, precious) = outside_folder(&scratch);
    let dist = scratch.path().join("dist");
    junction(&dist, &outside, &scratch.path().join("home"));

    assert_eq!(write_dist(&dist, &built), Err(DistError::OutputNotAFolder));
    assert_eq!(fs::read(&precious).unwrap(), b"precious");
    assert_eq!(names(&outside), vec!["precious.txt".to_string()]);
    // The junction itself is the scratch folder's to remove, not its target's contents.
    fs::remove_dir(&dist).unwrap();
}

#[cfg(windows)]
#[test]
fn a_junction_in_the_output_folder_is_refused_and_its_target_is_untouched() {
    let scratch = Scratch::new("junction-inside");
    let built = fake_built(&scratch, "debug");
    let (outside, precious) = outside_folder(&scratch);
    let dist = scratch.path().join("dist");
    fs::create_dir_all(&dist).unwrap();
    fs::write(dist.join("stale.txt"), "kept").unwrap();
    junction(&dist.join("link"), &outside, &scratch.path().join("home"));

    assert_eq!(
        write_dist(&dist, &built),
        Err(DistError::OutputHoldsNonFile)
    );
    assert_eq!(fs::read(&precious).unwrap(), b"precious");
    assert_eq!(fs::read(dist.join("stale.txt")).unwrap(), b"kept");
    fs::remove_dir(dist.join("link")).unwrap();
}

// The launcher's subsystem.

#[test]
fn the_pe_subsystem_is_read_from_the_optional_header() {
    assert_eq!(pe_subsystem(&fake_pe(2)), Some(2));
    assert_eq!(pe_subsystem(&fake_pe(3)), Some(3));
    assert_eq!(pe_subsystem(b""), None);
    assert_eq!(pe_subsystem(b"MZ not a program"), None);
    let mut bad_signature = fake_pe(2);
    bad_signature[0x80] = b'X';
    assert_eq!(pe_subsystem(&bad_signature), None);
    let mut far = fake_pe(2);
    far[0x3C..0x40].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(pe_subsystem(&far), None);
}

#[test]
fn a_launcher_that_is_a_console_program_is_refused() {
    let scratch = Scratch::new("console-launcher");
    let built = fake_built(&scratch, "debug");
    fs::write(built.join(KEEPER_TASK_FILE), fake_pe(3)).unwrap();
    let dist = scratch.path().join("dist");
    assert_eq!(
        write_dist(&dist, &built),
        Err(DistError::KeeperTaskNotWindows)
    );
    assert!(!dist.exists());
    // Bytes that are no PE file are refused the same way.
    fs::write(built.join(KEEPER_TASK_FILE), "plain").unwrap();
    assert_eq!(
        write_dist(&dist, &built),
        Err(DistError::KeeperTaskNotWindows)
    );
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
    let built = fake_built(&scratch, "debug");
    fs::copy(env!("CARGO_BIN_EXE_xtask"), built.join(PROGRAM_FILE)).unwrap();
    let dist = scratch.path().join("dist");
    write_dist(&dist, &built).unwrap();
    let home = scratch.path().join("home");
    fs::create_dir_all(&home).unwrap();

    let wrapper = dist.join(WRAPPER_FILE).to_string_lossy().replace('\\', "/");
    let mut command = Command::new(&sh);
    with_home(&mut command, &home)
        .arg(&wrapper)
        .arg("/x")
        .env_remove("MSYS_NO_PATHCONV")
        .env_remove("MSYS2_ARG_CONV_EXCL");
    let out = command.output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("'/x'"), "The argument changed: {stderr}");

    let mut command = Command::new(&sh);
    with_home(&mut command, &home).arg(&wrapper).arg("--help");
    assert_eq!(command.output().unwrap().status.code(), Some(0));
}

// The forwarder.

#[test]
fn forwarder_is_exactly_the_one_fixed_line_ended_in_crlf() {
    // FB-16: these are the bytes, in hex, and no others.
    let expected: Vec<u8> = vec![
        0x40, 0x22, 0x25, 0x7e, 0x64, 0x70, 0x30, 0x61, 0x67, 0x65, 0x6e, 0x74, 0x73, 0x70, 0x61,
        0x63, 0x65, 0x2d, 0x73, 0x73, 0x68, 0x2d, 0x70, 0x72, 0x6f, 0x78, 0x79, 0x2e, 0x65, 0x78,
        0x65, 0x22, 0x20, 0x25, 0x2a, 0x0d, 0x0a,
    ];
    assert_eq!(forwarder_bytes(), expected);
    assert_eq!(
        forwarder_bytes(),
        b"@\"%~dp0agentspace-ssh-proxy.exe\" %*\r\n".to_vec()
    );
}

#[test]
fn forwarder_holds_no_path_and_does_not_depend_on_the_checkout() {
    let text = String::from_utf8(forwarder_bytes()).unwrap();
    assert!(text.is_ascii());
    assert_eq!(text.matches('\n').count(), 1);
    // Its only path is its own folder, found when it runs.
    assert!(!text.contains(':'), "A drive letter is in the forwarder");
    assert!(!text.to_ascii_lowercase().contains("scripts"));
    // `call` would return to the forwarder, which would then read its next line.
    assert!(!text.to_ascii_lowercase().contains("call"));
    // The same bytes every time: a build in another folder writes the same file.
    let scratch = Scratch::new("forwarder-same");
    let one = scratch.path().join("one");
    let two = scratch.path().join("two");
    write_dist(&one, &fake_built(&scratch, "debug")).unwrap();
    write_dist(&two, &fake_built(&scratch, "release")).unwrap();
    assert_eq!(
        fs::read(one.join(FORWARDER_FILE)).unwrap(),
        fs::read(two.join(FORWARDER_FILE)).unwrap()
    );
}

/// The written forwarder, run the way a ProxyCommand runs it, against a stand-in for the program
/// beside it.
#[cfg(windows)]
#[test]
fn forwarder_carries_arguments_and_the_exit_code_to_the_program_beside_it() {
    use std::os::windows::process::CommandExt;

    let scratch = Scratch::new("forwarder-run");
    let built = fake_built(&scratch, "debug");
    // The xtask program stands in for the ProxyCommand: an unknown command exits 2 and quotes it.
    // The folder has spaces, parentheses and an ampersand, which cmd reads specially.
    fs::copy(
        env!("CARGO_BIN_EXE_xtask"),
        built.join("agentspace-ssh-proxy.exe"),
    )
    .unwrap();
    let dist = scratch.path().join("my dist (x86) & 100");
    write_dist(&dist, &built).unwrap();
    let home = scratch.path().join("home");
    fs::create_dir_all(&home).unwrap();

    let mut command = Command::new("cmd.exe");
    with_home(&mut command, &home)
        .args(["/d", "/c"])
        // A line that starts with a quote: cmd removes its outer pair of quotes.
        .raw_arg(format!(
            "\"\"{}\" bogus-alias \"two words\"\"",
            dist.join(FORWARDER_FILE).display()
        ));
    let out = command.output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "{stderr}");
    assert!(
        stderr.contains("Unknown command 'bogus-alias'."),
        "The argument did not arrive: {stderr}"
    );
}

// The lease CLI finds the ProxyCommand first as a program.

#[test]
fn the_lease_cli_in_the_output_folder_finds_the_exe_before_the_forwarder() {
    let scratch = Scratch::new("finds-exe");
    let built = fake_built(&scratch, "debug");
    let dist = scratch.path().join("dist");
    write_dist(&dist, &built).unwrap();
    let own_exe = dist.join(PROGRAM_FILE);
    assert_eq!(
        agentspace_alias::find_proxy_program(&own_exe),
        Some(dist.join("agentspace-ssh-proxy.exe"))
    );
    // Without the exe, the forwarder is what it falls back to.
    fs::remove_file(dist.join("agentspace-ssh-proxy.exe")).unwrap();
    assert_eq!(
        agentspace_alias::find_proxy_program(&own_exe),
        Some(dist.join(FORWARDER_FILE))
    );
}

// The command line and its messages.

#[test]
fn the_command_line_reads_its_flags() {
    let args = |a: &[&str]| parse_args(a.iter().map(|s| s.to_string()));
    assert_eq!(args(&["dist"]), Ok(Request::Dist { release: false }));
    assert_eq!(
        args(&["dist", "--release"]),
        Ok(Request::Dist { release: true })
    );
    assert_eq!(args(&["--help"]), Ok(Request::Help));
    assert_eq!(args(&["dist", "-h"]), Ok(Request::Help));
    assert!(args(&[]).is_err());
    assert!(args(&["build"]).is_err());
    assert!(args(&["dist", "--bogus"]).is_err());
    // v0.05's checkout flag is gone: no path is written into any file.
    assert!(args(&["dist", "--proxy-checkout", "a"]).is_err());
}

#[test]
fn the_program_names_itself_xtask_in_every_error() {
    let scratch = Scratch::new("cli-error");
    let mut command = Command::new(env!("CARGO_BIN_EXE_xtask"));
    let out = with_home(&mut command, scratch.path())
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
fn the_usage_names_the_eight_files() {
    assert!(xtask::USAGE.contains("eight files"));
    assert!(!xtask::USAGE.contains("proxy-checkout"));
}

#[test]
fn the_profile_is_debug_unless_release_is_asked_for() {
    assert_eq!(choose_profile(false), "debug");
    assert_eq!(choose_profile(true), "release");
}
