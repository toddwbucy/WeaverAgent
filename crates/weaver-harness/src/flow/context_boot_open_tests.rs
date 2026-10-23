//! Actual certified templates, Released/Recorded BootHead, decode SEQPACKET and
//! trace writer. These instruments do not activate serving or qualify artifacts.
use super::*;
use crate::channel::{DecodeChannel, OrganChannel};
use crate::flow::clock::testing::FakePumpIo;
use crate::flow::clock::{ReadyFd, WaitInterest, WaitOutcome};
use crate::flow::halt_snapshot::ExternalFacts;
use crate::flow::pump::{LifecyclePeer, LifecyclePorts};
use crate::flow::pump_transport::{DecodePort, Stage};
use nix::errno::Errno;
use nix::sys::socket::{MsgFlags, recv, send};
use std::io;
use std::os::fd::{AsRawFd, BorrowedFd, RawFd};
use std::time::{Duration, Instant};
use weaver_types::{Opened, SessionId, TokenAnswer, TokenDirective, TokenRefusal};

#[path = "context_plan_summary_tests.rs"]
mod plan_summary_tests;

const _: fn() = || {
    trait Private<M> {
        fn pin() {}
    }
    impl<T: ?Sized> Private<()> for T {}
    impl<T: Clone> Private<u8> for T {}
    impl<T: Default> Private<u16> for T {}
    impl<T: From<Opened>> Private<u32> for T {}
    let _ = <CheckedBootOpen as Private<_>>::pin;
    let _ = <boot_open_driver::ValidatedOpen<'static> as Private<_>>::pin;
};

fn send_bytes(fd: BorrowedFd<'_>, bytes: &[u8]) {
    assert_eq!(
        send(
            fd.as_raw_fd(),
            bytes,
            MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_NOSIGNAL
        ),
        Ok(bytes.len())
    );
}
fn receive(fd: BorrowedFd<'_>) -> Option<Vec<u8>> {
    let mut bytes = vec![0; weaver_types::MAX_ENVELOPE_BYTES];
    match recv(
        fd.as_raw_fd(),
        &mut bytes,
        MsgFlags::MSG_DONTWAIT | MsgFlags::MSG_TRUNC,
    ) {
        Ok(size) => {
            assert!(size > 0 && size <= bytes.len());
            bytes.truncate(size);
            Some(bytes)
        }
        Err(Errno::EAGAIN) => None,
        other => panic!("decode fixture receive: {other:?}"),
    }
}
struct ReadyClock<'a> {
    clock: &'a mut FakePumpIo,
    fd: RawFd,
}
impl PumpIo for ReadyClock<'_> {
    fn now(&mut self) -> io::Result<u64> {
        self.clock.now()
    }
    fn wait(
        &mut self,
        interests: &[WaitInterest<'_>],
        deadline: Option<u64>,
    ) -> io::Result<WaitOutcome> {
        let index = interests
            .iter()
            .position(|interest| interest.fd.as_raw_fd() == self.fd)
            .unwrap();
        let at = self.clock.now()? + 1;
        self.clock.schedule(
            at,
            vec![ReadyFd {
                index,
                // Interests include POLLRDHUP, but subscribing to a close is
                // not evidence that the peer closed. Signal only requested I/O.
                events: interests[index].events & (PollFlags::POLLIN | PollFlags::POLLOUT),
            }],
        )?;
        self.clock.wait(interests, deadline)
    }
}
struct OpenFixture {
    boot: BootFixture,
    decode: DecodePort,
    peer: crate::channel::ChildEnd,
}
impl OpenFixture {
    fn new() -> Self {
        let mut boot = BootFixture::new(true);
        let entry = boot.entry();
        let request = boot.begin();
        boot.released(&request, entry);
        assert!(matches!(boot.finish(), BootReadCompletion::Seated));
        let (decode, peer) = DecodeChannel::pair().unwrap();
        Self {
            boot,
            decode: DecodePort::from_channel(decode).unwrap(),
            peer,
        }
    }
    fn begin(&mut self) {
        let run = &mut self.boot.member.run;
        run.context
            .begin_boot_open(&mut self.decode, &mut run.clock)
            .unwrap();
    }
    fn advance(&mut self) -> Result<BootOpenProgress, RunFailure> {
        let run = &mut self.boot.member.run;
        let mut io = ReadyClock {
            clock: &mut run.clock,
            fd: self.decode.fd().as_raw_fd(),
        };
        run.context.advance_boot_open(&mut PumpPorts {
            lifecycle: LifecyclePorts::default(),
            gate: None,
            decode: Some(&mut self.decode),
            label: None,
            io: &mut io,
        })
    }
    fn sent(&mut self) -> TokenDirective {
        for _ in 0..32 {
            if let Some(raw) = receive(self.peer.as_fd()) {
                return serde_json::from_slice(&raw).unwrap();
            }
            self.advance().unwrap();
        }
        panic!("boot Open was not sent");
    }
    fn opened(&self) -> Opened {
        let rendered = "<system>Read caf\u{e9}.</system>";
        Opened { session: SessionId("session".into()), prefix_tokens: 3, capacity: 16384,
            generation_limit: 16, terminal_tokens: 1,
            request: serde_json::value::to_raw_value(&serde_json::json!({"rendered":rendered,"template":"fixture"})).unwrap(),
            measurement: serde_json::value::to_raw_value(&serde_json::json!({"model":"fixture","weights_hash":"fixture",
                "prefix_token_ids":[41,7,89],"blocks":[{"label":"identity-prefix","start":0,"end":rendered.len()}]})).unwrap() }
    }
    fn answer(&self, opened: Opened) -> Vec<u8> {
        let bytes = serde_json::to_vec(&TokenAnswer::Opened(opened)).unwrap();
        send_bytes(self.peer.as_fd(), &bytes);
        bytes
    }
    fn finish(&mut self) -> CheckedBootOpen {
        for _ in 0..32 {
            self.advance().unwrap();
            let run = &mut self.boot.member.run;
            if let Some(checked) = run.context.take_boot_open(&mut run.clock).unwrap() {
                return checked;
            }
        }
        panic!("boot continuation did not finish");
    }
    fn records(&self, kind: Kind) -> Vec<serde_json::Value> {
        self.boot.member.records(kind)
    }
    fn release(&mut self, allowance: u64) -> AbsoluteBound {
        let run = &mut self.boot.member.run;
        let leave = AbsoluteBound::after(run.clock.now().unwrap(), allowance).unwrap();
        run.context
            .begin_boot_open_release(leave, &mut run.clock)
            .unwrap();
        leave
    }
    fn account(&mut self) -> BootOpenProgress {
        let run = &mut self.boot.member.run;
        run.context
            .advance_boot_open_account(&mut self.decode, &mut run.clock)
            .unwrap()
    }
    fn fail(&mut self) -> RunFailure {
        let run = &mut self.boot.member.run;
        let first = run.context.fail(
            &mut run.clock,
            FlowHaltReason::ProtocolFailed,
            HaltPoint::Barrier,
            "fixture unrelated first failure",
        );
        run.context
            .freeze_halt_account(ExternalFacts {
                outstanding_pairs: vec![],
                capture: None,
                wait: run.context.halt_wait().cloned(),
                input_id: None,
            })
            .unwrap();
        run.context.begin_halt_report().unwrap();
        assert!(matches!(
            run.context.advance_halt_trace(&mut run.clock).unwrap(),
            HaltTraceProgress::FaultRecorded(_)
        ));
        first
    }
    fn drain(&mut self) {
        let run = &mut self.boot.member.run;
        run._reader.set_nonblocking(true).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut bytes = [0; 65536];
        while run.context.barrier.recorder().boundary().queued != 0 {
            assert!(Instant::now() < deadline);
            match run._reader.read(&mut bytes) {
                Ok(0) => panic!("trace closed"),
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => std::thread::yield_now(),
                Err(error) => panic!("trace drain: {error}"),
            }
        }
    }
}

