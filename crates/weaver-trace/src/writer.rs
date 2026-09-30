//! conforms: trace-sequence-gapless
//! conforms: trace-admission-precedes-fan-out
//! conforms: trace-queue-depth-not-config
//! conforms: trace-boundary-derived-not-stored
//! conforms: trace-whole-events-only
//! conforms: trace-pressure-reported-not-authored
//! conforms: trace-no-path-taking-constructor
//! conforms: trace-receive-site-takes-no-flag
//! conforms: trace-holds-no-recording-level
//! conforms: trace-receive-shape-pinned-by-doctest
//! conforms: trace-tee-no-turn-backpressure
//!
//! The emit path, the writer, and the commit boundary, per `weaver-trace-Spec`
//! sections 5 through 8. The recorder is the crate's principal type and owns
//! both sinks; the writer is a bounded queue and one thread draining to the
//! descriptor the harness was handed. Open takes descriptors and offers no
//! path-taking constructor, and this crate holds no recording level, because
//! none exists: everything produced is recorded.

use std::fs::File;
use std::io::Write;
use std::os::fd::OwnedFd;
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;

use crate::canonical::{Sequence, render};
use crate::event::{Event, Kind, Line, Payload, RunRef, SessionRef};
use crate::failure::{Failure, FieldName, SubmitRefusal, WriteError};
use crate::structure::{Record, WorkingStructure};
use crate::tee::Tee;

/// The queue's depth. A construction parameter of the deployment rather than a
/// configuration field, per `weaver-trace-Spec` section 6 - the composition
/// root supplies it when that root exists, and the receive shape's pin holds
/// the constructor to its three declared arguments until then. Generous
/// against loop 0's traffic.
pub const QUEUE_DEPTH: usize = 1024;

/// The depth at which pressure is reported rather than the depth of the queue
/// itself. An open election per section 12, settled by a measurement against a
/// real sink at a real rate; the interim value keeps the report ahead of the
/// queue filling.
pub const HIGH_WATER_MARK: usize = 768;

/// The interrogable state of the stream side, derived rather than stored:
/// committed is the highest sequence handed to the sink, admitted is the
/// highest assigned, queued is the difference, and a failed write is the last
/// error. Nothing about it survives the process.
#[derive(Debug, Clone, PartialEq)]
pub struct Boundary {
    pub committed: Option<Sequence>,
    pub admitted: Option<Sequence>,
    pub queued: usize,
    pub last_error: Option<WriteError>,
}

struct WriterShared {
    state: Mutex<WriterState>,
    drained: Condvar,
}

struct WriterState {
    committed: Option<Sequence>,
    last_error: Option<WriteError>,
    failed: Option<(Sequence, WriteError)>,
    /// **The drain's predicate, and it lives under the drain's mutex.**
    ///
    /// It was an `AtomicUsize` beside the mutex rather than inside it, which
    /// is a lost wakeup by construction: a waiter that has read the count and
    /// not yet reached `wait` holds the mutex, the writer needs no mutex to
    /// decrement and notify, so both can land in that window and the
    /// notification arrives before anyone is waiting for it. The waiter then
    /// sleeps on a count that is already zero and nothing will wake it.
    ///
    /// A condvar's predicate must be guarded by the condvar's mutex, which is
    /// what makes the wait atomic with respect to the change it waits for.
    /// Under the mutex the writer cannot decrement until the waiter has
    /// released it by sleeping, so the notify always finds the sleeper.
    queued: usize,
}

/// The stream writer: one thread, one descriptor, whole events only. A short
/// write is retried to completion rather than reported as success, which
/// `write_all` carries.
type QueueItem = (Sequence, Line);

struct Writer {
    queue: SyncSender<QueueItem>,
    shared: Arc<WriterShared>,
    thread: Option<JoinHandle<()>>,
}

