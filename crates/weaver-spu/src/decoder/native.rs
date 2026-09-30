//! conforms: spu-native-refuses-an-unserved-family-as-a-family
//!
//! The candle-native backend, per `weaver-spu-Spec` section 4.1.
//!
//! **The second peer, standing.** GGUF owns quantized artifacts on consumer
//! devices, and this path owns what a tensor-parallel forward and a
//! fine-tunable artifact need, since a GGUF cannot be fine-tuned and a program
//! that intends training as a continuation cannot let that path decay. The
//! model runs through the pinned candle fork, which is the readout's working
//! path: `forward_with_intermediates` is only a route to per-layer activations
//! if candle runs the forward, so serving through candle is what keeps the
//! readout election honorable when its act arrives.
//!
//! **The native path serves qwen2 on one device or a pair.** The single-device
//! forward is `candle_transformers::models::qwen2`, and the pair uses
//! `native_pair::ShardedModel` with its host-staged reductions. A binding
//! naming any other width refuses by name.
//!
//! **The resident model is pristine and the engine decodes against a clone.**
//! Candle's model holds its KV cache inside the model value, so a session's
//! state would otherwise live in the residency and survive the session. The
//! clone is cheap, the weight tensors sharing storage underneath, and it is
//! what makes `close` true: dropping the engine drops the session's state and
//! nothing else.
//!
//! **Truncation is reached by re-decoding the retained prefix.** The family
//! declares `TruncateToPosition` and the fork's cache exposes clear and
//! nothing finer, so this engine retains what it decoded and reaches the
//! truncated state by clearing and re-decoding the front of it. The outcome
//! is a true truncation, the resident state after holding exactly the first
//! `position` tokens, and the cost is a prefill the GGUF engine does not pay.
//! What Spec section 4.4 forbids is a truncation that returns success while
//! recurrent state stays, and none stays here. A cache that can narrow is
//! fork work for a later act, and this paragraph is what it would buy.

use std::path::Path;

use candle_core::{DType, Device, Tensor};
use candle_transformers::generation::{LogitsProcessor, Sampling};
use candle_transformers::models::qwen2::{Config, ModelForCausalLM};

use super::backend::{Backend, DecodeFault, TokenId};
use crate::family::{FamilyName, FamilyRefusal, same_key};
use crate::residency::{Admission, AdmitRefusal};
use crate::sampling::EffectiveKnobs;

/// The weights resident on the device, held by the residency.
///
/// Holding it is the residency: the tensors live on the admitted device and
/// dropping this frees them, so the release ordering is by construction, the
/// same property the GGUF peer states.
/// The forward this residency holds: one device's whole model, or two
/// devices' halves. One resident kind, because the seam above cares which
/// artifact is resident and not how many cards hold it.
#[derive(Clone)]
pub(crate) enum Forward {
    Single(ModelForCausalLM),
    Pair(super::native_pair::ShardedModel),
}

pub struct ResidentModel {
    model: Forward,
    tokenizer: tokenizers::Tokenizer,
    /// The artifact's declared end-of-sequence, `config.json`'s
    /// `eos_token_id`, retained at load because the stop set's backstop
    /// reads it after the sidecars are out of hand. The scalar is the
    /// artifact's declaration the way GGUF's `token_eos` is: where a
    /// generation config names several, the config's own scalar is the
    /// declared one and the family's promoted conditions carry the rest.
    eos: super::backend::TokenId,
    /// The admitted device's handle, held once at load: the tensors know
    /// where they live, but candle carries no model-level accessor, and
    /// reconstructing the handle per session would be a second account of a
    /// fact this struct already witnessed.
    device: Device,
}