#[test]
fn boot_open_real_head_records_exact_evidence_then_installs_once_and_licenses_pairs() {
    let mut fx = OpenFixture::new();
    let head = fx
        .boot
        .member
        .run
        .context
        .boot_head
        .as_ref()
        .unwrap()
        .recall_sequence();
    let span = fx
        .boot
        .member
        .run
        .context
        .boot_head
        .as_ref()
        .unwrap()
        .span()
        .clone();
    fx.begin();
    let sent = fx.sent();
    let TokenDirective::Open {
        session,
        messages,
        column_ask,
    } = sent
    else {
        panic!("Open only")
    };
    assert_eq!(session.0, "session");
    assert!(!column_ask);
    assert_eq!(
        messages,
        vec![
            fx.boot
                .member
                .run
                .context
                .boot_head
                .as_ref()
                .unwrap()
                .message()
                .clone()
        ]
    );
    let opened = fx.opened();
    let request = opened.request.get().to_owned();
    let measurement = opened.measurement.get().to_owned();
    let raw = fx.answer(opened);
    assert!(fx.boot.member.run.context.boot_journal.is_none());
    let checked = fx.finish();
    assert_eq!(
        fx.boot.member.run.context.boot_open_raw(),
        Some(raw.as_slice())
    );
    assert_eq!(checked.head(), head);
    assert_eq!(checked.limits(), (2048, 6144, 16, 1));
    let placements = fx.records(Kind::StatePlacement);
    assert_eq!(placements.len(), 1);
    let record = &placements[0];
    assert!(record["turn"].is_null());
    assert_eq!(record["causal_parent"], head.0.to_string());
    assert_eq!(record["payload"]["assembly_id"], "0");
    assert_eq!(record["payload"]["N"], 3);
    assert_eq!(record["payload"]["input_tokens"], 0);
    let encoded = fx
        .boot
        .member
        .run
        .context
        .barrier
        .recorder()
        .structure()
        .by_kind(Kind::StatePlacement)
        .next()
        .unwrap()
        .line
        .clone();
    assert!(encoded.contains(&request));
    assert!(encoded.contains(&measurement));
    assert_eq!(fx.records(Kind::StateMarkPosition).len(), 1);
    assert_eq!(fx.records(Kind::Recall).len(), 1);
    let snapshot = fx
        .boot
        .member
        .run
        .context
        .boot_journal
        .as_ref()
        .unwrap()
        .boot_snapshot();
    assert_eq!(
        (
            snapshot.0, snapshot.1, snapshot.2, snapshot.3, snapshot.4, snapshot.5, snapshot.6
        ),
        (3, 3, 16384, 1, 1, 1, 0)
    );
    assert_eq!(snapshot.7, vec![(0, span, s::ArgumentCursor::Head {})]);
    let run = &mut fx.boot.member.run;
    assert!(
        run.context
            .take_boot_open(&mut run.clock)
            .unwrap()
            .is_none()
    );
    run.context
        .begin_boot_pairs(PriorBootOpen::from_checked(checked), &mut run.clock)
        .unwrap();
    assert!(receive(fx.peer.as_fd()).is_none());
}

#[test]
fn boot_open_fit_failure_records_prefix_before_exact_boot_failed_without_journal() {
    for (capacity, generation, prefix_too_large) in [
        (8192, 16, false),
        (16384, u64::MAX, false),
        (16384, 16, true),
    ] {
        let mut fx = OpenFixture::new();
        fx.begin();
        fx.sent();
        let mut opened = fx.opened();
        opened.capacity = capacity;
        opened.generation_limit = generation;
        if prefix_too_large {
            opened.prefix_tokens = 2049;
            let mut measurement: serde_json::Value =
                serde_json::from_str(opened.measurement.get()).unwrap();
            measurement["prefix_token_ids"] = serde_json::json!(vec![1; 2049]);
            opened.measurement = serde_json::value::to_raw_value(&measurement).unwrap();
        }
        fx.answer(opened);
        let mut progress = BootOpenProgress::Pending;
        for _ in 0..12 {
            progress = fx.advance().unwrap();
            if progress == BootOpenProgress::Refused {
                break;
            }
        }
        assert_eq!(progress, BootOpenProgress::Refused);
        assert_eq!(fx.records(Kind::StatePlacement).len(), 1);
        let refusals = fx.records(Kind::Refusal);
        assert_eq!(refusals.len(), 1);
        let refusal = &refusals[0]["payload"]["refusal"];
        assert_eq!(refusal["kind"], "state_persistence_invalid");
        assert_eq!(refusal["stage"], "boot");
        assert_eq!(refusal["case"], "boot_failed");
        assert!(fx.boot.member.run.context.boot_journal.is_none());
        assert!(fx.records(Kind::StateMarkPosition).is_empty());
        assert!(fx.boot.member.run.context.failure().is_none());
        fx.release(1000);
        assert!(fx.boot.member.run.context.boot_open_account_settled());
    }
}

#[test]
fn boot_open_typed_refusal_has_its_real_account_and_no_placement() {
    let mut fx = OpenFixture::new();
    fx.begin();
    fx.sent();
    send_bytes(
        fx.peer.as_fd(),
        &serde_json::to_vec(&TokenRefusal::OutOfOrder).unwrap(),
    );
    for _ in 0..10 {
        if fx.advance().unwrap() == BootOpenProgress::Refused {
            break;
        }
    }
    let refusals = fx.records(Kind::Refusal);
    assert_eq!(refusals.len(), 1);
    assert_eq!(refusals[0]["payload"]["asked"]["ask"], "open");
    assert!(fx.records(Kind::StatePlacement).is_empty());
    assert!(fx.boot.member.run.context.boot_journal.is_none());
    assert_eq!(
        fx.boot.member.run.context.boot_open_refusal(),
        Some(&weaver_types::LifecycleRefusal::DeviceCannotAdmit)
    );
}

#[test]
fn boot_open_malformed_echo_and_counts_are_protocol_failure_not_fit_refusal() {
    for case in 0..4 {
        let mut fx = OpenFixture::new();
        fx.begin();
        fx.sent();
        let mut opened = fx.opened();
        match case {
            0 => opened.session.0 = "other".into(),
            1 => opened.terminal_tokens = 2,
            2 => opened.prefix_tokens = 4,
            _ => opened.capacity = 1,
        }
        let raw = fx.answer(opened);
        assert!(fx.advance().is_err());
        assert_eq!(
            fx.boot.member.run.context.boot_open_raw(),
            Some(raw.as_slice())
        );
        assert!(fx.records(Kind::StatePlacement).is_empty());
        assert!(fx.records(Kind::Refusal).is_empty());
        assert_eq!(fx.boot.member.run.context.phase(), RecordingPhase::Failed);
    }
}

#[test]
fn boot_open_pre_send_reply_never_acquires_complete_send_proof() {
    let mut fx = OpenFixture::new();
    fx.begin();
    fx.answer(fx.opened());
    assert!(fx.advance().is_err());
    assert!(fx.records(Kind::StatePlacement).is_empty());
}

