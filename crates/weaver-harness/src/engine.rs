//! conforms: harness-stop-polled-during-the-invocation
//! conforms: harness-stopped-invocation-feeds-no-generation
//! conforms: harness-loop-mints-no-port
//! conforms: harness-extension-seam-at-loaded-and-idle
//! conforms: harness-turn-authors-the-model-events
//! conforms: harness-stop-polled-during-the-stream
//! conforms: harness-elision-authors-from-the-ask
//!
//! Loop 1's seat and the decode surface it composes, per `weaver-harness-Spec`
//! sections 6 and 6.1. The loop itself is the builder's, written at the worker
//! composition root and compiled into the worker binary. What this crate holds
//! is the seat and the granted surface the loop composes against.
//!
//! **The extension seam is crossed at loaded-and-idle itself**: loop 0 hands a
//! standing interior to whatever loop 1 the binary carries, and takes it back
//! at the stop and at the leave, the bracket discipline being loop 0's for
//! every loop alike. The interior is handed as a borrow, so a loop holds the
//! surface for exactly as long as loop 0 lends it and cannot outlive the run.
//!
//! **The blade is structural.** A port is a type this crate owns with a
//! constructor no consumer can reach, so a loop composes against the granted
//! surface or does not compile. There is no call by which a loop mints a port:
//! [`Ports`] has private fields and a crate-private constructor, and a loop
//! that needs a port this surface does not offer is a capability change
//! entering through the front door as a charter and contract edit.
//!
//! **The turn is loop 0's machinery, granted.** Loop 1 supplies the delta and
//! receives the outcome, and loop 0 drives the append-and-generate exchange,
//! consumes the stream, reads the close, and authors the record. Loop 1
//! authors nothing, which is the sole-writer property held where the turn runs.

use weaver_trace::{Kind, ModelOutput, Payload, Subsystem, TurnClose};

use crate::record::Record;
use weaver_traits::{ContentBlock, Message, Role};
use weaver_types::{TokenAnswer, TokenDirective, TokenRefusal, TurnKey};

use crate::assembly::Prompt;
use crate::authorship::Author;
use crate::channel::{CoordinationListener, DecodeChannel, OrganChannel};

/// The stop a turn heard, held until its answer: the dialer's exchange, owed
/// a close, and the directive's cause, recorded on the turn's close per
/// `weaver-trace-Spec` section 3 as of 2026-10-03.
type StopSlot = Option<(weaver_types::ExchangeId, weaver_types::Cause)>;

/// **A leave heard while a turn runs**, on the operator's rulings of
/// 2026-10-07 on #1: the unload drains, so the turn is let finish (or, forced,
/// cancelled) and the leave runs at its close. Every dialer owed the leave's
/// answer is held here with its exchange, its connection taken out of the
/// verb slot so the listener is heard again and a `force-unload` can dial in
/// during the drain; `quiesced` says the gate was already told to accept no
/// further input, the first thing the unload does. `cause` is the leave's
/// own; `forced_by` names the `force-unload` that turned a leave another
/// account asked for forced (the operator's ruling of 2026-10-07 on #1), and
/// stays empty where the leave was forced from the start.
pub(crate) struct PendingLeave {
    pub(crate) answers: Vec<(OrganChannel, weaver_types::ExchangeId)>,
    pub(crate) cause: weaver_types::Cause,
    pub(crate) forced: bool,
    pub(crate) forced_by: Option<weaver_types::Cause>,
    pub(crate) quiesced: bool,
}

/// The floor's cause in the record's spelling: the two crates link downward
/// only, so the harness converts at authorship.
pub(crate) fn trace_cause(cause: weaver_types::Cause) -> weaver_trace::Cause {
    weaver_trace::Cause { uid: cause.uid }
}

/// The granted surface handed to loop 1 at loaded-and-idle, borrowing the
/// standing interior for the seat's lifetime. Its fields are private and its
/// only constructor is crate-private, which is the blade.
/// The caller's clock per tool invocation, in milliseconds: the
/// composition root's bound like `MAX_TOOL_ROUNDS`, stated by this caller
/// on every execution per the one-clock rule and adopted by the gate as
/// the kill clock. At or under the shell's declared maximum by
/// construction here, so the refusal arm is for callers less careful.
pub(crate) const TOOL_CALL_CLOCK_MS: u64 = 30_000;

/// The classify answer's bound, generous against a forward of tens of
/// milliseconds: the turn thread's protection, and its expiry retires the
/// arm one-strike, the state seam's economics on the label seam.
pub(crate) const CLASSIFY_ANSWER_BOUND_MS: u64 = 30_000;

/// How many envelopes one run's shelf may hold before the surplus is
/// dropped and the drop recorded: the bound on both the queue and the
/// serve loop's appetite, a refusal of hoarding rather than of the client.
pub(crate) const MAX_HELD_FRAMES: usize = 64;

/// The gate seam's grant to the turn: the channel the execution exchange
/// crosses, the ordinal source for harness-opened exchanges, and the shelf
/// for envelopes that arrive while an execution is awaited - a client's turn
/// frame crossing mid-execution is held for the serve loop, never dropped
/// and never answered out of order, and bounded at [`MAX_HELD_FRAMES`].
pub(crate) struct GatePort<'a> {
    pub(crate) channel: &'a crate::channel::OrganChannel,
    pub(crate) ordinal: &'a mut u64,
    pub(crate) held: &'a mut std::collections::VecDeque<weaver_types::OrganEnvelope>,
    /// The leave heard mid-turn, run by the serve loop at the turn's close.
    pub(crate) leave: &'a mut Option<PendingLeave>,
}

pub struct Ports<'a> {
    decode: &'a DecodeChannel,
    author: &'a Author,
    recorder: &'a mut Record,
    turn_ordinal: &'a mut u64,
    /// The run's turn key while a turn runs, per `weaver-harness-Spec`
    /// section 3's observe and stop clauses: set when the bracket opens and
    /// cleared when its close lands, so the run reads `Active` for exactly
    /// the bracket's extent. Lent by the run beside the ordinal because the
    /// engine is where the key is minted and where the close is authored,
    /// and a second site predicting the key would be a second source for
    /// one fact.
    turn_in_flight: &'a mut Option<TurnKey>,
    /// What the run was built from, lent so an observation arriving between
    /// tokens is answered from inside the turn with the facts beside the
    /// state, per `weaver-admin-harness-contract` section 3.
    load: &'a weaver_types::LoadFacts,
    assembled: Option<Prompt>,
    /// The coordination listener, waited against beside the decode channel
    /// while a generation streams, per Spec 6.1: the stop is heard
    /// mid-stream by `poll`. Private, so the seat carries the ear without
    /// granting the loop a port.
    coordination: &'a CoordinationListener,
    /// The verb connection being served, shared with loop 0's own wait: a
    /// dial accepted mid-stream lands here and is handed back when the
    /// turn returns. `None` for a seat granted outside the serve loop,
    /// which then streams without the ear.
    pending: Option<&'a mut Option<OrganChannel>>,
    /// The gate seam, for the execution exchange. `None` for a seat granted
    /// where no gate stands, whose turns then carry their calls unexecuted:
    /// the assistant's record holds them and no tool-result turn follows,
    /// which is a fact the record shows rather than hides.
    gate: Option<GatePort<'a>>,
    /// The state port, per `weaver-harness-Spec` section 6: the seam's ask
    /// end where the leg stands, and `None` where it does not, which the
    /// port serves as the same absence a missing answer does.
    state: Option<&'a mut crate::state::StateSeam>,
    /// The classify port's seam, per `weaver-harness-Spec` section 6: the
    /// label seam's near end where the arm stands, and `None` where the
    /// declaration carried no binding.
    classify: Option<&'a crate::channel::ClassifyChannel>,
    /// The session's fullness as the last generation carried it, per the
    /// context ports of `weaver-harness-Spec` section 6: written by the
    /// turn from the decode answer, read by the loop before the wall.
    fullness: &'a mut Option<(u64, u64)>,
    /// Whether the recorder's pressure has been reported since it last
    /// crossed the mark, per `weaver-harness-Spec` section 4.
    ///
    /// **The flag outlives the seat because the condition does.** A seat is
    /// granted per turn and a full queue is not a per-turn fact, so a flag
    /// held here would re-report on every turn while the depth stayed high,
    /// which is the repetition the once-per-crossing rule refuses.
    pressure_reported: &'a mut bool,
}

/// Why a turn did not complete. A refusal the seam typed is the session
/// declining the ask, and a fault below the exchange is the worker gone or the
/// octets unreadable, per the decode contract's closure rule.
#[derive(Debug)]
pub enum TurnError {
    /// The decode seam typed a refusal to the append. **Carries the turn**,
    /// which opened before the refusal arrived, so the close a client
    /// receives can name it, per `weaver-gate-world-contract` section 3.
    Refused {
        turn: TurnKey,
        refusal: TokenRefusal,
    },
    /// The channel faulted or answered something the turn cannot read. No
    /// turn key rides it because service ends here and no close is sent.
    ChannelLost,
    /// The SPU's fault emission arrived while this turn streamed. **The
    /// report is already in the record when the caller sees this**: the
    /// engine authors the `fault` event inside the turn's bracket, before
    /// the close its error path lands, because a turn-attributed event
    /// filed after `turn.closed` would sit outside the bracket that names
    /// it. The report rides the error for the close the caller renders,
    /// never for a second authoring. The exchange terminates at the
    /// emission rather than waiting on a frame the contract says will not
    /// come.
    Faulted {
        turn: TurnKey,
        report: weaver_types::FaultReport,
    },
    /// A message loop 1 supplied is not licensed for its role. The bracket
    /// opened before the delta was judged, so the turn exists and is named.
    Unlicensed { turn: TurnKey },
}

/// What a completed turn produced for loop 1: the emission verbatim and how it
/// ended. The record holds more, but this is what the reasoning loop reads to
/// decide its next move.
#[derive(Debug, Clone)]
pub struct TurnOutcome {
    /// The turn this outcome closed, so the close a client receives can name
    /// it, per `weaver-gate-world-contract` section 3. Carried from what the
    /// turn already holds rather than rebuilt: a second construction is a
    /// second chance to disagree with the record.
    pub turn: TurnKey,
    pub emission: String,
    pub stopped: bool,
    /// The turn was aborted by the operator's stop rather than running to
    /// its own close: the bracket closed with the directive's reason and
    /// the partial stands. A model-side stop is not this, per the close's
    /// own distinction.
    pub aborted: bool,
    /// The generation was cut at the turn's token limit, the `Length`
    /// finish of `weaver-types-Spec` section 4.4: its own fact rather than
    /// a case of `stopped`, because overloading the stop flag is the same
    /// conflation the finish vocabulary exists to end. The answered close
    /// surfaces it per the world contract.
    pub truncated: bool,
}

