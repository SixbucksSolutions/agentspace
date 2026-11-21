//! Shared fixtures and the adapters to agentspace-push's own API.
//!
//! Every call into the crate beyond the pinned `agentspace-operator-api` is in the section
//! "Adapters" below, and nowhere else: when the implementation's names differ from what the card
//! lets one expect, only that section changes, never an assertion.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.
#![allow(dead_code)]

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use agentspace_operator_api::{CallerLine, Context, Mode, Say};

// ---------------------------------------------------------------------------------------------
// Fixtures: every value is obviously fake.
// ---------------------------------------------------------------------------------------------

pub const INSTANCE: &str = "i-0123456789abcdef0";
pub const OTHER_INSTANCE: &str = "i-0aaaaaaaaaaaaaaa1";
pub const BASE: &str = "http://127.0.0.1:8811";
/// The planted dummy token. It is not a real token.
pub const TOKEN: &str = "dummy-_Zk3Qp9xL2mVb7RtYw5NcHs8DfGa1JeUo0iX4";
pub const TOKEN_B: &str = "dummyB_-Qq1Ww2Ee3Rr4Tt5Yy6Uu7Ii8Oo9Pp0Aa1Ss";
pub const ZONE: &str = "America/Los_Angeles";
pub const ALIAS: &str = "agentspace";

/// The content for `BASE` and `TOKEN` (sha256sum of exactly these bytes, computed independently).
pub const CONTENT: &str = "LOCALSWIM_URL=http://127.0.0.1:8811\nLOCALSWIM_TOKEN=dummy-_Zk3Qp9xL2mVb7RtYw5NcHs8DfGa1JeUo0iX4\n";
/// SHA-256 of `CONTENT`'s bytes, lowercase, as the box prints it.
pub const HASH_LOWER: &str = "b95583663ec5a991f4e37bd14f5eeee5a8d7bc3dcf19b90a3d9ad6c1431e38f4";
pub const HASH_UPPER: &str = "B95583663EC5A991F4E37BD14F5EEEE5A8D7BC3DCF19B90A3D9AD6C1431E38F4";

/// SHA-256 of `INSTANCE LF zone LF CONTENT`, uppercase hex (sha256sum, independently).
pub const STAMP_LA: &str = "6D390CD3654375C445E95E9FF2ABB9E0AFCAC3A179E9C0EF43AFB4D8CAC0B4BC";
pub const STAMP_EMPTY_ZONE: &str =
    "DE018CA20BBE2C5A5398304311851AC866DC1E4F92A28B9A09E4A75E8E81D590";
pub const STAMP_KOLKATA: &str = "D2B37FF89278D04D962F34667E06E0070F1DAF286F1E283BD638AD75E070E1AA";
pub const STAMP_LONDON: &str = "737877538A4A7E4960AB84705B72FEF25D4C7EC774406A07A26D5FDF7271A5EF";
/// The same as `STAMP_LA` for `OTHER_INSTANCE`.
pub const STAMP_LA_OTHER_INSTANCE: &str =
    "49F6B9EFA597C489A443EA57B665F0F02A22B8183D853417200326E093D9F02A";

pub const SENDING: &str = "Sending the localswim Bot token to the instance...";
pub const W_COULD_NOT_READ: &str = "agentspace: Could not read the localswim Bot token; the board will not work on the box this session.";
pub const W_NO_ZONE: &str =
    "agentspace: Could not work out the laptop's time zone; times on the box stay in its own zone.";
pub const W_TIMED_OUT: &str = "agentspace: Sending the localswim token took over 20 seconds and was stopped; the board will not work on the box this session.";
pub const W_NOT_CONFIRMED: &str = "agentspace: The instance did not confirm the localswim token; the board will not work on the box this session.";
pub const W_COULD_NOT_SEND: &str = "agentspace: Could not send the localswim token; the board will not work on the box this session.";
pub const W_BAD_ALIAS: &str = "agentspace: The SSH host alias for the localswim token is not valid; the board will not work on the box this session.";

pub fn w_zone_not_taken(zone: &str) -> String {
    format!(
        "agentspace: The instance did not take the time zone {zone}; times on the box stay in its own zone."
    )
}

