//! conforms: spu-session-never-rewinds
//! conforms: spu-no-scoped-clear-surface
//! conforms: spu-overflow-refuses-sheds-nothing
//! conforms: spu-terminator-on-every-path
//! conforms: spu-closing-count-includes-the-terminator
//! conforms: spu-cancel-bounded-by-one-token
//! conforms: spu-cancel-polled-not-signalled
//! conforms: spu-flush-mechanism-from-declaration
//! conforms: spu-elision-refuses-an-unremovable-span
//! conforms: spu-elision-removes-its-span-in-order
//! conforms: spu-signals-pre-sampler
//! conforms: spu-sampler-holds-nothing-between-generations
//!
//! The resident session and its append path, per `weaver-spu-Spec` sections 4.2
//! to 4.4.
//!
//! **Append-only, and nothing ever rewinds.** The session holds the resident
//! token sequence, per `weaver-spu-Spec` section 4.4 as of the cut act - the
//! re-establishing flush mechanism needs the tokens it re-decodes - and every
//! generation decodes only the delta at absolute positions from the sequence's
//! end forward. The archived tree's proof is the reason nothing rewinds: a
//! rewind fails silently on families that keep recurrent layers, because the
//! clear returns success while the recurrent state stays, and the failure
//! surfaces later as a position error far from its cause.
//!
//! **This crate calls no scoped-clear on a resident range**, which is the
//! discipline stated as an absence. There is no method here that clears a range,
//! and the pin below is that its absence is a compile error to depend on:
//!
//! ```compile_fail
//! use weaver_spu::decoder::session::Session;
//! fn pin(session: &mut Session) {
//!     // No scoped clear exists on this type, and this is what says so. The
//!     // day someone adds one, this doctest starts compiling and the pin fires.
//!     session.clear_range(0, 5);
//! }
//! ```
//!
//! The behaviour and the surface are two records with two instruments: the
//! resident length moving only by appends and the recorded reducers is a
//! perturbation test and the missing method is this compile-fail pin. Neither
//! claims the other's half, because a pin says a rewind cannot be written and
//! says nothing about a turn that re-prefills through the append path instead.

use crate::decoder::backend::{Backend, DecodeFault, FlushMechanism, TokenId};
use crate::measurement;

/// Why a generation stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stopped {
    /// The family's stop condition was reached.
    Complete,
    /// The harness cancelled, checked at a token boundary.
    Cancelled,
    /// The session reached its capacity mid-generation.
    CapacityReached,
    /// The turn's token limit was reached, and that limit alone, per
    /// `weaver-types-Spec` section 4.4's Length case: a cap that exited as
    /// complete left the record unable to distinguish a finished answer
    /// from a cut one, per issue #218.
    LimitReached,
}

/// What one generation produced.
#[derive(Debug, Clone, PartialEq)]
pub struct Generated {
    /// The tokens sampled, terminator excluded: it is resident but it is not
    /// something the model said.
    pub tokens: Vec<TokenId>,
    /// Why the loop ended. **A cancelled generation answers with what it
    /// produced, marked stopped, after the terminator lands.**
    pub stopped: Stopped,
    /// What the distribution measured at each step, per Spec section 6.
    ///
    /// **Positionally paired with `tokens`.** Absent where the backend served
    /// no distribution, and absent rather than empty where nothing was drawn.
    pub signals: measurement::Signals,
    /// The prefill span in nanoseconds: the delta's decode, before the loop.
    /// Measured here because the span runs here, the two readings of the
    /// record's `DecodeTimings` divided where the work divides.
    pub prefill_ns: u64,
    /// The decode span in nanoseconds: the sampling loop through the
    /// terminator's landing.
    pub decode_ns: u64,
    /// The stop token that ended the generation, drawn and not retained:
    /// present only where a stop ended it, absent for a length cut, a cancel,
    /// a capacity stop, and a re-feed, which draws no stop of its own. The
    /// parse reads it to tell a closing marker that came as the stop from one
    /// that never came, per Spec section 5.
    pub drawn_stop: Option<TokenId>,
    /// The residual reduction's figures where the residency was admitted
    /// with readout elected, per `weaver-spu-Spec` sections 6 and 7: one
    /// norm per layer per forward this generation ran, in order, drained
    /// from the tap when the generation closes. Absent rather than empty
    /// where nothing tapped, the record's own discipline.
    /// **The reduction itself rather than its figures**, so the shape
    /// travels with them. A bare array left a reader dividing by the token
    /// count plus one to recover the layer count, an arithmetic no document
    /// stated and which holds only while every forward taps every layer.
    pub residual: Option<crate::readout::Reduction>,
}

