//! The whole push with stand-ins for the files, the decryption, the ssh and `Say`.

use super::*;
use agentspace_operator_api::{CallerLine, Mode};
use std::cell::RefCell;
use std::collections::HashMap;

const DUMMY_TOKEN: &str = "PLANTED-dummy-token-0123456789-not-real";
const URL: &str = "http://127.0.0.1:8811";
const ID: &str = "i-0123456789abcdef0";
const ZONE: &str = "America/New_York";
const NS: &str = "http://schemas.microsoft.com/powershell/2004/04";

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

fn credential_xml(blob: &[u8]) -> Vec<u8> {
    let hex: String = blob.iter().map(|b| format!("{b:02x}")).collect();
    format!(
        r#"<Objs Version="1.1.0.1" xmlns="{NS}"><Obj RefId="0"><TN RefId="0"><T>System.Management.Automation.PSCredential</T><T>System.Object</T></TN><ToString>System.Management.Automation.PSCredential</ToString><Props><S N="UserName">bot</S><SS N="Password">{hex}</SS></Props></Obj></Objs>"#
    )
    .into_bytes()
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
        FileRead::Bytes(credential_xml(&blob_for(token))),
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
    let disk2 = laptop("another-dummy");
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
            FileRead::Bytes(br#"{"base":"http://x\ny","credential_directory":"creds/"}"#.to_vec()),
        ),
        (cred.clone(), FileRead::Missing),
        (cred.clone(), FileRead::Unreadable),
        (cred.clone(), FileRead::Bytes(b"<Objs/>".to_vec())),
        (cred.clone(), FileRead::Bytes(credential_xml(&[9, 9, 9, 9]))),
        (cred.clone(), FileRead::Bytes(credential_xml(&blob_for("")))),
        (
            cred.clone(),
            FileRead::Bytes(credential_xml(&blob_for("a\0b"))),
        ),
        (
            cred.clone(),
            FileRead::Bytes(credential_xml(&blob_for("a\rb"))),
        ),
        (cred, FileRead::Bytes(credential_xml(&[1, 0, 0, 0, 0x61]))),
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
        PathBuf::from(format!("a{sep}bot.xml"))
    );
    assert_eq!(credential_path("a/"), PathBuf::from("a/bot.xml"));
    assert_eq!(credential_path("a\\"), PathBuf::from("a\\bot.xml"));
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
        creds.join("bot.xml"),
        credential_xml(&blob_for(DUMMY_TOKEN)),
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