/// The head of a DPAPI blob as localswim's schema pins it: version 1, the provider's GUID, the
/// master key's version and GUID, and flags of 0 (user scope). Everything after it is a payload.
pub fn dpapi_head() -> Vec<u8> {
    let mut h = vec![
        0x01, 0x00, 0x00, 0x00, 0xD0, 0x8C, 0x9D, 0xDF, 0x01, 0x15, 0xD1, 0x11, 0x8C, 0x7A, 0x00,
        0xC0, 0x4F, 0xC2, 0x97, 0xEB, // The provider's GUID ends at byte 20.
        0x01, 0x00, 0x00, 0x00, // The master key's version.
    ];
    h.extend([0x11; 16]); // The master key's GUID, made up.
    h.extend([0x00; 4]); // The flags: user scope.
    h
}

/// A fake blob: the head, then a payload whose first byte shows which nibble comes first in the
/// hex. It is not a real blob, and the fake `Unprotect` ignores what it holds.
pub fn blob() -> Vec<u8> {
    let mut b = dpapi_head();
    b.extend_from_slice(&[0x12, 0xAB, 0x3C, 0xD4, 0xEF, 0x05, 0x60]);
    b
}

/// Localswim's example blob: the head, then bytes of value zero.
pub fn example_blob() -> Vec<u8> {
    let mut b = dpapi_head();
    b.extend([0x00; 16]);
    b
}

pub fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn utf16le(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(|u| u.to_le_bytes()).collect()
}

// ---------------------------------------------------------------------------------------------
// Encodings of a text file.
// ---------------------------------------------------------------------------------------------

pub fn enc_utf8(s: &str) -> Vec<u8> {
    s.as_bytes().to_vec()
}
pub fn enc_utf8_bom(s: &str) -> Vec<u8> {
    let mut v = vec![0xEF, 0xBB, 0xBF];
    v.extend_from_slice(s.as_bytes());
    v
}
pub fn enc_utf16le_bom(s: &str) -> Vec<u8> {
    let mut v = vec![0xFF, 0xFE];
    v.extend(utf16le(s));
    v
}
pub fn enc_utf16be_bom(s: &str) -> Vec<u8> {
    let mut v = vec![0xFE, 0xFF];
    v.extend(s.encode_utf16().flat_map(|u| u.to_be_bytes()));
    v
}

// ---------------------------------------------------------------------------------------------
// The Bot credential's JSON (localswim's windows-operations.md, "The credential files").
// ---------------------------------------------------------------------------------------------

/// A JSON string literal for `s`.
pub fn js(s: &str) -> String {
    serde_json::to_string(s).unwrap()
}

/// An object written from `(name, the JSON text of the value)` pairs, in order. It keeps a
/// duplicate name and writes a name as given, which a JSON library's map would not.
pub fn object<S: AsRef<str>>(members: &[(S, String)]) -> String {
    let parts: Vec<String> = members
        .iter()
        .map(|(name, value)| format!("{}:{value}", js(name.as_ref())))
        .collect();
    format!("{{{}}}", parts.join(","))
}

/// The four members of a good `bot.json`, in the order localswim writes them, for `hex`, as
/// `(name, the JSON text of the value)`.
pub fn members(hex: &str) -> Vec<(String, String)> {
    [
        ("format", js("localswim-credential")),
        ("format_version", "1".to_string()),
        ("role", js("bot")),
        ("token_dpapi_hex", js(hex)),
    ]
    .into_iter()
    .map(|(n, v)| (n.to_string(), v))
    .collect()
}

/// `members(hex)` with the member `name` given the value text `value`.
pub fn members_with(hex: &str, name: &str, value: &str) -> Vec<(String, String)> {
    members(hex)
        .into_iter()
        .map(|(n, v)| {
            if n == name {
                (n, value.to_string())
            } else {
                (n, v)
            }
        })
        .collect()
}

/// The JSON text of the good value of the member `name`.
pub fn good_value(hex: &str, name: &str) -> String {
    members(hex)
        .into_iter()
        .find(|(n, _)| n == name)
        .expect("a member of the credential")
        .1
}

/// The good document for `hex`.
pub fn credential_json(hex: &str) -> String {
    object(&members(hex))
}

/// The standard document for the standard blob, as UTF-8 with no mark.
pub fn good_credential() -> Vec<u8> {
    enc_utf8(&credential_json(&hex_lower(&blob())))
}

