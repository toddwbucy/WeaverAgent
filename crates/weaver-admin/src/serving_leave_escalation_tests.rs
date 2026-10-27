//! Escalation and failure are different conclusions of the same actual Leave.
use super::*;
use std::os::unix::process::ExitStatusExt;

pub(super) fn applies(case: &str) -> bool {
    case.starts_with("initial-") || case.starts_with("capture-") || case == "late-silent"
}

pub(super) fn check(
    case: &str,
    config: &ServiceConfig,
    result: Result<Concluded, Failure>,
    initial: Option<save_points::Marker>,
    before: (&recovery::Files, &recovery::Files),
    worker: &mut std::process::Child,
    log: &str,
) {
    let (room, saves) = before;
    assert!(!log.contains("Traceback"), "{case}: {log}");
    let escalated = matches!(
        case,
        "initial-silent"
            | "initial-out-of-order"
            | "initial-closed-marker"
            | "initial-absent-marker"
    );
    let evidence = if escalated {
        let concluded = result.as_ref().unwrap_or_else(|e| panic!("{case}: {e:?}"));
        assert_eq!(concluded.answer, crate::unloaded_answer());
        concluded.evidence.as_deref().unwrap()
    } else {
        let failure = result.as_ref().unwrap_err();
        if case == "initial-foreign-marker" {
            assert_eq!(
                failure.cleanup.as_deref(),
                Some(&LifecycleRefusal::AgentRunning)
            );
        } else if case == "initial-bad-record" {
            assert_eq!(
                failure.cleanup.as_deref(),
                Some(&LifecycleRefusal::BoundaryUnverified)
            );
        } else {
            assert!(failure.cleanup.is_none(), "{case}: {failure:?}");
        }
        if case == "late-silent" {
            assert_eq!(
                failure.refusal(),
                weaver_types::state_flow::capture::CaptureRefusal::Lifecycle(
                    LifecycleRefusal::Unanswered
                ),
                "silence does not infer worker failure: {failure:?}"
            );
        }
        failure.evidence.as_deref().unwrap()
    };
    assert!(evidence.report.is_none() && evidence.completed.is_none());
    assert_eq!(evidence.force_trigger.is_some(), escalated);
    if case == "initial-silent" {
        let began: u64 = fs::read_to_string(config.root.join("leave-began"))
            .unwrap()
            .parse()
            .unwrap();
        let ended: u64 = fs::read_to_string(config.root.join("term-seen"))
            .unwrap()
            .parse()
            .unwrap();
        assert!(
            (400_000_000..3_000_000_000).contains(&(ended - began)),
            "escalation ignored the original Leave bound"
        );
    }
    assert_eq!(recovery::files(&config.territory.join("state")), *room);
    if case == "late-silent" {
        assert!(evidence.committed);
        let failed = evidence.publication_failure.as_ref().unwrap();
        assert!(failed.published.is_some());
        assert!(
            fs::read_dir(config.territory.join("save-points"))
                .unwrap()
                .any(|p| p
                    .unwrap()
                    .file_name()
                    .to_str()
                    .unwrap()
                    .ends_with(".pending"))
        );
    } else {
        assert!(!evidence.committed && evidence.publication_failure.is_none());
        assert_eq!(
            recovery::files(&config.territory.join("save-points")),
            *saves
        );
    }
    if case == "initial-foreign-marker" {
        assert!(start::run_lock_held(&config.run_directory()).unwrap());
        assert!(worker.try_wait().unwrap().is_none());
        assert!(!config.root.join("term-seen").exists());
    } else {
        assert!(!start::run_lock_held(&config.run_directory()).unwrap());
        let status = worker.wait().unwrap();
        if case == "initial-eof" {
            assert!(status.success());
        } else {
            assert_eq!(
                status.signal(),
                Some(nix::libc::SIGKILL),
                "{case}: {status:?}"
            );
        }
        assert_eq!(
            config
                .run_directory()
                .join(crate::containment::RECORD)
                .exists(),
            case == "initial-bad-record"
        );
    }
    let expected = if matches!(case, "initial-silent" | "initial-out-of-order") {
        Some(save_points::Marker::Forced { run: RUN.into() })
    } else {
        initial
    };
    assert_eq!(
        save_points::load_marker(&config.root).unwrap(),
        expected,
        "{case}"
    );
    if let Some(path) = std::env::var_os("WEAVER_LEAVE_ESCALATION_EVIDENCE") {
        let path = std::path::PathBuf::from(path).join(case);
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("result.json"), serde_json::to_vec_pretty(&json!({
            "case":case,"result":format!("{result:?}"),"marker":format!("{expected:?}"),
            "room_before":room,"room_after":recovery::files(&config.territory.join("state")),
            "saves_before":saves,"saves_after":recovery::files(&config.territory.join("save-points")),
            "worker_log":log
        })).unwrap()).unwrap();
    }
}

#[test]
#[ignore = "private root delegated scope; real invocation, worker lock and escalation"]
fn forced_leave_invocation_escalates_initial_silence_and_out_of_order() {
    for case in [
        "initial-silent",
        "initial-out-of-order",
        "initial-closed-marker",
        "initial-absent-marker",
    ] {
        root_case(case);
    }
}

#[test]
#[ignore = "private root delegated scope; real invocation, worker lock and escalation"]
fn forced_leave_invocation_preserves_failure_and_foreign_marker() {
    for case in [
        "initial-eof",
        "initial-refused",
        "initial-foreign-exchange",
        "initial-malformed",
        "initial-foreign-marker",
        "initial-bad-record",
    ] {
        root_case(case);
    }
}

#[test]
#[ignore = "private root delegated scope; real invocation, worker lock and escalation"]
fn forced_leave_invocation_keeps_capture_timeouts_out_of_initial_escalation() {
    for case in ["capture-silent", "capture-out-of-order", "late-silent"] {
        root_case(case);
    }
}
