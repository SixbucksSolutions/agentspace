//! Secrecy (operator.md, Secrecy): a known dummy token is planted, every path runs, and every
//! output is searched for it. It MUST appear only on the ssh runner's stdin.
//!
//! RFC 2119 keywords, and the capitals are load-bearing.

mod common;
use common::*;

/// Every form of the token that a leak could take.
fn needles() -> Vec<(&'static str, Vec<u8>)> {
    let utf8 = TOKEN.as_bytes().to_vec();
    vec![
        ("utf-8", utf8.clone()),
        ("utf-16le", utf16le(TOKEN)),
        ("hex of utf-8, lowercase", hex_lower(&utf8).into_bytes()),
        (
            "hex of utf-8, uppercase",
            hex_lower(&utf8).to_uppercase().into_bytes(),
        ),
        ("hex of utf-16le", hex_lower(&utf16le(TOKEN)).into_bytes()),
        // A distinctive middle of the token, in case only part of it is quoted.
        ("middle", b"Zk3Qp9xL2mVb7RtYw5".to_vec()),
        ("tail", b"Hs8DfGa1JeUo0iX4".to_vec()),
    ]
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && hay.windows(needle.len()).any(|w| w == needle)
}

/// Searches everything the push produced, other than the ssh runner's stdin.
fn assert_token_only_on_stdin(what: &str, r: &Run) {
    let mut places: Vec<(String, Vec<u8>)> = Vec::new();
    for (i, l) in r.printed.iter().enumerate() {
        places.push((format!("printed line {i}"), l.clone().into_bytes()));
    }
    for (i, e) in r.events.iter().enumerate() {
        places.push((format!("say event {i}"), format!("{e:?}").into_bytes()));
    }
    places.push((
        "the push's returned value, as Debug".into(),
        r.debug.clone().into_bytes(),
    ));
    // What a failing assertion prints: the whole run's Debug text, whose ssh calls give the
    // stdin's byte count and never its bytes.
    places.push((
        "the Debug text of the whole run".into(),
        format!("{r:?}").into_bytes(),
    ));
    if let Some(b) = &r.stamp_after {
        places.push(("the stamp's bytes".into(), b.clone()));
    }
    for (name, bytes) in &r.home_files {
        places.push((format!("the file name {name}"), name.clone().into_bytes()));
        places.push((format!("the file {name}"), bytes.clone()));
    }
    for (i, c) in r.ssh_calls.iter().enumerate() {
        for (j, a) in c.args.iter().enumerate() {
            places.push((
                format!("ssh call {i}, argument {j}"),
                a.clone().into_bytes(),
            ));
        }
        // The place is named by its index: a failure MUST NOT print a variable's name or value.
        for (n, (k, v)) in c.env.iter().enumerate() {
            places.push((
                format!("ssh call {i}, environment variable {n}"),
                format!("{k}={v}").into_bytes(),
            ));
        }
    }
    for (place, hay) in &places {
        for (form, needle) in needles() {
            assert!(
                !contains(hay, &needle),
                "{what}: the token ({form}) is in {place}"
            );
        }
    }
    // The blob handed to Unprotect is the credential's, never the token.
    for b in &r.unprotect_calls {
        for (form, needle) in needles() {
            assert!(
                !contains(b, &needle),
                "{what}: the token ({form}) is in the blob"
            );
        }
    }
}

/// The throwaway folder outside the home holds the receipt and the credential as the scenario
/// planted them and no other file: the push keeps the token in no file, a temporary one included
/// (operator.md, Secrecy), and never writes localswim's files. The home's own files are searched
/// by `assert_token_only_on_stdin`.
fn assert_nothing_is_written_outside_the_home(what: &str, env: &Env, s: &Scenario) {
    let home = env.home();
    let mut planted: Vec<(std::path::PathBuf, Vec<u8>)> = Vec::new();
    if let Slot::Bytes(b) = &s.receipt {
        planted.push((env.receipt_path(), b.clone()));
    }
    if let Slot::Bytes(b) = &s.credential {
        planted.push((env.bot_json(), b.clone()));
    }
    let mut found: Vec<(std::path::PathBuf, Vec<u8>)> = Vec::new();
    let mut stack = vec![env.tmp.path().to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p == home {
                continue;
            }
            if p.is_dir() {
                stack.push(p);
            } else {
                let bytes = std::fs::read(&p).unwrap();
                found.push((p, bytes));
            }
        }
    }
    planted.sort();
    found.sort();
    let names = |v: &[(std::path::PathBuf, Vec<u8>)]| -> Vec<std::path::PathBuf> {
        v.iter().map(|(p, _)| p.clone()).collect()
    };
    assert_eq!(names(&found), names(&planted), "{what}: the files");
    assert!(found == planted, "{what}: a planted file's bytes changed");
}

