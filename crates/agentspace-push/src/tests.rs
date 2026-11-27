//! The whole push with stand-ins for the files, the decryption, the ssh and `Say`.

use super::*;
use agentspace_operator_api::{CallerLine, Mode};
use std::cell::RefCell;
use std::collections::HashMap;

const DUMMY_TOKEN: &str = "PLANTED-dummy_Zk3Qp9xL2mVb7RtYw5NcHs8DfGa1J";
const OTHER_TOKEN: &str = "ANOTHER-dummy_Aa1Bb2Cc3Dd4Ee5Ff6Gg7Hh8Ii9Jj";
// A token is 43 characters of base64url.
const _: () = assert!(DUMMY_TOKEN.len() == 43 && OTHER_TOKEN.len() == 43);
const URL: &str = "http://127.0.0.1:8811";
const ID: &str = "i-0123456789abcdef0";
const ZONE: &str = "America/New_York";

/// A `Say` that records what a real one would print, rewriting warnings in caller mode as the pin
/// says.
struct Recorder {
    mode: Mode,
    lines: RefCell<Vec<String>>,
}

impl Recorder {
    fn new(mode: Mode) -> Self {
        Recorder {
            mode,
            lines: RefCell::new(Vec::new()),
        }
    }
    fn lines(&self) -> Vec<String> {
        self.lines.borrow().clone()
    }
}

impl Say for Recorder {
    fn mode(&self) -> Mode {
        self.mode
    }
    fn progress(&self, person: &str, caller: Option<CallerLine>) {
        match self.mode {
            Mode::Person => self.lines.borrow_mut().push(person.to_owned()),
            Mode::Caller => {
                if let Some(c) = caller {
                    self.lines.borrow_mut().push(format!("{c:?}"));
                }
            }
        }
    }
    fn warning(&self, person: &str) {
        let text = match self.mode {
            Mode::Person => person.to_owned(),
            Mode::Caller => person
                .replace("the instance", "the host")
                .replace("The instance", "The host"),
        };
        self.lines.borrow_mut().push(text);
    }
    fn detail(&self, text: &str) {
        self.lines.borrow_mut().push(format!("detail: {text}"));
    }
}

#[derive(Default)]
struct Disk {
    files: RefCell<HashMap<PathBuf, FileRead>>,
    reads: RefCell<Vec<PathBuf>>,
    writes: RefCell<Vec<(PathBuf, Vec<u8>)>>,
    refuse_writes: bool,
}

impl Files for Disk {
    fn read(&self, path: &Path) -> FileRead {
        self.reads.borrow_mut().push(path.to_owned());
        self.files
            .borrow()
            .get(path)
            .cloned()
            .unwrap_or(FileRead::Missing)
    }
    fn write_stamp(&self, path: &Path, bytes: &[u8]) -> bool {
        if self.refuse_writes {
            return false;
        }
        self.writes
            .borrow_mut()
            .push((path.to_owned(), bytes.to_vec()));
        self.files
            .borrow_mut()
            .insert(path.to_owned(), FileRead::Bytes(bytes.to_vec()));
        true
    }
}

/// The stand-in "encryption" is a fixed prefix on the plaintext; any other blob fails.
struct FakeDpapi;
const PREFIX: [u8; 4] = [1, 0, 0, 0];

impl Unprotect for FakeDpapi {
    fn unprotect(&self, blob: &[u8]) -> Option<Zeroizing<Vec<u8>>> {
        blob.strip_prefix(&PREFIX)
            .map(|p| Zeroizing::new(p.to_vec()))
    }
}

/// How the stand-in box answers.
#[derive(Clone, Copy)]
enum Reply {
    /// As today's box: the hash of what it was sent, then the zone line when one was named.
    Honest,
    ZoneFailed,
    Exit(i32),
    WrongHash,
    TimedOut,
    CouldNotStart,
}

struct FakeSsh {
    answer: Reply,
    calls: RefCell<Vec<(Vec<String>, Vec<u8>)>>,
}

impl FakeSsh {
    fn new(answer: Reply) -> Self {
        FakeSsh {
            answer,
            calls: RefCell::new(Vec::new()),
        }
    }
}

impl SshRun for FakeSsh {
    fn run(&self, args: &[&str], stdin: &[u8]) -> SshResult {
        self.calls
            .borrow_mut()
            .push((args.iter().map(|s| s.to_string()).collect(), stdin.to_vec()));
        let hash = stamp::box_hash(&Content(Zeroizing::new(stdin.to_vec())));
        let command = args.last().copied().unwrap_or_default();
        let zone = command
            .split("set-timezone ")
            .nth(1)
            .and_then(|r| r.split(' ').next());
        let mut out = format!("Welcome to the box\r\n{hash}  -\n");
        match self.answer {
            Reply::CouldNotStart => return SshResult::CouldNotStart,
            Reply::TimedOut => return SshResult::TimedOut,
            Reply::Exit(code) => {
                return SshResult::Exited {
                    code,
                    stdout: Vec::new(),
                };
            }
            Reply::WrongHash => out = format!("{}  -\n", "0".repeat(64)),
            Reply::ZoneFailed => {
                if zone.is_some() {
                    out.push_str("zone-failed\n");
                }
            }
            Reply::Honest => {
                if let Some(z) = zone {
                    out.push_str(&format!("zone {z}\n"));
                }
            }
        }
        SshResult::Exited {
            code: 0,
            stdout: out.into_bytes(),
        }
    }
}

fn hex_of(blob: &[u8]) -> String {
    blob.iter().map(|b| format!("{b:02x}")).collect()
}

/// A good `bot.json` for `blob`: the four members of localswim's schema.
fn credential_json(blob: &[u8]) -> Vec<u8> {
    format!(
        r#"{{"format":"localswim-credential","format_version":1,"role":"bot","token_dpapi_hex":"{}"}}"#,
        hex_of(blob)
    )
    .into_bytes()
}

/// A `bot.json` of the member texts given, in order, each written as `"name":value`.
fn credential_with(members: &[&str]) -> Vec<u8> {
    format!("{{{}}}", members.join(",")).into_bytes()
}

fn blob_for(token: &str) -> Vec<u8> {
    let mut blob = PREFIX.to_vec();
    blob.extend(token.encode_utf16().flat_map(u16::to_le_bytes));
    blob
}

fn ctx() -> Context {
    Context {
        home: PathBuf::from("home"),
        local_app_data: Some(PathBuf::from("lad")),
        system_root: PathBuf::from("sys"),
        nested: false,
    }
}

