//! conforms: harness-idle-report-authors-without-a-turn
//! conforms: harness-one-constructor
//! conforms: harness-failed-set-refuses-construction
//! conforms: harness-channel-state-three-positions
//! conforms: harness-out-of-order-refused
//! conforms: harness-run-state-options-checked-unwind
//! conforms: harness-spu-channels-one-field
//! conforms: harness-decode-end-own-type
//! conforms: harness-one-sink-descriptor
//! conforms: harness-decode-pair-created-before-the-fork
//! conforms: harness-gate-pair-waits-on-residency
//! conforms: harness-diagnostic-enter-forks-no-gate
//! conforms: harness-scoped-refusal-account
//! conforms: harness-restoring-open-seats-the-record
//! conforms: harness-left-follows-drain
//! conforms: harness-announce-after-record
//! conforms: harness-dumpable-flag-cleared
//! conforms: harness-binds-coordination-socket-first
//! conforms: harness-loop-zero-takes-no-abstraction
//! conforms: harness-spawns-no-thread
//! conforms: harness-organ-forks-on-worker-lifetime-thread
//! conforms: harness-internal-dependency-set
//! conforms: harness-session-opens-at-enter
//! conforms: harness-frame-grants-the-seat
//! conforms: harness-parse-refuses-not-faults
//!
//! The lifecycle interior, per `weaver-harness-Spec` section 3: the harness
//! type, the run state, and the fan-out of loop 0.
//!
//! **Loop 0 takes neither a type nor a trait**, and the cell closes here: the
//! loop is the interval between two directives, its state is the [`Run`]
//! struct, and its control flow is the serial service below, so an
//! abstraction would have no second implementor and no caller that varies.
//! This crate spawns no thread - the one auxiliary thread the merged set
//! names is the stream writer's, and it belongs to `weaver-trace` - so the
//! organ forks run on a thread whose lifetime is the worker's, which is the
//! timing guarantee the gate's parent-death backing relies on.

use std::os::fd::OwnedFd;
use std::path::PathBuf;

use weaver_trace::{Kind, Payload, Recorder, RunRef, SessionRef, Subsystem, TurnClose};

use crate::engine::PendingLeave;
use weaver_types::{
    ExchangeId, LifecycleAnswer, LifecycleDirective, LifecycleRefusal, Opener, OrganEnvelope,
    Position, SessionId, TokenAnswer, TokenDirective, TurnKey,
};

use crate::authorship::Author;
use crate::channel::{CoordinationListener, DecodeChannel, OrganChannel};
use crate::failure::{AdoptionFault, ChannelFault, Outcome};

/// An organ whose channel the run met closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OrganDeath {
    Gate,
    Spu,
}

/// What the entered-state wait woke on, one of the four descriptors section
/// 6.2 spans. The tag is carried beside its `PollFd` so the wake names its
/// origin without re-deriving it from a position.
#[derive(Clone, Copy)]
enum Wake {
    Listener,
    Connection,
    Gate,
    Decode,
}

/// What a request line parsed to, per `weaver-gate-Spec` section 4 and the
/// seeding clause of `weaver-harness-Spec` section 6.1: the text, and whether
/// the line named the `system` role, which makes it the operator's seeding
/// line where the dialer is the operator.
#[derive(Debug)]
struct Request {
    text: String,
    system: bool,
}

/// The request's parse, per `weaver-gate-Spec` section 4 and the seeding
/// clause of `weaver-harness-Spec` section 6.1: one JSON object, one `text`
/// member, a string, and beside it at most `role` with the one value
/// `system`; an unknown member, a repeated member and any other role refuse,
/// the refusal naming the role. **The line decodes typed**, as the floor's
/// `TurnRequest`, because a value's map collapses a repeated member to its
/// last spelling and would read `{"text":"x","role":"user","role":"system"}`
/// as the seeding line. Who may send a `system` line is the frame path's
/// judgment, made against the frame's dialer and not here. Every failure is
/// the refused turn's reason, content the harness authors, never a channel
/// fault, which is the layer split the frame election bought.
fn parse_request(frame: &weaver_types::TurnFrame) -> Result<Request, String> {
    let Some(octets) = frame.octets() else {
        return Err("the frame's carriage is not the canonical encoding".into());
    };
    let request = weaver_types::TurnRequest::parse(&octets)?;
    let system = match request.role {
        None => false,
        Some(serde_json::Value::String(role)) if role == "system" => true,
        Some(role) => {
            return Err(format!(
                "a request's role is system or absent, and {role} is neither"
            ));
        }
    };
    Ok(Request {
        text: request.text,
        system,
    })
}

/// One response line, the kind-named close of `weaver-gate-Spec` section 4:
/// `answered` with `text`, `stopped` or `refused` with `reason`. The map
/// orders keys, so the kind leads whatever the member.
/// The close a client receives, per `weaver-gate-world-contract` section 3.
///
/// **`named` is the turn this close answers and the run it belongs to, and it
/// is absent where no turn opened.** A frame that does not parse is refused
/// before the seat is granted, and a prompt with a hole closes before it, so
/// there is no turn for those to identify and the close says only what it is.
/// A turn that opened carries its key whatever became of it. The members take
/// the trace envelope's own names so a consumer holding both joins on what it
/// already reads.
fn render_close(
    kind: &str,
    member: &str,
    value: &str,
    named: Option<(&TurnKey, &weaver_types::RunId)>,
) -> String {
    render_close_with_finish(kind, member, value, named, false)
}

/// The world contract's one optional member: an answered close whose
/// generation was cut at the turn's token limit carries `finish:
/// "length"`, and carries it only then, so a client renders a truncated
/// answer as truncated and pays nothing on the whole ones.
fn render_close_with_finish(
    kind: &str,
    member: &str,
    value: &str,
    named: Option<(&TurnKey, &weaver_types::RunId)>,
    truncated: bool,
) -> String {
    let mut map = serde_json::Map::new();
    map.insert(
        "kind".to_string(),
        serde_json::Value::String(kind.to_string()),
    );
    map.insert(
        member.to_string(),
        serde_json::Value::String(value.to_string()),
    );
    if truncated {
        map.insert(
            "finish".to_string(),
            serde_json::Value::String("length".to_string()),
        );
    }
    if let Some((turn, run)) = named {
        map.insert(
            "turn".to_string(),
            serde_json::Value::String(turn.0.clone()),
        );
        map.insert("run".to_string(), serde_json::Value::String(run.0.clone()));
    }
    serde_json::Value::Object(map).to_string()
}

/// Author the record's `refusal` for a lifecycle refusal that reached a
/// standing run, per `weaver-harness-Spec` section 6.
///
/// **The run stays entered and this is not a close.** A refused enter leaves
/// a run in place holding what the fan-out forked, and what ends the bracket
/// is the operator's `leave` arriving later, so this authors an event inside
/// a standing run rather than a last word before teardown.
///
/// **The ask is named and not reproduced.** `LifecycleAsk::Enter` says which
/// directive was refused, and the declaration it carried is not copied in:
/// the load event already holds the run's posture, and reproducing a
/// declaration here would be one fact in two places, per
/// `weaver-types-Spec` section 4's rule for the record.
///
/// A refusal that could not be recorded is latched on the record (#116), and
/// the answer to admin that follows reads the latch and ends service rather
/// than announcing over the missing refusal.
fn author_lifecycle_refusal(run: &mut Run, refusal: &LifecycleRefusal) {
    author_refusal_on(run, weaver_types::LifecycleAsk::Enter, refusal);
}

/// A lifecycle refusal on the record, naming the ask it answered.
fn author_refusal_on(run: &mut Run, asked: weaver_types::LifecycleAsk, refusal: &LifecycleRefusal) {
    let record = weaver_types::RefusalRecord::Lifecycle {
        asked,
        refusal: refusal.clone(),
    };
    // Latched on a failure, a render's included (#116): an answer to admin
    // reads the latch first, and the leave's seal refuses on it.
    let _ = run.author.author_refusal(&mut run.recorder, None, &record);
}

/// **A reset's reason as the record writes it: the wire's own spelling**
/// (#99 area 3, C5), `no-clean-unload` or `forced-unload`, taken from the
/// enum's serde so the record and the enter cannot drift apart. A unit
/// variant always renders as a string; the debug name stands only where it
/// would not.
fn reset_reason(reason: weaver_types::ResetReason) -> String {
    match serde_json::to_value(reason) {
        Ok(serde_json::Value::String(spelled)) => spelled,
        _ => format!("{reason:?}"),
    }
}

/// A decode refusal rendered as the stopped close's reason, content for the
/// client rather than the wire case itself.
///
/// **Derived from the recorded refusal rather than matched in parallel**,
/// per `weaver-types-Spec` section 4's conversion rule: a conversion may
/// narrow a refusal for a reader and may not be the only record of it. The
/// same value that reaches the record reaches this function, so the client's
/// sentence cannot say something the record does not. It stays lossy, a
/// client being owed a sentence rather than four integers, and what it stops
/// being is independent.
fn refusal_reason(refusal: &weaver_types::TokenRefusal) -> &'static str {
    match refusal {
        weaver_types::TokenRefusal::NotOpen => "the session is not open",
        weaver_types::TokenRefusal::OutOfOrder => "the ask was out of order for the seam",
        weaver_types::TokenRefusal::Overflow { .. } => "the session cannot take the delta",
        weaver_types::TokenRefusal::MalformedDelta => "the delta was malformed for the family",
        // The span's own numbers are the loop's to read off the refusal, so
        // this rendering says the kind and leaves the arithmetic where a
        // reader can act on it rather than flattening it into prose.
        weaver_types::TokenRefusal::UnremovableSpan { .. } => "the elision named no removable span",
        weaver_types::TokenRefusal::RefeedPermissionAbsent => {
            "the re-feed permission was not granted"
        }
        weaver_types::TokenRefusal::RefeedPathEmpty => "the re-feed carried no token path",
        weaver_types::TokenRefusal::ColumnPermissionAbsent => {
            "the column permission was not granted"
        }
        weaver_types::TokenRefusal::ColumnReadoutUnelected => {
            "the column ask stood without the readout election"
        }
        weaver_types::TokenRefusal::ColumnUndeclared => "the family's declaration holds no column",
    }
}

/// Where the organ binaries live: a deployment fact supplied by the
/// composition root as a construction parameter, the way `weaver-trace-Spec`
/// section 6 takes its queue depth. This is the one exception to the
/// no-path-taken rule of section 2.3, and it is not an operator election and
/// not a discovery.
#[derive(Debug, Clone)]
pub struct OrganBinaries {
    pub spu: PathBuf,
    pub gate: PathBuf,
    /// The classify process's binary, `weaver-spu-classify`, per
    /// `weaver-spu-Spec` section 11: optional because the arm is, and a
    /// declaration carrying the binding with no binary provisioned refuses
    /// the load rather than standing half an arm.
    pub classify: Option<PathBuf>,
}

/// The construction parameters this worker hands its organs, per
/// `weaver-harness-Spec` section 2.2.
///
/// **A host's facts and not an agent's.** What belongs here is a number two
/// agents sharing this host cannot sensibly disagree about. A number that is a
/// property of the agent reaches its organ in the declaration instead, which is
/// why the context capacity and the per-turn ceiling are not here.
///
/// Carried as the string a deployment wrote rather than parsed on the way
/// through: this crate hands it on and the organ's composition root is what
/// judges it, so a bad value is refused by the crate that knows what the
/// parameter means rather than twice.
#[derive(Debug, Clone, Default)]
pub struct OrganParameters {
    /// The admission headroom, in bytes, if this deployment supplies one. An
    /// organ given none keeps its own compiled default.
    pub headroom_bytes: Option<String>,
}

impl OrganParameters {
    /// The SPU's vector. Named flags rather than positions, because a
    /// deployment supplying one parameter and not another must not have to
    /// know the order, and an organ meeting an unknown flag says which.
    pub fn spu_arguments(&self) -> Vec<String> {
        let mut arguments = Vec::new();
        if let Some(headroom) = &self.headroom_bytes {
            arguments.push("--headroom-bytes".to_string());
            arguments.push(headroom.clone());
        }
        arguments
    }
}

#[cfg(test)]
mod parameter_tests;

/// The coordination channel's state: three positions, the last terminal, and
/// the middle one carrying the run. A directive out of order for the state
/// reaches a match arm rather than a flag check, which is what the type buys.
enum ChannelState {
    BeforeEnter,
    Entered(Box<Run>),
    Left,
}

/// The fan-out's progress held as data, which is what makes the unwind total.
/// Each `Option` is an arm of the enter fan-out that has or has not stood up,
/// so a leave arriving after a refused enter unwinds exactly what stands, and
/// the compiler's match on the options is what makes a forgotten arm
/// unrepresentable rather than unlikely.
struct Run {
    recorder: crate::record::Record,
    author: Author,
    session: SessionId,
    /// The operator's uid as the enter carried it, per `weaver-types-Spec`
    /// section 4 on the operator's ruling of 2026-10-06: the one dialer whose
    /// `system` line the frame path admits as the seeding turn, per
    /// `weaver-harness-Spec` section 6.1.
    operator: u32,
    /// The rendered size of this residency's seedings so far, as the state
    /// seam's `identity` answer would carry them, per `weaver-harness-Spec`
    /// section 6.1: the seam's answer bound spent at the sender, so a
    /// seeding whose addition would carry that answer past the bound
    /// refuses rather than seating a prefix the next load cannot read.
    seeded_bytes: usize,
    /// The run's own reference, retained so a close can name the run a turn
    /// belongs to, per `weaver-gate-world-contract` section 3. The author
    /// holds its own converted copy for the record, and this is the floor's,
    /// carried rather than rebuilt.
    run: weaver_types::RunId,
    spu: Option<SpuChannels>,
    gate: Option<GateChannel>,
    /// The handle on the agent's `tools/` cgroup, opened once at the enter
    /// before the gate's fork and handed it at descriptor 4
    /// (`weaver-harness-Spec` section 2.2; `weaver-admin-Spec` section 6.1,
    /// #111). Held by the run, not the gate's arm, so it outlives the gate
    /// for the kill at its death and at the unwind.
    tools: Option<crate::tools_cgroup::ToolsCgroup>,
    /// The engine's kill of `tools/` when the gate died with a call open,
    /// taken by the close that names the gate's death.
    gate_contained: Option<(
        crate::tools_cgroup::Context,
        crate::tools_cgroup::Containment,
    )>,
    /// The state seam's ask end, per `weaver-harness-Spec` section 6: a
    /// clone of the standing state channel, granted to the seat as the
    /// state port. `None` where the leg is not standing, which the port
    /// serves as the same absence a missing answer does.
    state: Option<crate::state::StateSeam>,
    /// The store's grant surface as the member stated it at the enter, per
    /// `weaver-trace-PRD` section 3.1 as of 2026-09-04, held for the leave
    /// to compare against. `None` where no member stood under a serving
    /// record, or where the enter's ask missed.
    grants_at_enter: Option<Vec<String>>,
    /// What this run was built from, held for the observation exchange of
    /// `weaver-admin-harness-contract` section 3 as of 2026-09-04: the same
    /// facts the load event was authored from, answered from the run and
    /// never read from the record.
    load: weaver_types::LoadFacts,
    /// The classify arm, per `weaver-harness-Spec` section 6: `None` where
    /// the declaration carried no binding, which the port serves as the
    /// missing leg's absence.
    classify: Option<ClassifyArm>,
    /// The session's fullness as the last generation carried it, granted to
    /// the seat as the fullness read, per the context ports of the Spec's
    /// section 6.
    fullness: Option<(u64, u64)>,
    /// Whether the recorder's pressure has been reported since it last
    /// crossed the high-water mark, per `weaver-harness-Spec` section 4.
    ///
    /// **Held by the run because the condition is the run's.** A seat is
    /// granted per turn and a full queue outlives any one turn, so a flag
    /// scoped to the seat would report again on every turn while the depth
    /// stayed high, which is the repetition the once-per-crossing rule
    /// refuses.
    pressure_reported: bool,
    turn_in_flight: Option<TurnKey>,
    /// The diagnostic run's own facts, present exactly where the binding
    /// is: what the load declared for the replay's opening event, the
    /// artifact the identity step checks the record against, and whether
    /// the seat has been granted, it being granted once at the run's
    /// opening per `weaver-harness-Spec` section 6.2's second criterion.
    diagnostic: Option<DiagnosticSeat>,
    /// Each initiator numbers exchanges on its own channel, so the SPU's and
    /// the gate's counters are separate and neither is hardcoded.
    spu_ordinal: u64,
    gate_ordinal: u64,
    /// Envelopes that arrived on the gate channel while a tool execution was
    /// awaited: a client's frame crossing mid-execution is held here by the
    /// turn and served after it, per the one-turn discipline.
    held_frames: std::collections::VecDeque<weaver_types::OrganEnvelope>,
    /// The turn counter loop 0 mints turn keys from, per the gate contract's
    /// rule that the turn does not exist until the harness opens it.
    turn_ordinal: u64,
    /// The gate was lowered by a leave that then stopped at its save point
    /// (S9): the run stands entered at rest with no gate, which a gate that
    /// died leaves too, so the lowering is held as its own fact.
    gate_lowered: bool,
    /// **A refusal the lower answered before the leave stopped in S9** (#140,
    /// S15): the unwind's first refusal, carried to the retry or the force
    /// that completes the unwind and named at its seal, since the stop
    /// authors nothing more (`weaver-harness-Spec` section 6).
    lower_refusal: Option<LifecycleRefusal>,
    /// The declared drain and wind-down bounds, in seconds, from the enter
    /// (`weaver-harness-Spec` section 6, the bounds); absent is unbounded.
    drain_bound: Option<u64>,
    wind_down_bound: Option<u64>,
    /// A leave heard mid-turn and held for this loop to carry out after the
    /// turn (3.0, S3 x unload), lent to the frame's seat.
    leave: Option<crate::engine::PendingLeave>,
}

impl Run {
    /// **The run's position among 3.0's states, derived and never stored**
    /// (`weaver-admin-Spec` 3.0), so the lifecycle table's input cannot drift
    /// from the run: from the turn, the pending leave's progress, whether a
    /// model is resident, the binding, and whether a stopped leave lowered
    /// the gate.
    fn position(
        &self,
        leave: Option<&crate::lifecycle_table::LeaveProgress>,
    ) -> crate::lifecycle_table::Position {
        use crate::lifecycle_table::{Binding, Leave, LeaveStep, Position, Spu};
        let binding = if self.diagnostic.is_some() {
            Binding::Diagnostic
        } else {
            Binding::Serving
        };
        match leave {
            None if self.turn_in_flight.is_some() => Position::S3,
            None if self.gate_lowered => Position::S9,
            None => Position::S2(if self.spu.is_some() {
                Spu::Resident
            } else {
                Spu::Dead
            }),
            // A sole force (or a rollback) stands in S10 until its release.
            Some(progress) => match (progress.directed, progress.step) {
                (_, LeaveStep::Releasing) => Position::S7,
                (Leave::Forced, LeaveStep::Draining | LeaveStep::Lowering | LeaveStep::Saving) => {
                    Position::S10
                }
                (Leave::Forced, LeaveStep::WindingDown)
                | (Leave::Graceful, LeaveStep::WindingDown) => Position::S5,
                (Leave::Graceful, LeaveStep::Draining) => Position::S4(progress.leave, binding),
                (Leave::Graceful, LeaveStep::Lowering | LeaveStep::Saving) => {
                    Position::S6(progress.leave)
                }
            },
        }
    }
}

/// The Gateless seat's standing, per `weaver-harness-Spec` section 6.2: the
/// run itself is the work, so what is held is what the one grant needs and
/// a flag that it happened.
struct DiagnosticSeat {
    reader_elected: bool,
    artifact: String,
    replayed: bool,
}

/// The SPU's arm is a pair of channels rather than one, and they are one field
/// because they stand up and fall together: the two are created in one act and
/// cross one fork, so an option over the pair keeps the arm's all-or-nothing
/// shape where two options would admit a half-stood arm the unwind would have
/// to reason about.
#[derive(Debug)]
struct SpuChannels {
    lifecycle: OrganChannel,
    decode: DecodeChannel,
    /// Retained so the leave can reap the organ rather than leaving a zombie
    /// entry for the life of the worker.
    pid: nix::unistd::Pid,
    /// The organ's last typed stderr line, held so a death's account can
    /// carry the reason, per `weaver-harness-Spec` section 2.2.
    last_word: crate::spawn::LastWord,
}

/// The classify readiness bound: generous against an admission that loads a
/// sub-gigabyte artifact in seconds, because the expiry refuses the whole
/// load and a slow disk should not read as a dead child.
const CLASSIFY_READY_BOUND_MS: u64 = 60_000;

