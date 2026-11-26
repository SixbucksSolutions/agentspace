//! The plan block (docs/lease-api.md, "The plan block"), through `plan_block`'s public API alone.
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Every expectation here is the spec's:
//! the fence lines, the near misses, the order the lines are read in, and each fault's `<why>`
//! word for word. A `<why>` has no final period (the message adds it).

use agentspace_board::plan_block;

const OPEN: &str = "```agentspace-plan";
const CLOSE: &str = "```";

const MORE_THAN_ONE: &str = "its description has more than one agentspace-plan block";
const NEAR_MISS: &str = "a line that starts an agentspace-plan block is not exactly the fence line";
const NEVER_CLOSED: &str = "its agentspace-plan block is never closed";
const NONE: &str = "its description has no agentspace-plan block";

/// The content, or the `<why>`.
fn block(description: &str) -> Result<String, String> {
    match plan_block(description) {
        Ok(content) => Ok(content.to_string()),
        Err(e) => Err(e.message),
    }
}

#[track_caller]
fn content_is(description: &str, expected: &str) {
    match block(description) {
        Ok(content) => assert_eq!(content, expected, "the content of {description:?}"),
        Err(why) => panic!("expected a block in {description:?}, got a refusal: {why}"),
    }
}

#[track_caller]
fn refused_with(description: &str, why: &str) {
    match block(description) {
        Ok(content) => panic!("expected a refusal for {description:?}, got the content {content:?}"),
        Err(got) => assert_eq!(got, why, "the refusal of {description:?}"),
    }
}

// ---------------------------------------------------------------------------------------------
// A block, between prose.

#[test]
fn a_block_between_prose_gives_its_content() {
    let d = format!("The epic's plan by turn.\n\n{OPEN}\n{{\"a\":1}}\n{CLOSE}\n\nMore prose, after it.\n");
    content_is(&d, "{\"a\":1}\n");
}

#[test]
fn a_block_alone_gives_its_content() {
    content_is(&format!("{OPEN}\n{{\"a\":1}}\n{CLOSE}\n"), "{\"a\":1}\n");
}

#[test]
fn the_last_line_of_the_content_keeps_its_line_end() {
    // The content runs to the character before the closing line's first backtick, so the line
    // end of its last line is content.
    content_is(&format!("{OPEN}\nx\n{CLOSE}"), "x\n");
    content_is(&format!("{OPEN}\nx\ny\n{CLOSE}\n"), "x\ny\n");
}

#[test]
fn a_closing_line_straight_after_the_opening_gives_empty_content() {
    content_is(&format!("{OPEN}\n{CLOSE}"), "");
    content_is(&format!("{OPEN}\n{CLOSE}\n"), "");
    content_is(&format!("{OPEN}\r\n{CLOSE}\r\n"), "");
}

#[test]
fn lines_may_end_in_lf_or_crlf_mixed_in_one_description() {
    // The carriage return of the opening line's end is no part of the content, and the content
    // keeps its own line ends as they are.
    content_is(&format!("{OPEN}\r\n{{\r\n\"a\":1\n}}\r\n{CLOSE}\r\n"), "{\r\n\"a\":1\n}\r\n");
    content_is(&format!("prose\r\n{OPEN}\n{{}}\r\n{CLOSE}\n"), "{}\r\n");
    // A closing line that ends in CRLF.
    content_is(&format!("{OPEN}\nx\n{CLOSE}\r\nafter\r\n"), "x\n");
}

#[test]
fn a_carriage_return_inside_a_line_is_text() {
    content_is(&format!("{OPEN}\na\rb\nc\r\r\n{CLOSE}"), "a\rb\nc\r\r\n");
}

#[test]
fn only_line_feed_ends_a_line() {
    // U+000B, U+000C, U+0085, U+2028 and U+2029 end no line, so each is content, and a fence
    // line behind one is no fence line.
    for c in ['\u{000B}', '\u{000C}', '\u{0085}', '\u{2028}', '\u{2029}'] {
        content_is(&format!("{OPEN}\na{c}b\n{CLOSE}"), &format!("a{c}b\n"));
        refused_with(&format!("x{c}{OPEN}\n{CLOSE}"), NONE);
        refused_with(&format!("{OPEN}\n{c}{CLOSE}\n"), NEVER_CLOSED);
    }
}

#[test]
fn the_content_is_a_slice_of_the_description() {
    let d = format!("before\n{OPEN}\nplan\n{CLOSE}\nafter\n");
    let content = plan_block(&d).unwrap_or_else(|e| panic!("a block was expected: {}", e.message));
    let start = d.as_ptr() as usize;
    let at = content.as_ptr() as usize;
    assert!(at >= start && at + content.len() <= start + d.len());
    assert_eq!(content, "plan\n");
}

