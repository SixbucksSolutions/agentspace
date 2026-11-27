//! Helpers of `template.rs`: the template's text, read from the repository and from the fixture
//! copied from `5d88698`, and the extraction of its `UserData` branches (E0.12-T03-S03, #0256).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Every function is a text function: no
//! CloudFormation, no AWS and no program is run.
#![allow(dead_code)]

use std::fs;
use std::path::PathBuf;

/// The repository's template.
pub fn template() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("infra")
        .join("agentspace.yaml");
    normalized(&fs::read_to_string(path).expect("The template is in the repository"))
}

/// The template at `5d88698`, before v0.12-T03 changed it: the fixture, copied with `git show`.
pub fn baseline() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("template-5d88698.yaml");
    normalized(&fs::read_to_string(path).expect("The baseline template's fixture is there"))
}

/// Line ends written LF, since `.gitattributes` makes every checkout's text LF but a reader on
/// another checkout MUST NOT make a byte comparison fail on a CR.
pub fn normalized(text: &str) -> String {
    text.replace("\r\n", "\n")
}

pub fn indent(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

/// One block scalar `Fn::Base64: !Sub |`: the line that opens it and its content, dedented by the
/// first content line's indentation, with trailing blank lines dropped (a clipped block scalar's
/// value ends in one line feed whatever follows it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub opener: String,
    pub content: String,
}

/// Every `Fn::Base64: !Sub |` block of `text`, in the file's order.
pub fn blocks(text: &str) -> Vec<Block> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start().trim_start_matches("- ").trim_start();
        let opens = trimmed.starts_with("Fn::Base64:")
            && trimmed.contains('|')
            && matches!(trimmed.trim_end().chars().last(), Some('|' | '-' | '+'));
        if !opens {
            i += 1;
            continue;
        }
        let item = indent(line);
        let mut body: Vec<&str> = Vec::new();
        let mut j = i + 1;
        while j < lines.len() {
            let l = lines[j];
            if l.trim().is_empty() {
                body.push("");
                j += 1;
                continue;
            }
            if indent(l) <= item {
                break;
            }
            body.push(l);
            j += 1;
        }
        let first = body
            .iter()
            .find(|l| !l.trim().is_empty())
            .map(|l| indent(l))
            .unwrap_or(0);
        let mut dedented: Vec<String> = body
            .iter()
            .map(|l| {
                if l.trim().is_empty() {
                    String::new()
                } else {
                    l[first.min(l.len())..].to_string()
                }
            })
            .collect();
        while dedented.last().is_some_and(|l| l.is_empty()) {
            dedented.pop();
        }
        out.push(Block {
            opener: line.trim().to_string(),
            content: dedented.join("\n"),
        });
        i = j;
    }
    out
}

pub fn is_windows(block: &Block) -> bool {
    block.content.contains("<powershell>")
}

/// The Linux blocks of `text`: every block that is not the Windows branch. The opener is where a
/// branch sits in the file, not part of its bytes, so it is normalized.
pub fn linux_blocks(text: &str) -> Vec<Block> {
    blocks(text)
        .into_iter()
        .filter(|b| !is_windows(b))
        .map(|b| Block {
            opener: "Fn::Base64: !Sub |".to_string(),
            content: b.content,
        })
        .collect()
}

/// The template's one Windows block.
pub fn windows_block() -> Block {
    let found: Vec<Block> = blocks(&template()).into_iter().filter(is_windows).collect();
    assert_eq!(
        found.len(),
        1,
        "The template MUST hold exactly one Windows UserData branch (a block holding <powershell>)"
    );
    found.into_iter().next().unwrap()
}

/// The lines of a `Parameters` entry named `name`: from its key to the next key of the same
/// indentation.
pub fn parameter(text: &str, name: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().collect();
    let key = format!("  {name}:");
    let start = lines
        .iter()
        .position(|l| *l == key)
        .unwrap_or_else(|| panic!("The template has no parameter {name}"));
    lines[start + 1..]
        .iter()
        .take_while(|l| l.trim().is_empty() || indent(l) > 2)
        .map(|l| l.to_string())
        .collect()
}

/// The value of a one-line member `Key: value` of a parameter, trimmed of YAML quotes.
pub fn member(lines: &[String], key: &str) -> Option<String> {
    lines.iter().find_map(|l| {
        l.trim()
            .strip_prefix(&format!("{key}:"))
            .map(|v| v.trim().trim_matches('"').trim_matches('\'').to_string())
    })
}
