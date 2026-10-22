use super::*;
use std::io::Read;
use std::os::fd::AsRawFd;
use std::time::Duration;

// A failed assertion can unwind before the reader starts draining. The writer's
// Drop joins its worker, so its real socket write must also have a finite bound.
// Reader timeouts similarly keep a missing sink close from hanging a join.
fn bounded_pair() -> (UnixStream, UnixStream) {
    let (sink, reader) = UnixStream::pair().unwrap();
    sink.set_write_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    reader
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    (sink, reader)
}

fn drained(writer: &Writer) {
    assert_eq!(
        writer
            .drain_by(Instant::now() + Duration::from_secs(3))
            .unwrap(),
        Drained::Emptied
    );
}

#[repr(C)]
struct PollFd {
    fd: i32,
    events: i16,
    revents: i16,
}
unsafe extern "C" {
    fn poll(fds: *mut PollFd, nfds: usize, timeout: i32) -> i32;
}
fn readable(fd: BorrowedFd<'_>, timeout_ms: i32) -> bool {
    let mut entry = PollFd {
        fd: fd.as_raw_fd(),
        events: 1,
        revents: 0,
    };
    // A single initialized pollfd remains alive for this synchronous syscall.
    let result = unsafe { poll(&mut entry, 1, timeout_ms) };
    assert!(result >= 0);
    result == 1 && entry.revents & 1 != 0
}
fn line(text: &str) -> Line {
    Arc::from(format!("{{\"text\":\"{text}\"}}\n"))
}

#[test]
fn real_stalled_sink_keeps_1017_ordinary_and_seven_distinct_terminal_slots() {
    let (sink, mut reader) = bounded_pair();
    let writer = Writer::start(sink.into()).unwrap();
    let large = line(&"x".repeat(1_000_000));
    writer
        .reserve(None)
        .unwrap()
        .unwrap()
        .enqueue(Sequence(0), large);
    for sequence in 1..ORDINARY_CAPACITY {
        writer
            .reserve(None)
            .unwrap()
            .unwrap()
            .enqueue(Sequence(sequence as u64), line("ordinary"));
    }
    assert_eq!(writer.pressure().queued, ORDINARY_CAPACITY);
    assert!(writer.reserve(None).unwrap().is_none());
    for (offset, permit) in [
        Terminal::Fault,
        Terminal::ToolCompleted,
        Terminal::ToolResult,
        Terminal::TurnClosed,
        Terminal::SessionClosed,
        Terminal::SaveRefused,
        Terminal::Unload,
    ]
    .into_iter()
    .enumerate()
    {
        writer.reserve(Some(permit)).unwrap().unwrap().enqueue(
            Sequence((ORDINARY_CAPACITY + offset) as u64),
            line("terminal"),
        );
        assert!(writer.reserve(Some(permit)).is_err(), "{permit:?}");
    }
    assert_eq!(writer.pressure().queued, QUEUE_DEPTH);
    assert_eq!(writer.pressure().peak, QUEUE_DEPTH);
    assert!(writer.reserve(None).unwrap().is_none());
    assert!(!readable(writer.capacity_fd(), 0));
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).unwrap();
        bytes
    });
    assert!(
        readable(writer.capacity_fd(), 2000),
        "capacity transition must wake the poll owner"
    );
    assert_eq!(
        writer
            .drain_by(Instant::now() + Duration::from_secs(2))
            .unwrap(),
        Drained::Emptied
    );
    let boundary = writer.boundary(Some(Sequence(1023)));
    assert_eq!(boundary.committed, Some(Sequence(1023)));
    assert_eq!(boundary.queued, 0);
    assert!(
        writer.reserve(Some(Terminal::Fault)).is_err(),
        "a committed permit is not reusable"
    );
    drop(writer);
    let bytes = reader.join().unwrap();
    assert_eq!(bytes.iter().filter(|&&b| b == b'\n').count(), QUEUE_DEPTH);
}

#[test]
fn cancelling_a_reservation_returns_its_slot_and_terminal_permit_and_wakes() {
    let (sink, _reader) = bounded_pair();
    let mut writer = Writer::start(sink.into()).unwrap();
    let mut held = Vec::new();
    for _ in 0..ORDINARY_CAPACITY {
        held.push(writer.reserve(None).unwrap().unwrap());
    }
    assert!(writer.reserve(None).unwrap().is_none());
    assert_eq!(
        writer.pressure().peak,
        0,
        "reservations are not admitted events"
    );
    drop(held.pop());
    assert!(readable(writer.capacity_fd(), 1000));
    writer.drain_capacity_wake().unwrap();
    assert!(!readable(writer.capacity_fd(), 0));
    assert!(writer.reserve(None).unwrap().is_some());
    drop(held);
    drop(writer.reserve(Some(Terminal::Fault)).unwrap().unwrap());
    drop(writer.reserve(Some(Terminal::Fault)).unwrap().unwrap());
    assert_eq!(writer.pressure().queued, 0);
}

