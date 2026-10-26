//! One raw-clock load window from dispatch through the whole Ready answer.
use std::cell::Cell;
use std::time::Duration;
use weaver_types::state_flow::EnterWindow;
use weaver_types::{FieldName, LifecycleRefusal};

pub(crate) struct LoadWindow {
    window: EnterWindow,
    last: Cell<u64>,
}
impl LoadWindow {
    pub(crate) fn begin(budget: Duration) -> Result<Self, LifecycleRefusal> {
        Self::at(
            raw_ns().map_err(|_| LifecycleRefusal::BoundaryUnverified)?,
            budget,
        )
    }
    fn at(began_ns: u64, budget: Duration) -> Result<Self, LifecycleRefusal> {
        let invalid = || LifecycleRefusal::ConfigInvalid {
            field: Some(FieldName("load-bound-seconds".into())),
        };
        let budget_ns = u64::try_from(budget.as_nanos()).map_err(|_| invalid())?;
        if budget_ns == 0 {
            return Err(invalid());
        }
        let deadline_ns = began_ns.checked_add(budget_ns).ok_or_else(invalid)?;
        Ok(Self {
            window: EnterWindow {
                began_ns: began_ns.into(),
                deadline_ns: deadline_ns.into(),
            },
            last: Cell::new(began_ns),
        })
    }
    pub(crate) fn original(&self) -> EnterWindow {
        self.window
    }
    pub(crate) fn check(&self) -> Result<(), LifecycleRefusal> {
        self.remaining().map(|_| ())
    }
    pub(crate) fn remaining(&self) -> Result<Duration, LifecycleRefusal> {
        self.observe(raw_ns().map_err(|_| LifecycleRefusal::BoundaryUnverified)?)
            .map(Duration::from_nanos)
    }
    fn observe(&self, now: u64) -> Result<u64, LifecycleRefusal> {
        if now < self.last.get() {
            return Err(LifecycleRefusal::BoundaryUnverified);
        }
        self.last.set(now);
        self.window
            .deadline_ns
            .0
            .checked_sub(now)
            .filter(|v| *v > 0)
            .ok_or(LifecycleRefusal::NoResidency)
    }
    pub(crate) fn check_channel(&self) -> Result<(), crate::channel::ChannelFault> {
        self.remaining_channel().map(|_| ())
    }
    pub(crate) fn remaining_channel(&self) -> Result<Duration, crate::channel::ChannelFault> {
        self.remaining().map_err(|error| match error {
            LifecycleRefusal::NoResidency => crate::channel::ChannelFault::Unanswered,
            _ => crate::channel::ChannelFault::Closed,
        })
    }
}
pub(crate) fn raw_ns() -> std::io::Result<u64> {
    let mut time = nix::libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: the valid stack timespec is initialized by clock_gettime.
    if unsafe { nix::libc::clock_gettime(nix::libc::CLOCK_MONOTONIC, &mut time) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    let seconds = u64::try_from(time.tv_sec).ok();
    let nanos = u64::try_from(time.tv_nsec)
        .ok()
        .filter(|n| *n < 1_000_000_000);
    seconds
        .zip(nanos)
        .and_then(|(s, n)| s.checked_mul(1_000_000_000)?.checked_add(n))
        .ok_or_else(|| std::io::Error::other("invalid monotonic clock"))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn load_window_keeps_exact_anchor_and_refuses_expiry_overflow_or_reversal() {
        let window = LoadWindow::at(100, Duration::from_nanos(20)).unwrap();
        assert_eq!(
            window.original(),
            EnterWindow {
                began_ns: 100.into(),
                deadline_ns: 120.into()
            }
        );
        assert_eq!(window.observe(105), Ok(15));
        assert_eq!(window.observe(119), Ok(1));
        assert_eq!(window.observe(120), Err(LifecycleRefusal::NoResidency));
        assert_eq!(
            window.observe(119),
            Err(LifecycleRefusal::BoundaryUnverified)
        );
        for (anchor, budget) in [
            (0, Duration::ZERO),
            (u64::MAX, Duration::from_nanos(1)),
            (0, Duration::MAX),
        ] {
            assert!(
                matches!(LoadWindow::at(anchor, budget), Err(LifecycleRefusal::ConfigInvalid { field: Some(FieldName(field)) }) if field == "load-bound-seconds")
            );
        }
    }
}
