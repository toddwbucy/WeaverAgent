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
use llama_cpp_2::model::{LlamaLoraAdapter, LlamaModel};
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::token::LlamaToken;

use crate::decoder::backend::{Backend, DecodeFault, TokenId};
use crate::decoder::gguf_tap;
use crate::sampling::EffectiveKnobs;
use llama_cpp_2::model::params::{LlamaModelParams, LlamaSplitMode};

use crate::division::Division;
use crate::residency::{Admission, AdmitRefusal};

/// The one llama.cpp backend this process holds.
///
/// `LlamaBackend::init` sets up global engine state and may be called once per
/// process, so it lives behind a `OnceLock`, the same one-owner discipline the
/// archived tree's own note records against itself. A failure is held as the
/// failure, so a second admit attempt reports the same fact instead of
/// re-initializing global state that half-exists.
static BACKEND: OnceLock<Result<LlamaBackend, String>> = OnceLock::new();

pub(crate) fn backend_started() -> bool {
    BACKEND.get().is_some()
}

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
    // LORA-5: this inert handle drops before the model that owns its allocation.
    // Boxed, so an adapted residency does not widen `LoadedModel`'s variant.
    adapter: Option<Box<ResidentAdapter>>,
    model: LlamaModel,
}

struct ResidentAdapter {
    handle: std::cell::RefCell<LlamaLoraAdapter>,
    /// The record's statement of this adapter (ADP-3), built once at load:
    /// the path the request named, never compared, and the identity's digest
    /// and scale.
    record: crate::lora::RecordedAdapter,
    identity: crate::lora::Identity,
    placement: std::collections::BTreeMap<String, weaver_types::DeviceOrdinal>,
}

impl ResidentModel {
    /// Load the admission's artifact onto the admission's devices.
    ///
    /// The devices are the binding's and this crate selects none. **Under the
    /// PAIR-1 mask they are positions**: the process sees exactly the
    /// binding's devices, renumbered from zero in binding order, and the
    /// admission judged the mask before any device query. One device loads
    /// with no split. A wider binding splits by layer across positions 0 to
    /// N - 1 with the PAIR-3 tensor split, never the engine's free-memory
    /// default.
    ///
    /// **An assumption crosses here and is named rather than silent.**
    /// `with_devices` indexes ggml's registered-device enumeration, and under
    /// the mask the admission's position i is CUDA's device i. The two
    /// numberings coincide while the libllama this build links registers
    /// CUDA devices first and in CUDA's order, which holds for the library
    /// the pinned build script compiles. A deployment linking a libllama with
    /// another backend registered ahead of CUDA, RPC or Vulkan or SYCL,
    /// breaks the coincidence and puts weights on a device the admission
    /// never judged.
    pub fn load(admission: &Admission<'_>) -> Result<ResidentModel, AdmitRefusal> {
        use crate::lora::observation;
        // LORA-3: refuse absent observation before any backend allocation.
        if let Some(prepared) = admission.adapter() {
            observation::with_records(|_| ()).map_err(|detail| {
                AdmitRefusal::Adapter(prepared.failure(
                    crate::lora::FailureKind::Load,
                    "adapter-observer",
                    detail,
                ))
            })?;
        }
        // ADP-2: the placement window opens at load. A failed load ends the
        // judgment, and the first session's context ends it otherwise.
        observation::open_window();
        let loaded = Self::load_observed(admission);
        if loaded.is_err() {
            observation::close_window();
        }
        loaded
    }