/// The family's stop condition and its terminator.
#[derive(Debug, Clone, PartialEq)]
pub struct StopCondition {
    /// Tokens that end a generation when sampled.
    pub stop_tokens: Vec<TokenId>,
    /// The token made resident after the loop on every path, so the next turn's
    /// framing begins at a boundary rather than mid-message.
    pub terminator: TokenId,
    /// A ceiling on sampled tokens, so a model that never emits a stop token
    /// still ends.
    pub max_tokens: usize,
}

/// The cancel, **polled rather than signalled**.
///
/// The decode socket is the one carrier, so the loop polls it between tokens
/// rather than waiting on it. That is why the check is a boundary check rather
/// than a signal handler: nothing asynchronous is needed to make a
/// token-boundary stop, and nothing asynchronous is introduced. A cancel is the
/// harness asking this process to stop, so it is a seam, and apex section 5.1
/// admits no seam across a process line that is not a socket.
pub trait CancelPoll {
    /// Answer without blocking. A poll that blocked would defeat the bound this
    /// exists to provide.
    fn cancelled(&mut self) -> bool;
}

/// A cancel that never fires, for callers with nothing to poll.
pub struct NeverCancels;

impl CancelPoll for NeverCancels {
    fn cancelled(&mut self) -> bool {
        false
    }
}

/// What the sampler is built from at the start of every generation, per
/// `weaver-spu-Spec` section 8.5.
///
/// **Grouped because the two fix one reseed together and nothing else.** The
/// seed is the caller's, derived from the declared one and the generation's
/// identity, and crosses to [`Backend::reseed`]. The window is the length of
/// resident tail the penalty knobs describe, and it is spent on this side of
/// that seam, cutting the slice the reseed is handed rather than crossing as
/// an argument of its own. A generation and a re-feed build the sampler the
/// same way, which is why one type serves both.
pub struct SamplerBuild {
    /// This generation's derived seed.
    pub seed: u64,
    /// How much of the resident tail the penalty reads, in tokens.
    pub penalty_window: usize,
}

/// Where a retained position's field lands: the position, the ranked
/// candidates, and the realized rank.
pub type FieldSink<'s> = &'s mut dyn FnMut(u64, Vec<(u32, f32)>, u32);

/// The positioned intermediates of the decode contract's section 2, and the
/// field's election that governs one of them.
///
/// **What the generate and the re-feed drive hand out per position, the
/// token stream excepted.** The field and the column each carry the position
/// they pair with, because neither crosses per renderable piece the way the
/// token does, and the re-feed drive produces both while streaming no token,
/// which is why the token sink is not a member here.
///
/// **The field's depth and its sink are one member rather than two**, because
/// either alone is meaningless: a depth with nowhere to put the measurement
/// runs the ranking and drops it, and a sink with no depth never fires. Held
/// as a pair, absent where unelected per `weaver-spu-Spec` section 7.5, the
/// type admits neither half-state rather than naming the rule in prose and
/// leaving it representable.
pub struct PositionedSinks<'s> {
    /// The field's elected depth paired with where each retained position's
    /// field lands, or `None` where the election does not stand. The sink
    /// takes the position, the ranked candidates, and the realized rank.
    pub field: Option<(usize, FieldSink<'s>)>,
    /// Where each sampled position's column lands, where the ask stood.
    pub on_column: &'s mut dyn FnMut(u64, Vec<Vec<f32>>),
}

