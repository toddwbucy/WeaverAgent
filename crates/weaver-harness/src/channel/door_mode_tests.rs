//! Test-only: the `door_mode_tests` suite of `channel.rs`,
//! moved out of the production file by #118.

use super::*;

/// **The coordination door denies every uid but its owner.**
///
/// `bind` sets no mode, so this name would land at `0777 & ~umask` and
/// the door's permissions would be whatever umask the process inherited -
/// `0777` on one box and `0775` on another from one build, on
/// 2026-08-28. Only `uid() == 0` is admitted at `accept_root`, so no
/// group reaches this door and `0700` is the access it offers.
///
/// Where the runner's own umask produces `0700` this run cannot tell an
/// elected mode from an inherited one, so it says so and skips rather
/// than asserting against a run that cannot show the property.
///
/// Perturbation: drop the `CoordinationUmask::deny_all_but_owner()` guard
/// from `bind_coordination` and this reports the runner's own mode.
/// Watched under exactly that removal, and watched **here** - the form
/// this replaced skipped wherever the ambient umask already produced
/// `0700`, which `0077` does, so on a hardened image it ran nowhere while
/// its record said otherwise.
///
/// conforms: harness-coordination-door-states-its-mode
#[test]
fn the_coordination_door_denies_every_uid_but_its_owner() {
    use std::os::unix::fs::PermissionsExt;
    let dir = crate::scratch::dir(format!(
        "weaver-coordination-mode-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let path = dir.join("coordination.sock");

    // **The umask is elected rather than read, so this runs everywhere.**
    // An earlier form read the ambient value and skipped where it already
    // produced `0700`, which `0077` does - a common hardened default, so
    // the watch skipped on a hardened image and a later removal of the
    // guard would have shipped green. `0o000` cannot produce `0700` by
    // itself, so under it the mode on disk is the election or nothing.
    //
    // The lock is held across the window and is reentrant, so
    // `bind_coordination` taking it again inside is a no-op.
    let (listener, seen) = with_umask_held(|| {
        let previous = nix::sys::stat::umask(nix::sys::stat::Mode::empty());
        let bound = bind_coordination(&path);
        let seen = std::fs::metadata(&path).map(|meta| meta.permissions().mode() & 0o777);
        nix::sys::stat::umask(previous);
        (bound, seen)
    });
    let listener = listener.expect("the name binds");
    let mode = seen.expect("the socket is on disk");
    assert_eq!(
        mode, 0o700,
        "the door states its mode rather than inheriting one, got {mode:04o}"
    );
    drop(listener);
}

/// **The gate's socket is the one the composition root handed**, per
/// `weaver-harness-Spec` section 3: a listener given the gate's door answers
/// that path, not a name beside the coordination socket, and the raise the
/// harness builds carries it. Perturbation: have `gate_socket` ignore the
/// handed path, and the first assertion reads the sibling.
#[test]
fn the_gate_socket_handed_is_the_one_answered() {
    let dir = crate::scratch::dir(format!(
        "weaver-gate-handed-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let door = dir
        .join("weaver.run")
        .join("a")
        .join("gate")
        .join("gate.sock");
    let listener = bind_coordination(&dir.join("coordination.sock"))
        .expect("bind")
        .with_gate_socket(door.clone());
    assert_eq!(listener.gate_socket(), door);
    let bare = bind_coordination(&dir.join("other.sock")).expect("bind");
    assert_eq!(bare.gate_socket(), dir.join("gate.sock"));
}