impl ResidentModel {
    /// Load the admission's artifact onto the admission's device.
    ///
    /// The artifact is the directory the resolution step found the container
    /// in: the weights are the container's, the shapes are `config.json`'s,
    /// and the vocabulary is `tokenizer.json`'s, all read from beside the
    /// container because a safetensors export is a directory-shaped artifact
    /// and its parts do not travel inside one file the way a GGUF's do.
    pub fn load(admission: &Admission<'_>) -> Result<ResidentModel, AdmitRefusal> {
        let devices = admission.devices();
        // One device or a pair. A wider set refuses by name: the width
        // election of Spec section 11 holds an N-way forward as future work.
        let ordinals: Vec<u32> = devices.iter().map(|d| d.0).collect();
        if ordinals.len() > 2 {
            return Err(AdmitRefusal::LoadFailed {
                detail: format!(
                    "the native path serves one device or two and the binding names {}",
                    ordinals.len()
                ),
            });
        }
        // **The family is judged before any device is opened.** A family this
        // peer does not serve is not a condition of the card, so a busy or
        // absent device must not answer for it: opening first would return
        // `LoadFailed` and cross as `DeviceCannotAdmit`, which is the report
        // issue #507 was filed against, for an artifact whose real fault the
        // free read already knew.
        let dir = sidecar_dir(admission.path())?;
        let config_path = dir.join("config.json");
        let declared = read_declaration(&config_path)?;
        judge_family(&declared).map_err(AdmitRefusal::Family)?;

        let device =
            Device::new_cuda(ordinals[0] as usize).map_err(|error| AdmitRefusal::LoadFailed {
                detail: format!("cuda device {}: {error}", ordinals[0]),
            })?;

        let config = read_config(&config_path)?;
        let eos = read_eos(&config_path)?;
        let tokenizer =
            tokenizers::Tokenizer::from_file(dir.join("tokenizer.json")).map_err(|error| {
                AdmitRefusal::LoadFailed {
                    detail: format!("tokenizer.json: {error}"),
                }
            })?;

        let model = if let [_, second] = ordinals.as_slice() {
            let second =
                Device::new_cuda(*second as usize).map_err(|error| AdmitRefusal::LoadFailed {
                    detail: format!("cuda device {second}: {error}"),
                })?;
            Forward::Pair(super::native_pair::ShardedModel::load(
                admission.paths(),
                &config,
                [device.clone(), second],
            )?)
        } else {
            // BF16 on the device, which is what the artifact holds. The mmap
            // is unsafe by the crate's own signature: the file must not
            // change underneath the map, the standing assumption every
            // reader of a pinned artifact makes. Built here rather than
            // above, because the pair branch maps the container itself and
            // an unconsumed map would be work the load never uses.
            let vb = unsafe {
                candle_nn::VarBuilder::from_mmaped_safetensors(
                    admission.paths(),
                    DType::BF16,
                    &device,
                )
            }
            .map_err(|error| AdmitRefusal::LoadFailed {
                detail: format!("safetensors map: {error}"),
            })?;
            Forward::Single(ModelForCausalLM::new(&config, vb).map_err(|error| {
                AdmitRefusal::LoadFailed {
                    detail: format!("model construction: {error}"),
                }
            })?)
        };

        Ok(ResidentModel {
            model,
            tokenizer,
            device,
            eos,
        })
    }

    /// The artifact's declared end-of-sequence.
    pub(crate) fn declared_eos(&self) -> super::backend::TokenId {
        self.eos
    }

    /// Tokenize against the artifact's own vocabulary.
    pub(crate) fn tokenize(&self, text: &str) -> Result<Vec<TokenId>, DecodeFault> {
        let encoding = self
            .tokenizer
            .encode(text, false)
            .map_err(|error| DecodeFault::Engine {
                detail: format!("encode: {error}"),
            })?;
        Ok(encoding.get_ids().iter().map(|&id| TokenId(id)).collect())
    }

    /// Render token ids back to text.
    pub(crate) fn detokenize(&self, tokens: &[TokenId]) -> Result<String, DecodeFault> {
        let ids: Vec<u32> = tokens.iter().map(|token| token.0).collect();
        self.tokenizer
            .decode(&ids, false)
            .map_err(|error| DecodeFault::Engine {
                detail: format!("decode: {error}"),
            })
    }
}

/// The directory the artifact's sidecar files live in.
///
/// The admission's path is the pin's `/proc/self/fd/N` on purpose, so the
/// real location is recovered by asking the kernel what it currently calls
/// the pinned inode, the same recovery the header read makes and for the
/// same stated limit: a sidecar is an open by name and cannot ride the
/// descriptor.
fn sidecar_dir(path: &Path) -> Result<std::path::PathBuf, AdmitRefusal> {
    let real = std::fs::read_link(path).unwrap_or_else(|_| path.to_path_buf());
    real.parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| AdmitRefusal::LoadFailed {
            detail: format!("{} has no parent directory", real.display()),
        })
}

