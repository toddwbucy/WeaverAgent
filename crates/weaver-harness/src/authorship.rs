//! conforms: harness-timestamps-stamped-at-authoring
//! conforms: harness-licensed-combinations-refused-before-submit
//! conforms: harness-refused-submission-not-retried
//! conforms: harness-nothing-waits-on-the-sink
//! conforms: harness-faults-authored-as-events
//! conforms: harness-fault-payload-carried-unchanged
//!
//! Trace authorship, per `weaver-harness-Spec` section 4: the authoring half
//! of `weaver-harness-trace-contract`, one module with one submit path. The
//! harness is the sole author and the recorder never infers anything it was
//! not handed.

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use weaver_trace::{
    Envelope, Event, Kind, MonotonicNs, Payload, RunRef, Sequence, SessionRef, Subsystem, TurnRef,
    raw_payload,
};
use weaver_traits::{ContentBlock, Message, Role};
use weaver_types::{RunId, SessionId, TurnKey};

use crate::failure::UnlicensedMessage;
use crate::record::{Record, RecordFailure};

/// The authoring path: holds the run's identity and the origin of its
/// monotonic clock, and stamps both timestamps at authoring from the standard
/// library's two clocks. The recorder's clock is never consulted, the contract
/// denying it the fields.
///
/// The monotonic reading is nanoseconds since the run's origin, an origin only
/// the author holds, so a component stamping its own report would carry a
/// reading placeable in no run.
pub struct Author {
    session: SessionRef,
    run: RunRef,
    origin: Instant,
}

impl Author {
    /// The origin is captured when the `load` event is authored, which is the
    /// first act of the run, so this constructor runs at that moment.
    pub fn new(session: &SessionId, run: &RunId) -> Self {
        Author {
            session: convert_session(session),
            run: convert_run(run),
            origin: Instant::now(),
        }
    }

    /// Authors one event and submits it. Both timestamps are stamped here.
    ///
    /// A refusal is not treated as recorded, not projected, and not retried
    /// under a new sequence: a refusal on the authoring path is a defect in
    /// the author, and it surfaces as a fault rather than a retry. Nothing on
    /// any turn path waits on the sink - the working structure's return is the
    /// acknowledgment the interior proceeds on.
    pub fn author(
        &self,
        recorder: &mut Record,
        kind: Kind,
        subsystem: Subsystem,
        turn: Option<&TurnKey>,
        payload: Option<Payload>,
    ) -> Result<Sequence, RecordFailure> {
        let event = Event {
            envelope: Envelope {
                session: self.session.clone(),
                run: self.run.clone(),
                turn: turn.map(convert_turn),
                // The recorder assigns the run-scoped sequence; a caller
                // cannot author a gapless order.
                sequence: Sequence(0),
                kind,
                subsystem,
                causal_parent: None,
                wall_ms: wall_clock_millis(),
                monotonic_ns: MonotonicNs(self.origin.elapsed().as_nanos() as u64),
            },
            payload,
        };
        recorder.submit(event)
    }

    /// Authors one diagnostic-native event and submits it, per
    /// `weaver-diagnostic-Spec` section 3: the replay's own kinds exist in
    /// no serving vocabulary and convert from nothing, so they are composed
    /// here in the diagnostic vocabulary directly, stamped by the same
    /// clocks at the same moment the serving road stamps.
    pub fn author_diagnostic(
        &self,
        recorder: &mut Record,
        kind: weaver_diagnostic::Kind,
        turn: Option<&TurnKey>,
        payload: Option<weaver_diagnostic::Payload>,
    ) -> Result<weaver_diagnostic::Sequence, RecordFailure> {
        let event = weaver_diagnostic::Event {
            envelope: weaver_diagnostic::Envelope {
                session: weaver_diagnostic::SessionRef(self.session.0.clone()),
                run: weaver_diagnostic::RunRef(self.run.0.clone()),
                turn: turn.map(|t| weaver_diagnostic::TurnRef(t.0.clone())),
                // The recorder assigns the run-scoped sequence, as the
                // serving road's does.
                sequence: weaver_diagnostic::Sequence(0),
                kind,
                subsystem: weaver_diagnostic::Subsystem::Harness,
                causal_parent: None,
                wall_ms: wall_clock_millis(),
                monotonic_ns: weaver_diagnostic::MonotonicNs(
                    self.origin.elapsed().as_nanos() as u64
                ),
            },
            payload,
        };
        recorder.submit_diagnostic(event)
    }

