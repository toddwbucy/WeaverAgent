//! Original absolute windows for demand saves, Leave and JoinLeave.
//! The actor validates exchange and phase before installing Beginning or Complete.
use crate::channel::ChannelFault;
use std::cell::Cell;
use std::time::Duration;
use weaver_types::state_flow::{DecimalU64, SaveWindow, capture::CaptureBeginning};

const LEAVE_NS: u64 = weaver_types::LEAVE_BOUND_MS * 1_000_000;

pub(crate) struct CaptureWindows {
    outer_deadline: Option<u64>,
    save_wait: Option<u64>,
    forced: bool,
    last: Cell<u64>,
    capture: Option<CaptureBeginning>,
    settled: bool,
    refusal_only: bool,
    demand: Option<SaveWindow>,
}

impl CaptureWindows {
    /// Begin after Observe and loaded-copy verification, before the first send.
    pub(crate) fn demand(save_wait: u64) -> Result<Self, ChannelFault> {
        Self::demand_at(Self::now()?, save_wait)
    }

    fn demand_at(now: u64, save_wait: u64) -> Result<Self, ChannelFault> {
        let end = now
            .checked_add(save_wait)
            .ok_or(ChannelFault::Undecodable)?;
        let mut windows = Self::with_limit(now, Some(end), Some(save_wait), false)?;
        windows.demand = Some(SaveWindow {
            began_ns: now.into(),
            deadline_ns: end.into(),
        });
        Ok(windows)
    }

    pub(crate) fn demand_window(&self) -> Option<SaveWindow> {
        self.demand
    }

    /// Start before carrying the actual forced Leave. Observe has its own bound.
    pub(crate) fn begin(outer: Duration, save_wait: u64) -> Result<Self, ChannelFault> {
        Self::begin_with(outer, Some(save_wait))
    }

    /// A join shares the original publisher; it may install no capture window.
    pub(crate) fn join(outer: Duration) -> Result<Self, ChannelFault> {
        Self::begin_with(outer, None)
    }

    fn begin_with(outer: Duration, save_wait: Option<u64>) -> Result<Self, ChannelFault> {
        let now = Self::now()?;
        let duration = u64::try_from(outer.as_nanos()).map_err(|_| ChannelFault::Undecodable)?;
        let deadline = now.checked_add(duration).ok_or(ChannelFault::Undecodable)?;
        Self::with_limit(now, Some(deadline), save_wait, true)
    }

    /// The loaded drain and wind-down bounds belong to this original Leave.
    /// Either omission (or an unrepresentable sum) leaves S4/S5 unbounded.
    pub(crate) fn graceful(
        tail: Duration,
        drain: Option<u64>,
        wind_down: Option<u64>,
        save_wait: u64,
    ) -> Result<Self, ChannelFault> {
        Self::graceful_at(Self::now()?, tail, drain, wind_down, save_wait)
    }

    fn graceful_at(
        now: u64,
        tail: Duration,
        drain: Option<u64>,
        wind_down: Option<u64>,
        save_wait: u64,
    ) -> Result<Self, ChannelFault> {
        let deadline = drain.zip(wind_down).and_then(|(drain, wind_down)| {
            tail.checked_add(Duration::from_secs(drain))?
                .checked_add(Duration::from_secs(wind_down))?
                .as_nanos()
                .try_into()
                .ok()
                .and_then(|duration| now.checked_add(duration))
        });
        Self::with_limit(now, deadline, Some(save_wait), false)
    }

    #[cfg(test)]
    fn at(now: u64, outer_deadline: u64, save_wait: u64) -> Result<Self, ChannelFault> {
        Self::with_limit(now, Some(outer_deadline), Some(save_wait), true)
    }

    fn with_limit(
        now: u64,
        outer_deadline: Option<u64>,
        save_wait: Option<u64>,
        forced: bool,
    ) -> Result<Self, ChannelFault> {
        if outer_deadline.is_some_and(|end| now >= end) || save_wait == Some(0) {
            return Err(ChannelFault::Undecodable);
        }
        Ok(Self {
            outer_deadline,
            save_wait,
            forced,
            last: Cell::new(now),
            capture: None,
            settled: false,
            refusal_only: false,
            demand: None,
        })
    }

    pub(crate) fn capture(&self) -> Option<&CaptureBeginning> {
        self.capture.as_ref()
    }

    pub(crate) fn is_join(&self) -> bool {
        self.save_wait.is_none()
    }

    pub(crate) fn matches_leave(&self, forced: bool) -> bool {
        self.demand.is_none() && !self.is_join() && self.forced == forced
    }

    pub(crate) fn accept(&mut self, value: &CaptureBeginning) -> Result<(), ChannelFault> {
        self.accept_at(value, Self::now()?)
    }

    fn accept_at(&mut self, value: &CaptureBeginning, now: u64) -> Result<(), ChannelFault> {
        self.remaining_at(now)?;
        if self.capture.is_some() || value.capture_id.0 == 0 {
            return Err(ChannelFault::Undecodable);
        }
        if let Some(original) = self.demand {
            if value.window != original || value.tail_window.is_some() {
                return Err(ChannelFault::Undecodable);
            }
            self.capture = Some(value.clone());
            return Ok(());
        }
        let tail = value.tail_window.ok_or(ChannelFault::Undecodable)?;
        value
            .window
            .validate()
            .map_err(|_| ChannelFault::Undecodable)?;
        tail.validate().map_err(|_| ChannelFault::Undecodable)?;
        let end = value
            .window
            .began_ns
            .0
            .checked_add(self.save_wait.ok_or(ChannelFault::Undecodable)?)
            .ok_or(ChannelFault::Undecodable)?;
        if value.window.began_ns.0 > now
            || tail.began_ns.0 > value.window.began_ns.0
            || tail.deadline_ns.0.checked_sub(tail.began_ns.0) != Some(LEAVE_NS)
            || value.window.deadline_ns.0 != end.min(tail.deadline_ns.0)
        {
            return Err(ChannelFault::Undecodable);
        }
        if now >= value.window.deadline_ns.0 || now >= tail.deadline_ns.0 {
            return Err(ChannelFault::Unanswered);
        }
        self.capture = Some(value.clone());
        Ok(())
    }

