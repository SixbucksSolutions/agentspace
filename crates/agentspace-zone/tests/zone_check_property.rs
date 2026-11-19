//! The zone check of operator.md, The time zone, step 3, compared with the spec's own pattern
//! over many generated names.
//!
//! RFC 2119 keywords in this file's comments, and the capitals are load-bearing.
//!
//! Two references stand beside the crate's hand-written check, and each name MUST get the same
//! answer from all three:
//! - The spec's pattern, word for word, compiled by the `regex` crate. In it `^` and `\z` are the
//!   start and the end of the text, and `[A-Za-z0-9_+-]` is a set of ASCII characters.
//! - A small state machine written here, which shares no code with the crate.
//!
//! The names come from an exhaustive walk of every short string over an alphabet of the
//! characters that sit on the pattern's edges, and from a seeded generator of longer ones.

use agentspace_zone::{check_zone, is_valid_zone, zone};
use regex::Regex;

/// The pattern of step 3, exactly as docs/operator.md gives it.
const SPEC_PATTERN: &str = r"^[A-Za-z0-9_+][A-Za-z0-9_+-]*(/[A-Za-z0-9_+-]+)*\z";

/// The reference state machine. It reads characters, not bytes, so a character outside ASCII is
/// one step that no state accepts.
fn reference(name: &str) -> bool {
    #[derive(Clone, Copy)]
    enum State {
        /// Nothing read yet: the next character MUST be one that may open the name.
        Start,
        /// Inside a segment with at least one character: the name may end here.
        InSegment,
        /// Just after a `/`: the segment that follows MUST have at least one character.
        AfterSlash,
    }
    let opens = |c: char| matches!(c, 'A'..='Z' | 'a'..='z' | '0'..='9' | '_' | '+');
    let continues = |c: char| opens(c) || c == '-';
    let mut state = State::Start;
    for c in name.chars() {
        state = match state {
            State::Start if opens(c) => State::InSegment,
            State::InSegment if continues(c) => State::InSegment,
            State::InSegment if c == '/' => State::AfterSlash,
            State::AfterSlash if continues(c) => State::InSegment,
            _ => return false,
        };
    }
    matches!(state, State::InSegment)
}

/// Checks one name against both references, and gives a message when they disagree.
fn disagreement(spec: &Regex, name: &str) -> Option<String> {
    let crate_says = is_valid_zone(name);
    let regex_says = spec.is_match(name);
    let reference_says = reference(name);
    let checked = check_zone(name) == Some(name);
    let through_zone = zone(None, Some(name)).is_some();
    if crate_says == regex_says
        && crate_says == reference_says
        && checked == crate_says
        // `zone` maps a legacy name first, and every legacy name and its current one pass, so
        // for any non-empty name `zone` passes exactly when the check does. An empty override
        // means the laptop's zone, which is `None` here.
        && (name.is_empty() || through_zone == crate_says)
    {
        None
    } else {
        Some(format!(
            "{name:?}: crate {crate_says}, spec pattern {regex_says}, reference {reference_says}, \
             check_zone {checked}, zone {through_zone}"
        ))
    }
}

/// The characters on the pattern's edges: each class's first and last members, the hyphen and
/// the slash, a dot, a space, a newline and a carriage return, a backslash, and characters
/// outside ASCII, one of which lowercases to an ASCII letter.
const EDGE: &[char] = &[
    'A', 'Z', 'a', 'z', '0', '9', '_', '+', '-', '/', '.', ' ', '\n', '\r', '\\', '@', '[', '`',
    '{', ':', '\u{0}', '\u{7f}', '\u{e9}', '\u{212a}', '\u{ff21}',
];

/// A small alphabet for the exhaustive walk: one character of each kind the pattern tells
/// apart, so every arrangement up to the walk's length is tried.
const SMALL: &[char] = &['a', '9', '_', '+', '-', '/', '\n', '.', ' ', '\u{e9}'];

/// Every string over `SMALL` up to six characters long is checked: 1,111,111 names.
#[test]
fn every_short_name_agrees_with_the_spec_pattern() {
    let spec = Regex::new(SPEC_PATTERN).expect("the spec's pattern compiles");
    let mut failures = Vec::new();
    let mut checked = 0usize;
    let mut name = String::new();
    // Each length's strings are counted in base `SMALL.len()`.
    'lengths: for len in 0..=6u32 {
        let count = SMALL.len().pow(len);
        for mut n in 0..count {
            name.clear();
            for _ in 0..len {
                name.push(SMALL[n % SMALL.len()]);
                n /= SMALL.len();
            }
            checked += 1;
            if let Some(message) = disagreement(&spec, &name) {
                failures.push(message);
                if failures.len() >= 20 {
                    break 'lengths;
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} disagreements:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert_eq!(checked, 1_111_111);
}

/// A seeded xorshift generator, so a failure repeats on every run and every platform.
struct XorShift(u64);

impl XorShift {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// Longer names, built mostly from characters the pattern allows, so that many of them pass
/// and the rest fail on one late character.
#[test]
fn many_generated_names_agree_with_the_spec_pattern() {
    let spec = Regex::new(SPEC_PATTERN).expect("the spec's pattern compiles");
    let allowed: Vec<char> = ('A'..='Z')
        .chain('a'..='z')
        .chain('0'..='9')
        .chain(['_', '+', '-', '/'])
        .collect();
    let mut rng = XorShift(0x9e37_79b9_7f4a_7c15);
    let mut failures = Vec::new();
    let (mut passed, mut failed) = (0usize, 0usize);
    for _ in 0..200_000 {
        let len = rng.below(40);
        let mut name = String::with_capacity(len);
        for _ in 0..len {
            // One character in sixteen is from the edge set; the rest are allowed ones.
            let c = if rng.below(16) == 0 {
                EDGE[rng.below(EDGE.len())]
            } else {
                allowed[rng.below(allowed.len())]
            };
            name.push(c);
        }
        if is_valid_zone(&name) {
            passed += 1;
        } else {
            failed += 1;
        }
        if let Some(message) = disagreement(&spec, &name) {
            failures.push(message);
            if failures.len() >= 20 {
                break;
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} disagreements:\n{}",
        failures.len(),
        failures.join("\n")
    );
    // The generator MUST reach both answers often, or the comparison proves little.
    assert!(
        passed > 10_000 && failed > 10_000,
        "passed {passed}, failed {failed}"
    );
}

/// The references themselves are checked on names whose answer the spec states, so that a
/// reference that agreed with a wrong check by being wrong the same way would be caught.
#[test]
fn the_references_give_the_specs_answers() {
    let spec = Regex::new(SPEC_PATTERN).expect("the spec's pattern compiles");
    for (name, expected) in [
        ("America/Los_Angeles", true),
        ("Etc/GMT+12", true),
        ("Etc/-x", true),
        ("a", true),
        ("", false),
        ("-h", false),
        ("UTC\n", false),
        ("UTC/", false),
        ("a//b", false),
        ("/UTC", false),
        ("Zürich", false),
        ("not a zone", false),
    ] {
        assert_eq!(spec.is_match(name), expected, "spec pattern on {name:?}");
        assert_eq!(reference(name), expected, "reference on {name:?}");
    }
}
