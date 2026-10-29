//! Projection checks use synthetic retained facts. The delegated invocation
//! and joint tests establish those facts against actual filesystem custody.
use super::*;
use crate::save_points::serving::publication;
use weaver_types::state_flow::{PersistencePhase, RetainedArtifact, RetainedArtifactKind};

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
        cleanup: None,
        joined: None,
        evidence: Some(Box::new(Evidence {
            force_trigger: None,
            run: weaver_types::RunId("original-run".into()),
            loaded,
            received: vec![],
            unexpected: None,
            report: None,
            completed: None,
            capture_id: Some(7.into()),
            committed: true,
            publication_failure: Some(publication::Failure {
                case: PersistenceCase::PublicationFailed,
                phase: PersistencePhase::Publishing,
                published: None,
                artifacts: vec![RetainedArtifact {
                    location: "save-points/.original.pending".into(),
                    kind: RetainedArtifactKind::Guard,
                    sha256: None,
                }],
                detail: "root-owned location probe failed for the renamed part".into(),
            }),
        })),
    }
}

#[test]
fn leave_public_timeout_does_not_infer_worker_failure_or_index_append() {
    let failure = fixture(FailureCause::Channel(ChannelFault::Unanswered));
    assert_eq!(
        failure.refusal(),
        CaptureRefusal::Lifecycle(LifecycleRefusal::Unanswered)
    );
    assert!(failure.to_string().contains("location probe failed"));
    assert!(
        failure
            .to_string()
            .contains("save-points/.original.pending")
    );
    let mut failure = failure;
    failure.cause = Fault::Sequence(FailureCause::Channel(ChannelFault::Closed));
    let public: serde_json::Value =
        serde_json::from_str(&crate::surface::render_serving_refusal(&failure.refusal())).unwrap();
    assert_eq!(public["kind"], "failed_run");
    assert_eq!(public["capture_id"], "7");
    assert!(
        public.get("published").is_none(),
        "a guard is not a verified index append"
    );
    assert!(public["artifacts"][0]["sha256"].is_null());
}

#[test]
fn leave_public_failure_keeps_local_identity_and_locations_over_worker_claims() {
    let mut failure = fixture(FailureCause::WorkerOutcome);
    let malicious = weaver_types::state_flow::capture::CaptureEnvelope {
        exchange: weaver_types::ExchangeId {
            opener: weaver_types::Opener::Admin,
            ordinal: 1,
        },
        position: weaver_types::Position::Close,
        payload: CapturePayload::Refusal(CaptureRefusal::Save(SaveRefusal::FailedRun {
            run: weaver_types::RunId("original-run".into()),
            capture_id: Some(7.into()),
            phase: PersistencePhase::Queued,
            published: Some(Box::new(failure.evidence.as_ref().unwrap().loaded.clone())),
            artifacts: vec![RetainedArtifact {
                location: "/worker-supplied/claim".into(),
                kind: RetainedArtifactKind::Dump,
                sha256: None,
            }],
            detail: "worker claims publication completed".into(),
        })),
    };
    let wire = malicious
        .encode(weaver_types::state_flow::capture::CaptureSender::Harness)
        .unwrap();
    failure.evidence.as_mut().unwrap().unexpected = Some(crate::channel::ReceivedCapture {
        envelope: malicious,
        wire: wire.clone(),
    });
    failure.cleanup = Some(Box::new(LifecycleRefusal::WorkerWouldNotExit));
    let rendered = crate::surface::render_serving_refusal(&failure.refusal());
    let public: serde_json::Value = serde_json::from_str(&rendered).unwrap();
    assert_eq!(public["phase"], "publishing");
    assert!(public.get("published").is_none());
    assert_eq!(
        public["artifacts"][0]["location"],
        "save-points/.original.pending"
    );
    assert!(!rendered.contains("worker-supplied"));
    assert!(
        public["detail"]
            .as_str()
            .unwrap()
            .contains("separate run cleanup refusal")
    );
    assert_eq!(
        failure
            .evidence
            .as_ref()
            .unwrap()
            .unexpected
            .as_ref()
            .unwrap()
            .wire,
        wire
    );
    assert_eq!(
        crate::surface::render_serving_refusal(&failure.refusal()),
        rendered
    );
}

