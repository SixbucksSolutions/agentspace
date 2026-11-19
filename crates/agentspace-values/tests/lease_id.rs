// RFC 2119 keywords in these comments; the capitals are load-bearing.
//
// Lease ID: docs/lease-api.md, Values, "Lease ID". A UUIDv4 in any case with surrounding whitespace,
// normalized to lowercase and trimmed.

mod common;

use agentspace_values::lease_id;
use common::*;

#[test]
fn lowercase_uuid_is_accepted_as_it_is() {
    assert_eq!(ok(lease_id(UUID), UUID), UUID);
}

#[test]
fn uppercase_uuid_is_normalized_to_lowercase() {
    let upper = UUID.to_ascii_uppercase();
    assert_eq!(ok(lease_id(&upper), &upper), UUID);
}

#[test]
fn mixed_case_uuid_is_normalized_to_lowercase() {
    let mixed = "3F2a9C1e-7B4d-4E6a-8C2f-0A1b2C3d4E5f";
    assert_eq!(ok(lease_id(mixed), mixed), UUID);
}

#[test]
fn surrounding_whitespace_is_trimmed() {
    for v in [
        format!(" {UUID}"),
        format!("{UUID} "),
        format!("  \t{UUID}\r\n "),
        format!("\n{}\n", UUID.to_ascii_uppercase()),
    ] {
        assert_eq!(ok(lease_id(&v), &v), UUID);
    }
}

#[test]
fn variant_nibble_8_9_a_b_is_accepted_in_either_case() {
    for y in ['8', '9', 'a', 'b', 'A', 'B'] {
        let v = format!("3f2a9c1e-7b4d-4e6a-{y}c2f-0a1b2c3d4e5f");
        assert_eq!(ok(lease_id(&v), &v), v.to_ascii_lowercase());
    }
}

#[test]
fn variant_nibble_other_than_8_9_a_b_is_refused() {
    for y in "01234567cdefCDEF".chars() {
        let v = format!("3f2a9c1e-7b4d-4e6a-{y}c2f-0a1b2c3d4e5f");
        invalid(lease_id(&v), &v, "id");
    }
}

#[test]
fn every_version_other_than_4_is_refused() {
    for ver in "0123567890abcdefABCDEF".chars() {
        let v = format!("3f2a9c1e-7b4d-{ver}e6a-8c2f-0a1b2c3d4e5f");
        invalid(lease_id(&v), &v, "id");
    }
}

#[test]
fn a_4_in_another_place_does_not_make_a_version_4_id() {
    // The version digit is the first of the third group.
    let v = "3f2a9c1e-7b4d-1e4a-8c2f-0a1b2c3d4e5f";
    invalid(lease_id(v), v, "id");
}

#[test]
fn conformance_cases_are_invalid_value_for_id() {
    // From conformance/lease-cli/test-values.ps1.
    let cases = [
        "not-a-uuid".to_string(),
        String::new(),
        "3f2a9c1e-7b4d-1e6a-8c2f-0a1b2c3d4e5f".to_string(),
        "3f2a9c1e-7b4d-4e6a-cc2f-0a1b2c3d4e5f".to_string(),
        format!("urn:uuid:{UUID}"),
        "3f2a9c1e7b4d4e6a8c2f0a1b2c3d4e5f".to_string(),
        format!("{UUID}/x"),
        "..\\x".to_string(),
        format!("{{{UUID}}}"),
        "nope".to_string(),
    ];
    for v in cases {
        invalid(lease_id(&v), &v, "id");
    }
}

#[test]
fn prefixes_braces_and_decoration_are_refused() {
    for v in [
        format!("URN:UUID:{UUID}"),
        format!("urn:uuid:{}", UUID.to_ascii_uppercase()),
        format!("{{{UUID}"),
        format!("{UUID}}}"),
        format!("({UUID})"),
        format!("\"{UUID}\""),
        format!("'{UUID}'"),
        format!("{UUID}."),
        format!("{UUID}/"),
        format!("/{UUID}"),
        format!("..{UUID}"),
        format!("{UUID}0"),
        format!("{UUID}-"),
        format!("0{UUID}"),
        format!("{UUID} {UUID}"),
        format!("{UUID}\n{UUID}"),
    ] {
        invalid(lease_id(&v), &v, "id");
    }
}

#[test]
fn wrong_length_is_refused() {
    invalid(lease_id(&UUID[..35]), &UUID[..35], "id");
    invalid(lease_id(&UUID[1..]), &UUID[1..], "id");
}

