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
mod tests {
    use super::*;

    use std::cell::RefCell;
    use std::rc::Rc;

    /// What the double saw, held behind a shared handle so a test can read it
    /// back after the session has taken ownership of the backend.
    #[derive(Default)]
    struct Log {
        /// Every reseed the session asked for, in order: the seed it
        /// derived and the penalty window it read from the resident tail.
        reseeds: Vec<(u64, Vec<TokenId>)>,
        decoded: Vec<(Vec<TokenId>, usize)>,
        truncated: Vec<usize>,
        reestablished: usize,
        /// What `sample` returns, in order. Exhausting it yields token 0.
        script: Vec<TokenId>,
        sampled: usize,
        /// Fail the nth `decode_at` call, counting from zero.
        fail_decode_call: Option<usize>,
        /// Fail every `sample`.
        fail_sample: bool,
        /// The order the seam's two read paths were called in, which is what
        /// the pre-sampler claim is about.
        order: Vec<&'static str>,
        /// Fail the nth `distribution` call, counting from zero.
        fail_distribution_call: Option<usize>,
        distributions: std::cell::Cell<usize>,
        /// What `take_columns` answers. `None` is the unarmed tap, which is
        /// what every test that does not reach for the column wants.
        columns: Option<Vec<Vec<f32>>>,
    }

    /// A backend that records what it was asked to do. The seam this doubles is
    /// the one Spec section 4.1 declares, so doubling it is reading the seam
    /// rather than inventing one.
    /// A fixed distribution the double serves. Non-uniform so the entropy is
    /// a number rather than a degenerate zero, and **wide enough to cover
    /// every token id the scripts draw**: a real distribution spans the
    /// vocabulary, so a fixture narrower than its own script would measure
    /// nothing on the tokens past its end and misreport the pairing.
    const DISTRIBUTION: &[f32] = &[
        1.0, 2.0, 3.0, 4.0, 1.0, 2.0, 3.0, 4.0, 1.0, 2.0, 3.0, 4.0, 1.0, 2.0, 3.0, 4.0,
    ];

    struct Recorder(Rc<RefCell<Log>>, Vec<f32>);

    impl Backend for Recorder {
        /// The seam's own, recorded rather than acted on: this backend
        /// samples from a script, so what a reseed does here is note that
        /// it happened and with what.
        fn reseed(&mut self, seed: u64, window: &[TokenId]) -> Result<(), DecodeFault> {
            self.0.borrow_mut().reseeds.push((seed, window.to_vec()));
            Ok(())
        }

        fn decode_at(&mut self, tokens: &[TokenId], position: usize) -> Result<(), DecodeFault> {
            let mut log = self.0.borrow_mut();
            let call = log.decoded.len();
            log.decoded.push((tokens.to_vec(), position));
            if log.fail_decode_call == Some(call) {
                return Err(DecodeFault::Engine {
                    detail: "scripted".into(),
                });
            }
            Ok(())
        }
        fn distribution(&self) -> Result<&[f32], DecodeFault> {
            let call = {
                let mut log = self.0.borrow_mut();
                log.order.push("distribution");
                let call = log.distributions.get();
                log.distributions.set(call + 1);
                (call, log.fail_distribution_call)
            };
            if Some(call.0) == call.1 {
                return Err(DecodeFault::Engine {
                    detail: "no distribution".into(),
                });
            }
            // **The double serves a fixed distribution**, which is enough for
            // the ordering claim: what the signals test reads is that the
            // measurement was taken from the state the sampler had not yet
            // consumed, not what the numbers were.
            Ok(&self.1)
        }
        fn sample(&mut self) -> Result<TokenId, DecodeFault> {
            let mut log = self.0.borrow_mut();
            log.order.push("sample");
            if log.fail_sample {
                return Err(DecodeFault::Engine {
                    detail: "scripted".into(),
                });
            }
            let token = log.script.get(log.sampled).copied().unwrap_or(TokenId(0));
            log.sampled += 1;
            Ok(token)
        }
        fn take_columns(&mut self) -> Option<Vec<Vec<f32>>> {
            self.0.borrow().columns.clone()
        }
        fn truncate_to(&mut self, position: usize) -> Result<(), DecodeFault> {
            self.0.borrow_mut().truncated.push(position);
            Ok(())
        }
        fn reestablish(&mut self) -> Result<(), DecodeFault> {
            self.0.borrow_mut().reestablished += 1;
            Ok(())
        }
        fn close(&mut self) {}
    }

