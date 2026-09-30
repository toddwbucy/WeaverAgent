//! conforms: spu-eval-callback-pinned-by-doctest
//!
//! The GGUF backend's residency half, per `weaver-spu-Spec` sections 3 and 4.1:
//! llama.cpp holds the model, the weights go to the binding's devices, and
//! dropping [`ResidentModel`] is what frees them. The decode half, the
//! [`crate::decoder::backend::Backend`] implementation over a llama.cpp
//! context, is the turn path's act and is not here.
//!
//! **The fork seam is a compile-time pin rather than a comment.** The fork
//! exists for one reason, the ggml scheduler's eval callback, which is the only
//! route to per-layer activations from a GGUF model without replacing the
//! engine, and the survey names this pin as the one thing standing between the
//! fork and a quiet loss. The doctest below calls the setter: it compiles
//! against the fork and fails to compile against the upstream crate, so
//! reverting the pin breaks the build rather than silently removing the
//! readout capability.
//!
//! ```
//! use llama_cpp_2::context::params::LlamaContextParams;
//! // The eval-callback seam: present in the fork, absent upstream. Reverting
//! // the pin makes this stop compiling, which is the pin firing. The callback
//! // is a real one and the assertion reads the state change, so the pin holds
//! // the seam open rather than only naming it.
//! unsafe extern "C" fn tap(
//!     _tensor: *mut llama_cpp_sys_2::ggml_tensor,
//!     _ask: bool,
//!     _user_data: *mut std::ffi::c_void,
//! ) -> bool {
//!     true
//! }
//! let params = LlamaContextParams::default();
//! assert!(!params.has_eval_callback());
//! let params = unsafe { params.with_eval_callback(Some(tap), std::ptr::null_mut()) };
//! assert!(params.has_eval_callback());
//! ```
//!
//! **The two loader shapes the Spec names cannot reach this module.** The one
//! entry is [`ResidentModel::load`], which takes the admission's proof, so a
//! bare `&str` or a `PathBuf` outside the admission path fails to compile:
//!
//! ```compile_fail
//! fn pin(path: &str) -> weaver_spu::decoder::gguf::ResidentModel {
//!     weaver_spu::decoder::gguf::ResidentModel::load(path).unwrap()
//! }
//! ```

use std::sync::OnceLock;

use llama_cpp_2::context::LlamaContext;
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::token::LlamaToken;

use crate::decoder::backend::{Backend, DecodeFault, TokenId};
use crate::decoder::gguf_tap;
use crate::sampling::EffectiveKnobs;
use llama_cpp_2::model::params::{LlamaModelParams, LlamaSplitMode};

use crate::residency::{Admission, AdmitRefusal};

/// The one llama.cpp backend this process holds.
///
/// `LlamaBackend::init` sets up global engine state and may be called once per
/// process, so it lives behind a `OnceLock`, the same one-owner discipline the
/// archived tree's own note records against itself. A failure is held as the
/// failure, so a second admit attempt reports the same fact instead of
/// re-initializing global state that half-exists.
static BACKEND: OnceLock<Result<LlamaBackend, String>> = OnceLock::new();

fn backend() -> Result<&'static LlamaBackend, AdmitRefusal> {
    let state = BACKEND.get_or_init(|| {
        LlamaBackend::init().map_err(|error| format!("llama.cpp backend init: {error}"))
    });
    match state {
        Ok(backend) => Ok(backend),
        Err(detail) => Err(AdmitRefusal::LoadFailed {
            detail: detail.clone(),
        }),
    }
}

/// A GGUF model resident on the binding's devices.
///
/// Holding this is the residency. The drop frees the device: llama.cpp returns
/// the weights' memory when the model is dropped, which is what lets the
/// release's ordering hold by construction.
pub struct ResidentModel {
    model: LlamaModel,
}

impl ResidentModel {
    /// Load the admission's artifact onto the admission's devices.
    ///
    /// The devices are the binding's and this crate selects none: one device
    /// pins `main_gpu` with no split, and a pair splits by layer across
    /// exactly the named ordinals, never whatever else the driver can see.
    /// The parameter shapes carry the archived tree's own working
    /// configuration forward.
    ///
    /// **An assumption crosses here and is named rather than silent.** The
    /// admission judges room and reach against CUDA ordinals, while
    /// `main_gpu` and `with_devices` index ggml's registered-device
    /// enumeration. The two numberings coincide while the libllama this
    /// build links registers CUDA devices only and in ordinal order, which
    /// holds for the library the pinned build script compiles. A deployment
    /// linking a libllama with another backend registered, RPC or Vulkan or
    /// SYCL, breaks the coincidence and puts weights on a card the admission
    /// never judged. The mapping through the ggml device registry belongs to
    /// the decode act, which touches the engine's device surface anyway, and
    /// until then this paragraph is the boundary of what is verified.
    pub fn load(admission: &Admission<'_>) -> Result<ResidentModel, AdmitRefusal> {
        let backend = backend()?;
        let devices = admission.devices();

        // Every layer goes to the device: a partial offload would leave part
        // of the model in host memory, a residency the admission never judged.
        let mut params = LlamaModelParams::default()
            .with_n_gpu_layers(u32::MAX)
            .with_main_gpu(devices[0].0 as i32)
            .with_split_mode(if devices.len() == 1 {
                LlamaSplitMode::None
            } else {
                LlamaSplitMode::Layer
            });
        if devices.len() > 1 {
            let ordinals: Vec<usize> = devices.iter().map(|d| d.0 as usize).collect();
            params = params
                .with_devices(&ordinals)
                .map_err(|error| AdmitRefusal::LoadFailed {
                    detail: format!("device set {ordinals:?}: {error}"),
                })?;
        }

        // **A split artifact loads by its whole pinned set.** llama.cpp
        // resolves siblings from the first file's name only when the name
        // carries the split pattern, and a descriptor path carries none, so
        // the set is named explicitly, in split order, every member the
        // admission pinned. One path takes the single-file door it always
        // took.
        let paths = admission.paths();
        let model = if paths.len() > 1 {
            LlamaModel::load_from_splits(backend, paths, &params).map_err(|error| {
                AdmitRefusal::LoadFailed {
                    detail: format!("load_from_splits: {error}"),
                }
            })?
        } else {
            LlamaModel::load_from_file(backend, admission.path(), &params).map_err(|error| {
                AdmitRefusal::LoadFailed {
                    detail: format!("load_from_file: {error}"),
                }
            })?
        };
        Ok(ResidentModel { model })
    }

