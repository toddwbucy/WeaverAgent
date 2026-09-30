//! conforms: spu-reduction-in-place-at-the-tap
//! conforms: spu-tap-failure-is-a-fault
//!
//! Residual readout, per `weaver-spu-Spec` section 7.
//!
//! **The reduction happens at the tap, before anything leaves the device.**
//! Apex section 3 step 6 has the activations reduced in place and the reduction
//! returning by the same path as the generation, so no per-layer tensor crosses
//! the seam and the volume the seam carries is the reduction's. That is stated
//! here as a type rather than as a discipline: [`Tap::observe`] is handed a
//! layer's activations by reference and answers nothing, accumulating into the
//! reduction it owns, and the only thing that leaves is [`Reduction`]. There is
//! no path on this seam that returns a per-layer tensor, so a caller cannot
//! carry one off the device by mistake.
//!
//! **One shape, whichever backend produced it.** The native path uses the
//! candle fork's `forward_with_intermediates` and the GGUF path the ggml
//! scheduler's eval callback the llama-cpp fork exposes. Both drive this seam
//! and both answer [`Reduction`], so a consumer reads one shape and cannot tell
//! which engine produced it. **The native tap stands as of 2026-08-19** for
//! the families whose declarations say so, at both widths, folding each
//! layer's device-side norm through [`Reduction::fold_norm`]. **The GGUF tap
//! stands as of 2026-08-22**, in [`crate::decoder::gguf_tap`], reading
//! `l_out-<il>` off the ggml scheduler's eval callback and folding through
//! [`Reduction::fold`]. Both answer [`Reduction`], so [`judge`] now reads
//! the family's flag alone: the container stopped being a ground for
//! refusing on the day the second tap stood.
//!
//! **What the GGUF tap has shown and what it still owes.** `weaver-spu-PRD`
//! section 13.7 obliges an elected readout to change no token, per tap
//! rather than once for the election. The GGUF tap is watched clearing that
//! bar on the host backend, in `gguf.rs`, which reaches everything but the
//! one hazard the Spec names: installing the callback turns one graph
//! compute into a walk of windows, and a fusion candidate straddling a
//! window boundary goes unapplied, which is a device concern no host run
//! can reach. **The measurement on the real device pair was therefore owed
//! and is taken**, in `tests/readout_neutral.rs`, against the artifact this
//! workshop deploys: two seeds, each drawing a sequence identical with the
//! election on and off, the elected run folding one figure per layer per
//! forward. The deployed family declares its tap on that run, so the
//! election is one an operator in service can make.
//!
//! **A tap that fails while elected is a fault, not an absence.** Per charter
//! section 13.10: an elected observability that silently stopped observing is a
//! record that reads as a run without readout rather than a run whose readout
//! broke. [`TapOutcome`] carries no case for a quiet nothing, which is what
//! makes the distinction unrepresentable rather than merely discouraged.

use crate::family::Declaration;

/// Whether the operator elected residual readout for this load.
///
/// A newtype rather than a bare `bool` because it travels beside other flags on
/// the admit path, and two booleans in a row is the argument order nobody
/// remembers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadoutElection(pub bool);

impl ReadoutElection {
    pub fn elected(&self) -> bool {
        self.0
    }
}

/// What a tap answers with, per layer.
///
/// **There is no case for a quiet absence.** A tap either observed or it
/// faulted, and the caller cannot record "nothing happened" without saying
/// which. That is the whole of section 7's fault-not-absence claim expressed as
/// a type.
#[derive(Debug, Clone, PartialEq)]
pub enum TapOutcome {
    /// The layer was observed and folded into the reduction.
    Observed,
    /// The tap failed while elected. Carries what failed, because a fault the
    /// record cannot name is a fault the operator cannot act on.
    Faulted { detail: String },
}