// ---------------------------------------------------------------------------------------------
// Fakes.
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ev {
    Progress {
        person: String,
        caller: Option<CallerLine>,
    },
    Warning(String),
    Detail(String),
}

/// A pinned `Say` that records what the push calls, and renders it the way the pin says each
/// call prints in `mode`: the caller form rewrites `the instance` and prints nothing for a
/// progress line without a caller line.
pub struct FakeSay {
    pub mode: Mode,
    pub events: RefCell<Vec<Ev>>,
}

impl FakeSay {
    pub fn new(mode: Mode) -> Self {
        FakeSay {
            mode,
            events: RefCell::new(Vec::new()),
        }
    }

    /// The lines that reach stderr, in order.
    pub fn printed(&self) -> Vec<String> {
        let mut out = Vec::new();
        for e in self.events.borrow().iter() {
            match (e, self.mode) {
                (Ev::Progress { person, .. }, Mode::Person) => out.push(person.clone()),
                (
                    Ev::Progress {
                        caller: Some(c), ..
                    },
                    Mode::Caller,
                ) => out.push(caller_line_text(*c).to_string()),
                (Ev::Progress { caller: None, .. }, Mode::Caller) => {}
                (Ev::Warning(w), Mode::Person) => out.push(w.clone()),
                (Ev::Warning(w), Mode::Caller) => out.push(to_caller(w)),
                (Ev::Detail(d), Mode::Caller) => out.push(d.clone()),
                (Ev::Detail(_), Mode::Person) => {}
            }
        }
        out
    }
}

impl Say for FakeSay {
    fn mode(&self) -> Mode {
        self.mode
    }
    fn progress(&self, person: &str, caller: Option<CallerLine>) {
        self.events.borrow_mut().push(Ev::Progress {
            person: person.to_string(),
            caller,
        });
    }
    fn warning(&self, person: &str) {
        self.events
            .borrow_mut()
            .push(Ev::Warning(person.to_string()));
    }
    fn detail(&self, text: &str) {
        self.events.borrow_mut().push(Ev::Detail(text.to_string()));
    }
}

pub fn caller_line_text(c: CallerLine) -> &'static str {
    match c {
        CallerLine::Starting => "Starting the host...",
        CallerLine::Waiting => "Waiting for the host to be ready...",
        CallerLine::Ready => "The host is ready.",
    }
}

/// What a person's line becomes in caller mode.
pub fn to_caller(s: &str) -> String {
    s.replace("the instance", "the host")
        .replace("The instance", "The host")
}

#[derive(Debug, Clone)]
pub enum UnprotectBehavior {
    /// Every blob decrypts to these plaintext bytes.
    Returns(Vec<u8>),
    /// Every decryption fails with this text.
    Fails(String),
}

#[derive(Debug, Clone)]
pub enum SshBehavior {
    /// The box confirms the standard content (`HASH_LOWER`) and, when `Scenario::confirm_zone`
    /// is set, the zone.
    Confirm,
    Answer {
        code: i32,
        stdout: Vec<u8>,
    },
    TimedOut,
    CouldNotStart,
}

#[derive(Clone)]
pub struct SshCall {
    pub args: Vec<String>,
    pub stdin: Vec<u8>,
    /// The test process's whole environment, at the moment of the call.
    pub env: Vec<(String, String)>,
}

/// The test process's whole environment. It is read with `vars_os`, since `std::env::vars`
/// panics on a value that is not Unicode, and that panic's message quotes the value.
pub fn environment() -> Vec<(String, String)> {
    std::env::vars_os()
        .map(|(k, v)| {
            (
                k.to_string_lossy().into_owned(),
                v.to_string_lossy().into_owned(),
            )
        })
        .collect()
}

/// `Run` derives `Debug`, and a failing assertion prints it. So this `Debug` MUST NOT print the
/// environment, neither a variable's name nor its value: it gives the count alone, and the
/// stdin's byte count rather than the content. The secrecy tests read `env` and `stdin` directly.
impl std::fmt::Debug for SshCall {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "SshCall {{ args: {:?}, stdin: <{} bytes>, env: <{} variables> }}",
            self.args,
            self.stdin.len(),
            self.env.len()
        )
    }
}