    /// The session's reference, for the one reader that names it in a
    /// payload: `replay.identity` carries the replayed session, which the
    /// contract makes the declared session's own name.
    pub fn session(&self) -> &str {
        &self.session.0
    }

    /// Authors a message event after judging it against the licensing rule.
    /// An unlicensed message is refused by this crate and never submitted,
    /// because the recorder judges the envelope and never the interior.
    pub fn author_message(
        &self,
        recorder: &mut Record,
        message: &Message,
        turn: &TurnKey,
    ) -> Result<Result<Sequence, RecordFailure>, UnlicensedMessage> {
        licensed(message)?;
        let kind = match message.role {
            Role::System => Kind::MessageSystem,
            Role::User => Kind::MessageUser,
            Role::Assistant => Kind::MessageAssistant,
            // **The tool-result door is the grant's and never this one**, per
            // `weaver-harness-Spec` section 6: a message this door authored
            // from a supplied block would be a fabricated result entering the
            // record, so the role refuses here and `author_tool_result` is
            // the one door, taking the granted value the gate exchange
            // constructed.
            Role::ToolResult => {
                return Err(UnlicensedMessage {
                    role: "tool_result",
                    block: "granted-door-only",
                });
            }
            // The role set grows with the conversation model, and a role this
            // crate cannot map to an event kind is not authorable.
            _ => {
                return Err(UnlicensedMessage {
                    role: "unmapped",
                    block: "any",
                });
            }
        };
        let rendered = serde_json::to_string(message).map_err(|_| UnlicensedMessage {
            role: "any",
            block: "unrenderable",
        })?;
        let payload = raw_payload(&rendered).ok_or(UnlicensedMessage {
            role: "any",
            block: "unrenderable",
        })?;
        Ok(self.author(
            recorder,
            kind,
            Subsystem::Harness,
            Some(turn),
            Some(Payload::Message(payload)),
        ))
    }

    /// Authors one seated identity message as `message.system`, the door for
    /// the prefix and the only turnless message door, per
    /// `weaver-harness-Spec` section 6.1. The prefix is seated at open through
    /// the declaration and passes `author_message` at no point, so before
    /// this door the accumulation rule of `weaver-trace-PRD` section 3.2
    /// based the effective context on a fact the record did not hold.
    ///
    /// **The turn is absent rather than borrowed from the turn that follows**,
    /// a prefix preceding every turn of the run, on the precedent the
    /// classify pair set for an exchange belonging to no turn.
    ///
    /// **Every role but system refuses here**, on the reasoning
    /// `author_message` refuses a tool result: a door that writes what it was
    /// not built for launders a bad declaration into a record that looks well
    /// formed.
    ///
    /// conforms: harness-identity-door-writes-system-only
    pub fn author_identity(
        &self,
        recorder: &mut Record,
        message: &Message,
    ) -> Result<Result<Sequence, RecordFailure>, UnlicensedMessage> {
        if !matches!(message.role, Role::System) {
            return Err(UnlicensedMessage {
                role: role_name(&message.role),
                block: "identity-door-system-only",
            });
        }
        licensed(message)?;
        let rendered = serde_json::to_string(message).map_err(|_| UnlicensedMessage {
            role: "system",
            block: "unrenderable",
        })?;
        let payload = raw_payload(&rendered).ok_or(UnlicensedMessage {
            role: "system",
            block: "unrenderable",
        })?;
        Ok(self.author(
            recorder,
            Kind::MessageSystem,
            Subsystem::Harness,
            None,
            Some(Payload::Message(payload)),
        ))
    }

