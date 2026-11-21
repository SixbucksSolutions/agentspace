//! A keeper (`-Watch`) with a holder on each host (docs/keeper.md, Watching, The keeper's wait,
//! Fast failures and backoff, Release grace, What a keeper does over time, Stopping and starting
//! the keeper, Conformance cases for a host per project; E0.07-T03-S05, #0154).
//!
//! RFC 2119 keywords in these comments; the capitals are load-bearing. A keeper keeps a state for
//! each host and one host's state never changes another's: the holder it tracks there and
//! whether it is proven, the host's count of fast failures, and its hold-off. Every wait is read
//! off the fake clock, so a time in an expected line is the start time plus the sleeps so far.
//! With the defaults a pass comes every 30 seconds, and a pass that ends the run (`-MaxPasses`)
//! logs no hold-off line, since the run ends before step 5.
//!
//! Timelines are named `t<seconds>`: the pass time of a pass is `t`, and the lines a look logs
//! before its pass carry the time of the look.

mod hosts_world;

use hosts_world::*;

fn sw(n: u32) -> Lease {
    Lease::new(BOX, n, FAR)
}

fn lease(project: &str, n: u32) -> Lease {
    Lease::new(project, n, FAR)
}

/// H9. `alpha`'s holder exits at once on every start while the box's stays alive: `alpha`'s
/// `failed ... host=alpha` lines come with `backoff seconds=<n> host=alpha` hold-offs growing per
/// `alpha`'s count, while the box's holder is kept in a pass every interval. No box line waits on
/// `alpha`'s backoff.
#[test]
fn one_hosts_failures_back_off_alone_while_the_others_holder_is_kept() {
    let w = World::new();
    w.registry(&["alpha"]);
    let (s, a) = (sw(1), lease("alpha", 2));
    w.set_leases(&[s.clone(), a.clone()]);
    w.plan_always(&s, World::hold(100, None));
    let why = exited_at_once(Some("alpha"));
    w.plan_always(&a, World::fail(&why));
    let ran = w.watch(&["-MaxPasses", "5"]);
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            // t0: the box starts; alpha's first start fails: count 1, backoff 30.
            started(0, BOX, &s, 100),
            failed_start(0, "alpha", &a, &why),
            backoff(0, 30, Some("alpha")),
            // t30: alpha's backoff has run; it fails again: count 2, backoff 60.
            kept(30, BOX, &s, 100),
            failed_start(30, "alpha", &a, &why),
            backoff(30, 60, Some("alpha")),
            // t60: alpha is in its backoff, so it has no line; the box is kept on time.
            kept(60, BOX, &s, 100),
            // t90: count 3, backoff 120.
            kept(90, BOX, &s, 100),
            failed_start(90, "alpha", &a, &why),
            backoff(90, 120, Some("alpha")),
            // t120: still in alpha's backoff.
            kept(120, BOX, &s, 100),
        ],
    );
    // alpha's backoff never lengthens a wait: passes come every interval.
    assert_eq!(w.sleeps_secs(), vec![30, 30, 30, 30]);
    assert_eq!(w.count(&format!("start:{}", s.id)), 1);
    assert_eq!(w.count(&format!("start:{}", a.id)), 3);
    assert!(w.is_alive(100));
}

/// The same the other way round: the box's holder exits at once on every start while `alpha`'s
/// stays alive. The box's lines carry no `host=` on its `backoff` lines (v0.06's), and `alpha` is
/// kept in a pass every interval.
#[test]
fn the_boxs_failures_back_off_alone_while_another_hosts_holder_is_kept() {
    let w = World::new();
    w.registry(&["alpha"]);
    let (s, a) = (sw(1), lease("alpha", 2));
    w.set_leases(&[s.clone(), a.clone()]);
    let why = exited_at_once(None);
    w.plan_always(&s, World::fail(&why));
    w.plan_always(&a, World::hold(200, None));
    let ran = w.watch(&["-MaxPasses", "5"]);
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            failed_start(0, BOX, &s, &why),
            started(0, "alpha", &a, 200),
            backoff(0, 30, None),
            failed_start(30, BOX, &s, &why),
            kept(30, "alpha", &a, 200),
            backoff(30, 60, None),
            kept(60, "alpha", &a, 200),
            failed_start(90, BOX, &s, &why),
            kept(90, "alpha", &a, 200),
            backoff(90, 120, None),
            kept(120, "alpha", &a, 200),
        ],
    );
    assert_eq!(w.sleeps_secs(), vec![30, 30, 30, 30]);
}

