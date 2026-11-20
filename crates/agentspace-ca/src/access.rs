//! Owner-only, and the `<problems>` of `ca`'s messages (docs/state.md, Access rules; docs/operator.md,
//! ca). Portable: it works on rules that the platform module has read.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! - **Owner-only is all four of state.md's rules:** the owner is the user's SID, the list is
//!   protected, it holds exactly one entry, and that entry allows the user every right of full
//!   control. The check MUST NOT look at an entry's inheritance flags.
//! - **`<problems>` MUST list what applies in operator.md's order:** the owner, then inheritance,
//!   then each entry in the list's own order, then the count.

/// Every right of full control for a file or a folder (`FILE_ALL_ACCESS`, `0x1F01FF`).
pub(crate) const FULL_CONTROL: u32 = 0x001F_01FF;

/// What an access control entry does. Only the Windows build reads entries; elsewhere the tests
/// alone make them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) enum AceKind {
    /// A plain allow entry (`ACCESS_ALLOWED_ACE_TYPE`), the one type an owner-only list holds.
    Allow,
    /// A deny entry, of any of Windows' deny types.
    Deny,
    /// An entry of any other type: an allow with a condition or for an object, or a type a file's
    /// list does not hold in practice. It is read as a rule that allows, whoever it names, the
    /// user included, so a list that holds one is never owner-only. The lease CLI's check
    /// (agentspace-credential) takes only a plain allow entry too, and the two MUST agree.
    Other,
}

/// One access control entry, by SID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Ace {
    pub(crate) kind: AceKind,
    /// The SID in `S-1-...` form, or `None` for an entry whose SID could not be read.
    pub(crate) sid: Option<String>,
    pub(crate) mask: u32,
    /// The entry's flags byte (`OBJECT_INHERIT_ACE` is 0x1, `CONTAINER_INHERIT_ACE` 0x2,
    /// `INHERITED_ACE` 0x10). The owner-only check does not read it.
    pub(crate) flags: u8,
}

/// A file's or a folder's owner and access control list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Rules {
    /// The owner's SID in `S-1-...` form.
    pub(crate) owner: String,
    /// Whether the list is protected from inheritance (`SE_DACL_PROTECTED`).
    pub(crate) protected: bool,
    /// The entries in their order; `None` for a NULL list, which allows everyone everything.
    pub(crate) aces: Option<Vec<Ace>>,
}

/// The well-known SID of `Everyone`, which a NULL list allows.
const EVERYONE_SID: &str = "S-1-1-0";

/// The `<problems>` of `ca`'s messages for `rules`, in operator.md's order, joined with `; `.
/// Empty when, and only when, the rules are owner-only for `user_sid`.
///
/// `user` is `<user>`, the value of `USERNAME`. `name_of` gives an account as Windows names it,
/// `<domain>\<name>`, or `None` when Windows cannot translate the SID, which is then shown as it is.
pub(crate) fn problems(
    rules: &Rules,
    user_sid: &str,
    user: &str,
    name_of: &dyn Fn(&str) -> Option<String>,
) -> String {
    let who = |sid: &str| name_of(sid).unwrap_or_else(|| sid.to_string());
    let mut list = Vec::new();
    if rules.owner != user_sid {
        list.push(format!("is owned by {}", who(&rules.owner)));
    }
    if !rules.protected {
        list.push("inherits access rules from its parent".to_string());
    }
    match &rules.aces {
        // A null list allows everyone everything: its <problems> is "has a rule allowing
        // Everyone; has 0 access rules instead of one" (operator.md, ca, Rulings for the
        // rewrite's ca: architect, 2026-11-19, for E0.06-T02-S01).
        None => {
            list.push(format!("has a rule allowing {}", who(EVERYONE_SID)));
            list.push("has 0 access rules instead of one".to_string());
        }
        Some(aces) => {
            for ace in aces {
                let sid = ace.sid.as_deref();
                let named = sid.map_or_else(|| "an unknown principal".to_string(), who);
                match ace.kind {
                    AceKind::Allow if sid == Some(user_sid) => {
                        if ace.mask & FULL_CONTROL != FULL_CONTROL {
                            list.push(format!(
                                "gives {user} {}, not full control",
                                rights_words(ace.mask)
                            ));
                        }
                    }
                    AceKind::Allow | AceKind::Other => {
                        list.push(format!("has a rule allowing {named}"));
                    }
                    AceKind::Deny => list.push(format!("has a rule denying {named}")),
                }
            }
            if aces.len() != 1 {
                list.push(format!("has {} access rules instead of one", aces.len()));
            }
        }
    }
    list.join("; ")
}

/// The composite rights, largest first, then each single right, in .NET's `FileSystemRights`
/// names, as the prototype writes them. The text is not part of the contract.
const RIGHTS: &[(u32, &str)] = &[
    (0x001F_01FF, "FullControl"),
    (0x0003_01BF, "Modify"),
    (0x0002_00A9, "ReadAndExecute"),
    (0x0002_0089, "Read"),
    (0x0000_0116, "Write"),
    (0x0000_0001, "ReadData"),
    (0x0000_0002, "WriteData"),
    (0x0000_0004, "AppendData"),
    (0x0000_0008, "ReadExtendedAttributes"),
    (0x0000_0010, "WriteExtendedAttributes"),
    (0x0000_0020, "ExecuteFile"),
    (0x0000_0040, "DeleteSubdirectoriesAndFiles"),
    (0x0000_0080, "ReadAttributes"),
    (0x0000_0100, "WriteAttributes"),
    (0x0001_0000, "Delete"),
    (0x0002_0000, "ReadPermissions"),
    (0x0004_0000, "ChangePermissions"),
    (0x0008_0000, "TakeOwnership"),
    (0x0010_0000, "Synchronize"),
    (0x1000_0000, "GenericAll"),
    (0x2000_0000, "GenericExecute"),
    (0x4000_0000, "GenericWrite"),
    (0x8000_0000, "GenericRead"),
];

