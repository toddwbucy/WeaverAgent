//! Replacement startup at the developer loop boundary. The active worker does
//! not select this path until serving, teardown and qualification are joined.
use weaver_harness::{
    FoundationEnter, FoundationEnterProgress, FoundationProgram, FoundationRun, FoundationServing,
};

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
