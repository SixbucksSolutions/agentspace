//! The EC2 driver's four sizes, the map from a size to an instance type, `meets`, `resize_target`
//! and the `unsupported_size` reason (docs/driver.md, "Sizes and instance types", "What it
//! supports" and "The driver's name"; docs/lease-api.md, "Size"; docs/decisions.md, D65 and D71;
//! E0.09-T02-S02, #0201).
//!
//! RFC 2119 keywords in these comments, and the capitals are load-bearing. The map's tests touch no
//! file. The reasons' tests place through the `Driver` trait over a throwaway home and a fake
//! detached start, so no test starts a process.

use agentspace_driver_api::Driver;
use agentspace_driver_ec2::{Ec2Driver, SIZES, instance_type_for, meets, resize_target};
use agentspace_state::Record;

/// The driver's name, which the CLI stores with every lease it places (driver.md, The driver's name).
const DRIVER: &str = "ec2";

/// The four types, smallest first, as driver.md's table gives them.
const TYPES: [&str; 4] = ["m9g.large", "m9g.xlarge", "m9g.2xlarge", "m9g.4xlarge"];

fn record(state: &str, project: &str, driver: Option<&str>, size: &str) -> Record {
    Record {
        sized_for: None,
        plan: None,
        plan_source: None,
        id: "3f2a9c1e-5b7d-4a60-8c11-0d2e4f6a8b90".to_string(),
        project: project.to_string(),
        state: state.to_string(),
        platform: "linux/arm64".to_string(),
        profile: "agent-linux".to_string(),
        size: size.to_string(),
        ttl_seconds: 3600,
        created_at: "2026-11-23T10:00:00Z".to_string(),
        expires_at: None,
        released_at: None,
        ssh: None,
        reasons: None,
        driver: driver.map(str::to_string),
    }
}

fn pending(project: &str, size: &str) -> Record {
    record("pending", project, Some(DRIVER), size)
}

// ---- The driver's name, which resize_target's selection uses -------------------------------------

#[test]
fn the_name_these_tests_select_by_is_the_drivers_own() {
    // driver.md, The driver's name: `ec2`.
    assert_eq!(Ec2Driver::new().name(), DRIVER);
}

// ---- SIZES ---------------------------------------------------------------------------------------

#[test]
fn sizes_is_s_m_l_and_xl_in_that_order() {
    // driver.md, Sizes and instance types: `SIZES` is `["s", "m", "l", "xl"]`, smallest first.
    assert_eq!(SIZES, ["s", "m", "l", "xl"]);
    assert_eq!(SIZES.len(), 4);
}

// ---- instance_type_for ---------------------------------------------------------------------------

#[test]
fn each_size_has_its_type() {
    // driver.md's table, and D65.
    assert_eq!(instance_type_for("s"), Some("m9g.large"));
    assert_eq!(instance_type_for("m"), Some("m9g.xlarge"));
    assert_eq!(instance_type_for("l"), Some("m9g.2xlarge"));
    assert_eq!(instance_type_for("xl"), Some("m9g.4xlarge"));
}

#[test]
fn m_stays_the_type_both_hosts_had_before_v0_09() {
    // driver.md: "`m` stays `m9g.xlarge`".
    assert_eq!(instance_type_for("m"), Some("m9g.xlarge"));
}

#[test]
fn every_size_in_sizes_has_a_type_and_the_types_ascend_in_the_tables_order() {
    let types: Vec<Option<&str>> = SIZES.iter().map(|s| instance_type_for(s)).collect();
    let want: Vec<Option<&str>> = TYPES.iter().map(|t| Some(*t)).collect();
    assert_eq!(types, want);
}

#[test]
fn a_size_outside_sizes_has_no_type() {
    // driver.md, pin P17: the size is compared exactly, with its case, and `XL` and ` m` are `None`.
    for text in [
        "xxl",
        "XL",
        " m",
        "m ",
        "M",
        "S",
        "L",
        "XXL",
        "",
        " ",
        "xl\n",
        "xs",
        "2xl",
        "m9g.xlarge",
    ] {
        assert_eq!(instance_type_for(text), None, "{text:?}");
    }
}