    /// **Each positioned sink fires at the position it names, and the
    /// unelected field fires at none.**
    ///
    /// The field's depth and its sink are one member, so what holds the
    /// pairing is a run that elects a depth and counts what arrives against a
    /// run that elects none and counts nothing. Both sinks are dispatched
    /// from the same loop and carry the same shape of position, which is the
    /// pair a wrong wiring would swap without the compiler noticing, the
    /// ranked candidates and the layer stack being different types only
    /// inside their vectors.
    #[test]
    fn each_positioned_sink_fires_at_the_position_it_names() {
        let columns = vec![vec![0.5_f32, 0.25], vec![0.125, 0.0625]];
        let log = Rc::new(RefCell::new(Log {
            script: vec![TokenId(1), TokenId(2), TokenId(0)],
            columns: Some(columns.clone()),
            ..Default::default()
        }));
        let mut session = Session::new(
            Box::new(Recorder(Rc::clone(&log), DISTRIBUTION.to_vec())),
            64,
            FlushMechanism::TruncateToPosition,
            false,
        );
        session.open(&[TokenId(7)]).expect("the prefix lands");
        session.ask_columns();

        let mut fields: Vec<(u64, usize, u32)> = Vec::new();
        let mut taken: Vec<(u64, Vec<Vec<f32>>)> = Vec::new();
        let mut on_field = |position: u64, ranked: Vec<(u32, f32)>, realized: u32| {
            fields.push((position, ranked.len(), realized));
        };
        session
            .append_and_generate(
                &[TokenId(8)],
                &StopCondition {
                    stop_tokens: vec![TokenId(0)],
                    terminator: TokenId(0),
                    max_tokens: 8,
                },
                &mut NeverCancels,
                &mut |_| {},
                PositionedSinks {
                    field: Some((4, &mut on_field as FieldSink)),
                    on_column: &mut |position: u64, layers: Vec<Vec<f32>>| {
                        taken.push((position, layers));
                    },
                },
                SamplerBuild {
                    seed: 11,
                    penalty_window: 64,
                },
            )
            .expect("the generation runs");

        assert!(
            !fields.is_empty(),
            "an elected depth reaches the field sink"
        );
        assert!(
            fields.iter().all(|(_, ranked, _)| *ranked == 4),
            "the elected depth is the ranking's width: {fields:?}"
        );
        assert!(!taken.is_empty(), "an armed tap reaches the column sink");
        assert!(
            taken.iter().all(|(_, layers)| *layers == columns),
            "the column sink takes the forward's layers, not the field's: {taken:?}"
        );
        // **The column is taken at the draw site and the field only where the
        // token is retained**, so the stop draw takes a column and owes no
        // field. The positions are therefore a prefix rather than a match, and
        // asserting equality here would be asserting against the rule the loop
        // states above the stop check.
        let field_positions: Vec<u64> = fields.iter().map(|(p, _, _)| *p).collect();
        let column_positions: Vec<u64> = taken.iter().map(|(p, _)| *p).collect();
        // **The absolute positions, because the two checks below are relative
        // and a shared wrong offset satisfies both.** The fixture fixes them:
        // the open leaves one token resident, the delta a second, and the
        // three scripted draws sit at 2, 3 and 4, the last being the stop.
        assert_eq!(
            column_positions,
            vec![2, 3, 4],
            "a column at each draw site, counted from the resident length"
        );
        assert_eq!(
            field_positions,
            vec![2, 3],
            "a field at each retained draw, the stop owing none"
        );
        assert_eq!(
            column_positions.len(),
            field_positions.len() + 1,
            "the stop draw takes a column and owes no field: {column_positions:?} \
             against {field_positions:?}"
        );
        assert_eq!(
            field_positions,
            column_positions[..field_positions.len()],
            "where both fire they name one position, not two readings of it"
        );
    }

    /// **The re-feed appends the recorded token whatever the draw said, and
    /// the draws land in the answer's output slots.** The scripted backend
    /// draws one path while the recorded path is another, so the two are
    /// distinguishable at every position: what became resident must be the
    /// recorded path and the terminator, the drawn identifiers must ride
    /// `Generated::tokens`, and the signals must count one measurement per
    /// recorded position, per `weaver-spu-Spec` section 4.6.
    ///
    /// Perturbation: append the draw instead of the recorded token in
    /// `refeed` and this fails, the decode log carrying the script where the
    /// path belongs. Watched under exactly that change.
    ///
    /// conforms: spu-refeed-appends-the-recorded-path
    #[test]
    fn the_refeed_appends_the_recorded_token_whatever_the_draw_said() {
        let log = Rc::new(RefCell::new(Log {
            script: vec![TokenId(1), TokenId(2), TokenId(3)],
            ..Default::default()
        }));
        let mut session = Session::new(
            Box::new(Recorder(Rc::clone(&log), DISTRIBUTION.to_vec())),
            64,
            FlushMechanism::TruncateToPosition,
            false,
        );
        session.open(&[TokenId(4)]).expect("the prefix lands");
        let generated = session
            .refeed(
                &[TokenId(5)],
                &[TokenId(7), TokenId(8), TokenId(9)],
                TokenId(0),
                &mut NeverCancels,
                PositionedSinks {
                    field: None,
                    on_column: &mut |_, _| {},
                },
                SamplerBuild {
                    seed: 11,
                    penalty_window: 64,
                },
            )
            .expect("the re-feed runs");

        assert_eq!(
            generated.tokens,
            vec![TokenId(1), TokenId(2), TokenId(3)],
            "the recomputed draws fill the output slots"
        );
        let decoded = log.borrow().decoded.clone();
        assert_eq!(
            decoded,
            vec![
                (vec![TokenId(4)], 0),
                (vec![TokenId(5)], 1),
                (vec![TokenId(7)], 2),
                (vec![TokenId(8)], 3),
                (vec![TokenId(9)], 4),
                (vec![TokenId(0)], 5),
            ],
            "the recorded token appends at every position, then the terminator, never the draw"
        );
        assert_eq!(
            generated.signals.steps(),
            3,
            "one measurement per recorded position"
        );
        assert_eq!(
            log.borrow().reseeds,
            vec![(11, vec![TokenId(4), TokenId(5)])],
            "the sampler reseeds after the prefill exactly as a generation does"
        );
    }

