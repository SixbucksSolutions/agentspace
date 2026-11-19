//! `Say` for a person's command and for caller mode (docs/operator.md, Messages and What a caller
//! sees; the pin on card #0111): progress, warnings and detail, and `-Quiet`.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.

mod common;

use common::*;

const STARTING: &str = "Starting the host...\n";
const WAITING: &str = "Waiting for the host to be ready...\n";
const READY: &str = "The host is ready.\n";

#[test]
fn the_mode_is_the_one_asked_for() {
    assert_eq!(mode(&person(false).0), Mode::Person);
    assert_eq!(mode(&caller(false, false).0), Mode::Caller);
    assert_eq!(mode(&person(true).0), Mode::Person);
    assert_eq!(mode(&caller(true, true).0), Mode::Caller);
}

#[test]
fn a_person_sees_the_persons_line_each_time() {
    let (say, cap) = person(false);
    progress(&say, "Starting i-0123...", Some(CallerLine::Starting));
    progress(&say, "Starting i-0123...", Some(CallerLine::Starting));
    progress(&say, "Waiting for Session Manager...", None);
    assert_eq!(
        cap.text(),
        "Starting i-0123...\nStarting i-0123...\nWaiting for Session Manager...\n"
    );
}

#[test]
fn a_person_never_sees_a_callers_line() {
    let (say, cap) = person(false);
    progress(&say, "Ready: i-0123", Some(CallerLine::Ready));
    assert_eq!(cap.text(), "Ready: i-0123\n");
}

#[test]
fn a_person_sees_a_warning_as_given() {
    let (say, cap) = person(false);
    warning(
        &say,
        "agentspace: The instance did not confirm the token; the instance is unchanged.",
    );
    assert_eq!(
        cap.text(),
        "agentspace: The instance did not confirm the token; the instance is unchanged.\n"
    );
}

#[test]
fn a_person_never_sees_detail() {
    for diagnostic in [false, true] {
        let (say, cap) = printer(Mode::Person, false, diagnostic);
        detail(&say, "backend text");
        assert_eq!(cap.text(), "", "diagnostic {diagnostic}");
    }
}

#[test]
fn a_caller_sees_each_progress_line_once_in_order() {
    let (say, cap) = caller(false, false);
    progress(&say, "Starting i-0123...", Some(CallerLine::Starting));
    progress(&say, "Starting i-0123...", Some(CallerLine::Starting));
    progress(
        &say,
        "Waiting for Session Manager...",
        Some(CallerLine::Waiting),
    );
    progress(&say, "Patching Ubuntu...", Some(CallerLine::Waiting));
    progress(&say, "Ready: i-0123", Some(CallerLine::Ready));
    progress(&say, "Ready: i-0123", Some(CallerLine::Ready));
    assert_eq!(cap.text(), format!("{STARTING}{WAITING}{READY}"));
}

#[test]
fn each_line_is_once_per_run_not_once_per_kind_of_call() {
    // A line printed once stays printed once however the other lines are interleaved.
    let (say, cap) = caller(false, false);
    progress(&say, "a", Some(CallerLine::Starting));
    progress(&say, "b", None);
    progress(&say, "c", Some(CallerLine::Starting));
    progress(&say, "d", Some(CallerLine::Waiting));
    progress(&say, "e", Some(CallerLine::Starting));
    assert_eq!(cap.text(), format!("{STARTING}{WAITING}"));
}

#[test]
fn a_caller_sees_nothing_for_none() {
    let (say, cap) = caller(false, false);
    progress(&say, "Waiting for i-0123 to finish stopping...", None);
    progress(
        &say,
        "Sending the localswim Bot token to the instance...",
        None,
    );
    assert_eq!(cap.text(), "");
}

#[test]
fn a_caller_never_sees_the_persons_line() {
    let (say, cap) = caller(false, true);
    progress(&say, "Ready: i-0123", Some(CallerLine::Ready));
    assert!(!cap.text().contains("i-0123"), "{:?}", cap.text());
    assert_eq!(cap.text(), READY);
}

#[test]
fn a_caller_sees_a_warning_with_the_host_for_the_instance() {
    let (say, cap) = caller(false, false);
    warning(
        &say,
        "agentspace: The instance did not confirm the token; the instance is unchanged.",
    );
    assert_eq!(
        cap.text(),
        "agentspace: The host did not confirm the token; the host is unchanged.\n"
    );
}

#[test]
fn a_warning_with_no_instance_in_it_is_the_same_for_a_caller() {
    let (say, cap) = caller(false, false);
    warning(&say, "agentspace: The board will not work on the box.");
    assert_eq!(
        cap.text(),
        "agentspace: The board will not work on the box.\n"
    );
}

#[test]
fn a_caller_sees_detail_only_with_diagnostic_on() {
    let (say, cap) = caller(false, false);
    detail(&say, "the call failed: backend text");
    assert_eq!(cap.text(), "");
    let (say, cap) = caller(false, true);
    detail(&say, "the call failed: backend text");
    assert_eq!(cap.text(), "the call failed: backend text\n");
}

#[test]
fn quiet_suppresses_progress_and_warnings_in_both_modes() {
    for mode in [Mode::Person, Mode::Caller] {
        let (say, cap) = printer(mode, true, true);
        progress(&say, "Starting i-0123...", Some(CallerLine::Starting));
        progress(&say, "Ready: i-0123", Some(CallerLine::Ready));
        progress(&say, "Waiting...", None);
        warning(&say, "agentspace: The instance said no.");
        assert_eq!(cap.text(), "", "{mode:?}");
    }
}

#[test]
fn quiet_does_not_suppress_detail() {
    // The architect's ruling, 2026-11-19, in Messages.
    let (say, cap) = caller(true, true);
    detail(&say, "backend text");
    assert_eq!(cap.text(), "backend text\n");
    let (say, cap) = caller(true, false);
    detail(&say, "backend text");
    assert_eq!(cap.text(), "");
}

#[test]
fn a_failure_prints_whatever_quiet_says() {
    for mode in [Mode::Person, Mode::Caller] {
        let (say, cap) = printer(mode, true, false);
        failure(&say, "agentspace: Usage is wrong.", CallerFailure::Same);
        assert_eq!(cap.text(), "agentspace: Usage is wrong.\n", "{mode:?}");
    }
}

#[test]
fn a_caller_sees_the_callers_failure_line() {
    let cases = [
        (CallerFailure::Same, "agentspace: Person's text.\n"),
        (
            CallerFailure::NotAvailable,
            "agentspace: The host is not available, and could not be started.\n",
        ),
        (
            CallerFailure::NotReady,
            "agentspace: The host was not ready within 15 minutes; try again later.\n",
        ),
        (
            CallerFailure::CouldNotReach,
            "agentspace: Could not reach the host. Set AGENTSPACE_DIAGNOSTIC=1 and connect again to see why.\n",
        ),
    ];
    for (kind, expected) in cases {
        let (say, cap) = caller(false, false);
        failure(&say, "agentspace: Person's text.", kind);
        assert_eq!(cap.text(), expected, "{kind:?}");
        // A person sees the person's message whatever the caller's line is.
        let (say, cap) = person(false);
        failure(&say, "agentspace: Person's text.", kind);
        assert_eq!(cap.text(), "agentspace: Person's text.\n", "{kind:?}");
    }
}
