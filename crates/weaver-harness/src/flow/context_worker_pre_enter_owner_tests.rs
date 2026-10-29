use super::*;
use crate::flow::clock::{WaitOutcome, testing::FakePumpIo};
use nix::sys::socket::{
    AddressFamily, ControlMessage, MsgFlags, SockFlag, SockType, recv, send, sendmsg, socketpair,
};
use std::io::{IoSlice, Read};
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicU64, Ordering};

const ENTER: &[u8] = include_bytes!("../../../weaver-types/tests/fixtures/state-enter.json");
static NEXT: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    owner: FoundationAdmission,
    peer: OwnedFd,
    _path: PathGuard,
}

struct PathGuard(std::path::PathBuf);
impl Drop for PathGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "wa-pre-enter-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let listener = LifecycleListener::new(crate::bind_coordination(&path).unwrap()).unwrap();
        let (end, peer) = socketpair(
            AddressFamily::Unix,
            SockType::SeqPacket,
            None,
            SockFlag::SOCK_CLOEXEC,
        )
        .unwrap();
        // Owner tests isolate an already authenticated socketpair. The process
        // fixtures exercise actual accept credentials and process-wide hygiene.
        let receiver = EnterReceiver::from_peer(LifecyclePeer::from_owned(end).unwrap())
            .ok()
            .unwrap();
        Self {
            owner: FoundationAdmission {
                listener,
                receiver: Some(receiver),
                received: None,
                ordinary: None,
                response: None,
                ready: false,
                stopped: false,
                detail: None,
                last_ns: None,
                wait_ready: false,
                delivery_loss: None,
            },
            peer,
            _path: PathGuard(path),
        }
    }
    fn request(&self, bytes: &[u8], handles: &[i32]) {
        assert_eq!(
            sendmsg::<()>(
                self.peer.as_raw_fd(),
                &[IoSlice::new(bytes)],
                &[ControlMessage::ScmRights(handles)],
                MsgFlags::MSG_NOSIGNAL,
                None
            )
            .unwrap(),
            bytes.len()
        );
    }
}

fn packet(peer: &OwnedFd) -> Option<Vec<u8>> {
    let mut bytes = vec![0; weaver_types::MAX_ENVELOPE_BYTES];
    match recv(peer.as_raw_fd(), &mut bytes, MsgFlags::MSG_DONTWAIT) {
        Ok(n) if n > 0 => {
            bytes.truncate(n);
            Some(bytes)
        }
        Ok(_) | Err(nix::errno::Errno::EAGAIN) => None,
        other => panic!("fixture receive: {other:?}"),
    }
}

fn ordinary(ordinal: u64, directive: LifecycleDirective) -> OrganEnvelope {
    OrganEnvelope {
        exchange: weaver_types::ExchangeId {
            opener: weaver_types::Opener::Admin,
            ordinal,
        },
        position: Position::Open,
        payload: Payload::Directive(directive),
    }
}

fn enter() -> Vec<u8> {
    let mut value: serde_json::Value = serde_json::from_slice(ENTER).unwrap();
    value["payload"]["body"]["payload"]["binding"]["enter_window"] =
        serde_json::json!({"began_ns":"1","deadline_ns":"100"});
    serde_json::to_vec(&value).unwrap()
}

fn fill(end: i32) {
    let bytes = vec![0x55; 4096];
    loop {
        match send(end, &bytes, MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL) {
            Ok(n) => assert_eq!(n, bytes.len()),
            Err(nix::errno::Errno::EAGAIN) => break,
            other => panic!("fixture fill: {other:?}"),
        }
    }
}