impl Writer {
    fn start(sink: OwnedFd) -> Writer {
        let (queue, receiver): (SyncSender<QueueItem>, Receiver<QueueItem>) =
            sync_channel(QUEUE_DEPTH);
        let shared = Arc::new(WriterShared {
            state: Mutex::new(WriterState {
                committed: None,
                last_error: None,
                failed: None,
                queued: 0,
            }),
            drained: Condvar::new(),
        });
        let thread_shared = Arc::clone(&shared);
        let mut file = File::from(sink);
        let thread = std::thread::spawn(move || {
            while let Ok((sequence, line)) = receiver.recv() {
                // A write failure is terminal: committed never advances past
                // the first failed sequence, and every later queued record is
                // discarded with its accounting kept consistent, so a drain
                // returns rather than hanging on a dead sink.
                let already_failed = thread_shared
                    .state
                    .lock()
                    .expect("writer state")
                    .failed
                    .is_some();
                let outcome = (!already_failed).then(|| file.write_all(line.as_bytes()));
                // **The decrement and the notify happen under the mutex the
                // drain waits on.** That is what closes the window: a waiter
                // between its predicate read and its `wait` still holds this
                // lock, so this thread cannot reach the decrement until the
                // waiter has released it by sleeping.
                let mut state = thread_shared.state.lock().expect("writer state");
                match outcome {
                    Some(Ok(())) => state.committed = Some(sequence),
                    Some(Err(e)) => {
                        let err = WriteError::from(&e);
                        state.last_error = Some(err.clone());
                        state.failed = Some((sequence, err));
                    }
                    None => {}
                }
                state.queued -= 1;
                drop(state);
                thread_shared.drained.notify_all();
            }
        });
        Writer {
            queue,
            shared,
            thread: Some(thread),
        }
    }

    fn enqueue(&self, sequence: Sequence, line: Line) {
        self.shared.state.lock().expect("writer state").queued += 1;
        // A full queue blocks here until the writer frees space: the
        // deployment's loss bound expressed as backpressure rather than loss,
        // reached only after the high-water report warned the harness. After a
        // terminal write failure the writer discards, so the block cannot
        // outlive a dead sink. A send failure means the writer thread is
        // gone, surfaced through the boundary's last error.
        if self.queue.send((sequence, line)).is_err() {
            let mut state = self.shared.state.lock().expect("writer state");
            state.queued -= 1;
            state.last_error = Some(WriteError {
                os_error: None,
                message: "writer thread terminated".to_string(),
            });
            drop(state);
            // **This path must notify and the reason is that nothing else
            // can.** The send failed because the writer thread is gone, so
            // the receive loop that carries the other notify has already
            // ended. A drain waiting here would sleep on a count this line
            // just took to zero with no thread left to wake it, which is not
            // a narrow window but every time the two overlap.
            self.shared.drained.notify_all();
        }
    }

    fn queued(&self) -> usize {
        self.shared.state.lock().expect("writer state").queued
    }

    /// The failure a later submission surfaces: the first failed write, named
    /// with its record, or a dead writer thread.
    fn standing_failure(&self) -> Option<Failure> {
        let state = self.shared.state.lock().expect("writer state");
        if let Some((sequence, source)) = &state.failed {
            return Some(Failure::CommitFailed {
                sequence: *sequence,
                source: source.clone(),
            });
        }
        state
            .last_error
            .as_ref()
            .map(|err| Failure::WriteTargetUnusable {
                source: err.clone(),
            })
    }

    fn wait_drained(&self) -> Result<(), Failure> {
        let mut state = self.shared.state.lock().expect("writer state");
        while state.queued > 0 {
            state = self.shared.drained.wait(state).expect("writer state");
        }
        match &state.failed {
            Some((sequence, source)) => Err(Failure::CommitFailed {
                sequence: *sequence,
                source: source.clone(),
            }),
            None => match &state.last_error {
                Some(err) => Err(Failure::WriteTargetUnusable {
                    source: err.clone(),
                }),
                None => Ok(()),
            },
        }
    }