/// The classify arm, per `weaver-harness-Spec` section 6: the label seam's
/// near end and the process behind it, standing only where the declaration
/// carried the binding, falling whole under the unwind like every arm.
struct ClassifyArm {
    channel: crate::channel::ClassifyChannel,
    pid: nix::unistd::Pid,
    /// Per `weaver-harness-Spec` section 2.2, as the SPU's.
    last_word: crate::spawn::LastWord,
}

/// How an enter refused, and whether the bracket stands. **A refusal before
/// the load event leaves the stream clean and the state at before-enter; a
/// refusal after it leaves the authored bracket standing and the run in place
/// for the leave that unwinds it**, which is the scoped account the Spec's
/// section 3 states.
enum EnterFailure {
    BeforeLoad(LifecycleRefusal),
    AfterLoad(Box<Run>, LifecycleRefusal),
}

/// The gate's arm: its channel and the pid the leave reaps.
struct GateChannel {
    channel: OrganChannel,
    pid: nix::unistd::Pid,
    /// Per `weaver-harness-Spec` section 2.2, as the SPU's.
    last_word: crate::spawn::LastWord,
}

/// The hub. Adopts the coordination end the unit's declared open delivered,
/// performs the worker's hygiene as sets and not checks, and serves loop 0.
pub struct Harness {
    coordination: CoordinationListener,
    organs: OrganBinaries,
    parameters: OrganParameters,
    state: ChannelState,
    /// The loop that composes this worker's prompts, handed to `serve` by
    /// the binary that runs it and named on every load, per
    /// `weaver-harness-Spec` section 6.1. None until `serve` is entered.
    composer: Option<weaver_trace::LoopIdentity>,
}

impl Harness {
    /// The crate's one constructor. There is no second path: the fields are
    /// private and no other function returns a `Harness`.
    ///
    /// The hygiene is two sets, not two checks: a check that finds the flag
    /// wrong and reports leaves the descriptor inheritable and the process
    /// attachable. Clearing the dumpable flag reparents the proc entries to
    /// root and refuses a same-uid attach, which is what closes the one route
    /// apex 5.1's possession-as-authentication argument assumes shut.
    pub fn listen(
        coordination: CoordinationListener,
        organs: OrganBinaries,
        parameters: OrganParameters,
    ) -> Result<Self, AdoptionFault> {
        clear_dumpable()?;
        Ok(Harness {
            coordination,
            organs,
            parameters,
            state: ChannelState::BeforeEnter,
            composer: None,
        })
    }

    /// Serves loop 0 until leave is answered or a fault below the exchange
    /// layer ends service, waiting where a landing can arrive, per
    /// `weaver-harness-Spec` section 6.2: the coordination listener, the
    /// connection a dialed verb is being served on, the gate channel, and
    /// the decode channel, one `poll` across all of them and one wake
    /// handled at a time.
    ///
    /// Dispatch is by payload kind, and only the frame grants the seat. A
    /// directive is the lifecycle interior's, judged against the channel's
    /// state, answered or refused, out-of-order refused and not queued. A
    /// report is clerked, no turn opening and no seat granted. A frame is
    /// the one arrival owed an answer: `entry` is the loop the binary
    /// carries across the dev boundary, called with the granted surface and
    /// the parsed request, and its return is the response the frame's
    /// exchange is answered with. `identity` and `tool_schemas` are the
    /// composition root's, the c-material of the assembled prompt.
    pub fn serve<F>(
        mut self,
        identity: &str,
        tool_schemas: &[String],
        composer: weaver_trace::LoopIdentity,
        mut entry: F,
    ) -> Result<Outcome, ChannelFault>
    where
        F: FnMut(
            &mut crate::engine::Ports<'_>,
            &str,
        ) -> Result<crate::engine::TurnOutcome, crate::engine::TurnError>,
    {
        // **The composer is the binary's fact and it names it once**, before
        // any enter can author a load.
        self.composer = Some(composer);
        // **One connection at a time**, held in the wait rather than blocked
        // on: the listener leaves the read set while a verb's connection is
        // being served, which is what holds the contract's
        // one-exchange-in-flight rule now that no fleet map does.
        let mut pending: Option<OrganChannel> = None;
        loop {
            // **No wait over a record that failed** (#116): a wake that
            // authored and announced nothing is still not served past.
            if let Err(fault) = self.record_holds() {
                self.unwind_if_entered(weaver_types::FaultCase::StreamWriteFailed, &fault);
                return Err(fault);
            }
            match self.wait(pending.as_ref()) {
                Ok(Wake::Listener) => match self.coordination.accept_root() {
                    Ok(connection) => pending = Some(connection),
                    // A peer that is not root never reaches an exchange. The
                    // listener stands and the next dial is accepted, because
                    // refusing one caller is not a reason to stop serving
                    // the one party that may call.
                    Err(ChannelFault::WrongPeer { .. }) => continue,
                    // The listener itself failed, which ends service.
                    Err(fault) => {
                        self.unwind_if_entered(weaver_types::FaultCase::ListenerLost, &fault);
                        return Err(fault);
                    }
                },
                Ok(Wake::Connection) => {
                    let connection = pending.take().expect("the wake proved a connection");
                    let (envelope, sink, state_end) = match connection.recv_with_descriptor() {
                        Ok(received) => received,
                        // The verb answered and admin closed. Ordinary, and
                        // not an ending: the run is held across connections,
                        // per `weaver-admin-harness-contract` section 4.
                        Err(ChannelFault::Closed) => continue,
                        Err(fault) => {
                            self.unwind_if_entered(
                                weaver_types::FaultCase::OrganDeathObserved,
                                &fault,
                            );
                            return Err(fault);
                        }
                    };
                    let exchange = envelope.exchange.clone();
                    let directive = match envelope.payload {
                        weaver_types::Payload::Directive(directive) => directive,
                        // Anything else on this channel is not a directive
                        // this crate can attribute to an exchange for a
                        // refusal to answer.
                        _ => {
                            self.unwind_if_entered(
                                weaver_types::FaultCase::OrganDeathObserved,
                                &ChannelFault::Undecodable,
                            );
                            return Err(ChannelFault::Undecodable);
                        }
                    };
                    match self.dispatch_on(&connection, exchange, directive, sink, state_end) {
                        Ok(Some(outcome)) => return Ok(outcome),
                        // The verb answered and the run stands: the same
                        // connection may carry another directive, so it
                        // returns to the wait.
                        Ok(None) => pending = Some(connection),
                        Err(fault) => {
                            self.unwind_if_entered(
                                weaver_types::FaultCase::OrganDeathObserved,
                                &fault,
                            );
                            return Err(fault);
                        }
                    }
                    // **The Gateless seat, granted once at the run's
                    // opening**, per `weaver-harness-Spec` section 6.2's
                    // second criterion: a diagnostic enter that just
                    // answered ready is the run arriving as the work, so
                    // the replay runs here, after the answer crossed and
                    // before the wait resumes. A serving run, and a
                    // diagnostic run already replayed, pass through.
                    if let Err(fault) = self.serve_replay(&mut pending) {
                        self.unwind_if_entered(weaver_types::FaultCase::OrganDeathObserved, &fault);
                        return Err(fault);
                    }
                }
                Ok(Wake::Gate) => {
                    if let Err(fault) =
                        self.serve_gate_wake(identity, tool_schemas, &mut entry, &mut pending)
                    {
                        self.unwind_if_entered(weaver_types::FaultCase::OrganDeathObserved, &fault);
                        return Err(fault);
                    }
                    // A held leave carried out after the turn ends service
                    // as a leave directed at rest does.
                    if matches!(self.state, ChannelState::Left) {
                        return Ok(Outcome::Left);
                    }
                }
                Ok(Wake::Decode) => {
                    // One at-rest arrival is legitimate: the cancel race's
                    // residue. A stop whose cancel lost to a natural
                    // completion reaches the SPU at rest and answers
                    // `AtRest`, which surfaces here and drains. Everything
                    // else is octets the seam's state cannot read, the
                    // report's wire case being issue 106's open election: a
                    // fault below the exchange layer, ending service the
                    // way section 3 ends it.
                    let verdict = match &self.state {
                        ChannelState::Entered(run) => {
                            run.spu.as_ref().map(|spu| spu.decode.recv_reply())
                        }
                        _ => None,
                    };
                    match verdict {
                        Some(Ok(crate::channel::DecodeReply::Answer(TokenAnswer::AtRest))) => {}
                        // A reply that decoded and is not the residue is the
                        // one place the synthetic fault is honest: the octets
                        // read, the seam's state cannot.
                        Some(Ok(_)) | None => {
                            self.unwind_if_entered(
                                weaver_types::FaultCase::OrganDeathObserved,
                                &ChannelFault::Undecodable,
                            );
                            return Err(ChannelFault::Undecodable);
                        }
                        // The channel's own fault is retained, so the fault
                        // event's account carries what the seam met rather
                        // than a synthetic stand-in.
                        // **The SPU's death at rest is decided by the table**
                        // (S2 x the SPU dies, ruling (B)): the run stands with
                        // no model, a later request refused `NoResidency`.
                        Some(Err(ChannelFault::Closed)) => {
                            if let Err(fault) = self.organ_died(OrganDeath::Spu) {
                                self.unwind_if_entered(
                                    weaver_types::FaultCase::OrganDeathObserved,
                                    &fault,
                                );
                                return Err(fault);
                            }
                        }
                        Some(Err(fault)) => {
                            self.unwind_if_entered(
                                weaver_types::FaultCase::OrganDeathObserved,
                                &fault,
                            );
                            return Err(fault);
                        }
                    }
                }
                Err(fault) => {
                    self.unwind_if_entered(weaver_types::FaultCase::ListenerLost, &fault);
                    return Err(fault);
                }
            }
        }
    }

    /// One wake from the entered-state wait, per `weaver-harness-Spec`
    /// section 6.2. The read set is what can originate now: the listener
    /// when no verb is mid-service, the verb's connection while one is, and
    /// the gate and decode channels while a run stands. `poll` sleeps
    /// against all of them and wakes on the first ready, serial as ever,
    /// and no executor enters.
    fn wait(&self, pending: Option<&OrganChannel>) -> Result<Wake, ChannelFault> {
        use std::os::fd::AsFd;

        use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
        loop {
            let mut fds: Vec<PollFd<'_>> = Vec::with_capacity(4);
            let mut wakes: Vec<Wake> = Vec::with_capacity(4);
            match pending {
                Some(connection) => {
                    fds.push(PollFd::new(connection.as_fd(), PollFlags::POLLIN));
                    wakes.push(Wake::Connection);
                }
                None => {
                    fds.push(PollFd::new(self.coordination.as_fd(), PollFlags::POLLIN));
                    wakes.push(Wake::Listener);
                }
            }
            if let ChannelState::Entered(run) = &self.state {
                if let Some(gate) = &run.gate {
                    fds.push(PollFd::new(gate.channel.as_fd(), PollFlags::POLLIN));
                    wakes.push(Wake::Gate);
                }
                if let Some(spu) = &run.spu {
                    fds.push(PollFd::new(spu.decode.as_fd(), PollFlags::POLLIN));
                    wakes.push(Wake::Decode);
                }
            }
            match poll(&mut fds, PollTimeout::NONE) {
                Ok(_) => {}
                Err(nix::errno::Errno::EINTR) => continue,
                Err(_) => return Err(ChannelFault::Closed),
            }
            let woken = PollFlags::POLLIN | PollFlags::POLLHUP | PollFlags::POLLERR;
            for (fd, wake) in fds.iter().zip(&wakes) {
                let revents = fd.revents().unwrap_or(PollFlags::empty());
                // An invalid descriptor never becomes readable, so treating
                // POLLNVAL as anything but an ending would spin this loop
                // forever against a wake that cannot arrive.
                if revents.contains(PollFlags::POLLNVAL) {
                    return Err(ChannelFault::Closed);
                }
                if revents.intersects(woken) {
                    return Ok(*wake);
                }
            }
        }
    }

    /// The diagnostic seat's one grant, per `weaver-harness-Spec` section
    /// 6.2: under a diagnostic binding the seat is granted once, at the
    /// run's opening, on the run itself as the work - the operator's
    /// sealed preload is what arrived owed an answer and the certification
    /// is the answer it is owed. No frame ever grants this seat, and a
    /// serving run never reaches the entry. Every outcome of the replay
    /// itself lands in the diagnostic-trace: only a channel-level loss
    /// surfaces here, ending service the way a serving turn's loss does.
    fn serve_replay(&mut self, pending: &mut Option<OrganChannel>) -> Result<(), ChannelFault> {
        let ChannelState::Entered(run) = &mut self.state else {
            return Ok(());
        };
        let Some(seat) = run.diagnostic.as_mut() else {
            return Ok(());
        };
        if seat.replayed {
            return Ok(());
        }
        seat.replayed = true;
        let reader_elected = seat.reader_elected;
        let artifact = seat.artifact.clone();
        let Some(spu) = run.spu.as_ref() else {
            return Ok(());
        };
        let mut ports = crate::engine::Ports::grant(
            &spu.decode,
            &run.author,
            &mut run.recorder,
            &mut run.turn_ordinal,
            &mut run.turn_in_flight,
            &run.load,
            // No frame, no prompt: the replay feeds the record's rendered
            // forms and assembles nothing.
            None,
            &self.coordination,
            Some(pending),
            // No Gate stands under this binding, by construction.
            None,
            run.state.as_mut(),
            run.classify.as_ref().map(|arm| &arm.channel),
            &mut run.fullness,
            &mut run.pressure_reported,
        );
        match crate::replay::drive(&mut ports, reader_elected, &artifact) {
            Ok(()) => Ok(()),
            Err(crate::engine::TurnError::ChannelLost) => Err(ChannelFault::Closed),
            // Refusals and faults are already the record's, authored
            // inside the pass, and the run stands for the operator's
            // leave.
            Err(_) => Ok(()),
        }
    }

    /// A gate-channel wake, dispatched by payload kind per section 6.2: a
    /// frame grants the seat, a report is clerked, and the gate gone is the
    /// survivor's fault to report, the run standing so admin can still
    /// leave.
    fn serve_gate_wake<F>(
        &mut self,
        identity: &str,
        tool_schemas: &[String],
        entry: &mut F,
        pending: &mut Option<OrganChannel>,
    ) -> Result<(), ChannelFault>
    where
        F: FnMut(
            &mut crate::engine::Ports<'_>,
            &str,
        ) -> Result<crate::engine::TurnOutcome, crate::engine::TurnError>,
    {
        let envelope = {
            let ChannelState::Entered(run) = &mut self.state else {
                return Ok(());
            };
            let Some(gate) = run.gate.as_ref() else {
                return Ok(());
            };
            match gate.channel.recv() {
                Ok(envelope) => envelope,
                // The gate died with the hook raised: the boundary is gone
                // and the interior is healthy, which is the survivor's fault
                // to report, per the fault-carrier ruling, decided by the
                // lifecycle table (S2 x the gate dies): the run stands so the
                // operator can still leave, the agent unreachable until then.
                Err(ChannelFault::Closed) => {
                    return self.organ_died(OrganDeath::Gate);
                }
                Err(fault) => return Err(fault),
            }
        };
        match envelope.payload {
            weaver_types::Payload::Frame(frame) => {
                self.serve_frame(
                    envelope.exchange,
                    frame,
                    identity,
                    tool_schemas,
                    entry,
                    pending,
                )?;
                // A leave the turn held is carried out now, the turn closed
                // and its dialer answered.
                self.carry_out_held_leave().map(|_| ())
            }
            // The gate's fault report is clerked to the record per the
            // fault-carrier ruling: the report arrives whole, the gate having
            // named its own case per `weaver-types-Spec` section 4.2, and the
            // harness authors what it is handed. No sender exists in the gate
            // today, which the working list registers against the relay act,
            // but the receive side no longer fabricates an account when one
            // arrives.
            weaver_types::Payload::Fault(report) => {
                if let ChannelState::Entered(run) = &mut self.state {
                    // Latched on a failure (#116), and read just below.
                    let _ =
                        run.author
                            .author_fault(&mut run.recorder, Subsystem::Gate, None, &report);
                }
                self.record_holds()
            }
            // Nothing else is the gate's to open on this seam.
            _ => Err(ChannelFault::Undecodable),
        }
    }

