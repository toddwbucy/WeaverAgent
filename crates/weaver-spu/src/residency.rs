//! conforms: spu-header-read-touches-no-device
//! conforms: spu-loader-shapes-pinned-by-doctest
//! conforms: spu-no-path-taking-loader
//! conforms: spu-devices-from-the-binding
//! conforms: spu-admission-judges-room-reach-and-width
//! conforms: spu-headroom-is-a-construction-parameter
//! conforms: spu-release-frees-before-answering
//! conforms: spu-no-idempotence-no-retry
//!
//! Admit and release, per `weaver-spu-Spec` section 3.
//!
//! Admit runs the charter's five steps and the first three are free: resolve
//! the binding to an artifact, read what the artifact declares about itself
//! without loading it, judge the assigned devices, take them in shard order and
//! load each shard, confirm. Every step before the fourth is refusable at no
//! cost, which converts the common shape of a bad binding, an artifact present
//! and wrong, into a refusal costing no device work.
//!
//! **The devices are the binding's and this crate selects none.** No device
//! survey, no ranking, no fallback: the archived tree's `auto_select_gpu` does
//! not cross.
//!
//! **Nothing is idempotent and nothing retries.** This crate begins empty,
//! admits once, and dies, so a second admit has no prior residency to match.
//!
//! **No path-taking model loader exists beyond the binding's own resolution.**
//! The one door to a load is [`Admission`], which only `admit` constructs, so
//! the loader cannot be handed a path that did not walk the free steps: the
//! resolution, the header read, and the judgments run first or the load is not
//! expressible. The two shapes the Spec names are pinned below, and the day
//! either compiles, the pin fires.
//!
//! ```compile_fail
//! fn pin(path: &str) -> weaver_spu::residency::Admission<'_> {
//!     // A loader door from a bare string does not exist.
//!     weaver_spu::residency::Admission::from(path)
//! }
//! ```
//!
//! ```compile_fail
//! fn pin(path: std::path::PathBuf) -> weaver_spu::residency::Admission<'static> {
//!     // Nor from a PathBuf outside the admission path.
//!     weaver_spu::residency::Admission::from(path)
//! }
//! ```
//!
//! A compile-fail pin passes on any compile error, so the two above would go
//! on passing if [`Admission`] were renamed or its accessors removed, proving
//! nothing. This one compiles, names the door and its surface, and fails
//! loudly the day either disappears:
//!
//! ```
//! fn reads<'a>(admission: &'a weaver_spu::residency::Admission<'a>) -> &'a std::path::Path {
//!     admission.path()
//! }
//! ```

use std::path::{Path, PathBuf};

use weaver_types::{DeviceOrdinal, LifecycleRefusal, ModelBinding};

use crate::artifact::{self, ArtifactHeader};
use crate::decoder::backend::{self, DecodeFault, FlushMechanism, TokenId};
use crate::decoder::session::Session;
use crate::family::{self, FamilyRefusal};
use crate::readout::{self, ReadoutElection, ReadoutRefusal};

/// The headroom term, in bytes, held beside the shard on each assigned device.
///
/// **A construction parameter until a measurement replaces it.** Charter
/// section 9 stages the figure and names the entry condition, a measurement on
/// a real artifact against a real device, so the admission inequality takes the
/// headroom from the worker's composition root rather than from an operator
/// election. A builder supplies it before the measurement exists.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Headroom(pub u64);

/// The weights hash that travels with every measurement.
///
/// Never empty from this crate: a hash that cannot be computed refuses the
/// admission, per Spec section 3 and the operator's ruling of 2026-10-01, and
/// apex section 8 rests replay on the identity being right.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeightsHash(pub String);