/// `<rights>`: the rights `mask` grants, in words a person can read, joined with `, `.
pub(crate) fn rights_words(mask: u32) -> String {
    if mask == 0 {
        return "no rights".to_string();
    }
    let mut left = mask;
    let mut words = Vec::new();
    for &(bits, name) in RIGHTS {
        if left & bits == bits && left & bits != 0 {
            words.push(name.to_string());
            left &= !bits;
        }
    }
    if left != 0 {
        words.push(format!("0x{left:08X}"));
    }
    words.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const ME: &str = "S-1-5-21-1-2-3-1001";

    fn names(sid: &str) -> Option<String> {
        match sid {
            "S-1-5-32-544" => Some("BUILTIN\\Administrators".to_string()),
            "S-1-5-18" => Some("NT AUTHORITY\\SYSTEM".to_string()),
            "S-1-1-0" => Some("Everyone".to_string()),
            ME => Some("EXAMPLE\\someone".to_string()),
            _ => None,
        }
    }

    fn ace(kind: AceKind, sid: &str, mask: u32) -> Ace {
        Ace {
            kind,
            sid: Some(sid.to_string()),
            mask,
            flags: 0,
        }
    }

    fn owner_only() -> Rules {
        Rules {
            owner: ME.to_string(),
            protected: true,
            aces: Some(vec![ace(AceKind::Allow, ME, FULL_CONTROL)]),
        }
    }

    #[test]
    fn owner_only_rules_have_no_problems() {
        assert_eq!(problems(&owner_only(), ME, "someone", &names), "");
    }

    #[test]
    fn rights_beyond_full_control_do_not_break_it() {
        let mut rules = owner_only();
        rules.aces = Some(vec![ace(AceKind::Allow, ME, FULL_CONTROL | 0x1000_0000)]);
        assert_eq!(problems(&rules, ME, "someone", &names), "");
    }

    #[test]
    fn inheritance_flags_are_not_read() {
        let mut rules = owner_only();
        rules.aces = Some(vec![Ace {
            flags: 0x13,
            ..ace(AceKind::Allow, ME, FULL_CONTROL)
        }]);
        assert_eq!(problems(&rules, ME, "someone", &names), "");
    }

    #[test]
    fn every_problem_in_operator_md_order() {
        let rules = Rules {
            owner: "S-1-5-32-544".to_string(),
            protected: false,
            aces: Some(vec![
                ace(AceKind::Deny, "S-1-5-99", 0x1),
                ace(AceKind::Allow, ME, 0x0012_00A9),
                ace(AceKind::Allow, "S-1-5-18", FULL_CONTROL),
                ace(AceKind::Deny, ME, 0x2),
            ]),
        };
        assert_eq!(
            problems(&rules, ME, "someone", &names),
            "is owned by BUILTIN\\Administrators; inherits access rules from its parent; \
             has a rule denying S-1-5-99; gives someone ReadAndExecute, Synchronize, not full control; \
             has a rule allowing NT AUTHORITY\\SYSTEM; has a rule denying EXAMPLE\\someone; \
             has 4 access rules instead of one"
        );
    }

    #[test]
    fn an_empty_list_is_counted() {
        let mut rules = owner_only();
        rules.aces = Some(Vec::new());
        assert_eq!(
            problems(&rules, ME, "someone", &names),
            "has 0 access rules instead of one"
        );
    }

    #[test]
    fn a_null_list_allows_everyone() {
        let mut rules = owner_only();
        rules.aces = None;
        assert_eq!(
            problems(&rules, ME, "someone", &names),
            "has a rule allowing Everyone; has 0 access rules instead of one"
        );
    }

    #[test]
    fn a_single_rule_for_someone_else_is_not_counted() {
        let mut rules = owner_only();
        rules.aces = Some(vec![ace(AceKind::Allow, "S-1-1-0", FULL_CONTROL)]);
        assert_eq!(
            problems(&rules, ME, "someone", &names),
            "has a rule allowing Everyone"
        );
    }

    #[test]
    fn an_entry_of_another_type_is_a_rule_allowing() {
        let mut rules = owner_only();
        rules.aces = Some(vec![
            ace(AceKind::Allow, ME, FULL_CONTROL),
            Ace {
                kind: AceKind::Other,
                sid: None,
                mask: 0,
                flags: 0,
            },
        ]);
        assert_eq!(
            problems(&rules, ME, "someone", &names),
            "has a rule allowing an unknown principal; has 2 access rules instead of one"
        );
    }

    #[test]
    fn an_entry_of_another_type_for_the_user_is_not_the_users_own_rule() {
        let mut rules = owner_only();
        rules.aces = Some(vec![ace(AceKind::Other, ME, FULL_CONTROL)]);
        assert_eq!(
            problems(&rules, ME, "someone", &names),
            "has a rule allowing EXAMPLE\\someone"
        );
    }

    #[test]
    fn rights_are_named() {
        assert_eq!(rights_words(0x0012_00A9), "ReadAndExecute, Synchronize");
        assert_eq!(rights_words(0x0013_01BF), "Modify, Synchronize");
        assert_eq!(rights_words(FULL_CONTROL), "FullControl");
        assert_eq!(rights_words(0), "no rights");
        assert_eq!(rights_words(0x0100_0000), "0x01000000");
    }
}
