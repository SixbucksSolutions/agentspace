//! `cargo xtask dist`: the build output folder (docs/operator.md, "The install folder").

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use xtask::{PROGRAMS, Request, parse_args, write_dist};

fn main() -> ExitCode {
    match parse_args(std::env::args().skip(1)) {
        Ok(Request::Help) => {
            print!("{}", xtask::USAGE);
            ExitCode::SUCCESS
        }
        Ok(Request::Dist { release }) => dist(release),
        Err(message) => {
            eprintln!("xtask: {message}");
            eprint!("{}", xtask::USAGE);
            ExitCode::from(2)
        }
    }
}

/// The workspace's root: two folders above this crate's own.
fn workspace_root() -> Option<PathBuf> {
    let manifest = std::env::var_os("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    manifest.parent()?.parent().map(Path::to_path_buf)
}

fn dist(release: bool) -> ExitCode {
    let Some(root) = workspace_root() else {
        eprintln!("xtask: The workspace's folder was not found.");
        return ExitCode::FAILURE;
    };
    let target_dir = root.join("target");
    let profile = xtask::choose_profile(release);
    for program in PROGRAMS {
        let mut build = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
        build
            .current_dir(&root)
            .arg("build")
            .args(["--package", program.package, "--bin", program.binary])
            .arg("--target-dir")
            .arg(&target_dir);
        if release {
            build.arg("--release");
        }
        match build.status() {
            Ok(status) if status.success() => {}
            _ => {
                eprintln!("xtask: The build of {} failed.", program.package);
                return ExitCode::FAILURE;
            }
        }
    }

    let built = target_dir.join(profile);
    let dist = target_dir.join("dist");
    match write_dist(&dist, &built) {
        Ok(()) => {
            println!("Wrote target\\dist\\ with the six programs, agentspace and the forwarder.");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("xtask: {error}");
            ExitCode::FAILURE
        }
    }
}