#[test]
fn other_fences_outside_the_block_are_text() {
    // No Markdown is parsed: a ```json fence, a bare ``` line and a four-backtick line outside a
    // block are text, and never read as inside another fence or as a closing line.
    let d = format!(
        "```json\n{{\"x\":1}}\n```\n\n````\ntext\n````\n{CLOSE}\n{OPEN}\nplan\n{CLOSE}\n```rust\nfn main() {{}}\n```\n"
    );
    content_is(&d, "plan\n");
}

#[test]
fn a_closing_line_is_exactly_three_backticks() {
    // Inside the block, ```` and ``` followed by a space are content, not closing lines.
    let d = format!("{OPEN}\nx\n````\n{CLOSE} \n{CLOSE}\n");
    content_is(&d, "x\n````\n``` \n");
    // And a closing line with a carriage return that is not before the line feed is not one.
    refused_with(&format!("{OPEN}\nx\n{CLOSE}\r"), NEVER_CLOSED);
    refused_with(&format!("{OPEN}\nx\n{CLOSE}\r\r\n"), NEVER_CLOSED);
}

#[test]
fn the_first_closing_line_ends_the_block() {
    let d = format!("{OPEN}\nx\n{CLOSE}\ny\n{CLOSE}\n");
    content_is(&d, "x\n");
}

#[test]
fn a_line_inside_a_block_is_never_trimmed_or_folded() {
    content_is(&format!("{OPEN}\n  indented\n\ttabbed\n\u{FEFF}bom\n{CLOSE}\n"), "  indented\n\ttabbed\n\u{FEFF}bom\n");
}

// ---------------------------------------------------------------------------------------------
// What is text, and so carries no block.

#[test]
fn no_description_and_no_block_are_refused() {
    refused_with("", NONE);
    refused_with("Just prose.\nNo plan here.\n", NONE);
    refused_with("```json\n{}\n```\n", NONE);
}

#[test]
fn a_quoted_block_is_text() {
    refused_with(&format!("> {OPEN}\n> {{}}\n> {CLOSE}\n"), NONE);
}

#[test]
fn an_indented_block_is_text() {
    refused_with(&format!(" {OPEN}\n{{}}\n {CLOSE}\n"), NONE);
    refused_with(&format!("\t{OPEN}\n{{}}\n\t{CLOSE}\n"), NONE);
    refused_with(&format!("  {OPEN}\n{{}}\n{CLOSE}\n"), NONE);
}

#[test]
fn a_fence_starting_with_anything_but_a_backtick_is_text() {
    refused_with(&format!("\u{FEFF}{OPEN}\n{{}}\n{CLOSE}\n"), NONE);
    refused_with("~~~agentspace-plan\n{}\n~~~\n", NONE);
    refused_with(&format!("- {OPEN}\n"), NONE);
}

#[test]
fn another_case_is_text() {
    refused_with("```Agentspace-Plan\n{}\n```\n", NONE);
    refused_with("```AGENTSPACE-PLAN\n{}\n```\n", NONE);
}

#[test]
fn backticks_not_followed_at_once_by_the_name_are_text() {
    // A near miss starts with three or more backticks followed AT ONCE by agentspace-plan.
    refused_with("``` agentspace-plan\n{}\n```\n", NONE);
    refused_with("```json agentspace-plan\n{}\n```\n", NONE);
    refused_with("``agentspace-plan\n{}\n```\n", NONE);
}

#[test]
fn a_block_inside_a_quote_does_not_count_as_a_second_block() {
    let d = format!("> {OPEN}\n> x\n> {CLOSE}\n\n{OPEN}\nplan\n{CLOSE}\n");
    content_is(&d, "plan\n");
}

// ---------------------------------------------------------------------------------------------
// The near misses, each refused.

#[test]
fn each_near_miss_outside_a_block_is_refused() {
    for line in [
        "```agentspace-plan ",
        "```agentspace-plan\t",
        "```agentspace-plan  ",
        "````agentspace-plan",
        "`````agentspace-plan",
        "```agentspace-plan json",
        "```agentspace-planner",
        "```agentspace-plan-v2",
        // Not "```agentspace-plan\r": the "\n" these cases add would make it the exact
        // opening line with a CRLF end. Two carriage returns before the line feed leave one as
        // text.
        "```agentspace-plan\r\r",
    ] {
        // On its own, before a block that would be fine, after one, and in the middle of prose.
        refused_with(&format!("{line}\n{{}}\n{CLOSE}\n"), NEAR_MISS);
        refused_with(&format!("{line}\n"), NEAR_MISS);
        refused_with(&format!("{line}"), NEAR_MISS);
        refused_with(&format!("prose\n{line}\nmore\n{OPEN}\nplan\n{CLOSE}\n"), NEAR_MISS);
        refused_with(&format!("{OPEN}\nplan\n{CLOSE}\n{line}\n"), NEAR_MISS);
    }
}

