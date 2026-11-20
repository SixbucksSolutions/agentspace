//! What `sync` MUST have started, in order, with which arguments (operator.md, sync, the table of
//! its ssh and scp calls; steps 3 to 7's messages). Shared by the tests of `sync` and of `deploy`.
#![allow(dead_code)]

use super::*;

pub const SKIP_LINE: &str =
    r"Skipping the localswim CLI: Go or C:\Projects\localswim is missing on the laptop.";

/// Whether the localswim CLI is built in a rig: its `go` finder finds the stand-in, and its own
/// localswim folder is there.
pub fn built(rig: &Rig) -> bool {
    rig.go && rig.has_checkout()
}

/// `sync`'s own progress lines and its ssh, scp and go calls, in the order the spec gives them.
pub fn expected_timeline(fp: &str, claude: bool, built: bool) -> Vec<String> {
    let mut t = vec![
        "say: Copying box/system to the instance...".to_string(),
        "ssh#1".to_string(),
        "scp#1".to_string(),
        format!("say: Copying the agentspace SSH CA's public key ({fp}) to the instance..."),
        "scp#2".to_string(),
        "ssh#2".to_string(),
    ];
    let (mut ssh, mut scp) = (2, 2);
    if claude {
        t.push("say: Copying ~/.claude/CLAUDE.md to the instance...".into());
        ssh += 1;
        t.push(format!("ssh#{ssh}"));
        scp += 1;
        t.push(format!("scp#{scp}"));
    }
    if built {
        t.push("say: Building the localswim CLI for linux/arm64...".into());
        t.push("go#1".into());
        t.push("say: Copying the localswim CLI to the instance...".into());
        scp += 1;
        t.push(format!("scp#{scp}"));
        ssh += 1;
        t.push(format!("ssh#{ssh}"));
    } else {
        t.push(format!("say: {SKIP_LINE}"));
    }
    t
}

/// The timeline of a finished run: `sync`'s lines and calls, the start-and-wait step's
/// `Ready: <id>` and everything of `deploy` left out.
pub fn timeline_of(log: &Log, extra_keep: impl Fn(&str) -> bool) -> Vec<String> {
    log.timeline(|t| {
        !t.starts_with("Ready: ")
            && extra_keep(t)
            && (t.starts_with("Copying ")
                || t.starts_with("Building ")
                || t.starts_with("Skipping "))
    })
}

fn is_ssh(r: &Rec, rig: &Rig, what: &str) -> String {
    rig.check(
        r.args.len() == 4
            && r.args[0] == "-o"
            && r.args[1] == "ClearAllForwardings=yes"
            && r.args[2] == "agentspace",
        &format!("{what}: an ssh MUST be `-o ClearAllForwardings=yes agentspace <remote command>`, the command one argument and the last, with no other option"),
    );
    let cmd = r.args[3].clone();
    rig.check(
        !cmd.contains('%')
            && !cmd.contains('"')
            && !cmd.chars().any(|c| c.is_control())
            && !cmd.ends_with('\\')
            && !cmd.is_empty(),
        &format!("{what}: the remote command MUST hold no %, no double quote, no control character, and not end in a backslash (FA-15)"),
    );
    cmd
}

