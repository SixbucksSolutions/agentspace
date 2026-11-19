//! Reading members of an AWS CLI answer (operator.md, Calls by command, What is read from each
//! answer, and gap FB-08's table).

use agentspace_operator_api::{CallerFailure, Failure};
use serde_json::Value;

/// A member of an answer: its JSON pointer, and its path as Calls by command writes it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Member {
    pub(crate) pointer: &'static str,
    pub(crate) path: &'static str,
}

/// `ec2 describe-instances`'s state, read by the first call.
pub(crate) const STATE_NAME: Member = Member {
    pointer: "/Reservations/0/Instances/0/State/Name",
    path: "Reservations[0].Instances[0].State.Name",
};

/// `ec2 describe-instances`'s launch time, read by the second call (FB-09).
pub(crate) const LAUNCH_TIME: Member = Member {
    pointer: "/Reservations/0/Instances/0/LaunchTime",
    path: "Reservations[0].Instances[0].LaunchTime",
};

/// The JSON pointer of `ssm describe-instance-information`'s list. An empty list is not online. It
/// is no member of its own in Calls by command's table, so an answer without the list is one with
/// no [`PING_STATUS`], the one path the table writes for this call.
pub(crate) const INSTANCE_INFORMATION_LIST: &str = "/InstanceInformationList";

/// The ping status of the list's first element.
pub(crate) const PING_STATUS: Member = Member {
    pointer: "/InstanceInformationList/0/PingStatus",
    path: "InstanceInformationList[0].PingStatus",
};

/// `ssm send-command`'s command ID.
pub(crate) const COMMAND_ID: Member = Member {
    pointer: "/Command/CommandId",
    path: "Command.CommandId",
};

/// A string member, or the "Unexpected answer" failure when it is missing or not a string.
pub(crate) fn string<'v>(
    answer: &'v Value,
    call: &str,
    member: Member,
) -> Result<&'v str, Failure> {
    answer
        .pointer(member.pointer)
        .and_then(Value::as_str)
        .ok_or_else(|| unexpected(call, member))
}

/// `agentspace: Unexpected answer from <call>: no <member>.`, which a caller sees as "Could not
/// reach the host" (operator.md, Calls by command, FB-08; What a caller sees, "any other
/// failure").
pub(crate) fn unexpected(call: &str, member: Member) -> Failure {
    Failure {
        message: format!(
            "agentspace: Unexpected answer from {call}: no {}.",
            member.path
        ),
        caller: CallerFailure::CouldNotReach,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_string_member_is_read() {
        let answer = json!({"Reservations":[{"Instances":[{"State":{"Name":"running"}}]}]});
        assert_eq!(
            string(&answer, "ec2 describe-instances", STATE_NAME).unwrap(),
            "running"
        );
    }

    #[test]
    fn a_missing_or_other_member_is_an_unexpected_answer() {
        for answer in [
            json!({}),
            json!({"Reservations":[]}),
            json!({"Reservations":[{"Instances":[]}]}),
            json!({"Reservations":[{"Instances":[{"State":{}}]}]}),
            json!({"Reservations":[{"Instances":[{"State":{"Name":16}}]}]}),
            json!(null),
            json!("text"),
        ] {
            let failure = string(&answer, "ec2 describe-instances", STATE_NAME).unwrap_err();
            assert_eq!(
                failure.message,
                "agentspace: Unexpected answer from ec2 describe-instances: no Reservations[0].Instances[0].State.Name."
            );
            assert_eq!(failure.caller, CallerFailure::CouldNotReach);
        }
    }
}
