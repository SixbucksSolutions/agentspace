//! The grow's script and the push's zone and remote half (docs/operator.md, grow on a Windows host;
//! The token push to a Windows host; The platform seam, the grow and push rows; D96, N17, N23).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.

mod common;

use common::{MAX_LINE, PREFIX, assert_agentspace_words, decode_line, encode, platform, script_of};

const ZONE: &str = "Mountain Standard Time";

fn grow() -> String {
    platform().grow_script().expect("the grow script")
}

fn push(zone: &str) -> String {
    platform().push_script(zone).expect("the push script")
}

// grow

#[test]
fn the_grow_script_extends_the_c_partition_to_the_largest_size_it_supports() {
    let folded = grow().to_lowercase();
    assert!(
        folded.contains("-driveletter c") || folded.contains("-driveletter 'c'"),
        "the C: partition"
    );
    assert!(folded.contains("resize-partition"), "extends the partition");
    assert!(
        folded.contains("sizemax"),
        "to the largest size it supports"
    );
}

#[test]
fn the_grow_script_prints_the_volumes_size_and_free_space_in_gib() {
    let folded = grow().to_lowercase();
    assert!(
        folded.contains("get-volume") || folded.contains("get-psdrive"),
        "reads the volume"
    );
    assert!(
        (folded.contains("size") && folded.contains("free")) || folded.contains("sizeremaining")
    );
    assert!(folded.contains("1gb") || folded.contains("gib"), "in GiB");
}

#[test]
fn the_grow_script_prints_only_after_the_extension() {
    let folded = grow().to_lowercase();
    let extend = folded.find("resize-partition").expect("the extension");
    let print = folded
        .find("get-volume")
        .or_else(|| folded.find("get-psdrive"))
        .expect("the print");
    assert!(extend < print);
}

#[test]
fn the_grow_script_ends_with_an_exit_status_and_never_prompts() {
    let script = grow();
    let folded = script.to_lowercase();
    assert!(folded.contains("exit"));
    assert!(!folded.contains("read-host"));
    assert!(!folded.contains("-confirm"), "no prompt");
}

#[test]
fn the_grow_script_goes_in_the_encoded_form_within_the_limit() {
    let line = encode(&grow());
    assert!(line.starts_with(PREFIX));
    assert!(line.len() <= MAX_LINE);
    assert!(!line.contains('='));
}

#[test]
fn the_grow_script_survives_the_encoded_form_whole() {
    let script = grow();
    assert_eq!(script_of(&encode(&script)), script.trim_end_matches(' '));
}

// The push's zone

#[test]
fn a_windows_zone_id_is_returned_as_it_is_and_not_the_iana_name() {
    let zone = platform()
        .push_zone(ZONE, "America/Denver")
        .expect("a valid zone");
    assert_eq!(zone, ZONE);
    assert_ne!(zone, "America/Denver");
}

#[test]
fn the_spec_examples_of_zone_ids_pass() {
    for id in [
        "Mountain Standard Time",
        "UTC-02",
        "Pacific Standard Time (Mexico)",
        "UTC",
        "Central Europe Standard Time",
        "Eastern Standard Time",
        "Tokyo Standard Time",
        "Aus Central W. Standard Time",
        "Russia Time Zone 3",
    ] {
        assert_eq!(platform().push_zone(id, "Etc/Unused").unwrap(), id, "{id}");
    }
}

#[test]
fn a_zone_id_of_the_longest_length_passes_and_one_longer_fails() {
    // The pattern: a letter, up to 62 of the middle characters, and a last that ends the ID.
    let longest = format!("A{}Z", "b".repeat(62));
    assert_eq!(longest.len(), 64);
    assert_eq!(platform().push_zone(&longest, "x").unwrap(), longest);
    let too_long = format!("A{}Z", "b".repeat(63));
    assert!(platform().push_zone(&too_long, "x").is_err());
}

#[test]
fn the_shortest_zone_id_is_two_characters() {
    assert!(platform().push_zone("Ab", "x").is_ok());
    assert!(platform().push_zone("A", "x").is_err());
    assert!(platform().push_zone("A)", "x").is_ok());
}

#[test]
fn a_zone_id_must_start_with_an_ascii_letter() {
    for id in ["1Eastern", " Eastern", "-UTC", "(UTC)", "+UTC", ".Eastern"] {
        assert!(platform().push_zone(id, "x").is_err(), "{id:?}");
    }
}

#[test]
fn a_zone_id_must_end_with_a_letter_digit_or_close_parenthesis() {
    for id in ["Eastern ", "Eastern-", "Eastern.", "Eastern+", "Eastern("] {
        assert!(platform().push_zone(id, "x").is_err(), "{id:?}");
    }
    for id in ["Eastern1", "Eastern)", "UTC+1"] {
        assert!(platform().push_zone(id, "x").is_ok(), "{id:?}");
    }
}

#[test]
fn a_zone_id_with_a_character_outside_its_set_fails() {
    for id in [
        "America/Denver",
        "Eastern_Standard_Time",
        "Eastern'Time",
        "Eastern\"Time",
        "Eastern;Time",
        "Eastern`Time",
        "Eastern$Time",
        "Eastern%Time",
        "Eastern\\Time",
        "Eastern,Time",
        "Eastern:Time",
        "Eastern&Time",
        "Eastern|Time",
        "Eastern\tTime",
    ] {
        assert!(platform().push_zone(id, "x").is_err(), "{id:?}");
    }
}