fn laptop(token: &str) -> Disk {
    let disk = Disk::default();
    let receipt = format!(r#"{{"base":"{URL}","credential_directory":"creds/","exe":"x"}}"#);
    disk.files.borrow_mut().insert(
        receipt_path(&ctx()).unwrap(),
        FileRead::Bytes(receipt.into_bytes()),
    );
    disk.files.borrow_mut().insert(
        credential_path("creds/"),
        FileRead::Bytes(credential_json(&blob_for(token))),
    );
    disk
}

struct Run {
    outcome: Outcome,
    said: Vec<String>,
    ssh: Vec<(Vec<String>, Vec<u8>)>,
    writes: Vec<(PathBuf, Vec<u8>)>,
}

fn run_with(
    ctx: &Context,
    disk: &Disk,
    mode: Mode,
    answer: Reply,
    zone: Option<&str>,
    alias: &str,
    forced: bool,
) -> Run {
    let say = Recorder::new(mode);
    let ssh = FakeSsh::new(answer);
    let zone_fn = || zone.map(str::to_owned);
    let seams = Seams {
        files: disk,
        unprotect: &FakeDpapi,
        ssh: &ssh,
        zone: &zone_fn,
    };
    disk.writes.borrow_mut().clear();
    let outcome = push_with(ctx, &say, ID, alias, forced, &seams);
    Run {
        outcome,
        said: say.lines(),
        ssh: ssh.calls.into_inner(),
        writes: disk.writes.borrow().clone(),
    }
}

fn run(disk: &Disk, mode: Mode, answer: Reply) -> Run {
    run_with(&ctx(), disk, mode, answer, Some(ZONE), "agentspace", false)
}

fn expected_stamp(zone: &str) -> String {
    let c = content(Some(URL), &Token(Zeroizing::new(DUMMY_TOKEN.to_owned()))).unwrap();
    stamp(ID, zone, &c)
}

#[test]
fn a_first_push_sends_the_content_on_stdin_and_writes_the_stamp() {
    let disk = laptop(DUMMY_TOKEN);
    let r = run(&disk, Mode::Person, Reply::Honest);
    assert_eq!(r.outcome, Outcome::Pushed);
    assert_eq!(r.said, [SENDING]);
    assert_eq!(r.ssh.len(), 1);
    let (args, stdin) = &r.ssh[0];
    assert_eq!(
        &args[..5],
        [
            "-o",
            "ClearAllForwardings=yes",
            "-o",
            "BatchMode=yes",
            "agentspace"
        ]
    );
    assert_eq!(args[5], remote_command(ZONE));
    assert_eq!(
        stdin,
        format!("LOCALSWIM_URL={URL}\nLOCALSWIM_TOKEN={DUMMY_TOKEN}\n").as_bytes()
    );
    assert_eq!(
        r.writes,
        [(stamp_path(&ctx()), stamp_file_bytes(&expected_stamp(ZONE)))]
    );
    assert_eq!(
        disk.reads.borrow()[..2],
        [receipt_path(&ctx()).unwrap(), credential_path("creds/")],
        "The receipt, then the credential"
    );
}

#[test]
fn the_second_push_is_not_due_and_prints_nothing() {
    let disk = laptop(DUMMY_TOKEN);
    run(&disk, Mode::Person, Reply::Honest);
    let r = run(&disk, Mode::Person, Reply::Honest);
    assert_eq!(r.outcome, Outcome::NotDue);
    assert!(r.said.is_empty());
    assert!(r.ssh.is_empty());
    assert!(r.writes.is_empty());
}

#[test]
fn a_changed_instance_zone_or_token_makes_it_due() {
    let disk = laptop(DUMMY_TOKEN);
    run(&disk, Mode::Person, Reply::Honest);
    let other_zone = run_with(
        &ctx(),
        &disk,
        Mode::Person,
        Reply::Honest,
        Some("UTC"),
        "agentspace",
        false,
    );
    assert_eq!(other_zone.outcome, Outcome::Pushed);
    let disk2 = laptop(OTHER_TOKEN);
    disk2.files.borrow_mut().insert(
        stamp_path(&ctx()),
        FileRead::Bytes(stamp_file_bytes(&expected_stamp(ZONE))),
    );
    assert_eq!(
        run(&disk2, Mode::Person, Reply::Honest).outcome,
        Outcome::Pushed
    );
}

#[test]
fn every_stored_stamp_that_cannot_be_read_makes_it_due() {
    let good = stamp_file_bytes(&expected_stamp(ZONE));
    for stored in [
        FileRead::Unreadable,
        FileRead::Bytes(Vec::new()),
        FileRead::Bytes(b"not json".to_vec()),
        FileRead::Bytes(expected_stamp(ZONE).into_bytes()),
        FileRead::Bytes(stamp_file_bytes(&expected_stamp("UTC"))),
        FileRead::Bytes(
            String::from_utf8(good.clone())
                .unwrap()
                .replace("\"version\":0", "\"version\":1")
                .into_bytes(),
        ),
    ] {
        let disk = laptop(DUMMY_TOKEN);
        disk.files
            .borrow_mut()
            .insert(stamp_path(&ctx()), stored.clone());
        assert_eq!(
            run(&disk, Mode::Person, Reply::Honest).outcome,
            Outcome::Pushed,
            "{stored:?}"
        );
    }
    let disk = laptop(DUMMY_TOKEN);
    let mut marked = vec![0xEF, 0xBB, 0xBF];
    marked.extend(&good);
    disk.files
        .borrow_mut()
        .insert(stamp_path(&ctx()), FileRead::Bytes(marked));
    assert_eq!(
        run(&disk, Mode::Person, Reply::Honest).outcome,
        Outcome::NotDue
    );
}

/// The push's decision is the stamp's rule exactly (operator.md, The stamp): due when forced, with
/// no stamp, or with one that is empty, unreadable, unparseable, fails its schema or differs from
/// the one computed; otherwise not. It takes nothing else: no input says whether the host was
/// started.
#[test]
fn the_decision_is_the_stamps_rule_and_nothing_else() {
    let path = stamp_path(&ctx());
    let computed = expected_stamp(ZONE);
    let good = stamp_file_bytes(&computed);
    let schema_failing = String::from_utf8(good.clone())
        .unwrap()
        .replace("\"version\":0", "\"version\":1")
        .into_bytes();
    let cases = [
        (None, true),
        (Some(FileRead::Missing), true),
        (Some(FileRead::Unreadable), true),
        (Some(FileRead::Bytes(Vec::new())), true),
        (Some(FileRead::Bytes(b"{".to_vec())), true),
        (Some(FileRead::Bytes(schema_failing)), true),
        (
            Some(FileRead::Bytes(stamp_file_bytes(&expected_stamp("UTC")))),
            true,
        ),
        (Some(FileRead::Bytes(good)), false),
    ];
    for (stored, due) in cases {
        let disk = Disk::default();
        if let Some(stored) = stored.clone() {
            disk.files.borrow_mut().insert(path.clone(), stored);
        }
        assert_eq!(is_due(&disk, &path, &computed, false), due, "{stored:?}");
        assert!(is_due(&disk, &path, &computed, true), "forced, {stored:?}");
    }
}

/// A start of the host MUST NOT make the push due by itself (#0167; P14). A stop and a start keep
/// the instance ID, the zone, the URL and the token, so the connection that starts the host finds
/// the stamp the last push wrote, and pushes nothing.
#[test]
fn a_start_of_the_host_does_not_make_the_push_due() {
    let disk = laptop(DUMMY_TOKEN);
    // The connection before the host was stopped pushed.
    assert_eq!(
        run(&disk, Mode::Caller, Reply::Honest).outcome,
        Outcome::Pushed
    );
    // The connection that starts it again, through the ProxyCommand: the same ID, unforced.
    let r = run(&disk, Mode::Caller, Reply::Honest);
    assert_eq!(r.outcome, Outcome::NotDue);
    assert!(r.ssh.is_empty(), "A start made the push due");
    assert!(r.said.is_empty());
    assert!(r.writes.is_empty());
    // After localswim's cold start the token is new, and the push is due whatever the host did.
    let cold = laptop(OTHER_TOKEN);
    cold.files.borrow_mut().insert(
        stamp_path(&ctx()),
        FileRead::Bytes(stamp_file_bytes(&expected_stamp(ZONE))),
    );
    assert_eq!(
        run(&cold, Mode::Caller, Reply::Honest).outcome,
        Outcome::Pushed
    );
}

#[test]
fn forced_pushes_whatever_the_stamp_and_nesting_say() {
    let disk = laptop(DUMMY_TOKEN);
    run(&disk, Mode::Person, Reply::Honest);
    let nested = Context {
        nested: true,
        ..ctx()
    };
    let r = run_with(
        &nested,
        &disk,
        Mode::Person,
        Reply::Honest,
        Some(ZONE),
        "agentspace",
        true,
    );
    assert_eq!(r.outcome, Outcome::Pushed);
    assert_eq!(r.ssh.len(), 1);
}

#[test]
fn a_nested_up_reads_nothing_and_prints_nothing() {
    let disk = laptop(DUMMY_TOKEN);
    let nested = Context {
        nested: true,
        ..ctx()
    };
    let r = run_with(
        &nested,
        &disk,
        Mode::Person,
        Reply::Honest,
        Some(ZONE),
        "agentspace",
        false,
    );
    assert_eq!(r.outcome, Outcome::Nested);
    assert!(r.said.is_empty());
    assert!(disk.reads.borrow().is_empty());
}

#[test]
fn a_bad_alias_warns_and_reads_nothing() {
    // By ruling (architect, 2026-11-19, for E0.06-T01-S04): no `Sending` line, the one bad-alias
    // warning, and the alias is checked before anything is read.
    for (mode, forced) in [
        (Mode::Person, false),
        (Mode::Person, true),
        (Mode::Caller, false),
    ] {
        let disk = laptop(DUMMY_TOKEN);
        let r = run_with(
            &ctx(),
            &disk,
            mode,
            Reply::Honest,
            Some(ZONE),
            "-oProxyCommand=x",
            forced,
        );
        assert_eq!(r.outcome, Outcome::BadAlias);
        assert_eq!(r.said, [BAD_ALIAS]);
        assert!(r.ssh.is_empty());
        assert!(r.writes.is_empty());
        assert!(disk.reads.borrow().is_empty(), "Nothing is read");
    }
}

#[test]
fn the_zone_is_guarded_and_never_mapped_here() {
    // The zone comes already mapped and checked; the push guards it and holds no legacy table
    // (architect, 2026-11-19, for E0.06-T01-S04). A legacy name that reaches the push is sent as
    // it is, and a name that fails step 3's check is never put on the command line.
    let disk = laptop(DUMMY_TOKEN);
    let r = run_with(
        &ctx(),
        &disk,
        Mode::Person,
        Reply::Honest,
        Some("Asia/Calcutta"),
        "agentspace",
        false,
    );
    assert_eq!(r.outcome, Outcome::Pushed);
    assert_eq!(r.ssh[0].0[5], remote_command("Asia/Calcutta"));
    assert_eq!(
        r.writes[0].1,
        stamp_file_bytes(&expected_stamp("Asia/Calcutta"))
    );

    for bad in ["-UTC", "UTC; reboot", "UTC\n", "a'b", "$(x)"] {
        let disk = laptop(DUMMY_TOKEN);
        let r = run_with(
            &ctx(),
            &disk,
            Mode::Person,
            Reply::Honest,
            Some(bad),
            "agentspace",
            false,
        );
        assert_eq!(r.said, [SENDING, NO_ZONE], "{bad:?}");
        assert_eq!(r.ssh[0].0[5], remote_command(""), "{bad:?}");
        assert_eq!(r.writes[0].1, stamp_file_bytes(&expected_stamp("")));
    }
}

#[test]
fn debug_of_a_read_and_of_an_ssh_result_gives_counts_not_bytes() {
    let read = FileRead::Bytes(b"QUOTE-ME-FILE".to_vec());
    assert_eq!(format!("{read:?}"), "Bytes(<13 bytes>)");
    assert_eq!(format!("{:?}", FileRead::Missing), "Missing");
    assert_eq!(format!("{:?}", FileRead::Unreadable), "Unreadable");
    let result = SshResult::Exited {
        code: 3,
        stdout: format!("LOCALSWIM_TOKEN={DUMMY_TOKEN}\n").into_bytes(),
    };
    let text = format!("{result:?}");
    assert!(!text.contains(DUMMY_TOKEN) && !text.contains("LOCALSWIM"));
    assert_eq!(
        text,
        format!(
            "Exited {{ code: 3, stdout: <{} bytes> }}",
            DUMMY_TOKEN.len() + 17
        )
    );
    assert_eq!(format!("{:?}", SshResult::TimedOut), "TimedOut");
    assert_eq!(format!("{:?}", SshResult::CouldNotStart), "CouldNotStart");
}

#[test]
fn no_receipt_skips_without_a_message() {
    for lad in [None, Some(PathBuf::from("  "))] {
        let c = Context {
            local_app_data: lad,
            ..ctx()
        };
        let r = run_with(
            &c,
            &laptop(DUMMY_TOKEN),
            Mode::Person,
            Reply::Honest,
            Some(ZONE),
            "agentspace",
            false,
        );
        assert_eq!(r.outcome, Outcome::NoReceipt);
        assert!(r.said.is_empty());
    }
    let r = run(&Disk::default(), Mode::Person, Reply::Honest);
    assert_eq!(r.outcome, Outcome::NoReceipt);
    assert!(r.said.is_empty());
}

#[test]
fn each_read_failure_warns_could_not_read_and_sends_nothing() {
    let receipt = receipt_path(&ctx()).unwrap();
    let cred = credential_path("creds/");
    let cases: Vec<(PathBuf, FileRead)> = vec![
        (receipt.clone(), FileRead::Unreadable),
        (receipt.clone(), FileRead::Bytes(b"[]".to_vec())),
        (
            receipt.clone(),
            FileRead::Bytes(br#"{"credential_directory":"creds/"}"#.to_vec()),
        ),
        (
            receipt.clone(),
            FileRead::Bytes(br#"{"base":"","credential_directory":"creds/"}"#.to_vec()),
        ),
        (
            receipt.clone(),
            FileRead::Bytes(
                br#"{"base":"http://127.0.0.1:8811\nx","credential_directory":"creds/"}"#.to_vec(),
            ),
        ),
        (
            receipt.clone(),
            FileRead::Bytes(br#"{"base":"http://127.0.0.1:8811"}"#.to_vec()),
        ),
        (
            receipt.clone(),
            FileRead::Bytes(
                br#"{"base":"http://127.0.0.1:8811/","credential_directory":"creds/"}"#.to_vec(),
            ),
        ),
        (cred.clone(), FileRead::Missing),
        (cred.clone(), FileRead::Unreadable),
        (cred.clone(), FileRead::Bytes(b"{}".to_vec())),
        (cred.clone(), FileRead::Bytes(b"not json".to_vec())),
        (
            cred.clone(),
            FileRead::Bytes(credential_json(&[9, 9, 9, 9])),
        ),
        (
            cred.clone(),
            FileRead::Bytes(credential_json(&blob_for(""))),
        ),
        (
            cred.clone(),
            FileRead::Bytes(credential_json(&blob_for("a\0b"))),
        ),
        (
            cred.clone(),
            FileRead::Bytes(credential_json(&blob_for("a\rb"))),
        ),
        (cred, FileRead::Bytes(credential_json(&[1, 0, 0, 0, 0x61]))),
    ];
    for (path, read) in cases {
        for mode in [Mode::Person, Mode::Caller] {
            let disk = laptop(DUMMY_TOKEN);
            disk.files.borrow_mut().insert(path.clone(), read.clone());
            let r = run(&disk, mode, Reply::Honest);
            assert_eq!(r.outcome, Outcome::CouldNotRead, "{path:?} {read:?}");
            assert_eq!(r.said, [COULD_NOT_READ]);
            assert!(r.ssh.is_empty());
            assert!(r.writes.is_empty());
        }
    }
}

#[test]
fn caller_mode_prints_no_sending_line_and_rewrites_the_instance() {
    let r = run(&laptop(DUMMY_TOKEN), Mode::Caller, Reply::Honest);
    assert_eq!(r.outcome, Outcome::Pushed);
    assert!(r.said.is_empty(), "{:?}", r.said);
    let r = run(&laptop(DUMMY_TOKEN), Mode::Caller, Reply::Exit(1));
    assert_eq!(
        r.said,
        [
            "agentspace: The host did not confirm the localswim token; the board will not work on the box this session."
        ]
    );
}

#[test]
fn each_answer_and_outcome() {
    let cases = [
        (
            Reply::Exit(1),
            Outcome::NotConfirmed,
            NOT_CONFIRMED.to_owned(),
        ),
        (
            Reply::Exit(255),
            Outcome::NotConfirmed,
            NOT_CONFIRMED.to_owned(),
        ),
        (
            Reply::WrongHash,
            Outcome::NotConfirmed,
            NOT_CONFIRMED.to_owned(),
        ),
        (
            Reply::ZoneFailed,
            Outcome::ZoneNotTaken,
            zone_not_taken(ZONE),
        ),
        (Reply::TimedOut, Outcome::TimedOut, TIMED_OUT.to_owned()),
        (
            Reply::CouldNotStart,
            Outcome::CouldNotSend,
            COULD_NOT_SEND.to_owned(),
        ),
    ];
    for (answer, outcome, warning) in cases {
        let r = run(&laptop(DUMMY_TOKEN), Mode::Person, answer);
        assert_eq!(r.outcome, outcome);
        assert_eq!(r.said, [SENDING.to_owned(), warning.clone()]);
        assert!(r.writes.is_empty(), "Nothing recorded");
        let r = run(&laptop(DUMMY_TOKEN), Mode::Caller, answer);
        assert_eq!(r.said, [warning.replace("The instance", "The host")]);
    }
}

#[test]
fn a_stamp_that_cannot_be_written_warns_could_not_send() {
    let mut disk = laptop(DUMMY_TOKEN);
    disk.refuse_writes = true;
    let r = run(&disk, Mode::Person, Reply::Honest);
    assert_eq!(r.outcome, Outcome::CouldNotSend);
    assert_eq!(r.said, [SENDING, COULD_NOT_SEND]);
}

#[test]
fn no_zone_warns_sends_none_and_stamps_it_empty() {
    for zone in [None, Some("not a zone")] {
        let disk = laptop(DUMMY_TOKEN);
        let r = run_with(
            &ctx(),
            &disk,
            Mode::Person,
            Reply::Honest,
            zone,
            "agentspace",
            false,
        );
        assert_eq!(r.outcome, Outcome::Pushed);
        assert_eq!(r.said, [SENDING, NO_ZONE]);
        assert!(!r.ssh[0].0[5].contains("timedatectl"));
        assert_eq!(r.writes[0].1, stamp_file_bytes(&expected_stamp("")));
    }
}

#[test]
fn the_planted_token_appears_nowhere_but_the_ssh_stdin() {
    let answers = [
        Reply::Honest,
        Reply::ZoneFailed,
        Reply::Exit(1),
        Reply::WrongHash,
        Reply::TimedOut,
        Reply::CouldNotStart,
    ];
    let mut seen = Vec::new();
    for mode in [Mode::Person, Mode::Caller] {
        for answer in answers {
            for zone in [Some(ZONE), None] {
                let disk = laptop(DUMMY_TOKEN);
                let r = run_with(&ctx(), &disk, mode, answer, zone, "agentspace", false);
                seen.extend(r.said);
                seen.push(format!("{:?}", r.outcome));
                for (args, stdin) in r.ssh {
                    seen.extend(args);
                    assert!(
                        String::from_utf8_lossy(&stdin).contains(DUMMY_TOKEN),
                        "The token travels on stdin"
                    );
                }
                for (path, bytes) in r.writes {
                    seen.push(path.display().to_string());
                    seen.push(String::from_utf8_lossy(&bytes).into_owned());
                }
                seen.push(format!("{:?}", disk.files.borrow()));
            }
        }
    }
    let token = Token(Zeroizing::new(DUMMY_TOKEN.to_owned()));
    let c = content(Some(URL), &token).unwrap();
    seen.push(format!("{token:?} {c:?}"));
    let joined = seen.join("\n");
    assert!(!joined.is_empty());
    assert!(!joined.contains(DUMMY_TOKEN));
    // The search can find something: the token's own text, planted in the joined output.
    assert!(format!("{joined}{DUMMY_TOKEN}").contains(DUMMY_TOKEN));
}

#[test]
fn credential_path_adds_a_separator_only_when_needed() {
    let sep = MAIN_SEPARATOR;
    assert_eq!(
        credential_path("a"),
        PathBuf::from(format!("a{sep}bot.json"))
    );
    assert_eq!(credential_path("a/"), PathBuf::from("a/bot.json"));
    assert_eq!(credential_path("a\\"), PathBuf::from("a\\bot.json"));
}

// ---- The credential's own rules, at the pinned API -----------------------------------------------

/// The document with `value` for the member `name` and every other member good.
fn credential_member(name: &str, value: &str) -> Vec<u8> {
    let hex = hex_of(&blob_for(DUMMY_TOKEN));
    let mut members = vec![
        ("format", "\"localswim-credential\"".to_string()),
        ("format_version", "1".to_string()),
        ("role", "\"bot\"".to_string()),
        ("token_dpapi_hex", format!("\"{hex}\"")),
    ];
    for m in &mut members {
        if m.0 == name {
            m.1 = value.to_string();
        }
    }
    let texts: Vec<String> = members
        .iter()
        .map(|(n, v)| format!("\"{n}\":{v}"))
        .collect();
    let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
    credential_with(&refs)
}

#[test]
fn a_good_bot_json_gives_its_blob_and_then_the_token() {
    let bytes = credential_json(&blob_for(DUMMY_TOKEN));
    assert_eq!(parse_credential_blob(&bytes), Some(blob_for(DUMMY_TOKEN)));
    let token = read_token(&bytes, &FakeDpapi).expect("A token");
    assert_eq!(token.0.as_str(), DUMMY_TOKEN);
}

#[test]
fn a_utf8_mark_and_any_whitespace_are_accepted() {
    let good = String::from_utf8(credential_json(&blob_for(DUMMY_TOKEN))).unwrap();
    let indented = good
        .replace("{\"", "{\r\n    \"")
        .replace(",\"", ",\r\n    \"")
        .replace("\":", "\":  ")
        .replace('}', "\r\n}");
    let mut marked = vec![0xEF, 0xBB, 0xBF];
    marked.extend(good.as_bytes());
    for bytes in [
        marked,
        indented.into_bytes(),
        format!("\n {good}\r\n").into_bytes(),
    ] {
        assert_eq!(parse_credential_blob(&bytes), Some(blob_for(DUMMY_TOKEN)));
    }
}

#[test]
fn the_four_members_in_any_order_are_accepted() {
    let hex = hex_of(&blob_for(DUMMY_TOKEN));
    let f = "\"format\":\"localswim-credential\"".to_string();
    let v = "\"format_version\":1".to_string();
    let r = "\"role\":\"bot\"".to_string();
    let t = format!("\"token_dpapi_hex\":\"{hex}\"");
    for order in [[&t, &r, &v, &f], [&r, &t, &f, &v], [&v, &f, &t, &r]] {
        let refs: Vec<&str> = order.iter().map(|s| s.as_str()).collect();
        assert_eq!(
            parse_credential_blob(&credential_with(&refs)),
            Some(blob_for(DUMMY_TOKEN))
        );
    }
}

#[test]
fn each_member_missing_is_refused() {
    let hex = hex_of(&blob_for(DUMMY_TOKEN));
    let all = [
        "\"format\":\"localswim-credential\"".to_string(),
        "\"format_version\":1".to_string(),
        "\"role\":\"bot\"".to_string(),
        format!("\"token_dpapi_hex\":\"{hex}\""),
    ];
    for skip in 0..4 {
        let kept: Vec<&str> = all
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != skip)
            .map(|(_, s)| s.as_str())
            .collect();
        assert_eq!(
            parse_credential_blob(&credential_with(&kept)),
            None,
            "{skip}"
        );
    }
}

#[test]
fn an_extra_member_is_refused() {
    let hex = hex_of(&blob_for(DUMMY_TOKEN));
    for extra in [
        "\"extra\":1",
        "\"extra\":null",
        "\"\":\"\"",
        "\"token\":\"x\"",
    ] {
        let members = [
            "\"format\":\"localswim-credential\"".to_string(),
            "\"format_version\":1".to_string(),
            "\"role\":\"bot\"".to_string(),
            format!("\"token_dpapi_hex\":\"{hex}\""),
            extra.to_string(),
        ];
        let refs: Vec<&str> = members.iter().map(String::as_str).collect();
        assert_eq!(
            parse_credential_blob(&credential_with(&refs)),
            None,
            "{extra}"
        );
    }
}

#[test]
fn a_member_name_in_another_ascii_case_is_refused_beside_the_real_one_and_in_its_place() {
    let hex = hex_of(&blob_for(DUMMY_TOKEN));
    let base = [
        ("format", "\"localswim-credential\"".to_string()),
        ("format_version", "1".to_string()),
        ("role", "\"bot\"".to_string()),
        ("token_dpapi_hex", format!("\"{hex}\"")),
    ];
    for (i, (name, value)) in base.iter().enumerate() {
        for variant in [
            name.to_ascii_uppercase(),
            format!("{}{}", name[..1].to_ascii_uppercase(), &name[1..]),
        ] {
            let mut beside: Vec<String> =
                base.iter().map(|(n, v)| format!("\"{n}\":{v}")).collect();
            beside.push(format!("\"{variant}\":{value}"));
            let refs: Vec<&str> = beside.iter().map(String::as_str).collect();
            assert_eq!(
                parse_credential_blob(&credential_with(&refs)),
                None,
                "{variant} beside"
            );

            let mut in_place: Vec<String> =
                base.iter().map(|(n, v)| format!("\"{n}\":{v}")).collect();
            in_place[i] = format!("\"{variant}\":{value}");
            let refs: Vec<&str> = in_place.iter().map(String::as_str).collect();
            assert_eq!(
                parse_credential_blob(&credential_with(&refs)),
                None,
                "{variant} in place"
            );
        }
    }
}

#[test]
fn a_duplicate_member_is_refused_with_the_same_value_or_a_different_one_in_either_order() {
    let hex = hex_of(&blob_for(DUMMY_TOKEN));
    let other_hex = hex_of(&[1, 0, 0, 0, 9, 9]);
    let good = [
        (
            "format",
            "\"localswim-credential\"".to_string(),
            "\"x\"".to_string(),
        ),
        ("format_version", "1".to_string(), "2".to_string()),
        ("role", "\"bot\"".to_string(), "\"human\"".to_string()),
        (
            "token_dpapi_hex",
            format!("\"{hex}\""),
            format!("\"{other_hex}\""),
        ),
    ];
    for (i, (name, value, other)) in good.iter().enumerate() {
        for (first, second) in [(value, value), (value, other), (other, value)] {
            let mut texts = Vec::new();
            for (j, (n, v, _)) in good.iter().enumerate() {
                if i == j {
                    texts.push(format!("\"{n}\":{first}"));
                    texts.push(format!("\"{n}\":{second}"));
                } else {
                    texts.push(format!("\"{n}\":{v}"));
                }
            }
            let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
            assert_eq!(
                parse_credential_blob(&credential_with(&refs)),
                None,
                "{name} {first} {second}"
            );
        }
    }
    // The same name written through an escape is the same name.
    let escaped = format!(
        r#"{{"format":"localswim-credential","format_version":1,"role":"human","token_dpapi_hex":"{hex}","role":"bot"}}"#
    );
    assert_eq!(parse_credential_blob(escaped.as_bytes()), None);
}

#[test]
fn a_wrong_format_format_version_or_role_is_refused() {
    for v in [
        "\"Localswim-Credential\"",
        "\"localswim-credential \"",
        "\"\"",
        "1",
        "null",
    ] {
        assert_eq!(
            parse_credential_blob(&credential_member("format", v)),
            None,
            "{v}"
        );
    }
    for v in [
        "1.0", "1.00", "1e0", "1E0", "2", "0", "-1", "\"1\"", "true", "null", "[1]",
    ] {
        assert_eq!(
            parse_credential_blob(&credential_member("format_version", v)),
            None,
            "{v}"
        );
    }
    for v in ["\"human\"", "\"Bot\"", "\"bot \"", "\"\"", "null", "1"] {
        assert_eq!(
            parse_credential_blob(&credential_member("role", v)),
            None,
            "{v}"
        );
    }
}

#[test]
fn hex_that_is_not_hex_odd_or_empty_is_refused() {
    let hex = hex_of(&blob_for(DUMMY_TOKEN));
    for v in [
        "\"\"".to_string(),
        "\"a\"".to_string(),
        format!("\"{hex}0\""),
        format!("\" {hex}\""),
        format!("\"{hex}\\n\""),
        format!("\"0x{hex}\""),
        format!("\"{}g{}\"", &hex[..10], &hex[12..]),
        "\"zz\"".to_string(),
        "12".to_string(),
        "null".to_string(),
        "[]".to_string(),
    ] {
        assert_eq!(
            parse_credential_blob(&credential_member("token_dpapi_hex", &v)),
            None,
            "{v}"
        );
    }
    // And a refusal is a refusal of the token as well.
    assert!(read_token(&credential_member("token_dpapi_hex", "\"zz\""), &FakeDpapi).is_none());
}

fn token_of(token: &str) -> Option<Token> {
    read_token(&credential_json(&blob_for(token)), &FakeDpapi)
}

#[test]
fn a_token_must_be_exactly_43_characters_of_base64url() {
    // Architect's ruling (2026-11-20, for v0.07-T02), after the plaintext's own checks.
    for good in [
        DUMMY_TOKEN,
        OTHER_TOKEN,
        "AZaz09-_AZaz09-_AZaz09-_AZaz09-_AZaz09-_AZa",
    ] {
        assert_eq!(good.len(), 43);
        assert_eq!(token_of(good).expect("A token").0.as_str(), good);
    }
    let spliced =
        |c: &str, at: usize| format!("{}{c}{}", &DUMMY_TOKEN[..at], &DUMMY_TOKEN[at + 1..]);
    let bad = [
        DUMMY_TOKEN[..42].to_string(),
        format!("{DUMMY_TOKEN}x"),
        DUMMY_TOKEN[..41].to_string(),
        spliced("+", 20),
        spliced("/", 20),
        spliced("+", 0),
        spliced("/", 42),
        format!("{}=", &DUMMY_TOKEN[..42]),
        format!("{DUMMY_TOKEN}="),
        spliced(" ", 20),
        spliced(".", 20),
        spliced("\u{e9}", 20),
        spliced("\u{4e2d}", 20),
        spliced("\u{1F600}", 20),
        "a".to_string(),
        String::new(),
    ];
    for token in bad {
        assert!(
            token_of(&token).is_none(),
            "{} characters",
            token.chars().count()
        );
    }
}

#[test]
fn hex_in_either_case_is_read_and_two_digits_are_enough() {
    let bytes = |hex: &str| {
        format!(
            r#"{{"format":"localswim-credential","format_version":1,"role":"bot","token_dpapi_hex":"{hex}"}}"#
        )
        .into_bytes()
    };
    let upper = hex_of(&blob_for(DUMMY_TOKEN)).to_ascii_uppercase();
    assert_eq!(
        parse_credential_blob(&bytes(&upper)),
        Some(blob_for(DUMMY_TOKEN))
    );
    assert_eq!(parse_credential_blob(&bytes("aB")), Some(vec![0xAB]));
    assert_eq!(parse_credential_blob(&bytes("00")), Some(vec![0x00]));
    assert_eq!(parse_credential_blob(&bytes("")), None);
    assert_eq!(parse_credential_blob(&bytes("a")), None);
    assert_eq!(parse_credential_blob(&bytes("abc")), None);
    assert!(read_token(&bytes(&upper), &FakeDpapi).is_some());
}

#[test]
fn a_file_that_is_not_one_json_object_in_utf8_is_refused() {
    let good = String::from_utf8(credential_json(&blob_for(DUMMY_TOKEN))).unwrap();
    let mut bad_utf8 = good.clone().into_bytes();
    bad_utf8.extend([0xC3, 0x28]);
    let mut utf16 = vec![0xFF, 0xFE];
    utf16.extend(good.encode_utf16().flat_map(u16::to_le_bytes));
    let mut two_marks = vec![0xEF, 0xBB, 0xBF, 0xEF, 0xBB, 0xBF];
    two_marks.extend(good.as_bytes());
    let cases: Vec<Vec<u8>> = vec![
        Vec::new(),
        b"   ".to_vec(),
        b"not json".to_vec(),
        b"[]".to_vec(),
        b"{}".to_vec(),
        b"null".to_vec(),
        b"\"text\"".to_vec(),
        format!("[{good}]").into_bytes(),
        format!("{good}{good}").into_bytes(),
        format!("{good}\n{good}").into_bytes(),
        format!("{good}x").into_bytes(),
        format!("{good},").into_bytes(),
        format!("{good}}}").into_bytes(),
        format!("x{good}").into_bytes(),
        good.as_bytes()[..good.len() - 1].to_vec(),
        format!("{},}}", &good[..good.len() - 1]).into_bytes(),
        bad_utf8,
        utf16,
        two_marks,
        vec![0xEF, 0xBB, 0xBF],
        (0..=255u8).collect(),
        "[".repeat(50_000).into_bytes(),
        "{\"a\":".repeat(50_000).into_bytes(),
    ];
    for (i, bytes) in cases.iter().enumerate() {
        assert_eq!(parse_credential_blob(bytes), None, "case {i}");
        assert!(read_token(bytes, &FakeDpapi).is_none(), "case {i}");
    }
}

/// A throwaway folder under the temp folder, removed when dropped.
struct Throwaway(PathBuf);

impl Throwaway {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "agentspace-push-unit-{name}-{}-{}",
            std::process::id(),
            unique_hex()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Throwaway(dir)
    }
}

impl Drop for Throwaway {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn std_files_reads_tell_missing_from_unreadable() {
    let t = Throwaway::new("read");
    let file = t.0.join("f");
    std::fs::write(&file, b"x").unwrap();
    assert_eq!(StdFiles.read(&file), FileRead::Bytes(b"x".to_vec()));
    assert_eq!(StdFiles.read(&t.0.join("none")), FileRead::Missing);
    assert_eq!(StdFiles.read(&file.join("under-a-file")), FileRead::Missing);
    assert_eq!(StdFiles.read(&t.0), FileRead::Unreadable);
}

#[test]
fn std_files_writes_the_stamp_creating_its_folder_and_leaving_no_temporary_file() {
    let t = Throwaway::new("write");
    let c = Context {
        home: t.0.clone(),
        ..ctx()
    };
    let path = stamp_path(&c);
    let first = stamp_file_bytes(&expected_stamp(ZONE));
    assert!(StdFiles.write_stamp(&path, &first));
    assert_eq!(std::fs::read(&path).unwrap(), first);
    let second = stamp_file_bytes(&expected_stamp(""));
    assert!(StdFiles.write_stamp(&path, &second));
    assert_eq!(std::fs::read(&path).unwrap(), second);
    let names: Vec<_> = std::fs::read_dir(path.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names, [std::ffi::OsString::from("localswim-pushed.json")]);
    // A folder where the stamp's folder would be: the write fails, and nothing is left.
    let blocked = Context {
        home: t.0.join("f"),
        ..ctx()
    };
    std::fs::write(t.0.join("f"), b"").unwrap();
    assert!(!StdFiles.write_stamp(&stamp_path(&blocked), &first));
}

#[test]
fn a_whole_push_on_disk_with_std_files() {
    let t = Throwaway::new("push");
    let lad = t.0.join("lad");
    let creds = lad.join("localswim-v2").join("credentials").join("x");
    std::fs::create_dir_all(&creds).unwrap();
    let receipt = serde_json::json!({ "base": URL, "credential_directory": creds });
    std::fs::write(
        lad.join("localswim-v2").join("service.json"),
        receipt.to_string(),
    )
    .unwrap();
    std::fs::write(
        creds.join("bot.json"),
        credential_json(&blob_for(DUMMY_TOKEN)),
    )
    .unwrap();
    let c = Context {
        home: t.0.join("home"),
        local_app_data: Some(lad),
        ..ctx()
    };
    let say = Recorder::new(Mode::Person);
    let ssh = FakeSsh::new(Reply::Honest);
    let zone = || Some(ZONE.to_owned());
    let seams = Seams {
        files: &StdFiles,
        unprotect: &FakeDpapi,
        ssh: &ssh,
        zone: &zone,
    };
    assert_eq!(
        push_with(&c, &say, ID, "agentspace", false, &seams),
        Outcome::Pushed
    );
    assert_eq!(
        std::fs::read(stamp_path(&c)).unwrap(),
        stamp_file_bytes(&expected_stamp(ZONE))
    );
    assert_eq!(
        push_with(&c, &say, ID, "agentspace", false, &seams),
        Outcome::NotDue
    );
    assert_eq!(say.lines(), [SENDING]);
}

// A host's platform (D91, D101): the host name, the stamp per host, and the push's zone and remote
// half from the host's rows of the platform seam (operator.md, The platform seam, the push's zone
// and remote half row; The token push to a Windows host).

use agentspace_operator_api::{Bootstrap, CallerFailure, Failure, PatchMessages, SyncPlan};
use std::sync::Mutex;

/// The laptop's Windows zone ID in these tests, as `GetDynamicTimeZoneInformation` gives one.
const WINDOWS_ZONE: &str = "Mountain Standard Time";

/// A stand-in `windows/amd64` row set: its zone is the Windows zone ID it is given, its remote half
/// and remote command are marked texts, and every other row fails. It records each zone call.
#[derive(Default)]
struct StandInWindows {
    refuse_script: bool,
    zone_calls: Mutex<Vec<(String, String)>>,
}

fn stand_in_failure() -> Failure {
    Failure {
        message: "agentspace: The stand-in has no such row.".to_string(),
        caller: CallerFailure::NotAvailable,
    }
}

/// The stand-in's remote command for the push half of `zone`.
fn stand_in_command(zone: &str) -> String {
    format!("stand-in-powershell [push-half zone={zone}]")
}

impl HostPlatform for StandInWindows {
    fn platform(&self) -> &'static str {
        "windows/amd64"
    }
    fn image_parameter(&self) -> Option<&'static str> {
        None
    }
    fn new_stack_instance_type(&self) -> Option<&'static str> {
        None
    }
    fn new_stack_extra_parameters(&self) -> &'static [&'static str] {
        &[]
    }
    fn login_user(&self) -> &'static str {
        "agent"
    }
    fn ssm_document(&self) -> &'static str {
        "AWS-RunPowerShellScript"
    }
    fn remote_command(&self, script: &str) -> Result<String, Failure> {
        Ok(format!("stand-in-powershell [{script}]"))
    }
    fn bootstrap(&self) -> Result<Bootstrap, Failure> {
        Err(stand_in_failure())
    }
    fn sync_plan(&self) -> Result<SyncPlan, Failure> {
        Err(stand_in_failure())
    }
    fn patch_check_command(&self) -> Result<String, Failure> {
        Err(stand_in_failure())
    }
    fn patch_messages(&self) -> PatchMessages {
        PatchMessages {
            phase: String::new(),
            log_command: String::new(),
        }
    }
    fn horizon_command(&self, _horizon: u64) -> Result<String, Failure> {
        Err(stand_in_failure())
    }
    fn grow_script(&self) -> Result<String, Failure> {
        Err(stand_in_failure())
    }
    fn push_zone(&self, windows_zone_id: &str, iana: &str) -> Result<String, Failure> {
        self.zone_calls
            .lock()
            .unwrap()
            .push((windows_zone_id.to_owned(), iana.to_owned()));
        if windows_zone_id.is_empty() {
            Err(stand_in_failure())
        } else {
            Ok(windows_zone_id.to_owned())
        }
    }
    fn push_script(&self, zone: &str) -> Result<String, Failure> {
        if self.refuse_script {
            Err(stand_in_failure())
        } else {
            Ok(format!("push-half zone={zone}"))
        }
    }
}

