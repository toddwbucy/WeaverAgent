use super::*;

/// A node carrying a name and nothing else, for the matching tests.
///
/// Zeroed rather than built, because the matcher reads the name alone and
/// a zeroed `ggml_tensor` is a valid C struct with an empty name. The
/// assertion keeps a longer name from filling the array and leaving the
/// terminator off, which would send the matcher off the end.
fn named(name: &str) -> ggml_tensor {
    let mut tensor: ggml_tensor = unsafe { std::mem::zeroed() };
    assert!(name.len() < tensor.name.len(), "the fixture name must fit");
    for (slot, byte) in tensor.name.iter_mut().zip(name.bytes()) {
        *slot = byte as std::ffi::c_char;
    }
    tensor
}

/// **The tap asks for the residual outputs and for nothing else.**
///
/// The ask is not free: a wanted node ends the scheduler's window, so
/// the graph is computed up to it and synchronised before it continues.
/// A matcher that said yes generously would turn one compute into a walk
/// of hundreds of windows, which is the cost section 7 names.
#[test]
fn the_ask_is_answered_for_the_residual_outputs_alone() {
    for (name, wanted) in [
        ("l_out-0", true),
        ("l_out-27", true),
        ("attn_norm-3", false),
        ("ffn_out-3", false),
        // Not the residual output: a prefix match alone would take it.
        ("l_out_bias-3", false),
        // The layer index is part of the name the context formats, so a
        // bare `l_out` is not a node this tap has an index for.
        ("l_out", false),
        ("l_out-", false),
        ("", false),
    ] {
        let mut tensor = named(name);
        let mut state = TapState::new(1);
        let answered = unsafe { tap(&mut tensor, true, TapState::as_user_data(&mut state)) };
        assert_eq!(answered, wanted, "the ask for {name:?}");
    }
}

/// **The data pass never abandons the graph.**
///
/// The scheduler reads a false on the second call as "stop computing",
/// which would leave the context holding a partial forward while the
/// decode above it reported success. Every route out of the trampoline's
/// data pass answers true, including the ones that observe nothing.
#[test]
fn the_data_pass_answers_true_on_every_route() {
    let mut state = TapState::new(1);
    let user_data = TapState::as_user_data(&mut state);
    let mut unmatched = named("attn_norm-3");
    assert!(unsafe { tap(&mut unmatched, false, user_data) });
    assert!(unsafe { tap(std::ptr::null_mut(), false, user_data) });
    let mut matched = named("l_out-0");
    assert!(unsafe { tap(&mut matched, false, std::ptr::null_mut()) });
    // And the ask declines rather than accepting when there is nothing
    // to write into, which is the same two guards read the other way.
    assert!(!unsafe { tap(std::ptr::null_mut(), true, user_data) });
    assert!(!unsafe { tap(&mut matched, true, std::ptr::null_mut()) });
}

/// **A short observation is a fault, not a short reduction.**
///
/// Per charter section 13.10. This is the whole of why the commit counts:
/// a reduction holding twenty figures for a twenty-four layer model reads
/// as a measurement rather than as a tap that stopped observing, and the
/// consumer has no way to tell the two apart from the reduction alone.
#[test]
fn a_commit_short_of_the_layer_count_faults() {
    let mut state = TapState::new(24);
    for _ in 0..20 {
        state
            .staging
            .fold_norm(1.0)
            .expect("staging takes loose figures");
    }
    let fault = state.commit().expect_err("a short observation faults");
    assert!(
        format!("{fault:?}").contains("observed 20 of 24"),
        "the fault names what was missed: {fault:?}"
    );
    // **And the staging does not survive its own fault.** A commit that
    // left it standing would hand its figures to the next decode, which
    // would then commit a full count assembled out of two forwards.
    assert_eq!(state.drain().layers(), 0);
    assert!(state.commit().is_err(), "the next commit is short too");
}

/// **What the walk latched is raised by the decode it happened in**, and
/// the first fault is the one reported.
#[test]
fn a_latched_fault_is_raised_once_and_names_the_first_cause() {
    let mut state = TapState::new(2);
    state.latch("l_out-1 is not f32".into());
    state.latch("l_out-1 has no allocation to read".into());
    let fault = state.commit().expect_err("the latched fault is raised");
    let text = format!("{fault:?}");
    assert!(text.contains("not f32"), "the first cause: {text}");
    assert!(!text.contains("no allocation"), "and only it: {text}");
    // Raised once: the tap is not left faulted forever by one bad graph,
    // and the commit after it fails on its own count rather than on a
    // fault it already reported.
    let again = state.commit().expect_err("the next commit is short");
    assert!(
        format!("{again:?}").contains("observed 0 of 2"),
        "the fault does not persist: {again:?}"
    );
}

/// **The drain hands back what accumulated and starts empty.**
#[test]
fn the_drain_empties_what_it_hands_back() {
    let mut state = TapState::new(2);
    state
        .staging
        .fold_norm(3.0)
        .expect("staging takes loose figures");
    state
        .staging
        .fold_norm(4.0)
        .expect("staging takes loose figures");
    state.commit().expect("a full count commits");
    state
        .staging
        .fold_norm(5.0)
        .expect("staging takes loose figures");
    state
        .staging
        .fold_norm(6.0)
        .expect("staging takes loose figures");
    state.commit().expect("a second decode commits too");

    let drained = state.drain();
    assert_eq!(drained.per_layer_norm(), &[3.0, 4.0, 5.0, 6.0]);
    assert_eq!(state.drain().layers(), 0, "the drain empties");
}