/// The resident session.
///
/// **The resident sequence moves only by appends and by the recorded
/// reducers.** It is private, and the paths that reduce it are
/// [`Session::flush`], to the cut's kept length and never below the prefix,
/// [`Session::elide`], by exactly its span and never into the prefix, and the
/// close over a backend fault, which clears it alongside `opened` because a
/// closed session accounts for nothing. Nothing else writes it downward over a
/// session that keeps serving, which is the discipline the perturbation test
/// watches from outside.
pub struct Session<'a> {
    /// **The backend borrows the residency it decodes against**, so a session
    /// cannot outlive the model it was opened over. The lifetime is what says
    /// so: an engine holds a context, a context borrows its model, and the
    /// residency owns the model. Nothing has to remember the rule.
    backend: Box<dyn Backend + 'a>,
    resident: Vec<TokenId>,
    prefix_len: usize,
    capacity: usize,
    opened: bool,
    /// Set by the close over a backend fault and never cleared. `opened` alone
    /// cannot carry this: a fresh session and a poisoned one both read as not
    /// opened, and `open` must refuse the second while serving the first,
    /// because the poisoned session's backend has been closed and a re-open
    /// would decode against a closed engine.
    poisoned: bool,
    flush_mechanism: FlushMechanism,
    /// Whether the column ask stood at the open, per charter section 13.7:
    /// the one ask whose answer runs for the residency. Where it stands
    /// the draw sites take the tap's held columns and hand them out, and
    /// where it does not no take happens at all.
    columns_asked: bool,
    /// Whether the per-position surprisal is produced, per charter section
    /// 13.12.
    ///
    /// **It sits on the session rather than on the generation because it is
    /// fixed for the load.** The field's election is per call, the depth
    /// being a thing a caller could vary, and this one cannot vary: the
    /// operator elected it once at admit and every generation of the
    /// residency answers to that. A parameter on the generation would
    /// invite a caller to disagree with the load it is serving.
    surprisal_elected: bool,
}

/// Checked, immutable placement facts from a serviceable open session.
/// A proposed keep is a count, never a buffer handle or permission to cut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlacementPosition {
    pub resident: u64,
    pub prefix_tokens: u64,
    pub keep: u64,
    pub capacity: u64,
}

