//! The pending graceful Leave stays with the unique serving seat while the
//! worker finishes its current turn. It is neither a Stop nor a forced Leave.
use super::*;
use crate::flow::clock::AbsoluteBound;
use crate::flow::context::startup_composition::refusal::response::listener::JoinedLeave;
use crate::flow::pump::{LifecycleMessage, LifecyclePeer, PumpDeadline};
use weaver_types::Cause;

#[path = "context_worker_graceful_quiesce.rs"]
pub(super) mod quiesce;

pub(in crate::flow::context::worker_startup) struct PendingGraceful {
    pub(super) peer: LifecyclePeer,
    pub(super) message: LifecycleMessage,
    pub(super) cause: Cause,
    pub(super) heard_ns: u64,
    pub(super) drain: Option<AbsoluteBound>,
    pub(super) wind_down_seconds: Option<u64>,
    pub(super) forced: bool,
    pub(super) forced_by: Option<Cause>,
    pub(super) joins: Vec<JoinedGraceful>,
    pub(super) peer_unavailable: bool,
    pub(super) detail: Option<String>,
    pub(super) quiesce: Option<Box<quiesce::Quiesce>>,
    pub(super) wind_down: Option<Box<super::wind_down::WindDown>>,
}

pub(super) struct JoinedGraceful {
    pub(super) peer: LifecyclePeer,
    pub(super) message: LifecycleMessage,
    pub(super) peer_unavailable: bool,
    pub(super) detail: Option<String>,
}

struct FailedJoin {
    _peer: crate::flow::pump::RetiredLifecyclePeer,
    _message: crate::flow::pump::RetiredLifecycleMessage,
    _peer_unavailable: bool,
    _detail: Option<String>,
}

/// Original phase accounts stay with the real Leave after its serving loans
/// end, including phases cancelled by force. The listener owns subsequent joins.
pub(in crate::flow::context::worker_startup) struct GracefulTail {
    _heard_ns: u64,
    _drain: Option<AbsoluteBound>,
    _wind_down_seconds: Option<u64>,
    forced: bool,
    forced_by: Option<Cause>,
    joins: Vec<JoinedGraceful>,
    pub(in crate::flow::context::worker_startup) quiesce: Option<Box<quiesce::Quiesce>>,
    pub(in crate::flow::context::worker_startup) wind_down: Option<Box<super::wind_down::WindDown>>,
}
pub(in crate::flow::context::worker_startup) struct GracefulParts {
    pub(in crate::flow::context::worker_startup) peer: LifecyclePeer,
    pub(in crate::flow::context::worker_startup) message: LifecycleMessage,
    pub(in crate::flow::context::worker_startup) peer_unavailable: bool,
    pub(in crate::flow::context::worker_startup) detail: Option<String>,
    pub(in crate::flow::context::worker_startup) tail: Box<GracefulTail>,
}
impl GracefulTail {
    pub(in crate::flow::context::worker_startup) fn outcome(&self) -> (bool, Option<Cause>) {
        (self.forced, self.forced_by)
    }
    pub(in crate::flow::context::worker_startup) fn take_joins(&mut self) -> Vec<JoinedLeave> {
        std::mem::take(&mut self.joins)
            .into_iter()
            .map(|join| {
                JoinedLeave::new(join.peer, join.message, join.peer_unavailable, join.detail)
            })
            .collect()
    }
}

pub(in crate::flow::context::worker_startup) struct FailedGraceful {
    _peer: crate::flow::pump::RetiredLifecyclePeer,
    _message: crate::flow::pump::RetiredLifecycleMessage,
    _cause: Cause,
    _heard_ns: u64,
    _drain: Option<AbsoluteBound>,
    _wind_down_seconds: Option<u64>,
    _forced: bool,
    _forced_by: Option<Cause>,
    _joins: Vec<FailedJoin>,
    _peer_unavailable: bool,
    _detail: Option<String>,
    _quiesce: Option<Box<quiesce::Quiesce>>,
    _wind_down: Option<Box<super::wind_down::WindDown>>,
}