/// Why an admit refused, with the step that refused it legible.
///
/// The step identity is the point: **section 10 buys the ordering rather than
/// the parsing**, and a test that could not tell which step refused would pass
/// with the whole ordering inverted.
#[derive(Debug, Clone, PartialEq)]
pub enum AdmitRefusal {
    /// The load elected residual readout and this family's engine cannot tap.
    /// Judged on the admit path, so the refusal costs the operator nothing
    /// beyond the header read.
    Readout(ReadoutRefusal),
    /// Step one: the binding did not resolve to an artifact, nothing standing
    /// at the path as this process. Carries the path and the step, because the
    /// account that stays on this side of the seam names them, per Spec
    /// section 3.
    Unresolvable { path: String, step: &'static str },
    /// Steps one and two: the artifact is present and could not be reached,
    /// opened, or read, its header or its size on disk, judged as this
    /// process's identity. Every read is of the artifact and every one is
    /// free. Carries the path the step failed on and the step, for the same
    /// account.
    Unreadable { path: String, step: &'static str },
    /// Step three, on the condition that reads nothing. Carries the family,
    /// because the no-silent-substitution test reads the name.
    Family(FamilyRefusal),
    /// Step three, on a condition that reads the driver, where no driver is
    /// compiled in to read.
    Device,
    /// Step three, on the set's own shape: an ordinal that appears twice. Two
    /// shards of one artifact on one card is not a two-device admission, and
    /// judging the pair as distinct would pass room on half the real demand and
    /// skip the peer check the pair would need. Free, so judged with the width.
    DuplicateDevice { ordinal: u32 },
    /// Step three, on a condition the driver answered. Carries which device and
    /// which condition, because an operator meeting this needs to know whether
    /// to free a card or to reassign the binding.
    #[cfg(feature = "cuda")]
    DeviceRefused(crate::gpu::DeviceRefusal),
    /// Step four, on a container whose backend this build does not carry.
    /// Named distinctly so that a build with a device attached is not told it
    /// has a device problem.
    BackendNotBuilt,
    /// Step four, on the engine itself: the backend was asked to take the
    /// weights and could not. The detail is the engine's own account.
    LoadFailed { detail: String },
    /// A second admit. This crate admits once.
    AlreadyAttempted,
}

impl AdmitRefusal {
    /// The floor's artifact case with the account this side keeps, per Spec
    /// section 3: absent is unresolvable and everything else is unreadable,
    /// each naming the path the step failed on and the step.
    fn on_artifact(refusal: LifecycleRefusal, path: &std::path::Path, step: &'static str) -> Self {
        let path = path.display().to_string();
        match refusal {
            LifecycleRefusal::ArtifactUnresolvable => AdmitRefusal::Unresolvable { path, step },
            LifecycleRefusal::ArtifactUnreadable => AdmitRefusal::Unreadable { path, step },
            // The artifact module raises the two cases above and no other,
            // so a third arriving here is an act that added one without
            // saying where it lands, and it declares itself rather than
            // reading as unreadable.
            other => unreachable!("the artifact module raised {other:?}, which it does not raise"),
        }
    }
}

impl From<AdmitRefusal> for LifecycleRefusal {
    fn from(refusal: AdmitRefusal) -> Self {
        match refusal {
            AdmitRefusal::Unresolvable { .. } => LifecycleRefusal::ArtifactUnresolvable,
            AdmitRefusal::Unreadable { .. } => LifecycleRefusal::ArtifactUnreadable,
            AdmitRefusal::Family(inner) => inner.into(),
            // The floor's set carries no readout-naming case, so the election
            // failure maps onto the device's inability to admit, which is what
            // it is: this device's engine cannot serve what was elected. The
            // family name stays on this side of the seam with the rest of the
            // detail, per the same rule the family refusal follows.
            AdmitRefusal::Readout(_) => LifecycleRefusal::DeviceCannotAdmit,
            AdmitRefusal::Device => LifecycleRefusal::DeviceCannotAdmit,
            AdmitRefusal::DuplicateDevice { .. } => LifecycleRefusal::DeviceCannotAdmit,
            #[cfg(feature = "cuda")]
            AdmitRefusal::DeviceRefused(_) => LifecycleRefusal::DeviceCannotAdmit,
            // The floor's closed set carries no case for a step this binary did
            // not build, and inventing one would be a floor edit this act has no
            // ruling for. It crosses as a device refusal, which is the nearest
            // true statement: this process cannot take the devices it was given.
            AdmitRefusal::BackendNotBuilt => LifecycleRefusal::DeviceCannotAdmit,
            // The charter's enumeration maps step-four failures onto the device
            // case: the admit was device work when it failed.
            AdmitRefusal::LoadFailed { .. } => LifecycleRefusal::DeviceCannotAdmit,
            AdmitRefusal::AlreadyAttempted => LifecycleRefusal::OutOfOrder,
        }
    }
}

/// The admission's proof, and the loader's one door.
///
/// Only `admit` constructs this, after the free steps have run, so a load from
/// a path that was never resolved, never header-read, and never judged is not
/// expressible. The fields are read-only views into the admit that made them.
pub struct Admission<'a> {
    path: &'a Path,
    /// Every shard's descriptor path in split order, one entry for a
    /// single-file artifact. The loader that can take a set takes this, and
    /// `path` stays the first shard for the readers a single path serves.
    paths: &'a [PathBuf],
    header: &'a ArtifactHeader,
    devices: &'a [DeviceOrdinal],
    /// What makes the door a door: nothing outside this module constructs one.
    _admitted: (),
}

impl Admission<'_> {
    pub fn path(&self) -> &Path {
        self.path
    }
    pub fn paths(&self) -> &[PathBuf] {
        self.paths
    }
    pub fn header(&self) -> &ArtifactHeader {
        self.header
    }
    pub fn devices(&self) -> &[DeviceOrdinal] {
        self.devices
    }
}

/// The model a successful admit holds resident, one case per backend.
///
/// Holding it is the residency: dropping it is what frees the device, so the
/// release ordering is by construction rather than by a comment.
pub enum LoadedModel {
    /// A GGUF model held by llama.cpp, weights on the assigned devices.
    #[cfg(feature = "gguf")]
    Gguf(crate::decoder::gguf::ResidentModel),
    /// A safetensors model held by the pinned candle fork, weights on the
    /// assigned device. Boxed for the variant-size lint's reason: the
    /// tokenizer's tables ride the value, and an enum sized for its largest
    /// member would tax the GGUF case with the native one's weight.
    #[cfg(feature = "cuda")]
    Native(Box<crate::decoder::native::ResidentModel>),
}

impl std::fmt::Debug for LoadedModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            #[cfg(feature = "gguf")]
            LoadedModel::Gguf(_) => f.write_str("LoadedModel::Gguf"),
            #[cfg(feature = "cuda")]
            LoadedModel::Native(_) => f.write_str("LoadedModel::Native"),
            #[allow(unreachable_patterns)]
            _ => f.write_str("LoadedModel"),
        }
    }
}