/// A host's count of fast failures is its own. `alpha` fails from the first pass, so its count is
/// 2 and then 3 while the box has no lease; when the box's lease becomes ready at t60 its first
/// failed start is count 1, a backoff of 30, not 120, and its next start, at t90, goes through
/// while `alpha` fails a third time and backs off 120.
#[test]
fn a_hosts_count_of_fast_failures_is_its_own() {
    let w = World::new();
    w.registry(&["alpha"]);
    let (s, a) = (sw(1), lease("alpha", 2));
    w.set_leases(std::slice::from_ref(&a));
    let alpha_why = exited_at_once(Some("alpha"));
    let box_why = exited_at_once(None);
    w.plan_always(&a, World::fail(&alpha_why));
    w.plan(&s, World::fail(&box_why));
    w.plan(&s, World::hold(100, None));
    // The box's lease appears just as the third pass begins (the lease state is read after the
    // pass lock is taken).
    let leases = [s.clone(), a.clone()];
    w.on("lock:pass", 3, move |w| w.set_leases(&leases));
    let ran = w.watch(&["-MaxPasses", "5"]);
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            idle(0, None),
            failed_start(0, "alpha", &a, &alpha_why),
            backoff(0, 30, Some("alpha")),
            idle(30, None),
            failed_start(30, "alpha", &a, &alpha_why),
            backoff(30, 60, Some("alpha")),
            // t60: the box's first failure, its own count of 1. alpha is in its backoff.
            failed_start(60, BOX, &s, &box_why),
            backoff(60, 30, None),
            // t90: both hold-offs have ended; the box starts, alpha fails a third time.
            started(90, BOX, &s, 100),
            failed_start(90, "alpha", &a, &alpha_why),
            backoff(90, 120, Some("alpha")),
            kept(120, BOX, &s, 100),
        ],
    );
}

/// H10. `alpha`'s holder is stopped, its lease expiring, while the box's lease stays ready:
/// `grace seconds=95 host=alpha`, passes go on every interval with the box's `kept`, and a new
/// `alpha` lease that becomes ready meanwhile gets no start until the grace has run, then a start
/// at the first pass after it.
#[test]
fn one_hosts_release_grace_never_delays_the_others_and_its_next_start_waits_for_it() {
    let w = World::new();
    w.registry(&["alpha"]);
    let s = sw(1);
    // alpha's first lease expires at t90 (12:01:30); its second becomes ready at t120.
    let a1 = Lease::new("alpha", 2, "2026-11-19T12:01:30Z");
    let a2 = lease("alpha", 3);
    w.set_leases(&[s.clone(), a1.clone()]);
    w.plan_always(&s, World::hold(100, None));
    w.plan(&a1, World::hold(200, None));
    w.plan(&a2, World::hold(201, None));
    let later = [s.clone(), a1.clone(), a2.clone()];
    w.on("lock:pass", 5, move |w| w.set_leases(&later));
    let ran = w.watch(&["-MaxPasses", "8"]);
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            started(0, BOX, &s, 100),
            started(0, "alpha", &a1, 200),
            kept(30, BOX, &s, 100),
            kept(30, "alpha", &a1, 200),
            kept(60, BOX, &s, 100),
            kept(60, "alpha", &a1, 200),
            // t90: the pass time equals alpha's expiry, so the lease is not ready: stop, grace.
            kept(90, BOX, &s, 100),
            stopped(90, "alpha", &a1, 200),
            grace(90, 95, Some("alpha")),
            // t120, t150, t180: the second lease is ready, but alpha's grace runs to t185.
            kept(120, BOX, &s, 100),
            kept(150, BOX, &s, 100),
            kept(180, BOX, &s, 100),
            // t210: the first pass after the grace starts alpha's holder.
            kept(210, BOX, &s, 100),
            started(210, "alpha", &a2, 201),
        ],
    );
    assert_eq!(w.sleeps_secs(), vec![30; 7]);
    assert_eq!(w.stopped_pids(), vec![200]);
}

