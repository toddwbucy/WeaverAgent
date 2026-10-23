//! Nonblocking STE100 duty protocol, per Spec 02 CHANNEL and PORT.
//!
//! The SPU driver keeps advancing this machine while its checker works. A
//! `Check` action transfers work once, not ownership of the socket. Blocking
//! on that work instead of pumping this machine would hide overlapping asks.
//! This module owns no scheduling policy, timer, grammar or model adapter.

use std::os::fd::BorrowedFd;

use weaver_types::state_flow::{
    DecimalU64, Ste100AdmissionRefused, Ste100Bootstrap, Ste100ComponentCode, Ste100Reply,
    Ste100Request, Ste100Result, decode_strict,
};

use super::admission::{AdmissionIdentity, Admitted, ValidatedBootstrap, validate_bootstrap};
use super::channel::{Channel, Fault as ChannelFault, Message, Receive, SendProgress, Tag};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Position {
    Starting,
    Admitting,
    ReadyPending,
    Idle,
    Checking,
    ReplyPending,
    Ending,
    Closed,
    Failed,
}

#[cfg(test)]
mod tests {
    use std::os::fd::AsRawFd;

    use super::super::admission::fixture_admitted;
    use super::*;
    use nix::sys::socket::{
        AddressFamily, MsgFlags, SockFlag, SockType, send as send_packet, socketpair,
    };
    use weaver_types::state_flow::{
        Binding, NounBinding, STE100_PROTOCOL, Span, Ste100AdmissionCode, Ste100DefectCode,
        Ste100Target,
    };

    fn pair(limit: usize) -> (Channel, Dispatcher) {
        let (left, right) = socketpair(
            AddressFamily::Unix,
            SockType::SeqPacket,
            None,
            SockFlag::SOCK_CLOEXEC,
        )
        .unwrap();
        (
            Channel::from_owned(left, limit).unwrap(),
            Dispatcher::new(Channel::from_owned(right, limit).unwrap()),
        )
    }

    fn send(peer: &mut Channel, tag: Tag, id: u64, bytes: Vec<u8>) {
        peer.begin_send(tag, id, bytes).unwrap();
        while peer.has_pending_send() {
            assert_ne!(peer.try_send().unwrap(), SendProgress::Pending);
        }
    }

    fn receive(peer: &mut Channel) -> Message {
        loop {
            match peer.try_receive().unwrap() {
                Receive::Complete(message) => return message,
                Receive::Fragment => {}
                Receive::Pending => panic!("complete answer must be available"),
            }
        }
    }

    fn first_request_fragment(peer: &Channel, request: &Ste100Request) {
        let bytes = serde_json::to_vec(request).unwrap();
        let mut datagram = vec![1, Tag::Request as u8];
        datagram.extend_from_slice(&request.request_id.0.to_be_bytes());
        datagram.extend_from_slice(&(bytes.len() as u64).to_be_bytes());
        datagram.extend_from_slice(&0_u64.to_be_bytes());
        // One valid, incomplete fragment. No later fragment is needed to expose overlap.
        datagram.push(bytes[0]);
        assert_eq!(
            send_packet(
                peer.readiness().as_raw_fd(),
                &datagram,
                MsgFlags::MSG_NOSIGNAL
            )
            .unwrap(),
            datagram.len()
        );
    }

    fn ready() -> (Channel, Dispatcher) {
        let (bootstrap, admitted) = fixture_admitted();
        let (mut peer, mut duty) = pair(bootstrap.instruction.max_wire_bytes.0 as usize);
        send(
            &mut peer,
            Tag::Bootstrap,
            0,
            serde_json::to_vec(&bootstrap).unwrap(),
        );
        assert!(
            matches!(duty.advance().unwrap(), Action::Bootstrap(checked) if checked.bootstrap() == &bootstrap)
        );
        assert_eq!(duty.position(), Position::Admitting);
        assert_eq!(duty.advance().unwrap(), Action::Pending);
        duty.admit(&admitted).unwrap();
        assert!(duty.wants_write());
        assert_eq!(duty.advance().unwrap(), Action::Progress);
        assert_eq!(duty.position(), Position::Idle);
        let message = receive(&mut peer);
        assert_eq!(message.tag, Tag::Ready);
        assert_eq!(message.id, 0);
        assert_eq!(
            message.payload,
            serde_json::to_vec(admitted.ready()).unwrap()
        );
        (peer, duty)
    }

