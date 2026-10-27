//! Real EOF identities and a delegated tools handle at recoverable clock exits.
use super::*;
use crate::flow::clock::{WaitInterest, WaitOutcome};
use crate::flow::context::input_tests::InputFixture;
use crate::tools_cgroup::tests::ScratchLayout;
use nix::sys::socket::{AddressFamily, SockFlag, SockType, socketpair};

struct Clock {
    now: u64,
    calls: usize,
    broken_first: bool,
    reversed_first: bool,
}
impl Clock {
    fn at(now: u64) -> Self {
        Self {
            now,
            calls: 0,
            broken_first: false,
            reversed_first: false,
        }
    }
}
impl PumpIo for Clock {
    fn now(&mut self) -> std::io::Result<u64> {
        self.calls += 1;
        if self.calls == 1 && self.broken_first {
            return Err(std::io::Error::other(
                "original organ-observation clock failed",
            ));
        }
        Ok(if self.calls == 1 && self.reversed_first {
            self.now - 1
        } else {
            self.now
        })
    }
    fn wait(&mut self, _: &[WaitInterest<'_>], _: Option<u64>) -> std::io::Result<WaitOutcome> {
        panic!("recording the original observation clock cannot wait");
    }
}

struct Ends {
    gate: OrganPort,
    decode: DecodePort,
    residency: OrganPort,
    _peers: Vec<std::os::fd::OwnedFd>,
}
impl Ends {
    fn new(gate_loss: bool) -> (Self, Failure) {
        let mut peers = Vec::new();
        let (near, far) = socketpair(
            AddressFamily::Unix,
            SockType::SeqPacket,
            None,
            SockFlag::SOCK_CLOEXEC,
        )
        .unwrap();
        let mut gate = OrganPort::from_owned(near).unwrap();
        if gate_loss {
            drop(far);
        } else {
            peers.push(far);
        }
        let (near, far) = socketpair(
            AddressFamily::Unix,
            SockType::SeqPacket,
            None,
            SockFlag::SOCK_CLOEXEC,
        )
        .unwrap();
        let decode = DecodePort::from_owned(near).unwrap();
        peers.push(far);
        let (near, far) = socketpair(
            AddressFamily::Unix,
            SockType::SeqPacket,
            None,
            SockFlag::SOCK_CLOEXEC,
        )
        .unwrap();
        let mut residency = OrganPort::from_owned(near).unwrap();
        if gate_loss {
            peers.push(far);
        } else {
            drop(far);
        }
        let failure = if gate_loss {
            gate.try_receive().unwrap_err()
        } else {
            residency.try_receive().unwrap_err()
        };
        assert_eq!(failure.fault, crate::failure::ChannelFault::Closed);
        (
            Self {
                gate,
                decode,
                residency,
                _peers: peers,
            },
            failure,
        )
    }
    fn ports<'a>(&'a self, tools: &'a ToolsCgroup) -> ServingOrganPorts<'a> {
        ServingOrganPorts {
            gate: &self.gate,
            decode: &self.decode,
            residency: &self.residency,
            tools,
        }
    }
}

#[test]
#[ignore = "needs a delegated cgroup scope"]
fn organ_loss_clock_survives_recoverable_save_expiry_without_reanchoring() {
    let layout = ScratchLayout::new("organ-clock-save");
    let tools = ToolsCgroup::open_beside_run(&layout.run).unwrap();
    for (gate_loss, key) in [
        (true, PumpDeadline::CaptureWindow),
        (false, PumpDeadline::SaveQueued),
    ] {
        let mut fx = InputFixture::new();
        let now = fx.run.clock.now().unwrap();
        let bound = AbsoluteBound::after(now, 20).unwrap();
        fx.run.context.arm(key, bound, &mut fx.run.clock).unwrap();
        let (ends, failure) = Ends::new(gate_loss);
        let source = if gate_loss {
            PumpSource::Gate
        } else {
            PumpSource::Residency
        };
        let mut clock = Clock::at(bound.deadline_ns() + 7);
        assert_eq!(
            fx.run.context.observe_serving_organ_end(
                source,
                &failure,
                ends.ports(&tools),
                false,
                &mut clock
            ),
            Err(RunFailure::Wait { key, bound })
        );
        let observation = fx
            .run
            .context
            .serving_organs
            .observation(gate_loss)
            .unwrap();
        assert_eq!(
            observation.observed_ns,
            Some(clock.now),
            "the original EOF must retain its clock before yielding expiry"
        );
        let settle = observation.settle_by;
        let containment = observation.containment_by;
        if gate_loss {
            assert!(observation.killed);
            assert_eq!(
                containment,
                Some(clock.now + crate::tools_cgroup::CONTAINMENT_BOUND.as_nanos() as u64)
            );
        }
        assert!(fx.run.context.failure().is_none());
        assert_eq!(fx.run.context.complete_wait(key), Some(bound));
        clock.now += 1_000_000;
        assert_eq!(
            fx.run.context.observe_serving_organ_end(
                source,
                &failure,
                ends.ports(&tools),
                false,
                &mut clock
            ),
            Ok(true)
        );
        let observation = fx
            .run
            .context
            .serving_organs
            .observation(gate_loss)
            .unwrap();
        assert_eq!(observation.observed_ns, Some(bound.deadline_ns() + 7));
        assert_eq!(observation.settle_by, settle);
        assert_eq!(observation.containment_by, containment);
    }
}

#[test]
#[ignore = "needs a delegated cgroup scope"]
fn organ_loss_clock_samples_after_a_preclock_inspection_expiry() {
    let layout = ScratchLayout::new("organ-clock-inspection");
    let tools = ToolsCgroup::open_beside_run(&layout.run).unwrap();
    crate::flow::context::read_tests::with_late_inspection_receipt(|context, io, bound| {
        let stale = context.last_ns;
        let now = io.now().unwrap() + 7;
        assert!(now > stale);
        let mut clock = Clock::at(now);
        let (ends, failure) = Ends::new(true);
        assert_eq!(
            context.observe_serving_organ_end(
                PumpSource::Gate,
                &failure,
                ends.ports(&tools),
                true,
                &mut clock
            ),
            Err(RunFailure::Wait {
                key: PumpDeadline::Leave,
                bound
            })
        );
        assert!(
            clock.calls > 0,
            "the early receipt yield supplied no new clock sample"
        );
        let observation = context.serving_organs.observation(true).unwrap();
        assert_eq!(observation.observed_ns, Some(now));
        assert_ne!(observation.observed_ns, Some(stale));
        assert_eq!(observation.containment, Some(Containment::Delivered));
        assert_eq!(context.deadlines.get(PumpDeadline::Leave), Some(bound));
        assert!(context.failure().is_none());
    });
}

#[test]
#[ignore = "needs a delegated cgroup scope"]
fn organ_loss_clock_preserves_fatal_expiry_without_starting_containment() {
    let layout = ScratchLayout::new("organ-clock-fatal");
    let tools = ToolsCgroup::open_beside_run(&layout.run).unwrap();
    let mut fx = InputFixture::new();
    let now = fx.run.clock.now().unwrap();
    let bound = AbsoluteBound::after(now, 20).unwrap();
    fx.run
        .context
        .arm(PumpDeadline::Gate, bound, &mut fx.run.clock)
        .unwrap();
    let (ends, failure) = Ends::new(true);
    let mut clock = Clock::at(bound.deadline_ns());
    let first = fx
        .run
        .context
        .observe_serving_organ_end(
            PumpSource::Gate,
            &failure,
            ends.ports(&tools),
            false,
            &mut clock,
        )
        .unwrap_err();
    assert!(matches!(
        first,
        RunFailure::Flow {
            reason: FlowHaltReason::GateBound,
            ..
        }
    ));
    assert_eq!(fx.run.context.failure(), Some(&first));
    let observation = fx.run.context.serving_organs.observation(true).unwrap();
    assert_eq!(observation.failure, failure);
    assert!(observation.observed_ns.is_none());
    assert!(observation.containment_by.is_none());
    assert!(!observation.killed);
}

#[test]
#[ignore = "needs a delegated cgroup scope"]
fn organ_loss_clock_failure_retains_eof_without_inventing_an_anchor() {
    let layout = ScratchLayout::new("organ-clock-failure");
    let tools = ToolsCgroup::open_beside_run(&layout.run).unwrap();
    for reversed in [false, true] {
        crate::flow::context::read_tests::with_late_inspection_receipt(|context, _io, _bound| {
            let (ends, failure) = Ends::new(true);
            let mut clock = Clock::at(context.last_ns);
            clock.broken_first = !reversed;
            clock.reversed_first = reversed;
            let first = context
                .observe_serving_organ_end(
                    PumpSource::Gate,
                    &failure,
                    ends.ports(&tools),
                    false,
                    &mut clock,
                )
                .unwrap_err();
            assert!(matches!(
                first,
                RunFailure::Flow {
                    reason: FlowHaltReason::TraceFailed,
                    ..
                }
            ));
            let observation = context.serving_organs.observation(true).unwrap();
            assert_eq!(observation.failure, failure);
            assert!(observation.observed_ns.is_none());
            assert!(observation.containment_by.is_none());
            assert_eq!(context.failure(), Some(&first));
            clock.now += 10;
            assert_eq!(
                context.observe_serving_organ_end(
                    PumpSource::Gate,
                    &failure,
                    ends.ports(&tools),
                    false,
                    &mut clock
                ),
                Ok(false)
            );
            assert!(
                context
                    .serving_organs
                    .observation(true)
                    .unwrap()
                    .observed_ns
                    .is_none()
            );
            assert_eq!(context.failure(), Some(&first));
        });
    }
}
