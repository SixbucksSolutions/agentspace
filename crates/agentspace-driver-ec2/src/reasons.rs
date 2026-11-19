//! What the EC2 driver supports, and the reasons for what it does not (docs/driver.md, "What it
//! supports" and "Unsatisfiable requests"; docs/lease-api.md, "Fields").
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! - **Matching MUST be exact on the normalized values.** `agent-linux@1` is not `agent-linux`.
//! - **Every reason that applies MUST be listed, each code once, in lease-api.md's order:** the
//!   platform, then the profile, then the size.
//! - **A reason's message MUST quote the value that cannot be met, and MUST name no backend.**

use agentspace_driver_api::Reason;

use crate::text::quote;

/// The one platform the driver supports.
const PLATFORM: &str = "linux/arm64";
/// The one profile it supports, with no version.
const PROFILE: &str = "agent-linux";
/// The one size it supports.
const SIZE: &str = "m";

/// The reasons a request cannot be met, in lease-api.md's order: empty when the request is
/// supported. The driver never answers `no_capacity`.
pub fn reasons(platform: &str, profile: &str, size: &str) -> Vec<Reason> {
    let mut out = Vec::new();
    if platform != PLATFORM {
        out.push(Reason {
            code: "unsupported_platform",
            message: format!(
                "Platform '{}' is not supported; v0 supports linux/arm64 only.",
                quote(platform)
            ),
        });
    }
    if profile != PROFILE {
        out.push(Reason {
            code: "unsupported_profile",
            message: format!(
                "Profile '{}' is not supported; v0 supports agent-linux with no version only.",
                quote(profile)
            ),
        });
    }
    if size != SIZE {
        out.push(Reason {
            code: "unsupported_size",
            message: format!(
                "Size '{}' is not supported; v0 supports size m only.",
                quote(size)
            ),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codes(r: &[Reason]) -> Vec<&'static str> {
        r.iter().map(|r| r.code).collect()
    }

    #[test]
    fn the_supported_request_has_no_reason() {
        assert!(reasons("linux/arm64", "agent-linux", "m").is_empty());
    }

    #[test]
    fn each_value_alone_gives_its_own_reason() {
        assert_eq!(
            codes(&reasons("linux/amd64", "agent-linux", "m")),
            ["unsupported_platform"]
        );
        assert_eq!(
            codes(&reasons("linux/arm64", "other", "m")),
            ["unsupported_profile"]
        );
        assert_eq!(
            codes(&reasons("linux/arm64", "agent-linux", "xl")),
            ["unsupported_size"]
        );
    }

    #[test]
    fn a_profile_with_a_version_is_unsupported() {
        assert_eq!(
            codes(&reasons("linux/arm64", "agent-linux@1", "m")),
            ["unsupported_profile"]
        );
    }

    #[test]
    fn every_combination_lists_each_code_once_in_lease_api_order() {
        let order = [
            "unsupported_platform",
            "unsupported_profile",
            "unsupported_size",
        ];
        for mask in 1..8u8 {
            let platform = if mask & 1 != 0 {
                "windows/amd64"
            } else {
                "linux/arm64"
            };
            let profile = if mask & 2 != 0 { "x@2" } else { "agent-linux" };
            let size = if mask & 4 != 0 { "xl" } else { "m" };
            let got = codes(&reasons(platform, profile, size));
            let want: Vec<&str> = order
                .iter()
                .enumerate()
                .filter(|(i, _)| mask & (1 << i) != 0)
                .map(|(_, c)| *c)
                .collect();
            assert_eq!(got, want, "mask {mask}");
        }
    }

    #[test]
    fn messages_quote_the_value_and_follow_lease_api_wording() {
        let r = reasons("windows/amd64", "agent-linux@1", "xl");
        assert_eq!(
            r[0].message,
            "Platform 'windows/amd64' is not supported; v0 supports linux/arm64 only."
        );
        assert_eq!(
            r[1].message,
            "Profile 'agent-linux@1' is not supported; v0 supports agent-linux with no version only."
        );
        assert_eq!(
            r[2].message,
            "Size 'xl' is not supported; v0 supports size m only."
        );
    }

    #[test]
    fn a_control_character_in_a_value_is_escaped() {
        let r = reasons("linux/arm64", "agent-linux", "a\nb");
        assert_eq!(
            r[0].message,
            "Size 'a\\u000ab' is not supported; v0 supports size m only."
        );
    }
}
