//! The OneDrive refusal of `ca`'s step 2, shared with `sync`'s CA check (docs/operator.md, ca,
//! step 2; Environment variables; docs/state.md, The CA, Its files).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. The library reads no environment variable
//! (operator.md, ca, Rulings for the rewrite's ca: architect, 2026-11-19, for E0.06-T02-S01): the
//! values of `OneDrive`, `OneDriveConsumer` and `OneDriveCommercial` reach it in the `CaEnv`, so
//! each case builds one directly and calls the library in process. No environment is read or
//! changed, and no child process is started.
#![cfg(windows)]

mod spec_common;

use std::ffi::OsString;

use agentspace_ca::CaEnv;
use spec_common::*;

/// The three OneDrive values, by the names of the variables they come from.
const VARS: [&str; 3] = ["OneDrive", "OneDriveConsumer", "OneDriveCommercial"];

/// The tests' [`ca_env`], with `var`'s value set to `value`.
fn with(env: CaEnv, var: &str, value: &str) -> CaEnv {
    let value = Some(OsString::from(value));
    match var {
        "OneDrive" => CaEnv {
            one_drive: value,
            ..env
        },
        "OneDriveConsumer" => CaEnv {
            one_drive_consumer: value,
            ..env
        },
        "OneDriveCommercial" => CaEnv {
            one_drive_commercial: value,
            ..env
        },
        _ => panic!("Not a OneDrive variable"),
    }
}

#[test]
fn a_ca_folder_inside_a_onedrive_folder_is_refused_for_each_of_the_three_variables() {
    for entry in BOTH {
        for var in VARS {
            let lab = Lab::new();
            let folder = lab.home.display().to_string();
            let o = run_with(entry, &lab.ctx(), &with(ca_env(), var, &folder));
            same_for(
                o.message(),
                m_synced_or_repo(&lab.ca_dir(), &folder),
                format!("{entry:?} with {var}"),
            );
            assert!(o.lines.is_empty(), "{entry:?} with {var} said something");
            assert!(
                !lab.ca_dir().exists(),
                "{entry:?} with {var} made the CA's folder"
            );
            assert!(!lab.key().exists() && !lab.pubkey().exists());
        }
    }
}

#[test]
fn a_onedrive_folder_above_the_home_refuses_it_too() {
    for entry in BOTH {
        let lab = Lab::new();
        let folder = lab.root().display().to_string();
        let o = run_with(entry, &lab.ctx(), &with(ca_env(), "OneDrive", &folder));
        same_for(
            o.message(),
            m_synced_or_repo(&lab.ca_dir(), &folder),
            format!("{entry:?}"),
        );
        assert!(!lab.ca_dir().exists());
    }
}

#[test]
fn the_comparison_is_made_without_regard_to_case() {
    for entry in BOTH {
        let lab = Lab::new();
        let shouted = lab.root().display().to_string().to_uppercase();
        let o = run_with(
            entry,
            &lab.ctx(),
            &with(ca_env(), "OneDriveCommercial", &shouted),
        );
        // `<folder>` is the value as it was set.
        same_for(
            o.message(),
            m_synced_or_repo(&lab.ca_dir(), &shouted),
            format!("{entry:?}"),
        );
        assert!(!lab.ca_dir().exists());
    }
}

#[test]
fn a_folder_that_only_shares_a_prefix_is_not_a_parent() {
    if skip_when_elevated() {
        return;
    }
    let lab = Lab::new();
    // `home2` starts with `home`, and is not it.
    let sibling = format!("{}2", lab.home.display());
    let o = run_with(Entry::Ca, &lab.ctx(), &with(ca_env(), "OneDrive", &sibling));
    o.ok();
    assert!(lab.key().is_file());
}

#[test]
fn a_onedrive_variable_that_is_empty_is_ignored() {
    if skip_when_elevated() {
        return;
    }
    let lab = Lab::new();
    let mut env = ca_env();
    for var in VARS {
        env = with(env, var, "");
    }
    let o = run_with(Entry::Ca, &lab.ctx(), &env);
    o.ok();
    assert!(lab.key().is_file());
}

#[test]
fn a_onedrive_folder_elsewhere_does_not_refuse_and_sync_gives_the_fingerprint() {
    if skip_when_elevated() {
        return;
    }
    let lab = Lab::new();
    let elsewhere = lab.root().join("OneDrive").display().to_string();
    let env = with(ca_env(), "OneDrive", &elsewhere);
    run_with(Entry::Ca, &lab.ctx(), &env).ok();
    let o = run_with(Entry::Sync, &lab.ctx(), &env);
    o.ok();
    assert_eq!(o.fingerprint, Some(fingerprint_of(&lab.pubkey())));
}
