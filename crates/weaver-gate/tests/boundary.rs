//! conforms: gate-authenticated-at-accept
//! conforms: gate-agent-uid-denied-by-construction
//! conforms: gate-stopped-follows-close
//!
//! The reference walk of `weaver-gate-Spec` section 6, exercised against a real
//! listener and a real connecting process.
//!
//! **The reference walk: an elected tool dials the agent's own mouth.** The
//! adversary is the agent's tool surface running as the agent uid, the attack a
//! dial of the named socket the instruction declares, the prompt-yourself loop
//! the charter names. The mechanism is the predicate at accept with the agent's
//! own uid denied by construction.

mod common;

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;

use common::{permissive_instruction, scratch as scratch_path};
use weaver_gate::hook::{AcceptOutcome, Hook};

fn scratch(name: &str) -> common::Scratch {
    scratch_path("boundary", name)
}

/// **The socket's mode is the boundary's election and not the umask's.**
/// `UnixListener::bind` sets no mode, so the file would land at
/// `0777 & ~umask` and the access control of the agent's front door would be
/// decided by whatever umask the process inherited. On 2026-08-28 the same
/// build bound `0777` on one box and `0775` on another for exactly that
/// reason, and neither figure was anyone's election.
///
/// Connecting to a Unix socket requires write permission, so the assertion is
/// that no bit outside owner and group is set. **The ambient umask is read
/// and left alone**, and where it would produce `0770` by itself the test
/// says so and skips - an earlier form loosened it across the raise, which
/// held the process-global umask open while sibling threads created files.
///
/// **The election is made through the umask around the bind**, so the
/// perturbation is the removal of that, not of a chmod: drop the
/// `Umask::deny_others()` guard from `Hook::raise` and this fails with
/// `0777`. Watched under exactly that removal.
///
/// conforms: gate-socket-mode-is-the-boundarys-election
#[test]
fn the_socket_denies_every_uid_outside_the_group() {
    use std::os::unix::fs::PermissionsExt;
    let path = scratch("socket-mode");
    // **The ambient umask is read rather than loosened.** An earlier form set
    // it to `0o000` across the raise so the test could not pass by accident
    // of the runner's mask, which meant holding the process-global umask
    // loose while a sibling thread might create a file under it. The last
    // assertion does that work instead, without the window.
    // **Where the runner's own umask would produce `0770`, this run cannot
    // tell an elected mode from an inherited one, so it reports that and
    // stops.** An earlier form asserted the distinguishability instead,
    // which turned a correct build red under a umask of `0007` - a vacuity
    // guard failing the build is the one thing a vacuity guard must not do.
    let ambient = ambient_umask();
    if 0o777 & !ambient == 0o770 {
        eprintln!(
            "SKIP the_socket_denies_every_uid_outside_the_group: the ambient \
             umask {ambient:04o} produces 0770 by itself"
        );
        return;
    }
    let hook = Hook::raise(&permissive_instruction(), &path).expect("the raise binds");

    let mode = std::fs::metadata(&path)
        .expect("the socket is on disk")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(
        mode, 0o770,
        "the socket states its mode rather than inheriting one, got {mode:04o}"
    );
    assert_eq!(
        mode & 0o007,
        0,
        "no bit outside owner and group, and write is what connecting needs"
    );
    drop(hook);
}

/// The ambient umask, read without holding it across anything.
///
/// **Reading it means setting it**, `umask(2)` returning the old value, so
/// even a read takes the lock this crate's own guard serializes on.
///
/// **That lock now reaches a sibling test calling `common::scratch`**, which
/// creates its directory under `with_umask_held` since the helpers were
/// shared, so the window this read opens is closed for every file the
/// fixtures create through it. What the read still exposes is the length of
/// two syscalls to any path creating a file in this binary by another
/// route, of which the suites have one today: the child log `entry.rs`
/// creates beside its scratch path.
///
/// **The hold is scoped and ends before the raise.** Holding it across would
/// deadlock, `Hook::raise` taking the same non-reentrant mutex, which is what
/// a first form of this did - `with_umask_held` is shaped so that cannot be
/// written by accident.
fn ambient_umask() -> u32 {
    weaver_gate::hook::with_umask_held(|| {
        let seen = nix::sys::stat::umask(nix::sys::stat::Mode::empty());
        nix::sys::stat::umask(seen);
        seen.bits()
    })
}

/// **The agent uid is denied by construction, not by configuration.**
///
/// The instruction here explicitly *permits* this process's own uid, which is
/// the operator mistake the design exists to survive. The raise adds this uid
/// to the deny set unconditionally, and denial wins over permission, so the
/// dial is refused anyway.
///
/// The test dials as this very process, which runs as the same uid the gate
/// does. That is exactly the reference walk's adversary: the agent's own tool
/// surface reaching the agent's own mouth.
///
/// Perturbation: remove the `rule.denied_uids.insert(getuid())` line from
/// `Hook::raise` and this test fails, because the operator's permissive rule
/// then admits the dial. Watched under exactly that removal.
#[test]
fn the_agent_uid_is_refused_even_when_the_rule_permits_it() {
    let path = scratch("agent-uid");
    let hook = Hook::raise(&permissive_instruction(), &path).expect("the raise binds");

    assert!(
        hook.rule()
            .denied_uids
            .contains(&nix::unistd::getuid().as_raw()),
        "the raise added this process's own uid to the deny set"
    );

    let mut client = UnixStream::connect(&path).expect("the dial reaches the listener");
    let outcome = hook.accept();
    assert_eq!(
        outcome.err(),
        Some(AcceptOutcome::Denied),
        "the agent's own uid is refused at accept"
    );

    // **Refused by closure, with nothing written back.** The client sees the
    // end of the connection rather than an answer: an admitted-looking reply to
    // a refused peer is a conversation the boundary already declined.
    let mut answer = Vec::new();
    client
        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .expect("a read bound");
    // Strictly `Ok(0)`: the peer wrote nothing before the refusal, so the
    // close is orderly and the read reaches end-of-stream rather than a reset.
    // Accepting any error here would have let a written answer followed by a
    // reset pass as a refusal.
    let read = client.read_to_end(&mut answer);
    assert_eq!(
        read.ok(),
        Some(0),
        "the refused peer reaches end-of-stream with nothing written, got {answer:?}"
    );

    hook.lower();
    std::fs::remove_file(&path).ok();
}