#[test]
fn before_enter_answers_only_the_original_ordinary_opener_and_closes_unused_handles() {
    for directive in [
        LifecycleDirective::Observe,
        LifecycleDirective::Lower,
        LifecycleDirective::Stop {
            cause: weaver_types::Cause { uid: 42 },
        },
    ] {
        let mut fx = Fixture::new();
        let request = ordinary(73, directive);
        let bytes = serde_json::to_vec(&request).unwrap();
        let (sink, mut trace) = UnixStream::pair().unwrap();
        let (state, mut member) = UnixStream::pair().unwrap();
        fx.request(&bytes, &[sink.as_raw_fd(), state.as_raw_fd()]);
        drop((sink, state));
        assert_eq!(
            fx.owner.advance_with_io(&mut FakePumpIo::new(10)),
            FoundationAdmissionProgress::Pending
        );
        let answer: OrganEnvelope = serde_json::from_slice(&packet(&fx.peer).unwrap()).unwrap();
        assert_eq!(answer.exchange, request.exchange);
        assert_eq!(answer.position, Position::Close);
        if matches!(
            request.payload,
            Payload::Directive(LifecycleDirective::Observe)
        ) {
            assert!(
                matches!(answer.payload, Payload::Answer(LifecycleAnswer::State {
                plan: None,
                state: weaver_types::AgentState::Unloaded, load: None, constituents
            }) if constituents.is_empty())
            );
        } else {
            assert!(matches!(
                answer.payload,
                Payload::Refusal(LifecycleRefusal::OutOfOrder)
            ));
        }
        trace.set_nonblocking(true).unwrap();
        member.set_nonblocking(true).unwrap();
        assert_eq!(trace.read(&mut [0]).unwrap(), 0);
        assert_eq!(member.read(&mut [0]).unwrap(), 0);
        assert_eq!(fx.owner.binding_kind(), None);
        assert_eq!(fx.owner.process_exit(), None);
    }
}