#[test]
fn boot_open_foreign_port_cannot_drive_or_retire_after_finish() {
    let mut fx = OpenFixture::new();
    fx.begin();
    let (other, _other_peer) = DecodeChannel::pair().unwrap();
    let mut other = DecodePort::from_channel(other).unwrap();
    let run = &mut fx.boot.member.run;
    assert!(matches!(
        run.context.advance_boot_open(&mut PumpPorts {
            lifecycle: LifecyclePorts::default(),
            gate: None,
            decode: Some(&mut other),
            label: None,
            io: &mut run.clock
        }),
        Err(RunFailure::BootOpenOwned)
    ));
    fx.sent();
    fx.answer(fx.opened());
    let _proof = fx.finish();
    // The source identity remains checked even after the reservation was finished.
    assert!(
        fx.boot
            .member
            .run
            .context
            .check_boot_open_port(&other)
            .is_err()
    );
}

#[test]
fn boot_open_recorder_pressure_retains_reservation_ticket_evidence_and_elapsed_wait() {
    let mut fx = OpenFixture::new();
    fx.begin();
    fx.sent();
    let raw = fx.answer(fx.opened());
    fx.advance().unwrap();
    fx.boot.member.fill_trace();
    assert_eq!(fx.advance().unwrap(), BootOpenProgress::Pending);
    let ticket = fx.boot.member.run.context.pending_author().unwrap().ticket;
    let event = format!(
        "{:?}",
        fx.boot.member.run.context.pending_author().unwrap().event
    );
    assert_eq!(
        fx.boot.member.run.context.boot_open_raw(),
        Some(raw.as_slice())
    );
    assert!(matches!(
        fx.decode
            .stage(TokenDirective::Open {
                session: SessionId("replacement".into()),
                messages: vec![],
                column_ask: false
            })
            .unwrap(),
        Stage::Busy(_)
    ));
    assert!(
        fx.boot
            .member
            .run
            .context
            .complete_wait(PumpDeadline::Enter)
            .is_none()
    );
    assert_eq!(fx.advance().unwrap(), BootOpenProgress::Pending);
    assert_eq!(
        fx.boot.member.run.context.pending_author().unwrap().ticket,
        ticket
    );
    assert_eq!(
        format!(
            "{:?}",
            fx.boot.member.run.context.pending_author().unwrap().event
        ),
        event
    );
    let before = fx.boot.member.now();
    fx.boot.member.run.clock.advance_to(before + 1000).unwrap();
    fx.drain();
    let _proof = fx.finish();
    let placement = &fx.records(Kind::StatePlacement)[0];
    let wait = placement["payload"]["wait_ns"]
        .as_str()
        .unwrap()
        .parse::<u64>()
        .unwrap();
    assert!(wait < 1000);
    assert_eq!(fx.records(Kind::StatePlacement).len(), 1);
}

#[test]
fn boot_open_unsent_cancel_withdraws_and_blocks_generic_pump() {
    let mut fx = OpenFixture::new();
    fx.begin();
    let run = &mut fx.boot.member.run;
    run.context.cancel_boot_open().unwrap();
    assert!(matches!(
        run.context.pump_once(&mut PumpPorts {
            lifecycle: LifecyclePorts::default(),
            gate: None,
            decode: Some(&mut fx.decode),
            label: None,
            io: &mut run.clock
        }),
        Err(RunFailure::BootOpenOwned)
    ));
    assert!(receive(fx.peer.as_fd()).is_none());
    fx.release(1000);
    assert_eq!(fx.account(), BootOpenProgress::Cancelled);
    assert!(fx.boot.member.run.context.boot_open_account_settled());
    assert!(fx.decode.failure().is_none());
    assert!(!fx.decode.send_pending());
    assert!(fx.records(Kind::StatePlacement).is_empty());
}

fn directive(value: weaver_types::LifecycleDirective) -> weaver_types::OrganEnvelope {
    weaver_types::OrganEnvelope {
        exchange: weaver_types::ExchangeId {
            opener: weaver_types::Opener::Admin,
            ordinal: 9,
        },
        position: weaver_types::Position::Open,
        payload: weaver_types::Payload::Directive(value),
    }
}
#[test]
fn boot_open_stop_observed_in_same_pump_prevents_first_byte_and_stays_revoked() {
    for stopped in [false, true] {
        let mut fx = OpenFixture::new();
        fx.begin();
        // Exercise the real alternating reader once. The next pump is a send
        // opportunity, demonstrated by the otherwise identical live control.
        fx.advance().unwrap();
        assert!(fx.decode.open_not_started());
        assert!(receive(fx.peer.as_fd()).is_none());
        let (admin, peer) = OrganChannel::pair().unwrap();
        let mut admin = LifecyclePeer::from_channel(admin).unwrap();
        if stopped {
            let stop = directive(weaver_types::LifecycleDirective::Stop {
                cause: weaver_types::Cause { uid: 0 },
            });
            send_bytes(peer.as_fd(), &serde_json::to_vec(&stop).unwrap());
        }
        let run = &mut fx.boot.member.run;
        let mut io = ReadyClock {
            clock: &mut run.clock,
            fd: fx.decode.fd().as_raw_fd(),
        };
        run.context
            .pump_once(&mut PumpPorts {
                lifecycle: LifecyclePorts {
                    listener: None,
                    peer: Some(&mut admin),
                    residency: None,
                },
                gate: None,
                decode: Some(&mut fx.decode),
                label: None,
                io: &mut io,
            })
            .unwrap();
        if stopped {
            assert!(admin.take_completion().unwrap().is_some());
            assert!(receive(fx.peer.as_fd()).is_none());
            assert_eq!(fx.advance().unwrap(), BootOpenProgress::Cancelled);
            fx.release(1000);
            assert_eq!(fx.account(), BootOpenProgress::Cancelled);
        } else {
            assert!(admin.take_completion().unwrap().is_none());
            let sent =
                receive(fx.peer.as_fd()).expect("live control must send on this exact opportunity");
            assert!(matches!(
                serde_json::from_slice::<TokenDirective>(&sent).unwrap(),
                TokenDirective::Open { .. }
            ));
        }
    }
}

#[test]
fn boot_open_sent_cancel_keeps_generic_pump_out_of_owed_exchange() {
    for cancelled in [false, true] {
        let mut fx = OpenFixture::new();
        fx.begin();
        fx.sent();
        assert!(!fx.decode.open_not_started());
        let raw = fx.answer(fx.opened());
        let run = &mut fx.boot.member.run;
        if cancelled {
            run.context.cancel_boot_open().unwrap();
        }
        let mut io = ReadyClock {
            clock: &mut run.clock,
            fd: fx.decode.fd().as_raw_fd(),
        };
        let result = run.context.pump_once(&mut PumpPorts {
            lifecycle: LifecyclePorts::default(),
            gate: None,
            decode: Some(&mut fx.decode),
            label: None,
            io: &mut io,
        });
        if cancelled {
            assert!(matches!(result, Err(RunFailure::BootOpenOwned)));
            assert!(fx.decode.held().is_none());
            assert!(fx.boot.member.run.context.boot_open_raw().is_none());
            fx.release(1000);
            for _ in 0..16 {
                fx.account();
                if fx.boot.member.run.context.boot_open_account_settled() {
                    break;
                }
            }
            assert!(fx.boot.member.run.context.boot_open_account_settled());
            assert_eq!(
                fx.boot.member.run.context.boot_open_raw(),
                Some(raw.as_slice())
            );
            assert_eq!(fx.records(Kind::StatePlacement).len(), 1);
            assert!(fx.boot.member.run.context.boot_journal.is_none());
        } else {
            result.unwrap();
            assert!(
                fx.decode.held().is_some(),
                "live control must consume the queued answer"
            );
            let _checked = fx.finish();
            assert!(fx.boot.member.run.context.boot_journal.is_some());
        }
    }
}

