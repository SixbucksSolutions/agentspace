//! The rows of docs/operator.md, The time zone, checked against `agentspace_zone::zone`.
//!
//! The doc is read at test time, so a row changed there is checked here without a copy in
//! the test. The parser is plain standard library code, and it fails loudly when the section,
//! its table or its example cannot be found, so a reformatted doc cannot pass silently.
//!
//! What the section holds, as the doc is written today:
//! - Step 1 gives one Windows-to-IANA pair in prose ("for example `Pacific Standard Time` to
//!   `America/Los_Angeles`"). The section has no Windows-to-IANA table, so that pair is the
//!   only Windows row this test can read.
//! - Step 2 gives a table of seven legacy names and their current names. Each legacy name is
//!   an IANA name, not a Windows ID, so it is checked through the override argument, which the
//!   doc says takes the place of step 1 and then goes through steps 2 and 3 like any name.

use agentspace_zone::zone;

const DOC: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/operator.md");
const HEADING: &str = "### The time zone";
const LEGACY_TABLE_MIN_ROWS: usize = 7;
const EXAMPLE_LEAD: &str = "for example `";

fn read_doc() -> String {
    std::fs::read_to_string(DOC).unwrap_or_else(|e| panic!("cannot read {}: {}", DOC, e))
}

/// The lines of the time zone section, from its heading up to the next heading of the same or
/// a higher level.
fn zone_section(doc: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for line in doc.lines() {
        if !inside {
            if line.trim_end() == HEADING {
                inside = true;
            }
            continue;
        }
        if line.starts_with("### ") || line.starts_with("## ") {
            break;
        }
        out.push_str(line);
        out.push('\n');
    }
    assert!(
        inside,
        "no `{}` section in {}; the doc was reformatted or moved",
        HEADING, DOC
    );
    out
}

/// The rows of the section's first table as (first cell, second cell), with the header and the
/// separator line left out. The header must read `Legacy name` and `Current name`.
fn first_table_rows(section: &str) -> Vec<(String, String)> {
    let mut rows = Vec::new();
    let mut seen_table = false;
    let mut header_seen = false;
    for line in section.lines() {
        let t = line.trim();
        if !t.starts_with('|') {
            if seen_table {
                break;
            }
            continue;
        }
        seen_table = true;
        let cells: Vec<String> = t
            .trim_matches('|')
            .split('|')
            .map(|c| c.trim().to_string())
            .collect();
        if !header_seen {
            assert_eq!(
                cells,
                vec!["Legacy name".to_string(), "Current name".to_string()],
                "the table in `{}` has an unexpected header; the doc was reformatted",
                HEADING
            );
            header_seen = true;
            continue;
        }
        if cells
            .iter()
            .all(|c| !c.is_empty() && c.chars().all(|ch| ch == '-' || ch == ':'))
        {
            continue;
        }
        assert_eq!(
            cells.len(),
            2,
            "table row {:?} in `{}` does not have two cells",
            t,
            HEADING
        );
        let first = cells[0].trim_matches('`').to_string();
        let second = cells[1].trim_matches('`').to_string();
        assert!(
            !first.is_empty() && !second.is_empty(),
            "table row {:?} in `{}` has an empty cell",
            t,
            HEADING
        );
        rows.push((first, second));
    }
    assert!(
        header_seen,
        "no table found in `{}`; the doc was reformatted",
        HEADING
    );
    rows
}

/// The one Windows-to-IANA pair the section gives, read from step 1's prose. The prose wraps
/// across lines, so whitespace is collapsed before the pair is read.
fn windows_example(section: &str) -> (String, String) {
    let flat = section.split_whitespace().collect::<Vec<_>>().join(" ");
    let start = flat.find(EXAMPLE_LEAD).unwrap_or_else(|| {
        panic!(
            "no `{}...` Windows example in `{}`; the doc was reformatted",
            EXAMPLE_LEAD, HEADING
        )
    }) + EXAMPLE_LEAD.len();
    let rest = &flat[start..];
    let end = rest
        .find('`')
        .unwrap_or_else(|| panic!("unclosed Windows zone ID in the example in `{}`", HEADING));
    let windows = rest[..end].to_string();
    let after = rest[end + 1..]
        .strip_prefix(" to `")
        .unwrap_or_else(|| panic!("no ` to ` after Windows ID {:?} in `{}`", windows, HEADING));
    let end2 = after
        .find('`')
        .unwrap_or_else(|| panic!("unclosed IANA name after {:?} in `{}`", windows, HEADING));
    (windows, after[..end2].to_string())
}

#[test]
fn windows_example_in_step_one_maps_to_its_iana_name() {
    let section = zone_section(&read_doc());
    let (windows, iana) = windows_example(&section);
    let got = zone(Some(windows.as_str()), None);
    assert_eq!(
        got,
        Some(iana.clone()),
        "row `{}` -> `{}` (docs/operator.md, step 1): zone(Some({:?}), None) returned {:?}",
        windows,
        iana,
        windows,
        got
    );
}