#[test]
fn pending_answer_waits_for_capacity_without_reading_a_second_packet_or_inventing_a_bound() {
    struct Wait {
        fd: i32,
        waited: bool,
    }
    impl PumpIo for Wait {
        fn now(&mut self) -> std::io::Result<u64> {
            Ok(10)
        }
        fn wait(
            &mut self,
            interests: &[WaitInterest<'_>],
            bound: Option<u64>,
        ) -> std::io::Result<WaitOutcome> {
            assert_eq!(interests.len(), 1);
            assert_eq!(interests[0].fd.as_raw_fd(), self.fd);
            assert!(interests[0].events.contains(PollFlags::POLLOUT));
            assert!(!interests[0].events.contains(PollFlags::POLLIN));
            assert_eq!(bound, None, "ordinary before-Enter has no invented cap");
            self.waited = true;
            Ok(WaitOutcome::Ready(vec![]))
        }
    }
    let mut fx = Fixture::new();
    let fd = fx
        .owner
        .receiver
        .as_ref()
        .unwrap()
        .interest()
        .fd
        .as_raw_fd();
    fill(fd);
    let first = serde_json::to_vec(&ordinary(73, LifecycleDirective::Observe)).unwrap();
    fx.request(&first, &[]);
    fx.request(
        &serde_json::to_vec(&ordinary(74, LifecycleDirective::Lower)).unwrap(),
        &[],
    );
    assert_eq!(
        fx.owner.advance_with_io(&mut FakePumpIo::new(10)),
        FoundationAdmissionProgress::Pending
    );
    assert_eq!(fx.owner.ordinary.as_ref().unwrap()._wire, first);
    assert!(
        fx.owner
            .response
            .as_ref()
            .unwrap()
            .send
            .as_ref()
            .is_some_and(|send| !send.sent())
    );
    let mut io = Wait { fd, waited: false };
    fx.owner.wait_with_io(&mut io);
    assert!(io.waited);
    assert_eq!(fx.owner.ordinary.as_ref().unwrap()._wire, first);
    while packet(&fx.peer).is_some() {}
    fx.owner.advance_with_io(&mut FakePumpIo::new(10));
    let answer: OrganEnvelope = serde_json::from_slice(&packet(&fx.peer).unwrap()).unwrap();
    assert_eq!(answer.exchange.ordinal, 73);
    assert!(
        packet(&fx.peer).is_none(),
        "the second opener was not consumed early"
    );
    assert_eq!(
        fx.owner
            .receiver
            .as_ref()
            .unwrap()
            .interest()
            .fd
            .as_raw_fd(),
        fd
    );
    fx.owner.advance_with_io(&mut FakePumpIo::new(10));
    let second: OrganEnvelope = serde_json::from_slice(&packet(&fx.peer).unwrap()).unwrap();
    assert_eq!(second.exchange.ordinal, 74);
    assert!(matches!(
        second.payload,
        Payload::Refusal(LifecycleRefusal::OutOfOrder)
    ));
    drop(fx.peer);
    fx.owner.advance_with_io(&mut FakePumpIo::new(10));
    assert!(
        fx.owner.receiver.is_none(),
        "actual admin EOF retires the connection"
    );
    assert_eq!(fx.owner.process_exit(), None);
}

#[test]
fn completed_enter_refusals_resume_the_same_peer_before_accepting_another_connection() {
    for future in [false, true] {
        let mut fx = Fixture::new();
        let mut value: serde_json::Value = serde_json::from_slice(&enter()).unwrap();
        if future {
            value["payload"]["body"]["payload"]["binding"]["enter_window"]["began_ns"] =
                "11".into();
        }
        let bytes = serde_json::to_vec(&value).unwrap();
        let original = weaver_types::state_flow::enter::EnterEnvelope::decode(&bytes).unwrap();
        fx.request(&bytes, &[]);
        let progress = fx.owner.advance_with_io(&mut FakePumpIo::new(10));
        if future {
            assert_eq!(progress, FoundationAdmissionProgress::Pending);
        } else {
            assert_eq!(progress, FoundationAdmissionProgress::Received);
            fx.owner = fx
                .owner
                .adopt_with_io(&mut FakePumpIo::new(10))
                .err()
                .unwrap();
            fx.owner.advance_with_io(&mut FakePumpIo::new(10));
        }
        let refused: OrganEnvelope = serde_json::from_slice(&packet(&fx.peer).unwrap()).unwrap();
        assert_eq!(refused.exchange, original.exchange);
        assert_eq!(
            refused.payload,
            Payload::Refusal(if future {
                LifecycleRefusal::Malformed
            } else {
                LifecycleRefusal::DescriptorsUnusable
            })
        );
        assert!(fx.owner.receiver.is_some());
        assert!(
            fx.owner.received.is_none(),
            "the answered source is no longer owed"
        );
        fx.request(
            &serde_json::to_vec(&ordinary(74, LifecycleDirective::Observe)).unwrap(),
            &[],
        );
        fx.owner.advance_with_io(&mut FakePumpIo::new(10));
        let answer: OrganEnvelope = serde_json::from_slice(&packet(&fx.peer).unwrap()).unwrap();
        assert_eq!(answer.exchange.ordinal, 74);
        assert!(
            matches!(answer.payload, Payload::Answer(LifecycleAnswer::State {
            plan: None,
            state: weaver_types::AgentState::Unloaded, load: None, constituents
        }) if constituents.is_empty())
        );
        assert_eq!(fx.owner.process_exit(), None);
    }
}

#[test]
fn expired_staged_enter_refusal_sends_no_late_packet_and_keeps_its_original_window() {
    struct AcrossStage {
        first: bool,
    }
    impl PumpIo for AcrossStage {
        fn now(&mut self) -> std::io::Result<u64> {
            Ok(if std::mem::take(&mut self.first) {
                99
            } else {
                107
            })
        }
        fn wait(&mut self, _: &[WaitInterest<'_>], _: Option<u64>) -> std::io::Result<WaitOutcome> {
            panic!("staging expiry is observed before a wait")
        }
    }
    let mut fx = Fixture::new();
    let bytes = enter();
    fx.request(&bytes, &[]);
    assert_eq!(
        fx.owner.advance_with_io(&mut FakePumpIo::new(10)),
        FoundationAdmissionProgress::Received
    );
    fx.owner = fx
        .owner
        .adopt_with_io(&mut FakePumpIo::new(10))
        .err()
        .unwrap();
    let bound = fx.owner.response.as_ref().unwrap().bound.unwrap();
    assert_eq!((bound.began_ns(), bound.deadline_ns()), (1, 100));
    assert_eq!(
        fx.owner.advance_with_io(&mut AcrossStage { first: true }),
        FoundationAdmissionProgress::Pending
    );
    assert!(
        packet(&fx.peer).is_none(),
        "no first refusal send after serialization spends Enter"
    );
    assert_eq!(fx.owner.received.as_ref().unwrap().wire(), bytes);
    assert_eq!(fx.owner.received.as_ref().unwrap().window(), Some(bound));
    assert_eq!(fx.owner.process_exit(), None);
}

#[test]
fn a_future_anchor_refusal_keeps_the_actual_deadline_without_waiting_for_its_start() {
    let mut fx = Fixture::new();
    let mut value: serde_json::Value = serde_json::from_slice(&enter()).unwrap();
    value["payload"]["body"]["payload"]["binding"]["enter_window"]["began_ns"] = "11".into();
    fill(
        fx.owner
            .receiver
            .as_ref()
            .unwrap()
            .interest()
            .fd
            .as_raw_fd(),
    );
    fx.request(&serde_json::to_vec(&value).unwrap(), &[]);
    assert_eq!(
        fx.owner.advance_with_io(&mut FakePumpIo::new(10)),
        FoundationAdmissionProgress::Pending
    );
    let bound = fx.owner.response.as_ref().unwrap().bound.unwrap();
    assert_eq!((bound.began_ns(), bound.deadline_ns()), (11, 100));
    assert!(
        fx.owner.response.as_ref().unwrap().send.is_some(),
        "future anchor does not delay staging"
    );
    while packet(&fx.peer).is_some() {}
    fx.owner.advance_with_io(&mut FakePumpIo::new(100));
    assert!(packet(&fx.peer).is_none());
    assert_eq!(fx.owner.process_exit(), None);
}

#[test]
fn physically_sent_answer_with_failed_clock_is_terminal_not_delivery_loss() {
    struct Clock {
        peer: i32,
    }
    impl PumpIo for Clock {
        fn now(&mut self) -> std::io::Result<u64> {
            let mut byte = [0];
            if recv(
                self.peer,
                &mut byte,
                MsgFlags::MSG_PEEK | MsgFlags::MSG_DONTWAIT,
            )
            .is_ok()
            {
                Err(std::io::Error::other("actual post-send clock failure"))
            } else {
                Ok(10)
            }
        }
        fn wait(&mut self, _: &[WaitInterest<'_>], _: Option<u64>) -> std::io::Result<WaitOutcome> {
            panic!("the original answer fits")
        }
    }
    let mut fx = Fixture::new();
    fx.request(
        &serde_json::to_vec(&ordinary(73, LifecycleDirective::Observe)).unwrap(),
        &[],
    );
    let mut io = Clock {
        peer: fx.peer.as_raw_fd(),
    };
    assert_eq!(
        fx.owner.advance_with_io(&mut io),
        FoundationAdmissionProgress::Stopped
    );
    assert!(
        packet(&fx.peer).is_some(),
        "the actual packet crossed before the clock failed"
    );
    assert_eq!(
        fx.owner.process_exit(),
        Some(FoundationProcessExit::Failure)
    );
    assert!(fx.owner.take_delivery_loss().is_none());
    assert!(
        fx.owner
            .response
            .as_ref()
            .unwrap()
            .send
            .as_ref()
            .unwrap()
            .sent()
    );
}

#[test]
fn diagnostic_keeps_original_descriptors_and_cannot_be_adopted_by_serving() {
    let mut fx = Fixture::new();
    let mut value: serde_json::Value = serde_json::from_slice(&enter()).unwrap();
    value["payload"]["body"]["payload"]["binding"] = serde_json::json!({
        "kind":"diagnostic", "state_election":weaver_types::StateElection::default()
    });
    let bytes = serde_json::to_vec(&value).unwrap();
    let (sink, mut trace) = UnixStream::pair().unwrap();
    let (state, mut member) = UnixStream::pair().unwrap();
    fx.request(&bytes, &[sink.as_raw_fd(), state.as_raw_fd()]);
    drop((sink, state));
    assert_eq!(
        fx.owner.advance_with_io(&mut FakePumpIo::new(10)),
        FoundationAdmissionProgress::Received
    );
    assert_eq!(fx.owner.binding_kind(), Some(BindingKind::Diagnostic));
    fx.owner = fx
        .owner
        .adopt_with_io(&mut FakePumpIo::new(10))
        .err()
        .unwrap();
    assert_eq!(fx.owner.binding_kind(), Some(BindingKind::Diagnostic));
    assert_eq!(fx.owner.received.as_ref().unwrap().wire(), bytes);
    trace.set_nonblocking(true).unwrap();
    member.set_nonblocking(true).unwrap();
    assert_eq!(
        trace.read(&mut [0]).unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert_eq!(
        member.read(&mut [0]).unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert!(packet(&fx.peer).is_none());
    assert_eq!(fx.owner.process_exit(), None);
}

#[test]
fn an_empty_run_identity_is_malformed_before_any_descriptor_is_consumed() {
    let mut fx = Fixture::new();
    let mut value: serde_json::Value = serde_json::from_slice(&enter()).unwrap();
    value["payload"]["body"]["payload"]["run"] = "".into();
    fx.request(&serde_json::to_vec(&value).unwrap(), &[]);
    assert_eq!(
        fx.owner.advance_with_io(&mut FakePumpIo::new(10)),
        FoundationAdmissionProgress::Received
    );
    fx.owner = fx
        .owner
        .adopt_with_io(&mut FakePumpIo::new(10))
        .err()
        .unwrap();
    fx.owner.advance_with_io(&mut FakePumpIo::new(10));
    let answer: OrganEnvelope = serde_json::from_slice(&packet(&fx.peer).unwrap()).unwrap();
    assert!(matches!(
        answer.payload,
        Payload::Refusal(LifecycleRefusal::Malformed)
    ));
    assert_eq!(fx.owner.process_exit(), None);
}

#[test]
fn post_constructor_expiry_or_clock_failure_closes_actual_handles_without_load_or_open() {
    struct ConstructorClock {
        samples: usize,
        fail: bool,
    }
    impl PumpIo for ConstructorClock {
        fn now(&mut self) -> std::io::Result<u64> {
            self.samples += 1;
            match self.samples {
                // Source precheck, then the constructor's origin capture.
                1 | 2 => Ok(10),
                // The next sample is the post-construction work boundary.
                3 if self.fail => Err(std::io::Error::other("post-construction clock")),
                3 => Ok(107),
                _ => panic!("no renewed budget or second admission check"),
            }
        }
        fn wait(&mut self, _: &[WaitInterest<'_>], _: Option<u64>) -> std::io::Result<WaitOutcome> {
            panic!("constructing a preflight recorder grants no wait")
        }
    }
    for fail in [false, true] {
        let mut fx = Fixture::new();
        let (sink, mut trace) = UnixStream::pair().unwrap();
        let (state, mut member) = UnixStream::pair().unwrap();
        let bytes = enter();
        fx.request(&bytes, &[sink.as_raw_fd(), state.as_raw_fd()]);
        drop((sink, state));
        assert_eq!(
            fx.owner.advance_with_io(&mut FakePumpIo::new(10)),
            FoundationAdmissionProgress::Received
        );
        let mut io = ConstructorClock { samples: 0, fail };
        fx.owner = fx
            .owner
            .adopt_with_io(&mut io)
            .err()
            .expect("construction spent the original window");
        assert_eq!(io.samples, 3);
        assert_eq!(fx.owner.received.as_ref().unwrap().wire(), bytes);
        assert_eq!(
            fx.owner.process_exit(),
            fail.then_some(FoundationProcessExit::Failure)
        );
        assert!(packet(&fx.peer).is_none(), "no substitute lifecycle answer");
        // The abandoned writer can drop its fd asynchronously; a finite real
        // watchdog judges actual EOF rather than crediting fixture Drop.
        trace
            .set_read_timeout(Some(std::time::Duration::from_secs(3)))
            .unwrap();
        member
            .set_read_timeout(Some(std::time::Duration::from_secs(3)))
            .unwrap();
        assert_eq!(trace.read(&mut [0]).unwrap(), 0);
        assert_eq!(member.read(&mut [0]).unwrap(), 0);
    }
}

#[test]
fn undeliverable_answer_notice_is_retained_once_before_another_accept() {
    let mut fx = Fixture::new();
    fill(
        fx.owner
            .receiver
            .as_ref()
            .unwrap()
            .interest()
            .fd
            .as_raw_fd(),
    );
    fx.request(
        &serde_json::to_vec(&ordinary(73, LifecycleDirective::Observe)).unwrap(),
        &[],
    );
    fx.owner.advance_with_io(&mut FakePumpIo::new(10));
    assert!(fx.owner.response.is_some());
    drop(fx.peer);
    fx.owner.advance_with_io(&mut FakePumpIo::new(10));
    assert_eq!(fx.owner.progress(), FoundationAdmissionProgress::Pending);
    assert_eq!(fx.owner.process_exit(), None);
    let notice = fx
        .owner
        .delivery_loss
        .clone()
        .expect("actual send failure is owed to worker");
    fx.owner.advance_with_io(&mut FakePumpIo::new(11));
    assert_eq!(fx.owner.delivery_loss.as_deref(), Some(notice.as_str()));
    assert!(
        !fx.owner.wait_ready,
        "caller must report the notice before another wait"
    );
    assert_eq!(fx.owner.take_delivery_loss(), Some(notice));
    assert!(fx.owner.take_delivery_loss().is_none());
}

#[test]
fn legacy_enter_cannot_fall_back_to_an_ordinary_before_enter_directive() {
    let canonical = weaver_types::state_flow::enter::EnterEnvelope::decode(ENTER).unwrap();
    let source = canonical.enter();
    let weaver_types::state_flow::enter::EnterBinding::Serving {
        gate_instruction,
        shell,
        ..
    } = &source.binding
    else {
        panic!("Serving fixture");
    };
    // The real legacy type supplies its different field spellings, so this
    // reaches the ordinary decoder instead of failing on fixture JSON syntax.
    let legacy = ordinary(
        73,
        LifecycleDirective::Enter {
            payload: Box::new(weaver_types::EnterPayload {
                session: source.session.clone(),
                run: source.run.clone(),
                spu_instruction: source.spu_instruction.clone(),
                binding: weaver_types::EnterBinding::Serving {
                    gate_instruction: gate_instruction.clone(),
                    shell: shell.clone(),
                },
                state_election: weaver_types::StateElection::default(),
                state_store: source.state_store.clone(),
                declaration: source.declaration.clone(),
                restore: source.restore.clone(),
                reset: source.reset.clone(),
                stack: source.stack.clone(),
                boundary: source.boundary.clone(),
                cause: source.cause,
                operator: source.operator,
                library_path: source.library_path.clone(),
                drain_bound: source.drain_bound,
                wind_down_bound: source.wind_down_bound,
            }),
        },
    );
    assert!(matches!(
        &legacy.payload,
        Payload::Directive(LifecycleDirective::Enter { .. })
    ));
    let bytes = serde_json::to_vec(&legacy).unwrap();
    assert!(weaver_types::state_flow::enter::EnterEnvelope::decode(&bytes).is_err());

    let mut fx = Fixture::new();
    let (sink, mut trace) = UnixStream::pair().unwrap();
    let (state, mut member) = UnixStream::pair().unwrap();
    fx.request(&bytes, &[sink.as_raw_fd(), state.as_raw_fd()]);
    drop((sink, state));
    assert_eq!(
        fx.owner.advance_with_io(&mut FakePumpIo::new(10)),
        FoundationAdmissionProgress::Stopped,
        "a retired Enter shape must not become an ordinary OutOfOrder refusal"
    );
    assert_eq!(
        fx.owner.process_exit(),
        Some(FoundationProcessExit::Failure)
    );
    assert!(
        packet(&fx.peer).is_none(),
        "no replacement lifecycle answer"
    );
    assert!(fx.owner.take_delivery_loss().is_none());
    trace.set_nonblocking(true).unwrap();
    member.set_nonblocking(true).unwrap();
    assert_eq!(trace.read(&mut [0]).unwrap(), 0);
    assert_eq!(member.read(&mut [0]).unwrap(), 0);
}