    /// The engine's handle, for the decode acts that follow. Crate-visible so
    /// nothing outside reaches the engine around the seam. Nothing calls it
    /// yet, and the allow says so instead of widening the visibility to quiet
    /// the lint: the decode half is this module's next act, and the field it
    /// reads is what holding the residency means.
    #[allow(dead_code)]
    pub(crate) fn model(&self) -> &LlamaModel {
        &self.model
    }
}

/// The GGUF engine, per `weaver-spu-Spec` section 4.
///
/// **It borrows the residency it decodes against.** `LlamaContext` borrows the
/// model, and that is the relationship in life as well as in types: a session
/// cannot outlive the residency it was opened over, and the borrow is what says
/// so without a rule anyone has to remember.
///
/// **The loop above this is `session.rs`'s and stays there.** What lives here is
/// the five primitives the seam declares and nothing else, so the terminator
/// discipline and the cancel bound hold for this engine by construction rather
/// than by this file remembering them.
pub struct GgufEngine<'a> {
    context: LlamaContext<'a>,
    sampler: LlamaSampler,
    /// The batch index whose logits the last decode left standing, which is the
    /// only position `distribution` and `sample` may read. `None` before the
    /// first decode, so a sample before any decode refuses rather than reading
    /// a buffer nothing filled.
    logits_at: Option<i32>,
    closed: bool,
    /// The knobs the chain is rebuilt from at each generation, per
    /// `weaver-spu-Spec` section 8.5. Held because the chain holds nothing
    /// between generations and is therefore built rather than reset.
    knobs: EffectiveKnobs,
    /// The residual tap's state where the load elected readout, per Spec
    /// section 7, and `None` where it did not.
    ///
    /// **Deliberately a raw pointer rather than a `Box`.** The ggml
    /// scheduler calls back into this state from inside `decode`, which
    /// runs under `&mut self`, and a `Box` field would have that borrow
    /// claim the state exclusively for the whole of the call the callback
    /// writes during. A raw pointer makes no such claim. Ownership is not
    /// thereby shared: this struct is the one owner and [`Drop`] is what
    /// frees it.
    tap: Option<std::ptr::NonNull<gguf_tap::TapState>>,
}

impl Drop for GgufEngine<'_> {
    fn drop(&mut self) {
        if let Some(state) = self.tap.take() {
            // Safety: allocated by `Box::into_raw` in `open`, handed to
            // nothing but this context, and taken here so a second drop
            // cannot reach it. The context is still standing when this
            // runs, holding the pointer as its `cb_eval_user_data`, and
            // never dereferences it: freeing a context computes no graph.
            drop(unsafe { Box::from_raw(state.as_ptr()) });
        }
    }
}

impl<'a> GgufEngine<'a> {
    /// Open a context over a resident model and build its sampler from the
    /// binary's resolved knobs.
    ///
    /// **The sampler is built once, from the effective values.** The knobs
    /// resolved at the composition root per Spec section 8, so nothing here
    /// re-reads a disposition or learns which side supplied a value.
    /// **The readout election arrives here and nowhere later.** `cb_eval`
    /// is a context parameter, so a tap can only be installed as the context
    /// is created, which is why the election travels from admit rather than
    /// from the turn that would like it.
    pub fn open(
        model: &'a ResidentModel,
        knobs: &EffectiveKnobs,
        capacity: u32,
        readout: bool,
    ) -> Result<GgufEngine<'a>, DecodeFault> {
        let backend = backend().map_err(|_| DecodeFault::Engine {
            detail: "the llama backend is not initialised".into(),
        })?;
        // **A zero capacity refuses rather than being substituted.**
        // `NonZeroU32::new(0)` is `None`, and `with_n_ctx(None)` leaves `n_ctx`
        // at zero, which llama.cpp reads as the model's own training context.
        // A caller asking for nothing would be served the whole window, and
        // the session above would hold a capacity the context does not have,
        // which is the disagreement `decode_at` exists to prevent.
        let capacity = std::num::NonZeroU32::new(capacity)
            .ok_or_else(|| Self::engine_fault("a session capacity of zero serves nothing"))?;
        let params = LlamaContextParams::default()
            .with_n_ctx(Some(capacity))
            .with_n_batch(capacity.get());
        // The state is allocated before the context because the context is
        // handed its address, and it is leaked out of its box here so that
        // no later `&mut self` claims the pointee the callback writes to.
        let tap = readout.then(|| {
            std::ptr::NonNull::from(Box::leak(gguf_tap::TapState::new(
                model.model().n_layer() as usize
            )))
        });
        let params = match tap {
            // Safety: the pointer is this engine's own allocation, freed by
            // its `Drop` and by nothing else, and the context that holds it
            // is a field of the same struct, so it cannot outlive it.
            Some(state) => unsafe {
                params.with_eval_callback(
                    Some(gguf_tap::tap),
                    state.as_ptr() as *mut std::ffi::c_void,
                )
            },
            None => params,
        };
        let context = model
            .model()
            .new_context(backend, params)
            .map_err(|error| DecodeFault::Engine {
                detail: format!("new_context: {error}"),
            })?;

