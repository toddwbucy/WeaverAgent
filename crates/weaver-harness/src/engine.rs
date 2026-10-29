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

/// **A leave heard mid-turn, held for the serve loop** (`weaver-admin-Spec`
/// 3.0, S3 x unload and x force-unload): admin's connection, kept open until
/// `Left`, the exchange it is answered on, the leave's cause and kind, and
/// its progress among the table's states, from which the run's position is
/// derived.
pub(crate) struct PendingLeave {
    pub(crate) connection: OrganChannel,
    pub(crate) exchange: weaver_types::ExchangeId,
    pub(crate) cause: weaver_types::Cause,
    pub(crate) rollback: bool,
    /// **The first force that turned the still-graceful leave forced**, a
    /// join's cause, named on `unload` (`weaver-admin-Spec` section 3, the
    /// forced unload, joining); none where a bound, a dead organ or the
    /// directive did, or where no join came.
    pub(crate) forced_by: Option<weaver_types::Cause>,
    pub(crate) progress: crate::lifecycle_table::LeaveProgress,
    /// The drain's deadline, the declared drain bound from the leave's
    /// arrival (`weaver-harness-Spec` section 6, the bounds); `None` is
    /// unbounded.
    pub(crate) drain_deadline: Option<std::time::Instant>,
    /// Every `JoinLeave` held for the leave's `Left`, its connection and
    /// exchange, answered the same `Left` at the seal's step 7.
    pub(crate) joins: Vec<(OrganChannel, weaver_types::ExchangeId)>,
}

impl PendingLeave {
    /// **A force joining this leave** (`weaver-admin-Spec` section 3, the
    /// forced unload, joining; S4 to S7 x force-unload): admitted only into
    /// an unload's leave, so one a force (S10) or a rollback directed answers
    /// `OutOfOrder`. Admitted, the join turns a leave still graceful forced,
    /// naming its cause as `forced_by` where it is the first force and the
    /// outcome is not yet `fixed` (S7 on), and is held for the same `Left`.
    /// Answers whether this join turned the leave forced.
    pub(crate) fn join(
        &mut self,
        connection: OrganChannel,
        exchange: weaver_types::ExchangeId,
        cause: weaver_types::Cause,
        fixed: bool,
    ) -> bool {
        use crate::lifecycle_table::Leave;
        if self.rollback || self.progress.directed == Leave::Forced {
            let _ = connection.send(&weaver_types::OrganEnvelope {
                exchange,
                position: weaver_types::Position::Close,
                payload: weaver_types::Payload::Refusal(weaver_types::LifecycleRefusal::OutOfOrder),
            });
            return false;
        }
        let turned = !fixed && self.progress.leave == Leave::Graceful;
        if turned {
            self.progress.leave = Leave::Forced;
            self.forced_by = Some(cause);
        }
        self.joins.push((connection, exchange));
        turned
    }
}

/// The floor's cause in the record's spelling: the two crates link downward
/// only, so the harness converts at authorship.
pub(crate) fn trace_cause(cause: weaver_types::Cause) -> weaver_trace::Cause {
    weaver_trace::Cause { uid: cause.uid }
}

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
    /// The handle on `tools/` the worker kills when the gate dies with a call
    /// open (`weaver-admin-Spec` 6.1), `None` where none was opened.
    pub(crate) tools: Option<&'a crate::tools_cgroup::ToolsCgroup>,
    /// Where that kill's outcome is left for the lifecycle, which names it
    /// in the gate's death and ends service on it while serving.
    pub(crate) contained: Option<
        &'a mut Option<(
            crate::tools_cgroup::Context,
            crate::tools_cgroup::Containment,
        )>,
    >,
    /// The enter carried the shell's grant (#29): only then does a call
    /// naming the shell cross to the gate.
    pub(crate) shell_granted: bool,
}

/// The wind-down turn's user message (`weaver-admin-Spec` 3.0, graceful step
/// 3), the harness's own words, constant so the record says what was asked.
pub(crate) const WIND_DOWN_REQUEST: &str = "The agent is unloading now. Summarize the work so far and where it stands, so that it can be resumed after the agent is reloaded. Do not call any tool; a tool call will not run.";

/// **The pressure reading**, per `weaver-harness-Spec` section 4: a `fault`
/// authored once per crossing of the recorder's mark, the flag lowered by a
/// reading under it. Shared by the turn's end and the leave's, the leave
/// reading it after the release and before `unload` (section 6, item 6).
pub(crate) fn report_pressure(recorder: &mut Record, author: &Author, reported: &mut bool) {
    let pressure = recorder.pressure();
    if !pressure.over_mark {
        *reported = false;
        return;
    }
    if *reported {
        return;
    }
    *reported = true;
    let account = format!(
        "{{\"organ\":\"harness\",\"queued\":{},\"mark\":{}}}",
        pressure.queued,
        weaver_trace::HIGH_WATER_MARK
    );
    // Latched on a failure (#116): read before the turn's dialer is answered, and at the
    // leave's seal.
    let _ = author.author_fault(
        recorder,
        Subsystem::Harness,
        None,
        &crate::authorship::harness_report(
            weaver_types::FaultCase::RecorderCommitPressure,
            &account,
        ),
    );
}

