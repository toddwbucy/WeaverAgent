//! The HARNESS-CLASSIFY near side. This is independent of the optional label
//! scorer and introduces no production classifier transport or scheduling policy.

use std::os::fd::{AsRawFd, BorrowedFd};

use nix::libc;

use weaver_types::state_flow::{
    Binding, DecimalU64, SchemeDeclaration, Ste100ComponentCode, Ste100Reply, Ste100Request,
    Ste100Result, Ste100Target,
};

/// One pump owns the adapter and its cursor. After the initial nonblocking
/// try_result, the pump calls it only when readiness says it can progress.
/// Completed transfers a reply once and frees the adapter's request slot; the
/// coordinator separately holds that reply until its trace authoring completes.
pub(crate) trait Ste100Port {
    fn readiness(&self) -> BorrowedFd<'_>;
    fn submit(&mut self, request: Ste100Request) -> Result<Submit, PortFailure>;
    fn try_result(&mut self) -> Result<ResultPoll, PortFailure>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Submit {
    Submitted,
    Busy(Ste100Request),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ResultPoll {
    Pending,
    Completed(Ste100Reply),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClassifierState {
    Idle,
    InFlight,
    ResultHeld,
    Failed,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClassifierPoll {
    Pending,
    ResultHeld,
}

/// A consumed, recorded input check. Only this classifier owner constructs it,
/// after its exact result has crossed the trace barrier. It is never Clone.
#[derive(Debug)]
pub(super) struct AcceptedReuse {
    run: String,
    offered_sequence: DecimalU64,
    request: Box<Ste100Request>,
    reply: Box<Ste100Reply>,
}

impl AcceptedReuse {
    pub(super) fn run(&self) -> &str {
        &self.run
    }
    pub(super) fn offered_sequence(&self) -> DecimalU64 {
        self.offered_sequence
    }
    pub(super) fn text(&self) -> &str {
        &self.request.text
    }
    pub(super) fn scheme_id(&self) -> &str {
        &self.request.scheme_id
    }
    pub(super) fn bindings(&self) -> &[Binding] {
        match &self.reply.result {
            Ste100Result::Accepted { bindings } => bindings,
            _ => unreachable!("only an accepted recorded check constructs this grant"),
        }
    }
    pub(super) fn request(&self) -> &Ste100Request {
        &self.request
    }
    pub(super) fn reply(&self) -> &Ste100Reply {
        &self.reply
    }
}

/// The classifier state inside FlowCoordinator, not a second transport owner
/// or acknowledgment. Only recorded can release a valid held reply for use.
pub(crate) struct Classifier<P> {
    port: P,
    scheme: SchemeDeclaration,
    run: String,
    state: ClassifierState,
    next_id: Option<u64>,
    outstanding: Option<Ste100Request>,
    held: Option<Ste100Reply>,
    failure: Option<PortFailure>,
}

impl<P: Ste100Port> Classifier<P> {
    pub(crate) fn new(
        port: P,
        scheme: SchemeDeclaration,
        run: String,
    ) -> Result<Self, PortFailure> {
        scheme
            .validate()
            .map_err(|error| PortFailure::protocol(error.to_string()))?;
        Ok(Self::with_scheme(port, scheme, run))
    }

    #[cfg(test)]
    pub(crate) fn fixture(port: P, run: String) -> Self {
        Self::with_scheme(port, scripted::fixture_scheme(), run)
    }

    fn with_scheme(port: P, scheme: SchemeDeclaration, run: String) -> Self {
        Self {
            port,
            scheme,
            run,
            state: ClassifierState::Idle,
            next_id: Some(1),
            outstanding: None,
            held: None,
            failure: None,
        }
    }

    pub(crate) fn readiness(&self) -> BorrowedFd<'_> {
        self.port.readiness()
    }

    pub(crate) fn state(&self) -> ClassifierState {
        self.state
    }

    pub(super) fn scheme(&self) -> &SchemeDeclaration {
        &self.scheme
    }
    pub(super) fn run(&self) -> &str {
        &self.run
    }

    pub(crate) fn outstanding(&self) -> Option<&Ste100Request> {
        self.outstanding.as_ref()
    }

    /// Borrowed only to author ste100.result. Usable completion ownership is
    /// returned exclusively by recorded after the recording proof matches.
    pub(crate) fn held(&self) -> Option<&Ste100Reply> {
        self.held.as_ref()
    }

    pub(crate) fn failure(&self) -> Option<&PortFailure> {
        self.failure.as_ref()
    }

    /// Observe terminal descriptor status without reading another completion
    /// into the full result slot. A readable second reply stays with the port;
    /// a peer close or transport error freezes the exact held account. A valid
    /// ComponentFailed already explains terminal loss: PORT requires the peer
    /// to close after sending it, and its exact reply must be recorded first.
    pub(crate) fn check_held_terminal(&mut self) -> Result<(), PortFailure> {
        self.ensure_active()?;
        if self.state != ClassifierState::ResultHeld {
            return Ok(());
        }
        if matches!(
            self.held.as_ref().map(|reply| &reply.result),
            Some(Ste100Result::ComponentFailed { .. })
        ) {
            return Ok(());
        }
        let fd = self.port.readiness();
        let mut status = libc::pollfd {
            fd: fd.as_raw_fd(),
            // POLLRDHUP also observes EOF from a peer's write-half shutdown.
            // nix's PollFlags does not expose this Linux flag.
            events: libc::POLLRDHUP,
            revents: 0,
        };
        // SAFETY: status is one writable pollfd and fd stays borrowed for the
        // immediate call. A zero timeout never introduces a second wait clock.
        if unsafe { libc::poll(&mut status, 1, 0) } < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::Interrupted {
                // An interrupted probe observed no terminal status. The common
                // pump may observe it next unit; this probe owns no retry wait.
                return Ok(());
            }
            return self.stop(PortFailure::component(
                Ste100ComponentCode::TransportFailed,
                error.to_string(),
            ));
        }
        if status.revents & (libc::POLLERR | libc::POLLNVAL) != 0 {
            return self.stop(PortFailure::component(
                Ste100ComponentCode::TransportFailed,
                "classifier readiness descriptor failed while a result was held",
            ));
        }
        if status.revents & (libc::POLLHUP | libc::POLLRDHUP) != 0 {
            return self.stop(PortFailure::component(
                Ste100ComponentCode::Crashed,
                "classifier peer closed while a result was held",
            ));
        }
        Ok(())
    }

    pub(crate) fn next_request_id(&mut self) -> Result<DecimalU64, PortFailure> {
        self.ensure_active()?;
        match self.next_id {
            Some(next) => Ok(DecimalU64(next)),
            None => self.stop(PortFailure::protocol(
                "classifier request counter exhausted",
            )),
        }
    }

    pub(crate) fn submit(&mut self, request: Ste100Request) -> Result<Submit, PortFailure> {
        self.ensure_active()?;
        if matches!(
            self.state,
            ClassifierState::InFlight | ClassifierState::ResultHeld
        ) {
            return Ok(Submit::Busy(request));
        }
        let expected = self.next_request_id()?;
        if request.request_id != expected
            || request.scheme_id != self.scheme.scheme_id
            || matches!(&request.target, Ste100Target::Source(source) if source.run != self.run)
            || request.validate().is_err()
        {
            return self.stop(PortFailure::protocol(
                "invalid classifier submission identity",
            ));
        }
        match self.port.submit(request.clone()) {
            Ok(Submit::Submitted) => {
                self.next_id = expected.0.checked_add(1);
                self.outstanding = Some(request);
                self.state = ClassifierState::InFlight;
                Ok(Submit::Submitted)
            }
            Ok(Submit::Busy(returned)) if returned == request => Ok(Submit::Busy(returned)),
            Ok(Submit::Busy(_)) => {
                self.stop(PortFailure::protocol("adapter changed a busy request"))
            }
            Err(failure) => self.stop(failure),
        }
    }

    pub(crate) fn try_result(&mut self) -> Result<ClassifierPoll, PortFailure> {
        self.ensure_active()?;
        match self.port.try_result() {
            Ok(ResultPoll::Pending) => Ok(if self.held.is_some() {
                ClassifierPoll::ResultHeld
            } else {
                ClassifierPoll::Pending
            }),
            Ok(ResultPoll::Completed(reply)) => {
                if self.state != ClassifierState::InFlight {
                    let mut failure = PortFailure::protocol("unsolicited classifier reply");
                    failure.received = Some(Box::new(reply));
                    return self.stop(failure);
                }
                let request = self
                    .outstanding
                    .as_ref()
                    .expect("in-flight request is owned");
                if let Err(failure) = validate_reply(&reply, request, &self.scheme) {
                    return self.stop(failure);
                }
                self.held = Some(reply);
                self.state = ClassifierState::ResultHeld;
                Ok(ClassifierPoll::ResultHeld)
            }
            Err(failure) => self.stop(failure),
        }
    }

    /// Check terminal descriptor status without receiving a second reply or
    /// acknowledging the wire. The receipt binds both the complete reply and
    /// this run, so another run's trace admission cannot release input/template
    /// IDs, and a dead peer cannot release a previously accepted result.
    pub(crate) fn recorded(
        &mut self,
        receipt: super::RecordedSte100,
    ) -> Result<Ste100Reply, PortFailure> {
        self.check_held_terminal()?;
        let Some(reply) = self.held.as_ref() else {
            return self.stop(PortFailure::protocol("no classifier result is held"));
        };
        if receipt.run() != self.run || !receipt.matches(reply) {
            return self.stop(PortFailure::protocol(
                "classifier recording proof does not match",
            ));
        }
        let reply = self.held.take().expect("the matched reply remains held");
        if let Ste100Result::ComponentFailed { code, detail } = &reply.result {
            if self.failure.is_none() {
                self.failure = Some(PortFailure::component(*code, detail.clone()));
            }
            self.state = ClassifierState::Failed;
        } else if self.state != ClassifierState::Failed {
            self.outstanding = None;
            self.state = ClassifierState::Idle;
        }
        Ok(reply)
    }

    /// Release an accepted gate check into the single-use input filing grant.
    /// Rejections use recorded() and remain ordinary conformance outcomes. This
    /// method refuses an attempted grant for any other target or disposition.
    pub(super) fn recorded_input(
        &mut self,
        receipt: super::RecordedSte100,
        offered_sequence: DecimalU64,
    ) -> Result<AcceptedReuse, PortFailure> {
        self.ensure_active()?;
        let Some(request) = self.outstanding.as_ref() else {
            return self.stop(PortFailure::protocol(
                "input grant has no outstanding check",
            ));
        };
        if offered_sequence.0 == 0
            || !matches!(request.target, Ste100Target::Input { input_id, block }
                if input_id.0 != 0 && block.0 == 0)
            || !matches!(
                self.held.as_ref().map(|reply| &reply.result),
                Some(Ste100Result::Accepted { .. })
            )
        {
            return self.stop(PortFailure::protocol(
                "input grant requires an accepted block-zero input check",
            ));
        }
        let request = request.clone();
        let reply = self.recorded(receipt)?;
        Ok(AcceptedReuse {
            run: self.run.clone(),
            offered_sequence,
            request: Box::new(request),
            reply: Box::new(reply),
        })
    }

    /// A trace failure or applicable bound is owned by the coordinator's halt
    /// latch. Freeze this slot without inventing a classifier component code.
    pub(crate) fn fail(&mut self) {
        if self.state != ClassifierState::Closed {
            self.state = ClassifierState::Failed;
        }
    }

    /// Teardown retains the outstanding request/reply for the halt account and
    /// cannot reopen this owner. Descriptor cleanup belongs to its owner.
    pub(crate) fn close(&mut self) {
        self.state = ClassifierState::Closed;
    }

    fn ensure_active(&self) -> Result<(), PortFailure> {
        match self.state {
            ClassifierState::Failed | ClassifierState::Closed => Err(self
                .failure
                .clone()
                .unwrap_or_else(|| PortFailure::protocol("classifier is failed or closed"))),
            _ => Ok(()),
        }
    }

    fn stop<T>(&mut self, failure: PortFailure) -> Result<T, PortFailure> {
        if self.failure.is_none() {
            self.failure = Some(failure.clone());
        }
        self.state = ClassifierState::Failed;
        Err(failure)
    }
}

impl Classifier<super::ste100_socket::SocketPort> {
    /// The common pump adds POLLOUT only while this concrete transport has
    /// unsent request fragments. A held result or failed owner grants no send.
    pub(super) fn wants_write(&self) -> bool {
        self.state == ClassifierState::InFlight && self.port.wants_write()
    }

    /// A consumed zero-length packet can leave a terminal-status probe pending
    /// after EINTR, without any remaining POLLIN wake. The pump checks this
    /// buffered work before it waits, while preserving the held-result barrier.
    pub(super) fn needs_progress(&self) -> bool {
        matches!(
            self.state,
            ClassifierState::Idle | ClassifierState::InFlight
        ) && self.port.needs_progress()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PortFailureKind {
    Component(Ste100ComponentCode),
    ProtocolFailed,
}

/// Local failure evidence, never a synthesized Ste100Reply. A received reply is
/// preserved when its identity or binding structure is the failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PortFailure {
    pub(crate) kind: PortFailureKind,
    pub(crate) detail: String,
    pub(crate) received: Option<Box<Ste100Reply>>,
}

impl PortFailure {
    pub(crate) fn component(code: Ste100ComponentCode, detail: impl Into<String>) -> Self {
        Self {
            kind: PortFailureKind::Component(code),
            detail: detail.into(),
            received: None,
        }
    }

    pub(crate) fn protocol(detail: impl Into<String>) -> Self {
        Self {
            kind: PortFailureKind::ProtocolFailed,
            detail: detail.into(),
            received: None,
        }
    }
}

/// Judge the entire result against the exact submitted bytes and the loaded
/// dictionary. The shared validator owns representation, not language truth.
pub(crate) fn validate_reply(
    reply: &Ste100Reply,
    request: &Ste100Request,
    scheme: &SchemeDeclaration,
) -> Result<(), PortFailure> {
    let error = if request.scheme_id != scheme.scheme_id {
        Some("classifier request differs from the loaded scheme".to_string())
    } else {
        reply
            .validate_for(request, &scheme.verbs)
            .err()
            .map(|error| error.to_string())
    };
    match error {
        Some(detail) => Err(PortFailure {
            kind: PortFailureKind::Component(Ste100ComponentCode::InvalidResult),
            detail,
            received: Some(Box::new(reply.clone())),
        }),
        None => Ok(()),
    }
}

/// A nonblocking fixture transport, deliberately confined to tests. Its LF
/// frame delimiter belongs only to this double; production uses Spec 02's
/// bounded SEQPACKET transport, preserving Ste100Port's near side.
#[cfg(test)]
pub(crate) mod scripted {
    use super::*;
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::io::{self, Read, Write};
    use std::os::fd::AsFd;
    use std::os::unix::net::UnixStream;
    use std::rc::Rc;
    use weaver_types::state_flow::{DecimalU64, Digest, decode_strict};

    pub(crate) const FIXTURE_ARTIFACT: &[u8] = b"fixture-read/1\nread\n";
    pub(crate) const FIXTURE_SHA256: &str =
        "b2f88f08ede1b76e5156f8d841b1c21de5c0afea2ab111cd51d56a0f71e0e03a";

    pub(crate) fn fixture_scheme() -> SchemeDeclaration {
        SchemeDeclaration {
            scheme_id: "fixture-read/1".into(),
            artifact_sha256: Digest::new(FIXTURE_SHA256).unwrap(),
            verbs: vec!["read".into()],
        }
    }

    #[derive(Debug)]
    pub(crate) enum ScriptStep {
        Hold,
        Reply(Ste100Reply),
        Bytes(Vec<u8>),
        Close,
        ShutdownWrite,
    }

    pub(crate) struct ScriptedPort {
        socket: UnixStream,
        scheme_id: String,
        next_id: Option<u64>,
        outstanding: Option<Ste100Request>,
        incoming: Vec<u8>,
        failure: Option<PortFailure>,
        submitted: Rc<RefCell<Vec<Ste100Request>>>,
    }

    pub(crate) struct ScriptedPeer {
        socket: Option<UnixStream>,
        steps: VecDeque<ScriptStep>,
        writing: Option<(Vec<u8>, usize)>,
        submitted: Rc<RefCell<Vec<Ste100Request>>>,
    }

    impl ScriptedPort {
        pub(crate) fn fixture(
            steps: impl IntoIterator<Item = ScriptStep>,
        ) -> io::Result<(Self, ScriptedPeer)> {
            Self::pair("fixture-read/1", steps)
        }

        pub(crate) fn checked(
            scheme: &SchemeDeclaration,
            steps: impl IntoIterator<Item = ScriptStep>,
        ) -> io::Result<(Self, ScriptedPeer)> {
            scheme.validate().map_err(io::Error::other)?;
            Self::pair(&scheme.scheme_id, steps)
        }

        fn pair(
            scheme_id: &str,
            steps: impl IntoIterator<Item = ScriptStep>,
        ) -> io::Result<(Self, ScriptedPeer)> {
            let (socket, peer) = UnixStream::pair()?;
            socket.set_nonblocking(true)?;
            peer.set_nonblocking(true)?;
            let submitted = Rc::new(RefCell::new(Vec::new()));
            Ok((
                Self {
                    socket,
                    scheme_id: scheme_id.into(),
                    next_id: Some(1),
                    outstanding: None,
                    incoming: Vec::new(),
                    failure: None,
                    submitted: Rc::clone(&submitted),
                },
                ScriptedPeer {
                    socket: Some(peer),
                    steps: steps.into_iter().collect(),
                    writing: None,
                    submitted,
                },
            ))
        }

        fn fail<T>(&mut self, failure: PortFailure) -> Result<T, PortFailure> {
            self.failure = Some(failure.clone());
            Err(failure)
        }
    }

    impl Ste100Port for ScriptedPort {
        fn readiness(&self) -> BorrowedFd<'_> {
            self.socket.as_fd()
        }

        fn submit(&mut self, request: Ste100Request) -> Result<Submit, PortFailure> {
            if let Some(failure) = &self.failure {
                return Err(failure.clone());
            }
            if self.outstanding.is_some() {
                return Ok(Submit::Busy(request));
            }
            let Some(next) = self.next_id else {
                return self.fail(PortFailure::protocol(
                    "classifier request counter exhausted",
                ));
            };
            if request.request_id != DecimalU64(next)
                || request.scheme_id != self.scheme_id
                || request.validate().is_err()
            {
                return self.fail(PortFailure::protocol(
                    "invalid classifier submission identity",
                ));
            }
            self.next_id = next.checked_add(1);
            self.submitted.borrow_mut().push(request.clone());
            self.outstanding = Some(request);
            Ok(Submit::Submitted)
        }

        fn try_result(&mut self) -> Result<ResultPoll, PortFailure> {
            if let Some(failure) = &self.failure {
                return Err(failure.clone());
            }
            loop {
                if let Some(end) = self.incoming.iter().position(|byte| *byte == b'\n') {
                    let frame: Vec<u8> = self.incoming.drain(..=end).collect();
                    let reply = std::str::from_utf8(&frame[..frame.len() - 1])
                        .ok()
                        .and_then(|text| decode_strict::<Ste100Reply>(text).ok());
                    let Some(reply) = reply else {
                        return self.fail(PortFailure::component(
                            Ste100ComponentCode::InvalidResult,
                            "malformed scripted classifier reply",
                        ));
                    };
                    self.outstanding = None;
                    return Ok(ResultPoll::Completed(reply));
                }
                // Do not consume bytes belonging to a second logical result.
                // They must keep the real readiness descriptor readable while
                // the coordinator holds this completion for trace admission.
                let mut bytes = [0; 1];
                match self.socket.read(&mut bytes) {
                    Ok(0) => {
                        return self.fail(PortFailure::component(
                            Ste100ComponentCode::Crashed,
                            "classifier peer closed",
                        ));
                    }
                    Ok(count) => self.incoming.extend_from_slice(&bytes[..count]),
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        return Ok(ResultPoll::Pending);
                    }
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(error) => {
                        return self.fail(PortFailure::component(
                            Ste100ComponentCode::TransportFailed,
                            error.to_string(),
                        ));
                    }
                }
            }
        }
    }

    impl ScriptedPeer {
        pub(crate) fn submitted(&self) -> Vec<Ste100Request> {
            self.submitted.borrow().clone()
        }

        pub(crate) fn push(&mut self, step: ScriptStep) {
            self.steps.push_back(step);
        }

        /// Advance one script step. A partial write remains owned here; false
        /// means it needs another writable opportunity, never an invented reply.
        pub(crate) fn advance(&mut self) -> io::Result<bool> {
            if self.writing.is_none() {
                match self.steps.pop_front() {
                    None | Some(ScriptStep::Hold) => return Ok(true),
                    Some(ScriptStep::Close) => {
                        self.socket.take();
                        return Ok(true);
                    }
                    Some(ScriptStep::ShutdownWrite) => {
                        self.socket
                            .as_ref()
                            .ok_or_else(|| io::Error::other("script peer closed"))?
                            .shutdown(std::net::Shutdown::Write)?;
                        return Ok(true);
                    }
                    Some(ScriptStep::Reply(reply)) => {
                        let mut bytes = serde_json::to_vec(&reply).map_err(io::Error::other)?;
                        bytes.push(b'\n');
                        self.writing = Some((bytes, 0));
                    }
                    Some(ScriptStep::Bytes(bytes)) => self.writing = Some((bytes, 0)),
                }
            }
            let socket = self
                .socket
                .as_mut()
                .ok_or_else(|| io::Error::other("script peer closed"))?;
            let (bytes, cursor) = self.writing.as_mut().expect("script has a write");
            while *cursor < bytes.len() {
                match socket.write(&bytes[*cursor..]) {
                    Ok(0) => return Err(io::Error::new(io::ErrorKind::WriteZero, "script write")),
                    Ok(count) => *cursor += count,
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(false),
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(error) => return Err(error),
                }
            }
            self.writing = None;
            Ok(true)
        }
    }
}

#[cfg(test)]
#[path = "classifier_tests.rs"]
mod tests;