/// A stand-in Windows host's ssh: the hash of what it was sent and, when `zone` is given, the zone
/// line, each ending in CR LF as Windows PowerShell's lines MAY (operator.md, The token push to a
/// Windows host).
struct WindowsSsh {
    zone: Option<String>,
    calls: RefCell<Vec<Vec<String>>>,
}

impl WindowsSsh {
    fn new(zone: Option<&str>) -> Self {
        WindowsSsh {
            zone: zone.map(str::to_owned),
            calls: RefCell::new(Vec::new()),
        }
    }
}

impl SshRun for WindowsSsh {
    fn run(&self, args: &[&str], stdin: &[u8]) -> SshResult {
        self.calls
            .borrow_mut()
            .push(args.iter().map(|s| s.to_string()).collect());
        let hash = stamp::box_hash(&Content(Zeroizing::new(stdin.to_vec())));
        let mut out = format!("{hash}  -\r\n");
        if let Some(zone) = &self.zone {
            out.push_str(&format!("zone {zone}\r\n"));
        }
        SshResult::Exited {
            code: 0,
            stdout: out.into_bytes(),
        }
    }
}

/// One push to `host` on `platform` with these stand-ins: the IANA zone [`ZONE`], and the Windows
/// zone ID `windows_zone`, counted each time it is asked. Returns the outcome, what was said, and
/// how many times the Windows zone ID was asked.
fn push_to(
    disk: &Disk,
    ssh: &dyn SshRun,
    host: Option<&str>,
    platform: &dyn HostPlatform,
    windows_zone: Option<&str>,
) -> (Outcome, Vec<String>, usize) {
    let say = Recorder::new(Mode::Person);
    let zone_fn = || Some(ZONE.to_owned());
    let asked = std::cell::Cell::new(0);
    let windows_zone_fn = || {
        asked.set(asked.get() + 1);
        windows_zone.map(str::to_owned)
    };
    let seams = Seams {
        files: disk,
        unprotect: &FakeDpapi,
        ssh,
        zone: &zone_fn,
    };
    let outcome = push_on(
        &ctx(),
        &say,
        ID,
        "agentspace-host.p.windows-amd64",
        false,
        &seams,
        host,
        platform,
        &windows_zone_fn,
    );
    (outcome, say.lines(), asked.get())
}