#[test]
fn a_zone_id_with_a_line_end_anywhere_fails_including_a_final_one() {
    for id in [
        "Eastern Standard Time\n",
        "Eastern Standard Time\r\n",
        "Eastern\nStandard Time",
        "\nEastern Standard Time",
    ] {
        assert!(platform().push_zone(id, "x").is_err(), "{id:?}");
    }
}

#[test]
fn a_zone_id_with_letters_beyond_ascii_fails() {
    for id in [
        "Éastern Standard Time",
        "Eastern Standärd Time",
        "日本標準時",
    ] {
        assert!(platform().push_zone(id, "x").is_err(), "{id:?}");
    }
}

#[test]
fn the_empty_zone_fails_so_the_caller_sends_none() {
    assert!(platform().push_zone("", "America/Denver").is_err());
}

#[test]
fn an_invalid_zone_id_fails_even_when_the_iana_name_is_fine() {
    let outcome = platform().push_zone("America/Denver", "America/Denver");
    assert!(outcome.is_err());
}

#[test]
fn an_invalid_zone_never_falls_back_to_the_iana_name() {
    for iana in ["America/Denver", "Europe/Paris", "UTC", ""] {
        if let Ok(zone) = platform().push_zone("not a zone/", iana) {
            panic!("returned {zone:?}");
        }
    }
}

#[test]
fn the_zone_failure_is_agentspace_words_naming_no_backend() {
    let failure = platform()
        .push_zone("America/Denver", "America/Denver")
        .unwrap_err();
    assert_agentspace_words(&failure);
}

#[test]
fn the_zone_failure_does_not_carry_the_zone_it_refused() {
    let failure = platform()
        .push_zone("Eastern$(Remove-Item x)", "x")
        .unwrap_err();
    assert!(!failure.message.contains("Remove-Item"));
}

#[test]
fn no_zone_input_panics() {
    for id in ["", "\0", "\u{FFFF}", &"A".repeat(100_000), "😀😀"] {
        let _ = platform().push_zone(id, id);
    }
}

// The push's remote half

#[test]
fn the_push_script_sets_the_zone_with_set_timezone_id() {
    assert!(push(ZONE).contains("Set-TimeZone"));
    assert!(push(ZONE).contains("-Id"));
}

#[test]
fn the_push_script_holds_the_zone_and_the_line_does_not() {
    let script = push(ZONE);
    assert!(script.contains(ZONE));
    let line = encode(&script);
    assert!(
        !line.contains("Mountain"),
        "the zone is inside the encoding"
    );
    assert!(decode_line(&line).contains(ZONE));
}

#[test]
fn with_no_zone_the_push_script_sets_none() {
    assert!(!push("").contains("Set-TimeZone"));
}

#[test]
fn the_push_script_answers_zone_failed_for_a_zone_that_did_not_take() {
    assert!(push(ZONE).contains("zone-failed"));
}

#[test]
fn the_push_script_prints_the_zone_line_only_with_a_zone() {
    assert!(push(ZONE).contains("zone "));
}

#[test]
fn the_push_script_writes_the_credential_file_in_agents_dot_config_localswim() {
    let script = push(ZONE).replace('/', "\\");
    assert!(script.contains(".config\\localswim"));
    assert!(script.contains("bot.env"));
}

#[test]
fn the_push_script_reads_the_two_lines_from_its_stdin() {
    let script = push(ZONE).to_lowercase();
    assert!(
        script.contains("[console]::in")
            || script.contains("$input")
            || script.contains("openstandardinput")
            || script.contains("readline"),
        "reads stdin"
    );
}

#[test]
fn the_push_script_checks_the_two_lines_by_their_prefixes() {
    let script = push(ZONE);
    assert!(script.contains("LOCALSWIM_URL="));
    assert!(script.contains("LOCALSWIM_TOKEN="));
}

#[test]
fn the_push_script_gives_the_folder_and_files_to_agent_and_administrators_alone() {
    let script = push(ZONE);
    assert!(script.contains("BUILTIN\\Administrators"));
    assert!(
        script.contains("SetAccessRuleProtection") || script.contains("/inheritance:r"),
        "inheritance off"
    );
}

#[test]
fn the_push_script_replaces_the_credential_file_with_one_rename() {
    let script = push(ZONE);
    assert!(
        script.contains("Move-Item")
            || script.contains("Rename-Item")
            || script.contains("File]::Move")
            || script.contains("File]::Replace")
            || script.contains(".MoveTo("),
        "one rename"
    );
}

#[test]
fn the_push_script_prints_the_sha256_in_lowercase_hex_with_two_spaces_and_a_dash() {
    let script = push(ZONE);
    assert!(script.to_lowercase().contains("sha256"));
    assert!(
        script.contains("ToLower") || script.contains("x2"),
        "lowercase hex"
    );
    assert!(script.contains("  -"), "two spaces and a dash");
}

#[test]
fn the_push_script_gives_the_zone_setter_five_seconds() {
    let script = push(ZONE);
    assert!(script.contains("Set-TimeZone"));
    assert!(
        script.contains('5') && (script.contains("Wait") || script.contains("Timeout")),
        "a 5 second limit"
    );
}

#[test]
fn the_push_script_goes_in_the_encoded_form_within_the_limit() {
    let line = encode(&push(ZONE));
    assert!(line.starts_with(PREFIX));
    assert!(line.len() <= MAX_LINE);
    assert!(!line.contains('='));
}
