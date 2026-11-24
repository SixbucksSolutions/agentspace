//! `status -All`'s lease lines: `Leases    <n> live` for every host, and `Guard     unexplained;
//! stops at <h:mm><am|pm>` for a running host that no live lease explains (docs/operator.md, status,
//! status -All; D56, D60, D63).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! The decisions about a lease (live, last lease end) are `agentspace-state`'s, and are never made
//! here: this module reads the lease state file, once, takes the report's time, and writes the two
//! lines. It never writes the file, never takes its lock, and never puts a lease's ID or any other
//! member of a lease on a line: only a count (D60).

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use agentspace_guard::GRACE_SECONDS;
use agentspace_state::{Record, StoreError, last_lease_end, live_count, parse_time, read};

/// The lease state file's name in the state folder (docs/state.md, The lease state file).
const LEASES_FILE: &str = "leases.json";

/// The lease state file as one reading of it: its records, and the report's time, which every
/// host's count and stop time use (operator.md, status -All).
#[derive(Debug, Clone)]
pub(crate) struct LeaseView {
    leases: Vec<Record>,
    /// The report's time, in Unix seconds, taken as the file was read.
    now: i64,
}

/// A time as Unix seconds, truncated to the whole second.
pub(crate) fn unix_seconds(time: SystemTime) -> i64 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(after) => i64::try_from(after.as_secs()).unwrap_or(i64::MAX),
        Err(before) => {
            let d = before.duration();
            -i64::try_from(d.as_secs()).unwrap_or(i64::MAX) - i64::from(d.subsec_nanos() > 0)
        }
    }
}

/// One sentence fragment, with no line break and no closing period, for the message's `<reason>`.
fn fragment(text: &str) -> String {
    let one_line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    one_line.trim_end_matches('.').to_string()
}

/// Reads `<state folder>\leases.json` whole, without its lock, and never writes it, at the report's
/// time `now`. `Err` holds the `<reason>` of the message
/// [`unreadable_message`] makes: a file that is missing, cannot be read or is not valid, by
/// state.md's checks. **A missing file is not "no leases"** (operator.md, status -All, P18, P24),
/// though the lease CLI reads it so: `agentspace_state::read` says it is, so this checks first.
pub(crate) fn read_leases(state_dir: &Path, now: i64) -> Result<LeaseView, String> {
    let path = state_dir.join(LEASES_FILE);
    let missing = matches!(
        std::fs::metadata(&path),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound
    );
    match read(state_dir) {
        Err(
            StoreError::Unreadable(why)
            | StoreError::NotValid(why)
            | StoreError::CannotLock(why)
            | StoreError::Unwritable(why),
        ) => Err(fragment(&why)),
        Err(StoreError::Busy) => Err("the state file could not be read".to_string()),
        // `read` calls a missing file no leases, and so a missing file is read here, not there.
        Ok(_) if missing => Err("the file does not exist".to_string()),
        Ok(file) => Ok(LeaseView {
            leases: file.leases,
            now,
        }),
    }
}

/// `agentspace: The lease state file <path> cannot be read: <reason>.`, which `status -All` prints
/// once, after the last host's part (operator.md, status -All, P18).
pub(crate) fn unreadable_message(state_dir: &Path, reason: &str) -> String {
    format!(
        "agentspace: The lease state file {} cannot be read: {reason}.",
        state_dir.join(LEASES_FILE).display()
    )
}

impl LeaseView {
    /// A view over records already read, at `now`.
    #[cfg(test)]
    pub(crate) fn new(leases: Vec<Record>, now: i64) -> LeaseView {
        LeaseView { leases, now }
    }

    /// The report's time, in Unix seconds.
    #[cfg(test)]
    pub(crate) fn now(&self) -> i64 {
        self.now
    }

    /// How many of the project's leases are live at the report's time.
    pub(crate) fn live(&self, project: &str) -> usize {
        live_count(&self.leases, project, self.now)
    }

