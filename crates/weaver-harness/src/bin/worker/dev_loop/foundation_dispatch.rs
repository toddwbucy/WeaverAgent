//! One adopted run owns startup, repeated turns and terminal process exit.
//! The active worker does not select this driver before the coupled cutover.
use super::{session, *};
use std::num::NonZeroUsize;
use weaver_harness::{
    FoundationExpiredRollbackProgress, FoundationProcessExit, FoundationRestatement,
    FoundationRollbackExitProgress, FoundationRollbackProgress,
};
use weaver_types::state_flow::CutMethod;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnPurpose {
    Input,
    WindDown,
}

/// Existing workflow elections are explicit caller inputs. The driver routes
/// lifecycle custody and never substitutes a prompt, turn or placement policy.
pub struct Policy<F> {
    pub family: &'static FoundationRestatement,
    pub method: CutMethod,
    pub wind_down_passes: NonZeroUsize,
    pub next_turn: F,
}

pub(super) enum ServingHandoff {
    Rollback(Box<weaver_harness::FoundationEnteredRollbackHandoff>),
    Forced(Box<weaver_harness::FoundationForcedLeaveHandoff>),
    Graceful(Box<weaver_harness::FoundationGracefulLeaveHandoff>),
    Failed(Box<weaver_harness::FoundationFailedHandoff>),
}

enum Handoff {
    Serving(ServingHandoff),
    Startup(Box<weaver_harness::FoundationStartupHandoff>),
    Rollback(Box<weaver_harness::FoundationRollbackHandoff>),
    Expired(Box<weaver_harness::FoundationExpiredEnter>),
}

/// Drive the exact adopted run until the worker process ends. All loans end
/// through their checked handoffs before that same run moves into cleanup.
/// Pre-Enter admission and the production selection remain separate duties.
///
/// # Safety
/// This must run on the worker's serving thread after descriptor admission.
/// It exits the whole process, including on an unrouteable ownership failure.
/// The caller must not host another agent or unrelated work in this process.
/// The policy callback must return promptly without unwinding while the driver
/// retains the current lifecycle owner.
pub unsafe fn drive<F>(
    mut run: Box<FoundationRun>,
    program: FoundationProgram,
    mut policy: Policy<F>,
) -> !
where
    F: FnMut(TurnPurpose) -> TurnKey,
{
    // SAFETY: checked outcomes retain the run and every failure owner through
    // process exit on this thread; the caller supplies the non-unwinding policy.
    let handoff = unsafe { start(&mut run, program, &mut policy) };
    match handoff {
        Handoff::Serving(ServingHandoff::Rollback(handoff)) => {
            let owner = match super::entered_rollback(run, handoff) {
                Ok(owner) => owner,
                Err(custody) => unrouteable(custody, "entered rollback join refused"),
            };
            finish(owner.process_exit(), owner)
        }
        Handoff::Serving(ServingHandoff::Forced(handoff)) => {
            let owner = match super::forced_leave(run, handoff) {
                Ok(owner) => owner,
                Err(custody) => unrouteable(custody, "forced Leave join refused"),
            };
            finish(owner.process_exit(), owner)
        }
        Handoff::Serving(ServingHandoff::Graceful(handoff)) => {
            // The existing owner remains in S9 after a refused capture. Its
            // wrapper waits for management and only returns after final cleanup.
            let owner = match super::graceful_leave(run, handoff) {
                Ok(owner) => owner,
                Err(custody) => unrouteable(custody, "graceful Leave join refused"),
            };
            finish(owner.process_exit(), owner)
        }
        Handoff::Serving(ServingHandoff::Failed(handoff)) => {
            let owner = match super::failed_exit(run, handoff) {
                Ok(owner) => owner,
                Err(custody) => unrouteable(custody, "failed serving join refused"),
            };
            finish(owner.process_exit(), owner)
        }
        Handoff::Startup(handoff) => {
            let owner = match super::failed_startup_exit(run, handoff) {
                Ok(owner) => owner,
                Err(custody) => unrouteable(custody, "failed startup join refused"),
            };
            finish(owner.process_exit(), owner)
        }
        Handoff::Rollback(handoff) => {
            let owner = match super::failed_rollback_exit(run, handoff) {
                Ok(owner) => owner,
                Err(custody) => unrouteable(custody, "failed rollback join refused"),
            };
            finish(owner.process_exit(), owner)
        }
        Handoff::Expired(handoff) => expired(run, handoff),
    }
}

