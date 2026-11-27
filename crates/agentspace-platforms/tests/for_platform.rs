//! `for_platform` picks each supported platform's seam, and no other (docs/operator.md, The
//! platform seam; D98, D101).

use agentspace_platforms::for_platform;

#[test]
fn each_supported_platform_gets_its_own_seam() {
    for platform in ["linux/arm64", "linux/amd64", "windows/amd64"] {
        let seam = for_platform(platform).expect("a supported platform has a seam");
        assert_eq!(seam.platform(), platform);
    }
}

#[test]
fn an_unsupported_or_unnormalized_platform_gets_none() {
    for platform in [
        "windows/arm64",
        "linux/amd64/v3",
        "Linux/AMD64",
        "linux/x86_64",
        "",
    ] {
        assert!(for_platform(platform).is_none(), "{platform}");
    }
}

#[test]
fn the_login_user_follows_the_platform() {
    assert_eq!(for_platform("linux/arm64").unwrap().login_user(), "ubuntu");
    assert_eq!(for_platform("linux/amd64").unwrap().login_user(), "ubuntu");
    assert_eq!(for_platform("windows/amd64").unwrap().login_user(), "agent");
}