    fn request(duty: &Dispatcher, id: u64) -> Ste100Request {
        Ste100Request {
            request_id: DecimalU64(id),
            target: Ste100Target::Input {
                input_id: DecimalU64(id),
                block: DecimalU64(0),
            },
            text: "Read file.".into(),
            scheme_id: duty.scheme_id.clone(),
        }
    }

    fn submit(peer: &mut Channel, duty: &mut Dispatcher, request: &Ste100Request) {
        send(
            peer,
            Tag::Request,
            request.request_id.0,
            serde_json::to_vec(request).unwrap(),
        );
        assert_eq!(duty.advance().unwrap(), Action::Check(request.clone()));
        assert_eq!(duty.position(), Position::Checking);
        assert_eq!(duty.advance().unwrap(), Action::Pending);
        assert_eq!(peer.try_receive().unwrap(), Receive::Pending);
    }

    fn span(start: u64, end: u64) -> Span {
        Span {
            start: DecimalU64(start),
            end: DecimalU64(end),
        }
    }

    fn accepted() -> Ste100Result {
        Ste100Result::Accepted {
            bindings: vec![Binding {
                statement_span: span(0, 10),
                verb: "read".into(),
                verb_span: span(0, 4),
                condition: None,
                condition_span: None,
                nouns: vec![NounBinding {
                    value: "file".into(),
                    span: span(5, 9),
                }],
            }],
        }
    }

    fn rejected() -> Ste100Result {
        Ste100Result::NotSte100 {
            code: Ste100DefectCode::NonSte100,
            detail: "fixture rule refuses the whole block".into(),
            spans: vec![span(5, 9)],
        }
    }

    #[test]
    fn accepted_and_rejected_blocks_each_answer_once_and_keep_the_duty_usable() {
        let (mut peer, mut duty) = ready();
        for (id, result) in [(1, accepted()), (2, rejected()), (3, accepted())] {
            let request = request(&duty, id);
            submit(&mut peer, &mut duty, &request);
            duty.complete(result.clone()).unwrap();
            assert_eq!(duty.position(), Position::ReplyPending);
            assert_eq!(peer.try_receive().unwrap(), Receive::Pending);
            assert_eq!(duty.advance().unwrap(), Action::Progress);
            assert_eq!(duty.position(), Position::Idle);
            let message = receive(&mut peer);
            assert_eq!(message.tag, Tag::Reply);
            assert_eq!(message.id, id);
            let reply: Ste100Reply =
                decode_strict(std::str::from_utf8(&message.payload).unwrap()).unwrap();
            assert_eq!(reply.result, result);
            reply.validate_for(&request, &duty.verbs).unwrap();
            assert_eq!(duty.advance().unwrap(), Action::Pending);
            assert_eq!(peer.try_receive().unwrap(), Receive::Pending);
        }
    }

    #[test]
    fn a_held_result_exposes_neither_a_prefix_nor_a_second_check() {
        let (mut peer, mut duty) = ready();
        let first = request(&duty, 1);
        submit(&mut peer, &mut duty, &first);
        for _ in 0..4 {
            assert_eq!(duty.advance().unwrap(), Action::Pending);
            assert_eq!(peer.try_receive().unwrap(), Receive::Pending);
        }
        let second = request(&duty, 2);
        send(
            &mut peer,
            Tag::Request,
            2,
            serde_json::to_vec(&second).unwrap(),
        );
        assert!(matches!(duty.advance(), Err(Fault::Protocol(_))));
        assert_eq!(duty.position(), Position::Failed);
        assert!(matches!(peer.try_receive(), Err(ChannelFault::Closed)));
        assert!(duty.complete(accepted()).is_err());
    }

    #[test]
    fn completion_cannot_hide_a_request_that_arrived_while_the_worker_was_held() {
        let (mut peer, mut duty) = ready();
        let first = request(&duty, 1);
        submit(&mut peer, &mut duty, &first);
        send(
            &mut peer,
            Tag::Request,
            2,
            serde_json::to_vec(&request(&duty, 2)).unwrap(),
        );
        assert!(matches!(duty.complete(accepted()), Err(Fault::Protocol(_))));
        assert!(matches!(peer.try_receive(), Err(ChannelFault::Closed)));
    }

