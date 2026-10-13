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
    // The three verbs of A3.2, each one fixed command line.
    for (verb, request) in [
        ("save-point", Request::SavePoint(AgentName("alpha".into()))),
        ("restore", Request::Restore(AgentName("alpha".into()))),
        (
            "force-unload",
            Request::ForceUnload(AgentName("alpha".into())),
        ),
    ] {
        assert_eq!(
            parse_arguments([verb.to_string(), "alpha".to_string()]),
            Ok(request)
        );
        assert_eq!(
            parse_arguments([verb.to_string(), "alpha".to_string(), "x".to_string()]),
            Err(LifecycleRefusal::Malformed),
            "{verb} takes no argument"
        );
    }

    for bad in [
        vec![],
        vec!["load".to_string()],
        vec!["load".to_string(), "a".to_string(), "b".to_string()],
        vec!["list".to_string()],
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