/// A family's stop conditions, promoted against one artifact's vocabulary.
///
/// Built by [`Resident::stop_set`], which is where the family's declaration and
/// the artifact's vocabulary meet. Nothing here is read from the artifact
/// alone: the strings are the family's and this is what became of them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StopSet {
    /// Every declared condition that promoted, plus the artifact's declared
    /// end-of-sequence as a backstop.
    pub tokens: Vec<TokenId>,
    /// The turn's close, made resident after a generation so the residency
    /// holds a closed turn. The family's first declared condition.
    pub terminator: TokenId,
    /// Declared conditions this artifact's vocabulary does not promote to one
    /// token, named so a run reports them rather than carrying a stop that
    /// could never fire.
    pub unpromoted: Vec<&'static str>,
}

/// Promote a family's declared stop conditions against one vocabulary.
///
/// The judgment of [`Resident::stop_set`], separated from the model so it can
/// be read without one. The tokenizer arrives as a closure for that reason and
/// for no other: what this decides is the same whatever supplied the ids.
pub fn promote_stop_conditions(
    declared: &[&'static str],
    eos: TokenId,
    tokenize: impl Fn(&str) -> Result<Vec<TokenId>, DecodeFault>,
) -> Result<StopSet, DecodeFault> {
    let mut tokens = Vec::new();
    let mut unpromoted = Vec::new();
    let mut terminator = None;

    for (position, condition) in declared.iter().enumerate() {
        match tokenize(condition)?.as_slice() {
            [token] => {
                if position == 0 {
                    terminator = Some(*token);
                }
                tokens.push(*token);
            }
            _ => unpromoted.push(*condition),
        }
    }

    let Some(terminator) = terminator else {
        return Err(DecodeFault::Engine {
            detail: format!(
                "the family's turn close does not promote against this artifact's \
                 vocabulary, so no turn could be recognised as ended: declared {declared:?}"
            ),
        });
    };

    // The artifact's own end-of-sequence, added rather than assumed. A model
    // that ends the sequence stops whatever its family declares.
    if !tokens.contains(&eos) {
        tokens.push(eos);
    }

    Ok(StopSet {
        tokens,
        terminator,
        unpromoted,
    })
}

/// What this process holds after a successful admit.
#[derive(Debug)]
pub struct Resident {
    pub artifact: PathBuf,
    pub header: ArtifactHeader,
    /// **The registry entry this artifact was selected against, held rather
    /// than looked up again.** The selection reads the artifact's template
    /// where its architecture is contested, so re-deriving it on the decode
    /// path would render the same answer once per turn. It is judged once, at
    /// admit, where Spec section 7 puts the family judgment, and what follows
    /// reads the answer.
    pub declaration: &'static family::Declaration,
    pub devices: Vec<DeviceOrdinal>,
    pub weights_hash: WeightsHash,
    /// The readout election this residency was admitted under, judged at
    /// admit and read at session open: an elected residency taps, an
    /// unelected one runs the plain forward, and nothing later re-decides.
    pub readout: ReadoutElection,
    /// The surprisal election this residency was admitted under, per
    /// charter section 13.12.
    ///
    /// **It is judged against nothing and so takes no election type.** The
    /// readout is judged against what the family's engine can tap and the
    /// field's depth against the sampling cutoff, so each of those carries
    /// a type that records a judgment. This one sizes nothing and asks the
    /// engine for nothing the generation was not already computing, so it
    /// is carried and obeyed rather than tested, and a bare boolean is the
    /// whole of what there is to hold.
    pub surprisal_elected: bool,
    /// The model itself. Private: what the rest of the crate may do with a
    /// residency is a question for the decode acts, and nothing reaches the
    /// engine around the seam.
    model: LoadedModel,
}

impl Resident {
    /// The held model, for the decode acts that follow. On a build carrying no
    /// backend [`LoadedModel`] has no cases, which is the type stating that
    /// such a build holds no residency to reach.
    pub fn model(&self) -> &LoadedModel {
        &self.model
    }

    /// Open a session over this residency.
    ///
    /// **The one door from a residency to a serving session.** The container
    /// derivation of `weaver-spu-Spec` section 4.1 decides which engine serves,
    /// read from the header this admit already validated rather than from a
    /// configuration field, and a container this build cannot serve refuses by
    /// name before an engine is asked for.
    ///
    /// **The session borrows this residency.** An engine holds a context, a
    /// context borrows its model, and this holds the model, so the returned
    /// session cannot outlive the residency it decodes against. The lifetime
    /// carries that rather than a rule a caller has to keep.
    pub fn open_session(
        &self,
        knobs: &crate::sampling::EffectiveKnobs,
        capacity: u32,
    ) -> Result<Session<'_>, DecodeFault> {
        backend::for_container(self.header.container)?;
        // On a build carrying no engine the two below are consumed by no arm,
        // and saying so here is cheaper than shaping the signature around a
        // build that cannot serve anything anyway.
        #[cfg(not(any(feature = "gguf", feature = "cuda")))]
        let _ = (knobs, capacity);
        match &self.model {
            #[cfg(feature = "gguf")]
            LoadedModel::Gguf(model) => {
                let engine = crate::decoder::gguf::GgufEngine::open(
                    model,
                    knobs,
                    capacity,
                    self.readout.elected(),
                )?;
                Ok(Session::new(
                    Box::new(engine),
                    capacity as usize,
                    self.flush_mechanism()?,
                    self.surprisal_elected,
                ))
            }
            #[cfg(feature = "cuda")]
            LoadedModel::Native(model) => {
                let engine = crate::decoder::native::NativeEngine::open(
                    model,
                    knobs,
                    capacity,
                    self.readout.elected(),
                )?;
                Ok(Session::new(
                    Box::new(engine),
                    capacity as usize,
                    self.flush_mechanism()?,
                    self.surprisal_elected,
                ))
            }
            // On a build carrying no backend `LoadedModel` has no cases, so
            // this arm is unreachable rather than unwritten.
            #[cfg(not(any(feature = "gguf", feature = "cuda")))]
            _ => Err(DecodeFault::ContainerNotBuilt {
                container: self.header.container,
            }),
        }
    }

    /// Tokenize rendered text against the resident model's vocabulary.
    ///
    /// On the residency rather than in the engine because the vocabulary is
    /// the model's and not the context's: rendering and tokenizing precede
    /// any session, and their product is what a session is opened with. On a
    /// build carrying no backend `LoadedModel` has no cases and the match
    /// says so.
    pub fn tokenize(&self, text: &str) -> Result<Vec<TokenId>, DecodeFault> {
        #[cfg(not(feature = "gguf"))]
        let _ = text;
        match &self.model {
            #[cfg(feature = "gguf")]
            LoadedModel::Gguf(model) => model
                .model()
                .str_to_token(text, llama_cpp_2::model::AddBos::Never)
                .map(|tokens| tokens.into_iter().map(|t| TokenId(t.0 as u32)).collect())
                .map_err(|error| DecodeFault::Engine {
                    detail: error.to_string(),
                }),
            #[cfg(feature = "cuda")]
            LoadedModel::Native(model) => model.tokenize(text),
            #[allow(unreachable_patterns)]
            _ => Err(DecodeFault::ContainerNotBuilt {
                container: self.header.container,
            }),
        }
    }

    /// The emission verbatim: the sampled tokens back as text, before any
    /// parse, which is what the record's `model.output` carries.
    pub fn detokenize(&self, tokens: &[TokenId]) -> Result<String, DecodeFault> {
        #[cfg(not(feature = "gguf"))]
        let _ = tokens;
        match &self.model {
            #[cfg(feature = "gguf")]
            LoadedModel::Gguf(model) => {
                // The whole sequence decodes together: a multi-byte character
                // split across two tokens is ordinary under a byte-pair
                // vocabulary, and per-token conversion would break it where
                // the split fell. The bytes are gathered per token and the
                // text conversion is one pass over the concatenation, so a
                // split character still fails as incomplete rather than
                // rendering wrong, which is the failure the stream's pending
                // buffer waits on. The engine reports a too-small buffer as
                // the negative of the size it needed, so one retry at that
                // size is exact - the wrapper's whole-sequence call caps the
                // buffer at eight bytes and never retries, which faults any
                // piece longer than that.
                let mut bytes = Vec::new();
                for token in tokens {
                    let token = llama_cpp_2::token::LlamaToken(token.0 as i32);
                    let piece = match model.model().token_to_piece_bytes(token, 8, true, None) {
                        Err(llama_cpp_2::TokenToStringError::InsufficientBufferSpace(needed)) => {
                            model.model().token_to_piece_bytes(
                                token,
                                (-needed) as usize,
                                true,
                                None,
                            )
                        }
                        other => other,
                    }
                    .map_err(|error| DecodeFault::Engine {
                        detail: error.to_string(),
                    })?;
                    bytes.extend_from_slice(&piece);
                }
                String::from_utf8(bytes).map_err(|error| DecodeFault::Engine {
                    detail: error.to_string(),
                })
            }
            #[cfg(feature = "cuda")]
            LoadedModel::Native(model) => model.detokenize(tokens),
            #[allow(unreachable_patterns)]
            _ => Err(DecodeFault::ContainerNotBuilt {
                container: self.header.container,
            }),
        }
    }

    /// The artifact's declared end-of-sequence token.
    ///
    /// **Kept as a backstop and no longer as the turn's close.** It was both
    /// until [`Self::stop_set`] landed, on the reading that "for every shipped
    /// chat family the EOS is the turn-close marker itself", which held for the
    /// qwen artifacts declaring `<|im_end|>` there and fails for gemma4, whose
    /// EOS is `<eos>` and whose turn closes with `<turn|>` in `eot_token_id`.
    /// A model that ends the sequence outright should still stop, so this joins
    /// the family's set rather than leaving it.
    pub fn declared_eos(&self) -> Result<TokenId, DecodeFault> {
        match &self.model {
            #[cfg(feature = "gguf")]
            LoadedModel::Gguf(model) => Ok(TokenId(model.model().token_eos().0 as u32)),
            #[cfg(feature = "cuda")]
            LoadedModel::Native(model) => Ok(model.declared_eos()),
            #[allow(unreachable_patterns)]
            _ => Err(DecodeFault::ContainerNotBuilt {
                container: self.header.container,
            }),
        }
    }

    /// **The stop set this artifact and this family agree on**, per
    /// `weaver-spu-Spec` sections 4.3 and 5.
    ///
    /// The family declares its stop conditions as strings and this promotes
    /// them against the artifact's own vocabulary, which is the only place the
    /// two can be checked against each other. **The declaration is the
    /// authority and the artifact is the check**, the reverse of what this
    /// crate did before: a stop set read from `token_eos` alone is a set the
    /// family never declared, and it agreed with the family only for as long
    /// as every carried artifact happened to put its turn-close marker there.
    ///
    /// **The first declared condition is the turn's close**, and that ordering
    /// is what [`StopSet::terminator`] reads. It is the token made resident
    /// after a generation, so the residency holds a closed turn rather than a
    /// turn ended by whatever the artifact calls end-of-sequence. All four
    /// carried families declare their turn-close first.
    ///
    /// A condition that does not promote to exactly one token is **named
    /// rather than dropped**, in [`StopSet::unpromoted`]. Llama's `<|eom_id|>`
    /// is the standing case: it arrived with 3.1 and the tokenizer this
    /// workshop can reach is 3.0-era, which splits it. A stop condition that
    /// reaches the sampler as several tokens can never match one drawn token,
    /// so carrying it would be a stop that silently never fires.
    ///
    /// **The turn's close failing to promote refuses**, because a family whose
    /// close cannot be recognised generates to the token cap on every turn and
    /// leaves the marker in the emission as prose.
    pub fn stop_set(&self, family: &dyn family::Family) -> Result<StopSet, DecodeFault> {
        promote_stop_conditions(
            family.stop_conditions(),
            self.declared_eos()?,
            |condition| self.tokenize(condition),
        )
    }

    /// The flush mechanism this residency's family declares, per Spec section
    /// 4.4. Read from the declaration rather than inferred from a version
    /// string, which is the whole of that section's claim.
    ///
    /// Reached only from the engine arm, so a build carrying no engine has no
    /// caller for it. The allow says that rather than widening the build's
    /// surface to quiet a lint.
    #[cfg_attr(not(feature = "gguf"), allow(dead_code))]
    fn flush_mechanism(&self) -> Result<FlushMechanism, DecodeFault> {
        Ok(self.declaration.flush)
    }
}