// ---- meets ---------------------------------------------------------------------------------------

#[test]
fn meets_for_every_type_and_size_is_the_truth_table() {
    // driver.md: true when the type is one the map holds, the size is in SIZES, and the type's size
    // is at or after that size's place. Each row is one type, over the sizes s, m, l, xl.
    let table: [(&str, [bool; 4]); 4] = [
        ("m9g.large", [true, false, false, false]),
        ("m9g.xlarge", [true, true, false, false]),
        ("m9g.2xlarge", [true, true, true, false]),
        ("m9g.4xlarge", [true, true, true, true]),
    ];
    let mut cases = 0;
    for (ty, row) in table {
        for (size, want) in ["s", "m", "l", "xl"].into_iter().zip(row) {
            assert_eq!(meets(ty, size), want, "meets({ty:?}, {size:?})");
            cases += 1;
        }
    }
    assert_eq!(cases, 16);
}

#[test]
fn the_specs_own_examples_of_meets() {
    // driver.md: `meets("m9g.2xlarge", "m")` is true, `meets("m9g.xlarge", "l")` is false and
    // `meets("m9g.xlarge", "m")` is true.
    assert!(meets("m9g.2xlarge", "m"));
    assert!(!meets("m9g.xlarge", "l"));
    assert!(meets("m9g.xlarge", "m"));
}

#[test]
fn a_type_the_map_does_not_hold_meets_no_size() {
    // driver.md: "A type the map does not hold meets no size", `m9g.8xlarge` and the empty text
    // among them; the type is compared exactly, with its case.
    for ty in [
        "m9g.8xlarge",
        "m9g.16xlarge",
        "",
        " ",
        "M9G.XLARGE",
        "M9g.xlarge",
        "m9g.xlarge ",
        " m9g.xlarge",
        "m9g",
        "xlarge",
        "m",
        "m7g.large",
    ] {
        for size in SIZES {
            assert!(!meets(ty, size), "meets({ty:?}, {size:?})");
        }
    }
}

#[test]
fn a_size_outside_sizes_is_met_by_no_type() {
    // driver.md: "a size not in `SIZES` is met by no type", the largest type included.
    for size in ["xxl", "XL", " m", "M", "", "2xl", "m9g.xlarge"] {
        for ty in TYPES {
            assert!(!meets(ty, size), "meets({ty:?}, {size:?})");
        }
    }
}

#[test]
fn neither_unknown_gives_a_meet() {
    assert!(!meets("", ""));
    assert!(!meets("m9g.8xlarge", "xxl"));
}

// ---- resize_target -------------------------------------------------------------------------------

#[test]
fn no_record_gives_no_target() {
    assert_eq!(resize_target(&[], "swimcoach"), None);
}

#[test]
fn no_pending_record_gives_no_target() {
    // Each state but `pending` is ignored.
    let leases: Vec<Record> = [
        "ready",
        "released",
        "expired",
        "unsatisfiable",
        "failed",
        "",
    ]
    .iter()
    .map(|state| record(state, "swimcoach", Some(DRIVER), "xl"))
    .collect();
    assert_eq!(resize_target(&leases, "swimcoach"), None);
}

#[test]
fn one_pending_record_gives_its_sizes_type() {
    for (size, ty) in SIZES.into_iter().zip(TYPES) {
        assert_eq!(
            resize_target(&[pending("swimcoach", size)], "swimcoach"),
            Some(ty),
            "{size}"
        );
    }
}

#[test]
fn several_pending_records_give_the_largest_sizes_type() {
    // driver.md: with `pending` records of the project at `s` and `l`, `m9g.2xlarge`.
    let leases = [pending("swimcoach", "s"), pending("swimcoach", "l")];
    assert_eq!(resize_target(&leases, "swimcoach"), Some("m9g.2xlarge"));
}

