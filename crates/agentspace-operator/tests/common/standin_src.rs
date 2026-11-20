// The stand-in the operator tests run in place of the AWS CLI, ssh and scp (docs/operator.md, The
// test seam). The tests compile this file with rustc once, and copy the program to `aws.exe`,
// `ssh.exe` and `scp.exe`: it picks its role from its own file name.
//
// RFC 2119 keywords, and the capitals are load-bearing.
//
// What it does, on every start:
// - Reads its whole stdin, and records it in `<dir>/stdin-<index>.bin`.
// - Appends one record to `<dir>/calls.log`: its role, its arguments one per line, the time, the
//   value of AGENTSPACE_NESTED, how many `*_POSIX_FD_STATE` variables it saw, and whether the
//   secret named in `<dir>/secret.txt` is in an argument or in any environment variable's value.
//   It never records an environment variable's value.
// - Answers from `<dir>/resp/<key>.<n>.out|err|code`, else `<dir>/resp/<key>.out|err|code`, where
//   `<key>` is `<service>-<operation>` for the AWS CLI and the role for ssh and scp, and `<n>` is
//   how many times that key has been called, counting this call from 1.
// - An AWS call with no answer file at all fails with exit 97, so a test sees the missing answer.
//   An ssh or scp with none exits 0 and prints nothing.
//
// `<dir>` is the folder that the variable STANDIN_DIR names.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

fn read_text(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok()
}

fn bump(path: &Path) -> u32 {
    let n = read_text(path)
        .and_then(|t| t.trim().parse::<u32>().ok())
        .unwrap_or(0)
        + 1;
    let _ = fs::write(path, n.to_string());
    n
}

fn main() {
    let exe = std::env::current_exe().expect("the stand-in's own path");
    let role = exe
        .file_stem()
        .map(|s| s.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let dir = PathBuf::from(std::env::var_os("STANDIN_DIR").expect("STANDIN_DIR"));
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|a| a.to_string_lossy().into_owned())
        .collect();

    let mut stdin = Vec::new();
    let _ = std::io::stdin().read_to_end(&mut stdin);

    let key = if role == "aws" {
        format!(
            "{}-{}",
            args.first().map(String::as_str).unwrap_or(""),
            args.get(1).map(String::as_str).unwrap_or("")
        )
    } else {
        role.clone()
    };

    let index = bump(&dir.join("seq"));
    let n = bump(&dir.join(format!("count-{key}")));

    let secret = read_text(&dir.join("secret.txt")).unwrap_or_default();
    let secret = secret.trim_end_matches(['\r', '\n']).to_string();
    let mut leaked = false;
    let mut fd_state = 0;
    if !secret.is_empty() {
        leaked = args.iter().any(|a| a.contains(&secret));
    }
    for (name, value) in std::env::vars_os() {
        let name = name.to_string_lossy().to_ascii_uppercase();
        if name.ends_with("_POSIX_FD_STATE") {
            fd_state += 1;
        }
        if !secret.is_empty() && value.to_string_lossy().contains(&secret) {
            leaked = true;
        }
    }
    let nested = std::env::var_os("AGENTSPACE_NESTED")
        .map(|v| v.to_string_lossy().into_owned())
        .unwrap_or_else(|| "-".to_string());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);

    let mut record = format!("CALL {index} {role} {key} {n}\nTIME {now}\n");
    for a in &args {
        record.push_str(&format!("ARG {a}\n"));
    }
    record.push_str(&format!(
        "NESTED {nested}\nFDSTATE {fd_state}\nSECRET {}\nSTDIN {}\nEND\n",
        u8::from(leaked),
        stdin.len()
    ));
    let _ = fs::write(dir.join(format!("stdin-{index}.bin")), &stdin);
    if let Ok(mut log) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("calls.log"))
    {
        let _ = log.write_all(record.as_bytes());
    }

    let resp = dir.join("resp");
    let pick = |ext: &str| -> Option<Vec<u8>> {
        fs::read(resp.join(format!("{key}.{n}.{ext}")))
            .or_else(|_| fs::read(resp.join(format!("{key}.{ext}"))))
            .ok()
    };
    let out = pick("out");
    let err = pick("err");
    let code = pick("code");
    if out.is_none() && err.is_none() && code.is_none() && role == "aws" {
        eprintln!("STANDIN-NO-ANSWER {key}");
        std::process::exit(97);
    }
    if let Some(sleep) = pick("sleep")
        && let Ok(ms) = String::from_utf8_lossy(&sleep).trim().parse::<u64>()
    {
        std::thread::sleep(std::time::Duration::from_millis(ms));
    }
    if let Some(out) = out {
        let _ = std::io::stdout().write_all(&out);
        let _ = std::io::stdout().flush();
    }
    if let Some(err) = err {
        let _ = std::io::stderr().write_all(&err);
        let _ = std::io::stderr().flush();
    }
    let code = code
        .and_then(|c| String::from_utf8_lossy(&c).trim().parse::<i32>().ok())
        .unwrap_or(0);
    std::process::exit(code);
}