    /// Whether the host's `Guard` line needs its `LaunchTime`: it is running, and no live lease of
    /// its project explains it. `LaunchTime` is read for such a host alone (operator.md, status
    /// -All).
    pub(crate) fn needs_launch_time(&self, project: &str, state: &str) -> bool {
        state == "running" && self.live(project) == 0
    }

    /// The lines after a host's report, in their order: `Leases    <n> live`, and for a running
    /// host with none live, `Guard     unexplained; stops at <h:mm><am|pm>`.
    ///
    /// `launch_time` is the instance's `LaunchTime` in Unix seconds, which a running host with no
    /// live lease needs and no other host is given. `clock_time` gives the text of the stop time,
    /// which is `agentspace_zone`'s alone (D63): this module never converts a time.
    pub(crate) fn lines(
        &self,
        project: &str,
        state: &str,
        launch_time: Option<i64>,
        clock_time: &dyn Fn(i64) -> String,
    ) -> Vec<String> {
        let mut lines = vec![self.leases_line(project)];
        if self.needs_launch_time(project, state)
            && let Some(launch) = launch_time
        {
            lines.push(guard_line(&clock_time(self.stop_time(project, launch))));
        }
        lines
    }

    /// `Leases    <n> live`: a count, never a lease's ID or any other member of one (D60).
    pub(crate) fn leases_line(&self, project: &str) -> String {
        format!("Leases    {} live", self.live(project))
    }

    /// What prints as the guard's stop time, in Unix seconds: the later of the stop time and the
    /// report's time. The stop time is the since plus the grace (D56), the since being the later of
    /// the launch time and the project's last lease end, or the launch time alone with no end. A
    /// time already past, perhaps by days with the laptop asleep, would read as one still to come,
    /// so the guard's next run, which stops that host, is shown as the report's own time.
    fn stop_time(&self, project: &str, launch_time: i64) -> i64 {
        let since = match last_lease_end(&self.leases, project, self.now) {
            Some(end) => end.max(launch_time),
            None => launch_time,
        };
        since.saturating_add(GRACE_SECONDS).max(self.now)
    }
}

/// `Guard     unexplained; stops at <h:mm><am|pm>`, the label padded to 10 characters, over the
/// stop time's text, which `agentspace_zone::local_clock_time` makes (rounding included).
fn guard_line(stop_text: &str) -> String {
    format!("Guard     unexplained; stops at {stop_text}")
}

/// An instance's `LaunchTime` as Unix seconds, in UTC, a fraction of a second truncated, or `None`
/// when it is not of its form: `YYYY-MM-DDTHH:MM:SS[.fraction](Z|+HH:MM|-HH:MM)` (operator.md,
/// Calls by command). The separator MAY also be `t` or a space, the `Z` MAY be `z`, and the offset
/// MAY be written without its colon, as `agentspace-box`'s reading of the same member allows. A time
/// with no UTC offset is not of its form.
pub(crate) fn parse_launch_time(text: &str) -> Option<i64> {
    let b = text.as_bytes();
    if b.len() < 20 || !text.is_ascii() {
        return None;
    }
    if !matches!(b[10], b'T' | b't' | b' ') {
        return None;
    }
    // A second of 60, which `agentspace-box` reads, is the next minute's first.
    let (seconds, leap) = if &text[17..19] == "60" {
        ("59", 1)
    } else {
        (&text[17..19], 0)
    };
    let core = format!("{}T{}{seconds}Z", &text[..10], &text[11..17]);
    let base = parse_time(&core)?;
    let mut rest = &text[19..];
    if let Some(fraction) = rest.strip_prefix('.') {
        let digits = fraction.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 {
            return None;
        }
        rest = &fraction[digits..];
    }
    let offset = match rest {
        "Z" | "z" => 0,
        _ => {
            let sign = match rest.as_bytes().first()? {
                b'+' => 1,
                b'-' => -1,
                _ => return None,
            };
            let zone = &rest[1..];
            let (hours, minutes) = match zone.len() {
                5 if &zone[2..3] == ":" => (&zone[..2], &zone[3..]),
                4 => (&zone[..2], &zone[2..]),
                _ => return None,
            };
            if !hours.bytes().all(|c| c.is_ascii_digit())
                || !minutes.bytes().all(|c| c.is_ascii_digit())
            {
                return None;
            }
            let (hours, minutes): (i64, i64) = (hours.parse().ok()?, minutes.parse().ok()?);
            if hours > 23 || minutes > 59 {
                return None;
            }
            sign * (hours * 3600 + minutes * 60)
        }
    };
    Some(base + leap - offset)
}

