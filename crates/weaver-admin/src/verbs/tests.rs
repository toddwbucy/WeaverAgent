use super::*;

#[test]
fn the_stop_answer_is_relayed_unchanged() {
    let aborted = LifecycleAnswer::TurnAborted {
        turn: weaver_types::TurnKey("t-1".into()),
    };
    assert_eq!(relay_stop_answer(aborted.clone()), aborted);
    assert_eq!(
        relay_stop_answer(LifecycleAnswer::AtRest),
        LifecycleAnswer::AtRest
    );
}