    /// The actor's matching CaptureComplete after Published calls this.
    /// Retain the original windows as evidence; remove only the capture from waits.
    pub(crate) fn complete(&mut self, id: DecimalU64) -> Result<(), ChannelFault> {
        self.complete_at(id, Self::now()?)
    }

    /// The actor has validated a pre-Commit Leave refusal or capture expiry.
    /// Success stays forbidden; only the original finite Leave bounds remain.
    pub(crate) fn begin_refusal(&mut self) -> Result<(), ChannelFault> {
        self.begin_refusal_at(Self::now()?)
    }

    fn begin_refusal_at(&mut self, now: u64) -> Result<(), ChannelFault> {
        if self.demand.is_some()
            || self.settled
            || self
                .capture
                .as_ref()
                .and_then(|value| value.tail_window)
                .is_none()
        {
            return Err(ChannelFault::Undecodable);
        }
        self.refusal_only = true;
        self.remaining_at(now).map(|_| ())
    }

    pub(crate) fn capture_expired(&self) -> Result<bool, ChannelFault> {
        self.capture_expired_at(Self::now()?)
    }

    fn capture_expired_at(&self, now: u64) -> Result<bool, ChannelFault> {
        self.observe_clock(now)?;
        Ok(self.demand.is_none()
            && !self.settled
            && !self.refusal_only
            && self
                .capture
                .as_ref()
                .is_some_and(|value| now >= value.window.deadline_ns.0))
    }

    /// CaptureRefused is a cleanup acknowledgement, not a late successful save.
    pub(crate) fn refused(&mut self, id: DecimalU64) -> Result<(), ChannelFault> {
        self.refused_at(id, Self::now()?)
    }

    fn refused_at(&mut self, id: DecimalU64, now: u64) -> Result<(), ChannelFault> {
        if self
            .capture
            .as_ref()
            .is_none_or(|value| value.capture_id != id)
        {
            return Err(ChannelFault::Undecodable);
        }
        self.begin_refusal_at(now)?;
        self.settled = true;
        Ok(())
    }
    fn complete_at(&mut self, id: DecimalU64, now: u64) -> Result<(), ChannelFault> {
        self.remaining_at(now)?;
        if self.demand.is_some()
            || self.settled
            || self.refusal_only
            || self
                .capture
                .as_ref()
                .is_none_or(|value| value.capture_id != id)
        {
            return Err(ChannelFault::Undecodable);
        }
        self.settled = true;
        Ok(())
    }
    pub(crate) fn check(&self) -> Result<(), ChannelFault> {
        self.remaining().map(|_| ())
    }
    pub(crate) fn remaining(&self) -> Result<Option<Duration>, ChannelFault> {
        self.remaining_at(Self::now()?)
            .map(|left| left.map(Duration::from_nanos))
    }
    fn observe_clock(&self, now: u64) -> Result<(), ChannelFault> {
        if now < self.last.get() {
            return Err(ChannelFault::Closed);
        }
        self.last.set(now);
        Ok(())
    }
    fn remaining_at(&self, now: u64) -> Result<Option<u64>, ChannelFault> {
        self.observe_clock(now)?;
        let mut deadline = self.outer_deadline;
        if let Some(value) = &self.capture {
            if let Some(tail) = value.tail_window {
                deadline = Some(
                    deadline.map_or(tail.deadline_ns.0, |outer| outer.min(tail.deadline_ns.0)),
                );
            }
            if !self.settled && !self.refusal_only {
                deadline = deadline.map(|end| end.min(value.window.deadline_ns.0));
            }
        }
        deadline
            .map(|deadline| {
                deadline
                    .checked_sub(now)
                    .filter(|left| *left > 0)
                    .ok_or(ChannelFault::Unanswered)
            })
            .transpose()
    }
    fn now() -> Result<u64, ChannelFault> {
        #[cfg(test)]
        if let Some(now) = TEST_CLOCK.with(|clock| {
            let mut clock = clock.borrow_mut();
            let times = clock.as_mut()?;
            if times.len() > 1 {
                times.pop_front()
            } else {
                times.front().copied()
            }
        }) {
            return Ok(now);
        }
        crate::serving_load::window::raw_ns().map_err(|_| ChannelFault::Closed)
    }
}

#[cfg(test)]
#[path = "serving_capture_window_tests.rs"]
mod tests;

#[cfg(test)]
thread_local! {
    static TEST_CLOCK: std::cell::RefCell<Option<std::collections::VecDeque<u64>>> = const { std::cell::RefCell::new(None) };
}

/// Controls only this thread's capture-clock readings, not socket readiness or
/// filesystem outcomes. The final reading persists until the scope exits.
#[cfg(test)]
pub(crate) fn with_test_times<T>(times: &[u64], action: impl FnOnce() -> T) -> T {
    assert!(!times.is_empty());
    struct Restore(Option<std::collections::VecDeque<u64>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            TEST_CLOCK.with(|clock| *clock.borrow_mut() = self.0.take());
        }
    }
    let _restore =
        Restore(TEST_CLOCK.with(|clock| clock.replace(Some(times.iter().copied().collect()))));
    action()
}