unsafe fn start<F>(
    run: &mut FoundationRun,
    program: FoundationProgram,
    policy: &mut Policy<F>,
) -> Handoff
where
    F: FnMut(TurnPurpose) -> TurnKey,
{
    // SAFETY: inherited from drive, which owns this serving thread to exit.
    let entered = unsafe { super::enter(run, program) };
    let owner = match entered {
        Ok(Ok(serving)) => return Handoff::Serving(serve(serving, policy)),
        Ok(Err(owner)) => owner,
        Err(detail) => unrouteable(detail, "startup construction refused"),
    };
    // The first component failure wins, then the already observed Enter
    // expiry. Neither can be turned back into ordinary refusal authoring.
    let owner = match owner.failed_handoff() {
        Ok(handoff) => return Handoff::Startup(handoff),
        Err(owner) => owner,
    };
    let owner = match owner.expired_handoff() {
        Ok(handoff) => return Handoff::Expired(handoff),
        Err(owner) => owner,
    };
    match owner.rollback_refusal() {
        Ok(rollback) => refused_startup(rollback),
        Err(owner) => unrouteable(owner, "stopped startup has no admitted cleanup handoff"),
    }
}

fn refused_startup(owner: Box<weaver_harness::FoundationRollback<'_>>) -> Handoff {
    let owner = super::rollback(owner);
    match owner.progress() {
        FoundationRollbackProgress::Closed => finish(owner.process_exit(), owner),
        FoundationRollbackProgress::Stopped => match owner.failed_handoff() {
            Ok(handoff) => Handoff::Rollback(handoff),
            Err(owner) => unrouteable(owner, "stopped refusal rollback has no failed handoff"),
        },
        _ => unrouteable(owner, "refusal rollback returned before its terminal"),
    }
}

fn expired(run: Box<FoundationRun>, handoff: Box<weaver_harness::FoundationExpiredEnter>) -> ! {
    let mut owner = match run.expired_rollback(handoff) {
        Ok(owner) => owner,
        Err(custody) => unrouteable(custody, "expired startup join refused"),
    };
    loop {
        match owner.advance() {
            FoundationExpiredRollbackProgress::AwaitRollback
            | FoundationExpiredRollbackProgress::Accepted => {
                owner.wait_for_progress();
            }
            FoundationExpiredRollbackProgress::Closed => finish(owner.process_exit(), owner),
            FoundationExpiredRollbackProgress::Stopped => {
                let mut owner = match owner.failed_exit() {
                    Ok(owner) => owner,
                    Err(owner) => unrouteable(owner, "expired rollback failed-exit join refused"),
                };
                loop {
                    match owner.advance() {
                        FoundationRollbackExitProgress::Progress => (),
                        FoundationRollbackExitProgress::Pending => {
                            owner.wait_for_progress();
                        }
                        FoundationRollbackExitProgress::Closed => {
                            finish(owner.process_exit(), owner)
                        }
                    }
                }
            }
        }
    }
}

fn serve<F>(serving: FoundationServing<'_>, policy: &mut Policy<F>) -> ServingHandoff
where
    F: FnMut(TurnPurpose) -> TurnKey,
{
    let mut session = session::Session::begin(serving, policy.family, policy.method);
    loop {
        use session::Progress;
        match session.progress() {
            Progress::Pending | Progress::ElectionRefused => {
                session.advance();
            }
            Progress::Accepted => {
                let turn = (policy.next_turn)(TurnPurpose::Input);
                if session.start_turn(turn).is_err() {
                    session.wait_for_election();
                }
            }
            Progress::Completed | Progress::Refused | Progress::Aborted => {
                if session.next_input().is_err() {
                    unrouteable(session, "completed turn could not return its serving seat");
                }
            }
            Progress::DirectivePending => {
                if session.queue_save()
                    || session.defer_graceful_leave()
                    || session.join_graceful_leave()
                {
                    continue;
                }
                session = match session.into_exit() {
                    Ok(handoff) => return handoff,
                    Err(session) => session,
                };
                // A matching handoff may discover a fatal fault while checking
                // its original bounds. Let that retained owner enter halt.
                // Ordinary pending work moves into cleanup; no socket wait can
                // release a caller-owned hold or make an unmatched verb valid.
                if session.progress() == Progress::DirectivePending {
                    unrouteable(session, "retained directive has no admitted handoff");
                }
            }
            Progress::LeavePending => {
                if !session.begin_graceful_quiesce() {
                    session = match session.into_exit() {
                        Ok(handoff) => return handoff,
                        Err(session) => session,
                    };
                    session.advance();
                }
            }
            Progress::Quiesced => {
                let turn = (policy.next_turn)(TurnPurpose::WindDown);
                if session
                    .begin_graceful_wind_down(turn, policy.wind_down_passes.get())
                    .is_err()
                {
                    session.wait_for_election();
                }
            }
            Progress::WindDownComplete => {
                session = match session.into_exit() {
                    Ok(handoff) => return handoff,
                    Err(session) => session,
                };
                if session.progress() == Progress::WindDownComplete {
                    unrouteable(session, "completed wind-down has no admitted handoff");
                }
            }
            Progress::Stopped => match session.into_exit() {
                Ok(handoff) => return handoff,
                Err(session) => unrouteable(session, "stopped session has no failed handoff"),
            },
        }
    }
}

pub(super) fn input_handoff(
    input: Box<FoundationInput<'_>>,
) -> Result<ServingHandoff, Box<FoundationInput<'_>>> {
    if input.progress() == FoundationInputProgress::Stopped {
        return match super::halt(Ok(input)) {
            Ok(halt) => Ok(halted(halt)),
            Err(Ok(input)) => Err(input),
            Err(Err(owner)) => unrouteable(owner, "input halt returned foreign management"),
        };
    }
    input.graceful_leave_handoff().map(ServingHandoff::Graceful)
}

