//! Socket-reactor ownership for the separate STE100 duty.
//!
//! One worker constructs, checks with, and drops its backend and retained inputs.
//! The reactor owns the only duty socket and continues observing it throughout
//! admission and whole-block work. This module elects no grammar/model adapter,
//! sentence scheduling policy, qualification value, or replacement serving path.

use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError};
use std::thread::{self, JoinHandle};

use nix::libc;
use weaver_types::state_flow::{
    Ste100AdmissionCode, Ste100AdmissionRefused, Ste100ComponentCode, Ste100Request, Ste100Result,
};

use super::admission::{
    AdapterAdmission, AdmissionIdentity, Admitted, CheckBackend, ValidatedBootstrap,
    admit_from_path,
};
use super::channel::Channel;
pub(super) use super::dispatcher::Fault;
use super::dispatcher::{Action, Dispatcher, Position, Stop};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RuntimeProgress {
    Pending,
    Progress,
    Stopped(Stop),
}

#[derive(Clone, Copy)]
enum AdmissionMode {
    Production,
    #[cfg(test)]
    Fixture,
}

enum Completion {
    Admitted(Box<AdmissionIdentity>),
    Refused(Ste100AdmissionRefused),
    Checked(Ste100Result),
}

/// Wake also on unwind, so a dead worker cannot leave the reactor waiting for a
/// completion that will never exist. The wake contains no result or authority.
struct Notify {
    wake: UnixStream,
    completed: Option<SyncSender<Completion>>,
}
impl Notify {
    fn send(&mut self, completion: Completion) -> bool {
        let sent = self
            .completed
            .as_ref()
            .is_some_and(|sender| sender.try_send(completion).is_ok());
        self.wake();
        sent
    }
    fn wake(&mut self) {
        loop {
            match self.wake.write(&[1]) {
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                // WouldBlock means an earlier notification is already readable.
                _ => return,
            }
        }
    }
}
impl Drop for Notify {
    fn drop(&mut self) {
        // Disconnect first, then wake: the reactor cannot drain the last wake
        // while the channel still appears live and sleep forever after unwind.
        drop(self.completed.take());
        self.wake();
    }
}

/// Fields drop in declaration order. Keep Notify first: disconnect and wake the
/// reactor before an admitted backend destructor can block, on return or unwind.
/// This ordering belongs to one owner and survives moving it through a helper.
struct WorkerState<B> {
    notify: Notify,
    owner: Option<Admitted<B>>,
}

struct Worker {
    commands: SyncSender<Ste100Request>,
    completions: Receiver<Completion>,
    wake: UnixStream,
    thread: JoinHandle<()>,
}

impl Worker {
    fn start<A>(bootstrap: ValidatedBootstrap, adapter: A, mode: AdmissionMode) -> io::Result<Self>
    where
        A: AdapterAdmission + Send + 'static,
        A::Backend: CheckBackend,
    {
        let (wake, wake_worker) = UnixStream::pair()?;
        wake.set_nonblocking(true)?;
        wake_worker.set_nonblocking(true)?;
        let (commands, work) = mpsc::sync_channel::<Ste100Request>(1);
        let (completed, completions) = mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .name("weaver-ste100-check".into())
            .spawn(move || {
                let mut state = WorkerState {
                    notify: Notify {
                        wake: wake_worker,
                        completed: Some(completed),
                    },
                    owner: None,
                };
                // B need not be Send: construction, every operation, and destruction
                // all happen here. Admission owns exact file bytes throughout use.
                let admission = match mode {
                    AdmissionMode::Production => admit_from_path(bootstrap, &adapter),
                    #[cfg(test)]
                    AdmissionMode::Fixture => {
                        super::admission::admit_from_path_fixture(bootstrap, &adapter)
                    }
                };
                state.owner = match admission {
                    Ok(owner) => Some(owner),
                    Err(refusal) => {
                        state.notify.send(Completion::Refused(refusal));
                        return;
                    }
                };
                let identity = state.owner.as_ref().unwrap().identity();
                if !state.notify.send(Completion::Admitted(Box::new(identity))) {
                    return;
                }
                while let Ok(request) = work.recv() {
                    let result = state.owner.as_mut().unwrap().check(&request);
                    if !state.notify.send(Completion::Checked(result)) {
                        return;
                    }
                }
            })?;
        Ok(Self {
            commands,
            completions,
            wake,
            thread,
        })
    }