/// **Every connection is authenticated at accept, before any byte is read.**
///
/// The client writes immediately on connecting. The refusal must land without
/// that content ever being read, which is what "before any content is read"
/// means: the accept judges the credential the kernel supplies, not anything
/// the peer says about itself.
///
/// Perturbation: move the `authorized` call in `Hook::accept` to after a read
/// of the stream and this test still passes on the refusal, which is why what
/// it asserts is the *absence of an answer* rather than the refusal alone.
/// Removing the `authorized` check entirely fails it. Watched under exactly
/// that removal.
#[test]
fn a_peer_is_judged_before_its_content_is_read() {
    let path = scratch("judged-first");
    let hook = Hook::raise(&permissive_instruction(), &path).expect("the raise binds");

    let mut client = UnixStream::connect(&path).expect("the dial reaches the listener");
    client
        .write_all(b"{\"this\":\"is never read\"}\n")
        .expect("the client speaks first");

    assert_eq!(
        hook.accept().err(),
        Some(AcceptOutcome::Denied),
        "the credential decides, not the content"
    );

    hook.lower();
    std::fs::remove_file(&path).ok();
}

/// **The lower closes the listener first and confirms after**, so nothing new
/// can arrive once the harness proceeds. Read from outside: a dial after the
/// lower is refused by the kernel, the listener being gone.
///
/// Perturbation: make `Hook::lower` leak the listener with `std::mem::forget`
/// and this test fails, because the dial after the lower still connects.
/// Watched under exactly that change.
#[test]
fn a_dial_after_the_lower_finds_no_listener() {
    let path = scratch("after-lower");
    let hook = Hook::raise(&permissive_instruction(), &path).expect("the raise binds");
    assert!(
        UnixStream::connect(&path).is_ok(),
        "the standing hook accepts a dial"
    );

    hook.lower();

    assert!(
        UnixStream::connect(&path).is_err(),
        "once lowered, the path is a stale entry and nothing is listening"
    );
    // And the path itself survives: this crate unlinks nothing.
    assert!(
        path.exists(),
        "the operator's artifact is left where it was"
    );
    std::fs::remove_file(&path).ok();
}

/// **The bind refuses what it finds in the way, and unlinks nothing.**
///
/// Perturbation: add an unlink before the bind in `Hook::raise` and this test
/// fails, because the second raise then succeeds by deleting the first's
/// socket. Watched under exactly that addition.
#[test]
fn an_occupied_path_refuses_and_the_occupant_survives() {
    let path = scratch("occupied");
    let first = Hook::raise(&permissive_instruction(), &path).expect("the first binds");

    let second = Hook::raise(&permissive_instruction(), &path);
    assert!(second.is_err(), "an occupied path refuses the raise");

    // The first hook is untouched: its listener still answers.
    assert!(
        UnixStream::connect(&path).is_ok(),
        "the occupant was not unlinked out of the way"
    );

    first.lower();
    std::fs::remove_file(&path).ok();
}

/// **A raise from inside `with_umask_held` completes rather than hanging.**
///
/// `with_umask_held` is public and `Hook::raise` takes the same mutex, so the
/// natural call an external caller writes - hold the umask, bind under it -
/// took a non-reentrant lock twice on one thread. The scoped shape prevents
/// holding the guard across a later call and does nothing about nesting the
/// call inside, which is a thread deadlocking against itself with no second
/// party.
///
/// Perturbation: dropping the per-thread `HELD` check from
/// `Serialized::acquire` hangs this test rather than failing it, so it is
/// watched on a timer - the whole binary is killed by the harness on a hang,
/// and a watch whose failure mode is an infinite wait is not one this suite
/// can report on. Watched hanging 2026-08-29.
///
/// conforms: gate-umask-lock-is-reentrant-within-a-thread
#[test]
fn a_raise_nested_inside_the_umask_lock_completes() {
    let path = scratch("nested");
    let (done, waited) = std::sync::mpsc::channel();
    // On its own thread, so a deadlock is a timeout here rather than a hung
    // test binary the harness kills with no report.
    std::thread::spawn(move || {
        let raised = weaver_gate::hook::with_umask_held(|| {
            weaver_gate::hook::with_umask_held(|| Hook::raise(&permissive_instruction(), &path))
        });
        let _ = done.send(raised.is_ok());
    });
    match waited.recv_timeout(std::time::Duration::from_secs(10)) {
        Ok(raised) => assert!(raised, "the nested raise binds"),
        Err(_) => panic!("a raise nested inside with_umask_held deadlocked the thread"),
    }
}
