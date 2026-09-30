//! conforms: spu-two-taps-one-shape
//! conforms: spu-tap-failure-is-a-fault
//!
//! The GGUF path's residual tap, per `weaver-spu-Spec` section 7.
//!
//! **This is the only unsafe surface the readout has.** The native tap reads
//! candle tensors through the fork's intermediates route and stays in safe
//! Rust throughout. The GGUF tap cannot: llama.cpp offers no route to a
//! layer's activations except the ggml scheduler's eval callback, which is a
//! C function pointer invoked from inside the graph walk, so the tap is a
//! trampoline over a pointer the caller hands back. Containing that in one
//! module is the point of the module.
//!
//! **What the tap matches, and why by name.** Every decoder-only body in the
//! fork ends its layer with `cb(cur, "l_out", il)`, which the context's own
//! naming callback formats as `l_out-<il>`. That is the post-residual-add
//! value the readout reduces, it is named identically across the hundred and
//! three bodies that carry it, and the bodies agree on nothing else, so the
//! name is a better handle than any position in the graph.
//!
//! **Three facts the mechanics turn on**, all read out of the fork rather
//! than assumed. The scheduler calls the callback twice per node, once
//! asking whether the data is wanted and once with the data standing, and
//! **a false answer on the second call abandons the rest of the graph**, so
//! this tap answers true there always and latches its faults instead. The
//! callback fires per **ubatch**, so a prefill longer than the micro-batch
//! crosses it several times and only the last of those carries the position
//! the decode asked logits for. And the final layer gathers its rows to the
//! output positions before the residual add, so the callback sees
//! `[n_embd, n_outputs]` there where the earlier layers give
//! `[n_embd, n_tokens]`: `ggml_get_rows` keeps `ne[0]` and replaces `ne[1]`,
//! so it is the column count that moves and not the width. The gather keeps
//! the batch order of the positions it retains, so the final column carries
//! the last position in both shapes, which is why the tap takes the last
//! column rather than a fixed index and needs no case for the final layer.
//!
//! **A ubatch is staged and committed rather than folded as it arrives.**
//! The figures for the ubatch in flight go to [`TapState::staging`], layer
//! zero clears it because layer zero is a new ubatch beginning, and the
//! decode commits what stands once the graph is done. The earlier ubatches
//! of a split prefill are therefore discarded rather than folded, which is
//! correct: their last column is the last position of a micro-batch and not
//! the position the generation is about to sample from.
//!
//! **A short observation is a fault rather than a short reduction.** Per
//! charter section 13.10, an elected observability that silently stopped
//! observing reads as a run without readout instead of a run whose readout
//! broke, so a commit whose staging does not hold one figure per layer
//! raises rather than folding what it has.

use std::ffi::{CStr, c_void};

use llama_cpp_sys_2::ggml_tensor;

use crate::decoder::backend::DecodeFault;
use crate::readout::Reduction;

/// The state the callback folds into, held at an address the context keeps.
///
/// **Its address outlives the engine's moves.** The context is handed a raw
/// pointer at creation and holds it for its whole life, so the state is boxed
/// before the context exists and the box is what the engine stores: moving
/// the engine moves the box and not its contents, and the pointer the
/// scheduler calls back through stays good.
pub struct TapState {
    /// The figures for the ubatch in flight, in the order the graph reached
    /// them. Folded through [`Reduction`] rather than into a bare `Vec` so
    /// the two taps cannot drift apart on how a figure is taken.
    staging: Reduction,
    /// What the drain answers with, accumulated across the decodes since the
    /// last drain.
    reduction: Reduction,
    /// The first fault the tap hit, held until a decode raises it. The
    /// callback cannot return a `Result` and must not abandon the graph, so
    /// this is how a failure inside the walk reaches the caller.
    fault: Option<String>,
    /// Where the one copied column lands, reused across layers so the
    /// callback allocates nothing after the first.
    column: Vec<f32>,
    /// How many figures a committed ubatch owes, from the model's own layer
    /// count.
    layers: usize,
    /// Whether the columns continue instead of dropping, per charter
    /// section 13.7 and Spec section 7's diagnostic clause: armed once at
    /// the open where the column ask stood, and never otherwise. The
    /// diagnostic answer is the copy this tap already takes, retained.
    hold_columns: bool,
    /// The ubatch in flight's columns where the hold stands, one per layer
    /// in layer order, cleared at layer zero with the staging it parallels.
    staged_columns: Vec<Vec<f32>>,
    /// The last committed forward's columns, taken by the draw site and
    /// overwritten by the next commit: a forward nothing samples at - the
    /// terminator's - is overwritten untaken, which is the position bound
    /// kept by construction.
    held_columns: Option<Vec<Vec<f32>>>,
}