    /// The restored prefix's door, per `weaver-harness-Spec` section 6.1:
    /// under a restoring load the door admits the roles the restored
    /// conversation carries, user, assistant, and tool result beside system,
    /// each authored turnless as `message.restored`, so the record of a
    /// branch is complete without its parent. **One kind for every role, the
    /// role riding in the message**, per `weaver-trace-Spec` section 3's
    /// restored-prefix clause as of 2026-09-26: the four turned message kinds
    /// stay turn-required, and a restored system message never reads as the
    /// identity the turnless `message.system` is. The system-only rule stands
    /// for the declaration's own field at `author_identity`, still the only
    /// prefix a declaration can write.
    pub fn author_restored(
        &self,
        recorder: &mut Record,
        message: &Message,
    ) -> Result<Result<Sequence, RecordFailure>, UnlicensedMessage> {
        licensed(message)?;
        // The floor's role set grows, per its non-exhaustive election, and a
        // role this door was not written for refuses by name rather than
        // being restored under a rule that never considered it.
        if !matches!(
            message.role,
            Role::System | Role::User | Role::Assistant | Role::ToolResult
        ) {
            return Err(UnlicensedMessage {
                role: role_name(&message.role),
                block: "restored-door-unknown-role",
            });
        }
        let rendered = serde_json::to_string(message).map_err(|_| UnlicensedMessage {
            role: role_name(&message.role),
            block: "unrenderable",
        })?;
        let payload = raw_payload(&rendered).ok_or(UnlicensedMessage {
            role: role_name(&message.role),
            block: "unrenderable",
        })?;
        Ok(self.author(
            recorder,
            Kind::MessageRestored,
            Subsystem::Harness,
            None,
            Some(Payload::Message(payload)),
        ))
    }

    /// Authors the `recall` event for an answered state-seam ask, per
    /// `weaver-trace-Spec` section 3's recall clause: the ask as it was sent
    /// and the identities of the events custody answered with, never their
    /// pairs, turnless. **Called before the answer reaches its consumer**, on
    /// the announce-after-record rule the flush follows: a loop, or an open,
    /// that built its input from an answer the record did not yet hold could
    /// act on it and leave no trace of what it was given. An ask that was not
    /// answered is never this door's, a dead seam being the refusal's or the
    /// fault's to record.
    pub fn author_recall(
        &self,
        recorder: &mut Record,
        verb: weaver_trace::RecallVerb,
        last_turns: Option<u64>,
        answered: &[crate::state::Recalled],
    ) -> Result<Sequence, RecordFailure> {
        let identity = |event: &crate::state::Recalled| weaver_trace::RecalledIdentity {
            run: event.run.clone(),
            turn: event.turn.clone(),
            sequence: event.sequence.clone(),
            kind: event.kind.clone(),
        };
        // A whole-session answer is named by its bounds and its count, a
        // partial one by every identity it returned.
        let (returned, count) = match verb {
            weaver_trace::RecallVerb::Replay => {
                let bounds = match answered {
                    [] => Vec::new(),
                    [only] => vec![identity(only)],
                    [first, .., last] => vec![identity(first), identity(last)],
                };
                (bounds, Some(answered.len() as u64))
            }
            weaver_trace::RecallVerb::Identity | weaver_trace::RecallVerb::Recall => {
                (answered.iter().map(identity).collect(), None)
            }
        };
        let account = weaver_trace::RecallAccount {
            ask: weaver_trace::RecallAsk { verb, last_turns },
            returned,
            count,
        };
        self.author(
            recorder,
            Kind::Recall,
            Subsystem::Harness,
            None,
            Some(Payload::Recall(account)),
        )
    }

    /// Authors a tool-result message from the granted value, the one door
    /// for the role, per `weaver-harness-Spec` section 6: the record is
    /// minted from the grant at this site and nowhere else, so what enters
    /// the conversation as a tool result is what crossed the gate exchange.
    pub fn author_tool_result(
        &self,
        recorder: &mut Record,
        grant: &crate::tools::ToolResult,
        turn: &TurnKey,
    ) -> Result<Sequence, RecordFailure> {
        let message = Message {
            role: Role::ToolResult,
            content: vec![weaver_traits::ContentBlock::ToolResult(grant.block())],
        };
        let rendered = serde_json::to_string(&message).expect("a grant's record renders");
        let payload = raw_payload(&rendered).expect("a grant's record splices");
        self.author(
            recorder,
            Kind::MessageToolResult,
            Subsystem::Harness,
            Some(turn),
            Some(Payload::Message(payload)),
        )
    }