fn answer(code: i32, stdout: &str) -> SshBehavior {
    SshBehavior::Answer {
        code,
        stdout: stdout.as_bytes().to_vec(),
    }
}

/// Every path the push can take, each as a named scenario.
fn every_path(env: &Env) -> Vec<(String, Scenario)> {
    let mut out: Vec<(String, Scenario)> = Vec::new();
    for mode in MODES {
        let base = |env: &Env| {
            let mut s = env.scenario();
            s.mode = mode;
            s
        };
        let mut add = |name: &str, f: &dyn Fn(&mut Scenario)| {
            let mut s = base(env);
            f(&mut s);
            out.push((format!("{name} ({mode:?})"), s));
        };
        add("success", &|_| {});
        add("success, sync", &|s| s.command = Cmd::Sync);
        add("success, no zone", &|s| {
            s.zone = None;
            s.confirm_zone = None;
        });
        add("not due", &|s| {
            s.stored_stamp = Slot::Bytes(stamp_file_bytes(STAMP_LA))
        });
        add("box refuses", &|s| s.ssh = answer(1, ""));
        add("ssh failed", &|s| s.ssh = answer(255, ""));
        add("box echoes junk", &|s| {
            s.ssh = answer(0, &format!("banner\n{TOKEN}\n"))
        });
        add("box echoes the content, then confirms", &|s| {
            s.ssh = answer(0, &format!("{CONTENT}{HASH_LOWER}  -\nzone {ZONE}\n"))
        });
        add("box answers the token as its zone line", &|s| {
            s.ssh = answer(0, &format!("{HASH_LOWER}  -\nzone {TOKEN}\n"))
        });
        add("box answers the token after the hash", &|s| {
            s.ssh = answer(0, &format!("{HASH_LOWER}  {TOKEN}\nzone-failed {TOKEN}\n"))
        });
        add("zone failed", &|s| {
            s.ssh = answer(0, &format!("{HASH_LOWER}  -\nzone-failed\n"))
        });
        add("nested up", &|s| s.nested = true);
        add("nested sync", &|s| {
            s.nested = true;
            s.command = Cmd::Sync;
        });
        add("stored stamp is a folder", &|s| {
            s.stored_stamp = Slot::Folder
        });
        add("timed out", &|s| s.ssh = SshBehavior::TimedOut);
        add("could not start", &|s| s.ssh = SshBehavior::CouldNotStart);
        add("stamp cannot be written", &|s| s.block_stamp_dir = true);
        add("bad alias", &|s| s.alias = "-oProxyCommand=x".to_string());
        add("decryption fails and its text quotes the token", &|s| {
            s.unprotect = UnprotectBehavior::Fails(format!("failed on {TOKEN}"));
        });
        add("token with a CR", &|s| {
            s.unprotect = UnprotectBehavior::Returns(utf16le(&format!("{TOKEN}\r")));
        });
        add("token with an LF", &|s| {
            s.unprotect = UnprotectBehavior::Returns(utf16le(&format!("{TOKEN}\nx")));
        });
        add("token with a NUL", &|s| {
            s.unprotect = UnprotectBehavior::Returns(utf16le(&format!("{TOKEN}\0")));
        });
        add("token with an odd byte count", &|s| {
            let mut p = utf16le(TOKEN);
            p.push(0);
            s.unprotect = UnprotectBehavior::Returns(p);
        });
        add("token in the hex member, as plain text", &|s| {
            s.credential = Slot::Bytes(enc_utf8(&credential_json(TOKEN)));
        });
        add("credential with the token in an unrelated member", &|s| {
            let mut m = members(&format!("zz{TOKEN}"));
            m.push((TOKEN.to_string(), js(TOKEN)));
            m[2].1 = js(TOKEN);
            s.credential = Slot::Bytes(enc_utf8(&object(&m)));
        });
        add("credential with the token as a duplicate member", &|s| {
            let mut m = members(&hex_lower(&blob()));
            m.push(("role".to_string(), js(TOKEN)));
            s.credential = Slot::Bytes(enc_utf8(&object(&m)));
        });
        add("receipt that quotes the token", &|s| {
            s.receipt = Slot::Bytes(enc_utf8(&format!("{{\"base\":\"{TOKEN}\"")));
        });
        add("receipt that is garbage", &|s| {
            s.receipt = Slot::Bytes(b"garbage".to_vec())
        });
        add("bot.json missing", &|s| s.credential = Slot::Absent);
        add("no receipt", &|s| s.receipt = Slot::Absent);
    }
    out
}

