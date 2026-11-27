//! What the EC2 driver supports, and the reasons for what it does not (docs/driver.md, "What it
//! supports" and "Unsatisfiable requests"; docs/lease-api.md, "Fields"; D93, D98).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! - **Matching MUST be exact on the normalized values** (pin P21). `agent-linux@1` is not
//!   `agent-linux`, and `linux/amd64/v3` is not `linux/amd64`.
//! - **Every reason that applies MUST be listed, each code once, in lease-api.md's order:** the
//!   platform, then the profile, then the size (pin P26).
//! - **On a supported platform, a profile other than that platform's own MUST be
//!   `unsupported_profile`; on a platform that is not supported, the profile MUST be
//!   `unsupported_profile` only when no supported platform supports it** (D93; pin P2).
//! - **A reason's message MUST quote the value that cannot be met, MUST name no backend, and MUST
//!   be driver.md's byte for byte.**

use agentspace_driver_api::Reason;

use crate::sizes::SIZES;
use crate::text::quote;

/// One supported platform: its normalized name, its one profile, which has no version, and the
/// user a lease on it logs in as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Platform {
    pub name: &'static str,
    pub profile: &'static str,
    pub user: &'static str,
}

/// The platforms the driver supports (driver.md, "What it supports"; D91, D93, D95, D97). The one
/// place the table is written.
pub const PLATFORMS: [Platform; 3] = [
    Platform {
        name: "linux/arm64",
        profile: "agent-linux",
        user: "ubuntu",
    },
    Platform {
        name: "linux/amd64",
        profile: "agent-linux",
        user: "ubuntu",
    },
    Platform {
        name: "windows/amd64",
        profile: "agent-windows",
        user: "agent",
    },
];

/// The supported platform `platform` names, compared exactly.
pub fn supported(platform: &str) -> Option<&'static Platform> {
    PLATFORMS.iter().find(|p| p.name == platform)
}

/// The user a lease on `platform` logs in as: the certificate's principal, the returned user and
/// the alias's `User` (driver.md, "What it supports"; D95; pin P19): `agent` on `windows/amd64` and
/// `ubuntu` on both Linux platforms. Only a supported request is ever issued anything, so the
/// `ubuntu` given for any other platform is never written for one.
pub fn login_user(platform: &str) -> &'static str {
    supported(platform).map_or("ubuntu", |p| p.user)
}

