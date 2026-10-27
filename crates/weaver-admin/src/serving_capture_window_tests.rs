use super::*;
use std::time::Duration;
use weaver_types::state_flow::SaveWindow;

#[test]
fn demand_window_keeps_send_time_and_rejects_leave_completion() {
    let mut windows = CaptureWindows::demand_at(100, 50).unwrap();
    let original = windows.demand_window().unwrap();
    assert_eq!(
        original,
        SaveWindow {
            began_ns: 100.into(),
            deadline_ns: 150.into()
        }
    );
    assert_eq!(windows.remaining_at(125).unwrap(), Some(25));
    assert!(!windows.is_join() && !windows.matches_leave(false) && !windows.matches_leave(true));
    let value = CaptureBeginning {
        capture_id: 9.into(),
        window: original,
        tail_window: None,
    };
    windows.accept_at(&value, 130).unwrap();
    assert_eq!(windows.remaining_at(149).unwrap(), Some(1));
    assert!(windows.complete_at(9.into(), 149).is_err());
    assert_eq!(windows.remaining_at(150), Err(ChannelFault::Unanswered));
    assert_eq!(windows.demand_window(), Some(original));
    assert_eq!(windows.capture(), Some(&value));
}

#[test]
fn demand_beginning_cannot_replace_the_original_window_or_add_a_tail() {
    for case in ["shift", "short", "long", "tail", "zero", "expired"] {
        let mut windows = CaptureWindows::demand_at(100, 50).unwrap();
        let mut value = CaptureBeginning {
            capture_id: 9.into(),
            window: windows.demand_window().unwrap(),
            tail_window: None,
        };
        let mut now = 120;
        match case {
            "shift" => {
                value.window.began_ns.0 += 1;
                value.window.deadline_ns.0 += 1;
            }
            "short" => value.window.deadline_ns.0 -= 1,
            "long" => value.window.deadline_ns.0 += 1,
            "tail" => value.tail_window = Some(value.window),
            "zero" => value.capture_id = 0.into(),
            "expired" => now = 150,
            _ => unreachable!(),
        }
        assert!(windows.accept_at(&value, now).is_err(), "{case}");
        assert!(windows.capture().is_none(), "{case}");
        assert_eq!(windows.demand_window().unwrap().deadline_ns.0, 150);
    }
    assert!(CaptureWindows::demand_at(100, 0).is_err());
    assert!(CaptureWindows::demand_at(u64::MAX, 1).is_err());
}

fn beginning() -> CaptureBeginning {
    CaptureBeginning {
        capture_id: 7.into(),
        window: SaveWindow {
            began_ns: 110.into(),
            deadline_ns: 210.into(),
        },
        tail_window: Some(SaveWindow {
            began_ns: 100.into(),
            deadline_ns: (100 + LEAVE_NS).into(),
        }),
    }
}

#[test]
fn leave_capture_windows_keep_all_three_original_deadlines() {
    // Keep the outer window later, so dropping the shared tail cannot be
    // hidden by two identical deadlines after capture completion.
    let mut windows = CaptureWindows::at(90, 100 + LEAVE_NS * 2, 100).unwrap();
    let value = beginning();
    windows.accept_at(&value, 115).unwrap();
    assert_eq!(windows.remaining_at(120).unwrap(), Some(90));
    assert_eq!(windows.capture(), Some(&value));
    assert!(windows.accept_at(&value, 120).is_err());
    assert!(windows.complete_at(8.into(), 121).is_err());
    assert_eq!(windows.remaining_at(122).unwrap(), Some(88));
    windows.complete_at(7.into(), 123).unwrap();
    assert_eq!(
        windows.remaining_at(210).unwrap(),
        Some(100 + LEAVE_NS - 210)
    );
    assert_eq!(windows.capture(), Some(&value));
    assert_eq!(
        windows.remaining_at(100 + LEAVE_NS),
        Err(ChannelFault::Unanswered)
    );
    assert!(windows.complete_at(7.into(), 124).is_err());
}