#[cfg(test)]
mod tests {
    use agentspace_state::format_time;
    use agentspace_zone::clock_time_at;

    use super::*;

    /// An arbitrary report time: 2026-11-22T20:34:10Z.
    const NOW: i64 = 1_795_379_650;

    fn at(text: &str) -> i64 {
        parse_time(text).unwrap()
    }

    fn lease(project: &str, state: &str, expires: Option<i64>, released: Option<i64>) -> Record {
        Record {
            sized_for: None,
            plan: None,
            id: "00000000-0000-4000-8000-000000000000".into(),
            project: project.into(),
            state: state.into(),
            platform: "linux-arm64".into(),
            profile: "default".into(),
            size: "small".into(),
            ttl_seconds: 3600,
            created_at: format_time(NOW - 7200),
            expires_at: expires.map(format_time),
            released_at: released.map(format_time),
            ssh: None,
            reasons: None,
            driver: None,
        }
    }

    /// The stop time's text in Pacific standard time, which the seam gives in a test. The 12-hour
    /// form, the rounding and the zone's rules are `agentspace_zone`'s, tested there.
    fn pacific(unix: i64) -> String {
        clock_time_at(unix, -8 * 3600)
    }

    // ---- the Leases line ----

    #[test]
    fn the_leases_line_counts_live_leases_with_no_leading_zero() {
        let view = LeaseView::new(Vec::new(), NOW);
        assert_eq!(view.leases_line("swimcoach"), "Leases    0 live");
        let one = LeaseView::new(
            vec![lease("swimcoach", "ready", Some(NOW + 600), None)],
            NOW,
        );
        assert_eq!(one.leases_line("swimcoach"), "Leases    1 live");
        let two = LeaseView::new(
            vec![
                lease("swimcoach", "ready", Some(NOW + 600), None),
                lease("swimcoach", "pending", None, None),
            ],
            NOW,
        );
        assert_eq!(two.leases_line("swimcoach"), "Leases    2 live");
    }

    #[test]
    fn the_label_is_padded_to_ten_characters() {
        let view = LeaseView::new(Vec::new(), NOW);
        assert!(view.leases_line("p").starts_with("Leases    0"));
        assert_eq!(&view.leases_line("p")[..10], "Leases    ");
        let lines = view.lines("p", "running", Some(NOW), &pacific);
        assert_eq!(&lines[1][..10], "Guard     ");
    }

    #[test]
    fn a_pending_lease_counts_live_and_so_explains_a_running_host() {
        let view = LeaseView::new(vec![lease("example", "pending", None, None)], NOW);
        let lines = view.lines("example", "running", Some(NOW - 100_000), &pacific);
        assert_eq!(lines, vec!["Leases    1 live".to_string()]);
    }

    #[test]
    fn a_lease_of_another_project_is_not_counted_and_the_project_is_compared_exactly() {
        let view = LeaseView::new(
            vec![
                lease("swimcoach", "ready", Some(NOW + 600), None),
                lease("swimcoach-2", "pending", None, None),
            ],
            NOW,
        );
        assert_eq!(view.live("swimcoach"), 1);
        assert_eq!(view.live("agentspace"), 0);
        assert_eq!(view.live("swim"), 0);
    }