/// The declared end-of-sequence from `config.json`, scalar or the first of a
/// list, refused where the file declares none: an artifact with no declared
/// end would leave the stop set without its backstop, which is a fact to
/// refuse at admit rather than discover mid-generation.
fn read_eos(path: &Path) -> Result<super::backend::TokenId, AdmitRefusal> {
    let text = std::fs::read_to_string(path).map_err(|error| AdmitRefusal::LoadFailed {
        detail: format!("{}: {error}", path.display()),
    })?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|error| AdmitRefusal::LoadFailed {
            detail: format!("{}: {error}", path.display()),
        })?;
    let eos = value.get("eos_token_id");
    eos.and_then(|v| v.as_u64())
        .or_else(|| {
            eos.and_then(|v| v.as_array())
                .and_then(|list| list.first())
                .and_then(|v| v.as_u64())
        })
        .ok_or_else(|| AdmitRefusal::LoadFailed {
            detail: format!("{}: no eos_token_id declared", path.display()),
        })
        .and_then(|id| {
            // The declared id must fit the wire's token width. A cast would
            // truncate a value above the ceiling into a different, valid
            // token id, the silent substitution the count discipline of
            // `sampling.rs` refuses, so the conversion is checked and an
            // oversized declaration refuses naming itself.
            u32::try_from(id)
                .map(super::backend::TokenId)
                .map_err(|_| AdmitRefusal::LoadFailed {
                    detail: format!(
                        "{}: eos_token_id {id} exceeds the token width",
                        path.display()
                    ),
                })
        })
}

/// The architecture this path's forward is written against, per the module's
/// stage-one clause: the registry's qwen2 entry.
pub const SERVED_ARCHITECTURE: &str = "qwen2";

/// This peer's name in a refusal, so the account says which of the two was
/// asked rather than reporting the binary as carrying nothing.
pub const NATIVE_BACKEND: &str = "native";

/// The architecture an artifact's `config.json` declares, spelled as the file
/// spells it.
///
/// `model_type` is the field a stock export carries. **A config carrying no
/// `model_type`, or one that is not a string, declares no family this path
/// can read**, and is judged by nothing here: the shapes parse as before, so
/// such an artifact is refused by the field it lacks or the type it got
/// wrong rather than by a family it never named. A number where a family
/// name belongs is a malformed declaration and not an unserved family, and
/// the shape error names the field and its line.
fn declared_architecture(config: &serde_json::Value) -> Option<&str> {
    config.get("model_type")?.as_str()
}

/// Judge the family before the shapes, per `weaver-spu-Spec` section 4.1.
///
/// **The refusal says this backend does not serve the family and never that
/// the binary does not carry it**, the registry carrying qwen3 and the rest
/// and serving them through the GGUF peer. A reader told the family is
/// unknown goes looking for a registry row that is present.
///
/// **It carries the spelling the file used and never the fold of it**, which
/// is [`same_key`]'s own rule, so an operator reads back the family their
/// artifact declared.
fn judge_family(config: &serde_json::Value) -> Result<(), FamilyRefusal> {
    match declared_architecture(config) {
        Some(declared) if !same_key(declared, SERVED_ARCHITECTURE) => {
            Err(FamilyRefusal::BackendDoesNotServe {
                family: FamilyName(declared.to_string()),
                backend: NATIVE_BACKEND,
            })
        }
        _ => Ok(()),
    }
}

/// Read the artifact's declaration, the free read the family judgment needs.
fn read_declaration(path: &Path) -> Result<serde_json::Value, AdmitRefusal> {
    let text = std::fs::read_to_string(path).map_err(|error| AdmitRefusal::LoadFailed {
        detail: format!("{}: {error}", path.display()),
    })?;
    serde_json::from_str(&text).map_err(|error| AdmitRefusal::LoadFailed {
        detail: format!("{}: {error}", path.display()),
    })
}