#[test]
fn leave_capture_window_rejects_forged_anchors_budgets_and_late_completion() {
    let valid = beginning();
    for case in [
        "zero-id",
        "no-tail",
        "future",
        "short",
        "long",
        "tail-length",
        "tail-anchor",
        "expired",
        "overflow",
    ] {
        let mut value = valid.clone();
        let now = match case {
            "zero-id" => {
                value.capture_id = 0.into();
                115
            }
            "no-tail" => {
                value.tail_window = None;
                115
            }
            "future" => 109,
            "short" => {
                value.window.deadline_ns = 209.into();
                115
            }
            "long" => {
                value.window.deadline_ns = 211.into();
                115
            }
            "tail-length" => {
                value.tail_window.as_mut().unwrap().deadline_ns.0 -= 1;
                115
            }
            "tail-anchor" => {
                value.tail_window = Some(SaveWindow {
                    began_ns: 111.into(),
                    deadline_ns: (111 + LEAVE_NS).into(),
                });
                115
            }
            "expired" => 210,
            "overflow" => {
                value.window = SaveWindow {
                    began_ns: (u64::MAX - 20).into(),
                    deadline_ns: u64::MAX.into(),
                };
                u64::MAX - 10
            }
            _ => unreachable!(),
        };
        let mut windows = CaptureWindows::at(90, u64::MAX, 100).unwrap();
        assert!(windows.accept_at(&value, now).is_err(), "{case}");
        assert!(
            windows.capture().is_none(),
            "{case} installed unaccepted windows"
        );
    }
    let mut windows = CaptureWindows::at(90, 100 + LEAVE_NS, 100).unwrap();
    windows.accept_at(&valid, 115).unwrap();
    assert_eq!(
        windows.complete_at(7.into(), 210),
        Err(ChannelFault::Unanswered)
    );
    assert_eq!(windows.remaining_at(211), Err(ChannelFault::Unanswered));
}

#[test]
fn leave_capture_window_applies_the_earliest_tail_outer_or_capture_limit() {
    for (outer, save_wait, expected) in [
        (160, 100, 160),
        (u64::MAX, LEAVE_NS * 2, 100 + LEAVE_NS),
        (u64::MAX, 100, 210),
    ] {
        let mut windows = CaptureWindows::at(90, outer, save_wait).unwrap();
        let mut value = beginning();
        value.window.deadline_ns = (110 + save_wait).min(100 + LEAVE_NS).into();
        windows.accept_at(&value, 115).unwrap();
        assert_eq!(windows.remaining_at(116).unwrap(), Some(expected - 116));
        assert_eq!(
            windows.remaining_at(expected),
            Err(ChannelFault::Unanswered)
        );
    }
    assert!(CaptureWindows::at(100, 100, 10).is_err());
    assert!(CaptureWindows::at(100, 99, 10).is_err());
    assert!(CaptureWindows::at(100, 120, 0).is_err());
    let windows = CaptureWindows::at(100, 120, 10).unwrap();
    assert_eq!(windows.remaining_at(110), Ok(Some(10)));
    assert_eq!(windows.remaining_at(109), Err(ChannelFault::Closed));
}

#[test]
fn graceful_leave_uses_loaded_bounds_without_starting_capture_early() {
    for (drain, wind_down, limit) in [
        (None, None, None),
        (Some(2), None, None),
        (None, Some(3), None),
        (Some(2), Some(3), Some(90 + LEAVE_NS + 5_000_000_000)),
        (Some(u64::MAX), Some(3), None),
    ] {
        let mut windows =
            CaptureWindows::graceful_at(90, Duration::from_nanos(LEAVE_NS), drain, wind_down, 100)
                .unwrap();
        assert!(windows.matches_leave(false));
        assert!(!windows.matches_leave(true));
        assert!(!windows.is_join());
        assert_eq!(windows.remaining_at(500), Ok(limit.map(|end| end - 500)));
        assert!(windows.capture().is_none());
        let value = CaptureBeginning {
            capture_id: 7.into(),
            window: SaveWindow {
                began_ns: 1_000.into(),
                deadline_ns: 1_100.into(),
            },
            tail_window: Some(SaveWindow {
                began_ns: 990.into(),
                deadline_ns: (990 + LEAVE_NS).into(),
            }),
        };
        windows.accept_at(&value, 1_010).unwrap();
        assert_eq!(windows.remaining_at(1_020), Ok(Some(80)));
        windows.complete_at(7.into(), 1_030).unwrap();
        let end = limit.map_or(990 + LEAVE_NS, |outer| outer.min(990 + LEAVE_NS));
        assert_eq!(windows.remaining_at(1_200), Ok(Some(end - 1_200)));
        assert_eq!(windows.remaining_at(end), Err(ChannelFault::Unanswered));
    }
}

#[test]
fn graceful_leave_finite_deadline_is_original_and_unbounded_still_checks_the_clock() {
    let windows =
        CaptureWindows::graceful_at(90, Duration::from_nanos(10), Some(2), Some(3), 100).unwrap();
    assert_eq!(windows.remaining_at(5_000_000_099), Ok(Some(1)));
    assert_eq!(
        windows.remaining_at(5_000_000_100),
        Err(ChannelFault::Unanswered)
    );
    let windows =
        CaptureWindows::graceful_at(90, Duration::from_nanos(10), None, None, 100).unwrap();
    assert_eq!(windows.remaining_at(1_000), Ok(None));
    assert_eq!(windows.remaining_at(999), Err(ChannelFault::Closed));
    assert!(CaptureWindows::graceful_at(90, Duration::from_nanos(10), None, None, 0).is_err());
}
