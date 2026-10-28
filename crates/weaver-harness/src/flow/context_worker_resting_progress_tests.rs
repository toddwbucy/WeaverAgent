//! S9's actual capture reply crosses a real decode EOF with stderr retained.
use super::*;
use crate::flow::clock::testing::NoReadyIo;
use std::path::Path;
use std::time::{Duration, Instant};

impl FoundationEnteredRollback {
    pub(in crate::flow::context::worker_startup) fn test_reduce_resting_reply_during_settlement(
        &mut self,
        root: &Path,
    ) {
        let end = Instant::now() + Duration::from_secs(3);
        while !root.join("s9-decode-closed").exists() {
            assert!(
                Instant::now() < end,
                "SPU did not close its actual decode socket"
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
        std::fs::write(
            root.join("s9-reply-reduced"),
            b"held reply reduced under original window",
        )
        .unwrap();
    }

    fn test_resting_settled(&mut self, io: &mut impl PumpIo) -> bool {
        let run = self.run.as_mut().unwrap();
        run.resources.refresh_spu_observation(&mut run.context);
        let (residency, decode) = run.resources.spu_ports_mut().unwrap();
        let mut lifecycle = self.dials.as_mut().unwrap().resting_ports();
        lifecycle.residency = Some(residency);
        matches!(
            run.context
                .advance_leaving_organ_faults(
                    &mut PumpPorts {
                        lifecycle,
                        gate: None,
                        decode: Some(decode),
                        label: None,
                        io,
                    },
                    self.gate.as_ref().and_then(RollbackGate::world_closed)
                )
                .unwrap(),
            serving_organ_loss::ObservationProgress::Complete
        )
    }
}