/// Read the model's `config.json`, judging the family before the shapes.
///
/// **A family this path does not serve refuses as a family and never as a
/// load**, per `weaver-spu-Spec` section 4.1. The architecture the file
/// declares is read first and only a served one is parsed into the forward's
/// own struct, because parsing first answers a family question with whichever
/// field the other family happens to spell differently: a stock Qwen3 export
/// carries `"sliding_window": null` where qwen2's struct requires a number,
/// and issue #507 met that as a parse error naming a config line that is
/// correct.
///
/// Monomorphic on purpose: the crate carries `serde_json` and not `serde`
/// itself, per the Spec's dependency list, so a generic bound would need a
/// dependency this signature does not justify. The one consumer is the
/// config.
fn read_config(path: &Path) -> Result<Config, AdmitRefusal> {
    let text = std::fs::read_to_string(path).map_err(|error| AdmitRefusal::LoadFailed {
        detail: format!("{}: {error}", path.display()),
    })?;
    let declared: serde_json::Value =
        serde_json::from_str(&text).map_err(|error| AdmitRefusal::LoadFailed {
            detail: format!("{}: {error}", path.display()),
        })?;
    judge_family(&declared).map_err(AdmitRefusal::Family)?;
    // **The shapes parse from the text and never from the value**, because
    // `serde_json` carries a line and column on an error from the first and
    // none from the second. A served family whose config has a bad shape is
    // exactly the case a line number is for, and the remedy for #507 sending
    // a reader to a correct line must not be that no line is named at all.
    // Measured: `at line 1 column 21` against the bare message. The second
    // parse of a small file at load is what it costs.
    serde_json::from_str(&text).map_err(|error| AdmitRefusal::LoadFailed {
        detail: format!("{}: {error}", path.display()),
    })
}

/// The native engine: the five primitives over a session's clone of the
/// resident model.
pub struct NativeEngine {
    model: Forward,
    device: Device,
    /// Every token decoded in order, the one account this engine holds of its
    /// own state, retained because truncation re-decodes the front of it.
    resident: Vec<TokenId>,
    /// The last decode's logits, the only position `distribution` and
    /// `sample` may read. `None` before the first decode, so a sample before
    /// any decode refuses rather than reading a buffer nothing filled.
    logits: Option<Vec<f32>>,
    sampler: LogitsProcessor,
    /// The sampling shape the knobs describe, held so each generation's
    /// sampler is rebuilt from it and the derived seed, per
    /// `weaver-spu-Spec` section 8.5.
    sampling: Sampling,
    /// The repetition knobs, applied at sample over the resident tail: an
    /// effective value the record claims was in effect must be in effect,
    /// and the GGUF engine applies these in its sampler chain.
    repetition_penalty: f32,
    repetition_window: usize,
    capacity: usize,
    /// Whether this session's residency was admitted with readout elected,
    /// judged at admit and read here: an elected engine taps every forward
    /// and an unelected one runs the plain path and accumulates nothing.
    readout: bool,
    /// The reduction accumulating since the last drain, per
    /// `weaver-spu-Spec` section 7: one figure per layer per forward, the
    /// norm taken on the device so the activations never leave it.
    reduction: crate::readout::Reduction,
    closed: bool,
}

impl NativeEngine {
    /// Open a session over the residency.
    pub fn open(
        model: &ResidentModel,
        knobs: &EffectiveKnobs,
        capacity: u32,
        readout: bool,
    ) -> Result<NativeEngine, DecodeFault> {
        if capacity == 0 {
            return Err(DecodeFault::Engine {
                detail: "a session capacity of zero serves nothing".into(),
            });
        }
        let device = model.device.clone();
        // The sampling chain mirrors the effective knobs the way the GGUF
        // engine's sampler does: temperature zero is argmax, and the top-k
        // and top-p gates compose where both are live.
        // **A top-k of zero disables the gate rather than refusing**, which
        // is the GGUF engine's semantics for the same knob: llama.cpp reads
        // k <= 0 as keep-everything, and the two engines answering one knob
        // two ways would make the effective values mean different things per
        // container.
        let sampling = if knobs.temperature <= 0.0 {
            Sampling::ArgMax
        } else if knobs.top_k == 0 {
            Sampling::TopP {
                p: knobs.top_p as f64,
                temperature: knobs.temperature as f64,
            }
        } else {
            Sampling::TopKThenTopP {
                k: knobs.top_k as usize,
                p: knobs.top_p as f64,
                temperature: knobs.temperature as f64,
            }
        };
        Ok(NativeEngine {
            model: match &model.model {
                Forward::Single(single) => {
                    // The single model's caches also ride a derived clone,
                    // and a fresh residency's are empty. Cleared anyway, for
                    // the same reason the pair door exists: the session's
                    // account starts at zero and its state must too.
                    let mut clone = single.clone();
                    clone.clear_kv_cache();
                    Forward::Single(clone)
                }
                Forward::Pair(pair) => Forward::Pair(pair.session_clone()),
            },
            device,
            resident: Vec::new(),
            logits: None,
            sampler: LogitsProcessor::from_sampling(knobs.seed, sampling.clone()),
            sampling,
            repetition_penalty: knobs.repetition_penalty,
            repetition_window: knobs.repetition_window as usize,
            capacity: capacity as usize,
            readout,
            reduction: crate::readout::Reduction::new(),
            closed: false,
        })
    }

