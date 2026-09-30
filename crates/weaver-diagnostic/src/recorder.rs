//! conforms: diagnostic-surface-mirrors-the-recorder
//! conforms: diagnostic-session-is-the-replays-own
//! conforms: diagnostic-admission-precedes-the-write
//! conforms: diagnostic-canonical-form-follows-trace
//!
//! The receive, the submit, and the write, per `weaver-diagnostic-Spec`
//! section 5. The surface mirrors `weaver-trace`'s receive and submit and
//! shares no type with it: the shape is what lets the harness hold the two
//! recorders under one form with the binding's kind selecting the arm, and
//! sharing no type is what keeps the mirror from becoming a dependency.
//!
//! **Where the sibling queues, this writer is synchronous, and the
//! difference is the working structure's absence.** The serving recorder
//! holds a RAM copy of the run and a background writer behind a queue; this
//! crate holds neither, per the contract's section 2 - a replay's present is
//! the holdings `weaver-state` serves, so a second copy here would hold by
//! one road what the loop already holds by another. With nothing to hold,
//! there is nothing to queue against, and the write happens in the submit:
//! admission first, the sequence assigned, the line rendered once, the bytes
//! on the sink before the call returns.

use std::io::Write;
use std::os::fd::OwnedFd;

use crate::event::{Event, Kind, Payload, RunRef, Sequence, SessionRef};
use crate::failure::{Failure, FieldName, SubmitRefusal, WriteError};

/// The diagnostic record's writer. Private members, no accessor yielding a
/// held event, and no path at any point: the receive takes a descriptor,
/// per the contract's section 6 and the handle discipline.
pub struct Recorder {
    session: SessionRef,
    run: RunRef,
    next: u64,
    sink: std::fs::File,
    standing: Option<Failure>,
}

impl Recorder {
    /// Takes possession of the sink admin opened for the binding. An
    /// `OwnedFd` and nothing that could name a path: close-on-exec is the
    /// harness's at its own receive, and append-only rides the open file
    /// description from admin's open.
    pub fn receive(sink: OwnedFd, run: RunRef, session: SessionRef) -> Result<Recorder, Failure> {
        Ok(Recorder {
            session,
            run,
            next: 0,
            sink: std::fs::File::from(sink),
            standing: None,
        })
    }

    /// The emit path, in order, always: admit or refuse, bind the identity,
    /// assign the sequence, render once, write. A refusal touches the sink
    /// at no point and consumes no sequence; a write failure is terminal for
    /// the record and every later submission answers with it.
    ///
    /// **The envelope's session, run, and sequence are bound here by
    /// construction rather than trusted**: the record is the diagnostic
    /// run's own, per section 3.1, so a caller cannot label an event with
    /// the replayed session's name - the one place that name may appear is
    /// the identity payload. A gapless run-scoped order likewise cannot be
    /// authored by the caller.
    pub fn submit(&mut self, mut event: Event) -> Result<Sequence, Failure> {
        if let Some(failure) = &self.standing {
            return Err(failure.clone());
        }
        admit(&event)?;
        event.envelope.session = self.session.clone();
        event.envelope.run = self.run.clone();
        let sequence = Sequence(self.next);
        event.envelope.sequence = sequence;
        let line = render(&event)?;
        if let Err(error) = self.sink.write_all(line.as_bytes()) {
            let failure = Failure::WriteFailed {
                error: WriteError(error.to_string()),
            };
            self.standing = Some(failure.clone());
            return Err(failure);
        }
        self.next += 1;
        Ok(sequence)
    }
}