#[test]
fn sink_failure_is_standing_and_wakes_the_same_poll_descriptor() {
    let (sink, reader) = bounded_pair();
    drop(reader);
    let mut writer = Writer::start(sink.into()).unwrap();
    writer
        .reserve(None)
        .unwrap()
        .unwrap()
        .enqueue(Sequence(12), line("fail"));
    assert!(readable(writer.capacity_fd(), 1000));
    assert!(matches!(
        writer.drain_capacity_wake(),
        Err(Failure::CommitFailed {
            sequence: Sequence(12),
            ..
        })
    ));
    assert!(matches!(
        writer.reserve(None),
        Err(Failure::CommitFailed {
            sequence: Sequence(12),
            ..
        })
    ));
    assert!(
        writer
            .drain_by(Instant::now() + Duration::from_secs(3))
            .is_err()
    );
    assert_eq!(writer.boundary(Some(Sequence(12))).committed, None);
}

#[test]
fn a_broken_capacity_notifier_is_a_trace_failure() {
    let (sink, _reader) = bounded_pair();
    let writer = Writer::start(sink.into()).unwrap();
    let mut held = Vec::new();
    for _ in 0..ORDINARY_CAPACITY {
        held.push(writer.reserve(None).unwrap().unwrap());
    }
    writer.wake.shutdown(std::net::Shutdown::Read).unwrap();
    drop(held.pop());
    assert!(matches!(
        writer.standing_failure(),
        Some(Failure::WriteTargetUnusable { .. })
    ));
    assert!(writer.reserve(None).is_err());
    drop(held);
}

#[test]
fn bounded_drain_and_abandon_do_not_join_a_stalled_write() {
    let (sink, reader) = bounded_pair();
    let writer = Writer::start(sink.into()).unwrap();
    writer
        .reserve(None)
        .unwrap()
        .unwrap()
        .enqueue(Sequence(0), line(&"x".repeat(1_000_000)));
    assert_eq!(
        writer.drain_by(Instant::now()).unwrap(),
        Drained::Expired { queued: 1 }
    );
    let began = Instant::now();
    writer.abandon();
    drop(writer);
    assert!(began.elapsed() < Duration::from_secs(1));
    drop(reader);
}

#[test]
fn reservation_limits_and_spent_terminal_permits_are_independent_of_sink_speed() {
    let (sink, _reader) = bounded_pair();
    let writer = Writer::start(sink.into()).unwrap();
    assert_eq!(ORDINARY_CAPACITY, 1017);
    assert_eq!(TERMINAL_RESERVE, 7);
    let mut held = Vec::new();
    for _ in 0..1017 {
        held.push(writer.reserve(None).unwrap().unwrap());
    }
    assert!(writer.reserve(None).unwrap().is_none());
    drop(held);
    writer
        .reserve(Some(Terminal::Fault))
        .unwrap()
        .unwrap()
        .enqueue(Sequence(0), line("first fault"));
    drained(&writer);
    assert_eq!(writer.pressure().queued, 0);
    assert!(
        writer.reserve(Some(Terminal::Fault)).is_err(),
        "a drained terminal record cannot restore its permit"
    );
}

#[test]
fn a_small_queue_wakes_the_pump_when_its_last_write_finishes() {
    let (sink, mut reader) = bounded_pair();
    let mut writer = Writer::start(sink.into()).unwrap();
    let large = line(&"x".repeat(1_000_000));
    let expected = Arc::clone(&large);
    writer
        .reserve(None)
        .unwrap()
        .unwrap()
        .enqueue(Sequence(31), large);
    assert_eq!(
        writer.drain_status().unwrap(),
        DrainStatus::Pending { queued: 1 }
    );
    assert!(!readable(writer.capacity_fd(), 0));
    let reader = std::thread::spawn(move || {
        let mut bytes = vec![0_u8; expected.len()];
        reader.read_exact(&mut bytes).unwrap();
        assert_eq!(bytes, expected.as_bytes());
        reader
    });
    assert!(
        readable(writer.capacity_fd(), 2000),
        "a queue below the capacity threshold must wake on complete drain"
    );
    assert_eq!(writer.drain_status().unwrap(), DrainStatus::Emptied);
    assert_eq!(
        writer.boundary(Some(Sequence(31))).committed,
        Some(Sequence(31))
    );
    assert_eq!(writer.try_capacity_wake().unwrap(), CapacityWake::Consumed);
    assert_eq!(writer.try_capacity_wake().unwrap(), CapacityWake::Empty);
    drop(writer);
    drop(reader.join().unwrap());
}