#[test]
fn boot_open_stop_before_owner_creation_is_retained_for_this_enter() {
    let mut fx = OpenFixture::new();
    let (admin, peer) = OrganChannel::pair().unwrap();
    let mut admin = LifecyclePeer::from_channel(admin).unwrap();
    send_bytes(
        peer.as_fd(),
        &serde_json::to_vec(&directive(weaver_types::LifecycleDirective::Stop {
            cause: weaver_types::Cause { uid: 0 },
        }))
        .unwrap(),
    );
    let run = &mut fx.boot.member.run;
    run.context
        .pump_once(&mut PumpPorts {
            lifecycle: LifecyclePorts {
                listener: None,
                peer: Some(&mut admin),
                residency: None,
            },
            gate: None,
            decode: None,
            label: None,
            io: &mut run.clock,
        })
        .unwrap();
    admin.take_completion().unwrap().unwrap();
    assert_eq!(
        run.context
            .begin_boot_open(&mut fx.decode, &mut run.clock)
            .unwrap_err()
            .refusal,
        BootOpenBeginRefusal::WrongPhase
    );
    assert!(receive(fx.peer.as_fd()).is_none());
}
#[test]
fn boot_open_observe_yields_without_inventing_cancellation() {
    let mut fx = OpenFixture::new();
    fx.begin();
    let (admin, peer) = OrganChannel::pair().unwrap();
    let mut admin = LifecyclePeer::from_channel(admin).unwrap();
    send_bytes(
        peer.as_fd(),
        &serde_json::to_vec(&directive(weaver_types::LifecycleDirective::Observe)).unwrap(),
    );
    let run = &mut fx.boot.member.run;
    run.context
        .pump_once(&mut PumpPorts {
            lifecycle: LifecyclePorts {
                listener: None,
                peer: Some(&mut admin),
                residency: None,
            },
            gate: None,
            decode: Some(&mut fx.decode),
            label: None,
            io: &mut run.clock,
        })
        .unwrap();
    admin.take_completion().unwrap().unwrap();
    assert!(receive(fx.peer.as_fd()).is_none());
    fx.sent();
    fx.answer(fx.opened());
    let _proof = fx.finish();
}

#[test]
fn boot_open_releasing_sent_completion_records_without_any_journal_grant() {
    let mut fx = OpenFixture::new();
    fx.begin();
    fx.sent();
    fx.release(1000);
    let raw = fx.answer(fx.opened());
    for _ in 0..16 {
        fx.account();
        if fx.boot.member.run.context.boot_open_account_settled() {
            break;
        }
    }
    assert!(fx.boot.member.run.context.boot_open_account_settled());
    assert_eq!(
        fx.boot.member.run.context.boot_open_raw(),
        Some(raw.as_slice())
    );
    assert_eq!(fx.records(Kind::StatePlacement).len(), 1);
    assert!(fx.records(Kind::StateMarkPosition).is_empty());
    assert!(fx.boot.member.run.context.boot_journal.is_none());
    assert!(
        fx.boot
            .member
            .run
            .context
            .retire_boot_decode(fx.decode)
            .is_ok()
    );
}

#[test]
fn boot_open_failed_account_cannot_be_stolen_by_generic_halt_trace() {
    let mut fx = OpenFixture::new();
    fx.begin();
    fx.sent();
    fx.answer(fx.opened());
    fx.advance().unwrap();
    fx.boot.member.fill_trace();
    fx.advance().unwrap();
    let ticket = fx.boot.member.run.context.pending_author().unwrap().ticket;
    let first = fx.fail();
    fx.drain();
    let run = &mut fx.boot.member.run;
    for _ in 0..2 {
        assert_eq!(
            run.context.advance_halt_trace(&mut run.clock).unwrap(),
            HaltTraceProgress::Pending
        );
    }
    assert_eq!(run.context.pending_author().unwrap().ticket, ticket);
    assert_eq!(fx.account(), BootOpenProgress::Cancelled);
    assert_eq!(fx.records(Kind::StatePlacement).len(), 1);
    assert_eq!(fx.boot.member.run.context.failure(), Some(&first));
    assert!(fx.boot.member.run.context.boot_journal.is_none());
    assert!(fx.records(Kind::StateMarkPosition).is_empty());
    assert!(
        fx.boot
            .member
            .run
            .context
            .retire_boot_decode(fx.decode)
            .is_ok()
    );
}

#[test]
fn boot_open_expired_enter_still_reaches_incomplete_disposal_on_both_waiting_roads() {
    for queued in [false, true] {
        let mut fx = OpenFixture::new();
        fx.begin();
        fx.sent();
        if queued {
            fx.answer(fx.opened());
            fx.advance().unwrap();
            fx.boot.member.fill_trace();
            fx.advance().unwrap();
        }
        let enter = fx
            .boot
            .member
            .run
            .context
            .deadlines
            .get(PumpDeadline::Enter)
            .unwrap();
        fx.boot
            .member
            .run
            .clock
            .advance_to(enter.deadline_ns())
            .unwrap();
        fx.release(1000);
        assert_eq!(fx.account(), BootOpenProgress::Incomplete);
        assert!(fx.boot.member.run.context.boot_open_incomplete().is_some());
        assert!(fx.boot.member.run.context.boot_open_account_settled());
        assert!(fx.records(Kind::StatePlacement).is_empty());
        assert!(fx.boot.member.run.context.pending_author().is_none());
        fx.drain();
        assert!(
            fx.boot
                .member
                .run
                .context
                .retire_boot_decode(fx.decode)
                .is_ok()
        );
    }
}
#[test]
fn boot_open_earliest_leave_cap_survives_global_wait_retirement() {
    let mut fx = OpenFixture::new();
    fx.begin();
    fx.sent();
    let leave = fx.release(5);
    assert!(
        fx.boot
            .member
            .run
            .context
            .complete_wait(PumpDeadline::Leave)
            .is_none()
    );
    // Adversarial internal instrument: even retirement by a different lifecycle
    // reducer cannot renew the already inherited private deadline.
    fx.boot
        .member
        .run
        .context
        .deadlines
        .disarm(PumpDeadline::Leave);
    let now = fx.boot.member.now();
    fx.boot
        .member
        .run
        .context
        .deadlines
        .arm(
            PumpDeadline::Leave,
            AbsoluteBound::after(now, 1000).unwrap(),
        )
        .unwrap();
    fx.boot
        .member
        .run
        .clock
        .advance_to(leave.deadline_ns())
        .unwrap();
    fx.answer(fx.opened());
    assert_eq!(fx.account(), BootOpenProgress::Incomplete);
    assert!(fx.records(Kind::StatePlacement).is_empty());
}
#[test]
fn boot_open_ordinary_capacity_is_required_after_stop_despite_terminal_reserve() {
    let mut fx = OpenFixture::new();
    fx.begin();
    fx.sent();
    fx.boot.member.fill_trace();
    fx.release(1000);
    fx.answer(fx.opened());
    for _ in 0..4 {
        fx.account();
    }
    assert!(!fx.boot.member.run.context.boot_open_account_settled());
    assert!(fx.records(Kind::StatePlacement).is_empty());
    let pending = fx.boot.member.run.context.pending_author().unwrap().ticket;
    fx.drain();
    assert_eq!(fx.account(), BootOpenProgress::Cancelled);
    assert_eq!(fx.records(Kind::StatePlacement).len(), 1);
    assert!(fx.boot.member.run.context.pending_author().is_none());
    let _ = pending;
}

