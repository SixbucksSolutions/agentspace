//! The plan document, its peak, its size and its TTL (docs/lease-api.md, Plan, Turn and TTL; D73
//! to D76, D81, D84, D85).
//!
//! [`parse`] checks a plan document's bytes against every rule lease-api.md gives a plan, in its
//! table's order, and counts each turn's agents at once. [`Plan::turn_agents`] and
//! [`Plan::budget_minutes`] cut one turn from the plan (D84). [`size_for`] picks the smallest size
//! that holds a number of agents at agentspace's per-agent footprint, and [`ttl_seconds_for`] the
//! TTL a turn's budget gives at agentspace's TTL factor; both figures live here alone (D73, D85).
//!
//! Portable, with no Windows code. The crate reads no file, writes nothing to stdout or stderr, and
//! never panics on its input. A [`PlanError`]'s message is the `<why>` alone; the lease CLI frames
//! it as `Plan '<value>' is not a valid plan: <why>.` (D81, as changed for v0.10-T01).

mod document;
mod peak;
mod size;

pub use document::{MAX_FILE_BYTES, WHY_TOO_LARGE, WHY_UNREADABLE};
pub use size::{
    PER_AGENT_MIB, PER_AGENT_MILLIVCPUS, TTL_FACTOR_PERCENT, most_agents, size_for, ttl_seconds_for,
};

use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fmt;

/// Why a plan is not well formed: one `<why>` of lease-api.md's table (Plan), word for word, for
/// the first check the plan fails. It carries no operating system's error text and no path, and
/// quotes no model, effort or other value of the document but a name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanError {
    pub message: String,
}

impl fmt::Display for PlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for PlanError {}

/// A well-formed plan: its document as parsed, and what sizing reads from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    document: Value,
    name: Option<String>,
    busiest_turn: String,
    busiest_agents: u32,
    turns: Vec<TurnSummary>,
}

/// What a lease for one turn reads from it: its name as written, its agents at once, and its
/// budget, if it has one.
#[derive(Debug, Clone, PartialEq, Eq)]
struct TurnSummary {
    name: String,
    agents: u32,
    budget_minutes: Option<u32>,
}

impl Plan {
    /// The plan's `name`, as written, or `None` when it is absent or `null`.
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// The busiest turn's name, as written, and its agents at once: the sum of its stories' peaks
    /// plus 1 for its dev lead. The first such turn in the plan's order on a tie (D74).
    pub fn busiest(&self) -> (&str, u32) {
        (&self.busiest_turn, self.busiest_agents)
    }

    /// The named turn's agents at once, as if the plan held that turn and no other: the sum of its
    /// stories' peaks plus 1 for its dev lead (D74, D84). `None` when no turn of the plan has that
    /// name, compared exactly, character by character, never trimmed or case-folded.
    pub fn turn_agents(&self, name: &str) -> Option<u32> {
        self.turn(name).map(|t| t.agents)
    }

    /// The named turn's `budget_minutes`, 1 to 960 (D85). `None` when no turn of the plan has that
    /// name, compared exactly, and when that turn has no budget, absent or `null`.
    pub fn budget_minutes(&self, turn: &str) -> Option<u32> {
        self.turn(turn).and_then(|t| t.budget_minutes)
    }

    /// The document as parsed, members in the order the plan writes them, for the lease's record.
    pub fn document(&self) -> &Value {
        &self.document
    }

    fn turn(&self, name: &str) -> Option<&TurnSummary> {
        self.turns.iter().find(|t| t.name == name)
    }
}

