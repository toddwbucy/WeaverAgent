//! The composed Leave wait uses real poll and its loaded capture bounds.
//! Scripted member results arrive only after the original request is aborted.
use super::*;
use crate::flow::clock::{AbsoluteBound, MonotonicIo, PumpIo, WaitInterest, WaitOutcome};

pub(super) struct Probe {
    key: PumpDeadline,
    finished: bool,
    published: bool,
    forced: bool,
    original: Option<AbsoluteBound>,
    expired_in_wait: bool,
}

impl Probe {
    pub(super) fn new(mode: &str, member: &mut Member) -> Option<Self> {
        let finished = mode.ends_with("-finished-timeout");
        let published = mode.ends_with("-published-timeout");
        let window = mode.ends_with("-window-timeout") || published;
        if !finished && !window && !mode.ends_with("-snapshot-timeout") {
            return None;
        }
        assert!(member.held_control.is_none());
        member.hold_control = if published {
            None
        } else if finished {
            Some("finished")
        } else {
            Some("snapshot")
        };
        Some(Self {
            key: if window {
                PumpDeadline::CaptureWindow
            } else {
                PumpDeadline::CaptureResult
            },
            finished,
            published,
            forced: expected_forced(mode),
            original: None,
            expired_in_wait: false,
        })
    }

    pub(super) fn wait(&mut self, owner: &mut LeaveOwner) {
        let inner = owner.test_inner();
        let context = inner.test_context();
        let expected_phase = if self.finished {
            crate::flow::coordinator::CapturePhase::AcknowledgePending
        } else {
            crate::flow::coordinator::CapturePhase::SnapshotPending
        };
        let bound = context.deadlines.get(self.key).filter(|_| {
            self.key == PumpDeadline::CaptureWindow
                || context
                    .capture_snapshot()
                    .is_some_and(|s| s.phase == expected_phase)
        });
        if let Some(bound) = bound {
            if let Some(original) = self.original {
                assert_eq!(bound, original, "capture wait cannot restart");
            } else {
                self.original = Some(bound);
            }
        }
        let mut io = WaitProbe {
            deadline: bound.map(AbsoluteBound::deadline_ns),
            expired: false,
        };
        inner.wait_with_io(&mut io);
        if io.expired {
            self.expired_in_wait = true;
            assert!(
                inner.test_context().failure().is_none(),
                "capture deadline must return to its owner before failure is elected"
            );
            assert_eq!(inner.test_context().deadlines.get(self.key), bound);
            assert_eq!(
                inner
                    .test_context()
                    .deadlines
                    .expired(MonotonicIo.now().unwrap())
                    .min_by_key(|(_, bound)| bound.deadline_ns())
                    .map(|(key, _)| key),
                Some(self.key),
                "the fixture must exercise its declared capture deadline"
            );
        }
    }