fn stamp_of(zone: &str) -> Vec<u8> {
    stamp_file_bytes(&expected_stamp(zone))
}

#[test]
fn a_host_name_is_a_project_or_a_project_and_a_supported_name_part() {
    for (name, project, platform) in [
        ("p", "p", "linux/arm64"),
        ("agentspace", "agentspace", "linux/arm64"),
        ("p.linux-amd64", "p", "linux/amd64"),
        ("p.windows-amd64", "p", "windows/amd64"),
        ("a-b.windows-amd64", "a-b", "windows/amd64"),
    ] {
        assert_eq!(parse_host_name(name), Some((project, platform)), "{name}");
        assert!(host_name_is_valid(name), "{name}");
        assert_eq!(host_platform(Some(name)), Some(platform), "{name}");
    }
    // `linux/arm64` never appears in a name, and a name part is compared exactly.
    for bad in [
        "p.linux-arm64",
        "p.windows-arm64",
        "p.Windows-amd64",
        "p.windows/amd64",
        "p.windows-amd64 ",
        "p.windows-amd64\n",
        "p.",
        ".windows-amd64",
        "P.windows-amd64",
        "-p.windows-amd64",
        "a.b.windows-amd64",
        "p..windows-amd64",
        "p.linux",
        "p.example-one",
        "",
        "..",
    ] {
        assert_eq!(parse_host_name(bad), None, "{bad:?}");
        assert!(!host_name_is_valid(bad), "{bad:?}");
        assert_eq!(host_platform(Some(bad)), None, "{bad:?}");
    }
    let longest = "a".repeat(32);
    assert!(host_name_is_valid(&format!("{longest}.windows-amd64")));
    assert!(!host_name_is_valid(&format!("{longest}a.windows-amd64")));
    assert_eq!(host_platform(None), Some("linux/arm64"));
}