    #[test]
    fn an_expired_a_released_and_an_unsatisfiable_lease_are_not_counted() {
        let view = LeaseView::new(
            vec![
                lease("p", "ready", Some(NOW - 1), None),
                lease("p", "released", Some(NOW + 600), Some(NOW - 60)),
                lease("p", "unsatisfiable", None, None),
            ],
            NOW,
        );
        assert_eq!(view.leases_line("p"), "Leases    0 live");
    }

    #[test]
    fn a_lease_ending_at_the_report_time_is_not_live() {
        let view = LeaseView::new(vec![lease("p", "ready", Some(NOW), None)], NOW);
        assert_eq!(view.live("p"), 0);
    }

    // ---- the Guard line ----

    #[test]
    fn an_unexplained_running_host_gets_the_guard_line_after_the_leases_line() {
        // Launched 5 minutes before the report: it stops 10 minutes after it, at 12:44:10pm
        // Pacific, which rounds up to 12:45pm.
        let view = LeaseView::new(Vec::new(), NOW);
        let lines = view.lines("agentspace", "running", Some(NOW - 300), &pacific);
        assert_eq!(
            lines,
            vec![
                "Leases    0 live".to_string(),
                "Guard     unexplained; stops at 12:45pm".to_string(),
            ]
        );
    }

    #[test]
    fn a_host_that_is_not_running_gets_no_guard_line() {
        let view = LeaseView::new(Vec::new(), NOW);
        for state in [
            "pending",
            "stopping",
            "stopped",
            "shutting-down",
            "terminated",
        ] {
            let lines = view.lines("p", state, None, &pacific);
            assert_eq!(lines, vec!["Leases    0 live".to_string()], "{state}");
            assert!(!view.needs_launch_time("p", state), "{state}");
        }
        assert!(view.needs_launch_time("p", "running"));
    }

    #[test]
    fn a_host_with_no_instance_gets_its_count_alone() {
        // `Instance  none recorded` has no state and no launch time.
        let view = LeaseView::new(vec![lease("example", "pending", None, None)], NOW);
        assert_eq!(view.leases_line("example"), "Leases    1 live");
    }

    #[test]
    fn a_running_host_with_a_live_lease_never_needs_its_launch_time() {
        let view = LeaseView::new(vec![lease("p", "ready", Some(NOW + 60), None)], NOW);
        assert!(!view.needs_launch_time("p", "running"));
    }

    #[test]
    fn the_since_is_the_later_of_the_launch_time_and_the_last_lease_end() {
        // The report is at 20:34:10Z. A launch 100 minutes ago, a lease that ended 2 minutes ago:
        // the end is later, so the host stops 13 minutes from now, at 20:47:10Z, 12:48pm Pacific.
        let ended = lease("p", "released", Some(NOW + 3000), Some(NOW - 120));
        let view = LeaseView::new(vec![ended], NOW);
        assert_eq!(view.stop_time("p", NOW - 6000), NOW - 120 + GRACE_SECONDS);
        // A lease that ended before the launch adds nothing: the launch is later.
        let view = LeaseView::new(
            vec![lease("p", "released", Some(NOW - 7000), Some(NOW - 7000))],
            NOW,
        );
        assert_eq!(view.stop_time("p", NOW - 60), NOW - 60 + GRACE_SECONDS);
        // An expired lease ends at its `expires_at`.
        let view = LeaseView::new(vec![lease("p", "ready", Some(NOW - 30), None)], NOW);
        assert_eq!(view.stop_time("p", NOW - 5000), NOW - 30 + GRACE_SECONDS);
        // An `unsatisfiable` lease has no end, so the launch time stands alone.
        let view = LeaseView::new(vec![lease("p", "unsatisfiable", None, None)], NOW);
        assert_eq!(view.stop_time("p", NOW - 60), NOW - 60 + GRACE_SECONDS);
    }