// ---------------------------------------------------------------------------------------------
// Adapters: the only place that names agentspace-push's own API.
//
// The crate's API, as these adapters call it (src/lib.rs's top doc comment):
//   pub trait Unprotect { fn unprotect(&self, blob: &[u8]) -> Option<Zeroizing<Vec<u8>>>; }
//   pub trait SshRun { fn run(&self, args: &[&str], stdin: &[u8]) -> SshResult; }
//   pub enum SshResult { CouldNotStart, TimedOut, Exited { code: i32, stdout: Vec<u8> } }
//   pub struct Seams { files: &dyn Files, unprotect, ssh, zone: &dyn Fn() -> Option<String> }
//   pub fn push_with(ctx, say, instance_id: &str, alias: &str, forced: bool, &Seams) -> Outcome
//     (`sync` is forced; the files are `StdFiles` over the throwaway folders `ctx` names)
//   pub fn content(base: Option<&str>, token: &Token) -> Option<Content>
//   pub fn read_token(bot_json: &[u8], &dyn Unprotect) -> Option<Token>   // the only Token maker
//   pub fn parse_credential_blob(bot_json: &[u8]) -> Option<Vec<u8>>   // bot.json's bytes to the blob
//   pub fn stamp(instance_id: &str, zone: &str, content: &Content) -> String   // uppercase hex
//   pub fn stamp_file_bytes(stamp: &str) -> Vec<u8>
//   pub fn check_zone(zone: Option<&str>) -> String    // the guard: step 3's check, no renames
//
// The zone itself is `agentspace-zone`'s (architect, 2026-11-19, for E0.06-T01-S04):
// `Seams.zone` gives it as `agentspace_zone::zone` does, already mapped and checked, and the push
// guards it again. `Scenario::zone` is handed to the push as it is, so a test can hand it a zone
// that fails the check; `operator_zone` is what the operator hands it for a name.
// ---------------------------------------------------------------------------------------------

pub struct FakeUnprotect {
    pub behavior: UnprotectBehavior,
    pub calls: RefCell<Vec<Vec<u8>>>,
}

impl agentspace_push::Unprotect for FakeUnprotect {
    fn unprotect(&self, blob: &[u8]) -> Option<agentspace_push::Zeroizing<Vec<u8>>> {
        self.calls.borrow_mut().push(blob.to_vec());
        match &self.behavior {
            UnprotectBehavior::Returns(b) => Some(agentspace_push::Zeroizing::new(b.clone())),
            UnprotectBehavior::Fails(_) => None,
        }
    }
}

pub struct FakeSsh {
    pub behavior: SshBehavior,
    pub confirm_zone: Option<String>,
    pub calls: RefCell<Vec<SshCall>>,
}