impl TapState {
    pub fn new(layers: usize) -> Box<TapState> {
        Box::new(TapState {
            staging: Reduction::new(),
            reduction: Reduction::new(),
            fault: None,
            column: Vec::new(),
            layers,
            hold_columns: false,
            staged_columns: Vec::new(),
            held_columns: None,
        })
    }

    /// The pointer the context is handed, which is the box's contents rather
    /// than the box.
    pub fn as_user_data(state: &mut TapState) -> *mut c_void {
        state as *mut TapState as *mut c_void
    }

    /// Fold the ubatch that stands into the reduction, or raise what the
    /// walk latched.
    ///
    /// **Called by the decode and by nothing else.** The context reserves its
    /// graph at creation, which allocates without computing, so no
    /// observation reaches this before the first real decode, and anything
    /// the tap staged outside a decode is cleared by the next layer zero.
    pub fn commit(&mut self) -> Result<(), DecodeFault> {
        let staged = std::mem::take(&mut self.staging);
        if let Some(detail) = self.fault.take() {
            return Err(DecodeFault::Engine {
                detail: format!("the readout tap failed: {detail}"),
            });
        }
        if staged.figures() != self.layers {
            return Err(DecodeFault::Engine {
                detail: format!(
                    "the readout tap observed {} of {} layers",
                    staged.figures(),
                    self.layers
                ),
            });
        }
        // **Handed over as a forward, not as loose figures**, so the
        // reduction records where this one ended. A flat extend would leave
        // the boundary to arithmetic, which is the defect #293 names.
        self.reduction
            .fold_forward(staged.per_layer_norm())
            .map_err(|detail| DecodeFault::Engine {
                detail: format!("the readout tap folded a ragged forward: {detail}"),
            })?;
        if self.hold_columns {
            let columns = std::mem::take(&mut self.staged_columns);
            if columns.len() != self.layers {
                return Err(DecodeFault::Engine {
                    detail: format!(
                        "the column hold retained {} of {} layers",
                        columns.len(),
                        self.layers
                    ),
                });
            }
            self.held_columns = Some(columns);
        }
        Ok(())
    }

    /// Hand back what has accumulated and start empty.
    pub fn drain(&mut self) -> Reduction {
        std::mem::take(&mut self.reduction)
    }

    /// Arm or disarm the column hold, per the open's one ask.
    pub fn set_hold_columns(&mut self, hold: bool) {
        self.hold_columns = hold;
        if !hold {
            self.staged_columns.clear();
            self.held_columns = None;
        }
    }

    /// The last committed forward's columns, taken once: the draw site
    /// calls this at the moment it reads the distribution, so what it
    /// takes is the forward that feeds the draw.
    pub fn take_columns(&mut self) -> Option<Vec<Vec<f32>>> {
        self.held_columns.take()
    }

    /// Hold the first fault and let the later ones pass.
    ///
    /// The first is the one that explains the rest, and a tap that
    /// overwrote it would report the last symptom of a failure whose cause
    /// it had already seen.
    fn latch(&mut self, detail: String) {
        if self.fault.is_none() {
            self.fault = Some(detail);
        }
    }

