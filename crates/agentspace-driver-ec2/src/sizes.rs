//! The map from a size to an instance type, and what follows from it (docs/driver.md, "Sizes and
//! instance types"; docs/decisions.md, D65, D67, D71).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! - **The map MUST live in this module alone.** Every other part of agentspace that needs it MUST
//!   call [`SIZES`], [`instance_type_for`], [`meets`] or [`resize_target`], never restate the table.
//! - **A size and an instance type MUST be compared exactly, with their case** (driver.md, pin
//!   P17): the driver is only ever given a normalized size, and a type comes only from the host
//!   registry.
//! - **A function MUST NOT panic on any input.**

use agentspace_state::Record;

use crate::NAME;

/// The table, smallest size first: each size and its instance type (driver.md, "Sizes and instance
/// types"). The one place the map is written.
const TABLE: [(&str, &str); 4] = [
    ("s", "m9g.large"),
    ("m", "m9g.xlarge"),
    ("l", "m9g.2xlarge"),
    ("xl", "m9g.4xlarge"),
];

/// The sizes the driver supports, smallest first. A size's place is its index in it.
pub const SIZES: [&str; 4] = [TABLE[0].0, TABLE[1].0, TABLE[2].0, TABLE[3].0];

/// The place of `size` in [`SIZES`], compared exactly.
fn size_place(size: &str) -> Option<usize> {
    SIZES.iter().position(|s| *s == size)
}

/// The instance type for `size`, or `None` for text that is not in [`SIZES`]. The size is compared
/// exactly, with its case, so `"XL"` and `" m"` are `None`.
pub fn instance_type_for(size: &str) -> Option<&'static str> {
    size_place(size).map(|place| TABLE[place].1)
}

/// Whether a host of `instance_type` meets a lease of `size`: true only when the type is one the
/// map holds, the size is in [`SIZES`], and the type's place is at or after the size's. A type the
/// map does not hold, the empty text among them, meets no size, and no type meets a size outside
/// [`SIZES`].
pub fn meets(instance_type: &str, size: &str) -> bool {
    let type_place = TABLE.iter().position(|(_, t)| *t == instance_type);
    match (type_place, size_place(size)) {
        (Some(have), Some(want)) => have >= want,
        _ => false,
    }
}

/// The instance type a resizing run gives `project`'s host: the type of the largest size among the
/// records stored `pending`, of `project`, placed by this driver, whose size is in [`SIZES`]; `None`
/// when there is none. It reads nothing but those four members of a record.
pub fn resize_target(leases: &[Record], project: &str) -> Option<&'static str> {
    leases
        .iter()
        .filter(|r| {
            r.state == "pending" && r.project == project && r.driver.as_deref() == Some(NAME)
        })
        .filter_map(|r| size_place(&r.size))
        .max()
        .map(|place| TABLE[place].1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(state: &str, project: &str, driver: Option<&str>, size: &str) -> Record {
        Record {
            sized_for: None,
            plan: None,
            id: "id".to_string(),
            project: project.to_string(),
            state: state.to_string(),
            platform: "linux/arm64".to_string(),
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
    fn resize_target_takes_the_largest_matching_pending_size() {
        let leases = [
            record("pending", "p", Some(NAME), "s"),
            record("pending", "p", Some(NAME), "l"),
            record("pending", "p", Some(NAME), "xxl"),
            record("ready", "p", Some(NAME), "xl"),
            record("pending", "q", Some(NAME), "xl"),
            record("pending", "p", Some("other"), "xl"),
            record("pending", "p", None, "xl"),
        ];
        assert_eq!(resize_target(&leases, "p"), Some("m9g.2xlarge"));
        assert_eq!(resize_target(&leases, "none"), None);
        assert_eq!(resize_target(&[], "p"), None);
    }
}