    /// A frame grants the seat, per section 6.2 and the dev boundary: the
    /// parse at the threshold, the entry with the granted surface, and the
    /// response frame answered on the exchange. The parse refuses rather
    /// than faults, per the ruling: a line that is not the request answers
    /// `refused` as content and the channel stands.
    fn serve_one_frame<F>(
        &mut self,
        exchange: ExchangeId,
        frame: weaver_types::TurnFrame,
        identity: &str,
        tool_schemas: &[String],
        entry: &mut F,
        pending: &mut Option<OrganChannel>,
    ) -> Result<(), ChannelFault>
    where
        F: FnMut(
            &mut crate::engine::Ports<'_>,
            &str,
        ) -> Result<crate::engine::TurnOutcome, crate::engine::TurnError>,
    {
        let ChannelState::Entered(run) = &mut self.state else {
            return Ok(());
        };
        // Taken before the borrow the seat needs, so a close can name the run
        // without holding the run across the grant.
        let run_ref = run.run.clone();
        // **The request is admitted by the lifecycle table** (`weaver-admin-Spec`
        // 3.0): at `S2(Dead)` no model is resident and the request is refused
        // through the gate's refused close (ruling (B),
        // `weaver-gate-world-contract` section 5), the run standing.
        let no_model = crate::lifecycle_table::respond(
            run.position(None),
            crate::lifecycle_table::Event::DialerRequest,
        ) == crate::lifecycle_table::Response::RefuseNoResidency;
        let response = match parse_request(&frame) {
            _ if no_model => render_close("refused", "reason", "no model is resident", None),
            Err(reason) => render_close("refused", "reason", &reason, None),
            // **A `system` line is the operator's alone**, per Spec 6.1's
            // seeding clause: admitted only where the frame's dialer, the
            // peer uid the gate read at accept and carries inward per
            // `weaver-harness-gate-contract` section 2, is the operator's
            // uid the enter carried. No dialer, or any other uid, the
            // connector's service user among them, is a refused turn naming
            // the role, and the channel stands.
            Ok(Request { system: true, .. }) if frame.dialer != Some(run.operator) => render_close(
                "refused",
                "reason",
                "a line of role system is admitted from the operator alone",
                None,
            ),
            Ok(Request { text, system }) => 'turn: {
                // The seat, granted at loaded-and-idle: the assembly read,
                // the grant across the dev boundary, and the take-back at
                // the entry's return.
                let structure = run
                    .recorder
                    .structure()
                    .expect("the frame path runs under the serving record");
                let prompt = crate::assembly::assemble(structure, identity, tool_schemas);
                if prompt.undecodable > 0 {
                    // An incomplete prompt does not cross the seam: the hole
                    // becomes the fault event and the turn does not run.
                    let account = format!(
                        "{{\"organ\":\"harness\",\"undecodable-message-records\":{}}}",
                        prompt.undecodable
                    );
                    // Latched on a failure (#116), read before the dialer is answered.
                    let _ = run.author.author_fault(
                        &mut run.recorder,
                        Subsystem::Harness,
                        None,
                        &crate::authorship::harness_report(
                            weaver_types::FaultCase::MessageRecordUndecodable,
                            &account,
                        ),
                    );
                    break 'turn render_close(
                        "stopped",
                        "reason",
                        "the working structure holds a hole",
                        None,
                    );
                }
                let Some(spu) = run.spu.as_ref() else {
                    // A run with no SPU is not loaded, and nothing can
                    // turn against it.
                    return Err(ChannelFault::Undecodable);
                };
                // **The seeding line enters the prompt**, per Spec 6.1: one
                // `system` message of the line's text verbatim, authored
                // turnless through the identity door before the turn that
                // appends it, so the record and the tee carry it as the
                // prefix the next load seats. The door judges the four rules
                // of `weaver-types-Spec` section 5, the empty text it writes
                // no turn for among them, and its refusal is the turn's,
                // naming the rule; a recorder that would not take it ends
                // service, the record being untrustworthy.
                let seeding = system.then(|| weaver_traits::Message {
                    role: weaver_traits::Role::System,
                    content: vec![weaver_traits::ContentBlock::Text { text: text.clone() }],
                });
                // **The seam's answer bound, spent at the sender**, per Spec
                // 6.1 and `weaver-harness-state-contract` section 3: the
                // identity answer carries every seeding of the newest run,
                // so a seeding whose addition would carry it past the frame
                // bound refuses before anything is authored or appended.
                let cost = seeding.as_ref().map(|message| {
                    let rendered = serde_json::to_string(message).map_or(0, |json| json.len());
                    crate::state::identity_entry_cost(rendered, &run.session.0, &run_ref.0)
                });
                if let Some(cost) = cost
                    && run.seeded_bytes + cost + crate::state::ANSWER_FRAME_BYTES
                        > crate::state::ANSWER_BOUND_BYTES
                {
                    break 'turn render_close(
                        "refused",
                        "reason",
                        &format!(
                            "the seeding would carry this residency's prefix to {} octets in the state seam's identity answer, past its bound of {} octets",
                            run.seeded_bytes + cost + crate::state::ANSWER_FRAME_BYTES,
                            crate::state::ANSWER_BOUND_BYTES
                        ),
                        None,
                    );
                }
                if let Some(message) = seeding.as_ref() {
                    match run.author.author_identity(&mut run.recorder, message) {
                        Ok(Ok(_)) => run.seeded_bytes += cost.unwrap_or(0),
                        Err(unlicensed) => {
                            break 'turn render_close(
                                "refused",
                                "reason",
                                &format!(
                                    "the identity door refused the system line: {} {}",
                                    unlicensed.role, unlicensed.block
                                ),
                                None,
                            );
                        }
                        Ok(Err(_)) => return Err(ChannelFault::Closed),
                    }
                }
                let gate_port = run.gate.as_ref().map(|gate| crate::engine::GatePort {
                    channel: &gate.channel,
                    ordinal: &mut run.gate_ordinal,
                    held: &mut run.held_frames,
                    tools: run.tools.as_ref(),
                    contained: Some(&mut run.gate_contained),
                });
                let mut ports = crate::engine::Ports::grant(
                    &spu.decode,
                    &run.author,
                    &mut run.recorder,
                    &mut run.turn_ordinal,
                    &mut run.turn_in_flight,
                    &run.load,
                    Some(prompt),
                    &self.coordination,
                    Some(pending),
                    gate_port,
                    run.state.as_mut(),
                    run.classify.as_ref().map(|arm| &arm.channel),
                    &mut run.fullness,
                    &mut run.pressure_reported,
                );
                ports.hold_leave(&mut run.leave, run.drain_bound);
                // The seeding turn is loop 0's own and crosses the dev
                // boundary nowhere: the prompt is appended and the model
                // answers, per Spec 6.1. Every other line is the loop's.
                let ran = match seeding {
                    Some(message) => ports.seed(message),
                    None => entry(&mut ports, &text),
                };
                match ran {
                    // A turn the operator's stop aborted answers the
                    // stopped close, the partial standing in the record.
                    Ok(outcome) if outcome.aborted => render_close(
                        "stopped",
                        "reason",
                        "the operator stopped the turn",
                        Some((&outcome.turn, &run_ref)),
                    ),
                    // A model-side stop is a completed turn whose
                    // truncation the record holds, so the client is
                    // answered with what stands.
                    Ok(outcome) => render_close_with_finish(
                        "answered",
                        "text",
                        &outcome.emission,
                        Some((&outcome.turn, &run_ref)),
                        outcome.truncated,
                    ),
                    Err(crate::engine::TurnError::Refused { turn, refusal }) => render_close(
                        "stopped",
                        "reason",
                        refusal_reason(&refusal),
                        Some((&turn, &run_ref)),
                    ),
                    // The emission arrived mid-stream. **The engine
                    // already authored the report inside the turn's
                    // bracket**, before the close its error path lands,
                    // so this arm renders the client's close and authors
                    // nothing: a second authoring here would file one
                    // fact twice, and filing it here at all would put it
                    // after `turn.closed`. Service continues, the fault
                    // being one the worker survives by definition of the
                    // case set.
                    Err(crate::engine::TurnError::Faulted { turn, report: _ }) => render_close(
                        "stopped",
                        "reason",
                        "the model organ reported a fault",
                        Some((&turn, &run_ref)),
                    ),
                    Err(crate::engine::TurnError::Unlicensed { turn }) => render_close(
                        "stopped",
                        "reason",
                        "a message was not licensed for its role",
                        Some((&turn, &run_ref)),
                    ),
                    // **Nothing hung up: no organ's death explains the
                    // loss** (the record, or octets that do not decode), so
                    // the run is untrustworthy and service ends, a leave
                    // held or not, with no organ taken for dead and no
                    // `Left` claiming a clean close (the security review of
                    // 1b50cae).
                    Err(crate::engine::TurnError::ChannelLost) if !organ_hung_up(run) => {
                        return Err(ChannelFault::Closed);
                    }
                    // **An organ that died mid-turn is met by the table**
                    // (3.0, S3 and S4 x the SPU or the gate dies, ruling
                    // (B)): the turn closes failed, its dialer answered where
                    // the gate stands, the dead organ's fault authored and
                    // its channel dropped, and the run stands, a later
                    // request refused `NoResidency` where the SPU died. **A
                    // leave held is not unwound** (the Planner's ruling of
                    // 2026-10-09): it carries on, turned forced by (A). Only
                    // a worker that dies leaves admin no `Left`.
                    Err(crate::engine::TurnError::ChannelLost) => {
                        let turn = close_turn_on_death(run)?;
                        if let Some(held) = run.leave.as_mut() {
                            held.progress.leave =
                                crate::lifecycle_table::leave_on_arrival(held.progress.leave, true);
                        }
                        render_close(
                            "stopped",
                            "reason",
                            "the model organ failed",
                            turn.as_ref().map(|turn| (turn, &run_ref)),
                        )
                    }
                }
            }
        };
        // **The latch is read before the dialer is answered** (#116): a turn
        // whose record is missing an event, its close landed or not, is not
        // announced, and service ends instead.
        if run.recorder.failed() {
            return Err(ChannelFault::Closed);
        }
        let Some(gate) = run.gate.as_ref() else {
            return Ok(());
        };
        let sent = gate.channel.send(&OrganEnvelope {
            exchange,
            position: Position::Close,
            payload: weaver_types::Payload::Frame(weaver_types::TurnFrame::carry(
                response.as_bytes(),
            )),
        });
        match sent {
            Ok(()) => Ok(()),
            // **The gate died while the turn ran** (3.0, S3 and S4 x the gate
            // dies; the Planner's grade of #119): the answer is lost with it,
            // as a delivery the gate cannot make is, and its death is decided
            // by the table at rest: a `fault`, the run serving on; a leave
            // the turn held is carried out forced by (A).
            Err(_)
                if {
                    use std::os::fd::AsFd;
                    hung_up(gate.channel.as_fd())
                } =>
            {
                self.organ_died(OrganDeath::Gate)
            }
            Err(fault) => Err(fault),
        }
    }

    /// One frame's turn, then the shelf, per [`Run::held_frames`]: the
    /// frames the turn held are served after its close, in arrival order. A
    /// client that spoke while an execution was awaited was neither dropped
    /// nor answered out of order: its envelope waited on the run's shelf
    /// and is served here as if just received, the one-turn discipline
    /// preserved across the executions inside a turn.
    ///
    /// **The drain is a flat loop, never a recursive serve per held
    /// frame.** A held frame's own turn can hold more frames, and a serve
    /// that recursed would let a client that speaks through every execution
    /// grow the stack with the shelf; the loop re-reads the queue after
    /// each turn instead, the depth constant however long the shelf runs.
    fn serve_frame<F>(
        &mut self,
        exchange: ExchangeId,
        frame: weaver_types::TurnFrame,
        identity: &str,
        tool_schemas: &[String],
        entry: &mut F,
        pending: &mut Option<OrganChannel>,
    ) -> Result<(), ChannelFault>
    where
        F: FnMut(
            &mut crate::engine::Ports<'_>,
            &str,
        ) -> Result<crate::engine::TurnOutcome, crate::engine::TurnError>,
    {
        self.serve_one_frame(exchange, frame, identity, tool_schemas, entry, pending)?;
        self.serve_shelf(identity, tool_schemas, entry, pending)
    }

    /// The shelf's flat loop, after a frame's turn ([`Self::serve_frame`]).
    /// **A leave held by the turn leaves the shelf to its drain** (3.0, S4
    /// x dialer request): a frame queued behind the turn is refused
    /// `Unloading` there, never served as a turn under the leave.
    fn serve_shelf<F>(
        &mut self,
        identity: &str,
        tool_schemas: &[String],
        entry: &mut F,
        pending: &mut Option<OrganChannel>,
    ) -> Result<(), ChannelFault>
    where
        F: FnMut(
            &mut crate::engine::Ports<'_>,
            &str,
        ) -> Result<crate::engine::TurnOutcome, crate::engine::TurnError>,
    {
        loop {
            // **No held frame is served over a latched record** (#116): a
            // turn, and the tools it runs in the world, never start past a
            // record already missing an event.
            self.record_holds()?;
            let held = {
                let ChannelState::Entered(run) = &mut self.state else {
                    return Ok(());
                };
                if run.leave.is_some() {
                    return Ok(());
                }
                let Some(held) = run.held_frames.pop_front() else {
                    return Ok(());
                };
                held
            };
            match held.payload {
                weaver_types::Payload::Frame(frame) => {
                    self.serve_one_frame(
                        held.exchange,
                        frame,
                        identity,
                        tool_schemas,
                        entry,
                        pending,
                    )?;
                }
                weaver_types::Payload::Fault(report) => {
                    let ChannelState::Entered(run) = &mut self.state else {
                        return Ok(());
                    };
                    let turn = run.turn_in_flight.clone();
                    // Latched on a failure (#116), read at the top of the loop
                    // before the next held frame is served.
                    let _ = run.author.author_fault(
                        &mut run.recorder,
                        Subsystem::Gate,
                        turn.as_ref(),
                        &report,
                    );
                }
                // Nothing else is the gate's to open, and an answer with no
                // exchange awaiting it correlates to nothing: dropped as the
                // protocol noise it is rather than faulting the run.
                _ => {}
            }
        }
    }

    /// **An organ's death, decided by the lifecycle table** (`weaver-admin-Spec`
    /// 3.0, the organ-death table): at rest the run serves on, a `fault`
    /// authored for the death, the dead organ's channel dropped and the organ
    /// reaped; a gate dying with the gate already lowered (S9) is no cell.
    /// **The position is read at rest** (`position(None)`), a leave the turn
    /// held not entering it: a gate found dead at a turn's answer send, a
    /// leave held, is served as S2's death here, and the held leave is then
    /// carried out forced by (A) where it is carried out. A death mid-leave is
    /// met by the leave's own handling; the cell that would end the run below
    /// is reached by no caller.
    fn organ_died(&mut self, organ: OrganDeath) -> Result<(), ChannelFault> {
        use crate::lifecycle_table::{Event, Response, respond};
        let ChannelState::Entered(run) = &mut self.state else {
            return Ok(());
        };
        let event = match organ {
            OrganDeath::Gate => Event::GateDies,
            OrganDeath::Spu => Event::SpuDies,
        };
        let author_death =
            |run: &mut Run, name: &str, last_word: Option<String>, contained: Option<Contained>| {
                let mut account =
                    serde_json::json!({"organ": name, "death": "channel closed, the run standing"});
                if let Some(line) = last_word {
                    account["last_word"] =
                        serde_json::from_str(&line).unwrap_or(serde_json::Value::String(line));
                }
                if let Some(words) = containment_words(contained) {
                    account["containment"] = serde_json::Value::String(words.to_string());
                }
                let account = account.to_string();
                run.author
                    .author_fault(
                        &mut run.recorder,
                        Subsystem::Harness,
                        run.turn_in_flight.clone().as_ref(),
                        &crate::authorship::harness_report(
                            weaver_types::FaultCase::OrganDeathObserved,
                            &account,
                        ),
                    )
                    .is_ok()
            };
        match respond(run.position(None), event) {
            Response::NoCell => {}
            // **The record holds or service ends** (the Planner's ruling of
            // 2026-10-09): a death's fault that cannot be authored ends
            // service before the organ is torn down, so the run never serves
            // on with the record of it missing.
            // **The worker kills `tools/` and waits a full bound from its
            // own kill** (`weaver-admin-Spec` 6.1, the gate dies while
            // serving), the account naming what that came to, and a `tools/`
            // that will not empty, or a kill not delivered, ends service.
            Response::GateFaultServeOn => {
                let last_word = run.gate.as_ref().and_then(|gate| gate.last_word.take());
                let contained = contain_tools(run, crate::tools_cgroup::Context::Serving);
                if !author_death(run, "gate", last_word, contained) {
                    return Err(ChannelFault::Closed);
                }
                if let Some(gate) = run.gate.take() {
                    drop(gate.channel);
                    reap_by(gate.pid, lower_window());
                }
                if ends_service(contained) {
                    return Err(ChannelFault::Closed);
                }
            }
            Response::SpuFaultServeOnRefuseLater | Response::SpuFaultRetriedReleaseUnconfirmed => {
                let last_word = run.spu.as_ref().and_then(|spu| spu.last_word.take());
                if !author_death(run, "spu", last_word, None) {
                    return Err(ChannelFault::Closed);
                }
                if let Some(spu) = run.spu.take() {
                    let pid = spu.pid;
                    drop(spu);
                    reap_by(pid, lower_window());
                }
            }
            // A cell of a turn or a leave in flight: not reachable from the
            // serve loop's wake, the turn and the leave meeting their organs'
            // deaths themselves.
            _ => self.unwind_if_entered(
                weaver_types::FaultCase::OrganDeathObserved,
                &ChannelFault::Closed,
            ),
        }
        Ok(())
    }

    /// **The graceful leave's wind-down turn** (`weaver-admin-Spec` 3.0,
    /// graceful step 3), run after the drain and before the lower: one turn
    /// asking the model for where the work stands, on a serving run with a
    /// model resident. A diagnostic binding has no turn to run and takes
    /// none, and a leave retried from S9 took its own before its lower. The seat is granted as a frame's is, without the gate (drained,
    /// so a call is never sent) and without the ear. Returns whether the SPU
    /// died in it (A) or its declared bound passed (S5 x a bound passes),
    /// either of which turns the leave forced; a loss no organ's hang-up
    /// explains ends service, as a frame's turn's does.
    fn wind_down(&mut self, held: &mut Option<PendingLeave>) -> Result<(), ChannelFault> {
        let ChannelState::Entered(run) = &mut self.state else {
            return Ok(());
        };
        // **Once, before the lower**: a leave retried from S9, its gate
        // lowered by the leave that stopped, wound down then.
        if run.diagnostic.is_some() || run.gate_lowered {
            return Ok(());
        }
        let Some(spu) = run.spu.as_ref() else {
            return Ok(());
        };
        let wind_down_bound = run.wind_down_bound;
        let Some(leave) = held.as_mut() else {
            return Ok(());
        };
        let cause = leave.cause;
        // S5, only where the wind-down runs: a diagnostic binding, a leave
        // retried from S9 and a run with no SPU never stand there.
        leave.progress.step = crate::lifecycle_table::LeaveStep::WindingDown;
        // The drain is done: its deadline bounds nothing here.
        leave.drain_deadline = None;
        // **The wind-down hears the listener** beside its generation: a dial
        // lands in this slot, and a join turns the lent leave forced and
        // cancels the generation (S5 x force-unload).
        let mut dialed: Option<OrganChannel> = None;
        // **The gate is lent to be watched**: a call the wind-down asks is
        // never sent, and a gate dying in it turns the leave forced at that
        // moment (S5 x the gate dies).
        let gate_port = run.gate.as_ref().map(|gate| crate::engine::GatePort {
            channel: &gate.channel,
            ordinal: &mut run.gate_ordinal,
            held: &mut run.held_frames,
            tools: run.tools.as_ref(),
            contained: Some(&mut run.gate_contained),
        });
        let mut ports = crate::engine::Ports::grant(
            &spu.decode,
            &run.author,
            &mut run.recorder,
            &mut run.turn_ordinal,
            &mut run.turn_in_flight,
            &run.load,
            None,
            &self.coordination,
            Some(&mut dialed),
            gate_port,
            run.state.as_mut(),
            run.classify.as_ref().map(|arm| &arm.channel),
            &mut run.fullness,
            &mut run.pressure_reported,
        );
        ports.hold_leave(held, None);
        let (ran, _) = ports.wind_down(deadline_after(wind_down_bound), cause);
        drop(ports);
        let leave = held.as_mut().expect("the leave lent back");
        // A dial the turn accepted and did not take is heard now.
        if let Some(connection) = dialed {
            hear_dial(connection, leave, false, run.recorder.failed());
        }
        match ran {
            Err(crate::engine::TurnError::ChannelLost) if !organ_hung_up(run) => {
                Err(ChannelFault::Closed)
            }
            Err(crate::engine::TurnError::ChannelLost) => {
                close_turn_on_death(run)?;
                // **An SPU dead in the wind-down turns the leave forced**
                // (A): the wind-down cannot run.
                if run.spu.is_none() {
                    leave.progress.leave = crate::lifecycle_table::Leave::Forced;
                }
                Ok(())
            }
            // The turn closed, completed or stopped, its close authored by
            // the engine; a bound or a join that cut it turned the lent leave
            // forced there.
            _ => Ok(()),
        }
    }

    /// **Carries out a leave**, directed at rest or held from a turn (3.0,
    /// S2 and S3 x unload and x force-unload), answering it on the connection
    /// and exchange it came on, and every join it held the same.
    ///
    /// **Every wait of the leave hears the coordination listener**
    /// (`weaver-harness-Spec` section 6): the drain's wait polls it beside
    /// the gate and the wind-down's beside its generation, so a force joins
    /// at once where the wait is unbounded; the lower and the save point's
    /// legs, each bounded, are followed by a sweep of the backlog before
    /// their outcome is read, so a join heard while they ran turns a missed
    /// leg forced rather than stopping in S9. **S7 runs the seal's steps**
    /// (`weaver-admin-Spec` section 3, the seal): the pathname unlinked, the
    /// backlog swept into held joins, the release, `unload`, the writer's
    /// drain, and only then the same answer to the leave's dialer and every
    /// held join.
    fn carry_out_leave(&mut self, leave: PendingLeave) -> Result<Option<Outcome>, ChannelFault> {
        use crate::lifecycle_table::{Leave, LeaveStep};
        let cause = leave.cause;
        let rollback = leave.rollback;
        let mut leave = leave;
        // **A rollback is forced** (3.0, S1 x `Leave`), whatever its wire
        // `forced` says: no wind-down, `unload` and `Left` forced.
        if rollback {
            leave.progress.leave = Leave::Forced;
        }
        let mut held = Some(leave);
        // **Ruling (A) where the leave is carried out**: a leave held from a
        // turn meets the run as the turn left it, its gate or SPU perhaps
        // gone since the leave arrived; a serving run with no gate a leave
        // lowered, or no SPU, carries it out forced.
        if let ChannelState::Entered(run) = &self.state {
            let serving = run.diagnostic.is_none();
            let dead = run.spu.is_none() || (serving && run.gate.is_none() && !run.gate_lowered);
            if dead && let Some(leave) = held.as_mut() {
                leave.progress.leave = Leave::Forced;
            }
        }
        let forced_now = |held: &Option<PendingLeave>| {
            held.as_ref()
                .is_some_and(|l| l.progress.leave == Leave::Forced)
        };
        // **The graceful leave drains, then winds down** (`weaver-admin-Spec`
        // 3.0, graceful steps 2 and 3): the gate is quiesced and every
        // request it had admitted, or this loop held behind the turn, is
        // refused `Unloading` on its own exchange, its connection standing;
        // then the wind-down turn runs. A forced leave and a rollback lower
        // at once. **A declared bound passing, or a join, turns the leave
        // forced from where it stands** (S4 and S5 x a bound passes and x
        // force-unload): past the drain, the wind-down is skipped.
        if !forced_now(&held)
            && !rollback
            && let ChannelState::Entered(run) = &mut self.state
        {
            drain_gate(run, &self.coordination, held.as_mut().expect("the leave"));
            // **No wind-down over a latched record** (#116): its turn would
            // run a generation past a record already missing an event; the
            // leave goes on to its seal, which refuses.
            if !forced_now(&held) && !run.recorder.failed() {
                self.wind_down(&mut held)?;
            }
        }
        let ChannelState::Entered(run) = &mut self.state else {
            answer_all(
                held.as_ref().expect("the leave"),
                Err(LifecycleRefusal::OutOfOrder),
                false,
            );
            return Ok(None);
        };
        // **The gate is lowered before the leave's save point is
        // taken** (Codex on #94, round 12), per Spec section 6: a
        // turn the gate admitted while the snapshot ran would be
        // dropped at the lower with no record and no answer, so no
        // new work crosses the at-rest boundary once the leave has
        // begun; a lower that refuses is remembered and the leave
        // names it as its first refusal, the unwind running whole.
        // A leave refused at its save point leaves the run entered
        // at rest with the gate lowered, the retry or the force
        // finding it so. **The shared deadline runs from the lower's
        // start** (`weaver-harness-Spec` section 6).
        let shared = shared_deadline();
        held.as_mut().expect("the leave").progress.step = LeaveStep::Lowering;
        // A gate that died in the wind-down, or before the lower, is met here.
        {
            use std::os::fd::AsFd;
            if run
                .gate
                .as_ref()
                .is_some_and(|gate| hung_up(gate.channel.as_fd()))
            {
                gate_died_in_leave(run, held.as_mut().expect("the leave"));
            }
        }
        let (lowered, died) = lower_gate(run, shared);
        if died {
            held.as_mut().expect("the leave").progress.leave = Leave::Forced;
        }
        let latched = run.recorder.failed();
        hear_dials(
            &self.coordination,
            held.as_mut().expect("the leave"),
            false,
            latched,
        );
        // **The leave's save point first, and the leave does not
        // complete without it**, per Spec section 6 on the operator's
        // ruling of 2026-10-06 on #1 (A3.0 item 6). **Every leave takes its
        // save point but a load's rollback** (3.0: `forced` no longer means
        // lost, and a rollback is a run that never served). A missed leg is
        // decided by the lifecycle table (S6 x a missed leg), after the
        // sweep, so a join heard while the legs ran has turned the leave
        // forced: a leave that stayed graceful stops in S9, refused naming
        // the leg; a forced one records the miss as a refusal of the leave
        // and comes down with no save point.
        let save_point = if rollback {
            None
        } else {
            held.as_mut().expect("the leave").progress.step = LeaveStep::Saving;
            let taken = take_save_point(run, Some(cause));
            let latched = run.recorder.failed();
            hear_dials(
                &self.coordination,
                held.as_mut().expect("the leave"),
                false,
                latched,
            );
            match taken {
                Ok(report) => report,
                Err(missed) => {
                    use crate::lifecycle_table::{Event, Position, Response, respond};
                    let leave = held.as_ref().expect("the leave").progress.leave;
                    let refusal = LifecycleRefusal::SavePointNotTaken { missed };
                    match respond(Position::S6(leave), Event::LegMissed) {
                        Response::ForcedMissToS7 => {
                            author_refusal_on(run, weaver_types::LifecycleAsk::Leave, &refusal);
                            None
                        }
                        // `StopInS9`, the one other answer to a missed leg.
                        // The lower's refusal is carried to the leave that
                        // completes the unwind (#140, S15).
                        _ => {
                            run.lower_refusal = lowered;
                            let latched = run.recorder.failed();
                            answer_all(held.as_ref().expect("the leave"), Err(refusal), latched);
                            return Ok(None);
                        }
                    }
                }
            }
        };
        // **S7, the seal** (`weaver-admin-Spec` section 3): the outcome
        // fixed, the pathname unlinked so no new dial reaches this worker,
        // and the backlog swept into held joins, each answered the `Left` as
        // it stands and naming nothing.
        self.coordination.unlink();
        let mut leave = held.take().expect("the leave");
        leave.progress.step = LeaveStep::Releasing;
        let latched = run.recorder.failed();
        hear_dials(&self.coordination, &mut leave, true, latched);
        let forced = leave.progress.leave == Leave::Forced;
        // **No state to keep**: a diagnostic binding, or a serving run
        // with no member seam (`weaver-types-Spec` section 4.2,
        // `Left.no_state`).
        let no_state = run.recorder.serving().is_none() || run.state.is_none();
        let mut run = match std::mem::replace(&mut self.state, ChannelState::Left) {
            ChannelState::Entered(run) => *run,
            // Unreachable: the match arm above proved the position.
            other => {
                self.state = other;
                return Err(ChannelFault::Undecodable);
            }
        };
        let left = leave_after(
            &mut run,
            Some(cause),
            forced,
            leave.forced_by,
            lowered,
            shared,
        )
        // `forced` is the leave's state at its end (I2).
        .map(|()| LifecycleAnswer::Left {
            save_point,
            forced,
            no_state,
        });
        // Everything admitted did not reach the stream where the release
        // refused, so the answer says so rather than claiming a clean close.
        answer_all(&leave, left, run.recorder.failed());
        Ok(Some(Outcome::Left))
    }

    /// **The leave a turn held, carried out after the turn** (3.0, S3 x
    /// unload): the turn closed and its dialer was answered, so the leave
    /// proceeds on admin's connection, which the turn kept open.
    fn carry_out_held_leave(&mut self) -> Result<Option<Outcome>, ChannelFault> {
        let ChannelState::Entered(run) = &mut self.state else {
            return Ok(None);
        };
        let Some(held) = run.leave.take() else {
            return Ok(None);
        };
        self.carry_out_leave(held)
    }

    /// Unwinds a standing run, for a path that ends service without a
    /// leave, and records why before it does. A worker that dies seconds
    /// after a fault with no fault event in the stream is the record gap
    /// issue #221 filed: the stream is the program's one fault carrier,
    /// and a silent death lies by omission. The case is the caller's,
    /// because the site that met the fault knows which party it lost, and
    /// the account carries the fault's own spelling.
    fn unwind_if_entered(&mut self, case: weaver_types::FaultCase, fault: &ChannelFault) {
        if let ChannelState::Entered(run) = std::mem::replace(&mut self.state, ChannelState::Left) {
            let mut run = *run;
            // **A latched record names itself** (#116): service ending on a
            // record missing an event is `StreamWriteFailed`, whichever
            // reader met it, the account carrying the fault the reader
            // returned.
            let case = if run.recorder.failed() {
                weaver_types::FaultCase::StreamWriteFailed
            } else {
                case
            };
            let turn = run.turn_in_flight.clone();
            let mut account = serde_json::json!({
                "organ": "harness",
                "service-ended": format!("{fault:?}"),
            });
            // **The dead organ's last word rides the account**, per
            // `weaver-harness-Spec` section 2.2: present where an organ
            // spoke a line that parses, absent otherwise - garble is the
            // journal's to keep, carried there by the tee.
            let mut words = serde_json::Map::new();
            for (name, spoken) in [
                ("spu", run.spu.as_ref().map(|a| a.last_word.take())),
                ("gate", run.gate.as_ref().map(|a| a.last_word.take())),
                (
                    "classify",
                    run.classify.as_ref().map(|a| a.last_word.take()),
                ),
            ] {
                if let Some(Some(line)) = spoken {
                    let value =
                        serde_json::from_str(&line).unwrap_or(serde_json::Value::String(line));
                    words.insert(name.to_string(), value);
                }
            }
            if !words.is_empty() {
                account["last_word"] = serde_json::Value::Object(words);
            }
            let account = account.to_string();
            // Best-effort: service is already ending here, so a failure has nothing left
            // to end.
            let _ = run.author.author_fault(
                &mut run.recorder,
                Subsystem::Harness,
                turn.as_ref(),
                &crate::authorship::harness_report(case, &account),
            );
            // The worker unwinds itself, so no leave asked and the unload
            // carries no cause.
            // No admin waits on an unwind, so its refusal has no one to reach.
            let _ = leave(&mut run, None, false);
        }
    }

    /// The serial service's one step. Returns `Some(outcome)` when service
    /// ends. The exchange the directive opened is carried through, because an
    /// answer closes the exchange the directive named rather than one this
    /// crate invents.
    fn dispatch_on(
        &mut self,
        connection: &OrganChannel,
        exchange: ExchangeId,
        directive: LifecycleDirective,
        sink: Option<OwnedFd>,
        state_end: Option<OwnedFd>,
    ) -> Result<Option<Outcome>, ChannelFault> {
        match (&mut self.state, directive) {
            (ChannelState::BeforeEnter, LifecycleDirective::Enter { payload }) => {
                match self.enter(*payload, sink, state_end) {
                    Ok(run) => {
                        self.state = ChannelState::Entered(Box::new(run));
                        self.answer(connection, &exchange, LifecycleAnswer::Ready)?;
                    }
                    Err(EnterFailure::BeforeLoad(refusal)) => {
                        // Nothing was authored and nothing stood up, so the
                        // stream is clean and the position holds.
                        self.refuse(connection, &exchange, refusal)?;
                    }
                    Err(EnterFailure::AfterLoad(mut run, refusal)) => {
                        // **The refusal is clerked before the answer goes
                        // out**, per `weaver-harness-Spec` section 6 and the
                        // admin contract's clause of 2026-08-22: a bracket
                        // that stands is a run whose record can say why its
                        // enter was refused, and the answer alone leaves
                        // nothing behind. This is the one site for every
                        // after-load path that raises a lifecycle refusal,
                        // a refusal being one kind of event whatever raised
                        // it.
                        author_lifecycle_refusal(&mut run, &refusal);
                        // The bracket stands, so the run stays in place for
                        // the leave that unwinds it. Dropping it here would
                        // orphan what forked and leave the bracket unclosed.
                        self.state = ChannelState::Entered(run);
                        self.refuse(connection, &exchange, refusal)?;
                    }
                }
                Ok(None)
            }
            // A rollback is told by the directive's `rollback`, never by the
            // position (`weaver-harness-Spec` section 6, the rollback's row).
            (
                ChannelState::Entered(run),
                LifecycleDirective::Leave {
                    cause,
                    forced,
                    rollback,
                },
            ) => {
                // **A leave is never refused for activity**: mid-turn the
                // engine holds it. A turn still standing here is one whose
                // close could not be authored, the record having failed, so
                // service ends and no `Left` is answered over it (I2).
                if run.turn_in_flight.is_some() {
                    return Err(ChannelFault::Closed);
                }
                // Traffic the gate sent and this loop has not taken is met by
                // the graceful leave's drain, refused `Unloading` (3.0,
                // graceful step 2), and by the forced lower, recorded refused.
                // **Ruling (A) at the leave's arrival**: a graceful leave
                // directed with the SPU dead, or a serving run's gate gone
                // without a leave having lowered it, starts forced from where
                // it stands, the graceful caller's cause kept.
                use crate::lifecycle_table::{Leave, LeaveProgress, LeaveStep, leave_on_arrival};
                let serving = run.diagnostic.is_none();
                let dead =
                    run.spu.is_none() || (serving && run.gate.is_none() && !run.gate_lowered);
                let directed = if forced {
                    Leave::Forced
                } else {
                    Leave::Graceful
                };
                // The drain's bound runs from the leave's arrival, and the
                // leave holds its dialer's connection as a held leave does.
                let leave = PendingLeave {
                    connection: connection.try_clone()?,
                    exchange,
                    cause,
                    rollback,
                    forced_by: None,
                    progress: LeaveProgress {
                        directed,
                        leave: leave_on_arrival(directed, dead),
                        step: LeaveStep::Draining,
                    },
                    drain_deadline: deadline_after(run.drain_bound),
                    joins: Vec::new(),
                };
                self.carry_out_leave(leave)
            }
            // **Observe answers from any position and authors nothing**, per
            // `weaver-admin-harness-contract` section 3 as of 2026-09-04: the
            // state is a read of whether a turn key stands, the facts are
            // held on the run, and the left position answers rather than
            // refuses because an observation is not an act.
            (ChannelState::BeforeEnter, LifecycleDirective::Observe)
            | (ChannelState::Left, LifecycleDirective::Observe) => {
                self.answer(
                    connection,
                    &exchange,
                    LifecycleAnswer::State {
                        state: weaver_types::AgentState::Unloaded,
                        load: None,
                        constituents: Vec::new(),
                    },
                )?;
                Ok(None)
            }
            (ChannelState::Entered(run), LifecycleDirective::Observe) => {
                let state = if run.turn_in_flight.is_some() {
                    weaver_types::AgentState::Active
                } else {
                    weaver_types::AgentState::Idle
                };
                let load = run.load.clone();
                self.answer(
                    connection,
                    &exchange,
                    LifecycleAnswer::State {
                        state,
                        load: Some(Box::new(load)),
                        constituents: Vec::new(),
                    },
                )?;
                Ok(None)
            }
            // **A save point on demand**, admin's `save-point` verb, per Spec
            // section 6: at rest only, the same four legs and the same event
            // as the leave's, answered with the report admin's manifest
            // records, or refused naming the leg that missed.
            (ChannelState::Entered(run), LifecycleDirective::SavePoint { cause }) => {
                if run.turn_in_flight.is_some() {
                    self.refuse(connection, &exchange, LifecycleRefusal::ActivityNotAtRest)?;
                    return Ok(None);
                }
                match take_save_point(run, Some(cause)) {
                    Ok(Some(report)) => {
                        self.answer(
                            connection,
                            &exchange,
                            LifecycleAnswer::SavePointTaken { report },
                        )?;
                    }
                    // **A run that takes no save point refuses the demand
                    // as out of order**, per Spec section 6: a diagnostic
                    // binding, or a serving run with no member seam, has no
                    // member to ask, and calling a healthy member dead would
                    // misreport it.
                    Ok(None) => {
                        self.refuse(connection, &exchange, LifecycleRefusal::OutOfOrder)?;
                    }
                    Err(missed) => {
                        self.refuse(
                            connection,
                            &exchange,
                            LifecycleRefusal::SavePointNotTaken { missed },
                        )?;
                    }
                }
                Ok(None)
            }
            (ChannelState::Entered(run), LifecycleDirective::Stop { cause }) => {
                // Announce after record: the turn's close event is placed with
                // the stop reason, and only then does the answer carry
                // TurnAborted.
                let answer = match run.turn_in_flight.take() {
                    Some(turn) => {
                        // Latched on a failure (#116): the answer below reads the latch
                        // before `TurnAborted` is announced.
                        let _ = run.author.author(
                            &mut run.recorder,
                            Kind::TurnClosed,
                            Subsystem::Harness,
                            Some(&turn),
                            Some(Payload::TurnClosed(TurnClose::Stopped {
                                reason: weaver_trace::StopReason::Directive,
                                cause: Some(crate::engine::trace_cause(cause)),
                            })),
                        );
                        LifecycleAnswer::TurnAborted {
                            turn: TurnKey(turn.0.clone()),
                        }
                    }
                    // A stop at rest is a clean close and not a refusal.
                    None => LifecycleAnswer::AtRest,
                };
                self.answer(connection, &exchange, answer)?;
                Ok(None)
            }
            // Every other pairing is out of order for the position. The left
            // position is terminal, so a directive of any kind arriving after
            // a leave answers the same refusal.
            _ => {
                self.refuse(connection, &exchange, LifecycleRefusal::OutOfOrder)?;
                Ok(None)
            }
        }
    }

    /// Enter runs the charter's steps in order, and the answer is the
    /// aggregate. The fan-out's extent follows the match on the payload's
    /// binding, per `weaver-harness-Spec` section 3: the serving case runs
    /// the whole walk and the diagnostic case ends it at the SPU's
    /// confirmation, forking no gate.
    fn enter(
        &mut self,
        payload: weaver_types::EnterPayload,
        sink: Option<OwnedFd>,
        state_end: Option<OwnedFd>,
    ) -> Result<Run, EnterFailure> {
        let state_member = state_end.is_some();
        // The sink descriptor arrives on the directive's own message, the
        // state channel's end beside it where the member stands, per the
        // ruling of 2026-08-26. The sink's absence refuses and the end's
        // absence is the leg not standing, never a refusal. Nothing is
        // authored yet, so a refusal here leaves the stream clean.
        let sink = sink.ok_or(EnterFailure::BeforeLoad(
            LifecycleRefusal::DescriptorsUnusable,
        ))?;
        let session = payload.session.clone();
        // **The kind selects the record's mechanism**, per apex section 6 as
        // ruled 2026-08-24 and `weaver-harness-Spec` section 9's settled
        // election: a serving run authors the trace and a diagnostic run
        // authors a diagnostic-trace, one shape over both arms, this crate
        // the sole writer under either. The diagnostic record opens with
        // `replay.opened`, the loop's to author, so the load bracket and
        // the seated prefix below are the serving arm's alone.
        let diagnostic = matches!(payload.binding, weaver_types::EnterBinding::Diagnostic);
        let mut recorder = if diagnostic {
            crate::record::Record::from(
                weaver_diagnostic::Recorder::receive(
                    sink,
                    weaver_diagnostic::RunRef(payload.run.0.clone()),
                    weaver_diagnostic::SessionRef(session.0.clone()),
                )
                .map_err(|_| EnterFailure::BeforeLoad(LifecycleRefusal::DescriptorsUnusable))?,
            )
        } else {
            crate::record::Record::from(
                Recorder::receive(
                    sink,
                    RunRef(payload.run.0.clone()),
                    SessionRef(session.0.clone()),
                )
                .map_err(|_| EnterFailure::BeforeLoad(LifecycleRefusal::DescriptorsUnusable))?,
            )
        };

        // The state seam, per `weaver-harness-state-contract` as ruled
        // 2026-08-26: a nameless pair whose harness end arrived on this
        // enter when admin stood the member, and its absence is the leg not
        // standing, never a refused load. Attached before the load event is authored
        // so the run's opening distills like everything after it. The
        // election is the declaration's, resolved by admin and carried in
        // the enter per the contract's sections 3 and 5, converted here at
        // the one site the way the session reference is: the floor's
        // spelling in, the tee's own out. The ask end is a clone of the
        // same channel, per Spec section 6, taken before the tee owns the
        // original, and it stands only where the tee does: one seam, both
        // directions or neither.
        let election = weaver_trace::Election {
            all_kinds: payload.state_election.all_kinds,
            keys: payload
                .state_election
                .keys
                .iter()
                .map(|elected| weaver_trace::ElectedKind {
                    kind: elected.kind.clone(),
                    paths: elected.paths.clone(),
                })
                .collect(),
        };
        // The record carries the same rule the tee applies, cloned before
        // the seam consumes it and written whether or not the seam stands,
        // per `weaver-trace-Spec` section 3: the rule is the load's fact
        // and not the channel's, and absence on a record is reserved for
        // one that predates the member.
        let tee_election = election.clone();
        let mut state_seam = None;
        // The channel arrived in the enter directive as the harness half of
        // the pair admin created at the member's spawn, per the ruling of
        // 2026-08-26: dialed nowhere, derived from nothing, possession the
        // authentication. No end means no member stood, the leg down and
        // the load unrefused.
        if let Some(end) = state_end {
            let channel = std::os::unix::net::UnixStream::from(end);
            match recorder.serving() {
                Some(_) => {
                    let ask_end = channel.try_clone();
                    // The session rides the opener beside the election, both
                    // being facts this enter declared, per the contract's
                    // amended term: the custodian bounds its answers to it,
                    // so a store holding an earlier session answers within
                    // the running one.
                    if let Ok(tee) = weaver_trace::Tee::open(channel, session.0.clone(), election) {
                        recorder.attach_tee(tee);
                        state_seam = ask_end.ok().map(crate::state::StateSeam::new);
                    }
                }
                // **No tee under the diagnostic record**: the tee distills a
                // serving trace into the store, and a replay reads the store
                // through this same channel, which is the ask end whole -
                // the third refusal, nothing writes back to what is under
                // examination, per `weaver-diagnostic-PRD` section 2. **The
                // opener still crosses**, because the contract makes it the
                // channel's first traffic on every standing, tee or no tee:
                // the custodian reads its session bound from it, and a
                // channel whose first line is an ask would have that ask
                // eaten as an opener naming no session. The harness sends
                // it here, the same rendering the tee would have sent.
                None => {
                    use std::io::Write;
                    let mut channel = channel;
                    let frame = weaver_trace::opener(&session.0, &election);
                    if channel.write_all(frame.as_bytes()).is_ok() {
                        state_seam = Some(crate::state::StateSeam::new(channel));
                    }
                }
            }
        }

        // The load event is the run's opening and the origin of its monotonic
        // clock, so the author is constructed at this moment.
        let author = Author::new(&session, &payload.run);
        // **The load declares the posture it was written in**, per
        // `weaver-trace-PRD` section 3.1: each diagnostic election of this
        // enter, named individually and none bundled. This crate is the
        // party that holds them already, and they arrive by two routes per
        // `weaver-harness-Spec` section 6.1: the readout, the field, and
        // the surprisal ride the SPU instruction, and the tee's rule is a
        // sibling of the instruction on the enter payload, converted above
        // for the seam and cloned for the record. The declaration still
        // costs one read of what is in hand. Without it a record holding
        // no field and a record whose election stood and produced nothing
        // are one absence on disk.
        let elections = weaver_trace::Elections {
            // **Two members are not elections and arrive by a third route**,
            // per Spec section 6.1: the member stands where its end arrived
            // on this enter, and the composer is what the binary handed
            // `serve`. Neither is read from the deployment.
            state_member,
            declaration: payload.declaration.clone(),
            // **The load names its lineage, its reset and its stack**, per
            // `weaver-trace-Spec` section 3 on the rulings of 2026-10-02 on
            // #58: admin's facts, copied from the enter and read from no
            // deployment, the lineage present only where the load restored a
            // save point and the reset only where the last run stopped
            // unclean, the member having confirmed the lineage above.
            lineage: payload.restore.as_ref().map(|lineage| {
                Box::new(weaver_trace::Lineage {
                    save_point: lineage.save_point.clone(),
                    run: lineage.run.0.clone(),
                    sequence: lineage.sequence,
                    turn: lineage.turn,
                    named_at_restore: lineage.named_at_restore,
                    built_from: lineage
                        .built_from
                        .as_ref()
                        .map(|branch| weaver_trace::Branch {
                            parent: branch.parent.0.clone(),
                            run: branch.run.0.clone(),
                            through: branch.through,
                        }),
                })
            }),
            reset: payload.reset.as_ref().map(|reset| weaver_trace::Reset {
                prior_run: reset.prior_run.0.clone(),
                reason: reset_reason(reset.reason),
            }),
            stack: payload.stack.clone(),
            // **Boundary, cause and libraries are admin's facts**, per
            // `weaver-trace-Spec` section 3 as of 2026-10-03, copied from the
            // enter and read from no deployment.
            boundary: payload.boundary.clone(),
            cause: crate::engine::trace_cause(payload.cause),
            library_path: payload.library_path.clone(),
            state_store: weaver_trace::StoreIdentity {
                engine: match payload.state_store.engine {
                    weaver_types::StoreEngine::None => "none",
                    weaver_types::StoreEngine::Sqlite => "sqlite",
                }
                .to_string(),
            },
            composer: self
                .composer
                .clone()
                .expect("serve names the composer before any enter"),
            residual_readout: payload.spu_instruction.decoder.residual_readout_election,
            field: payload
                .spu_instruction
                .decoder
                .field_election
                .as_ref()
                .map(|election| election.depth),
            surprisal: payload.spu_instruction.decoder.surprisal_election,
            tee: Some(tee_election),
        };
        // **Every serving enter where the member stands asks `restored`
        // before it authors `load`**, per `weaver-harness-Spec` section 6.1
        // and `weaver-harness-state-contract` section 2 on the ruling of
        // 2026-10-02 on #58's fourth review round: the answered stamp must
        // equal the enter's lineage in its four members, or be absent where
        // the enter names none, and a refusal, a miss or a disagreement
        // refuses the enter before the load, the stream still clean, so no
        // `load` event names state the member did not restore. The dead-peer
        // clause does not convert this ask, and a member whose end arrived
        // but whose seam did not stand cannot be asked, which is the miss.
        //
        // **An enter naming a lineage with no member to ask refuses too**
        // (#99, K8; survey S19): no member stands to have restored it, so
        // the `load` would name state nothing restored, and the arm that
        // skips the ask is the one where nothing is named.
        if !diagnostic {
            let answered = match (state_member, state_seam.as_mut()) {
                (false, None) if payload.restore.is_some() => {
                    return Err(EnterFailure::BeforeLoad(
                        LifecycleRefusal::DescriptorsUnusable,
                    ));
                }
                (false, None) => None,
                (true, None) => {
                    return Err(EnterFailure::BeforeLoad(
                        LifecycleRefusal::DescriptorsUnusable,
                    ));
                }
                (_, Some(seam)) => Some(seam.ask_restored()),
            };
            if let Some(answered) = answered
                && let Err(why) = restored_agrees(answered.as_ref(), payload.restore.as_ref())
            {
                eprintln!(
                    "{}",
                    serde_json::json!({
                        "organ": "harness",
                        "refusal": "restored-disagrees",
                        "why": why,
                    })
                );
                return Err(EnterFailure::BeforeLoad(
                    LifecycleRefusal::DescriptorsUnusable,
                ));
            }
        }
        // **The serving arm's alone**: the diagnostic record carries no
        // `load` kind and opens with `replay.opened`, which is the loop's
        // to author at the replay, per `weaver-diagnostic-Spec` section 4.
        if !diagnostic {
            author
                .author(
                    &mut recorder,
                    Kind::Load,
                    Subsystem::Harness,
                    None,
                    Some(Payload::Elections(Box::new(elections))),
                )
                .map_err(|_| EnterFailure::BeforeLoad(LifecycleRefusal::Malformed))?;
        }

        // **The seated prefix reaches the record beside the load**, per
        // `weaver-harness-Spec` section 6.1 and `weaver-trace-PRD` section 5.
        // The accumulation rule of the trace charter's section 3.2 bases the
        // effective context on the identity prefix, and before this the
        // prefix lived in the configuration alone, so a consumer holding the
        // record could not close the reconstruction. The write is one read of
        // what is in hand: the same instruction the elections came from.
        //
        // A refusal here is a defect in the declaration rather than a reason
        // to abandon the load, per `weaver-harness-PRD` section 5's fifth
        // case: the door refuses a role that is not system and a load that
        // has already bracketed does not fail on a record it could not
        // write, so what is owed is the account rather than the abort. **The
        // refusal is authored rather than dropped**, because the prefix is
        // seated at the session's open regardless and a dropped refusal
        // launders the declaration by omission - the reader cannot tell an
        // agent that seated no prefix from one that seated an unrecorded
        // one, which is the door's stated purpose defeated by its caller.
        // Where the rule that judges a declaration lands is `weaver-types`'
        // to say, and until it does this is the record's only account.
        //
        // Since 2026-09-04 the write happens below, once the open's material
        // is known: the prefix door authors what the open carries, the
        // store's answer or the declaration's seed, and never the seed
        // alone, per `weaver-harness-Spec` section 6.1.

        // **Past this line the bracket stands**, so every refusal below
        // carries the partial run back rather than dropping it: the leave that
        // follows unwinds exactly what stood up, closes the bracket, and
        // drains. Dropping it here would orphan whatever forked and leave an
        // unclosed bracket on the stream.
        // **The leave reads the boundary back**, per `weaver-trace-PRD`
        // section 3.1 as of 2026-09-04, so the enter reads it first: the
        // grants ask goes out once the seam stands under a serving record,
        // and the reading is held on the run for the unload to compare.
        // No member, or a diagnostic record that authors no unload, reads
        // nothing.
        let grants_at_enter = match (recorder.serving().is_some(), state_seam.as_mut()) {
            (true, Some(seam)) => seam.ask_grants(),
            _ => None,
        };
        let mut run = Run {
            classify: None,
            recorder,
            author,
            session,
            operator: payload.operator,
            seeded_bytes: 0,
            run: payload.run.clone(),
            spu: None,
            gate: None,
            tools: None,
            gate_contained: None,
            state: state_seam,
            grants_at_enter,
            load: weaver_types::LoadFacts {
                session: SessionId(payload.session.0.clone()),
                run: payload.run.clone(),
                declaration: payload.declaration.clone(),
                artifact: payload
                    .spu_instruction
                    .decoder
                    .model_binding
                    .artifact
                    .clone(),
                residual_readout: payload.spu_instruction.decoder.residual_readout_election,
                field: payload
                    .spu_instruction
                    .decoder
                    .field_election
                    .as_ref()
                    .map(|election| election.depth),
                surprisal: payload.spu_instruction.decoder.surprisal_election,
                state_election: payload.state_election.clone(),
                state_store: payload.state_store.clone(),
                state_member,
                composer: {
                    let loop_identity = self
                        .composer
                        .clone()
                        .expect("serve names the composer before any enter");
                    weaver_types::Composer {
                        binary: loop_identity.binary,
                        file: loop_identity.file.map(std::path::PathBuf::from),
                        sha256: loop_identity.sha256,
                    }
                },
                // **The bounds the run holds ride on its facts**, from the
                // enter alone (`weaver-harness-Spec` section 6), so admin's
                // deadline reads what this run times its leave by.
                drain_bound: payload.drain_bound,
                wind_down_bound: payload.wind_down_bound,
            },
            fullness: None,
            pressure_reported: false,
            turn_in_flight: None,
            spu_ordinal: 0,
            gate_ordinal: 0,
            held_frames: std::collections::VecDeque::new(),
            // **A restoring run numbers its turns from one**, per
            // `weaver-harness-Spec` section 6.1 on the operator's ruling of
            // 2026-10-02 on #59: a restore is a new branch and so a new run,
            // and the lineage's last turn numbers nothing.
            turn_ordinal: initial_turn_ordinal(payload.restore.as_ref()),
            gate_lowered: false,
            lower_refusal: None,
            drain_bound: payload.drain_bound,
            wind_down_bound: payload.wind_down_bound,
            leave: None,
            diagnostic: diagnostic.then(|| DiagnosticSeat {
                reader_elected: payload.spu_instruction.decoder.residual_readout_election,
                artifact: payload
                    .spu_instruction
                    .decoder
                    .model_binding
                    .artifact
                    .0
                    .clone(),
                replayed: false,
            }),
        };

        macro_rules! after_load {
            ($run:expr, $refusal:expr) => {
                return Err(EnterFailure::AfterLoad(Box::new($run), $refusal))
            };
        }

        // **The identity's one source is the member, under both bindings**,
        // per `weaver-harness-Spec` section 6.1 on the operator's ruling of
        // 2026-10-06 (#1) that the system prompt is state: the open seats the
        // member's `identity` answer, the turnless `message.system` events of
        // the newest run the holdings carry, which under a serving binding is
        // the prompt the seeding turn entered, carried by the save point the
        // load restored, and under a diagnostic binding the preloaded
        // record's own. An empty answer is an agent not yet seeded, which
        // opens with no prefix. The serving ask runs under the bound a member
        // answering from holdings at rest takes; **the diagnostic ask waits
        // on the seal where the door stands**, per
        // `weaver-harness-state-contract` section 2, under the parked ask's
        // bound, and what is waited on is named before the wait, so an
        // operator who has not started the driver reads why the load stands
        // still. A missed ask refuses the enter under either binding, the one
        // ask beside `restored` that the dead-peer clause does not convert,
        // because a run whose bounding cannot be read is not a run with no
        // bounding; the refusal is carried under `DescriptorsUnusable`, the
        // enter's own descriptor for the member having proved unusable for
        // the one ask the enter owes it. A serving enter with no member
        // elected has nothing to ask and seats nothing. **Under a serving
        // binding an answer that returned events is recorded as a `recall`**
        // before the open is built from it, per `weaver-trace-Spec` section
        // 3's recall clause; **under a diagnostic binding the enter records
        // nothing**, as it records no load and no prefix: that record opens
        // with `replay.opened`, and the ask that feeds the model there is the
        // replay port's.
        let ask_bound = if diagnostic {
            crate::state::PARKED_ASK_BOUND_MS
        } else {
            crate::state::ANSWER_BOUND_MS
        };
        if diagnostic && state_member {
            eprintln!("{}", parked_ask_notice(ask_bound));
        }
        let identity = match run.state.as_mut() {
            // The end arrived and the seam did not stand, the clone or the
            // opener's write having failed: the ask the enter owes cannot be
            // made, which is the miss and not the first load.
            None if state_member => after_load!(run, LifecycleRefusal::DescriptorsUnusable),
            None => Vec::new(),
            Some(seam) => match seam.ask_identity_within(ask_bound) {
                None => after_load!(run, LifecycleRefusal::DescriptorsUnusable),
                Some(answered) => {
                    if !diagnostic && !answered.is_empty() {
                        record_enter_ask(
                            &run.author,
                            &mut run.recorder,
                            weaver_trace::RecallVerb::Identity,
                            &answered,
                        );
                    }
                    match crate::state::identity_material(&answered) {
                        Some(material) => material,
                        None => after_load!(run, LifecycleRefusal::DescriptorsUnusable),
                    }
                }
            },
        };
        // **The load authors none of what it seats**, on the operator's
        // ruling of 2026-10-06 (later, #1) that a session's trace and the
        // state it loaded from are not tied together once the agent is
        // unloaded: the load event's lineage names the save point, the
        // recall above names the member's answer, and the prompt's text is
        // on the trace at its seeding turn. The open's messages are the
        // identity, prefix material permanent for the residency, per Spec
        // section 6.1.
        let opening = identity.clone();
        // The residency and decode pairs are created in one act before the SPU
        // fork: a socket with no address cannot be reached later by resolving
        // one, so the only moment it can reach a child is before that child
        // exists.
        let (lifecycle, lifecycle_child) = match OrganChannel::pair() {
            Ok(pair) => pair,
            Err(_) => after_load!(run, LifecycleRefusal::DescriptorsUnusable),
        };
        let (decode, decode_child) = match DecodeChannel::pair() {
            Ok(pair) => pair,
            Err(_) => after_load!(run, LifecycleRefusal::DescriptorsUnusable),
        };
        // SAFETY: the fork runs on the serving thread, whose lifetime is the
        // worker's, and the child performs only the async-signal-safe calls of
        // `place_child_ends` before its exec.
        let (spu_word, spu_stderr) = match crate::spawn::LastWord::stand() {
            Ok(pair) => pair,
            Err(_) => after_load!(run, LifecycleRefusal::DescriptorsUnusable),
        };
        let spu_pid = match unsafe {
            crate::spawn::fork_organ(
                &self.organs.spu,
                &[lifecycle_child.as_fd(), decode_child.as_fd()],
                &self.parameters.spu_arguments(),
                Some(std::os::fd::AsRawFd::as_raw_fd(&spu_stderr)),
            )
        } {
            Ok(pid) => pid,
            Err(_) => after_load!(run, LifecycleRefusal::BindFailed),
        };
        drop(lifecycle_child);
        drop(decode_child);
        // The parent's write-end copy closes here, or the reader never sees
        // EOF and the last word never settles.
        drop(spu_stderr);
        run.spu = Some(SpuChannels {
            lifecycle,
            decode,
            pid: spu_pid,
            last_word: spu_word,
        });

        // Admit carries the SPU instruction uninterpreted.
        run.spu_ordinal += 1;
        let ordinal = run.spu_ordinal;
        let spu = run.spu.as_ref().expect("the arm just stood up");
        if let Err(refusal) = exchange(
            &spu.lifecycle,
            Opener::Harness,
            ordinal,
            weaver_types::RefusingOrgan::Spu,
            LifecycleDirective::Admit {
                instruction: payload.spu_instruction.clone(),
            },
        ) {
            // A closed channel means the organ is gone: its exit status says
            // whether the placement or the exec failed, which the exchange
            // itself could only report as an absent residency.
            let refusal = match refusal {
                LifecycleRefusal::NoResidency => {
                    let pid = run.spu.as_ref().expect("the arm stood up").pid;
                    classify_organ_death(pid).unwrap_or(LifecycleRefusal::NoResidency)
                }
                other => other,
            };
            after_load!(run, refusal);
        }

        // **The decode session opens once residency confirms**, per
        // `weaver-harness-Spec` section 6.1: the session opens at the enter
        // fan-out and not at the first turn, so the interior loop 0 grants is
        // a session at rest. The open carries the identity material resolved
        // above as its messages and the run's session as its session. A refused open is a refused enter,
        // returned after-load so the bracket stands for the leave.
        let spu = run.spu.as_ref().expect("residency stands");
        // **The column ask crosses where and only where the binding is
        // diagnostic and the readout is elected**, per `weaver-spu-PRD`
        // section 13.7's cadence election and the decode contract's
        // supplies clause: a serving harness never writes the ask, and a
        // diagnostic load without the readout election has no tap running
        // and no column to ask for - the ask would meet the registry's
        // second arm.
        let column_ask = column_ask_for(
            run.diagnostic.is_some(),
            payload.spu_instruction.decoder.residual_readout_election,
        );
        if let Err(refusal) = open_session(
            &spu.decode,
            SessionId(run.session.0.clone()),
            opening,
            column_ask,
        ) {
            after_load!(run, refusal);
        }

        // The classify arm, per `weaver-harness-Spec` section 6 and
        // `weaver-spu-PRD` section 15.3: where the declaration carries the
        // binding the fan-out grows this arm, the model having admitted
        // first so a device too small for both refuses deterministically at
        // this arm and names it. Absent binding, no process: the port then
        // serves the missing leg's absence.
        if let Some(instruction) = payload.spu_instruction.classify.as_ref() {
            let Some(binary) = self.organs.classify.as_ref() else {
                // A binding with no binary provisioned is half an arm, and
                // half an arm refuses rather than standing.
                after_load!(
                    run,
                    LifecycleRefusal::ConfigInvalid {
                        field: Some(weaver_types::FieldName("classify".into())),
                    }
                );
            };
            let (channel, child) = match crate::channel::ClassifyChannel::pair() {
                Ok(pair) => pair,
                Err(_) => after_load!(run, LifecycleRefusal::DescriptorsUnusable),
            };
            let Some(device) = instruction.model_binding.devices.first() else {
                after_load!(
                    run,
                    LifecycleRefusal::ConfigInvalid {
                        field: Some(weaver_types::FieldName("classify".into())),
                    }
                );
            };
            let arguments = vec![
                instruction.model_binding.artifact.0.clone(),
                device.0.to_string(),
            ];
            // SAFETY: as the SPU's fork above, on the serving thread, the
            // child performing only the async-signal-safe calls.
            let (classify_word, classify_stderr) = match crate::spawn::LastWord::stand() {
                Ok(pair) => pair,
                Err(_) => after_load!(run, LifecycleRefusal::DescriptorsUnusable),
            };
            let pid = match unsafe {
                crate::spawn::fork_organ(
                    binary,
                    &[child.as_fd()],
                    &arguments,
                    Some(std::os::fd::AsRawFd::as_raw_fd(&classify_stderr)),
                )
            } {
                Ok(pid) => pid,
                Err(_) => after_load!(run, LifecycleRefusal::BindFailed),
            };
            drop(child);
            drop(classify_stderr);
            let arm = ClassifyArm {
                channel,
                pid,
                last_word: classify_word,
            };
            // Readiness gates service, per the contract: the seam's first
            // message is the admission's outcome, and a typed refusal, a
            // closure, or the bound's expiry refuses the load whole. The
            // wait is bounded because a child hung mid-admission would
            // otherwise hold the load forever, and the expiry path kills
            // before it reaps: a process still loading weights does not
            // exit on a channel it has not read yet.
            match arm.channel.recv_reply_within(CLASSIFY_READY_BOUND_MS) {
                Ok(crate::channel::ClassifyReply::Answer(weaver_types::LabelAnswer::Ready)) => {
                    run.classify = Some(arm);
                }
                _ => {
                    drop(arm.channel);
                    let _ = nix::sys::signal::kill(arm.pid, nix::sys::signal::Signal::SIGKILL);
                    reap(arm.pid);
                    after_load!(run, LifecycleRefusal::NoResidency);
                }
            }
        }

        // The fan-out's extent follows the kind, per `weaver-harness-Spec`
        // section 3: the serving case runs the gate step and the diagnostic
        // case ends the fan-out at the SPU's confirmation - no pair created,
        // no binary forked, no raise opened. The run state's gate arm stays
        // empty for the whole residency, so the leave and the unwind lower
        // what stands, reading the same option they always read.
        let gate_instruction = match &payload.binding {
            weaver_types::EnterBinding::Serving { gate_instruction } => gate_instruction,
            weaver_types::EnterBinding::Diagnostic => return Ok(run),
        };

        // **A worker that cannot open or judge `tools/` stands no gate**
        // (`weaver-harness-Spec` section 2.2; `weaver-admin-Spec` 6.1, the
        // worker's row): the gate's arm refuses `BoundaryUnverified`, wrapped
        // `OrganRefused` as any refusal of the gate's arm is, and the load
        // rolls back. Opened once, here, and never resolved by path again.
        let tools = match open_tools() {
            Ok(tools) => tools,
            Err(refusal) => after_load!(run, refusal),
        };
        // The gate pair is created only after the SPU's answer confirms
        // residency: one organ's readiness gates another organ's
        // construction, which neither organ can see from inside its domain.
        let (gate, gate_child) = match OrganChannel::pair() {
            Ok(pair) => pair,
            Err(_) => after_load!(run, LifecycleRefusal::DescriptorsUnusable),
        };
        // SAFETY: as above.
        let (gate_word, gate_stderr) = match crate::spawn::LastWord::stand() {
            Ok(pair) => pair,
            Err(_) => after_load!(run, LifecycleRefusal::DescriptorsUnusable),
        };
        let gate_pid = match unsafe {
            fork_gate(
                &self.organs.gate,
                &gate_child,
                std::os::fd::AsFd::as_fd(&tools),
                Some(std::os::fd::AsRawFd::as_raw_fd(&gate_stderr)),
            )
        } {
            Ok(pid) => pid,
            Err(_) => after_load!(run, LifecycleRefusal::BindFailed),
        };
        drop(gate_child);
        drop(gate_stderr);
        run.tools = Some(tools);
        run.gate_ordinal += 1;
        if let Err(refusal) = exchange(
            &gate,
            Opener::Harness,
            run.gate_ordinal,
            weaver_types::RefusingOrgan::Gate,
            LifecycleDirective::Raise {
                instruction: gate_instruction.clone(),
                // The socket is this crate's rather than the declaration's,
                // per `weaver-gate-PRD` section 2, named beside the
                // coordination socket so the manager's runtime directory
                // covers both and no pathname outlives its worker.
                socket: self.coordination.gate_socket(),
            },
        ) {
            let refusal = match refusal {
                // Only a classified death replaces the exchange's own
                // refusal: an unreadable status is not evidence of a failed
                // bind, and claiming one would tell the peer something this
                // crate does not know.
                LifecycleRefusal::NoResidency => {
                    classify_organ_death(gate_pid).unwrap_or(LifecycleRefusal::NoResidency)
                }
                other => other,
            };
            // The gate stood up far enough to reap even though it refused.
            run.gate = Some(GateChannel {
                channel: gate,
                pid: gate_pid,
                last_word: gate_word,
            });
            after_load!(run, refusal);
        }
        run.gate = Some(GateChannel {
            channel: gate,
            pid: gate_pid,
            last_word: gate_word,
        });

        Ok(run)
    }

    /// The extension seam, crossed at loaded-and-idle: loop 0 hands a standing
    /// interior to whatever loop 1 the binary carries, and takes it back at
    /// the stop and at the leave. A loop composes what this grants or does not
    /// compile - there is no call by which it mints a port.
    pub fn grant_seat(
        &mut self,
        identity: &str,
        tool_schemas: &[String],
    ) -> Option<crate::Ports<'_>> {
        match &mut self.state {
            // **The extension seam is crossed at loaded-and-idle itself.** A
            // turn in flight is the active state, not the idle one, and loop 0
            // has not taken the interior back yet, so there is no standing
            // interior to hand across.
            ChannelState::Entered(run) if run.turn_in_flight.is_some() => None,
            // **No seat over a latched record** (#116).
            ChannelState::Entered(run) if run.recorder.failed() => None,
            ChannelState::Entered(run) => {
                // **A diagnostic run grants no seat here.** Its record holds
                // no working structure and no frame ever arrives to assemble
                // for - the one grant is the run's opening, per Spec 6.2's
                // second criterion - so this seam answers the same
                // nothing-to-hand-across the other arms give rather than
                // panicking on a record shape the guard above never read.
                let structure = run.recorder.structure()?;
                let prompt = crate::assembly::assemble(structure, identity, tool_schemas);
                // **An incomplete prompt does not cross the seam.** A
                // message-kind record that did not decode is a hole where a
                // turn's content was, so the count becomes the `fault` event
                // this crate authors and the seat is not granted: handing a
                // loop a prompt with a hole would put the loss in the model's
                // context and nowhere else.
                if prompt.undecodable > 0 {
                    let account = format!(
                        "{{\"organ\":\"harness\",\"undecodable-message-records\":{}}}",
                        prompt.undecodable
                    );
                    let turn = run.turn_in_flight.clone();
                    // Latched on a failure (#116), read at the serve loop's next wake.
                    let _ = run.author.author_fault(
                        &mut run.recorder,
                        Subsystem::Harness,
                        turn.as_ref(),
                        &crate::authorship::harness_report(
                            weaver_types::FaultCase::MessageRecordUndecodable,
                            &account,
                        ),
                    );
                    return None;
                }
                let spu = run.spu.as_ref()?;
                Some(crate::engine::Ports::grant(
                    &spu.decode,
                    &run.author,
                    &mut run.recorder,
                    &mut run.turn_ordinal,
                    &mut run.turn_in_flight,
                    &run.load,
                    Some(prompt),
                    &self.coordination,
                    // A seat granted outside the serve loop streams without
                    // the ear, the verb slot being the loop's own, and
                    // executes no tools: the dev boundary's seat reasons and
                    // the serve loop's seat reaches the world.
                    None,
                    None,
                    run.state.as_mut(),
                    run.classify.as_ref().map(|arm| &arm.channel),
                    &mut run.fullness,
                    &mut run.pressure_reported,
                ))
            }
            // Before enter and after leave there is no standing interior to
            // hand across, which is the bracket discipline being loop 0's.
            _ => None,
        }
    }

    /// Closes the exchange the directive opened. Admin opens every
    /// coordination exchange, so the answer carries the identity that arrived
    /// rather than a fresh one this crate numbered.
    /// The answer goes to the connection the directive arrived on, not to a
    /// channel this crate holds: the listener is what it holds, and each verb
    /// brings its own connection.
    /// **The record holds, or service ends** (#116; `weaver-harness-Spec`
    /// section 4): the latch of a failed authoring on the standing run's
    /// record, read before any answer is announced and at every wake, so the
    /// run never answers or serves on over a record it is missing. It ends
    /// service as an organ death the record cannot hold does (62aa181),
    /// tearing nothing down here.
    pub(crate) fn record_holds(&self) -> Result<(), ChannelFault> {
        match &self.state {
            ChannelState::Entered(run) if run.recorder.failed() => Err(ChannelFault::Closed),
            _ => Ok(()),
        }
    }

    /// An answer to admin, announced only over a record that holds.
    fn answer(
        &mut self,
        connection: &OrganChannel,
        exchange: &ExchangeId,
        answer: LifecycleAnswer,
    ) -> Result<(), ChannelFault> {
        self.record_holds()?;
        deliver(
            connection,
            &OrganEnvelope {
                exchange: exchange.clone(),
                position: Position::Close,
                payload: weaver_types::Payload::Answer(answer),
            },
        )
    }

    /// A refusal to admin, announced only over a record that holds.
    fn refuse(
        &mut self,
        connection: &OrganChannel,
        exchange: &ExchangeId,
        refusal: LifecycleRefusal,
    ) -> Result<(), ChannelFault> {
        self.record_holds()?;
        deliver(
            connection,
            &OrganEnvelope {
                exchange: exchange.clone(),
                position: Position::Close,
                payload: weaver_types::Payload::Refusal(refusal),
            },
        )
    }
}