#[test]
fn two_hosts_of_one_project_on_two_platforms_keep_two_stamps() {
    let disk = laptop(DUMMY_TOKEN);
    let linux = FakeSsh::new(Reply::Honest);
    let (outcome, _, _) = push_to(
        &disk,
        &linux,
        Some("p"),
        &agentspace_platforms::LINUX_ARM64,
        None,
    );
    assert_eq!(outcome, Outcome::Pushed);
    let windows = WindowsSsh::new(Some(WINDOWS_ZONE));
    let (outcome, _, _) = push_to(
        &disk,
        &windows,
        Some("p.windows-amd64"),
        &StandInWindows::default(),
        Some(WINDOWS_ZONE),
    );
    assert_eq!(outcome, Outcome::Pushed);
    let hosts = ctx().home.join(".agentspace").join("hosts");
    assert_eq!(
        *disk.writes.borrow(),
        [
            (
                hosts.join("p").join("localswim-pushed.json"),
                stamp_of(ZONE)
            ),
            (
                hosts.join("p.windows-amd64").join("localswim-pushed.json"),
                stamp_of(WINDOWS_ZONE)
            ),
        ]
    );
    // Each is due by its own stamp alone: the second push did not make the first's due.
    let (again, _, _) = push_to(
        &disk,
        &FakeSsh::new(Reply::Honest),
        Some("p"),
        &agentspace_platforms::LINUX_ARM64,
        None,
    );
    assert_eq!(again, Outcome::NotDue);
}