    fn engine_fault(detail: &str) -> DecodeFault {
        DecodeFault::Engine {
            detail: detail.into(),
        }
    }

    /// One forward over `tokens` at the engine's own resident length.
    fn forward(&mut self, tokens: &[TokenId]) -> Result<Vec<f32>, DecodeFault> {
        let ids: Vec<u32> = tokens.iter().map(|token| token.0).collect();
        match &mut self.model {
            Forward::Single(model) => {
                let input = Tensor::new(ids.as_slice(), &self.device)
                    .and_then(|t| t.unsqueeze(0))
                    .map_err(|error| Self::engine_fault(&format!("input tensor: {error}")))?;
                let logits = if self.readout {
                    // The tap, per Spec section 7: the fork's intermediates
                    // route, each layer's norm taken on the device and one
                    // scalar folded, the activations never leaving.
                    let (logits, intermediates) = model
                        .forward_with_intermediates(&input, self.resident.len())
                        .map_err(|error| Self::engine_fault(&format!("forward: {error}")))?;
                    // **One forward, handed over as one**, so the reduction
                    // records where it ended rather than leaving the boundary
                    // to arithmetic over the token count.
                    let mut forward = Vec::with_capacity(intermediates.len());
                    for layer in &intermediates {
                        forward.push(
                            layer_norm_figure(layer)
                                .map_err(|error| Self::engine_fault(&format!("tap: {error}")))?,
                        );
                    }
                    self.reduction
                        .fold_forward(&forward)
                        .map_err(|detail| Self::engine_fault(&format!("tap: {detail}")))?;
                    logits
                } else {
                    model
                        .forward(&input, self.resident.len())
                        .map_err(|error| Self::engine_fault(&format!("forward: {error}")))?
                };
                logits
                    .squeeze(0)
                    .and_then(|t| t.squeeze(0))
                    .and_then(|t| t.to_dtype(DType::F32))
                    .and_then(|t| t.to_vec1::<f32>())
                    .map_err(|error| Self::engine_fault(&format!("forward: {error}")))
            }
            Forward::Pair(model) => {
                let norms = if self.readout {
                    let mut folded = Vec::new();
                    let logits = model.forward(&ids, self.resident.len(), Some(&mut folded))?;
                    self.reduction
                        .fold_forward(&folded)
                        .map_err(|detail| Self::engine_fault(&format!("tap: {detail}")))?;
                    return Ok(logits);
                } else {
                    None
                };
                model.forward(&ids, self.resident.len(), norms)
            }
        }
    }

    fn clear_model_cache(&mut self) {
        match &mut self.model {
            Forward::Single(model) => model.clear_kv_cache(),
            Forward::Pair(model) => model.clear_kv_cache(),
        }
    }
}

/// One layer's norm, taken on the device: the square, the sum, the root,
/// and a single scalar crossing to the host, which is the in-place clause
/// of `weaver-spu-Spec` section 7 read at its strongest.
pub(crate) fn layer_norm_figure(layer: &Tensor) -> candle_core::Result<f32> {
    layer
        .to_dtype(DType::F32)?
        .sqr()?
        .sum_all()?
        .sqrt()?
        .to_scalar::<f32>()
}

impl Backend for NativeEngine {
    /// **Rebuilt outright**, per `weaver-spu-Spec` section 8.5. A
    /// `LogitsProcessor` is a plain value holding the sampler and nothing
    /// else: this path applies the repetition penalty itself, reading the
    /// resident tail at each draw, so the window argument is already
    /// satisfied by state this engine keeps and rebuilding costs nothing
    /// beyond the value.
    fn reseed(&mut self, seed: u64, _window: &[TokenId]) -> Result<(), DecodeFault> {
        self.sampler = LogitsProcessor::from_sampling(seed, self.sampling.clone());
        Ok(())
    }

