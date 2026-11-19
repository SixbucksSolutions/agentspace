//! `find_proxy_program` on Windows (docs/state.md, "The alias file"; docs/operator.md, "How each
//! program finds the others"). Every folder is a throwaway under the temp folder.
#![cfg(windows)]

mod common;

use agentspace_alias::*;
use common::*;
use std::fs;

#[test]
fn exe_is_preferred_over_cmd() {
    let s = scratch("p1");
    let own = s.0.join("agentspace.exe");
    fs::write(&own, b"").unwrap();
    fs::write(s.0.join("agentspace-ssh-proxy.exe"), b"").unwrap();
    fs::write(s.0.join("agentspace-ssh-proxy.cmd"), b"").unwrap();
    assert_eq!(
        find_proxy_program(&own),
        Some(s.0.join("agentspace-ssh-proxy.exe"))
    );
}

#[test]
fn cmd_is_found_when_there_is_no_exe() {
    let s = scratch("p2");
    let own = s.0.join("agentspace.exe");
    fs::write(&own, b"").unwrap();
    fs::write(s.0.join("agentspace-ssh-proxy.cmd"), b"").unwrap();
    assert_eq!(
        find_proxy_program(&own),
        Some(s.0.join("agentspace-ssh-proxy.cmd"))
    );
}

#[test]
fn none_when_neither_is_there() {
    let s = scratch("p3");
    let own = s.0.join("agentspace.exe");
    fs::write(&own, b"").unwrap();
    // Other files, and a folder with the program's name, are not the program.
    fs::write(s.0.join("agentspace-ssh-proxy.txt"), b"").unwrap();
    fs::write(s.0.join("agentspace-operator.exe"), b"").unwrap();
    assert_eq!(find_proxy_program(&own), None);
}

#[test]
fn only_the_folder_of_the_running_executable_is_searched() {
    let s = scratch("p4");
    let bin = s.0.join("bin");
    let elsewhere = s.0.join("elsewhere");
    fs::create_dir_all(&bin).unwrap();
    fs::create_dir_all(&elsewhere).unwrap();
    let own = bin.join("agentspace.exe");
    fs::write(&own, b"").unwrap();
    fs::write(elsewhere.join("agentspace-ssh-proxy.exe"), b"").unwrap();
    assert_eq!(find_proxy_program(&own), None);
}

#[test]
fn the_result_is_a_full_path_for_a_full_path_input() {
    let s = scratch("p5");
    let own = s.0.join("agentspace.exe");
    fs::write(&own, b"").unwrap();
    fs::write(s.0.join("agentspace-ssh-proxy.cmd"), b"").unwrap();
    let found = find_proxy_program(&own).unwrap();
    assert!(found.is_absolute());
    assert!(found.is_file());
}