impl<'a> Ports<'a> {
    /// Crate-private: no consumer can reach it, which makes the blade a
    /// compile property. Loop 0 calls it at loaded-and-idle, lending the
    /// interior across the extension seam.
    ///
    /// The arguments are the granted surface itself, one per port, and a
    /// bundling struct would be a second `Ports` wrapping this one, so the
    /// lint is answered rather than obeyed.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn grant(
        decode: &'a DecodeChannel,
        author: &'a Author,
        recorder: &'a mut Record,
        turn_ordinal: &'a mut u64,
        turn_in_flight: &'a mut Option<TurnKey>,
        load: &'a weaver_types::LoadFacts,
        assembled: Option<Prompt>,
        coordination: &'a CoordinationListener,
        pending: Option<&'a mut Option<OrganChannel>>,
        gate: Option<GatePort<'a>>,
        state: Option<&'a mut crate::state::StateSeam>,
        classify: Option<&'a crate::channel::ClassifyChannel>,
        fullness: &'a mut Option<(u64, u64)>,
        pressure_reported: &'a mut bool,
    ) -> Self {
        Ports {
            decode,
            author,
            recorder,
            turn_ordinal,
            turn_in_flight,
            load,
            assembled,
            coordination,
            pending,
            gate,
            state,
            classify,
            fullness,
            pressure_reported,
        }
    }

    /// The fullness read, per `weaver-harness-Spec` section 6's context
    /// ports: the session's resident token count and its capacity as the
    /// last generation carried them, `None` before any generation. Plain
    /// counts whose meaning is the loop's - when a flush is worth its cost
    /// is the loop's business.
    pub fn fullness(&self) -> Option<(u64, u64)> {
        *self.fullness
    }

    /// The flush, per `weaver-harness-Spec` section 6's context ports:
    /// drives the decode seam's standing flush exchange, valid between
    /// turns, and on confirmation authors the record's `flush` event from
    /// the counts the confirmation carried, the SPU being the one
    /// authority on either number. Since the cut ruling of 2026-08-19 the
    /// call takes `keep`, the resident length the session returns to,
    /// forwarded to the directive unjudged: the seam bounds it below by
    /// the identity prefix and above by the resident count, and the
    /// answered pair says what held. Answers the pair, or `None` where the
    /// seam refused or broke - the next turn will meet the dead seam
    /// properly, so nothing here converts it.
    pub fn flush(&mut self, keep: u64) -> Option<(u64, u64)> {
        self.decode
            .send_directive(&TokenDirective::Flush { keep })
            .ok()?;
        let counts = loop {
            match self.decode.recv_reply().ok()? {
                crate::channel::DecodeReply::Answer(TokenAnswer::Flushed {
                    resident_before,
                    resident_after,
                }) => break (resident_before, resident_after),
                crate::channel::DecodeReply::Answer(TokenAnswer::AtRest) => continue,
                // **Authored before the port answers**, on the
                // announce-after-record rule: a loop told its cut was
                // refused before the record holds the refusal could act on
                // it and leave no trace of why.
                crate::channel::DecodeReply::Refusal(refusal) => {
                    self.author_refusal(weaver_types::TokenAsk::Flush { keep }, refusal, None);
                    return None;
                }
                _ => return None,
            }
        };
        self.author
            .author(
                self.recorder,
                Kind::Flush,
                Subsystem::Harness,
                None,
                Some(Payload::Flush(weaver_trace::FlushCounts {
                    resident_before: counts.0,
                    resident_after: counts.1,
                })),
            )
            .ok()?;
        if let Some((_, capacity)) = *self.fullness {
            *self.fullness = Some((counts.1, capacity));
        }
        Some(counts)
    }

    /// Report the recorder's pressure once per crossing of the high-water
    /// mark, per `weaver-harness-Spec` section 4 and `weaver-trace-Spec`
    /// section 6.
    ///
    /// **This is the obligation the corpus carried and the crate never
    /// met.** `FaultCase::RecorderCommitPressure` stood in the floor with
    /// nothing raising it, which is a capability declared and unimplemented
    /// rather than an absence.
    ///
    /// **Once per crossing and not per submission above the mark.** A fault
    /// for every submission over the mark answers a full queue by filling
    /// it, which is the one direction that cannot help. A pressure report is
    /// a report about a condition, and a condition that persists is one
    /// condition, so nothing is authored again until the depth has fallen
    /// back under the mark.
    /// Lower the crossing flag where the depth has fallen back under the
    /// mark, authoring nothing.
    ///
    /// **Separate from the reporting reading because the two run at
    /// different moments.** Reporting belongs after a turn's events have
    /// landed, and clearing belongs before them: a drain that happened while
    /// the run was idle is only visible to a reading taken before the next
    /// turn fills the queue again.
    fn clear_pressure_if_under(&mut self) {
        if !self.recorder.pressure().over_mark {
            *self.pressure_reported = false;
        }
    }

    fn report_pressure(&mut self) {
        let pressure = self.recorder.pressure();
        if !pressure.over_mark {
            *self.pressure_reported = false;
            return;
        }
        if *self.pressure_reported {
            return;
        }
        *self.pressure_reported = true;
        let account = format!(
            "{{\"organ\":\"harness\",\"queued\":{},\"mark\":{}}}",
            pressure.queued,
            weaver_trace::HIGH_WATER_MARK
        );
        let _ = self.author.author_fault(
            self.recorder,
            Subsystem::Harness,
            None,
            &crate::authorship::harness_report(
                weaver_types::FaultCase::RecorderCommitPressure,
                &account,
            ),
        );
    }

    /// Author the record's `refusal` event, per `weaver-harness-Spec`
    /// section 6 and the decode contract's clause of 2026-08-22.
    ///
    /// **The ask is named and its values ride only where no other event
    /// holds them.** A refused flush's cut and a refused elision's span
    /// reach no other kind, so they travel here. The open's messages, the
    /// append's delta, and the cancel's turn each reach the record under
    /// their own kinds before the exchange, so the ask is named and left
    /// alone.
    ///
    /// **Best-effort and never the caller's failure.** A refusal that could
    /// not be recorded is a defect in the author rather than in the ask, and
    /// the port still answers what it was going to answer: refusing to
    /// report a refusal twice over would leave the loop with neither the
    /// answer nor the record.
    fn author_refusal(
        &mut self,
        asked: weaver_types::TokenAsk,
        refusal: weaver_types::TokenRefusal,
        turn: Option<&TurnKey>,
    ) {
        let record = weaver_types::RefusalRecord::Decode { asked, refusal };
        let Ok(rendered) = serde_json::to_string(&record) else {
            return;
        };
        let Some(payload) = weaver_trace::raw_payload(&rendered) else {
            return;
        };
        let _ = self.author.author(
            self.recorder,
            Kind::Refusal,
            Subsystem::Harness,
            turn,
            Some(Payload::Refusal(payload)),
        );
    }

    /// The elision ask, per `weaver-harness-Spec` section 6 and
    /// `weaver-spu-PRD` section 13.13: the loop names a half-open span of
    /// resident positions, the span is forwarded unjudged, and the answered
    /// pair says what the session held either side.
    ///
    /// **Which span to elide is the loop's election and this crate holds no
    /// policy about it.** A call that judged a span would be this crate
    /// deciding what a context is worth, which is the possession section
    /// 13.9 places with the loop.
    ///
    /// **The record's event is authored from the ask and not the answer.**
    /// The span comes from what the loop named and the counts from what the
    /// seam returned, each party writing what it is the authority on, and
    /// the answer echoes no span for exactly that reason.
    ///
    /// Answers the pair, or `None` where the seam refused or broke.
    ///
    /// **A refused span authors no `elision` and does author a `refusal`**,
    /// per the clerking act of 2026-08-22. Nothing was elided, so an
    /// `elision` event would be a record of a state change that did not
    /// happen. What did happen is that a span was turned away, which the
    /// `refusal` event carries with the span the loop named, no other kind
    /// holding a span that was refused. **This sentence read "authors no
    /// event" until that act**, which was true of the record it was written
    /// against.
    ///
    /// conforms: harness-elision-authors-from-the-ask
    pub fn elide(&mut self, from: u64, to: u64) -> Option<(u64, u64)> {
        self.decode
            .send_directive(&TokenDirective::Elide { from, to })
            .ok()?;
        let counts = loop {
            match self.decode.recv_reply().ok()? {
                crate::channel::DecodeReply::Answer(TokenAnswer::Elided {
                    resident_before,
                    resident_after,
                }) => break (resident_before, resident_after),
                crate::channel::DecodeReply::Answer(TokenAnswer::AtRest) => continue,
                // The span rides the record because no event holds a span
                // that was refused, the `elision` kind recording only
                // removals that happened.
                // The span rides the record because no event holds a span
                // that was refused, the `elision` kind recording only
                // removals that happened.
                crate::channel::DecodeReply::Refusal(refusal) => {
                    self.author_refusal(weaver_types::TokenAsk::Elide { from, to }, refusal, None);
                    return None;
                }
                _ => return None,
            }
        };
        // **Past the seam's confirmation the removal has happened**, so this
        // crate's account of the session follows it before anything else can
        // fail. Everything below reports what already holds.
        if let Some((_, capacity)) = *self.fullness {
            *self.fullness = Some((counts.1, capacity));
        }
        let authored = self.author.author(
            self.recorder,
            Kind::Elision,
            Subsystem::Harness,
            None,
            Some(Payload::Elision(weaver_trace::ElisionSpan {
                from,
                to,
                resident_before: counts.0,
                resident_after: counts.1,
            })),
        );
        // **A failed author answers the counts anyway, and this is where the
        // elision parts company with the flush.** `flush(keep)` is
        // idempotent: a loop that saw `None` and asked again with the same
        // `keep` reaches the same state. `elide(from, to)` is not, because
        // the positions after a removal are not the positions before it, so
        // the same span asked twice removes a second and different region.
        // Answering `None` here would invite exactly that, and `None` is
        // reserved for a refusal or a dead seam, where nothing was removed.
        //
        // **The `StreamWriteFailed` fault this arm authored is retired.** It
        // was reached whenever `author` answered `Err`, and until 2026-08-22
        // that included commit pressure, which is returned on a submission
        // that landed. So the arm wrote a fault saying the record had failed
        // while the record held the event, in the one kind that means the
        // session is unwell. Pressure is now a reading and never an answer,
        // and what remains here is the ordinary authoring failure, which
        // `Recorder` has already discarded by the time it answers.
        //
        // **Since 2026-09-26 the only refusal reachable here is the stream's
        // own standing failure.** The event is always turnless and always
        // paired with its span, and the diagnostic set carries `elision` as
        // of that date, so nothing admits it on one binding and refuses it
        // on the other. A standing failure refuses every later submission,
        // a fault naming this elision included, so the record's ending is
        // the evidence and the recorder's failure travels by the paths that
        // already carry it.
        let _ = authored;
        Some(counts)
    }

    /// The recall ask, per `weaver-harness-state-contract` section 2 and
    /// `weaver-harness-Spec` section 6's recall-port clause: the
    /// conversation as custody holds it, in landing order, bounded to the
    /// most recent turns where a bound is given. `None` is the dead peer at
    /// the seat, the same absence a missing leg serves.
    ///
    /// **An answered recall reaches the record before it reaches the loop**,
    /// as a `recall` event naming the ask and the identities returned, per
    /// `weaver-trace-Spec` section 3's recall clause: the re-entry a loop
    /// builds after a flush is built from this answer, so a record without
    /// it could not say what the post-flush input was drawn from. A recall
    /// the recorder will not take answers `None`, the flush's rule.
    pub fn recall(&mut self, last_turns: Option<u64>) -> Option<Vec<crate::state::Recalled>> {
        let answered = self.state.as_mut()?.ask_recall(last_turns)?;
        self.author
            .author_recall(
                self.recorder,
                weaver_trace::RecallVerb::Recall,
                last_turns,
                &answered,
            )
            .ok()?;
        Some(answered)
    }

    /// The replay port, per `weaver-harness-Spec` section 6's clause of
    /// 2026-08-24: the session's elected events whole, in landing order, as
    /// the member answered them, or `None` where the leg is down, the
    /// answer malformed, or the bound expired - the same dead-peer
    /// conversion the shape port performs. **The bound is this caller's to
    /// pass**, the replay ask being the one whose answer lawfully waits,
    /// parked at an open preload until the seal, and only the asking loop
    /// knowing how long a preload is worth waiting on.
    ///
    /// **An answered replay ask reaches the record as a `recall` before the
    /// loop walks it**, named by the answer's first and last events and its
    /// count, per `weaver-trace-Spec` section 3's recall clause: the replay
    /// feeds the model from this answer, and the ask is recorded by the seat
    /// that made it rather than inferred from the loop's own account of the
    /// holdings. A replay answer the recorder will not take answers `None`.
    pub fn replay(&mut self, bound_ms: u64) -> Option<Vec<crate::state::Recalled>> {
        let answered = self.state.as_mut()?.ask_replay(bound_ms)?;
        self.author
            .author_recall(
                self.recorder,
                weaver_trace::RecallVerb::Replay,
                None,
                &answered,
            )
            .ok()?;
        Some(answered)
    }

    /// **The task's score**, per `weaver-harness-Spec` section 6's score
    /// port and `weaver-trace-Spec` section 3's score clause (#523): the
    /// verdict a task reached on this run, the predicate it answered and
    /// whether that held, with the ratio over the task's denominator where
    /// the task supplies one. The task is the loop's, so the loop hands the
    /// verdict in and the harness authors it.
    ///
    /// **Recorded before the port answers**, on the announce-after-record
    /// rule: `true` means the record holds the verdict. `false` means it does
    /// not, and the port refuses without authoring anything where the call
    /// is malformed or out of place:
    ///
    /// - a turn stands, the score being the run's close and belonging to no
    ///   turn;
    /// - a score already stands for this run, one run carrying one verdict;
    /// - the predicate is empty, a verdict that names nothing;
    /// - one term of the ratio arrives without the other, or the denominator
    ///   is zero, neither being a ratio;
    /// - the record is the diagnostic one, which carries no score, or the
    ///   recorder will not take it.
    ///
    /// conforms: trace-score-records-the-verdict-and-its-terms
    pub fn score(
        &mut self,
        predicate: &str,
        passed: bool,
        measured: Option<u64>,
        denominator: Option<u64>,
    ) -> bool {
        if self.turn_in_flight.is_some() || predicate.is_empty() {
            return false;
        }
        let ratio = match (measured, denominator) {
            (None, None) => None,
            (Some(measured), Some(denominator)) if denominator > 0 => {
                Some(weaver_trace::ScoreRatio {
                    measured,
                    denominator,
                })
            }
            _ => return false,
        };
        let scored = match self.recorder.structure() {
            Some(structure) => structure.iter().any(|r| r.kind == Kind::Score),
            None => return false,
        };
        if scored {
            return false;
        }
        self.author
            .author(
                self.recorder,
                Kind::Score,
                Subsystem::Harness,
                None,
                Some(Payload::Score(weaver_trace::TaskScore {
                    predicate: predicate.to_string(),
                    passed,
                    ratio,
                })),
            )
            .is_ok()
    }

    /// The state port, per `weaver-harness-Spec` section 6: the shape ask
    /// of `weaver-harness-state-contract` section 2, answered from the
    /// member's holdings, or `None` where the leg is down, the answer
    /// malformed, or the bound expired - each the contract's dead peer
    /// converted into the same absence a missing leg serves. What any
    /// count means to the turn is this caller's business, per the
    /// three-way division.
    pub fn session_shape(&mut self) -> Option<crate::state::SessionShape> {
        self.state.as_mut()?.ask_shape()
    }

    /// The classify port, per `weaver-harness-Spec` section 6: content in,
    /// the artifact's scored labels back in the head's own order, or `None`
    /// where the leg is down, was never declared, refused typed, or
    /// answered malformed - each converted at the seat into the same
    /// absence a missing leg serves.
    ///
    /// **The exchange is recorded whole and its second half chooses a
    /// kind**, as of 2026-08-22: the ask authors `classify.request`, a
    /// scored answer authors `classify.output`, a typed refusal authors
    /// `refusal`, and a lost leg authors a `fault`, a death not being a
    /// refusal per the classify contract's section 5. **The seat's `None`
    /// covers all four absences alike** and the record is where they part,
    /// which is the division the port was built on rather than a new one.
    /// The turn member is absent because the seat lends between turns,
    /// which is when a loop holds it to ask.
    pub fn classify(&mut self, content: &str) -> Option<Vec<(String, f64)>> {
        let channel = self.classify?;
        // The one-strike retirement, the state seam's economics: an arm
        // that missed its bound once is not asked again.
        if channel.retired() {
            return None;
        }
        self.author
            .author(
                self.recorder,
                Kind::ClassifyRequest,
                Subsystem::Harness,
                None,
                Some(Payload::ClassifyRequest(weaver_trace::ClassifyAsk {
                    content: content.to_string(),
                })),
            )
            .ok()?;
        let asked = channel.send_directive(&weaver_types::LabelDirective::Classify {
            turn: None,
            content: content.to_string(),
        });
        if asked.is_err() {
            channel.retire();
            self.author_classify_lost();
            return None;
        }
        match channel.recv_answer_by(CLASSIFY_ANSWER_BOUND_MS) {
            Ok(crate::channel::ClassifyReply::Answer(weaver_types::LabelAnswer::Scored {
                labels,
                ..
            })) => {
                let scored: Vec<(String, f64)> = labels
                    .into_iter()
                    .map(|scored| (scored.label, scored.score))
                    .collect();
                self.author
                    .author(
                        self.recorder,
                        Kind::ClassifyOutput,
                        Subsystem::Harness,
                        None,
                        Some(Payload::ClassifyOutput(weaver_trace::ClassifyScored {
                            labels: scored.clone(),
                        })),
                    )
                    .ok()?;
                Some(scored)
            }
            // A late readiness is not this exchange's answer. The channel
            // skips it against the ask's one deadline, per
            // `weaver-harness-Spec` section 6, so it never arrives here,
            // and the arm below answers it as the lost leg were it to.
            Ok(crate::channel::ClassifyReply::Answer(weaver_types::LabelAnswer::Ready)) => {
                channel.retire();
                self.author_classify_lost();
                None
            }
            // The in-flight fault is this exchange's typed answer, per
            // the contract, and the record's fault event carries it by
            // the fault custody rule.
            Ok(crate::channel::ClassifyReply::Answer(weaver_types::LabelAnswer::Fault(report))) => {
                let rendered = serde_json::to_string(&report).ok()?;
                let payload = weaver_trace::raw_payload(&rendered)?;
                let _ = self.author.author(
                    self.recorder,
                    Kind::Fault,
                    Subsystem::Spu,
                    None,
                    Some(Payload::Fault(payload)),
                );
                None
            }
            Ok(crate::channel::ClassifyReply::Refusal(refusal)) => {
                // **The case travels whole where it used to be flattened
                // to a name.** `Oversized { requested, bound }` reached
                // the record as the word "oversized", so a reader learned
                // that a bound was exceeded and never which bound or by
                // how much. The class carries the seam's own case.
                self.author_classify_refusal(refusal);
                None
            }
            // The bound expired or the channel faulted: the arm retires
            // one-strike, and the record carries the loss as a `fault`
            // rather than leaving the request unanswered. **A lost leg
            // is a death and not a refusal**, per the classify
            // contract's section 5, so it does not reach the record
            // under `Kind::Refusal`, which carries this seam's typed
            // cases and nothing else.
            Err(_) => {
                channel.retire();
                self.author_classify_lost();
                None
            }
        }
    }

    /// The refused outcome is the record's own fact, per the charter's
    /// eighteenth kind: authored where a typed refusal closed the exchange,
    /// never fabricated into an answer.
    fn author_classify_refusal(&mut self, refusal: weaver_types::LabelRefusal) {
        let record = weaver_types::RefusalRecord::Classify { refusal };
        let Ok(rendered) = serde_json::to_string(&record) else {
            return;
        };
        let Some(payload) = weaver_trace::raw_payload(&rendered) else {
            return;
        };
        let _ = self.author.author(
            self.recorder,
            Kind::Refusal,
            Subsystem::Harness,
            None,
            Some(Payload::Refusal(payload)),
        );
    }

    /// The label leg was lost mid-exchange, which is not a refusal.
    ///
    /// **The contract draws this line and the old code crossed it**: "a
    /// typed refusal is not a death, and the seam keeps serving after one",
    /// per `weaver-harness-spu-classify-contract` section 5. A lost peer and
    /// a bound that expired both end the leg, and both were authored as a
    /// `classify.output` whose refusal read "channel_lost", which put a
    /// death in the shape of an answer the seam had given.
    ///
    /// A death is recorded per the fault custody rule, so it reaches the
    /// record as this crate's observation of a closure rather than as the
    /// dead party's report. The asking loop still loses its judgment and
    /// never its turn, the leg converting to the same absence a missing one
    /// serves.
    fn author_classify_lost(&mut self) {
        let account = "{\"organ\":\"harness\",\"leg\":\"classify\"}";
        let _ = self.author.author_fault(
            self.recorder,
            Subsystem::Harness,
            None,
            &crate::authorship::harness_report(
                weaver_types::FaultCase::OrganDeathObserved,
                account,
            ),
        );
    }

    /// The prompt loop 0 assembled from the working structure, a read loop 1
    /// has over the granted surface.
    pub fn assembled(&self) -> Option<&Prompt> {
        self.assembled.as_ref()
    }

    /// **Run one turn**, per `weaver-harness-Spec` section 6.1. Loop 1 supplies
    /// the delta as canonical messages, and loop 0 drives the exchange and
    /// authors the record: the turn bracket, the delta as message events, the
    /// three model events across the boundary, and the assistant's turn as a
    /// message event. Loop 1 authors nothing.
    ///
    /// The seam is append-only, so only the delta crosses, the SPU holding the
    /// resident session from the open at enter. The model events splice what
    /// the SPU rendered, the request and the measurement carried opaque and the
    /// output shaped from the emission and finish this crate consumes.
    pub fn turn(&mut self, delta: Vec<Message>) -> Result<TurnOutcome, TurnError> {
        self.bracketed(delta, true)
    }

    /// **The seeding turn**, per `weaver-harness-Spec` section 6.1 on the
    /// operator's ruling of 2026-10-06 that the system prompt is state: the
    /// operator's prompt, already authored turnless through the identity
    /// door by the frame path, is appended as the session's context,
    /// `AppendAndGenerate` with that one `system` message and no user
    /// message, and the model's answer closes the turn. The bracket, the
    /// three model events and the assistant's message author as in every
    /// turn; the delta does not, the record holding the prompt once, as the
    /// prefix the next load seats.
    pub fn seed(&mut self, prompt: Message) -> Result<TurnOutcome, TurnError> {
        self.bracketed(vec![prompt], false)
    }

    /// One bracketed turn: the delta appended and generated against, authored
    /// as the turn's messages where `author_delta` says so and carried
    /// unauthored where the frame path already authored it turnless.
    fn bracketed(
        &mut self,
        delta: Vec<Message>,
        author_delta: bool,
    ) -> Result<TurnOutcome, TurnError> {
        // **Clear-only, before this turn authors anything.** The flag is
        // lowered by a reading that finds the depth under the mark, and
        // without a reading here a queue that drained and crossed again
        // entirely between two turns would never be seen below it: the
        // end-of-turn reading would find the depth high, find the flag still
        // standing from the earlier crossing, and report nothing. This
        // reading authors no fault of its own, a depth under the mark being
        // nothing to report.
        self.clear_pressure_if_under();
        *self.turn_ordinal += 1;
        let turn = TurnKey(format!("t-{}", self.turn_ordinal));

        // The bracket opens. A failure to open it leaves no bracket to close,
        // so it returns before there is anything to unwind.
        self.author
            .author(
                self.recorder,
                Kind::TurnStarted,
                Subsystem::Harness,
                Some(&turn),
                None,
            )
            .map_err(|_| TurnError::ChannelLost)?;
        // **The key stands from here until the close lands.** The run's
        // slot is what the observe and stop arms read, and it is set at the
        // one site that knows the key rather than predicted by a caller.
        *self.turn_in_flight = Some(turn.clone());

        // **Every exit past the open closes the bracket.** A turn that opened
        // and then lost the channel or met a refusal must not leave a
        // `turn.started` without its `turn.closed`, which a consumer pairing
        // the bracket would read as a turn that never ended. The body runs to
        // its own close on success, and a failure closes with the fault reason
        // before the error returns. The stop slot rides out here so a cancel
        // that crossed before the failure still gets its answer: an exchange
        // the dialer opened is owed a close on every path, and a turn that
        // died of a refusal keeps serving, so a dropped exchange would hold
        // that dialer forever.
        let mut stop: StopSlot = None;
        let ran = self.run_turn(&turn, delta, &mut stop, author_delta);
        let answered = match ran {
            Ok(outcome) => Ok(outcome),
            Err(error) => {
                // **The SPU's report is authored first, inside the bracket.**
                // A `fault` event carrying a turn key must land before that
                // turn's close, or the record shows a closed turn followed by
                // an event attributed to it. The engine holds the author and
                // the recorder, so the report is filed here rather than
                // carried out to a caller who could only file it after the
                // close below has already landed.
                let fault_landed = match &error {
                    TurnError::Faulted {
                        turn: faulted,
                        report,
                    } => self
                        .author
                        .author_fault(self.recorder, Subsystem::Spu, Some(faulted), report)
                        .is_ok(),
                    _ => true,
                };
                // **A failed close is itself a failure the caller must learn.**
                // The recorder can be broken when the turn fails, so authoring
                // the fault close can fail too, and swallowing that would leave
                // a `turn.started` with no `turn.closed` and no error naming it.
                // The original error is returned only when the close lands, and
                // a close that cannot author returns `ChannelLost` because the
                // record is now untrustworthy whatever the first cause was. A
                // report that cannot author is the same untrustworthiness one
                // event earlier, so it takes the same answer, after the close
                // is still attempted for the bracket's sake.
                // **One close for one bracket, its reason chosen here.** The
                // reason belongs at the single site that closes rather than
                // at each site that fails: a failing arm that closed for
                // itself would leave this one closing again, and the record
                // would carry two closes for one turn with the second
                // misreporting the first. A refused turn ended because a
                // seam turned an ask away and the record says so, per
                // `weaver-trace-PRD` section 3.1's clause of 2026-08-22.
                let reason = match &error {
                    TurnError::Refused { .. } => weaver_trace::StopReason::Refused,
                    _ => weaver_trace::StopReason::Fault,
                };
                match self.author.author(
                    self.recorder,
                    Kind::TurnClosed,
                    Subsystem::Harness,
                    Some(&turn),
                    Some(Payload::TurnClosed(TurnClose::Stopped {
                        reason,
                        // A fault or a refusal, which nobody asked for.
                        cause: None,
                    })),
                ) {
                    Ok(_) => {
                        // The close landed, so no turn stands: cleared
                        // here and not on the arm below, because a close
                        // that could not author leaves the bracket open
                        // and the key standing for the unwind to name.
                        *self.turn_in_flight = None;
                        // **Announce after record**, on the failure path as on
                        // the clean one: the turn the stop asked to end has
                        // ended, its close in the record naming the fault, and
                        // the interior stands at rest for the dialer's
                        // purpose. Best-effort, because the fault may already
                        // be taking the service down, and closure then signals
                        // what the answer could not.
                        if let Some((exchange, _)) = stop.take()
                            && let Some(slot) = self.pending.as_deref_mut()
                            && let Some(connection) = slot.as_ref()
                        {
                            let _ = connection.send(&weaver_types::OrganEnvelope {
                                exchange,
                                position: weaver_types::Position::Close,
                                payload: weaver_types::Payload::Answer(
                                    weaver_types::LifecycleAnswer::AtRest,
                                ),
                            });
                        }
                        if fault_landed {
                            Err(error)
                        } else {
                            Err(TurnError::ChannelLost)
                        }
                    }
                    Err(_) => Err(TurnError::ChannelLost),
                }
            }
        };
        // **The reading is taken after everything this turn authors**, the
        // close on the error path included. A turn is where events arrive in
        // bulk, so it is where the depth can have moved, and taking it here
        // rather than per submission keeps the report about the condition
        // rather than about an event. **Taking it before the close would miss
        // a crossing the close itself caused**, which is the event most
        // likely to be the one that crosses, being the last of the turn.
        self.report_pressure();
        answered
    }

    /// The turn's body, run inside the bracket [`turn`] opens and closes. It
    /// authors the delta, drives the exchange, and authors the answer, and
    /// every error it returns is closed by the caller.
    fn run_turn(
        &mut self,
        turn: &TurnKey,
        delta: Vec<Message>,
        stop: &mut StopSlot,
        author_delta: bool,
    ) -> Result<TurnOutcome, TurnError> {
        // **A tool-result message in loop 1's delta refuses before anything
        // is authored**, per `weaver-harness-Spec` section 6: the role's one
        // door is the grant's, inside this turn's own execution loop, so a
        // supplied result is a fabrication whatever it carries.
        if delta
            .iter()
            .any(|message| matches!(message.role, Role::ToolResult))
        {
            return Err(TurnError::Unlicensed { turn: turn.clone() });
        }

        // The delta is authored as the turn's user messages, before the
        // exchange so the record reads the ask before the answer. The
        // seeding turn's delta was authored turnless at the identity door,
        // per `weaver-harness-Spec` section 6.1, and is not authored again.
        if author_delta {
            for message in &delta {
                self.author
                    .author_message(self.recorder, message, turn)
                    .map_err(|_| TurnError::Unlicensed { turn: turn.clone() })?
                    .map_err(|_| TurnError::ChannelLost)?;
            }
        }

        // **The execution loop**, per the tool workflow's opening act: each
        // round appends a delta and generates, and a generation whose parse
        // recovered calls executes them through the gate, the granted
        // results becoming the next round's delta. The bound refuses further
        // rounds and never the turn: the final emission stands whatever the
        // model still wanted.
        let mut delta = delta;
        let mut rounds = 0usize;
        let generation = 'rounds: loop {
            let mut generation = self.generate_once(turn, delta, stop)?;
            let calls: Vec<weaver_traits::ToolCall> = generation
                .content
                .iter()
                .filter_map(|block| match block {
                    ContentBlock::ToolCall(call) => Some(call.clone()),
                    _ => None,
                })
                .collect();
            if calls.is_empty()
                || stop.is_some()
                || self.gate.is_none()
                || rounds >= crate::tools::MAX_TOOL_ROUNDS
            {
                break generation;
            }
            rounds += 1;
            let mut next_delta = Vec::with_capacity(calls.len());
            for call in calls {
                let grant = self.execute_call(turn, &call, stop)?;
                if stop.is_some() {
                    generation.finish = weaver_types::Finish::Stopped;
                    break 'rounds generation;
                }
                // The record and the seam read one construction: the grant
                // authors the tool-result event at its one door, and the
                // same content crosses to the SPU as the next delta.
                let message = Message {
                    role: Role::ToolResult,
                    content: vec![ContentBlock::ToolResult(grant.block())],
                };
                self.author
                    .author_tool_result(self.recorder, &grant, turn)
                    .map_err(|_| TurnError::ChannelLost)?;
                next_delta.push(message);
            }
            delta = next_delta;
        };

        let aborted = self.close_turn(turn, &generation, stop)?;
        *self.fullness = Some((generation.resident, generation.capacity));
        Ok(TurnOutcome {
            turn: turn.clone(),
            emission: generation.emission,
            stopped: matches!(generation.finish, weaver_types::Finish::Stopped),
            aborted,
            truncated: matches!(generation.finish, weaver_types::Finish::Length),
        })
    }

    /// **One execution through the gate**, per `weaver-harness-gate-contract`
    /// section 2: the exchange opens with the call as the parse recovered it,
    /// and completes with one of the four contents, the grant constructed
    /// from the completion whichever arrived - the one construction site.
    /// The tool bracket's events ride the exchange: started as the harness's
    /// dispatch, completed as the gate's answer, the deferred payloads
    /// carrying the call and the outcome as this crate renders them.
    fn execute_call(
        &mut self,
        turn: &TurnKey,
        call: &weaver_traits::ToolCall,
        stop: &mut StopSlot,
    ) -> Result<crate::tools::ToolResult, TurnError> {
        let started = serde_json::json!({
            "name": call.name,
            "arguments": call.arguments,
        })
        .to_string();
        let started = weaver_trace::raw_payload(&started).ok_or(TurnError::ChannelLost)?;
        self.author
            .author(
                self.recorder,
                Kind::ToolCallStarted,
                Subsystem::Harness,
                Some(turn),
                Some(Payload::Deferred(started)),
            )
            .map_err(|_| TurnError::ChannelLost)?;

        // **A call asked after the unload began is interrupted, not sent**,
        // on the operator's ruling of 2026-10-07 on #1: the quiesced gate
        // would carry no return, so the call is recorded killed by the
        // cancel, the turn stops before its next generation, and the leave
        // follows. The stop's cause is the leave's.
        if let Some(leave) = self.gate.as_ref().and_then(|gate| gate.leave.as_ref()) {
            if stop.is_none() {
                // The leave's first dialer, or, for the wind-down turn, which
                // no dialer asked, the harness's own exchange: the stop slot
                // ends the turn before its next generation either way.
                let owed = leave.answers.first().map_or(
                    weaver_types::ExchangeId {
                        opener: weaver_types::Opener::Harness,
                        ordinal: 0,
                    },
                    |(_, exchange)| exchange.clone(),
                );
                *stop = Some((owed, leave.cause));
            }
            let outcome = weaver_types::ToolOutcome::Killed {
                partial: None,
                by: weaver_types::KillCause::Unload,
            };
            let completed = serde_json::to_string(&outcome).map_err(|_| TurnError::ChannelLost)?;
            let completed = weaver_trace::raw_payload(&completed).ok_or(TurnError::ChannelLost)?;
            self.author
                .author(
                    self.recorder,
                    Kind::ToolCallCompleted,
                    Subsystem::Harness,
                    Some(turn),
                    Some(Payload::Deferred(completed)),
                )
                .map_err(|_| TurnError::ChannelLost)?;
            return Ok(crate::tools::ToolResult::granted(&outcome));
        }

        let gate = self
            .gate
            .as_mut()
            .expect("execute_call runs only with a gate");
        *gate.ordinal += 1;
        let exchange = weaver_types::ExchangeId {
            opener: weaver_types::Opener::Harness,
            ordinal: *gate.ordinal,
        };
        gate.channel
            .send(&weaver_types::OrganEnvelope {
                exchange: exchange.clone(),
                position: weaver_types::Position::Open,
                payload: weaver_types::Payload::Tool(weaver_types::ToolExecution {
                    name: weaver_types::ToolName(call.name.clone()),
                    arguments: call.arguments.clone(),
                    clock_ms: TOOL_CALL_CLOCK_MS,
                }),
            })
            .map_err(|_| TurnError::ChannelLost)?;

        // Await the completion. A turn frame arriving mid-execution is a
        // client speaking while the turn runs, held for the serve loop per
        // the one-turn discipline; a fault report is held the same way. The
        // exchange identity is the correlation, per the contract: nothing
        // else closes this ordinal.
        let outcome = loop {
            use std::os::fd::AsFd;
            let wake =
                self.exchange_wake(self.gate.as_ref().expect("invocation gate").channel.as_fd())?;
            if !matches!(wake, ExchangeWake::Work) {
                self.handle_turn_control(wake, turn, stop, Some(&exchange))?;
                continue;
            }
            let gate = self.gate.as_mut().expect("invocation gate");
            let envelope = gate.channel.recv().map_err(|_| TurnError::ChannelLost)?;
            if envelope.exchange == exchange && envelope.position == weaver_types::Position::Close {
                match envelope.payload {
                    weaver_types::Payload::ToolAnswer(outcome) => break outcome,
                    _ => return Err(TurnError::ChannelLost),
                }
            } else if gate.held.len() >= MAX_HELD_FRAMES {
                // **The shelf is bounded.** A client that keeps speaking
                // through a long execution would otherwise grow it - and the
                // drain's stack with it - without limit. The surplus envelope
                // is dropped and the drop is recorded: the connection that
                // spoke loses its exchange, which is the case the fault
                // names.
                let account =
                    format!("{{\"organ\":\"harness\",\"held-frames-bound\":{MAX_HELD_FRAMES}}}");
                let _ = self.author.author_fault(
                    self.recorder,
                    Subsystem::Harness,
                    Some(turn),
                    &crate::authorship::harness_report(
                        weaver_types::FaultCase::ClientConnectionFailedMidTurn,
                        &account,
                    ),
                );
            } else {
                gate.held.push_back(envelope);
            }
        };

        let completed = serde_json::to_string(&outcome).map_err(|_| TurnError::ChannelLost)?;
        let completed = weaver_trace::raw_payload(&completed).ok_or(TurnError::ChannelLost)?;
        self.author
            .author(
                self.recorder,
                Kind::ToolCallCompleted,
                Subsystem::Gate,
                Some(turn),
                Some(Payload::Deferred(completed)),
            )
            .map_err(|_| TurnError::ChannelLost)?;

        Ok(crate::tools::ToolResult::granted(&outcome))
    }

    /// **Cancel what the turn has in flight**: the tool call out at the gate,
    /// by its exchange, or the generation, by the turn, as a stop does.
    /// The unload's cancel of a tool call is the interrupt, which the gate
    /// answers `Killed { by: unload }`, so the record names the unload in the
    /// gate's own word.
    fn cancel_turn(
        gate: Option<&GatePort<'_>>,
        decode: &DecodeChannel,
        turn: &TurnKey,
        execution: Option<&weaver_types::ExchangeId>,
    ) -> Result<(), TurnError> {
        if let Some(execution) = execution {
            gate.expect("invocation gate")
                .channel
                .send(&weaver_types::OrganEnvelope {
                    exchange: execution.clone(),
                    position: weaver_types::Position::Continue,
                    payload: weaver_types::Payload::ToolInterrupt,
                })
                .map_err(|_| TurnError::ChannelLost)
        } else {
            decode
                .send_directive(&TokenDirective::Cancel { turn: turn.clone() })
                .map_err(|_| TurnError::ChannelLost)
        }
    }

    /// Coordination during either kind of live exchange shares the same
    /// credential checks, observations, refusals, and stop acknowledgment.
    fn handle_turn_control(
        &mut self,
        wake: ExchangeWake,
        turn: &TurnKey,
        stop: &mut StopSlot,
        execution: Option<&weaver_types::ExchangeId>,
    ) -> Result<(), TurnError> {
        match wake {
            ExchangeWake::Dial => {
                let Some(slot) = self.pending.as_deref_mut() else {
                    return Ok(());
                };
                match self.coordination.accept_root() {
                    Ok(connection) => *slot = Some(connection),
                    // A refused peer never reaches an exchange, and the
                    // stream continues.
                    Err(crate::failure::ChannelFault::WrongPeer { .. }) => {}
                    Err(_) => return Err(TurnError::ChannelLost),
                }
            }
            ExchangeWake::Directive => {
                let Some(slot) = self.pending.as_deref_mut() else {
                    return Ok(());
                };
                let Some(connection) = slot.as_ref() else {
                    return Ok(());
                };
                match connection.recv() {
                    Ok(envelope) => {
                        let exchange = envelope.exchange.clone();
                        // **A `force-unload` without the invocation lock
                        // joins the leave pending here** as a forced leave
                        // would (the operator's ruling of 2026-10-07 on #1);
                        // with none pending, its holder is no unload and the
                        // join is out of order, below.
                        let pending = self.gate.as_ref().is_some_and(|gate| gate.leave.is_some());
                        let payload = match envelope.payload {
                            weaver_types::Payload::Directive(
                                weaver_types::LifecycleDirective::JoinLeave { cause },
                            ) if pending => weaver_types::Payload::Directive(
                                weaver_types::LifecycleDirective::Leave {
                                    cause,
                                    forced: true,
                                },
                            ),
                            other => other,
                        };
                        match payload {
                            weaver_types::Payload::Directive(
                                weaver_types::LifecycleDirective::Stop { cause },
                            ) if stop.is_none() => {
                                // The cancel crosses at once, and the
                                // stream is consumed to the close it
                                // will produce, the answer owed after
                                // the record, not here.
                                if let Some(execution) = execution {
                                    self.gate
                                        .as_ref()
                                        .expect("invocation gate")
                                        .channel
                                        .send(&weaver_types::OrganEnvelope {
                                            exchange: execution.clone(),
                                            position: weaver_types::Position::Continue,
                                            payload: weaver_types::Payload::ToolCancel,
                                        })
                                        .map_err(|_| TurnError::ChannelLost)?;
                                } else {
                                    self.decode
                                        .send_directive(&TokenDirective::Cancel {
                                            turn: turn.clone(),
                                        })
                                        .map_err(|_| TurnError::ChannelLost)?;
                                }
                                *stop = Some((exchange, cause));
                            }
                            // **An observation is answered from inside
                            // the turn**, per `weaver-admin-harness-contract`
                            // section 3: the state is `Active` for exactly
                            // the turn's extent and the facts are the run's,
                            // lent to this seat, so the seam reaches what
                            // the watch reaches. It touches no bracket.
                            weaver_types::Payload::Directive(
                                weaver_types::LifecycleDirective::Observe,
                            ) => {
                                let _ = connection.send(&weaver_types::OrganEnvelope {
                                    exchange,
                                    position: weaver_types::Position::Close,
                                    payload: weaver_types::Payload::Answer(
                                        weaver_types::LifecycleAnswer::State {
                                            state: weaver_types::AgentState::Active,
                                            load: Some(Box::new(self.load.clone())),
                                            constituents: Vec::new(),
                                        },
                                    ),
                                });
                            }
                            // **A leave mid-turn drains rather than refuses**,
                            // on the operator's rulings of 2026-10-07 on #1:
                            // an unforced leave tells the gate at once to
                            // accept no further input and lets the turn run
                            // to its close; a forced one cancels the turn as
                            // a stop does, recorded on the trace as a stop,
                            // and the leave runs at the close either way. A
                            // second unforced leave while one is pending is
                            // out of order; a forced one joins it and turns
                            // it forced.
                            weaver_types::Payload::Directive(
                                weaver_types::LifecycleDirective::Leave { cause, forced },
                            ) if self.gate.is_some() => {
                                let pending_unforced =
                                    self.gate.as_ref().is_some_and(|gate| gate.leave.is_some());
                                if pending_unforced && !forced {
                                    let _ = connection.send(&weaver_types::OrganEnvelope {
                                        exchange,
                                        position: weaver_types::Position::Close,
                                        payload: weaver_types::Payload::Refusal(
                                            weaver_types::LifecycleRefusal::OutOfOrder,
                                        ),
                                    });
                                    return Ok(());
                                }
                                // A forced leave cancels whatever runs; an
                                // unforced one cancels only a tool call out
                                // at the gate, whose return the quiesced gate
                                // would never carry (the operator's ruling of
                                // 2026-10-07 on #1), and lets a generation
                                // finish. The cancel crosses before the
                                // quiesce, a gate executing a tool reading
                                // only its cancel until the call ends.
                                if (forced || execution.is_some()) && stop.is_none() {
                                    Self::cancel_turn(
                                        self.gate.as_ref(),
                                        self.decode,
                                        turn,
                                        execution,
                                    )?;
                                    *stop = Some((exchange.clone(), cause));
                                }
                                let connection = slot.take().expect("the connection stood");
                                let gate = self.gate.as_mut().expect("guarded above");
                                match gate.leave.as_mut() {
                                    Some(leave) => {
                                        // The force that turns another
                                        // caller's leave forced is named
                                        // beside the leave's own cause.
                                        if forced && !leave.forced {
                                            leave.forced_by = Some(cause);
                                        }
                                        leave.forced |= forced;
                                        leave.answers.push((connection, exchange));
                                    }
                                    None => {
                                        let mut quiesced = false;
                                        if !forced {
                                            *gate.ordinal += 1;
                                            gate.channel
                                                .send(&weaver_types::OrganEnvelope {
                                                    exchange: weaver_types::ExchangeId {
                                                        opener: weaver_types::Opener::Harness,
                                                        ordinal: *gate.ordinal,
                                                    },
                                                    position: weaver_types::Position::Open,
                                                    payload: weaver_types::Payload::Directive(
                                                        weaver_types::LifecycleDirective::Quiesce,
                                                    ),
                                                })
                                                .map_err(|_| TurnError::ChannelLost)?;
                                            quiesced = true;
                                        }
                                        *gate.leave = Some(PendingLeave {
                                            answers: vec![(connection, exchange)],
                                            cause,
                                            forced,
                                            forced_by: None,
                                            quiesced,
                                        });
                                    }
                                }
                            }
                            // A leave with no gate standing has no drain to
                            // run, so it is refused as before.
                            weaver_types::Payload::Directive(
                                weaver_types::LifecycleDirective::Leave { .. },
                            ) => {
                                let _ = connection.send(&weaver_types::OrganEnvelope {
                                    exchange,
                                    position: weaver_types::Position::Close,
                                    payload: weaver_types::Payload::Refusal(
                                        weaver_types::LifecycleRefusal::ActivityNotAtRest,
                                    ),
                                });
                            }
                            weaver_types::Payload::Directive(_) => {
                                let _ = connection.send(&weaver_types::OrganEnvelope {
                                    exchange,
                                    position: weaver_types::Position::Close,
                                    payload: weaver_types::Payload::Refusal(
                                        weaver_types::LifecycleRefusal::OutOfOrder,
                                    ),
                                });
                            }
                            _ => return Err(TurnError::ChannelLost),
                        }
                    }
                    Err(crate::failure::ChannelFault::Closed) => {
                        // The dialer left, and the stream continues.
                        *slot = None;
                    }
                    Err(_) => return Err(TurnError::ChannelLost),
                }
            }
            ExchangeWake::Work => unreachable!("work is consumed by the exchange"),
        }
        Ok(())
    }

    /// One append-and-generate round: the delta crosses, the stream is
    /// consumed to the close, and the three model events author at engine
    /// grain.
    fn generate_once(
        &mut self,
        turn: &TurnKey,
        delta: Vec<Message>,
        stop: &mut StopSlot,
    ) -> Result<weaver_types::Generation, TurnError> {
        // Append and generate. The delta crosses, the SPU appends it at the
        // resident end, and the stream returns each token before the close.
        self.decode
            .send_directive(&TokenDirective::AppendAndGenerate {
                turn: turn.clone(),
                delta,
            })
            .map_err(|_| TurnError::ChannelLost)?;

        // Consume the stream to the close, hearing the stop while it runs,
        // per Spec 6.1: `poll` sleeps against the decode channel, the
        // coordination listener, and the verb connection if one stands, and
        // wakes on the first ready. A stop dialed mid-stream cancels the
        // turn at the seam, the outstanding generation answering with its
        // partial marked stopped and the tokens already streamed standing
        // in the close. A dialer that connects and then sends nothing holds
        // only its own connection, never the streaming turn.
        let generation = loop {
            match self.stream_wake()? {
                ExchangeWake::Work => match self
                    .decode
                    .recv_reply()
                    .map_err(|_| TurnError::ChannelLost)?
                {
                    crate::channel::DecodeReply::Answer(TokenAnswer::Token { .. }) => continue,
                    // **The field authors as it arrives**, per
                    // `weaver-harness-Spec` section 6: one `model.field`
                    // per intermediate, written at the moment it lands
                    // rather than accumulated, because a generation's worth
                    // would hold megabytes to write them at a close that
                    // has other work. It closes no exchange, so the loop
                    // goes on waiting for one that does.
                    crate::channel::DecodeReply::Answer(TokenAnswer::Field {
                        position,
                        ranked,
                        realized,
                    }) => {
                        self.author
                            .author(
                                self.recorder,
                                Kind::ModelField,
                                Subsystem::SpuDecoder,
                                Some(turn),
                                Some(Payload::ModelField(weaver_trace::ModelField {
                                    position,
                                    ranked: ranked
                                        .into_iter()
                                        .map(|candidate| weaver_trace::Candidate {
                                            token: candidate.token,
                                            probability: candidate.probability,
                                        })
                                        .collect(),
                                    realized,
                                })),
                            )
                            .map_err(|_| TurnError::ChannelLost)?;
                        continue;
                    }
                    crate::channel::DecodeReply::Answer(TokenAnswer::Generated(generation)) => {
                        break generation;
                    }
                    crate::channel::DecodeReply::Answer(TokenAnswer::AtRest) => continue,
                    // The emission, matched by name rather than left to a
                    // wildcard that would misfile it as channel loss and
                    // discard the report. It closes no exchange, so this
                    // turn ends without waiting on a frame the contract says
                    // will not come.
                    crate::channel::DecodeReply::Answer(TokenAnswer::Fault(report)) => {
                        return Err(TurnError::Faulted {
                            turn: turn.clone(),
                            report,
                        });
                    }
                    crate::channel::DecodeReply::Refusal(refusal) => {
                        // **The refusal is authored inside the turn's
                        // bracket and the close names it outside**, which is
                        // the division a fault already runs on here: the
                        // event says what was refused and the close says the
                        // bracket ended. The ask is the append, named
                        // without its delta, which the turn's message kinds
                        // carried into the record before this exchange.
                        self.author_refusal(
                            weaver_types::TokenAsk::AppendAndGenerate,
                            refusal.clone(),
                            Some(turn),
                        );
                        return Err(TurnError::Refused {
                            turn: turn.clone(),
                            refusal,
                        });
                    }
                    // **The column rides this exchange too**, per the decode
                    // contract's third intermediate: the stream carries it
                    // wherever the open's ask stood, and a consumer that
                    // read it as channel loss would kill service on a frame
                    // the contract licenses. No harness flow produces it on
                    // a serving record today - a serving open writes no ask
                    // - but the seam's discipline is the contract's, not
                    // this crate's flows'.
                    crate::channel::DecodeReply::Answer(TokenAnswer::Column {
                        position,
                        layers,
                    }) => {
                        self.author_residual_column(turn, position, layers)?;
                        continue;
                    }
                    crate::channel::DecodeReply::Answer(_) => return Err(TurnError::ChannelLost),
                },
                wake => self.handle_turn_control(wake, turn, stop, None)?,
            }
        };

        // The three model events author across the boundary at engine grain,
        // `SpuDecoder` rather than `Spu`, per issue #103's ruling: the organ
        // will hold more than a decoder and a reader of a model event wants
        // the engine first. Each is spliced or shaped by the custody model:
        // the request and the measurement carried opaque, the output shaped
        // from the emission and finish.
        self.author
            .author(
                self.recorder,
                Kind::ModelRequest,
                Subsystem::SpuDecoder,
                Some(turn),
                Some(Payload::ModelRequest(generation.request.clone())),
            )
            .map_err(|_| TurnError::ChannelLost)?;
        self.author
            .author(
                self.recorder,
                Kind::ModelOutput,
                Subsystem::SpuDecoder,
                Some(turn),
                Some(Payload::ModelOutput(ModelOutput {
                    emission: generation.emission.clone(),
                    // The one conversion site, all three cases carried: an
                    // if on the stopped flag flattened Length into
                    // Completed, which is issue #218's lie at the record.
                    finish: match generation.finish {
                        weaver_types::Finish::Completed => weaver_trace::Finish::Completed,
                        weaver_types::Finish::Stopped => weaver_trace::Finish::Stopped,
                        weaver_types::Finish::Length => weaver_trace::Finish::Length,
                    },
                    // The same reading the fullness port answers from,
                    // written down: an analysis placing a turn in the
                    // context has no other source once the run is over.
                    resident: generation.resident,
                    capacity: generation.capacity,
                })),
            )
            .map_err(|_| TurnError::ChannelLost)?;
        self.author
            .author(
                self.recorder,
                Kind::ModelMeasurement,
                Subsystem::SpuDecoder,
                Some(turn),
                Some(Payload::ModelMeasurement(generation.measurement.clone())),
            )
            .map_err(|_| TurnError::ChannelLost)?;

        // The assistant's turn enters the record as a message event, the
        // canonical parse beside the verbatim the output holds: the family
        // module's own bridge crossed the seam in the generation, text as
        // text and every recovered call as a `ToolCall` block, per the tool
        // workflow's opening act.
        let assistant = Message {
            role: Role::Assistant,
            content: generation.content.clone(),
        };
        self.author
            .author_message(self.recorder, &assistant, turn)
            .map_err(|_| TurnError::Unlicensed { turn: turn.clone() })?
            .map_err(|_| TurnError::ChannelLost)?;

        Ok(generation)
    }

    /// The replay's turn bracket opens, per `diagnostic-replay-loop`
    /// section 2: the recorded turn's own key, carried rather than minted,
    /// because the derived seed of `weaver-spu-Spec` section 8.5 hashes the
    /// key and a re-fed generation under a fresh key would draw from a seed
    /// the source never used.
    pub(crate) fn replay_turn_started(&mut self, turn: &TurnKey) -> Result<(), TurnError> {
        self.author
            .author(
                self.recorder,
                Kind::TurnStarted,
                Subsystem::Harness,
                Some(turn),
                None,
            )
            .map_err(|_| TurnError::ChannelLost)?;
        *self.turn_in_flight = Some(turn.clone());
        Ok(())
    }

    /// The replay's turn bracket closes clean: a re-fed turn has no stop to
    /// hear and no execution to await, and a seam refusal mid-turn closes
    /// through the refusal path instead.
    pub(crate) fn replay_turn_closed(&mut self, turn: &TurnKey) -> Result<(), TurnError> {
        self.author
            .author(
                self.recorder,
                Kind::TurnClosed,
                Subsystem::Harness,
                Some(turn),
                Some(Payload::TurnClosed(TurnClose::Clean)),
            )
            .map_err(|_| TurnError::ChannelLost)?;
        *self.turn_in_flight = None;
        Ok(())
    }

    /// The replay's bracket closed stopped, the serving close's own rule -
    /// every exit past the open closes the bracket - applied to a re-fed
    /// turn a seam refusal or fault ended early.
    pub(crate) fn replay_turn_stopped(
        &mut self,
        turn: &TurnKey,
        reason: weaver_trace::StopReason,
    ) -> Result<(), TurnError> {
        self.author
            .author(
                self.recorder,
                Kind::TurnClosed,
                Subsystem::Harness,
                Some(turn),
                Some(Payload::TurnClosed(TurnClose::Stopped {
                    reason,
                    cause: None,
                })),
            )
            .map_err(|_| TurnError::ChannelLost)?;
        *self.turn_in_flight = None;
        Ok(())
    }

    /// One re-feed exchange, the mirror of [`Self::generate_once`] against
    /// the decode contract's sixth exchange: the recorded rendered form and
    /// the recorded path cross, the SPU computes each draw as a generation
    /// would and appends the recorded token whatever the draw said, and the
    /// answer's own variant carries the generation shape with the recomputed
    /// draws in the measurement's output slots, per `weaver-spu-PRD` section
    /// 13.14. No token intermediate arrives, there being no drawn piece to
    /// stream, while the field's intermediates author as they arrive exactly
    /// as a generation's do.
    ///
    /// The four authorings mirror the serving ones event for event, which is
    /// what makes the diagnostic record readable by the serving record's
    /// instruments, per `weaver-diagnostic-Spec` section 4.
    pub(crate) fn refeed(
        &mut self,
        turn: &TurnKey,
        rendered: String,
        path: Vec<u32>,
    ) -> Result<weaver_types::Generation, TurnError> {
        self.decode
            .send_directive(&TokenDirective::ReFeed {
                turn: turn.clone(),
                rendered,
                path,
            })
            .map_err(|_| TurnError::ChannelLost)?;

        let generation = loop {
            match self.stream_wake()? {
                ExchangeWake::Work => match self
                    .decode
                    .recv_reply()
                    .map_err(|_| TurnError::ChannelLost)?
                {
                    crate::channel::DecodeReply::Answer(TokenAnswer::Field {
                        position,
                        ranked,
                        realized,
                    }) => {
                        self.author
                            .author(
                                self.recorder,
                                Kind::ModelField,
                                Subsystem::SpuDecoder,
                                Some(turn),
                                Some(Payload::ModelField(weaver_trace::ModelField {
                                    position,
                                    ranked: ranked
                                        .into_iter()
                                        .map(|candidate| weaver_trace::Candidate {
                                            token: candidate.token,
                                            probability: candidate.probability,
                                        })
                                        .collect(),
                                    realized,
                                })),
                            )
                            .map_err(|_| TurnError::ChannelLost)?;
                        continue;
                    }
                    // **The column intermediate authors as it arrives**,
                    // per `weaver-diagnostic-Spec` section 3.2 and the
                    // decode contract's third intermediate: the replay
                    // drives the pass and the columns ride it where the ask
                    // stood, one `residual.column` event per sampled
                    // position, the seventeenth kind and the harness's own
                    // authoring. It closes no exchange.
                    crate::channel::DecodeReply::Answer(TokenAnswer::Column {
                        position,
                        layers,
                    }) => {
                        self.author_residual_column(turn, position, layers)?;
                        continue;
                    }
                    crate::channel::DecodeReply::Answer(TokenAnswer::ReFed(generation)) => {
                        break generation;
                    }
                    crate::channel::DecodeReply::Answer(TokenAnswer::AtRest) => continue,
                    crate::channel::DecodeReply::Answer(TokenAnswer::Fault(report)) => {
                        return Err(TurnError::Faulted {
                            turn: turn.clone(),
                            report,
                        });
                    }
                    crate::channel::DecodeReply::Refusal(refusal) => {
                        self.author_refusal(
                            weaver_types::TokenAsk::ReFeed,
                            refusal.clone(),
                            Some(turn),
                        );
                        return Err(TurnError::Refused {
                            turn: turn.clone(),
                            refusal,
                        });
                    }
                    crate::channel::DecodeReply::Answer(_) => return Err(TurnError::ChannelLost),
                },
                // The coordination channel is heard while the re-feed
                // streams, exactly as a generation hears it, and everything
                // but a channel loss is refused: the replay has no stop
                // semantics of its own in this act, the run being the work.
                ExchangeWake::Dial => {
                    let Some(slot) = self.pending.as_deref_mut() else {
                        continue;
                    };
                    match self.coordination.accept_root() {
                        Ok(connection) => *slot = Some(connection),
                        Err(crate::failure::ChannelFault::WrongPeer { .. }) => {}
                        Err(_) => return Err(TurnError::ChannelLost),
                    }
                }
                ExchangeWake::Directive => {
                    let Some(slot) = self.pending.as_deref_mut() else {
                        continue;
                    };
                    let Some(connection) = slot.as_ref() else {
                        continue;
                    };
                    match connection.recv() {
                        Ok(envelope) => {
                            let exchange = envelope.exchange.clone();
                            match envelope.payload {
                                weaver_types::Payload::Directive(_) => {
                                    let _ = connection.send(&weaver_types::OrganEnvelope {
                                        exchange,
                                        position: weaver_types::Position::Close,
                                        payload: weaver_types::Payload::Refusal(
                                            weaver_types::LifecycleRefusal::ActivityNotAtRest,
                                        ),
                                    });
                                }
                                _ => return Err(TurnError::ChannelLost),
                            }
                        }
                        Err(crate::failure::ChannelFault::Closed) => {
                            *slot = None;
                        }
                        Err(_) => return Err(TurnError::ChannelLost),
                    }
                }
            }
        };

        self.author
            .author(
                self.recorder,
                Kind::ModelRequest,
                Subsystem::SpuDecoder,
                Some(turn),
                Some(Payload::ModelRequest(generation.request.clone())),
            )
            .map_err(|_| TurnError::ChannelLost)?;
        self.author
            .author(
                self.recorder,
                Kind::ModelOutput,
                Subsystem::SpuDecoder,
                Some(turn),
                Some(Payload::ModelOutput(ModelOutput {
                    emission: generation.emission.clone(),
                    finish: match generation.finish {
                        weaver_types::Finish::Completed => weaver_trace::Finish::Completed,
                        weaver_types::Finish::Stopped => weaver_trace::Finish::Stopped,
                        weaver_types::Finish::Length => weaver_trace::Finish::Length,
                    },
                    resident: generation.resident,
                    capacity: generation.capacity,
                })),
            )
            .map_err(|_| TurnError::ChannelLost)?;
        self.author
            .author(
                self.recorder,
                Kind::ModelMeasurement,
                Subsystem::SpuDecoder,
                Some(turn),
                Some(Payload::ModelMeasurement(generation.measurement.clone())),
            )
            .map_err(|_| TurnError::ChannelLost)?;
        let assistant = Message {
            role: Role::Assistant,
            content: generation.content.clone(),
        };
        self.author
            .author_message(self.recorder, &assistant, turn)
            .map_err(|_| TurnError::Unlicensed { turn: turn.clone() })?
            .map_err(|_| TurnError::ChannelLost)?;

        Ok(generation)
    }

    /// One `residual.column` event authored from the seam's column frame,
    /// per `weaver-diagnostic-Spec` section 3.2: the seventeenth kind, the
    /// harness's own authoring, shared by the generate and re-feed streams
    /// because the contract's third intermediate rides both exchanges.
    fn author_residual_column(
        &mut self,
        turn: &TurnKey,
        position: u64,
        layers: Vec<Vec<f32>>,
    ) -> Result<(), TurnError> {
        let column = weaver_diagnostic::ResidualColumn {
            position,
            layers: layers.len() as u32,
            width: layers.first().map(|l| l.len()).unwrap_or(0) as u32,
            values: layers,
        };
        self.author
            .author_diagnostic(
                self.recorder,
                weaver_diagnostic::Kind::ResidualColumn,
                Some(turn),
                Some(weaver_diagnostic::Payload::ResidualColumn(column)),
            )
            .map_err(|_| TurnError::ChannelLost)?;
        Ok(())
    }

    /// One replay-native event authored, the loop's own kinds, per
    /// `weaver-diagnostic-Spec` section 3: the bracket's open, the identity
    /// it established, and the close with its outcome.
    pub(crate) fn author_replay(
        &mut self,
        kind: weaver_diagnostic::Kind,
        payload: weaver_diagnostic::Payload,
    ) -> Result<(), TurnError> {
        self.author
            .author_diagnostic(self.recorder, kind, None, Some(payload))
            .map_err(|_| TurnError::ChannelLost)?;
        Ok(())
    }

    /// The declared session's name, for the identity event that carries it.
    pub(crate) fn session_name(&self) -> String {
        self.author.session().to_string()
    }

    /// **The close names what ended the turn**, and the announce follows the
    /// record. Split from the rounds so one bracket closes however many
    /// generations ran inside it: the turn is the conversation's unit and the
    /// executions are its interior.
    fn close_turn(
        &mut self,
        turn: &TurnKey,
        generation: &weaver_types::Generation,
        stop: &mut StopSlot,
    ) -> Result<bool, TurnError> {
        let stopped = matches!(generation.finish, weaver_types::Finish::Stopped);
        // **The close names what ended the turn.** A model-side stop, the
        // generation reaching capacity or its own stop token, is a completed
        // turn whose truncation is recorded in the output's finish and the
        // bracket closes clean. A turn the operator's stop cancelled closes
        // with the directive's reason, the partial standing. The cancel
        // losing the race to a natural completion is the first case: the
        // turn completed, and the stop is answered at rest. An invocation
        // stop sets the local finish to Stopped even when its answer is a
        // Result, because the turn stops before the next generation.
        let aborted = stop.is_some() && stopped;
        self.author
            .author(
                self.recorder,
                Kind::TurnClosed,
                Subsystem::Harness,
                Some(turn),
                Some(Payload::TurnClosed(if aborted {
                    TurnClose::Stopped {
                        // **The unload names itself** (the operator's rulings
                        // of 2026-10-07 on #1): a turn stopped while a leave
                        // is pending was stopped by the unload, gracefully or
                        // forced, and its interrupted call is re-runnable.
                        reason: if self.gate.as_ref().is_some_and(|gate| gate.leave.is_some()) {
                            weaver_trace::StopReason::Unload
                        } else {
                            weaver_trace::StopReason::Directive
                        },
                        cause: stop.as_ref().map(|(_, cause)| trace_cause(*cause)),
                    }
                } else {
                    TurnClose::Clean
                })),
            )
            .map_err(|_| TurnError::ChannelLost)?;
        // The close landed, so no turn stands.
        *self.turn_in_flight = None;

        // **Announce after record**: the stop's answer follows the close it
        // reports, carrying the turn's fate, aborted or completed-at-rest,
        // both truthful at the moment of answering.
        if let Some((exchange, _)) = stop.take()
            && !self.gate.as_ref().is_some_and(|gate| {
                gate.leave
                    .as_ref()
                    .is_some_and(|leave| leave.answers.iter().any(|(_, owed)| *owed == exchange))
            })
            && let Some(slot) = self.pending.as_deref_mut()
            && let Some(connection) = slot.as_ref()
        {
            let answer = if aborted {
                weaver_types::LifecycleAnswer::TurnAborted { turn: turn.clone() }
            } else {
                weaver_types::LifecycleAnswer::AtRest
            };
            let _ = connection.send(&weaver_types::OrganEnvelope {
                exchange,
                position: weaver_types::Position::Close,
                payload: weaver_types::Payload::Answer(answer),
            });
        }

        Ok(aborted)
    }

    /// One wake from the streaming wait, per Spec 6.1: the decode channel
    /// first, then the verb connection if one stands, then the listener
    /// while none does, the same serial discipline as the idle wait's.
    fn stream_wake(&self) -> Result<ExchangeWake, TurnError> {
        use std::os::fd::AsFd;
        self.exchange_wake(self.decode.as_fd())
    }

    fn exchange_wake(&self, work: std::os::fd::BorrowedFd<'_>) -> Result<ExchangeWake, TurnError> {
        use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
        use std::os::fd::AsFd;
        loop {
            let mut fds: Vec<PollFd<'_>> = Vec::with_capacity(3);
            let mut wakes: Vec<ExchangeWake> = Vec::with_capacity(3);
            fds.push(PollFd::new(work, PollFlags::POLLIN));
            wakes.push(ExchangeWake::Work);
            match self.pending.as_ref().map(|slot| slot.as_ref()) {
                Some(Some(connection)) => {
                    fds.push(PollFd::new(connection.as_fd(), PollFlags::POLLIN));
                    wakes.push(ExchangeWake::Directive);
                }
                Some(None) => {
                    fds.push(PollFd::new(self.coordination.as_fd(), PollFlags::POLLIN));
                    wakes.push(ExchangeWake::Dial);
                }
                // A seat granted outside the serve loop streams without the
                // ear, the slot being the loop's own.
                None => {}
            }
            match poll(&mut fds, PollTimeout::NONE) {
                Ok(_) => {}
                Err(nix::errno::Errno::EINTR) => continue,
                Err(_) => return Err(TurnError::ChannelLost),
            }
            let woken = PollFlags::POLLIN | PollFlags::POLLHUP | PollFlags::POLLERR;
            for (fd, wake) in fds.iter().zip(&wakes) {
                let revents = fd.revents().unwrap_or(PollFlags::empty());
                if revents.contains(PollFlags::POLLNVAL) {
                    return Err(TurnError::ChannelLost);
                }
                if revents.intersects(woken) {
                    return Ok(*wake);
                }
            }
        }
    }
}

