//! The map from a size to an instance type on each platform, and what follows from it
//! (docs/driver.md, "Sizes and instance types"; docs/decisions.md, D65, D67, D71, D94).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! - **The map MUST live in this module alone.** Every other part of agentspace that needs it MUST
//!   call [`SIZES`], [`instance_type_for`], [`meets`], [`resize_target`] or, from v0.12, their
//!   platform's forms, [`instance_type_on`], [`meets_on`] and [`resize_target_keyed`], never
//!   restate the table.
//! - **A size, a platform and an instance type MUST be compared exactly, with their case**
//!   (driver.md, pins P17 and P33): the driver is only ever given normalized values, and a type
//!   comes only from the host registry.
//! - **A type MUST be judged only against its host's own platform's column** (pin P28): a type
//!   that only another column holds meets no size.
//! - **The four items before v0.12 MUST keep their signatures and mean `linux/arm64`** (D101).
//! - **A function MUST NOT panic on any input.**

use agentspace_hosts::{HostKey, LINUX_ARM64};
use agentspace_state::Record;

use crate::NAME;

/// The sizes the driver supports, smallest first. A size's place is its index in it.
pub const SIZES: [&str; 4] = ["s", "m", "l", "xl"];

/// The table: each platform's column, its type for each size in [`SIZES`]'s order (driver.md,
/// "Sizes and instance types", a column per platform from v0.12; D65, D94). The one place the map
/// is written.
const COLUMNS: [(&str, [&str; 4]); 3] = [
    (
        LINUX_ARM64,
        ["m9g.large", "m9g.xlarge", "m9g.2xlarge", "m9g.4xlarge"],
    ),
    (
        "linux/amd64",
        ["m8a.large", "m8a.xlarge", "m8a.2xlarge", "m8a.4xlarge"],
    ),
    (
        "windows/amd64",
        ["m8a.large", "m8a.xlarge", "m8a.2xlarge", "m8a.4xlarge"],
    ),
];

/// The place of `size` in [`SIZES`], compared exactly.
fn size_place(size: &str) -> Option<usize> {
    SIZES.iter().position(|s| *s == size)
}

/// The column of `platform`, compared exactly, or `None` for a platform the table has none for.
fn column(platform: &str) -> Option<&'static [&'static str; 4]> {
    COLUMNS
        .iter()
        .find(|(p, _)| *p == platform)
        .map(|(_, types)| types)
}

/// The place of `instance_type` in `platform`'s column, compared exactly.
fn type_place(platform: &str, instance_type: &str) -> Option<usize> {
    column(platform)?.iter().position(|t| *t == instance_type)
}

/// The instance type for `size` on `linux/arm64`, or `None` for text that is not in [`SIZES`]: it
/// is [`instance_type_on`]`("linux/arm64", size)`. The size is compared exactly, with its case, so
/// `"XL"` and `" m"` are `None`.
pub fn instance_type_for(size: &str) -> Option<&'static str> {
    instance_type_on(LINUX_ARM64, size)
}

/// driver.md's `instance_type_on(<platform>, <size>)`: the type in `platform`'s column for a size
/// in [`SIZES`], and `None` for a size outside it or a platform the table has no column for, both
/// compared exactly. So `instance_type_on("windows/amd64", "l")` is `m8a.2xlarge`, and
/// `instance_type_on("windows/arm64", "l")` is `None`.
pub fn instance_type_on(platform: &str, size: &str) -> Option<&'static str> {
    Some(column(platform)?[size_place(size)?])
}

/// Whether a `linux/arm64` host of `instance_type` meets a lease of `size`: it is
/// [`meets_on`]`("linux/arm64", instance_type, size)`. A type the map does not hold in that column,
/// the empty text among them, meets no size, and no type meets a size outside [`SIZES`].
pub fn meets(instance_type: &str, size: &str) -> bool {
    meets_on(LINUX_ARM64, instance_type, size)
}

/// driver.md's `meets_on(<platform>, <instance type>, <size>)`: true only when the platform has a
/// column, the type is one that column holds, the size is in [`SIZES`], and the type's place in
/// the column is at or after the size's place. A type that only another column holds meets no size.
pub fn meets_on(platform: &str, instance_type: &str, size: &str) -> bool {
    match (type_place(platform, instance_type), size_place(size)) {
        (Some(have), Some(want)) => have >= want,
        _ => false,
    }
}