/// Why a readout election could not be honored.
#[derive(Debug, Clone, PartialEq)]
pub enum ReadoutRefusal {
    /// The election was made and this family's engine cannot tap.
    ///
    /// **Refused at admit rather than at the first turn**, per the charter's
    /// fail-cheap-or-lie-expensive rule. A load that succeeded and then failed
    /// on the first turn has already cost the operator the load.
    NotTappable { family: &'static str },
}

/// The reduction a tap accumulates, and the only thing that leaves the device.
///
/// **In place.** The tap folds each layer into the running figures and never
/// retains the layer, so the memory this holds is a function of the layer count
/// rather than of the activation width, and nothing here can be widened into a
/// per-layer tensor without changing the type.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Reduction {
    /// One figure per layer observed, in layer order, across every forward
    /// folded.
    per_layer_norm: Vec<f32>,
    /// Layers per forward, established by the first forward folded.
    ///
    /// **Zero until something is folded**, because a reduction holding
    /// nothing has no shape to report and reporting one would be an
    /// assertion about a tap that never ran.
    layers: usize,
}

impl Reduction {
    pub fn new() -> Self {
        Reduction::default()
    }

    /// Fold one layer in, keeping the figure and dropping the activations.
    ///
    /// The caller's slice is borrowed and not retained, which is what "in
    /// place" means here: it keeps a scalar per layer.
    ///
    /// **This declares no forward boundary**, so a reduction built only from
    /// per-layer folds reports no layer count and no forward count. That is
    /// the staging shape the GGUF tap uses while a forward is in flight, and
    /// the completed forward reaches the reduction that answers through
    /// [`Reduction::fold_forward`].
    pub fn fold(&mut self, activations: &[f32]) -> Result<(), String> {
        self.refuse_loose_after_a_forward()?;
        let sum_of_squares: f32 = activations.iter().map(|value| value * value).sum();
        self.per_layer_norm.push(sum_of_squares.sqrt());
        Ok(())
    }

    /// Fold one layer whose norm was already taken on the device, which is
    /// the stronger reading of the in-place clause: the activations never
    /// leave the device at all and one scalar crosses.
    pub fn fold_norm(&mut self, norm: f32) -> Result<(), String> {
        self.refuse_loose_after_a_forward()?;
        self.per_layer_norm.push(norm);
        Ok(())
    }

    /// **A reduction is a staging buffer or an accumulator, never both.**
    ///
    /// Loose folds build one forward's figures with no boundary between
    /// them, which is the staging shape. `fold_forward` accumulates whole
    /// forwards and records where each ended. Mixing them breaks the one
    /// property section 6 asks the counts to have, in either order: a loose
    /// figure after a forward grows the total without growing the forward
    /// count, and a forward after loose figures leaves those figures inside
    /// no forward at all. Both leave `layers` times `forwards` no longer
    /// equal to `figures`, silently.
    fn refuse_loose_after_a_forward(&self) -> Result<(), String> {
        if self.layers == 0 {
            return Ok(());
        }
        Err(format!(
            "a loose figure cannot join a reduction that has folded {} forward(s) \
             of {} layers, since the counts would then describe neither",
            self.forwards(),
            self.layers
        ))
    }

    /// One forward's figures, in layer order, and the layer count with them.
    ///
    /// **The boundary is declared rather than inferred.** A flat run of
    /// figures cannot say where one forward ended and the next began, and
    /// that is the whole of the defect this closes: a reader was left
    /// dividing by the token count plus one, an arithmetic that holds only
    /// while every forward taps every layer.
    ///
    /// A forward whose layer count differs from the first is a defect in the
    /// tap rather than a reading, so it is refused here rather than folded
    /// into a shape the counts would then misdescribe.
    pub fn fold_forward(&mut self, norms: &[f32]) -> Result<(), String> {
        if norms.is_empty() {
            return Err("a forward folded no layer at all".into());
        }
        // The other order: loose figures already staged here belong to no
        // forward, and declaring one now would leave them outside every
        // count.
        if self.layers == 0 && !self.per_layer_norm.is_empty() {
            return Err(format!(
                "a forward cannot be declared over {} loose figures already \
                 folded, which would belong to no forward",
                self.per_layer_norm.len()
            ));
        }
        if self.layers == 0 {
            self.layers = norms.len();
        } else if self.layers != norms.len() {
            return Err(format!(
                "a forward folded {} layers where the first folded {}",
                norms.len(),
                self.layers
            ));
        }
        self.per_layer_norm.extend_from_slice(norms);
        Ok(())
    }