    fn load_observed(admission: &Admission<'_>) -> Result<ResidentModel, AdmitRefusal> {
        let backend = backend()?;
        let devices = admission.devices();
        let division = admission
            .division()
            .ok_or_else(|| AdmitRefusal::LoadFailed {
                detail: "a GGUF admission carries no layer division".into(),
            })?;
        let mut params =
            placement_params(division).map_err(|detail| AdmitRefusal::LoadFailed { detail })?;
        if devices.len() > 1 {
            let positions: Vec<usize> = (0..devices.len()).collect();
            params = params
                .with_devices(&positions)
                .map_err(|error| AdmitRefusal::LoadFailed {
                    detail: format!("device positions {positions:?}: {error}"),
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
        // LORA-3: load after the base. A failed observer drops both through
        // the model owner. A successful handle alone is not qualification.
        let adapter = if let Some(prepared) = admission.adapter() {
            use crate::lora::{FailureKind, observation};
            let failure = |detail| {
                AdmitRefusal::Adapter(prepared.failure(FailureKind::Load, "adapter-load", detail))
            };
            let start = observation::with_records(<[_]>::len).map_err(&failure)?;
            // The capture's lock is not held across the engine call, whose
            // log lines the capture takes.
            let handle = model
                .lora_adapter_init(prepared.path())
                .map_err(|error| failure(format!("lora_adapter_init: {error}")))?;
            let placement = observation::with_records(|records| {
                observation::judge(
                    records.get(start..).unwrap_or_default(),
                    &prepared.identity.targets,
                    devices,
                    division,
                )
            })
            .and_then(|judged| judged)
            .map_err(&failure)?;
            Some(Box::new(ResidentAdapter {
                handle: std::cell::RefCell::new(handle),
                record: crate::lora::RecordedAdapter {
                    artifact: prepared.artifact().display().to_string(),
                    sha256: prepared.identity.sha256.clone(),
                    scale: prepared.identity.scale,
                },
                identity: prepared.identity.clone(),
                placement,
            }))
        } else {
            None
        };
        Ok(ResidentModel { adapter, model })
    }

    /// The adapter as the record states it (ADP-3), `None` for a base-only
    /// load.
    pub fn adapter_record(&self) -> Option<&crate::lora::RecordedAdapter> {
        self.adapter.as_ref().map(|adapter| &adapter.record)
    }

    pub fn adapter_identity(&self) -> Option<&crate::lora::Identity> {
        self.adapter.as_ref().map(|a| &a.identity)
    }

    /// Each adapter target's observed device, by binding ordinal.
    pub fn adapter_placement(
        &self,
    ) -> Option<&std::collections::BTreeMap<String, weaver_types::DeviceOrdinal>> {
        self.adapter.as_ref().map(|a| &a.placement)
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

/// **The model parameters every GGUF load sets before its device list**, per
/// the device pair amendment PAIR-3.
///
/// Every layer is offloaded: a partial offload would leave part of the model
/// in host memory, a residency the admission never judged, and the PAIR-3
/// mapping holds only with at least L + 1 layers offloaded, because the
/// engine's first offloaded slot is L + 1 minus the count. `u32::MAX` crosses
/// as `i32::MAX`.
///
/// One device loads with split mode none, where `main_gpu` selects the
/// device among those visible; under the mask position 0 is the only one. A
/// wider binding loads with layer split mode and the division's tensor split,
/// where `main_gpu` has no effect. The split is never empty for a
/// multi-device set, because an empty split restores the engine's
/// free-memory default.
///
/// One defect stays on the pinned fork and upstream: `add_cpu_buft_override`
/// stores its pattern's pointer with no lifetime tied to the parameters, so a
/// pattern freed before load dangles. Nothing here calls it, and any later
/// use of buffer-type overrides must fix it first.
pub(crate) fn placement_params(division: &Division) -> Result<LlamaModelParams, String> {
    let params = LlamaModelParams::default()
        .with_n_gpu_layers(u32::MAX)
        .with_main_gpu(0);
    if division.counts().len() == 1 {
        return Ok(params.with_split_mode(LlamaSplitMode::None));
    }
    params
        .with_split_mode(LlamaSplitMode::Layer)
        .with_tensor_split(&division.split_values())
        .map_err(|error| format!("tensor split {:?}: {error}", division.split_values()))
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
        let context_result =
            model
                .model()
                .new_context(backend, params)
                .map_err(|error| DecodeFault::Engine {
                    detail: format!("new_context: {error}"),
                });
        // ADP-2: the first session's cache records are in, and the placement
        // window closes. Every later open finds it closed.
        crate::lora::observation::close_window();
        let context = match context_result {
            Ok(context) => context,
            Err(error) => {
                if let Some(state) = tap {
                    // No context survived to retain this allocation.
                    drop(unsafe { Box::from_raw(state.as_ptr()) });
                }
                return Err(error);
            }
        };

        // LORA-4: attach before the caller can decode even one prefix token.
        // Every recreated context comes through this same open operation.
        let context = match crate::lora::attach_before_use(
            context,
            model.adapter.as_ref(),
            |context, adapter| {
                context.lora_adapter_set(&mut adapter.handle.borrow_mut(), adapter.identity.scale)
            },
        ) {
            Ok(context) => context,
            Err(error) => {
                if let Some(state) = tap {
                    // LORA-5: destroy the failed context before its callback state.
                    drop(unsafe { Box::from_raw(state.as_ptr()) });
                }
                return Err(Self::engine_fault(format!(
                    "adapter attachment failed: {error}"
                )));
            }
        };

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
        // **The draw is accepted once, by the engine**, per `weaver-spu-Spec`
        // section 8.5: the window is the last `repetition_window` resident
        // positions, each counted once. `llama_sampler_sample` accepts the
        // token it returns before returning it, so the chain's penalty ring
        // already holds this draw. A second accept here put every draw in
        // the ring twice, which left a ring of 64 covering the last 32 drawn
        // tokens where the native path reads 64 positions.
        let token = self.sampler.sample(&self.context, at);
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
// Test-only hook kept here (#118): the host-only load the gguf suite drives.
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
    Ok(ResidentModel {
        adapter: None,
        model,
    })
}

#[cfg(test)]
mod tests;