/// The granted surface handed to loop 1 at loaded-and-idle, borrowing the
/// standing interior for the seat's lifetime. Its fields are private and its
/// only constructor is crate-private, which is the blade.
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
    /// Where a leave heard mid-turn is held, the run's own slot lent by the
    /// serve loop. `None` for a seat granted outside it, which refuses a
    /// leave mid-turn as not at rest, having nowhere to hold it.
    leave: Option<&'a mut Option<PendingLeave>>,
    /// A call of this turn was ended by the unload's interrupt, or never
    /// sent because the leave was heard first, so the turn closes stopped by
    /// the unload (`TurnCloses { unload_interrupted }`).
    unload_interrupted: bool,
    /// The seat runs the leave's wind-down turn (3.0, graceful step 3): no
    /// gate stands to carry a call, so a call the model asks is never sent,
    /// completing killed by the unload as one asked after the leave is.
    winding_down: bool,
    /// The deadline the turn's waits keep for the leave, a declared bound's
    /// (`weaver-harness-Spec` section 6, the bounds): the wind-down's, here.
    /// `None` is unbounded.
    leave_deadline: Option<std::time::Instant>,
    /// The leave's cause, for a wind-down its bound cuts short.
    wind_down_cause: Option<weaver_types::Cause>,
    /// The declared drain bound in seconds, lent with the leave slot, from
    /// which a leave heard mid-turn takes its drain's deadline.
    drain_bound: Option<u64>,
    /// The leave's deadline passed during the turn, which turns the leave
    /// forced from where it stands.
    deadline_passed: bool,
    /// The leave cut the turn short, its bound passing or a join turning it
    /// forced: the generation cancelled or the call interrupted.
    cut_by_leave: bool,
    /// The gate's channel hung up while the turn waited on something else
    /// (`weaver-admin-Spec` 3.0, S3 to S5 x the gate dies): no longer
    /// watched, its death the run's to author once the turn returns.
    gate_lost: bool,
    /// When the call out was interrupted (`ToolInterrupt`): **the lower's
    /// bound runs from the interrupt** (`weaver-admin-Spec` 6.1, a forced
    /// leave does not wait it out), and past it the call is closed
    /// `Killed { by: fault }` without the gate's answer.
    interrupted_at: std::cell::Cell<Option<std::time::Instant>>,
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
    /// The parse's report on every call the turn's generations attempted and
    /// could not recover, in the order they were drawn, per
    /// `weaver-harness-Spec` section 6.1 on the operator's ruling of
    /// 2026-10-13 (#31): handed to loop 1 beside the emission so a loop sees
    /// what the parse refused without parsing. What a loop does about it is
    /// state management's to design (#1).
    pub unrecovered: Vec<weaver_types::UnrecoveredCall>,
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
            leave: None,
            unload_interrupted: false,
            winding_down: false,
            leave_deadline: None,
            drain_bound: None,
            wind_down_cause: None,
            deadline_passed: false,
            cut_by_leave: false,
            gate_lost: false,
            interrupted_at: std::cell::Cell::new(None),
        }
    }

    /// Lends the run's leave slot, so a leave heard mid-turn is held there
    /// for the serve loop rather than refused (3.0, S3 x unload).
    pub(crate) fn hold_leave(
        &mut self,
        slot: &'a mut Option<PendingLeave>,
        drain_bound: Option<u64>,
    ) {
        self.leave = Some(slot);
        self.drain_bound = drain_bound;
    }

    /// Whether a leave was heard this turn and is held.
    fn leave_held(&self) -> bool {
        self.winding_down || self.leave.as_ref().is_some_and(|held| held.is_some())
    }

    /// **What runs is ended by the unload** (3.0, S4 and S5 x force-unload
    /// and x a bound passes): a call out is interrupted, answered `Killed {
    /// by: unload }`, never cancelled; with none out, the generation is
    /// cancelled. The one send a forced leave, a join and a bound share.
    fn interrupt_running(
        &self,
        turn: &TurnKey,
        execution: Option<&weaver_types::ExchangeId>,
    ) -> Result<(), TurnError> {
        if execution.is_some() && self.interrupted_at.get().is_none() {
            self.interrupted_at.set(Some(std::time::Instant::now()));
        }
        match execution {
            Some(execution) => self
                .gate
                .as_ref()
                .expect("invocation gate")
                .channel
                .send(&weaver_types::OrganEnvelope {
                    exchange: execution.clone(),
                    position: weaver_types::Position::Continue,
                    payload: weaver_types::Payload::ToolInterrupt {
                        by: weaver_types::KillCause::Unload,
                    },
                })
                .map_err(|_| TurnError::ChannelLost),
            None => self
                .decode
                .send_directive(&TokenDirective::Cancel { turn: turn.clone() })
                .map_err(|_| TurnError::ChannelLost),
        }
    }

    /// **When this turn interrupted its call out**, for the leave that
    /// follows: the lower's window and the leave's shared deadline are both
    /// measured from it (`weaver-admin-Spec` 6.1, the lower's bound runs
    /// from the interrupt), so one instant anchors the whole leave.
    pub(crate) fn interrupted_at(&self) -> Option<std::time::Instant> {
        self.interrupted_at.get()
    }

    /// The drain's deadline of a leave held while it stays graceful.
    fn held_drain_deadline(&self) -> Option<std::time::Instant> {
        let held = self.leave.as_ref()?.as_ref()?;
        (held.progress.leave == crate::lifecycle_table::Leave::Graceful)
            .then_some(held.drain_deadline)
            .flatten()
    }

    /// Whether the unload ends the turn now: a call it interrupted or never
    /// sent, or a forced leave held, which cancels the turn.
    fn unload_ends_turn(&self) -> bool {
        self.unload_interrupted
            || self.leave.as_ref().is_some_and(|held| {
                held.as_ref().is_some_and(|held| {
                    held.progress.directed == crate::lifecycle_table::Leave::Forced
                })
            })
    }

    /// The held leave's position among the table's states, for a turn in
    /// flight: draining as `S4` (a frame's turn being a serving binding's),
    /// or `S10` under a sole force.
    fn held_position(&self) -> Option<(crate::lifecycle_table::Position, weaver_types::Cause)> {
        use crate::lifecycle_table::{Binding, Leave, Position};
        let held = self.leave.as_ref()?.as_ref()?;
        let position = match held.progress.directed {
            Leave::Forced => Position::S10,
            Leave::Graceful => Position::S4(held.progress.leave, Binding::Serving),
        };
        Some((position, held.cause))
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
        report_pressure(self.recorder, self.author, self.pressure_reported);
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
    /// **Never the caller's failure, and never unseen.** The port still
    /// answers what it was going to answer, refusing to report a refusal
    /// twice over leaving the loop with neither the answer nor the record,
    /// and a refusal that could not be recorded is latched on the record
    /// (#116), so the turn's dialer or the wake's answer is never announced
    /// over it: service ends there instead.
    fn author_refusal(
        &mut self,
        asked: weaver_types::TokenAsk,
        refusal: weaver_types::TokenRefusal,
        turn: Option<&TurnKey>,
    ) {
        let record = weaver_types::RefusalRecord::Decode { asked, refusal };
        // Latched on a failure, a render's included (#116), read where the
        // turn or the wake answers.
        let _ = self.author.author_refusal(self.recorder, turn, &record);
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
        // the evidence. **The failure is latched on the record** (#116), so
        // the counts are answered to the loop and service ends before the
        // turn's dialer or the wake's answer is announced.
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
                // Latched on a failure, a render's included (#116), read before
                // the dialer is answered.
                let _ = self
                    .author
                    .author_fault(self.recorder, Subsystem::Spu, None, &report);
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
        // Latched on a failure, a render's included (#116), read where the
        // turn or the wake answers.
        let _ = self.author.author_refusal(self.recorder, None, &record);
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
        // Latched on a failure (#116), read before the dialer is answered.
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

    /// **The leave's wind-down turn** (`weaver-admin-Spec` 3.0, graceful
    /// step 3): one bracketed turn whose user message is
    /// [`WIND_DOWN_REQUEST`], authored as every turn's is, so the record
    /// carries the model's account of where the work stands for the reload.
    /// A call the model asks is never sent, the gate being drained. **Its
    /// generation is cancelled at `deadline`**, the declared wind-down bound
    /// (S5 x a bound passes); the second value says whether it was, which
    /// turns the leave forced.
    pub(crate) fn wind_down(
        &mut self,
        deadline: Option<std::time::Instant>,
        cause: weaver_types::Cause,
    ) -> (Result<TurnOutcome, TurnError>, bool) {
        self.winding_down = true;
        self.leave_deadline = deadline;
        self.wind_down_cause = Some(cause);
        let request = Message {
            role: weaver_traits::Role::User,
            content: vec![ContentBlock::Text {
                text: WIND_DOWN_REQUEST.into(),
            }],
        };
        let ran = self.bracketed(vec![request], true);
        (ran, self.cut_by_leave)
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
                        if !self.recorder.failed()
                            && let Some((exchange, _)) = stop.take()
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
        let mut unrecovered = Vec::new();
        let generation = 'rounds: loop {
            let mut generation = self.generate_once(turn, delta, stop)?;
            unrecovered.extend(generation.unrecovered.iter().cloned());
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
                || self.unload_ends_turn()
                || (self.gate.is_none() && !self.winding_down)
                || rounds >= crate::tools::MAX_TOOL_ROUNDS
            {
                break generation;
            }
            rounds += 1;
            let mut next_delta = Vec::with_capacity(calls.len());
            for call in calls {
                let grant = self.execute_call(turn, &call, stop)?;
                // A call the unload ended closes the turn there without it
                // (3.0, S4 x tool return), as a stop does.
                if stop.is_some() || self.unload_ends_turn() {
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
            unrecovered,
        })
    }

    /// **The parse's report, authored at the answer**, per
    /// `weaver-harness-Spec` section 6.1 on the operator's ruling of
    /// 2026-10-13 (#31): one `model.unrecovered_call` per entry, in its order,
    /// after `model.measurement` and before the turn's `message.assistant`,
    /// whose `UnrecoveredCall` blocks hold the same fragments. The one helper
    /// both the generation's and the re-feed's authoring call, through the
    /// one road a failed render latches on.
    fn author_unrecovered(
        &mut self,
        turn: &TurnKey,
        generation: &weaver_types::Generation,
    ) -> Result<(), TurnError> {
        for report in &generation.unrecovered {
            self.author
                .author_unrecovered_call(self.recorder, turn, report)
                .map_err(|_| TurnError::ChannelLost)?;
        }
        Ok(())
    }

    /// **One execution through the gate**, per `weaver-harness-gate-contract`
    /// section 2: the exchange opens with the call as the parse recovered it,
    /// and completes with one of the four contents, the grant constructed
    /// from the completion whichever arrived. This function holds every site
    /// that constructs a grant: that completion, and the three this crate
    /// closes in its own word (a gate's death, the leave, an ungranted tool).
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

        // **A call naming a tool the enter did not grant opens no exchange**
        // (#29, `weaver-harness-Spec` section 6): this crate closes it
        // `Refused` in its own word, naming the tool as not elected, and the
        // tool-result turn carries the refusal to the model. Nothing crosses
        // the gate, which refuses an unelected name too, the second wall.
        // Judged before the leave's branch wherever a gate stands, so a call
        // that could never have run reads `Refused` during a wind-down too,
        // never killed by the unload. A wind-down with no gate left keeps
        // the leave's word below.
        let refused = self
            .gate
            .as_ref()
            .is_some_and(|gate| !(gate.shell_granted && call.name == crate::tools::SHELL_TOOL));
        if refused {
            let outcome = weaver_types::ToolOutcome::Refused {
                reason: format!(
                    "{} is not a tool this agent's declaration elects - nothing ran",
                    call.name
                ),
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

        // **A call asked after the leave was heard is never sent** (3.0, S4 x
        // tool return): it completes killed by the unload in this crate's own
        // word, never finished and re-runnable at the reload.
        if self.leave_held() {
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
            self.unload_interrupted = true;
            return Ok(crate::tools::ToolResult::granted(&outcome));
        }
        self.interrupted_at.set(None);
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
            // **Past the lower's bound from the interrupt the call is closed
            // without the gate** (`weaver-admin-Spec` 6.1, a forced leave's
            // reap past the lower's bound): `Killed { by: fault }`, its
            // outcome indeterminate and never re-run, `tools/` killed once
            // and not waited on, the leave going on to the lower, which
            // reaps the gate by `SIGKILL`. The gate's late answer, if any,
            // meets the lower.
            if matches!(wake, ExchangeWake::InterruptBound) {
                let gate = self.gate.as_mut().expect("invocation gate");
                if let Some(tools) = gate.tools {
                    let context = crate::tools_cgroup::Context::Leave;
                    let contained = crate::tools_cgroup::contain(
                        tools,
                        context,
                        crate::tools_cgroup::CONTAINMENT_BOUND,
                    );
                    if let Some(slot) = gate.contained.as_mut() {
                        **slot = Some((context, contained));
                    }
                }
                // The interrupt's instant stays for the leave's lower, whose
                // window it already spent: the gate is reaped there at once.
                self.unload_interrupted = true;
                let outcome = weaver_types::ToolOutcome::Killed {
                    partial: None,
                    by: weaver_types::KillCause::Fault,
                };
                let completed =
                    serde_json::to_string(&outcome).map_err(|_| TurnError::ChannelLost)?;
                let completed =
                    weaver_trace::raw_payload(&completed).ok_or(TurnError::ChannelLost)?;
                // The harness's own word: the gate never answered.
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
            if !matches!(wake, ExchangeWake::Work) {
                self.handle_turn_control(wake, turn, stop, Some(&exchange))?;
                continue;
            }
            let gate = self.gate.as_mut().expect("invocation gate");
            let envelope = match gate.channel.recv() {
                Ok(envelope) => envelope,
                // **A call open at a gate that died is closed killed by the
                // fault** (`weaver-admin-Spec` 3.0, S3, S4, S5 and S10 x the
                // gate dies; `weaver-harness-gate-contract` section 5): its
                // outcome indeterminate and never re-run automatically.
                // **The worker kills `tools/` first** (6.1, the worker's
                // rows): serving, it waits a full bound from its own kill
                // before it closes the call; with a leave held, it kills and
                // does not wait. The outcome is left for the lifecycle.
                Err(_) => {
                    let context = if self.leave.as_ref().is_some_and(|slot| slot.is_some()) {
                        crate::tools_cgroup::Context::Leave
                    } else {
                        crate::tools_cgroup::Context::Serving
                    };
                    // Only a gate that hung up is dead: a live gate's
                    // unreadable frame ends service without a kill, the
                    // unwind's lower sweeping what remains.
                    let dead = {
                        use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
                        use std::os::fd::AsFd;
                        let mut fds = [PollFd::new(gate.channel.as_fd(), PollFlags::POLLIN)];
                        matches!(poll(&mut fds, PollTimeout::ZERO), Ok(n) if n > 0)
                            && fds[0].revents().is_some_and(|revents| {
                                revents.intersects(PollFlags::POLLHUP | PollFlags::POLLERR)
                            })
                    };
                    if let Some(tools) = gate.tools.filter(|_| dead) {
                        let contained = crate::tools_cgroup::contain(
                            tools,
                            context,
                            crate::tools_cgroup::CONTAINMENT_BOUND,
                        );
                        if let Some(slot) = gate.contained.as_mut() {
                            **slot = Some((context, contained));
                        }
                    }
                    let outcome = weaver_types::ToolOutcome::Killed {
                        partial: None,
                        by: weaver_types::KillCause::Fault,
                    };
                    if let Some(completed) = serde_json::to_string(&outcome)
                        .ok()
                        .and_then(|completed| weaver_trace::raw_payload(&completed))
                    {
                        // Best-effort: the turn ends on the loss returned below.
                        let _ = self.author.author(
                            self.recorder,
                            Kind::ToolCallCompleted,
                            Subsystem::Harness,
                            Some(turn),
                            Some(Payload::Deferred(completed)),
                        );
                    }
                    return Err(TurnError::ChannelLost);
                }
            };
            if envelope.exchange == exchange && envelope.position == weaver_types::Position::Close {
                match envelope.payload {
                    weaver_types::Payload::ToolAnswer(outcome) => {
                        // Only a call the interrupt actually ended is the
                        // unload's; a result that crossed it stands.
                        if matches!(
                            outcome,
                            weaver_types::ToolOutcome::Killed {
                                by: weaver_types::KillCause::Unload,
                                ..
                            }
                        ) {
                            self.unload_interrupted = true;
                        }
                        break outcome;
                    }
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
                // Latched on a failure (#116), read before the dialer is answered.
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
            // **The leave's bound passed** (S4 and S5 x a bound passes):
            // the generation is cancelled as a stop's is, a call out
            // interrupted, and the leave turns forced from where it stands.
            // **The gate died while the turn waited on the SPU** (3.0, S3 to
            // S5 x the gate dies): with no leave held the turn goes on, its
            // answer lost and the death authored once it returns (S3); a
            // leave held still graceful turns forced at that moment (A),
            // the turn cancelled (S4, S5).
            ExchangeWake::GateLost => {
                self.gate_lost = true;
                let turned = match self.leave.as_deref_mut() {
                    Some(Some(held))
                        if held.progress.leave == crate::lifecycle_table::Leave::Graceful =>
                    {
                        held.progress.leave = crate::lifecycle_table::Leave::Forced;
                        true
                    }
                    _ => false,
                };
                if turned {
                    self.unload_interrupted = true;
                    self.cut_by_leave = true;
                    self.interrupt_running(turn, execution)?;
                }
            }
            ExchangeWake::Deadline => {
                self.deadline_passed = true;
                self.cut_by_leave = true;
                // A held leave turns forced from where it stands, the turn
                // ending by the unload (S4 x a bound passes).
                if let Some(Some(held)) = self.leave.as_deref_mut() {
                    held.progress.leave = crate::lifecycle_table::Leave::Forced;
                    self.unload_interrupted = true;
                }
                self.interrupt_running(turn, execution)?;
            }
            // Taken by `execute_call` before it reaches here.
            ExchangeWake::InterruptBound => {}
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
                // **No answer to admin mid-turn over a latched record**
                // (#116): the turn ends on the loss, its one close authored
                // by the bracket, and the frame path ends service.
                if self.recorder.failed() {
                    return Err(TurnError::ChannelLost);
                }
                // Read before the slot is borrowed: whether a leave is
                // pending, and whether one is held here to join.
                let in_transition = self.leave_held();
                let joinable = self.leave.as_ref().is_some_and(|held| held.is_some());
                let Some(slot) = self.pending.as_deref_mut() else {
                    return Ok(());
                };
                let Some(connection) = slot.as_ref() else {
                    return Ok(());
                };
                match crate::lifecycle::receive_entered(connection) {
                    Ok(Some(envelope)) => {
                        let exchange = envelope.exchange.clone();
                        match envelope.payload {
                            weaver_types::Payload::Directive(
                                weaver_types::LifecycleDirective::Stop { cause },
                            ) if stop.is_none() && !in_transition => {
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
                            // **A leave pending answers `InTransition`**
                            // (`weaver-types-Spec` section 3.1): the run is
                            // between states, and no state is claimed.
                            weaver_types::Payload::Directive(
                                weaver_types::LifecycleDirective::Observe,
                            ) if in_transition => {
                                let _ = connection.send(&weaver_types::OrganEnvelope {
                                    exchange,
                                    position: weaver_types::Position::Close,
                                    payload: weaver_types::Payload::Answer(
                                        weaver_types::LifecycleAnswer::InTransition {
                                            constituents: Vec::new(),
                                        },
                                    ),
                                });
                            }
                            weaver_types::Payload::Directive(
                                weaver_types::LifecycleDirective::JoinLeave { cause },
                            ) if joinable => {
                                let connection = slot.take().expect("the join arrived on it");
                                let held = self
                                    .leave
                                    .as_deref_mut()
                                    .and_then(Option::as_mut)
                                    .expect("a leave is held");
                                // **The join cancels what runs**, as a
                                // forced leave does (S4 and S5 x
                                // force-unload): a call out interrupted, the
                                // generation cancelled.
                                if held.join(connection, exchange, cause, false) {
                                    self.unload_interrupted = true;
                                    self.cut_by_leave = true;
                                    self.interrupt_running(turn, execution)?;
                                }
                            }
                            weaver_types::Payload::Directive(
                                weaver_types::LifecycleDirective::Observe,
                            ) => {
                                let _ = connection.send(&weaver_types::OrganEnvelope {
                                    exchange,
                                    position: weaver_types::Position::Close,
                                    payload: weaver_types::Payload::Answer(
                                        weaver_types::LifecycleAnswer::State {
                                            plan: None,
                                            state: weaver_types::AgentState::Active,
                                            load: Some(Box::new(self.load.clone())),
                                            constituents: Vec::new(),
                                        },
                                    ),
                                });
                            }
                            // **A leave heard mid-turn is held** (3.0, S3 x
                            // unload and x force-unload), where the serve
                            // loop lent its slot: admin's connection moves
                            // into the held leave, answered at `Left`. A
                            // forced leave cancels the turn as a stop does,
                            // a call out interrupted rather than cancelled
                            // (`ToolInterrupt`, answered `Killed { by:
                            // unload }`); a graceful one interrupts a call
                            // out and lets the turn go on.
                            weaver_types::Payload::Directive(
                                weaver_types::LifecycleDirective::Leave {
                                    cause,
                                    forced,
                                    rollback,
                                },
                            ) if stop.is_none()
                                && self.leave.as_ref().is_some_and(|held| held.is_none()) =>
                            {
                                use crate::lifecycle_table::{
                                    Leave, LeaveProgress, LeaveStep, leave_on_arrival,
                                };
                                let directed = if forced {
                                    Leave::Forced
                                } else {
                                    Leave::Graceful
                                };
                                let connection = slot.take().expect("the leave arrived on it");
                                // A call out is interrupted by any leave; a
                                // forced one with none out cancels the turn.
                                if execution.is_some() || forced {
                                    self.interrupt_running(turn, execution)?;
                                }
                                if let Some(held) = self.leave.as_deref_mut() {
                                    *held = Some(PendingLeave {
                                        connection,
                                        exchange,
                                        cause,
                                        rollback,
                                        forced_by: None,
                                        progress: LeaveProgress {
                                            directed,
                                            // The turn runs, so the gate and
                                            // the SPU stand: (A) is met at
                                            // rest, by the serve loop.
                                            leave: leave_on_arrival(directed, false),
                                            step: LeaveStep::Draining,
                                        },
                                        drain_deadline: crate::lifecycle::deadline_after(
                                            self.drain_bound,
                                        ),
                                        joins: Vec::new(),
                                    });
                                }
                            }
                            // A leave and a save point are refused as not
                            // at rest; a second stop and everything else
                            // are out of order for a turn in flight,
                            // refused and not queued.
                            // A save point is taken at rest, per the
                            // admin-harness contract section 3: mid-turn it
                            // is refused as not at rest, which tells the
                            // caller to retry, and never `OutOfOrder`, which
                            // reads as a run that takes none (#99 area 3, C2).
                            // **Meeting a pending leave they are `OutOfOrder`**
                            // (`weaver-harness-Spec` section 6), as the
                            // leave's sweeps answer them at rest; a stop
                            // meeting one falls to the arm below the same.
                            weaver_types::Payload::Directive(
                                weaver_types::LifecycleDirective::Leave { .. }
                                | weaver_types::LifecycleDirective::SavePoint { .. },
                            ) => {
                                let _ = connection.send(&weaver_types::OrganEnvelope {
                                    exchange,
                                    position: weaver_types::Position::Close,
                                    payload: weaver_types::Payload::Refusal(if in_transition {
                                        weaver_types::LifecycleRefusal::OutOfOrder
                                    } else {
                                        weaver_types::LifecycleRefusal::ActivityNotAtRest
                                    }),
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
                    Ok(None) => {}
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
                // This retiring engine sent canonical messages alone. Keep
                // those rendered bytes until the foundation's placement
                // owner supplies its exact measured proposal and schemas.
                tool_schemas: Vec::new(),
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

        self.author_unrecovered(turn, &generation)?;

        // The assistant's turn enters the record as a message event, the
        // canonical parse beside the verbatim the output holds: the family
        // module's own bridge crossed the seam in the generation, text as
        // text, every recovered call as a `ToolCall` block, and every call
        // it could not recover as a marked `UnrecoveredCall` block, per the
        // tool workflow's opening act and #31.
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
                // A re-feed keeps no leave deadline; one met is spent.
                ExchangeWake::Deadline => self.deadline_passed = true,
                // A diagnostic replay stands no gate; a hang-up is not
                // watched again.
                ExchangeWake::GateLost => self.gate_lost = true,
                // Only an execution waits on the interrupt's bound.
                ExchangeWake::InterruptBound => {}
                // The coordination channel is heard while the re-feed
                // streams, exactly as a generation hears it. Observe reads
                // the running replay's facts without changing its bracket.
                // Other directives retain the existing refusal: replay has no stop
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
                    // No answer to admin over a latched record (#116).
                    if self.recorder.failed() {
                        return Err(TurnError::ChannelLost);
                    }
                    let Some(slot) = self.pending.as_deref_mut() else {
                        continue;
                    };
                    let Some(connection) = slot.as_ref() else {
                        continue;
                    };
                    match crate::lifecycle::receive_entered(connection) {
                        Ok(Some(envelope)) => {
                            let exchange = envelope.exchange.clone();
                            match envelope.payload {
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
                                                plan: None,
                                                constituents: Vec::new(),
                                            },
                                        ),
                                    });
                                }
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
                        Ok(None) => {}
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
        self.author_unrecovered(turn, &generation)?;
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
        // **Under a held leave the close is the lifecycle table's** (3.0, S4
        // and S10 x turn closes): clean where the turn finished, stopped by
        // the unload with the leave's cause where the unload ended it.
        let held = if self.winding_down {
            None
        } else {
            self.held_position()
        };
        let close = match held {
            // **The wind-down closes to S6** (3.0, S5 x turn closes): its
            // calls never sent and recorded interrupted, the turn itself
            // finished, so the close is clean.
            None if self.winding_down => {
                use crate::lifecycle_table::{Event, Position, Response, respond};
                debug_assert_eq!(
                    respond(
                        Position::S5,
                        Event::TurnCloses {
                            unload_interrupted: self.unload_interrupted,
                        },
                    ),
                    Response::WindDownClosesToS6
                );
                // A wind-down its bound or a join cut short is a stop
                // naming the unload, with the leave's cause, the graceful
                // caller's.
                if self.cut_by_leave {
                    TurnClose::Stopped {
                        reason: weaver_trace::StopReason::Unload,
                        cause: self.wind_down_cause.map(trace_cause),
                    }
                } else {
                    TurnClose::Clean
                }
            }
            Some((position, cause)) => {
                use crate::lifecycle_table::{Event, Response, respond};
                match respond(
                    position,
                    Event::TurnCloses {
                        unload_interrupted: self.unload_interrupted,
                    },
                ) {
                    Response::CloseStoppedUnloadToS5 | Response::CancelTurnNoWindDown => {
                        TurnClose::Stopped {
                            reason: weaver_trace::StopReason::Unload,
                            cause: Some(trace_cause(cause)),
                        }
                    }
                    // Clean where the turn finished; no other cell answers
                    // a turn's close in S4 or S10.
                    _ => TurnClose::Clean,
                }
            }
            None if aborted => TurnClose::Stopped {
                reason: weaver_trace::StopReason::Directive,
                cause: stop.as_ref().map(|(_, cause)| trace_cause(*cause)),
            },
            None => TurnClose::Clean,
        };
        self.author
            .author(
                self.recorder,
                Kind::TurnClosed,
                Subsystem::Harness,
                Some(turn),
                Some(Payload::TurnClosed(close)),
            )
            .map_err(|_| TurnError::ChannelLost)?;
        // The close landed, so no turn stands.
        *self.turn_in_flight = None;

        // **Announce after record**: the stop's answer follows the close it
        // reports, carrying the turn's fate, aborted or completed-at-rest,
        // both truthful at the moment of answering. **Not over a record
        // missing an event** (#116): the latch is read first, and a latched
        // record leaves the stop unanswered, the frame path ending service
        // on the same read.
        if !self.recorder.failed()
            && let Some((exchange, _)) = stop.take()
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
        use nix::poll::{PollFd, PollFlags, poll};
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
            // **The gate is watched for its death** while the turn waits on
            // the SPU (3.0, S3 to S5 x the gate dies): no events asked, so a
            // client's frame queued there wakes nothing and a hang-up does.
            // During an execution the gate is the work and is read already.
            if let Some(gate) = self.gate.as_ref()
                && !self.gate_lost
                && std::os::fd::AsRawFd::as_raw_fd(&gate.channel.as_fd())
                    != std::os::fd::AsRawFd::as_raw_fd(&work)
            {
                fds.push(PollFd::new(gate.channel.as_fd(), PollFlags::empty()));
                wakes.push(ExchangeWake::GateLost);
            }
            // **The leave's deadline bounds the wait** where one stands and
            // has not passed: the wind-down's, or a held leave's drain while
            // it stays graceful. Past it the wake is the deadline's.
            let deadline = self.leave_deadline.or_else(|| self.held_drain_deadline());
            let deadline = deadline.filter(|_| !self.deadline_passed);
            // **The interrupted call's bound**, while the gate is the work.
            let interrupt = self
                .interrupted_at
                .get()
                .filter(|_| {
                    self.gate.as_ref().is_some_and(|gate| {
                        std::os::fd::AsRawFd::as_raw_fd(&gate.channel.as_fd())
                            == std::os::fd::AsRawFd::as_raw_fd(&work)
                    })
                })
                .and_then(|at| {
                    at.checked_add(std::time::Duration::from_millis(
                        weaver_types::LOWER_BOUND_MS,
                    ))
                });
            if interrupt.is_some_and(|d| std::time::Instant::now() >= d) {
                return Ok(ExchangeWake::InterruptBound);
            }
            let deadline = match (deadline, interrupt) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            };
            // **The clock is read before every wait** (Codex on #119, round
            // 1): a generation that keeps its channel readable never lets the
            // wait time out, and it is the one the bound exists to cut.
            if deadline.is_some_and(|d| std::time::Instant::now() >= d) {
                return Ok(ExchangeWake::Deadline);
            }
            let wait = crate::lifecycle::poll_timeout_until(deadline);
            match poll(&mut fds, wait) {
                Ok(0) if interrupt.is_some_and(|d| std::time::Instant::now() >= d) => {
                    return Ok(ExchangeWake::InterruptBound);
                }
                Ok(0) if deadline.is_some_and(|d| std::time::Instant::now() >= d) => {
                    return Ok(ExchangeWake::Deadline);
                }
                Ok(_) => {}
                Err(nix::errno::Errno::EINTR) => continue,
                Err(_) => return Err(TurnError::ChannelLost),
            }
            let woken = PollFlags::POLLIN | PollFlags::POLLHUP | PollFlags::POLLERR;
            if fds.iter().any(|fd| {
                fd.revents()
                    .is_some_and(|revents| revents.contains(PollFlags::POLLNVAL))
            }) {
                return Err(TurnError::ChannelLost);
            }
            // **The lifecycle wakes are scanned before the work** (Codex on
            // #119, round 3; 3.0, S3 x force-unload): an SPU that keeps its
            // channel readable must not starve a directive, a dial or the
            // gate's death, the work being taken once none of them is ready.
            let ready = |wake: &ExchangeWake, fd: &PollFd<'_>| {
                fd.revents()
                    .is_some_and(|revents| revents.intersects(woken))
                    && !matches!(wake, ExchangeWake::Work)
            };
            if let Some((_, wake)) = fds.iter().zip(&wakes).find(|(fd, wake)| ready(wake, fd)) {
                return Ok(*wake);
            }
            for (fd, wake) in fds.iter().zip(&wakes) {
                if fd
                    .revents()
                    .is_some_and(|revents| revents.intersects(woken))
                {
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
    /// The leave's deadline passed.
    Deadline,
    /// The gate's channel hung up.
    GateLost,
    /// The lower's bound passed from the interrupt of the call out, the
    /// gate not having answered it.
    InterruptBound,
}

/// The load facts a test lends the seat, where no enter built them.
// Test-only hook kept here (#118): the engine and replay suites reach it as `crate::engine`.
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
        drain_bound: None,
        wind_down_bound: None,
    }
}

#[cfg(test)]
mod tests;
