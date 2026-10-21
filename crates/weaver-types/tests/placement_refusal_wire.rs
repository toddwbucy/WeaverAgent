//! TYPES-DECODE's exact placement refusal vocabulary. A refusal attributes an
//! assembly and never reuses the decoder's own refusal vocabulary.

use weaver_types::RefusalRecord;
use weaver_types::state_flow::{DecimalU64, PlacementCode};

#[test]
fn placement_refusals_preserve_exact_assembly_and_closed_code() {
    for (code, spelling) in [
        (PlacementCode::InvalidSpan, "invalid_span"),
        (PlacementCode::InvalidPair, "invalid_pair"),
        (PlacementCode::InvalidCut, "invalid_cut"),
        (PlacementCode::HeadOverCapacity, "head_over_capacity"),
        (PlacementCode::TailOverCapacity, "tail_over_capacity"),
        (PlacementCode::GenerationRoom, "generation_room"),
        (PlacementCode::StalePosition, "stale_position"),
    ] {
        let refusal = RefusalRecord::Placement {
            assembly_id: DecimalU64(u64::MAX),
            code,
            detail: "exact reason".into(),
        };
        let bytes = serde_json::to_string(&refusal).unwrap();
        assert_eq!(
            bytes,
            format!(
                r#"{{"seam":"placement","assembly_id":"18446744073709551615","code":"{spelling}","detail":"exact reason"}}"#
            )
        );
        assert_eq!(
            serde_json::from_str::<RefusalRecord>(&bytes).unwrap(),
            refusal
        );
    }
}

#[test]
fn placement_refusals_require_all_members_and_refuse_other_vocabularies() {
    let canonical = serde_json::json!({
        "seam": "placement",
        "assembly_id": "0",
        "code": "invalid_span",
        "detail": "malformed family render"
    });
    assert!(serde_json::from_value::<RefusalRecord>(canonical.clone()).is_ok());
    for key in ["seam", "assembly_id", "code", "detail"] {
        let mut missing = canonical.clone();
        missing.as_object_mut().unwrap().remove(key);
        assert!(serde_json::from_value::<RefusalRecord>(missing).is_err());
        let mut null = canonical.clone();
        null[key] = serde_json::Value::Null;
        assert!(serde_json::from_value::<RefusalRecord>(null).is_err());
    }
    for (key, value) in [
        ("assembly_id", serde_json::json!(0)),
        ("assembly_id", serde_json::json!("01")),
        ("assembly_id", serde_json::json!("18446744073709551616")),
        ("code", serde_json::json!("malformed_delta")),
        ("code", serde_json::json!("component_failure")),
        ("seam", serde_json::json!("decode")),
        (
            "asked",
            serde_json::json!({"ask": "measure_delta", "keep": 1}),
        ),
    ] {
        let mut wrong = canonical.clone();
        wrong[key] = value;
        assert!(serde_json::from_value::<RefusalRecord>(wrong).is_err());
    }
}
