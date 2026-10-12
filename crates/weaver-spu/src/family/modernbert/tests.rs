use super::*;

/// The selection is the operation filter: the classify table answers for
/// its architecture and for no decode architecture, per the Spec's rule
/// that neither operation's candidates shadow the other's.
#[test]
fn the_classify_selection_reads_the_operation_table() {
    let selected = select_classify(&[ARCHITECTURE.to_string()]).expect("selects");
    assert_eq!(selected.family, "modernbert");
    assert!(select_classify(&["Qwen2ForCausalLM".to_string()]).is_none());
    assert!(select_classify(&[]).is_none());
}