    #[test]
    fn component_failure_and_invalid_backend_result_attempt_terminal_answer_then_close() {
        for result in [
            Ste100Result::ComponentFailed {
                code: Ste100ComponentCode::NoResult,
                detail: "artifact_capacity".into(),
            },
            Ste100Result::Accepted { bindings: vec![] },
            Ste100Result::NotSte100 {
                code: Ste100DefectCode::NonSte100,
                detail: String::new(),
                spans: vec![],
            },
        ] {
            let (mut peer, mut duty) = ready();
            let first = request(&duty, 1);
            submit(&mut peer, &mut duty, &first);
            duty.complete(result).unwrap();
            assert_eq!(duty.position(), Position::Ending);
            assert_eq!(
                duty.advance().unwrap(),
                Action::Stopped(Stop::ComponentFailed)
            );
            let message = receive(&mut peer);
            let reply: Ste100Reply =
                decode_strict(std::str::from_utf8(&message.payload).unwrap()).unwrap();
            assert!(matches!(reply.result, Ste100Result::ComponentFailed { .. }));
            reply.validate_for(&first, &duty.verbs).unwrap();
            assert!(matches!(peer.try_receive(), Err(ChannelFault::Closed)));
        }
    }

    #[test]
    fn bootstrap_identity_must_match_the_checked_admission() {
        let (mut bootstrap, admitted) = fixture_admitted();
        bootstrap.instruction.instances += 1;
        let (mut peer, mut duty) = pair(bootstrap.instruction.max_wire_bytes.0 as usize);
        send(
            &mut peer,
            Tag::Bootstrap,
            0,
            serde_json::to_vec(&bootstrap).unwrap(),
        );
        assert!(matches!(duty.advance().unwrap(), Action::Bootstrap(_)));
        assert!(matches!(duty.admit(&admitted), Err(Fault::Protocol(_))));
        assert!(matches!(peer.try_receive(), Err(ChannelFault::Closed)));
    }

    #[test]
    fn non_echo_bootstrap_fields_cannot_borrow_another_admission_proof() {
        for field in ["devices", "measurement_ref"] {
            let (mut bootstrap, admitted) = fixture_admitted();
            match field {
                "devices" => bootstrap.instruction.model_binding.devices[0].0 += 1,
                "measurement_ref" => bootstrap.instruction.measurement_ref.push_str("-changed"),
                _ => unreachable!(),
            }
            // Every Ready echo still matches. Checking only those fields misses this case.
            assert_eq!(bootstrap.scheme_text, admitted.bootstrap().scheme_text);
            assert_eq!(
                bootstrap.instruction.max_wire_bytes,
                admitted.ready().max_wire_bytes
            );
            assert_eq!(bootstrap.instruction.instances, admitted.ready().instances);
            assert_eq!(
                bootstrap.instruction.model_artifact_sha256,
                admitted.ready().checker_artifact_sha256
            );
            bootstrap
                .validate(bootstrap.instruction.max_wire_bytes)
                .unwrap();
            let (mut peer, mut duty) = pair(bootstrap.instruction.max_wire_bytes.0 as usize);
            send(
                &mut peer,
                Tag::Bootstrap,
                0,
                serde_json::to_vec(&bootstrap).unwrap(),
            );
            assert!(
                matches!(duty.advance().unwrap(), Action::Bootstrap(checked) if checked.bootstrap() == &bootstrap)
            );
            assert!(
                matches!(duty.admit(&admitted), Err(Fault::Protocol(_))),
                "{field}"
            );
            assert_eq!(duty.position(), Position::Failed);
            assert!(!duty.wants_write());
            assert!(matches!(peer.try_receive(), Err(ChannelFault::Closed)));
        }
    }