        let sampler = Self::chain(knobs, knobs.seed);

        Ok(GgufEngine {
            context,
            sampler,
            logits_at: None,
            closed: false,
            knobs: *knobs,
            tap,
        })
    }

    /// The sampler chain, built from the knobs and one seed.
    ///
    /// **The chain order is the one llama.cpp's own samplers assume**:
    /// penalties, then the truncating filters, then temperature, then the
    /// draw. A distribution sampler last is what makes the seed mean
    /// something.
    ///
    /// One function so the open and the per-generation rebuild cannot
    /// drift: a chain built two ways in two places is two samplers that
    /// agree until someone edits one of them.
    fn chain(knobs: &EffectiveKnobs, seed: u64) -> LlamaSampler {
        LlamaSampler::chain_simple([
            LlamaSampler::penalties(
                knobs.repetition_window as i32,
                knobs.repetition_penalty,
                0.0,
                0.0,
            ),
            LlamaSampler::top_k(knobs.top_k as i32),
            LlamaSampler::top_p(knobs.top_p, 1),
            LlamaSampler::temp(knobs.temperature),
            // **Both halves fold in**, per `weaver-spu-Spec` section 8.5's
            // derivation. This sampler takes a `u32` where the derivation
            // answers a `u64` and the native path consumes the whole of
            // it, so a plain truncation would drop every distinction above
            // bit 31 and leave the two engines drawing from different
            // amounts of one seed. Folding keeps the upper half's
            // contribution in the value that reaches the draw.
            LlamaSampler::dist(((seed >> 32) ^ (seed & 0xFFFF_FFFF)) as u32),
        ])
    }

    /// The tap's state where one stands.
    fn tap_mut(&mut self) -> Option<&mut gguf_tap::TapState> {
        // Safety: the pointer came from `Box::leak`, this struct is its one
        // owner, and the reference borrows `self` so it cannot outlive the
        // engine or coexist with a second one.
        self.tap.map(|state| unsafe { &mut *state.as_ptr() })
    }

    fn engine_fault(detail: impl Into<String>) -> DecodeFault {
        DecodeFault::Engine {
            detail: detail.into(),
        }
    }
}