#[test]
fn a_windows_host_takes_its_rows_zone_and_remote_half() {
    let disk = laptop(DUMMY_TOKEN);
    let ssh = WindowsSsh::new(Some(WINDOWS_ZONE));
    let platform = StandInWindows::default();
    let (outcome, said, asked) = push_to(
        &disk,
        &ssh,
        Some("p.windows-amd64"),
        &platform,
        Some(WINDOWS_ZONE),
    );
    assert_eq!(outcome, Outcome::Pushed);
    assert_eq!(said, [SENDING]);
    assert_eq!(asked, 1, "The Windows zone ID is asked once");
    // The row is given the Windows zone ID and the IANA name, and its zone is the one sent.
    assert_eq!(
        *platform.zone_calls.lock().unwrap(),
        [(WINDOWS_ZONE.to_owned(), ZONE.to_owned())]
    );
    let calls = ssh.calls.borrow();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0][4], "agentspace-host.p.windows-amd64");
    assert_eq!(calls[0][5], stand_in_command(WINDOWS_ZONE));
    // The stamp covers the zone as sent: the Windows zone ID.
    assert_eq!(
        *disk.writes.borrow(),
        [(
            stamp_path_for(&ctx(), Some("p.windows-amd64")),
            stamp_of(WINDOWS_ZONE)
        )]
    );
}

