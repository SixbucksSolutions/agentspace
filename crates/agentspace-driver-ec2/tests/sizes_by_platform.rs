//! A column per platform: `instance_type_on`, `meets_on` and `resize_target` by host
//! (docs/driver.md, Sizes and instance types; docs/decisions.md, D91 and D94; architect,
//! 2026-11-26, for v0.12-T01, pins P28 and P33; E0.12-T03-S01, #0254).
//!
//! RFC 2119 keywords in these comments, and the capitals are load-bearing. These tests touch no
//! file. The page names `resize_target_on`; the code's name for it is `resize_target_keyed`, which
//! takes the host's key, its project and its platform.

use agentspace_driver_ec2::{
    SIZES, instance_type_for, instance_type_on, meets, meets_on, resize_target, resize_target_keyed,
};
use agentspace_hosts::HostKey;
use agentspace_state::Record;

const M9G: [&str; 4] = ["m9g.large", "m9g.xlarge", "m9g.2xlarge", "m9g.4xlarge"];
const M8A: [&str; 4] = ["m8a.large", "m8a.xlarge", "m8a.2xlarge", "m8a.4xlarge"];

/// Each platform the table has a column for, with its column.
const COLUMNS: [(&str, [&str; 4]); 3] = [
    ("linux/arm64", M9G),
    ("linux/amd64", M8A),
    ("windows/amd64", M8A),
];

fn record(state: &str, project: &str, platform: &str, driver: Option<&str>, size: &str) -> Record {
    Record {
        sized_for: None,
        plan: None,
        plan_source: None,
        id: "3f2a9c1e-5b7d-4a60-8c11-0d2e4f6a8b90".to_string(),
        project: project.to_string(),
        state: state.to_string(),
        platform: platform.to_string(),
        profile: "agent-linux".to_string(),
        size: size.to_string(),
        ttl_seconds: 3600,
        created_at: "2026-11-26T10:00:00Z".to_string(),
        expires_at: None,
        released_at: None,
        ssh: None,
        reasons: None,
        driver: driver.map(str::to_string),
    }
}

fn pending_on(project: &str, platform: &str, size: &str) -> Record {
    record("pending", project, platform, Some("ec2"), size)
}

// ---- instance_type_on -----------------------------------------------------------------------------

#[test]
fn instance_type_on_gives_each_platforms_column() {
    for (platform, types) in COLUMNS {
        for (size, want) in SIZES.into_iter().zip(types) {
            assert_eq!(
                instance_type_on(platform, size),
                Some(want),
                "{platform} {size}"
            );
        }
    }
    // The spec's own example.
    assert_eq!(instance_type_on("windows/amd64", "l"), Some("m8a.2xlarge"));
    assert_eq!(instance_type_on("linux/amd64", "s"), Some("m8a.large"));
}

#[test]
fn instance_type_on_gives_none_for_a_platform_with_no_column() {
    // driver.md: `windows/arm64` and `Linux/amd64` are none; both arguments are compared exactly.
    for platform in [
        "windows/arm64",
        "Linux/amd64",
        "linux/AMD64",
        " linux/amd64",
        "linux/amd64 ",
        "linux/amd64/v3",
        "linux/aarch64",
        "darwin/arm64",
        "linux",
        "",
    ] {
        assert_eq!(instance_type_on(platform, "l"), None, "{platform:?}");
    }
}

#[test]
fn instance_type_on_gives_none_for_a_size_outside_sizes_on_every_column() {
    for size in ["xxl", "XL", " m", "m ", "", "2xl"] {
        for (platform, _) in COLUMNS {
            assert_eq!(
                instance_type_on(platform, size),
                None,
                "{platform} {size:?}"
            );
        }
    }
}