#[test]
fn misplaced_or_missing_hyphens_are_refused() {
    for v in [
        "3f2a9c1e7b4d-4e6a-8c2f-0a1b2c3d4e5f",
        "3f2a9c1e-7b4d4e6a-8c2f-0a1b2c3d4e5f",
        "3f2a9c1e-7b4d-4e6a8c2f-0a1b2c3d4e5f",
        "3f2a9c1e-7b4d-4e6a-8c2f0a1b2c3d4e5f",
        "3f2a9c1-e7b4d-4e6a-8c2f-0a1b2c3d4e5f",
        "3f2a9c1e_7b4d_4e6a_8c2f_0a1b2c3d4e5f",
        "3f2a9c1e 7b4d 4e6a 8c2f 0a1b2c3d4e5f",
        "3f2a9c1e\u{2010}7b4d\u{2010}4e6a\u{2010}8c2f\u{2010}0a1b2c3d4e5f",
        "3f2a9c1e\u{2212}7b4d\u{2212}4e6a\u{2212}8c2f\u{2212}0a1b2c3d4e5f",
    ] {
        invalid(lease_id(v), v, "id");
    }
}

#[test]
fn non_hex_characters_are_refused() {
    for bad in [
        'g', 'G', 'z', 'x', ' ', '/', '\\', '.', '\u{ff10}', '\u{ff41}', '\u{661}',
    ] {
        let v = format!("3f2a9c1e-7b4d-4e6a-8c2f-0a1b2c3d4e5{bad}");
        invalid(lease_id(&v), &v, "id");
    }
}

#[test]
fn non_ascii_digits_and_letters_are_not_hex_digits() {
    // Fullwidth and Arabic-Indic digits, and a fullwidth letter, are not ASCII hex digits.
    for v in [
        "\u{ff13}f2a9c1e-7b4d-4e6a-8c2f-0a1b2c3d4e5f",
        "3f2a9c1e-7b4d-4e6a-8c2f-0a1b2c3d4e5\u{669}",
        "\u{ff23}f2a9c1e-7b4d-4e6a-8c2f-0a1b2c3d4e5f",
        "3f2a9c1e-7b4d-\u{ff14}e6a-8c2f-0a1b2c3d4e5f",
    ] {
        invalid(lease_id(v), v, "id");
    }
}

#[test]
fn empty_and_whitespace_only_are_refused() {
    for v in ["", " ", "\t", "\n", "\u{3000}", " \r\n "] {
        invalid(lease_id(v), v, "id");
    }
}

#[test]
fn a_trailing_line_feed_is_trimmed_and_never_kept() {
    // #0029: the normal form never holds a line feed, and a line feed that is not trailing refuses.
    for v in [
        format!("{UUID}\n"),
        format!("{UUID}\n\n"),
        format!("{UUID}\r\n"),
    ] {
        let n = ok(lease_id(&v), &v);
        assert_eq!(n, UUID);
        assert!(!n.contains('\n'));
    }
    let v = format!("{UUID}\n\u{0}");
    invalid(lease_id(&v), &v, "id");
    let v = format!("{UUID}\n\u{feff}");
    invalid(lease_id(&v), &v, "id");
    let v = "3f2a9c1e-7b4d-4e6a\n-8c2f-0a1b2c3d4e5f";
    invalid(lease_id(v), v, "id");
}

#[test]
fn a_normalized_id_never_has_a_dot_or_a_path_separator() {
    for v in [
        UUID.to_string(),
        UUID.to_ascii_uppercase(),
        format!("  {UUID}\n"),
    ] {
        let n = ok(lease_id(&v), &v);
        assert!(!n.contains(['.', '/', '\\']));
        assert!(!n.ends_with('.'));
        assert_eq!(n.len(), 36);
        assert!(
            n.chars()
                .all(|c| c.is_ascii_digit() || c.is_ascii_lowercase() || c == '-')
        );
    }
}

#[test]
fn invalid_message_names_the_id_untrimmed() {
    let v = " nope ";
    assert_eq!(
        invalid(lease_id(v), v, "id"),
        "Lease ID ' nope ' is not a UUIDv4. Give the id that create printed."
    );
    assert_eq!(
        invalid(lease_id(""), "", "id"),
        "Lease ID '' is not a UUIDv4. Give the id that create printed."
    );
    let v = format!("urn:uuid:{UUID}");
    assert_eq!(
        invalid(lease_id(&v), &v, "id"),
        format!("Lease ID 'urn:uuid:{UUID}' is not a UUIDv4. Give the id that create printed.")
    );
}

#[test]
fn invalid_message_escapes_control_characters() {
    let v = "ab\ncd";
    assert_eq!(
        invalid(lease_id(v), v, "id"),
        "Lease ID 'ab\\u000acd' is not a UUIDv4. Give the id that create printed."
    );
}

#[test]
fn other_uuid_versions_of_the_same_shape_are_refused() {
    // Versions 1, 3, 5 and 7, however well formed.
    for v in [
        "c232ab00-9414-11ec-b3c8-9f6bdeced846",
        "6fa459ea-ee8a-3ca4-894e-db77e160355e",
        "886313e1-3b8a-5372-9b90-0c9aee199e5d",
        "017f22e2-79b0-7cc3-98c4-dc0c0c07398f",
    ] {
        invalid(lease_id(v), v, "id");
    }
}
