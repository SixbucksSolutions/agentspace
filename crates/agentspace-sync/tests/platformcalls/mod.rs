//! What the tests of a host on a platform share (E0.12-T03-S03, #0256, part 2): the keyed entry
//! points of `agentspace-sync` (`sync_keyed_using`, `deploy_keyed_using`, `bootstrap_keyed_using`
//! and `provision_keyed_using`), the host registry's records by platform, the names a host has by
//! platform, and the stand-in `aws`'s answers for a platform's image parameter.
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Written from docs/operator.md, A host's
//! platform and its names, The platform seam, deploy, sync, bootstrap and provision, and from
//! docs/state.md, The host registry, A host's platform and names, A host's folder, A host's
//! operator alias, alone. **Every name below is computed from operator.md's table, by hand, and
//! never from `agentspace_hosts::HostKey`'s methods**, so a mistake there is seen here.
//!
//! The keyed entry points are called only through [`Rig`]'s methods in this file, so a change to
//! their signatures changes this file and no test.
#![cfg(windows)]
#![allow(dead_code)]

use agentspace_hosts::HostKey;
use agentspace_operator_api::Failure;

use crate::common::hosts::PROJECT;
use crate::common::{ID, ID2, RecSay, Rig, ca_env, svec};
use crate::waitcalls::FakeClock;

pub mod encoded;

pub const ARM64: &str = "linux/arm64";
pub const AMD64: &str = "linux/amd64";
pub const WINDOWS: &str = "windows/amd64";

/// A third and a fourth host's instances, made up (they MUST NOT match the identifier scan).
pub const ID3: &str = "i-abcdef12345678901";
pub const ID4: &str = "i-0a0a0a0a0a0a0a0a0";

/// A made-up image, the form of `^ami-[0-9a-f]{8}([0-9a-f]{9})?\z` with eight digits, and another
/// with seventeen. Neither is a real image, so the repository's rule on image IDs holds.
pub const IMAGE: &str = "ami-0a0a0a0a";
pub const IMAGE17: &str = "ami-0b0b0b0b0b0b0b0b0";

/// The platforms' public image parameters (operator.md, The platform seam, the image row).
pub const AMD64_PARAMETER: &str =
    "/aws/service/canonical/ubuntu/server/26.04/stable/current/amd64/hvm/ebs-gp3/ami-id";
pub const WINDOWS_PARAMETER: &str =
    "/aws/service/ami-windows-latest/Windows_Server-2025-English-Full-Base";

/// The platform's name part, `<os>-<arch>`, or `None` for `linux/arm64`, which never appears in a
/// name (operator.md, A host's platform and its names).
pub fn name_part(platform: &str) -> Option<&'static str> {
    match platform {
        ARM64 => None,
        AMD64 => Some("linux-amd64"),
        WINDOWS => Some("windows-amd64"),
        other => panic!("{other} is no platform of these tests"),
    }
}

/// The host's name: `demo`, or `demo.<os>-<arch>`.
pub fn host_name(platform: &str) -> String {
    match name_part(platform) {
        None => PROJECT.to_string(),
        Some(part) => format!("{PROJECT}.{part}"),
    }
}

/// The host's stack: `agentspace-host-demo`, or `agentspace-<os>-<arch>-host-demo`.
pub fn stack(platform: &str) -> String {
    match name_part(platform) {
        None => format!("agentspace-host-{PROJECT}"),
        Some(part) => format!("agentspace-{part}-host-{PROJECT}"),
    }
}

/// The host's operator alias and host-key name: `agentspace-host.<host name>`.
pub fn alias(platform: &str) -> String {
    format!("agentspace-host.{}", host_name(platform))
}

/// The host's login user (operator.md, The platform seam, the login user row).
pub fn user(platform: &str) -> &'static str {
    if platform == WINDOWS {
        "agent"
    } else {
        "ubuntu"
    }
}

/// The platform's image parameter, for a platform that has one.
pub fn parameter(platform: &str) -> &'static str {
    match platform {
        AMD64 => AMD64_PARAMETER,
        WINDOWS => WINDOWS_PARAMETER,
        other => panic!("{other} has no image parameter"),
    }
}

pub fn key(platform: &str) -> HostKey {
    HostKey::new(PROJECT, platform)
}

