//! Reserved, nonblocking admission to one ordered sink writer.

use std::collections::VecDeque;
use std::fs::File;
use std::io::{self, Read, Write};
use std::os::fd::{AsFd, BorrowedFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::Instant;

use crate::{Boundary, Drained, Failure, HIGH_WATER_MARK, Line, Pressure, Sequence, WriteError};

pub const QUEUE_DEPTH: usize = 1024;
pub const TERMINAL_RESERVE: usize = 7;
pub const ORDINARY_CAPACITY: usize = QUEUE_DEPTH - TERMINAL_RESERVE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Terminal {
    Fault,
    ToolCompleted,
    ToolResult,
    TurnClosed,
    SessionClosed,
    SaveRefused,
    Unload,
}

impl Terminal {
    fn bit(self) -> u8 {
        1 << self as u8
    }
}

struct State {
    queue: VecDeque<(Sequence, Line)>,
    occupied: usize,
    peak: usize,
    committed: Option<Sequence>,
    failed: Option<(Sequence, WriteError)>,
    last_error: Option<WriteError>,
    reserved_terminals: u8,
    used_terminals: u8,
    closing: bool,
    abandoned: bool,
}

struct Shared {
    state: Mutex<State>,
    work: Condvar,
    drained: Condvar,
    notifier: UnixStream,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().expect("trace writer mutex")
    }

    fn signal(&self, state: &mut State) {
        let mut notifier = &self.notifier;
        match notifier.write(&[1]) {
            Ok(1) => {}
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
            Ok(_) => Self::notifier_failed(
                state,
                io::Error::new(
                    io::ErrorKind::WriteZero,
                    "trace capacity notifier wrote no byte",
                ),
            ),
            Err(error) => Self::notifier_failed(state, error),
        }
        self.drained.notify_all();
    }

    fn notifier_failed(state: &mut State, error: io::Error) {
        if state.last_error.is_none() {
            state.last_error = Some(WriteError::from(&error));
        }
    }
}

fn failure(state: &State) -> Option<Failure> {
    if let Some((sequence, source)) = &state.failed {
        Some(Failure::CommitFailed {
            sequence: *sequence,
            source: source.clone(),
        })
    } else {
        state
            .last_error
            .as_ref()
            .map(|source| Failure::WriteTargetUnusable {
                source: source.clone(),
            })
    }
}

pub(super) struct Writer {
    shared: Arc<Shared>,
    wake: UnixStream,
    thread: Option<JoinHandle<()>>,
}

pub(super) struct Reservation {
    shared: Arc<Shared>,
    terminal: Option<Terminal>,
    active: bool,
}

impl Reservation {
    pub fn enqueue(mut self, sequence: Sequence, line: Line) {
        let mut state = self.shared.lock();
        // All storage and occupancy were reserved before rendering or append.
        // The consumer removes entries, so push cannot require more capacity.
        state.queue.push_back((sequence, line));
        if let Some(terminal) = self.terminal {
            state.reserved_terminals &= !terminal.bit();
            state.used_terminals |= terminal.bit();
        }
        state.peak = state.peak.max(state.occupied);
        self.active = false;
        drop(state);
        self.shared.work.notify_one();
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        let mut state = self.shared.lock();
        let was_full = state.occupied >= ORDINARY_CAPACITY;
        state.occupied -= 1;
        if let Some(terminal) = self.terminal {
            state.reserved_terminals &= !terminal.bit();
        }
        if was_full && state.occupied < ORDINARY_CAPACITY {
            self.shared.signal(&mut state);
        }
        self.shared.drained.notify_all();
    }
}