    /// **An asked column that does not arrive at a sampled position
    /// faults**, per charter section 13.10 and Spec section 7: the ask
    /// passed the registry, so a backend whose tap holds no column - the
    /// native tap's own shape - faults loudly rather than omitting
    /// silently. The scripted backend's `take_columns` answers `None`,
    /// which is exactly that backend.
    ///
    /// Perturbation: restore the silent skip at the draw site - emit only
    /// where the take answers - and the armed generation completes with no
    /// column and no fault. Watched under exactly that restoration.
    ///
    /// conforms: spu-asked-column-arrives-or-faults
    #[test]
    fn an_asked_column_that_does_not_arrive_faults() {
        let log = Rc::new(RefCell::new(Log {
            script: vec![TokenId(1), TokenId(9)],
            ..Default::default()
        }));
        let mut session = Session::new(
            Box::new(Recorder(Rc::clone(&log), DISTRIBUTION.to_vec())),
            64,
            FlushMechanism::TruncateToPosition,
            false,
        );
        session.open(&[TokenId(7)]).expect("the prefix lands");
        session.ask_columns();
        let outcome = session.append_and_generate(
            &[TokenId(8)],
            &StopCondition {
                stop_tokens: vec![TokenId(9)],
                terminator: TokenId(0),
                max_tokens: 8,
            },
            &mut NeverCancels,
            &mut |_| {},
            PositionedSinks {
                field: None,
                on_column: &mut |_, _| {},
            },
            SamplerBuild {
                seed: 11,
                penalty_window: 64,
            },
        );
        assert!(
            matches!(outcome, Err(DecodeFault::Engine { .. })),
            "the silent absence faults: {outcome:?}"
        );
    }