/// The reasons a request cannot be met, in lease-api.md's order: empty when the request is
/// supported. `no_capacity` is decided by the host, never here.
pub fn reasons(platform: &str, profile: &str, size: &str) -> Vec<Reason> {
    let mut out = Vec::new();
    let on = supported(platform);
    if on.is_none() {
        out.push(Reason {
            code: "unsupported_platform",
            message: format!(
                "Platform '{}' is not supported; agentspace supports linux/arm64, linux/amd64 and windows/amd64.",
                quote(platform)
            ),
        });
    }
    let profile_unmet = match on {
        Some(p) => p.profile != profile,
        None => !PLATFORMS.iter().any(|p| p.profile == profile),
    };
    if profile_unmet {
        out.push(Reason {
            code: "unsupported_profile",
            message: format!(
                "Profile '{}' is not supported on platform '{}'; agentspace supports agent-linux on linux/arm64 and linux/amd64, and agent-windows on windows/amd64, each with no version only.",
                quote(profile),
                quote(platform)
            ),
        });
    }
    if !SIZES.contains(&size) {
        out.push(Reason {
            code: "unsupported_size",
            message: format!(
                "Size '{}' is not supported; v0 supports sizes s, m, l and xl.",
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
    fn each_platform_with_its_own_profile_has_no_reason_at_any_size() {
        for (platform, profile) in [
            ("linux/arm64", "agent-linux"),
            ("linux/amd64", "agent-linux"),
            ("windows/amd64", "agent-windows"),
        ] {
            for size in ["s", "m", "l", "xl"] {
                assert!(
                    reasons(platform, profile, size).is_empty(),
                    "{platform} {profile} {size}"
                );
            }
        }
    }

    #[test]
    fn a_profile_that_is_not_the_platforms_own_is_unsupported_profile_alone() {
        for (platform, profile) in [
            ("windows/amd64", "agent-linux"),
            ("linux/amd64", "agent-windows"),
            ("linux/arm64", "agent-windows"),
            ("linux/arm64", "other"),
            ("windows/amd64", "agent-windows@1"),
            ("linux/amd64", "agent-linux@1"),
        ] {
            assert_eq!(
                codes(&reasons(platform, profile, "m")),
                ["unsupported_profile"],
                "{platform} {profile}"
            );
        }
    }

    #[test]
    fn windows_arm64_and_a_variant_are_unsupported_platforms() {
        // D98, pin P21: matching is exact, so none of these is a supported platform.
        for platform in [
            "windows/arm64",
            "linux/amd64/v3",
            "Linux/amd64",
            "linux/aarch64",
        ] {
            assert_eq!(
                codes(&reasons(platform, "agent-linux", "l")),
                ["unsupported_platform"],
                "{platform}"
            );
        }
    }

    #[test]
    fn on_an_unsupported_platform_a_profile_some_platform_supports_adds_no_reason() {
        // Pin P2: `windows/arm64` with `agent-linux` or `agent-windows` lists the platform alone.
        for profile in ["agent-linux", "agent-windows"] {
            assert_eq!(
                codes(&reasons("windows/arm64", profile, "m")),
                ["unsupported_platform"],
                "{profile}"
            );
        }
        assert_eq!(
            codes(&reasons("windows/arm64", "agent-linux@1", "m")),
            ["unsupported_platform", "unsupported_profile"]
        );
        assert_eq!(
            codes(&reasons("windows/arm64", "agent-linux", "xxl")),
            ["unsupported_platform", "unsupported_size"]
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
                "windows/arm64"
            } else {
                "windows/amd64"
            };
            let profile = if mask & 2 != 0 {
                "x@2"
            } else {
                "agent-windows"
            };
            let size = if mask & 4 != 0 { "xxl" } else { "m" };
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
    fn messages_are_driver_mds_byte_for_byte() {
        let r = reasons("windows/arm64", "agent-linux@1", "xxl");
        assert_eq!(
            r[0].message,
            "Platform 'windows/arm64' is not supported; agentspace supports linux/arm64, linux/amd64 and windows/amd64."
        );
        assert_eq!(
            r[1].message,
            "Profile 'agent-linux@1' is not supported on platform 'windows/arm64'; agentspace supports agent-linux on linux/arm64 and linux/amd64, and agent-windows on windows/amd64, each with no version only."
        );
        assert_eq!(
            r[2].message,
            "Size 'xxl' is not supported; v0 supports sizes s, m, l and xl."
        );
        // The profile's message names the request's platform when it is supported too.
        let r = reasons("windows/amd64", "agent-linux", "m");
        assert_eq!(
            r[0].message,
            "Profile 'agent-linux' is not supported on platform 'windows/amd64'; agentspace supports agent-linux on linux/arm64 and linux/amd64, and agent-windows on windows/amd64, each with no version only."
        );
    }

    #[test]
    fn a_control_character_in_a_value_is_escaped() {
        let r = reasons("linux/arm64", "agent-linux", "a\nb");
        assert_eq!(
            r[0].message,
            "Size 'a\\u000ab' is not supported; v0 supports sizes s, m, l and xl."
        );
        let r = reasons("a\u{1}b", "agent-linux", "m");
        assert_eq!(
            r[0].message,
            "Platform 'a\\u0001b' is not supported; agentspace supports linux/arm64, linux/amd64 and windows/amd64."
        );
    }

    #[test]
    fn each_platform_has_its_login_user() {
        assert_eq!(login_user("linux/arm64"), "ubuntu");
        assert_eq!(login_user("linux/amd64"), "ubuntu");
        assert_eq!(login_user("windows/amd64"), "agent");
    }
}
