//! Before-Enter service joined to the dormant adopted-run dispatcher.
//! Production selection and the separate Diagnostic driver remain caller duties.
use super::dispatch;
use std::io::Write;
use weaver_harness::{FoundationAdmission, FoundationAdmissionProgress, FoundationProgram};
use weaver_types::{BindingKind, TurnKey};

/// Drive the original listener through before-load replies and Serving admission.
/// A Serving run moves directly into the dispatcher and retains process custody
/// through final exit. A Diagnostic Enter returns its original admission owner,
/// without adoption, reply, descriptor replacement or a Serving context.
///
/// The composition root must route that returned owner to the separate Diagnostic
/// lifecycle before selecting this entry in production. This function stays
/// dormant while that routing and production qualification remain outstanding.
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
        let progress = admission.advance();
        if let Some(detail) = admission.take_delivery_loss() {
            // The lifecycle contract owes this diagnostic. Failed stderr must
            // not unwind the listener; its synchronous write has no separate
            // timeout and is not claimed as a nonblocking transport operation.
            let _ = writeln!(std::io::stderr().lock(), "worker: {detail}");
        }
        match progress {
            FoundationAdmissionProgress::Pending => {
                admission.wait_for_progress();
            }
            FoundationAdmissionProgress::Received => {
                if admission.binding_kind() == Some(BindingKind::Diagnostic) {
                    return admission;
                }
                admission = match admission.adopt() {
                    Ok(run) => {
                        // SAFETY: the caller grants this process and serving
                        // thread; adoption retains the exact original Enter.
                        unsafe { dispatch::drive(Box::new(run), program, policy) }
                    }
                    Err(original) => original,
                };
            }
            FoundationAdmissionProgress::Stopped => {
                // No admitted run exists here. Keep the source and any original
                // descriptor custody live until actual failed process exit. In
                // particular, do not write a potentially blocked stderr first.
                let _custody = admission;
                std::process::exit(1);
            }
        }
    }
}