/// The arguments of the `ssm get-parameter` call for `name` (operator.md, AWS failures: the
/// service, the operation, `--region us-west-2`, `--output json`, then the operation's own).
pub fn get_parameter_args(name: &str) -> Vec<String> {
    svec(&[
        "ssm",
        "get-parameter",
        "--region",
        "us-west-2",
        "--output",
        "json",
        "--name",
        name,
    ])
}

/// One record of `PROJECT` to seed into `hosts.json`, besides the box's.
#[derive(Clone, Copy)]
pub struct PRec {
    pub platform: &'static str,
    pub state: &'static str,
    pub instance_id: Option<&'static str>,
    pub instance_type: Option<&'static str>,
    pub last_attempt_at: Option<&'static str>,
}

impl PRec {
    pub fn new(platform: &'static str, state: &'static str) -> PRec {
        PRec {
            platform,
            state,
            instance_id: None,
            instance_type: None,
            last_attempt_at: None,
        }
    }
    pub fn id(mut self, id: &'static str) -> PRec {
        self.instance_id = Some(id);
        self
    }
    pub fn itype(mut self, t: &'static str) -> PRec {
        self.instance_type = Some(t);
        self
    }
    pub fn attempt(mut self, at: &'static str) -> PRec {
        self.last_attempt_at = Some(at);
        self
    }

    /// The record's JSON, its members in state.md's order. A `linux/arm64` record holds no
    /// `platform` (state.md, A host's platform and names: a writer MUST NOT write it).
    pub fn json(&self) -> String {
        let mut s = format!("{{\"project\":\"{PROJECT}\"");
        if self.platform != ARM64 {
            s.push_str(&format!(",\"platform\":\"{}\"", self.platform));
        }
        s.push_str(&format!(
            ",\"stack\":\"{}\",\"state\":\"{}\"",
            stack(self.platform),
            self.state
        ));
        if let Some(id) = self.instance_id {
            s.push_str(&format!(",\"instance_id\":\"{id}\""));
        }
        if let Some(t) = self.instance_type {
            s.push_str(&format!(",\"instance_type\":\"{t}\""));
        }
        if let Some(at) = self.last_attempt_at {
            s.push_str(&format!(",\"last_attempt_at\":\"{at}\""));
        }
        s.push('}');
        s
    }
}

/// The box's record, with its instance ID.
pub const BOX_RECORD: &str = "{\"project\":\"swimcoach\",\"stack\":\"agentbox\",\"state\":\"ready\",\"instance_id\":\"i-1234567890abcdef0\"}";

impl Rig {
    /// Writes `hosts.json` holding the box's record, then `recs`, in that order (parent only).
    pub fn seed_recs(&self, recs: &[PRec]) {
        let mut records = vec![BOX_RECORD.to_string()];
        records.extend(recs.iter().map(PRec::json));
        let text = format!("{{\"version\":0,\"hosts\":[{}]}}\n", records.join(","));
        self.write_home(".agentspace/hosts.json", text.as_bytes());
    }

    /// `projects.json` listing `PROJECT`, with `platforms` as the entry's approvals, or no
    /// `platforms` member for `None` (parent only).
    pub fn list_project_for(&self, platforms: Option<&[&str]>) {
        let members = match platforms {
            None => String::new(),
            Some(list) => {
                let items: Vec<String> = list
                    .iter()
                    .map(|p| format!("{{\"platform\":\"{p}\",\"approved\":\"2026-11-26\"}}"))
                    .collect();
                format!(",\"platforms\":[{}]", items.join(","))
            }
        };
        let text = format!(
            "{{\"version\":0,\"projects\":[{{\"name\":\"{PROJECT}\",\"approved\":\"2026-11-20\"{members}}}]}}\n"
        );
        self.write_home(".agentspace/projects.json", text.as_bytes());
    }

    /// The stand-in `aws` answers `ssm get-parameter` with `value` as `Parameter.Value`.
    pub fn image_is(&self, value: &str) {
        self.rule(
            "aws",
            0,
            "get-parameter",
            0,
            &format!("{{\"Parameter\":{{\"Value\":\"{value}\"}}}}"),
            "",
        );
    }

    /// The stand-in `aws` answers `ssm get-parameter` with `raw`, whole.
    pub fn parameter_answers(&self, raw: &str) {
        self.rule("aws", 0, "get-parameter", 0, raw, "");
    }