#[test]
fn the_first_four_judgments_mean_linux_arm64() {
    // driver.md: `instance_type_for(s)` is `instance_type_on("linux/arm64", s)`, and `meets` is
    // `meets_on("linux/arm64", ...)`.
    for size in SIZES.into_iter().chain(["xxl", "XL", ""]) {
        assert_eq!(
            instance_type_for(size),
            instance_type_on("linux/arm64", size),
            "{size:?}"
        );
    }
    for ty in M9G.into_iter().chain(M8A).chain(["m9g.8xlarge", ""]) {
        for size in SIZES.into_iter().chain(["xxl"]) {
            assert_eq!(
                meets(ty, size),
                meets_on("linux/arm64", ty, size),
                "{ty} {size}"
            );
        }
    }
}

// ---- meets_on -------------------------------------------------------------------------------------------

#[test]
fn meets_on_is_each_columns_truth_table() {
    let mut cases = 0;
    for (platform, types) in COLUMNS {
        for (place, ty) in types.into_iter().enumerate() {
            for (want_place, size) in SIZES.into_iter().enumerate() {
                assert_eq!(
                    meets_on(platform, ty, size),
                    place >= want_place,
                    "meets_on({platform:?}, {ty:?}, {size:?})"
                );
                cases += 1;
            }
        }
    }
    assert_eq!(cases, 48);
}

#[test]
fn the_specs_own_examples_of_meets_on() {
    // driver.md: `meets_on("linux/amd64", "m8a.2xlarge", "m")` is true,
    // `meets_on("linux/amd64", "m9g.2xlarge", "m")` is false and
    // `meets_on("windows/amd64", "m8a.xlarge", "l")` is false.
    assert!(meets_on("linux/amd64", "m8a.2xlarge", "m"));
    assert!(!meets_on("linux/amd64", "m9g.2xlarge", "m"));
    assert!(!meets_on("windows/amd64", "m8a.xlarge", "l"));
}

#[test]
fn a_type_that_only_another_column_holds_meets_no_size() {
    // Pin P28: `m9g.xlarge` on a linux/amd64 host is a type the map does not hold for it, and an
    // m8a type on linux/arm64 is the same.
    for size in SIZES {
        assert!(!meets_on("linux/amd64", "m9g.xlarge", size), "amd64 {size}");
        assert!(
            !meets_on("windows/amd64", "m9g.4xlarge", size),
            "windows {size}"
        );
        assert!(
            !meets_on("linux/arm64", "m8a.4xlarge", size),
            "arm64 {size}"
        );
        assert!(!meets("m8a.4xlarge", size), "meets {size}");
    }
}

#[test]
fn meets_on_is_false_for_a_platform_with_no_column() {
    for size in SIZES {
        for platform in ["windows/arm64", "Linux/amd64", "linux/amd64/v3", ""] {
            assert!(
                !meets_on(platform, "m8a.4xlarge", size),
                "{platform:?} {size}"
            );
            assert!(
                !meets_on(platform, "m9g.4xlarge", size),
                "{platform:?} {size}"
            );
        }
    }
}

#[test]
fn meets_on_is_false_for_a_missing_type_a_type_in_another_case_and_a_size_outside_sizes() {
    for (platform, types) in COLUMNS {
        for size in SIZES {
            assert!(!meets_on(platform, "", size), "{platform} empty type");
            assert!(
                !meets_on(platform, &types[3].to_uppercase(), size),
                "{platform} case"
            );
        }
        for ty in types {
            for size in ["xxl", "XL", "", " m"] {
                assert!(!meets_on(platform, ty, size), "{platform} {ty} {size:?}");
            }
        }
    }
}

// ---- resize_target by host -------------------------------------------------------------------------------

#[test]
fn resize_target_gives_the_largest_pending_size_in_the_hosts_column() {
    for (platform, types) in COLUMNS {
        let key = HostKey::new("swimcoach", platform);
        assert_eq!(resize_target_keyed(&[], &key), None, "{platform}");
        for (size, want) in SIZES.into_iter().zip(types) {
            let one = [pending_on("swimcoach", platform, size)];
            assert_eq!(
                resize_target_keyed(&one, &key),
                Some(want),
                "{platform} {size}"
            );
        }
        let several = [
            pending_on("swimcoach", platform, "s"),
            pending_on("swimcoach", platform, "l"),
            pending_on("swimcoach", platform, "m"),
        ];
        assert_eq!(
            resize_target_keyed(&several, &key),
            Some(types[2]),
            "{platform}"
        );
    }
}

