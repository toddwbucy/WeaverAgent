//! Actual setup waits must hear management before granting a Diagnostic run.
use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn identity(
    root: &std::path::Path,
    mode: &str,
    child: &mut std::process::Child,
    source: &EnterEnvelope,
    sink: OwnedFd,
    state: OwnedFd,
    reader: &mut UnixStream,
    member: &mut UnixStream,
    end: Instant,
) {
    let mut original = Some(dial(&root.join("admin.sock")));
    send_enter(
        original.as_ref().unwrap(),
        &source.encode().unwrap(),
        &[sink.as_raw_fd(), state.as_raw_fd()],
    );
    drop((sink, state));
    member.set_nonblocking(false).unwrap();
    member
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut input = BufReader::new(&mut *member);
    let mut line = String::new();
    input.read_line(&mut line).unwrap();
    assert_eq!(
        line.trim_end(),
        weaver_trace::opener(
            "session",
            &weaver_trace::Election {
                all_kinds: true,
                keys: Vec::new(),
            },
        )
        .trim_end()
    );
    line.clear();
    input.read_line(&mut line).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&line).unwrap(),
        serde_json::json!({"ask":{"identity":{}}})
    );
    assert!(input.buffer().is_empty());
    drop(input);
    // Keep the identity answer incomplete. The control owner cannot depend
    // on a complete line, a member timeout or the preload becoming sealed.
    member.write_all(b"{\"answer\":{\"identity\":").unwrap();
    assert!(!root.join("diagnostic-spawned").exists());
    no_bytes(reader);
    if mode.ends_with("-eof") {
        drop(original.take());
    }

    // A silent accepted peer must not monopolize setup's receive service.
    let silent = dial(&root.join("admin.sock"));
    let probe = dial(&root.join("admin.sock"));
    send(&probe, 960, LifecycleDirective::Observe);
    let response = ObservationEnvelope::decode(&receive(&probe, end)).unwrap();
    assert_eq!(response.exchange.ordinal, 960);
    assert!(matches!(
        response.payload,
        ObservationContent::Answer(ObservationAnswer::InTransition { .. })
    ));
    send(
        &probe,
        961,
        LifecycleDirective::Stop {
            cause: Cause { uid: 0 },
        },
    );
    let stopped: OrganEnvelope = serde_json::from_slice(&receive(&probe, end)).unwrap();
    assert_eq!(stopped.exchange.ordinal, 961);
    assert_eq!(
        stopped.payload,
        Payload::Refusal(LifecycleRefusal::OutOfOrder)
    );
    drop(silent);
    let new = (!mode.ends_with("-same")).then(|| dial(&root.join("admin.sock")));
    let rollback = new.as_ref().or(original.as_ref()).unwrap();
    send(
        rollback,
        962,
        LifecycleDirective::Leave {
            cause: Cause { uid: 0 },
            forced: true,
            rollback: true,
        },
    );
    if let Some(original) = &original {
        let refused: OrganEnvelope = serde_json::from_slice(&receive(original, end)).unwrap();
        assert_eq!(refused.exchange, source.exchange);
        assert_eq!(refused.position, Position::Close);
        assert_eq!(
            refused.payload,
            Payload::Refusal(LifecycleRefusal::NoResidency)
        );
    }
    let left: OrganEnvelope = serde_json::from_slice(&receive(rollback, end)).unwrap();
    assert_eq!(left.exchange.ordinal, 962);
    assert_eq!(left.position, Position::Close);
    assert!(matches!(
        left.payload,
        Payload::Answer(LifecycleAnswer::Left {
            forced: true,
            no_state: true,
            save_point: None,
            ..
        })
    ));
    let (status, expired) = crate::test_child::wait_until(child, end).unwrap();
    assert!(
        !expired && status.success(),
        "setup rollback did not finish"
    );
    assert!(!root.join("diagnostic-spawned").exists());
    assert!(!root.join("diagnostic-refeed").exists());
    assert!(!root.join("diagnostic-forbidden-organ").exists());
    closed_empty(reader, end);
    closed_empty(member, end);
}
