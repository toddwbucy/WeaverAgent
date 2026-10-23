//! Replacement startup at the developer loop boundary. The active worker does
//! not select this path until serving, teardown and qualification are joined.
use weaver_harness::{
    FoundationEnter, FoundationEnterProgress, FoundationInput, FoundationInputProgress,
    FoundationInputResult, FoundationProgram, FoundationRun, FoundationServing,
};
use weaver_types::TurnKey;

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