#[test]
fn boot_open_recorded_sequence_is_saved_before_post_record_clock_failure() {
    struct Broken<'a> {
        clock: &'a mut FakePumpIo,
        calls: usize,
    }
    impl PumpIo for Broken<'_> {
        fn now(&mut self) -> io::Result<u64> {
            self.calls += 1;
            if self.calls == 3 {
                Err(io::Error::other("post-Recorded fixture"))
            } else {
                self.clock.now()
            }
        }
        fn wait(&mut self, _: &[WaitInterest<'_>], _: Option<u64>) -> io::Result<WaitOutcome> {
            panic!("record step must not wait")
        }
    }
    let mut fx = OpenFixture::new();
    fx.begin();
    fx.sent();
    fx.answer(fx.opened());
    fx.advance().unwrap();
    let run = &mut fx.boot.member.run;
    let mut io = Broken {
        clock: &mut run.clock,
        calls: 0,
    };
    assert!(
        run.context
            .advance_boot_open(&mut PumpPorts {
                lifecycle: LifecyclePorts::default(),
                gate: None,
                decode: Some(&mut fx.decode),
                label: None,
                io: &mut io
            })
            .is_err()
    );
    assert_eq!(fx.records(Kind::StatePlacement).len(), 1);
    assert!(fx.boot.member.run.context.pending_author().is_none());
    assert!(fx.boot.member.run.context.boot_journal.is_none());
    // The generic halt road sees no unadmitted placement to repeat.
    let run = &mut fx.boot.member.run;
    run.context
        .freeze_halt_account(ExternalFacts {
            outstanding_pairs: vec![],
            capture: None,
            wait: None,
            input_id: None,
        })
        .unwrap();
    run.context.begin_halt_report().unwrap();
    run.context.advance_halt_trace(&mut run.clock).unwrap();
    fx.account();
    assert_eq!(fx.records(Kind::StatePlacement).len(), 1);
}

#[test]
fn boot_open_partial_answer_after_cancellation_drains_same_exchange() {
    let mut fx = OpenFixture::new();
    fx.begin();
    fx.sent();
    let mut opened = fx.opened();
    let rendered = "x".repeat(100_000);
    opened.request = serde_json::value::to_raw_value(
        &serde_json::json!({"rendered":rendered,"template":"fixture"}),
    )
    .unwrap();
    opened.measurement = serde_json::value::to_raw_value(&serde_json::json!({"model":"fixture","weights_hash":"fixture",
        "prefix_token_ids":[41,7,89],"blocks":[{"label":"identity-prefix","start":0,"end":100_000}]})).unwrap();
    let raw = serde_json::to_vec(&TokenAnswer::Opened(opened)).unwrap();
    let chunks: Vec<_> = raw.chunks(weaver_types::MAX_ENVELOPE_BYTES).collect();
    send_bytes(
        fx.peer.as_fd(),
        &serde_json::to_vec(&weaver_types::SegmentPreamble {
            segments: chunks.len() as u64,
            bytes: raw.len() as u64,
        })
        .unwrap(),
    );
    fx.advance().unwrap();
    send_bytes(fx.peer.as_fd(), chunks[0]);
    fx.advance().unwrap();
    fx.release(1000);
    send_bytes(fx.peer.as_fd(), chunks[1]);
    for _ in 0..16 {
        fx.account();
        if fx.boot.member.run.context.boot_open_account_settled() {
            break;
        }
    }
    assert_eq!(
        fx.boot.member.run.context.boot_open_raw(),
        Some(raw.as_slice())
    );
    assert_eq!(fx.records(Kind::StatePlacement).len(), 1);
    assert!(fx.boot.member.run.context.boot_journal.is_none());
}

#[test]
fn boot_open_pre_owner_release_is_refused_without_arming_leave() {
    let mut fx = OpenFixture::new();
    let run = &mut fx.boot.member.run;
    run.context.cancel_boot_open().unwrap();
    let leave = AbsoluteBound::after(run.clock.now().unwrap(), 100).unwrap();
    assert_eq!(
        run.context.begin_boot_open_release(leave, &mut run.clock),
        Err(RunFailure::BootOpenOwned)
    );
    assert!(run.context.deadlines.get(PumpDeadline::Leave).is_none());
}

#[test]
fn boot_open_enter_expiry_inside_common_pump_revokes_without_component_latch() {
    struct Expires<'a> {
        clock: &'a mut FakePumpIo,
        until: u64,
    }
    impl PumpIo for Expires<'_> {
        fn now(&mut self) -> io::Result<u64> {
            self.clock.now()
        }
        fn wait(
            &mut self,
            _: &[WaitInterest<'_>],
            deadline: Option<u64>,
        ) -> io::Result<WaitOutcome> {
            assert!(deadline.is_some_and(|deadline| deadline <= self.until));
            self.clock.advance_to(self.until)?;
            Ok(WaitOutcome::Timeout)
        }
    }
    let mut fx = OpenFixture::new();
    fx.begin();
    fx.sent();
    let run = &mut fx.boot.member.run;
    let enter = run.context.deadlines.get(PumpDeadline::Enter).unwrap();
    let mut io = Expires {
        clock: &mut run.clock,
        until: enter.deadline_ns(),
    };
    let mut result = Ok(BootOpenProgress::Pending);
    for _ in 0..32 {
        result = run.context.advance_boot_open(&mut PumpPorts {
            lifecycle: LifecyclePorts::default(),
            gate: None,
            decode: Some(&mut fx.decode),
            label: None,
            io: &mut io,
        });
        if result.is_err() {
            break;
        }
    }
    assert_eq!(
        result,
        Err(RunFailure::Wait {
            key: PumpDeadline::Enter,
            bound: enter
        })
    );
    assert!(run.context.failure().is_none());
    assert!(run.context.boot_open_revoked());
    fx.release(1000);
    assert_eq!(fx.account(), BootOpenProgress::Incomplete);
}

fn unload() -> AuthoredEvent {
    AuthoredEvent {
        kind: Kind::Unload,
        subsystem: Subsystem::Harness,
        turn: None,
        source: crate::flow::author::AuthoredSource::Runtime,
        causal_parent: None,
        payload: Some(Payload::Unload(Box::new(serving::payloads::UnloadClose {
            grant_surface: None,
            cause: None,
            forced: true,
            forced_by: None,
            release: None,
            end_reason: None,
            state_flow_failure: None,
        }))),
    }
}
#[test]
fn boot_open_full_stopped_release_accounts_then_retires_unloads_and_drains() {
    for expired in [false, true] {
        let mut fx = OpenFixture::new();
        fx.begin();
        fx.sent();
        if expired {
            let end = fx
                .boot
                .member
                .run
                .context
                .deadlines
                .get(PumpDeadline::Enter)
                .unwrap()
                .deadline_ns();
            fx.boot.member.run.clock.advance_to(end).unwrap();
        }
        fx.release(1000);
        // The reserved terminal path cannot bypass the owed account either.
        let context = &mut fx.boot.member.run.context;
        assert!(
            context
                .barrier
                .submit_terminal_authored_ref(&unload(), &context.author, context.last_ns, 1)
                .is_err()
        );
        assert!(!context.barrier.is_closed());
        if !expired {
            fx.answer(fx.opened());
        }
        for _ in 0..16 {
            fx.account();
            if fx.boot.member.run.context.boot_open_account_settled() {
                break;
            }
        }
        assert!(fx.boot.member.run.context.boot_open_account_settled());
        assert!(
            fx.boot
                .member
                .run
                .context
                .retire_boot_decode(fx.decode)
                .is_ok()
        );
        let run = &mut fx.boot.member.run;
        assert!(run.context.complete_wait(PumpDeadline::Enter).is_some());
        assert_eq!(
            run.context.retire_state(&mut run.clock).unwrap(),
            Advance::Progress
        );
        assert_eq!(
            run.context.retire_classifier(&mut run.clock).unwrap(),
            Advance::Progress
        );
        let recorded = run
            .context
            .author_and_wait(
                &mut PumpPorts {
                    lifecycle: LifecyclePorts::default(),
                    gate: None,
                    decode: None,
                    label: None,
                    io: &mut run.clock,
                },
                unload(),
            )
            .unwrap();
        assert!(recorded.0 > 0);
        assert!(run.context.barrier.is_closed());
        run._reader.set_nonblocking(true).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut bytes = [0; 65536];
        loop {
            if matches!(
                run.context
                    .barrier
                    .drain_status(&mut run.context.budget)
                    .unwrap(),
                serving::DrainStatus::Emptied
            ) {
                break;
            }
            assert!(Instant::now() < deadline);
            match run._reader.read(&mut bytes) {
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => std::thread::yield_now(),
                Err(error) => panic!("release trace drain: {error}"),
            }
        }
        run.context
            .close_after_drain(&mut PumpPorts {
                lifecycle: LifecyclePorts::default(),
                gate: None,
                decode: None,
                label: None,
                io: &mut run.clock,
            })
            .unwrap();
        assert_eq!(run.context.phase(), RecordingPhase::Closed);
    }
}