impl agentspace_push::SshRun for FakeSsh {
    fn run(&self, args: &[&str], stdin: &[u8]) -> agentspace_push::SshResult {
        self.calls.borrow_mut().push(SshCall {
            args: args.iter().map(|a| (*a).to_string()).collect(),
            stdin: stdin.to_vec(),
            env: environment(),
        });
        match &self.behavior {
            SshBehavior::Confirm => {
                let mut out = format!("{HASH_LOWER}  -\n");
                if let Some(z) = &self.confirm_zone {
                    out.push_str(&format!("zone {z}\n"));
                }
                agentspace_push::SshResult::Exited {
                    code: 0,
                    stdout: out.into_bytes(),
                }
            }
            SshBehavior::Answer { code, stdout } => agentspace_push::SshResult::Exited {
                code: *code,
                stdout: stdout.clone(),
            },
            SshBehavior::TimedOut => agentspace_push::SshResult::TimedOut,
            SshBehavior::CouldNotStart => agentspace_push::SshResult::CouldNotStart,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cmd {
    Up,
    Sync,
}

fn call_push(
    ctx: &Context,
    say: &FakeSay,
    s: &Scenario,
    unprotect: &FakeUnprotect,
    ssh: &FakeSsh,
) -> String {
    // `sync`'s push is the forced one; `up`'s is not.
    let forced = match s.command {
        Cmd::Up => false,
        Cmd::Sync => true,
    };
    let zone = s.zone.clone();
    let zone_fn = move || zone.clone();
    // The real files, in the test's throwaway folders, which `ctx` points at.
    let files = agentspace_push::StdFiles;
    let seams = agentspace_push::Seams {
        files: &files,
        unprotect,
        ssh,
        zone: &zone_fn,
    };
    let outcome = agentspace_push::push_with(ctx, say, &s.instance, &s.alias, forced, &seams);
    format!("{outcome:?}")
}

/// An `Unprotect` that gives fixed plaintext, for building a `Token` through the public API.
struct PlainUnprotect(Vec<u8>);

impl agentspace_push::Unprotect for PlainUnprotect {
    fn unprotect(&self, _blob: &[u8]) -> Option<agentspace_push::Zeroizing<Vec<u8>>> {
        Some(agentspace_push::Zeroizing::new(self.0.clone()))
    }
}

/// A `Token` for `token`, through the crate's own credential read (the standard document, whose
/// blob decrypts to `token` in UTF-16LE).
fn token_of(token: &str) -> agentspace_push::Token {
    agentspace_push::read_token(&good_credential(), &PlainUnprotect(utf16le(token)))
        .expect("the standard credential gives a token")
}

/// The content for `base` and `token`, as bytes.
pub fn content(base: &str, token: &str) -> Vec<u8> {
    agentspace_push::content(Some(base), &token_of(token))
        .expect("the content is refused")
        .as_bytes()
        .to_vec()
}

/// The stamp for the three parts, as uppercase hex. `content` is split back into its URL and
/// token and rebuilt by the crate, since its `Content` is made only by `content`.
pub fn stamp(instance: &str, zone: &str, content: &[u8]) -> String {
    let text = std::str::from_utf8(content).unwrap();
    let rest = text.strip_prefix("LOCALSWIM_URL=").unwrap();
    let (base, rest) = rest.split_once("\nLOCALSWIM_TOKEN=").unwrap();
    let token = rest.strip_suffix('\n').unwrap();
    let c = agentspace_push::content(Some(base), &token_of(token)).unwrap();
    assert_eq!(c.as_bytes(), content, "the content round-trips");
    agentspace_push::stamp(instance, zone, &c)
}

/// `bot.json`'s bytes to the DPAPI blob, or `None` for a refusal.
pub fn parse_blob(bot_json: &[u8]) -> Option<Vec<u8>> {
    agentspace_push::parse_credential_blob(bot_json)
}

/// The bytes the stamp file holds for `stamp`.
pub fn stamp_file_bytes(stamp: &str) -> Vec<u8> {
    agentspace_push::stamp_file_bytes(stamp)
}

/// The guarded zone: the zone itself when it passes step 3's check, or empty. The push renames
/// nothing; the seven legacy renames are `agentspace-zone`'s.
pub fn check_zone(zone: &str) -> String {
    agentspace_push::check_zone(Some(zone))
}

/// The zone the operator hands the push for `name`: `agentspace_zone::zone` with `name` in
/// `AGENTSPACE_TIMEZONE`'s place, so the legacy names are mapped and the check applied.
pub fn operator_zone(name: &str) -> Option<String> {
    agentspace_zone::zone(None, Some(name))
}

/// The zone as it is sent and stamped for `name`: worked out by the operator, then guarded by
/// the push. Empty when there is none.
pub fn zone_as_sent(name: &str) -> String {
    agentspace_push::check_zone(operator_zone(name).as_deref())
}

// ---------------------------------------------------------------------------------------------
// The throwaway environment and the scenario runner.
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lad {
    Unset,
    Empty,
    Blank,
    Dir,
}

#[derive(Debug, Clone)]
pub enum Slot {
    Absent,
    Bytes(Vec<u8>),
    /// A folder where the file belongs.
    Folder,
}

#[derive(Debug, Clone)]
pub struct Scenario {
    pub mode: Mode,
    pub command: Cmd,
    pub nested: bool,
    pub instance: String,
    pub alias: String,
    pub zone: Option<String>,
    /// The zone line the default `Confirm` answer carries.
    pub confirm_zone: Option<String>,
    pub lad: Lad,
    pub receipt: Slot,
    pub credential: Slot,
    pub unprotect: UnprotectBehavior,
    pub stored_stamp: Slot,
    /// The prototype's `localswim-pushed.sha256`, which the push MUST NOT touch.
    pub proto_stamp: Option<String>,
    /// A file where the stamp's folder `.agentspace` belongs, so the stamp cannot be written.
    pub block_stamp_dir: bool,
    /// Leaves the throwaway home as the last run left it.
    pub keep_home: bool,
    /// Other files in the credential folder, as `(file name, bytes)`: localswim's folder holds
    /// `human.json` beside `bot.json`, and the push MUST ignore every file but `bot.json`.
    pub other_files: Vec<(String, Vec<u8>)>,
    pub ssh: SshBehavior,
}

/// A throwaway folder with a home and a LocalAppData in it.
pub struct Env {
    pub tmp: tempfile::TempDir,
}

impl Env {
    pub fn new() -> Self {
        Env {
            tmp: tempfile::Builder::new()
                .prefix("asp-push-")
                .tempdir()
                .unwrap(),
        }
    }
    pub fn home(&self) -> PathBuf {
        self.tmp.path().join("home")
    }
    pub fn lad(&self) -> PathBuf {
        self.tmp.path().join("lad")
    }
    pub fn receipt_path(&self) -> PathBuf {
        self.lad().join("localswim-v2").join("service.json")
    }
    pub fn cred_dir(&self) -> PathBuf {
        self.lad()
            .join("localswim-v2")
            .join("credentials")
            .join("g1")
    }
    pub fn bot_json(&self) -> PathBuf {
        self.cred_dir().join("bot.json")
    }
    pub fn stamp_dir(&self) -> PathBuf {
        self.home().join(".agentspace")
    }
    pub fn stamp_path(&self) -> PathBuf {
        self.stamp_dir().join("localswim-pushed.json")
    }
    /// The credential folder as the receipt names it, ending in `/`, which both Windows and
    /// Linux read as a separator.
    pub fn cred_dir_text(&self) -> String {
        format!("{}/", self.cred_dir().display())
    }
    /// A JSON string literal for `s`.
    pub fn json_str(s: &str) -> String {
        serde_json::to_string(s).unwrap()
    }
    /// The standard receipt's text, with `base` as the JSON text of the `base` member's value.
    pub fn receipt_text(&self, base_json: &str) -> String {
        format!(
            "{{\"base\":{base_json},\"executable\":\"fake.exe\",\"credential_directory\":{}}}",
            Self::json_str(&self.cred_dir_text())
        )
    }
    pub fn good_receipt(&self) -> Vec<u8> {
        enc_utf8(&self.receipt_text(&Self::json_str(BASE)))
    }

    pub fn scenario(&self) -> Scenario {
        Scenario {
            mode: Mode::Person,
            command: Cmd::Up,
            nested: false,
            instance: INSTANCE.to_string(),
            alias: ALIAS.to_string(),
            zone: Some(ZONE.to_string()),
            confirm_zone: Some(ZONE.to_string()),
            lad: Lad::Dir,
            receipt: Slot::Bytes(self.good_receipt()),
            credential: Slot::Bytes(good_credential()),
            unprotect: UnprotectBehavior::Returns(utf16le(TOKEN)),
            stored_stamp: Slot::Absent,
            proto_stamp: None,
            block_stamp_dir: false,
            keep_home: false,
            other_files: Vec::new(),
            ssh: SshBehavior::Confirm,
        }
    }

    fn put(path: &Path, slot: &Slot) {
        match slot {
            Slot::Absent => {}
            Slot::Bytes(b) => {
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(path, b).unwrap();
            }
            Slot::Folder => std::fs::create_dir_all(path).unwrap(),
        }
    }

    pub fn run(&self, s: &Scenario) -> Run {
        let home = self.home();
        let lad = self.lad();
        if !s.keep_home {
            let _ = std::fs::remove_dir_all(&home);
        }
        let _ = std::fs::remove_dir_all(&lad);
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&lad).unwrap();
        Self::put(&self.receipt_path(), &s.receipt);
        Self::put(&self.bot_json(), &s.credential);
        for (name, bytes) in &s.other_files {
            Self::put(&self.cred_dir().join(name), &Slot::Bytes(bytes.clone()));
        }
        if s.block_stamp_dir {
            std::fs::write(self.stamp_dir(), b"in the way").unwrap();
        } else {
            Self::put(&self.stamp_path(), &s.stored_stamp);
        }
        if let Some(p) = &s.proto_stamp {
            std::fs::create_dir_all(self.stamp_dir()).unwrap();
            std::fs::write(self.stamp_dir().join("localswim-pushed.sha256"), p).unwrap();
        }

        let ctx = Context {
            home: home.clone(),
            local_app_data: match s.lad {
                Lad::Unset => None,
                Lad::Empty => Some(PathBuf::new()),
                Lad::Blank => Some(PathBuf::from("   ")),
                Lad::Dir => Some(lad.clone()),
            },
            system_root: self.tmp.path().join("sysroot"),
            nested: s.nested,
        };
        let say = FakeSay::new(s.mode);
        let unprotect = FakeUnprotect {
            behavior: s.unprotect.clone(),
            calls: RefCell::new(vec![]),
        };
        let ssh = FakeSsh {
            behavior: s.ssh.clone(),
            confirm_zone: s.confirm_zone.clone(),
            calls: RefCell::new(vec![]),
        };
        let debug = call_push(&ctx, &say, s, &unprotect, &ssh);

        let stamp_after = std::fs::read(self.stamp_path()).ok();
        let proto_after =
            std::fs::read_to_string(self.stamp_dir().join("localswim-pushed.sha256")).ok();
        let mut home_files = Vec::new();
        walk(&home, &home, &mut home_files);
        let ssh_calls = ssh.calls.borrow().clone();
        let unprotect_calls = unprotect.calls.borrow().clone();
        let events = say.events.borrow().clone();
        Run {
            printed: say.printed(),
            events,
            ssh_calls,
            unprotect_calls,
            stamp_after,
            proto_after,
            home_files,
            debug,
        }
    }
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        let rel = p
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if p.is_dir() {
            out.push((format!("{rel}/"), Vec::new()));
            walk(root, &p, out);
        } else {
            out.push((rel, std::fs::read(&p).unwrap_or_default()));
        }
    }
}

#[derive(Debug)]
pub struct Run {
    /// The lines that reached stderr.
    pub printed: Vec<String>,
    pub events: Vec<Ev>,
    pub ssh_calls: Vec<SshCall>,
    pub unprotect_calls: Vec<Vec<u8>>,
    /// The stamp file's bytes after the run, if it is a file.
    pub stamp_after: Option<Vec<u8>>,
    pub proto_after: Option<String>,
    /// Every file and folder under the throwaway home, as `(relative path, bytes)`; folders end
    /// in `/`.
    pub home_files: Vec<(String, Vec<u8>)>,
    /// The Debug text of whatever `push` returned.
    pub debug: String,
}

impl Run {
    pub fn pushed(&self) -> bool {
        !self.ssh_calls.is_empty()
    }
}

/// What a run prints, given what each item prints for a person and for a caller.
pub enum Item<'a> {
    Sending,
    Warn(&'a str),
}

pub fn expected_printed(mode: Mode, items: &[Item]) -> Vec<String> {
    let mut v = Vec::new();
    for i in items {
        match (i, mode) {
            (Item::Sending, Mode::Person) => v.push(SENDING.to_string()),
            (Item::Sending, Mode::Caller) => {}
            (Item::Warn(w), Mode::Person) => v.push((*w).to_string()),
            (Item::Warn(w), Mode::Caller) => v.push(to_caller(w)),
        }
    }
    v
}

pub const MODES: [Mode; 2] = [Mode::Person, Mode::Caller];

/// Asserts the remote command (the last argument) obeys operator.md's rules for it.
pub fn assert_remote_command_ok(cmd: &str) {
    assert!(!cmd.is_empty(), "the remote command is empty");
    assert!(!cmd.contains('%'), "a % in the remote command: {cmd:?}");
    assert!(
        !cmd.contains('"'),
        "a double quote in the remote command: {cmd:?}"
    );
    assert!(
        !cmd.ends_with('\\'),
        "the remote command ends in a backslash: {cmd:?}"
    );
    assert!(
        !cmd.chars().any(|c| c.is_control()),
        "a control character in the remote command: {cmd:?}"
    );
    assert!(
        !cmd.contains(TOKEN) && !cmd.contains(TOKEN_B),
        "the token is in the remote command"
    );
}
