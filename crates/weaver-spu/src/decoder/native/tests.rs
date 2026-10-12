use super::*;

/// **A family this path does not serve refuses as a family**, naming the
/// spelling the artifact used, and the shapes are never reached.
///
/// The config is a stock Qwen3 export's two deciding fields: the family it
/// declares, and the `sliding_window` null that qwen2's struct refuses. So
/// the order is what this watch reads and not the judgment alone.
///
/// Perturbation: judge after the parse instead and this fails, the refusal
/// arriving as `LoadFailed` naming a config line that is correct, which is
/// the state issue #507 met.
///
/// conforms: spu-native-refuses-an-unserved-family-as-a-family
#[test]
fn an_unserved_family_refuses_as_a_family_before_the_shapes() {
    let dir = fixture(
        "unserved-family",
        br#"{"model_type": "qwen3", "sliding_window": null}"#,
    );
    let refusal = read_config(&dir.join("config.json"));
    // **Cleaned before the assert**, so the run this test is meant to be
    // put through red leaves nothing behind each time it is.
    let _ = std::fs::remove_dir_all(&dir);

    assert_eq!(
        refusal.expect_err("an unserved family refuses"),
        AdmitRefusal::Family(FamilyRefusal::BackendDoesNotServe {
            family: FamilyName("qwen3".into()),
            backend: NATIVE_BACKEND,
        }),
        "the refusal names the backend and the file's own spelling, and never \
         claims the registry lacks a family it carries"
    );
}

/// **A served family whose shapes are wrong keeps its line and column**,
/// which is the half of the message #507's reader needed and did not get
/// pointed at.
///
/// Perturbation: parse the shapes from the value rather than the text and
/// this fails, the position going with it.
///
/// conforms: spu-native-refuses-an-unserved-family-as-a-family
#[test]
fn a_served_familys_shape_error_names_where_it_is() {
    let dir = fixture(
        "served-shape",
        br#"{"model_type": "qwen2", "hidden_size": "wide"}"#,
    );
    let refusal = read_config(&dir.join("config.json"));
    let _ = std::fs::remove_dir_all(&dir);

    let AdmitRefusal::LoadFailed { detail } =
        refusal.expect_err("a bad shape under a served family refuses as a load")
    else {
        panic!("a served family's shape error is a load failure");
    };
    assert!(
        detail.contains("line") && detail.contains("column"),
        "the shape error names where it is: {detail}"
    );
}

/// A fixture directory of this module's own, named for its test so two
/// of them in one binary cannot collide.
fn fixture(name: &str, config: &[u8]) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("weaver-spu-family-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("the fixture directory stands");
    std::fs::write(dir.join("config.json"), config).expect("the fixture config is written");
    dir
}

/// The served family passes, folded rather than matched byte for byte,
/// and a config naming no family is left to the shapes.
///
/// conforms: spu-native-refuses-an-unserved-family-as-a-family
#[test]
fn the_served_family_passes_and_an_unnamed_one_is_left_to_the_shapes() {
    for spelling in ["qwen2", "Qwen2", "qwen-2"] {
        let config = serde_json::json!({"model_type": spelling});
        assert_eq!(
            judge_family(&config),
            Ok(()),
            "{spelling} is the served key"
        );
    }
    let unnamed = serde_json::json!({"hidden_size": 1});
    assert_eq!(judge_family(&unnamed), Ok(()));
}
