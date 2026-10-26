//! Suspend only the actual graceful directive, returning the existing input
//! to its worker policy with the request and its original clocks still owned.
use super::*;
use crate::flow::clock::AbsoluteBound;
use crate::flow::pump::PumpDeadline;

impl<'a> FoundationManagement<'a> {
    pub fn defer_graceful_leave(self: Box<Self>) -> Result<Box<FoundationInput<'a>>, Box<Self>> {
        self.defer_graceful_leave_with_io(&mut MonotonicIo)
    }

    pub(in crate::flow::context) fn defer_graceful_leave_with_io(
        mut self: Box<Self>,
        io: &mut impl PumpIo,
    ) -> Result<Box<FoundationInput<'a>>, Box<Self>> {
        if self.progress != FoundationManagementProgress::DirectivePending
            || self.message.is_some()
            || self.answer.is_some()
            || self.stop.is_some()
            || self.send.is_some()
            || self.input.has_graceful_leave()
            || !self.input.rollback_admissible()
        {
            return Err(self);
        }
        let Some(held) = self.peer.held() else {
            return Err(self);
        };
        let Payload::Directive(LifecycleDirective::Leave {
            cause,
            forced: false,
            rollback: false,
        }) = held.envelope.payload
        else {
            return Err(self);
        };
        if held.envelope.exchange.opener != Opener::Admin
            || held.envelope.position != Position::Open
            || held.sink.is_some()
            || held.state.is_some()
        {
            return Err(self);
        }
        let Some(heard_ns) = self.heard_ns else {
            return Err(self);
        };
        let owner = &mut self.input.serving.owner;
        let Some(load) = owner.resident.evidence.startup.loaded.as_ref() else {
            return Err(self);
        };
        let wind_down_seconds = load.wind_down_bound;
        let drain = match load.drain_bound {
            None => None,
            Some(seconds) => {
                let Some(nanos) = seconds.checked_mul(1_000_000_000) else {
                    return Err(self);
                };
                let Ok(bound) = AbsoluteBound::after(heard_ns, nanos) else {
                    return Err(self);
                };
                Some(bound)
            }
        };
        if owner
            .context
            .deadlines
            .get(PumpDeadline::GracefulDrain)
            .is_some()
            || owner.context.deadlines.get(PumpDeadline::Leave).is_some()
        {
            return Err(self);
        }
        if let Err(failure) = owner.context.live(io) {
            self.input.failure = Some(failure);
            self.input.stage = Stage::Stopped;
            self.progress = FoundationManagementProgress::Stopped;
            return Err(self);
        }
        self.message = match self.peer.take_completion() {
            Ok(Some(message)) => Some(message),
            Ok(None) => return Err(self),
            Err(error) => {
                self.detail = Some(format!("graceful Leave directive: {error:?}"));
                return Err(self);
            }
        };
        let Self {
            mut input,
            peer,
            message,
            peer_unavailable,
            detail,
            ..
        } = *self;
        input.serving.graceful = Some(Box::new(super::super::graceful::PendingGraceful {
            peer,
            message: message.expect("consumed actual graceful Leave"),
            cause,
            heard_ns,
            drain,
            wind_down_seconds,
            forced: false,
            forced_by: None,
            joins: Vec::new(),
            peer_unavailable,
            detail,
            quiesce: None,
            wind_down: None,
        }));
        if let Some(bound) = drain
            && let Err(failure) =
                input
                    .serving
                    .owner
                    .context
                    .arm(PumpDeadline::GracefulDrain, bound, io)
        {
            input.failure = Some(failure);
            input.stage = Stage::Stopped;
        }
        Ok(input)
    }

    /// During drain, only a real JoinLeave can convert this original graceful
    /// request. Its peer and cause remain with the original holder through the
    /// later cleanup. A second Leave never becomes a join.
    pub fn join_graceful_leave(mut self: Box<Self>) -> Result<Box<FoundationInput<'a>>, Box<Self>> {
        if self.progress != FoundationManagementProgress::DirectivePending
            || self.message.is_some()
            || self.answer.is_some()
            || self.stop.is_some()
            || self.send.is_some()
            || self.read_armed
            || !self.input.has_graceful_leave()
            || !self.input.rollback_admissible()
        {
            return Err(self);
        }
        let Some(held) = self.peer.held() else {
            return Err(self);
        };
        let Payload::Directive(LifecycleDirective::JoinLeave { cause }) = held.envelope.payload
        else {
            return Err(self);
        };
        if held.envelope.exchange.opener != Opener::Admin
            || held.envelope.position != Position::Open
            || held.sink.is_some()
            || held.state.is_some()
        {
            return Err(self);
        }
        let leave = self.input.serving.graceful.as_ref().unwrap();
        let phase = [PumpDeadline::WindDown, PumpDeadline::GracefulDrain]
            .into_iter()
            .find_map(|key| {
                self.input
                    .serving
                    .owner
                    .context
                    .deadlines
                    .get(key)
                    .map(|bound| (key, bound))
            });
        let before_expiry = !leave.forced
            && self
                .heard_ns
                .is_some_and(|heard| phase.is_none_or(|(_, bound)| !bound.expired(heard)));
        // Hearing, not a later loop handoff, orders the join against expiry.
        // A previously forced leave keeps its earlier cause. A timely heard
        // join does not become a bound election merely because reduction waits.
        if let Err(failure) = self.input.serving.owner.context.live(&mut MonotonicIo) {
            let timely_join = before_expiry
                && matches!(&failure,
                RunFailure::Wait { key, bound }
                if phase == Some((*key, *bound)));
            if !timely_join && !self.input.retain_graceful_expiry(&failure) {
                self.input.failure = Some(failure);
                self.input.stage = Stage::Stopped;
                self.progress = FoundationManagementProgress::Stopped;
                return Err(self);
            }
        }
        self.message = match self.peer.take_completion() {
            Ok(Some(message)) => Some(message),
            Ok(None) => return Err(self),
            Err(error) => {
                self.detail = Some(format!("graceful join directive: {error:?}"));
                return Err(self);
            }
        };
        let Self {
            mut input,
            peer,
            message,
            peer_unavailable,
            detail,
            ..
        } = *self;
        let leave = input.serving.graceful.as_mut().unwrap();
        if !leave.forced {
            leave.forced = true;
            leave.forced_by = Some(cause);
        }
        leave.joins.push(super::super::graceful::JoinedGraceful {
            peer,
            message: message.expect("actual complete joining directive"),
            peer_unavailable,
            detail,
        });
        input
            .serving
            .owner
            .context
            .complete_wait(PumpDeadline::GracefulDrain);
        input
            .serving
            .owner
            .context
            .complete_wait(PumpDeadline::WindDown);
        Ok(input)
    }
}