/// **An answer the dialer is not there to take ends nothing** (the operator's
/// ruling of 2026-10-08 on #99, N4): the verb's caller went away, an
/// operator's interrupt, a connector's own timeout or a killed invocation,
/// and the run it asked about stands as the directive left it. The send's
/// failure is said on standard error and the serve loop returns to its wait,
/// where the dead connection reads closed, as a verb answered and closed
/// does; only the listener's loss ends service. A run's end is the directive
/// that ended it, never a failed write to the party that asked.
fn deliver(connection: &OrganChannel, envelope: &OrganEnvelope) -> Result<(), ChannelFault> {
    if let Err(fault) = connection.send(envelope) {
        eprintln!(
            "weaver-harness: the answer to exchange {} was not delivered ({fault:?}); the dialer is gone and the run stands as the directive left it",
            envelope.exchange.ordinal
        );
    }
    Ok(())
}

/// Leave runs the reverse order and drains before it answers: lower the gate
/// first, author the unload event, drain the writer's queue, and release the
/// SPU. Left is answered only after the drain returns, which is what makes the
/// answer mean that everything admitted reached the stream.
///
/// The match on the options is the checked unwind: a forgotten arm is a
/// compile error rather than a leaked residency.
fn leave(
    run: &mut Run,
    cause: Option<weaver_types::Cause>,
    forced: bool,
) -> Result<(), LifecycleRefusal> {
    let shared = shared_deadline();
    let (lowered, _) = lower_gate(run, shared);
    leave_after(run, cause, forced, None, lowered, shared)
}