    #[test]
    fn invalid_bootstrap_refuses_admission_and_never_becomes_idle() {
        for bytes in [
            b"{}".to_vec(),
            b"{\"protocol\":1,\"protocol\":1}".to_vec(),
            vec![0xff],
        ] {
            let (mut peer, mut duty) = pair(4096);
            send(&mut peer, Tag::Bootstrap, 0, bytes);
            assert_eq!(duty.advance().unwrap(), Action::Progress);
            assert_eq!(duty.position(), Position::Ending);
            assert_eq!(
                duty.advance().unwrap(),
                Action::Stopped(Stop::AdmissionRefused)
            );
            let message = receive(&mut peer);
            assert_eq!(message.tag, Tag::AdmissionRefused);
            let refusal: Ste100AdmissionRefused =
                decode_strict(std::str::from_utf8(&message.payload).unwrap()).unwrap();
            refusal.validate().unwrap();
            assert!(matches!(peer.try_receive(), Err(ChannelFault::Closed)));
        }
    }

    #[test]
    fn unsupported_backend_refusal_is_one_answer_and_exit() {
        let (bootstrap, _) = fixture_admitted();
        let (mut peer, mut duty) = pair(bootstrap.instruction.max_wire_bytes.0 as usize);
        send(
            &mut peer,
            Tag::Bootstrap,
            0,
            serde_json::to_vec(&bootstrap).unwrap(),
        );
        assert!(matches!(duty.advance().unwrap(), Action::Bootstrap(_)));
        duty.refuse(Ste100AdmissionRefused {
            protocol: STE100_PROTOCOL.into(),
            code: Ste100AdmissionCode::UnsupportedArtifact,
            detail: "no compiled adapter".into(),
        })
        .unwrap();
        assert_eq!(
            duty.advance().unwrap(),
            Action::Stopped(Stop::AdmissionRefused)
        );
        assert_eq!(receive(&mut peer).tag, Tag::AdmissionRefused);
        assert!(matches!(peer.try_receive(), Err(ChannelFault::Closed)));
    }

    #[test]
    fn request_json_id_scheme_order_and_utf8_are_checked_before_dispatch() {
        for case in 0..7 {
            let (mut peer, mut duty) = ready();
            let mut ask = request(&duty, 1);
            let mut header_id = 1;
            match case {
                0 => ask.scheme_id.push('x'),
                1 => ask.request_id = DecimalU64(2),
                2 => header_id = 2,
                _ => {}
            }
            let mut bytes = serde_json::to_vec(&ask).unwrap();
            match case {
                3 => bytes = b"{\"request_id\":\"1\",\"request_id\":\"1\"}".to_vec(),
                4 => bytes = b"{\"Scored\":[]}".to_vec(),
                5 => bytes = vec![0xff],
                6 => bytes = b"{\"request_id\":1}".to_vec(),
                _ => {}
            }
            send(&mut peer, Tag::Request, header_id, bytes);
            assert!(
                matches!(duty.advance(), Err(Fault::Protocol(_))),
                "case {case}"
            );
            assert_eq!(duty.position(), Position::Failed);
            assert!(matches!(peer.try_receive(), Err(ChannelFault::Closed)));
        }
    }

    #[test]
    fn next_request_waits_for_the_entire_prior_answer() {
        let (mut peer, mut duty) = ready();
        let first = request(&duty, 1);
        submit(&mut peer, &mut duty, &first);
        duty.complete(accepted()).unwrap();
        send(
            &mut peer,
            Tag::Request,
            2,
            serde_json::to_vec(&request(&duty, 2)).unwrap(),
        );
        assert!(matches!(duty.advance(), Err(Fault::Protocol(_))));
        assert!(matches!(peer.try_receive(), Err(ChannelFault::Closed)));
    }

    #[test]
    fn a_repeated_request_id_after_a_completed_answer_is_not_new_work() {
        for result in [accepted(), rejected()] {
            let (mut peer, mut duty) = ready();
            let first = request(&duty, 1);
            submit(&mut peer, &mut duty, &first);
            duty.complete(result).unwrap();
            assert_eq!(duty.advance().unwrap(), Action::Progress);
            assert_eq!(duty.position(), Position::Idle);
            assert_eq!(receive(&mut peer).id, 1);
            send(
                &mut peer,
                Tag::Request,
                1,
                serde_json::to_vec(&first).unwrap(),
            );
            assert!(matches!(duty.advance(), Err(Fault::Protocol(_))));
            assert_eq!(duty.position(), Position::Failed);
            assert!(matches!(peer.try_receive(), Err(ChannelFault::Closed)));
        }
    }

