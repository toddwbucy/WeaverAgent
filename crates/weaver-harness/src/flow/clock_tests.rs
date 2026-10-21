use super::testing::FakePumpIo;
use super::*;
use std::io::Write;
use std::os::fd::AsFd;
use std::os::unix::net::UnixStream;

#[test]
fn real_wait_preserves_write_half_closure_without_a_read_interest() {
    // A full completion slot deliberately disables POLLIN. Half-close must
    // still wake the source, even though nix's PollFlags has no named bit for
    // Linux POLLRDHUP and its checked conversion would discard this result.
    for readable in [false, true] {
        let (near, peer) = UnixStream::pair().unwrap();
        peer.shutdown(std::net::Shutdown::Write).unwrap();
        let mut events = PollFlags::from_bits_retain(nix::libc::POLLRDHUP);
        if readable {
            events |= PollFlags::POLLIN;
        }
        let mut io = MonotonicIo;
        let deadline = io.now().unwrap().checked_add(1_000_000_000).unwrap();
        let result = io
            .wait(
                &[WaitInterest {
                    fd: near.as_fd(),
                    events,
                }],
                Some(deadline),
            )
            .unwrap();
        let WaitOutcome::Ready(ready) = result else {
            panic!("write-half closure was hidden until the deadline");
        };
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].index, 0);
        assert_ne!(ready[0].events.bits() & nix::libc::POLLRDHUP, 0);
        assert_eq!(ready[0].events.contains(PollFlags::POLLIN), readable);
    }
}