impl Backend for GgufEngine<'_> {
    /// **Built afresh, and the window restored from the tail**, per
    /// `weaver-spu-Spec` section 8.5.
    ///
    /// The chain accumulates its penalty window from the tokens it
    /// accepts, so a chain built new has an empty one and would leave a
    /// generation's opening tokens unpenalised against what preceded them.
    /// The caller supplies the resident tail the knobs describe and it is
    /// fed straight in, which is the same window read from the state
    /// rather than carried in the sampler. The native path already read
    /// the tail directly, so one source makes the two engines agree by
    /// construction rather than by two implementations staying in step.
    ///
    /// **Nothing is reset.** `LlamaSampler::reset` reaches every sampler
    /// in the chain, which reseeded the draw while clearing the window and
    /// is the coupling this retires. A chain that is built rather than
    /// reset has nothing to clear.
    fn reseed(&mut self, seed: u64, window: &[TokenId]) -> Result<(), DecodeFault> {
        if self.closed {
            return Err(Self::engine_fault("the engine is closed"));
        }
        let mut sampler = Self::chain(&self.knobs, seed);
        sampler.accept_many(window.iter().map(|token| LlamaToken(token.0 as i32)));
        self.sampler = sampler;
        Ok(())
    }

    fn decode_at(&mut self, tokens: &[TokenId], position: usize) -> Result<(), DecodeFault> {
        if self.closed {
            return Err(Self::engine_fault("the engine is closed"));
        }
        if tokens.is_empty() {
            return Ok(());
        }
        let mut batch = LlamaBatch::new(tokens.len(), 1);
        let last = tokens.len() - 1;
        for (offset, token) in tokens.iter().enumerate() {
            // **The addition needs no overflow check and carries none.** The
            // review asked for one, and there is no input that reaches it:
            // offsets run from zero, so the first pass converts `position`
            // itself, and any position large enough to overflow a `usize` when
            // an offset is added has already failed this conversion and
            // returned. A guard here would be a mechanic whose premise the
            // loop's own order removes.
            let pos = i32::try_from(position + offset)
                .map_err(|_| Self::engine_fault("position exceeds the engine's range"))?;
            // **The token converts rather than wrapping.** `sample` below
            // already refuses a value it cannot represent, and a `TokenId` is a
            // `u32`, so the inbound half owes the same posture: `as i32` would
            // turn anything above `i32::MAX` into a negative id and hand it to
            // llama.cpp without a word.
            let id = i32::try_from(token.0)
                .map_err(|_| Self::engine_fault("the token is outside the engine's range"))?;
            // **Logits are asked for on the last token only.** Every earlier
            // token is context being made resident, and asking for its
            // distribution would allocate a vocabulary-wide buffer per token
            // for a reading nothing takes.
            batch
                .add(LlamaToken(id), pos, &[0], offset == last)
                .map_err(|error| Self::engine_fault(format!("batch add: {error}")))?;
        }
        self.context
            .decode(&mut batch)
            .map_err(|error| Self::engine_fault(format!("decode: {error}")))?;
        self.logits_at = Some(last as i32);
        // **The tap's fault is raised by the decode it happened in.** The
        // callback cannot fail the graph without abandoning the rest of it,
        // so the walk latches and this is where an elected readout that
        // stopped observing becomes a fault rather than a shorter reduction.
        if let Some(state) = self.tap_mut() {
            state.commit()?;
        }
        Ok(())
    }

    fn distribution(&self) -> Result<&[f32], DecodeFault> {
        if self.closed {
            return Err(Self::engine_fault("the engine is closed"));
        }
        // **Nothing to read before a decode has left logits standing.** A
        // zeroed buffer would measure as a uniform distribution over the
        // vocabulary, which is a reading rather than an absence.
        let at = self
            .logits_at
            .ok_or_else(|| Self::engine_fault("no decode has left a distribution to read"))?;
        Ok(self.context.get_logits_ith(at))
    }

    fn sample(&mut self) -> Result<TokenId, DecodeFault> {
        if self.closed {
            return Err(Self::engine_fault("the engine is closed"));
        }
        let at = self
            .logits_at
            .ok_or_else(|| Self::engine_fault("no decode has left a distribution to sample"))?;
        let token = self.sampler.sample(&self.context, at);
        // The penalty window is a function of what was drawn, so the sampler is
        // told what it drew.
        self.sampler.accept(token);
        u32::try_from(token.0)
            .map(TokenId)
            .map_err(|_| Self::engine_fault("the engine answered a negative token"))
    }

    fn truncate_to(&mut self, position: usize) -> Result<(), DecodeFault> {
        if self.closed {
            return Err(Self::engine_fault("the engine is closed"));
        }
        let from = i32::try_from(position)
            .map_err(|_| Self::engine_fault("position exceeds the engine's range"))?;
        // Everything at or after the position leaves the cache. The open end is
        // what makes this a truncation rather than a hole.
        //
        // **The engine's answer is read.** `clear_kv_cache_seq` returns
        // `Result<bool, _>` where the error is the argument conversion and the
        // **bool is llama.cpp's own account of whether the removal happened**.
        // Discarding it is how a family that cannot roll back reports a flush it
        // did not perform: `llama_memory_recurrent::seq_rm` refuses a partial
        // erase that includes the final position, because a recurrent state is
        // a running summary and not a per-position cache, and
        // `llama_memory_hybrid::seq_rm` tries the recurrent half first and
        // returns its refusal. Measured on 2026-08-17: a Qwen3.6 artifact
        // answers `false` here while the seam answered `Flushed`.
        let rolled_back = self
            .context
            .clear_kv_cache_seq(Some(0), Some(from as u32), None)
            .map_err(|error| Self::engine_fault(format!("clear_kv_cache_seq: {error}")))?;
        if !rolled_back {
            // Nothing was mutated on this path, so the state is the state
            // before the attempt. It is still a fault: the caller asked for a
            // truncation and did not get one, and the session closing is what
            // keeps a resident length from outrunning the engine's real state.
            return Err(Self::engine_fault(format!(
                "the engine refused to roll back to position {from}, which is a \
                 family whose state cannot be partially erased declaring a \
                 truncating flush"
            )));
        }
        // **The standing distribution belongs to a position that is gone.**
        // Leaving it readable would let a sample after a truncation draw from
        // the state the truncation removed.
        self.logits_at = None;
        // **Nothing is cleared here as of 2026-08-21**, per
        // `weaver-spu-Spec` section 8.5. The penalty window is read from
        // the resident tail when each generation reseeds, so after this
        // rewrite the next read is of the rewritten tail. The reset that
        // stood here cleared the window and reseeded the draw together,
        // llama.cpp's chain reset reaching every sampler in it, which is
        // the coupling that ruling retires.
        Ok(())
    }

    fn reestablish(&mut self) -> Result<(), DecodeFault> {
        if self.closed {
            return Err(Self::engine_fault("the engine is closed"));
        }
        self.context.clear_kv_cache();
        self.logits_at = None;
        // **Nothing is cleared here as of 2026-08-21**, per
        // `weaver-spu-Spec` section 8.5. The penalty window is read from
        // the resident tail when each generation reseeds, so after this
        // rewrite the next read is of the rewritten tail. The reset that
        // stood here cleared the window and reseeded the draw together,
        // llama.cpp's chain reset reaching every sampler in it, which is
        // the coupling that ruling retires.
        Ok(())
    }

    fn take_reduction(&mut self) -> Option<crate::readout::Reduction> {
        self.tap_mut().map(|state| state.drain())
    }

    fn hold_columns(&mut self, hold: bool) {
        if let Some(state) = self.tap_mut() {
            state.set_hold_columns(hold);
        }
    }

    fn take_columns(&mut self) -> Option<Vec<Vec<f32>>> {
        self.tap_mut().and_then(|state| state.take_columns())
    }

    fn close(&mut self) {
        // The context and the sampler release with this value. What close marks
        // is that nothing further may be asked, which every method above reads.
        self.closed = true;
        self.logits_at = None;
    }
}