/// The residency, which begins empty and admits once.
#[derive(Debug, Default)]
pub struct Residency {
    resident: Option<Resident>,
    /// Set once an admit has been attempted, successfully or not. Once-only is
    /// a property of the process rather than of whether the first attempt
    /// happened to succeed.
    admit_attempted: bool,
}

impl Residency {
    pub fn new() -> Self {
        Residency::default()
    }

    pub fn resident(&self) -> Option<&Resident> {
        self.resident.as_ref()
    }

    /// Run the charter's five steps in order.
    pub fn admit(
        &mut self,
        binding: &ModelBinding,
        headroom: Headroom,
        readout: ReadoutElection,
        surprisal_elected: bool,
    ) -> Result<&Resident, AdmitRefusal> {
        if self.admit_attempted {
            return Err(AdmitRefusal::AlreadyAttempted);
        }
        self.admit_attempted = true;

        // Step one. Resolve the binding to an artifact. Free.
        let (path, reference_kind) =
            artifact::resolve_with_kind(&binding.artifact).map_err(|refusal| {
                AdmitRefusal::on_artifact(
                    refusal,
                    std::path::Path::new(&binding.artifact.0),
                    "resolve",
                )
            })?;

        // Open it once and hold it. Every read after this, the header, the
        // load, and the hash, goes through this descriptor, so a name replaced
        // mid-admit cannot make the three observe three different files, and
        // the kind check runs on what was opened rather than on the name.
        let mut pinned = artifact::pin(&path)
            .map_err(|refusal| AdmitRefusal::on_artifact(refusal, &path, "pin"))?;

        // Step two. Read what the artifact declares about itself, without
        // loading it and without touching a device. Free.
        let header = artifact::read_header(&mut pinned)
            .map_err(|refusal| AdmitRefusal::on_artifact(refusal, &path, "header"))?;

        // Step three, cheapest first. The width condition is a comparison
        // between the binding's count and the family's declaration and reads
        // nothing; the room and reach conditions each cost a driver query. A
        // set failing more than one condition refuses on the cheapest, which is
        // what puts the width refusal inside a test on a machine with no device.
        // **The entry is selected here, at the first step that needs it.**
        // This step already resolved the family, because the width it compares
        // against is a property of an entry rather than of an architecture.
        // Selecting reads the artifact's template where the architecture is
        // contested, which is a string this step already holds and no device
        // query, so the cheapest-first ordering is unchanged.
        let declaration = family::select(&header.family, header.chat_template.as_deref())
            .map_err(AdmitRefusal::Family)?;
        family::judge_width(declaration, binding.devices.len() as u32)
            .map_err(AdmitRefusal::Family)?;

        // **The readout election is judged against the entry selected above**,
        // per Spec
        // section 7 and the charter's fail-cheap-or-lie-expensive rule. It
        // reads the family's declaration and nothing else, so it costs no
        // device query and sits with the other free judgments. Moved to the
        // first turn instead, the load succeeds and the turn fails, which is
        // the expensive lie the rule forbids and what section 10's watch
        // perturbs.
        readout::judge(readout, declaration).map_err(AdmitRefusal::Readout)?;
        judge_distinct(&binding.devices)?;
        // The shard each device must hold, read from the held descriptor
        // rather than from the name. The width was judged above, so the
        // divisor is known non-zero.
        let shard_bytes = pinned
            .len()
            .map_err(|refusal| AdmitRefusal::on_artifact(refusal, &path, "size"))?
            / binding.devices.len() as u64;
        judge_room_and_reach(&binding.devices, shard_bytes, headroom)?;

        // **The weights hash is judged here, the last free step**, per Spec
        // section 3: computed by reading the artifact, never taken from a
        // manifest handed in, fresh with no cache across an artifact change,
        // and an artifact whose identity cannot be computed refuses before any
        // device is taken, on the operator's ruling of 2026-10-01. The members
        // the load reads are hashed through the descriptors it will read them
        // through. The hash's subject is the operator's reference, which may
        // name a directory whose members beyond the container are part of the
        // artifact's identity.
        let reference = std::path::PathBuf::from(&binding.artifact.0);
        let weights_hash = artifact::weights_hash(&reference, reference_kind, &mut pinned)
            .map_err(|refusal| AdmitRefusal::on_artifact(refusal, &reference, "hash"))?;

        // Step four. Take the devices in shard order and load each shard. The
        // binding's order is the shard order, and the loader's one door is the
        // admission this function just proved.
        // The loader is handed the descriptor's own path, not the operator's
        // name, so it opens the file the judgments were made against.
        //
        // **This carries no instrument and the ground is stated.** Handing the
        // operator's name here instead loads the same file on every path a
        // fixture can build, so the perturbation passes: what the pin defends
        // against is a replacement landing inside the load, and watching it
        // needs a concurrent replacer this suite does not introduce. The kind
        // check on the descriptor is watched, the identity of the descriptor
        // is not.
        let pinned_path = pinned.path();
        let pinned_paths = pinned.paths();
        let admission = Admission {
            path: &pinned_path,
            paths: &pinned_paths,
            header: &header,
            devices: &binding.devices,
            _admitted: (),
        };
        let model = load(&admission)?;

        // Step five. Confirm.
        self.resident = Some(Resident {
            artifact: path,
            header,
            declaration,
            devices: binding.devices.clone(),
            weights_hash,
            readout,
            surprisal_elected,
            model,
        });
        Ok(self.resident.as_ref().expect("set immediately above"))
    }