#[test]
fn cancelling_the_only_reservation_wakes_the_drain_predicate() {
    let (sink, _reader) = bounded_pair();
    let mut writer = Writer::start(sink.into()).unwrap();
    let reservation = writer.reserve(None).unwrap().unwrap();
    assert_eq!(
        writer.drain_status().unwrap(),
        DrainStatus::Pending { queued: 1 }
    );
    drop(reservation);
    assert!(readable(writer.capacity_fd(), 1000));
    assert_eq!(writer.drain_status().unwrap(), DrainStatus::Emptied);
    assert_eq!(writer.try_capacity_wake().unwrap(), CapacityWake::Consumed);
    assert_eq!(writer.boundary(None).committed, None);
}

#[test]
fn bounded_wake_consumes_one_buffer_and_returns_after_an_interruption() {
    let (sink, _reader) = bounded_pair();
    let mut writer = Writer::start(sink.into()).unwrap();
    let mut notifier = &writer.shared.notifier;
    notifier.write_all(&[1_u8; 512]).unwrap();
    let calls = std::cell::Cell::new(0);
    assert_eq!(
        writer
            .try_capacity_wake_with(|_, _| {
                calls.set(calls.get() + 1);
                Err(io::ErrorKind::Interrupted.into())
            })
            .unwrap(),
        CapacityWake::Interrupted
    );
    assert_eq!(calls.get(), 1);
    assert!(readable(writer.capacity_fd(), 0));
    assert_eq!(writer.try_capacity_wake().unwrap(), CapacityWake::Consumed);
    assert!(
        readable(writer.capacity_fd(), 0),
        "one call must leave a second buffer for the next pump step"
    );
    assert_eq!(writer.try_capacity_wake().unwrap(), CapacityWake::Consumed);
    assert!(!readable(writer.capacity_fd(), 0));
    assert_eq!(writer.try_capacity_wake().unwrap(), CapacityWake::Empty);
    assert_eq!(writer.drain_status().unwrap(), DrainStatus::Emptied);
}

#[test]
fn bounded_status_and_wake_keep_the_exact_standing_commit_failure() {
    let (sink, reader) = bounded_pair();
    drop(reader);
    let mut writer = Writer::start(sink.into()).unwrap();
    writer
        .reserve(None)
        .unwrap()
        .unwrap()
        .enqueue(Sequence(45), line("fail"));
    assert!(readable(writer.capacity_fd(), 1000));
    let failure = writer.standing_failure().expect("the sink failed");
    assert!(matches!(
        failure,
        Failure::CommitFailed {
            sequence: Sequence(45),
            ..
        }
    ));
    assert_eq!(writer.drain_status(), Err(failure.clone()));
    assert_eq!(writer.try_capacity_wake(), Err(failure.clone()));
    assert_eq!(writer.drain_status(), Err(failure));
}

#[test]
fn bounded_wake_eof_is_standing_and_never_a_successful_empty_drain() {
    let (sink, _reader) = bounded_pair();
    let mut writer = Writer::start(sink.into()).unwrap();
    writer
        .shared
        .notifier
        .shutdown(std::net::Shutdown::Write)
        .unwrap();
    let failure = writer.try_capacity_wake().unwrap_err();
    assert!(matches!(failure, Failure::WriteTargetUnusable { .. }));
    assert_eq!(writer.drain_status(), Err(failure.clone()));
    assert_eq!(writer.standing_failure(), Some(failure));
}

#[test]
fn an_empty_transition_that_cannot_signal_is_a_standing_failure() {
    let (sink, _reader) = bounded_pair();
    let writer = Writer::start(sink.into()).unwrap();
    let reservation = writer.reserve(None).unwrap().unwrap();
    writer.wake.shutdown(std::net::Shutdown::Read).unwrap();
    drop(reservation);
    assert!(matches!(
        writer.drain_status(),
        Err(Failure::WriteTargetUnusable { .. })
    ));
}