/// **The shared deadline** (`weaver-harness-Spec` section 6): taken at the
/// lower's start, the leave bound from it, which the trace's drain, the SPU's
/// `Release` and the reaps share with whatever the lower and the save
/// point's legs leave.
fn shared_deadline() -> Option<std::time::Instant> {
    std::time::Instant::now().checked_add(std::time::Duration::from_millis(
        weaver_types::LEAVE_BOUND_MS,
    ))
}

/// **Lower the gate**, the leave's first step, taken on its own before the
/// leave's save point (Codex on #94, round 12) so no turn is admitted while
/// the snapshot runs: the lower is an exchange, not a shot, its confirmation
/// being what admin's leave aggregate rests on, so the answer is read before
/// the channel drops rather than raced against it; a refusal is answered to
/// the caller to name as the leave's first. A run with no gate standing
/// lowers nothing.
fn lower_gate(
    run: &mut Run,
    shared: Option<std::time::Instant>,
) -> (Option<LifecycleRefusal>, bool) {
    lower_gate_within(run, weaver_types::LOWER_BOUND_MS, shared)
}

/// [`lower_gate`] under a stated bound, the tests' short one.
fn lower_gate_within(
    run: &mut Run,
    bound_ms: u64,
    shared: Option<std::time::Instant>,
) -> (Option<LifecycleRefusal>, bool) {
    let Some(gate) = run.gate.take() else {
        // From S9, the gate already lowered: a refusal that lower answered
        // is this unwind's first (#140, S15). `tools/` was swept at that
        // lower.
        return (run.lower_refusal.take(), false);
    };
    // Held as its own fact, since a gate that died leaves no gate too: a
    // leave stopped after this stands the run in S9 (3.0).
    run.gate_lowered = true;
    run.gate_ordinal += 1;
    // **The lower's window bounds the gate's reap too**: the save point's
    // legs come after the lower, so a gate that never exits is killed at the
    // lower's bound, never held to the shared deadline (the self-check of B2).
    let window = std::time::Instant::now().checked_add(std::time::Duration::from_millis(bound_ms));
    let lowered = lower_exchange(run, &gate.channel, run.gate_ordinal, bound_ms);
    // **Answered or not, the channel closes and the gate is reaped**: a gate
    // that did not answer inside the bound meets its channel closed, which is
    // death to it (`weaver-harness-gate-contract` section 1).
    // **A gate that died at the lower** (S6 x the gate dies) is no refusal
    // of the leave: its death is a `fault`, and the caller turns the leave
    // forced (A). The second value says it died.
    use std::os::fd::AsFd;
    let died = lowered.is_err() && hung_up(gate.channel.as_fd());
    // **The unwind's kill of `tools/`, before `Left`** (6.1, the worker's
    // unwind; `weaver-harness-Spec` section 2.2): every leave lowers the gate
    // here, before its save point and its `unload`, so this is the last
    // sweep, the write or one pinned pass, never a wait. After every
    // answered execution `tools/` is already empty.
    let contained = run
        .gate_contained
        .take()
        .or_else(|| contain_tools(run, crate::tools_cgroup::Context::Leave));
    if died {
        author_gate_death(
            run,
            gate.last_word.take(),
            "its channel was lost at the lower",
            contained,
        );
    } else if matches!(
        contained,
        Some((_, crate::tools_cgroup::Containment::Undelivered))
    ) {
        // `Left` is answered all the same, the account naming the failed
        // containment (6.1, the unwind with both failing).
        author_gate_death(run, None, "lowered at the unwind", contained);
    }
    drop(gate.channel);
    reap_by(gate.pid, [window, shared].into_iter().flatten().min());
    (if died { None } else { lowered.err() }, died)
}