    #[test]
    fn the_first_overlapping_fragment_fails_without_waiting_for_its_remainder() {
        for complete_at_boundary in [false, true] {
            let (mut peer, mut duty) = ready();
            let first = request(&duty, 1);
            submit(&mut peer, &mut duty, &first);
            first_request_fragment(&peer, &request(&duty, 2));
            if complete_at_boundary {
                assert!(matches!(duty.complete(accepted()), Err(Fault::Protocol(_))));
            } else {
                assert!(matches!(duty.advance(), Err(Fault::Protocol(_))));
            }
            assert_eq!(duty.position(), Position::Failed);
            assert!(matches!(peer.try_receive(), Err(ChannelFault::Closed)));
        }
    }

    #[test]
    fn no_request_is_licensed_before_the_readiness_answer() {
        let (bootstrap, admitted) = fixture_admitted();
        for phase in 0..3 {
            let (mut peer, mut duty) = pair(bootstrap.instruction.max_wire_bytes.0 as usize);
            if phase != 0 {
                send(
                    &mut peer,
                    Tag::Bootstrap,
                    0,
                    serde_json::to_vec(&bootstrap).unwrap(),
                );
                assert!(matches!(duty.advance().unwrap(), Action::Bootstrap(_)));
            }
            if phase == 2 {
                duty.admit(&admitted).unwrap();
            }
            send(&mut peer, Tag::Request, 1, b"{}".to_vec());
            assert!(matches!(duty.advance(), Err(Fault::Protocol(_))));
            assert!(matches!(peer.try_receive(), Err(ChannelFault::Closed)));
        }
    }

    #[test]
    fn idle_eof_is_teardown_but_checking_eof_is_component_loss() {
        let (peer, mut duty) = ready();
        drop(peer);
        assert_eq!(duty.advance().unwrap(), Action::Stopped(Stop::PeerClosed));
        let (mut peer, mut duty) = ready();
        let first = request(&duty, 1);
        submit(&mut peer, &mut duty, &first);
        drop(peer);
        assert!(matches!(
            duty.advance(),
            Err(Fault::Transport(ChannelFault::Closed))
        ));
        assert_eq!(duty.position(), Position::Failed);
    }

    #[test]
    fn eof_after_a_partial_request_is_failure_even_before_a_check_is_dispatched() {
        let (peer, mut duty) = ready();
        first_request_fragment(&peer, &request(&duty, 1));
        assert_eq!(duty.advance().unwrap(), Action::Progress);
        assert_eq!(duty.position(), Position::Idle);
        assert!(duty.outstanding.is_none());
        drop(peer);
        assert!(matches!(
            duty.advance(),
            Err(Fault::Transport(ChannelFault::Closed))
        ));
        assert_eq!(duty.position(), Position::Failed);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    PeerClosed,
    AdmissionRefused,
    ComponentFailed,
}

#[derive(Debug, PartialEq)]
pub enum Action {
    Pending,
    Progress,
    Bootstrap(ValidatedBootstrap),
    Check(Ste100Request),
    Stopped(Stop),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fault {
    Transport(ChannelFault),
    Protocol(&'static str),
    Failed,
}

impl std::fmt::Display for Fault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "STE100 duty: {self:?}")
    }
}

impl std::error::Error for Fault {}

/// One driver owns this machine and its receive cursor. The eventual process
/// entry must terminate after a failure or terminal stop. A checker result is
/// whole-block only, and a correctly rejected block leaves the duty usable.
pub struct Dispatcher {
    channel: Channel,
    position: Position,
    bootstrap: Option<Ste100Bootstrap>,
    scheme_id: String,
    verbs: Vec<String>,
    next_id: Option<u64>,
    outstanding: Option<Ste100Request>,
    stop: Option<Stop>,
}

impl Dispatcher {
    pub fn new(channel: Channel) -> Self {
        Self {
            channel,
            position: Position::Starting,
            bootstrap: None,
            scheme_id: String::new(),
            verbs: Vec::new(),
            next_id: Some(1),
            outstanding: None,
            stop: None,
        }
    }

