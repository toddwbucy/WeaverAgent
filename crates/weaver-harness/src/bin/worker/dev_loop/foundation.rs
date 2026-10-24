//! Replacement startup at the developer loop boundary. The active worker does
//! not select this path until serving, teardown and qualification are joined.
use weaver_harness::{
    FoundationEnter, FoundationEnterProgress, FoundationInput, FoundationInputProgress,
    FoundationInputResult, FoundationProgram, FoundationRun, FoundationServing,
};
use weaver_types::TurnKey;

#[path = "foundation_turn.rs"]
pub mod turn;

/// Workflow choices are separate from the harness's protocol transitions. The
/// caller chooses when to start a task, select its source and use its placement.
/// Management and refusal return the same owner for the loop's next decision.
pub fn task<'a>(mut input: Box<FoundationInput<'a>>, turn: TurnKey) -> Box<FoundationInput<'a>> {
    if input.start_task(turn).is_ok() {
        drive(&mut input);
    }
    input
}
pub fn place_task<'a>(
    mut input: Box<FoundationInput<'a>>,
    label: String,
    method: weaver_types::state_flow::CutMethod,
) -> Box<FoundationInput<'a>> {
    if input.place_task(label, method).is_ok() {
        drive(&mut input);
    }
    input
}
pub fn generate(mut input: Box<FoundationInput<'_>>) -> Box<FoundationInput<'_>> {
    if input.generate().is_ok() {
        drive(&mut input);
    }
    input
}
pub fn place_results(
    mut input: Box<FoundationInput<'_>>,
    calls: Vec<weaver_types::state_flow::CallId>,
    label: String,
    method: weaver_types::state_flow::CutMethod,
) -> Box<FoundationInput<'_>> {
    if input.place_results(calls, label, method).is_ok() {
        drive(&mut input);
    }
    input
}
pub fn repair<'a>(
    mut input: Box<FoundationInput<'a>>,
    family: &'static weaver_harness::FoundationRestatement,
) -> Box<FoundationInput<'a>> {
    if input.repair(family).is_ok() {
        drive(&mut input);
    }
    input
}
pub fn outbound<'a>(
    mut input: Box<FoundationInput<'a>>,
    acts: Vec<weaver_types::state_flow::SortAct>,
) -> Box<FoundationInput<'a>> {
    if input.sort_outbound(acts).is_ok() {
        drive(&mut input);
    }
    input
}
pub fn place_restatement<'a>(
    mut input: Box<FoundationInput<'a>>,
    label: String,
    method: weaver_types::state_flow::CutMethod,
) -> Box<FoundationInput<'a>> {
    if input.place_restatement(label, method).is_ok() {
        drive(&mut input);
        if input.close_correction_refusal().is_ok() {
            drive(&mut input);
        }
    }
    input
}
pub fn generate_correction(mut input: Box<FoundationInput<'_>>) -> Box<FoundationInput<'_>> {
    if input.generate_correction().is_ok() {
        drive(&mut input);
    }
    input
}
pub fn pairs(mut input: Box<FoundationInput<'_>>) -> Box<FoundationInput<'_>> {
    if input.begin_pairs().is_ok() {
        drive(&mut input);
    }
    input
}
pub fn recall(
    mut input: Box<FoundationInput<'_>>,
    call: weaver_types::state_flow::CallId,
) -> Box<FoundationInput<'_>> {
    if input.recall(call).is_ok() {
        drive(&mut input);
    }
    input
}
fn drive(input: &mut FoundationInput<'_>) {
    while input.progress() == FoundationInputProgress::Pending {
        input.advance();
    }
}

pub fn execute(
    mut input: Box<FoundationInput<'_>>,
    call: weaver_types::state_flow::CallId,
) -> Box<FoundationInput<'_>> {
    if input.execute(call).is_ok() {
        drive(&mut input);
    }
    input
}

/// Service the actual held root connection, then resume the same
/// input. Without a pending connection it returns the input unchanged. Other
/// lifecycle directives retain their complete management owner.
pub fn management(
    input: Box<FoundationInput<'_>>,
) -> Result<Box<FoundationInput<'_>>, Box<weaver_harness::FoundationManagement<'_>>> {
    let mut owner = match input.management() {
        Ok(owner) => owner,
        Err(input) => return Ok(input),
    };
    while owner.progress() == weaver_harness::FoundationManagementProgress::Pending {
        owner.advance();
    }
    owner.finish()
}

/// Drive a single gate request through the actual serving ports. The loop owns
/// the proposed turn identity; it is used only after recorded input acceptance.
/// A pending management or failed arm returns all custody for its next owner.
pub fn input(
    serving: FoundationServing<'_>,
    turn: TurnKey,
) -> Result<FoundationInputResult<'_>, Box<FoundationInput<'_>>> {
    let mut owner = serving.input();
    while owner.progress() == FoundationInputProgress::Pending {
        owner.advance();
    }
    if owner.progress() == FoundationInputProgress::Accepted {
        owner.start_turn(turn).expect("checked acceptance");
        while owner.progress() == FoundationInputProgress::Pending {
            owner.advance();
        }
    }
    owner.finish()
}

/// Begin with the run that actually adopted root's Enter. The worker retains
/// that outer run for the complete lifetime of the returned startup/serving seat.
///
/// # Safety
/// The current serving thread must retain and reap all actual children, including
/// any stopped owner, or exit the whole worker as failed.
pub unsafe fn enter(
    run: &mut FoundationRun,
    program: FoundationProgram,
) -> Result<Result<FoundationServing<'_>, FoundationEnter<'_>>, String> {
    let mut owner = run.begin(program)?;
    while owner.progress() == FoundationEnterProgress::Pending {
        // SAFETY: the caller owns the serving thread and every returned arm.
        unsafe { owner.advance() };
    }
    // A stopped stage returns the exact owner. It is not a retry, a successful
    // Enter or a completed failed-run cleanup.
    Ok(owner.finish())
}
