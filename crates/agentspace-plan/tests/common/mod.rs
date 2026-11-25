//! Shared builders for the plan crate's integration tests.
//!
//! RFC 2119 keywords in this crate's tests are the specs' own (docs/lease-api.md, Plan). Every
//! builder here makes a document as `plan.input.schema.json` shapes it; a test that wants a
//! refused document edits the value it gets back.

#![allow(dead_code)]

use agentspace_plan::Plan;
use serde_json::{Value, json};

/// The `<why>` of check 6.
pub const SCHEMA: &str = "the document does not match the plan schema";
/// The `<why>` of check 4.
pub const NOT_JSON: &str = "the file is not valid JSON";
/// The `<why>` of check 3.
pub const NOT_UTF8: &str = "the file is not valid UTF-8";
/// The `<why>` of check 1.
pub const TOO_LARGE: &str = "the file is larger than 262144 bytes";

/// Parse bytes that must be a plan.
pub fn parse_ok(bytes: &[u8]) -> Plan {
    match agentspace_plan::parse(bytes) {
        Ok(plan) => plan,
        Err(refusal) => panic!("expected a plan, got the refusal: {}", refusal.message),
    }
}

/// Parse bytes that must be refused, and return the `<why>`.
pub fn parse_err(bytes: &[u8]) -> String {
    match agentspace_plan::parse(bytes) {
        Ok(_) => panic!("expected a refusal, but the plan was accepted"),
        Err(refusal) => refusal.message,
    }
}

/// Parse a document that must be a plan.
pub fn ok(document: &Value) -> Plan {
    parse_ok(document.to_string().as_bytes())
}

/// Parse a document that must be refused, and return the `<why>`.
pub fn err(document: &Value) -> String {
    parse_err(document.to_string().as_bytes())
}

/// A subagent that starts after none.
pub fn sub(name: &str) -> Value {
    json!({"name": name, "model": "Opus", "effort": "high"})
}

/// A subagent that starts after the named ones.
pub fn sub_after(name: &str, after: &[&str]) -> Value {
    json!({"name": name, "model": "Opus", "effort": "high", "after": after})
}

pub fn story(name: &str, subagents: Vec<Value>) -> Value {
    json!({"name": name, "subagents": subagents})
}

pub fn turn(name: &str, stories: Vec<Value>) -> Value {
    json!({"name": name, "stories": stories})
}

pub fn plan(turns: Vec<Value>) -> Value {
    json!({"turns": turns})
}

pub fn named_plan(name: &str, turns: Vec<Value>) -> Value {
    json!({"name": name, "turns": turns})
}

/// A plan of one turn, `T1`, and one story, `S1`, with these subagents.
pub fn one_story_plan(subagents: Vec<Value>) -> Value {
    plan(vec![turn("T1", vec![story("S1", subagents)])])
}

/// `n` subagents, `a0` to `a{n-1}`, none after another.
pub fn antichain(n: usize) -> Vec<Value> {
    (0..n).map(|i| sub(&format!("a{i}"))).collect()
}

/// `n` subagents, each after the one before it.
pub fn chain(n: usize) -> Vec<Value> {
    (0..n)
        .map(|i| {
            if i == 0 {
                sub("a0")
            } else {
                sub_after(&format!("a{i}"), &[&format!("a{}", i - 1)])
            }
        })
        .collect()
}

/// A story's peak, read as the plan reports it: its turn's agents less the dev lead.
pub fn peak_of(subagents: Vec<Value>) -> u32 {
    let plan = ok(&one_story_plan(subagents));
    let (_, agents) = plan.busiest();
    agents - 1
}

/// A turn whose agents at once, with its dev lead, are `agents` (at least 2, at most 1025),
/// made of stories of at most 32 independent subagents.
pub fn turn_with_agents(name: &str, agents: u32) -> Value {
    assert!((2..=1025).contains(&agents));
    let mut left = agents - 1;
    let mut stories = Vec::new();
    let mut i = 0;
    while left > 0 {
        let k = left.min(32);
        stories.push(story(&format!("S{i}"), antichain(k as usize)));
        left -= k;
        i += 1;
    }
    turn(name, stories)
}

/// A seeded generator (splitmix64); the tests use no other source of randomness.
pub struct Rng(pub u64);

impl Rng {
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }

    pub fn chance(&mut self, numerator: u64, denominator: u64) -> bool {
        self.next_u64() % denominator < numerator
    }

    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.below(i + 1);
            items.swap(i, j);
        }
    }
}

/// Subagents `s0` to `s{n-1}` for a graph in which an edge `(a, b)` means `b` starts after `a`.
/// They are written in `document_order`, so an `after` MAY name a subagent written later.
pub fn graph_subagents(n: usize, edges: &[(usize, usize)], document_order: &[usize]) -> Vec<Value> {
    assert_eq!(document_order.len(), n);
    document_order
        .iter()
        .map(|&j| {
            let after: Vec<String> = edges
                .iter()
                .filter(|(_, b)| *b == j)
                .map(|(a, _)| format!("s{a}"))
                .collect();
            let after: Vec<&str> = after.iter().map(String::as_str).collect();
            if after.is_empty() {
                sub(&format!("s{j}"))
            } else {
                sub_after(&format!("s{j}"), &after)
            }
        })
        .collect()
}

/// The size of the largest set of nodes in which none is reachable from another, found by
/// trying every subset of the transitive closure. `n` MUST be at most 16.
pub fn oracle_width(n: usize, edges: &[(usize, usize)]) -> u32 {
    assert!(n <= 16);
    let mut reach = vec![vec![false; n]; n];
    for &(a, b) in edges {
        reach[a][b] = true;
    }
    for k in 0..n {
        for i in 0..n {
            for j in 0..n {
                if reach[i][k] && reach[k][j] {
                    reach[i][j] = true;
                }
            }
        }
    }
    // comparable[i]: the nodes that i reaches or that reach i, as a bit set.
    let comparable: Vec<u32> = (0..n)
        .map(|i| {
            (0..n)
                .filter(|&j| i != j && (reach[i][j] || reach[j][i]))
                .fold(0u32, |set, j| set | (1 << j))
        })
        .collect();
    let mut best = 0;
    for mask in 1u32..(1u32 << n) {
        let independent = (0..n).all(|i| mask & (1 << i) == 0 || comparable[i] & mask == 0);
        if independent {
            best = best.max(mask.count_ones());
        }
    }
    best
}
