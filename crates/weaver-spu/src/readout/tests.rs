/// **The open's registry, three arms in the clause's own order and no
/// fourth.** The third arm is judged here because no small artifact in
/// this workshop carries a family with a tap and no column, and the
/// first two are also bought at the seam against the real engine.
///
/// Perturbation: remove any arm from `judge_column_ask`, or reorder
/// the first two, and this fails on the arm's own case. Watched under
/// exactly those changes.
///
/// conforms: spu-column-registry-three-arms
#[test]
fn the_column_registry_holds_three_arms_in_order() {
    use weaver_types::TokenRefusal;
    assert_eq!(
        super::judge_column_ask(false, false, false),
        Some(TokenRefusal::ColumnPermissionAbsent),
        "the permission is judged first"
    );
    assert_eq!(
        super::judge_column_ask(true, false, false),
        Some(TokenRefusal::ColumnReadoutUnelected),
        "the election second"
    );
    assert_eq!(
        super::judge_column_ask(true, true, false),
        Some(TokenRefusal::ColumnUndeclared),
        "the declaration third"
    );
    assert_eq!(
        super::judge_column_ask(true, true, true),
        None,
        "and the ask stands past all three"
    );
}

use super::*;
use crate::decoder::backend::FlushMechanism;

const TAPPABLE: Declaration = Declaration {
    family: "tappable",
    shard_widths: &[1],
    template: "{message}",
    generation_opener: "",
    renderer: crate::family::qwen2::renderer,
    // These two fixtures are about the readout election and never reach a
    // selection, so an empty set is what they carry rather than a
    // borrowed one that would read as meaningful.
    selecting_markers: &[],
    flush: FlushMechanism::TruncateToPosition,
    taps_readout: true,
    taps_column: false,
};

const UNTAPPABLE: Declaration = Declaration {
    family: "untappable",
    shard_widths: &[1],
    template: "{message}",
    generation_opener: "",
    renderer: crate::family::qwen2::renderer,
    selecting_markers: &[],
    flush: FlushMechanism::TruncateToPosition,
    taps_readout: false,
    taps_column: false,
};

/// **An election a family cannot honor refuses at admit**, naming the
/// family, rather than loading and failing at the first turn.
///
/// Perturbation: section 10's watch is the check moving off the admit path,
/// under which the load succeeds and the first turn fails. Watched at the
/// admit path in `residency.rs`, where the ordering is observable; what
/// this test buys is the judgment itself.
#[test]
fn an_election_the_family_cannot_honor_is_refused() {
    assert_eq!(
        judge(ReadoutElection(true), &UNTAPPABLE),
        Err(ReadoutRefusal::NotTappable {
            family: "untappable"
        })
    );
    assert_eq!(judge(ReadoutElection(true), &TAPPABLE), Ok(()));
}

/// **A family answers columns only where that answer has been shown**,
/// which is the readout's own rule pointed at the second half of the
/// declaration: the tap's one-column copy continuing is a claim about
/// the family, per `weaver-spu-Spec` section 7, and a family joining
/// this set without its showing is the same defect the readout's
/// tripwire catches.
///
/// Perturbation: add a family here without flipping its declaration,
/// or flip a declaration without adding it here, and this fails.
#[test]
fn no_shipped_family_answers_a_column_it_cannot_hold() {
    // `qwen2` on the GGUF answer shown 2026-09-01, `qwen3` on the
    // showing of 2026-09-02 against Qwen3-8B-BF16.
    const COLUMNED: &[&str] = &["qwen2", "qwen3"];
    for declaration in crate::family::REGISTRY {
        assert_eq!(
            declaration.taps_column,
            COLUMNED.contains(&declaration.family),
            "{} disagrees with the shown-column set",
            declaration.family
        );
    }
}

/// **A column answered where no readout is elected is unreachable by
/// construction**, which the registry alone can say: every family that
/// declares a column declares the readout that produces it, and a
/// column without one would be a declaration the open's second
/// registry arm exists to refuse at every load.
#[test]
fn a_column_never_stands_without_its_readout() {
    for declaration in crate::family::REGISTRY {
        if declaration.taps_column {
            assert!(
                declaration.taps_readout,
                "{} declares a column and no readout to produce it",
                declaration.family
            );
        }
    }
}

