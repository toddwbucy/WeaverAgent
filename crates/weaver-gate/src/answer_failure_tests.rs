//! Exercise the served tool path's final reporter in a subprocess, including
//! its real standard error and process status. Socket producers live in tools.
use super::*;
use weaver_gate::tools::UndeliveredAnswer;

#[test]
fn bounded_nondelivery_reports_failure_and_orderly_close_reports_clean_exit() {
    const CHILD: &str = "WEAVER_GATE_DELIVERY_REPORT_CHILD";
    if let Some(mode) = std::env::var_os(CHILD) {
        let failure = match mode.to_str().unwrap() {
            "tool_answer" => Unanswered::DeliveryBound {
                exchange: ExchangeId {
                    opener: Opener::Harness,
                    ordinal: 7,
                },
                answer: UndeliveredAnswer::ToolAnswer,
            },
            "out_of_order" => Unanswered::DeliveryBound {
                exchange: ExchangeId {
                    opener: Opener::Harness,
                    ordinal: 9,
                },
                answer: UndeliveredAnswer::OutOfOrder,
            },
            "closed" => Unanswered::Channel(ChannelFault::Closed),
            _ => panic!("unknown test mode"),
        };
        // This is the same reporter whose status serve_channel_event returns.
        let status = unanswered_exit(failure);
        std::process::exit(if status == ExitCode::SUCCESS { 0 } else { 1 });
    }
    for (mode, ordinal) in [("tool_answer", 7), ("out_of_order", 9), ("closed", 0)] {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "answer_failure_tests::bounded_nondelivery_reports_failure_and_orderly_close_reports_clean_exit",
                "--nocapture",
            ])
            .env(CHILD, mode)
            .output().unwrap();
        if mode == "closed" {
            assert!(output.status.success(), "{output:?}");
            assert!(
                output.stderr.is_empty(),
                "ordinary peer teardown is not a failure"
            );
        } else {
            assert_eq!(output.status.code(), Some(1), "{output:?}");
            let lines = std::str::from_utf8(&output.stderr)
                .unwrap()
                .lines()
                .collect::<Vec<_>>();
            assert_eq!(lines.len(), 1, "one diagnostic accounts for non-delivery");
            let diagnostic: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
            assert_eq!(
                diagnostic,
                serde_json::json!({
                    "fault": "bounded_nondelivery", "at": "containment_deadline",
                    "answer": mode, "exchange": {"opener": "harness", "ordinal": ordinal},
                })
            );
        }
    }
}
