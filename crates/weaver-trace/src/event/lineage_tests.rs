//! Test-only: the lineage suite of `event.rs`, moved out of the production file by #118.

use super::*;

fn elections(lineage: Option<Box<Lineage>>) -> Elections {
    let mut stack = std::collections::BTreeMap::new();
    stack.insert("weaver-harness".to_string(), "ab12".to_string());
    Elections {
        residual_readout: false,
        field: None,
        surprisal: false,
        state_member: true,
        state_store: StoreIdentity {
            engine: "sqlite".into(),
        },
        composer: LoopIdentity::compiled("test"),
        declaration: "d".into(),
        tee: None,
        lineage,
        reset: None,
        stack,
        boundary: String::new(),
        cause: Cause { uid: 0 },
        library_path: None,
    }
}

/// **The load names its lineage where it restored a save point, its reset
/// where the last run stopped unclean, and its stack on every load**, per
/// `weaver-trace-Spec` section 3 on the rulings of 2026-10-02 on #58: the
/// lineage and the reset each absent rather than null where the load
/// carries none, the lineage's `built_from` absent where no builder cut
/// it, and the stack keyed by name.
///
/// Perturbation: drop the `skip_serializing_if` on `lineage` or on
/// `reset` and the absence assertion fails on a null member. Watched
/// under each removal.
#[test]
fn the_load_names_its_lineage_its_reset_and_its_stack() {
    let rendered = serde_json::to_string(&elections(None)).expect("renders");
    assert!(
        !rendered.contains("lineage"),
        "absent, never null: {rendered}"
    );
    assert!(
        !rendered.contains("reset"),
        "absent, never null: {rendered}"
    );
    assert!(rendered.contains("\"stack\":{\"weaver-harness\":\"ab12\"}"));
    let rendered = serde_json::to_string(&elections(Some(Box::new(Lineage {
        save_point: "ab12".into(),
        run: "r-a".into(),
        sequence: 41,
        turn: 2,
        named_at_restore: false,
        built_from: None,
    }))))
    .expect("renders");
    assert!(
        rendered.contains(concat!(
            "\"lineage\":{\"save_point\":\"ab12\",\"run\":\"r-a\",",
            "\"sequence\":41,\"turn\":2,\"named_at_restore\":false}"
        )),
        "{rendered}"
    );
    let mut reset = elections(None);
    reset.reset = Some(Reset {
        prior_run: "r-z".into(),
        reason: "no-clean-unload".into(),
    });
    let rendered = serde_json::to_string(&reset).expect("renders");
    assert!(
        rendered.contains("\"reset\":{\"prior_run\":\"r-z\",\"reason\":\"no-clean-unload\"}"),
        "{rendered}"
    );
}
