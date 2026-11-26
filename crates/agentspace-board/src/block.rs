//! The plan block: finding the one fenced `agentspace-plan` block in an epic's description
//! (docs/lease-api.md, The plan block; D83, D89).
//!
//! The description is read as lines and nothing else: a line ends at a line feed, a carriage
//! return just before that line feed belongs to the line end, and no other character ends a line.
//! No Markdown is parsed. The function is pure and portable, never panics, and writes nothing.
//! The content it returns is checked by the caller, as a `--plan` file's bytes are.

use std::fmt;

/// The fence line that opens a block.
const OPENING: &str = "```agentspace-plan";
/// The fence line that closes a block.
const CLOSING: &str = "```";
/// What a near miss starts with, after its three or more backticks.
const NAME: &str = "agentspace-plan";

/// The why of a description with more than one block.
const MORE_THAN_ONE: &str = "its description has more than one agentspace-plan block";
/// The why of a near miss outside a block.
const NEAR_MISS: &str = "a line that starts an agentspace-plan block is not exactly the fence line";
/// The why of a block that is never closed.
const NEVER_CLOSED: &str = "its agentspace-plan block is never closed";
/// The why of a description with no block.
const NO_BLOCK: &str = "its description has no agentspace-plan block";

/// Why a description holds no usable block: lease-api.md's `<why>`, word for word, with no final
/// period and no framing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockError {
    /// The `<why>` of The plan block's table.
    pub message: String,
}

impl BlockError {
    fn new(why: &str) -> BlockError {
        BlockError {
            message: why.to_string(),
        }
    }
}

impl fmt::Display for BlockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for BlockError {}

/// What one line is, by its text alone.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Line {
    /// Exactly the opening line.
    Opening,
    /// Exactly the closing line.
    Closing,
    /// Not the opening line, and starts with three or more backticks and then `agentspace-plan`.
    NearMiss,
    /// Text for people, or content.
    Other,
}

fn classify(text: &str) -> Line {
    if text == OPENING {
        return Line::Opening;
    }
    if text == CLOSING {
        return Line::Closing;
    }
    let backticks = text.bytes().take_while(|b| *b == b'`').count();
    if backticks >= 3 && text[backticks..].starts_with(NAME) {
        return Line::NearMiss;
    }
    Line::Other
}

/// A line's text: the segment less its line feed, and less one carriage return only when a line
/// feed was there. Any other carriage return is text, and nothing is trimmed.
fn line_text(segment: &str) -> &str {
    match segment.strip_suffix('\n') {
        Some(rest) => rest.strip_suffix('\r').unwrap_or(rest),
        None => segment,
    }
}