/// Checks a plan document's bytes, the file's whole content, and counts its busiest turn.
///
/// The checks are lease-api.md's table in its order (Plan), and only the first that fails is
/// reported, with its first fault in document order: the size, UTF-8, JSON (a leading UTF-8 byte
/// order mark set aside), a member named twice, the plan schema (with a turn's `budget_minutes`
/// written as digits alone, which the schema cannot see; pin P24), then two turns of one name, two
/// stories of one turn of one name, two subagents of one story of one name, an `after` entry that
/// names no subagent of its story, and a cycle in a story's starts. Reading the file, check 2, is
/// the caller's.
pub fn parse(bytes: &[u8]) -> Result<Plan, PlanError> {
    let document = document::read(bytes).map_err(error)?;
    let turns = shape(&document).ok_or_else(|| error(document::WHY_NOT_SCHEMA.to_string()))?;
    check_names(&turns).map_err(error)?;
    let agents = turn_agents(&turns).map_err(error)?;
    let (busiest_turn, busiest_agents) =
        busiest(&turns, &agents).ok_or_else(|| error(document::WHY_NOT_SCHEMA.to_string()))?;
    let summaries = turns
        .iter()
        .zip(&agents)
        .map(|(turn, agents)| TurnSummary {
            name: turn.name.to_string(),
            agents: *agents,
            budget_minutes: turn.budget_minutes,
        })
        .collect();
    let name = document
        .get("name")
        .and_then(Value::as_str)
        .map(str::to_string);
    Ok(Plan {
        busiest_turn: busiest_turn.to_string(),
        busiest_agents,
        turns: summaries,
        name,
        document,
    })
}

fn error(message: String) -> PlanError {
    PlanError { message }
}

/// A subagent as sizing and the name checks read it.
struct Subagent<'a> {
    name: &'a str,
    after: Vec<&'a str>,
}

struct Story<'a> {
    name: &'a str,
    subagents: Vec<Subagent<'a>>,
}

struct Turn<'a> {
    name: &'a str,
    stories: Vec<Story<'a>>,
    budget_minutes: Option<u32>,
}

/// The least and the most a turn's `budget_minutes` can be (docs/lease-api.md, Plan; D85).
const BUDGET_MINUTES: std::ops::RangeInclusive<u32> = 1..=960;

/// The turns of a document the schema accepted, or `None` if it is not of the schema's shape. The
/// schema has passed it, so `None` comes only from what the schema cannot see: a `budget_minutes`
/// that is a whole number not written as digits alone, such as `90.0`, `9e1` or `-0` (pin P24).
fn shape(document: &Value) -> Option<Vec<Turn<'_>>> {
    document
        .get("turns")?
        .as_array()?
        .iter()
        .map(|turn| {
            Some(Turn {
                budget_minutes: budget(turn.get("budget_minutes"))?,
                name: turn.get("name")?.as_str()?,
                stories: turn
                    .get("stories")?
                    .as_array()?
                    .iter()
                    .map(|story| {
                        Some(Story {
                            name: story.get("name")?.as_str()?,
                            subagents: story
                                .get("subagents")?
                                .as_array()?
                                .iter()
                                .map(subagent)
                                .collect::<Option<_>>()?,
                        })
                    })
                    .collect::<Option<_>>()?,
            })
        })
        .collect()
}

/// A turn's `budget_minutes` member: `Some(None)` when it is absent or `null`, `Some(Some(n))` for
/// a JSON integer written as digits alone from 1 to 960, and `None`, a schema fault, for anything
/// else. serde_json holds a number written with a fraction or an exponent, or as `-0`, as a float,
/// which `as_u64` never reads, so `90.0`, `9e1` and `-0` are refused here though they are whole.
fn budget(value: Option<&Value>) -> Option<Option<u32>> {
    match value {
        None | Some(Value::Null) => Some(None),
        Some(Value::Number(number)) => {
            let minutes = u32::try_from(number.as_u64()?).ok()?;
            BUDGET_MINUTES.contains(&minutes).then_some(Some(minutes))
        }
        Some(_) => None,
    }
}

fn subagent(value: &Value) -> Option<Subagent<'_>> {
    let after = match value.get("after") {
        None | Some(Value::Null) => Vec::new(),
        Some(list) => list
            .as_array()?
            .iter()
            .map(Value::as_str)
            .collect::<Option<_>>()?,
    };
    Some(Subagent {
        name: value.get("name")?.as_str()?,
        after,
    })
}

/// The first name, in document order, that repeats an earlier sibling's, among the given names.
fn first_repeat<'a>(names: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    let mut seen = HashSet::new();
    names.into_iter().find(|name| !seen.insert(*name))
}

