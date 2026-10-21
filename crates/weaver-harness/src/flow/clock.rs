//! Common numeric clock and wait mechanics for HARNESS-CLOCK and HARNESS-PUMP.
//!
//! The caller owns barrier policy, completion slots and reduction. This module
//! neither chooses a bound nor runs a source. Raw readings are CLOCK_MONOTONIC
//! nanoseconds; only envelope timestamps subtract the captured run origin.

use std::fmt;
use std::io;
use std::os::fd::{AsRawFd, BorrowedFd};

use nix::poll::{PollFlags, PollTimeout};

const NANOS_PER_SECOND: u64 = 1_000_000_000;
const NANOS_PER_MILLISECOND: u64 = 1_000_000;

#[derive(Debug, Clone, Copy)]
pub(super) struct WaitInterest<'fd> {
    pub fd: BorrowedFd<'fd>,
    pub events: PollFlags,
}

/// Index into the interests supplied to this wait, with the kernel's flags.
/// HUP, ERR and NVAL remain visible to the owning source even if not requested.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ReadyFd {
    pub index: usize,
    pub events: PollFlags,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum WaitOutcome {
    Ready(Vec<ReadyFd>),
    Timeout,
}

/// Tests replace the clock and the wait together. A fake numeric clock must
/// never be combined with a real poll that it cannot wake.
pub(super) trait PumpIo {
    fn now(&mut self) -> io::Result<u64>;
    fn wait(
        &mut self,
        interests: &[WaitInterest<'_>],
        absolute_deadline: Option<u64>,
    ) -> io::Result<WaitOutcome>;
}

#[derive(Debug, Default)]
pub(super) struct MonotonicIo;

impl PumpIo for MonotonicIo {
    fn now(&mut self) -> io::Result<u64> {
        monotonic_ns()
    }

    fn wait(
        &mut self,
        interests: &[WaitInterest<'_>],
        absolute_deadline: Option<u64>,
    ) -> io::Result<WaitOutcome> {
        unique_descriptors(interests)?;
        let mut descriptors = Vec::new();
        descriptors
            .try_reserve_exact(interests.len())
            .map_err(io::Error::other)?;
        descriptors.extend(interests.iter().map(|interest| nix::libc::pollfd {
            fd: interest.fd.as_raw_fd(),
            events: interest.events.bits(),
            revents: 0,
        }));
        wait_until(absolute_deadline, monotonic_ns, |timeout| {
            let count = nix::libc::nfds_t::try_from(descriptors.len())
                .map_err(|_| io::Error::other("poll descriptor count overflows"))?;
            // SAFETY: every fd is borrowed for the entire wait, and this vector
            // owns count writable pollfd entries. Raw revents preserves Linux
            // POLLRDHUP, which the pinned nix PollFd::revents would discard.
            if unsafe { nix::libc::poll(descriptors.as_mut_ptr(), count, i32::from(timeout)) } < 0 {
                return Err(io::Error::last_os_error());
            }
            let mut ready = Vec::new();
            ready
                .try_reserve_exact(interests.len())
                .map_err(io::Error::other)?;
            for (index, descriptor) in descriptors.iter().enumerate() {
                let events = PollFlags::from_bits_retain(descriptor.revents);
                if !events.is_empty() {
                    ready.push(ReadyFd { index, events });
                }
            }
            Ok(ready)
        })
    }
}

/// The one raw source used by authoring, intake and the production wait.
pub(super) fn monotonic_ns() -> io::Result<u64> {
    let mut time = nix::libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: clock_gettime initializes the valid stack timespec on success.
    if unsafe { nix::libc::clock_gettime(nix::libc::CLOCK_MONOTONIC, &mut time) } != 0 {
        return Err(io::Error::last_os_error());
    }
    timespec_ns(time.tv_sec, time.tv_nsec)
}

// The production clock and poll travel together above. Unit tests replace both
// callbacks below to check exact syscall-boundary cases without sleeping or
// trying to wake real poll with a fake clock.
fn wait_until(
    absolute_deadline: Option<u64>,
    mut now: impl FnMut() -> io::Result<u64>,
    mut wait: impl FnMut(PollTimeout) -> io::Result<Vec<ReadyFd>>,
) -> io::Result<WaitOutcome> {
    let mut previous = None;
    let mut sample = || {
        let observed = now()?;
        if previous.is_some_and(|previous| observed < previous) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "monotonic clock reversed during wait",
            ));
        }
        previous = Some(observed);
        Ok(observed)
    };
    loop {
        let before = sample()?;
        if absolute_deadline.is_some_and(|deadline| before >= deadline) {
            return Ok(WaitOutcome::Timeout);
        }
        let ready = match wait(poll_timeout(before, absolute_deadline)) {
            Ok(ready) => ready,
            // The same absolute deadline is rechecked at the loop head.
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        // A ready source cannot win against a bound that has now expired.
        let after = sample()?;
        if absolute_deadline.is_some_and(|deadline| after >= deadline) {
            return Ok(WaitOutcome::Timeout);
        }
        if !ready.is_empty() {
            return Ok(WaitOutcome::Ready(ready));
        }
        // Long waits use multiple representable poll timeouts. An early
        // timeout never declares expiry or creates a fresh deadline.
    }
}