#[test]
fn the_largest_wins_whatever_the_order() {
    let sizes = ["m", "xl", "s", "l"];
    let forward: Vec<Record> = sizes.iter().map(|s| pending("swimcoach", s)).collect();
    let mut backward = forward.clone();
    backward.reverse();
    assert_eq!(resize_target(&forward, "swimcoach"), Some("m9g.4xlarge"));
    assert_eq!(resize_target(&backward, "swimcoach"), Some("m9g.4xlarge"));
    let small: Vec<Record> = ["m", "s", "m"]
        .iter()
        .map(|s| pending("swimcoach", s))
        .collect();
    assert_eq!(resize_target(&small, "swimcoach"), Some("m9g.xlarge"));
}

#[test]
fn an_unsupported_size_among_them_is_ignored() {
    // `xxl` is larger than `xl` in no place of SIZES: it is not in SIZES, and never sets a type.
    let leases = [
        pending("swimcoach", "s"),
        pending("swimcoach", "xxl"),
        pending("swimcoach", "XL"),
        pending("swimcoach", ""),
        pending("swimcoach", " l"),
    ];
    assert_eq!(resize_target(&leases, "swimcoach"), Some("m9g.large"));
    // Alone, it gives nothing.
    assert_eq!(
        resize_target(&[pending("swimcoach", "xxl")], "swimcoach"),
        None
    );
}

#[test]
fn another_projects_pending_record_is_ignored_even_when_larger() {
    let leases = [pending("swimcoach", "s"), pending("other", "xl")];
    assert_eq!(resize_target(&leases, "swimcoach"), Some("m9g.large"));
    assert_eq!(resize_target(&leases, "other"), Some("m9g.4xlarge"));
    assert_eq!(resize_target(&leases, "third"), None);
}

#[test]
fn the_project_is_compared_exactly() {
    // driver.md: "compared exactly as the normalized strings both are".
    let leases = [pending("swimcoach", "xl")];
    for project in [
        "SwimCoach",
        "swimcoach ",
        " swimcoach",
        "swim",
        "swimcoach2",
        "",
    ] {
        assert_eq!(resize_target(&leases, project), None, "{project:?}");
    }
    assert_eq!(resize_target(&leases, "swimcoach"), Some("m9g.4xlarge"));
}

#[test]
fn another_drivers_record_and_one_with_no_driver_are_ignored() {
    // driver.md: a record that only a hand edit makes, with another driver or no `driver`, never
    // sets a host's type. `driver` is an `Option` in `agentspace_state::Record`, so it can be absent.
    let leases = [
        pending("swimcoach", "s"),
        record("pending", "swimcoach", Some("other"), "xl"),
        record("pending", "swimcoach", Some("EC2"), "xl"),
        record("pending", "swimcoach", Some(""), "xl"),
        record("pending", "swimcoach", None, "xl"),
    ];
    assert_eq!(resize_target(&leases, "swimcoach"), Some("m9g.large"));
    // Alone, each gives nothing.
    for driver in [Some("other"), Some("EC2"), Some(""), None] {
        assert_eq!(
            resize_target(&[record("pending", "swimcoach", driver, "xl")], "swimcoach"),
            None,
            "{driver:?}"
        );
    }
}

#[test]
fn a_ready_record_of_a_larger_size_is_ignored() {
    let leases = [
        pending("swimcoach", "m"),
        record("ready", "swimcoach", Some(DRIVER), "xl"),
        record("released", "swimcoach", Some(DRIVER), "xl"),
    ];
    assert_eq!(resize_target(&leases, "swimcoach"), Some("m9g.xlarge"));
}

#[test]
fn a_pending_records_other_members_do_not_matter() {
    // driver.md: it "looks at no other member of a record, `created_at` among them".
    let mut odd = pending("swimcoach", "l");
    odd.created_at = "1999-01-01T00:00:00Z".to_string();
    odd.ttl_seconds = 60;
    odd.id = "not an id".to_string();
    odd.profile = "other".to_string();
    odd.expires_at = Some("2000-01-01T00:00:00Z".to_string());
    odd.released_at = Some("2000-01-02T00:00:00Z".to_string());
    assert_eq!(resize_target(&[odd], "swimcoach"), Some("m9g.2xlarge"));
}