/// **The lower reads its channel until the gate's own answer**, per Spec
/// section 6 (Codex on #94, round 14): a frame the gate admitted before it
/// took the `Lower` reaches this channel ahead of `GateStopped`, and a lower
/// that read one message would take it for a malformed answer, the record
/// silent. Every frame met here is authored as a lifecycle refusal of the
/// leave, so the record has its line; a fault report the gate sends
/// meanwhile is authored as the serve loop authors one. **No response is
/// sent for it** (Codex on #94, round 15): the gate drops its relay and with
/// it every served connection when it takes the `Lower`, before it answers
/// `GateStopped`, so a response written now would be discarded and its
/// dialer reads the end of the connection whatever this crate sends. A
/// graceful leave's drain has already refused, through the gate, what the
/// gate admitted before its quiesce ([`drain_gate`]); this loop meets what a
/// forced leave's lower finds standing.
///
/// **The wait is bounded** (`weaver-harness-Spec` section 6, the lower bound):
/// one deadline, `bound_ms` from the `Lower`'s send, and a gate that has not
/// answered by it is not waited on further. That is no refusal of the leave,
/// which goes on; the caller closes the channel and reaps. Each frame met is
/// recorded and the read goes on at once, so a full channel costs the reads
/// and nothing more.
fn lower_exchange(
    run: &mut Run,
    channel: &OrganChannel,
    ordinal: u64,
    bound_ms: u64,
) -> Result<(), LifecycleRefusal> {
    channel
        .send(&OrganEnvelope {
            exchange: ExchangeId {
                opener: Opener::Harness,
                ordinal,
            },
            position: Position::Open,
            payload: weaver_types::Payload::Directive(LifecycleDirective::Lower),
        })
        .map_err(|_| LifecycleRefusal::NoResidency)?;
    let deadline = std::time::Instant::now()
        .checked_add(std::time::Duration::from_millis(bound_ms))
        .ok_or(LifecycleRefusal::Malformed)?;
    loop {
        if !readable_by(channel, deadline) {
            return Ok(());
        }
        let envelope = channel.recv().map_err(|_| LifecycleRefusal::NoResidency)?;
        match envelope.payload {
            weaver_types::Payload::Frame(_) if envelope.exchange.opener == Opener::Gate => {
                // Recorded, and not answered: the gate dropped this frame's
                // connection when it took the `Lower`.
                author_refusal_on(
                    run,
                    weaver_types::LifecycleAsk::Leave,
                    &LifecycleRefusal::Unloading,
                );
            }
            weaver_types::Payload::Fault(report) => {
                // Latched on a failure (#116), read at the leave's seal, which refuses
                // `DescriptorsUnusable`.
                let _ = run
                    .author
                    .author_fault(&mut run.recorder, Subsystem::Gate, None, &report);
            }
            // **Only the lower's own answer ends it** (Codex on #119, round
            // 3): an answer or a refusal to another exchange, the drain's
            // `GateQuiesced` left queued when a join cut the drain short, is
            // read past, so a frame the gate still owes ahead of its
            // `GateStopped` is read and recorded before the channel closes.
            weaver_types::Payload::Answer(_) | weaver_types::Payload::Refusal(_)
                if envelope.exchange
                    != (ExchangeId {
                        opener: Opener::Harness,
                        ordinal,
                    }) => {}
            weaver_types::Payload::Answer(LifecycleAnswer::GateStopped) => return Ok(()),
            weaver_types::Payload::Answer(_) => return Err(LifecycleRefusal::Malformed),
            weaver_types::Payload::Refusal(reason) => {
                return Err(LifecycleRefusal::OrganRefused {
                    organ: weaver_types::RefusingOrgan::Gate,
                    reason: Box::new(reason),
                });
            }
            _ => return Err(LifecycleRefusal::Malformed),
        }
    }
}

/// The leave after its gate is lowered: the rest of the unwind, whole.
fn leave_after(
    run: &mut Run,
    cause: Option<weaver_types::Cause>,
    forced: bool,
    forced_by: Option<weaver_types::Cause>,
    lowered: Option<LifecycleRefusal>,
    shared: Option<std::time::Instant>,
) -> Result<(), LifecycleRefusal> {
    // The unwind runs whole whatever refuses along it, because stopping at
    // the first refusal leaks everything after it: a refused lower must not
    // leave a device held. The first refusal in sequence order is what the
    // answer names, per the contract's a-refusal-names-where-it-stopped.
    let first_refusal = lowered;
    if let Some(classify) = run.classify.take() {
        // Closure is the release, per the classify contract's failure
        // section: the process exits on its seam's close and the reap reads
        // it out, the arm falling second because it stood second to last.
        drop(classify.channel);
        reap_by(classify.pid, shared);
    }
    // The serving arm's bracket close: the diagnostic record carries no
    // `unload`, its own close being `replay.closed`, the loop's to author,
    // per `weaver-diagnostic-Spec` section 4.
    let serving = run.recorder.serving().is_some();
    // **The grant surface read back**, per `weaver-trace-PRD` section 3.1 as
    // of 2026-09-04, before the member is released: where a member stood,
    // the leave asks again and the event carries the comparison, a surface
    // that varied inside the session being a boundary move the record
    // carries and never absorbs, and one unreadable at the close said to be
    // so rather than reported unchanged. Where no member stood there was no
    // boundary to read and the surface is absent.
    let grant_surface = match run.state.as_mut() {
        Some(seam) if serving => {
            let at_leave = seam.ask_grants();
            Some(match (&run.grants_at_enter, at_leave) {
                (Some(entered), Some(left)) if *entered == left => {
                    weaver_trace::GrantSurface::Unchanged
                }
                (Some(_), Some(_)) => weaver_trace::GrantSurface::Varied,
                _ => weaver_trace::GrantSurface::Unreadable,
            })
        }
        _ => None,
    };
    // **The release, before `unload`** (`weaver-harness-Spec` section 6,
    // item 6; I9): the SPU by its `Release`, the member by closing the
    // state seam, which has no release exchange. A release unconfirmed is
    // recorded on `unload` and the leave goes on.
    // **The record holds or no `Left`**: a release fault the writer could
    // not take is a writer that has failed, and the drain below refuses the
    // leave on it.
    let spu = release_spu(run, serving, shared);
    // `Closed` says only that the harness closed its ask end, the tee's
    // closing with the writer after `unload` crosses it; one already
    // dead reads `Unconfirmed`, a dead peer being no fault.
    let member = run.state.take().map(|seam| {
        if seam.dead() {
            weaver_trace::MemberRelease::Unconfirmed
        } else {
            weaver_trace::MemberRelease::Closed
        }
    });
    // The pressure reading, then `unload`, the run's terminal event.
    // **A full queue takes nothing more but `unload`**, whose slot the writer
    // reserves (`weaver-trace-Spec` section 7): any other record would block,
    // so the leave's optional records wait on room below the reserve.
    let has_room = |run: &Run| {
        run.recorder.pressure().queued < weaver_trace::QUEUE_DEPTH - weaver_trace::UNLOAD_RESERVE
    };
    if has_room(run) {
        crate::engine::report_pressure(&mut run.recorder, &run.author, &mut run.pressure_reported);
    }
    // **The trace's drain under the shared deadline**: what the writer has
    // not taken by it is forfeited, named before `unload` where the queue
    // can still take the naming, and forfeited unnamed where it cannot.
    if let Some(deadline) = shared
        && let Ok(weaver_trace::Drained::Expired { queued }) = run.recorder.drain_by(deadline)
        && has_room(run)
    {
        let account = serde_json::json!({
            "organ": "harness",
            "forfeited": queued,
            "at": "the leave's shared deadline",
        })
        .to_string();
        // Latched on a failure (#116), read at the leave's seal, which refuses
        // `DescriptorsUnusable`.
        let _ = run.author.author_fault(
            &mut run.recorder,
            Subsystem::Harness,
            None,
            &crate::authorship::harness_report(
                weaver_types::FaultCase::StreamWriteFailed,
                &account,
            ),
        );
    }
    if serving {
        let payload = Some(weaver_trace::Payload::Unload(weaver_trace::UnloadClose {
            grant_surface,
            cause: cause.map(crate::engine::trace_cause),
            forced,
            forced_by: forced_by.map(crate::engine::trace_cause),
            release: Some(weaver_trace::Release { spu, member }),
        }));
        // Latched on a failure (#116), read at the leave's seal, which refuses
        // `DescriptorsUnusable`.
        let _ = run.author.author(
            &mut run.recorder,
            Kind::Unload,
            Subsystem::Harness,
            None,
            payload,
        );
        // **The leave's save point was taken before this event**, by the
        // directive's arm per `weaver-harness-Spec` section 6 on the
        // operator's rulings of 2026-10-06 on #1 (A3.0 items 4 and 6), so the
        // record names an unload only when the leave completes; a forced
        // leave took none and the event says so.
    }

    // **The drain's outcome is carried, not discarded.** `Left` means
    // everything admitted reached the stream, so a failed drain must not be
    // answered as one: the peer could not otherwise tell a complete stream
    // from a truncated one.
    // **No `unload` on the stream, no `Left`** (I2, on the Planner's ruling
    // of 2026-10-10): past the shared deadline the wait stops, the tail
    // forfeited, and a drain that did not reach `unload` refuses the leave,
    // admin concluding as for no answer.
    let drained = match shared {
        Some(deadline) => match run.recorder.drain_by(deadline) {
            Ok(weaver_trace::Drained::Emptied) => Ok(()),
            // The writer is abandoned with its queue (#117): the worker
            // must exit, and a stuck sink would otherwise hold it.
            Ok(weaver_trace::Drained::Expired { .. }) => {
                run.recorder.abandon();
                Err(LifecycleRefusal::DescriptorsUnusable)
            }
            Err(_) => Err(LifecycleRefusal::DescriptorsUnusable),
        },
        None => run
            .recorder
            .drain()
            .map_err(|_| LifecycleRefusal::DescriptorsUnusable),
    };
    // **The queue's peak, outside the record** (#120; `weaver-harness-Spec`
    // section 4): after the last drain, one line on standard error, which
    // admin's start step points at worker.log. A diagnostic record queues
    // nothing and says nothing. **A failed write is no fault of the leave's**:
    // `eprintln!` panics where standard error cannot take the line (a full
    // disk under worker.log), which would end the worker before its answer,
    // so the write's error is dropped, as `channel.rs` drops its own.
    if serving {
        use std::io::Write;
        let _ = writeln!(
            std::io::stderr(),
            "{}",
            queue_peak_notice(run.recorder.pressure().peak)
        );
    }
    if let Some(refusal) = first_refusal {
        return Err(refusal);
    }
    // **A record missing an event is not answered `Left` either** (#116): the
    // drain sees a writer that failed, and the latch also sees a submission
    // refused on its own, the writer sound.
    if run.recorder.failed() {
        return Err(LifecycleRefusal::DescriptorsUnusable);
    }
    // The stream did not close cleanly, which is a fault the operator learns
    // from the refusal rather than from a silence.
    drained
}

