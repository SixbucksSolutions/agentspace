//! A provisioning run's log, read by a test (E0.07-T03-S03; operator.md, provision, Its log).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. A line is
//! `<time> <step> <outcome>[ reason=<reason>]`. The log holds the run's own lines alone: the
//! programs' output goes to the run's streams, not the log (architect, 2026-11-21, for v0.07-T03).
#![allow(dead_code)]

use std::fs;

use super::Rig;
use super::times::parse_utc;

/// What step 6's line says after `<time> ` when its patch wait ended on `failed`, exactly.
pub const PATCH_FAILED_LINE: &str = "sync done; the patch run failed, see the host's journal";

/// One of a run's own lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogLine {
    pub time: i64,
    pub step: String,
    pub outcome: String,
    /// The field between the outcome and the reason: `placed=<n>` or `lease=<id>`, for step 9.
    pub field: Option<String>,
    pub reason: Option<String>,
    /// Whether the line is step 6's `sync done; the patch run failed, see the host's journal`
    /// (operator.md, provision, Its log; pin P27).
    pub patch_failed: bool,
}

impl LogLine {
    /// The step and outcome, `run begun` or `sync failed`.
    pub fn name(&self) -> String {
        format!("{} {}", self.step, self.outcome)
    }
}

/// Reads one line as a run's own line, or `None` when it is not one (such as a program's output).
pub fn parse_log_line(line: &str) -> Option<LogLine> {
    let time = parse_utc(line.get(..20)?)?;
    let rest = line.get(20..)?.strip_prefix(' ')?;
    // The one line with more after `done` (operator.md, provision, Its log; pin P27).
    if rest == PATCH_FAILED_LINE {
        return Some(LogLine {
            time,
            step: "sync".to_string(),
            outcome: "done".to_string(),
            field: None,
            reason: None,
            patch_failed: true,
        });
    }
    let (step, rest) = rest.split_once(' ')?;
    if ![
        "run",
        "deploy",
        "start",
        "sync",
        "bootstrap",
        "record",
        "place",
    ]
    .contains(&step)
    {
        return None;
    }
    let (outcome, reason) = match rest.split_once(" reason=") {
        Some((o, r)) => (o, Some(r.to_string())),
        None => (rest, None),
    };
    // Step 9's result: `place done placed=<n>`, and `place failed lease=<id>` for each lease it
    // left pending (the dev lead's interim reading of the T03 ruling, pending its wording).
    let (outcome, field) = match outcome.split_once(' ') {
        Some((o, f)) => (o, Some(f.to_string())),
        None => (outcome, None),
    };
    if !["begun", "done", "failed"].contains(&outcome) {
        return None;
    }
    if let Some(f) = &field {
        let fits = match (step, outcome) {
            ("place", "done") => f
                .strip_prefix("placed=")
                .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())),
            ("place", "failed") => f
                .strip_prefix("lease=")
                .is_some_and(|id| !id.is_empty() && !id.contains(' ')),
            _ => false,
        };
        if !fits {
            return None;
        }
    }
    Some(LogLine {
        time,
        step: step.to_string(),
        outcome: outcome.to_string(),
        field,
        reason,
        patch_failed: false,
    })
}

impl Rig {
    /// The host's `provision.log` as text (output in another encoding reads lossily), or `None`
    /// when there is none.
    pub fn provision_log(&self, project: &str) -> Option<String> {
        fs::read(agentspace_hosts::host_log_path(&self.home, project))
            .ok()
            .map(|b| String::from_utf8_lossy(&b).into_owned())
    }

    /// The run's own lines in the host's log, in order.
    pub fn own_log_lines(&self, project: &str) -> Vec<LogLine> {
        self.provision_log(project)
            .unwrap_or_default()
            .lines()
            .filter_map(parse_log_line)
            .collect()
    }

    /// The names of the run's own lines, such as `run begun`, in order.
    pub fn own_log_names(&self, project: &str) -> Vec<String> {
        self.own_log_lines(project)
            .iter()
            .map(LogLine::name)
            .collect()
    }
}