/// Checks 7 to 10, each over the whole plan before the next.
fn check_names(turns: &[Turn<'_>]) -> Result<(), String> {
    // 7. Turns.
    if let Some(turn) = first_repeat(turns.iter().map(|t| t.name)) {
        return Err(format!("turn '{turn}' is named twice"));
    }
    // 8. Stories.
    for turn in turns {
        if let Some(story) = first_repeat(turn.stories.iter().map(|s| s.name)) {
            return Err(format!(
                "story '{story}' is named twice in turn '{}'",
                turn.name
            ));
        }
    }
    // 9. Subagents.
    for story in turns.iter().flat_map(|t| &t.stories) {
        if let Some(name) = first_repeat(story.subagents.iter().map(|s| s.name)) {
            return Err(format!(
                "subagent '{name}' is named twice in story '{}'",
                story.name
            ));
        }
    }
    // 10. After.
    for story in turns.iter().flat_map(|t| &t.stories) {
        let names: HashSet<&str> = story.subagents.iter().map(|s| s.name).collect();
        for subagent in &story.subagents {
            if let Some(after) = subagent.after.iter().find(|a| !names.contains(*a)) {
                return Err(format!(
                    "subagent '{}' in story '{}' starts after '{after}', which is no subagent of that story",
                    subagent.name, story.name
                ));
            }
        }
    }
    Ok(())
}

/// Check 11, a cycle, for each story in document order, then each turn's agents at once, in the
/// plan's order: the sum of its stories' peaks plus 1 for its dev lead. Names are known to be
/// unique among siblings and every `after` entry to name a subagent.
fn turn_agents(turns: &[Turn<'_>]) -> Result<Vec<u32>, String> {
    let mut peaks: Vec<Vec<u32>> = Vec::with_capacity(turns.len());
    for turn in turns {
        let mut turn_peaks = Vec::with_capacity(turn.stories.len());
        for story in &turn.stories {
            let index: HashMap<&str, usize> = story
                .subagents
                .iter()
                .enumerate()
                .map(|(i, s)| (s.name, i))
                .collect();
            let after: Vec<Vec<usize>> = story
                .subagents
                .iter()
                .map(|s| {
                    s.after
                        .iter()
                        .filter_map(|a| index.get(a).copied())
                        .collect()
                })
                .collect();
            let Some(story_peak) = peak::peak(&after) else {
                return Err(format!(
                    "the subagents of story '{}' start after each other in a cycle",
                    story.name
                ));
            };
            turn_peaks.push(u32::try_from(story_peak).unwrap_or(u32::MAX));
        }
        peaks.push(turn_peaks);
    }
    Ok(peaks
        .iter()
        .map(|turn_peaks| {
            turn_peaks
                .iter()
                .fold(1u32, |sum, peak| sum.saturating_add(*peak))
        })
        .collect())
}

/// The busiest turn and its agents at once: the turn with the most, the first in the plan's order
/// on a tie (D74). `None` only for a plan of no turns, which the schema refuses.
fn busiest<'a>(turns: &[Turn<'a>], agents: &[u32]) -> Option<(&'a str, u32)> {
    let mut best: Option<(&'a str, u32)> = None;
    for (turn, agents) in turns.iter().zip(agents) {
        if best.is_none_or(|(_, most)| *agents > most) {
            best = Some((turn.name, *agents));
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    fn why(text: &str) -> String {
        parse(text.as_bytes()).unwrap_err().message
    }

    fn plan(text: &str) -> Plan {
        parse(text.as_bytes()).unwrap()
    }

    const P0203: &str = r#"{"name":"v0.09","turns":[{"name":"T03","stories":[{"name":"E0.09-T03-S01","subagents":[
  {"name":"implementer","model":"Opus","effort":"high"},
  {"name":"test author","model":"Sonnet","effort":"high"},
  {"name":"runner","model":"Sonnet","effort":"medium"},
  {"name":"reviewer","model":"Opus","effort":"high","after":["implementer","test author","runner"]}]}]}]}"#;

    #[test]
    fn the_0203_plan_is_t03_with_4_agents_and_size_s() {
        let p = plan(P0203);
        assert_eq!(p.name(), Some("v0.09"));
        assert_eq!(p.busiest(), ("T03", 4));
        assert_eq!(size_for(p.busiest().1), Some("s"));
        let names: Vec<&String> = p.document().as_object().unwrap().keys().collect();
        assert_eq!(names, ["name", "turns"]);
    }

    #[test]
    fn a_null_or_absent_name_is_none() {
        let one = r#"[{"name":"T","stories":[{"name":"S","subagents":[{"name":"a","model":"m","effort":"e"}]}]}]"#;
        assert_eq!(plan(&format!(r#"{{"turns":{one}}}"#)).name(), None);
        assert_eq!(
            plan(&format!(r#"{{"name":null,"turns":{one}}}"#)).name(),
            None
        );
    }

    #[test]
    fn the_busiest_turn_is_the_first_on_a_tie() {
        let text = r#"{"turns":[
          {"name":"A","stories":[{"name":"S","subagents":[{"name":"a","model":"m","effort":"e"}]}]},
          {"name":"B","stories":[{"name":"S","subagents":[{"name":"a","model":"m","effort":"e"},{"name":"b","model":"m","effort":"e"}]}]},
          {"name":"C","stories":[{"name":"S","subagents":[{"name":"a","model":"m","effort":"e"}]},{"name":"S2","subagents":[{"name":"a","model":"m","effort":"e"}]}]}]}"#;
        assert_eq!(plan(text).busiest(), ("B", 3));
    }

    #[test]
    fn a_turns_agents_are_its_stories_peaks_plus_its_dev_lead() {
        let text = r#"{"turns":[{"name":"T","stories":[
          {"name":"S1","subagents":[{"name":"a","model":"m","effort":"e"},{"name":"b","model":"m","effort":"e","after":["a"]}]},
          {"name":"S2","subagents":[{"name":"a","model":"m","effort":"e"},{"name":"b","model":"m","effort":"e","after":null},{"name":"c","model":"m","effort":"e","after":[]}]}]}]}"#;
        assert_eq!(plan(text).busiest(), ("T", 1 + 1 + 3));
    }

    #[test]
    fn the_name_checks_come_in_order_and_quote_raw_names() {
        let sub = r#"{"name":"a","model":"m","effort":"e"}"#;
        // 7 before 8: both faults present.
        let text = format!(
            r#"{{"turns":[{{"name":"T","stories":[{{"name":"S","subagents":[{sub}]}},{{"name":"S","subagents":[{sub}]}}]}},{{"name":"T","stories":[{{"name":"X","subagents":[{sub}]}}]}}]}}"#
        );
        assert_eq!(why(&text), "turn 'T' is named twice");
        let text = format!(
            r#"{{"turns":[{{"name":"T 1","stories":[{{"name":"S","subagents":[{sub},{sub}]}}]}},{{"name":"U","stories":[{{"name":"S ","subagents":[{sub}]}},{{"name":"S","subagents":[{sub}]}}]}}]}}"#
        );
        // The schema refuses a trailing space, so that plan is a schema failure.
        assert_eq!(why(&text), document::WHY_NOT_SCHEMA);
        let text = format!(
            r#"{{"turns":[{{"name":"T 1","stories":[{{"name":"S","subagents":[{sub},{sub}]}}]}},{{"name":"U","stories":[{{"name":"S'","subagents":[{sub}]}},{{"name":"S'","subagents":[{sub}]}}]}}]}}"#
        );
        assert_eq!(why(&text), "story 'S'' is named twice in turn 'U'");
        let text = format!(
            r#"{{"turns":[{{"name":"T 1","stories":[{{"name":"S","subagents":[{sub},{sub}]}}]}}]}}"#
        );
        assert_eq!(why(&text), "subagent 'a' is named twice in story 'S'");
    }

    #[test]
    fn an_after_naming_no_subagent_is_found_before_a_cycle() {
        let text = r#"{"turns":[{"name":"T","stories":[
          {"name":"C","subagents":[{"name":"a","model":"m","effort":"e","after":["b"]},{"name":"b","model":"m","effort":"e","after":["a"]}]},
          {"name":"S","subagents":[{"name":"a","model":"m","effort":"e","after":["a","x","y"]}]}]}]}"#;
        assert_eq!(
            why(text),
            "subagent 'a' in story 'S' starts after 'x', which is no subagent of that story"
        );
        // A story of another turn has a subagent of that name; it still names none of its own.
        let text = r#"{"turns":[{"name":"T","stories":[{"name":"S","subagents":[{"name":"a","model":"m","effort":"e"}]}]},
          {"name":"U","stories":[{"name":"S","subagents":[{"name":"b","model":"m","effort":"e","after":["a"]}]}]}]}"#;
        assert_eq!(
            why(text),
            "subagent 'b' in story 'S' starts after 'a', which is no subagent of that story"
        );
    }

    #[test]
    fn the_first_story_with_a_cycle_is_named() {
        let text = r#"{"turns":[{"name":"T","stories":[
          {"name":"Fine","subagents":[{"name":"a","model":"m","effort":"e"}]},
          {"name":"Self","subagents":[{"name":"a","model":"m","effort":"e","after":["a"]}]},
          {"name":"Pair","subagents":[{"name":"a","model":"m","effort":"e","after":["b"]},{"name":"b","model":"m","effort":"e","after":["a"]}]}]}]}"#;
        assert_eq!(
            why(text),
            "the subagents of story 'Self' start after each other in a cycle"
        );
    }

    #[test]
    fn model_and_effort_are_in_no_message() {
        let text = r#"{"turns":[{"name":"T","stories":[{"name":"S","subagents":[{"name":"a","model":"SecretModel","effort":"secreteffort","after":["zz"]}]}]}]}"#;
        let message = why(text);
        assert!(!message.contains("SecretModel"));
        assert!(!message.contains("secreteffort"));
    }

    #[test]
    fn the_largest_plan_counts_and_finds_no_size() {
        // 32 turns of 32 stories of 32 subagents is past the file's bound, so the largest turn is
        // built alone: 32 stories of 32 subagents with no starts, 1025 agents at once.
        let subs: Vec<String> = (0..32)
            .map(|i| format!(r#"{{"name":"a{i}","model":"m","effort":"e"}}"#))
            .collect();
        let stories: Vec<String> = (0..32)
            .map(|i| format!(r#"{{"name":"s{i}","subagents":[{}]}}"#, subs.join(",")))
            .collect();
        let text = format!(
            r#"{{"turns":[{{"name":"T","stories":[{}]}}]}}"#,
            stories.join(",")
        );
        assert!(text.len() <= MAX_FILE_BYTES);
        let p = plan(&text);
        assert_eq!(p.busiest(), ("T", 1025));
        assert_eq!(size_for(p.busiest().1), None);
    }

    /// A one-turn plan whose turn `T` has `budget_minutes` written as `raw`, exactly.
    fn with_budget(raw: &str) -> String {
        format!(
            r#"{{"turns":[{{"name":"T","budget_minutes":{raw},"stories":[{{"name":"S","subagents":[{{"name":"a","model":"m","effort":"e"}}]}}]}}]}}"#
        )
    }

    /// Four turns: A of 2 + 1 agents with a budget of 30, B of 3 + 1 with 960, C of 1 + 1 with a
    /// null budget, D of 1 + 1 with none.
    const BUDGETED: &str = r#"{"turns":[
      {"name":"A","budget_minutes":30,"stories":[{"name":"S","subagents":[{"name":"a","model":"m","effort":"e"},{"name":"b","model":"m","effort":"e"}]}]},
      {"name":"B","stories":[{"name":"S","subagents":[{"name":"a","model":"m","effort":"e"},{"name":"b","model":"m","effort":"e"},{"name":"c","model":"m","effort":"e"}]}],"budget_minutes":960},
      {"name":"C","budget_minutes":null,"stories":[{"name":"S","subagents":[{"name":"a","model":"m","effort":"e"}]}]},
      {"name":"D","stories":[{"name":"S","subagents":[{"name":"a","model":"m","effort":"e"}]}]}]}"#;

    #[test]
    fn each_turns_budget_is_read_and_a_null_or_absent_one_is_none() {
        let p = plan(BUDGETED);
        assert_eq!(p.budget_minutes("A"), Some(30));
        assert_eq!(p.budget_minutes("B"), Some(960));
        assert_eq!(p.budget_minutes("C"), None);
        assert_eq!(p.budget_minutes("D"), None);
        assert_eq!(p.budget_minutes("Z"), None);
    }

    #[test]
    fn a_budget_from_1_to_960_is_accepted() {
        for minutes in [1u32, 2, 90, 959, 960] {
            assert_eq!(
                plan(&with_budget(&minutes.to_string())).budget_minutes("T"),
                Some(minutes)
            );
        }
    }

    #[test]
    fn a_budget_out_of_range_or_of_another_type_is_a_schema_fault() {
        for raw in [
            "0",
            "961",
            "-1",
            "90.5",
            r#""90""#,
            "true",
            "[]",
            "{}",
            "4294967297",
        ] {
            assert_eq!(why(&with_budget(raw)), document::WHY_NOT_SCHEMA, "{raw}");
        }
    }

    #[test]
    fn a_whole_budget_not_written_as_digits_alone_is_a_schema_fault() {
        // Pin P24: the schema accepts 90.0 and 9e1 (document.rs shows it), and the program refuses.
        for raw in [
            "90.0", "9e1", "9E1", "90e0", "9e+1", "1.0", "960.0", "1e2", "-0", "-0.0", "-0e0",
        ] {
            assert_eq!(why(&with_budget(raw)), document::WHY_NOT_SCHEMA, "{raw}");
        }
    }

    #[test]
    fn the_budget_reader_holds_the_range_and_the_form_itself() {
        // The schema refuses 0 and 961 before shape() reads them; the reader refuses them too, so
        // the bound does not rest on the schema alone.
        let read = |raw: &str| budget(Some(&serde_json::from_str::<Value>(raw).unwrap()));
        assert_eq!(budget(None), Some(None));
        assert_eq!(read("null"), Some(None));
        assert_eq!(read("1"), Some(Some(1)));
        assert_eq!(read("960"), Some(Some(960)));
        for raw in [
            "0",
            "961",
            "-1",
            "4294967297",
            "90.0",
            "9e1",
            "-0",
            "\"90\"",
            "true",
        ] {
            assert_eq!(read(raw), None, "{raw}");
        }
    }

    #[test]
    fn a_budget_fault_comes_before_the_names_checks() {
        // Check 6 before check 7: two turns named T, one with a budget written 90.0.
        let sub = r#"{"name":"a","model":"m","effort":"e"}"#;
        let text = format!(
            r#"{{"turns":[{{"name":"T","budget_minutes":90.0,"stories":[{{"name":"S","subagents":[{sub}]}}]}},{{"name":"T","stories":[{{"name":"S","subagents":[{sub}]}}]}}]}}"#
        );
        assert_eq!(why(&text), document::WHY_NOT_SCHEMA);
    }

    #[test]
    fn turn_agents_is_each_turns_own_and_none_for_no_turn() {
        let p = plan(BUDGETED);
        assert_eq!(p.turn_agents("A"), Some(3));
        assert_eq!(p.turn_agents("B"), Some(4));
        assert_eq!(p.turn_agents("C"), Some(2));
        assert_eq!(p.turn_agents("D"), Some(2));
        assert_eq!(p.busiest(), ("B", 4));
        for name in ["a", "A ", " A", "", "Z", "S"] {
            assert_eq!(p.turn_agents(name), None, "{name:?}");
            assert_eq!(p.budget_minutes(name), None, "{name:?}");
        }
    }

    #[test]
    fn a_budget_changes_no_turns_agents() {
        let without = plan(
            &BUDGETED
                .replace(r#""budget_minutes":30,"#, "")
                .replace(r#","budget_minutes":960"#, ""),
        );
        let with = plan(BUDGETED);
        assert_eq!(without.budget_minutes("A"), None);
        assert_eq!(without.budget_minutes("B"), None);
        for name in ["A", "B", "C", "D"] {
            assert_eq!(with.turn_agents(name), without.turn_agents(name), "{name}");
        }
        assert_eq!(with.busiest(), without.busiest());
    }

    #[test]
    fn a_v0_10_plan_with_no_budget_parses_as_before() {
        let p = plan(P0203);
        assert_eq!(p.busiest(), ("T03", 4));
        assert_eq!(p.turn_agents("T03"), Some(4));
        assert_eq!(p.budget_minutes("T03"), None);
        assert!(!p.document().to_string().contains("budget_minutes"));
    }

    #[test]
    fn the_error_displays_its_message() {
        let e = parse(b"").unwrap_err();
        assert_eq!(e.to_string(), "the file is not valid JSON");
    }
}
