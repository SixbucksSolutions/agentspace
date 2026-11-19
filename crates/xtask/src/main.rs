//! `cargo xtask dist`: the build output folder (docs/operator.md, "The install folder").

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use xtask::{Request, parse_args, write_dist};

/// The programs of the workspace that ship: the package and the binary in it.
const PROGRAMS: [(&str, &str); 1] = [("agentspace-cli", "agentspace")];

fn main() -> ExitCode {
    match parse_args(std::env::args().skip(1)) {
        Ok(Request::Help) => {
            print!("{}", xtask::USAGE);
            ExitCode::SUCCESS
        }
        Ok(Request::Dist {
            release,
            proxy_checkout,
        }) => dist(release, proxy_checkout),
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

fn dist(release: bool, proxy_checkout: Option<PathBuf>) -> ExitCode {
    let Some(root) = workspace_root() else {
        eprintln!("xtask: The workspace's folder was not found.");
        return ExitCode::FAILURE;
    };
    let checkout = match xtask::choose_checkout(proxy_checkout.as_deref(), &root) {
        Ok(checkout) => checkout,
        Err(error) => {
            eprintln!("xtask: {error}");
            return ExitCode::FAILURE;
        }
    };
    // Refuse before the build, so that a missing target costs nothing and touches nothing.
    if let Err(error) = xtask::proxy_target(&checkout) {
        eprintln!("xtask: {error}");
        return ExitCode::FAILURE;
    }

    let target_dir = root.join("target");
    let profile = xtask::choose_profile(release);
    for (package, binary) in PROGRAMS {
        let mut build = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
        build
            .current_dir(&root)
            .arg("build")
            .args(["--package", package, "--bin", binary])
            .arg("--target-dir")
            .arg(&target_dir);
        if release {
            build.arg("--release");
        }
        match build.status() {
            Ok(status) if status.success() => {}
            _ => {
                eprintln!("xtask: The build of {package} failed.");
                return ExitCode::FAILURE;
            }
        }
    }

    let built = target_dir
        .join(profile)
        .join(format!("agentspace{}", std::env::consts::EXE_SUFFIX));
    let dist = target_dir.join("dist");
    match write_dist(&dist, &built, &checkout) {
        Ok(()) => {
            println!(
                "Wrote target\\dist\\ with agentspace.exe, agentspace and agentspace-ssh-proxy.cmd."
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("xtask: {error}");
            ExitCode::FAILURE
        }
    }
}