/// The content of the description's one `agentspace-plan` block: every character from the one
/// after the opening line's line feed to the one before the closing line's first backtick, as a
/// slice of `description`. A closing line straight after the opening line gives `""`.
///
/// The lines are read in order and the first fault stops the reading; the reading runs to the end
/// of the description even after a block closes, so a later line fault is the refusal. The
/// content's own checks (UTF-8, size, JSON, the plan's shape) are not made here.
pub fn plan_block(description: &str) -> Result<&str, BlockError> {
    // The content of the block found, once it has closed.
    let mut found: Option<&str> = None;
    // Where the content of a block still open starts.
    let mut open_from: Option<usize> = None;
    // Where the line being read starts.
    let mut start = 0usize;

    for segment in description.split_inclusive('\n') {
        let line_start = start;
        start += segment.len();
        let kind = classify(line_text(segment));
        match (open_from, kind) {
            (None, Line::Opening) => {
                if found.is_some() {
                    return Err(BlockError::new(MORE_THAN_ONE));
                }
                open_from = Some(start);
            }
            (None, Line::NearMiss) => return Err(BlockError::new(NEAR_MISS)),
            (None, _) => {}
            (Some(from), Line::Closing) => {
                found = Some(&description[from..line_start]);
                open_from = None;
            }
            (Some(_), Line::Opening | Line::NearMiss) => {
                return Err(BlockError::new(NEVER_CLOSED));
            }
            (Some(_), Line::Other) => {}
        }
    }

    if open_from.is_some() {
        return Err(BlockError::new(NEVER_CLOSED));
    }
    found.ok_or_else(|| BlockError::new(NO_BLOCK))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MORE: &str = "its description has more than one agentspace-plan block";
    const MISS: &str = "a line that starts an agentspace-plan block is not exactly the fence line";
    const OPEN: &str = "its agentspace-plan block is never closed";
    const NONE: &str = "its description has no agentspace-plan block";

    fn why(description: &str) -> String {
        match plan_block(description) {
            Ok(content) => panic!("expected a refusal, got content of {} bytes", content.len()),
            Err(e) => e.message,
        }
    }

    fn content(description: &str) -> &str {
        match plan_block(description) {
            Ok(content) => content,
            Err(e) => panic!("expected a block, got: {}", e.message),
        }
    }

    #[test]
    fn empty_description_has_no_block() {
        assert_eq!(why(""), NONE);
    }

    #[test]
    fn lone_carriage_return_has_no_block() {
        assert_eq!(why("\r"), NONE);
        assert_eq!(why("\r\n"), NONE);
    }

    #[test]
    fn prose_alone_has_no_block() {
        assert_eq!(why("The epic's plan by turn.\n\nT1: one story.\n"), NONE);
    }

    #[test]
    fn other_fences_are_text_and_not_a_block() {
        assert_eq!(why("```json\n{}\n```\n"), NONE);
        assert_eq!(why("```\n{}\n```\n"), NONE);
    }

    #[test]
    fn closing_straight_after_opening_gives_empty_content() {
        assert_eq!(content("```agentspace-plan\n```\n"), "");
        assert_eq!(content("```agentspace-plan\n```"), "");
        assert_eq!(content("```agentspace-plan\r\n```\r\n"), "");
    }

    #[test]
    fn block_between_prose_gives_the_lines_between_the_fences() {
        let d = "Intro.\n\n```agentspace-plan\n{\"a\": 1}\n```\n\nOutro.\n";
        assert_eq!(content(d), "{\"a\": 1}\n");
    }

    #[test]
    fn content_keeps_the_line_end_of_its_last_line() {
        assert_eq!(content("```agentspace-plan\n{}\n```\n"), "{}\n");
        assert_eq!(content("```agentspace-plan\r\n{}\r\n```\r\n"), "{}\r\n");
        assert_eq!(content("```agentspace-plan\n\n\n```\n"), "\n\n");
    }

    #[test]
    fn crlf_lines_are_read() {
        let d = "Intro.\r\n```agentspace-plan\r\n{\r\n  \"a\": 1\r\n}\r\n```\r\nOutro.\r\n";
        assert_eq!(content(d), "{\r\n  \"a\": 1\r\n}\r\n");
    }

    #[test]
    fn mixed_line_ends_are_read() {
        let d = "Intro.\n```agentspace-plan\r\n{\n\"a\": 1\r\n}\n```\r\nOutro.";
        assert_eq!(content(d), "{\n\"a\": 1\r\n}\n");
    }

    #[test]
    fn carriage_return_inside_content_is_text() {
        let d = "```agentspace-plan\n{\r\r\n\"a\":\r1}\n```\n";
        assert_eq!(content(d), "{\r\r\n\"a\":\r1}\n");
    }

    #[test]
    fn closing_line_with_a_trailing_space_does_not_close() {
        assert_eq!(why("```agentspace-plan\n{}\n``` \n"), OPEN);
    }

    #[test]
    fn closing_line_with_a_trailing_tab_does_not_close() {
        assert_eq!(why("```agentspace-plan\n{}\n```\t\n"), OPEN);
    }

    #[test]
    fn closing_line_with_four_backticks_does_not_close() {
        assert_eq!(why("```agentspace-plan\n{}\n````\n"), OPEN);
    }

    #[test]
    fn closing_line_with_a_lone_carriage_return_at_the_end_does_not_close() {
        assert_eq!(why("```agentspace-plan\n{}\n```\r"), OPEN);
        assert_eq!(why("```agentspace-plan\n{}\n```\r\r\n"), OPEN);
    }

    #[test]
    fn closing_line_at_the_end_without_a_line_feed_closes() {
        assert_eq!(content("```agentspace-plan\n{}\n```"), "{}\n");
    }

    #[test]
    fn first_closing_line_ends_the_block() {
        let d = "```agentspace-plan\n{}\n```\n{not content}\n```\nmore\n";
        assert_eq!(content(d), "{}\n");
    }

    #[test]
    fn unclosed_block_is_refused() {
        assert_eq!(why("```agentspace-plan\n{}\n"), OPEN);
        assert_eq!(why("```agentspace-plan\n"), OPEN);
        assert_eq!(why("```agentspace-plan"), OPEN);
        assert_eq!(why("```agentspace-plan\r"), MISS);
    }

    #[test]
    fn two_blocks_are_refused() {
        let d = "```agentspace-plan\n{}\n```\ntext\n```agentspace-plan\n{}\n```\n";
        assert_eq!(why(d), MORE);
    }

    #[test]
    fn second_opening_after_a_block_is_refused_even_when_unclosed() {
        assert_eq!(
            why("```agentspace-plan\n{}\n```\n```agentspace-plan\n"),
            MORE
        );
    }

    #[test]
    fn near_misses_outside_a_block_are_refused() {
        let lines = [
            "```agentspace-plan ",
            "```agentspace-plan\t",
            "```agentspace-plan\r\r",
            "````agentspace-plan",
            "``````agentspace-plan",
            "```agentspace-plan json",
            "```agentspace-planner",
            "```agentspace-plan{",
        ];
        for line in lines {
            let d = format!("Intro.\n{line}\n```\n");
            assert_eq!(why(&d), MISS, "line {line:?}");
        }
    }

    #[test]
    fn carriage_return_not_before_a_line_feed_makes_a_near_miss() {
        assert_eq!(why("```agentspace-plan\rx\n```\n"), MISS);
    }

    #[test]
    fn carriage_return_at_the_end_of_the_description_makes_a_near_miss() {
        assert_eq!(why("Intro.\n```agentspace-plan\r"), MISS);
    }

    #[test]
    fn near_misses_inside_a_block_are_refused_as_never_closed() {
        let lines = [
            "```agentspace-plan ",
            "```agentspace-plan\t",
            "````agentspace-plan",
            "```agentspace-plan json",
            "```agentspace-planner",
            "```agentspace-plan\rx",
        ];
        for line in lines {
            let d = format!("```agentspace-plan\n{{}}\n{line}\n```\n");
            assert_eq!(why(&d), OPEN, "line {line:?}");
        }
    }

    #[test]
    fn opening_line_inside_a_block_is_refused_as_never_closed() {
        assert_eq!(
            why("```agentspace-plan\n{}\n```agentspace-plan\n{}\n```\n"),
            OPEN
        );
    }

    #[test]
    fn near_misses_after_a_block_are_refused_as_near_misses() {
        assert_eq!(
            why("```agentspace-plan\n{}\n```\n```agentspace-plan \n"),
            MISS
        );
        assert_eq!(
            why("```agentspace-plan\n{}\n```\n````agentspace-plan\n"),
            MISS
        );
    }

    #[test]
    fn line_fault_after_a_block_wins_over_the_blocks_content() {
        let d = "```agentspace-plan\nnot json at all\n```\n```agentspace-plan\n{}\n```\n";
        assert_eq!(why(d), MORE);
        let d = "```agentspace-plan\nnot json at all\n```\n```agentspace-planner\n";
        assert_eq!(why(d), MISS);
    }

    #[test]
    fn lines_that_are_not_fence_lines_outside_a_block_are_text() {
        let lines = [
            " ```agentspace-plan",
            "\t```agentspace-plan",
            "> ```agentspace-plan",
            "\u{feff}```agentspace-plan",
            "~~~agentspace-plan",
            "```Agentspace-Plan",
            "```AGENTSPACE-PLAN",
            "``agentspace-plan",
            "`agentspace-plan",
            "``",
            "`",
            "agentspace-plan",
        ];
        for line in lines {
            let d = format!("{line}\n{{}}\n```\n");
            assert_eq!(why(&d), NONE, "line {line:?}");
            let own = format!("{line}\n```agentspace-plan\n{{}}\n```\n{line}\n");
            assert_eq!(content(&own), "{}\n", "line {line:?}");
        }
    }

    #[test]
    fn indented_opening_line_is_text_and_its_block_carries_no_plan() {
        assert_eq!(why("  ```agentspace-plan\n  {}\n  ```\n"), NONE);
        assert_eq!(why("> ```agentspace-plan\n> {}\n> ```\n"), NONE);
    }

    #[test]
    fn lines_inside_a_block_that_are_not_fence_lines_are_content() {
        let d = "```agentspace-plan\n ```\n\t```\n> ```\n\u{feff}```\n~~~\n``\n`\n```Agentspace-Plan\n```\n";
        assert_eq!(
            content(d),
            " ```\n\t```\n> ```\n\u{feff}```\n~~~\n``\n`\n```Agentspace-Plan\n"
        );
    }

    #[test]
    fn other_line_break_characters_do_not_end_a_line() {
        for sep in ["\u{b}", "\u{c}", "\u{85}", "\u{2028}", "\u{2029}"] {
            // An opening line after the character is not at the start of a line: text.
            let d = format!("x{sep}```agentspace-plan\n{{}}\n```\n");
            assert_eq!(why(&d), NONE, "separator {sep:?}");
            // A closing line after the character is not a closing line.
            let d = format!("```agentspace-plan\n{{}}{sep}```\n");
            assert_eq!(why(&d), OPEN, "separator {sep:?}");
            // Nor does the character after a fence line end it.
            let d = format!("```agentspace-plan{sep}\n{{}}\n```\n");
            assert_eq!(why(&d), MISS, "separator {sep:?}");
            let d = format!("```agentspace-plan\n{{}}\n```{sep}\n");
            assert_eq!(why(&d), OPEN, "separator {sep:?}");
        }
    }

    #[test]
    fn opening_line_inside_a_markdown_fence_counts() {
        let example = "```markdown\n```agentspace-plan\n{}\n```\n```\n";
        let own = "```agentspace-plan\n{}\n```\n";
        assert_eq!(why(&format!("{example}{own}")), MORE);
        assert_eq!(why(&format!("{own}{example}")), MORE);
    }

    #[test]
    fn content_is_a_slice_of_the_description() {
        let d = String::from("Intro.\n```agentspace-plan\n{\"a\": 1}\n```\nOutro.\n");
        let c = content(&d);
        let offset = c.as_ptr() as usize - d.as_ptr() as usize;
        assert_eq!(offset, "Intro.\n```agentspace-plan\n".len());
        assert_eq!(c.len(), "{\"a\": 1}\n".len());
    }

    #[test]
    fn multi_byte_characters_next_to_fences_are_read() {
        let d = "é日本\n```agentspace-plan\n{\"n\": \"ü日本\"}\n```\nü\n";
        assert_eq!(content(d), "{\"n\": \"ü日本\"}\n");
        assert_eq!(why("é```agentspace-plan\n{}\n```\n"), NONE);
        assert_eq!(why("```agentspace-plané\n{}\n```\n"), MISS);
        assert_eq!(why("```é\n{}\n```\n"), NONE);
        assert_eq!(why("```agentspace-plan\n{}\n```é\n"), OPEN);
        assert_eq!(why("```agentspace-plan\n日本"), OPEN);
        assert_eq!(why("```agentspace-plan\n日本\n```日本"), OPEN);
        assert_eq!(content("```agentspace-plan\n日本\n```"), "日本\n");
    }

    #[test]
    fn every_prefix_of_a_mixed_description_is_read_without_panic() {
        let d = "é\r\n```agentspace-plan\r\n{\"n\": \"日\"}\r\n```\r\n```x\u{2028}\n````agentspace-plan\r";
        for (i, _) in d.char_indices().chain(std::iter::once((d.len(), ' '))) {
            let _ = plan_block(&d[..i]);
        }
    }

    #[test]
    fn error_is_displayed_as_its_message() {
        let e = plan_block("").unwrap_err();
        assert_eq!(e.to_string(), NONE);
        assert_eq!(e.clone(), e);
    }
}
