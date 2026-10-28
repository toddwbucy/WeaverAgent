//! Replacement startup at the developer loop boundary. The active worker does
//! not select this path until serving, teardown and qualification are joined.
use weaver_harness::{
    FoundationEnter, FoundationEnterProgress, FoundationInput, FoundationInputProgress,
    FoundationInputResult, FoundationProgram, FoundationRun, FoundationServing,
};
use weaver_types::TurnKey;

#[path = "foundation_turn.rs"]
pub mod turn;

#[path = "foundation_wind_down.rs"]
mod wind_down;

#[path = "foundation_session.rs"]
pub mod session;

#[path = "foundation_dispatch.rs"]
pub mod dispatch;

pub type FailedOwner<'a> =
    Result<Box<FoundationInput<'a>>, Box<weaver_harness::FoundationManagement<'a>>>;

/// Consume the successful Ready seat's checked rollback handoff into its actual
/// run. Only this worker loop drives the retained cleanup owner to closure.
#[allow(clippy::result_large_err)]
pub fn entered_rollback(
    run: Box<FoundationRun>,
    handoff: Box<weaver_harness::FoundationEnteredRollbackHandoff>,
) -> Result<
    Box<weaver_harness::FoundationEnteredRollback>,
    (
        Box<FoundationRun>,
        Box<weaver_harness::FoundationEnteredRollbackHandoff>,
    ),
> {
    let mut owner = run.entered_rollback(handoff)?;
    loop {
        match owner.advance() {
            weaver_harness::FoundationEnteredRollbackProgress::Progress => (),
            weaver_harness::FoundationEnteredRollbackProgress::Pending => {
                owner.wait_for_progress();
            }
            weaver_harness::FoundationEnteredRollbackProgress::Closed => return Ok(owner),
        }
    }
}

/// Drive the actual forced Leave through capture success or cleaned refusal
/// and its final lifecycle outcome. Foundation selection remains held.
#[allow(clippy::result_large_err)]
pub fn forced_leave(
    run: Box<FoundationRun>,
    handoff: Box<weaver_harness::FoundationForcedLeaveHandoff>,
) -> Result<
    Box<weaver_harness::FoundationForcedLeave>,
    (
        Box<FoundationRun>,
        Box<weaver_harness::FoundationForcedLeaveHandoff>,
    ),
> {
    let mut owner = run.forced_leave(handoff)?;
    loop {
        match owner.advance() {
            weaver_harness::FoundationForcedLeaveProgress::Progress => (),
            weaver_harness::FoundationForcedLeaveProgress::Pending => {
                owner.wait_for_progress();
            }
            weaver_harness::FoundationForcedLeaveProgress::Closed => return Ok(owner),
        }
    }
}

/// Drive the original graceful Leave after its settled wind-down, keeping
/// the entered run at S9 after a refused capture. Foundation selection stays held.
#[allow(clippy::result_large_err)]
pub fn graceful_leave(
    run: Box<FoundationRun>,
    handoff: Box<weaver_harness::FoundationGracefulLeaveHandoff>,
) -> Result<
    Box<weaver_harness::FoundationGracefulLeave>,
    (
        Box<FoundationRun>,
        Box<weaver_harness::FoundationGracefulLeaveHandoff>,
    ),
> {
    let mut owner = run.graceful_leave(handoff)?;
    loop {
        match owner.advance() {
            weaver_harness::FoundationGracefulLeaveProgress::Progress => (),
            weaver_harness::FoundationGracefulLeaveProgress::Pending
            | weaver_harness::FoundationGracefulLeaveProgress::CaptureRefused => {
                owner.wait_for_progress();
            }
            weaver_harness::FoundationGracefulLeaveProgress::Closed => return Ok(owner),
        }
    }
}

/// Finish stopped rollback under its original accepted Leave/halt deadline.
/// The returned Closed evidence grants only failed worker exit, never Left.
#[allow(clippy::result_large_err)]
pub fn failed_rollback_exit(
    run: Box<FoundationRun>,
    handoff: Box<weaver_harness::FoundationRollbackHandoff>,
) -> Result<
    Box<weaver_harness::FoundationRollbackExit>,
    (
        Box<FoundationRun>,
        Box<weaver_harness::FoundationRollbackHandoff>,
    ),
> {
    let mut owner = run.failed_rollback_exit(handoff)?;
    loop {
        match owner.advance() {
            weaver_harness::FoundationRollbackExitProgress::Progress => {}
            weaver_harness::FoundationRollbackExitProgress::Pending => {
                owner.wait_for_progress();
            }
            weaver_harness::FoundationRollbackExitProgress::Closed => return Ok(owner),
        }
    }
}

/// Finish the original latched startup failure. A refused join retains both
/// inputs; Closed grants only failed worker exit, never lifecycle Left.
#[allow(clippy::result_large_err)]
pub fn failed_startup_exit(
    run: Box<FoundationRun>,
    handoff: Box<weaver_harness::FoundationStartupHandoff>,
) -> Result<
    Box<weaver_harness::FoundationStartupExit>,
    (
        Box<FoundationRun>,
        Box<weaver_harness::FoundationStartupHandoff>,
    ),
> {
    let mut owner = run.failed_startup_exit(handoff)?;
    loop {
        match owner.advance() {
            weaver_harness::FoundationStartupExitProgress::Progress => {}
            weaver_harness::FoundationStartupExitProgress::Pending => {
                owner.wait_for_progress();
            }
            weaver_harness::FoundationStartupExitProgress::Closed => return Ok(owner),
        }
    }
}

/// Continue the original failed run from its consumed serving seat. Only this
/// loop waits; the harness transitions keep every actual resource and receipt.
#[allow(clippy::result_large_err)]
pub fn failed_exit(
    run: Box<FoundationRun>,
    handoff: Box<weaver_harness::FoundationFailedHandoff>,
) -> Result<
    Box<weaver_harness::FoundationFailedExit>,
    (
        Box<FoundationRun>,
        Box<weaver_harness::FoundationFailedHandoff>,
    ),
> {
    let mut owner = run.failed_exit(handoff)?;
    loop {
        match owner.advance() {
            weaver_harness::FoundationExitProgress::Progress => {}
            weaver_harness::FoundationExitProgress::Pending => {
                owner.wait_for_progress();
            }
            weaver_harness::FoundationExitProgress::Closed => return Ok(owner),
        }
    }
}

/// Drive the retained failed owner only through report and gate interruption.
/// The result still owes closing, actual release and final failed drain.
pub fn halt(
    owner: FailedOwner<'_>,
) -> Result<Box<weaver_harness::FoundationHalt<'_>>, FailedOwner<'_>> {
    let mut halt = match owner {
        Ok(input) => input.halt().map_err(Ok)?,
        Err(management) => management.halt().map_err(Err)?,
    };
    loop {
        match halt.advance() {
            weaver_harness::FoundationHaltProgress::Progress => {}
            weaver_harness::FoundationHaltProgress::Pending => {
                halt.wait_for_progress();
            }
            _ => return Ok(halt),
        }
    }
}

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

/// Complete the explicit rollback of a recorded classifier refusal. Every
/// stopped chain is returned whole for the failed-startup owner; it grants no
/// successful Enter, retry, or Left. Awaiting root uses the same owned listener.
pub fn rollback(
    mut owner: Box<weaver_harness::FoundationRollback<'_>>,
) -> Box<weaver_harness::FoundationRollback<'_>> {
    use weaver_harness::FoundationRollbackProgress as Progress;
    while matches!(
        owner.progress(),
        Progress::Pending | Progress::AwaitRollback
    ) {
        owner.advance();
    }
    owner
}
