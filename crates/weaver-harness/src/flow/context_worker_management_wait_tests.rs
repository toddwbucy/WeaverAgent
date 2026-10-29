use super::*;
use crate::flow::clock::testing::{NoReadyIo, NonblockingIo};
use crate::flow::clock::{WaitInterest, WaitOutcome};
use crate::flow::context::worker_startup::tests::Member;
use nix::poll::PollFlags;
use nix::sys::socket::{MsgFlags, send};
use std::os::fd::AsRawFd;
use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use weaver_types::ExchangeId;

struct WitnessIo {
    peer: i32,
    leg: PollFlags,
    release: Option<mpsc::Sender<()>>,
    waits: usize,
    wall: Duration,
    cpu_ns: u64,
}

fn thread_cpu_ns() -> u64 {
    let mut time = nix::libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: clock_gettime initializes the valid stack timespec on success.
    assert_eq!(
        unsafe { nix::libc::clock_gettime(nix::libc::CLOCK_THREAD_CPUTIME_ID, &mut time) },
        0
    );
    u64::try_from(time.tv_sec).unwrap() * 1_000_000_000 + u64::try_from(time.tv_nsec).unwrap()
}

impl PumpIo for WitnessIo {
    fn now(&mut self) -> std::io::Result<u64> {
        MonotonicIo.now()
    }