/// Checks the ssh, scp and go calls of a finished `sync` and what they saw.
pub fn check_calls(rig: &Rig, log: &Log, claude: bool, built: bool) {
    let home = rig.home.clone();
    let ca_pub = home
        .join(".agentspace")
        .join("ca")
        .join("agentspace_ca.pub");
    let operator = home.join(".agentspace").join("operator");

    let ssh = log.calls("ssh");
    let scp = log.calls("scp");
    let want_ssh = 2 + if claude { 1 } else { 0 } + if built { 1 } else { 0 };
    let want_scp = 2 + if claude { 1 } else { 0 } + if built { 1 } else { 0 };
    rig.check(
        ssh.len() == want_ssh && scp.len() == want_scp,
        &format!(
            "sync MUST start {want_ssh} ssh and {want_scp} scp here (push off), and started {} ssh and {} scp",
            ssh.len(),
            scp.len()
        ),
    );

    // ssh 1: remove the staging folder.
    let c1 = is_ssh(ssh[0], rig, "Call 1");
    rig.check(
        c1.contains(STAGING) && c1.contains("rm"),
        "Call 1 MUST remove the staging folder /tmp/agentspace-system",
    );
    // scp 1: box/system, whole and recursively.
    let s1 = scp[0];
    rig.check(
        s1.args.len() == 6
            && s1.args[0] == "-o"
            && s1.args[1] == "ClearAllForwardings=yes"
            && s1.args[2] == "-q"
            && s1.args[3] == "-r"
            && s1.args[5] == format!("agentspace:{STAGING}"),
        "Call 2 MUST be `scp -o ClearAllForwardings=yes -q -r <system folder> agentspace:/tmp/agentspace-system`",
    );
    rig.same_path("Call 2's source", &s1.args[4], &operator.join("system"));
    // scp 2: the CA's public key.
    let s2 = scp[1];
    rig.check(
        s2.args.len() == 5
            && s2.args[0] == "-o"
            && s2.args[1] == "ClearAllForwardings=yes"
            && s2.args[2] == "-q"
            && s2.args[4] == format!("agentspace:{STAGING}/agentspace_ca.pub"),
        "Call 3 MUST be `scp -o ClearAllForwardings=yes -q <ca pub> agentspace:/tmp/agentspace-system/agentspace_ca.pub`",
    );
    rig.same_path("Call 3's source", &s2.args[3], &ca_pub);
    // ssh 2: the install, as root, removing the staging folder only after it.
    let c4 = is_ssh(ssh[1], rig, "Call 4");
    let at_install = c4.find("install.sh");
    let at_rm = c4.rfind("rm");
    rig.check(
        c4.contains(STAGING)
            && c4.contains("sudo")
            && at_install.is_some()
            && at_rm.is_some()
            && at_install < at_rm,
        "Call 4 MUST run the staged install.sh as root, then remove the staging folder",
    );

    let mut next_ssh = 2;
    let mut next_scp = 2;
    if claude {
        let c5 = is_ssh(ssh[next_ssh], rig, "Call 5");
        rig.check(
            c5.contains(".claude")
                && c5.contains("mkdir")
                && c5.contains("-p")
                && !c5.contains("sudo"),
            "Call 5 MUST create ~/.claude with its parents (mkdir -p) as the login user",
        );
        let s = scp[next_scp];
        rig.check(
            s.args.len() == 5
                && s.args[0] == "-o"
                && s.args[1] == "ClearAllForwardings=yes"
                && s.args[2] == "-q"
                && s.args[4] == "agentspace:.claude/CLAUDE.md",
            "Call 6 MUST be `scp -o ClearAllForwardings=yes -q <CLAUDE.md> agentspace:.claude/CLAUDE.md`",
        );
        rig.same_path(
            "Call 6's source",
            &s.args[3],
            &home.join(".claude").join("CLAUDE.md"),
        );
        let snap = log.file_snap("scp", (next_scp + 1) as u32);
        rig.check(
            snap.len() == 1
                && snap[0].1 == CLAUDE_MD.len() as u64
                && snap[0].2 == fnv(CLAUDE_MD.as_bytes()),
            "Call 6 MUST copy the laptop's CLAUDE.md as it is",
        );
        next_ssh += 1;
        next_scp += 1;
    }
    if built {
        let s = scp[next_scp];
        rig.check(
            s.args.len() == 5
                && s.args[0] == "-o"
                && s.args[1] == "ClearAllForwardings=yes"
                && s.args[2] == "-q"
                && s.args[4] == "agentspace:/tmp/localswim",
            "Call 7 MUST be `scp -o ClearAllForwardings=yes -q <built CLI> agentspace:/tmp/localswim`",
        );
        rig.same_path(
            "Call 7's source",
            &s.args[3],
            &operator.join("localswim-linux-arm64"),
        );
        let snap = log.file_snap("scp", (next_scp + 1) as u32);
        let built_bytes = b"FAKE-LOCALSWIM-BINARY\n";
        rig.check(
            snap.len() == 1 && snap[0].2 == fnv(built_bytes),
            "Call 7 MUST copy the file the build wrote",
        );
        let c8 = is_ssh(ssh[next_ssh], rig, "Call 8");
        rig.check(
            c8.contains("/usr/local/lib/localswim/localswim")
                && c8.contains("/tmp/localswim")
                && c8.contains("sudo")
                && c8.contains("755"),
            "Call 8 MUST install /tmp/localswim at /usr/local/lib/localswim/localswim as root, mode 0755",
        );
    }
}

/// Every program a run started saw `AGENTSPACE_NESTED=1` and no `_POSIX_FD_STATE` variable
/// (agentspace-run's public function; the architect's ruling of 2026-11-19).
pub fn check_environment(rig: &Rig, log: &Log) {
    let calls = log.all_calls();
    rig.check(!calls.is_empty(), "The run started no program");
    let bad_nested = calls.iter().filter(|c| c.nested != "1").count();
    let bad_fd = calls.iter().filter(|c| c.fd != 0).count();
    rig.check(
        bad_nested == 0,
        &format!(
            "{bad_nested} of {} programs did not see AGENTSPACE_NESTED=1",
            calls.len()
        ),
    );
    rig.check(
        bad_fd == 0,
        &format!(
            "{bad_fd} of {} programs still saw a _POSIX_FD_STATE variable",
            calls.len()
        ),
    );
}

/// The fingerprint `sync` prints, as the CA check gives it.
pub fn fingerprint(rig: &Rig) -> String {
    match agentspace_ca::check_for_sync(&rig.ctx(), &ca_env()) {
        Ok(fp) => fp,
        Err(f) => panic!("The CA check failed: {}", rig.mask(&f.message)),
    }
}