#[test]
fn the_planted_token_appears_only_on_the_ssh_runners_stdin() {
    let env = Env::new();
    let mut sent = 0;
    for (name, s) in every_path(&env) {
        let r = env.run(&s);
        assert_token_only_on_stdin(&name, &r);
        assert_nothing_is_written_outside_the_home(&name, &env, &s);
        for c in &r.ssh_calls {
            sent += 1;
            // What is sent is the content, and nothing else, and the token is in it.
            assert_eq!(c.stdin, CONTENT.as_bytes(), "{name}");
            assert!(contains(&c.stdin, TOKEN.as_bytes()), "{name}");
        }
    }
    assert!(sent >= 10, "the paths that send did send: {sent}");
}

#[test]
fn the_token_is_in_the_content_the_content_is_the_only_stdin_and_nothing_else_carries_it() {
    let env = Env::new();
    let r = env.run(&env.scenario());
    assert_eq!(r.ssh_calls.len(), 1);
    assert!(contains(&r.ssh_calls[0].stdin, TOKEN.as_bytes()));
    for a in &r.ssh_calls[0].args {
        assert!(!a.contains(TOKEN));
    }
    assert_token_only_on_stdin("success", &r);
}

#[test]
fn the_test_process_environment_never_gains_the_token() {
    let env = Env::new();
    for (_, s) in every_path(&env) {
        let _ = env.run(&s);
    }
    // `vars_os`, so a value that is not Unicode cannot panic the search; the message gives the
    // index alone, never a variable's name or value.
    for (n, (k, v)) in std::env::vars_os().enumerate() {
        let (k, v) = (k.to_string_lossy(), v.to_string_lossy());
        assert!(
            !v.contains(TOKEN) && !k.contains(TOKEN),
            "Environment variable {n} holds the token"
        );
    }
}

#[test]
fn the_token_is_in_no_file_under_the_home_or_the_temp_folder_but_the_credential_it_came_from() {
    // The credential is the DPAPI blob's hex, never the token; every other file the push leaves is
    // the stamp. Walk the whole throwaway folder, receipt and credential included.
    let env = Env::new();
    let r = env.run(&env.scenario());
    assert_token_only_on_stdin("success", &r);
    let mut stack = vec![env.tmp.path().to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                let bytes = std::fs::read(&p).unwrap();
                for (form, n) in needles() {
                    assert!(
                        !contains(&bytes, &n),
                        "the token ({form}) is in {}",
                        p.display()
                    );
                }
            }
        }
    }
}

#[test]
fn a_failure_never_quotes_the_error_that_caused_it() {
    // The decryption's own text carries part of the credential; no output may carry that text.
    let env = Env::new();
    let secret_text = "PARTIAL-CREDENTIAL-TEXT-12345";
    let mut s = env.scenario();
    s.unprotect = UnprotectBehavior::Fails(format!("cannot decrypt {secret_text}"));
    let r = env.run(&s);
    assert_eq!(r.printed, vec![W_COULD_NOT_READ.to_string()]);
    let all = format!("{:?} {:?} {}", r.printed, r.events, r.debug);
    assert!(!all.contains(secret_text), "{all}");

    // The receipt's text, and the credential file's text, are not quoted either.
    let mut s = env.scenario();
    s.receipt = Slot::Bytes(enc_utf8(
        "{\"base\":\"QUOTE-ME-RECEIPT\",\"credential_directory\":}",
    ));
    let r = env.run(&s);
    let all = format!("{:?} {:?} {}", r.printed, r.events, r.debug);
    assert!(!all.contains("QUOTE-ME-RECEIPT"), "{all}");

    let mut s = env.scenario();
    s.credential = Slot::Bytes(enc_utf8("{\"format\":\"QUOTE-ME-CREDENTIAL\""));
    let r = env.run(&s);
    let all = format!("{:?} {:?} {}", r.printed, r.events, r.debug);
    assert!(!all.contains("QUOTE-ME-CREDENTIAL"), "{all}");

    // A well-formed credential with a wrong value is not quoted either.
    let mut s = env.scenario();
    s.credential = Slot::Bytes(enc_utf8(&object(&members_with(
        &hex_lower(&blob()),
        "role",
        &js("QUOTE-ME-ROLE"),
    ))));
    let r = env.run(&s);
    assert_eq!(r.printed, vec![W_COULD_NOT_READ.to_string()]);
    let all = format!("{:?} {:?} {}", r.printed, r.events, r.debug);
    assert!(!all.contains("QUOTE-ME-ROLE"), "{all}");
}