    fn boundary(&self, admitted: Option<Sequence>) -> Boundary {
        let state = self.shared.state.lock().expect("writer state");
        Boundary {
            committed: state.committed,
            admitted,
            queued: state.queued,
            last_error: state.last_error.clone(),
        }
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        // Closing the channel ends the thread's loop; the join keeps the
        // descriptor's writes ordered before the process moves on.
        let (closed, _) = sync_channel::<QueueItem>(1);
        let _ = std::mem::replace(&mut self.queue, closed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The recorder: the crate's principal type, owning the working structure and
/// the writer, per `weaver-trace-Spec` section 5. The emit path fans out to
/// both in one act, so one item holds both, and nothing else in the crate can
/// append to the structure or enqueue to the writer.
pub struct Recorder {
    session: SessionRef,
    run: RunRef,
    next: u64,
    structure: WorkingStructure,
    writer: Writer,
    tee: Option<Tee>,
}

impl Recorder {
    /// The crate's one constructor and its one receive site. It accepts an
    /// `OwnedFd` and nothing that could name a path, and it takes no flag
    /// argument: close-on-exec is supplied by the harness at its own receive,
    /// and append-only rides the open file description from admin's open.
    pub fn receive(sink: OwnedFd, run: RunRef, session: SessionRef) -> Result<Self, Failure> {
        Ok(Recorder {
            session,
            run,
            next: 0,
            structure: WorkingStructure::new(),
            writer: Writer::start(sink),
            tee: None,
        })
    }

    /// Attach the tee, per `weaver-trace-PRD` section 11: the harness
    /// applies it as the one party that writes, at load, after standing the
    /// state seam. The recorder feeds every admitted line through it, and a
    /// broken seam detaches it silently - the distillate is lost, never the
    /// turn, per the contract's dead-peer clause.
    pub fn attach_tee(&mut self, tee: Tee) {
        self.tee = Some(tee);
    }

    /// The four steps of the charter's emit path, in order, always: admit or
    /// refuse, assign the sequence, render once, fan out. The structure lands
    /// first and the return is the acknowledgment; a refusal has no effect on
    /// either sink, so the two cannot disagree about what was admitted.
    ///
    /// The sequence member of the submitted envelope is assigned here - step
    /// two is the recorder's - and the submitted value is overwritten rather
    /// than trusted, since a gapless run-scoped order cannot be authored by
    /// the caller.
    ///
    /// **A submission that lands answers `Ok` whatever the queue holds.**
    /// Pressure was returned here as an `Err` until 2026-08-22, after the
    /// event had reached the working structure and the queue, so a caller
    /// reading it was told the opposite of what happened. The queue's state
    /// is read from [`Recorder::pressure`] instead, and the harness authors
    /// the `fault` in response; this crate authors no event and holds no
    /// event kind.
    ///
    /// **At the queue's full depth a submission blocks**, which is the
    /// deployment's loss bound expressed as backpressure: the high-water
    /// reading warns first, and a terminal write failure frees the block by
    /// discarding. A block is not an answer this returns, so a caller that
    /// never reads the depth meets backpressure as latency rather than as a
    /// value.
    pub fn submit(&mut self, mut event: Event) -> Result<Sequence, Failure> {
        admit(&event, &self.session, &self.run)?;
        if let Some(failure) = self.writer.standing_failure() {
            return Err(failure);
        }
        let sequence = Sequence(self.next);
        event.envelope.sequence = sequence;
        let line = render(&event)?;
        self.next += 1;
        self.structure.append(Record {
            sequence,
            kind: event.envelope.kind,
            turn: event.envelope.turn.clone(),
            line: Line::clone(&line),
        });
        if let Some(tee) = &mut self.tee
            && !tee.feed(&line)
        {
            self.tee = None;
        }
        self.writer.enqueue(sequence, line);
        // **The event has landed and the answer says so**, per
        // `weaver-trace-Spec` section 9 as of 2026-08-22. Pressure was
        // returned here as a `Failure` after this point, so a caller reading
        // that `Err` was told its event had not been recorded when it had,
        // and every caller that discarded the result treated a recorded
        // event as a lost one. What the queue holds is read from
        // `pressure` rather than carried back in place of a sequence.
        Ok(sequence)
    }

    /// The writer's queue depth against the mark at which pressure is
    /// reported, per `weaver-trace-Spec` section 9.
    ///
    /// **A property of this recorder rather than of any submission**, so a
    /// caller with no interest in pressure is correct without saying so and
    /// one that wants to throttle asks. That is what keeps the mistake this
    /// replaces unavailable rather than corrected: a return that carried
    /// pressure in place of a sequence could be misread at the next port
    /// anyone adds, and a reading nobody takes changes nothing.
    ///
    /// **This crate reports and never authors.** What the harness does with
    /// the depth is its own, per charter section 2's sole-writer rule, and a
    /// recorder that emitted its own pressure event would have ended that
    /// rule in the act of reporting on it.
    pub fn pressure(&self) -> Pressure {
        let queued = self.writer.queued();
        Pressure {
            queued,
            over_mark: queued > HIGH_WATER_MARK,
        }
    }

    pub fn structure(&self) -> &WorkingStructure {
        &self.structure
    }

    pub fn boundary(&self) -> Boundary {
        let admitted = self.next.checked_sub(1).map(Sequence);
        self.writer.boundary(admitted)
    }

    /// Empties the queue and returns. At unload the harness calls this before
    /// answering left, so a left answer means everything admitted reached the
    /// sink. At process death nothing drains and the tail is forfeited, which
    /// this crate cannot report because the process reporting it died.
    pub fn drain(&mut self) -> Result<(), Failure> {
        self.writer.wait_drained()
    }
}

/// Admission, step one: the envelope's identity fields bind the event by
/// construction, the required fields are present for the kind, the kind is
/// known by type, and the payload is well-formed octets by `RawValue`
/// construction. What admission may judge is bounded: the interior of a
/// message is the harness's, and a recorder that parsed one would have taken a
/// judgment the charter denies it.
fn admit(event: &Event, session: &SessionRef, run: &RunRef) -> Result<(), Failure> {
    let refuse = |reason| Err(Failure::RefusedOnSubmit { reason });
    if event.envelope.session != *session || event.envelope.run != *run {
        // A submission bound to another run's identity is missing this run's:
        // the closest case the contract's vocabulary expresses.
        return refuse(SubmitRefusal::RequiredFieldAbsent {
            field: FieldName("session".to_string()),
        });
    }
    let kind = event.envelope.kind;
    if turn_required(kind) && event.envelope.turn.is_none() {
        return refuse(SubmitRefusal::RequiredFieldAbsent {
            field: FieldName("turn".to_string()),
        });
    }
    if turn_forbidden(kind) && event.envelope.turn.is_some() {
        // A run-level event carrying a turn would be a join key the work never
        // held, a false attribution, refused as a malformed submission - the
        // closest case the contract's vocabulary expresses.
        return refuse(SubmitRefusal::PayloadMalformed);
    }
    if !pairing_licensed(kind, event.payload.as_ref()) {
        return match event.payload {
            None => refuse(SubmitRefusal::RequiredFieldAbsent {
                field: FieldName("payload".to_string()),
            }),
            Some(_) => refuse(SubmitRefusal::PayloadMalformed),
        };
    }
    Ok(())
}

/// A run-level event belongs to no turn and refuses one; a turn-level kind
/// carries one, never inferred. `fault` may occur inside or outside a turn,
/// so its option stays the caller's fact.
fn turn_forbidden(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::Load
            | Kind::Unload
            | Kind::SessionClosed
            | Kind::Flush
            | Kind::Elision
            | Kind::Recall
            | Kind::MessageRestored
            | Kind::Score
    )
}

fn turn_required(kind: Kind) -> bool {
    match kind {
        // The classify pair is turn-optional like `fault`: a classify
        // between turns belongs to no turn, per the charter's adding text.
        Kind::Load
        | Kind::Unload
        | Kind::SessionClosed
        | Kind::Fault
        // A refusal is turn-optional for the fault's reason: an append
        // refused falls inside a turn and a flush or an elision refused
        // falls between them, so the turn is present exactly when the
        // refused ask belonged to one.
        | Kind::Refusal
        | Kind::Flush
        // The elision is asked between turns on the flush's ground, so it
        // belongs to no turn for the flush's reason.
        | Kind::Elision
        // A recall is asked between turns, the enter's before the first and
        // the seat's between a flush and the re-entry, so it belongs to no
        // turn for the flush's reason too.
        | Kind::Recall
        // A restored message is seated at the open, ahead of every turn of
        // the run, so it belongs to none, as the identity prefix does.
        | Kind::MessageRestored
        // A score is the task's verdict at the run's close, between turns.
        | Kind::Score
        | Kind::ClassifyRequest
        | Kind::ClassifyOutput
        // **`message.system` serves two cases and so is turn-optional**, per
        // `weaver-trace-Spec` section 3 as of the prefix act. A system
        // message submitted inside a turn carries that turn as every other
        // message kind does. The seated identity prefix carries none,
        // preceding every turn of the run, and the turn is present exactly
        // when the message belongs to one.
        | Kind::MessageSystem => false,
        Kind::TurnStarted
        | Kind::TurnClosed
        | Kind::MessageUser
        | Kind::MessageAssistant
        | Kind::MessageToolResult
        | Kind::ToolCallStarted
        | Kind::ToolCallCompleted
        | Kind::ModelRequest
        | Kind::ModelOutput
        | Kind::ModelMeasurement
        // The field is a decode position's, and every decode position
        // belongs to the turn that generated it.
        | Kind::ModelField => true,
    }
}

/// What the writer's queue holds, and whether it has passed the mark.
///
/// **Carrying the mark's verdict beside the depth is what keeps the
/// comparison in one place.** A caller deciding for itself whether 769 is
/// pressure would be holding a second copy of a constant this crate owns,
/// and the two would drift the first time the mark moved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pressure {
    pub queued: usize,
    pub over_mark: bool,
}

/// The total kind-to-payload mapping, twenty-four kinds and eighteen
/// dispositions, matching charter section 3.1 whole, enforced here because
/// the untagged payload leaves serde unable to. **`load` stopped being
/// payload-free 2026-08-21**: it carries the diagnostic elections of its
/// load, so a record declares the posture it was written in. **`unload`
/// carries `UnloadClose` where a member stood, as of 2026-09-04**, the
/// grant surface read back at the leave, and stays payload-free where no
/// member stood and no boundary was there to read.
fn pairing_licensed(kind: Kind, payload: Option<&Payload>) -> bool {
    matches!(
        (kind, payload),
        (Kind::Unload | Kind::SessionClosed | Kind::TurnStarted, None)
            | (Kind::Unload, Some(Payload::Unload(_)))
            | (Kind::Load, Some(Payload::Elections(_)))
            | (
                Kind::MessageSystem
                    | Kind::MessageUser
                    | Kind::MessageAssistant
                    | Kind::MessageToolResult
                    | Kind::MessageRestored,
                Some(Payload::Message(_))
            )
            | (Kind::TurnClosed, Some(Payload::TurnClosed(_)))
            | (Kind::Fault, Some(Payload::Fault(_)))
            | (Kind::Flush, Some(Payload::Flush(_)))
            | (Kind::Elision, Some(Payload::Elision(_)))
            | (Kind::Refusal, Some(Payload::Refusal(_)))
            | (Kind::ModelRequest, Some(Payload::ModelRequest(_)))
            | (Kind::ModelOutput, Some(Payload::ModelOutput(_)))
            | (Kind::ModelMeasurement, Some(Payload::ModelMeasurement(_)))
            | (Kind::ModelField, Some(Payload::ModelField(_)))
            | (Kind::ClassifyRequest, Some(Payload::ClassifyRequest(_)))
            | (Kind::ClassifyOutput, Some(Payload::ClassifyOutput(_)))
            | (Kind::Recall, Some(Payload::Recall(_)))
            | (Kind::Score, Some(Payload::Score(_)))
            | (
                Kind::ToolCallStarted | Kind::ToolCallCompleted,
                Some(Payload::Deferred(_))
            )
    )
}
