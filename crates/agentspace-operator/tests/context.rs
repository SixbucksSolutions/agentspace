//! The pinned `Context`, built once from an injected lookup (docs/operator.md, Environment
//! variables; the pin on card #0111). No child-environment rule is tested here: it is
//! agentspace-run's.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.

mod common;

use common::*;
use std::path::PathBuf;

const PROFILE: &str = "/throwaway/profile";
const SYSTEM_ROOT: &str = "/throwaway/system";
const LOCAL: &str = "/throwaway/local";

fn base() -> Vec<(&'static str, &'static str)> {
    vec![
        ("USERPROFILE", PROFILE),
        ("SystemRoot", SYSTEM_ROOT),
        ("LOCALAPPDATA", LOCAL),
    ]
}

fn with(extra: &[(&'static str, &'static str)]) -> Vec<(&'static str, &'static str)> {
    let mut v = base();
    v.extend_from_slice(extra);
    v
}

#[test]
fn every_field_comes_from_the_lookup() {
    let c = context(&with(&[("AGENTSPACE_NESTED", "1")])).expect("a context");
    assert_eq!(c.home, PathBuf::from(PROFILE));
    assert_eq!(c.local_app_data, Some(PathBuf::from(LOCAL)));
    assert_eq!(c.system_root, PathBuf::from(SYSTEM_ROOT));
    assert!(c.nested);
}

#[test]
fn local_app_data_is_none_when_unset_or_empty() {
    let unset = context(&[("USERPROFILE", PROFILE), ("SystemRoot", SYSTEM_ROOT)]).unwrap();
    assert_eq!(unset.local_app_data, None);
    let empty = context(&[
        ("USERPROFILE", PROFILE),
        ("SystemRoot", SYSTEM_ROOT),
        ("LOCALAPPDATA", ""),
    ])
    .unwrap();
    assert_eq!(empty.local_app_data, None);
}

#[test]
fn a_missing_local_app_data_fails_no_command() {
    assert!(context(&[("USERPROFILE", PROFILE), ("SystemRoot", SYSTEM_ROOT)]).is_ok());
}

#[test]
fn nested_is_true_when_set_and_not_empty() {
    for value in ["1", "0", "yes", " ", "anything"] {
        let c = context(&with(&[("AGENTSPACE_NESTED", value)])).unwrap();
        assert!(c.nested, "{value:?}");
    }
}

#[test]
fn nested_is_false_when_unset_or_empty() {
    assert!(!context(&base()).unwrap().nested);
    assert!(!context(&with(&[("AGENTSPACE_NESTED", "")])).unwrap().nested);
}

#[test]
fn a_missing_home_fails_with_the_message_and_the_could_not_reach_line() {
    for pairs in [
        vec![("SystemRoot", SYSTEM_ROOT)],
        vec![("USERPROFILE", ""), ("SystemRoot", SYSTEM_ROOT)],
    ] {
        let f = context(&pairs).expect_err("no home");
        assert_eq!(
            f.message,
            "agentspace: USERPROFILE is not set, so there is no home folder."
        );
        assert_eq!(f.caller, CallerFailure::CouldNotReach);
    }
}

#[test]
fn a_missing_system_root_fails_with_the_message_and_the_could_not_reach_line() {
    for pairs in [
        vec![("USERPROFILE", PROFILE)],
        vec![("USERPROFILE", PROFILE), ("SystemRoot", "")],
    ] {
        let f = context(&pairs).expect_err("no system folder");
        assert_eq!(
            f.message,
            "agentspace: SystemRoot is not set, so Windows' programs cannot be found."
        );
        assert_eq!(f.caller, CallerFailure::CouldNotReach);
    }
}

#[test]
fn the_home_is_checked_before_the_system_folder() {
    for pairs in [
        vec![],
        vec![("USERPROFILE", ""), ("SystemRoot", "")],
        vec![("SystemRoot", "")],
    ] {
        let f = context(&pairs).expect_err("nothing set");
        assert_eq!(
            f.message,
            "agentspace: USERPROFILE is not set, so there is no home folder."
        );
    }
}

#[test]
fn a_caller_sees_the_could_not_reach_line_for_either() {
    for pairs in [vec![], vec![("USERPROFILE", PROFILE)]] {
        let f = context(&pairs).expect_err("a failure");
        let (say, cap) = caller(false, false);
        failure(&say, &f.message, f.caller);
        assert_eq!(
            cap.text(),
            "agentspace: Could not reach the host. Set AGENTSPACE_DIAGNOSTIC=1 and connect again to see why.\n"
        );
    }
}

#[test]
fn the_diagnostic_flag_is_set_when_set_and_not_empty() {
    assert!(!diagnostic(&[]));
    assert!(!diagnostic(&[("AGENTSPACE_DIAGNOSTIC", "")]));
    assert!(diagnostic(&[("AGENTSPACE_DIAGNOSTIC", "1")]));
    assert!(diagnostic(&[("AGENTSPACE_DIAGNOSTIC", "0")]));
}