#[test]
fn origin_and_absolute_bounds_use_checked_numeric_arithmetic() {
    let mut io = FakePumpIo::new(9_007_199_254_740_993);
    let origin = ClockOrigin::capture(&mut io).unwrap();
    assert_eq!(origin.raw(), 9_007_199_254_740_993);
    assert_eq!(ClockOrigin::at(origin.raw()), origin);
    assert_eq!(origin.elapsed(origin.raw()).unwrap(), 0);
    assert_eq!(origin.elapsed(origin.raw() + 17).unwrap(), 17);
    assert_eq!(origin.elapsed(origin.raw() - 1), Err(BoundError::Reversed));

    let bound = AbsoluteBound::after(origin.raw(), 37).unwrap();
    assert_eq!(bound.began_ns(), origin.raw());
    assert_eq!(bound.deadline_ns(), origin.raw() + 37);
    assert_eq!(bound.elapsed(origin.raw() + 12).unwrap(), 12);
    assert_eq!(bound.elapsed(origin.raw() - 1), Err(BoundError::Reversed));
    assert!(!bound.expired(bound.deadline_ns() - 1));
    assert!(bound.expired(bound.deadline_ns()));
    assert!(bound.expired(bound.deadline_ns() + 1));
    assert_eq!(AbsoluteBound::after(u64::MAX, 1), Err(BoundError::Overflow));
    assert_eq!(
        AbsoluteBound::from_absolute(2, 1),
        Err(BoundError::Reversed)
    );
    let exhausted = AbsoluteBound::after(8, 0).unwrap();
    assert!(
        exhausted.expired(8),
        "an exhausted inherited budget cannot renew"
    );
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TestBound {
    Gate,
    Decision,
    Leave,
}

#[test]
fn repeated_probes_and_partial_progress_cannot_rearm_or_extend_a_bound() {
    let mut deadlines = DeadlineRegistry::new();
    let original = AbsoluteBound::after(100, 40).unwrap();
    deadlines.arm(TestBound::Decision, original).unwrap();
    for progress_at in [101, 115, 139, 140, 175] {
        assert_eq!(
            deadlines.arm(
                TestBound::Decision,
                AbsoluteBound::after(progress_at, 40).unwrap(),
            ),
            Err(BoundError::AlreadyArmed)
        );
        assert_eq!(deadlines.get(TestBound::Decision), Some(original));
    }
    assert_eq!(
        deadlines.shorten(TestBound::Decision, 141),
        Err(BoundError::Extension)
    );
    assert_eq!(
        deadlines.shorten(TestBound::Decision, 99),
        Err(BoundError::Reversed)
    );
    assert_eq!(deadlines.get(TestBound::Decision), Some(original));
    deadlines.shorten(TestBound::Decision, 125).unwrap();
    let shortened = deadlines.get(TestBound::Decision).unwrap();
    assert_eq!(
        shortened.began_ns(),
        100,
        "a force join retains the first anchor"
    );
    assert_eq!(shortened.deadline_ns(), 125);
    assert_eq!(
        deadlines.shorten(TestBound::Gate, 110),
        Err(BoundError::NotArmed)
    );
    assert_eq!(deadlines.disarm(TestBound::Decision), Some(shortened));
    assert_eq!(deadlines.disarm(TestBound::Decision), None);
    assert_eq!(deadlines.nearest(), None);
    // Only the owner's explicit completed-disposition removal permits a new barrier.
    deadlines
        .arm(TestBound::Decision, AbsoluteBound::after(200, 40).unwrap())
        .unwrap();
    assert_eq!(deadlines.get(TestBound::Decision).unwrap().began_ns(), 200);
}

#[test]
fn nearest_and_all_due_bounds_do_not_choose_failure_precedence() {
    let mut deadlines = DeadlineRegistry::new();
    assert!(deadlines.expired(u64::MAX).next().is_none());
    deadlines
        .arm(TestBound::Leave, AbsoluteBound::after(1, 99).unwrap())
        .unwrap();
    deadlines
        .arm(TestBound::Decision, AbsoluteBound::after(20, 40).unwrap())
        .unwrap();
    deadlines
        .arm(TestBound::Gate, AbsoluteBound::after(30, 30).unwrap())
        .unwrap();
    assert_eq!(deadlines.nearest(), Some(60));
    assert!(deadlines.expired(59).next().is_none());
    let due: Vec<_> = deadlines.expired(60).collect();
    assert_eq!(due.len(), 2, "both equal deadlines must reach their owner");
    assert!(due.iter().any(|(key, _)| *key == TestBound::Gate));
    assert!(due.iter().any(|(key, _)| *key == TestBound::Decision));
    assert_eq!(deadlines.expired(100).count(), 3);
    assert_eq!(
        deadlines.nearest(),
        Some(60),
        "inspection cannot clear a bound"
    );
    deadlines.disarm(TestBound::Gate).unwrap();
    assert_eq!(deadlines.nearest(), Some(60));
    deadlines.disarm(TestBound::Decision).unwrap();
    assert_eq!(deadlines.nearest(), Some(100));
}

#[test]
fn timespec_and_poll_conversion_reject_overflow_and_round_up_without_wrapping() {
    assert_eq!(timespec_ns(12, 34).unwrap(), 12_000_000_034);
    for (seconds, nanos) in [(-1, 0), (0, -1), (0, 1_000_000_000), (i64::MAX, 0)] {
        assert_eq!(
            timespec_ns(seconds, nanos).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }
    assert_eq!(poll_timeout(0, None), PollTimeout::NONE);
    assert_eq!(poll_timeout(7, Some(7)), PollTimeout::ZERO);
    assert_eq!(poll_timeout(8, Some(7)), PollTimeout::ZERO);
    for remaining in [1, 999_999, 1_000_000] {
        assert_eq!(
            poll_timeout(100, Some(100 + remaining)).as_millis(),
            Some(1)
        );
    }
    assert_eq!(poll_timeout(0, Some(1_000_001)).as_millis(), Some(2));
    assert_eq!(poll_timeout(0, Some(u64::MAX)), PollTimeout::MAX);
    assert_eq!(
        poll_timeout(0, Some((i32::MAX as u64 + 1) * NANOS_PER_MILLISECOND)),
        PollTimeout::MAX,
        "a long wait is chunked, never wrapped or declared expired",
    );
}

#[test]
fn production_wait_loop_checks_the_clock_after_ready_and_propagates_failures() {
    use std::cell::Cell;
    let time = Cell::new(5);
    let ready = vec![ReadyFd {
        index: 0,
        events: PollFlags::POLLIN,
    }];
    let result = wait_until(
        Some(10),
        || Ok(time.get()),
        |_| {
            time.set(10);
            Ok(ready.clone())
        },
    );
    assert_eq!(result.unwrap(), WaitOutcome::Timeout);

    let calls = Cell::new(0);
    assert_eq!(
        wait_until(
            Some(10),
            || {
                calls.set(calls.get() + 1);
                if calls.get() == 1 {
                    Ok(5)
                } else {
                    Err(io::Error::other("clock failed after poll"))
                }
            },
            |_| Ok(ready.clone()),
        )
        .unwrap_err()
        .kind(),
        io::ErrorKind::Other,
    );
    time.set(5);
    assert_eq!(
        wait_until(
            Some(10),
            || Ok(time.get()),
            |_| {
                time.set(4);
                Ok(ready.clone())
            },
        )
        .unwrap_err()
        .kind(),
        io::ErrorKind::InvalidData,
    );
    assert_eq!(
        wait_until(
            Some(10),
            || Ok(5),
            |_| Err(io::Error::from(io::ErrorKind::PermissionDenied)),
        )
        .unwrap_err()
        .kind(),
        io::ErrorKind::PermissionDenied,
    );
    assert_eq!(
        wait_until(Some(5), || Ok(5), |_| panic!("expired wait reached poll")).unwrap(),
        WaitOutcome::Timeout,
    );
}

#[test]
fn production_wait_loop_recomputes_eintr_and_long_chunks_from_the_original_deadline() {
    use std::cell::Cell;
    let chunk = i32::MAX as u64 * NANOS_PER_MILLISECOND;
    let deadline = 2 * chunk + 11;
    let time = Cell::new(0);
    let mut timeouts = Vec::new();
    let result = wait_until(
        Some(deadline),
        || Ok(time.get()),
        |timeout| {
            timeouts.push(timeout);
            match timeouts.len() {
                1 => {
                    time.set(chunk);
                    Err(io::Error::from(io::ErrorKind::Interrupted))
                }
                2 => {
                    time.set(2 * chunk);
                    Ok(Vec::new())
                }
                3 => {
                    time.set(deadline);
                    Ok(Vec::new())
                }
                _ => panic!("the original deadline was renewed"),
            }
        },
    );
    assert_eq!(result.unwrap(), WaitOutcome::Timeout);
    assert_eq!(
        timeouts,
        vec![PollTimeout::MAX, PollTimeout::MAX, PollTimeout::from(1u16)]
    );
    assert_eq!(time.get(), deadline);
}

#[test]
fn fake_wait_advances_one_clock_with_readiness_and_preserves_deadline_priority() {
    let file = std::fs::File::open("/dev/null").unwrap();
    let interests = [WaitInterest {
        fd: file.as_fd(),
        events: PollFlags::POLLIN,
    }];
    let ready = vec![ReadyFd {
        index: 0,
        events: PollFlags::POLLIN,
    }];
    let mut io = FakePumpIo::new(100);
    io.schedule(110, ready.clone()).unwrap();
    io.schedule(120, ready.clone()).unwrap();
    assert_eq!(
        io.wait(&interests, Some(120)).unwrap(),
        WaitOutcome::Ready(ready.clone())
    );
    assert_eq!(io.now().unwrap(), 110);
    assert_eq!(
        io.wait(&interests, Some(120)).unwrap(),
        WaitOutcome::Timeout
    );
    assert_eq!(io.now().unwrap(), 120);
    assert_eq!(
        io.wait(&interests, Some(120)).unwrap(),
        WaitOutcome::Timeout
    );
    // Timeout did not consume the answer sitting exactly at the terminal bound.
    assert_eq!(
        io.wait(&interests, None).unwrap(),
        WaitOutcome::Ready(ready)
    );
    assert_eq!(io.now().unwrap(), 120);
    assert_eq!(io.wait(&[], Some(137)).unwrap(), WaitOutcome::Timeout);
    assert_eq!(io.now().unwrap(), 137);
    assert_eq!(io.waits()[0].began_ns, 100);
    assert_eq!(io.waits()[0].absolute_deadline, Some(120));
    assert_eq!(
        io.waits()[0].descriptors,
        vec![(file.as_raw_fd(), PollFlags::POLLIN)]
    );
    assert_eq!(
        io.wait(&[], None).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    assert!(io.advance_to(136).is_err());
    io.advance_to(150).unwrap();
    assert_eq!(io.now().unwrap(), 150);
}

#[test]
fn fake_ready_traffic_keeps_the_original_nearest_deadline() {
    let file = std::fs::File::open("/dev/null").unwrap();
    let interests = [WaitInterest {
        fd: file.as_fd(),
        events: PollFlags::POLLIN,
    }];
    let mut io = FakePumpIo::new(10);
    let mut deadlines = DeadlineRegistry::new();
    deadlines
        .arm(TestBound::Decision, AbsoluteBound::after(10, 90).unwrap())
        .unwrap();
    deadlines
        .arm(TestBound::Gate, AbsoluteBound::after(10, 30).unwrap())
        .unwrap();
    for at in [11, 12, 18, 25, 38, 39, 40] {
        io.schedule(
            at,
            vec![ReadyFd {
                index: 0,
                events: PollFlags::POLLIN,
            }],
        )
        .unwrap();
    }
    for _ in 0..6 {
        assert!(matches!(
            io.wait(&interests, deadlines.nearest()).unwrap(),
            WaitOutcome::Ready(_)
        ));
        assert_eq!(deadlines.get(TestBound::Gate).unwrap().began_ns(), 10);
    }
    assert_eq!(
        io.wait(&interests, deadlines.nearest()).unwrap(),
        WaitOutcome::Timeout
    );
    assert_eq!(io.now().unwrap(), 40);
    assert_eq!(
        deadlines
            .expired(io.now().unwrap())
            .map(|(key, _)| key)
            .collect::<Vec<_>>(),
        vec![TestBound::Gate]
    );
    assert!(
        io.waits()
            .iter()
            .all(|wait| wait.absolute_deadline == Some(40))
    );
}

#[test]
fn duplicate_descriptors_and_invalid_fake_wakes_refuse_before_waiting() {
    let file = std::fs::File::open("/dev/null").unwrap();
    let interest = WaitInterest {
        fd: file.as_fd(),
        events: PollFlags::POLLIN,
    };
    let mut real = MonotonicIo;
    assert_eq!(
        real.wait(&[interest, interest], None).unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
    let mut fake = FakePumpIo::new(0);
    assert_eq!(
        fake.wait(&[interest, interest], None).unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
    assert!(fake.waits().is_empty());
    assert!(fake.schedule(1, Vec::new()).is_err());
    fake.schedule(
        1,
        vec![ReadyFd {
            index: 1,
            events: PollFlags::POLLIN,
        }],
    )
    .unwrap();
    assert_eq!(
        fake.wait(&[interest], Some(10)).unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
    fake.schedule(
        2,
        vec![ReadyFd {
            index: 0,
            events: PollFlags::POLLOUT,
        }],
    )
    .unwrap();
    assert_eq!(
        fake.wait(&[interest], Some(10)).unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
    fake.schedule(
        3,
        vec![ReadyFd {
            index: 0,
            events: PollFlags::POLLIN | PollFlags::POLLOUT,
        }],
    )
    .unwrap();
    assert_eq!(
        fake.wait(&[interest], Some(10)).unwrap_err().kind(),
        io::ErrorKind::InvalidInput,
        "one requested flag cannot license an unrequested ordinary flag",
    );
}

// A broken wait cannot hang the test runner. The child exercises real poll and
// sockets; Instant is only the separate parent's test watchdog, never PumpIo.
fn under_watchdog(name: &str, body: impl FnOnce()) {
    const WATCHDOG_CASE: &str = "WEAVER_HARNESS_CLOCK_WATCHDOG";
    if std::env::var(WATCHDOG_CASE).as_deref() == Ok(name) {
        body();
        return;
    }
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture", "--test-threads=1"])
        .env(WATCHDOG_CASE, name)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if child.try_wait().unwrap().is_some() {
            let output = child.wait_with_output().unwrap();
            assert!(
                output.status.success(),
                "real clock instrument failed: {}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
            return;
        }
        if std::time::Instant::now() >= deadline {
            child.kill().unwrap();
            let output = child.wait_with_output().unwrap();
            panic!(
                "real clock instrument exceeded watchdog: {}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[test]
fn real_poll_reports_socket_readiness_and_peer_loss_without_consuming_bytes() {
    under_watchdog(
        "flow::clock::tests::real_poll_reports_socket_readiness_and_peer_loss_without_consuming_bytes",
        || {
            let (mut near, mut far) = UnixStream::pair().unwrap();
            let mut io = MonotonicIo;
            far.write_all(b"ready").unwrap();
            let deadline = AbsoluteBound::after(io.now().unwrap(), 1_000_000_000).unwrap();
            let result = io
                .wait(
                    &[WaitInterest {
                        fd: near.as_fd(),
                        events: PollFlags::POLLIN,
                    }],
                    Some(deadline.deadline_ns()),
                )
                .unwrap();
            assert!(
                matches!(result, WaitOutcome::Ready(ref ready) if ready.len() == 1 && ready[0].index == 0 && ready[0].events.contains(PollFlags::POLLIN))
            );
            let mut bytes = [0; 5];
            std::io::Read::read_exact(&mut near, &mut bytes).unwrap();
            assert_eq!(
                &bytes, b"ready",
                "the wait must not own a second read cursor"
            );
            drop(far);
            let result = io
                .wait(
                    &[WaitInterest {
                        fd: near.as_fd(),
                        events: PollFlags::empty(),
                    }],
                    Some(deadline.deadline_ns()),
                )
                .unwrap();
            assert!(
                matches!(result, WaitOutcome::Ready(ref ready) if ready[0].events.contains(PollFlags::POLLHUP))
            );
        },
    );
}

#[test]
fn real_poll_ready_data_cannot_beat_an_already_expired_deadline() {
    under_watchdog(
        "flow::clock::tests::real_poll_ready_data_cannot_beat_an_already_expired_deadline",
        || {
            let (near, mut far) = UnixStream::pair().unwrap();
            far.write_all(b"ready").unwrap();
            let mut io = MonotonicIo;
            let deadline = io.now().unwrap();
            assert_eq!(
                io.wait(
                    &[WaitInterest {
                        fd: near.as_fd(),
                        events: PollFlags::POLLIN
                    }],
                    Some(deadline)
                )
                .unwrap(),
                WaitOutcome::Timeout
            );
            let begin = io.now().unwrap();
            let deadline = AbsoluteBound::after(begin, 5_000_001).unwrap();
            assert_eq!(
                io.wait(&[], Some(deadline.deadline_ns())).unwrap(),
                WaitOutcome::Timeout
            );
            assert!(
                io.now().unwrap() >= deadline.deadline_ns(),
                "poll rounding cannot expire the bound early"
            );
        },
    );
}

#[test]
fn real_poll_eintr_keeps_one_absolute_deadline_under_a_signal_stream() {
    under_watchdog(
        "flow::clock::tests::real_poll_eintr_keeps_one_absolute_deadline_under_a_signal_stream",
        || {
            use std::sync::Arc;
            use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
            static SIGNALS: AtomicUsize = AtomicUsize::new(0);
            extern "C" fn interrupted(_: nix::libc::c_int) {
                SIGNALS.fetch_add(1, Ordering::Relaxed);
            }
            // This isolated test process runs exactly this test. SIGUSR1 is directed
            // to its polling thread, with the prior handler and mask restored below.
            let mut action: nix::libc::sigaction = unsafe { std::mem::zeroed() };
            let mut prior_action: nix::libc::sigaction = unsafe { std::mem::zeroed() };
            action.sa_sigaction = interrupted as *const () as usize;
            action.sa_flags = 0;
            let mut signal_set: nix::libc::sigset_t = unsafe { std::mem::zeroed() };
            let mut prior_mask: nix::libc::sigset_t = unsafe { std::mem::zeroed() };
            // SAFETY: all signal structures are valid stack storage; the handler
            // touches only a lock-free atomic and the saved thread lives until join.
            let thread = unsafe {
                assert_eq!(nix::libc::sigemptyset(&mut action.sa_mask), 0);
                assert_eq!(
                    nix::libc::sigaction(nix::libc::SIGUSR1, &action, &mut prior_action),
                    0
                );
                assert_eq!(nix::libc::sigemptyset(&mut signal_set), 0);
                assert_eq!(nix::libc::sigaddset(&mut signal_set, nix::libc::SIGUSR1), 0);
                assert_eq!(
                    nix::libc::pthread_sigmask(
                        nix::libc::SIG_UNBLOCK,
                        &signal_set,
                        &mut prior_mask
                    ),
                    0
                );
                nix::libc::pthread_self()
            };
            let stop = Arc::new(AtomicBool::new(false));
            let finished = stop.clone();
            let sender = std::thread::spawn(move || {
                while !finished.load(Ordering::Acquire) {
                    std::thread::sleep(std::time::Duration::from_millis(2));
                    // SAFETY: the polling thread remains alive until this sender joins.
                    assert_eq!(
                        unsafe { nix::libc::pthread_kill(thread, nix::libc::SIGUSR1) },
                        0
                    );
                }
            });
            let mut io = MonotonicIo;
            let began = io.now().unwrap();
            let deadline = AbsoluteBound::after(began, 75_000_000).unwrap();
            let result = io.wait(&[], Some(deadline.deadline_ns()));
            let ended = io.now().unwrap();
            stop.store(true, Ordering::Release);
            sender.join().unwrap();
            // SAFETY: the signal sender is joined and the saved values came from the
            // successful signal operations above.
            unsafe {
                assert_eq!(
                    nix::libc::pthread_sigmask(
                        nix::libc::SIG_SETMASK,
                        &prior_mask,
                        std::ptr::null_mut()
                    ),
                    0
                );
                assert_eq!(
                    nix::libc::sigaction(nix::libc::SIGUSR1, &prior_action, std::ptr::null_mut()),
                    0
                );
            }
            assert_eq!(result.unwrap(), WaitOutcome::Timeout);
            assert!(
                SIGNALS.load(Ordering::Relaxed) > 0,
                "the real poll was never interrupted"
            );
            assert!(ended >= deadline.deadline_ns());
            assert!(
                ended - began < 1_000_000_000,
                "signal traffic renewed or hid the original deadline"
            );
        },
    );
}