impl Writer {
    pub fn start(sink: OwnedFd) -> Result<Self, Failure> {
        let (wake, notifier) = UnixStream::pair().map_err(unusable)?;
        wake.set_nonblocking(true).map_err(unusable)?;
        notifier.set_nonblocking(true).map_err(unusable)?;
        let mut queue = VecDeque::new();
        queue
            .try_reserve_exact(QUEUE_DEPTH)
            .map_err(|error| Failure::WriteTargetUnusable {
                source: WriteError {
                    os_error: None,
                    message: error.to_string(),
                },
            })?;
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                queue,
                occupied: 0,
                peak: 0,
                committed: None,
                failed: None,
                last_error: None,
                reserved_terminals: 0,
                used_terminals: 0,
                closing: false,
                abandoned: false,
            }),
            work: Condvar::new(),
            drained: Condvar::new(),
            notifier,
        });
        let worker = Arc::clone(&shared);
        let thread = std::thread::Builder::new()
            .name("serving-trace-writer".into())
            .spawn(move || {
                let mut sink = File::from(sink);
                loop {
                    let mut state = worker.lock();
                    while state.queue.is_empty() && !state.closing {
                        state = worker.work.wait(state).expect("trace writer mutex");
                    }
                    let Some((sequence, line)) = state.queue.pop_front() else {
                        break;
                    };
                    let writes = !state.abandoned && failure(&state).is_none();
                    drop(state);
                    let outcome = writes.then(|| sink.write_all(line.as_bytes()));
                    let mut state = worker.lock();
                    let was_full = state.occupied >= ORDINARY_CAPACITY;
                    let failed_now = match outcome {
                        Some(Ok(())) => {
                            state.committed = Some(sequence);
                            false
                        }
                        Some(Err(error)) => {
                            let error = WriteError::from(&error);
                            state.last_error = Some(error.clone());
                            state.failed = Some((sequence, error));
                            true
                        }
                        None => false,
                    };
                    state.occupied -= 1;
                    if failed_now || (was_full && state.occupied < ORDINARY_CAPACITY) {
                        worker.signal(&mut state);
                    }
                    worker.drained.notify_all();
                }
            })
            .map_err(unusable)?;
        Ok(Self {
            shared,
            wake,
            thread: Some(thread),
        })
    }

    pub fn standing_failure(&self) -> Option<Failure> {
        failure(&self.shared.lock())
    }

    pub fn reserve(&self, terminal: Option<Terminal>) -> Result<Option<Reservation>, Failure> {
        let mut state = self.shared.lock();
        if let Some(error) = failure(&state) {
            return Err(error);
        }
        if state.abandoned || state.closing {
            return Err(unusable(io::Error::other("trace writer is closed")));
        }
        if let Some(terminal) = terminal {
            if (state.reserved_terminals | state.used_terminals) & terminal.bit() != 0 {
                return Err(super::event::malformed());
            }
            state.reserved_terminals |= terminal.bit();
        } else if state.occupied >= ORDINARY_CAPACITY {
            return Ok(None);
        }
        // Each of the seven terminal bits has its own slot. Ordinary callers
        // can hold at most 1017, including the in-flight write and reservations.
        debug_assert!(state.occupied < QUEUE_DEPTH);
        state.occupied += 1;
        Ok(Some(Reservation {
            shared: Arc::clone(&self.shared),
            terminal,
            active: true,
        }))
    }

    pub fn capacity_fd(&self) -> BorrowedFd<'_> {
        self.wake.as_fd()
    }

    pub fn drain_capacity_wake(&mut self) -> Result<(), Failure> {
        let mut bytes = [0_u8; 256];
        loop {
            match self.wake.read(&mut bytes) {
                Ok(0) => {
                    return self.wake_failed(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "trace capacity notifier closed",
                    ));
                }
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    return self.standing_failure().map_or(Ok(()), Err);
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => return self.wake_failed(error),
            }
        }
    }

    fn wake_failed(&self, error: io::Error) -> Result<(), Failure> {
        let mut state = self.shared.lock();
        Shared::notifier_failed(&mut state, error);
        self.shared.drained.notify_all();
        Err(failure(&state).expect("notifier failure recorded"))
    }

    pub fn pressure(&self) -> Pressure {
        let state = self.shared.lock();
        Pressure {
            queued: state.occupied,
            over_mark: state.occupied > HIGH_WATER_MARK,
            peak: state.peak,
        }
    }

    pub fn boundary(&self, admitted: Option<Sequence>) -> Boundary {
        let state = self.shared.lock();
        Boundary {
            committed: state.committed,
            admitted,
            queued: state.occupied,
            last_error: state.last_error.clone(),
        }
    }

    pub fn drain(&self) -> Result<(), Failure> {
        let mut state = self.shared.lock();
        while state.occupied != 0 {
            state = self.shared.drained.wait(state).expect("trace writer mutex");
        }
        failure(&state).map_or(Ok(()), Err)
    }

    pub fn drain_by(&self, deadline: Instant) -> Result<Drained, Failure> {
        let mut state = self.shared.lock();
        while state.occupied != 0 {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(Drained::Expired {
                    queued: state.occupied,
                });
            }
            state = self
                .shared
                .drained
                .wait_timeout(state, remaining)
                .expect("trace writer mutex")
                .0;
        }
        failure(&state).map_or(Ok(Drained::Emptied), Err)
    }

    pub fn abandon(&self) {
        let mut state = self.shared.lock();
        state.abandoned = true;
        self.shared.work.notify_all();
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        let mut state = self.shared.lock();
        state.closing = true;
        let abandoned = state.abandoned;
        drop(state);
        self.shared.work.notify_all();
        if abandoned {
            drop(self.thread.take());
        } else if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn unusable(error: io::Error) -> Failure {
    Failure::WriteTargetUnusable {
        source: WriteError::from(&error),
    }
}

#[cfg(test)]
#[path = "writer_tests.rs"]
mod tests;