#[test]
fn leave_public_local_publication_failure_after_commit_keeps_the_guard_account() {
    let mut failure = fixture(FailureCause::Channel(ChannelFault::Closed));
    let publication = failure
        .evidence
        .as_ref()
        .unwrap()
        .publication_failure
        .clone()
        .unwrap();
    failure.cause = Fault::Sequence(FailureCause::Publication(Box::new(publication)));
    assert!(matches!(
        failure.refusal(),
        CaptureRefusal::Save(SaveRefusal::FailedRun {
            published: None,
            ..
        })
    ));
    failure.evidence.as_mut().unwrap().committed = false;
    assert!(matches!(
        failure.refusal(),
        CaptureRefusal::Lifecycle(LifecycleRefusal::StatePersistenceInvalid {
            stage: PersistenceStage::Publish,
            case: PersistenceCase::PublicationFailed,
            ..
        })
    ));
}

#[test]
fn leave_public_worker_outcomes_never_promote_publication_claims_from_owner_or_joiner() {
    use weaver_types::state_flow::capture::{CaptureEnvelope, CaptureSender};
    use weaver_types::{ExchangeId, Opener, Position, RefusingOrgan};
    for joined in [false, true] {
        let claim = LifecycleRefusal::StatePersistenceInvalid {
            stage: PersistenceStage::Publish,
            case: PersistenceCase::PublicationFailed,
            artifact_id: Some("a".repeat(64)),
            detail: "worker-supplied publication".into(),
        };
        for (refusal, accepted) in [
            (claim.clone(), false),
            (
                LifecycleRefusal::OrganRefused {
                    organ: RefusingOrgan::Spu,
                    reason: Box::new(claim.clone()),
                },
                false,
            ),
            (LifecycleRefusal::NoResidency, false),
            (
                LifecycleRefusal::OrganRefused {
                    organ: RefusingOrgan::Gate,
                    reason: Box::new(claim),
                },
                true,
            ),
            (LifecycleRefusal::DescriptorsUnusable, true),
        ] {
            let mut failure = fixture(FailureCause::WorkerOutcome);
            failure.evidence.as_mut().unwrap().committed = false;
            let envelope = CaptureEnvelope {
                exchange: ExchangeId {
                    opener: Opener::Admin,
                    ordinal: 1,
                },
                position: Position::Close,
                payload: CapturePayload::Refusal(CaptureRefusal::Lifecycle(refusal.clone())),
            };
            let wire = envelope.encode(CaptureSender::Harness).unwrap();
            let received = crate::channel::ReceivedCapture {
                envelope,
                wire: wire.clone(),
            };
            if joined {
                failure.evidence = None;
                failure.joined = Some(Box::new(super::super::join::Evidence {
                    noted_run: Some(Some("original-run".into())),
                    directive: None,
                    received: Some(received),
                }));
            } else {
                failure.evidence.as_mut().unwrap().unexpected = Some(received);
            }
            assert_eq!(
                failure.refusal(),
                CaptureRefusal::Lifecycle(if accepted {
                    refusal
                } else {
                    LifecycleRefusal::Malformed
                })
            );
            if !accepted {
                assert!(
                    !crate::surface::render_serving_refusal(&failure.refusal())
                        .contains("worker-supplied")
                );
            }
            let retained = if joined {
                &failure
                    .joined
                    .as_ref()
                    .unwrap()
                    .received
                    .as_ref()
                    .unwrap()
                    .wire
            } else {
                &failure
                    .evidence
                    .as_ref()
                    .unwrap()
                    .unexpected
                    .as_ref()
                    .unwrap()
                    .wire
            };
            assert_eq!(retained, &wire);
        }
    }
}
