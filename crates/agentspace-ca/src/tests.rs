//! The implementer's tests of `ca` and `sync`'s check: every step and message, against a fake
//! host (any platform) and against the real Windows API (Windows only).
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
//!
//! - **Every test MUST work only in a throwaway folder under the temp folder**, made by the test
//!   itself, and MUST NOT touch the real `%USERPROFILE%\.agentspace` or its CA. The home is always
//!   the throwaway's, passed in the `Context`, and the `CaEnv` is built here, with made-up names
//!   (user `someone`, computer `EXAMPLEPC`), never from the environment. The environment is read
//!   only to hide this machine's names in a failure (`masked`) and to find Windows OpenSSH's
//!   `ssh-keygen.exe` under `SystemRoot`, the oracle of one test, which runs it with a throwaway
//!   home in its environment.
//! - **A folder or key a test loosens is lifted in the throwaway's drop guard**, before it is
//!   removed, so cleanup works whatever the test left.
//! - **A test MUST NOT print a message, a line or a path as it is**: every throwaway path is under
//!   the temp folder, which is under the user's profile and so carries the Windows user name. A
//!   message, a line or a path is compared with `same`, `same_for` or `same_lines` and unwrapped
//!   with `fine`, which print it only through `masked`; an owner, a SID or a set of rules is
//!   compared with `assert!`, which prints no value (AGENTS.md, Terry's real files in a review).

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use agentspace_operator_api::{CallerFailure, CallerLine, Context, Failure, Mode, Say};

use crate::access::{Ace, AceKind, FULL_CONTROL, Rules};
use crate::{CaEnv, CaPaths, Host, run};

const ME: &str = "S-1-5-21-1000-2000-3000-1001";
const SYSTEM: &str = "S-1-5-18";
const ADMINS: &str = "S-1-5-32-544";
const EVERYONE: &str = "S-1-1-0";

// ---- The throwaway folder ----

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// A throwaway folder under the temp folder, lifted and removed when it drops.
struct Scratch {
    root: PathBuf,
}

