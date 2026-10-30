//! Ready selection precedes delivery, including the entered release rules.
use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn control(
    root: &std::path::Path,
    child: &mut std::process::Child,
    source: &EnterEnvelope,
    original: &OwnedFd,
    reader: &mut UnixStream,
    member: &mut UnixStream,
    end: Instant,
) {
    pressure::fill(original, root, end);
    no_bytes(reader);
    marker(root, "diagnostic-setup-ready");
    let probe = dial(&root.join("admin.sock"));
    loop {
        send(&probe, 990, LifecycleDirective::Observe);
        let answer = ObservationEnvelope::decode(&receive(&probe, end)).unwrap();
        assert_eq!(answer.exchange.ordinal, 990);
        match answer.payload {
            ObservationContent::Answer(ObservationAnswer::State {
                state: AgentState::Idle,
                ..
            }) => break,
            ObservationContent::Answer(ObservationAnswer::InTransition { .. }) => (),
            other => panic!("Ready selection observation: {other:?}"),
        }
        assert!(Instant::now() < end);
    }
    // Original has consumed no reply. Ready is selected behind the observed
    // blocked queue, so this rollback must obey the entered-run rule.
    no_bytes(reader);
    assert!(!root.join("diagnostic-refeed").exists());
    marker(root, "diagnostic-setup-settle");
    send(
        &probe,
        991,
        LifecycleDirective::Leave {
            cause: Cause { uid: 71 },
            forced: true,
            rollback: true,
        },
    );
    let answer: OrganEnvelope = serde_json::from_slice(&receive(&probe, end)).unwrap();
    assert_eq!(answer.exchange.opener, Opener::Admin);
    assert_eq!(answer.exchange.ordinal, 991);
    assert_eq!(answer.position, Position::Close);
    assert_eq!(
        answer.payload,
        Payload::Answer(LifecycleAnswer::Left {
            save_point: None,
            forced: true,
            no_state: true,
        })
    );
    pressure::drain_outcome(
        original,
        source,
        end,
        Payload::Answer(LifecycleAnswer::Ready),
    );
    let (status, expired) = crate::test_child::wait_until(child, end).unwrap();
    assert!(
        !expired && status.success(),
        "entered cleanup must finish after unconfirmed release"
    );
    for marker_name in [
        "diagnostic-setup-spu-pid",
        "diagnostic-setup-classifier-pid",
    ] {
        let pid = std::fs::read_to_string(root.join(marker_name)).unwrap();
        assert!(!std::path::Path::new(&format!("/proc/{}", pid.trim())).exists());
    }
    assert!(root.join("diagnostic-setup-release").exists());
    assert!(!root.join("diagnostic-refeed").exists());
    closed_empty(reader, end);
    closed_empty(member, end);
}