/// Whether `platform`'s column holds `instance_type`: a type outside it meets no size (D67; pins
/// P14 and P28).
pub(crate) fn is_mapped_on(platform: &str, instance_type: &str) -> bool {
    type_place(platform, instance_type).is_some()
}

/// The instance type a resizing run gives `project`'s `linux/arm64` host: it is
/// [`resize_target_keyed`] of `HostKey::linux_arm64(project)`, so from v0.12 it also passes over a
/// record whose `platform` is not `linux/arm64`. `None` when there is no such record.
pub fn resize_target(leases: &[Record], project: &str) -> Option<&'static str> {
    resize_target_keyed(leases, &HostKey::linux_arm64(project))
}

/// driver.md's `resize_target_on(<leases>, <project>, <platform>)`, for the host `key`: the type,
/// in the key's platform's column, of the largest size among the records stored `pending`, whose
/// `project` and `platform` equal the key's, compared exactly, placed by this driver, and whose
/// size is in [`SIZES`]; `None` when there is none, or the platform has no column. **It counts the
/// project's leases on that platform alone** (D91), and reads no member of a record but `state`,
/// `project`, `platform`, `driver` and `size`.
pub fn resize_target_keyed(leases: &[Record], key: &HostKey) -> Option<&'static str> {
    let types = column(&key.platform)?;
    leases
        .iter()
        .filter(|r| is_runs(r, key))
        .filter_map(|r| size_place(&r.size))
        .max()
        .map(|place| types[place])
}