    /// Free the device, then answer.
    ///
    /// Stop serving, free the weights and the working allocations and the cache
    /// together because they are one residency, then confirm. A confirmation is
    /// a fact about the device rather than a statement of intent, and the
    /// archived tree's inverse ordering is what its own record names as
    /// producing an overcommit. The free happens inside this function, so the
    /// caller cannot answer before it: the ordering is by construction rather
    /// than by a comment asking the caller to wait.
    pub fn release(&mut self) -> Result<(), LifecycleRefusal> {
        let Some(resident) = self.resident.take() else {
            return Err(LifecycleRefusal::NoResidency);
        };
        // The drop is the free: the weights, the working allocations, and the
        // cache go together because they are one residency, and they go here,
        // inside this function, so the caller cannot answer before the device
        // is free. The engine's own drop is what returns the memory.
        drop(resident);
        Ok(())
    }
}

/// The set's ordinals must be distinct, judged with the free conditions.
fn judge_distinct(devices: &[DeviceOrdinal]) -> Result<(), AdmitRefusal> {
    for (index, device) in devices.iter().enumerate() {
        if devices[..index].contains(device) {
            return Err(AdmitRefusal::DuplicateDevice { ordinal: device.0 });
        }
    }
    Ok(())
}

/// The room and reach conditions, each of which costs a driver query.
///
/// **The device judgment reads the driver rather than this crate's own
/// accounting.** This crate holds no fleet ledger to prefer, and the case the
/// check exists to catch is a device occupied by something this program did not
/// put there, so the authority is what the device reports free at the moment of
/// admission. Each assigned device must have room for its shard plus the
/// residency's headroom, the one inequality read per device, and the devices
/// must be able to reach each other, because a sharded forward exchanges
/// activations across them and a set without peer access cannot serve.
#[cfg(not(feature = "cuda"))]
fn judge_room_and_reach(
    _devices: &[DeviceOrdinal],
    _shard_bytes: u64,
    _headroom: Headroom,
) -> Result<(), AdmitRefusal> {
    // No driver is compiled in, so there is no device this build can admit to.
    // Refusing here rather than pretending the conditions passed is what keeps
    // the no-feature build from claiming a residency it cannot hold.
    Err(AdmitRefusal::Device)
}

