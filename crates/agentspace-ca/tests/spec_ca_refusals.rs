//! The refusals of `ca`'s steps 1 to 3, which `sync`'s CA check shares with it
//! (docs/operator.md, ca and sync step 1, gaps FA-14 and FB-12; The prototype took from its
//! checkout, git working trees).
//!
//! RFC 2119 keywords, and the capitals are load-bearing. Each case runs against both `ca` and
//! `check_for_sync`, which MUST give the same message. The OneDrive refusal, whose folders come in
//! the `CaEnv`, is in spec_ca_onedrive.rs.
#![cfg(windows)]

mod spec_common;

use std::fs;
use std::path::Path;

use spec_common::*;

fn nothing_made(lab: &Lab) {
    assert!(!lab.key().exists(), "A private key was made");
    assert!(!lab.pubkey().exists(), "A public key was made");
}

// ---- Step 1: ssh-keygen.exe ----

#[test]
fn step_1_a_missing_ssh_keygen_fails_with_the_install_message() {
    for entry in BOTH {
        let lab = Lab::new();
        let mut ctx = lab.ctx();
        ctx.system_root = lab.empty_system_root();
        let o = run(entry, &ctx);
        same_for(
            o.message(),
            m_keygen_missing(&ctx.system_root),
            format!("{entry:?}"),
        );
        assert!(
            o.lines.is_empty(),
            "{entry:?} said something before failing"
        );
        assert!(!lab.ca_dir().exists(), "{entry:?} made the CA's folder");
    }
}

#[test]
fn step_1_comes_before_step_2_and_step_3() {
    for entry in BOTH {
        // A git working tree and a half-made CA, and no ssh-keygen: step 1's message.
        let lab = Lab::new();
        fs::create_dir(lab.home.join(".git")).unwrap();
        fs::create_dir_all(lab.ca_dir()).unwrap();
        fs::write(lab.key(), "x").unwrap();
        let mut ctx = lab.ctx();
        ctx.system_root = lab.empty_system_root();
        same_for(
            run(entry, &ctx).message(),
            m_keygen_missing(&ctx.system_root),
            format!("{entry:?}"),
        );
    }
}

// ---- Step 2: a git working tree ----

/// Makes a `.git` entry at `at`, a folder or the file a git worktree has.
fn make_git(at: &Path, as_file: bool) {
    fs::create_dir_all(at).unwrap();
    if as_file {
        fs::write(at.join(".git"), "gitdir: ../elsewhere\n").unwrap();
    } else {
        fs::create_dir(at.join(".git")).unwrap();
    }
}

#[test]
fn step_2_a_git_working_tree_above_the_home_is_refused_with_its_folder_named() {
    for entry in BOTH {
        for as_file in [false, true] {
            let lab = Lab::new();
            let tree_dir = lab.root().join("tree");
            let home = tree_dir.join("home");
            fs::create_dir_all(&home).unwrap();
            make_git(&tree_dir, as_file);
            let ctx = ctx_for(&home);
            let ca_dir = home.join(".agentspace").join("ca");
            let o = run(entry, &ctx);
            same_for(
                o.message(),
                m_synced_or_repo(&ca_dir, &tree_dir.display().to_string()),
                format!("{entry:?}, .git as a file: {as_file}"),
            );
            assert!(o.lines.is_empty());
            assert!(
                !ca_dir.exists(),
                "{entry:?} made the CA's folder in a working tree"
            );
        }
    }
}

#[test]
fn step_2_a_home_that_is_itself_a_working_tree_refuses_every_ca_under_it() {
    for entry in BOTH {
        let lab = Lab::new();
        make_git(&lab.home, false);
        let o = run(entry, &lab.ctx());
        same_for(
            o.message(),
            m_synced_or_repo(&lab.ca_dir(), &lab.home.display().to_string()),
            format!("{entry:?}"),
        );
        assert!(!lab.ca_dir().exists());
    }
}