/// Whether `record` is a `pending` lease a run for the host `key` would place, its size aside: stored
/// `pending`, of the key's project and platform, compared exactly, placed by this driver (driver.md,
/// "On a `ready` host", the fourth bullet; pins P2, P18 and P20; D91).
pub(crate) fn is_runs(record: &Record, key: &HostKey) -> bool {
    record.state == "pending"
        && record.project == key.project
        && record.platform == key.platform
        && record.driver.as_deref() == Some(NAME)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reasons::PLATFORMS;

    fn record(state: &str, project: &str, driver: Option<&str>, size: &str) -> Record {
        on("linux/arm64", state, project, driver, size)
    }

    fn on(platform: &str, state: &str, project: &str, driver: Option<&str>, size: &str) -> Record {
        Record {
            sized_for: None,
            plan: None,
            plan_source: None,
            id: "id".to_string(),
            project: project.to_string(),
            state: state.to_string(),
            platform: platform.to_string(),
            profile: "agent-linux".to_string(),
            size: size.to_string(),
            ttl_seconds: 3600,
            created_at: "2026-11-23T00:00:00Z".to_string(),
            expires_at: None,
            released_at: None,
            ssh: None,
            reasons: None,
            driver: driver.map(str::to_string),
        }
    }

    #[test]
    fn the_map_is_the_tables() {
        assert_eq!(SIZES, ["s", "m", "l", "xl"]);
        assert_eq!(instance_type_for("s"), Some("m9g.large"));
        assert_eq!(instance_type_for("m"), Some("m9g.xlarge"));
        assert_eq!(instance_type_for("l"), Some("m9g.2xlarge"));
        assert_eq!(instance_type_for("xl"), Some("m9g.4xlarge"));
        for odd in ["XL", " m", "xxl", ""] {
            assert_eq!(instance_type_for(odd), None, "{odd:?}");
        }
    }

    #[test]
    fn each_amd64_platform_takes_the_m8a_ladder() {
        for platform in ["linux/amd64", "windows/amd64"] {
            let got: Vec<_> = SIZES
                .iter()
                .map(|s| instance_type_on(platform, s))
                .collect();
            assert_eq!(
                got,
                [
                    Some("m8a.large"),
                    Some("m8a.xlarge"),
                    Some("m8a.2xlarge"),
                    Some("m8a.4xlarge")
                ],
                "{platform}"
            );
        }
        for size in SIZES {
            assert_eq!(
                instance_type_on("linux/arm64", size),
                instance_type_for(size)
            );
        }
    }

    #[test]
    fn a_platform_with_no_column_or_another_case_has_no_type() {
        for platform in ["windows/arm64", "Linux/amd64", "linux/amd64/v3", ""] {
            assert_eq!(instance_type_on(platform, "l"), None, "{platform:?}");
            assert!(!meets_on(platform, "m8a.4xlarge", "s"), "{platform:?}");
        }
        assert_eq!(instance_type_on("linux/amd64", "xxl"), None);
        assert_eq!(instance_type_on("linux/amd64", "L"), None);
    }

    #[test]
    fn every_supported_platform_has_a_column_and_no_other_does() {
        let platforms: Vec<&str> = COLUMNS.iter().map(|(p, _)| *p).collect();
        let supported: Vec<&str> = PLATFORMS.iter().map(|p| p.name).collect();
        assert_eq!(platforms, supported);
    }

    #[test]
    fn meets_compares_places_exactly() {
        assert!(meets("m9g.2xlarge", "m"));
        assert!(meets("m9g.xlarge", "m"));
        assert!(!meets("m9g.xlarge", "l"));
        assert!(!meets("m9g.8xlarge", "s"));
        assert!(!meets("", "s"));
        assert!(!meets("M9G.xlarge", "m"));
        assert!(!meets("m9g.4xlarge", "xxl"));
    }

    #[test]
    fn meets_on_reads_only_the_host_platforms_column() {
        assert!(meets_on("linux/amd64", "m8a.2xlarge", "m"));
        assert!(!meets_on("linux/amd64", "m9g.2xlarge", "m"));
        assert!(!meets_on("windows/amd64", "m8a.xlarge", "l"));
        assert!(meets_on("windows/amd64", "m8a.4xlarge", "xl"));
        // The first four mean linux/arm64: an m8a type meets nothing there.
        assert!(!meets("m8a.4xlarge", "s"));
        assert!(!meets_on("linux/arm64", "m8a.4xlarge", "s"));
        for t in ["m9g.large", "m9g.xlarge", "m9g.2xlarge", "m9g.4xlarge"] {
            for s in SIZES {
                assert_eq!(meets(t, s), meets_on("linux/arm64", t, s), "{t} {s}");
            }
        }
        assert!(is_mapped_on("linux/amd64", "m8a.large"));
        assert!(!is_mapped_on("linux/amd64", "m9g.large"));
        assert!(!is_mapped_on("windows/arm64", "m8a.large"));
    }

    #[test]
    fn resize_target_takes_the_largest_matching_pending_size() {
        let leases = [
            record("pending", "p", Some(NAME), "s"),
            record("pending", "p", Some(NAME), "l"),
            record("pending", "p", Some(NAME), "xxl"),
            record("ready", "p", Some(NAME), "xl"),
            record("pending", "q", Some(NAME), "xl"),
            record("pending", "p", Some("other"), "xl"),
            record("pending", "p", None, "xl"),
            // From v0.12, a lease of the project on another platform never sets this host's type.
            on("linux/amd64", "pending", "p", Some(NAME), "xl"),
            on("windows/amd64", "pending", "p", Some(NAME), "xl"),
        ];
        assert_eq!(resize_target(&leases, "p"), Some("m9g.2xlarge"));
        assert_eq!(resize_target(&leases, "none"), None);
        assert_eq!(resize_target(&[], "p"), None);
    }

    #[test]
    fn resize_target_keyed_counts_the_projects_leases_on_its_platform_alone() {
        // driver.md's example: `l` on linux/amd64 and `xl` on linux/arm64.
        let leases = [
            on("linux/amd64", "pending", "p", Some(NAME), "l"),
            on("linux/arm64", "pending", "p", Some(NAME), "xl"),
            on("linux/amd64", "pending", "p", Some("other"), "xl"),
            on("linux/amd64", "ready", "p", Some(NAME), "xl"),
            on("linux/amd64", "pending", "q", Some(NAME), "xl"),
        ];
        assert_eq!(
            resize_target_keyed(&leases, &HostKey::new("p", "linux/amd64")),
            Some("m8a.2xlarge")
        );
        assert_eq!(
            resize_target_keyed(&leases, &HostKey::linux_arm64("p")),
            Some("m9g.4xlarge")
        );
        assert_eq!(
            resize_target_keyed(&leases, &HostKey::new("p", "windows/amd64")),
            None
        );
        // A platform with no column gives none, whatever its records.
        let odd = [on("windows/arm64", "pending", "p", Some(NAME), "s")];
        assert_eq!(
            resize_target_keyed(&odd, &HostKey::new("p", "windows/arm64")),
            None
        );
        assert_eq!(
            resize_target(&leases, "p"),
            resize_target_keyed(&leases, &HostKey::linux_arm64("p"))
        );
    }
}