#[test]
fn the_result_is_the_same_as_the_maps_for_the_largest_size() {
    // The type resize_target gives is the one instance_type_for gives for the same size.
    for size in SIZES {
        assert_eq!(
            resize_target(&[pending("p", size)], "p"),
            instance_type_for(size),
            "{size}"
        );
    }
}

// ---- The reasons, through the driver --------------------------------------------------------------
//
// `reasons` is a private function of the crate, so these place through the `Driver` trait, which
// answers an unsupported request before it reads any file (driver.md, What it supports).

#[cfg(windows)]
mod common;
#[cfg(windows)]
#[path = "common/pending.rs"]
mod pending_support;

#[cfg(windows)]
mod reasons {
    use super::*;
    use crate::common::*;
    use crate::pending_support::*;
    use agentspace_driver_api::{Placed, Reason};

    /// The message of `unsupported_size` for `size`, byte for byte (driver.md, What it supports;
    /// lease-api.md, Fields).
    fn message(size: &str) -> String {
        format!("Size '{size}' is not supported; v0 supports sizes s, m, l and xl.")
    }

    /// A home with a CA and the operator installed, and a fake start that records each start.
    fn world() -> (Fx, std::sync::Arc<FakeDetach>) {
        let mut fx = Fx::with_ca();
        fx.install_operator();
        let detach = FakeDetach::new();
        (fx, detach)
    }

    /// Places a request for `project` and returns the reasons of an unsatisfiable answer.
    fn unsatisfiable(
        fx: &Fx,
        detach: &std::sync::Arc<FakeDetach>,
        project: &str,
        platform: &str,
        profile: &str,
        size: &str,
    ) -> Vec<(String, String)> {
        let d = fx.driver_with(detach);
        let req = fx.request_for(project, ID, platform, profile, size, 600);
        match d.place(&fx.ctx, &req) {
            Ok(Placed::Unsatisfiable { reasons }) => reasons
                .iter()
                .map(|Reason { code, message }| (code.to_string(), message.clone()))
                .collect(),
            other => panic!("Expected unsatisfiable for {platform} {profile} {size}: {other:?}"),
        }
    }

    #[test]
    fn s_m_l_and_xl_are_each_supported_so_none_gives_unsupported_size() {
        // A project that is not listed has no host: the one reason is `no_capacity`, which reads no
        // file and starts nothing, and `unsupported_size` is in no answer.
        let (fx, detach) = world();
        for size in SIZES {
            let reasons =
                unsatisfiable(&fx, &detach, "unlisted", "linux/arm64", "agent-linux", size);
            let codes: Vec<&str> = reasons.iter().map(|(c, _)| c.as_str()).collect();
            assert_eq!(codes, ["no_capacity"], "{size}");
        }
        assert_eq!(detach.count(), 0);
    }

    #[test]
    fn xxl_gives_unsupported_size_with_the_specs_message_byte_for_byte() {
        // driver.md: "Size 'xxl' is not supported; v0 supports sizes s, m, l and xl."
        let (fx, detach) = world();
        let reasons = unsatisfiable(
            &fx,
            &detach,
            "swimcoach",
            "linux/arm64",
            "agent-linux",
            "xxl",
        );
        assert_eq!(
            reasons,
            [(
                "unsupported_size".to_string(),
                "Size 'xxl' is not supported; v0 supports sizes s, m, l and xl.".to_string()
            )]
        );
        assert_eq!(message("xxl").as_bytes(), reasons[0].1.as_bytes());
        assert_eq!(detach.count(), 0);
    }

    #[test]
    fn the_message_quotes_the_size_it_was_given() {
        let (fx, detach) = world();
        for size in ["xxl", "xxxl", "2xl", "huge"] {
            let reasons = unsatisfiable(
                &fx,
                &detach,
                "swimcoach",
                "linux/arm64",
                "agent-linux",
                size,
            );
            assert_eq!(
                reasons,
                [("unsupported_size".to_string(), message(size))],
                "{size}"
            );
        }
    }

    #[test]
    fn the_message_no_longer_says_size_m_only() {
        let (fx, detach) = world();
        let reasons = unsatisfiable(
            &fx,
            &detach,
            "swimcoach",
            "linux/arm64",
            "agent-linux",
            "xxl",
        );
        assert!(
            !reasons[0].1.contains("size m only"),
            "The v0.08 wording is gone: {}",
            reasons[0].1
        );
    }