impl<'a> Session<'a> {
    pub fn new(
        backend: Box<dyn Backend + 'a>,
        capacity: usize,
        flush_mechanism: FlushMechanism,
        surprisal_elected: bool,
    ) -> Session<'a> {
        Session {
            backend,
            resident: Vec::new(),
            prefix_len: 0,
            capacity,
            opened: false,
            poisoned: false,
            flush_mechanism,
            columns_asked: false,
            surprisal_elected,
        }
    }

    /// Arm the column answer, per the open's one ask: the tap's hold and
    /// this session's take stand together for the residency, and no call
    /// disarms them because no directive un-asks.
    pub fn ask_columns(&mut self) {
        self.columns_asked = true;
        self.backend.hold_columns(true);
    }

    /// What is resident now. The session's own account of itself, which the
    /// overflow refusal hands back to the harness.
    /// The resident sequence itself, for tests and for the elision's own
    /// assertion: the counts cannot show that surviving positions kept
    /// their order, and order is what the decode contract guarantees.
    // Test-only hook kept here (#118): a session method the suite reads.
    #[cfg(test)]
    pub fn resident_tokens(&self) -> &[TokenId] {
        &self.resident
    }

    pub fn resident_len(&self) -> usize {
        self.resident.len()
    }

    /// Where the identity prefix ends. **The prefix is established at open and
    /// is permanent:** over a session that keeps serving, no operation reduces
    /// the length below this - the flush bounds here, the elision refuses a
    /// span that reaches in, and the close over a backend fault clears the
    /// account with the session.
    pub fn prefix_len(&self) -> usize {
        self.prefix_len
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Read the actual session position for SPU-MEASURE without touching the
    /// backend, sampler or readout. Clamp in u64 space, so a large wire count
    /// cannot wrap through usize on a narrower host. Flush uses the same bounds.
    /// Closed or poisoned sessions certify no resident state.
    pub fn placement_position(&self, keep: u64) -> Result<PlacementPosition, DecodeFault> {
        if !self.opened {
            return Err(DecodeFault::NotOpen);
        }
        let count = |value: usize| {
            u64::try_from(value).map_err(|_| DecodeFault::Engine {
                detail: "session count does not fit the placement contract".into(),
            })
        };
        let resident = count(self.resident.len())?;
        let prefix_tokens = count(self.prefix_len)?;
        Ok(PlacementPosition {
            resident,
            prefix_tokens,
            keep: prefix_tokens.max(keep.min(resident)),
            capacity: count(self.capacity)?,
        })
    }

    /// Render the identity prefix resident, once, and record the length it
    /// produced.
    pub fn open(&mut self, prefix: &[TokenId]) -> Result<(), DecodeFault> {
        // Open is first and happens once. Without this guard a second open
        // writes the resident length back down over accumulated turns with no
        // backend truncate or reestablish beneath it, which is the silent
        // rewind the module header forbids.
        if self.opened {
            return Err(DecodeFault::AlreadyOpen);
        }
        // A poisoned session is not a fresh one: its backend is closed, so
        // there is nothing here to open over.
        if self.poisoned {
            return Err(DecodeFault::NotOpen);
        }
        if prefix.len() > self.capacity {
            return Err(DecodeFault::Overflow {
                resident: 0,
                requested: prefix.len(),
                capacity: self.capacity,
            });
        }
        self.backend.decode_at(prefix, 0)?;
        self.resident = prefix.to_vec();
        self.prefix_len = prefix.len();
        self.opened = true;
        Ok(())
    }

    /// Append the delta at the resident end and generate until the family's stop
    /// condition, the harness's cancel, or the session's capacity.
    ///
    /// **The terminator is made resident before this returns, on every path.**
    /// The archived tree's hard-won correctness note: a generation that stops at
    /// the end-of-generation marker without decoding it leaves the terminator
    /// absent, and the next turn's framing is then malformed at a boundary
    /// nobody looks at.
    pub fn append_and_generate(
        &mut self,
        delta: &[TokenId],
        stop: &StopCondition,
        cancel: &mut dyn CancelPoll,
        on_token: &mut dyn FnMut(TokenId),
        mut sinks: PositionedSinks<'_>,
        sampler: SamplerBuild,
    ) -> Result<Generated, DecodeFault> {
        if !self.opened {
            return Err(DecodeFault::NotOpen);
        }

        // **Overflow refuses and sheds nothing.** This crate evicts and compacts
        // nothing: either would be this crate deciding which part of the agent's
        // context matters, which is the harness's domain. The refusal carries
        // the session's own account so the loop decides with the fact rather
        // than without it. One slot is reserved for the terminator, because a
        // delta that fits only by displacing it would produce the malformed
        // framing the terminator discipline exists to prevent.
        let needed = delta.len() + 1;
        if self.resident.len() + needed > self.capacity {
            return Err(DecodeFault::Overflow {
                resident: self.resident.len(),
                requested: delta.len(),
                capacity: self.capacity,
            });
        }

        // Any reduction standing from the prefix decode or an earlier
        // generation is drained and dropped, so what this generation
        // reports is its own forwards and nothing staler.
        let _ = self.backend.take_reduction();

        let prefill_started = std::time::Instant::now();
        if let Err(fault) = self.backend.decode_at(delta, self.resident.len()) {
            return Err(self.poison(fault));
        }
        self.resident.extend_from_slice(delta);
        let prefill_ns = prefill_started.elapsed().as_nanos() as u64;

        // **The sampler is built here, after the delta is resident and
        // before the first draw**, per `weaver-spu-Spec` section 8.5. It
        // holds nothing from the last generation: the seed is the
        // caller's, derived from the declared one and this generation's
        // identity, and the penalty window is the resident tail read now.
        //
        // **After the delta rather than before it, because the delta is in
        // the window.** The native path reads the tail live at each draw
        // and therefore sees the delta's tokens, so a window taken before
        // the prefill would penalise a different set on the two engines,
        // which is the divergence this section's one-source rule exists to
        // prevent. After a flush the tail is the truncated tail, which is
        // why nothing needs clearing.
        //
        // **And after the overflow check**, so a refused generation
        // reseeds nothing: a refusal that had already rebuilt the sampler
        // would leave the session altered by a call that answered an
        // error.
        //
        // **A failure here poisons rather than returns**, because the delta
        // is already decoded and resident above. A fault that answered
        // plainly would leave a session whose state holds a delta no
        // generation ever consumed, appendable and quietly wrong, which is
        // the disagreement the poison exists to refuse. This sits after a
        // commit and therefore takes the commit's discipline.
        let window_start = self.resident.len().saturating_sub(sampler.penalty_window);
        let window: Vec<TokenId> = self.resident[window_start..].to_vec();
        if let Err(fault) = self.backend.reseed(sampler.seed, &window) {
            return Err(self.poison(fault));
        }
        let decode_started = std::time::Instant::now();

        let mut produced = Vec::new();
        let mut signals = measurement::Accumulator::new(self.surprisal_elected);
        let mut drawn_stop = None;
        let stopped = loop {
            // **The cancel is checked between sampled tokens**, which bounds
            // the stop by one token's decode rather than by a kernel's
            // completion. Checked before sampling, so a cancel that arrives
            // during the previous token's decode costs no further token.
            if cancel.cancelled() {
                break Stopped::Cancelled;
            }
            if produced.len() >= stop.max_tokens {
                break Stopped::LimitReached;
            }
            // A slot for the terminator is held back, so capacity is reached
            // one short of the true end.
            if self.resident.len() + 1 >= self.capacity {
                break Stopped::CapacityReached;
            }

            // **Measured before the sampler**, per Spec section 6. The
            // distribution exists at this point and not after: `sample`
            // consumes it and answers one token, so a measurement taken below
            // would be reading a state the draw has already moved past. The
            // index the surprisal needs is the token's own, so the entropy is
            // taken here and the surprisal completed once the draw is known,
            // both from the same distribution rather than from two.
            let step = match self.backend.distribution() {
                Ok(logits) => Some((measurement::entropy_bits(logits), logits.to_vec())),
                // A backend serving no distribution measures nothing rather
                // than faulting the generation. The signals are observability
                // and an absent one is a reported absence, per section 6.
                Err(_) => None,
            };
            // **The column crosses at the draw moment where the ask
            // stood**, per charter section 13.7's position bound: what the
            // take answers is the forward that feeds this draw - the
            // append's prefill final on the first step, the decode forward
            // after it - and the position is the one this draw's token
            // would fill, the field's own pairing. The terminator's forward
            // is never taken, no draw following it.
            if self.columns_asked {
                // **An asked column that does not arrive at a sampled
                // position is charter 13.10's fault**, per `weaver-spu-Spec`
                // section 7: the ask passed the registry, so a backend whose
                // tap holds no column - the native tap's own shape - faults
                // loudly here rather than omitting silently, the silent
                // absence being what the fault rule forbids.
                match self.backend.take_columns() {
                    Some(columns) => (sinks.on_column)(self.resident.len() as u64, columns),
                    None => {
                        return Err(self.poison(DecodeFault::Engine {
                            detail: "the column ask stood and no column arrived at a \
                                     sampled position"
                                .into(),
                        }));
                    }
                }
            }

            let token = match self.backend.sample() {
                Ok(token) => token,
                Err(fault) => return Err(self.poison(fault)),
            };

            // **The stop token is drawn but not retained, so it is not
            // measured.** The signals are positionally paired with the tokens
            // this answers with, and a measurement for a token the answer does
            // not carry shifts every reader's pairing by one.
            if stop.stop_tokens.contains(&token) {
                drawn_stop = Some(token);
                break Stopped::Complete;
            }

            // Retained from here, so this token owes a measurement. A step that
            // cannot produce one abandons the whole run rather than leaving a
            // vector one short: a gap misattributes every entry after it, and a
            // reader cannot see that it happened.
            // **The field is produced here or nowhere**, per
            // `weaver-spu-Spec` section 7.5: the logits above are already
            // held for the signals and the draw is now known, so the
            // ranking costs no second read of the distribution and no
            // second copy of it. Absent where unelected, and absent rather
            // than empty where the distribution carried none.
            if let Some((depth, on_field)) = sinks.field.as_mut()
                && let Some((entropy_and_logits, position)) =
                    step.as_ref().map(|s| (s, self.resident.len() as u64))
                && let Some((ranked, realized)) =
                    measurement::field(&entropy_and_logits.1, token.0 as usize, *depth)
            {
                on_field(position, ranked, realized);
            }
            match step.and_then(|(entropy, logits)| {
                measurement::surprisal_bits(&logits, token.0 as usize)
                    .map(|surprisal| (entropy, surprisal))
            }) {
                Some((entropy, surprisal)) => signals.record(entropy, surprisal),
                None => signals.abandon(),
            }
            produced.push(token);
            // **The stream fires for every retained token and no other**, per
            // the contract's coherence guarantee: the stop token is drawn and
            // not retained, so it is not streamed, and the close's account is
            // exactly the streamed sequence.
            on_token(token);
            if let Err(fault) = self.backend.decode_at(&[token], self.resident.len()) {
                return Err(self.poison(fault));
            }
            self.resident.push(token);
        };

        // Every path above converges here. The terminator lands on the clean
        // path and the cancelled path alike, which is what makes the cancel of
        // charter section 13.5 leave a well-framed session. The engine-fault
        // paths do not reach here and do not pretend to: a backend that just
        // faulted cannot be asked to decode a terminator, so those paths close
        // the session instead, and a closed session is the honest state where a
        // well-framed one cannot be produced.
        if let Err(fault) = self
            .backend
            .decode_at(&[stop.terminator], self.resident.len())
        {
            return Err(self.poison(fault));
        }
        self.resident.push(stop.terminator);

        Ok(Generated {
            tokens: produced,
            stopped,
            drawn_stop,
            signals: signals.finish(),
            prefill_ns,
            decode_ns: decode_started.elapsed().as_nanos() as u64,
            residual: self.backend.take_reduction(),
        })
    }

    /// The re-feed drive, per `weaver-spu-PRD` section 13.14 and
    /// `weaver-spu-Spec` section 4.6: the append's machinery with the draw's
    /// destination changed. The rendered contribution arrives already
    /// tokenized by the caller - no family rendering on the way, per the
    /// ruling of 2026-08-12 - the forward passes run along the recorded
    /// path, and at each position the draw is computed exactly as a
    /// generation would compute it, **and the recorded token appends
    /// whatever the draw said**, so a divergent position stays comparable
    /// instead of cascading.
    ///
    /// **The recomputed draws land in `Generated::tokens`**, the slots the
    /// sampled identifiers would fill, per the contract: the answer's own
    /// variant is what keeps a supplied path out of a sampled path's
    /// clothes, and the caller wraps this return under it. The signals are
    /// the recorded token's - the entropy of each distribution and the
    /// recorded token's surprisal in it - because those are the figures the
    /// source measurement holds and the comparison reads. The field's
    /// `realized` is likewise the recorded token's rank, comparability
    /// being the reader pass's whole point.
    ///
    /// **No token streams**: the drive draws nothing of its own, so there
    /// is no drawn piece to cross, and the caller passes no token sink.
    /// The stop set does not apply - the recorded path is what it is, and
    /// a stop token inside it was the source's business - and the
    /// terminator lands after the path exactly as it landed after the
    /// source's generation, so the resident state mirrors the source's.
    pub fn refeed(
        &mut self,
        delta: &[TokenId],
        path: &[TokenId],
        terminator: TokenId,
        cancel: &mut dyn CancelPoll,
        mut sinks: PositionedSinks<'_>,
        sampler: SamplerBuild,
    ) -> Result<Generated, DecodeFault> {
        if !self.opened {
            return Err(DecodeFault::NotOpen);
        }
        // Overflow refuses and sheds nothing, exactly as the generate does,
        // the whole recorded extent judged upfront because the path's length
        // is known where a generation's is not.
        let needed = delta.len() + path.len() + 1;
        if self.resident.len() + needed > self.capacity {
            return Err(DecodeFault::Overflow {
                resident: self.resident.len(),
                requested: delta.len() + path.len(),
                capacity: self.capacity,
            });
        }
        let _ = self.backend.take_reduction();
        let prefill_started = std::time::Instant::now();
        if let Err(fault) = self.backend.decode_at(delta, self.resident.len()) {
            return Err(self.poison(fault));
        }
        self.resident.extend_from_slice(delta);
        let prefill_ns = prefill_started.elapsed().as_nanos() as u64;
        // The sampler is built exactly as a generation builds it - the
        // derived seed, the resident tail's window - because the recomputed
        // draw is only evidence if it is the draw the source would have
        // made.
        let window_start = self.resident.len().saturating_sub(sampler.penalty_window);
        let window: Vec<TokenId> = self.resident[window_start..].to_vec();
        if let Err(fault) = self.backend.reseed(sampler.seed, &window) {
            return Err(self.poison(fault));
        }
        let decode_started = std::time::Instant::now();
        let mut recomputed = Vec::new();
        let mut signals = measurement::Accumulator::new(self.surprisal_elected);
        let mut stopped = Stopped::Complete;
        for recorded in path {
            if cancel.cancelled() {
                stopped = Stopped::Cancelled;
                break;
            }
            let step = match self.backend.distribution() {
                Ok(logits) => Some((measurement::entropy_bits(logits), logits.to_vec())),
                Err(_) => None,
            };
            // The column at the draw moment, exactly as the generate takes
            // it: the re-fed forward is the source's, so the position bound
            // and the pairing are unchanged.
            if self.columns_asked {
                // The generate's own 13.10 rule, unchanged by the re-feed:
                // an asked column arrives at every sampled position or the
                // pass faults.
                match self.backend.take_columns() {
                    Some(columns) => (sinks.on_column)(self.resident.len() as u64, columns),
                    None => {
                        return Err(self.poison(DecodeFault::Engine {
                            detail: "the column ask stood and no column arrived at a \
                                     sampled position"
                                .into(),
                        }));
                    }
                }
            }
            let draw = match self.backend.sample() {
                Ok(token) => token,
                Err(fault) => return Err(self.poison(fault)),
            };
            if let Some((depth, on_field)) = sinks.field.as_mut()
                && let Some((_, logits)) = step.as_ref()
                && let Some((ranked, realized)) =
                    measurement::field(logits, recorded.0 as usize, *depth)
            {
                on_field(self.resident.len() as u64, ranked, realized);
            }
            match step.and_then(|(entropy, logits)| {
                measurement::surprisal_bits(&logits, recorded.0 as usize)
                    .map(|surprisal| (entropy, surprisal))
            }) {
                Some((entropy, surprisal)) => signals.record(entropy, surprisal),
                None => signals.abandon(),
            }
            recomputed.push(draw);
            // **The recorded token appends, whatever the draw said.**
            if let Err(fault) = self.backend.decode_at(&[*recorded], self.resident.len()) {
                return Err(self.poison(fault));
            }
            self.resident.push(*recorded);
        }
        if let Err(fault) = self.backend.decode_at(&[terminator], self.resident.len()) {
            return Err(self.poison(fault));
        }
        self.resident.push(terminator);
        Ok(Generated {
            tokens: recomputed,
            stopped,
            drawn_stop: None,
            signals: signals.finish(),
            prefill_ns,
            decode_ns: decode_started.elapsed().as_nanos() as u64,
            residual: self.backend.take_reduction(),
        })
    }

    /// Reach the ask's kept length: the session's first `keep` tokens
    /// resident, everything beyond them gone, per `weaver-spu-Spec` section
    /// 4.4 as of the cut act.
    ///
    /// **The cut is bounded as arithmetic, never refused**: below by the
    /// identity prefix, whose permanence is the decode seam's own guarantee,
    /// and above by the resident count, where there is nothing to cut. The
    /// caller reads what held from the resident length, which is why the
    /// confirmation's counts are taken around this call.
    ///
    /// **The mechanism is per family.** The family declares which it is and
    /// this reads the declaration rather than inferring it from a version
    /// string.
    pub fn flush(&mut self, keep: usize) -> Result<(), DecodeFault> {
        if !self.opened {
            return Err(DecodeFault::NotOpen);
        }
        let kept = keep.max(self.prefix_len).min(self.resident.len());
        let outcome = match self.flush_mechanism {
            FlushMechanism::TruncateToPosition => self.backend.truncate_to(kept),
            FlushMechanism::ReestablishAndReprefill => self
                .backend
                .reestablish()
                .and_then(|()| self.backend.decode_at(&self.resident[..kept], 0)),
        };
        // A flush that fails partway leaves the backend's state and the
        // session's account disagreeing: a reestablish that succeeded before a
        // re-prefill that failed is an empty backend under a large resident
        // length, and the next append would decode at an absolute position far
        // past the backend's real state, the position-error-far-from-cause
        // failure the append-only discipline exists to prevent. The session
        // closes instead of carrying the disagreement.
        if let Err(fault) = outcome {
            return Err(self.poison(fault));
        }
        // One of the type's two reducers - the elision at `elide` is the
        // other - and this one reduces to the bounded cut rather than to an
        // arbitrary position.
        self.resident.truncate(kept);
        Ok(())
    }

    /// Make a half-open span of the resident sequence absent, per charter
    /// section 13.13. Answers the resident counts either side.
    ///
    /// **The span is state and never the record.** These positions index
    /// the sequence this type holds resident, and nothing in this program
    /// removes anything from a trace.
    ///
    /// **The bounds refuse rather than clamp**, which is the flush's rule
    /// reversed and deliberately: an over-large `keep` describes less
    /// context than exists and the arithmetic satisfies it without
    /// misreporting, while a span overlapping the identity prefix, running
    /// past the resident count, ending before it starts, or empty
    /// describes no removable region and has no smaller true version.
    ///
    /// conforms: spu-elision-refuses-an-unremovable-span
    /// conforms: spu-elision-removes-its-span-in-order
    pub fn elide(&mut self, from: usize, to: usize) -> Result<(usize, usize), DecodeFault> {
        if !self.opened {
            return Err(DecodeFault::NotOpen);
        }
        if from < self.prefix_len || to > self.resident.len() || to <= from {
            return Err(DecodeFault::UnremovableSpan {
                from: from as u64,
                to: to as u64,
                prefix: self.prefix_len as u64,
                resident: self.resident.len() as u64,
            });
        }
        let before = self.resident.len();
        // The sequence the session will hold: what preceded the span
        // followed by what followed it, every surviving position keeping
        // its order, per the decode contract's no-resequencing guarantee.
        let mut kept: Vec<TokenId> = Vec::with_capacity(before - (to - from));
        kept.extend_from_slice(&self.resident[..from]);
        kept.extend_from_slice(&self.resident[to..]);
        // **An interior removal has no truncation mechanism.** The flush
        // can roll a backend back to a position because its outcome is a
        // prefix of what stood. This outcome is not a prefix of anything
        // the backend holds, so the sequence is re-established whatever the
        // family's flush mechanism is, per charter 13.13's rule that the
        // outcome is specified and the mechanism the Spec's.
        let outcome = self
            .backend
            .reestablish()
            .and_then(|()| self.backend.decode_at(&kept, 0));
        // A failure partway leaves the backend and this account
        // disagreeing, exactly as a failed flush does, so the session
        // closes rather than carrying the disagreement.
        if let Err(fault) = outcome {
            return Err(self.poison(fault));
        }
        self.resident = kept;
        Ok((before, self.resident.len()))
    }

    pub fn close(&mut self) {
        self.backend.close();
        self.opened = false;
    }

    /// Close the session over a backend fault and hand the fault back.
    ///
    /// After an engine fault or a failed flush the backend's state and this
    /// session's account of it can no longer be trusted to agree, so the
    /// session stops being appendable rather than carrying the disagreement
    /// forward. Every later operation answers [`DecodeFault::NotOpen`], which
    /// is the truth: no serviceable prefix stands.
    fn poison(&mut self, fault: DecodeFault) -> DecodeFault {
        self.poisoned = true;
        self.backend.close();
        self.opened = false;
        self.resident.clear();
        fault
    }
}

#[cfg(test)]
mod tests;