#[test]
fn boot_open_fault_before_opened_is_wrong_phase_emission_with_original_evidence() {
    // Decode contract permits this emission only after Open, at rest with no
    // exchange outstanding; measurement's later handling cannot license it here.
    for sent in [false, true] {
        let mut fx = OpenFixture::new();
        fx.begin();
        if sent {
            fx.sent();
        }
        let raw = serde_json::to_vec(&TokenAnswer::Fault(weaver_types::FaultReport {
            case: weaver_types::FaultCase::ResidencyDegraded,
            account: serde_json::value::to_raw_value(
                &serde_json::json!({"exact":"fault evidence"}),
            )
            .unwrap(),
        }))
        .unwrap();
        send_bytes(fx.peer.as_fd(), &raw);
        assert!(fx.advance().is_err());
        assert_eq!(
            fx.boot.member.run.context.boot_open_raw(),
            Some(raw.as_slice())
        );
        assert!(fx.records(Kind::StatePlacement).is_empty());
        assert!(fx.boot.member.run.context.boot_journal.is_none());
        assert!(matches!(
            fx.boot.member.run.context.failure(),
            Some(RunFailure::Flow {
                reason: FlowHaltReason::ProtocolFailed,
                ..
            })
        ));
    }
}

#[test]
fn boot_open_transferred_grant_does_not_block_later_lifecycle_decode_progress() {
    let mut fx = OpenFixture::new();
    fx.begin();
    fx.sent();
    fx.answer(fx.opened());
    let _checked = fx.finish();
    let (admin, peer) = OrganChannel::pair().unwrap();
    let mut admin = LifecyclePeer::from_channel(admin).unwrap();
    send_bytes(
        peer.as_fd(),
        &serde_json::to_vec(&directive(weaver_types::LifecycleDirective::Stop {
            cause: weaver_types::Cause { uid: 0 },
        }))
        .unwrap(),
    );
    let run = &mut fx.boot.member.run;
    run.context
        .pump_once(&mut PumpPorts {
            lifecycle: LifecyclePorts {
                listener: None,
                peer: Some(&mut admin),
                residency: None,
            },
            gate: None,
            decode: Some(&mut fx.decode),
            label: None,
            io: &mut run.clock,
        })
        .unwrap();
    assert!(admin.take_completion().unwrap().is_some());
    assert!(!run.context.boot_open_revoked());
    let cancel = TokenDirective::Cancel {
        turn: weaver_types::TurnKey("later-serving-turn".into()),
    };
    assert!(matches!(
        fx.decode.stage(cancel.clone()).unwrap(),
        Stage::Queued
    ));
    let mut received = None;
    for _ in 0..16 {
        let mut io = ReadyClock {
            clock: &mut run.clock,
            fd: fx.decode.fd().as_raw_fd(),
        };
        run.context
            .pump_once(&mut PumpPorts {
                lifecycle: LifecyclePorts::default(),
                gate: None,
                decode: Some(&mut fx.decode),
                label: None,
                io: &mut io,
            })
            .unwrap();
        received = receive(fx.peer.as_fd());
        if received.is_some() {
            break;
        }
    }
    assert_eq!(
        serde_json::from_slice::<TokenDirective>(&received.unwrap()).unwrap(),
        cancel
    );
}

#[test]
fn boot_open_raw_box_line_breaks_follow_trace_failure_without_normalizing_or_granting() {
    // Raw whitespace is valid JSON and is not a new protocol refusal. Preserving
    // it exactly meets the existing trace encoder's single-line limitation.
    for request_box in [false, true] {
        for separator in ['\r', '\n'] {
            let mut fx = OpenFixture::new();
            fx.begin();
            fx.sent();
            let mut opened = fx.opened();
            let raw = if request_box {
                &mut opened.request
            } else {
                &mut opened.measurement
            };
            *raw = serde_json::value::RawValue::from_string(raw.get().replacen(
                '{',
                &format!("{{{separator}"),
                1,
            ))
            .unwrap();
            assert!(crate::open_evidence::validate(&opened, &SessionId("session".into())).is_ok());
            let original = fx.answer(opened);
            fx.advance().unwrap();
            assert!(fx.advance().is_err());
            assert!(matches!(
                fx.boot.member.run.context.failure(),
                Some(RunFailure::Flow {
                    reason: FlowHaltReason::TraceFailed,
                    ..
                })
            ));
            assert_eq!(
                fx.boot.member.run.context.boot_open_raw(),
                Some(original.as_slice())
            );
            assert!(fx.records(Kind::StatePlacement).is_empty());
            assert!(fx.records(Kind::StateMarkPosition).is_empty());
            assert!(fx.boot.member.run.context.boot_journal.is_none());
            let run = &mut fx.boot.member.run;
            assert!(run.context.take_boot_open(&mut run.clock).is_err());
        }
    }
}

#[test]
fn boot_open_escaped_render_line_breaks_preserve_evidence_and_allow_recording() {
    let mut fx = OpenFixture::new();
    fx.begin();
    fx.sent();
    let mut opened = fx.opened();
    let rendered = "<system>Read.\r\nContinue.</system>";
    opened.request = serde_json::value::to_raw_value(
        &serde_json::json!({"rendered":rendered,"template":"fixture"}),
    )
    .unwrap();
    opened.measurement = serde_json::value::to_raw_value(&serde_json::json!({"model":"fixture","weights_hash":"fixture",
        "prefix_token_ids":[41,7,89],"blocks":[{"label":"identity-prefix","start":0,"end":rendered.len()}]})).unwrap();
    let original = fx.answer(opened);
    let _checked = fx.finish();
    assert_eq!(
        fx.boot.member.run.context.boot_open_raw(),
        Some(original.as_slice())
    );
    assert_eq!(fx.records(Kind::StatePlacement).len(), 1);
    assert!(fx.boot.member.run.context.boot_journal.is_some());
}