/// The room and reach conditions, asked of the driver.
///
/// The shard size is the artifact's bytes divided across the assigned set, read
/// from the filesystem at admit. It is an over-estimate of what a shard costs
/// in device memory for a quantized artifact and an under-estimate of the
/// working allocations beside it, which is why the headroom term sits in the
/// inequality and why charter section 9 stages a measurement to replace both.
#[cfg(feature = "cuda")]
fn judge_room_and_reach(
    devices: &[DeviceOrdinal],
    shard_bytes: u64,
    headroom: Headroom,
) -> Result<(), AdmitRefusal> {
    crate::gpu::room_and_reach(devices, shard_bytes, headroom.0)
        .map_err(AdmitRefusal::DeviceRefused)
}

/// Step four: load by the container the header declared.
///
/// **Both paths are real where the build carries them.** A container whose
/// backend this build does not carry refuses by name rather than reporting a
/// device condition, so an operator with a healthy card is not sent to look
/// at it.
fn load(admission: &Admission<'_>) -> Result<LoadedModel, AdmitRefusal> {
    match admission.header().container {
        #[cfg(feature = "gguf")]
        crate::artifact::Container::Gguf => {
            crate::decoder::gguf::ResidentModel::load(admission).map(LoadedModel::Gguf)
        }
        #[cfg(not(feature = "gguf"))]
        crate::artifact::Container::Gguf => Err(AdmitRefusal::BackendNotBuilt),
        #[cfg(feature = "cuda")]
        crate::artifact::Container::Safetensors => {
            crate::decoder::native::ResidentModel::load(admission)
                .map(|model| LoadedModel::Native(Box::new(model)))
        }
        #[cfg(not(feature = "cuda"))]
        crate::artifact::Container::Safetensors => Err(AdmitRefusal::BackendNotBuilt),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::family::FamilyName;
    use weaver_types::ArtifactRef;

    /// A vocabulary that promotes what it was told to promote and splits the
    /// rest, which is what a real tokenizer does to a marker it does not carry.
    fn vocabulary(
        promoting: &'static [(&'static str, u32)],
    ) -> impl Fn(&str) -> Result<Vec<TokenId>, DecodeFault> {
        move |text: &str| {
            Ok(match promoting.iter().find(|(marker, _)| *marker == text) {
                Some((_, id)) => vec![TokenId(*id)],
                None => text.chars().map(|c| TokenId(c as u32)).collect(),
            })
        }
    }

    /// **The turn's close is the family's declaration, not the artifact's
    /// end-of-sequence.**
    ///
    /// The two coincide on every Qwen artifact, which declares `<|im_end|>` as
    /// its EOS, and that coincidence is what let the composition root read the
    /// artifact alone for as long as it did. The fixture is drawn from gemma4,
    /// where they differ: `<eos>` is 1 and the turn closes with `<turn|>` at
    /// 106. What the terminator carries decides what is made resident after a
    /// generation, so a residency holding `<eos>` holds a turn the next
    /// rendering does not continue from.
    ///
    /// Perturbation: return the artifact's `eos` as the terminator and this
    /// fails on the first assertion. Watched under exactly that.
    #[test]
    fn the_terminator_is_the_familys_close_rather_than_the_artifacts_eos() {
        let promoted = promote_stop_conditions(
            &["<turn|>"],
            TokenId(1),
            vocabulary(&[("<turn|>", 106), ("<eos>", 1)]),
        )
        .expect("a family whose close promotes has a stop set");

        assert_eq!(
            promoted.terminator,
            TokenId(106),
            "the turn's close is what the family declared"
        );
        assert!(
            promoted.tokens.contains(&TokenId(106)),
            "and it stops the generation"
        );
        assert!(
            promoted.tokens.contains(&TokenId(1)),
            "the artifact's end-of-sequence joins the set as a backstop rather \
             than replacing it"
        );
        assert!(promoted.unpromoted.is_empty());
    }

    /// **A declared condition the vocabulary splits is named rather than
    /// carried.**
    ///
    /// Llama is the standing case: `<|eom_id|>` arrived with 3.1 and the
    /// tokenizer this workshop can reach is 3.0-era, which splits it into
    /// several tokens. A split marker can never equal one drawn token, so
    /// carrying it would be a stop condition that silently never fires.
    ///
    /// Perturbation: push the first token of a split condition instead of
    /// naming it, and the `unpromoted` assertion fails while a reader is told
    /// the family's whole set is in force.
    #[test]
    fn a_condition_the_vocabulary_splits_is_named_rather_than_carried() {
        let promoted = promote_stop_conditions(
            &["<|eot_id|>", "<|eom_id|>"],
            TokenId(128009),
            vocabulary(&[("<|eot_id|>", 128009)]),
        )
        .expect("the close promotes even where a later condition does not");

        assert_eq!(promoted.terminator, TokenId(128009));
        assert_eq!(
            promoted.unpromoted,
            vec!["<|eom_id|>"],
            "the condition that could never fire is reported"
        );
        assert_eq!(
            promoted.tokens,
            vec![TokenId(128009)],
            "and it is not in the set, nor is the EOS twice"
        );
    }

    /// **A family whose turn close does not promote refuses rather than
    /// running.**
    ///
    /// Every turn would generate to the token cap and leave the close in the
    /// emission as prose, which is a load that reports success and produces a
    /// trace no reader can bracket.
    #[test]
    fn a_close_that_does_not_promote_refuses_the_open() {
        let outcome = promote_stop_conditions(&["<turn|>"], TokenId(1), vocabulary(&[]));
        assert!(
            matches!(outcome, Err(DecodeFault::Engine { .. })),
            "the open refuses, got {outcome:?}"
        );
    }

    /// **A second admit refuses on the ordering rather than re-running the
    /// steps.** Nothing is idempotent: this crate begins empty, admits once,
    /// and dies.
    ///
    /// Perturbation: remove the `admit_attempted` guard from `admit` and this
    /// test fails, because the second call runs step one again and refuses as
    /// `Unresolvable` rather than as `AlreadyAttempted`. Watched under exactly
    /// that removal.
    #[test]
    fn a_second_admit_refuses_on_the_ordering() {
        let mut residency = Residency::new();
        let binding = ModelBinding {
            artifact: ArtifactRef("/nonexistent/artifact".into()),
            devices: vec![DeviceOrdinal(0)],
        };
        let first = residency
            .admit(&binding, Headroom(0), ReadoutElection(false), false)
            .err();
        assert!(
            matches!(first, Some(AdmitRefusal::Unresolvable { .. })),
            "the fixture refuses at step one, got {first:?}"
        );
        let second = residency
            .admit(&binding, Headroom(0), ReadoutElection(false), false)
            .err();
        assert_eq!(
            second,
            Some(AdmitRefusal::AlreadyAttempted),
            "the second admit refuses on the ordering, not on the artifact"
        );
    }

    /// **Release with nothing resident refuses.**
    #[test]
    fn release_without_residency_refuses() {
        let mut residency = Residency::new();
        assert_eq!(residency.release(), Err(LifecycleRefusal::NoResidency));
    }

    /// A minimal GGUF whose header carries exactly the one key the family
    /// lookup needs and resolves to exactly one registry entry, so an admit
    /// against it walks past steps one and two.
    ///
    /// **It declares an uncontested architecture on purpose.** The fixture
    /// carries a header and no chat template, and a contested architecture
    /// needs the template to reach an entry, so declaring one here would make
    /// this fixture refuse at selection and every test using it would read
    /// that refusal instead of the condition it means to exercise. `qwen2` is
    /// carried by one entry and needs no template to resolve.
    ///
    /// **It answers a guard rather than a path**, which removes the scratch
    /// directory when the test ends, pass or fail, as issue #704's guards do
    /// across the other crates. Its `Drop` runs on the unwind a failed
    /// assertion takes, so the caller binds it for the length of the test.
    fn resolvable_gguf() -> Fixture {
        use std::io::Write;
        let dir = std::env::temp_dir().join(format!("weaver-spu-residency-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a scratch dir");
        let path = dir.join("resolvable.gguf");
        let mut file = Vec::new();
        file.extend_from_slice(b"GGUF");
        file.extend_from_slice(&3u32.to_le_bytes());
        file.extend_from_slice(&0u64.to_le_bytes());
        file.extend_from_slice(&1u64.to_le_bytes());
        let key = b"general.architecture";
        file.extend_from_slice(&(key.len() as u64).to_le_bytes());
        file.extend_from_slice(key);
        file.extend_from_slice(&8u32.to_le_bytes());
        let value = b"qwen2";
        file.extend_from_slice(&(value.len() as u64).to_le_bytes());
        file.extend_from_slice(value);
        let mut handle = std::fs::File::create(&path).expect("a fixture file");
        handle.write_all(&file).expect("the fixture is written");
        Fixture { dir, path }
    }

    /// The fixture's path and the directory holding it, removed on drop.
    struct Fixture {
        dir: std::path::PathBuf,
        path: std::path::PathBuf,
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    /// **An ordinal that appears twice refuses on the set's shape, on the admit
    /// path.** Two shards of one artifact on one card is not a two-device
    /// admission: judged as distinct, the pair would pass room on half the real
    /// demand and skip the peer check.
    ///
    /// The fixture resolves and its header reads, so the admit walks past the
    /// free steps and the refusal read here is the shape judgment's own. A
    /// first version of this test called `judge_distinct` directly and could
    /// not fail under the call-site removal, which is why this one drives
    /// `admit`.
    ///
    /// Perturbation: remove the `judge_distinct` call from `admit` and this
    /// test fails, because the refusal becomes the driver condition's rather
    /// than the shape's. Watched under exactly that removal.
    #[test]
    fn a_duplicate_ordinal_refuses_on_the_admit_path() {
        let mut residency = Residency::new();
        let fixture = resolvable_gguf();
        let binding = ModelBinding {
            artifact: ArtifactRef(fixture.path.to_string_lossy().into_owned()),
            devices: vec![DeviceOrdinal(0), DeviceOrdinal(0)],
        };
        assert_eq!(
            residency
                .admit(&binding, Headroom(0), ReadoutElection(false), false)
                .err(),
            Some(AdmitRefusal::DuplicateDevice { ordinal: 0 }),
            "the refusal is the set's shape, before any driver condition"
        );
    }

    /// The floor's refusal set carries no family-naming case, so the name is
    /// lost at the boundary by design and kept on this side of it. This pins
    /// that the conversion is total rather than lossy in some other way.
    #[test]
    fn a_family_refusal_crosses_the_seam_as_a_floor_case() {
        let refusal = AdmitRefusal::Family(FamilyRefusal::UnknownFamily(FamilyName("x".into())));
        assert_eq!(
            LifecycleRefusal::from(refusal),
            LifecycleRefusal::ArtifactUnreadable
        );
    }
}