#[test]
fn step_2_a_git_entry_in_the_state_folder_or_the_ca_folder_refuses_too() {
    for entry in BOTH {
        let lab = Lab::new();
        make_git(&lab.state_dir(), false);
        same_for(
            run(entry, &lab.ctx()).message(),
            m_synced_or_repo(&lab.ca_dir(), &lab.state_dir().display().to_string()),
            format!("{entry:?}, in the state folder"),
        );
        let lab = Lab::new();
        make_git(&lab.ca_dir(), false);
        same_for(
            run(entry, &lab.ctx()).message(),
            m_synced_or_repo(&lab.ca_dir(), &lab.ca_dir().display().to_string()),
            format!("{entry:?}, in the CA folder"),
        );
        nothing_made(&lab);
    }
}

#[test]
fn step_2_only_an_entry_named_dot_git_makes_a_working_tree() {
    if skip_when_elevated() {
        return;
    }
    let lab = Lab::new();
    // Neighbours with similar names, and a working tree beside the home, are not above it.
    fs::create_dir_all(lab.home.join(".github")).unwrap();
    fs::write(lab.home.join(".gitignore"), "x\n").unwrap();
    make_git(&lab.root().join("sibling"), false);
    make_ca(&lab);
    assert!(lab.key().is_file());
}

#[test]
fn step_2_comes_before_step_3() {
    for entry in BOTH {
        let lab = Lab::new();
        make_git(&lab.home, false);
        fs::create_dir_all(lab.ca_dir()).unwrap();
        fs::write(lab.key(), "x").unwrap();
        same_for(
            run(entry, &lab.ctx()).message(),
            m_synced_or_repo(&lab.ca_dir(), &lab.home.display().to_string()),
            format!("{entry:?}"),
        );
    }
}

// ---- Step 3: a half-made CA ----

#[test]
fn step_3_a_private_key_without_a_public_key_is_refused_and_not_repaired() {
    for entry in BOTH {
        let lab = Lab::new();
        fs::create_dir_all(lab.ca_dir()).unwrap();
        fs::write(lab.key(), "not a real key\n").unwrap();
        let o = run(entry, &lab.ctx());
        same_for(
            o.message(),
            m_key_without_pub(&lab.key(), &lab.pubkey()),
            format!("{entry:?}"),
        );
        assert!(
            o.lines.is_empty(),
            "{entry:?} said something before failing"
        );
        assert!(!lab.pubkey().exists(), "{entry:?} wrote a public key");
        assert_eq!(
            fs::read(lab.key()).unwrap(),
            b"not a real key\n",
            "{entry:?} changed the key"
        );
    }
}

#[test]
fn step_3_a_public_key_without_a_private_key_is_refused_and_no_ca_is_made_over_it() {
    for entry in BOTH {
        let lab = Lab::new();
        fs::create_dir_all(lab.ca_dir()).unwrap();
        let line = format!("{}\n", other_public_key_line());
        fs::write(lab.pubkey(), &line).unwrap();
        let o = run(entry, &lab.ctx());
        same_for(
            o.message(),
            m_pub_without_key(&lab.key(), &lab.pubkey()),
            format!("{entry:?}"),
        );
        assert!(
            o.lines.is_empty(),
            "{entry:?} said something before failing"
        );
        assert!(!lab.key().exists(), "{entry:?} made a private key");
        assert_eq!(
            fs::read_to_string(lab.pubkey()).unwrap(),
            line,
            "{entry:?} changed the key"
        );
    }
}

#[test]
fn step_3_a_half_made_ca_made_by_ca_itself_is_refused_the_same_way() {
    if skip_when_elevated() {
        return;
    }
    // Lose each file of a real CA in turn, moving it aside so that no copy is made.
    for lose_pub in [true, false] {
        for entry in BOTH {
            let lab = Lab::new();
            make_ca(&lab);
            let aside = lab.root().join("aside");
            let lost = if lose_pub { lab.pubkey() } else { lab.key() };
            fs::rename(&lost, &aside).unwrap();
            let o = run(entry, &lab.ctx());
            let want = if lose_pub {
                m_key_without_pub(&lab.key(), &lab.pubkey())
            } else {
                m_pub_without_key(&lab.key(), &lab.pubkey())
            };
            same_for(
                o.message(),
                want,
                format!("{entry:?}, lost the public key: {lose_pub}"),
            );
            assert!(!lost.exists(), "{entry:?} put the lost file back");
        }
    }
}