#[test]
fn a_carriage_return_not_just_before_the_line_feed_makes_a_near_miss() {
    // CR CR LF: the second carriage return is part of the line end, the first is text.
    refused_with(&format!("{OPEN}\r\r\nx\n{CLOSE}\n"), NEAR_MISS);
    // A lone carriage return at the end of the description.
    refused_with(&format!("{OPEN}\r"), NEAR_MISS);
}

#[test]
fn a_near_miss_outside_a_block_after_a_block_is_the_near_miss_not_a_second_block() {
    refused_with(&format!("{OPEN}\nx\n{CLOSE}\n```agentspace-plan json\n"), NEAR_MISS);
}

// ---------------------------------------------------------------------------------------------
// More than one block.

#[test]
fn two_blocks_are_refused() {
    refused_with(&format!("{OPEN}\na\n{CLOSE}\n{OPEN}\nb\n{CLOSE}\n"), MORE_THAN_ONE);
    refused_with(&format!("{OPEN}\na\n{CLOSE}\ntext\n{OPEN}\n"), MORE_THAN_ONE);
}

#[test]
fn an_example_block_inside_another_fence_is_an_opening_line() {
    // No Markdown is parsed, so an example plan block shown in a ```markdown fence, and the
    // description's own, are two blocks.
    let d = format!("```markdown\n{OPEN}\n{{}}\n{CLOSE}\n```\n\n{OPEN}\nplan\n{CLOSE}\n");
    refused_with(&d, MORE_THAN_ONE);
}

#[test]
fn a_line_fault_wins_over_the_content_of_an_earlier_block() {
    // The whole description is read before the content is looked at: a first block whose content
    // is no plan, and then a second block, is refused as two blocks.
    let d = format!("{OPEN}\nthis is not a plan\n{CLOSE}\n{OPEN}\n{{}}\n{CLOSE}\n");
    refused_with(&d, MORE_THAN_ONE);
}

// ---------------------------------------------------------------------------------------------
// A block never closed.

#[test]
fn a_block_that_runs_to_the_end_is_never_closed() {
    refused_with(&format!("{OPEN}\n{{}}\n"), NEVER_CLOSED);
    refused_with(&format!("{OPEN}"), NEVER_CLOSED);
    refused_with(&format!("{OPEN}\n"), NEVER_CLOSED);
    refused_with(&format!("prose\n{OPEN}\nx\n````\n``` \n"), NEVER_CLOSED);
}

#[test]
fn an_opening_line_inside_a_block_means_the_first_was_never_closed() {
    refused_with(&format!("{OPEN}\n{{\n{OPEN}\n}}\n{CLOSE}\n"), NEVER_CLOSED);
}

#[test]
fn a_near_miss_inside_a_block_means_it_was_never_closed() {
    for line in [
        "```agentspace-plan ",
        "````agentspace-plan",
        "```agentspace-plan json",
        "```agentspace-planner",
    ] {
        refused_with(&format!("{OPEN}\n{{\n{line}\n}}\n{CLOSE}\n"), NEVER_CLOSED);
    }
}

#[test]
fn a_fault_found_first_stops_the_reading() {
    // The first fault stops the reading: a near miss outside a block, before a block never
    // closed, is the near miss.
    refused_with(&format!("```agentspace-plan x\n{OPEN}\n"), NEAR_MISS);
    // A block never closed, before anything else that could fault.
    refused_with(&format!("{OPEN}\n{OPEN}\n"), NEVER_CLOSED);
}

// ---------------------------------------------------------------------------------------------
// The refusals' wording.

#[test]
fn a_why_has_no_final_period_and_quotes_nothing_of_the_description() {
    let secret = "SECRET-PROSE-LINE-1f3a";
    let descriptions = [
        format!("{secret}\n{OPEN}\n{secret}\n"),
        format!("{secret}\n{OPEN}\na\n{CLOSE}\n{OPEN}\n{secret}\n{CLOSE}\n"),
        format!("{secret}\n```agentspace-plan {secret}\n"),
        format!("{secret}\n"),
    ];
    for d in &descriptions {
        let why = block(d).expect_err("a refusal");
        assert!(!why.contains(secret), "a why quoted the description");
        assert!(!why.ends_with('.'), "a why ends with a period");
        assert!(!why.contains('\n'));
        assert!([MORE_THAN_ONE, NEAR_MISS, NEVER_CLOSED, NONE].contains(&why.as_str()));
    }
}