    fn decode_at(&mut self, tokens: &[TokenId], position: usize) -> Result<(), DecodeFault> {
        if self.closed {
            return Err(Self::engine_fault("the engine is closed"));
        }
        // **The caller's absolute position must agree with the engine's own
        // account**, or one of the two has lost the session. The session
        // holds the one account of what is resident, and this check is what
        // keeps a backend from silently disagreeing with it.
        if position != self.resident.len() {
            return Err(Self::engine_fault(&format!(
                "position {position} against a resident length of {}",
                self.resident.len()
            )));
        }
        // **An empty decode is a no-op, not a kernel launch.** The session
        // sends the delta as given, and a turn whose delta is empty samples
        // from the distribution the prefix's own decode left standing. A
        // zero-length tensor reaching the embedding kernel is an invalid
        // argument at the driver, measured on this workshop, so the empty
        // case returns before the device is asked.
        if tokens.is_empty() {
            return Ok(());
        }
        if self.resident.len() + tokens.len() > self.capacity {
            return Err(DecodeFault::Overflow {
                resident: self.resident.len(),
                requested: tokens.len(),
                capacity: self.capacity,
            });
        }
        // **A failed forward poisons the engine.** Candle advances layer
        // caches as it walks, so a failure partway leaves cache the engine's
        // own account never admitted, and a later decode against that state
        // would diverge silently. The session poisons itself on the same
        // fault, and this is the engine holding the property for callers the
        // session does not mediate.
        let logits = match self.forward(tokens) {
            Ok(logits) => logits,
            Err(fault) => {
                self.close();
                return Err(fault);
            }
        };
        self.resident.extend_from_slice(tokens);
        self.logits = Some(logits);
        Ok(())
    }

    fn distribution(&self) -> Result<&[f32], DecodeFault> {
        if self.closed {
            return Err(Self::engine_fault("the engine is closed"));
        }
        self.logits
            .as_deref()
            .ok_or_else(|| Self::engine_fault("no decode has produced a distribution"))
    }

    fn sample(&mut self) -> Result<TokenId, DecodeFault> {
        if self.closed {
            return Err(Self::engine_fault("the engine is closed"));
        }
        let logits = self
            .logits
            .as_deref()
            .ok_or_else(|| Self::engine_fault("no decode has produced a distribution"))?;
        let mut tensor = Tensor::new(logits, &Device::Cpu)
            .map_err(|error| Self::engine_fault(&format!("logits tensor: {error}")))?;
        // The penalty reads the resident tail, the same window the knobs
        // describe. `distribution` stays the raw measurement: the signals
        // read the model's own distribution, and the penalty is the
        // sampler's business, which is where the GGUF chain applies it too.
        if self.repetition_penalty != 1.0 && self.repetition_window > 0 {
            let start = self.resident.len().saturating_sub(self.repetition_window);
            let context: Vec<u32> = self.resident[start..].iter().map(|t| t.0).collect();
            tensor = candle_transformers::utils::apply_repeat_penalty(
                &tensor,
                self.repetition_penalty,
                &context,
            )
            .map_err(|error| Self::engine_fault(&format!("repeat penalty: {error}")))?;
        }
        let token = self
            .sampler
            .sample(&tensor)
            .map_err(|error| Self::engine_fault(&format!("sample: {error}")))?;
        Ok(TokenId(token))
    }

    fn truncate_to(&mut self, position: usize) -> Result<(), DecodeFault> {
        if self.closed {
            return Err(Self::engine_fault("the engine is closed"));
        }
        if position > self.resident.len() {
            return Err(Self::engine_fault(&format!(
                "truncate to {position} beyond a resident length of {}",
                self.resident.len()
            )));
        }
        // Truncating to the standing length changes nothing, and paying a
        // full re-decode to change nothing would make the common flush the
        // expensive case for no outcome.
        if position == self.resident.len() {
            return Ok(());
        }
        // Clear, then re-decode the retained front: the state after holds
        // exactly the first `position` tokens, a true truncation reached the
        // expensive way, per this module's header. A re-decode that fails
        // poisons, for `decode_at`'s reason: the cache state is
        // indeterminate and nothing may build on it.
        self.clear_model_cache();
        let front: Vec<TokenId> = self.resident[..position].to_vec();
        self.resident.clear();
        self.logits = None;
        if !front.is_empty() {
            let logits = match self.forward(&front) {
                Ok(logits) => logits,
                Err(fault) => {
                    self.close();
                    return Err(fault);
                }
            };
            self.resident = front;
            self.logits = Some(logits);
        }
        Ok(())
    }

