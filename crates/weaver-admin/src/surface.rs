//! conforms: admin-runs-as-root-or-performs-nothing
//! conforms: admin-answer-and-exit-status-agree
//!
//! The invocation's interface, per `weaver-admin-Spec` section 2: the operator
//! runs the binary with root, one verb per run.
//!
//! **The socket this module carried until 2026-08-05 retired with the service
//! account**, and what replaces it is the process boundary the operating
//! system already draws around an executed program. No listener, no accept
//! loop, no peer predicate, and no threads: one invocation runs one verb and
//! exits.

use weaver_types::{AgentName, LifecycleAnswer, LifecycleRefusal};

/// The status a refusal exits with. Any non-zero value would satisfy the
/// agreement, and one fixed value is what lets a shell test for a refusal
/// without parsing the object.
pub const EXIT_REFUSED: i32 = 1;

/// One verb and the agent it names. Every verb names one: admin is one agent's
/// organ, on the operator's ruling of 2026-10-01, and a verb that answered for
/// several agents would be fleet management, which is not this crate's.
#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    Load(AgentName),
    Unload(AgentName),
    Validate(AgentName),
    Stop(AgentName),
    Show(AgentName),
    /// A save point of the running agent on demand, published at once, as
    /// of A3.2 on the operator's rulings of 2026-10-06 on #1.
    SavePoint(AgentName),
    /// Name the save point the next load restores, the declaration's.
    Restore(AgentName),
    /// The unload that completes without its leave save point.
    ForceUnload(AgentName),
}

impl Request {
    /// The agent the verb names, whose root the invocation reads.
    pub fn agent(&self) -> &AgentName {
        match self {
            Request::Load(agent)
            | Request::Unload(agent)
            | Request::Validate(agent)
            | Request::Stop(agent)
            | Request::Show(agent)
            | Request::SavePoint(agent)
            | Request::Restore(agent)
            | Request::ForceUnload(agent) => agent,
        }
    }
}

/// **The verb and its agent arrive as arguments**, rather than as a parsed
/// request line: the kernel already delivered them as a vector, and
/// re-encoding them into a wire format would be inventing a wire where no seam
/// crosses.
///
/// A verb this crate does not carry, a missing agent, or a trailing argument
/// is `Malformed`. Nothing is guessed at and no default verb exists.
pub fn parse_arguments<I>(arguments: I) -> Result<Request, LifecycleRefusal>
where
    I: IntoIterator<Item = String>,
{
    let arguments: Vec<String> = arguments.into_iter().collect();
    let (verb, rest) = arguments.split_first().ok_or(LifecycleRefusal::Malformed)?;

    let named = |rest: &[String]| -> Result<AgentName, LifecycleRefusal> {
        match rest {
            [agent] => Ok(AgentName(agent.clone())),
            _ => Err(LifecycleRefusal::Malformed),
        }
    };

    match verb.as_str() {
        "load" => Ok(Request::Load(named(rest)?)),
        "unload" => Ok(Request::Unload(named(rest)?)),
        "validate" => Ok(Request::Validate(named(rest)?)),
        "stop" => Ok(Request::Stop(named(rest)?)),
        "show" => Ok(Request::Show(named(rest)?)),
        "save-point" => Ok(Request::SavePoint(named(rest)?)),
        "restore" => Ok(Request::Restore(named(rest)?)),
        "force-unload" => Ok(Request::ForceUnload(named(rest)?)),
        _ => Err(LifecycleRefusal::Malformed),
    }
}

/// Whether this invocation holds root. **Authorization is the kernel's**: the
/// operator is the party it already admitted, so no predicate, no allow set,
/// and no deny set exist here.
pub fn running_as_root() -> bool {
    nix::unistd::getuid().is_root()
}

/// One `lifecycle-answer` in the floor's internally tagged rendering.
pub fn render_answer(answer: &LifecycleAnswer) -> String {
    serde_json::to_string(answer).expect("the floor's wire types render")
}

/// One `lifecycle-refusal` in the same rendering. The discriminant between the
/// two is the tag itself, the case sets being disjoint at the floor, so a
/// consumer keys on the tag and needs no wrapper.
pub fn render_refusal(refusal: &LifecycleRefusal) -> String {
    serde_json::to_string(refusal).expect("the floor's wire types render")
}

/// The paired serving projection carries FailedRun without altering the legacy
/// command dispatcher before the qualified cutover.
#[allow(dead_code)]
pub(crate) fn render_serving_refusal(
    refusal: &weaver_types::state_flow::capture::CaptureRefusal,
) -> String {
    serde_json::to_string(refusal).expect("the floor's serving refusal renders")
}

/// Whether a descriptor carries close-on-exec, for the walks that assert it.
// #118: a test-only hook, kept beside the production code it observes.
#[cfg(test)]
pub fn is_close_on_exec(fd: std::os::fd::BorrowedFd<'_>) -> bool {
    use std::os::fd::AsRawFd;
    // SAFETY: F_GETFD on a descriptor the caller owns.
    let flags = unsafe { nix::libc::fcntl(fd.as_raw_fd(), nix::libc::F_GETFD) };
    flags >= 0 && (flags & nix::libc::FD_CLOEXEC) != 0
}

#[cfg(test)]
mod tests;