impl PendingGraceful {
    /// The original input, Quiesce and wind-down accounts move together into
    /// Lower's cancellation path. No completed phase is invented by force.
    pub(in crate::flow::context::worker_startup) fn converted(&self) -> bool {
        self.forced
    }

    pub(in crate::flow::context::worker_startup) fn phases_settled(&self) -> bool {
        self.quiesce.as_ref().is_some_and(|work| work.settled())
            && self.wind_down.as_ref().is_some_and(|work| work.settled())
    }

    pub(in crate::flow::context::worker_startup) fn into_tail(self) -> GracefulParts {
        GracefulParts {
            peer: self.peer,
            message: self.message,
            peer_unavailable: self.peer_unavailable,
            detail: self.detail,
            tail: Box::new(GracefulTail {
                _heard_ns: self.heard_ns,
                _drain: self.drain,
                _wind_down_seconds: self.wind_down_seconds,
                forced: self.forced,
                forced_by: self.forced_by,
                joins: self.joins,
                quiesce: self.quiesce,
                wind_down: self.wind_down,
            }),
        }
    }
    pub(in crate::flow::context::worker_startup) fn retire_failed(self) -> FailedGraceful {
        FailedGraceful {
            _peer: self.peer.retire_failed(),
            _message: self.message.retire_failed(),
            _cause: self.cause,
            _heard_ns: self.heard_ns,
            _drain: self.drain,
            _wind_down_seconds: self.wind_down_seconds,
            _forced: self.forced,
            _forced_by: self.forced_by,
            _joins: self
                .joins
                .into_iter()
                .map(|join| FailedJoin {
                    _peer: join.peer.retire_failed(),
                    _message: join.message.retire_failed(),
                    _peer_unavailable: join.peer_unavailable,
                    _detail: join.detail,
                })
                .collect(),
            _peer_unavailable: self.peer_unavailable,
            _detail: self.detail,
            _quiesce: self.quiesce,
            _wind_down: self.wind_down,
        }
    }
}

impl FoundationInput<'_> {
    /// The original root request remains owned through turn completion and
    /// failure. This observation grants no capture, Lower, release or Left.
    pub fn has_graceful_leave(&self) -> bool {
        self.serving.graceful.is_some()
    }

    pub(super) fn graceful_waiting(&self) -> bool {
        self.serving.graceful.as_ref().is_some_and(|leave| {
            leave.forced
                || self.turn.is_none()
                || matches!(
                    self.stage,
                    Stage::Completed | Stage::Refused | Stage::Aborted
                )
        })
    }

    pub(super) fn graceful_pairs_need_step(&self) -> bool {
        self.has_graceful_leave()
            && self
                .engine
                .as_ref()
                .is_some_and(|work| work.graceful_pairs_need_step(self.serving.owner.context))
    }

    pub(super) fn retain_graceful_expiry(&mut self, failure: &RunFailure) -> bool {
        if self.retain_wind_down_expiry(failure) {
            return true;
        }
        let RunFailure::Wait {
            key: PumpDeadline::GracefulDrain,
            bound,
        } = failure
        else {
            return false;
        };
        let Some(leave) = &mut self.serving.graceful else {
            return false;
        };
        if leave.drain != Some(*bound)
            || self
                .serving
                .owner
                .context
                .deadlines
                .get(PumpDeadline::GracefulDrain)
                != Some(*bound)
            || self.serving.owner.context.failure().is_some()
        {
            return false;
        }
        // Keep the original phase window as evidence. The forthcoming forced
        // Lower owns its own anchor; expiry must not mint the 150-second tail.
        leave.forced = true;
        self.serving
            .owner
            .context
            .complete_wait(PumpDeadline::GracefulDrain);
        true
    }
}