    pub fn position(&self) -> Position {
        self.position
    }

    pub fn readiness(&self) -> BorrowedFd<'_> {
        self.channel.readiness()
    }

    /// Include writable readiness only while this returns true. Readable and
    /// peer-death readiness remain observed throughout Checking and sending.
    pub fn wants_write(&self) -> bool {
        self.channel.has_pending_send()
    }

    /// Install only checked admission. It must belong to this exact bootstrap,
    /// not another successfully admitted artifact or another launch limit.
    pub fn admit<B>(&mut self, admitted: &Admitted<B>) -> Result<(), Fault> {
        self.admit_identity(&admitted.identity())
    }

    pub(super) fn admit_identity(&mut self, admitted: &AdmissionIdentity) -> Result<(), Fault> {
        if self.position != Position::Admitting {
            return self.fail(Fault::Protocol("admission outside bootstrap"));
        }
        let bootstrap = self.bootstrap.as_ref().expect("admitting owns bootstrap");
        let ready = admitted.ready();
        if admitted.bootstrap() != bootstrap {
            return self.fail(Fault::Protocol("admission belongs to another bootstrap"));
        }
        let body =
            serde_json::to_vec(ready).map_err(|_| Fault::Protocol("readiness cannot be encoded"));
        let body = self.checked(body)?;
        let queued = self
            .channel
            .begin_send(Tag::Ready, 0, body)
            .map_err(Fault::Transport);
        self.checked(queued)?;
        self.scheme_id.clone_from(&ready.scheme_id);
        self.verbs = admitted.verbs().to_vec();
        self.bootstrap = None;
        self.position = Position::ReadyPending;
        Ok(())
    }

    pub fn refuse(&mut self, refusal: Ste100AdmissionRefused) -> Result<(), Fault> {
        if self.position != Position::Admitting {
            return self.fail(Fault::Protocol("refusal outside bootstrap"));
        }
        let valid = refusal
            .validate()
            .map_err(|_| Fault::Protocol("invalid admission refusal"));
        self.checked(valid)?;
        let body = serde_json::to_vec(&refusal)
            .map_err(|_| Fault::Protocol("admission refusal cannot be encoded"));
        let body = self.checked(body)?;
        let queued = self
            .channel
            .begin_send(Tag::AdmissionRefused, 0, body)
            .map_err(Fault::Transport);
        self.checked(queued)?;
        self.bootstrap = None;
        self.stop = Some(Stop::AdmissionRefused);
        self.position = Position::Ending;
        Ok(())
    }

    /// The checker supplies one terminal result, never partial bindings. An
    /// invalid backend result is converted to component failure at this seam,
    /// not a language verdict. The checked request supplies all correlation.
    pub fn complete(&mut self, result: Ste100Result) -> Result<(), Fault> {
        if self.position != Position::Checking {
            return self.fail(Fault::Protocol("completion without outstanding work"));
        }
        // Check queued traffic before accepting a completion, including when a
        // worker and the socket become ready in the same reactor iteration.
        match self.channel.try_receive() {
            Ok(Receive::Pending) => {}
            Ok(_) => return self.fail(Fault::Protocol("overlapping checker request")),
            Err(error) => return self.fail(Fault::Transport(error)),
        }
        let request = self.outstanding.as_ref().expect("Checking owns work");
        let mut reply = Ste100Reply {
            request_id: request.request_id,
            target: request.target.clone(),
            scheme_id: request.scheme_id.clone(),
            result,
        };
        if reply.validate_for(request, &self.verbs).is_err() {
            reply.result = Ste100Result::ComponentFailed {
                code: Ste100ComponentCode::InvalidResult,
                detail: "checker returned an invalid complete result".into(),
            };
        }
        let terminal = matches!(reply.result, Ste100Result::ComponentFailed { .. });
        let body =
            serde_json::to_vec(&reply).map_err(|_| Fault::Protocol("result cannot be encoded"));
        let body = self.checked(body)?;
        let queued = self
            .channel
            .begin_send(Tag::Reply, reply.request_id.0, body)
            .map_err(Fault::Transport);
        self.checked(queued)?;
        self.position = if terminal {
            self.stop = Some(Stop::ComponentFailed);
            Position::Ending
        } else {
            Position::ReplyPending
        };
        Ok(())
    }

    /// Advance at most one incoming datagram and one outgoing fragment. The
    /// caller waits on readiness when Pending and continues after Progress.
    pub fn advance(&mut self) -> Result<Action, Fault> {
        match self.position {
            Position::Failed => return Err(Fault::Failed),
            Position::Closed => return Ok(Action::Stopped(self.stop.expect("closed reason"))),
            _ => {}
        }
        let was_partial = self.channel.has_partial_receive();
        match self.channel.try_receive() {
            Err(ChannelFault::Closed) if self.position == Position::Idle && !was_partial => {
                self.position = Position::Closed;
                self.stop = Some(Stop::PeerClosed);
                return Ok(Action::Stopped(Stop::PeerClosed));
            }
            Err(error) => return self.fail(Fault::Transport(error)),
            Ok(Receive::Pending) => {}
            Ok(Receive::Fragment) => {
                if !matches!(self.position, Position::Starting | Position::Idle) {
                    return self.fail(Fault::Protocol("traffic before prior answer completed"));
                }
                return Ok(Action::Progress);
            }
            Ok(Receive::Complete(message)) => return self.received(message),
        }
        if !self.channel.has_pending_send() {
            return Ok(Action::Pending);
        }
        let sent = self.channel.try_send().map_err(Fault::Transport);
        match self.checked(sent)? {
            SendProgress::Pending => Ok(Action::Pending),
            SendProgress::Progress => Ok(Action::Progress),
            SendProgress::Complete => {
                match self.position {
                    Position::ReadyPending => self.position = Position::Idle,
                    Position::ReplyPending => {
                        let request = self.outstanding.take().expect("reply owns work");
                        self.next_id = request.request_id.0.checked_add(1);
                        self.position = Position::Idle;
                    }
                    Position::Ending => {
                        self.channel.close();
                        self.outstanding = None;
                        self.position = Position::Closed;
                        return Ok(Action::Stopped(self.stop.expect("terminal reason")));
                    }
                    _ => return self.fail(Fault::Protocol("unsolicited outgoing message")),
                }
                Ok(Action::Progress)
            }
        }
    }

    fn received(&mut self, message: Message) -> Result<Action, Fault> {
        match self.position {
            Position::Starting if message.tag == Tag::Bootstrap => {
                self.position = Position::Admitting;
                match validate_bootstrap(
                    DecimalU64(self.channel.max_wire_bytes() as u64),
                    &message.payload,
                ) {
                    Ok(bootstrap) => {
                        self.bootstrap = Some(bootstrap.bootstrap().clone());
                        Ok(Action::Bootstrap(bootstrap))
                    }
                    Err(refusal) => {
                        self.refuse(refusal)?;
                        Ok(Action::Progress)
                    }
                }
            }
            Position::Idle if message.tag == Tag::Request => {
                let text = std::str::from_utf8(&message.payload)
                    .map_err(|_| Fault::Protocol("request is not UTF-8"));
                let text = self.checked(text)?;
                let request = decode_strict::<Ste100Request>(text)
                    .map_err(|_| Fault::Protocol("invalid request JSON"));
                let request = self.checked(request)?;
                if request.validate().is_err()
                    || Some(request.request_id.0) != self.next_id
                    || request.request_id.0 != message.id
                    || request.scheme_id != self.scheme_id
                {
                    return self.fail(Fault::Protocol("invalid request identity or shape"));
                }
                self.outstanding = Some(request.clone());
                self.position = Position::Checking;
                Ok(Action::Check(request))
            }
            _ => self.fail(Fault::Protocol("message is not licensed in this state")),
        }
    }

    fn checked<T>(&mut self, result: Result<T, Fault>) -> Result<T, Fault> {
        match result {
            Ok(value) => Ok(value),
            Err(fault) => self.fail(fault),
        }
    }

    pub(super) fn fail<T>(&mut self, fault: Fault) -> Result<T, Fault> {
        self.channel.close();
        self.bootstrap = None;
        self.outstanding = None;
        self.position = Position::Failed;
        Err(fault)
    }
}