    #[test]
    fn a_stop_time_already_past_shows_the_report_time() {
        // Launched two days ago, with the laptop asleep: it prints the report's own minute.
        let view = LeaseView::new(Vec::new(), NOW);
        assert_eq!(view.stop_time("p", NOW - 2 * 86_400), NOW);
        let lines = view.lines("p", "running", Some(NOW - 2 * 86_400), &pacific);
        // 20:34:10Z is 12:34:10pm Pacific, rounded up to 12:35pm.
        assert_eq!(lines[1], "Guard     unexplained; stops at 12:35pm");
    }

    #[test]
    fn the_printed_time_is_never_more_than_the_grace_after_the_report() {
        let view = LeaseView::new(Vec::new(), NOW);
        // A launch time no later than the report's: a future one (a skewed clock) is not shown
        // clipped, since operator.md gives no rule for it.
        for launch in [NOW, NOW - 899, NOW - 900, NOW - 901, NOW - 100_000] {
            let shown = view.stop_time("p", launch);
            assert!((NOW..=NOW + GRACE_SECONDS).contains(&shown), "{launch}");
        }
    }

    #[test]
    fn the_text_is_asked_for_the_stop_time_and_for_no_other_instant() {
        // The text is made from the stop time itself, the later of the stop time and the report's
        // time, so the one formatter rounds it and applies the zone's rule at that instant.
        let view = LeaseView::new(Vec::new(), NOW);
        let asked = std::cell::RefCell::new(Vec::new());
        let ask = |unix: i64| {
            asked.borrow_mut().push(unix);
            "text".to_string()
        };
        let lines = view.lines("p", "running", Some(NOW - 300), &ask);
        assert_eq!(lines[1], "Guard     unexplained; stops at text");
        assert_eq!(asked.into_inner(), vec![NOW - 300 + GRACE_SECONDS]);
        // No `Guard` line, so no text is asked for.
        let asked = std::cell::RefCell::new(Vec::new());
        let ask = |unix: i64| {
            asked.borrow_mut().push(unix);
            String::new()
        };
        view.lines("p", "stopped", None, &ask);
        assert!(asked.into_inner().is_empty());
    }

    #[test]
    fn the_guard_line_carries_no_lease_id() {
        let mut one = lease("p", "released", Some(NOW + 3000), Some(NOW - 120));
        one.id = "11111111-2222-4333-8444-555555555555".into();
        let view = LeaseView::new(vec![one], NOW);
        for line in view.lines("p", "running", Some(NOW - 6000), &pacific) {
            assert!(!line.contains("11111111"), "{line}");
        }
    }

    // ---- the lease state file ----

    #[test]
    fn the_report_time_is_the_views() {
        assert_eq!(LeaseView::new(Vec::new(), NOW).now(), NOW);
    }

    #[test]
    fn unix_seconds_truncates() {
        let time = UNIX_EPOCH + std::time::Duration::new(1_795_379_650, 900_000_000);
        assert_eq!(unix_seconds(time), NOW);
    }

    #[test]
    fn the_unreadable_message_names_the_full_path_and_the_reason() {
        let message = unreadable_message(Path::new("C:\\state"), "the file does not exist");
        assert_eq!(
            message,
            format!(
                "agentspace: The lease state file {} cannot be read: the file does not exist.",
                Path::new("C:\\state").join("leases.json").display()
            )
        );
    }

    #[test]
    fn a_reason_is_one_line_with_no_closing_period() {
        assert_eq!(
            fragment("the state file is not valid."),
            "the state file is not valid"
        );
        assert_eq!(fragment("two\r\nlines"), "two lines");
    }

    struct Folder(std::path::PathBuf);