/// **A shipped family advertises a tap only where one stands.** The
/// tripwire this replaces held that no family advertises one, and its
/// own text named the edit that retires it: the day a backend stands
/// its tap up, the declaration flips and this test is edited in the
/// same act. That act is 2026-08-19: the native tap stands for qwen2,
/// and the set below is the deliberate record of which families may
/// advertise. A family joining it without its tap is the defect this
/// test still catches.
#[test]
fn no_shipped_family_advertises_a_tap_it_cannot_perform() {
    // `qwen3` joined 2026-09-02 on the measurement its declaration
    // names: the GGUF bar taken against Qwen3-8B-BF16 on a real
    // device. Its sibling keys did not, each owing its own.
    const TAPPED: &[&str] = &["qwen2", "qwen35moe", "qwen3"];
    for declaration in crate::family::REGISTRY {
        assert_eq!(
            declaration.taps_readout,
            TAPPED.contains(&declaration.family),
            "{} disagrees with the stood-tap set",
            declaration.family
        );
    }
}

/// **The refusal is reachable against the shipped registry**, not only
/// against a fixture.
///
/// A judgment whose refusing branch only ever fires for a test-local
/// declaration proves the function and says nothing about the binary. This
/// walks every family this binary actually carries.
#[test]
fn an_elected_readout_refuses_against_every_shipped_family() {
    for declaration in crate::family::REGISTRY {
        // **The declaration is the whole answer** as of the tap act.
        // While the GGUF tap did not exist this loop asserted a blanket
        // refusal under that container and read the flag only under
        // safetensors, which is the asymmetry the tap removes.
        assert_eq!(
            judge(ReadoutElection(true), declaration).is_ok(),
            declaration.taps_readout,
            "{} disagrees with its own flag",
            declaration.family
        );
        assert_eq!(
            judge(ReadoutElection(false), declaration),
            Ok(()),
            "{} serves an unelected load",
            declaration.family
        );
    }
}

/// **No election, no refusal.** A family that cannot tap serves every load
/// that did not ask it to, which is what keeps the capability from becoming
/// a requirement.
#[test]
fn an_unelected_load_is_served_by_an_untappable_family() {
    assert_eq!(judge(ReadoutElection(false), &UNTAPPABLE), Ok(()));
}

/// **A forward folding a different layer count than the first is
/// refused**, per Spec section 6.
///
/// This is the condition the counts exist to survive. Layer election is
/// named as an economy the readout may take, and the day it lands a tap
/// could fold four layers on one forward and forty on the next. Folded
/// flat, the two counts would then describe neither, and a reader
/// dividing by the token count would get a number that is wrong without
/// being detectably wrong.
///
/// **No device produces this today**, which is why it is pinned here
/// rather than against a tap: on every artifact this workshop holds,
/// every forward taps every layer, so removing the engine's own short
/// forward guard changes nothing observable. A property whose violation
/// no fixture can produce is one a unit test has to hold.
#[test]
fn a_ragged_forward_is_refused_rather_than_folded_flat() {
    let mut reduction = Reduction::new();
    reduction
        .fold_forward(&[1.0, 2.0, 3.0])
        .expect("the first forward establishes the shape");
    assert_eq!(reduction.layers(), 3);
    assert_eq!(reduction.forwards(), 1);

    let refused = reduction
        .fold_forward(&[1.0, 2.0])
        .expect_err("a shorter forward is refused");
    assert!(
        refused.contains("2 layers") && refused.contains("first folded 3"),
        "the refusal names both counts: {refused}"
    );

    // **And the refusal leaves the reduction describable.** A forward
    // half folded would give counts whose product is not the figure
    // count, which is the state the whole clause exists to prevent.
    assert_eq!(
        reduction.figures(),
        3,
        "nothing of the ragged forward landed"
    );
    assert_eq!(
        reduction.layers() * reduction.forwards(),
        reduction.figures(),
        "the counts still describe the figures"
    );

    // An empty forward is not a forward.
    assert!(
        Reduction::new().fold_forward(&[]).is_err(),
        "a forward folding no layer is refused"
    );
}