    /// Take one layer's last column and fold it into the ubatch in flight.
    ///
    /// # Safety
    ///
    /// `tensor` is a live node of the graph being computed, whose data the
    /// scheduler has synchronised before this call.
    unsafe fn observe(&mut self, layer: usize, tensor: *const ggml_tensor) {
        // Layer zero is a ubatch beginning. What stood belonged to the
        // ubatch before it and is not the position this decode will sample.
        if layer == 0 {
            self.staging = Reduction::new();
            self.staged_columns.clear();
        }
        let node = unsafe { &*tensor };
        // **The value is f32 or the tap says so.** The residual stream is
        // f32 in every body the fork carries, and a half-width or quantised
        // node read as f32 would fold into a figure that means nothing,
        // which is the silently wrong reading the fault rule forbids.
        if node.type_ != llama_cpp_sys_2::GGML_TYPE_F32 {
            self.latch(format!("l_out-{layer} is not f32"));
            return;
        }
        let width = node.ne[0] as usize;
        let columns = node.ne[1] as usize;
        if width == 0 || columns == 0 {
            // A ubatch carrying no output position gathers the final layer
            // to nothing. That is the graph's shape rather than a failure,
            // and the ubatch it belongs to is discarded at the next layer
            // zero regardless.
            return;
        }
        // **The column must be contiguous to be read flat.** `nb[0]` is the
        // stride between elements within a column, and a value other than
        // one `f32` means the flat copy below would read across a gap.
        if node.nb[0] != std::mem::size_of::<f32>() {
            self.latch(format!("l_out-{layer} is not contiguous in its column"));
            return;
        }
        let bytes = width * std::mem::size_of::<f32>();
        let offset = node.nb[1].saturating_mul(columns - 1);
        // **The bounds are checked here because ggml checks them by
        // aborting.** `ggml_backend_tensor_get` asserts the buffer, the
        // allocation, and the range, and a failed `GGML_ASSERT` takes the
        // process down. A readout that killed the SPU rather than faulting
        // would be the most expensive lie in the program.
        let buffer = if node.view_src.is_null() {
            node.buffer
        } else {
            unsafe { (*node.view_src).buffer }
        };
        if buffer.is_null() || node.data.is_null() {
            self.latch(format!("l_out-{layer} has no allocation to read"));
            return;
        }
        let held = unsafe { llama_cpp_sys_2::ggml_nbytes(tensor) };
        if offset.saturating_add(bytes) > held {
            self.latch(format!(
                "l_out-{layer} holds {held} bytes and the last column ends at {}",
                offset + bytes
            ));
            return;
        }
        self.column.clear();
        self.column.resize(width, 0.0);
        unsafe {
            llama_cpp_sys_2::ggml_backend_tensor_get(
                tensor,
                self.column.as_mut_ptr() as *mut c_void,
                offset,
                bytes,
            );
        }
        // **The activations are folded and dropped here**, per apex section
        // 3 step 6: about eight kibibytes crossed off the device for one
        // layer and one scalar is kept, and `self.column` is overwritten by
        // the next layer rather than retained.
        // The staging reduction folds loose figures and never a forward, so
        // this refuses only if that discipline were broken above. Latched
        // rather than ignored, because a tap that silently stopped folding is
        // the absence the fault rule forbids.
        if let Err(detail) = self.staging.fold(&self.column) {
            self.latch(format!("l_out-{layer}: {detail}"));
        }
        // **The column continues instead of dropping where the ask stood**,
        // per Spec section 7's diagnostic clause: the same copy the norm
        // was folded from, retained per layer, and no second capture
        // exists.
        if self.hold_columns {
            self.staged_columns.push(self.column.clone());
        }
    }
}

/// The layer index if this node is a body's post-residual output.
///
/// # Safety
///
/// `tensor` is a live graph node, whose `name` ggml NUL-terminates through
/// `snprintf`.
unsafe fn residual_layer_index(tensor: *const ggml_tensor) -> Option<usize> {
    let name = unsafe { CStr::from_ptr((*tensor).name.as_ptr()) };
    name.to_str().ok()?.strip_prefix("l_out-")?.parse().ok()
}

/// The trampoline the ggml scheduler calls, twice per node.
///
/// # Safety
///
/// Installed only by [`super::gguf::GgufEngine::open`], which passes a
/// pointer into a [`TapState`] the engine owns and outlives the context.
pub unsafe extern "C" fn tap(tensor: *mut ggml_tensor, ask: bool, user_data: *mut c_void) -> bool {
    if tensor.is_null() || user_data.is_null() {
        // Nothing observable. On the ask this declines the node, and on the
        // data pass it continues the graph, which is the answer that never
        // abandons a decode.
        return !ask;
    }
    let layer = unsafe { residual_layer_index(tensor) };
    if ask {
        // **Answering false here is free and answering true is not.** A
        // wanted node ends its window, so the scheduler computes up to it,
        // synchronises, and starts again, which is the cost section 7 names
        // and the reason nothing but `l_out-<il>` is asked for.
        return layer.is_some();
    }
    if let Some(layer) = layer {
        let state = unsafe { &mut *(user_data as *mut TapState) };
        unsafe { state.observe(layer, tensor) };
    }
    // Always, per the module header: false abandons the graph.
    true
}

#[cfg(test)]
mod tests {
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
}