#[test]
fn resize_target_counts_the_projects_leases_on_that_platform_alone() {
    // driver.md's example: `pending` records of the project at `l` on linux/amd64 and at `xl` on
    // linux/arm64: the amd64 host's target is `m8a.2xlarge`, and the arm64 host's `m9g.4xlarge`.
    let leases = [
        pending_on("swimcoach", "linux/amd64", "l"),
        pending_on("swimcoach", "linux/arm64", "xl"),
    ];
    assert_eq!(
        resize_target_keyed(&leases, &HostKey::new("swimcoach", "linux/amd64")),
        Some("m8a.2xlarge")
    );
    assert_eq!(
        resize_target_keyed(&leases, &HostKey::new("swimcoach", "linux/arm64")),
        Some("m9g.4xlarge")
    );
    assert_eq!(
        resize_target_keyed(&leases, &HostKey::new("swimcoach", "windows/amd64")),
        None,
        "A host with no pending lease of its own has no target"
    );
}

#[test]
fn a_pending_lease_on_another_platform_never_sets_this_hosts_type() {
    let leases = [pending_on("swimcoach", "windows/amd64", "xl")];
    assert_eq!(
        resize_target_keyed(&leases, &HostKey::new("swimcoach", "linux/amd64")),
        None
    );
    assert_eq!(
        resize_target_keyed(&leases, &HostKey::new("swimcoach", "linux/arm64")),
        None
    );
    assert_eq!(
        resize_target_keyed(&leases, &HostKey::new("swimcoach", "windows/amd64")),
        Some("m8a.4xlarge")
    );
}

#[test]
fn resize_target_keeps_every_other_selection_rule_on_each_platform() {
    // The same selection as v0.09's: another project's, another driver's, a record with no driver,
    // a size outside SIZES and a record not stored `pending` are ignored, and the platform is
    // compared exactly.
    let key = HostKey::new("p", "linux/amd64");
    let mut ready = pending_on("p", "linux/amd64", "xl");
    ready.state = "ready".to_string();
    let leases = [
        pending_on("other", "linux/amd64", "xl"),
        record("pending", "p", "linux/amd64", Some("fake"), "xl"),
        record("pending", "p", "linux/amd64", None, "xl"),
        pending_on("p", "linux/amd64", "xxl"),
        ready,
        pending_on("p", "Linux/amd64", "xl"),
        pending_on("p", "linux/amd64", "m"),
    ];
    assert_eq!(resize_target_keyed(&leases, &key), Some("m8a.xlarge"));
}

#[test]
fn a_platform_with_no_column_has_no_target() {
    for platform in ["windows/arm64", "Linux/amd64", "linux/amd64/v3"] {
        let leases = [pending_on("p", platform, "m")];
        assert_eq!(
            resize_target_keyed(&leases, &HostKey::new("p", platform)),
            None,
            "{platform}"
        );
    }
}

#[test]
fn resize_target_means_linux_arm64_and_passes_over_every_other_platform() {
    // driver.md: `resize_target(<leases>, <project>)` is the arm64 host's, so a pending lease of
    // the project on another platform is passed over, whatever its size.
    let leases = [
        pending_on("p", "windows/amd64", "xl"),
        pending_on("p", "linux/amd64", "xl"),
        pending_on("p", "linux/arm64", "m"),
    ];
    assert_eq!(resize_target(&leases, "p"), Some("m9g.xlarge"));
    let only_others = [pending_on("p", "windows/amd64", "xl")];
    assert_eq!(resize_target(&only_others, "p"), None);
}