    fn wait(
        &mut self,
        interests: &[WaitInterest<'_>],
        deadline: Option<u64>,
    ) -> std::io::Result<WaitOutcome> {
        if let Some(release) = self.release.take() {
            assert!(interests.iter().any(|interest| {
                interest.fd.as_raw_fd() == self.peer && interest.events.contains(self.leg)
            }));
            release.send(()).unwrap();
        }
        self.waits += 1;
        let wall = Instant::now();
        let cpu = thread_cpu_ns();
        let result = MonotonicIo.wait(interests, deadline);
        self.cpu_ns += thread_cpu_ns() - cpu;
        self.wall += wall.elapsed();
        result
    }
}

pub(in crate::flow::context::worker_startup) fn exercise(
    serving: FoundationServing<'_>,
    member: &mut Member,
    socket: &Path,
    mode: &str,
) {
    let pressure = mode.ends_with("pressure");
    let mut guard = tests::Guard {
        input: Some(serving.input()),
        management: None,
    };
    let peer = tests::dial(socket);
    let opener = OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Admin,
            ordinal: 811,
        },
        position: Position::Open,
        payload: Payload::Directive(LifecycleDirective::Observe),
    };
    let bytes = serde_json::to_vec(&opener).unwrap();
    if pressure {
        assert_eq!(
            send(peer.as_raw_fd(), &bytes, MsgFlags::MSG_NOSIGNAL).unwrap(),
            bytes.len()
        );
    }
    let watch = Instant::now() + Duration::from_secs(3);
    let mut probe = NonblockingIo { waits: 0 };
    while guard.input.as_ref().unwrap().progress() != FoundationInputProgress::ManagementPending {
        assert_ne!(
            guard.input.as_mut().unwrap().advance_with_io(&mut probe),
            FoundationInputProgress::Stopped
        );
        member.step_live("ready");
        assert!(
            Instant::now() < watch,
            "actual root connection was not accepted"
        );
    }
    guard.management = Some(guard.input.take().unwrap().management().ok().unwrap());
    let owner = guard.management.as_mut().unwrap();
    let owner_address = std::ptr::from_ref(&**owner) as usize;
    let input_address = std::ptr::from_ref(&*owner.input) as usize;
    let peer_fd = owner.peer.fd().as_raw_fd();
    let stage = owner.input.stage;
    let deadlines = format!("{:?}", owner.input.serving.owner.context.deadlines);

    if pressure {
        owner.peer.try_receive().unwrap();
        assert_eq!(owner.peer.held().unwrap().envelope, opener);
        let mut no_ready = NoReadyIo {
            now: MonotonicIo.now().unwrap(),
            waits: 0,
        };
        assert_eq!(
            owner.wait_with_io(&mut no_ready),
            FoundationManagementProgress::Pending
        );
        assert_eq!(no_ready.waits, 0, "a held opener already has runnable work");
        let mut filled = 0;
        loop {
            match send(
                peer_fd,
                &[b'x'; 4096],
                MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL,
            ) {
                Ok(4096) => filled += 1,
                Err(nix::errno::Errno::EAGAIN) => break,
                other => panic!("fill actual admin answer transport: {other:?}"),
            }
        }
        assert!(filled > 0);
        while owner.send.is_none() {
            assert_eq!(
                owner.advance_with_io(&mut probe),
                FoundationManagementProgress::Pending
            );
            member.step_live("ready");
            assert!(Instant::now() < watch);
        }
        assert!(!owner.send.as_ref().unwrap().sent());
        assert_eq!(owner.message.as_ref().unwrap().envelope, opener);
    } else {
        assert_eq!(
            owner.advance_with_io(&mut probe),
            FoundationManagementProgress::Pending
        );
        assert!(owner.peer.held().is_none());
        assert!(owner.message.is_none());
    }

    let (release, released) = mpsc::channel();
    let controller = std::thread::spawn(move || {
        // A missing wait still lets the test terminate and fail its witness.
        let entered_wait = released.recv_timeout(Duration::from_millis(500)).is_ok();
        std::thread::sleep(Duration::from_millis(100));
        if !pressure {
            assert_eq!(
                send(peer.as_raw_fd(), &bytes, MsgFlags::MSG_NOSIGNAL).unwrap(),
                bytes.len()
            );
        }
        let watch = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(bytes) = tests::read_packet(&peer) {
                if pressure && bytes.iter().all(|byte| *byte == b'x') {
                    continue;
                }
                return (entered_wait, bytes);
            }
            assert!(
                Instant::now() < watch,
                "the actual management answer was not delivered"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
    });
    let mut io = WitnessIo {
        peer: peer_fd,
        leg: if pressure {
            PollFlags::POLLOUT
        } else {
            PollFlags::POLLIN
        },
        release: Some(release),
        waits: 0,
        wall: Duration::ZERO,
        cpu_ns: 0,
    };
    while owner.progress() == FoundationManagementProgress::Pending {
        if owner.advance_with_io(&mut io) == FoundationManagementProgress::Pending {
            owner.wait_with_io(&mut io);
        }
        assert!(
            Instant::now() < watch,
            "management did not resume after its real wake"
        );
    }
    assert_eq!(owner.progress(), FoundationManagementProgress::Answered);
    let (entered_wait, answer) = controller.join().unwrap();
    assert!(
        entered_wait && io.waits > 0,
        "pending management must enter its actual wait"
    );
    assert!(
        io.wall >= Duration::from_millis(50),
        "the peer withheld readiness during poll"
    );
    assert!(
        u128::from(io.cpu_ns) * 2 < io.wall.as_nanos(),
        "waiting must block, not hot-poll"
    );
    assert_eq!(std::ptr::from_ref(&**owner) as usize, owner_address);
    assert_eq!(std::ptr::from_ref(&*owner.input) as usize, input_address);
    assert_eq!(owner.peer.fd().as_raw_fd(), peer_fd);
    assert!(owner.input.stage == stage);
    assert_eq!(
        format!("{:?}", owner.input.serving.owner.context.deadlines),
        deadlines
    );
    assert_eq!(owner.message.as_ref().unwrap().envelope, opener);
    let send = owner.send.as_ref().unwrap();
    assert!(send.sent() && send.sent_at().is_some() && owner.peer.owns_send(send));
    assert!(send.matches_exchange(&opener.exchange));
    let answer: ObservationEnvelope = serde_json::from_slice(&answer).unwrap();
    assert_eq!(answer.exchange, opener.exchange);
    assert_eq!(answer.position, Position::Close);
    assert!(matches!(
        answer.payload,
        ObservationContent::Answer(ObservationAnswer::State { .. })
    ));
}