#[test]
fn legacy_names_in_step_two_map_to_their_current_names() {
    let section = zone_section(&read_doc());
    let rows = first_table_rows(&section);
    assert!(
        rows.len() >= LEGACY_TABLE_MIN_ROWS,
        "parsed {} legacy rows from `{}`, expected at least {}; the table was cut short or not found",
        rows.len(),
        HEADING,
        LEGACY_TABLE_MIN_ROWS
    );

    let mut failures = Vec::new();
    for (legacy, current) in &rows {
        let got = zone(None, Some(legacy.as_str()));
        if got.as_deref() != Some(current.as_str()) {
            failures.push(format!(
                "row `{}` -> `{}` (docs/operator.md, step 2): zone(None, Some({:?})) returned {:?}",
                legacy, current, legacy, got
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} legacy rows failed:\n{}",
        failures.len(),
        rows.len(),
        failures.join("\n")
    );
}

/// Twenty `territory="001"` mapZone rows of CLDR release-49's windowsZones.xml, spread across
/// the file: the first row, the last, and about every seventh between, plus the five rows that
/// name `Pacific Standard Time`, `Central Asia Standard Time`, `Alberta Standard Time`,
/// `India Standard Time` and `UTC`. The file's SHA-256 is
/// 5f902f94106a396cb7774f0b0e1ca4dfecb686494045350c0b4935d378e3913b (sha256sum, read when the
/// test was written). Each pair is (Windows ID, CLDR type) exactly as the file gives them. The
/// expected zone is the CLDR type after the legacy mapping below, which is docs/operator.md
/// step 2's table, written here so the test does not read the doc.
const CLDR_SAMPLE: &[(&str, &str)] = &[
    ("Dateline Standard Time", "Etc/GMT+12"),
    ("Pacific Standard Time (Mexico)", "America/Tijuana"),
    ("Pacific Standard Time", "America/Los_Angeles"),
    ("Alberta Standard Time", "America/Edmonton"),
    ("Manitoba Standard Time", "America/Winnipeg"),
    ("Turks And Caicos Standard Time", "America/Grand_Turk"),
    ("Newfoundland Standard Time", "America/St_Johns"),
    ("Greenland Standard Time", "America/Godthab"),
    ("UTC", "Etc/UTC"),
    ("Morocco Standard Time", "Africa/Casablanca"),
    ("Central European Standard Time", "Europe/Warsaw"),
    ("South Africa Standard Time", "Africa/Johannesburg"),
    ("Sudan Standard Time", "Africa/Khartoum"),
    ("Russian Standard Time", "Europe/Moscow"),
    ("Saratov Standard Time", "Europe/Saratov"),
    ("West Asia Standard Time", "Asia/Tashkent"),
    ("India Standard Time", "Asia/Calcutta"),
    ("Central Asia Standard Time", "Asia/Bishkek"),
    ("Myanmar Standard Time", "Asia/Rangoon"),
    ("Line Islands Standard Time", "Pacific/Kiritimati"),
];

/// The legacy names of docs/operator.md, The time zone, step 2, with their current names.
const LEGACY_MAP: &[(&str, &str)] = &[
    ("Asia/Calcutta", "Asia/Kolkata"),
    ("Asia/Katmandu", "Asia/Kathmandu"),
    ("Asia/Rangoon", "Asia/Yangon"),
    ("Europe/Kiev", "Europe/Kyiv"),
    ("America/Buenos_Aires", "America/Argentina/Buenos_Aires"),
    ("America/Godthab", "America/Nuuk"),
    ("America/Indianapolis", "America/Indiana/Indianapolis"),
];

fn after_legacy_mapping(cldr: &str) -> &str {
    LEGACY_MAP
        .iter()
        .find(|(legacy, _)| *legacy == cldr)
        .map(|(_, current)| *current)
        .unwrap_or(cldr)
}

#[test]
fn cldr_sample_rows() {
    assert_eq!(CLDR_SAMPLE.len(), 20, "the CLDR sample must hold 20 rows");
    let mut failures = Vec::new();
    for &(windows, cldr) in CLDR_SAMPLE {
        let expected = after_legacy_mapping(cldr);
        let got = zone(Some(windows), None);
        if got.as_deref() != Some(expected) {
            failures.push(format!(
                "row `{}` -> `{}` (CLDR release-49, expected `{}` after the legacy mapping): zone(Some({:?}), None) returned {:?}",
                windows, cldr, expected, windows, got
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} CLDR sample rows failed:\n{}",
        failures.len(),
        CLDR_SAMPLE.len(),
        failures.join("\n")
    );
}