#[test]
fn a_linux_host_of_the_same_project_keeps_todays_command_and_its_iana_zone() {
    for (host, platform) in [
        ("p", &agentspace_platforms::LINUX_ARM64),
        ("p.linux-amd64", &agentspace_platforms::LINUX_AMD64),
    ] {
        let disk = laptop(DUMMY_TOKEN);
        let ssh = FakeSsh::new(Reply::Honest);
        let (outcome, said, asked) = push_to(&disk, &ssh, Some(host), platform, Some(WINDOWS_ZONE));
        assert_eq!(outcome, Outcome::Pushed, "{host}");
        assert_eq!(said, [SENDING], "{host}");
        assert_eq!(
            asked, 0,
            "A Linux host never asks the Windows zone ID: {host}"
        );
        assert_eq!(ssh.calls.borrow()[0].0[5], remote_command(ZONE), "{host}");
        assert_eq!(
            *disk.writes.borrow(),
            [(stamp_path_for(&ctx(), Some(host)), stamp_of(ZONE))],
            "{host}"
        );
    }
}

#[test]
fn a_windows_zone_the_row_refuses_is_the_empty_zone() {
    let disk = laptop(DUMMY_TOKEN);
    let ssh = WindowsSsh::new(None);
    let (outcome, said, _) = push_to(
        &disk,
        &ssh,
        Some("p.windows-amd64"),
        &StandInWindows::default(),
        None,
    );
    assert_eq!(outcome, Outcome::Pushed);
    assert_eq!(said, [SENDING, NO_ZONE]);
    assert_eq!(ssh.calls.borrow()[0][5], stand_in_command(""));
    assert_eq!(
        *disk.writes.borrow(),
        [(
            stamp_path_for(&ctx(), Some("p.windows-amd64")),
            stamp_of("")
        )]
    );
}

