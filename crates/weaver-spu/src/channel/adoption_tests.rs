//! Inherited adopters share one ownership claim, exercised after isolated exec.
use super::*;
use std::io::Read;
use std::process::{Command, Stdio};

use crate::test_common as common;

#[derive(Debug)]
enum Held {
    Single(OwnedFd),
    Classify(ClassifySocket),
    Decoder(Inherited),
}
impl Held {
    fn fd(&self) -> BorrowedFd<'_> {
        match self {
            Self::Single(fd) => fd.as_fd(),
            Self::Classify(fd) => fd.end.as_fd(),
            Self::Decoder(ends) => ends.lifecycle.as_fd(),
        }
    }
}
unsafe fn adopt_named(name: &str) -> Result<Held, EntryFault> {
    // SAFETY: caller provides uniquely inherited raw ends for the first claim,
    // or proves the permanent claim already exists for a refused repeat.
    match name {
        "ste100" => unsafe { adopt_ste100() }.map(Held::Single),
        "classify" => unsafe { adopt_classify() }.map(Held::Classify),
        "decoder" => unsafe { adopt() }.map(Held::Decoder),
        _ => panic!("unknown isolated adoption case"),
    }
}
fn require_repeat_refusal(result: Result<Held, EntryFault>) {
    match result {
        Err(error) => assert_eq!(error, EntryFault::AlreadyAdopted),
        Ok(duplicate) => {
            // A deliberately removed guard must fail this assertion without
            // dropping a second owner of the original fd on the unwind path.
            std::mem::forget(duplicate);
            panic!("a second inherited adopter created another owner");
        }
    }
}
fn still_open(fd: BorrowedFd<'_>) {
    assert!(fcntl(fd, FcntlArg::F_GETFD).is_ok());
}
fn probe_child(test: &str, key: &str, case: &str, count: usize, complete: &str) {
    let pairs: Vec<_> = (0..count).map(|_| common::seqpacket_pair()).collect();
    let sources: Vec<_> = pairs.iter().map(|(_, child)| child.as_raw_fd()).collect();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", test, "--nocapture", "--test-threads=1"])
        .env(key, case)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    common::place_inherited(&mut command, &sources);
    let mut child = command.spawn().unwrap();
    let status = common::wait_bounded(&mut child, 30, "isolated inherited ownership claim");
    let mut stdout = String::new();
    let mut stderr = String::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut stdout)
        .unwrap();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    assert!(status.success(), "{case}: {stdout}\n{stderr}");
    assert_eq!(
        stdout.lines().filter(|line| *line == complete).count(),
        1,
        "the exact child must finish its descriptor checks: {case}: {stdout}\n{stderr}"
    );
    let mut packet = [0; 32];
    let read = recv(pairs[0].0.as_raw_fd(), &mut packet, MsgFlags::MSG_DONTWAIT).unwrap();
    assert_eq!(&packet[..read], b"original still owned");
}

