//! S9's actual capture reply crosses a real decode EOF with stderr retained.
use super::*;
use crate::flow::clock::testing::{NoReadyIo, NonblockingIo};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

struct RequestedEofIo<'a> {
    root: &'a Path,
    waited: bool,
    resume_claim:
        Option<std::sync::Arc<crate::flow::context::serving_observation::ServingObservationAuthor>>,
}

impl PumpIo for RequestedEofIo<'_> {
    fn now(&mut self) -> std::io::Result<u64> {
        crate::flow::clock::monotonic_ns()
    }

    fn wait(
        &mut self,
        interests: &[crate::flow::clock::WaitInterest<'_>],
        deadline: Option<u64>,
    ) -> std::io::Result<crate::flow::clock::WaitOutcome> {
        assert!(
            self.resume_claim
                .as_ref()
                .is_none_or(|claim| claim.sequence().is_none()),
            "S9 must return to the owed organ observation before waiting after Requested is Recorded"
        );
        if !self.waited {
            self.waited = true;
            std::fs::write(
                self.root.join("s9-requested-author-waiting"),
                b"actual Requested author waits for recorder capacity",
            )?;
        }
        crate::flow::clock::MonotonicIo.wait(interests, deadline)
    }
}

impl FoundationEnteredRollback {
    pub(in crate::flow::context::worker_startup) fn test_resting_requested_author_yields_to_observation(
        &mut self,
        root: &Path,
        hold_trace: &AtomicBool,
        trace_paused: &AtomicBool,
    ) -> bool {
        if self.resting.save.is_none() {
            return false;
        }
        let end = Instant::now() + Duration::from_secs(10);
        hold_trace.store(true, Ordering::SeqCst);
        while !trace_paused.load(Ordering::SeqCst) {
            assert!(Instant::now() < end, "actual S9 trace reader did not pause");
            std::thread::yield_now();
        }
        let context = &mut self.run.as_mut().unwrap().context;
        let window = context.deadlines.get(PumpDeadline::SaveQueued);
        assert!(window.is_some());
        let mut clock = crate::flow::clock::testing::FakePumpIo::new(context.last_ns);
        crate::flow::context::worker_startup::input::management::stop::tests::fill_trace(
            context, &mut clock,
        );
        assert_eq!(
            context.barrier.recorder().boundary().queued,
            weaver_trace::serving::ORDINARY_CAPACITY
        );
        let mut io = RequestedEofIo {
            root,
            waited: false,
            resume_claim: None,
        };
        let progress = self.advance_with_io(&mut io);
        assert!(self.detail().is_none(), "{progress:?}: {:?}", self.detail());
        assert!(
            io.waited,
            "S9 Requested must encounter actual recorder pressure"
        );
        let context = &self.run.as_ref().unwrap().context;
        assert!(
            context.serving_organs.no_residency(),
            "actual SPU EOF must interrupt S9's pending Requested author"
        );
        let pending = context
            .pending_author()
            .expect("original S9 Requested author");
        assert_eq!(pending.event.kind, weaver_trace::serving::Kind::SavePoint);
        let claim = context.serving_observation_author.as_ref().unwrap().clone();
        assert_eq!(claim.ticket(), Some(pending.ticket));
        assert!(claim.sequence().is_none());
        assert!(!self.resting.save.as_ref().unwrap().test_boundary_pending());
        loop {
            match self.test_resting_observation(&mut io) {
                Ok(serving_organ_loss::ObservationProgress::EarlierAuthor) => break,
                Ok(serving_organ_loss::ObservationProgress::Pending { .. }) => (),
                Ok(serving_organ_loss::ObservationProgress::Complete) => {
                    panic!("S9 Requested still owns the original author claim");
                }
                Err(failure) => {
                    assert!(matches!(
                        failure,
                        RunFailure::Transport {
                            source: PumpSource::Residency,
                            ..
                        }
                    ));
                    assert!(self.retain_resting_interrupt(&failure, &mut io).unwrap());
                }
            }
            assert!(Instant::now() < end, "actual S9 organ loss did not settle");
            std::thread::yield_now();
        }
        assert!(std::sync::Arc::ptr_eq(
            self.run
                .as_ref()
                .unwrap()
                .context
                .serving_observation_author
                .as_ref()
                .unwrap(),
            &claim
        ));
        hold_trace.store(false, Ordering::SeqCst);
        io.resume_claim = Some(std::sync::Arc::clone(&claim));
        let progress = self.advance_with_io(&mut io);
        assert!(self.detail().is_none(), "{progress:?}: {:?}", self.detail());
        assert!(
            claim.sequence().is_some(),
            "resume the exact S9 Requested claim"
        );
        assert_eq!(
            progress,
            FoundationEnteredRollbackProgress::Progress,
            "finishing S9's earlier author must yield to the owed organ observation, not a blocking wait"
        );
        // The ordinary caller's wait API must also return without polling.
        assert_eq!(
            self.wait_with_io(&mut io),
            FoundationEnteredRollbackProgress::Progress
        );
        assert!(self.resting.save.as_ref().unwrap().test_boundary_pending());
        let context = &self.run.as_ref().unwrap().context;
        assert!(context.serving_observation_author.is_none());
        assert!(context.has_owed_organ_observation());
        assert_eq!(context.deadlines.get(PumpDeadline::SaveQueued), window);
        assert!(context.deadlines.get(PumpDeadline::CaptureWindow).is_none());
        assert!(context.capture_snapshot().is_none());
        while !self.test_resting_settled(&mut crate::flow::clock::MonotonicIo) {
            assert!(
                Instant::now() < end,
                "S9 organ observation was not Recorded"
            );
            std::thread::yield_now();
        }
        std::fs::write(
            root.join("s9-organ-observation-recorded"),
            b"actual organ observation completed before boundary election",
        )
        .unwrap();
        true
    }