#[test]
fn a_windows_remote_half_that_cannot_be_made_warns_could_not_send() {
    let disk = laptop(DUMMY_TOKEN);
    let ssh = WindowsSsh::new(Some(WINDOWS_ZONE));
    let platform = StandInWindows {
        refuse_script: true,
        ..StandInWindows::default()
    };
    let (outcome, said, _) = push_to(
        &disk,
        &ssh,
        Some("p.windows-amd64"),
        &platform,
        Some(WINDOWS_ZONE),
    );
    assert_eq!(outcome, Outcome::CouldNotSend);
    assert_eq!(said, [SENDING, COULD_NOT_SEND]);
    assert!(ssh.calls.borrow().is_empty(), "No ssh was started");
    assert!(disk.writes.borrow().is_empty(), "Nothing was recorded");
}

#[test]
fn a_platform_that_is_not_the_hosts_own_is_refused_before_anything_is_read() {
    let windows = StandInWindows::default();
    let cases: [(Option<&str>, &dyn HostPlatform); 4] = [
        (Some("p.windows-amd64"), &agentspace_platforms::LINUX_ARM64),
        (Some("p"), &windows),
        (None, &windows),
        (Some("p.linux-amd64"), &agentspace_platforms::LINUX_ARM64),
    ];
    for (host, platform) in cases {
        let disk = laptop(DUMMY_TOKEN);
        let ssh = FakeSsh::new(Reply::Honest);
        let (outcome, said, _) = push_to(&disk, &ssh, host, platform, Some(WINDOWS_ZONE));
        assert_eq!(outcome, Outcome::BadAlias, "{host:?}");
        assert_eq!(said, [BAD_HOST], "{host:?}");
        assert!(disk.reads.borrow().is_empty(), "{host:?}");
        assert!(ssh.calls.borrow().is_empty(), "{host:?}");
    }
}

#[test]
fn the_named_hosts_platform_decides_whether_the_windows_zone_is_asked() {
    for (host, asks) in [
        (None, false),
        (Some("p"), false),
        (Some("p.linux-amd64"), false),
        (Some("p.windows-amd64"), true),
    ] {
        let disk = laptop(DUMMY_TOKEN);
        let ssh = FakeSsh::new(Reply::Honest);
        let say = Recorder::new(Mode::Person);
        let zone_fn = || Some(ZONE.to_owned());
        let asked = std::cell::Cell::new(false);
        let windows_zone_fn = || {
            asked.set(true);
            Some(WINDOWS_ZONE.to_owned())
        };
        let seams = Seams {
            files: &disk,
            unprotect: &FakeDpapi,
            ssh: &ssh,
            zone: &zone_fn,
        };
        push_with_host_zone(
            &ctx(),
            &say,
            ID,
            "agentspace",
            false,
            &seams,
            host,
            &windows_zone_fn,
        );
        assert_eq!(asked.get(), asks, "{host:?}");
        if !asks {
            // A Linux host's push is today's, byte for byte.
            assert_eq!(ssh.calls.borrow()[0].0[5], remote_command(ZONE), "{host:?}");
        }
    }
    // A name that is no host's is refused by the entry that picks the platform, too.
    let disk = laptop(DUMMY_TOKEN);
    let ssh = FakeSsh::new(Reply::Honest);
    let say = Recorder::new(Mode::Person);
    let zone_fn = || Some(ZONE.to_owned());
    let seams = Seams {
        files: &disk,
        unprotect: &FakeDpapi,
        ssh: &ssh,
        zone: &zone_fn,
    };
    let outcome = push_with_host(
        &ctx(),
        &say,
        ID,
        "agentspace",
        false,
        &seams,
        Some("p.linux-arm64"),
    );
    assert_eq!(outcome, Outcome::BadAlias);
    assert_eq!(say.lines(), [BAD_HOST]);
    assert!(disk.reads.borrow().is_empty());
}