    impl Folder {
        fn new(name: &str) -> Folder {
            let path = std::env::temp_dir().join(format!(
                "agentspace-operator-status-{}-{name}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            Folder(path)
        }
    }

    impl Drop for Folder {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_missing_file_is_not_no_leases() {
        let folder = Folder::new("missing");
        let error = read_leases(&folder.0, NOW).unwrap_err();
        assert_eq!(error, "the file does not exist");
        // Nor is a state folder that does not exist.
        let error = read_leases(&folder.0.join("no-such-folder"), NOW).unwrap_err();
        assert_eq!(error, "the file does not exist");
    }

    #[test]
    fn an_empty_valid_file_is_no_leases() {
        let folder = Folder::new("empty");
        std::fs::write(
            folder.0.join("leases.json"),
            "{\"version\":0,\"leases\":[]}\n",
        )
        .unwrap();
        let view = read_leases(&folder.0, NOW).unwrap();
        assert_eq!(view.leases_line("p"), "Leases    0 live");
        assert_eq!(view.now(), NOW);
    }

    #[test]
    fn a_file_that_is_not_valid_is_refused_and_left_alone() {
        let folder = Folder::new("invalid");
        let path = folder.0.join("leases.json");
        std::fs::write(&path, "not json").unwrap();
        let error = read_leases(&folder.0, NOW).unwrap_err();
        assert!(!error.is_empty() && !error.contains('\n') && !error.ends_with('.'));
        assert_eq!(std::fs::read(&path).unwrap(), b"not json");
    }

    #[test]
    fn a_folder_at_the_path_is_refused() {
        let folder = Folder::new("folder-at-path");
        std::fs::create_dir(folder.0.join("leases.json")).unwrap();
        let error = read_leases(&folder.0, NOW).unwrap_err();
        assert!(!error.is_empty() && !error.contains('\n'));
    }

    #[test]
    fn reading_takes_no_lock_and_writes_nothing() {
        let folder = Folder::new("no-lock");
        std::fs::write(
            folder.0.join("leases.json"),
            "{\"version\":0,\"leases\":[]}\n",
        )
        .unwrap();
        read_leases(&folder.0, NOW).unwrap();
        let mut names: Vec<String> = std::fs::read_dir(&folder.0)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, vec!["leases.json".to_string()]);
    }

    // ---- LaunchTime ----

    #[test]
    fn the_launch_times_the_cli_prints_parse() {
        let expected = at("2026-11-19T08:12:34Z");
        assert_eq!(
            parse_launch_time("2026-11-19T08:12:34+00:00"),
            Some(expected)
        );
        assert_eq!(parse_launch_time("2026-11-19T08:12:34Z"), Some(expected));
        assert_eq!(
            parse_launch_time("2026-11-19T08:12:34.000Z"),
            Some(expected)
        );
        assert_eq!(
            parse_launch_time("2026-11-19T08:12:34.999999+00:00"),
            Some(expected)
        );
        assert_eq!(parse_launch_time("2026-11-19t08:12:34z"), Some(expected));
        assert_eq!(parse_launch_time("2026-11-19 08:12:34Z"), Some(expected));
    }

    #[test]
    fn an_offset_is_taken_off_the_time() {
        let expected = at("2026-11-19T08:12:34Z");
        assert_eq!(
            parse_launch_time("2026-11-19T01:12:34-07:00"),
            Some(expected)
        );
        assert_eq!(
            parse_launch_time("2026-11-19T13:42:34+0530"),
            Some(expected)
        );
    }

    #[test]
    fn a_launch_time_not_of_its_form_is_none() {
        for text in [
            "",
            "2026-11-19T08:12:34",
            "2026-11-19T08:12:34.Z",
            "2026-11-19T08:12:34+0",
            "2026-11-19T08:12:34+24:00",
            "2026-11-19T08:12:34+00:60",
            "2026-13-19T08:12:34Z",
            "2026-02-30T08:12:34Z",
            "2026-11-19T24:12:34Z",
            "2026-11-19T08:12:61Z",
            "2026/11/19T08:12:34Z",
            "yesterday at noon, or so I recall",
            "2026-11-19T08:12:34Zé",
        ] {
            assert_eq!(parse_launch_time(text), None, "{text:?}");
        }
    }

    #[test]
    fn a_leap_second_is_the_next_minutes_first() {
        assert_eq!(
            parse_launch_time("2026-12-31T23:59:60Z"),
            Some(at("2027-01-01T00:00:00Z"))
        );
    }
}