    #[test]
    fn a_size_is_compared_exactly_so_a_size_that_is_not_normalized_is_unsupported() {
        // driver.md: matching is exact on the normalized values, so the driver, which is only ever
        // given a normalized size, answers `XL` and ` m` with `unsupported_size`.
        let (fx, detach) = world();
        for size in ["XL", " m", "M", "S"] {
            let reasons = unsatisfiable(
                &fx,
                &detach,
                "swimcoach",
                "linux/arm64",
                "agent-linux",
                size,
            );
            assert_eq!(
                reasons,
                [("unsupported_size".to_string(), message(size))],
                "{size:?}"
            );
        }
    }

    #[test]
    fn an_unsupported_size_is_answered_with_neither_file_read_and_nothing_started() {
        // driver.md, Z2: it reads no `hosts.json`, so one that does not validate gives the same
        // answer, and it writes the registry and starts nothing.
        let (fx, detach) = world();
        fx.write_hosts_raw(b"not json");
        fx.write_projects_raw(b"not json");
        let hosts_before = fx.hosts_bytes();
        let reasons = unsatisfiable(
            &fx,
            &detach,
            "swimcoach",
            "linux/arm64",
            "agent-linux",
            "xxl",
        );
        assert_eq!(reasons, [("unsupported_size".to_string(), message("xxl"))]);
        assert_eq!(fx.hosts_bytes(), hosts_before);
        assert_eq!(detach.count(), 0);
    }

    #[test]
    fn an_unsupported_platform_with_a_supported_size_lists_the_platform_alone() {
        // lease-api.md, Size, and driver.md, pin P9: `windows/arm64` with size `l` lists
        // `unsupported_platform` alone; a size is judged on its own.
        let (fx, detach) = world();
        for size in SIZES {
            let reasons = unsatisfiable(
                &fx,
                &detach,
                "swimcoach",
                "windows/arm64",
                "agent-linux",
                size,
            );
            let codes: Vec<&str> = reasons.iter().map(|(c, _)| c.as_str()).collect();
            assert_eq!(codes, ["unsupported_platform"], "{size}");
        }
        assert_eq!(detach.count(), 0);
    }

    #[test]
    fn an_unsupported_platform_with_xxl_lists_the_platform_then_the_size() {
        // driver.md, Z2: with `--platform windows/arm64` as well, the reasons are
        // `unsupported_platform` then `unsupported_size`, in the contract's order.
        let (fx, detach) = world();
        let reasons = unsatisfiable(
            &fx,
            &detach,
            "swimcoach",
            "windows/arm64",
            "agent-linux",
            "xxl",
        );
        let codes: Vec<&str> = reasons.iter().map(|(c, _)| c.as_str()).collect();
        assert_eq!(codes, ["unsupported_platform", "unsupported_size"]);
        assert_eq!(reasons[1].1, message("xxl"));
    }

    #[test]
    fn all_three_reasons_are_listed_in_the_contracts_order_with_xxl() {
        let (fx, detach) = world();
        let reasons = unsatisfiable(
            &fx,
            &detach,
            "swimcoach",
            "windows/arm64",
            "agent-linux@3",
            "xxl",
        );
        let codes: Vec<&str> = reasons.iter().map(|(c, _)| c.as_str()).collect();
        assert_eq!(
            codes,
            [
                "unsupported_platform",
                "unsupported_profile",
                "unsupported_size"
            ]
        );
        assert_eq!(reasons[2].1, message("xxl"));
    }

    #[test]
    fn an_unsupported_profile_with_a_supported_size_lists_the_profile_alone() {
        let (fx, detach) = world();
        for size in SIZES {
            let reasons = unsatisfiable(
                &fx,
                &detach,
                "swimcoach",
                "linux/arm64",
                "agent-linux@1",
                size,
            );
            let codes: Vec<&str> = reasons.iter().map(|(c, _)| c.as_str()).collect();
            assert_eq!(codes, ["unsupported_profile"], "{size}");
        }
    }
}
