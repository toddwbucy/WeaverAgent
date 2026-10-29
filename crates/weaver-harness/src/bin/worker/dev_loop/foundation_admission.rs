//! Before-Enter service joined to the dormant adopted-run dispatcher.
//! Both bindings are routed here. Production selection remains held for cutover.
use super::dispatch;
use std::io::Write;
use weaver_harness::{FoundationAdmission, FoundationAdmissionProgress, FoundationProgram};
use weaver_types::{BindingKind, TurnKey};

/// Drive the original listener through before-load replies and Serving admission.
/// A Serving run moves directly into the dispatcher and retains process custody
/// through final exit. A Diagnostic Enter returns its original admission owner,
/// without adoption, reply, descriptor replacement or a Serving context.
///
/// The two-binding entry below supplies the Diagnostic routing. Both entries
/// remain dormant until the remaining integration and qualification work clears.
///
/// # Safety
/// Run only on the worker's serving thread after the first-act inherited descriptor
/// hygiene. Serving and terminal admission failure exit this whole process. It
/// must contain no unrelated work or other agent. The policy callback must return
/// promptly without unwinding, as required by `dispatch::drive`.
pub unsafe fn drive_serving<F>(
    mut admission: FoundationAdmission,
    program: FoundationProgram,
    policy: dispatch::Policy<F>,
) -> FoundationAdmission
where
    F: FnMut(dispatch::TurnPurpose) -> TurnKey,
{
    loop {
        if await_enter(&mut admission) == BindingKind::Diagnostic {
            return admission;
        }
        admission = match admission.adopt() {
            // SAFETY: inherited from this entry's serving-thread contract.
            Ok(run) => unsafe { dispatch::drive(Box::new(run), program, policy) },
            Err(original) => original,
        };
    }
}

/// Route both canonical bindings while retaining the original admission owner
/// through refusals. Diagnostic uses its existing recorder/replay lifecycle.
/// This entry remains unselected in production pending the qualified cutover.
///
/// # Safety
/// The caller grants this worker process and serving thread through final exit,
/// with first-act descriptor hygiene complete and no unrelated work. The policy
/// callback must return promptly without unwinding, as in `dispatch::drive`.
pub unsafe fn drive<F>(
    mut admission: FoundationAdmission,
    program: FoundationProgram,
    policy: dispatch::Policy<F>,
) -> !
where
    F: FnMut(dispatch::TurnPurpose) -> TurnKey,
{
    loop {
        if await_enter(&mut admission) == BindingKind::Diagnostic {
            match admission.serve_diagnostic(&program) {
                weaver_harness::FoundationDiagnosticResult::Admission(original) => {
                    admission = *original;
                    continue;
                }
                weaver_harness::FoundationDiagnosticResult::Finished(result) => {
                    std::process::exit(if result.is_ok() { 0 } else { 1 });
                }
            }
        }
        admission = match admission.adopt() {
            // SAFETY: inherited from this entry's serving-thread contract.
            Ok(run) => unsafe { dispatch::drive(Box::new(run), program, policy) },
            Err(original) => original,
        };
    }
}

fn await_enter(admission: &mut FoundationAdmission) -> BindingKind {
    loop {
        let progress = admission.advance();
        if let Some(detail) = admission.take_delivery_loss() {
            // Diagnostic output cannot unwind the retained lifecycle owner.
            let _ = writeln!(std::io::stderr().lock(), "worker: {detail}");
        }
        match progress {
            FoundationAdmissionProgress::Pending => {
                admission.wait_for_progress();
            }
            FoundationAdmissionProgress::Received => return admission.binding_kind().unwrap(),
            // Retain the borrowed owner and all custody until process exit.
            FoundationAdmissionProgress::Stopped => std::process::exit(1),
        }
    }
}
