//! A provisioning run's log, read by a test (E0.07-T03-S03; operator.md, provision, Its log).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. A line is
//! `<time> <step> <outcome>[ reason=<reason>]`; the programs' output sits between a run's own
//! lines, and a test tells the two apart by the form.
#![allow(dead_code)]

use std::fs;

use super::Rig;
use super::times::parse_utc;

/// One of a run's own lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogLine {
    pub time: i64,
    pub step: String,
    pub outcome: String,
    pub reason: Option<String>,
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
    if !["begun", "done", "failed"].contains(&outcome) {
        return None;
    }
    Some(LogLine {
        time,
        step: step.to_string(),
        outcome: outcome.to_string(),
        reason,
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
