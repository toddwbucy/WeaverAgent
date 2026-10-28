//! Synthetic retained evidence checks the projection; joint tests establish
//! actual publication custody independently.
use super::*;
use crate::channel::{ChannelFault, ReceivedCapture};
use crate::save_points::serving::publication;
use weaver_types::state_flow::capture::{CaptureDirective, CaptureSender, SavePointLeg};
use weaver_types::state_flow::{
    PersistenceCase, PersistencePhase, RetainedArtifact, RetainedArtifactKind, SaveWindow,
};

fn fixture(cause: FailureCause) -> Failure {
    let enter: serde_json::Value = serde_json::from_str(include_str!(
        "../../weaver-types/tests/fixtures/state-enter.json"
    ))
    .unwrap();
    let loaded =
        serde_json::from_value(enter["payload"]["body"]["payload"]["binding"]["start"].clone())
            .unwrap();
    Failure {
        cause: Fault::Sequence(cause),
        evidence: Some(Box::new(Evidence {
            run: RunId("original-run".into()),
            loaded,
            request: CaptureEnvelope {
                exchange: weaver_types::ExchangeId {
                    opener: weaver_types::Opener::Admin,
                    ordinal: 1,
                },
                position: weaver_types::Position::Open,
                payload: CapturePayload::Directive(CaptureDirective::SavePoint {
                    cause: Cause { uid: 74 },
                    window: SaveWindow {
                        began_ns: 100.into(),
                        deadline_ns: 200.into(),
                    },
                }),
            },
            received: vec![],
            unexpected: None,
            report: None,
            completed: None,
            abort: None,
            cleanup_failure: None,
            capture_id: Some(7.into()),
            committed: true,
            publication_failure: Some(publication::Failure {
                case: PersistenceCase::PublicationFailed,
                phase: PersistencePhase::Publishing,
                published: None,
                artifacts: vec![RetainedArtifact {
                    location: "save-points/.actual.pending".into(),
                    kind: RetainedArtifactKind::Guard,
                    sha256: None,
                }],
                detail: "retained original publication".into(),
            }),
        })),
    }
}

fn heard(evidence: &mut Evidence, refusal: CaptureRefusal) {
    let envelope = CaptureEnvelope {
        exchange: evidence.request.exchange.clone(),
        position: weaver_types::Position::Close,
        payload: CapturePayload::Refusal(refusal),
    };
    let wire = envelope.encode(CaptureSender::Harness).unwrap();
    evidence.received.push(ReceivedCapture { envelope, wire });
}

#[test]
fn demand_silence_retains_unanswered_even_when_a_publication_guard_exists() {
    let mut failure = fixture(FailureCause::Channel(ChannelFault::Unanswered));
    assert_eq!(
        failure.refusal(),
        CaptureRefusal::Lifecycle(LifecycleRefusal::Unanswered)
    );
    assert!(failure.to_string().contains("save-points/.actual.pending"));
    failure.cause = Fault::Sequence(FailureCause::Publication(Box::new(
        failure
            .evidence
            .as_ref()
            .unwrap()
            .publication_failure
            .as_ref()
            .unwrap()
            .clone(),
    )));
    failure.evidence.as_mut().unwrap().cleanup_failure =
        Some(FailureCause::Channel(ChannelFault::Unanswered));
    assert_eq!(
        failure.refusal(),
        CaptureRefusal::Lifecycle(LifecycleRefusal::Unanswered)
    );
    assert!(failure.to_string().contains("capture cleanup"));
}

#[test]
fn demand_worker_failure_returns_only_root_owned_publication_locations() {
    let mut failure = fixture(FailureCause::WorkerOutcome);
    let evidence = failure.evidence.as_mut().unwrap();
    heard(
        evidence,
        CaptureRefusal::Save(SaveRefusal::FailedRun {
            run: RunId("original-run".into()),
            capture_id: Some(7.into()),
            phase: PersistencePhase::Queued,
            published: Some(Box::new(evidence.loaded.clone())),
            artifacts: vec![RetainedArtifact {
                location: "/worker-chosen/location".into(),
                kind: RetainedArtifactKind::Dump,
                sha256: None,
            }],
            detail: "worker claim".into(),
        }),
    );
    let CaptureRefusal::Save(SaveRefusal::FailedRun {
        run,
        capture_id,
        phase,
        published,
        artifacts,
        ..
    }) = failure.refusal()
    else {
        panic!("actual failed capture")
    };
    assert_eq!(run.0, "original-run");
    assert_eq!(capture_id, Some(7.into()));
    assert_eq!(phase, PersistencePhase::Publishing);
    assert!(published.is_none());
    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0].location, "save-points/.actual.pending");
}

#[test]
fn demand_capture_not_ready_requires_a_correlated_worker_outcome() {
    let mut failure = fixture(FailureCause::WorkerOutcome);
    let evidence = failure.evidence.as_mut().unwrap();
    evidence.committed = false;
    evidence.publication_failure = None;
    evidence.capture_id = None;
    heard(
        evidence,
        CaptureRefusal::Save(SaveRefusal::SavePointNotTaken {
            missed: SavePointLeg::CaptureNotReady,
        }),
    );
    assert_eq!(
        failure.refusal(),
        CaptureRefusal::Save(SaveRefusal::SavePointNotTaken {
            missed: SavePointLeg::CaptureNotReady
        })
    );
    let original = failure.evidence.as_ref().unwrap().received[0].wire.clone();
    failure.cause = Fault::Sequence(FailureCause::Protocol("foreign exchange"));
    assert_eq!(
        failure.refusal(),
        CaptureRefusal::Lifecycle(LifecycleRefusal::Malformed)
    );
    assert_eq!(
        failure.evidence.as_ref().unwrap().received[0].wire,
        original
    );
}

#[test]
fn demand_public_worker_outcome_rejects_direct_publication_claims() {
    let mut failure = fixture(FailureCause::WorkerOutcome);
    let evidence = failure.evidence.as_mut().unwrap();
    evidence.committed = false;
    heard(
        evidence,
        CaptureRefusal::Lifecycle(LifecycleRefusal::StatePersistenceInvalid {
            stage: weaver_types::state_flow::PersistenceStage::Publish,
            case: PersistenceCase::PublicationFailed,
            artifact_id: Some("a".repeat(64)),
            detail: "worker-supplied publication".into(),
        }),
    );
    let original = evidence.received[0].wire.clone();
    assert_eq!(
        failure.refusal(),
        CaptureRefusal::Lifecycle(LifecycleRefusal::Malformed)
    );
    assert!(
        !crate::surface::render_serving_refusal(&failure.refusal()).contains("worker-supplied")
    );
    assert_eq!(
        failure.evidence.as_ref().unwrap().received[0].wire,
        original
    );
}