    pub(super) fn assert_finished(
        &self,
        owner: &mut LeaveOwner,
        member: &Member,
        packets: &[CapturePayload],
        rows: &[serde_json::Value],
        report: Option<&s::SavePointReport>,
    ) {
        assert!(self.expired_in_wait, "the real composed wait must expire");
        assert!(report.is_none());
        assert!(!packets.iter().any(|packet| matches!(
            packet,
            CapturePayload::SavePointCaptureComplete(_)
                | CapturePayload::Answer(CaptureAnswer::Left {
                    save_point: Some(_),
                    ..
                })
                | CapturePayload::Answer(CaptureAnswer::SavePointTaken { .. })
        )));
        if self.published {
            assert!(
                !packets
                    .iter()
                    .any(|packet| matches!(packet, CapturePayload::Answer(_)))
            );
            assert_eq!(owner.progress(), LeaveProgress::Closed);
            assert!(owner.test_inner().test_context().failure().is_some());
            assert!(
                packets
                    .iter()
                    .any(|p| matches!(p, CapturePayload::SavePointCommit(_)))
            );
            return;
        }
        assert_eq!(
            owner.progress(),
            if self.forced {
                LeaveProgress::Closed
            } else {
                LeaveProgress::CaptureRefused
            }
        );
        assert!(owner.test_inner().test_context().failure().is_none());
        assert!(
            owner
                .test_inner()
                .test_context()
                .capture_snapshot()
                .is_none()
        );
        assert_eq!(owner.unload_sequence().is_some(), self.forced);
        assert!(member.held_control.is_none());
        let mut expected = if self.finished {
            vec!["snapshot", "acknowledge", "abort"]
        } else {
            vec!["snapshot", "abort"]
        };
        if self.forced {
            expected.push("grants");
        }
        let missed = if self.finished {
            SavePointLeg::Finished
        } else {
            SavePointLeg::Answer
        };
        assert!(packets.iter().any(|packet| matches!(packet,
            CapturePayload::SavePointCaptureRefused(ack) if ack.missed == missed)));
        if self.forced {
            assert!(matches!(
                packets.last(),
                Some(CapturePayload::Answer(CaptureAnswer::Left {
                    save_point: None,
                    forced: true,
                    no_state: false,
                }))
            ));
        } else {
            assert!(
                matches!(packets.last(), Some(CapturePayload::Refusal(CaptureRefusal::Save(
                SaveRefusal::SavePointNotTaken { missed: actual }
            ))) if *actual == missed)
            );
        }
        assert!(
            member
                .requests
                .ends_with(&expected.iter().map(|s| s.to_string()).collect::<Vec<_>>())
        );
        assert!(!packets.iter().any(|p| matches!(
            p,
            CapturePayload::SavePointImageFinished(_) | CapturePayload::SavePointCommit(_)
        )));
        if !self.finished {
            assert!(
                !packets
                    .iter()
                    .any(|p| matches!(p, CapturePayload::SavePointImage(_)))
            );
        }
        let saves: Vec<_> = rows
            .iter()
            .filter(|row| row["kind"] == "save_point")
            .collect();
        assert_eq!(saves.len(), 2);
        assert_eq!(saves[0]["payload"]["phase"], "requested");
        assert_eq!(saves[1]["payload"]["phase"], "refused");
        let late_operation = if self.finished {
            "acknowledge"
        } else {
            "snapshot"
        };
        let measured = |operation| {
            rows.iter()
                .filter(|row| row["payload"]["observation"]["operation"] == operation)
                .collect::<Vec<_>>()
        };
        let late = measured(late_operation);
        let abort = measured("abort");
        assert_eq!(late.len(), 1, "retain the actual late result observation");
        assert_eq!(abort.len(), 1, "only matching abort proves cleanup");
        let number = |value: &serde_json::Value| value.as_str().unwrap().parse::<u64>().unwrap();
        assert!(
            number(&late[0]["payload"]["harness_received_ns"])
                >= self.original.unwrap().deadline_ns()
        );
        assert_eq!(
            late[0]["payload"]["observation"]["request_id"],
            abort[0]["payload"]["observation"]["request_id"]
        );
        assert!(number(&late[0]["sequence"]) < number(&abort[0]["sequence"]));
        assert!(number(&abort[0]["sequence"]) < number(&saves[1]["sequence"]));
        assert_eq!(
            saves[1]["payload"]["refusal_code"],
            if self.finished {
                "finished_timeout"
            } else {
                "snapshot_timeout"
            }
        );
        assert!(!rows.iter().any(|row| row["kind"] == "fault"));
    }
}

struct WaitProbe {
    deadline: Option<u64>,
    expired: bool,
}

impl PumpIo for WaitProbe {
    fn now(&mut self) -> std::io::Result<u64> {
        MonotonicIo.now()
    }
    fn wait(
        &mut self,
        interests: &[WaitInterest<'_>],
        deadline: Option<u64>,
    ) -> std::io::Result<WaitOutcome> {
        let result = MonotonicIo.wait(interests, deadline)?;
        self.expired |= self.deadline.is_some()
            && deadline == self.deadline
            && matches!(result, WaitOutcome::Timeout);
        Ok(result)
    }
}