    /// **A step that cannot be measured makes the whole measurement absent**,
    /// not a vector one short.
    ///
    /// The vectors are positionally paired with the tokens the answer carries,
    /// so dropping one entry shifts every entry after it onto the wrong token.
    /// A reader cannot detect that; a reader can detect absence.
    ///
    /// Perturbation: replace the `None => signals.abandon()` arm with a plain
    /// skip and this fails, the run answering two measurements against three
    /// tokens. Watched under exactly that change.
    #[test]
    fn a_step_that_cannot_be_measured_makes_the_whole_run_absent() {
        let log = Rc::new(RefCell::new(Log {
            script: vec![TokenId(1), TokenId(2), TokenId(3), TokenId(9)],
            // The second retained token loses its distribution.
            fail_distribution_call: Some(1),
            ..Default::default()
        }));
        let mut session = Session::new(
            Box::new(Recorder(Rc::clone(&log), DISTRIBUTION.to_vec())),
            64,
            FlushMechanism::TruncateToPosition,
            false,
        );
        session.open(&[TokenId(7)]).expect("the prefix lands");
        let generated = session
            .append_and_generate(
                &[TokenId(8)],
                &StopCondition {
                    stop_tokens: vec![TokenId(9)],
                    terminator: TokenId(0),
                    max_tokens: 8,
                },
                &mut NeverCancels,
                &mut |_| {},
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

        assert_eq!(generated.tokens.len(), 3, "the generation still answers");
        assert_eq!(
            generated.signals.steps(),
            0,
            "and the measurement is absent rather than short"
        );
        assert!(generated.signals.entropy_bits.is_none());
        assert!(generated.signals.surprisal_bits.is_none());
    }

    /// **The signals are measured before the sampler, at every step.**
    ///
    /// The distribution exists until the draw consumes it, so a measurement
    /// taken after `sample` is reading a state the draw has already moved
    /// past. What this reads is the seam's call order: every `distribution`
    /// sits immediately before the `sample` it measured, never after.
    ///
    /// Perturbation: move the `distribution` read below the `sample` call in
    /// `append_and_generate` and this fails, the recorded order arriving as
    /// sample-then-distribution. Watched under exactly that move.
    #[test]
    fn the_distribution_is_read_before_the_draw_at_every_step() {
        let log = Rc::new(RefCell::new(Log {
            script: vec![TokenId(1), TokenId(2), TokenId(9)],
            ..Default::default()
        }));
        let mut session = Session::new(
            Box::new(Recorder(Rc::clone(&log), DISTRIBUTION.to_vec())),
            64,
            FlushMechanism::TruncateToPosition,
            false,
        );
        session.open(&[TokenId(7)]).expect("the prefix lands");
        let generated = session
            .append_and_generate(
                &[TokenId(8)],
                &StopCondition {
                    stop_tokens: vec![TokenId(9)],
                    terminator: TokenId(0),
                    max_tokens: 8,
                },
                &mut NeverCancels,
                &mut |_| {},
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

        let order = log.borrow().order.clone();
        assert!(!order.is_empty(), "the seam was read at all");
        for pair in order.chunks(2) {
            assert_eq!(
                pair,
                ["distribution", "sample"],
                "every measurement precedes its draw, got {order:?}"
            );
        }

        // **The signals pair with the tokens the answer carries.** The stop
        // token is drawn and measured against, but it is not retained, so it
        // owes no entry: a signal for a token the answer omits shifts every
        // reader's pairing by one.
        assert_eq!(generated.tokens.len(), 2, "two tokens before the stop");
        assert_eq!(
            generated.signals.steps(),
            generated.tokens.len(),
            "one measurement per retained token, positionally paired"
        );
        assert_eq!(order.len() / 2, 3, "three draws, the stop token among them");
    }

    /// A cancel mid-path stops the re-feed where the poll saw it, and the
    /// session is left well framed: the recorded prefix that landed, the
    /// terminator after it, and one recomputed draw and one measurement per
    /// landed position. This is what the caller reads to know that a cancelled
    /// re-feed certifies a prefix rather than the path it was handed.
    ///
    /// **The caller faults on it rather than answering.** The drive is given
    /// the whole path upfront, so the text it owes is the record's while the
    /// measurement is the run's, and a cancel parts them. A part-way answer
    /// could only certify a prefix, which is the nothing the empty-path arm
    /// refuses, so `main.rs` treats a cancelled re-feed as a fault. This test
    /// pins what the session does under the cancel, not what the binary
    /// decides about it.
    ///
    /// **The whole decode log is asserted, positions included**, the way the
    /// sibling above asserts it. A value-only reading cannot see a recorded
    /// token landed at the wrong absolute position, which the append-only
    /// discipline this module pins forbids, and it drops the terminator
    /// entirely because the terminator is in no path.
    ///
    /// Perturbations watched, each run under exactly the change named. Land
    /// the recorded token before honouring the cancel and the log carries
    /// three recorded positions against two draws. Return from the cancel arm
    /// without landing the terminator and the log ends at the last recorded
    /// token, which `spu-terminator-on-every-path` forbids. Move the signals
    /// record above the cancel check and the step count runs one past the
    /// draws it is paired with.
    ///
    /// **What this does not watch.** The caller's fault is taken in `main.rs`,
    /// which is the binary, so nothing here would go red if that branch were
    /// dropped and a cancelled re-feed answered instead. A seam-level
    /// cancelled re-feed would catch it and there is none, which is bounded
    /// by the fact that nothing can produce one: the decode socket's peer is
    /// the harness by descriptor possession, and the harness refuses every
    /// directive during this drive rather than cancelling it.
    #[test]
    fn a_cancelled_refeed_lands_only_the_prefix_its_draws_count() {
        let log = Rc::new(RefCell::new(Log {
            script: vec![TokenId(1), TokenId(2), TokenId(3)],
            ..Default::default()
        }));
        let mut session = Session::new(
            Box::new(Recorder(Rc::clone(&log), DISTRIBUTION.to_vec())),
            64,
            FlushMechanism::TruncateToPosition,
            false,
        );
        session.open(&[TokenId(4)]).expect("the prefix lands");
        let path = [TokenId(7), TokenId(8), TokenId(9)];
        let generated = session
            .refeed(
                &[TokenId(5)],
                &path,
                TokenId(0),
                &mut CancelsAfter { polls: 0, limit: 2 },
                PositionedSinks {
                    field: None,
                    on_column: &mut |_, _| {},
                },
                SamplerBuild {
                    seed: 11,
                    penalty_window: 64,
                },
            )
            .expect("the re-feed runs");

        assert_eq!(
            generated.stopped,
            Stopped::Cancelled,
            "the cancel is what stopped it"
        );
        assert_eq!(
            generated.tokens.len(),
            2,
            "one recomputed draw per recorded token landed, and the third never was"
        );
        assert_eq!(
            generated.signals.steps(),
            generated.tokens.len(),
            "one measurement per landed position, the pairing a break could part"
        );
        assert_eq!(
            log.borrow().decoded.clone(),
            vec![
                (vec![TokenId(4)], 0),
                (vec![TokenId(5)], 1),
                (vec![TokenId(7)], 2),
                (vec![TokenId(8)], 3),
                (vec![TokenId(0)], 4),
            ],
            "two recorded positions and then the terminator, the cancelled third \
             never landing, so the prefix the draw count names is what the \
             emission may report as said"
        );
    }

    /// Cancels after a set number of polls, which is how a token-boundary bound
    /// is made observable without a clock.
    struct CancelsAfter {
        polls: usize,
        limit: usize,
    }

    impl CancelPoll for CancelsAfter {
        fn cancelled(&mut self) -> bool {
            let now = self.polls >= self.limit;
            self.polls += 1;
            now
        }
    }

    fn stop_at(terminator: u32) -> StopCondition {
        StopCondition {
            stop_tokens: vec![TokenId(99)],
            terminator: TokenId(terminator),
            max_tokens: 64,
        }
    }

    fn with_mechanism(
        script: Vec<TokenId>,
        capacity: usize,
        mechanism: FlushMechanism,
    ) -> (Session<'static>, Rc<RefCell<Log>>) {
        let log = Rc::new(RefCell::new(Log {
            script,
            ..Default::default()
        }));
        let mut session = Session::new(
            Box::new(Recorder(Rc::clone(&log), DISTRIBUTION.to_vec())),
            capacity,
            mechanism,
            false,
        );
        session.open(&[TokenId(1), TokenId(2)]).expect("open");
        (session, log)
    }

    fn opened(script: Vec<TokenId>, capacity: usize) -> (Session<'static>, Rc<RefCell<Log>>) {
        with_mechanism(script, capacity, FlushMechanism::TruncateToPosition)
    }

    fn last_decoded(log: &Rc<RefCell<Log>>) -> Vec<TokenId> {
        log.borrow()
            .decoded
            .last()
            .map(|(tokens, _)| tokens.clone())
            .unwrap_or_default()
    }

    /// **Every generation decodes the delta at the resident end**, never back
    /// at the prefix. This reads the position the backend was asked to decode
    /// at, which is the property itself.
    ///
    /// An earlier version of this test asserted only that `resident_len` grew
    /// across turns, and **it could not fail**: under the rewind, the second
    /// turn decodes at the prefix, runs past its exhausted script to
    /// `max_tokens`, and ends up longer than the first turn anyway. Growth was
    /// passing for non-rewinding. The position is the thing.
    ///
    /// Perturbation: decode the delta at `self.prefix_len` instead of
    /// `self.resident_len`, which is the re-prefill rewind the archived tree's
    /// proof warns about, and this test fails on the recorded position. Watched
    /// under exactly that change.
    /// **The stream fires for exactly the retained tokens, in order.** The
    /// on_token callback is the seam's stream, so what it fires must equal the
    /// generation's tokens, no more and no fewer: the stop token is drawn and
    /// not retained, so it is never streamed, and a consumer accumulating the
    /// stream holds the same sequence the close carries.
    ///
    /// Perturbation: fire on_token before the stop-token check instead of
    /// after the retain, and this test fails, the stop token appearing in the
    /// stream but not in `Generated.tokens`. Watched under exactly that change.
    #[test]
    fn the_stream_fires_for_exactly_the_retained_tokens() {
        // A script whose third token is the stop token, so the run retains two
        // and stops on the third, which must not stream.
        let (mut session, _log) = opened(vec![TokenId(7), TokenId(8), TokenId(9)], 128);
        let mut cancel = NeverCancels;
        let mut streamed: Vec<TokenId> = Vec::new();
        let generated = session
            .append_and_generate(
                &[TokenId(3)],
                &StopCondition {
                    stop_tokens: vec![TokenId(9)],
                    terminator: TokenId(0),
                    max_tokens: 50,
                },
                &mut cancel,
                &mut |token| streamed.push(token),
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
        assert_eq!(
            streamed, generated.tokens,
            "the stream equals the retained tokens in order"
        );
        assert!(
            !streamed.contains(&TokenId(9)),
            "the stop token is drawn and not streamed"
        );
    }

    #[test]
    fn the_delta_decodes_at_the_resident_end_never_at_the_prefix() {
        // A script that stops immediately, so generation adds a known amount
        // and cannot mask a wrong position by running long.
        let (mut session, log) = opened(vec![TokenId(99), TokenId(99)], 128);
        let mut cancel = NeverCancels;

        session
            .append_and_generate(
                &[TokenId(3)],
                &stop_at(50),
                &mut cancel,
                &mut |_| {},
                PositionedSinks {
                    field: None,
                    on_column: &mut |_, _| {},
                },
                SamplerBuild {
                    seed: 11,
                    penalty_window: 64,
                },
            )
            .expect("first turn");
        let after_first = session.resident_len();
        assert!(
            after_first > session.prefix_len(),
            "the first turn extended"
        );

        let before = log.borrow().decoded.len();
        session
            .append_and_generate(
                &[TokenId(4)],
                &stop_at(50),
                &mut cancel,
                &mut |_| {},
                PositionedSinks {
                    field: None,
                    on_column: &mut |_, _| {},
                },
                SamplerBuild {
                    seed: 11,
                    penalty_window: 64,
                },
            )
            .expect("second turn");

        let (tokens, position) = log.borrow().decoded[before].clone();
        assert_eq!(tokens, vec![TokenId(4)], "the second delta is what landed");
        assert_eq!(
            position, after_first,
            "the delta decodes at the resident end, not at the prefix"
        );
    }

    /// The resident length never falls across turns that ask no reducer, which
    /// is the behavioural half beside the position check above.
    #[test]
    fn the_resident_length_never_rewinds_across_turns() {
        let (mut session, _log) = opened(vec![TokenId(99), TokenId(99)], 128);
        let mut cancel = NeverCancels;
        let mut seen = session.resident_len();
        for delta in [TokenId(3), TokenId(4), TokenId(5)] {
            session
                .append_and_generate(
                    &[delta],
                    &stop_at(50),
                    &mut cancel,
                    &mut |_| {},
                    PositionedSinks {
                        field: None,
                        on_column: &mut |_, _| {},
                    },
                    SamplerBuild {
                        seed: 11,
                        penalty_window: 64,
                    },
                )
                .expect("a turn");
            assert!(
                session.resident_len() > seen,
                "each turn extends, never rewinding"
            );
            seen = session.resident_len();
        }
    }

    /// **The turn terminator is made resident on the clean path.**
    ///
    /// Perturbation: move the terminator decode inside the `Stopped::Complete`
    /// arm, or delete it, and this test fails because the last decoded token is
    /// no longer the terminator. Watched under exactly that deletion.
    #[test]
    fn the_terminator_lands_on_the_clean_path() {
        let (mut session, log) = opened(vec![TokenId(7), TokenId(99)], 128);
        let mut cancel = NeverCancels;
        let generated = session
            .append_and_generate(
                &[TokenId(3)],
                &stop_at(50),
                &mut cancel,
                &mut |_| {},
                PositionedSinks {
                    field: None,
                    on_column: &mut |_, _| {},
                },
                SamplerBuild {
                    seed: 11,
                    penalty_window: 64,
                },
            )
            .expect("a turn");
        assert_eq!(generated.stopped, Stopped::Complete);
        assert_eq!(
            generated.tokens,
            vec![TokenId(7)],
            "99 stops, 7 is produced"
        );
        assert_eq!(last_decoded(&log), vec![TokenId(50)]);
    }

    /// **And on the cancelled path alike**, which is what makes the cancel leave
    /// a well-framed session rather than one that ends mid-message.
    ///
    /// Perturbation: return early from the `Stopped::Cancelled` break without
    /// falling through to the terminator decode and this test fails. Watched
    /// under exactly that early return.
    #[test]
    fn the_terminator_lands_on_the_cancelled_path_too() {
        let (mut session, log) = opened(vec![TokenId(7), TokenId(8), TokenId(9)], 128);
        let mut cancel = CancelsAfter { polls: 0, limit: 2 };
        let generated = session
            .append_and_generate(
                &[TokenId(3)],
                &stop_at(50),
                &mut cancel,
                &mut |_| {},
                PositionedSinks {
                    field: None,
                    on_column: &mut |_, _| {},
                },
                SamplerBuild {
                    seed: 11,
                    penalty_window: 64,
                },
            )
            .expect("a cancelled turn");
        assert_eq!(generated.stopped, Stopped::Cancelled);
        assert_eq!(
            last_decoded(&log),
            vec![TokenId(50)],
            "the terminator is resident even though the turn was stopped"
        );
    }

    /// **The cancel is bounded by one token.** Cancelling at the second poll
    /// yields exactly the tokens sampled before it, never more, so the stop
    /// costs one token's decode rather than a kernel's completion.
    ///
    /// Perturbation: move the cancel check to after the sample-and-decode, and
    /// this test fails because a third token is produced. Watched under exactly
    /// that move.
    #[test]
    fn the_cancel_is_bounded_by_one_token() {
        let (mut session, _log) = opened(vec![TokenId(7), TokenId(8), TokenId(9)], 128);
        let mut cancel = CancelsAfter { polls: 0, limit: 2 };
        let generated = session
            .append_and_generate(
                &[TokenId(3)],
                &stop_at(50),
                &mut cancel,
                &mut |_| {},
                PositionedSinks {
                    field: None,
                    on_column: &mut |_, _| {},
                },
                SamplerBuild {
                    seed: 11,
                    penalty_window: 64,
                },
            )
            .expect("a cancelled turn");
        assert_eq!(
            generated.tokens,
            vec![TokenId(7), TokenId(8)],
            "two polls passed, so exactly two tokens were produced"
        );
    }

    /// **Overflow refuses and sheds nothing**, and the refusal carries the
    /// session's own account so the harness can decide with the numbers.
    #[test]
    fn an_overflowing_delta_refuses_with_the_session_s_account() {
        let (mut session, _log) = opened(vec![], 8);
        let mut cancel = NeverCancels;
        let refusal = session.append_and_generate(
            &[
                TokenId(1),
                TokenId(2),
                TokenId(3),
                TokenId(4),
                TokenId(5),
                TokenId(6),
                TokenId(7),
            ],
            &stop_at(50),
            &mut cancel,
            &mut |_| {},
            PositionedSinks {
                field: None,
                on_column: &mut |_, _| {},
            },
            SamplerBuild {
                seed: 11,
                penalty_window: 64,
            },
        );
        match refusal {
            Err(DecodeFault::Overflow {
                resident,
                requested,
                capacity,
            }) => {
                assert_eq!((resident, requested, capacity), (2, 7, 8));
            }
            other => panic!("an overflowing delta refuses, got {other:?}"),
        }
        assert_eq!(
            session.resident_len(),
            2,
            "and sheds nothing: the session is untouched"
        );
    }

    /// **The flush mechanism is read from the declaration**, not inferred. A
    /// truncating family truncates to the prefix.
    #[test]
    fn a_truncating_family_flushes_by_truncating_to_the_prefix() {
        let (mut session, log) = opened(vec![TokenId(99)], 128);
        let mut cancel = NeverCancels;
        session
            .append_and_generate(
                &[TokenId(3)],
                &stop_at(50),
                &mut cancel,
                &mut |_| {},
                PositionedSinks {
                    field: None,
                    on_column: &mut |_, _| {},
                },
                SamplerBuild {
                    seed: 11,
                    penalty_window: 64,
                },
            )
            .expect("a turn");
        session.flush(0).expect("flush");
        assert_eq!(session.resident_len(), session.prefix_len());
        assert_eq!(log.borrow().truncated.clone(), vec![2]);
    }

    /// **The cut's bounds hold at their edges**, per the decode contract's
    /// conformance list: zero, a middle length, the resident count, and
    /// beyond it each resolve to the clamped length, and the identity
    /// prefix is never cut.
    #[test]
    fn the_cuts_bounds_hold_at_their_edges() {
        let grown = |mechanism| {
            let (mut session, log) = with_mechanism(vec![TokenId(99), TokenId(98)], 128, mechanism);
            let mut cancel = NeverCancels;
            session
                .append_and_generate(
                    &[TokenId(3), TokenId(4)],
                    &stop_at(50),
                    &mut cancel,
                    &mut |_| {},
                    PositionedSinks {
                        field: None,
                        on_column: &mut |_, _| {},
                    },
                    SamplerBuild {
                        seed: 11,
                        penalty_window: 64,
                    },
                )
                .expect("a turn");
            (session, log)
        };
        for mechanism in [
            FlushMechanism::TruncateToPosition,
            FlushMechanism::ReestablishAndReprefill,
        ] {
            // Beyond the resident count: nothing to cut, nothing moves.
            let (mut session, _log) = grown(mechanism);
            let standing = session.resident_len();
            session.flush(standing + 10).expect("flush");
            assert_eq!(session.resident_len(), standing, "{mechanism:?} beyond");

            // At the resident count: the cut lands exactly there.
            session.flush(standing).expect("flush");
            assert_eq!(session.resident_len(), standing, "{mechanism:?} at");

            // A middle length, inside the span: the cut is the ask's.
            let middle = session.prefix_len() + 1;
            session.flush(middle).expect("flush");
            assert_eq!(session.resident_len(), middle, "{mechanism:?} middle");

            // Zero: clamped up to the identity prefix, never below it.
            session.flush(0).expect("flush");
            assert_eq!(
                session.resident_len(),
                session.prefix_len(),
                "{mechanism:?} zero clamps to the prefix"
            );
        }
    }

    /// A family that cannot roll back is re-established and re-prefilled, which
    /// is expensive and correct where the cheap path is silently wrong.
    #[test]
    fn a_non_rewinding_family_flushes_by_reestablishing() {
        let (mut session, log) = with_mechanism(
            vec![TokenId(99)],
            128,
            FlushMechanism::ReestablishAndReprefill,
        );
        session.flush(0).expect("flush");
        assert_eq!(session.resident_len(), 2);
        assert_eq!(log.borrow().reestablished, 1);
        assert_eq!(log.borrow().truncated.clone(), Vec::<usize>::new());
    }

    /// **Open is first and happens once.** A second open refuses rather than
    /// silently rewinding the resident length over accumulated turns.
    ///
    /// Perturbation: remove the `opened` guard from `open` and this test fails,
    /// because the second open succeeds and the length rewinds to the prefix.
    /// Watched under exactly that removal.
    #[test]
    fn a_second_open_refuses_rather_than_rewinding() {
        let (mut session, _log) = opened(vec![TokenId(99)], 128);
        let mut cancel = NeverCancels;
        session
            .append_and_generate(
                &[TokenId(3)],
                &stop_at(50),
                &mut cancel,
                &mut |_| {},
                PositionedSinks {
                    field: None,
                    on_column: &mut |_, _| {},
                },
                SamplerBuild {
                    seed: 11,
                    penalty_window: 64,
                },
            )
            .expect("a turn");
        let grown = session.resident_len();
        assert_eq!(
            session.open(&[TokenId(1)]),
            Err(DecodeFault::AlreadyOpen),
            "a second open refuses"
        );
        assert_eq!(session.resident_len(), grown, "and rewinds nothing");
    }

    /// **A flush that fails partway closes the session** rather than leaving a
    /// large resident length over an empty backend, where the next append would
    /// decode far past the backend's real state.
    ///
    /// Perturbation: remove the poisoning from `flush` and this test fails,
    /// because the session stays open with its pre-flush length and the append
    /// runs. Watched under exactly that removal.
    #[test]
    fn a_failed_flush_closes_the_session() {
        let (mut session, log) = with_mechanism(
            vec![TokenId(99)],
            128,
            FlushMechanism::ReestablishAndReprefill,
        );
        // The re-prefill after the reestablish is the next decode call.
        let next_call = log.borrow().decoded.len();
        log.borrow_mut().fail_decode_call = Some(next_call);
        assert!(session.flush(0).is_err(), "the flush reports its failure");
        let mut cancel = NeverCancels;
        assert_eq!(
            session.append_and_generate(
                &[TokenId(3)],
                &stop_at(50),
                &mut cancel,
                &mut |_| {},
                PositionedSinks {
                    field: None,
                    on_column: &mut |_, _| {},
                },
                SamplerBuild {
                    seed: 11,
                    penalty_window: 64,
                },
            ),
            Err(DecodeFault::NotOpen),
            "the session no longer serves"
        );
    }

    /// **An engine fault mid-generation closes the session.** The terminator
    /// discipline covers the clean path and the cancelled path; a backend that
    /// just faulted cannot be asked to decode a terminator, so the fault path
    /// closes the session instead of leaving a terminator-less prefix open for
    /// the next append.
    ///
    /// Perturbation: propagate the sample fault without the poison and this
    /// test fails on the second append, which runs against the mid-message
    /// state. Watched under exactly that removal.
    #[test]
    fn an_engine_fault_mid_generation_closes_the_session() {
        let (mut session, log) = opened(vec![], 128);
        log.borrow_mut().fail_sample = true;
        let mut cancel = NeverCancels;
        assert!(
            session
                .append_and_generate(
                    &[TokenId(3)],
                    &stop_at(50),
                    &mut cancel,
                    &mut |_| {},
                    PositionedSinks {
                        field: None,
                        on_column: &mut |_, _| {},
                    },
                    SamplerBuild {
                        seed: 11,
                        penalty_window: 64,
                    },
                )
                .is_err(),
            "the fault surfaces"
        );
        assert_eq!(
            session.append_and_generate(
                &[TokenId(4)],
                &stop_at(50),
                &mut cancel,
                &mut |_| {},
                PositionedSinks {
                    field: None,
                    on_column: &mut |_, _| {},
                },
                SamplerBuild {
                    seed: 11,
                    penalty_window: 64,
                },
            ),
            Err(DecodeFault::NotOpen),
            "and the session no longer serves"
        );
        assert_eq!(
            session.open(&[TokenId(1)]),
            Err(DecodeFault::NotOpen),
            "and cannot be reopened over its closed backend"
        );
    }

    /// **A span describing no removable region refuses, and the session
    /// stands.** Charter 13.13 gives four such spans and the refusal is the
    /// contract's typed case rather than a fault: the ask was answerable
    /// and wrong. **The empty span is the one worth asserting hardest**,
    /// because eliding nothing succeeds trivially and would report a state
    /// change that did not happen.
    ///
    /// Perturbation: narrow the guard to `to < from` and the empty span
    /// answers `Ok` with equal counts, an elision authored over nothing.
    /// Watched under exactly that narrowing. A full drop of the bound fails
    /// earlier and differently, on the inverted span's underflow, and never
    /// reaches the empty case.
    ///
    /// conforms: spu-elision-refuses-an-unremovable-span
    #[test]
    fn an_unremovable_span_refuses_and_the_session_stands() {
        let (mut session, _log) = with_mechanism(
            vec![TokenId(5), TokenId(6), TokenId(0)],
            64,
            FlushMechanism::TruncateToPosition,
        );
        session
            .append_and_generate(
                &[TokenId(3)],
                &StopCondition {
                    stop_tokens: vec![TokenId(9)],
                    terminator: TokenId(0),
                    max_tokens: 8,
                },
                &mut NeverCancels,
                &mut |_| {},
                PositionedSinks {
                    field: None,
                    on_column: &mut |_, _| {},
                },
                SamplerBuild {
                    seed: 11,
                    penalty_window: 64,
                },
            )
            .expect("a generation stands so there is an interior");

        let resident = session.resident_len();
        let prefix = 2;
        for (from, to, why) in [
            (0, resident, "a span over the identity prefix"),
            (prefix, resident + 1, "a span past the resident count"),
            (resident, prefix, "a span ending before it starts"),
            (prefix, prefix, "an empty span"),
        ] {
            let refused = session.elide(from, to);
            assert!(
                matches!(refused, Err(DecodeFault::UnremovableSpan { .. })),
                "{why} refuses, got {refused:?}"
            );
            assert_eq!(
                session.resident_len(),
                resident,
                "and {why} leaves the session as it found it"
            );
        }
    }

    /// **The elision removes exactly its span and resequences nothing**,
    /// which is the decode contract's guarantee: what remains is the
    /// sequence it was, shorter, with every surviving position in the order
    /// it held. Driven under both flush mechanisms, because the elision
    /// ignores the declaration on purpose: the interior outcome is reached
    /// through the re-establish path for every family and never through a
    /// truncate, per Spec section 4.5, and the log asserts both halves.
    ///
    /// Perturbation: build the kept vector from `[to..]` alone, or reverse
    /// the tail, and the counts still agree while the membership or the
    /// order does not. The counts cannot catch either, which is why this
    /// asserts the tokens themselves. A regression consulting the declared
    /// mechanism and truncating fails the log's two counters under the
    /// truncating family.
    ///
    /// conforms: spu-elision-removes-its-span-in-order
    #[test]
    fn the_elision_removes_its_span_and_keeps_the_rest_in_order() {
        for mechanism in [
            FlushMechanism::TruncateToPosition,
            FlushMechanism::ReestablishAndReprefill,
        ] {
            let (mut session, log) = with_mechanism(
                vec![
                    TokenId(5),
                    TokenId(0),
                    TokenId(6),
                    TokenId(0),
                    TokenId(7),
                    TokenId(0),
                ],
                64,
                mechanism,
            );
            for token in [TokenId(3), TokenId(4), TokenId(8)] {
                session
                    .append_and_generate(
                        &[token],
                        &StopCondition {
                            stop_tokens: vec![TokenId(9)],
                            terminator: TokenId(0),
                            max_tokens: 8,
                        },
                        &mut NeverCancels,
                        &mut |_| {},
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
            }

            let before_tokens = session.resident_tokens().to_vec();
            let resident = before_tokens.len();
            // An interior span: past the prefix and short of the end, so what
            // survives is a head and a tail rather than a prefix.
            let (from, to) = (3, 5);
            assert!(to < resident, "the fixture leaves a tail to preserve");
            let expected: Vec<TokenId> = before_tokens[..from]
                .iter()
                .chain(before_tokens[to..].iter())
                .copied()
                .collect();
            let truncates_before = log.borrow().truncated.len();
            let reestablished_before = log.borrow().reestablished;

            let (was, now) = session.elide(from, to).expect("the span is removable");
            assert_eq!(was, resident, "the before count is what stood");
            assert_eq!(now, resident - (to - from), "and exactly the span went");
            assert_eq!(
                session.resident_tokens(),
                expected.as_slice(),
                "the survivors keep their order and their membership"
            );
            assert_eq!(
                log.borrow().truncated.len(),
                truncates_before,
                "the elision issued no truncate under {mechanism:?}"
            );
            assert_eq!(
                log.borrow().reestablished,
                reestablished_before + 1,
                "and re-established exactly once under {mechanism:?}"
            );
        }
    }

    /// **The sampler holds nothing between generations, a flush included**,
    /// per `weaver-spu-Spec` section 8.5. Two generations with a flush
    /// between them: each is reseeded with the seed its caller derived, and
    /// the second's penalty window is the truncated resident tail rather
    /// than anything the sampler carried across.
    ///
    /// This is the case the retired coupling lived in. The GGUF chain's
    /// reset cleared the penalty window and reseeded the draw together,
    /// so a flush moved the stream at a point nothing in the record
    /// explained - and two arms of a comparison differing in resident
    /// length crossed the flush threshold at different turns.
    ///
    /// Perturbation: reseed once at open rather than per generation and
    /// the second call never arrives, so the count fails.
    #[test]
    fn the_sampler_is_rebuilt_per_generation_across_a_flush() {
        let (mut session, log) = opened(vec![TokenId(7), TokenId(8)], 128);
        let stop = stop_at(50);
        let mut cancel = NeverCancels;

        session
            .append_and_generate(
                &[TokenId(3)],
                &stop,
                &mut cancel,
                &mut |_| {},
                PositionedSinks {
                    field: None,
                    on_column: &mut |_, _| {},
                },
                SamplerBuild {
                    seed: 0xAAAA,
                    penalty_window: 2,
                },
            )
            .expect("the first generation runs");
        let window_before = log.borrow().reseeds[0].1.len();
        session.flush(0).expect("the flush lands");
        session
            .append_and_generate(
                &[TokenId(4)],
                &stop,
                &mut cancel,
                &mut |_| {},
                PositionedSinks {
                    field: None,
                    on_column: &mut |_, _| {},
                },
                SamplerBuild {
                    seed: 0xBBBB,
                    penalty_window: 2,
                },
            )
            .expect("the second generation runs");

        let reseeds = log.borrow().reseeds.clone();
        assert_eq!(reseeds.len(), 2, "one reseed per generation: {reseeds:?}");
        assert_eq!(reseeds[0].0, 0xAAAA, "each takes its caller's seed");
        assert_eq!(reseeds[1].0, 0xBBBB, "and the flush does not change it");

        // **The window's tokens, not its length.** A length alone passes
        // on a window holding the wrong tokens, and the ordering this
        // guards is exactly about which tokens are in it: the reseed
        // follows the prefill, so each window ends with that generation's
        // own delta. Taken before the prefill both windows end at the
        // prefix and the deltas are missing, which is the native path's
        // disagreement this pins.
        assert_eq!(
            reseeds[0].1,
            vec![TokenId(2), TokenId(3)],
            "the first window is the last two resident, the bound holding: \
             one of the prefix and the delta after it"
        );
        assert_eq!(
            reseeds[1].1,
            vec![TokenId(2), TokenId(4)],
            "and the second is the flush's prefix with the second delta, \
             the first generation's tokens having gone with the flush"
        );
        // **The second window does not grow, which is the whole point.**
        // A generation ran between the two and produced tokens, so a
        // window carried inside the sampler would hold them still. Read
        // from the resident tail after a flush, it is the prefix again,
        // exactly what the first generation saw.
        assert_eq!(
            reseeds[1].1.len(),
            window_before,
            "the window follows the resident state rather than accumulating: \
             {:?} against {:?}",
            reseeds[1].1,
            reseeds[0].1
        );
    }
}
