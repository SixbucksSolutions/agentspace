//! The zone's guard (operator.md, The time zone, step 3; What the laptop checks, the architect's
//! 2026-11-19 push rulings).
//!
//! The zone comes from `agentspace-zone`, already mapped and checked (steps 1 to 3). The push
//! checks step 3 again only as a guard before the zone goes on a command line, and holds no copy
//! of the legacy table.

/// The guarded zone: `name` as it is when it passes step 3's check
/// (`agentspace_zone::check_zone`), and the empty zone otherwise. `None`, no zone at all, is the
/// empty zone. Only a zone this returns is ever put on the box's command line.
pub fn check_zone(name: Option<&str>) -> String {
    name.and_then(agentspace_zone::check_zone)
        .map(str::to_owned)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_guard_holds_no_copy_of_the_legacy_table() {
        // Step 2 is `agentspace-zone`'s (architect, 2026-11-19, for E0.06-T01-S04): a legacy name
        // that reaches the guard passes the check and is kept as it is.
        for (legacy, _) in agentspace_zone::LEGACY_NAMES {
            assert_eq!(check_zone(Some(legacy)), legacy);
        }
        assert_eq!(check_zone(Some("asia/calcutta")), "asia/calcutta");
    }

    #[test]
    fn the_check() {
        for ok in [
            "America/New_York",
            "UTC",
            "Etc/GMT+5",
            "Etc/GMT-5",
            "_x",
            "+x/-y",
        ] {
            assert_eq!(check_zone(Some(ok)), ok);
        }
        for bad in [
            "",
            "-UTC",
            "not a zone",
            "America/",
            "/UTC",
            "a//b",
            "UTC\n",
            "Zürich",
            "a;b",
            "a'b",
        ] {
            assert_eq!(check_zone(Some(bad)), "", "{bad:?}");
        }
        assert_eq!(check_zone(None), "");
    }
}