/// Admission, before anything else: the kind-to-payload pairing is total
/// per section 3.3, and what admission may judge is bounded - the interior
/// of a spliced message is the harness's, and a recorder that parsed one
/// would have taken a judgment the charter denies it.
fn admit(event: &Event) -> Result<(), Failure> {
    let refuse = |refusal| Err(Failure::SubmitRefused { refusal });
    // **A spliced body that is the JSON literal `null` refuses as
    // malformed, and the check is framing rather than interpretation.** The
    // envelope's rule is that a kind carrying no payload emits no member at
    // all rather than a null one, and a `null` splice would render
    // `"payload":null` - a line no record carries. Judging that the whole
    // body is the absent-payload literal reads no member and takes no
    // judgment the charter denies: the interior stays the harness's.
    if let Some(Payload::Spliced(raw)) = &event.payload
        && raw.get() == "null"
    {
        return refuse(SubmitRefusal::PayloadMalformed);
    }
    // **The turn rule before the pairing**, per section 3.2's table: a kind
    // that belongs to no turn refuses one, a kind that belongs to a turn
    // refuses its absence, and the serving vocabulary's kinds keep their
    // serving rule, as they keep their serving meaning. The refusals are the
    // serving recorder's own, so one malformed event answers alike in both.
    match (turn_rule(event.envelope.kind), &event.envelope.turn) {
        (TurnRule::Forbidden, Some(_)) => return refuse(SubmitRefusal::PayloadMalformed),
        (TurnRule::Required, None) => {
            return refuse(SubmitRefusal::RequiredFieldAbsent {
                field: FieldName("turn".into()),
            });
        }
        _ => {}
    }
    match (&event.envelope.kind, &event.payload) {
        (Kind::ReplayOpened, Some(Payload::ReplayOpened(_)))
        | (Kind::ReplayIdentity, Some(Payload::ReplayIdentity(_)))
        | (Kind::ReplayClosed, Some(Payload::ReplayClosed(_)))
        | (Kind::ResidualColumn, Some(Payload::ResidualColumn(_))) => Ok(()),
        // `turn.started` is the one serving kind that carries no payload,
        // read from the record rather than assumed: a serving bracket opens
        // bare and closes carrying its close. A spliced payload here would
        // claim bytes the serving record never held.
        (Kind::TurnStarted, None) => Ok(()),
        (
            Kind::TurnClosed
            | Kind::MessageSystem
            | Kind::MessageUser
            | Kind::MessageAssistant
            | Kind::MessageToolResult
            | Kind::ModelRequest
            | Kind::ModelOutput
            | Kind::ModelMeasurement
            | Kind::ModelField
            | Kind::Flush
            | Kind::Refusal
            | Kind::Fault
            | Kind::Recall
            | Kind::Elision,
            Some(Payload::Spliced(_)),
        ) => Ok(()),
        (_, None) => refuse(SubmitRefusal::RequiredFieldAbsent {
            field: FieldName("payload".into()),
        }),
        (_, Some(_)) => refuse(SubmitRefusal::PayloadKindMismatch),
    }
}

/// Whether a kind belongs to a turn, per section 3.2's table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TurnRule {
    /// The kind belongs to no turn and refuses one.
    Forbidden,
    /// The kind belongs to a turn and refuses its absence.
    Required,
    /// The turn is present exactly when the event belongs to one.
    Optional,
}

/// **Exhaustive and wildcard-free**, so a kind added to the set cannot be
/// admitted until it is given a rule here and a row in section 3.2's table.
fn turn_rule(kind: Kind) -> TurnRule {
    match kind {
        // The bracket and the fact the pass established belong to the pass,
        // and a cut or an ask between turns belongs to none, as in a serving
        // record.
        Kind::ReplayOpened
        | Kind::ReplayIdentity
        | Kind::ReplayClosed
        | Kind::Flush
        | Kind::Elision
        | Kind::Recall => TurnRule::Forbidden,
        // What a replayed turn carries, and a column taken at one of its
        // positions.
        Kind::TurnStarted
        | Kind::TurnClosed
        | Kind::MessageUser
        | Kind::MessageAssistant
        | Kind::MessageToolResult
        | Kind::ModelRequest
        | Kind::ModelOutput
        | Kind::ModelMeasurement
        | Kind::ModelField
        | Kind::ResidualColumn => TurnRule::Required,
        // The seated prefix precedes every turn and a message in one carries
        // it, and a refusal or a fault falls inside a turn or between them.
        Kind::MessageSystem | Kind::Refusal | Kind::Fault => TurnRule::Optional,
    }
}

/// Renders an event to its one canonical line, newline-terminated, per
/// `weaver-trace-Spec` section 2 followed as prose in a crate that cannot
/// import it: one line of UTF-8 JSON, no interior newline, field order as
/// declared, big integers as decimal strings. serde_json escapes an
/// embedded newline in a string to `\n`, so prose cannot split an event -
/// but a spliced `RawValue` carries its bytes verbatim, and pretty-printed
/// JSON holds real newlines between tokens, which are valid JSON and fatal
/// to the framing. A body carrying one refuses as malformed rather than
/// splitting the stream.
fn render(event: &Event) -> Result<String, Failure> {
    let body = serde_json::to_string(event).map_err(|_| Failure::SubmitRefused {
        refusal: SubmitRefusal::PayloadMalformed,
    })?;
    if body.contains('\n') || body.contains('\r') {
        return Err(Failure::SubmitRefused {
            refusal: SubmitRefusal::PayloadMalformed,
        });
    }
    Ok(format!("{body}\n"))
}