    /// Stop after actual S9 admission, before its Requested author or boundary
    /// election. The child closes decode but retains stderr until both queued
    /// states have been checked under the original save window.
    pub(in crate::flow::context::worker_startup) fn test_keep_resting_queued_save_behind_settlement(
        &mut self,
        root: &Path,
        requested: bool,
    ) -> bool {
        if self.resting.save.is_none() {
            return false;
        }
        let end = Instant::now() + Duration::from_secs(3);
        if !requested {
            loop {
                let save = self.resting.save.as_mut().unwrap();
                if save.test_boundary_pending() {
                    break;
                }
                let run = self.run.as_mut().unwrap();
                let (residency, decode) = run.resources.spu_ports_mut().unwrap();
                let mut lifecycle = self.dials.as_mut().unwrap().resting_ports();
                lifecycle.residency = Some(residency);
                save.advance(
                    &mut run.context,
                    PumpPorts {
                        lifecycle,
                        gate: None,
                        decode: Some(decode),
                        label: None,
                        io: &mut crate::flow::clock::MonotonicIo,
                    },
                    false,
                )
                .unwrap();
                assert!(Instant::now() < end, "S9 Requested author did not settle");
            }
        }
        assert_eq!(
            self.resting.save.as_ref().unwrap().test_boundary_pending(),
            !requested
        );
        let original = self
            .run
            .as_ref()
            .unwrap()
            .context
            .deadlines
            .get(PumpDeadline::SaveQueued);
        assert!(original.is_some());
        assert!(!self.resting.save.as_ref().unwrap().reply_held());
        std::fs::write(root.join("s9-save-queued"), b"actual S9 save admitted").unwrap();
        while !root.join("s9-decode-closed").exists() {
            assert!(
                Instant::now() < end,
                "SPU must close its actual decode socket"
            );
            std::thread::yield_now();
        }
        let failure = loop {
            let (_, decode) = self
                .run
                .as_mut()
                .unwrap()
                .resources
                .spu_ports_mut()
                .unwrap();
            if let Err(failure) = decode.try_receive() {
                break failure;
            }
            assert!(
                Instant::now() < end,
                "actual queued S9 decode EOF was not observed"
            );
            std::thread::yield_now();
        };
        let mut io = NoReadyIo {
            now: crate::flow::clock::monotonic_ns().unwrap(),
            waits: 0,
        };
        assert!(
            self.retain_resting_interrupt(
                &RunFailure::Transport {
                    source: PumpSource::Decode,
                    failure,
                },
                &mut io,
            )
            .unwrap()
        );
        assert!(!self.test_resting_settled(&mut io));
        loop {
            let progress = self.advance_with_io(&mut io);
            assert!(self.detail().is_none(), "{progress:?}: {:?}", self.detail());
            let context = &self.run.as_ref().unwrap().context;
            assert_eq!(
                context.deadlines.get(PumpDeadline::SaveQueued),
                original,
                "S9 queued save began before organ settlement"
            );
            assert!(
                context.deadlines.get(PumpDeadline::CaptureWindow).is_none(),
                "S9 Requested cannot acquire a boundary in the same advance call"
            );
            assert!(context.capture_snapshot().is_none());
            assert!(!self.resting.save.as_ref().unwrap().reply_held());
            assert!(!self.test_resting_settled(&mut io));
            if self.resting.save.as_ref().unwrap().test_boundary_pending() {
                break;
            }
            assert!(
                Instant::now() < end,
                "S9 Requested must finish accounting while stderr settlement remains pending"
            );
            std::thread::yield_now();
        }
        std::fs::write(
            root.join("s9-queued-boundary-checked"),
            b"queued save retained; permit actual stderr EOF",
        )
        .unwrap();
        // Settle only the observed death before returning to the save owner.
        // A completed observation owns its Recorded sequence; admin checks this
        // causal marker when the later, actual Beginning packet arrives.
        while !self.test_resting_settled(&mut crate::flow::clock::MonotonicIo) {
            assert!(
                Instant::now() < end,
                "S9 organ observation was not Recorded"
            );
            std::thread::yield_now();
        }
        let context = &self.run.as_ref().unwrap().context;
        assert_eq!(context.deadlines.get(PumpDeadline::SaveQueued), original);
        assert!(context.deadlines.get(PumpDeadline::CaptureWindow).is_none());
        assert!(context.capture_snapshot().is_none());
        std::fs::write(
            root.join("s9-organ-observation-recorded"),
            b"actual organ observation completed before boundary election",
        )
        .unwrap();
        true
    }