    /// The record of `PROJECT` on `platform`, or a panic naming the platform.
    pub fn record_on(&self, platform: &str) -> agentspace_hosts::Host {
        match self
            .read_hosts()
            .hosts
            .into_iter()
            .find(|h| h.project == PROJECT && h.platform() == platform)
        {
            Some(h) => h,
            None => panic!("The host registry has no record of '{PROJECT}' on {platform}"),
        }
    }

    /// Every record's JSON in the registry file, in the file's order, for a comparison of bytes.
    pub fn registry_text(&self) -> String {
        self.hosts_bytes()
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default()
    }

    // ---- The keyed entry points, over the rig's seams

    /// `sync_keyed_using` for `key`.
    pub fn do_sync_keyed(&self, say: &RecSay, key: &HostKey) -> Result<(), Failure> {
        let ctx = self.ctx();
        let run = agentspace_run::Runner::new(ctx.clone());
        let find_go = || self.found_go();
        agentspace_sync::Seams::real_except_go(&find_go, self.localswim(), |seams| {
            agentspace_sync::sync_keyed_using(&ctx, &ca_env(), key, say, &run, &run, seams)
        })
    }

    /// `sync_keyed_using` for `key`, with the fake clock the patch wait runs on.
    pub fn do_sync_keyed_clocked(
        &self,
        say: &RecSay,
        key: &HostKey,
        clock: &FakeClock,
    ) -> Result<(), Failure> {
        let ctx = self.ctx();
        let run = agentspace_run::Runner::new(ctx.clone());
        let find_go = || self.found_go();
        agentspace_sync::Seams::real_except_go_and_clock(
            &find_go,
            self.localswim(),
            clock,
            |seams| agentspace_sync::sync_keyed_using(&ctx, &ca_env(), key, say, &run, &run, seams),
        )
    }

    /// `deploy_keyed_using` for `key`, with `-InstanceType`'s value.
    pub fn do_deploy_keyed(
        &self,
        say: &RecSay,
        key: &HostKey,
        instance_type: Option<&str>,
    ) -> Result<(), Failure> {
        let ctx = self.ctx();
        let run = agentspace_run::Runner::new(ctx.clone());
        let find_go = || self.found_go();
        agentspace_sync::Seams::real_except_go(&find_go, self.localswim(), |seams| {
            agentspace_sync::deploy_keyed_using(
                &ctx,
                &ca_env(),
                key,
                instance_type,
                say,
                &run,
                &run,
                seams,
            )
        })
    }

    /// `bootstrap_keyed_using` for `key`.
    pub fn do_bootstrap_keyed(&self, say: &RecSay, key: &HostKey) -> Result<(), Failure> {
        let ctx = self.ctx();
        let run = agentspace_run::Runner::new(ctx.clone());
        let find_go = || self.found_go();
        agentspace_sync::Seams::real_except_go(&find_go, self.localswim(), |seams| {
            agentspace_sync::bootstrap_keyed_using(&ctx, &ca_env(), key, say, &run, &run, seams)
        })
    }

    /// `provision_keyed_using` for `key`.
    pub fn do_provision_keyed(&self, say: &RecSay, key: &HostKey) -> Result<(), Failure> {
        let ctx = self.ctx();
        let run = agentspace_run::Runner::new(ctx.clone());
        let find_go = || self.found_go();
        agentspace_sync::Seams::real_except_go(&find_go, self.localswim(), |seams| {
            agentspace_sync::provision_keyed_using(&ctx, &ca_env(), key, say, &run, &run, seams)
        })
    }
}

/// A throwaway home for a host on a platform: a CA, a public key, a parameters file that names
/// `InstanceType` and `RootVolumeGiB` (which a new host's stack MUST NOT take from it), the
/// ProxyCommand program beside the operator, and a registry holding the box and `recs`. `go` says
/// whether the rig's `go` finder finds the stand-in.
pub fn plat_rig(go: bool, recs: &[PRec]) -> Rig {
    let rig = Rig::new(go);
    rig.make_ca();
    rig.with_deploy_inputs();
    rig.write_home(
        ".agentspace/stack-parameters.json",
        br#"{"version":0,"parameters":{"SubnetId":"SUBNET_VALUE","VpcId":"VPC_VALUE","InstanceType":"m9g.8xlarge","RootVolumeGiB":"40"}}"#,
    );
    rig.beside_operator(true);
    rig.seed_recs(recs);
    rig
}

/// The ids the rigs' made-up hosts use, for a test that names them.
pub fn ids() -> [&'static str; 4] {
    [ID, ID2, ID3, ID4]
}
