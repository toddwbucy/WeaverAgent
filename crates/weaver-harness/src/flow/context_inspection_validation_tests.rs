//! Defensive inspection of copies, never a replacement Recorded completion.

use super::*;

#[derive(Clone, Copy, Debug)]
pub(in crate::flow::context) enum OwnerMismatch {
    RequestId,
    Turn,
}

/// Corrupt only the expected identity, leaving the member answer and its genuine
/// Recorded completion untouched. This pins each consumer's defensive boundary.
pub(in crate::flow::context) fn changed_owner(
    context: &mut RecordingContext,
    io: &mut impl PumpIo,
    mismatch: OwnerMismatch,
    settle: bool,
) -> Result<(), RunFailure> {
    if settle {
        context.retire_recorded_inspection_wait(io)?;
        assert!(context.inspection.as_ref().unwrap().recorded);
    }
    let owner = context.inspection.as_mut().unwrap();
    match mismatch {
        OwnerMismatch::RequestId => {
            owner.request_id = Some((owner.request_id.unwrap().0 + 1).into());
        }
        OwnerMismatch::Turn => owner.turn = Some(TurnRef("foreign-owner".into())),
    }
    if settle {
        context.settle_recorded_inspection(io)
    } else {
        context.retire_recorded_inspection_wait(io)
    }
}

pub(in crate::flow::context) fn associations(value: &OperationCompletion) {
    let OperationCompletion::State {
        request,
        turn,
        completion,
        ..
    } = value
    else {
        panic!("real state completion");
    };
    let id = completion.answer.common().request_id;
    assert_eq!(request, &StateControl::Shape {});
    assert!(inspection_identity_matches(
        request,
        turn,
        id,
        request,
        turn,
        Some(id)
    ));
    assert!(!inspection_identity_matches(
        request,
        turn,
        (id.0 + 1).into(),
        request,
        turn,
        Some(id)
    ));
    assert!(!inspection_identity_matches(
        request,
        &Some(TurnRef("foreign".into())),
        id,
        request,
        turn,
        Some(id)
    ));
    assert!(!inspection_identity_matches(
        &StateControl::Grants {},
        turn,
        id,
        request,
        turn,
        Some(id)
    ));
    assert!(inspection_answer_matches(
        InspectionKind::Shape,
        &completion.answer,
        false
    ));
    assert!(!inspection_answer_matches(
        InspectionKind::Shape,
        &completion.answer,
        true
    ));
    let mut changed = completion.answer.clone();
    let StateFrame::ControlAnswer { frame, .. } = &mut changed else {
        panic!("answer");
    };
    *frame = StateControlAnswer::Grants { surface: vec![] };
    assert!(!inspection_answer_matches(
        InspectionKind::Shape,
        &changed,
        false
    ));
    let StateFrame::ControlAnswer { frame, .. } = &mut changed else {
        panic!("answer");
    };
    *frame = StateControlAnswer::Released {
        snapshot_id: 1.into(),
    };
    assert!(!inspection_answer_matches(
        InspectionKind::Shape,
        &changed,
        false
    ));
}