    pub(in crate::flow::context::worker_startup) fn test_reduce_resting_reply_during_settlement(
        &mut self,
        root: &Path,
    ) {
        let end = Instant::now() + Duration::from_secs(3);
        while !root.join("s9-decode-closed").exists() || !root.join("s9-member-paused").exists() {
            assert!(
                Instant::now() < end,
                "actual decode EOF and paused member must precede ImageChecked"
            );
            std::thread::yield_now();
        }
        let failure = loop {
            let (_, decode) = self
                .run
                .as_mut()
                .unwrap()
                .resources
                .spu_ports_mut()
                .unwrap();
            if let Err(failure) = decode.try_receive() {
                break failure;
            }
            assert!(
                Instant::now() < end,
                "actual S9 decode EOF was not observed"
            );
            std::thread::yield_now();
        };
        std::fs::write(root.join("s9-pin-ready"), b"capture reader ready").unwrap();
        let save = self.resting.save.as_mut().unwrap();
        while !save.reply_held() {
            save.port().unwrap().try_receive().unwrap();
            assert!(
                Instant::now() < end,
                "admin did not supply its actual ImageChecked"
            );
            std::thread::yield_now();
        }
        assert!(matches!(
            save.port()
                .unwrap()
                .capture_held()
                .unwrap()
                .envelope
                .payload,
            CapturePayload::SavePointImageChecked(_)
        ));
        let mut io = NoReadyIo {
            now: crate::flow::clock::monotonic_ns().unwrap(),
            waits: 0,
        };
        assert!(
            self.retain_resting_interrupt(
                &RunFailure::Transport {
                    source: PumpSource::Decode,
                    failure,
                },
                &mut io
            )
            .unwrap()
        );
        assert!(!self.test_resting_settled(&mut io));
        let window = self
            .run
            .as_ref()
            .unwrap()
            .context
            .deadlines
            .get(PumpDeadline::CaptureWindow);
        assert!(window.is_some());
        let before_waits = io.waits;
        let progress = self.advance_with_io(&mut io);
        assert!(self.detail().is_none(), "{progress:?}: {:?}", self.detail());
        assert_eq!(progress, FoundationEnteredRollbackProgress::Progress);
        assert!(
            !self.resting.save.as_ref().unwrap().reply_held(),
            "S9 stranded ImageChecked behind organ settlement"
        );
        assert!(
            io.waits - before_waits <= 1,
            "S9 added a second wait after save progress"
        );
        assert_eq!(
            self.run
                .as_ref()
                .unwrap()
                .context
                .deadlines
                .get(PumpDeadline::CaptureWindow),
            window
        );
        assert!(
            !self.test_resting_settled(&mut io),
            "save reduction must not end or renew settlement"
        );
        assert!(
            !root.join("s9-finished-carried").exists(),
            "the actual member has not answered Acknowledge yet"
        );
        let mut io = NonblockingIo { waits: io.waits };
        std::fs::write(root.join("s9-member-resume"), b"allow actual Finished").unwrap();
        while !root.join("s9-finished-carried").exists() {
            assert!(
                !self.resting.save.as_ref().unwrap().reply_held(),
                "Prepared remains withheld while S9 advances member completion"
            );
            let before_waits = io.waits;
            let progress = self.advance_with_io(&mut io);
            assert!(self.detail().is_none(), "{progress:?}: {:?}", self.detail());
            assert!(io.waits - before_waits <= 1);
            assert_eq!(
                self.run
                    .as_ref()
                    .unwrap()
                    .context
                    .deadlines
                    .get(PumpDeadline::CaptureWindow),
                window
            );
            assert!(
                !self.test_resting_settled(&mut io),
                "S9 member completion must progress before stderr settlement finishes"
            );
            assert!(
                Instant::now() < end,
                "without a held admin packet S9 must still carry the member Finished"
            );
            std::thread::yield_now();
        }
        assert!(!self.resting.save.as_ref().unwrap().reply_held());
        std::fs::write(
            root.join("s9-reply-reduced"),
            b"held reply reduced under original window",
        )
        .unwrap();
    }

    fn test_resting_settled(&mut self, io: &mut impl PumpIo) -> bool {
        matches!(
            self.test_resting_observation(io).unwrap(),
            serving_organ_loss::ObservationProgress::Complete
        )
    }

    fn test_resting_observation(
        &mut self,
        io: &mut impl PumpIo,
    ) -> Result<serving_organ_loss::ObservationProgress, RunFailure> {
        let run = self.run.as_mut().unwrap();
        run.resources.refresh_spu_observation(&mut run.context);
        let (residency, decode) = run.resources.spu_ports_mut().unwrap();
        let mut lifecycle = self.dials.as_mut().unwrap().resting_ports();
        lifecycle.residency = Some(residency);
        run.context.advance_leaving_organ_faults(
            &mut PumpPorts {
                lifecycle,
                gate: None,
                decode: Some(decode),
                label: None,
                io,
            },
            self.gate.as_ref().and_then(RollbackGate::world_closed),
        )
    }
}