/// **The SPU's release** (`weaver-harness-Spec` section 6, item 6): its
/// `Release` answered `Released` is `Confirmed`, anything else `Unconfirmed`,
/// and an SPU absent, already dead, has nothing to release and reads
/// `Unconfirmed` with no second fault. **An SPU that dies in it is a
/// `fault`** (`weaver-harness-spu-contract` section 5; 3.0, S7 x the SPU
/// dies), authored here, before `unload`, on a serving record: a diagnostic
/// record carries no `unload` and nothing outside its replay's bracket
/// (`weaver-diagnostic-Spec` section 4).
fn release_spu(
    run: &mut Run,
    serving: bool,
    shared: Option<std::time::Instant>,
) -> weaver_trace::ReleaseOutcome {
    let Some(spu) = run.spu.take() else {
        return weaver_trace::ReleaseOutcome::Unconfirmed;
    };
    run.spu_ordinal += 1;
    // **The decode end drops first**, ending the worker's decode phase so
    // the release lands on a seam that is listening, per the serve loop's
    // one-loop rule: from admit until its decode end closes, the worker reads
    // nothing else. Issue 113's repair, and the ordering the seam tests
    // always had.
    drop(spu.decode);
    // The release is the exchange the contract says it is: confirmed after
    // the device is free and never before, so the answer is read before the
    // channel drops.
    // An answer not read by the shared deadline is `Unconfirmed`.
    let released = exchange_by(
        &spu.lifecycle,
        Opener::Harness,
        run.spu_ordinal,
        weaver_types::RefusingOrgan::Spu,
        LifecycleDirective::Release,
        shared,
    );
    let outcome = match released {
        Ok(LifecycleAnswer::Released) => weaver_trace::ReleaseOutcome::Confirmed,
        _ => {
            use std::os::fd::AsFd;
            // Authored where the queue has room below `unload`'s reserve.
            let room = run.recorder.pressure().queued
                < weaver_trace::QUEUE_DEPTH - weaver_trace::UNLOAD_RESERVE;
            if serving && room && hung_up(spu.lifecycle.as_fd()) {
                let account = serde_json::json!({
                    "organ": "spu",
                    "death": "its channel was lost in the release",
                })
                .to_string();
                // Latched on a failure (#116), read at the leave's seal, which refuses
                // `DescriptorsUnusable`.
                let _ = run.author.author_fault(
                    &mut run.recorder,
                    Subsystem::Harness,
                    None,
                    &crate::authorship::harness_report(
                        weaver_types::FaultCase::OrganDeathObserved,
                        &account,
                    ),
                );
            }
            weaver_trace::ReleaseOutcome::Unconfirmed
        }
    };
    drop(spu.lifecycle);
    reap_by(spu.pid, shared);
    outcome
}

/// Reads a dead organ's exit status and names what killed it, so a placement
/// fault is not reported to the peer as a residency problem.
///
/// **The child writes no status**, and that is deliberate rather than an
/// omission: a setup-status pipe would put a fourth call between fork and
/// exec, and `weaver-harness-Spec` section 2.2 enumerates exactly three "and
/// nothing else". The exit status carries the same fact without adding one,
/// and it is read here - after the exchange observed closure - where the child
/// is already dead and there is no race to lose.
/// Open the decode session once residency confirms, per
/// `weaver-harness-Spec` section 6.1. The identity messages are the open's
/// messages and the run's session its session, and a refusal or a fault below
/// the exchange maps onto the load-refusal set the enter aggregate carries.
fn open_session(
    decode: &DecodeChannel,
    session: SessionId,
    identity: Vec<weaver_traits::Message>,
    column_ask: bool,
) -> Result<(), LifecycleRefusal> {
    decode
        .send_directive(&TokenDirective::Open {
            session,
            messages: identity,
            column_ask,
        })
        .map_err(|_| LifecycleRefusal::NoResidency)?;
    match decode.recv_reply() {
        Ok(crate::channel::DecodeReply::Answer(TokenAnswer::Opened)) => Ok(()),
        // The emission, matched by name: a fault at open is the residency
        // unfit to serve, not the load's shape refused, and the wildcard
        // below would misname it. No recorder stands this early, so the
        // refusal carries the fact to the aggregate rather than the record.
        Ok(crate::channel::DecodeReply::Answer(TokenAnswer::Fault(_))) => {
            Err(LifecycleRefusal::NoResidency)
        }
        // A typed refusal on the open is the session declining to stand, which
        // the harness carries into the aggregate as the SPU unable to admit
        // what the load asked, the decode seam's refusals having no floor
        // lifecycle case of their own.
        Ok(_) => Err(LifecycleRefusal::DeviceCannotAdmit),
        // A fault below the exchange, the worker gone or the octets
        // undecodable, is the residency lost the moment it was confirmed.
        Err(_) => Err(LifecycleRefusal::NoResidency),
    }
}

fn classify_organ_death(pid: nix::unistd::Pid) -> Option<LifecycleRefusal> {
    // **The wait does not block.** An organ that closed its end and kept
    // running would otherwise hold the serving thread here forever, which is
    // a worse failure than the misreport this classification exists to
    // prevent. A child still running, or a status this call cannot read,
    // yields nothing and the caller keeps the refusal the exchange produced.
    match nix::sys::wait::waitpid(pid, Some(nix::sys::wait::WaitPidFlag::WNOHANG)) {
        Ok(nix::sys::wait::WaitStatus::Exited(_, crate::spawn::PLACEMENT_FAILED)) => {
            // The ends this organ was owed never reached descriptor 3, which
            // is a descriptor fault and not a residency one.
            Some(LifecycleRefusal::DescriptorsUnusable)
        }
        Ok(nix::sys::wait::WaitStatus::Exited(
            _,
            crate::spawn::EXEC_FAILED | crate::spawn::DEATH_SIGNAL_REFUSED,
        )) => {
            // The organ binary never ran, its exec failing or the kernel
            // refusing the death signal that binds it to the worker, so
            // nothing it was asked to stand up could have stood up.
            Some(LifecycleRefusal::BindFailed)
        }
        _ => None,
    }
}

/// **The drain's quiesce** (`weaver-harness-gate-contract` section 2): the
/// gate is told `Quiesce`, and its channel is read to `GateQuiesced`. Every
/// frame met there, and every frame the loop held behind the turn, is a
/// request this run will not serve: answered the refused close naming the
/// unload on its own exchange, and recorded as a refusal of the leave carrying
/// `Unloading` (S4 x dialer request). A gate whose channel fails meets the
/// lower next, which finds it so. **The wait ends at the declared drain
/// bound** where one stands, or at a join, either of which turns the leave
/// forced.
fn drain_gate(
    run: &mut Run,
    listener: &crate::channel::CoordinationListener,
    leave: &mut crate::engine::PendingLeave,
) {
    use crate::lifecycle_table::Leave;
    use std::os::fd::AsFd;
    let deadline = leave.drain_deadline;
    let refused = render_close("refused", "reason", "the agent is unloading", None);
    // **Recorded, then announced** (#116): the refusal is authored first and
    // the dialer's refused close is sent only over a record that holds; a
    // dialer left unanswered loses its connection at the lower.
    let refuse = |run: &mut Run, exchange: ExchangeId| {
        author_refusal_on(
            run,
            weaver_types::LifecycleAsk::Leave,
            &LifecycleRefusal::Unloading,
        );
        if !run.recorder.failed()
            && let Some(gate) = run.gate.as_ref()
        {
            let _ = gate.channel.send(&OrganEnvelope {
                exchange,
                position: Position::Close,
                payload: weaver_types::Payload::Frame(weaver_types::TurnFrame::carry(
                    refused.as_bytes(),
                )),
            });
        }
    };
    // A frame held behind the turn is refused; a fault report the gate
    // sent meanwhile is authored, as the shelf authors one.
    while let Some(held) = run.held_frames.pop_front() {
        match held.payload {
            weaver_types::Payload::Frame(_) => refuse(run, held.exchange),
            weaver_types::Payload::Fault(report) => {
                // Latched on a failure (#116), read at the leave's seal, which refuses
                // `DescriptorsUnusable`.
                let _ = run
                    .author
                    .author_fault(&mut run.recorder, Subsystem::Gate, None, &report);
            }
            _ => {}
        }
    }
    let Some(gate) = run.gate.as_ref() else {
        return;
    };
    run.gate_ordinal += 1;
    let exchange = ExchangeId {
        opener: Opener::Harness,
        ordinal: run.gate_ordinal,
    };
    if gate
        .channel
        .send(&OrganEnvelope {
            exchange: exchange.clone(),
            position: Position::Open,
            payload: weaver_types::Payload::Directive(LifecycleDirective::Quiesce),
        })
        .is_err()
    {
        // **A gate dead before the drain** (S4 x the gate dies): the
        // quiesce's send failing on a hung-up gate is its death, met as the
        // read's below is, (A).
        if run
            .gate
            .as_ref()
            .is_some_and(|gate| hung_up(gate.channel.as_fd()))
        {
            gate_died_in_leave(run, leave);
        }
        return;
    }
    loop {
        let Some(gate) = run.gate.as_ref() else {
            return;
        };
        // **The declared drain bound** (S4 x a bound passes): the quiesce
        // unanswered by it turns the leave forced from where it stands.
        // **The listener is heard beside the gate**: a join turns the leave
        // forced at once, the drain left there (S4 x force-unload).
        // **The listener is scanned before the gate** (Codex on #119, round
        // 3): a gate whose channel never falls quiet must not starve a join.
        match wait_either(listener.as_fd(), gate.channel.as_fd(), deadline) {
            None => {
                leave.progress.leave = Leave::Forced;
                return;
            }
            Some(0) => {
                hear_dials(listener, leave, false, run.recorder.failed());
                if leave.progress.leave == Leave::Forced {
                    return;
                }
                continue;
            }
            Some(_) => {}
        }
        let Ok(envelope) = gate.channel.recv() else {
            // **A gate dead at the drain** (S4 x the gate dies): (A).
            if hung_up(gate.channel.as_fd()) {
                gate_died_in_leave(run, leave);
            }
            return;
        };
        match envelope.payload {
            weaver_types::Payload::Frame(_) => refuse(run, envelope.exchange),
            weaver_types::Payload::Fault(report) => {
                // Latched on a failure (#116), read at the leave's seal, which refuses
                // `DescriptorsUnusable`.
                let _ = run
                    .author
                    .author_fault(&mut run.recorder, Subsystem::Gate, None, &report);
            }
            weaver_types::Payload::Answer(LifecycleAnswer::GateQuiesced)
                if envelope.exchange == exchange =>
            {
                return;
            }
            // Anything else ends the drain; the lower meets the gate next.
            _ => return,
        }
    }
}

/// **A turn that lost an organ closes failed** (3.0, S3 and S4 x the SPU or
/// the gate dies, ruling (B)): the turn's close and each dead organ's fault
/// are authored, and only then is the dead organ's channel dropped and the
/// organ reaped. **The record holds or service ends**: a record that cannot
/// be authored refuses before anything is torn down (the Planner's ruling of
/// 2026-10-09). Returns the turn that closed, for its dialer's close.
fn close_turn_on_death(run: &mut Run) -> Result<Option<TurnKey>, ChannelFault> {
    let turn = run.turn_in_flight.take();
    let mut recorded = true;
    if let Some(turn) = turn.as_ref() {
        recorded &= run
            .author
            .author(
                &mut run.recorder,
                weaver_trace::Kind::TurnClosed,
                Subsystem::Harness,
                Some(turn),
                Some(weaver_trace::Payload::TurnClosed(
                    weaver_trace::TurnClose::Stopped {
                        reason: weaver_trace::StopReason::Fault,
                        cause: None,
                    },
                )),
            )
            .is_ok();
    }
    // **Which organ died is read from its channel's hang-up**: a gate lost
    // in the tool wait, or the SPU lost in the stream or the re-feed; the
    // caller ended service where neither hung up.
    use std::os::fd::AsFd;
    let gate_dead = run
        .gate
        .as_ref()
        .is_some_and(|gate| hung_up(gate.channel.as_fd()));
    let spu_dead = run
        .spu
        .as_ref()
        .is_some_and(|spu| hung_up(spu.decode.as_fd()));
    // **A gate lost mid-turn has `tools/` killed before its death is
    // named** (`weaver-admin-Spec` 6.1): the engine's kill where a call was
    // open, else the worker's own, serving.
    let from_call = if gate_dead {
        run.gate_contained.take()
    } else {
        None
    };
    let contained = if gate_dead {
        from_call.or_else(|| contain_tools(run, crate::tools_cgroup::Context::Serving))
    } else {
        None
    };
    for (organ, dead) in [("gate", gate_dead), ("spu", spu_dead)] {
        if !dead {
            continue;
        }
        let mut account = serde_json::json!({
            "organ": organ,
            "death": "its channel was lost mid-turn",
        });
        if organ == "gate"
            && let Some(words) = containment_words(contained)
        {
            account["containment"] = serde_json::Value::String(words.to_string());
            if from_call.is_some() {
                account["call"] = serde_json::Value::String(
                    "closed killed by the fault once tools/ was contained".to_string(),
                );
            }
        }
        let account = account.to_string();
        recorded &= run
            .author
            .author_fault(
                &mut run.recorder,
                Subsystem::Harness,
                turn.as_ref(),
                &crate::authorship::harness_report(
                    weaver_types::FaultCase::OrganDeathObserved,
                    &account,
                ),
            )
            .is_ok();
    }
    if !recorded {
        return Err(ChannelFault::Closed);
    }
    if gate_dead && let Some(gate) = run.gate.take() {
        drop(gate.channel);
        reap_by(gate.pid, lower_window());
    }
    if spu_dead && let Some(spu) = run.spu.take() {
        let pid = spu.pid;
        drop(spu);
        reap_by(pid, lower_window());
    }
    // The turn closed `Stopped { reason: fault }` above, its one close: a
    // `tools/` that would not empty, or a kill not delivered, ends service.
    if ends_service(contained) {
        return Err(ChannelFault::Closed);
    }
    Ok(turn)
}

/// Whether `channel` has something to read, or has hung up, before
/// `deadline`: a poll for the time remaining, an interrupted wait resumed.
fn readable_by(channel: &OrganChannel, deadline: std::time::Instant) -> bool {
    use nix::poll::{PollFd, PollFlags, poll};
    use std::os::fd::AsFd;
    loop {
        // **Past the deadline, nothing is read**, however readable the
        // channel stays: a peer that floods it cannot hold the wait open
        // (Codex on #119, round 1). An expiry is believed only once the clock
        // agrees.
        if std::time::Instant::now() >= deadline {
            return false;
        }
        let wait = poll_timeout_until(Some(deadline));
        let mut fds = [PollFd::new(channel.as_fd(), PollFlags::POLLIN)];
        match poll(&mut fds, wait) {
            Ok(0) if std::time::Instant::now() < deadline => continue,
            Ok(0) => return false,
            Ok(_) => return true,
            Err(nix::errno::Errno::EINTR) => continue,
            Err(_) => return true,
        }
    }
}

/// **A declared bound's deadline, as checked time** (`weaver-harness-Spec`
/// section 6, the bounds): `bound_s` seconds from now, or none where the
/// bound is absent or the clock cannot hold the deadline, either of which is
/// unbounded.
pub(crate) fn deadline_after(bound_s: Option<u64>) -> Option<std::time::Instant> {
    std::time::Instant::now().checked_add(std::time::Duration::from_secs(bound_s?))
}

/// **A gate dying during a leave turns it forced** (`weaver-admin-Spec` 3.0,
/// S4, S5 and S6 x the gate dies, ruling (A)): its death is a `fault`, its
/// channel dropped and the organ reaped, and the leave turns forced from where
/// it stands, no `forced_by`, the graceful caller's cause kept. The save
/// point's legs go on, needing no gate.
fn gate_died_in_leave(run: &mut Run, leave: &mut PendingLeave) {
    if let Some(gate) = run.gate.take() {
        // Inside a leave the worker kills `tools/` and does not wait (6.1).
        let contained = run
            .gate_contained
            .take()
            .or_else(|| contain_tools(run, crate::tools_cgroup::Context::Leave));
        author_gate_death(
            run,
            gate.last_word.take(),
            "its channel was lost during the leave",
            contained,
        );
        drop(gate.channel);
        reap_by(gate.pid, lower_window());
    }
    leave.progress.leave = crate::lifecycle_table::Leave::Forced;
}

/// **The worker's handle on `tools/`, or the gate's arm's refusal**
/// (`weaver-harness-Spec` section 2.2; `weaver-admin-Spec` 6.1, the worker's
/// row): a worker that cannot open or judge it stands no gate, refusing
/// `BoundaryUnverified` wrapped `OrganRefused` as any refusal of the gate's
/// arm is, the reason on standard error.
fn open_tools() -> Result<crate::tools_cgroup::ToolsCgroup, LifecycleRefusal> {
    crate::tools_cgroup::ToolsCgroup::open_from_own().map_err(|refused| {
        eprintln!("weaver-harness: the agent's tools/ cgroup: {refused}");
        LifecycleRefusal::OrganRefused {
            organ: weaver_types::RefusingOrgan::Gate,
            reason: Box::new(LifecycleRefusal::BoundaryUnverified),
        }
    })
}

/// **The gate's fork**: its end at 3 and the handle on `tools/` at 4, by the
/// channels' own order (`weaver-harness-Spec` section 2.2), and no argument.
///
/// # Safety
///
/// [`crate::spawn::fork_organ`]'s: the caller is the serving thread.
unsafe fn fork_gate(
    binary: &std::path::Path,
    end: &crate::channel::ChildEnd,
    tools: std::os::fd::BorrowedFd<'_>,
    stderr: Option<std::os::fd::RawFd>,
) -> Result<nix::unistd::Pid, nix::Error> {
    // SAFETY: the caller's.
    unsafe { crate::spawn::fork_organ(binary, &[end.as_fd(), tools], &[], stderr) }
}

/// The worker's kill of `tools/` and what it came to, in the context it ran.
type Contained = (
    crate::tools_cgroup::Context,
    crate::tools_cgroup::Containment,
);

/// **The worker's kill of `tools/`** (`weaver-admin-Spec` 6.1, the worker's
/// rows), `None` where no handle was opened (a run with no gate).
fn contain_tools(run: &Run, context: crate::tools_cgroup::Context) -> Option<Contained> {
    run.tools.as_ref().map(|tools| {
        (
            context,
            crate::tools_cgroup::contain(tools, context, crate::tools_cgroup::CONTAINMENT_BOUND),
        )
    })
}

/// **Whether a containment ends service** (6.1, the worker's rows): while
/// serving, a `tools/` that did not empty within the bound, or a kill the
/// write and the fallback both failed to deliver. Inside a leave nothing
/// ends service; the leave goes on.
fn ends_service(contained: Option<Contained>) -> bool {
    use crate::tools_cgroup::{Containment, Context};
    matches!(
        contained,
        Some((
            Context::Serving,
            Containment::PastBound | Containment::Undelivered
        ))
    )
}

/// What the gate's death account says of the containment.
fn containment_words(contained: Option<Contained>) -> Option<&'static str> {
    use crate::tools_cgroup::Containment;
    contained.map(|(_, containment)| match containment {
        Containment::Emptied => "tools/ killed and emptied",
        Containment::Delivered => "tools/ killed, not waited on inside the leave",
        Containment::PastBound => "tools/ killed and not empty within the bound: service ends",
        Containment::Undelivered => {
            "containment failed: the write of cgroup.kill and the pinned fallback both failed, the kill not delivered"
        }
    })
}