impl Scratch {
    fn new() -> Scratch {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        let root = std::env::temp_dir().join(format!(
            "agentspace-ca-unit-{}-{}-{nanos}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir_all(&root).expect("make the throwaway folder");
        Scratch { root }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        #[cfg(windows)]
        lift(&self.root);
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// Gives the user full control of `path` and everything in it, top down, so that it can be
/// listed and removed whatever rules a test set.
#[cfg(windows)]
fn lift(path: &Path) {
    use crate::platform::testing::set_sddl;
    let Ok(me) = crate::platform::Windows.user_sid() else {
        return;
    };
    let Ok(meta) = fs::symlink_metadata(path) else {
        return;
    };
    if meta.is_dir() {
        set_sddl(path, &format!("D:P(A;OICI;FA;;;{me})"));
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                lift(&entry.path());
            }
        }
    } else {
        set_sddl(path, &format!("D:P(A;;FA;;;{me})"));
    }
}

/// A throwaway home and system folder, with a stand-in `ssh-keygen.exe` that is never run.
struct Lab {
    scratch: Scratch,
    ctx: Context,
    paths: CaPaths,
}

fn lab() -> Lab {
    let scratch = Scratch::new();
    let home = scratch.root.join("home");
    let system_root = scratch.root.join("sys");
    fs::create_dir_all(&home).unwrap();
    let openssh = system_root.join("System32").join("OpenSSH");
    fs::create_dir_all(&openssh).unwrap();
    fs::write(openssh.join("ssh-keygen.exe"), b"never run").unwrap();
    let paths = CaPaths::under(&home);
    Lab {
        ctx: Context {
            home,
            local_app_data: None,
            system_root,
            nested: false,
        },
        paths,
        scratch,
    }
}

/// The operator's environment as these tests make it up: no value of this machine's.
fn env() -> CaEnv {
    CaEnv {
        user_name: Some("someone".to_string()),
        computer_name: Some("EXAMPLEPC".to_string()),
        ..CaEnv::default()
    }
}

// ---- The fake host ----

fn ace(kind: AceKind, sid: &str, mask: u32, flags: u8) -> Ace {
    Ace {
        kind,
        sid: Some(sid.to_string()),
        mask,
        flags,
    }
}

fn owner_only(flags: u8) -> Rules {
    Rules {
        owner: ME.to_string(),
        protected: true,
        aces: Some(vec![ace(AceKind::Allow, ME, FULL_CONTROL, flags)]),
    }
}

/// What a default profile passes down: the user, SYSTEM and Administrators, inherited.
fn inherited() -> Rules {
    Rules {
        owner: ME.to_string(),
        protected: false,
        aces: Some(vec![
            ace(AceKind::Allow, SYSTEM, FULL_CONTROL, 0x10),
            ace(AceKind::Allow, ADMINS, FULL_CONTROL, 0x10),
            ace(AceKind::Allow, ME, FULL_CONTROL, 0x10),
        ]),
    }
}

/// A fake platform: rules kept in a map, and every lock and elevation answer set by the test.
struct Fake {
    elevated: Result<bool, String>,
    sid: Result<String, String>,
    user_name: Option<String>,
    rules: RefCell<HashMap<PathBuf, Result<Rules, String>>>,
    folder_lock: Result<(), String>,
    folder_after_lock: Rules,
    key_lock: Result<(), String>,
    key_after_lock: Rules,
    events: RefCell<Vec<String>>,
}

impl Fake {
    fn new() -> Fake {
        Fake {
            elevated: Ok(false),
            sid: Ok(ME.to_string()),
            user_name: Some("token-name".to_string()),
            rules: RefCell::new(HashMap::new()),
            folder_lock: Ok(()),
            folder_after_lock: owner_only(0x3),
            key_lock: Ok(()),
            key_after_lock: owner_only(0),
            events: RefCell::new(Vec::new()),
        }
    }

    fn set(&self, path: &Path, rules: Result<Rules, String>) {
        self.rules.borrow_mut().insert(path.to_path_buf(), rules);
    }

    fn events(&self) -> Vec<String> {
        self.events.borrow().clone()
    }
}

impl Host for Fake {
    fn user_sid(&self) -> Result<String, String> {
        self.sid.clone()
    }

    fn account_name(&self, sid: &str) -> Option<String> {
        match sid {
            SYSTEM => Some("NT AUTHORITY\\SYSTEM".to_string()),
            ADMINS => Some("BUILTIN\\Administrators".to_string()),
            EVERYONE => Some("Everyone".to_string()),
            ME => Some("EXAMPLE\\someone".to_string()),
            _ => None,
        }
    }

    fn user_name(&self) -> Option<String> {
        self.user_name.clone()
    }

    fn elevated(&self) -> Result<bool, String> {
        self.events.borrow_mut().push("elevated".to_string());
        self.elevated.clone()
    }

    fn rules(&self, path: &Path) -> Result<Rules, String> {
        if let Some(rules) = self.rules.borrow().get(path) {
            return rules.clone();
        }
        if path.exists() {
            Ok(inherited())
        } else {
            Err("The system cannot find the file specified.".to_string())
        }
    }

    fn lock_folder(&self, path: &Path, user_sid: &str) -> Result<(), String> {
        let files = fs::read_dir(path).map_or(0, |entries| entries.count());
        self.events
            .borrow_mut()
            .push(format!("lock_folder {user_sid} files={files}"));
        self.folder_lock.clone()?;
        self.set(path, Ok(self.folder_after_lock.clone()));
        Ok(())
    }

    fn lock_key(&self, path: &Path, user_sid: &str) -> Result<(), String> {
        self.events
            .borrow_mut()
            .push(format!("lock_key {user_sid} exists={}", path.is_file()));
        self.key_lock.clone()?;
        self.set(path, Ok(self.key_after_lock.clone()));
        Ok(())
    }

    fn local_date(&self) -> String {
        "2026-11-19".to_string()
    }
}

/// A `Say` that records each line; a person's command.
#[derive(Default)]
struct Lines(RefCell<Vec<String>>);

impl Say for Lines {
    fn mode(&self) -> Mode {
        Mode::Person
    }
    fn progress(&self, person: &str, caller: Option<CallerLine>) {
        assert!(caller.is_none(), "ca has no caller line");
        self.0.borrow_mut().push(person.to_string());
    }
    fn warning(&self, person: &str) {
        self.0.borrow_mut().push(format!("WARNING {person}"));
    }
    fn detail(&self, text: &str) {
        self.0.borrow_mut().push(format!("DETAIL {text}"));
    }
}

impl Lines {
    fn lines(&self) -> Vec<String> {
        self.0.borrow().clone()
    }
}

fn ca_with(lab: &Lab, env: &CaEnv, host: &dyn Host) -> (Result<String, Failure>, Vec<String>) {
    let say = Lines::default();
    let result = run(&lab.ctx, env, host, Some(&say));
    (result, say.lines())
}

fn sync_with(lab: &Lab, env: &CaEnv, host: &dyn Host) -> Result<String, Failure> {
    run(&lab.ctx, env, host, None)
}

fn message(result: Result<String, Failure>) -> String {
    // A run that passed has only a throwaway key's fingerprint to show.
    let failure = result.expect_err("the run fails");
    failure.message
}

// ---- What a failing test may print ----

/// `text` with `what` replaced by `with` wherever it stands, in any case.
fn replace_ignoring_case(text: &str, what: &str, with: &str) -> String {
    if what.is_empty() {
        return text.to_string();
    }
    // ASCII lower case keeps every byte offset, so the two texts line up.
    let (hay, needle) = (text.to_ascii_lowercase(), what.to_ascii_lowercase());
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while let Some(found) = hay[at..].find(&needle) {
        out.push_str(&text[at..at + found]);
        out.push_str(with);
        at += found + needle.len();
    }
    out.push_str(&text[at..]);
    out
}

/// `text` with whatever names this machine or its user replaced by a placeholder: the temp folder
/// and the home folder, as paths, then the computer's, the domain's and the user's names. The
/// values are read only to be hidden.
fn masked(text: &str) -> String {
    let variable = |name: &str| {
        std::env::var_os(name)
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    let temp = std::env::temp_dir().to_string_lossy().into_owned();
    let mut hidden = [
        (temp.trim_end_matches(['\\', '/']).to_string(), "<temp>"),
        (variable("USERPROFILE"), "<home>"),
        (variable("HOME"), "<home>"),
        (variable("COMPUTERNAME"), "<computer>"),
        (variable("HOSTNAME"), "<computer>"),
        (variable("USERDOMAIN"), "<domain>"),
        (variable("USERNAME"), "<user>"),
        (variable("USER"), "<user>"),
    ];
    hide(text, &mut hidden)
}

/// `text` with each of `hidden`'s values replaced by its placeholder, in any case.
fn hide(text: &str, hidden: &mut [(String, &str)]) -> String {
    // The longest first: one name may hold another (a user named for the computer, say), and the
    // shorter one replaced first would leave the rest of the longer one showing.
    hidden.sort_by_key(|(value, _)| std::cmp::Reverse(value.len()));
    let mut out = text.to_string();
    for (value, placeholder) in hidden {
        // A value of one or two characters would hide half the text and name nobody.
        if value.len() > 2 {
            out = replace_ignoring_case(&out, value, placeholder);
        }
    }
    out
}

/// Passes when the two texts are equal. A failure shows both only as `masked` leaves them.
#[track_caller]
fn same(actual: impl AsRef<str>, expected: impl AsRef<str>) {
    same_for(actual, expected, "");
}

/// `same`, with a word on which case failed.
#[track_caller]
fn same_for(actual: impl AsRef<str>, expected: impl AsRef<str>, what: impl std::fmt::Display) {
    let (actual, expected) = (actual.as_ref(), expected.as_ref());
    if actual == expected {
        return;
    }
    let what = match what.to_string() {
        what if what.is_empty() => what,
        what => format!("{what} "),
    };
    let (shown, wanted) = (masked(actual), masked(expected));
    if shown == wanted {
        panic!(
            "{what}The two texts differ only in what is hidden: a name or a path of this machine's."
        );
    }
    panic!("{what}The text is not the one expected.\n  Actual:   {shown}\n  Expected: {wanted}");
}

/// Passes when the two lists of lines are equal, and shows them only as `masked` leaves them.
#[track_caller]
fn same_lines(actual: &[String], expected: &[String]) {
    same(actual.join("\n"), expected.join("\n"));
    assert!(
        actual.len() == expected.len(),
        "The lines are split differently."
    );
}

/// The fingerprint of a run that passed. A failure's message is shown only as `masked` leaves it.
#[track_caller]
fn fine(result: Result<String, Failure>) -> String {
    match result {
        Ok(fingerprint) => fingerprint,
        Err(failure) => panic!("The run failed: {}", masked(&failure.message)),
    }
}

#[test]
fn masked_hides_the_temp_folder_in_any_case() {
    let temp = std::env::temp_dir().to_string_lossy().into_owned();
    let temp = temp.trim_end_matches(['\\', '/']);
    let text = format!(
        "{}/x is inside {}",
        temp.to_ascii_uppercase(),
        temp.to_ascii_lowercase()
    );
    let shown = masked(&text).to_ascii_lowercase();
    assert!(!shown.contains(&temp.to_ascii_lowercase()));
    assert!(shown.matches("<temp>").count() == 2);
    assert!(replace_ignoring_case("aXbxc", "x", "-") == "a-b-c");
    // A name that holds another is hidden whole, whatever the order they are given in.
    let mut names = [
        ("HOST".to_string(), "<computer>"),
        ("HOST-9".to_string(), "<user>"),
        ("ab".to_string(), "<short>"),
    ];
    assert!(hide(r"host\Host-9 has a tab", &mut names) == r"<computer>\<user> has a tab");
    assert!(replace_ignoring_case("abc", "", "-") == "abc");
}

fn d(path: &Path) -> String {
    path.display().to_string()
}

/// The check 2 message for `path`, with `<problems>`.
fn check2(lab: &Lab, path: &Path, problems: &str) -> String {
    let (folder, key) = (d(&lab.paths.folder), d(&lab.paths.key));
    format!(
        "agentspace: {} MUST be someone's alone, but it {problems}. ca does not change access rules on an existing CA. If anyone else may have read the key, replace the CA (D17). Otherwise lock it by hand: icacls \"{folder}\" /inheritance:r /grant:r \"someone:(OI)(CI)F\", then icacls \"{key}\" /reset, then icacls \"{key}\" /inheritance:d, then remove any other rule icacls still lists, and run ca again.",
        d(path)
    )
}

fn snapshot(lab: &Lab) -> (Option<Vec<u8>>, Option<Vec<u8>>) {
    (
        fs::read(&lab.paths.key).ok(),
        fs::read(&lab.paths.public).ok(),
    )
}

/// A lab with a sound CA made by `ca` against the fake host.
fn made() -> (Lab, Fake) {
    let lab = lab();
    let fake = Fake::new();
    let (result, _) = ca_with(&lab, &env(), &fake);
    fine(result);
    (lab, fake)
}

// ---- The environment ----

#[test]
fn from_lookup_gives_the_user_the_computer_and_the_three_one_drive_values() {
    let set: HashMap<&str, &str> = [
        ("USERNAME", "someone"),
        ("COMPUTERNAME", "EXAMPLEPC"),
        ("OneDriveCommercial", "third"),
        ("OneDrive", "first"),
        ("OneDriveConsumer", "second"),
        ("USERDOMAIN", "not read"),
    ]
    .into_iter()
    .collect();
    let env = CaEnv::from_lookup(|name| set.get(name).map(OsString::from));
    assert_eq!(env.user_name.as_deref(), Some("someone"));
    assert_eq!(env.computer_name.as_deref(), Some("EXAMPLEPC"));
    assert_eq!(env.one_drive, Some(OsString::from("first")));
    assert_eq!(env.one_drive_consumer, Some(OsString::from("second")));
    assert_eq!(env.one_drive_commercial, Some(OsString::from("third")));
    assert_eq!(
        env.one_drive_folders(),
        vec![
            OsString::from("first"),
            OsString::from("second"),
            OsString::from("third")
        ]
    );
    assert!(CaEnv::from_lookup(|_| None) == CaEnv::default());
}

#[test]
fn an_empty_value_is_the_same_as_an_unset_one() {
    let env = CaEnv {
        user_name: Some(String::new()),
        computer_name: Some(String::new()),
        one_drive: Some(OsString::new()),
        one_drive_consumer: Some(OsString::from("x")),
        one_drive_commercial: None,
    };
    assert!(env.user().is_none() && env.computer().is_none());
    assert_eq!(env.one_drive_folders(), vec![OsString::from("x")]);
}

#[test]
fn an_empty_user_name_falls_back_to_the_token_name() {
    let (lab, fake) = made();
    let mut env = env();
    env.user_name = Some(String::new());
    fake.set(&lab.paths.folder, Ok(inherited()));
    let text = message(sync_with(&lab, &env, &fake));
    assert!(
        text.contains(" MUST be token-name's alone, but it inherits"),
        "{}",
        masked(&text)
    );
}

#[test]
fn debug_says_which_values_are_set_and_never_what_they_are() {
    let env = CaEnv {
        user_name: Some("someone".to_string()),
        computer_name: Some(String::new()),
        one_drive: Some(OsString::from("onedrive-folder")),
        ..CaEnv::default()
    };
    let shown = format!("{env:?}");
    assert!(!shown.contains("someone") && !shown.contains("onedrive-folder"));
    assert!(shown.contains("user_name: \"set\""));
    assert!(shown.contains("computer_name: \"unset\""));
    assert!(shown.contains("one_drive: \"set\""));
    assert!(shown.contains("one_drive_consumer: \"unset\""));
}

// ---- Step 1 ----

#[test]
fn step_1_a_missing_ssh_keygen_fails_first_and_makes_nothing() {
    let lab = lab();
    let keygen = lab.ctx.system_root.join(r"System32\OpenSSH\ssh-keygen.exe");
    let keygen = if cfg!(windows) {
        keygen
    } else {
        lab.ctx
            .system_root
            .join("System32")
            .join("OpenSSH")
            .join("ssh-keygen.exe")
    };
    fs::remove_file(&keygen).unwrap();
    let fake = Fake::new();
    let expected = format!(
        "agentspace: {} is missing; install the Windows OpenSSH client.",
        d(&keygen)
    );
    let (result, lines) = ca_with(&lab, &env(), &fake);
    let failure = result.unwrap_err();
    same(failure.message, &expected);
    assert_eq!(failure.caller, CallerFailure::CouldNotReach);
    assert!(lines.is_empty());
    assert!(!lab.ctx.home.join(".agentspace").exists());
    same(message(sync_with(&lab, &env(), &fake)), &expected);

    // A folder of that name is missing too.
    fs::create_dir(&keygen).unwrap();
    same(message(ca_with(&lab, &env(), &fake).0), &expected);
}

// ---- Step 2 ----

#[test]
fn step_2_a_ca_folder_inside_a_one_drive_folder_is_refused() {
    let lab = lab();
    let fake = Fake::new();
    for position in 0..3 {
        let mut env = env();
        let mut values = vec![
            Some(OsString::from("unused-a")),
            Some(OsString::from("unused-b")),
        ];
        values.insert(position, Some(lab.ctx.home.clone().into()));
        env.one_drive = values[0].clone();
        env.one_drive_consumer = values[1].clone();
        env.one_drive_commercial = values[2].clone();
        let expected = format!(
            "agentspace: {} is inside {}; the CA key MUST NOT live in a synced folder or in the repo.",
            d(&lab.paths.folder),
            d(&lab.ctx.home)
        );
        let (result, lines) = ca_with(&lab, &env, &fake);
        same(message(result), &expected);
        assert!(lines.is_empty());
        same(message(sync_with(&lab, &env, &fake)), &expected);
    }
    assert!(!lab.ctx.home.join(".agentspace").exists());
    assert!(fake.events().is_empty());
}

#[cfg(windows)]
#[test]
fn step_2_one_drive_is_compared_without_regard_to_case() {
    let lab = lab();
    let fake = Fake::new();
    let mut env = env();
    let shouted = d(&lab.scratch.root).to_uppercase();
    env.one_drive_commercial = Some(OsString::from(&shouted));
    let expected = format!(
        "agentspace: {} is inside {shouted}; the CA key MUST NOT live in a synced folder or in the repo.",
        d(&lab.paths.folder)
    );
    same(message(ca_with(&lab, &env, &fake).0), &expected);
    assert!(!lab.ctx.home.join(".agentspace").exists());
}

#[test]
fn step_2_a_ca_folder_inside_a_git_working_tree_is_refused() {
    let fake = Fake::new();
    // A .git folder in the home, a .git file (a worktree's) above it, and one in the CA's folder.
    for case in 0..3 {
        let lab = lab();
        let (holder, as_file) = match case {
            0 => (lab.ctx.home.clone(), false),
            1 => (lab.scratch.root.clone(), true),
            _ => (lab.paths.folder.clone(), false),
        };
        fs::create_dir_all(&holder).unwrap();
        if as_file {
            fs::write(holder.join(".git"), b"gitdir: elsewhere\n").unwrap();
        } else {
            fs::create_dir(holder.join(".git")).unwrap();
        }
        let expected = format!(
            "agentspace: {} is inside {}; the CA key MUST NOT live in a synced folder or in the repo.",
            d(&lab.paths.folder),
            d(&holder)
        );
        let (result, _) = ca_with(&lab, &env(), &fake);
        same_for(message(result), &expected, format!("Case {case}."));
        same(message(sync_with(&lab, &env(), &fake)), &expected);
        assert!(!lab.paths.key.exists() && !lab.paths.public.exists());
    }
    assert!(fake.events().is_empty());
}

#[test]
fn step_2_one_drive_is_named_before_a_git_working_tree() {
    let lab = lab();
    fs::create_dir(lab.ctx.home.join(".git")).unwrap();
    let mut env = env();
    env.one_drive = Some(lab.scratch.root.clone().into());
    let result = ca_with(&lab, &env, &Fake::new()).0;
    assert!(message(result).contains(&format!(" is inside {};", d(&lab.scratch.root))));
}

// ---- Step 3 ----

#[test]
fn step_3_a_private_key_without_its_public_key_is_refused_and_not_repaired() {
    let (lab, fake) = made();
    fs::remove_file(&lab.paths.public).unwrap();
    let before = snapshot(&lab);
    let expected = format!(
        "agentspace: {} exists but {} does not. ca does not repair a half-made CA: restore the public key, or write it again from the private key with ssh-keygen -y, then run ca again.",
        d(&lab.paths.key),
        d(&lab.paths.public)
    );
    let (result, lines) = ca_with(&lab, &env(), &fake);
    same(message(result), &expected);
    assert!(lines.is_empty());
    same(message(sync_with(&lab, &env(), &fake)), &expected);
    assert_eq!(snapshot(&lab), before);
}

#[test]
fn step_3_a_public_key_without_its_private_key_is_refused_and_no_ca_is_made() {
    let (lab, fake) = made();
    fs::remove_file(&lab.paths.key).unwrap();
    let before = snapshot(&lab);
    let (key, public) = (d(&lab.paths.key), d(&lab.paths.public));
    let expected = format!(
        "agentspace: {public} exists but {key} does not: the CA's private key is lost, and nothing more can be signed with it. ca does not replace it on its own. To make a new CA, delete {public} and run ca again, then run sync for every host and re-issue every lease's certificate."
    );
    let (result, lines) = ca_with(&lab, &env(), &fake);
    same(message(result), &expected);
    assert!(lines.is_empty());
    same(message(sync_with(&lab, &env(), &fake)), &expected);
    assert_eq!(snapshot(&lab), before);
}

// ---- Step 4 ----

const ELEVATED: &str = "agentspace: ca MUST NOT create the CA from an elevated shell; run it without \"Run as administrator\".";

#[test]
fn step_4_an_elevated_ca_creates_nothing() {
    let lab = lab();
    for answer in [Ok(true), Err("Access is denied.".to_string())] {
        let mut fake = Fake::new();
        fake.elevated = answer;
        let (result, lines) = ca_with(&lab, &env(), &fake);
        same(message(result), ELEVATED);
        assert!(lines.is_empty(), "nothing is printed before the refusal");
        assert!(!lab.ctx.home.join(".agentspace").exists());
        assert_eq!(fake.events(), vec!["elevated".to_string()]);
    }
}

#[test]
fn step_4_ca_creates_the_ca_and_prints_its_four_lines() {
    let lab = lab();
    let fake = Fake::new();
    let (result, lines) = ca_with(&lab, &env(), &fake);
    let fingerprint = fine(result);
    let public = fs::read(&lab.paths.public).unwrap();
    assert_eq!(
        crate::keys::fingerprint(&public).as_deref(),
        Some(fingerprint.as_str())
    );
    assert!(fingerprint.starts_with("SHA256:"));
    same_lines(
        &lines,
        &[
            format!(
                "Creating the agentspace SSH CA in {}...",
                d(&lab.paths.folder)
            ),
            "Created the agentspace SSH CA.".to_string(),
            format!("Private key  {}", d(&lab.paths.key)),
            format!("Public key   {}", d(&lab.paths.public)),
            format!("Fingerprint  {fingerprint}"),
        ],
    );
    // The folder is locked while it is empty, before any key exists in it; the key once it does.
    assert_eq!(
        fake.events(),
        vec![
            "elevated".to_string(),
            format!("lock_folder {ME} files=0"),
            format!("lock_key {ME} exists=true"),
        ]
    );
    // An ed25519 pair, no passphrase, with state.md's comment.
    let text = String::from_utf8(public).unwrap();
    assert!(text.starts_with("ssh-ed25519 "));
    assert!(text.ends_with(" agentspace-ca EXAMPLEPC 2026-11-19\n"));
    let private = fs::read(&lab.paths.key).unwrap();
    agentspace_credential::check_unencrypted(&private).expect("no passphrase");
    assert!(agentspace_credential::check_ca_keys(&private, text.as_bytes()).is_ok());
    assert_eq!(fs::read_dir(&lab.paths.folder).unwrap().count(), 2);
}

#[test]
fn step_4_the_comment_drops_a_computer_name_that_is_not_set() {
    let lab = lab();
    let mut env = env();
    env.computer_name = None;
    fine(ca_with(&lab, &env, &Fake::new()).0);
    let text = fs::read_to_string(&lab.paths.public).unwrap();
    assert!(text.ends_with(" agentspace-ca 2026-11-19\n"));
}

#[test]
fn step_4_a_folder_that_cannot_be_locked_gets_no_key() {
    let lab = lab();
    let mut fake = Fake::new();
    fake.folder_lock = Err("Access is denied.".to_string());
    let (result, lines) = ca_with(&lab, &env(), &fake);
    same(
        message(result),
        format!(
            "agentspace: icacls could not lock {} to someone: Access is denied.",
            d(&lab.paths.folder)
        ),
    );
    assert_eq!(lines.len(), 1, "only the Creating line");
    assert!(!lab.paths.key.exists() && !lab.paths.public.exists());
}

#[test]
fn step_4_a_user_sid_that_cannot_be_read_fails_the_lock() {
    let lab = lab();
    let mut fake = Fake::new();
    fake.sid = Err("The token cannot be read.".to_string());
    same(
        message(ca_with(&lab, &env(), &fake).0),
        format!(
            "agentspace: icacls could not lock {} to someone: The token cannot be read.",
            d(&lab.paths.folder)
        ),
    );
    assert!(!lab.paths.key.exists());
}

#[test]
fn step_4_a_folder_still_open_to_others_gets_no_key() {
    let lab = lab();
    let mut fake = Fake::new();
    let mut open = owner_only(0x3);
    open.aces
        .as_mut()
        .unwrap()
        .push(ace(AceKind::Allow, EVERYONE, 0x0012_0089, 0));
    fake.folder_after_lock = open;
    same(
        message(ca_with(&lab, &env(), &fake).0),
        format!(
            "agentspace: {} MUST be someone's alone before the CA is made in it, but it has a rule allowing Everyone; has 2 access rules instead of one. Remove the other rules, or delete the directory, and run ca again.",
            d(&lab.paths.folder)
        ),
    );
    assert!(!lab.paths.key.exists() && !lab.paths.public.exists());
}

#[test]
fn step_4_a_key_that_cannot_be_written_fails_and_leaves_nothing_made() {
    let lab = lab();
    // A folder where the private key goes: not a key, so not a CA, and it is not removed.
    fs::create_dir_all(lab.paths.key.join("inside")).unwrap();
    let result = ca_with(&lab, &env(), &Fake::new()).0;
    let text = message(result);
    assert!(
        text.starts_with("agentspace: ssh-keygen could not create the CA: "),
        "{}",
        masked(&text)
    );
    assert!(lab.paths.key.join("inside").is_dir());
    assert!(!lab.paths.public.exists());
}

#[test]
fn step_4_a_key_that_cannot_be_locked_is_left_for_a_person() {
    let lab = lab();
    let mut fake = Fake::new();
    fake.key_lock = Err("Access is denied.".to_string());
    let (key, public) = (d(&lab.paths.key), d(&lab.paths.public));
    same(
        message(ca_with(&lab, &env(), &fake).0),
        format!(
            "agentspace: icacls could not lock the new {key} to someone: Access is denied.. Delete {key} and {public}, and run ca again."
        ),
    );
    assert!(lab.paths.key.is_file() && lab.paths.public.is_file());
}

#[test]
fn step_4_a_key_still_open_to_others_is_treated_as_exposed() {
    let lab = lab();
    let mut fake = Fake::new();
    fake.key_after_lock = Rules {
        owner: ADMINS.to_string(),
        protected: true,
        aces: Some(vec![ace(AceKind::Allow, ME, FULL_CONTROL, 0)]),
    };
    let (key, public) = (d(&lab.paths.key), d(&lab.paths.public));
    same(
        message(ca_with(&lab, &env(), &fake).0),
        format!(
            "agentspace: The new {key} MUST be someone's alone, but it is owned by BUILTIN\\Administrators. Treat it as exposed: delete {key} and {public}, and run ca again."
        ),
    );
}

#[test]
fn step_4_sync_never_creates_a_ca() {
    let lab = lab();
    let fake = Fake::new();
    same(
        message(sync_with(&lab, &env(), &fake)),
        format!("agentspace: {} does not exist; run ca.", d(&lab.paths.key)),
    );
    assert!(!lab.ctx.home.join(".agentspace").exists());
    assert!(fake.events().is_empty());
}

// ---- Step 5 ----

#[test]
fn step_5_a_sound_ca_is_checked_and_nothing_changes() {
    let (lab, fake) = made();
    let before = snapshot(&lab);
    let events = fake.events().len();
    let (result, lines) = ca_with(&lab, &env(), &fake);
    let fingerprint = fine(result);
    same_lines(
        &lines,
        &[
            "The agentspace SSH CA already exists; nothing changed.".to_string(),
            format!("Private key  {}", d(&lab.paths.key)),
            format!("Public key   {}", d(&lab.paths.public)),
            format!("Fingerprint  {fingerprint}"),
        ],
    );
    assert_eq!(snapshot(&lab), before);
    assert_eq!(fake.events().len(), events, "no lock and no elevation test");
    assert_eq!(fine(sync_with(&lab, &env(), &fake)), fingerprint);
    assert_eq!(fake.events().len(), events);
}

#[test]
fn step_5_check_2_a_loosened_folder_is_refused_first() {
    let (lab, fake) = made();
    let mut loose = owner_only(0x3);
    loose.protected = false;
    loose
        .aces
        .as_mut()
        .unwrap()
        .push(ace(AceKind::Allow, SYSTEM, FULL_CONTROL, 0x13));
    fake.set(&lab.paths.folder, Ok(loose));
    let mut key = owner_only(0);
    key.owner = ADMINS.to_string();
    fake.set(&lab.paths.key, Ok(key));
    let expected = check2(
        &lab,
        &lab.paths.folder,
        "inherits access rules from its parent; has a rule allowing NT AUTHORITY\\SYSTEM; has 2 access rules instead of one",
    );
    let before = snapshot(&lab);
    same(message(ca_with(&lab, &env(), &fake).0), &expected);
    same(message(sync_with(&lab, &env(), &fake)), &expected);
    assert_eq!(snapshot(&lab), before);
}

#[test]
fn step_5_check_2_a_loosened_key_is_refused() {
    let (lab, fake) = made();
    let key = Rules {
        owner: ADMINS.to_string(),
        protected: false,
        aces: Some(vec![
            ace(AceKind::Deny, "S-1-5-21-9-9-9-500", 0x1, 0),
            ace(AceKind::Allow, ME, 0x0012_00A9, 0),
            ace(AceKind::Allow, EVERYONE, 0x0012_0089, 0x10),
        ]),
    };
    fake.set(&lab.paths.key, Ok(key));
    let expected = check2(
        &lab,
        &lab.paths.key,
        "is owned by BUILTIN\\Administrators; inherits access rules from its parent; has a rule denying S-1-5-21-9-9-9-500; gives someone ReadAndExecute, Synchronize, not full control; has a rule allowing Everyone; has 3 access rules instead of one",
    );
    same(message(ca_with(&lab, &env(), &fake).0), &expected);
    same(message(sync_with(&lab, &env(), &fake)), &expected);
}

#[test]
fn step_5_check_2_rules_that_cannot_be_read_fail_closed() {
    let (lab, fake) = made();
    fake.set(&lab.paths.key, Err("Access is denied.".to_string()));
    same(
        message(sync_with(&lab, &env(), &fake)),
        check2(
            &lab,
            &lab.paths.key,
            "could not be checked: Access is denied.",
        ),
    );
}

#[test]
fn step_5_user_is_the_token_name_when_username_is_not_set() {
    let (lab, fake) = made();
    let mut env = env();
    env.user_name = None;
    fake.set(&lab.paths.folder, Ok(inherited()));
    let text = message(sync_with(&lab, &env, &fake));
    assert!(
        text.contains(" MUST be token-name's alone, but it inherits"),
        "{}",
        masked(&text)
    );
    assert!(text.contains("/grant:r \"token-name:(OI)(CI)F\""));
}

#[test]
fn step_5_check_3_a_key_that_does_not_load_is_refused() {
    let (lab, fake) = made();
    fs::write(&lab.paths.key, b"not a key").unwrap();
    let expected = format!(
        "agentspace: ssh-keygen could not read {} without a passphrase; the lease CLI signs unattended (D17).",
        d(&lab.paths.key)
    );
    same(message(ca_with(&lab, &env(), &fake).0), &expected);
    same(message(sync_with(&lab, &env(), &fake)), &expected);
    assert_eq!(fs::read(&lab.paths.key).unwrap(), b"not a key");
}

#[test]
fn step_5_check_4_a_mismatched_pair_is_refused_and_not_repaired() {
    let (lab, fake) = made();
    let other = crate::keys::new_ca("other").unwrap();
    fs::write(&lab.paths.public, other.public.as_bytes()).unwrap();
    let before = snapshot(&lab);
    let expected = format!(
        "agentspace: {} is not the ed25519 public key of {}. ca does not repair a mismatched pair.",
        d(&lab.paths.public),
        d(&lab.paths.key)
    );
    same(message(ca_with(&lab, &env(), &fake).0), &expected);
    same(message(sync_with(&lab, &env(), &fake)), &expected);
    assert_eq!(snapshot(&lab), before);
}

#[test]
fn step_5_check_5_a_public_key_that_does_not_parse_is_refused() {
    let (lab, fake) = made();
    fs::write(&lab.paths.public, b"ssh-ed25519").unwrap();
    let expected = format!(
        "agentspace: ssh-keygen could not read {}: It is not an OpenSSH public key.",
        d(&lab.paths.public)
    );
    same(message(ca_with(&lab, &env(), &fake).0), &expected);
    same(message(sync_with(&lab, &env(), &fake)), &expected);
}

#[test]
fn utc_dates_are_civil() {
    use std::time::{Duration, UNIX_EPOCH};
    assert_eq!(crate::utc_date(UNIX_EPOCH), "1970-01-01");
    assert_eq!(
        crate::utc_date(UNIX_EPOCH + Duration::from_secs(1_795_000_000)),
        "2026-11-18"
    );
    assert_eq!(
        crate::utc_date(UNIX_EPOCH + Duration::from_secs(951_782_400)),
        "2000-02-29"
    );
}

// ---- The real Windows API, in throwaway folders ----

#[cfg(windows)]
mod windows {
    use super::*;
    use crate::platform::Windows;
    use crate::platform::testing::set_sddl;

    fn me() -> String {
        Windows.user_sid().expect("the user's SID")
    }

    /// Skips a test that makes a CA when the test itself runs elevated, where `ca` refuses.
    fn not_elevated() -> bool {
        let elevated = Windows.elevated().expect("the token's elevation");
        if elevated {
            eprintln!("Skipped: the tests run elevated, and ca refuses to create a CA there.");
        }
        !elevated
    }

    fn made_for_real() -> Option<Lab> {
        if !not_elevated() {
            return None;
        }
        let lab = lab();
        fine(run(&lab.ctx, &env(), &Windows, Some(&Lines::default())));
        Some(lab)
    }

    fn everyone() -> String {
        Windows
            .account_name(EVERYONE)
            .unwrap_or_else(|| EVERYONE.to_string())
    }

    #[test]
    fn the_user_and_accounts_are_named() {
        assert!(me().starts_with("S-1-5-"));
        let system = Windows.account_name(SYSTEM).expect("SYSTEM is named");
        assert!(system.contains('\\'), "domain and name");
        assert!(Windows.account_name("S-1-5-21-1-2-3-4242").is_none());
        let date = Windows.local_date();
        assert_eq!(date.len(), 10);
        assert_eq!(&date[4..5], "-");
    }

    #[test]
    fn ca_locks_the_folder_and_the_key_owner_only() {
        let Some(lab) = made_for_real() else { return };
        let me = me();
        let folder = Windows.rules(&lab.paths.folder).unwrap();
        assert!(folder.owner == me, "the folder's owner is the user");
        assert!(folder.protected);
        let aces = folder.aces.unwrap();
        assert_eq!(aces.len(), 1);
        assert_eq!(aces[0].kind, AceKind::Allow);
        assert!(
            aces[0].sid.as_deref() == Some(me.as_str()),
            "the entry is the user's"
        );
        assert_eq!(aces[0].mask & FULL_CONTROL, FULL_CONTROL);
        assert_eq!(aces[0].flags & 0x3, 0x3, "inherited by files and folders");
        assert_eq!(aces[0].flags & 0x10, 0, "explicit");

        let key = Windows.rules(&lab.paths.key).unwrap();
        assert!(key.owner == me, "the key's owner is the user");
        assert!(key.protected);
        let aces = key.aces.unwrap();
        assert_eq!(aces.len(), 1);
        assert!(
            aces[0].sid.as_deref() == Some(me.as_str()),
            "the entry is the user's"
        );
        assert_eq!(aces[0].mask & FULL_CONTROL, FULL_CONTROL);
        assert_eq!(aces[0].flags, 0, "one explicit entry");
        // The lease CLI's own check agrees.
        agentspace_credential::check_ca(&lab.ctx.home).expect("the lease CLI finds it usable");
    }

    /// Runs Windows OpenSSH's own `ssh-keygen.exe`, the oracle that
    /// conformance/operator/test-ca.ps1 uses for the key pair (D34), and returns whether it
    /// exited 0 and what it wrote to stdout. `None` when the laptop has none. The child is this
    /// test's own, with a throwaway working folder and a throwaway home in its environment.
    fn ssh_keygen(lab: &Lab, args: &[&str]) -> Option<(bool, String)> {
        let exe = PathBuf::from(std::env::var_os("SystemRoot")?)
            .join("System32")
            .join("OpenSSH")
            .join("ssh-keygen.exe");
        if !exe.is_file() {
            return None;
        }
        let home = lab.scratch.root.join("keygen-home");
        fs::create_dir_all(&home).unwrap();
        let out = std::process::Command::new(exe)
            .args(args)
            .current_dir(&home)
            .env("USERPROFILE", &home)
            .env("HOME", &home)
            .env("LOCALAPPDATA", home.join("AppData").join("Local"))
            .env("APPDATA", home.join("AppData").join("Roaming"))
            .env("TEMP", &home)
            .env("TMP", &home)
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()
            .ok()?;
        Some((
            out.status.success(),
            String::from_utf8_lossy(&out.stdout).into_owned(),
        ))
    }

    #[test]
    fn windows_openssh_loads_the_key_ca_made_and_gives_the_same_fingerprint() {
        let Some(lab) = made_for_real() else { return };
        let (key, public) = (d(&lab.paths.key), d(&lab.paths.public));
        // The private key loads with an empty passphrase, under the rules `ca` locked it with
        // (ssh-keygen refuses a key that others can read), and its public key is the stored one.
        let Some((loaded, derived)) = ssh_keygen(&lab, &["-y", "-P", "", "-f", &key]) else {
            eprintln!("Skipped: this machine has no Windows OpenSSH ssh-keygen.exe.");
            return;
        };
        assert!(
            loaded,
            "ssh-keygen loads the private key with no passphrase"
        );
        let stored = fs::read_to_string(&lab.paths.public).unwrap();
        let derived: Vec<&str> = derived.split_whitespace().collect();
        let stored: Vec<&str> = stored.split_whitespace().collect();
        assert!(
            derived.len() >= 2 && derived[0] == "ssh-ed25519" && derived[1] == stored[1],
            "the stored public key is the one ssh-keygen derives, and ed25519"
        );
        // The fingerprint `ca` and `sync` give is the one ssh-keygen prints.
        let (listed, line) = ssh_keygen(&lab, &["-l", "-f", &public]).unwrap();
        assert!(listed, "ssh-keygen reads the public key's fingerprint");
        let fingerprint = fine(run(&lab.ctx, &env(), &Windows, None));
        assert!(
            line.split_whitespace().nth(1) == Some(fingerprint.as_str()),
            "the fingerprint is ssh-keygen's own"
        );
    }

    /// conformance/operator/test-ca.ps1's half-made and mismatched cases, in its order: each file
    /// is moved aside and moved back (a copy would get new access rules), nothing is repaired,
    /// and with the pair put back by hand `ca` passes again.
    #[test]
    fn a_pair_put_back_by_hand_passes_again() {
        let Some(lab) = made_for_real() else { return };
        let aside = lab.scratch.root.join("aside");
        fs::create_dir(&aside).unwrap();
        let (key_aside, public_aside) = (aside.join("agentspace_ca"), aside.join("pub"));
        let before = snapshot(&lab);
        let ca = || run(&lab.ctx, &env(), &Windows, Some(&Lines::default()));

        fs::rename(&lab.paths.public, &public_aside).unwrap();
        assert!(message(ca()).contains(" does not repair a half-made CA: "));
        assert!(!lab.paths.public.exists(), "no public key is made again");
        fs::rename(&public_aside, &lab.paths.public).unwrap();

        fs::rename(&lab.paths.key, &key_aside).unwrap();
        assert!(message(ca()).contains(": the CA's private key is lost, "));
        assert!(!lab.paths.key.exists(), "no CA is made over the public key");
        fs::rename(&key_aside, &lab.paths.key).unwrap();

        fs::rename(&lab.paths.public, &public_aside).unwrap();
        let other = crate::keys::new_ca("other").unwrap();
        fs::write(&lab.paths.public, other.public.as_bytes()).unwrap();
        assert!(message(ca()).contains(" does not repair a mismatched pair."));
        assert!(fs::read(&lab.paths.public).unwrap() == other.public.as_bytes());
        fs::remove_file(&lab.paths.public).unwrap();
        fs::rename(&public_aside, &lab.paths.public).unwrap();

        let say = Lines::default();
        fine(run(&lab.ctx, &env(), &Windows, Some(&say)));
        same(
            &say.lines()[0],
            "The agentspace SSH CA already exists; nothing changed.",
        );
        assert!(snapshot(&lab) == before, "both files are as ca made them");
    }

    #[test]
    fn a_second_ca_changes_nothing() {
        let Some(lab) = made_for_real() else { return };
        let before = snapshot(&lab);
        let times = (
            fs::metadata(&lab.paths.key).unwrap().modified().unwrap(),
            fs::metadata(&lab.paths.public).unwrap().modified().unwrap(),
        );
        let rules = (
            Windows.rules(&lab.paths.folder).unwrap(),
            Windows.rules(&lab.paths.key).unwrap(),
        );
        let say = Lines::default();
        let fingerprint = fine(run(&lab.ctx, &env(), &Windows, Some(&say)));
        same(
            &say.lines()[0],
            "The agentspace SSH CA already exists; nothing changed.",
        );
        assert_eq!(snapshot(&lab), before);
        assert_eq!(
            times,
            (
                fs::metadata(&lab.paths.key).unwrap().modified().unwrap(),
                fs::metadata(&lab.paths.public).unwrap().modified().unwrap(),
            )
        );
        let rules_after = (
            Windows.rules(&lab.paths.folder).unwrap(),
            Windows.rules(&lab.paths.key).unwrap(),
        );
        assert!(rules == rules_after, "no access rule changed");
        assert_eq!(fine(run(&lab.ctx, &env(), &Windows, None)), fingerprint);
    }

    #[test]
    fn a_loosened_folder_is_refused_and_left_as_it_is() {
        let Some(lab) = made_for_real() else { return };
        let me = me();
        assert!(set_sddl(
            &lab.paths.folder,
            &format!("D:P(A;OICI;FA;;;{me})(A;;FR;;;WD)")
        ));
        let before = Windows.rules(&lab.paths.folder).unwrap();
        let expected = check2(
            &lab,
            &lab.paths.folder,
            &format!(
                "has a rule allowing {}; has 2 access rules instead of one",
                everyone()
            ),
        );
        let say = Lines::default();
        same(
            message(run(&lab.ctx, &env(), &Windows, Some(&say))),
            &expected,
        );
        assert!(say.lines().is_empty());
        same(message(run(&lab.ctx, &env(), &Windows, None)), &expected);
        assert!(
            Windows.rules(&lab.paths.folder).unwrap() == before,
            "the folder's rules are left as they were"
        );
    }

    #[test]
    fn a_loosened_key_is_refused_and_left_as_it_is() {
        let Some(lab) = made_for_real() else { return };
        let me = me();
        assert!(set_sddl(
            &lab.paths.key,
            &format!("D:P(D;;WD;;;WD)(A;;FA;;;{me})(A;;FR;;;WD)")
        ));
        let before = Windows.rules(&lab.paths.key).unwrap();
        let who = everyone();
        let expected = check2(
            &lab,
            &lab.paths.key,
            &format!(
                "has a rule denying {who}; has a rule allowing {who}; has 3 access rules instead of one"
            ),
        );
        same(message(run(&lab.ctx, &env(), &Windows, None)), &expected);
        same(
            message(run(&lab.ctx, &env(), &Windows, Some(&Lines::default()))),
            &expected,
        );
        assert!(
            Windows.rules(&lab.paths.key).unwrap() == before,
            "the key's rules are left as they were"
        );
    }

    #[test]
    fn a_key_that_inherits_or_is_short_of_full_control_is_refused() {
        let Some(lab) = made_for_real() else { return };
        let me = me();
        // Not protected: the folder's entry comes down as well as the key's own.
        assert!(set_sddl(&lab.paths.key, &format!("D:(A;;FA;;;{me})")));
        let text = message(run(&lab.ctx, &env(), &Windows, None));
        assert!(
            text.contains(
                "but it inherits access rules from its parent; has 2 access rules instead of one."
            ),
            "{}",
            masked(&text).replace(&me, "<sid>")
        );
        // The user's own rule, short of full control.
        assert!(set_sddl(&lab.paths.key, &format!("D:P(A;;FR;;;{me})")));
        let text = message(run(&lab.ctx, &env(), &Windows, None));
        assert!(text.contains("but it gives someone "), "{}", masked(&text));
        assert!(
            text.contains(", not full control. ca does not change"),
            "{}",
            masked(&text)
        );
    }

    #[test]
    fn an_existing_folder_open_to_others_is_locked_to_the_user_alone() {
        if !not_elevated() {
            return;
        }
        let lab = lab();
        let me = me();
        fs::create_dir_all(&lab.paths.folder).unwrap();
        // An explicit rule of someone else's, a deny, and a rule of the user's short of full
        // control, on top of what the folder inherits.
        assert!(set_sddl(
            &lab.paths.folder,
            &format!("D:(D;;WD;;;BG)(A;;FR;;;{me})(A;;FR;;;WD)")
        ));
        let say = Lines::default();
        fine(run(&lab.ctx, &env(), &Windows, Some(&say)));
        same(&say.lines()[1], "Created the agentspace SSH CA.");
        let rules = Windows.rules(&lab.paths.folder).unwrap();
        assert!(rules.protected);
        let aces = rules.aces.unwrap();
        assert_eq!(aces.len(), 1, "every other rule is gone");
        assert!(
            aces[0].sid.as_deref() == Some(me.as_str()),
            "the entry is the user's"
        );
        assert_eq!(aces[0].mask & FULL_CONTROL, FULL_CONTROL);
        assert_eq!(aces[0].flags, 0x3);
    }

    #[test]
    fn a_rule_of_the_users_with_a_condition_is_not_owner_only_as_the_lease_cli_reads_it() {
        let Some(lab) = made_for_real() else { return };
        let me = me();
        // One entry, for the user, full control, protected: but an allow with a condition, which
        // the lease CLI's check does not take for the user's own rule. `ca` MUST agree with it.
        assert!(set_sddl(
            &lab.paths.key,
            &format!("D:P(XA;;FA;;;{me};(Member_of {{SID(BA)}}))")
        ));
        let rules = Windows.rules(&lab.paths.key).unwrap();
        let aces = rules.aces.unwrap();
        assert_eq!(aces.len(), 1);
        assert_eq!(aces[0].kind, AceKind::Other);
        assert!(
            aces[0].sid.as_deref() == Some(me.as_str()),
            "the entry names the user"
        );
        let who = Windows.account_name(&me).unwrap_or_else(|| me.clone());
        let expected = check2(&lab, &lab.paths.key, &format!("has a rule allowing {who}"));
        same(message(run(&lab.ctx, &env(), &Windows, None)), &expected);
        same(
            message(run(&lab.ctx, &env(), &Windows, Some(&Lines::default()))),
            &expected,
        );
        assert!(agentspace_credential::check_ca(&lab.ctx.home).is_err());
    }

    #[test]
    fn a_public_key_that_cannot_be_read_does_not_hide_the_private_keys_failure() {
        let Some(lab) = made_for_real() else { return };
        let me = me();
        // The user may write the public key and not read it: the file is there, its bytes are not.
        assert!(set_sddl(&lab.paths.public, &format!("D:P(A;;FW;;;{me})")));
        assert!(fs::read(&lab.paths.public).is_err());
        // Check 5, with the system's own error text, while the private key is sound.
        let text = message(run(&lab.ctx, &env(), &Windows, None));
        let head = format!(
            "agentspace: ssh-keygen could not read {}: ",
            d(&lab.paths.public)
        );
        assert!(
            text.starts_with(&head) && text.len() > head.len(),
            "{}",
            masked(&text)
        );
        // Check 3 comes before it when the private key does not load.
        fs::write(&lab.paths.key, b"not a key").unwrap();
        let expected = format!(
            "agentspace: ssh-keygen could not read {} without a passphrase; the lease CLI signs unattended (D17).",
            d(&lab.paths.key)
        );
        same(message(run(&lab.ctx, &env(), &Windows, None)), &expected);
        same(
            message(run(&lab.ctx, &env(), &Windows, Some(&Lines::default()))),
            &expected,
        );
    }

    #[test]
    fn the_test_is_not_elevated_or_the_refusal_holds() {
        let lab = lab();
        let result = run(&lab.ctx, &env(), &Windows, Some(&Lines::default()));
        if Windows.elevated().unwrap() {
            same(message(result), ELEVATED);
        } else {
            fine(result);
        }
    }
}