#[test]
fn inherited_adoption_is_once_only_across_all_entry_points_and_fd_reuse() {
    const KEY: &str = "SPU_INHERITED_ADOPTION_CASE";
    const COMPLETE: &str = "SPU_SHARED_INHERITED_CLAIM_CONFIRMED";
    if let Some(case) = std::env::var_os(KEY) {
        let case = case.to_str().unwrap();
        let (first, second) = case.split_once(':').unwrap();
        // SAFETY: this isolated exec receives only the raw ends placed by
        // probe_child; no owner has wrapped either inherited number yet.
        let mut owned = unsafe { adopt_named(first) }.unwrap();
        assert_eq!(owned.fd().as_raw_fd(), 3);
        let mut extra = None;
        if first == "decoder" && second != "decoder" {
            let Held::Decoder(ends) = owned else {
                unreachable!()
            };
            drop(ends.decode);
            owned = Held::Single(ends.lifecycle.end);
        } else if first != "decoder" && second == "decoder" {
            let file = std::fs::File::open("/dev/null").unwrap();
            assert_eq!(file.as_raw_fd(), 4);
            extra = Some(file);
        }
        // SAFETY: the successful first call permanently holds the claim.
        require_repeat_refusal(unsafe { adopt_named(second) });
        still_open(owned.fd());
        if let Some(file) = &extra {
            still_open(file.as_fd());
        }
        let flags = fcntl(owned.fd(), FcntlArg::F_GETFD).unwrap();
        assert_ne!(flags & nix::libc::FD_CLOEXEC, 0);
        assert_eq!(unsafe { nix::libc::prctl(nix::libc::PR_GET_DUMPABLE) }, 0);
        assert_eq!(
            send(
                owned.fd().as_raw_fd(),
                b"original still owned",
                MsgFlags::empty()
            )
            .unwrap(),
            20
        );
        drop(owned);
        drop(extra);
        assert_eq!(unsafe { nix::libc::fcntl(3, nix::libc::F_GETFD) }, -1);
        // A new, real owned file reuses fd3. A repeated call must neither adopt
        // nor close it, even though the old inherited socket has been dropped.
        let reused = std::fs::File::open("/dev/null").unwrap();
        assert_eq!(reused.as_raw_fd(), 3);
        // SAFETY: the earlier adoption permanently holds the claim, so this
        // retry cannot wrap or close the genuinely owned reused descriptor.
        require_repeat_refusal(unsafe { adopt_ste100() }.map(Held::Single));
        still_open(reused.as_fd());
        println!("\n{COMPLETE}");
        return;
    }
    for case in [
        "ste100:ste100",
        "classify:classify",
        "ste100:classify",
        "classify:ste100",
        "decoder:decoder",
        "decoder:ste100",
        "decoder:classify",
        "ste100:decoder",
        "classify:decoder",
    ] {
        probe_child(
            "channel::adoption_tests::inherited_adoption_is_once_only_across_all_entry_points_and_fd_reuse",
            KEY,
            case,
            if case.starts_with("decoder:") { 2 } else { 1 },
            COMPLETE,
        );
    }
}

#[test]
fn refused_preownership_descriptor_validation_does_not_consume_the_first_claim() {
    const KEY: &str = "SPU_INHERITED_PRECHECK_CASE";
    const COMPLETE: &str = "SPU_PREOWNERSHIP_VALIDATION_CONFIRMED";
    if let Some(case) = std::env::var_os(KEY) {
        if case == "count" {
            assert!(matches!(
                // SAFETY: only raw inherited ends exist; count refusal occurs
                // before ownership of either can be transferred.
                unsafe { adopt_ste100() },
                Err(EntryFault::DescriptorCountWrong { found: 2 })
            ));
            assert_eq!(unsafe { nix::libc::close(4) }, 0);
        } else {
            assert_eq!(unsafe { nix::libc::dup2(3, 4) }, 4);
            assert_eq!(unsafe { nix::libc::close(3) }, 0);
            assert!(matches!(
                // SAFETY: the raw inherited end was moved to fd4 without a
                // Rust owner; validation rejects the now-closed fd3.
                unsafe { adopt_classify() },
                Err(EntryFault::DescriptorsUnusable)
            ));
            assert_eq!(unsafe { nix::libc::dup2(4, 3) }, 3);
            assert_eq!(unsafe { nix::libc::close(4) }, 0);
        }
        // SAFETY: the raw inherited fd3 has no other owner; both preceding
        // malformed-layout cases refused before consuming the claim.
        let owned = unsafe { adopt_ste100() }.unwrap();
        still_open(owned.as_fd());
        assert_eq!(
            send(
                owned.as_raw_fd(),
                b"original still owned",
                MsgFlags::empty()
            )
            .unwrap(),
            20
        );
        drop(owned);
        println!("\n{COMPLETE}");
        return;
    }
    for case in ["count", "number"] {
        probe_child(
            "channel::adoption_tests::refused_preownership_descriptor_validation_does_not_consume_the_first_claim",
            KEY,
            case,
            if case == "count" { 2 } else { 1 },
            COMPLETE,
        );
    }
}
