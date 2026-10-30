//! Ready selection precedes delivery, including the entered release rules.
use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn control(
    root: &std::path::Path,
    mode: &str,
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
    // Original has consumed no reply. Ready selection must nevertheless
    // start the entered owner and its parked replay ask (#412 R1).
    reader.set_nonblocking(false).unwrap();
    reader
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut input = BufReader::new(&mut *reader);
    let mut line = String::new();
    input.read_line(&mut line).unwrap();
    let opened: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!(opened["kind"], "replay.opened");
    assert!(input.buffer().is_empty());
    drop(input);
    member.set_nonblocking(false).unwrap();
    member
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut input = BufReader::new(&mut *member);
    line.clear();
    input.read_line(&mut line).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&line).unwrap(),
        serde_json::json!({"ask":{"replay":{}}})
    );
    assert!(input.buffer().is_empty());
    drop(input);
    // The at-rest rollback stops the parked ask and keeps entered cleanup.
    // Neither the unopened preload nor this bystander may hold it hostage.
    assert!(!root.join("diagnostic-refeed").exists());
    marker(root, "diagnostic-setup-settle");
    let leave_began = Instant::now();
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
    for marker_name in [
        "diagnostic-setup-spu-pid",
        "diagnostic-setup-classifier-pid",
    ] {
        let pid = std::fs::read_to_string(root.join(marker_name)).unwrap();
        assert!(!std::path::Path::new(&format!("/proc/{}", pid.trim())).exists());
    }
    if mode.ends_with("-ready-bystander-expiry") {
        // Left is already delivered and both actual children are reaped.
        // Keep the original bystander unread until the original 150s cap.
        observed(&probe, 992, AgentState::Unloaded, end);
        assert!(child.try_wait().unwrap().is_none());
        let original_end = leave_began + Duration::from_millis(weaver_types::LEAVE_BOUND_MS);
        let (status, expired) =
            crate::test_child::wait_until(child, end.min(original_end + Duration::from_secs(5)))
                .unwrap();
        assert!(
            !expired && status.success(),
            "a completed Left must survive bystander delivery expiry"
        );
        assert!(
            Instant::now() >= original_end,
            "the original reply interval must not be shortened"
        );
    } else {
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
    }
    assert!(root.join("diagnostic-setup-release").exists());
    assert!(!root.join("diagnostic-refeed").exists());
    if mode.ends_with("-ready-release-eof") {
        let mut tail = String::new();
        reader.read_to_string(&mut tail).unwrap();
        let rows: Vec<serde_json::Value> = tail
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["kind"], "fault");
        assert_eq!(rows[0]["payload"]["case"], "organ_death_observed");
        assert_eq!(rows[0]["payload"]["account"]["organ"], "spu");
        assert!(rows[0].get("turn").is_none());
    } else {
        closed_empty(reader, end);
    }
    closed_empty(member, end);
}