    /// Authors the `fault` event a reported condition becomes. The payload is
    /// the floor's fault report serialized whole, the `case` round-tripping
    /// by spelling and the `account` passing verbatim, per `weaver-types-Spec`
    /// section 4.2: a case exists before any `fault` event is authored, so no
    /// raw account reaches the record unclassified. The reporting organ named
    /// the case, this crate included, and this function classifies nothing.
    pub fn author_fault(
        &self,
        recorder: &mut Record,
        subsystem: Subsystem,
        turn: Option<&TurnKey>,
        report: &weaver_types::FaultReport,
    ) -> Result<Sequence, RecordFailure> {
        let malformed = || {
            RecordFailure::Serving(weaver_trace::Failure::RefusedOnSubmit {
                reason: weaver_trace::SubmitRefusal::PayloadMalformed,
            })
        };
        let octets = serde_json::to_string(report).map_err(|_| malformed())?;
        let payload = raw_payload(&octets)
            .map(Payload::Fault)
            .ok_or_else(malformed)?;
        self.author(recorder, Kind::Fault, subsystem, turn, Some(payload))
    }
}

/// A report of this crate's own, the harness being an organ too, per apex
/// section 5.4, and therefore the renderer of its own accounts. The account
/// arrives as the JSON this crate wrote, and a string that does not parse is
/// this crate's own defect, answered by substituting the account with an
/// object that says so: the case survives and the fault still lands
/// classified, because every caller drops the authoring result, so refusing
/// here would lose the whole fault to a discarded error while the
/// substitution loses only the account and names the loss in the record.
pub fn harness_report(case: weaver_types::FaultCase, account: &str) -> weaver_types::FaultReport {
    let account =
        serde_json::value::RawValue::from_string(account.to_string()).unwrap_or_else(|_| {
            serde_json::value::RawValue::from_string(r#"{"account":"unrenderable"}"#.into())
                .expect("a literal object parses")
        });
    weaver_types::FaultReport { case, account }
}

/// The licensing rule of `weaver-traits-Spec` section 3: a `System` message
/// carries `Text` blocks - the operator's or the loop's framing of the
/// field, never a call and never a result - a `User` message carries `Text`
/// blocks, an `Assistant` message carries `Text` and `ToolCall` blocks, a
/// `ToolResult` message carries `ToolResult` blocks, and every other pairing
/// is unlicensed.
pub fn licensed(message: &Message) -> Result<(), UnlicensedMessage> {
    for block in &message.content {
        let ok = matches!(
            (&message.role, block),
            (Role::System, ContentBlock::Text { .. })
                | (Role::User, ContentBlock::Text { .. })
                | (Role::Assistant, ContentBlock::Text { .. })
                | (Role::Assistant, ContentBlock::ToolCall(_))
                | (Role::ToolResult, ContentBlock::ToolResult(_))
        );
        if !ok {
            return Err(UnlicensedMessage {
                role: role_name(&message.role),
                block: block_name(block),
            });
        }
    }
    Ok(())
}

fn role_name(role: &Role) -> &'static str {
    match role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::ToolResult => "tool_result",
        _ => "unknown",
    }
}

fn block_name(block: &ContentBlock) -> &'static str {
    match block {
        ContentBlock::Text { .. } => "text",
        ContentBlock::ToolCall(_) => "tool_call",
        ContentBlock::ToolResult(_) => "tool_result",
        _ => "unknown",
    }
}

/// The conversion the no-dependency rule of `weaver-trace-Spec` section 1
/// forces, a total function at the one site that submits.
fn convert_session(session: &SessionId) -> SessionRef {
    SessionRef(session.0.clone())
}

fn convert_run(run: &RunId) -> RunRef {
    RunRef(run.0.clone())
}

fn convert_turn(turn: &TurnKey) -> TurnRef {
    TurnRef(turn.0.clone())
}

fn wall_clock_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