    /// Layers per forward, and zero where nothing has been folded.
    pub fn layers(&self) -> usize {
        self.layers
    }

    /// How many forwards were folded.
    ///
    /// Derived rather than counted, since the product of the two counts is
    /// the array's length by construction and a third field would be a
    /// second source for one fact.
    pub fn forwards(&self) -> usize {
        self.per_layer_norm
            .len()
            .checked_div(self.layers)
            .unwrap_or(0)
    }

    /// Every figure, across every forward.
    ///
    /// **Named for what it returns.** This was `layers` until 2026-08-24,
    /// which was true only where exactly one forward had been folded, and
    /// every caller happened to be in that case. A name true under a
    /// condition nobody states is the shape of defect the perturbation
    /// obligation exists to catch.
    pub fn figures(&self) -> usize {
        self.per_layer_norm.len()
    }

    pub fn per_layer_norm(&self) -> &[f32] {
        &self.per_layer_norm
    }
}

/// The seam both taps drive.
///
/// **`observe` answers no tensor.** It is handed a borrowed layer and folds it,
/// so the per-layer activations have no route off the device through this
/// trait. `finish` yields the reduction, which is the volume the seam carries.
pub trait Tap {
    /// Fold one layer's activations in, at the tap.
    fn observe(&mut self, layer: usize, activations: &[f32]) -> TapOutcome;

    /// Hand back the reduction and end the tap.
    fn finish(self: Box<Self>) -> Reduction;
}

/// Judge a readout election against what the family declares, at admit.
///
/// **This runs on the admit path and nowhere else.** Moving it later is what
/// section 10's watch perturbs: the load then succeeds and the first turn
/// fails, which is the expensive lie the charter's rule forbids.
/// **The container is not a parameter.** It was one while the GGUF tap did
/// not exist and the container was therefore a second ground for refusing.
/// The tap stands, the ground is gone, and a parameter kept against a reader
/// that no longer exists is the reserved slot the apex forbids: the next
/// reader would take its presence for a judgment this function makes.
pub fn judge(election: ReadoutElection, declaration: &Declaration) -> Result<(), ReadoutRefusal> {
    if !election.elected() {
        return Ok(());
    }
    // **The family's flag is the whole of the ground.** The container was a
    // second ground while the GGUF tap did not exist, and it stopped being
    // one when the tap stood on 2026-08-22: both engines tap now, so which
    // one will serve says nothing about whether the election can be
    // honored, and a family that does not declare its tap still refuses
    // here, at admit, rather than serving turns whose measurement quietly
    // lacks what the operator elected.
    if !declaration.taps_readout {
        return Err(ReadoutRefusal::NotTappable {
            family: declaration.family,
        });
    }
    Ok(())
}

/// The open's column registry, per charter section 13.7: three arms and no
/// others, judged in the clause's own order at the open - the cheapest
/// moment that knows the ask. `None` is the ask standing, which arms the
/// answer for the residency.
///
/// A function rather than an inline chain so the third arm is reachable by
/// a test: a family with a tap and no column has no small artifact in this
/// workshop, and the seam tests buy the first two arms against the real
/// engine.
pub fn judge_column_ask(
    permission: bool,
    readout_elected: bool,
    taps_column: bool,
) -> Option<weaver_types::TokenRefusal> {
    if !permission {
        return Some(weaver_types::TokenRefusal::ColumnPermissionAbsent);
    }
    if !readout_elected {
        return Some(weaver_types::TokenRefusal::ColumnReadoutUnelected);
    }
    if !taps_column {
        return Some(weaver_types::TokenRefusal::ColumnUndeclared);
    }
    None
}

#[cfg(test)]
mod tests {
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
}
