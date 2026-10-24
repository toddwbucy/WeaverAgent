//! Safe inherited adopters share one ownership claim, exercised after exec.
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
fn adopt_named(name: &str) -> Result<Held, EntryFault> {
    match name {
        "ste100" => adopt_ste100().map(Held::Single),
        "classify" => adopt_classify().map(Held::Classify),
        "decoder" => adopt().map(Held::Decoder),
        _ => panic!("unknown isolated adoption case"),
    }
}
fn require_repeat_refusal(result: Result<Held, EntryFault>) {
    match result {
        Err(error) => assert_eq!(error, EntryFault::DescriptorsUnusable),
        Ok(duplicate) => {
            // A deliberately removed guard must fail this assertion without
            // dropping a second owner of the original fd on the unwind path.
            std::mem::forget(duplicate);
            panic!("a second safe inherited adopter created another owner");
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
fn inherited_adoption_is_once_only_across_all_safe_entry_points_and_fd_reuse() {
    const KEY: &str = "SPU_INHERITED_ADOPTION_CASE";
    const COMPLETE: &str = "SPU_SHARED_INHERITED_CLAIM_CONFIRMED";
    if let Some(case) = std::env::var_os(KEY) {
        let case = case.to_str().unwrap();
        let (first, second) = case.split_once(':').unwrap();
        let mut owned = adopt_named(first).unwrap();
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
        require_repeat_refusal(adopt_named(second));
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
        require_repeat_refusal(adopt_ste100().map(Held::Single));
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
            "channel::adoption_tests::inherited_adoption_is_once_only_across_all_safe_entry_points_and_fd_reuse",
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
                adopt_ste100(),
                Err(EntryFault::DescriptorCountWrong { found: 2 })
            ));
            assert_eq!(unsafe { nix::libc::close(4) }, 0);
        } else {
            assert_eq!(unsafe { nix::libc::dup2(3, 4) }, 4);
            assert_eq!(unsafe { nix::libc::close(3) }, 0);
            assert!(matches!(
                adopt_classify(),
                Err(EntryFault::DescriptorsUnusable)
            ));
            assert_eq!(unsafe { nix::libc::dup2(4, 3) }, 3);
            assert_eq!(unsafe { nix::libc::close(4) }, 0);
        }
        let owned = adopt_ste100().unwrap();
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