#[test]
fn boot_open_pre_send_receipt_cannot_be_repaired_by_a_later_complete_send() {
    let mut fx = OpenFixture::new();
    fx.begin();
    let raw = fx.answer(fx.opened());
    // Use the actual transport in adversarial order: receive stamps the receipt
    // before send, then the whole request leaves. Scalar sent=true is not enough.
    fx.decode.try_receive().unwrap();
    assert!(fx.decode.held().is_some());
    assert!(fx.decode.open_not_started());
    fx.decode.try_send().unwrap();
    assert!(receive(fx.peer.as_fd()).is_some());
    assert!(!fx.decode.send_pending());
    assert!(fx.advance().is_err());
    assert_eq!(
        fx.boot.member.run.context.boot_open_raw(),
        Some(raw.as_slice())
    );
    assert!(fx.records(Kind::StatePlacement).is_empty());
    assert!(fx.boot.member.run.context.boot_journal.is_none());
}

#[test]
fn boot_open_cancelled_unsent_protocol_debris_never_sends_open() {
    for failed in [false, true] {
        let mut fx = OpenFixture::new();
        fx.begin();
        // Leave the real alternating pump poised to send on its next unit.
        fx.advance().unwrap();
        assert!(fx.decode.open_not_started());
        fx.answer(fx.opened());
        fx.decode.try_receive().unwrap();
        assert!(fx.decode.held().is_some());
        assert!(fx.decode.open_not_started());
        if failed {
            fx.fail();
        } else {
            fx.release(1000);
        }
        assert_eq!(fx.account(), BootOpenProgress::Incomplete);
        assert!(receive(fx.peer.as_fd()).is_none());
        assert!(fx.boot.member.run.context.boot_open_account_settled());
        assert!(fx.records(Kind::StatePlacement).is_empty());
        assert!(fx.records(Kind::StateMarkPosition).is_empty());
        assert!(
            fx.boot
                .member
                .run
                .context
                .retire_boot_decode(fx.decode)
                .is_ok()
        );
    }
}

#[test]
fn boot_open_transferred_grant_refuses_cancellation_and_stays_complete_after_failure() {
    let mut fx = OpenFixture::new();
    fx.begin();
    fx.sent();
    fx.answer(fx.opened());
    let proof = fx.finish();
    let run = &mut fx.boot.member.run;
    assert_eq!(
        run.context.cancel_boot_open(),
        Err(RunFailure::BootOpenOwned)
    );
    assert!(!run.context.boot_open_revoked());
    assert!(run.context.boot_journal.is_some());
    assert!(
        run.context
            .take_boot_open(&mut run.clock)
            .unwrap()
            .is_none()
    );
    let first = fx.fail();
    assert_eq!(fx.account(), BootOpenProgress::Complete);
    assert_eq!(fx.boot.member.run.context.failure(), Some(&first));
    assert!(!fx.boot.member.run.context.boot_open_revoked());
    assert_eq!(fx.records(Kind::StatePlacement).len(), 1);
    assert_eq!(fx.records(Kind::StateMarkPosition).len(), 1);
    drop(proof);
}

#[test]
fn boot_open_live_rejects_enter_bound_drift_even_before_either_bound_expires() {
    let mut fx = OpenFixture::new();
    fx.begin();
    let run = &mut fx.boot.member.run;
    let original = run.context.deadlines.disarm(PumpDeadline::Enter).unwrap();
    let replacement = AbsoluteBound::after(run.clock.now().unwrap(), 1000).unwrap();
    assert_ne!(replacement, original);
    assert!(!original.expired(run.clock.now().unwrap()));
    run.context
        .deadlines
        .arm(PumpDeadline::Enter, replacement)
        .unwrap();
    assert_eq!(
        run.context.boot_open_live(&mut run.clock),
        Err(RunFailure::BootOpenOwned)
    );
    assert!(run.context.failure().is_none());
    assert!(receive(fx.peer.as_fd()).is_none());
    assert!(run.context.boot_journal.is_none());
}

#[test]
fn boot_open_live_revocation_survives_retiring_a_recoverable_leave_wait() {
    let mut fx = OpenFixture::new();
    fx.begin();
    let run = &mut fx.boot.member.run;
    let leave = AbsoluteBound::after(run.clock.now().unwrap(), 1).unwrap();
    run.context
        .deadlines
        .arm(PumpDeadline::Leave, leave)
        .unwrap();
    run.clock.advance_to(leave.deadline_ns()).unwrap();
    assert_eq!(
        run.context.boot_open_live(&mut run.clock),
        Err(RunFailure::Wait {
            key: PumpDeadline::Leave,
            bound: leave,
        })
    );
    assert!(run.context.failure().is_none());
    assert!(!run.context.boot_enter_cancelled);
    assert_eq!(
        run.context.deadlines.disarm(PumpDeadline::Leave),
        Some(leave)
    );
    assert_eq!(
        run.context.boot_open_live(&mut run.clock),
        Err(RunFailure::BootOpenOwned)
    );
    assert!(receive(fx.peer.as_fd()).is_none());
    assert!(run.context.boot_journal.is_none());
}

#[test]
fn boot_open_unrelated_held_lifecycle_directive_does_not_suppress_first_byte() {
    let mut fx = OpenFixture::new();
    fx.begin();
    fx.advance().unwrap();
    assert!(fx.decode.open_not_started());
    let (admin, peer) = OrganChannel::pair().unwrap();
    let mut admin = LifecyclePeer::from_channel(admin).unwrap();
    let observe = directive(weaver_types::LifecycleDirective::Observe);
    send_bytes(peer.as_fd(), &serde_json::to_vec(&observe).unwrap());
    let run = &mut fx.boot.member.run;
    let mut io = ReadyClock {
        clock: &mut run.clock,
        fd: fx.decode.fd().as_raw_fd(),
    };
    run.context
        .pump_once(&mut PumpPorts {
            lifecycle: LifecyclePorts {
                listener: None,
                peer: Some(&mut admin),
                residency: None,
            },
            gate: None,
            decode: Some(&mut fx.decode),
            label: None,
            io: &mut io,
        })
        .unwrap();
    assert!(admin.held().is_some());
    let sent = receive(fx.peer.as_fd()).expect("held Observe cannot suppress the ready Open send");
    assert!(matches!(
        serde_json::from_slice::<TokenDirective>(&sent).unwrap(),
        TokenDirective::Open { .. }
    ));
    assert!(!run.context.boot_open_revoked());
    assert_eq!(admin.take_completion().unwrap().unwrap().envelope, observe);
}

#[test]
fn boot_open_cannot_consume_a_replacement_author_ticket_under_pressure() {
    let mut fx = OpenFixture::new();
    fx.begin();
    fx.sent();
    fx.answer(fx.opened());
    fx.advance().unwrap();
    fx.boot.member.fill_trace();
    assert_eq!(fx.advance().unwrap(), BootOpenProgress::Pending);
    let run = &mut fx.boot.member.run;
    let held = run.context.barrier.pending_author().unwrap();
    let original_ticket = held.ticket;
    let other = held.event.clone();
    // An adversarial internal caller replaces the slot with a genuinely minted
    // new ticket. No scalar equality or same-kind event can substitute for it.
    assert!(run.context.barrier.abandon_boot_author(original_ticket));
    let progress = run.context.barrier.submit_authored_with_clock(
        other,
        RecordPurpose::Observation,
        &run.context.author,
        1,
        &mut run.context.inbox,
        &mut run.context.budget,
        &mut || run.clock.now(),
    );
    assert!(
        matches!(progress, RecordProgress::Pending),
        "replacement must be a valid pending author: {progress:?}; failure={:?}",
        run.context.barrier.failure()
    );
    let replacement = run.context.barrier.pending_author().unwrap().ticket;
    assert_ne!(replacement, original_ticket);
    assert!(fx.advance().is_err());
    assert_eq!(
        fx.boot.member.run.context.pending_author().unwrap().ticket,
        replacement
    );
    assert!(fx.records(Kind::StatePlacement).is_empty());
    assert!(fx.boot.member.run.context.boot_journal.is_none());
    fx.drain();
}

