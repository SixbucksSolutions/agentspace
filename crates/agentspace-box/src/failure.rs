//! The one line a failure prints, in a person's mode or the caller's.

use agentspace_operator_api::{CallerFailure, Failure, Mode};

/// The line a [`Failure`] prints in `mode` (operator.md, What a caller sees, the caller's failure
/// messages): the person's `message` in [`Mode::Person`], and in [`Mode::Caller`] the caller's
/// line its `caller` picks, `CallerFailure::Same` printing `message`.
pub fn failure_line(failure: &Failure, mode: Mode) -> &str {
    match mode {
        Mode::Person => &failure.message,
        Mode::Caller => match failure.caller {
            CallerFailure::Same => &failure.message,
            CallerFailure::NotAvailable => {
                "agentspace: The host is not available, and could not be started."
            }
            CallerFailure::NotReady => {
                "agentspace: The host was not ready within 15 minutes; try again later."
            }
            CallerFailure::CouldNotReach => {
                "agentspace: Could not reach the host. Set AGENTSPACE_DIAGNOSTIC=1 and connect again to see why."
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failure(caller: CallerFailure) -> Failure {
        Failure {
            message: "agentspace: Instance i-0123456789abcdef0 is terminated.".to_string(),
            caller,
        }
    }

    #[test]
    fn a_person_sees_the_message() {
        for caller in [
            CallerFailure::Same,
            CallerFailure::NotAvailable,
            CallerFailure::NotReady,
            CallerFailure::CouldNotReach,
        ] {
            assert_eq!(
                failure_line(&failure(caller), Mode::Person),
                "agentspace: Instance i-0123456789abcdef0 is terminated."
            );
        }
    }

    #[test]
    fn a_caller_sees_the_callers_line() {
        assert_eq!(
            failure_line(&failure(CallerFailure::Same), Mode::Caller),
            "agentspace: Instance i-0123456789abcdef0 is terminated."
        );
        assert_eq!(
            failure_line(&failure(CallerFailure::NotAvailable), Mode::Caller),
            "agentspace: The host is not available, and could not be started."
        );
        assert_eq!(
            failure_line(&failure(CallerFailure::NotReady), Mode::Caller),
            "agentspace: The host was not ready within 15 minutes; try again later."
        );
        assert_eq!(
            failure_line(&failure(CallerFailure::CouldNotReach), Mode::Caller),
            "agentspace: Could not reach the host. Set AGENTSPACE_DIAGNOSTIC=1 and connect again to see why."
        );
    }
}