pub(super) fn management_handoff(
    owner: Box<weaver_harness::FoundationManagement<'_>>,
) -> Result<ServingHandoff, Box<weaver_harness::FoundationManagement<'_>>> {
    if owner.progress() == weaver_harness::FoundationManagementProgress::Stopped {
        return match super::halt(Err(owner)) {
            Ok(halt) => Ok(halted(halt)),
            Err(Err(owner)) => Err(owner),
            Err(Ok(input)) => unrouteable(input, "management halt returned foreign input"),
        };
    }
    let owner = match owner.rollback_handoff() {
        Ok(handoff) => return Ok(ServingHandoff::Rollback(handoff)),
        Err(owner) => owner,
    };
    owner.forced_leave_handoff().map(ServingHandoff::Forced)
}

fn halted(halt: Box<weaver_harness::FoundationHalt<'_>>) -> ServingHandoff {
    match halt.handoff() {
        Ok(handoff) => ServingHandoff::Failed(handoff),
        Err(halt) => unrouteable(
            halt,
            "terminal halt could not return its original run handoff",
        ),
    }
}

fn finish<T>(outcome: Option<FoundationProcessExit>, custody: T) -> ! {
    let code = match outcome {
        Some(FoundationProcessExit::Success) => 0,
        Some(FoundationProcessExit::Failure) => 1,
        None => unrouteable(custody, "cleanup has no terminal process disposition"),
    };
    // Keep all worker-lifetime resources alive until the actual process ends.
    let _custody = custody;
    std::process::exit(code)
}

fn unrouteable<T>(custody: T, _detail: &str) -> ! {
    let _custody = custody;
    // There is no admitted author or bounded diagnostic sink on this fallback.
    // In particular, stderr can block or fail: eprintln! would then unwind and
    // drop the live custody this path must retain until the process ends.
    std::process::exit(1)
}

#[cfg(test)]
mod tests {
    #[test]
    fn fatal_exit_retains_custody_with_unwritable_stderr() {
        const MARKER: &str = "WEAVER_DISPATCH_FATAL_CUSTODY_MARKER";
        if let Some(path) = std::env::var_os(MARKER) {
            struct Retained(std::path::PathBuf);
            impl Drop for Retained {
                fn drop(&mut self) {
                    std::fs::write(&self.0, b"custody dropped before process exit").unwrap();
                }
            }
            super::unrouteable(Retained(path.into()), "test retained ownership failure");
        }
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "weaver-dispatch-fatal-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir(&root).unwrap();
        let marker = root.join("dropped");
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "fatal_exit_retains_custody_with_unwritable_stderr",
                "--nocapture",
            ])
            .env(MARKER, &marker)
            .stdout(std::process::Stdio::null())
            .stderr(
                std::fs::OpenOptions::new()
                    .write(true)
                    .open("/dev/full")
                    .unwrap(),
            )
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(1));
        assert!(!marker.exists(), "fatal exit unwound the retained owner");
        std::fs::remove_dir(root).unwrap();
    }
}