#[test]
fn boot_open_unload_guards_return_both_originals_without_latching_failure() {
    for authored in [false, true] {
        let mut fx = OpenFixture::new();
        fx.begin();
        let run = &mut fx.boot.member.run;
        let original = unload();
        let progress = if authored {
            run.context.barrier.submit_authored_with_clock(
                original,
                RecordPurpose::Observation,
                &run.context.author,
                1,
                &mut run.context.inbox,
                &mut run.context.budget,
                &mut || run.clock.now(),
            )
        } else {
            let prepared = run
                .context
                .author
                .prepare(&original, run.clock.now().unwrap(), 1)
                .unwrap();
            run.context.barrier.submit_with_clock(
                prepared,
                RecordPurpose::Observation,
                &mut run.context.inbox,
                &mut run.context.budget,
                &mut || run.clock.now(),
            )
        };
        match progress {
            RecordProgress::BusyAuthored { event, purpose } if authored => {
                assert_eq!(event.kind, Kind::Unload);
                assert!(matches!(purpose, RecordPurpose::Observation));
            }
            RecordProgress::Busy { event, purpose } if !authored => {
                assert_eq!(event.envelope.kind, Kind::Unload);
                assert!(matches!(purpose, RecordPurpose::Observation));
            }
            other => panic!("owed boot must retain Unload outside the barrier: {other:?}"),
        }
        assert!(run.context.barrier.failure().is_none());
        assert!(!run.context.budget.is_frozen());
        assert!(run.context.barrier.pending().is_none());
        assert!(run.context.barrier.pending_author().is_none());
        assert!(!run.context.barrier.is_closed());
        assert!(run.context.failure().is_none());
        fx.release(1000);
        assert_eq!(fx.account(), BootOpenProgress::Cancelled);
    }
}

#[test]
fn boot_open_clock_failure_before_preparation_retains_exact_ticket_until_bounded_disposal() {
    struct FailsOnce<'a> {
        clock: &'a mut FakePumpIo,
        at: usize,
        calls: usize,
    }
    impl PumpIo for FailsOnce<'_> {
        fn now(&mut self) -> io::Result<u64> {
            self.calls += 1;
            if self.calls == self.at {
                Err(io::Error::other("boot preparation clock fixture"))
            } else {
                self.clock.now()
            }
        }
        fn wait(&mut self, _: &[WaitInterest<'_>], _: Option<u64>) -> io::Result<WaitOutcome> {
            panic!("placement author step does not wait")
        }
    }
    // Select by the actual custody state, not a baseline number of clock samples.
    let mut witnessed = false;
    for at in 1..=8 {
        let mut fx = OpenFixture::new();
        fx.begin();
        fx.sent();
        fx.answer(fx.opened());
        fx.advance().unwrap();
        let run = &mut fx.boot.member.run;
        let result = run.context.advance_boot_open(&mut PumpPorts {
            lifecycle: LifecyclePorts::default(),
            gate: None,
            decode: Some(&mut fx.decode),
            label: None,
            io: &mut FailsOnce {
                clock: &mut run.clock,
                at,
                calls: 0,
            },
        });
        if result.is_ok()
            || run.context.barrier.pending_author().is_none()
            || run.context.barrier.pending().is_some()
        {
            continue;
        }
        witnessed = true;
        let ticket = run.context.barrier.pending_author().unwrap().ticket;
        assert!(run.context.boot_owns_pending_author());
        assert!(fx.records(Kind::StatePlacement).is_empty());
        let first = fx.fail();
        fx.account();
        let run = &mut fx.boot.member.run;
        assert_eq!(run.context.pending_author().unwrap().ticket, ticket);
        assert_eq!(
            run.context.advance_halt_trace(&mut run.clock).unwrap(),
            HaltTraceProgress::Pending
        );
        let enter = run.context.deadlines.get(PumpDeadline::Enter).unwrap();
        run.clock.advance_to(enter.deadline_ns()).unwrap();
        assert_eq!(fx.account(), BootOpenProgress::Incomplete);
        assert!(fx.boot.member.run.context.pending_author().is_none());
        assert_eq!(fx.boot.member.run.context.failure(), Some(&first));
        assert!(
            fx.boot
                .member
                .run
                .context
                .retire_boot_decode(fx.decode)
                .is_ok()
        );
        break;
    }
    assert!(
        witnessed,
        "must exercise the actual unprepared author-ticket failure window"
    );
}

#[test]
fn boot_open_cancellation_demotes_both_unsubmitted_and_queued_mark_before_accounting() {
    for queued in [false, true] {
        let mut fx = OpenFixture::new();
        fx.begin();
        fx.sent();
        fx.answer(fx.opened());
        fx.advance().unwrap();
        assert_eq!(fx.advance().unwrap(), BootOpenProgress::PlacementRecorded);
        fx.advance().unwrap(); // Placement continuation creates the mark author.
        assert_eq!(
            fx.boot
                .member
                .run
                .context
                .boot_open
                .as_ref()
                .unwrap()
                .unsubmitted_event_for_test()
                .unwrap()
                .kind,
            Kind::StateMarkPosition
        );
        if queued {
            fx.boot.member.fill_trace();
            assert_eq!(fx.advance().unwrap(), BootOpenProgress::Pending);
            assert_eq!(
                fx.boot
                    .member
                    .run
                    .context
                    .pending_author()
                    .unwrap()
                    .event
                    .kind,
                Kind::StateMarkPosition
            );
        }
        fx.release(1000);
        assert_eq!(fx.account(), BootOpenProgress::Cancelled);
        assert!(fx.boot.member.run.context.boot_open_account_settled());
        assert!(fx.boot.member.run.context.pending_author().is_none());
        assert!(fx.boot.member.run.context.boot_journal.is_none());
        assert_eq!(fx.records(Kind::StatePlacement).len(), 1);
        assert!(fx.records(Kind::StateMarkPosition).is_empty());
        if queued {
            fx.drain();
        }
    }
}

#[test]
fn boot_open_placement_transfers_the_same_payload_allocation_into_its_barrier_ticket() {
    fn placement(event: &AuthoredEvent) -> *const serving::payloads::PlacementRecord {
        let Some(Payload::Placement(value)) = &event.payload else {
            panic!("placement payload")
        };
        &**value
    }
    for stopped in [false, true] {
        let mut fx = OpenFixture::new();
        fx.begin();
        fx.sent();
        fx.answer(fx.opened());
        fx.advance().unwrap();
        let allocation = placement(
            fx.boot
                .member
                .run
                .context
                .boot_open
                .as_ref()
                .unwrap()
                .unsubmitted_event_for_test()
                .unwrap(),
        );
        fx.boot.member.fill_trace();
        if stopped {
            fx.release(1000);
            fx.account();
        } else {
            assert_eq!(fx.advance().unwrap(), BootOpenProgress::Pending);
        }
        let context = &fx.boot.member.run.context;
        assert!(
            context
                .boot_open
                .as_ref()
                .unwrap()
                .unsubmitted_event_for_test()
                .is_none()
        );
        assert_eq!(
            placement(&context.pending_author().unwrap().event),
            allocation
        );
        assert!(context.boot_owns_pending_author());
        fx.drain();
        if stopped {
            assert_eq!(fx.account(), BootOpenProgress::Cancelled);
        } else {
            let _proof = fx.finish();
        }
        assert_eq!(fx.records(Kind::StatePlacement).len(), 1);
    }
}