/// The gate's death as a `fault`, its last word riding the account where it
/// spoke one that parses.
fn author_gate_death(
    run: &mut Run,
    last_word: Option<String>,
    death: &str,
    contained: Option<Contained>,
) {
    let mut account = serde_json::json!({"organ": "gate", "death": death});
    if let Some(line) = last_word {
        account["last_word"] =
            serde_json::from_str(&line).unwrap_or(serde_json::Value::String(line));
    }
    if let Some(words) = containment_words(contained) {
        account["containment"] = serde_json::Value::String(words.to_string());
    }
    // Latched on a failure (#116), read at the leave's seal, which refuses
    // `DescriptorsUnusable`.
    let _ = run.author.author_fault(
        &mut run.recorder,
        Subsystem::Harness,
        None,
        &crate::authorship::harness_report(
            weaver_types::FaultCase::OrganDeathObserved,
            &account.to_string(),
        ),
    );
}

/// **The leave's answer to every dialer it holds** (`weaver-admin-Spec`
/// section 3, the seal, step 7): its own and every held join's, the same
/// answer each. A dialer gone is not this leave's failure.
fn answer_all(
    leave: &crate::engine::PendingLeave,
    answer: Result<LifecycleAnswer, LifecycleRefusal>,
    latched: bool,
) {
    // **Over a latched record every held caller is refused
    // `DescriptorsUnusable`** (#116), as the seal refuses: nothing the leave
    // would answer is announced over a record missing an event.
    let answer = if latched {
        Err(LifecycleRefusal::DescriptorsUnusable)
    } else {
        answer
    };
    let payload = match answer {
        Ok(answer) => weaver_types::Payload::Answer(answer),
        Err(refusal) => weaver_types::Payload::Refusal(refusal),
    };
    let dialers = std::iter::once((&leave.connection, &leave.exchange)).chain(
        leave
            .joins
            .iter()
            .map(|(connection, exchange)| (connection, exchange)),
    );
    for (connection, exchange) in dialers {
        let _ = deliver(
            connection,
            &OrganEnvelope {
                exchange: exchange.clone(),
                position: Position::Close,
                payload: payload.clone(),
            },
        );
    }
}

/// **The leave's sweep of the listener's backlog** (`weaver-harness-Spec`
/// section 6; `weaver-admin-Spec` section 3, the seal, step 3): every dial
/// pending now is accepted without waiting and heard, a peer that is not
/// root refused at the accept as ever.
fn hear_dials(
    listener: &crate::channel::CoordinationListener,
    leave: &mut crate::engine::PendingLeave,
    fixed: bool,
    latched: bool,
) {
    loop {
        match listener.try_accept_root() {
            Ok(Some(connection)) => hear_dial(connection, leave, fixed, latched),
            Err(ChannelFault::WrongPeer { .. }) => continue,
            Ok(None) | Err(_) => return,
        }
    }
}

/// **One dial heard while a leave is pending** (S4 to S7): its directive is
/// read inside the state seam's answer bound, a dialer that says nothing by
/// then being dropped. A `JoinLeave` joins the leave
/// ([`crate::engine::PendingLeave::join`]), `fixed` from S7 on; an `Observe`
/// answers `InTransition`; any other directive is `OutOfOrder`, a `Leave`
/// meeting a pending leave included (`weaver-harness-Spec` section 6).
///
/// **Over a latched record (#116) nothing is answered**: every dial, a join
/// included, is dropped unanswered, admin concluding as for no answer.
fn hear_dial(
    connection: OrganChannel,
    leave: &mut crate::engine::PendingLeave,
    fixed: bool,
    latched: bool,
) {
    let Some(deadline) = std::time::Instant::now().checked_add(std::time::Duration::from_millis(
        crate::state::ANSWER_BOUND_MS,
    )) else {
        return;
    };
    if !readable_by(&connection, deadline) {
        return;
    }
    let Ok(envelope) = connection.recv() else {
        return;
    };
    // Read before a join too, which `join` may refuse on its own.
    if latched {
        return;
    }
    let answer = match envelope.payload {
        weaver_types::Payload::Directive(LifecycleDirective::JoinLeave { cause }) => {
            leave.join(connection, envelope.exchange, cause, fixed);
            return;
        }
        weaver_types::Payload::Directive(LifecycleDirective::Observe) => {
            weaver_types::Payload::Answer(LifecycleAnswer::InTransition {
                constituents: Vec::new(),
            })
        }
        weaver_types::Payload::Directive(_) => {
            weaver_types::Payload::Refusal(LifecycleRefusal::OutOfOrder)
        }
        _ => return,
    };
    let _ = deliver(
        &connection,
        &OrganEnvelope {
            exchange: envelope.exchange,
            position: Position::Close,
            payload: answer,
        },
    );
}

/// **A poll's wait to `deadline`**, `None` being unbounded: the time
/// remaining, rounded up so the wait reaches the deadline rather than
/// stopping short of it by the truncated fraction, and clamped to what a poll
/// takes. The one conversion every deadline wait of the leave and the turn
/// shares.
pub(crate) fn poll_timeout_until(deadline: Option<std::time::Instant>) -> nix::poll::PollTimeout {
    use nix::poll::PollTimeout;
    let Some(deadline) = deadline else {
        return PollTimeout::NONE;
    };
    let remaining = deadline.saturating_duration_since(std::time::Instant::now());
    PollTimeout::try_from(i32::try_from(remaining.as_micros().div_ceil(1000)).unwrap_or(i32::MAX))
        .unwrap_or(PollTimeout::MAX)
}

/// Which of two descriptors is readable or hung up, `0` or `1`, the first
/// argument winning where both are, so the caller ranks them; `None` once
/// `deadline` passes, and with no deadline the wait is unbounded.
fn wait_either(
    first: std::os::fd::BorrowedFd<'_>,
    second: std::os::fd::BorrowedFd<'_>,
    deadline: Option<std::time::Instant>,
) -> Option<usize> {
    use nix::poll::{PollFd, PollFlags, poll};
    loop {
        // Past the deadline, nothing is read, however readable either stays.
        if deadline.is_some_and(|d| std::time::Instant::now() >= d) {
            return None;
        }
        let wait = poll_timeout_until(deadline);
        let mut fds = [
            PollFd::new(first, PollFlags::POLLIN),
            PollFd::new(second, PollFlags::POLLIN),
        ];
        match poll(&mut fds, wait) {
            Ok(0) if deadline.is_some_and(|d| std::time::Instant::now() >= d) => return None,
            Ok(0) | Err(nix::errno::Errno::EINTR) => continue,
            // A failed poll reads as the first readable, whose read then
            // meets the failure.
            Err(_) => return Some(0),
            Ok(_) => {}
        }
        let woken = PollFlags::POLLIN | PollFlags::POLLHUP | PollFlags::POLLERR;
        for (at, fd) in fds.iter().enumerate() {
            if fd
                .revents()
                .is_some_and(|revents| revents.intersects(woken))
            {
                return Some(at);
            }
        }
    }
}

/// Whether the gate's or the SPU's channel hung up, the organ dead.
fn organ_hung_up(run: &Run) -> bool {
    use std::os::fd::AsFd;
    run.gate
        .as_ref()
        .is_some_and(|gate| hung_up(gate.channel.as_fd()))
        || run
            .spu
            .as_ref()
            .is_some_and(|spu| hung_up(spu.decode.as_fd()))
}

/// Whether a channel's peer hung up: a zero-timeout poll reading its end.
fn hung_up(fd: std::os::fd::BorrowedFd<'_>) -> bool {
    use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
    let mut fds = [PollFd::new(fd, PollFlags::POLLIN)];
    matches!(poll(&mut fds, PollTimeout::ZERO), Ok(n) if n > 0)
        && fds[0]
            .revents()
            .is_some_and(|revents| revents.intersects(PollFlags::POLLHUP | PollFlags::POLLERR))
}

/// Reaps a forked organ so it does not stay a zombie entry for the life of the
/// worker. The wait is bounded by the organ's own exit: both organs are sent
/// their closing directive and have their channel dropped first, so the read
/// that follows returns closure and the process exits.
fn reap(pid: nix::unistd::Pid) {
    let _ = nix::sys::wait::waitpid(pid, None);
}

/// **A dead organ is reaped inside the lower bound** (`weaver-admin-Spec`
/// 3.0, S4, S5, S6 and S10 x the gate dies: "reaps the gate, inside the
/// lower bound"; and the SPU's death the run serves on past, Codex on #119,
/// round 1): an organ whose channel hung up but whose process lingers is
/// killed at the bound, never waited on unbounded, which would hold the run
/// the table says serves on.
fn lower_window() -> Option<std::time::Instant> {
    std::time::Instant::now().checked_add(std::time::Duration::from_millis(
        weaver_types::LOWER_BOUND_MS,
    ))
}

/// [`reap`] by a deadline where one stands (`weaver-harness-Spec` section 6,
/// the shared deadline): an organ not exited by it is killed (`SIGKILL`),
/// then reaped.
fn reap_by(pid: nix::unistd::Pid, deadline: Option<std::time::Instant>) {
    use nix::sys::wait::{WaitPidFlag, WaitStatus, waitpid};
    let Some(deadline) = deadline else {
        return reap(pid);
    };
    loop {
        match waitpid(pid, Some(WaitPidFlag::WNOHANG)) {
            Ok(WaitStatus::StillAlive) => {}
            // Exited, or not this worker's child to wait on.
            _ => return,
        }
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            let _ = nix::sys::signal::kill(pid, nix::sys::signal::Signal::SIGKILL);
            return reap(pid);
        }
        std::thread::sleep(remaining.min(std::time::Duration::from_millis(10)));
    }
}

/// [`exchange`] with its answer read by `deadline` where one stands; an
/// answer not there by it is `NoResidency`, the organ unconfirmed.
fn exchange_by(
    channel: &OrganChannel,
    opener: Opener,
    ordinal: u64,
    organ: weaver_types::RefusingOrgan,
    directive: LifecycleDirective,
    deadline: Option<std::time::Instant>,
) -> Result<LifecycleAnswer, LifecycleRefusal> {
    let Some(deadline) = deadline else {
        return exchange(channel, opener, ordinal, organ, directive);
    };
    channel
        .send(&OrganEnvelope {
            exchange: ExchangeId { opener, ordinal },
            position: Position::Open,
            payload: weaver_types::Payload::Directive(directive),
        })
        .map_err(|_| LifecycleRefusal::NoResidency)?;
    if !readable_by(channel, deadline) {
        return Err(LifecycleRefusal::NoResidency);
    }
    match channel.recv() {
        Ok(envelope) => match envelope.payload {
            weaver_types::Payload::Answer(answer) => Ok(answer),
            weaver_types::Payload::Refusal(reason) => Err(LifecycleRefusal::OrganRefused {
                organ,
                reason: Box::new(reason),
            }),
            _ => Err(LifecycleRefusal::Malformed),
        },
        Err(_) => Err(LifecycleRefusal::NoResidency),
    }
}

/// Opens one exchange on an organ channel and reads its answer, carrying a
/// refusing organ's reason into the aggregate unchanged.
fn exchange(
    channel: &OrganChannel,
    opener: Opener,
    ordinal: u64,
    organ: weaver_types::RefusingOrgan,
    directive: LifecycleDirective,
) -> Result<LifecycleAnswer, LifecycleRefusal> {
    channel
        .send(&OrganEnvelope {
            exchange: ExchangeId { opener, ordinal },
            position: Position::Open,
            payload: weaver_types::Payload::Directive(directive),
        })
        .map_err(|_| LifecycleRefusal::NoResidency)?;
    match channel.recv() {
        Ok(envelope) => match envelope.payload {
            weaver_types::Payload::Answer(answer) => Ok(answer),
            // The refusing organ is the one the caller spoke to: a gate
            // refusal reported as the SPU's would have the peer acting on the
            // wrong organ.
            weaver_types::Payload::Refusal(reason) => Err(LifecycleRefusal::OrganRefused {
                organ,
                reason: Box::new(reason),
            }),
            _ => Err(LifecycleRefusal::Malformed),
        },
        Err(_) => Err(LifecycleRefusal::NoResidency),
    }
}

fn clear_dumpable() -> Result<(), AdoptionFault> {
    // SAFETY: prctl with PR_SET_DUMPABLE affects only this process.
    let rc = unsafe { nix::libc::prctl(nix::libc::PR_SET_DUMPABLE, 0, 0, 0, 0) };
    if rc == -1 {
        return Err(AdoptionFault::DumpableNotCleared {
            errno: std::io::Error::last_os_error().raw_os_error().unwrap_or(0),
        });
    }
    Ok(())
}

/// The column ask's derivation, per `weaver-harness-Spec` section 6.1: the
/// ask is written where and only where the binding is diagnostic and the
/// readout is elected. A serving harness never writes it - the discipline
/// `weaver-spu-PRD` section 13.7 watches on this crate - and a diagnostic
/// load without the election asks nothing, the registry's second arm
/// waiting for exactly that ask.
///
/// conforms: harness-serving-writes-no-column-ask
fn column_ask_for(diagnostic: bool, readout_elected: bool) -> bool {
    diagnostic && readout_elected
}

/// **The trace queue's peak at the leave** (#120; `weaver-harness-Spec`
/// section 4): the run's peak depth, the queue's capacity and the high-water
/// mark, one JSON line in the harness's diagnostic form, the evidence for
/// [`weaver_trace::QUEUE_DEPTH`] that the pressure fault, silent below the
/// mark, does not give.
fn queue_peak_notice(peak: usize) -> String {
    serde_json::json!({
        "organ": "harness",
        "queue": "trace",
        "peak": peak,
        "depth": weaver_trace::QUEUE_DEPTH,
        "mark": weaver_trace::HIGH_WATER_MARK,
    })
    .to_string()
}

/// The typed line that names what a diagnostic enter waits on, per
/// `weaver-harness-Spec` section 6.1 as of 2026-09-06: the door stands under
/// a diagnostic binding alone, so a serving load names no wait.
fn parked_ask_notice(bound_ms: u64) -> String {
    serde_json::json!({
        "organ": "harness",
        "waiting": "the state member's preload seal",
        "binding": "diagnostic",
        "bound_ms": bound_ms,
    })
    .to_string()
}

/// **Whether the member's answer to `restored` agrees with the enter's
/// lineage**, per `weaver-harness-Spec` section 6.1: a miss or a refusal
/// disagrees, an empty answer agrees with an enter naming no save point and
/// with nothing else, and a stamp agrees where its digest, run, sequence and
/// turn equal the lineage's four, `named_at_restore` and `built_from` being
/// admin's resolution and never the member's to answer. The reason names
/// what disagreed, for the diagnostic stream.
fn restored_agrees(
    answered: Option<&crate::state::RestoredAnswer>,
    lineage: Option<&weaver_types::Lineage>,
) -> Result<(), String> {
    use crate::state::RestoredAnswer;
    match (answered, lineage) {
        (None, _) => Err("the restored ask was not answered".into()),
        (Some(RestoredAnswer::Refused(reason)), _) => {
            Err(format!("the member refused the save point: {reason}"))
        }
        (Some(RestoredAnswer::Empty), None) => Ok(()),
        (Some(RestoredAnswer::Empty), Some(lineage)) => Err(format!(
            "the member stood empty where the enter names save point {}",
            lineage.save_point
        )),
        (Some(RestoredAnswer::Lineage(stamp)), None) => Err(format!(
            "the member restored save point {} where the enter names none",
            stamp.digest
        )),
        (Some(RestoredAnswer::Lineage(stamp)), Some(lineage)) => {
            if stamp.digest == lineage.save_point
                && stamp.run == lineage.run.0
                && stamp.sequence == lineage.sequence
                && stamp.turn == lineage.turn
            {
                Ok(())
            } else {
                Err(format!(
                    "the member restored {}:{}:{}:{} where the enter names {}:{}:{}:{}",
                    stamp.digest,
                    stamp.run,
                    stamp.sequence,
                    stamp.turn,
                    lineage.save_point,
                    lineage.run.0,
                    lineage.sequence,
                    lineage.turn
                ))
            }
        }
    }
}

/// **A save point, taken at rest through the seam's four legs and recorded**,
/// per `weaver-harness-Spec` section 6 on the operator's rulings of
/// 2026-10-06 on #1: the leave's, before the `unload` event, and the
/// operator's on demand through admin's `save-point` verb. `Ok(None)` is a run
/// with no member or under a diagnostic binding, which takes none; `Err`
/// names the leg that missed, and nothing is authored for it. The `save_point`
/// event is authored on the member's `finished` answer, so the record names
/// only a file that has its finished name, and the report carries the
/// event's own position for admin's manifest.
fn take_save_point(
    run: &mut Run,
    cause: Option<weaver_types::Cause>,
) -> Result<Option<weaver_types::SavePointReport>, weaver_types::SavePointLeg> {
    if run.recorder.serving().is_none() {
        return Ok(None);
    }
    let Some(seam) = run.state.as_mut() else {
        return Ok(None);
    };
    // **No save point after the tee was lost** (the operator's ruling of
    // 2026-10-08 on #99, R1): the tee detaches silently on a stalled or
    // broken seam, and every distillate since is missing from the holdings,
    // while the seam, a clone of the same socket, still answers. A snapshot
    // now would be stale state published as whole, so the leg is the dead
    // member's: the save point and the clean unload refuse, and a force
    // ends the run with the reset recorded.
    if run
        .recorder
        .serving()
        .is_some_and(weaver_trace::Recorder::tee_lost)
    {
        return Err(weaver_types::SavePointLeg::MemberDead);
    }
    let taken = seam.ask_snapshot()?;
    let position = run
        .author
        .author(
            &mut run.recorder,
            Kind::SavePoint,
            Subsystem::Harness,
            None,
            Some(weaver_trace::Payload::SavePoint(
                weaver_trace::SavePointTaken {
                    save_point: taken.stamp.digest.clone(),
                    run: taken.stamp.run.clone(),
                    sequence: taken.stamp.sequence,
                    turn: taken.stamp.turn,
                    name: taken.name.clone(),
                    // **The cause the directive carried** (Codex on #94,
                    // round 7), as `unload` carries its own, so a save point
                    // is attributed to the account that asked for it.
                    cause: cause.map(crate::engine::trace_cause),
                },
            )),
        )
        // The record would not take the event: the file stands finished in
        // the member's room, but a save point the trace does not name is
        // not a fact, per the Planner's rule on #1, so the leg is the
        // finished answer's and the operator retries.
        .map_err(|_| weaver_types::SavePointLeg::Finished)?;
    // **The event's run is this run's, the covered position's the stamp's**
    // (Codex on #94, round 6): after a restore the holdings may still stand
    // at the prior run's position, and the manifest's position names the
    // event, which is in the standing run.
    Ok(Some(weaver_types::SavePointReport {
        save_point: taken.stamp.digest,
        name: taken.name,
        run: weaver_types::RunId(taken.stamp.run),
        sequence: taken.stamp.sequence,
        turn: taken.stamp.turn,
        event_run: run.run.clone(),
        position: position.0,
    }))
}

/// **A restoring run numbers its turns from one**, per `weaver-harness-Spec`
/// section 6.1 on the operator's ruling of 2026-10-02 on #59: the ordinal a
/// run starts from is zero whatever the lineage carries, its last turn being
/// a recorded fact that numbers nothing, so the first minted turn is one and
/// the derived seeds draw the new run's own streams.
fn initial_turn_ordinal(_restore: Option<&weaver_types::Lineage>) -> u64 {
    0
}

/// The enter's answered identity ask reaches the record as a `recall`
/// event, per `weaver-trace-Spec` section 3's recall clause, before the open
/// is built from it. **A recall the recorder will not take is named in a
/// fault rather than refusing the enter**, the miss accounting the seated
/// prefix already runs on: the answer is good and the load can stand, and
/// what went unrecorded is the provenance of the prefix the run opens under,
/// which is the case that fault names.
fn record_enter_ask(
    author: &crate::authorship::Author,
    recorder: &mut crate::record::Record,
    verb: weaver_trace::RecallVerb,
    answered: &[crate::state::Recalled],
) {
    if let Err(failure) = author.author_recall(recorder, verb, None, answered) {
        let account = serde_json::json!({
            "organ": "harness",
            "miss": "recall-unrecorded",
            "failure": format!("{failure:?}"),
        });
        // Latched on a failure (#116): the answer to admin reads the latch first.
        let _ = author.author_fault(
            recorder,
            Subsystem::Harness,
            None,
            &crate::authorship::harness_report(
                weaver_types::FaultCase::IdentityPrefixUnrecorded,
                &account.to_string(),
            ),
        );
    }
}

#[cfg(test)]
mod tests;
