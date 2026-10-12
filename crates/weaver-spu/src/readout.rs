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
mod tests;