/// **Loose figures and folded forwards do not mix, in either order.**
///
/// A reduction is a staging buffer, where loose folds build one forward's
/// figures with no boundary between them, or an accumulator, where whole
/// forwards arrive and each records where it ended. Mixing them breaks
/// the property section 6 asks the counts to have, and breaks it
/// silently: nothing errors and the figures still look like figures.
///
/// **Reachable by no caller today**, which is why it is refused rather
/// than left to discipline. The GGUF tap stages loose and accumulates
/// forwards on two different reductions, and the native path folds only
/// forwards. A type whose misuse is currently impossible is one whose
/// misuse arrives with the next tap.
#[test]
fn a_reduction_is_a_staging_buffer_or_an_accumulator_and_not_both() {
    // Forward first, then a loose figure: the total grows and the
    // forward count does not.
    let mut accumulating = Reduction::new();
    accumulating
        .fold_forward(&[1.0, 2.0, 3.0])
        .expect("the first forward establishes the shape");
    let refused = accumulating
        .fold_norm(9.0)
        .expect_err("a loose figure after a forward is refused");
    assert!(
        refused.contains("1 forward(s) of 3 layers"),
        "the refusal names what the reduction already holds: {refused}"
    );
    assert!(
        accumulating.fold(&[1.0, 1.0]).is_err(),
        "and the activation form is refused on the same ground"
    );
    assert_eq!(
        accumulating.layers() * accumulating.forwards(),
        accumulating.figures(),
        "the counts still describe the figures"
    );

    // Loose first, then a forward: those figures would belong to no
    // forward at all.
    let mut staged = Reduction::new();
    staged
        .fold_norm(1.0)
        .expect("a loose fold before any forward");
    staged.fold_norm(2.0).expect("and another");
    let refused = staged
        .fold_forward(&[3.0, 4.0, 5.0])
        .expect_err("a forward declared over loose figures is refused");
    assert!(
        refused.contains("2 loose figures"),
        "the refusal names what would be orphaned: {refused}"
    );
    assert_eq!(staged.figures(), 2, "and nothing of the forward landed");
    assert_eq!(
        staged.layers(),
        0,
        "the reduction is still a staging buffer"
    );
}

/// **The reduction keeps a figure per layer and never the layer.** What
/// leaves is a function of the layer count, not the activation width.
#[test]
fn the_reduction_holds_a_figure_per_layer_rather_than_the_layers() {
    let mut reduction = Reduction::new();
    // Two layers, wildly different widths. What comes out is two numbers.
    reduction
        .fold(&[3.0, 4.0])
        .expect("a loose fold before any forward");
    reduction.fold(&vec![1.0; 4096]).expect("and another");

    // **Figures, not layers.** This asserted `layers()` until
    // 2026-08-24, when that method returned the figure count and the two
    // coincided here. They no longer do: `fold` accumulates without
    // declaring a forward, so the layer count is still unset.
    assert_eq!(reduction.figures(), 2);
    assert_eq!(reduction.layers(), 0, "no forward has been declared");
    assert_eq!(reduction.forwards(), 0, "and so none is counted");
    assert_eq!(reduction.per_layer_norm().len(), 2);
    assert!((reduction.per_layer_norm()[0] - 5.0).abs() < 1e-5);
    assert!((reduction.per_layer_norm()[1] - 64.0).abs() < 1e-3);
}

/// **A tap outcome cannot say "nothing happened".** The enum carries two
/// cases and a failure has to name itself, so an elected readout that
/// stopped observing cannot be recorded as a run without readout.
#[test]
fn a_failed_tap_names_itself_rather_than_reporting_an_absence() {
    let faulted = TapOutcome::Faulted {
        detail: "the eval callback returned no tensor".into(),
    };
    match faulted {
        TapOutcome::Faulted { detail } => assert!(!detail.is_empty()),
        TapOutcome::Observed => panic!("this arm is the only other one"),
    }
}