fn timespec_ns(seconds: nix::libc::time_t, nanos: nix::libc::c_long) -> io::Result<u64> {
    let seconds = u64::try_from(seconds)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "negative monotonic seconds"))?;
    let nanos = u64::try_from(nanos)
        .ok()
        .filter(|nanos| *nanos < NANOS_PER_SECOND)
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "invalid monotonic nanoseconds")
        })?;
    seconds
        .checked_mul(NANOS_PER_SECOND)
        .and_then(|seconds| seconds.checked_add(nanos))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "monotonic clock overflow"))
}

fn unique_descriptors(interests: &[WaitInterest<'_>]) -> io::Result<()> {
    for (index, interest) in interests.iter().enumerate() {
        if interests[..index]
            .iter()
            .any(|earlier| earlier.fd.as_raw_fd() == interest.fd.as_raw_fd())
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "one transport descriptor appears twice in the wait set",
            ));
        }
    }
    Ok(())
}

fn poll_timeout(now: u64, absolute_deadline: Option<u64>) -> PollTimeout {
    match absolute_deadline {
        None => PollTimeout::NONE,
        Some(deadline) => {
            let millis = deadline
                .saturating_sub(now)
                .div_ceil(NANOS_PER_MILLISECOND)
                .min(i32::MAX as u64);
            PollTimeout::try_from(millis).expect("timeout is in the poll i32 range")
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ClockOrigin(u64);

impl ClockOrigin {
    pub(super) fn capture(io: &mut impl PumpIo) -> io::Result<Self> {
        io.now().map(Self)
    }

    pub(super) fn at(origin_ns: u64) -> Self {
        Self(origin_ns)
    }

    pub(super) fn raw(self) -> u64 {
        self.0
    }

    pub(super) fn elapsed(self, now: u64) -> Result<u64, BoundError> {
        now.checked_sub(self.0).ok_or(BoundError::Reversed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BoundError {
    Overflow,
    Reversed,
    AlreadyArmed,
    NotArmed,
    Extension,
    Allocation,
}

impl fmt::Display for BoundError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Overflow => "absolute deadline overflow",
            Self::Reversed => "clock or deadline precedes its original anchor",
            Self::AlreadyArmed => "deadline is already armed",
            Self::NotArmed => "deadline is not armed",
            Self::Extension => "an armed deadline cannot be extended",
            Self::Allocation => "deadline registry allocation failed",
        })
    }
}

impl std::error::Error for BoundError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct AbsoluteBound {
    began_ns: u64,
    deadline_ns: u64,
}

impl AbsoluteBound {
    /// Zero remaining time represents an already exhausted inherited budget.
    /// Declared MeasuredWait positivity is validated at the configuration seam.
    pub(super) fn after(began_ns: u64, elapsed_ns: u64) -> Result<Self, BoundError> {
        let deadline_ns = began_ns
            .checked_add(elapsed_ns)
            .ok_or(BoundError::Overflow)?;
        Self::from_absolute(began_ns, deadline_ns)
    }

    pub(super) fn from_absolute(began_ns: u64, deadline_ns: u64) -> Result<Self, BoundError> {
        if deadline_ns < began_ns {
            return Err(BoundError::Reversed);
        }
        Ok(Self {
            began_ns,
            deadline_ns,
        })
    }

    pub(super) fn began_ns(self) -> u64 {
        self.began_ns
    }

    pub(super) fn deadline_ns(self) -> u64 {
        self.deadline_ns
    }

    pub(super) fn expired(self, now: u64) -> bool {
        now >= self.deadline_ns
    }

    pub(super) fn elapsed(self, now: u64) -> Result<u64, BoundError> {
        now.checked_sub(self.began_ns).ok_or(BoundError::Reversed)
    }
}

/// Keys belong to the owner: this registry does not invent barrier types,
/// priorities or elected durations. Observation ticks are separate wake times.
#[derive(Debug)]
pub(super) struct DeadlineRegistry<K> {
    armed: Vec<(K, AbsoluteBound)>,
}

impl<K: Copy + Eq> DeadlineRegistry<K> {
    pub(super) fn new() -> Self {
        Self { armed: Vec::new() }
    }

    pub(super) fn arm(&mut self, key: K, bound: AbsoluteBound) -> Result<(), BoundError> {
        if self.get(key).is_some() {
            return Err(BoundError::AlreadyArmed);
        }
        self.armed
            .try_reserve(1)
            .map_err(|_| BoundError::Allocation)?;
        self.armed.push((key, bound));
        Ok(())
    }

    pub(super) fn get(&self, key: K) -> Option<AbsoluteBound> {
        self.armed
            .iter()
            .find_map(|(armed, bound)| (*armed == key).then_some(*bound))
    }

    /// A force join or earlier lifecycle deadline can shorten a bound while
    /// preserving the first anchor. No call can renew it after partial progress.
    pub(super) fn shorten(&mut self, key: K, deadline_ns: u64) -> Result<(), BoundError> {
        let bound = self
            .armed
            .iter_mut()
            .find_map(|(armed, bound)| (*armed == key).then_some(bound))
            .ok_or(BoundError::NotArmed)?;
        if deadline_ns > bound.deadline_ns {
            return Err(BoundError::Extension);
        }
        if deadline_ns < bound.began_ns {
            return Err(BoundError::Reversed);
        }
        bound.deadline_ns = deadline_ns;
        Ok(())
    }

    /// The owner calls this only at the contract's completed disposition.
    /// Transport progress and readiness never disarm a registry entry.
    pub(super) fn disarm(&mut self, key: K) -> Option<AbsoluteBound> {
        let index = self.armed.iter().position(|(armed, _)| *armed == key)?;
        Some(self.armed.remove(index).1)
    }

    pub(super) fn nearest(&self) -> Option<u64> {
        self.armed.iter().map(|(_, bound)| bound.deadline_ns).min()
    }

    /// Returns all due bounds; the owning state machine determines precedence.
    pub(super) fn expired(&self, now: u64) -> impl Iterator<Item = (K, AbsoluteBound)> + '_ {
        self.armed
            .iter()
            .copied()
            .filter(move |(_, bound)| bound.expired(now))
    }
}

#[cfg(test)]
pub(super) mod testing {
    use super::*;
    use std::collections::VecDeque;

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub(in crate::flow) struct FakeWait {
        pub began_ns: u64,
        pub absolute_deadline: Option<u64>,
        pub descriptors: Vec<(i32, PollFlags)>,
    }

    /// Deterministic clock and readiness timeline. It never calls poll, reads
    /// descriptors, or advances a second independently controlled test clock.
    #[derive(Debug)]
    pub(in crate::flow) struct FakePumpIo {
        now: u64,
        scheduled: VecDeque<(u64, Vec<ReadyFd>)>,
        waits: Vec<FakeWait>,
    }

    impl FakePumpIo {
        pub(in crate::flow) fn new(now: u64) -> Self {
            Self {
                now,
                scheduled: VecDeque::new(),
                waits: Vec::new(),
            }
        }

        pub(in crate::flow) fn schedule(&mut self, at: u64, ready: Vec<ReadyFd>) -> io::Result<()> {
            if at < self.now
                || self.scheduled.back().is_some_and(|(last, _)| at < *last)
                || ready.is_empty()
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "fake readiness must be nonempty and in timeline order",
                ));
            }
            self.scheduled.push_back((at, ready));
            Ok(())
        }

        pub(in crate::flow) fn advance_to(&mut self, now: u64) -> io::Result<()> {
            if now < self.now {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "fake monotonic clock cannot reverse",
                ));
            }
            self.now = now;
            Ok(())
        }

        pub(in crate::flow) fn waits(&self) -> &[FakeWait] {
            &self.waits
        }
    }

    impl PumpIo for FakePumpIo {
        fn now(&mut self) -> io::Result<u64> {
            Ok(self.now)
        }

        fn wait(
            &mut self,
            interests: &[WaitInterest<'_>],
            absolute_deadline: Option<u64>,
        ) -> io::Result<WaitOutcome> {
            unique_descriptors(interests)?;
            self.waits.push(FakeWait {
                began_ns: self.now,
                absolute_deadline,
                descriptors: interests
                    .iter()
                    .map(|interest| (interest.fd.as_raw_fd(), interest.events))
                    .collect(),
            });
            if absolute_deadline.is_some_and(|deadline| self.now >= deadline) {
                return Ok(WaitOutcome::Timeout);
            }
            match self.scheduled.front() {
                Some((at, _)) if absolute_deadline.is_some_and(|deadline| *at >= deadline) => {
                    self.now = absolute_deadline.expect("the deadline was compared");
                    Ok(WaitOutcome::Timeout)
                }
                Some(_) => {
                    let (at, ready) = self.scheduled.pop_front().expect("scheduled wake exists");
                    if ready.iter().any(|item| {
                        interests.get(item.index).is_none_or(|interest| {
                            item.events.is_empty()
                                || !(interest.events
                                    | PollFlags::POLLHUP
                                    | PollFlags::POLLERR
                                    | PollFlags::POLLNVAL)
                                    .contains(item.events)
                        })
                    }) || ready.iter().enumerate().any(|(index, item)| {
                        ready[..index]
                            .iter()
                            .any(|earlier| earlier.index == item.index)
                    }) {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "fake wake does not name the supplied wait interests exactly once",
                        ));
                    }
                    self.now = self.now.max(at);
                    Ok(WaitOutcome::Ready(ready))
                }
                None => match absolute_deadline {
                    Some(deadline) => {
                        self.now = deadline;
                        Ok(WaitOutcome::Timeout)
                    }
                    None => Err(io::Error::new(
                        io::ErrorKind::WouldBlock,
                        "fake unbounded wait has no scheduled wake",
                    )),
                },
            }
        }
    }
}

#[cfg(test)]
#[path = "clock_tests.rs"]
mod tests;
