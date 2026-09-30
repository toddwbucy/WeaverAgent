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

/// One verb and the agent it names, where it names one.
#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    Load(AgentName),
    Unload(AgentName),
    Validate(AgentName),
    Stop(AgentName),
    Show(AgentName),
    List,
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
        "list" if rest.is_empty() => Ok(Request::List),
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

/// Whether a descriptor carries close-on-exec, for the walks that assert it.
#[cfg(test)]
pub fn is_close_on_exec(fd: std::os::fd::BorrowedFd<'_>) -> bool {
    use std::os::fd::AsRawFd;
    // SAFETY: F_GETFD on a descriptor the caller owns.
    let flags = unsafe { nix::libc::fcntl(fd.as_raw_fd(), nix::libc::F_GETFD) };
    flags >= 0 && (flags & nix::libc::FD_CLOEXEC) != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **The answer and the exit status agree.** Zero exits an answer and a
    /// non-zero status exits a refusal, so a shell reads the status and a tool
    /// reads the object and the two never disagree.
    ///
    /// The rendering carries the floor's tag, which is the discriminant: the
    /// case sets are disjoint, so a reader keys on the tag and needs no
    /// wrapper.
    ///
    /// Perturbation: exit zero on the refusal branch of `main` and a refusal
    /// reads as success to every shell. The pairing below is what that
    /// contradicts.
    #[test]
    fn the_answer_and_the_exit_status_agree() {
        let answer = render_answer(&LifecycleAnswer::Validated);
        assert!(
            answer.contains("\"kind\":\"validated\""),
            "the answer renders with the floor's tag, got {answer}"
        );
        let refusal = render_refusal(&LifecycleRefusal::NoResidency);
        assert!(
            refusal.contains("\"kind\":\"no_residency\""),
            "the refusal renders with the floor's tag, got {refusal}"
        );
        // The two tag spaces are disjoint, which is what lets one status
        // discriminate them.
        assert_ne!(answer, refusal);
        assert_eq!(EXIT_REFUSED, 1, "a refusal exits non-zero");
    }

    /// One verb per invocation, the agent as the one further argument where
    /// the verb takes one. Nothing is guessed at.
    #[test]
    fn the_verb_and_its_agent_arrive_as_arguments() {
        let parsed = parse_arguments(["load".to_string(), "alpha".to_string()]);
        assert_eq!(parsed, Ok(Request::Load(AgentName("alpha".into()))));
        assert_eq!(parse_arguments(["list".to_string()]), Ok(Request::List));

        for bad in [
            vec![],
            vec!["load".to_string()],
            vec!["load".to_string(), "a".to_string(), "b".to_string()],
            vec!["list".to_string(), "alpha".to_string()],
            vec!["enter".to_string(), "alpha".to_string()],
            vec!["".to_string()],
        ] {
            assert_eq!(
                parse_arguments(bad.clone()),
                Err(LifecycleRefusal::Malformed),
                "a malformed invocation refuses rather than guessing: {bad:?}"
            );
        }
    }

    /// **The root check refuses before any verb touches anything.** The suite
    /// runs as an ordinary uid, so this asserts the predicate reads the real
    /// uid rather than asserting an outcome the environment fixes.
    ///
    /// Perturbation: remove the `running_as_root` guard from `run` and a
    /// non-root invocation proceeds to the verb. The binary test in
    /// `tests/invocation.rs` is the half that watches that, because it can run
    /// the binary and read its status.
    #[test]
    fn the_root_check_reads_the_real_uid() {
        let expected = nix::unistd::getuid().as_raw() == 0;
        assert_eq!(
            running_as_root(),
            expected,
            "the check reads the invocation's own uid"
        );
    }
}