    fn completion(&mut self) -> Result<Option<Completion>, ()> {
        let mut bytes = [0; 64];
        loop {
            match self.wake.read(&mut bytes) {
                Ok(0) => break,
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(_) => return Err(()),
            }
        }
        match self.completions.try_recv() {
            Ok(completion) => Ok(Some(completion)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(()),
        }
    }

    fn retire(self) {
        // Drop both directions before inspecting join readiness. A held native
        // call may outlive this reactor; it still owns all inputs until its last
        // use and drop. The process owner retains its existing kill/reap bound.
        let Self {
            commands,
            completions,
            wake,
            thread,
        } = self;
        drop((commands, completions, wake));
        if thread.is_finished() {
            let _ = thread.join();
        }
        // Otherwise detach, never block peer-failure disposal on a model call.
    }
}

pub(super) struct Runtime<A: AdapterAdmission> {
    dispatcher: Dispatcher,
    adapter: Option<A>,
    mode: AdmissionMode,
    worker: Option<Worker>,
}

impl<A> Runtime<A>
where
    A: AdapterAdmission + Send + 'static,
    A::Backend: CheckBackend,
{
    pub(super) fn new(channel: Channel, adapter: A) -> Self {
        Self {
            dispatcher: Dispatcher::new(channel),
            adapter: Some(adapter),
            mode: AdmissionMode::Production,
            worker: None,
        }
    }

    #[cfg(test)]
    pub(super) fn new_fixture(channel: Channel, adapter: A) -> Self {
        Self {
            dispatcher: Dispatcher::new(channel),
            adapter: Some(adapter),
            mode: AdmissionMode::Fixture,
            worker: None,
        }
    }

    pub(super) fn position(&self) -> Position {
        self.dispatcher.position()
    }

    pub(super) fn advance(&mut self) -> Result<RuntimeProgress, Fault> {
        let result = self.advance_inner();
        if result.is_err() || matches!(result, Ok(RuntimeProgress::Stopped(_))) {
            self.retire_worker();
        }
        result
    }

    fn advance_inner(&mut self) -> Result<RuntimeProgress, Fault> {
        // Socket traffic wins over worker completion in the same iteration.
        // Dispatcher::complete repeats this check immediately before queuing a
        // result, so no completion race licenses overlapping asks.
        let action = self.dispatcher.advance()?;
        let mut progress = !matches!(action, Action::Pending);
        match action {
            Action::Bootstrap(bootstrap) => {
                let Some(adapter) = self.adapter.take() else {
                    return self
                        .dispatcher
                        .fail(Fault::Protocol("bootstrap worker already consumed"));
                };
                match Worker::start(bootstrap, adapter, self.mode) {
                    Ok(worker) => self.worker = Some(worker),
                    Err(_) => self
                        .dispatcher
                        .refuse(unavailable("checker worker could not start"))?,
                }
            }
            Action::Check(request) => {
                let accepted = self
                    .worker
                    .as_ref()
                    .is_some_and(|worker| worker.commands.try_send(request).is_ok());
                if !accepted {
                    self.worker_failed()?;
                }
            }
            Action::Stopped(reason) => return Ok(RuntimeProgress::Stopped(reason)),
            Action::Pending | Action::Progress => {}
        }
        if let Some(worker) = self.worker.as_mut() {
            match worker.completion() {
                Ok(Some(Completion::Admitted(identity))) => {
                    self.dispatcher.admit_identity(&identity)?;
                    progress = true;
                }
                Ok(Some(Completion::Refused(refusal))) => {
                    self.dispatcher.refuse(refusal)?;
                    self.retire_worker();
                    progress = true;
                }
                Ok(Some(Completion::Checked(result))) => {
                    self.dispatcher.complete(result)?;
                    if self.position() == Position::Ending {
                        self.retire_worker();
                    }
                    progress = true;
                }
                Ok(None) => {}
                Err(()) => {
                    self.worker_failed()?;
                    progress = true;
                }
            }
        }
        Ok(if progress {
            RuntimeProgress::Progress
        } else {
            RuntimeProgress::Pending
        })
    }

    fn worker_failed(&mut self) -> Result<(), Fault> {
        self.retire_worker();
        match self.position() {
            Position::Admitting => self
                .dispatcher
                .refuse(unavailable("checker admission worker stopped")),
            Position::Checking => self.dispatcher.complete(Ste100Result::ComponentFailed {
                code: Ste100ComponentCode::NoResult,
                detail: "checker worker stopped without a complete result".into(),
            }),
            _ => self.dispatcher.fail(Fault::Protocol(
                "checker worker stopped outside an outstanding operation",
            )),
        }
    }

    /// Wait on only transport and completion readiness. No new operation clock
    /// is introduced; the parent process owner retains the admitted bounds.
    pub(super) fn wait(&mut self) -> Result<(), Fault> {
        let mut pollfds = [
            libc::pollfd {
                fd: self.dispatcher.readiness().as_raw_fd(),
                events: libc::POLLIN
                    | if self.dispatcher.wants_write() {
                        libc::POLLOUT
                    } else {
                        0
                    },
                revents: 0,
            },
            libc::pollfd {
                // poll ignores a negative descriptor when no worker stands.
                fd: self
                    .worker
                    .as_ref()
                    .map_or(-1, |worker| worker.wake.as_raw_fd()),
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        loop {
            // SAFETY: this contiguous pollfd allocation stays live and unchanged
            // throughout poll; every fd remains owned by self until it returns.
            let result =
                unsafe { libc::poll(pollfds.as_mut_ptr(), pollfds.len() as libc::nfds_t, -1) };
            if result >= 0 {
                return Ok(());
            }
            if io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
                self.retire_worker();
                return self
                    .dispatcher
                    .fail(Fault::Protocol("STE100 readiness wait failed"));
            }
        }
    }
}

impl<A: AdapterAdmission> Runtime<A> {
    fn retire_worker(&mut self) {
        if let Some(worker) = self.worker.take() {
            worker.retire();
        }
    }
}

impl<A: AdapterAdmission> Drop for Runtime<A> {
    fn drop(&mut self) {
        self.retire_worker();
    }
}

fn unavailable(detail: &'static str) -> Ste100AdmissionRefused {
    Ste100AdmissionRefused {
        protocol: weaver_types::state_flow::STE100_PROTOCOL.into(),
        code: Ste100AdmissionCode::UnsupportedArtifact,
        detail: detail.into(),
    }
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;