/// A look that finds one host's holder gone sets that host's hold-off alone: the line comes at the
/// look, before the pass; the box's holder is kept in the same pass, and the pass is not delayed,
/// since the box is due. The grace is counted from the look, 95 seconds, and the first pass after
/// it starts alpha's next holder.
#[test]
fn a_look_that_finds_one_hosts_holder_gone_sets_that_hosts_grace_alone() {
    let w = World::new();
    w.registry(&["alpha"]);
    let (s, a) = (sw(1), lease("alpha", 2));
    w.set_leases(&[s.clone(), a.clone()]);
    w.plan_always(&s, World::hold(100, None));
    // alpha's first holder lives 45 seconds, so the look at t60 finds it gone, unproven.
    w.plan(&a, World::hold(200, Some(secs(45))));
    w.plan(&a, World::hold(201, None));
    let ran = w.watch(&["-MaxPasses", "7"]);
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            started(0, BOX, &s, 100),
            started(0, "alpha", &a, 200),
            kept(30, BOX, &s, 100),
            kept(30, "alpha", &a, 200),
            // t60: the look finds alpha's holder gone; its grace runs 95 seconds from here.
            grace(60, 95, Some("alpha")),
            kept(60, BOX, &s, 100),
            kept(90, BOX, &s, 100),
            kept(120, BOX, &s, 100),
            kept(150, BOX, &s, 100),
            // t180: the first pass at or after t155.
            kept(180, BOX, &s, 100),
            started(180, "alpha", &a, 201),
        ],
    );
}

/// H11. A keeper tracking holders on two hosts; `alpha`'s record is removed between the look and
/// its pass, as #0068's check does: `lost ... host=alpha` and its grace, with the box's holder
/// kept in the same pass. The process the record named is left alone, and the first pass after
/// the grace starts the next holder.
#[test]
fn a_tracked_holder_whose_record_went_is_lost_on_its_host_alone() {
    let w = World::new();
    w.registry(&["alpha"]);
    let (s, a) = (sw(1), lease("alpha", 2));
    w.set_leases(&[s.clone(), a.clone()]);
    w.plan_always(&s, World::hold(100, None));
    w.plan(&a, World::hold(200, None));
    w.plan(&a, World::hold(201, None));
    // Between the second pass's look and its trust check, alpha's record is removed.
    w.on("lock:pass", 2, |w| {
        std::fs::remove_file(w.record_path(Some("alpha"))).expect("alpha's record is there");
    });
    let ran = w.watch(&["-MaxPasses", "6"]);
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            started(0, BOX, &s, 100),
            started(0, "alpha", &a, 200),
            kept(30, BOX, &s, 100),
            lost(30, "alpha", &a, 200),
            grace(30, 95, Some("alpha")),
            kept(60, BOX, &s, 100),
            kept(90, BOX, &s, 100),
            kept(120, BOX, &s, 100),
            // t150: the first pass at or after t125, the end of the grace counted from the pass.
            kept(150, BOX, &s, 100),
            started(150, "alpha", &a, 201),
        ],
    );
    assert!(
        w.is_alive(200),
        "The process a dropped record named is left alone"
    );
    assert_eq!(w.stopped_pids(), Vec::<u32>::new());
}

// ---- Adoption by a keeper (H15) ----

/// H15, in a keeper. v0.06's box holder is running and recorded, with no registry: the first pass
/// logs `kept ... host=swimcoach` and adopts it with no start, the record untouched. An adopted
/// holder is proven, so when it ends the look sets the grace, 95 seconds, and the wait after the
/// look waits it out before the pass that starts the next holder.
#[test]
fn a_keeper_started_over_v006s_box_holder_adopts_it_and_the_grace_follows_its_end() {
    let w = World::new();
    let s = sw(1);
    w.set_leases(std::slice::from_ref(&s));
    // The holder v0.06 started dies at t70.
    w.adopt(None, 100, &s, Some(secs(70)));
    w.plan(&s, World::hold(101, None));
    let bytes = w.record_bytes(None).unwrap();
    let ran = w.watch(&["-MaxPasses", "4"]);
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            kept(0, BOX, &s, 100),
            kept(30, BOX, &s, 100),
            kept(60, BOX, &s, 100),
            // t90: the look finds it gone. Only the box is a host, so the keeper waits the grace.
            grace(90, 95, None),
            started(185, BOX, &s, 101),
        ],
    );
    assert_eq!(w.sleeps_secs(), vec![30, 30, 30, 95]);
    assert_ne!(
        w.record_bytes(None),
        Some(bytes),
        "The new holder is recorded"
    );
}