#[test]
fn a_warning_names_no_path_and_no_value_from_the_receipt() {
    let env = Env::new();
    let mut s = env.scenario();
    s.credential = Slot::Absent;
    let r = env.run(&s);
    let all = format!("{:?}", r.events);
    assert!(!all.contains("bot.json") && !all.contains(&env.tmp.path().display().to_string()));
    assert!(!all.contains(BASE));
}

#[test]
fn the_token_is_not_kept_for_a_later_push() {
    // Two pushes in one process: the second reads the files afresh, so the first token never
    // reaches the second push, nor any output of it.
    let env = Env::new();
    let mut first = env.scenario();
    first.ssh = answer(1, "");
    let _ = env.run(&first);

    let mut second = env.scenario();
    second.unprotect = UnprotectBehavior::Fails("gone".to_string());
    let r = env.run(&second);
    assert!(!r.pushed(), "a push does not reuse the earlier token");
    assert_eq!(r.printed, vec![W_COULD_NOT_READ.to_string()]);

    let mut third = env.scenario();
    third.credential = Slot::Absent;
    let r = env.run(&third);
    assert!(!r.pushed());
}

#[test]
fn hostile_stamp_files_never_panic_and_never_leak() {
    let env = Env::new();
    let big = vec![b'{'; 1 << 20];
    for bytes in [
        vec![],
        big,
        (0..=255u8).collect::<Vec<u8>>(),
        b"{\"version\":0,\"stamp\":\"".to_vec(),
        enc_utf8(&"[".repeat(100_000)),
    ] {
        let mut s = env.scenario();
        s.stored_stamp = Slot::Bytes(bytes);
        let r = env.run(&s);
        assert_eq!(r.ssh_calls.len(), 1);
        assert_token_only_on_stdin("hostile stamp", &r);
    }
}

#[test]
fn a_failing_assertion_prints_no_environment_variable_and_no_stdin() {
    // Many assertions print the whole run (`{r:?}`) when they fail. That text MUST NOT carry the
    // test process's environment, which the ssh stand-in records, nor the content sent on stdin.
    let env = Env::new();
    let r = env.run(&env.scenario());
    assert_eq!(r.ssh_calls.len(), 1);
    let call = &r.ssh_calls[0];
    assert!(
        !call.env.is_empty(),
        "The stand-in recorded the environment"
    );
    let text = format!("{r:?}");
    let count = format!("env: <{} variables>", call.env.len());
    assert!(text.contains(&count), "The count alone is printed");
    assert!(text.contains(&format!("stdin: <{} bytes>", CONTENT.len())));
    assert!(!text.contains(TOKEN), "The stdin is not printed");
    for (i, pair) in call.env.iter().enumerate() {
        // The form a derived Debug would have printed. The message names the index alone.
        assert!(
            !text.contains(&format!("{pair:?}")),
            "Environment variable {i} is in the Debug text"
        );
    }
    let alone = format!("{call:?}");
    assert!(alone.ends_with(&format!("{count} }}")), "{}", alone.len());
}

#[test]
fn the_ssh_never_sees_a_modified_environment() {
    // The push sets no environment variable of its own: the ssh's environment is the test
    // process's, at the start and at the call.
    let env = Env::new();
    let before: Vec<(String, String)> = {
        let mut v = environment();
        v.sort();
        v
    };
    let r = env.run(&env.scenario());
    let mut seen = r.ssh_calls[0].env.clone();
    seen.sort();
    // `assert!`, not `assert_eq!`: a failure MUST NOT print either side, which is the environment.
    assert!(seen == before, "The environment at the ssh call differs");
    let mut after = environment();
    after.sort();
    assert!(after == before, "The environment after the push differs");
}