/// Load a model on the host alone, for the unit suite.
///
/// Test builds only. [`ResidentModel::load`] is the one path a shipped binary
/// has and it takes an [`Admission`], which is the proof that the judgments of
/// Spec section 3 ran. This takes a path and no proof, so it cannot stand in a
/// shipped build, and it puts no layer on a device: what it buys is that the
/// engine's own behaviour is watchable on a machine with no card, which is what
/// the feature comment in `Cargo.toml` says the `gguf` build exists for.
#[cfg(test)]
fn host_only(path: &std::path::Path) -> Result<ResidentModel, DecodeFault> {
    let backend = backend().map_err(|_| DecodeFault::Engine {
        detail: "backend".into(),
    })?;
    let params = LlamaModelParams::default().with_n_gpu_layers(0);
    let model = LlamaModel::load_from_file(backend, path, &params).map_err(|error| {
        DecodeFault::Engine {
            detail: format!("host_only load: {error}"),
        }
    })?;
    Ok(ResidentModel { model })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decoder::session::{
        NeverCancels, PositionedSinks, SamplerBuild, Session, StopCondition,
    };
    use crate::sampling::{Disposition, Knobs, TunableValues};

    /// A small instruct model that loads on the host in about a second.
    const MODEL: &str = "/opt/weaver/models/qwen2.5-0.5b-instruct-q6_k.gguf";

    fn frozen() -> EffectiveKnobs {
        Knobs {
            temperature: Disposition::Frozen(0.7),
            top_k: Disposition::Frozen(40),
            top_p: Disposition::Frozen(0.95),
            repetition_penalty: Disposition::Frozen(1.1),
            repetition_window: Disposition::Frozen(64),
            seed: Disposition::Frozen(11),
        }
        .resolve(&TunableValues::new())
        .expect("nothing is tunable")
    }

    fn engine(model: &ResidentModel) -> GgufEngine<'_> {
        GgufEngine::open(model, &frozen(), 512, false).expect("the context opens")
    }

    /// The fixture these tests decode against.
    ///
    /// **An explicit request that cannot be met is a failure rather than a
    /// skip.** A skip is a pass to the harness, so a green run on a machine
    /// without the fixture reports that the engine was watched decoding when
    /// nothing was decoded. The implicit skip survives only for the machine
    /// that simply does not hold the fixture. This is the convention
    /// `tests/loaded.rs` states and follows, and this file owed it.
    ///
    /// **Its own variable rather than `WEAVER_TEST_GGUF`.** That one points the
    /// load path at any small artifact, because that path is about loading and
    /// not about the model. These tests read a Qwen vocabulary: the terminator
    /// below is `<|im_end|>` at 151645, past the end of a smaller family's
    /// vocabulary, so one variable serving both would answer a load-path
    /// request by failing here as an engine error rather than as the mismatch
    /// it is.
    fn model_or_skip() -> Option<ResidentModel> {
        let path = match std::env::var_os("WEAVER_DECODE_GGUF") {
            Some(named) => {
                let path = std::path::PathBuf::from(named);
                // Both branches demand a regular file, since a directory at the
                // path would fail later and blame the loader.
                assert!(
                    path.is_file(),
                    "WEAVER_DECODE_GGUF names {}, which is not a regular file",
                    path.display()
                );
                path
            }
            None => {
                let path = std::path::Path::new(MODEL);
                if !path.is_file() {
                    eprintln!("SKIP: no model at {MODEL}");
                    return None;
                }
                path.to_path_buf()
            }
        };
        Some(host_only(&path).expect("the model loads on the host"))
    }

    /// **An unelected engine taps nothing**, which is the absence the
    /// backend's default `None` states and this is the GGUF engine honoring
    /// it rather than answering an empty reduction. An empty reduction and no
    /// reduction read the same to a consumer that only counts figures, and
    /// they mean different things.
    #[test]
    fn an_unelected_engine_taps_nothing() {
        let Some(model) = model_or_skip() else { return };
        let mut engine = engine(&model);
        engine
            .decode_at(&[TokenId(9707), TokenId(11), TokenId(1879)], 0)
            .expect("the prompt decodes");
        assert!(
            engine.take_reduction().is_none(),
            "an unelected load answers no reduction at all"
        );
    }

    /// **The tap folds one figure per layer, per decode, against real
    /// weights**, per Spec section 7.
    ///
    /// This is the end-to-end reading of the whole mechanism: the callback
    /// installed at context creation, the name matched against what the
    /// context formatted, the last column copied off a real buffer, and the
    /// count checked against the model's own layer count rather than against
    /// a number written here.
    ///
    /// **The figures must vary.** A tap that read the wrong column, or read
    /// an unwritten buffer, would answer the right count of the wrong thing,
    /// and a run of identical or zero norms is what that looks like. The
    /// residual grows through a transformer's depth, so figures that are
    /// finite, positive, and not all equal is the cheapest reading that a
    /// buffer of zeros and a constant both fail.
    #[test]
    fn the_elected_tap_folds_one_figure_per_layer() {
        let Some(model) = model_or_skip() else { return };
        let layers = model.model().n_layer() as usize;
        assert!(layers > 1, "the fixture must have layers to count");
        let mut engine = GgufEngine::open(&model, &frozen(), 512, true).expect("the context opens");

        engine
            .decode_at(&[TokenId(9707), TokenId(11), TokenId(1879)], 0)
            .expect("the prompt decodes");
        let first = engine.take_reduction().expect("an elected engine taps");
        assert_eq!(
            first.layers(),
            layers,
            "the reduction names the model's own layer count"
        );
        assert_eq!(first.forwards(), 1, "one decode, one forward");
        assert_eq!(first.figures(), layers, "and one figure per layer of it");
        let norms = first.per_layer_norm();
        assert!(
            norms.iter().all(|norm| norm.is_finite() && *norm > 0.0),
            "the figures are the model's: {norms:?}"
        );
        assert!(
            norms.iter().any(|norm| *norm != norms[0]),
            "a constant run means the wrong column: {norms:?}"
        );

        // **The drain empties.** A second decode folds its own set and the
        // reduction handed back holds that set alone, so a generation cannot
        // report figures belonging to the one before it.
        assert_eq!(
            engine.take_reduction().expect("still elected").figures(),
            0,
            "the drain left nothing behind"
        );
        engine
            .decode_at(&[TokenId(1879)], 3)
            .expect("a single token decodes");
        assert_eq!(
            engine.take_reduction().expect("still elected").figures(),
            layers,
            "the decode after the drain folds its own layers alone"
        );

        // **Two forwards into one reduction, which is where the old naming
        // lied.** With a single forward the figure count and the layer count
        // coincide, and every caller happened to be in that case. Draining
        // only after both decodes separates them.
        engine
            .decode_at(&[TokenId(1879)], 4)
            .expect("a token decodes");
        engine
            .decode_at(&[TokenId(1879)], 5)
            .expect("and another, undrained between");
        let both = engine.take_reduction().expect("still elected");
        assert_eq!(both.layers(), layers, "the layer count is the model's");
        assert_eq!(both.forwards(), 2, "two decodes, two forwards");
        assert_eq!(
            both.figures(),
            layers * 2,
            "and the product of the counts is the figure count"
        );
    }

    /// **A prefill longer than the micro-batch still folds one set.**
    ///
    /// The callback fires per ubatch rather than per decode, so a prefill
    /// that splits crosses it several times, and a tap that folded as it
    /// went would answer a multiple of the layer count whose extra figures
    /// belong to positions nothing sampled from. The staging discipline is
    /// what makes this one set, and this is the test that reads it: the
    /// default micro-batch is 512, so a prompt past that splits.
    #[test]
    fn a_split_prefill_folds_one_set_and_not_one_per_ubatch() {
        let Some(model) = model_or_skip() else { return };
        let layers = model.model().n_layer() as usize;
        let capacity = 2048;
        let mut engine =
            GgufEngine::open(&model, &frozen(), capacity, true).expect("the context opens");

        // Past the 512 micro-batch and inside the capacity above.
        let prompt: Vec<TokenId> = (0..600).map(|_| TokenId(1879)).collect();
        engine.decode_at(&prompt, 0).expect("the prompt decodes");

        assert_eq!(
            engine
                .take_reduction()
                .expect("an elected engine taps")
                .layers(),
            layers,
            "a split prefill folds the layer count once, not once per ubatch"
        );
    }

    /// **The figures are the last position's, whatever the batching.**
    ///
    /// The count tests above would pass a tap reading any column, and the
    /// column is the one thing about this tap that cannot be read off the
    /// name. So this reads it by a route that has no column to choose: a
    /// single-token decode has exactly one, and it must agree with the same
    /// position taken as the tail of a batched prefill.
    ///
    /// Perturbation: taking column zero, or `nb[1]` as the offset instead of
    /// the stride, diverges the two here while every count stays right.
    ///
    /// The tolerance is relative and loose because a batched matmul and a
    /// single-row one accumulate in different orders on this backend. What
    /// is being told apart is the same position from a different one, which
    /// is a difference of tens of percent rather than of ulps.
    #[test]
    fn the_figures_belong_to_the_last_position_however_it_was_batched() {
        let Some(model) = model_or_skip() else { return };
        let prompt = [TokenId(9707), TokenId(11), TokenId(1879), TokenId(0)];

        let mut batched =
            GgufEngine::open(&model, &frozen(), 512, true).expect("the context opens");
        batched.decode_at(&prompt, 0).expect("the prompt decodes");
        let batched = batched
            .take_reduction()
            .expect("elected")
            .per_layer_norm()
            .to_vec();

        // The same last position, reached one token at a time, so the tap has
        // one column to take and cannot have taken the wrong one.
        let mut singly = GgufEngine::open(&model, &frozen(), 512, true).expect("the context opens");
        for (position, token) in prompt.iter().enumerate() {
            singly
                .decode_at(&[*token], position)
                .expect("a single token decodes");
            // Everything but the last is context, and its figures are not
            // what this compares.
            if position + 1 < prompt.len() {
                singly.take_reduction();
            }
        }
        let singly = singly
            .take_reduction()
            .expect("elected")
            .per_layer_norm()
            .to_vec();

        assert_eq!(
            batched.len(),
            singly.len(),
            "the same layer count either way"
        );
        for (layer, (a, b)) in batched.iter().zip(&singly).enumerate() {
            let apart = (a - b).abs() / a.max(*b);
            assert!(
                apart < 0.05,
                "layer {layer} reads {a} batched and {b} singly, \
                 which is a different position rather than a different order"
            );
        }
    }

    /// **The tap changes no token**, watched on this backend.
    ///
    /// `weaver-spu-PRD` section 13.7 obliges an elected readout to change no
    /// token: the same declaration and the same seed produce the same token
    /// sequence with the election on and off. **What this test buys and what
    /// it does not**: it reads the property on the host backend, where the
    /// scheduler has one backend and the windowing the callback forces costs
    /// synchronisation and nothing else. The hazard the Spec names is a
    /// device one, a fusion candidate straddling a window boundary going
    /// unapplied and the unfused kernel not being bit-identical to the
    /// fused, and no CPU run can reach it. The charter's bar is therefore a
    /// measurement against the real artifact on the real device pair, taken
    /// in `tests/readout_neutral.rs` and kept beside this rather than
    /// replacing it: a failure there against a pass here localises a defect
    /// to the device path immediately. What this buys is the whole of the rest: that the
    /// tap mutates no engine state, consumes nothing from the sampler, and
    /// perturbs no arithmetic that the host path shares with the device one.
    ///
    /// Perturbation: making the tap accept the token it observed, or drawing
    /// from the sampler inside the callback, diverges the two runs here.
    #[test]
    fn the_tap_changes_no_token_on_this_backend() {
        let Some(model) = model_or_skip() else { return };
        let prompt = [TokenId(9707), TokenId(11), TokenId(1879)];

        let draw = |readout: bool| {
            let mut engine =
                GgufEngine::open(&model, &frozen(), 512, readout).expect("the context opens");
            engine.decode_at(&prompt, 0).expect("the prompt decodes");
            let mut drawn = Vec::new();
            for step in 0..16 {
                let token = engine.sample().expect("a token is drawn");
                drawn.push(token);
                engine
                    .decode_at(&[token], prompt.len() + step)
                    .expect("the draw decodes");
            }
            drawn
        };

        let without = draw(false);
        let with = draw(true);
        assert_eq!(
            with, without,
            "the elected run drew a different sequence, so the tap is not observational"
        );
        // The comparison is only worth something if the elected run tapped.
        let mut elected =
            GgufEngine::open(&model, &frozen(), 512, true).expect("the context opens");
        elected.decode_at(&prompt, 0).expect("the prompt decodes");
        assert!(
            elected.take_reduction().is_some_and(|r| r.layers() > 0),
            "the elected run observed nothing, so the comparison is vacuous"
        );
    }

    /// **The engine decodes a prompt and draws a token from it.** The first
    /// end-to-end exercise of this seam in the program: real weights, a real
    /// forward, a real draw.
    ///
    /// What it reads is that the distribution is the model's rather than a
    /// buffer of zeros. A vocabulary-wide slice of equal values would satisfy a
    /// length check and mean nothing, so the assertion is that the logits vary
    /// and that the token drawn is inside the vocabulary.
    #[test]
    fn the_engine_decodes_and_draws() {
        let Some(model) = model_or_skip() else { return };
        let mut engine = engine(&model);

        engine
            .decode_at(&[TokenId(9707), TokenId(11), TokenId(1879)], 0)
            .expect("the prompt decodes");

        // **The borrow ends before the draw, and the compiler is what says
        // so.** `distribution` borrows and `sample` needs the engine mutably,
        // so a reading held across the draw does not compile. That is the
        // pre-sampler ordering of Spec section 6 enforced by the seam's own
        // signatures rather than by a rule the caller remembers.
        let (width, varies, finite) = {
            let logits = engine.distribution().expect("a distribution stands");
            let first = logits[0];
            (
                logits.len(),
                logits.iter().any(|l| (l - first).abs() > 1e-3),
                logits.iter().all(|l| l.is_finite()),
            )
        };
        assert!(width > 1000, "a vocabulary, got {width}");
        assert!(varies, "the distribution varies rather than being zeroed");
        assert!(finite, "no logit is NaN");

        let token = engine.sample().expect("a token is drawn");
        assert!(
            (token.0 as usize) < width,
            "the draw is inside the vocabulary, got {token:?}"
        );
    }

    /// **A session over this engine generates.** The loop of `session.rs`
    /// driving the engine of this module against real weights, which is the
    /// whole of the decode path below the seam and the first time in this
    /// program that it runs.
    ///
    /// What it reads is that tokens come back, that the measurement is
    /// positionally paired with them, and that the terminator landed: the
    /// resident length exceeds what was decoded by the one slot the terminator
    /// takes, which is the property section 4.2 holds on every path.
    #[test]
    fn a_session_over_the_engine_generates() {
        let Some(model) = model_or_skip() else { return };
        let mut session = Session::new(
            Box::new(engine(&model)),
            512,
            crate::decoder::backend::FlushMechanism::TruncateToPosition,
            false,
        );

        // A short identity prefix, then a turn's delta.
        session
            .open(&[TokenId(9707), TokenId(11)])
            .expect("the prefix goes resident");
        let before = session.resident_len();
        assert_eq!(before, 2, "the prefix is what was decoded");

        // The stream counter reads the real engine's callback: it fires once
        // per retained token, so the count equals the tokens the close carries.
        let mut streamed = 0usize;
        let generated = session
            .append_and_generate(
                &[TokenId(1879)],
                &StopCondition {
                    stop_tokens: vec![],
                    terminator: TokenId(151645),
                    max_tokens: 8,
                },
                &mut NeverCancels,
                &mut |_| streamed += 1,
                PositionedSinks {
                    field: None,
                    on_column: &mut |_, _| {},
                },
                SamplerBuild {
                    seed: 11,
                    penalty_window: 64,
                },
            )
            .expect("the generation runs");

        assert_eq!(generated.tokens.len(), 8, "the ceiling bounds the run");
        assert_eq!(
            streamed,
            generated.tokens.len(),
            "the real engine streamed once per retained token"
        );
        assert_eq!(
            generated.signals.steps(),
            generated.tokens.len(),
            "one measurement per retained token, positionally paired"
        );

        // **The terminator landed.** Two prefix, one delta, eight generated,
        // one terminator.
        assert_eq!(
            session.resident_len(),
            2 + 1 + 8 + 1,
            "the terminator is resident beyond what was drawn"
        );

        let entropies = generated
            .signals
            .entropy_bits
            .as_ref()
            .expect("the run measured");
        assert!(
            entropies
                .iter()
                .all(|bits| bits.is_finite() && *bits >= 0.0),
            "every entropy is a finite non-negative number of bits"
        );
    }

    /// **Nothing may be read before a decode has left a distribution.**
    ///
    /// Perturbation: return `get_logits_ith(0)` unconditionally rather than
    /// keying on `logits_at` and this passes while reading a buffer no forward
    /// filled, which measures as a uniform distribution over the vocabulary.
    /// Watched under exactly that change.
    #[test]
    fn a_read_before_any_decode_refuses() {
        let Some(model) = model_or_skip() else { return };
        let mut engine = engine(&model);
        assert!(engine.distribution().is_err(), "nothing has been decoded");
        assert!(engine.sample().is_err(), "and nothing may be drawn");
    }

    /// **A truncation takes the standing distribution with the positions it
    /// removed**, so a draw afterwards cannot come from state that is gone.
    ///
    /// Perturbation: leave `logits_at` set in `truncate_to` and this fails,
    /// the sample succeeding against a position the cache no longer holds.
    /// Watched under exactly that change.
    #[test]
    fn a_truncation_drops_the_standing_distribution() {
        let Some(model) = model_or_skip() else { return };
        let mut engine = engine(&model);
        engine
            .decode_at(&[TokenId(9707), TokenId(11)], 0)
            .expect("decode");
        assert!(engine.distribution().is_ok(), "a distribution stands");

        engine.truncate_to(1).expect("the truncation runs");
        assert!(
            engine.distribution().is_err(),
            "the reading went with the positions"
        );
    }

    /// **A capacity of zero refuses rather than being substituted.**
    ///
    /// `NonZeroU32::new(0)` is `None` and llama.cpp reads an absent `n_ctx` as
    /// the model's own training context, so without the guard a caller asking
    /// for nothing is served the whole window and the session above holds a
    /// capacity the context does not have.
    ///
    /// Perturbation: pass the capacity through unchecked and this fails, the
    /// open succeeding and reporting a context far larger than was asked for.
    /// Watched under exactly that change.
    #[test]
    fn a_zero_capacity_refuses() {
        let Some(model) = model_or_skip() else { return };
        assert!(
            GgufEngine::open(&model, &frozen(), 0, false).is_err(),
            "a session that can hold nothing is refused at the open"
        );
        assert!(
            GgufEngine::open(&model, &frozen(), 1, false).is_ok(),
            "a positive capacity still opens"
        );
    }

    /// **A token the engine cannot represent refuses by name.** `sample`
    /// already refuses a value it cannot represent, and a `TokenId` is a
    /// `u32`, so the inbound half owes the same posture.
    ///
    /// **The refusal is read rather than the failure**, because both the guard
    /// and its absence end in a refusal and only the reason separates them.
    /// Wrapping to a negative id fails later inside llama.cpp's batch, so an
    /// `is_err` assertion passes with the guard gone and watches nothing.
    ///
    /// Perturbation: restore `token.0 as i32` and this fails, the refusal
    /// arriving as `batch add` from the engine rather than naming the token.
    /// Watched under exactly that change.
    #[test]
    fn an_unrepresentable_token_refuses_by_name() {
        let Some(model) = model_or_skip() else { return };
        let mut engine = engine(&model);
        let refusal = engine
            .decode_at(&[TokenId(u32::MAX)], 0)
            .expect_err("a token above i32::MAX is refused");
        let DecodeFault::Engine { detail } = refusal else {
            panic!("the refusal is an engine fault, got {refusal:?}");
        };
        assert!(
            detail.contains("the token is outside the engine's range"),
            "the refusal names the token rather than the batch, got {detail:?}"
        );
    }

    /// **Two seeds differing only above bit 31 reach the draw as two
    /// seeds.** This sampler takes a `u32` where the derivation answers a
    /// `u64`, so a plain truncation would make `0x0000_0000_0000_0001` and
    /// `0xFFFF_FFFF_0000_0001` one seed - and the native path, which
    /// consumes the whole `u64`, would disagree with it. The fold is what
    /// keeps the upper half in the value that draws.
    ///
    /// Read against the arithmetic rather than against a device, so it
    /// runs without a model: what is under test is the narrowing, and the
    /// narrowing is a pure function.
    ///
    /// Perturbation: write `seed as u32` and the two collapse.
    #[test]
    fn both_halves_of_the_derived_seed_reach_the_draw() {
        let narrow = |seed: u64| ((seed >> 32) ^ (seed & 0xFFFF_FFFF)) as u32;
        assert_ne!(
            narrow(0x0000_0000_0000_0001),
            narrow(0xFFFF_FFFF_0000_0001),
            "seeds differing only above bit 31 must not collapse"
        );
        // The derivation's own neighbours, which is where this matters.
        let turn = weaver_types::TurnKey("t-1".to_string());
        let a = crate::sampling::derived_seed(11, &turn, 0);
        let b = crate::sampling::derived_seed(11, &turn, 1);
        assert_ne!(narrow(a), narrow(b), "adjacent generations stay apart");
        // And the fold is not lossy in the ordinary direction: a value
        // whose upper half is zero passes through unchanged, so nothing
        // that worked before this act now narrows differently.
        assert_eq!(narrow(0x0000_0000_DEAD_BEEF), 0xDEAD_BEEF);
    }

    /// **A reseed restores the window from the tail it is handed**, per
    /// `weaver-spu-Spec` section 8.5, which is what makes a flush harmless
    /// to sampling rather than something a flush has to repair.
    ///
    /// What this reads is that the seed still means something across a
    /// rewrite: the same resident state, reseeded with the same seed and
    /// the same window, draws the same token. It replaces a test that read
    /// the same property through `forget_sampled`, which delivered it by
    /// clearing the window and reseeding the draw together - the coupling
    /// that ruling retires.
    ///
    /// Perturbation: drop the `accept_many` from `reseed` and this fails,
    /// the second draw seeing an empty window where the first saw a full
    /// one.
    #[test]
    fn a_reseed_restores_the_window_it_is_handed() {
        let Some(model) = model_or_skip() else { return };
        let mut engine = engine(&model);
        let prefix = [TokenId(9707), TokenId(11), TokenId(1879)];
        let window = [TokenId(9707), TokenId(11)];

        engine.decode_at(&prefix, 0).expect("the prefix decodes");
        engine.reseed(4242, &window).expect("the first reseed");
        let first = engine.sample().expect("a token is drawn");

        // Rewrite the cache and come back to the same resident state.
        engine.reestablish().expect("the rewrite runs");
        engine
            .decode_at(&prefix, 0)
            .expect("the prefix decodes again");
        engine.reseed(4242, &window).expect("the second reseed");
        let after = engine.sample().expect("a token is drawn");

        assert_eq!(
            first, after,
            "one seed and one window draw one token, whatever happened to \
             the cache between them"
        );
    }

    /// **A closed engine answers every ask with a refusal**, rather than a
    /// stale value from the state it held.
    #[test]
    fn a_closed_engine_serves_nothing() {
        let Some(model) = model_or_skip() else { return };
        let mut engine = engine(&model);
        engine.decode_at(&[TokenId(9707)], 0).expect("decode");
        engine.close();

        assert!(engine.distribution().is_err());
        assert!(engine.sample().is_err());
        assert!(engine.decode_at(&[TokenId(11)], 1).is_err());
        assert!(engine.truncate_to(0).is_err());
        assert!(engine.reestablish().is_err());
    }
}