/// The adopted record is untouched while it is kept: a keeper that only keeps writes nothing to
/// it, in any pass.
#[test]
fn a_keeper_never_rewrites_a_record_it_keeps() {
    let w = World::new();
    w.registry(&["alpha"]);
    let (s, a) = (sw(1), lease("alpha", 2));
    w.set_leases(&[s.clone(), a.clone()]);
    w.adopt(None, 100, &s, None);
    w.adopt(Some("alpha"), 200, &a, None);
    let before = (w.record_bytes(None), w.record_bytes(Some("alpha")));
    let ran = w.watch(&["-MaxPasses", "3"]);
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            kept(0, BOX, &s, 100),
            kept(0, "alpha", &a, 200),
            kept(30, BOX, &s, 100),
            kept(30, "alpha", &a, 200),
            kept(60, BOX, &s, 100),
            kept(60, "alpha", &a, 200),
        ],
    );
    assert_eq!(
        (w.record_bytes(None), w.record_bytes(Some("alpha"))),
        before
    );
    assert_eq!(w.count_prefix("start:"), 0);
}

/// A keeper started over both hosts' holders adopts each independently: when `alpha`'s ends, only
/// `alpha` gets the grace, and the box is kept in every pass meanwhile.
#[test]
fn a_keeper_adopts_each_hosts_holder_and_the_graces_are_per_host() {
    let w = World::new();
    w.registry(&["alpha"]);
    let (s, a) = (sw(1), lease("alpha", 2));
    w.set_leases(&[s.clone(), a.clone()]);
    w.adopt(None, 100, &s, None);
    w.adopt(Some("alpha"), 200, &a, Some(secs(70)));
    w.plan(&a, World::hold(201, None));
    let ran = w.watch(&["-MaxPasses", "8"]);
    assert_eq!(ran.exit, 0);
    assert_lines(
        &w,
        &ran,
        vec![
            kept(0, BOX, &s, 100),
            kept(0, "alpha", &a, 200),
            kept(30, BOX, &s, 100),
            kept(30, "alpha", &a, 200),
            kept(60, BOX, &s, 100),
            kept(60, "alpha", &a, 200),
            // t90: the look finds alpha's adopted holder gone; the grace is alpha's alone.
            grace(90, 95, Some("alpha")),
            kept(90, BOX, &s, 100),
            kept(120, BOX, &s, 100),
            kept(150, BOX, &s, 100),
            kept(180, BOX, &s, 100),
            // t210: the first pass at or after t185.
            kept(210, BOX, &s, 100),
            started(210, "alpha", &a, 201),
        ],
    );
    assert!(w.is_alive(100));
}

// ---- Locks ----

/// One pass lock serves every host: a keeper with two hosts takes it once per pass, never once
/// per host, and releases it before the wait (keeper.md, Locks).
#[test]
fn a_pass_takes_the_one_pass_lock_for_all_its_hosts() {
    let w = World::new();
    w.registry(&["alpha"]);
    let (s, a) = (sw(1), lease("alpha", 2));
    w.set_leases(&[s.clone(), a.clone()]);
    w.adopt(None, 100, &s, None);
    w.adopt(Some("alpha"), 200, &a, None);
    let ran = w.watch(&["-MaxPasses", "3"]);
    assert_eq!(ran.exit, 0);
    assert_eq!(w.count("lock:pass"), 3);
    assert_eq!(w.count("release:pass"), 3);
    assert_eq!(w.count("lock:keeper"), 1);
    // Each pass holds the lock across both hosts' queries: the box's and alpha's.
    let names = w.names();
    let first_lock = names.iter().position(|n| n == "lock:pass").unwrap();
    let first_release = names.iter().position(|n| n == "release:pass").unwrap();
    let held = &names[first_lock..first_release];
    assert!(held.iter().any(|n| n == "query:100"), "{held:?}");
    assert!(held.iter().any(|n| n == "query:200"), "{held:?}");
}