    fn take_reduction(&mut self) -> Option<crate::readout::Reduction> {
        if !self.readout {
            return None;
        }
        Some(std::mem::take(&mut self.reduction))
    }

    fn reestablish(&mut self) -> Result<(), DecodeFault> {
        if self.closed {
            return Err(Self::engine_fault("the engine is closed"));
        }
        self.clear_model_cache();
        self.resident.clear();
        self.logits = None;
        Ok(())
    }

    fn close(&mut self) {
        self.clear_model_cache();
        self.resident.clear();
        self.logits = None;
        self.closed = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **A family this path does not serve refuses as a family**, naming the
    /// spelling the artifact used, and the shapes are never reached.
    ///
    /// The config is a stock Qwen3 export's two deciding fields: the family it
    /// declares, and the `sliding_window` null that qwen2's struct refuses. So
    /// the order is what this watch reads and not the judgment alone.
    ///
    /// Perturbation: judge after the parse instead and this fails, the refusal
    /// arriving as `LoadFailed` naming a config line that is correct, which is
    /// the state issue #507 met.
    ///
    /// conforms: spu-native-refuses-an-unserved-family-as-a-family
    #[test]
    fn an_unserved_family_refuses_as_a_family_before_the_shapes() {
        let dir = fixture(
            "unserved-family",
            br#"{"model_type": "qwen3", "sliding_window": null}"#,
        );
        let refusal = read_config(&dir.join("config.json"));
        // **Cleaned before the assert**, so the run this test is meant to be
        // put through red leaves nothing behind each time it is.
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(
            refusal.expect_err("an unserved family refuses"),
            AdmitRefusal::Family(FamilyRefusal::BackendDoesNotServe {
                family: FamilyName("qwen3".into()),
                backend: NATIVE_BACKEND,
            }),
            "the refusal names the backend and the file's own spelling, and never \
             claims the registry lacks a family it carries"
        );
    }

    /// **A served family whose shapes are wrong keeps its line and column**,
    /// which is the half of the message #507's reader needed and did not get
    /// pointed at.
    ///
    /// Perturbation: parse the shapes from the value rather than the text and
    /// this fails, the position going with it.
    ///
    /// conforms: spu-native-refuses-an-unserved-family-as-a-family
    #[test]
    fn a_served_familys_shape_error_names_where_it_is() {
        let dir = fixture(
            "served-shape",
            br#"{"model_type": "qwen2", "hidden_size": "wide"}"#,
        );
        let refusal = read_config(&dir.join("config.json"));
        let _ = std::fs::remove_dir_all(&dir);

        let AdmitRefusal::LoadFailed { detail } =
            refusal.expect_err("a bad shape under a served family refuses as a load")
        else {
            panic!("a served family's shape error is a load failure");
        };
        assert!(
            detail.contains("line") && detail.contains("column"),
            "the shape error names where it is: {detail}"
        );
    }

    /// A fixture directory of this module's own, named for its test so two
    /// of them in one binary cannot collide.
    fn fixture(name: &str, config: &[u8]) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("weaver-spu-family-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the fixture directory stands");
        std::fs::write(dir.join("config.json"), config).expect("the fixture config is written");
        dir
    }

    /// The served family passes, folded rather than matched byte for byte,
    /// and a config naming no family is left to the shapes.
    ///
    /// conforms: spu-native-refuses-an-unserved-family-as-a-family
    #[test]
    fn the_served_family_passes_and_an_unnamed_one_is_left_to_the_shapes() {
        for spelling in ["qwen2", "Qwen2", "qwen-2"] {
            let config = serde_json::json!({"model_type": spelling});
            assert_eq!(
                judge_family(&config),
                Ok(()),
                "{spelling} is the served key"
            );
        }
        let unnamed = serde_json::json!({"hidden_size": 1});
        assert_eq!(judge_family(&unnamed), Ok(()));
    }
}