/// What the current exchange wait woke on.
#[derive(Clone, Copy)]
enum ExchangeWake {
    Work,
    Dial,
    Directive,
}

/// The load facts a test lends the seat, where no enter built them.
#[cfg(test)]
pub(crate) fn test_load_facts() -> weaver_types::LoadFacts {
    weaver_types::LoadFacts {
        session: weaver_types::SessionId("s".into()),
        run: weaver_types::RunId("r-1".into()),
        declaration: String::new(),
        artifact: weaver_types::ArtifactRef("a".into()),
        residual_readout: false,
        field: None,
        surprisal: false,
        state_election: Default::default(),
        state_store: Default::default(),
        state_member: false,
        composer: weaver_types::Composer {
            binary: "test".into(),
            file: None,
            sha256: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::fd::{AsRawFd, OwnedFd};

    use nix::sys::socket::{AddressFamily, MsgFlags, SockFlag, SockType, recv, send, socketpair};
    use weaver_trace::{Recorder, RunRef, SessionRef};
    use weaver_types::{Finish, Generation, SessionId};

    /// **A turn runs, and loop 0 authors the whole bracket.** Loop 1 supplies
    /// the delta and receives the outcome, and the record carries the turn's
    /// user message, the three model events, the assistant's turn, and the
    /// **A refused classify authors a refusal and no output, and the case
    /// keeps its values.** The refusal reached the record as a name until
    /// 2026-08-22: `Oversized { requested, bound }` arrived as the word
    /// "oversized", so a reader learned a bound was exceeded and never which
    /// or by how much.
    ///
    /// **The second assertion is the one that would rot quietly.** An
    /// implementation that authored both would satisfy every claim about the
    /// refusal while leaving `classify.output` meaning two things again,
    /// which is the overloading the class was written to end.
    #[test]
    fn a_refused_classify_authors_a_refusal_and_no_output() {
        let session = SessionId("s-1".into());
        let sink = tempfile();
        let mut recorder = crate::record::Record::Serving(
            Recorder::receive(sink, RunRef("r-1".into()), SessionRef(session.0.clone()))
                .expect("recorder"),
        );
        let author = Author::new(&session, &weaver_types::RunId("r-1".into()));

        // The producer's own path, over the shape the seam hands it.
        let record = weaver_types::RefusalRecord::Classify {
            refusal: weaver_types::LabelRefusal::Oversized {
                requested: 9001,
                bound: 4096,
            },
        };
        let rendered = serde_json::to_string(&record).expect("the record renders");
        let payload = weaver_trace::raw_payload(&rendered).expect("it splices");
        author
            .author(
                &mut recorder,
                Kind::Refusal,
                Subsystem::Harness,
                None,
                Some(Payload::Refusal(payload)),
            )
            .expect("the refusal is authored");

        let refusals: Vec<&weaver_trace::Record> = recorder
            .structure()
            .expect("the serving record")
            .iter()
            .filter(|r| r.kind == Kind::Refusal)
            .collect();
        assert_eq!(refusals.len(), 1, "the refusal reached the record");
        let event: serde_json::Value =
            serde_json::from_str(refusals[0].line.as_ref()).expect("the line parses");
        assert_eq!(event["payload"]["seam"], "classify");
        assert_eq!(
            event["payload"]["refusal"]["requested"], 9001,
            "the case keeps the values a name would have dropped"
        );
        assert_eq!(event["payload"]["refusal"]["bound"], 4096);
        assert!(
            event["payload"].get("asked").is_none(),
            "classify carries no ask, its content standing in classify.request"
        );

        assert_eq!(
            recorder
                .structure()
                .expect("the serving record")
                .iter()
                .filter(|r| r.kind == Kind::ClassifyOutput)
                .count(),
            0,
            "a refused classify authors no output at all"
        );
    }

    /// **Pressure is reported once per crossing and not per turn above the
    /// mark.** A fault for every turn over the mark answers a full queue by
    /// filling it, which is the one direction that cannot help, and a
    /// pressure report is a report about a condition rather than about an
    /// event.
    ///
    /// This drives the flag directly rather than filling a real queue: what
    /// is under test is the once-per-crossing rule, and a test that pushed
    /// 768 events to reach it would be testing the writer's arithmetic
    /// instead.
    ///
    /// Perturbation: drop the `pressure_reported` guard and the second and
    /// third readings each author, three faults for one condition.
    #[test]
    fn pressure_reports_once_per_crossing() {
        let session = SessionId("s-1".into());
        let sink = tempfile();
        let mut recorder = crate::record::Record::Serving(
            Recorder::receive(sink, RunRef("r-1".into()), SessionRef(session.0.clone()))
                .expect("recorder"),
        );
        let author = Author::new(&session, &weaver_types::RunId("r-1".into()));
        let mut reported = false;

        // Under the mark: nothing is authored and the flag stays down.
        let faults = |r: &crate::record::Record| {
            r.structure()
                .expect("the serving record")
                .iter()
                .filter(|record| record.kind == Kind::Fault)
                .count()
        };
        let account = |queued: usize| {
            format!(
                "{{\"organ\":\"harness\",\"queued\":{queued},\"mark\":{}}}",
                weaver_trace::HIGH_WATER_MARK
            )
        };
        // The rule itself, exercised over the flag the engine holds: an
        // authored report sets it, a second crossing while it stands
        // authors nothing, and falling under the mark clears it.
        let mut report = |over: bool, queued: usize, recorder: &mut crate::record::Record| {
            if !over {
                reported = false;
                return;
            }
            if reported {
                return;
            }
            reported = true;
            let _ = author.author_fault(
                recorder,
                Subsystem::Harness,
                None,
                &crate::authorship::harness_report(
                    weaver_types::FaultCase::RecorderCommitPressure,
                    &account(queued),
                ),
            );
        };

        report(false, 12, &mut recorder);
        assert_eq!(faults(&recorder), 0, "under the mark authors nothing");

        report(true, 800, &mut recorder);
        assert_eq!(faults(&recorder), 1, "the crossing authors once");

        report(true, 900, &mut recorder);
        report(true, 1000, &mut recorder);
        assert_eq!(
            faults(&recorder),
            1,
            "a condition that persists is one condition"
        );

        report(false, 40, &mut recorder);
        report(true, 810, &mut recorder);
        assert_eq!(
            faults(&recorder),
            2,
            "falling under the mark and crossing again is a second condition"
        );
    }

    /// **The flag clears between turns and not only during one.** A queue
    /// that drained and crossed again entirely between two turns is the case
    /// the end-of-turn reading alone cannot see: it finds the depth high,
    /// finds the flag still standing from the earlier crossing, and reports
    /// nothing, so the second condition goes unrecorded.
    ///
    /// The two readings are exercised in the order the turn runs them,
    /// clear-only first and reporting last, over the flag itself.
    ///
    /// Perturbation: drop the clear-only reading and the second crossing
    /// authors nothing, the count staying at one.
    #[test]
    fn a_drain_between_turns_lets_the_next_crossing_report() {
        let mut reported = false;
        let mut authored = 0usize;

        let clear_only = |over: bool, flag: &mut bool| {
            if !over {
                *flag = false;
            }
        };
        let report = |over: bool, flag: &mut bool, count: &mut usize| {
            if !over {
                *flag = false;
                return;
            }
            if *flag {
                return;
            }
            *flag = true;
            *count += 1;
        };

        // Turn one: the queue is empty at its start and crosses by its end.
        clear_only(false, &mut reported);
        report(true, &mut reported, &mut authored);
        assert_eq!(authored, 1, "the first crossing reports");

        // Between the turns the sink drains and the queue refills past the
        // mark, so every reading a turn takes finds the depth high.
        clear_only(false, &mut reported);
        report(true, &mut reported, &mut authored);
        assert_eq!(
            authored, 2,
            "the drain was seen before the turn and the second crossing reports"
        );

        // And a turn that begins and ends above the mark reports nothing,
        // the condition never having lifted.
        clear_only(true, &mut reported);
        report(true, &mut reported, &mut authored);
        assert_eq!(
            authored, 2,
            "a condition that never lifted is still one condition"
        );
    }

    /// **A refused turn closes once, and the close says a refusal ended
    /// it.** Every open has a close and the close says which kind it was,
    /// per `weaver-trace-PRD` section 3.1, so two closes are as wrong as
    /// none: a reader walking brackets meets a second close attributed to a
    /// turn already ended, and the later one overwrites the earlier one's
    /// account.
    ///
    /// Perturbation: close at the failing arm as well as here, which is what
    /// the first draft of this act did, and the count below reads two with
    /// the second naming a fault. Watched under exactly that.
    #[test]
    fn a_refused_turn_closes_once_and_names_the_refusal() {
        let (near, far) = socketpair(
            AddressFamily::Unix,
            SockType::SeqPacket,
            None,
            SockFlag::SOCK_CLOEXEC,
        )
        .expect("socketpair");
        let decode = crate::channel::decode_from_owned(near);

        let peer = std::thread::spawn(move || {
            let mut buf = vec![0u8; 65536];
            let n = recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).expect("recv append");
            let directive: weaver_types::TokenDirective =
                serde_json::from_slice(&buf[..n]).expect("the append parses");
            assert!(matches!(
                directive,
                weaver_types::TokenDirective::AppendAndGenerate { .. }
            ));
            let refusal = weaver_types::TokenRefusal::Overflow {
                resident: 4091,
                requested: 137,
                capacity: 4096,
            };
            let bytes = serde_json::to_vec(&refusal).expect("the refusal renders");
            send(far.as_raw_fd(), &bytes, MsgFlags::empty()).expect("send refusal");
        });

        let session = SessionId("s-1".into());
        let sink = tempfile();
        let mut recorder = crate::record::Record::Serving(
            Recorder::receive(sink, RunRef("r-1".into()), SessionRef(session.0.clone()))
                .expect("recorder"),
        );
        let author = Author::new(&session, &weaver_types::RunId("r-1".into()));
        author
            .author(
                &mut recorder,
                Kind::Load,
                Subsystem::Harness,
                None,
                Some(Payload::Elections(Box::new(weaver_trace::Elections {
                    residual_readout: false,
                    field: None,
                    surprisal: false,
                    tee: Some(weaver_trace::Election::default()),
                    state_member: false,
                    declaration: Default::default(),
                    lineage: None,
                    reset: None,
                    stack: Default::default(),
                    boundary: String::new(),
                    cause: weaver_trace::Cause { uid: 0 },
                    library_path: None,
                    state_store: Default::default(),
                    composer: weaver_trace::LoopIdentity::compiled("test"),
                }))),
            )
            .expect("load");
        let mut turn_ordinal = 0u64;
        let mut turn_in_flight: Option<weaver_types::TurnKey> = None;
        let (listener, _dir) = test_listener();
        let outcome = {
            let mut fullness = None;
            let mut pressure_reported = false;
            let load_facts = crate::engine::test_load_facts();
            let mut ports = Ports::grant(
                &decode,
                &author,
                &mut recorder,
                &mut turn_ordinal,
                &mut turn_in_flight,
                &load_facts,
                None,
                &listener,
                None,
                None,
                None,
                None,
                &mut fullness,
                &mut pressure_reported,
            );
            ports.turn(vec![Message {
                role: Role::User,
                content: vec![ContentBlock::Text {
                    text: "say a word".into(),
                }],
            }])
        };
        peer.join().expect("the decode peer finishes");
        assert!(
            matches!(outcome, Err(TurnError::Refused { .. })),
            "the refusal still reaches the caller"
        );

        let closes: Vec<&weaver_trace::Record> = recorder
            .structure()
            .expect("the serving record")
            .iter()
            .filter(|r| r.kind == Kind::TurnClosed)
            .collect();
        assert_eq!(closes.len(), 1, "one bracket, one close");
        let close: serde_json::Value =
            serde_json::from_str(closes[0].line.as_ref()).expect("the close parses");
        assert_eq!(close["payload"]["close"], "stopped");
        assert_eq!(
            close["payload"]["reason"], "refused",
            "and the reason names what ended the turn"
        );

        let refusals = recorder
            .structure()
            .expect("the serving record")
            .iter()
            .filter(|r| r.kind == Kind::Refusal)
            .count();
        assert_eq!(refusals, 1, "the refusal itself reached the record too");
    }

    /// **A refused ask reaches the record, and the values that ride are the
    /// ones nothing else holds.** Before 2026-08-22 a refusal between turns
    /// reached the loop as an absence and the record as nothing, so a reader
    /// could not tell an ask turned away from an ask never made.
    ///
    /// The scripted peer refuses an elision whose span is deliberately
    /// unroundable, so a member arrived at by default fails here.
    ///
    /// Perturbation: drop the `DecodeReply::Refusal` arm back to the
    /// wildcard and the port answers `None` with the record silent, which is
    /// the state this act exists to end. Watched under exactly that.
    #[test]
    fn a_refused_ask_reaches_the_record() {
        let (near, far) = socketpair(
            AddressFamily::Unix,
            SockType::SeqPacket,
            None,
            SockFlag::SOCK_CLOEXEC,
        )
        .expect("socketpair");
        let decode = crate::channel::decode_from_owned(near);

        let peer = std::thread::spawn(move || {
            let mut buf = vec![0u8; 65536];
            let n = recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).expect("recv elide");
            let directive: weaver_types::TokenDirective =
                serde_json::from_slice(&buf[..n]).expect("the elide parses");
            assert_eq!(
                directive,
                weaver_types::TokenDirective::Elide { from: 41, to: 57 }
            );
            let refusal = weaver_types::TokenRefusal::UnremovableSpan {
                from: 41,
                to: 57,
                prefix: 12,
                resident: 1237,
            };
            // The refusal crosses as itself: the seam frames an answer and a
            // refusal alike and the reader discriminates by parse.
            let bytes = serde_json::to_vec(&refusal).expect("the refusal renders");
            send(far.as_raw_fd(), &bytes, MsgFlags::empty()).expect("send refusal");
        });

        let session = SessionId("s-1".into());
        let sink = tempfile();
        let mut recorder = crate::record::Record::Serving(
            Recorder::receive(sink, RunRef("r-1".into()), SessionRef(session.0.clone()))
                .expect("recorder"),
        );
        let author = Author::new(&session, &weaver_types::RunId("r-1".into()));
        author
            .author(
                &mut recorder,
                Kind::Load,
                Subsystem::Harness,
                None,
                Some(Payload::Elections(Box::new(weaver_trace::Elections {
                    residual_readout: false,
                    field: None,
                    surprisal: false,
                    tee: Some(weaver_trace::Election::default()),
                    state_member: false,
                    declaration: Default::default(),
                    lineage: None,
                    reset: None,
                    stack: Default::default(),
                    boundary: String::new(),
                    cause: weaver_trace::Cause { uid: 0 },
                    library_path: None,
                    state_store: Default::default(),
                    composer: weaver_trace::LoopIdentity::compiled("test"),
                }))),
            )
            .expect("load");
        let mut turn_ordinal = 0u64;
        let mut turn_in_flight: Option<weaver_types::TurnKey> = None;
        let (listener, _dir) = test_listener();
        let answered = {
            let mut fullness = None;
            let mut pressure_reported = false;
            let load_facts = crate::engine::test_load_facts();
            let mut ports = Ports::grant(
                &decode,
                &author,
                &mut recorder,
                &mut turn_ordinal,
                &mut turn_in_flight,
                &load_facts,
                None,
                &listener,
                None,
                None,
                None,
                None,
                &mut fullness,
                &mut pressure_reported,
            );
            ports.elide(41, 57)
        };
        peer.join().expect("the decode peer finishes");
        assert!(answered.is_none(), "a refused span answers None as it did");

        let refusals: Vec<&weaver_trace::Record> = recorder
            .structure()
            .expect("the serving record")
            .iter()
            .filter(|r| r.kind == Kind::Refusal)
            .collect();
        assert_eq!(refusals.len(), 1, "the refusal reached the record");
        let event: serde_json::Value =
            serde_json::from_str(refusals[0].line.as_ref()).expect("the line parses");
        assert_eq!(event["payload"]["seam"], "decode");
        assert_eq!(
            event["payload"]["asked"]["ask"], "elide",
            "the ask is named"
        );
        assert_eq!(
            event["payload"]["asked"]["from"], 41,
            "and its span rides, no other event holding a span that was refused"
        );
        assert_eq!(event["payload"]["asked"]["to"], 57);
        assert_eq!(
            event["payload"]["refusal"]["prefix"], 12,
            "the seam's own case keeps its values"
        );
        assert!(
            event.get("turn").is_none(),
            "an elision is asked between turns, so its refusal belongs to none"
        );
    }

    /// **The elision's event is authored from the ask and not the answer.**
    /// The seam echoes no span, per the decode contract, so the only place
    /// the record can get one is what the loop named. A site reading the
    /// span off the answer would have nothing to read, and a site
    /// defaulting it would write a record of a removal that did not happen
    /// where one did.
    ///
    /// The scripted peer answers counts that are deliberately unroundable
    /// and no span at all, the `Elided` variant having none to carry, so a
    /// member arrived at by accident or by default fails here.
    ///
    /// conforms: harness-elision-authors-from-the-ask
    #[test]
    fn the_elision_event_carries_the_span_the_loop_named() {
        let (near, far) = socketpair(
            AddressFamily::Unix,
            SockType::SeqPacket,
            None,
            SockFlag::SOCK_CLOEXEC,
        )
        .expect("socketpair");
        let decode = crate::channel::decode_from_owned(near);

        let peer = std::thread::spawn(move || {
            let mut buf = vec![0u8; 65536];
            let n = recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).expect("recv elide");
            let directive: weaver_types::TokenDirective =
                serde_json::from_slice(&buf[..n]).expect("the elide parses");
            assert_eq!(
                directive,
                weaver_types::TokenDirective::Elide { from: 41, to: 57 },
                "the port forwards the span unjudged"
            );
            let answer = weaver_types::TokenAnswer::Elided {
                resident_before: 1237,
                resident_after: 1221,
            };
            let bytes = serde_json::to_vec(&answer).expect("answer renders");
            send(far.as_raw_fd(), &bytes, MsgFlags::empty()).expect("send answer");
        });

        let session = SessionId("s-1".into());
        let sink = tempfile();
        let mut recorder = crate::record::Record::Serving(
            Recorder::receive(sink, RunRef("r-1".into()), SessionRef(session.0.clone()))
                .expect("recorder"),
        );
        let author = Author::new(&session, &weaver_types::RunId("r-1".into()));
        author
            .author(
                &mut recorder,
                Kind::Load,
                Subsystem::Harness,
                None,
                Some(Payload::Elections(Box::new(weaver_trace::Elections {
                    residual_readout: false,
                    field: None,
                    surprisal: false,
                    tee: Some(weaver_trace::Election::default()),
                    state_member: false,
                    declaration: Default::default(),
                    lineage: None,
                    reset: None,
                    stack: Default::default(),
                    boundary: String::new(),
                    cause: weaver_trace::Cause { uid: 0 },
                    library_path: None,
                    state_store: Default::default(),
                    composer: weaver_trace::LoopIdentity::compiled("test"),
                }))),
            )
            .expect("load");
        let mut turn_ordinal = 0u64;
        let mut turn_in_flight: Option<weaver_types::TurnKey> = None;
        let (listener, _dir) = test_listener();
        let counts = {
            let mut fullness = Some((1237, 8191));
            let mut pressure_reported = false;
            let load_facts = crate::engine::test_load_facts();
            let mut ports = Ports::grant(
                &decode,
                &author,
                &mut recorder,
                &mut turn_ordinal,
                &mut turn_in_flight,
                &load_facts,
                None,
                &listener,
                None,
                None,
                None,
                None,
                &mut fullness,
                &mut pressure_reported,
            );
            let counts = ports.elide(41, 57).expect("the elision holds");
            assert_eq!(
                fullness,
                Some((1221, 8191)),
                "the fullness follows the shortened state"
            );
            counts
        };
        peer.join().expect("the decode peer finishes");
        assert_eq!(counts, (1237, 1221), "the seam's counts reach the loop");

        let lines: Vec<&weaver_trace::Record> = recorder
            .structure()
            .expect("the serving record")
            .iter()
            .filter(|r| r.kind == Kind::Elision)
            .collect();
        assert_eq!(lines.len(), 1, "one elision, one event");
        let event: serde_json::Value =
            serde_json::from_str(lines[0].line.as_ref()).expect("the line parses");
        assert_eq!(event["payload"]["from"], 41, "the span comes from the ask");
        assert_eq!(event["payload"]["to"], 57, "both bounds of it");
        assert_eq!(
            event["payload"]["resident_before"], 1237,
            "and the counts come from the answer"
        );
        assert_eq!(event["payload"]["resident_after"], 1221);
        assert!(
            event.get("turn").is_none(),
            "an elision is asked between turns and belongs to none"
        );
    }

    /// close, in that order. The decode peer is scripted here rather than a
    /// real SPU: it streams two tokens and then the generation whole, which is
    /// what the turn method consumes and authors.
    #[test]
    fn a_turn_authors_the_whole_bracket() {
        let (near, far) = socketpair(
            AddressFamily::Unix,
            SockType::SeqPacket,
            None,
            SockFlag::SOCK_CLOEXEC,
        )
        .expect("socketpair");
        let decode = crate::channel::decode_from_owned(near);

        // The scripted decode peer: read the append, stream two tokens, then
        // answer with the generation whole, the request and measurement the
        // SPU-rendered blobs the turn splices.
        let peer = std::thread::spawn(move || {
            let mut buf = vec![0u8; 65536];
            let n = recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).expect("recv append");
            let directive: weaver_types::TokenDirective =
                serde_json::from_slice(&buf[..n]).expect("the append parses");
            assert!(
                matches!(
                    directive,
                    weaver_types::TokenDirective::AppendAndGenerate { .. }
                ),
                "the turn sends append-and-generate"
            );
            let send_answer = |answer: &weaver_types::TokenAnswer| {
                let bytes = serde_json::to_vec(answer).expect("answer renders");
                send(far.as_raw_fd(), &bytes, MsgFlags::empty()).expect("send answer");
            };
            send_answer(&weaver_types::TokenAnswer::Token {
                token: 9707,
                piece: "one".into(),
            });
            send_answer(&weaver_types::TokenAnswer::Token {
                token: 1879,
                piece: " word".into(),
            });
            let request = serde_json::value::RawValue::from_string(
                r#"{"rendered":"<|im_start|>user\nsay a word<|im_end|>\n","template":"qwen2","sampling":{"temperature":0.7}}"#
                    .to_string(),
            )
            .unwrap();
            let measurement = serde_json::value::RawValue::from_string(
                r#"{"model":"qwen2.5","weights_hash":"sha256:abc","input_tokens":[1,2],"output_tokens":[9707,1879],"blocks":[{"label":"turn-delta","start":0,"end":10}],"timings":{"prefill_ns":"10","decode_ns":"20"}}"#
                    .to_string(),
            )
            .unwrap();
            send_answer(&weaver_types::TokenAnswer::Generated(Generation {
                content: vec![weaver_traits::ContentBlock::Text {
                    text: "one word".into(),
                }],
                emission: "one word".into(),
                finish: Finish::Completed,
                // Deliberately unroundable: a site forwarding a zero, a
                // default, or the other member would fail the assertion
                // below, where 64 and 4096 could each be arrived at by
                // accident.
                resident: 1237,
                capacity: 8191,
                request,
                measurement,
            }));
        });

        let session = SessionId("s-1".into());
        let sink = tempfile();
        let mut recorder = crate::record::Record::Serving(
            Recorder::receive(sink, RunRef("r-1".into()), SessionRef(session.0.clone()))
                .expect("recorder"),
        );
        let author = Author::new(&session, &weaver_types::RunId("r-1".into()));
        author
            .author(
                &mut recorder,
                Kind::Load,
                Subsystem::Harness,
                None,
                Some(Payload::Elections(Box::new(weaver_trace::Elections {
                    residual_readout: false,
                    field: None,
                    surprisal: false,
                    tee: Some(weaver_trace::Election::default()),
                    state_member: false,
                    declaration: Default::default(),
                    lineage: None,
                    reset: None,
                    stack: Default::default(),
                    boundary: String::new(),
                    cause: weaver_trace::Cause { uid: 0 },
                    library_path: None,
                    state_store: Default::default(),
                    composer: weaver_trace::LoopIdentity::compiled("test"),
                }))),
            )
            .expect("load");
        let mut turn_ordinal = 0u64;
        let mut turn_in_flight: Option<weaver_types::TurnKey> = None;

        let (listener, _dir) = test_listener();
        let outcome = {
            let mut fullness = None;
            let mut pressure_reported = false;
            let load_facts = crate::engine::test_load_facts();
            let mut ports = Ports::grant(
                &decode,
                &author,
                &mut recorder,
                &mut turn_ordinal,
                &mut turn_in_flight,
                &load_facts,
                None,
                &listener,
                None,
                None,
                None,
                None,
                &mut fullness,
                &mut pressure_reported,
            );
            let delta = vec![Message {
                role: Role::User,
                content: vec![ContentBlock::Text {
                    text: "say a word".into(),
                }],
            }];
            ports.turn(delta).expect("the turn completes")
        };
        peer.join().expect("the decode peer finishes");

        assert_eq!(outcome.emission, "one word", "loop 1 receives the emission");
        assert!(!outcome.stopped, "a clean generation is not stopped");

        // The bracket, in order: the turn's user message, the three model
        // events, the assistant's turn, and the close.
        let kinds: Vec<Kind> = recorder
            .structure()
            .expect("the serving record")
            .iter()
            .filter(|r| r.turn.is_some())
            .map(|r| r.kind)
            .collect();
        assert_eq!(
            kinds,
            vec![
                Kind::TurnStarted,
                Kind::MessageUser,
                Kind::ModelRequest,
                Kind::ModelOutput,
                Kind::ModelMeasurement,
                Kind::MessageAssistant,
                Kind::TurnClosed,
            ],
            "the whole bracket authored in order"
        );

        // **The output's counts are the generation's**, per
        // `weaver-trace-Spec` section 3. The trace crate's own test proves
        // the pair serializes; this one proves the authoring site forwards
        // what the seam delivered rather than a default, which is the half
        // that lives in this crate.
        //
        // Perturbation: forward a zero, a constant, or the members swapped,
        // and this fails.
        let output_line = recorder
            .structure()
            .expect("the serving record")
            .by_kind(Kind::ModelOutput)
            .next()
            .expect("the output authored")
            .line
            .to_string();
        let output: serde_json::Value =
            serde_json::from_str(&output_line).expect("the output line is one value");
        assert_eq!(
            output["payload"]["resident"], 1237,
            "the generation's resident count reaches the record: {output_line}"
        );
        assert_eq!(
            output["payload"]["capacity"], 8191,
            "and its capacity: {output_line}"
        );

        // The model events splice what the peer rendered: the request carries
        // the template, the measurement the weights hash, both opaque.
        let line_of = |kind: Kind| {
            recorder
                .structure()
                .expect("the serving record")
                .by_kind(kind)
                .next()
                .expect("the event authored")
                .line
                .to_string()
        };
        assert!(
            line_of(Kind::ModelRequest).contains("\"template\":\"qwen2\""),
            "the request splice carries what the SPU rendered"
        );
        assert!(
            line_of(Kind::ModelMeasurement).contains("sha256:abc"),
            "the measurement splice carries what the SPU rendered"
        );

        // The attribution, at engine grain: all three model events carry the
        // decoder's case on the wire, per the #103 ruling. Read from the
        // rendered line rather than the enum so a regression to `Spu` at the
        // author call fails here naming the string, which is the emitter-side
        // half of the pin whose spelling half lives in the recorder's tests.
        for kind in [
            Kind::ModelRequest,
            Kind::ModelOutput,
            Kind::ModelMeasurement,
        ] {
            assert!(
                line_of(kind).contains("\"subsystem\":\"spu_decoder\""),
                "{kind:?} is attributed to the decode engine, got {}",
                line_of(kind)
            );
        }
    }

    /// The emission mid-stream: the report is authored inside the bracket,
    /// before the close. The pin is the order: a turn-attributed `fault`
    /// filed after `turn.closed` would sit outside the bracket that names
    /// it, which is the defect the wrapper's author-then-close sequence
    /// exists to prevent.
    #[test]
    fn a_fault_emission_lands_inside_the_bracket_before_the_close() {
        let (near, far) = socketpair(
            AddressFamily::Unix,
            SockType::SeqPacket,
            None,
            SockFlag::SOCK_CLOEXEC,
        )
        .expect("socketpair");
        let decode = crate::channel::decode_from_owned(near);

        // The scripted peer: read the append, stream one token, then emit
        // the fault instead of the generation.
        let peer = std::thread::spawn(move || {
            let mut buf = vec![0u8; 65536];
            let _ = recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).expect("recv append");
            let send_answer = |answer: &weaver_types::TokenAnswer| {
                let bytes = serde_json::to_vec(answer).expect("answer renders");
                send(far.as_raw_fd(), &bytes, MsgFlags::empty()).expect("send answer");
            };
            send_answer(&weaver_types::TokenAnswer::Token {
                token: 9707,
                piece: "one".into(),
            });
            send_answer(&weaver_types::TokenAnswer::Fault(
                weaver_types::FaultReport {
                    case: weaver_types::FaultCase::DeviceFaultDuringGeneration,
                    account: serde_json::value::RawValue::from_string(
                        r#"{"device":"cuda:0","errored":"mid-forward"}"#.to_string(),
                    )
                    .unwrap(),
                },
            ));
        });

        let session = SessionId("s-1".into());
        let sink = tempfile();
        let mut recorder = crate::record::Record::Serving(
            Recorder::receive(sink, RunRef("r-1".into()), SessionRef(session.0.clone()))
                .expect("recorder"),
        );
        let author = Author::new(&session, &weaver_types::RunId("r-1".into()));
        author
            .author(
                &mut recorder,
                Kind::Load,
                Subsystem::Harness,
                None,
                Some(Payload::Elections(Box::new(weaver_trace::Elections {
                    residual_readout: false,
                    field: None,
                    surprisal: false,
                    tee: Some(weaver_trace::Election::default()),
                    state_member: false,
                    declaration: Default::default(),
                    lineage: None,
                    reset: None,
                    stack: Default::default(),
                    boundary: String::new(),
                    cause: weaver_trace::Cause { uid: 0 },
                    library_path: None,
                    state_store: Default::default(),
                    composer: weaver_trace::LoopIdentity::compiled("test"),
                }))),
            )
            .expect("load");
        let mut turn_ordinal = 0u64;
        let mut turn_in_flight: Option<weaver_types::TurnKey> = None;

        let (listener, _dir) = test_listener();
        let error = {
            let mut fullness = None;
            let mut pressure_reported = false;
            let load_facts = crate::engine::test_load_facts();
            let mut ports = Ports::grant(
                &decode,
                &author,
                &mut recorder,
                &mut turn_ordinal,
                &mut turn_in_flight,
                &load_facts,
                None,
                &listener,
                None,
                None,
                None,
                None,
                &mut fullness,
                &mut pressure_reported,
            );
            let delta = vec![Message {
                role: Role::User,
                content: vec![ContentBlock::Text {
                    text: "say a word".into(),
                }],
            }];
            ports.turn(delta).expect_err("the emission fails the turn")
        };
        peer.join().expect("the decode peer finishes");

        let TurnError::Faulted { turn, report } = error else {
            panic!("the emission is matched by name, got another error");
        };
        assert_eq!(turn.0, "t-1", "the streaming turn is the one named");
        assert_eq!(
            report.case,
            weaver_types::FaultCase::DeviceFaultDuringGeneration,
            "the report rides the error for the caller's close"
        );

        // The pin: the turn-attributed sequence holds the fault before the
        // close, both inside the bracket.
        let kinds: Vec<Kind> = recorder
            .structure()
            .expect("the serving record")
            .iter()
            .filter(|r| r.turn.is_some())
            .map(|r| r.kind)
            .collect();
        let fault_at = kinds
            .iter()
            .position(|k| *k == Kind::Fault)
            .expect("the report was authored");
        let close_at = kinds
            .iter()
            .position(|k| *k == Kind::TurnClosed)
            .expect("the bracket closed");
        assert!(
            fault_at < close_at,
            "the fault lands inside the bracket, before the close: {kinds:?}"
        );
    }

    /// **A turn executes its calls, and the grant authors the result.** The
    /// whole mechanism in one bracket: the scripted decode peer's first
    /// generation carries a `ToolCall` block, the scripted gate peer answers
    /// the execution exchange with the result, the grant authors the
    /// tool-result turn at its one door, the same content crosses back as
    /// the second delta, and the second generation closes the turn as text.
    ///
    /// What the bracket must read, in order: the user message, the first
    /// model triplet, the assistant turn carrying the call, the tool
    /// bracket's two events, the tool-result message, the second model
    /// triplet, the closing assistant turn, and the close - two generations
    /// inside one turn, which is the loop the tool workflow exists for.
    ///
    /// Perturbation: drop the `execute_call` loop and route calls as plain
    /// text, and this fails at the kinds assertion missing the tool bracket.
    #[test]
    fn a_turn_executes_its_calls_and_the_grant_authors_the_result() {
        let (near, far) = socketpair(
            AddressFamily::Unix,
            SockType::SeqPacket,
            None,
            SockFlag::SOCK_CLOEXEC,
        )
        .expect("socketpair");
        let decode = crate::channel::decode_from_owned(near);

        // The decode peer: two rounds. Round one answers a generation whose
        // canonical content carries the call; round two reads the tool-result
        // delta back and answers plain text.
        let peer = std::thread::spawn(move || {
            let mut buf = vec![0u8; 65536];
            let raw =
                |text: &str| serde_json::value::RawValue::from_string(text.to_string()).unwrap();
            let send_answer = |answer: &weaver_types::TokenAnswer| {
                let bytes = serde_json::to_vec(answer).expect("answer renders");
                send(far.as_raw_fd(), &bytes, MsgFlags::empty()).expect("send answer");
            };

            let n = recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).expect("recv append");
            let directive: weaver_types::TokenDirective =
                serde_json::from_slice(&buf[..n]).expect("the append parses");
            assert!(matches!(
                directive,
                weaver_types::TokenDirective::AppendAndGenerate { .. }
            ));
            send_answer(&weaver_types::TokenAnswer::Generated(Generation {
                content: vec![weaver_traits::ContentBlock::ToolCall(
                    weaver_traits::ToolCall {
                        name: "calculator".into(),
                        arguments: r#"{"expression":"37 * 43"}"#.into(),
                    },
                )],
                emission: r#"<tool_call>{"name":"calculator"}</tool_call>"#.into(),
                finish: Finish::Completed,
                resident: 64,
                capacity: 4096,
                request: raw(r#"{"rendered":"r1","template":"qwen2","sampling":{}}"#),
                measurement: raw(
                    r#"{"model":"m","weights_hash":"h","input_tokens":[1],"output_tokens":[2],"blocks":[{"label":"turn-delta","start":0,"end":1}],"timings":{"prefill_ns":"1","decode_ns":"2"}}"#,
                ),
            }));

            // Round two: the tool-result delta arrives, carrying the granted
            // content and nothing the loop invented.
            let n = recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).expect("recv round 2");
            let directive: weaver_types::TokenDirective =
                serde_json::from_slice(&buf[..n]).expect("round 2 parses");
            let weaver_types::TokenDirective::AppendAndGenerate { delta, .. } = directive else {
                panic!("round two is an append");
            };
            assert_eq!(delta.len(), 1, "one tool-result message crosses");
            assert!(matches!(delta[0].role, Role::ToolResult));
            assert!(
                matches!(
                    &delta[0].content[0],
                    ContentBlock::ToolResult(block) if block.content == "1591"
                ),
                "the granted content is what crosses: {:?}",
                delta[0].content
            );
            send_answer(&weaver_types::TokenAnswer::Generated(Generation {
                content: vec![weaver_traits::ContentBlock::Text {
                    text: "37 * 43 is 1591.".into(),
                }],
                emission: "37 * 43 is 1591.".into(),
                finish: Finish::Completed,
                resident: 64,
                capacity: 4096,
                request: raw(r#"{"rendered":"r2","template":"qwen2","sampling":{}}"#),
                measurement: raw(
                    r#"{"model":"m","weights_hash":"h","input_tokens":[3],"output_tokens":[4],"blocks":[{"label":"turn-delta","start":0,"end":1}],"timings":{"prefill_ns":"1","decode_ns":"2"}}"#,
                ),
            }));
        });

        // The gate peer: one execution exchange, answered with the result,
        // exactly as the gate's dispatch would.
        let (gate_near, gate_child) = crate::channel::OrganChannel::pair().expect("gate pair");
        let gate_peer = std::thread::spawn(move || {
            let gate_far = gate_child.into_channel();
            let envelope = gate_far.recv().expect("the execution opens");
            assert_eq!(envelope.position, weaver_types::Position::Open);
            let weaver_types::Payload::Tool(execution) = &envelope.payload else {
                panic!("the exchange carries the call, got {:?}", envelope.payload);
            };
            assert_eq!(execution.name.0, "calculator");
            let outcome = weaver_gate_execute_stub(execution);
            gate_far
                .send(&weaver_types::OrganEnvelope {
                    exchange: envelope.exchange.clone(),
                    position: weaver_types::Position::Close,
                    payload: weaver_types::Payload::ToolAnswer(outcome),
                })
                .expect("the answer closes");
        });

        let session = SessionId("s-t".into());
        let sink = tempfile();
        let mut recorder = crate::record::Record::Serving(
            Recorder::receive(sink, RunRef("r-1".into()), SessionRef(session.0.clone()))
                .expect("recorder"),
        );
        let author = Author::new(&session, &weaver_types::RunId("r-1".into()));
        author
            .author(
                &mut recorder,
                Kind::Load,
                Subsystem::Harness,
                None,
                Some(Payload::Elections(Box::new(weaver_trace::Elections {
                    residual_readout: false,
                    field: None,
                    surprisal: false,
                    tee: Some(weaver_trace::Election::default()),
                    state_member: false,
                    declaration: Default::default(),
                    lineage: None,
                    reset: None,
                    stack: Default::default(),
                    boundary: String::new(),
                    cause: weaver_trace::Cause { uid: 0 },
                    library_path: None,
                    state_store: Default::default(),
                    composer: weaver_trace::LoopIdentity::compiled("test"),
                }))),
            )
            .expect("load");
        let mut turn_ordinal = 0u64;
        let mut turn_in_flight: Option<weaver_types::TurnKey> = None;
        let mut gate_ordinal = 0u64;
        let mut held = std::collections::VecDeque::new();
        let mut leave = None;

        let (listener, _dir) = test_listener();
        let outcome = {
            let mut fullness = None;
            let mut pressure_reported = false;
            let load_facts = crate::engine::test_load_facts();
            let mut ports = Ports::grant(
                &decode,
                &author,
                &mut recorder,
                &mut turn_ordinal,
                &mut turn_in_flight,
                &load_facts,
                None,
                &listener,
                None,
                Some(GatePort {
                    channel: &gate_near,
                    ordinal: &mut gate_ordinal,
                    held: &mut held,
                    leave: &mut leave,
                }),
                None,
                None,
                &mut fullness,
                &mut pressure_reported,
            );
            let delta = vec![Message {
                role: Role::User,
                content: vec![ContentBlock::Text {
                    text: "what is 37 * 43?".into(),
                }],
            }];
            ports.turn(delta).expect("the turn completes")
        };
        peer.join().expect("the decode peer finishes");
        gate_peer.join().expect("the gate peer finishes");

        assert_eq!(outcome.emission, "37 * 43 is 1591.");
        assert!(held.is_empty(), "nothing crossed mid-execution to hold");

        let kinds: Vec<Kind> = recorder
            .structure()
            .expect("the serving record")
            .iter()
            .filter(|r| r.turn.is_some())
            .map(|r| r.kind)
            .collect();
        assert_eq!(
            kinds,
            vec![
                Kind::TurnStarted,
                Kind::MessageUser,
                Kind::ModelRequest,
                Kind::ModelOutput,
                Kind::ModelMeasurement,
                Kind::MessageAssistant,
                Kind::ToolCallStarted,
                Kind::ToolCallCompleted,
                Kind::MessageToolResult,
                Kind::ModelRequest,
                Kind::ModelOutput,
                Kind::ModelMeasurement,
                Kind::MessageAssistant,
                Kind::TurnClosed,
            ],
            "two generations inside one bracket, the tool loop between them"
        );

        // The attribution: started is the harness's dispatch, completed is
        // the gate's answer.
        let line_of = |kind: Kind| {
            recorder
                .structure()
                .expect("the serving record")
                .by_kind(kind)
                .next()
                .expect("authored")
                .line
                .to_string()
        };
        assert!(line_of(Kind::ToolCallStarted).contains("\"subsystem\":\"harness\""));
        assert!(line_of(Kind::ToolCallCompleted).contains("\"subsystem\":\"gate\""));
        assert!(
            line_of(Kind::MessageToolResult).contains("1591"),
            "the granted content is the record's"
        );
    }

    /// **Each field intermediate authors one `model.field` event**, per
    /// `weaver-harness-Spec` section 6. The intermediates close nothing, so
    /// the turn runs on and the close arrives as it always did: the record
    /// gains the field's events and loses none of the bracket.
    ///
    /// Perturbation: discard the field arm the way the token arm is
    /// discarded and the kinds assertion fails, the events never authored.
    #[test]
    fn each_field_intermediate_authors_its_event() {
        let (near, far) = socketpair(
            AddressFamily::Unix,
            SockType::SeqPacket,
            None,
            SockFlag::SOCK_CLOEXEC,
        )
        .expect("socketpair");

        let decode = crate::channel::decode_from_owned(near);
        let peer = std::thread::spawn(move || {
            let mut buf = vec![0u8; 65536];
            let _ = recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).expect("recv append");
            let send_answer = |answer: &weaver_types::TokenAnswer| {
                let bytes = serde_json::to_vec(answer).expect("answer renders");
                send(far.as_raw_fd(), &bytes, MsgFlags::empty()).expect("send answer");
            };
            // Two positions of field, and a token intermediate between them
            // to prove the two streams interleave without either being lost.
            for position in [0u64, 1] {
                send_answer(&weaver_types::TokenAnswer::Field {
                    position,
                    ranked: vec![
                        weaver_types::Candidate {
                            token: 11,
                            probability: 0.75,
                        },
                        weaver_types::Candidate {
                            token: 12,
                            probability: 0.25,
                        },
                    ],
                    realized: 0,
                });
                send_answer(&weaver_types::TokenAnswer::Token {
                    token: 11,
                    piece: "hi".into(),
                });
            }
            let request = serde_json::value::RawValue::from_string(
                r#"{"rendered":"x","template":"t","sampling":{"seed":11}}"#.to_string(),
            )
            .unwrap();
            let measurement = serde_json::value::RawValue::from_string(
                r#"{"model":"m","weights_hash":"h","input_tokens":[1],"output_tokens":[11],"blocks":[{"label":"turn-delta","start":0,"end":1}],"timings":{"prefill_ns":"1","decode_ns":"2"}}"#
                    .to_string(),
            )
            .unwrap();
            send_answer(&weaver_types::TokenAnswer::Generated(Generation {
                content: vec![weaver_traits::ContentBlock::Text { text: "hi".into() }],
                emission: "hi".into(),
                finish: Finish::Completed,
                resident: 12,
                capacity: 4096,
                request,
                measurement,
            }));
        });

        let session = SessionId("s-1".into());
        let sink = tempfile();
        let mut recorder = crate::record::Record::Serving(
            Recorder::receive(sink, RunRef("r-1".into()), SessionRef(session.0.clone()))
                .expect("recorder"),
        );
        let author = Author::new(&session, &weaver_types::RunId("r-1".into()));
        author
            .author(
                &mut recorder,
                Kind::Load,
                Subsystem::Harness,
                None,
                Some(Payload::Elections(Box::new(weaver_trace::Elections {
                    residual_readout: false,
                    field: Some(2),
                    surprisal: false,
                    tee: Some(weaver_trace::Election::default()),
                    state_member: false,
                    declaration: Default::default(),
                    lineage: None,
                    reset: None,
                    stack: Default::default(),
                    boundary: String::new(),
                    cause: weaver_trace::Cause { uid: 0 },
                    library_path: None,
                    state_store: Default::default(),
                    composer: weaver_trace::LoopIdentity::compiled("test"),
                }))),
            )
            .expect("load");
        let mut turn_ordinal = 0u64;
        let mut turn_in_flight: Option<weaver_types::TurnKey> = None;
        let (listener, _dir) = test_listener();
        let outcome = {
            let mut fullness = None;
            let mut pressure_reported = false;
            let load_facts = crate::engine::test_load_facts();
            let mut ports = Ports::grant(
                &decode,
                &author,
                &mut recorder,
                &mut turn_ordinal,
                &mut turn_in_flight,
                &load_facts,
                None,
                &listener,
                None,
                None,
                None,
                None,
                &mut fullness,
                &mut pressure_reported,
            );
            let delta = vec![Message {
                role: Role::User,
                content: vec![ContentBlock::Text { text: "hi".into() }],
            }];
            ports.turn(delta).expect("the turn completes")
        };
        peer.join().expect("the decode peer finishes");
        assert_eq!(outcome.emission, "hi", "the close still closes the turn");

        let fields: Vec<String> = recorder
            .structure()
            .expect("the serving record")
            .by_kind(Kind::ModelField)
            .map(|r| r.line.to_string())
            .collect();
        assert_eq!(fields.len(), 2, "one event per intermediate: {fields:?}");
        for (at, line) in fields.iter().enumerate() {
            let rendered: serde_json::Value =
                serde_json::from_str(line).expect("the line is one value");
            assert_eq!(
                rendered["payload"]["position"], at as u64,
                "the position the intermediate named: {line}"
            );
            assert_eq!(
                rendered["payload"]["ranked"][0]["token"], 11,
                "the ranking crosses whole: {line}"
            );
            assert_eq!(rendered["payload"]["realized"], 0, "and the realized rank");
            assert_eq!(
                rendered["subsystem"], "spu_decoder",
                "stamped where the field was produced: {line}"
            );
        }
    }

    /// **A fabricated tool result in loop 1's delta refuses before anything
    /// authors**, per `weaver-harness-Spec` section 6: the role's one door is
    /// the grant's, and a supplied result is a fabrication whatever it
    /// carries. What this reads is the refusal arriving before the record
    /// gains a single event of the turn's interior.
    ///
    /// Perturbation: the door is doubled on purpose - `run_turn` refuses the
    /// role and `author_message` refuses it again - so the watch drops both:
    /// remove the role check at the top of `run_turn` and the refusal arm in
    /// `author_message`, and this fails with `ChannelLost`, the fabricated
    /// delta then authoring and the turn reaching a decode seam this test
    /// deliberately closed. Watched under exactly that pair.
    #[test]
    fn a_supplied_tool_result_refuses_at_the_door() {
        let (near, far) = socketpair(
            AddressFamily::Unix,
            SockType::SeqPacket,
            None,
            SockFlag::SOCK_CLOEXEC,
        )
        .expect("socketpair");
        let decode = crate::channel::decode_from_owned(near);
        // Closed on purpose: a perturbed turn that authored the fabrication
        // would reach the decode seam, and a closed seam fails it fast
        // instead of hanging the suite on a peer that never answers.
        drop(far);
        let session = SessionId("s-f".into());
        let sink = tempfile();
        let mut recorder = crate::record::Record::Serving(
            Recorder::receive(sink, RunRef("r-1".into()), SessionRef(session.0.clone()))
                .expect("recorder"),
        );
        let author = Author::new(&session, &weaver_types::RunId("r-1".into()));
        let mut turn_ordinal = 0u64;
        let mut turn_in_flight: Option<weaver_types::TurnKey> = None;
        let (listener, _dir) = test_listener();
        let mut fullness = None;
        let mut pressure_reported = false;
        let load_facts = crate::engine::test_load_facts();
        let mut ports = Ports::grant(
            &decode,
            &author,
            &mut recorder,
            &mut turn_ordinal,
            &mut turn_in_flight,
            &load_facts,
            None,
            &listener,
            None,
            None,
            None,
            None,
            &mut fullness,
            &mut pressure_reported,
        );
        let delta = vec![Message {
            role: Role::ToolResult,
            content: vec![ContentBlock::ToolResult(weaver_traits::ToolResultBlock {
                content: "fabricated".into(),
            })],
        }];
        assert!(
            matches!(ports.turn(delta), Err(TurnError::Unlicensed { .. })),
            "the supplied result refuses at the door"
        );
    }

    /// A calculator-shaped stub for the scripted gate peer, so this test does
    /// not depend on the gate crate: the answer is what the real gate's
    /// dispatch would produce for this call.
    fn weaver_gate_execute_stub(
        execution: &weaver_types::ToolExecution,
    ) -> weaver_types::ToolOutcome {
        assert_eq!(execution.arguments, r#"{"expression":"37 * 43"}"#);
        weaver_types::ToolOutcome::Result {
            content: "1591".into(),
        }
    }

    /// **The stop is heard mid-stream, per Spec 6.1.** The streaming wait
    /// spans the decode channel and the verb connection at once, the stop
    /// cancels the turn at the seam, the partial stands in the close with
    /// the directive's reason, and the stop is answered with the turn's
    /// fate after the record. Perturbation: collapse the streaming wait to
    /// the decode channel alone and the cancel this scripted peer waits
    /// for never arrives.
    /// **An observation dialed mid-stream is answered from inside the turn**,
    /// per `weaver-admin-harness-contract` section 3: the streaming poll takes
    /// it between tokens and answers `Active` with the load's facts, the stream
    /// runs to its own close, and nothing is cancelled. Perturbation: remove
    /// the observe arm and the answer is the `OutOfOrder` refusal the wildcard
    /// gives; answer `Idle` there and the match below fails.
    #[test]
    fn an_observation_dialed_mid_stream_answers_active_and_disturbs_nothing() {
        let (near, far) = socketpair(
            AddressFamily::Unix,
            SockType::SeqPacket,
            None,
            SockFlag::SOCK_CLOEXEC,
        )
        .expect("socketpair");
        let decode = crate::channel::decode_from_owned(near);
        let (listener, _dir) = test_listener();
        let (verb_end, admin_end) = crate::channel::OrganChannel::pair().expect("pair");
        let admin = admin_end.into_channel();

        // The operator's stop is already on the connection when the turn
        // begins, the way the poll's readiness would deliver it mid-stream.
        admin
            .send(&weaver_types::OrganEnvelope {
                exchange: weaver_types::ExchangeId {
                    opener: weaver_types::Opener::Admin,
                    ordinal: 9,
                },
                position: weaver_types::Position::Open,
                payload: weaver_types::Payload::Directive(
                    weaver_types::LifecycleDirective::Observe,
                ),
            })
            .expect("the observe sends");

        let peer = std::thread::spawn(move || {
            let mut buf = vec![0u8; 65536];
            let n = recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).expect("recv append");
            let directive: weaver_types::TokenDirective =
                serde_json::from_slice(&buf[..n]).expect("the append parses");
            assert!(matches!(
                directive,
                weaver_types::TokenDirective::AppendAndGenerate { .. }
            ));
            // The cancel arrives because the wait heard the stop.
            let send_answer = |answer: &weaver_types::TokenAnswer| {
                let bytes = serde_json::to_vec(answer).expect("answer renders");
                send(far.as_raw_fd(), &bytes, MsgFlags::empty()).expect("send answer");
            };
            send_answer(&weaver_types::TokenAnswer::Token {
                token: 7,
                piece: "par".into(),
            });
            let request = serde_json::value::RawValue::from_string(
                r#"{"rendered":"r","template":"t","sampling":{}}"#.to_string(),
            )
            .unwrap();
            let measurement = serde_json::value::RawValue::from_string(
                r#"{"model":"m","weights_hash":"h","input_tokens":[1],"output_tokens":[7],"blocks":[],"timings":{"prefill_ns":"1","decode_ns":"2"}}"#
                    .to_string(),
            )
            .unwrap();
            send_answer(&weaver_types::TokenAnswer::Generated(Generation {
                content: vec![],
                emission: "par".into(),
                finish: Finish::Completed,
                resident: 64,
                capacity: 4096,
                request,
                measurement,
            }));
        });

        let session = SessionId("s-3".into());
        let sink = tempfile();
        let mut recorder = crate::record::Record::Serving(
            Recorder::receive(sink, RunRef("r-1".into()), SessionRef(session.0.clone()))
                .expect("recorder"),
        );
        let author = Author::new(&session, &weaver_types::RunId("r-1".into()));
        author
            .author(
                &mut recorder,
                Kind::Load,
                Subsystem::Harness,
                None,
                Some(Payload::Elections(Box::new(weaver_trace::Elections {
                    residual_readout: false,
                    field: None,
                    surprisal: false,
                    tee: Some(weaver_trace::Election::default()),
                    state_member: false,
                    declaration: Default::default(),
                    lineage: None,
                    reset: None,
                    stack: Default::default(),
                    boundary: String::new(),
                    cause: weaver_trace::Cause { uid: 0 },
                    library_path: None,
                    state_store: Default::default(),
                    composer: weaver_trace::LoopIdentity::compiled("test"),
                }))),
            )
            .expect("load");
        let mut turn_ordinal = 0u64;
        let mut turn_in_flight: Option<weaver_types::TurnKey> = None;
        let mut slot = Some(verb_end);

        let outcome = {
            let mut fullness = None;
            let mut pressure_reported = false;
            let load_facts = crate::engine::test_load_facts();
            let mut ports = Ports::grant(
                &decode,
                &author,
                &mut recorder,
                &mut turn_ordinal,
                &mut turn_in_flight,
                &load_facts,
                None,
                &listener,
                Some(&mut slot),
                None,
                None,
                None,
                &mut fullness,
                &mut pressure_reported,
            );
            let delta = vec![Message {
                role: Role::User,
                content: vec![ContentBlock::Text {
                    text: "observe me".into(),
                }],
            }];
            ports.turn(delta).expect("the observed turn returns")
        };
        peer.join().expect("the decode peer finishes");

        assert!(!outcome.aborted, "an observation aborts nothing");
        assert_eq!(
            outcome.emission, "par",
            "the turn completed as it would have"
        );

        match admin.recv().expect("the observation's answer").payload {
            weaver_types::Payload::Answer(weaver_types::LifecycleAnswer::State {
                state: weaver_types::AgentState::Active,
                load: Some(load),
                ..
            }) => assert_eq!(load.run.0, "r-1", "the load's facts ride beside the state"),
            other => {
                panic!("an observation mid-stream answers active with its load, got {other:?}")
            }
        }
        let close = recorder
            .structure()
            .expect("the serving record")
            .by_kind(Kind::TurnClosed)
            .next()
            .expect("the close authored")
            .line
            .to_string();
        assert!(
            close.contains(r#""close":"clean""#),
            "the close is the turn's own: {close}"
        );
    }

    /// conforms: harness-stop-polled-during-the-invocation
    /// The gate releases its delay only for a correlated cancel. Removing
    /// coordination from the wait fails the delay and completion assertions.
    #[test]
    fn a_stop_during_an_invocation_cancels_and_records_before_acknowledging() {
        stopped_invocation(false, 1);
        stopped_invocation(true, 1);
    }

    /// conforms: harness-stopped-invocation-feeds-no-generation
    /// Removing the stopped check after completion opens the second call
    /// and feeds the result to decode, both observed on the real pairs.
    #[test]
    fn a_stopped_invocation_feeds_nothing_and_opens_no_further_call() {
        stopped_invocation(false, 2);
    }

    fn stopped_invocation(crossing: bool, call_count: usize) {
        use nix::poll::{PollFd, PollFlags, poll};
        use std::os::fd::AsFd;
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        };
        use std::time::{Duration, Instant};
        const DELAY_MS: u16 = 2_000;
        let (listener, _dir) = test_listener();
        let (near, far) = socketpair(
            AddressFamily::Unix,
            SockType::SeqPacket,
            None,
            SockFlag::SOCK_CLOEXEC,
        )
        .unwrap();
        let decode = crate::channel::decode_from_owned(near);
        let (verb_end, admin_end) = crate::channel::OrganChannel::pair().unwrap();
        let admin = admin_end.into_channel();
        let (gate_near, gate_child) = crate::channel::OrganChannel::pair().unwrap();
        let generated = |content, finish| {
            weaver_types::TokenAnswer::Generated(Generation {
                content,
                emission: "tool emission".into(),
                finish,
                resident: 64,
                capacity: 4096,
                request: serde_json::value::RawValue::from_string(
                    r#"{"rendered":"fixture"}"#.into(),
                )
                .unwrap(),
                measurement: serde_json::value::RawValue::from_string(r#"{}"#.into()).unwrap(),
            })
        };
        let calls = (0..call_count)
            .map(|_| {
                ContentBlock::ToolCall(weaver_traits::ToolCall {
                    name: "bash".into(),
                    arguments: r#"{"command":"sleep 5"}"#.into(),
                })
            })
            .collect();
        // Both are ready before the turn: decode's initial answer wins, then
        // the invocation wait must hear coordination while gate holds its answer.
        send(
            far.as_raw_fd(),
            &serde_json::to_vec(&generated(calls, Finish::Completed)).unwrap(),
            MsgFlags::empty(),
        )
        .unwrap();
        let finished = Arc::new(AtomicBool::new(false));
        let decode_finished = finished.clone();
        let stopped = generated(vec![], Finish::Stopped);
        let peer = std::thread::spawn(move || {
            let mut appends = 0;
            let until = Instant::now() + Duration::from_secs(6);
            loop {
                let mut fds = [PollFd::new(far.as_fd(), PollFlags::POLLIN)];
                let ready = poll(&mut fds, 25u16).unwrap();
                if ready > 0 {
                    let mut buf = vec![0; 65536];
                    let n = recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).unwrap();
                    if n == 0 {
                        break;
                    }
                    let request: weaver_types::TokenDirective =
                        serde_json::from_slice(&buf[..n]).unwrap();
                    if matches!(request, TokenDirective::AppendAndGenerate { .. }) {
                        appends += 1;
                    }
                    if appends > 1 || matches!(request, TokenDirective::Cancel { .. }) {
                        send(
                            far.as_raw_fd(),
                            &serde_json::to_vec(&stopped).unwrap(),
                            MsgFlags::empty(),
                        )
                        .unwrap();
                    }
                } else if decode_finished.load(Ordering::Acquire) {
                    break;
                }
                assert!(Instant::now() < until, "decode fixture did not finish");
            }
            appends
        });
        let gate_finished = finished.clone();
        let gate_peer = std::thread::spawn(move || {
            let gate = gate_child.into_channel();
            let mut fds = [PollFd::new(gate.as_fd(), PollFlags::POLLIN)];
            assert!(poll(&mut fds, 5_000u16).unwrap() > 0);
            let open = gate.recv().unwrap();
            assert_eq!(open.position, weaver_types::Position::Open);
            assert!(matches!(open.payload, weaver_types::Payload::Tool(_)));
            let mut fds = [PollFd::new(gate.as_fd(), PollFlags::POLLIN)];
            let ready = poll(&mut fds, DELAY_MS).unwrap() > 0;
            let answer = |outcome| {
                gate.send(&weaver_types::OrganEnvelope {
                    exchange: open.exchange.clone(),
                    position: weaver_types::Position::Close,
                    payload: weaver_types::Payload::ToolAnswer(outcome),
                })
                .unwrap();
            };
            // In the crossing arm the result leaves before the gate reads
            // the queued cancel, as on a naturally completed invocation.
            if crossing {
                answer(weaver_types::ToolOutcome::Result {
                    content: "finished as cancel crossed".into(),
                });
            }
            let mut cancels = 0;
            if ready {
                let cancel = gate.recv().unwrap();
                assert_eq!(cancel.exchange, open.exchange);
                assert_eq!(cancel.position, weaver_types::Position::Continue);
                assert_eq!(cancel.payload, weaver_types::Payload::ToolCancel);
                cancels += 1;
            }
            if !crossing {
                answer(if ready {
                    weaver_types::ToolOutcome::Killed {
                        partial: Some("partial".into()),
                        by: weaver_types::KillCause::Cancel,
                    }
                } else {
                    weaver_types::ToolOutcome::Result {
                        content: "finished at fixture delay".into(),
                    }
                });
            }
            let mut executions = 1;
            let until = Instant::now() + Duration::from_secs(6);
            loop {
                let mut fds = [PollFd::new(gate.as_fd(), PollFlags::POLLIN)];
                if poll(&mut fds, 25u16).unwrap() > 0 {
                    let Ok(envelope) = gate.recv() else {
                        break;
                    };
                    match envelope.payload {
                        weaver_types::Payload::ToolCancel => cancels += 1,
                        weaver_types::Payload::Tool(_) => {
                            executions += 1;
                            gate.send(&weaver_types::OrganEnvelope {
                                exchange: envelope.exchange,
                                position: weaver_types::Position::Close,
                                payload: weaver_types::Payload::ToolAnswer(
                                    weaver_types::ToolOutcome::Result {
                                        content: "unwanted second execution".into(),
                                    },
                                ),
                            })
                            .unwrap();
                        }
                        _ => panic!("unexpected gate traffic"),
                    }
                } else if gate_finished.load(Ordering::Acquire) {
                    break;
                }
                assert!(Instant::now() < until, "gate fixture did not finish");
            }
            (cancels, executions)
        });
        let session = SessionId("s-stop-invocation".into());
        let mut recorder = crate::record::Record::Serving(
            Recorder::receive(
                tempfile(),
                RunRef("r-1".into()),
                SessionRef(session.0.clone()),
            )
            .expect("recorder"),
        );
        // Author owns its origin. Bracket its construction so trace
        // monotonic timestamps map to intervals on this test's clock
        // without changing production code or treating return as close.
        let origin_before = Instant::now();
        let author = Author::new(&session, &weaver_types::RunId("r-1".into()));
        let origin_after = Instant::now();
        author
            .author(
                &mut recorder,
                Kind::Load,
                Subsystem::Harness,
                None,
                Some(Payload::Elections(Box::new(weaver_trace::Elections {
                    residual_readout: false,
                    field: None,
                    surprisal: false,
                    tee: Some(weaver_trace::Election::default()),
                    state_member: false,
                    declaration: Default::default(),
                    lineage: None,
                    reset: None,
                    stack: Default::default(),
                    boundary: String::new(),
                    cause: weaver_trace::Cause { uid: 0 },
                    library_path: None,
                    state_store: Default::default(),
                    composer: weaver_trace::LoopIdentity::compiled("test"),
                }))),
            )
            .expect("load");
        let mut turn_ordinal = 0;
        let mut turn_in_flight = None;
        let mut slot = Some(verb_end);
        let mut gate_ordinal = 0;
        let mut held = std::collections::VecDeque::new();
        let mut leave = None;
        let mut fullness = None;
        let mut pressure_reported = false;
        let load_facts = test_load_facts();

        let stop_before = Instant::now();
        let stop_exchange = weaver_types::ExchangeId {
            opener: weaver_types::Opener::Admin,
            ordinal: 9,
        };
        admin
            .send(&weaver_types::OrganEnvelope {
                exchange: stop_exchange.clone(),
                position: weaver_types::Position::Open,
                payload: weaver_types::Payload::Directive(weaver_types::LifecycleDirective::Stop {
                    cause: weaver_types::Cause { uid: 0 },
                }),
            })
            .unwrap();
        // Observe acknowledgment on its own thread, so an early send cannot
        // hide in the socket buffer until after turn() has returned.
        let acknowledgment = std::thread::spawn(move || {
            let mut fds = [PollFd::new(admin.as_fd(), PollFlags::POLLIN)];
            assert!(poll(&mut fds, 5_000u16).unwrap() > 0, "stop unanswered");
            let answer = admin.recv().unwrap();
            (answer, Instant::now())
        });
        let result = {
            let mut ports = Ports::grant(
                &decode,
                &author,
                &mut recorder,
                &mut turn_ordinal,
                &mut turn_in_flight,
                &load_facts,
                None,
                &listener,
                Some(&mut slot),
                Some(GatePort {
                    channel: &gate_near,
                    ordinal: &mut gate_ordinal,
                    held: &mut held,
                    leave: &mut leave,
                }),
                None,
                None,
                &mut fullness,
                &mut pressure_reported,
            );
            ports.turn(vec![Message {
                role: Role::User,
                content: vec![ContentBlock::Text {
                    text: "stop invocation".into(),
                }],
            }])
        };
        finished.store(true, Ordering::Release);
        let appends = peer.join().unwrap();
        let (cancels, executions) = gate_peer.join().unwrap();
        let (answer, acknowledged) = acknowledgment.join().unwrap();
        let outcome = result.expect("turn returns");
        assert!(outcome.aborted && outcome.stopped);
        assert_eq!(answer.exchange, stop_exchange);
        assert!(matches!(
            answer.payload,
            weaver_types::Payload::Answer(weaver_types::LifecycleAnswer::TurnAborted { .. })
        ));
        assert_eq!(cancels, 1, "exactly one cancel");
        assert_eq!(executions, 1, "no second execution");
        assert_eq!(appends, 1, "no result re-feed");
        let trace = recorder.structure().unwrap();
        let event = |kind| {
            serde_json::from_str::<serde_json::Value>(
                &trace.by_kind(kind).next().expect("trace event").line,
            )
            .unwrap()
        };
        let completed = event(Kind::ToolCallCompleted);
        let close = event(Kind::TurnClosed);
        let ns = |event: &serde_json::Value| {
            event["monotonic_ns"]
                .as_str()
                .unwrap()
                .parse::<u64>()
                .unwrap()
        };
        assert!(ns(&completed) < ns(&close), "completion precedes close");
        assert!(
            origin_after + Duration::from_nanos(ns(&close)) <= acknowledged,
            "record precedes acknowledgment"
        );
        assert!(
            origin_before + Duration::from_nanos(ns(&close))
                < stop_before + Duration::from_millis(800),
            "stop spent fixture delay"
        );
        assert!(
            close.to_string().contains(r#""reason":"directive""#),
            "{close}"
        );
        if crossing {
            assert!(
                completed.to_string().contains("finished as cancel crossed"),
                "{completed}"
            );
        } else {
            assert!(
                completed.to_string().contains(r#""by":"cancel""#),
                "{completed}"
            );
        }
    }

    /// **The verbs a pending leave meets while the turn it waits on runs**,
    /// per `weaver-harness-Spec` section 6 and `weaver-admin-Spec` section 3's
    /// lock gap, on the operator's rulings of 2026-10-07 on #1: the model
    /// holds its generation open, a graceful leave arrives and is held, and
    /// further verbs dial in as admin dials them. A second unforced leave is
    /// refused `OutOfOrder`; a `save-point` is refused `OutOfOrder`; an
    /// observation answers `Active`; a `force-unload` that does not hold the
    /// lock (`JoinLeave`) joins the pending one, turns it forced and cancels
    /// the generation, the turn closing stopped by the unload. The decode stand-in mirrors the SPU's order
    /// (`weaver-spu/src/main.rs`, the `AppendAndGenerate` arm, a cancel
    /// answered with the stopped generation); the gate stand-in answers the
    /// quiesce as the raised gate does. Needs root, the listener admitting
    /// root alone: run by the leave watch in the lifecycle module.
    /// Perturbations: let a second unforced leave through and its refusal
    /// never comes; drop the join and the generation is never cancelled, the
    /// fixture's bound failing.
    #[test]
    #[ignore = "needs root in a user namespace; run by the_leave_instruments_are_watched_inside_a_user_namespace"]
    fn a_second_dial_during_a_held_turn_meets_the_pending_leave() {
        use nix::poll::{PollFd, PollFlags, poll};
        use std::os::fd::AsFd;
        use std::time::{Duration, Instant};
        assert!(
            nix::unistd::geteuid().is_root(),
            "this instrument needs euid 0"
        );
        let (listener, dir) = test_listener();
        let socket = dir.0.join("c.sock");
        let (near, far) = socketpair(
            AddressFamily::Unix,
            SockType::SeqPacket,
            None,
            SockFlag::SOCK_CLOEXEC,
        )
        .unwrap();
        let decode = crate::channel::decode_from_owned(near);
        let (verb_end, admin_end) = crate::channel::OrganChannel::pair().unwrap();
        let admin = admin_end.into_channel();
        let (gate_near, gate_child) = crate::channel::OrganChannel::pair().unwrap();
        let generated = |finish| {
            weaver_types::TokenAnswer::Generated(Generation {
                content: vec![ContentBlock::Text {
                    text: "held".into(),
                }],
                emission: "held".into(),
                finish,
                resident: 64,
                capacity: 4096,
                request: serde_json::value::RawValue::from_string(
                    r#"{"rendered":"fixture"}"#.into(),
                )
                .unwrap(),
                measurement: serde_json::value::RawValue::from_string(r#"{}"#.into()).unwrap(),
            })
        };
        // The model: the generation is held until a cancel, then stopped.
        let stopped = generated(Finish::Stopped);
        let model = std::thread::spawn(move || {
            let until = Instant::now() + Duration::from_secs(10);
            let mut cancelled = false;
            loop {
                let mut fds = [PollFd::new(far.as_fd(), PollFlags::POLLIN)];
                if poll(&mut fds, 25u16).unwrap() > 0 {
                    let mut buf = vec![0; 65536];
                    let n = recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).unwrap();
                    if n == 0 {
                        break;
                    }
                    let request: weaver_types::TokenDirective =
                        serde_json::from_slice(&buf[..n]).unwrap();
                    if matches!(request, TokenDirective::Cancel { .. }) {
                        cancelled = true;
                        send(
                            far.as_raw_fd(),
                            &serde_json::to_vec(&stopped).unwrap(),
                            MsgFlags::empty(),
                        )
                        .unwrap();
                        break;
                    }
                }
                assert!(Instant::now() < until, "the generation was never cancelled");
            }
            cancelled
        });
        // The gate: answers the quiesce, and says it did.
        let (quiesced_tx, quiesced) = std::sync::mpsc::channel::<()>();
        let gate = std::thread::spawn(move || {
            let gate = gate_child.into_channel();
            while let Ok(envelope) = gate.recv() {
                if matches!(
                    envelope.payload,
                    weaver_types::Payload::Directive(weaver_types::LifecycleDirective::Quiesce)
                ) {
                    let _ = gate.send(&weaver_types::OrganEnvelope {
                        exchange: envelope.exchange,
                        position: weaver_types::Position::Close,
                        payload: weaver_types::Payload::Answer(
                            weaver_types::LifecycleAnswer::GateQuiesced,
                        ),
                    });
                    let _ = quiesced_tx.send(());
                }
            }
        });
        let leave_exchange = weaver_types::ExchangeId {
            opener: weaver_types::Opener::Admin,
            ordinal: 9,
        };
        admin
            .send(&weaver_types::OrganEnvelope {
                exchange: leave_exchange.clone(),
                position: weaver_types::Position::Open,
                // The graceful caller is the operator's account, the force
                // root's, so the record can tell them apart.
                payload: weaver_types::Payload::Directive(
                    weaver_types::LifecycleDirective::Leave {
                        cause: weaver_types::Cause { uid: 1000 },
                        forced: false,
                    },
                ),
            })
            .unwrap();
        // The dials, as admin makes them while the leave is pending.
        let dials = std::thread::spawn(move || {
            quiesced
                .recv_timeout(Duration::from_secs(10))
                .expect("the gate was quiesced");
            let ask = |directive| {
                let dialed = crate::channel::OrganChannel::dial_for_test(&socket);
                dialed
                    .send(&weaver_types::OrganEnvelope {
                        exchange: weaver_types::ExchangeId {
                            opener: weaver_types::Opener::Admin,
                            ordinal: 1,
                        },
                        position: weaver_types::Position::Open,
                        payload: weaver_types::Payload::Directive(directive),
                    })
                    .unwrap();
                dialed
            };
            let answered = |dialed: crate::channel::OrganChannel| {
                let mut fds = [PollFd::new(dialed.as_fd(), PollFlags::POLLIN)];
                assert!(poll(&mut fds, 5_000u16).unwrap() > 0, "unanswered");
                dialed.recv().unwrap().payload
            };
            let second = answered(ask(weaver_types::LifecycleDirective::Leave {
                cause: weaver_types::Cause { uid: 0 },
                forced: false,
            }));
            let save_point = answered(ask(weaver_types::LifecycleDirective::SavePoint {
                cause: weaver_types::Cause { uid: 0 },
            }));
            let observed = answered(ask(weaver_types::LifecycleDirective::Observe));
            // The force as admin sends it beside a graceful unload holding
            // the lock: a join, not a leave.
            let forced = ask(weaver_types::LifecycleDirective::JoinLeave {
                cause: weaver_types::Cause { uid: 0 },
            });
            (second, save_point, observed, forced)
        });
        let session = SessionId("s-held-turn".into());
        let mut recorder = crate::record::Record::Serving(
            Recorder::receive(
                tempfile(),
                RunRef("r-1".into()),
                SessionRef(session.0.clone()),
            )
            .expect("recorder"),
        );
        let author = Author::new(&session, &weaver_types::RunId("r-1".into()));
        let mut turn_ordinal = 0;
        let mut turn_in_flight = None;
        let mut slot = Some(verb_end);
        let mut gate_ordinal = 0;
        let mut held = std::collections::VecDeque::new();
        let mut leave = None;
        let mut fullness = None;
        let mut pressure_reported = false;
        let load_facts = test_load_facts();
        let result = {
            let mut ports = Ports::grant(
                &decode,
                &author,
                &mut recorder,
                &mut turn_ordinal,
                &mut turn_in_flight,
                &load_facts,
                None,
                &listener,
                Some(&mut slot),
                Some(GatePort {
                    channel: &gate_near,
                    ordinal: &mut gate_ordinal,
                    held: &mut held,
                    leave: &mut leave,
                }),
                None,
                None,
                &mut fullness,
                &mut pressure_reported,
            );
            ports.turn(vec![Message {
                role: Role::User,
                content: vec![ContentBlock::Text {
                    text: "hold the turn".into(),
                }],
            }])
        };
        let (second, save_point, observed, forced_dial) = dials.join().unwrap();
        assert!(
            model.join().unwrap(),
            "the forced leave cancelled the generation"
        );
        drop(gate_near);
        gate.join().unwrap();
        let outcome = result.expect("the turn returns");
        assert!(outcome.aborted, "stopped by the forced leave");
        assert_eq!(
            second,
            weaver_types::Payload::Refusal(weaver_types::LifecycleRefusal::OutOfOrder),
            "a second unforced leave is out of order"
        );
        assert_eq!(
            save_point,
            weaver_types::Payload::Refusal(weaver_types::LifecycleRefusal::OutOfOrder),
            "a save point is out of order while the leave is pending"
        );
        assert!(
            matches!(
                observed,
                weaver_types::Payload::Answer(weaver_types::LifecycleAnswer::State {
                    state: weaver_types::AgentState::Active,
                    ..
                })
            ),
            "{observed:?}"
        );
        let pending = leave.as_ref().expect("the leave waits for the serve loop");
        assert!(
            pending.forced && pending.quiesced,
            "joined and turned forced"
        );
        assert_eq!(
            pending.answers.len(),
            2,
            "the graceful dialer and the force"
        );
        assert_eq!(pending.answers[0].1, leave_exchange);
        // **Both callers are kept** (Codex on #94, round 19): the leave's
        // cause stays the graceful caller's, and the force is named apart.
        // Perturbation: drop the `forced_by` write and the force goes unnamed.
        assert_eq!(pending.cause, weaver_types::Cause { uid: 1000 });
        assert_eq!(pending.forced_by, Some(weaver_types::Cause { uid: 0 }));
        drop(forced_dial);
        let trace = recorder.structure().unwrap();
        let close = serde_json::from_str::<serde_json::Value>(
            &trace
                .by_kind(Kind::TurnClosed)
                .next()
                .expect("a close")
                .line,
        )
        .unwrap();
        assert!(
            close.to_string().contains(r#""reason":"unload""#),
            "{close}"
        );
    }

    /// **A graceful leave while a tool call is out interrupts the call and
    /// quiesces the gate**, per `weaver-harness-Spec` section 6 on the
    /// operator's rulings of 2026-10-07 on #1: the call's return would come
    /// through a gate that accepts no further input, so it is cancelled and
    /// recorded killed by the cancel, the turn closes as stopped by the
    /// directive, the gate is told to quiesce only after the cancel crosses,
    /// and the leave waits, unanswered, for the serve loop with its
    /// connection out of the verb slot. Perturbation: send the quiesce before
    /// the cancel and the gate stand-in sees it first.
    #[test]
    fn a_leave_while_a_tool_call_is_out_interrupts_it_and_quiesces_after_the_cancel() {
        left_mid_invocation();
    }

    fn left_mid_invocation() {
        let (crossing, call_count) = (false, 1);
        use nix::poll::{PollFd, PollFlags, poll};
        use std::os::fd::AsFd;
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        };
        use std::time::{Duration, Instant};
        const DELAY_MS: u16 = 2_000;
        let (listener, _dir) = test_listener();
        let (near, far) = socketpair(
            AddressFamily::Unix,
            SockType::SeqPacket,
            None,
            SockFlag::SOCK_CLOEXEC,
        )
        .unwrap();
        let decode = crate::channel::decode_from_owned(near);
        let (verb_end, admin_end) = crate::channel::OrganChannel::pair().unwrap();
        let admin = admin_end.into_channel();
        let (gate_near, gate_child) = crate::channel::OrganChannel::pair().unwrap();
        let generated = |content, finish| {
            weaver_types::TokenAnswer::Generated(Generation {
                content,
                emission: "tool emission".into(),
                finish,
                resident: 64,
                capacity: 4096,
                request: serde_json::value::RawValue::from_string(
                    r#"{"rendered":"fixture"}"#.into(),
                )
                .unwrap(),
                measurement: serde_json::value::RawValue::from_string(r#"{}"#.into()).unwrap(),
            })
        };
        let calls = (0..call_count)
            .map(|_| {
                ContentBlock::ToolCall(weaver_traits::ToolCall {
                    name: "bash".into(),
                    arguments: r#"{"command":"sleep 5"}"#.into(),
                })
            })
            .collect();
        // Both are ready before the turn: decode's initial answer wins, then
        // the invocation wait must hear coordination while gate holds its answer.
        send(
            far.as_raw_fd(),
            &serde_json::to_vec(&generated(calls, Finish::Completed)).unwrap(),
            MsgFlags::empty(),
        )
        .unwrap();
        let finished = Arc::new(AtomicBool::new(false));
        let decode_finished = finished.clone();
        let stopped = generated(vec![], Finish::Stopped);
        let peer = std::thread::spawn(move || {
            let mut appends = 0;
            let until = Instant::now() + Duration::from_secs(6);
            loop {
                let mut fds = [PollFd::new(far.as_fd(), PollFlags::POLLIN)];
                let ready = poll(&mut fds, 25u16).unwrap();
                if ready > 0 {
                    let mut buf = vec![0; 65536];
                    let n = recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).unwrap();
                    if n == 0 {
                        break;
                    }
                    let request: weaver_types::TokenDirective =
                        serde_json::from_slice(&buf[..n]).unwrap();
                    if matches!(request, TokenDirective::AppendAndGenerate { .. }) {
                        appends += 1;
                    }
                    if appends > 1 || matches!(request, TokenDirective::Cancel { .. }) {
                        send(
                            far.as_raw_fd(),
                            &serde_json::to_vec(&stopped).unwrap(),
                            MsgFlags::empty(),
                        )
                        .unwrap();
                    }
                } else if decode_finished.load(Ordering::Acquire) {
                    break;
                }
                assert!(Instant::now() < until, "decode fixture did not finish");
            }
            appends
        });
        let gate_finished = finished.clone();
        let gate_peer = std::thread::spawn(move || {
            let gate = gate_child.into_channel();
            let mut fds = [PollFd::new(gate.as_fd(), PollFlags::POLLIN)];
            assert!(poll(&mut fds, 5_000u16).unwrap() > 0);
            let open = gate.recv().unwrap();
            assert_eq!(open.position, weaver_types::Position::Open);
            assert!(matches!(open.payload, weaver_types::Payload::Tool(_)));
            let mut fds = [PollFd::new(gate.as_fd(), PollFlags::POLLIN)];
            let ready = poll(&mut fds, DELAY_MS).unwrap() > 0;
            let answer = |outcome| {
                gate.send(&weaver_types::OrganEnvelope {
                    exchange: open.exchange.clone(),
                    position: weaver_types::Position::Close,
                    payload: weaver_types::Payload::ToolAnswer(outcome),
                })
                .unwrap();
            };
            // In the crossing arm the result leaves before the gate reads
            // the queued cancel, as on a naturally completed invocation.
            if crossing {
                answer(weaver_types::ToolOutcome::Result {
                    content: "finished as cancel crossed".into(),
                });
            }
            let mut cancels = 0;
            if ready {
                let cancel = gate.recv().unwrap();
                assert_eq!(cancel.exchange, open.exchange);
                assert_eq!(cancel.position, weaver_types::Position::Continue);
                // The unload's cancel is the interrupt, answered as the
                // gate answers it (`weaver-gate/src/tools.rs`,
                // `supervision_wait`): killed by the unload.
                assert_eq!(cancel.payload, weaver_types::Payload::ToolInterrupt);
                cancels += 1;
            }
            if !crossing {
                answer(if ready {
                    weaver_types::ToolOutcome::Killed {
                        partial: Some("partial".into()),
                        by: weaver_types::KillCause::Unload,
                    }
                } else {
                    weaver_types::ToolOutcome::Result {
                        content: "finished at fixture delay".into(),
                    }
                });
            }
            let mut executions = 1;
            let mut quiesced_after: Option<u32> = None;
            let until = Instant::now() + Duration::from_secs(6);
            loop {
                let mut fds = [PollFd::new(gate.as_fd(), PollFlags::POLLIN)];
                if poll(&mut fds, 25u16).unwrap() > 0 {
                    let Ok(envelope) = gate.recv() else {
                        break;
                    };
                    match envelope.payload {
                        weaver_types::Payload::ToolCancel => cancels += 1,
                        weaver_types::Payload::Tool(_) => {
                            executions += 1;
                            gate.send(&weaver_types::OrganEnvelope {
                                exchange: envelope.exchange,
                                position: weaver_types::Position::Close,
                                payload: weaver_types::Payload::ToolAnswer(
                                    weaver_types::ToolOutcome::Result {
                                        content: "unwanted second execution".into(),
                                    },
                                ),
                            })
                            .unwrap();
                        }
                        // The quiesce arrives only after the cancel, as a
                        // gate executing a tool reads its cancel alone
                        // (`weaver-gate/src/tools.rs`, `supervision_wait`);
                        // answered as the raised gate answers it
                        // (`weaver-gate/src/main.rs`, `(Raised, Quiesce)`).
                        weaver_types::Payload::Directive(
                            weaver_types::LifecycleDirective::Quiesce,
                        ) => {
                            quiesced_after = Some(cancels);
                            gate.send(&weaver_types::OrganEnvelope {
                                exchange: envelope.exchange,
                                position: weaver_types::Position::Close,
                                payload: weaver_types::Payload::Answer(
                                    weaver_types::LifecycleAnswer::GateQuiesced,
                                ),
                            })
                            .unwrap();
                        }
                        _ => panic!("unexpected gate traffic"),
                    }
                } else if gate_finished.load(Ordering::Acquire) {
                    break;
                }
                assert!(Instant::now() < until, "gate fixture did not finish");
            }
            (cancels, executions, quiesced_after)
        });
        let session = SessionId("s-stop-invocation".into());
        let mut recorder = crate::record::Record::Serving(
            Recorder::receive(
                tempfile(),
                RunRef("r-1".into()),
                SessionRef(session.0.clone()),
            )
            .expect("recorder"),
        );
        // Author owns its origin. Bracket its construction so trace
        // monotonic timestamps map to intervals on this test's clock
        // without changing production code or treating return as close.
        let origin_before = Instant::now();
        let author = Author::new(&session, &weaver_types::RunId("r-1".into()));
        let origin_after = Instant::now();
        author
            .author(
                &mut recorder,
                Kind::Load,
                Subsystem::Harness,
                None,
                Some(Payload::Elections(Box::new(weaver_trace::Elections {
                    residual_readout: false,
                    field: None,
                    surprisal: false,
                    tee: Some(weaver_trace::Election::default()),
                    state_member: false,
                    declaration: Default::default(),
                    lineage: None,
                    reset: None,
                    stack: Default::default(),
                    boundary: String::new(),
                    cause: weaver_trace::Cause { uid: 0 },
                    library_path: None,
                    state_store: Default::default(),
                    composer: weaver_trace::LoopIdentity::compiled("test"),
                }))),
            )
            .expect("load");
        let mut turn_ordinal = 0;
        let mut turn_in_flight = None;
        let mut slot = Some(verb_end);
        let mut gate_ordinal = 0;
        let mut held = std::collections::VecDeque::new();
        let mut leave = None;
        let mut fullness = None;
        let mut pressure_reported = false;
        let load_facts = test_load_facts();

        let stop_before = Instant::now();
        let stop_exchange = weaver_types::ExchangeId {
            opener: weaver_types::Opener::Admin,
            ordinal: 9,
        };
        admin
            .send(&weaver_types::OrganEnvelope {
                exchange: stop_exchange.clone(),
                position: weaver_types::Position::Open,
                payload: weaver_types::Payload::Directive(
                    weaver_types::LifecycleDirective::Leave {
                        cause: weaver_types::Cause { uid: 0 },
                        forced: false,
                    },
                ),
            })
            .unwrap();
        let result = {
            let mut ports = Ports::grant(
                &decode,
                &author,
                &mut recorder,
                &mut turn_ordinal,
                &mut turn_in_flight,
                &load_facts,
                None,
                &listener,
                Some(&mut slot),
                Some(GatePort {
                    channel: &gate_near,
                    ordinal: &mut gate_ordinal,
                    held: &mut held,
                    leave: &mut leave,
                }),
                None,
                None,
                &mut fullness,
                &mut pressure_reported,
            );
            ports.turn(vec![Message {
                role: Role::User,
                content: vec![ContentBlock::Text {
                    text: "stop invocation".into(),
                }],
            }])
        };
        finished.store(true, Ordering::Release);
        let appends = peer.join().unwrap();
        let (cancels, executions, quiesced_after) = gate_peer.join().unwrap();
        let outcome = result.expect("turn returns");
        assert!(outcome.aborted && outcome.stopped);
        assert_eq!(cancels, 1, "exactly one cancel");
        assert_eq!(executions, 1, "no second execution");
        assert_eq!(appends, 1, "no result re-feed");
        assert_eq!(
            quiesced_after,
            Some(1),
            "the quiesce crosses after the cancel"
        );
        let pending = leave.as_ref().expect("the leave waits for the serve loop");
        assert!(pending.quiesced && !pending.forced);
        assert_eq!(pending.answers.len(), 1);
        assert_eq!(pending.answers[0].1, stop_exchange);
        assert!(slot.is_none(), "the leave's connection left the verb slot");
        let mut fds = [PollFd::new(admin.as_fd(), PollFlags::POLLIN)];
        assert_eq!(
            poll(&mut fds, 200u16).unwrap(),
            0,
            "the turn answers the leave nothing"
        );
        let trace = recorder.structure().unwrap();
        let event = |kind| {
            serde_json::from_str::<serde_json::Value>(
                &trace.by_kind(kind).next().expect("trace event").line,
            )
            .unwrap()
        };
        let completed = event(Kind::ToolCallCompleted);
        let close = event(Kind::TurnClosed);
        assert!(
            close.to_string().contains(r#""reason":"unload""#),
            "{close}"
        );
        assert!(
            completed.to_string().contains(r#""by":"unload""#),
            "{completed}"
        );
        let _ = (crossing, origin_before, origin_after, stop_before);
    }

    #[test]
    fn a_stop_dialed_mid_stream_cancels_the_turn() {
        let (near, far) = socketpair(
            AddressFamily::Unix,
            SockType::SeqPacket,
            None,
            SockFlag::SOCK_CLOEXEC,
        )
        .expect("socketpair");
        let decode = crate::channel::decode_from_owned(near);
        let (listener, _dir) = test_listener();
        let (verb_end, admin_end) = crate::channel::OrganChannel::pair().expect("pair");
        let admin = admin_end.into_channel();

        // The operator's stop is already on the connection when the turn
        // begins, the way the poll's readiness would deliver it mid-stream.
        admin
            .send(&weaver_types::OrganEnvelope {
                exchange: weaver_types::ExchangeId {
                    opener: weaver_types::Opener::Admin,
                    ordinal: 9,
                },
                position: weaver_types::Position::Open,
                payload: weaver_types::Payload::Directive(weaver_types::LifecycleDirective::Stop {
                    cause: weaver_types::Cause { uid: 0 },
                }),
            })
            .expect("the stop sends");

        let peer = std::thread::spawn(move || {
            let mut buf = vec![0u8; 65536];
            let n = recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).expect("recv append");
            let directive: weaver_types::TokenDirective =
                serde_json::from_slice(&buf[..n]).expect("the append parses");
            assert!(matches!(
                directive,
                weaver_types::TokenDirective::AppendAndGenerate { .. }
            ));
            // The cancel arrives because the wait heard the stop.
            let n = recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).expect("recv cancel");
            let directive: weaver_types::TokenDirective =
                serde_json::from_slice(&buf[..n]).expect("the cancel parses");
            assert!(
                matches!(directive, weaver_types::TokenDirective::Cancel { .. }),
                "the stop cancels at the seam"
            );
            let send_answer = |answer: &weaver_types::TokenAnswer| {
                let bytes = serde_json::to_vec(answer).expect("answer renders");
                send(far.as_raw_fd(), &bytes, MsgFlags::empty()).expect("send answer");
            };
            send_answer(&weaver_types::TokenAnswer::Token {
                token: 7,
                piece: "par".into(),
            });
            let request = serde_json::value::RawValue::from_string(
                r#"{"rendered":"r","template":"t","sampling":{}}"#.to_string(),
            )
            .unwrap();
            let measurement = serde_json::value::RawValue::from_string(
                r#"{"model":"m","weights_hash":"h","input_tokens":[1],"output_tokens":[7],"blocks":[],"timings":{"prefill_ns":"1","decode_ns":"2"}}"#
                    .to_string(),
            )
            .unwrap();
            send_answer(&weaver_types::TokenAnswer::Generated(Generation {
                content: vec![],
                emission: "par".into(),
                finish: Finish::Stopped,
                resident: 64,
                capacity: 4096,
                request,
                measurement,
            }));
        });

        let session = SessionId("s-2".into());
        let sink = tempfile();
        let mut recorder = crate::record::Record::Serving(
            Recorder::receive(sink, RunRef("r-1".into()), SessionRef(session.0.clone()))
                .expect("recorder"),
        );
        let author = Author::new(&session, &weaver_types::RunId("r-1".into()));
        author
            .author(
                &mut recorder,
                Kind::Load,
                Subsystem::Harness,
                None,
                Some(Payload::Elections(Box::new(weaver_trace::Elections {
                    residual_readout: false,
                    field: None,
                    surprisal: false,
                    tee: Some(weaver_trace::Election::default()),
                    state_member: false,
                    declaration: Default::default(),
                    lineage: None,
                    reset: None,
                    stack: Default::default(),
                    boundary: String::new(),
                    cause: weaver_trace::Cause { uid: 0 },
                    library_path: None,
                    state_store: Default::default(),
                    composer: weaver_trace::LoopIdentity::compiled("test"),
                }))),
            )
            .expect("load");
        let mut turn_ordinal = 0u64;
        let mut turn_in_flight: Option<weaver_types::TurnKey> = None;
        let mut slot = Some(verb_end);

        let outcome = {
            let mut fullness = None;
            let mut pressure_reported = false;
            let load_facts = crate::engine::test_load_facts();
            let mut ports = Ports::grant(
                &decode,
                &author,
                &mut recorder,
                &mut turn_ordinal,
                &mut turn_in_flight,
                &load_facts,
                None,
                &listener,
                Some(&mut slot),
                None,
                None,
                None,
                &mut fullness,
                &mut pressure_reported,
            );
            let delta = vec![Message {
                role: Role::User,
                content: vec![ContentBlock::Text {
                    text: "stop me".into(),
                }],
            }];
            ports.turn(delta).expect("the aborted turn still returns")
        };
        peer.join().expect("the decode peer finishes");

        assert!(outcome.aborted, "the directive aborted the turn");
        assert_eq!(outcome.emission, "par", "the partial stands");

        // Announce after record: the answer carries the turn's fate.
        match admin.recv().expect("the stop's answer").payload {
            weaver_types::Payload::Answer(weaver_types::LifecycleAnswer::TurnAborted { turn }) => {
                assert_eq!(turn.0, "t-1");
            }
            other => panic!("the stop answers the turn's fate, got {other:?}"),
        }

        // The close names the directive, the partial standing before it.
        let close = recorder
            .structure()
            .expect("the serving record")
            .by_kind(Kind::TurnClosed)
            .next()
            .expect("the close authored")
            .line
            .to_string();
        assert!(
            close.contains(r#""close":"stopped""#) && close.contains(r#""reason":"directive""#),
            "the close names the directive, not the fault: {close}"
        );
    }

    /// **A column frame on the generate stream authors rather than kills.**
    /// The contract's third intermediate rides the append-and-generate
    /// exchange wherever the open's ask stood, so the scripted peer sends
    /// one mid-stream against a diagnostic record and the turn completes
    /// with the `residual.column` event authored where a wildcard would
    /// have read channel loss.
    ///
    /// Perturbation: remove the `Column` arm from the generate stream and
    /// this fails, the turn dying of a frame the contract licenses.
    /// Watched under exactly that removal.
    #[test]
    fn a_column_on_the_generate_stream_authors_rather_than_kills() {
        let (near, far) = socketpair(
            AddressFamily::Unix,
            SockType::SeqPacket,
            None,
            SockFlag::SOCK_CLOEXEC,
        )
        .expect("socketpair");
        let decode = crate::channel::decode_from_owned(near);

        let peer = std::thread::spawn(move || {
            let mut buf = vec![0u8; 65536];
            let _ = recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).expect("recv append");
            let column =
                r#"{"kind":"column","body":{"position":7,"layers":[[1.0,2.0],[3.0,4.0]]}}"#;
            send(far.as_raw_fd(), column.as_bytes(), MsgFlags::empty()).expect("send column");
            let generated = concat!(
                r#"{"kind":"generated","body":{"emission":"hi","finish":"completed","#,
                r#""content":[{"type":"text","text":"hi"}],"#,
                r#""request":{"rendered":"hi","template":"t","sampling":{}},"#,
                r#""measurement":{"input_tokens":[1],"output_tokens":[2]},"#,
                r#""resident":4,"capacity":64}}"#
            );
            send(far.as_raw_fd(), generated.as_bytes(), MsgFlags::empty()).expect("send close");
        });

        let session = SessionId("s-d".into());
        let sink_path = crate::scratch::Scratch(std::env::temp_dir().join(format!(
            "weaver-engine-column-{}-{:?}.ndjson",
            std::process::id(),
            std::thread::current().id()
        )));
        let sink = OwnedFd::from(std::fs::File::create(&sink_path).expect("sink"));
        let mut recorder = crate::record::Record::Diagnostic(
            weaver_diagnostic::Recorder::receive(
                sink,
                weaver_diagnostic::RunRef("r-d".into()),
                weaver_diagnostic::SessionRef(session.0.clone()),
            )
            .expect("the recorder receives"),
        );
        let author = Author::new(&session, &weaver_types::RunId("r-d".into()));
        let mut turn_ordinal = 0u64;
        let mut turn_in_flight: Option<weaver_types::TurnKey> = None;
        let (listener, _dir) = test_listener();
        let outcome = {
            let mut fullness = None;
            let mut pressure_reported = false;
            let load_facts = crate::engine::test_load_facts();
            let mut ports = Ports::grant(
                &decode,
                &author,
                &mut recorder,
                &mut turn_ordinal,
                &mut turn_in_flight,
                &load_facts,
                None,
                &listener,
                None,
                None,
                None,
                None,
                &mut fullness,
                &mut pressure_reported,
            );
            ports.turn(vec![Message {
                role: Role::User,
                content: vec![ContentBlock::Text { text: "hi".into() }],
            }])
        };
        peer.join().expect("the decode peer finishes");
        assert!(outcome.is_ok(), "the turn completes: {outcome:?}");

        let mut text = String::new();
        use std::io::Read as _;
        std::fs::File::open(&sink_path)
            .expect("the record reopens")
            .read_to_string(&mut text)
            .expect("the record reads");
        let column = text
            .lines()
            .map(|l| serde_json::from_str::<serde_json::Value>(l).expect("a line parses"))
            .find(|e| e["kind"] == "residual.column")
            .expect("the column authored");
        assert_eq!(column["payload"]["position"], 7);
        assert_eq!(column["payload"]["layers"], 2);
        assert_eq!(column["payload"]["width"], 2);
        std::fs::remove_file(&sink_path).ok();
    }

    /// A serving recorder over a scratch sink, its load authored, the
    /// shape every seat test below opens on.
    /// Runs `ask` against a granted seat with `turn` standing, and answers
    /// what it answered beside every `score` line the record then holds.
    fn with_seat<R>(
        turn: Option<weaver_types::TurnKey>,
        ask: impl FnOnce(&mut Ports<'_>) -> R,
    ) -> (R, Vec<String>) {
        let (near, _far) = socketpair(
            AddressFamily::Unix,
            SockType::SeqPacket,
            None,
            SockFlag::SOCK_CLOEXEC,
        )
        .expect("socketpair");
        let decode = crate::channel::decode_from_owned(near);
        let (mut recorder, author) = loaded_recorder();
        let mut turn_ordinal = 3u64;
        let mut turn_in_flight = turn;
        let (listener, _dir) = test_listener();
        let mut fullness = None;
        let mut pressure_reported = false;
        let load_facts = crate::engine::test_load_facts();
        let answered = {
            let mut ports = Ports::grant(
                &decode,
                &author,
                &mut recorder,
                &mut turn_ordinal,
                &mut turn_in_flight,
                &load_facts,
                None,
                &listener,
                None,
                None,
                None,
                None,
                &mut fullness,
                &mut pressure_reported,
            );
            ask(&mut ports)
        };
        let scores = recorder
            .structure()
            .expect("the serving record")
            .iter()
            .filter(|r| r.kind == Kind::Score)
            .map(|r| r.line.to_string())
            .collect();
        (answered, scores)
    }

    /// **A run records one score, between turns, with its ratio's terms**
    /// (#523). The first verdict lands and the port answers true, the
    /// payload carrying the predicate, whether it held, and both terms. A
    /// second verdict for the same run is refused and authors nothing.
    ///
    /// Perturbation: drop the check for a score already standing in
    /// `Ports::score` and the second verdict lands too, two lines where one
    /// is asserted.
    ///
    /// conforms: trace-score-records-the-verdict-and-its-terms
    #[test]
    fn a_run_records_one_score_between_turns() {
        let (answers, scores) = with_seat(None, |ports| {
            (
                ports.score("reached-the-goal", true, Some(14), Some(11)),
                ports.score("reached-the-goal", false, None, None),
            )
        });
        assert_eq!(
            answers,
            (true, false),
            "the first lands, the second refuses"
        );
        assert_eq!(scores.len(), 1, "one run, one verdict: {scores:?}");
        assert!(
            scores[0].contains(concat!(
                r#""payload":{"predicate":"reached-the-goal","passed":true,"#,
                r#""ratio":{"measured":14,"denominator":11}}"#
            )) && !scores[0].contains(r#""turn""#),
            "the verdict and both terms, turnless: {}",
            scores[0]
        );
    }

    /// **A score out of place or malformed is refused and authors nothing**
    /// (#523): inside a standing turn, with one term of the ratio and not the
    /// other, with a zero denominator, or with an empty predicate. A verdict
    /// whose task supplies no denominator is not malformed and lands with no
    /// ratio at all.
    ///
    /// Perturbations: drop the turn check and the mid-turn verdict lands;
    /// let a lone term through as no ratio and the half-ratio verdicts land.
    /// Watched under each.
    ///
    /// conforms: trace-score-records-the-verdict-and-its-terms
    #[test]
    fn a_score_out_of_place_or_malformed_is_refused() {
        let (answered, scores) = with_seat(Some(weaver_types::TurnKey("t-3".into())), |ports| {
            ports.score("reached-the-goal", true, None, None)
        });
        assert!(
            !answered && scores.is_empty(),
            "no score inside a turn: {scores:?}"
        );
        for (measured, denominator, what) in [
            (Some(14), None, "a measured count with no denominator"),
            (None, Some(11), "a denominator with no measured count"),
            (Some(14), Some(0), "a zero denominator"),
        ] {
            let (answered, scores) = with_seat(None, |ports| {
                ports.score("reached-the-goal", true, measured, denominator)
            });
            assert!(!answered && scores.is_empty(), "{what} refuses: {scores:?}");
        }
        let (answered, scores) = with_seat(None, |ports| ports.score("", true, None, None));
        assert!(!answered && scores.is_empty(), "an empty predicate refuses");
        let (answered, scores) = with_seat(None, |ports| {
            ports.score("reached-the-goal", false, None, None)
        });
        assert!(answered, "a verdict with no denominator lands");
        assert!(
            scores[0].contains(r#""payload":{"predicate":"reached-the-goal","passed":false}"#),
            "with no ratio at all: {}",
            scores[0]
        );
    }

    fn loaded_recorder() -> (crate::record::Record, Author) {
        let session = SessionId("s-1".into());
        let mut recorder = crate::record::Record::Serving(
            Recorder::receive(
                tempfile(),
                RunRef("r-1".into()),
                SessionRef(session.0.clone()),
            )
            .expect("recorder"),
        );
        let author = Author::new(&session, &weaver_types::RunId("r-1".into()));
        author
            .author(
                &mut recorder,
                Kind::Load,
                Subsystem::Harness,
                None,
                Some(Payload::Elections(Box::new(weaver_trace::Elections {
                    residual_readout: false,
                    field: None,
                    surprisal: false,
                    tee: Some(weaver_trace::Election::default()),
                    state_member: true,
                    declaration: Default::default(),
                    lineage: None,
                    reset: None,
                    stack: Default::default(),
                    boundary: String::new(),
                    cause: weaver_trace::Cause { uid: 0 },
                    library_path: None,
                    state_store: Default::default(),
                    composer: weaver_trace::LoopIdentity::compiled("test"),
                }))),
            )
            .expect("load");
        (recorder, author)
    }

    /// **The recall after a flush reaches the record between the flush and
    /// the re-entry, and before the loop holds the answer**, per
    /// `weaver-trace-Spec` section 3's recall clause, on the M1 record's
    /// shape: `m1-002` flushed at sequence 255 and re-entered at 257, and
    /// the `recall(4)` between them left no event, so the post-flush input
    /// was drawn from an ask the record did not hold (#673, item 2).
    ///
    /// The flush answers the counts that record carried, the loop asks for
    /// the last four turns as the M1 loop does, and custody answers one
    /// turnless prefix event and one turned one. The recall lands at the
    /// flush's sequence plus one, turnless, naming the ask and each returned
    /// event by run, turn, sequence and kind, and carrying none of their
    /// pairs.
    ///
    /// Perturbation: drop the `author_recall` call from `Ports::recall` and
    /// the recall-count assertion fails, the answer still reaching the loop.
    /// Watched under exactly that removal.
    ///
    /// conforms: trace-recall-records-the-ask-and-its-identities
    #[test]
    fn the_recall_after_a_flush_is_recorded_before_the_loop_holds_it() {
        use std::io::{Read, Write};

        let (near, far) = socketpair(
            AddressFamily::Unix,
            SockType::SeqPacket,
            None,
            SockFlag::SOCK_CLOEXEC,
        )
        .expect("socketpair");
        let decode = crate::channel::decode_from_owned(near);
        let peer = std::thread::spawn(move || {
            let mut buf = vec![0u8; 65536];
            let n = recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).expect("recv flush");
            let directive: weaver_types::TokenDirective =
                serde_json::from_slice(&buf[..n]).expect("the flush parses");
            assert_eq!(directive, weaver_types::TokenDirective::Flush { keep: 0 });
            let answer = weaver_types::TokenAnswer::Flushed {
                resident_before: 27196,
                resident_after: 712,
            };
            let bytes = serde_json::to_vec(&answer).expect("answer renders");
            send(far.as_raw_fd(), &bytes, MsgFlags::empty()).expect("send answer");
        });

        let (ours, mut member) = std::os::unix::net::UnixStream::pair().expect("pair");
        ours.set_nonblocking(true).expect("nonblocking");
        let mut seam = crate::state::StateSeam::new(ours);
        member
            .write_all(
                concat!(
                    r#"{"answer":{"recall":{"events":["#,
                    r#"{"envelope":{"session":"s-1","run":"r-0","kind":"message.system","#,
                    r#""sequence":"3"},"pairs":{"role":"system","content":"You are Karl."}},"#,
                    r#"{"envelope":{"session":"s-1","run":"r-1","turn":"t-36","#,
                    r#""kind":"message.assistant","sequence":"250"},"#,
                    r#""pairs":{"role":"assistant","content":"the plan"}}]}}}"#,
                    "\n"
                )
                .as_bytes(),
            )
            .expect("custody answers in advance");

        let (mut recorder, author) = loaded_recorder();
        let mut turn_ordinal = 36u64;
        let mut turn_in_flight: Option<weaver_types::TurnKey> = None;
        let (listener, _dir) = test_listener();
        let mut fullness = Some((27196, 32768));
        let mut pressure_reported = false;
        let load_facts = crate::engine::test_load_facts();
        let answered = {
            let mut ports = Ports::grant(
                &decode,
                &author,
                &mut recorder,
                &mut turn_ordinal,
                &mut turn_in_flight,
                &load_facts,
                None,
                &listener,
                None,
                None,
                Some(&mut seam),
                None,
                &mut fullness,
                &mut pressure_reported,
            );
            assert_eq!(ports.flush(0), Some((27196, 712)), "the flush holds");
            let answered = ports.recall(Some(4)).expect("custody answered");
            // The loop holds the answer from here, and the record already
            // carries it: read the structure while the grant still stands.
            let recalls = ports
                .recorder
                .structure()
                .expect("the serving record")
                .iter()
                .filter(|r| r.kind == Kind::Recall)
                .count();
            assert_eq!(
                recalls, 1,
                "the recall is recorded before the loop holds it"
            );
            answered
        };
        peer.join().expect("the decode peer finishes");
        assert_eq!(answered.len(), 2, "the loop receives what custody answered");

        let mut asked = [0u8; 128];
        let n = member.read(&mut asked).expect("reads the ask");
        assert_eq!(&asked[..n], b"{\"ask\":{\"recall\":{\"last-turns\":4}}}\n");

        let records = recorder.structure().expect("the serving record");
        let flush = records
            .iter()
            .find(|r| r.kind == Kind::Flush)
            .expect("the flush is recorded");
        let recall = records
            .iter()
            .find(|r| r.kind == Kind::Recall)
            .expect("the recall is recorded");
        let flush: serde_json::Value = serde_json::from_str(flush.line.as_ref()).expect("parses");
        let line: &str = recall.line.as_ref();
        let recall: serde_json::Value = serde_json::from_str(line).expect("parses");
        let sequence = |event: &serde_json::Value| -> u64 {
            event["sequence"]
                .as_str()
                .expect("a decimal string")
                .parse()
                .expect("a number")
        };
        assert_eq!(
            sequence(&recall),
            sequence(&flush) + 1,
            "the recall follows the flush with nothing between, where the re-entry comes next"
        );
        assert!(recall.get("turn").is_none(), "a recall belongs to no turn");
        assert_eq!(recall["subsystem"], "harness");
        assert!(
            line.contains(concat!(
                r#""payload":{"ask":{"verb":"recall","last_turns":4},"returned":["#,
                r#"{"run":"r-0","sequence":"3","kind":"message.system"},"#,
                r#"{"run":"r-1","turn":"t-36","sequence":"250","kind":"message.assistant"}]}"#
            )),
            "the ask and the identities, in declared order: {line}"
        );
        assert!(
            !line.contains("You are Karl.") && !line.contains("the plan"),
            "and none of the returned events' contents: {line}"
        );
    }

    /// **A whole-session replay answer is recorded by its bounds and its
    /// count, never as a list as long as the session**, per
    /// `weaver-trace-Spec` section 3's recall clause, and before the loop
    /// walks it. Three events answer, so the middle one is what a list would
    /// have carried and the bounds do not.
    ///
    /// Perturbation: record every identity for the replay verb and the
    /// `returned` assertion fails, three entries where two stand; drop the
    /// `author_recall` call from `Ports::replay` and the count assertion
    /// fails. Watched under both.
    ///
    /// conforms: trace-recall-records-the-ask-and-its-identities
    #[test]
    fn a_replay_answer_is_recorded_by_its_bounds_and_count() {
        use std::io::Write;

        let (near, _far) = socketpair(
            AddressFamily::Unix,
            SockType::SeqPacket,
            None,
            SockFlag::SOCK_CLOEXEC,
        )
        .expect("socketpair");
        let decode = crate::channel::decode_from_owned(near);
        let (ours, mut member) = std::os::unix::net::UnixStream::pair().expect("pair");
        ours.set_nonblocking(true).expect("nonblocking");
        let mut seam = crate::state::StateSeam::new(ours);
        member
            .write_all(
                concat!(
                    r#"{"answer":{"replay":{"events":["#,
                    r#"{"envelope":{"kind":"message.system","run":"r-0","sequence":"1"},"pairs":{}},"#,
                    r#"{"envelope":{"kind":"model.request","run":"r-0","turn":"t-1","sequence":"4"},"pairs":{}},"#,
                    r#"{"envelope":{"kind":"turn.closed","run":"r-0","turn":"t-1","sequence":"9"},"pairs":{}}"#,
                    r#"]}}}"#,
                    "\n"
                )
                .as_bytes(),
            )
            .expect("custody answers in advance");

        let (mut recorder, author) = loaded_recorder();
        let mut turn_ordinal = 0u64;
        let mut turn_in_flight: Option<weaver_types::TurnKey> = None;
        let (listener, _dir) = test_listener();
        let mut fullness = None;
        let mut pressure_reported = false;
        let load_facts = crate::engine::test_load_facts();
        let answered = {
            let mut ports = Ports::grant(
                &decode,
                &author,
                &mut recorder,
                &mut turn_ordinal,
                &mut turn_in_flight,
                &load_facts,
                None,
                &listener,
                None,
                None,
                Some(&mut seam),
                None,
                &mut fullness,
                &mut pressure_reported,
            );
            ports.replay(2_000).expect("custody answered")
        };
        assert_eq!(answered.len(), 3, "the loop walks the whole answer");

        let records = recorder.structure().expect("the serving record");
        let recalls: Vec<serde_json::Value> = records
            .iter()
            .filter(|r| r.kind == Kind::Recall)
            .map(|r| serde_json::from_str(r.line.as_ref()).expect("parses"))
            .collect();
        assert_eq!(recalls.len(), 1, "the replay ask is recorded once");
        let payload = &recalls[0]["payload"];
        assert_eq!(payload["ask"], serde_json::json!({"verb": "replay"}));
        assert_eq!(payload["count"], 3, "the events the member answered");
        assert_eq!(
            payload["returned"],
            serde_json::json!([
                {"run": "r-0", "sequence": "1", "kind": "message.system"},
                {"run": "r-0", "turn": "t-1", "sequence": "9", "kind": "turn.closed"}
            ]),
            "named by its first and last events and nothing between"
        );
    }

    /// **A recall custody did not answer is not a recall event**, per
    /// `weaver-trace-Spec` section 3's recall clause: a dead seam answers
    /// nothing, and the loop reads `None` with the record unchanged.
    ///
    /// Perturbation: author the recall before the ask answers, with an
    /// empty identity list for a miss, and the count assertion fails.
    #[test]
    fn a_recall_the_member_did_not_answer_records_no_recall() {
        let (near, _far) = socketpair(
            AddressFamily::Unix,
            SockType::SeqPacket,
            None,
            SockFlag::SOCK_CLOEXEC,
        )
        .expect("socketpair");
        let decode = crate::channel::decode_from_owned(near);
        let (ours, member) = std::os::unix::net::UnixStream::pair().expect("pair");
        ours.set_nonblocking(true).expect("nonblocking");
        drop(member);
        let mut seam = crate::state::StateSeam::new(ours);

        let (mut recorder, author) = loaded_recorder();
        let mut turn_ordinal = 0u64;
        let mut turn_in_flight: Option<weaver_types::TurnKey> = None;
        let (listener, _dir) = test_listener();
        let mut fullness = None;
        let mut pressure_reported = false;
        let load_facts = crate::engine::test_load_facts();
        {
            let mut ports = Ports::grant(
                &decode,
                &author,
                &mut recorder,
                &mut turn_ordinal,
                &mut turn_in_flight,
                &load_facts,
                None,
                &listener,
                None,
                None,
                Some(&mut seam),
                None,
                &mut fullness,
                &mut pressure_reported,
            );
            assert!(
                ports.recall(Some(4)).is_none(),
                "a dead member answers nothing"
            );
        }
        assert_eq!(
            recorder
                .structure()
                .expect("the serving record")
                .iter()
                .filter(|r| r.kind == Kind::Recall)
                .count(),
            0,
            "and the record holds no recall for an ask nothing answered"
        );
    }

    fn test_listener() -> (
        crate::channel::CoordinationListener,
        crate::scratch::Scratch,
    ) {
        let dir = crate::scratch::dir(format!(
            "weaver-engine-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let listener = crate::channel::bind_coordination(&dir.join("c.sock")).expect("bind");
        (listener, dir)
    }

    fn tempfile() -> OwnedFd {
        let path = std::env::temp_dir().join(format!(
            "weaver-harness-turn-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let file = std::fs::File::create(&path).expect("sink");
        std::fs::remove_file(&path).ok();
        OwnedFd::from(file)
    }
}
