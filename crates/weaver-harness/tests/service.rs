//! conforms: harness-out-of-order-refused
//! conforms: harness-scoped-refusal-account
//!
//! The service tests of `weaver-harness-Spec` section 8: the out-of-order
//! refusal with its two watches and the scoped refusal account. The
//! announce-after-record discipline needs an entered run, which needs organ
//! doubles, so it is tested from inside the crate in `lifecycle.rs` where the
//! run can be built directly.
#![cfg(target_os = "linux")]

use std::os::fd::{AsFd, AsRawFd, OwnedFd};

use weaver_harness::{CoordinationListener, Harness, OrganBinaries, Outcome, bind_coordination};
use weaver_types::{
    ExchangeId, LifecycleAnswer, LifecycleDirective, LifecycleRefusal, MAX_ENVELOPE_BYTES, Opener,
    OrganEnvelope, Payload, Position,
};

/// Stands a coordination peer up against a harness serving on a thread, so a
/// test can drive directives and read answers. The harness's own fork sites
/// are not reached by these directives, which is what keeps the service
/// testable without organ binaries.
///
/// **`adopt` clears the dumpable flag for this whole test process**, since
/// `prctl(PR_SET_DUMPABLE, 0)` is process-wide and this stands the harness up
/// on a thread rather than in a forked child. The first test to run therefore
/// disables core dumps and same-uid `ptrace` for this binary and every test
/// after it. A maintainer who cannot attach a debugger to these tests is owed
/// the reason, and the containment a forked child would buy is not taken
/// because the peer must outlive the harness to drive it.
/// The suite drives the harness through its real listener, so the dialing peer
/// must be root: the accept refuses every other uid, which is the property
/// under test elsewhere in this file.
///
/// **Run these under a user namespace to exercise them without privilege:**
/// `unshare -r cargo test -p weaver-harness`. Measured 2026-08-06, `unshare -r`
/// maps the caller to uid 0 and `SO_PEERCRED` reads 0 across a socket inside
/// it, so the whole suite runs unprivileged that way. Without it they skip
/// loudly rather than passing vacuously.
fn root_or_skip(what: &str) -> bool {
    if nix::unistd::getuid().is_root() {
        return true;
    }
    eprintln!("SKIP {what}: the harness accepts root alone. Run `unshare -r cargo test`.");
    false
}

/// A scratch directory standing in for the unit's runtime directory, which the
/// init system creates and removes in production.
struct Scratch(std::path::PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn bound_listener() -> (CoordinationListener, std::path::PathBuf, Scratch) {
    let dir = Scratch(std::env::temp_dir().join(format!(
        "weaver-harness-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    )));
    let _ = std::fs::remove_dir_all(&dir.0);
    std::fs::create_dir_all(&dir.0).expect("scratch");
    let path = dir.0.join("coordination.sock");
    let listener = bind_coordination(&path).expect("bind");
    (listener, path, dir)
}

/// Admin's part: dial the name the worker bound. It carries whatever
/// credentials this process holds, which is what makes both halves of the
/// accept's check reachable: root under `unshare -r`, and not root outside it.
fn dial(path: &std::path::Path) -> OwnedFd {
    let fd = nix::sys::socket::socket(
        nix::sys::socket::AddressFamily::Unix,
        nix::sys::socket::SockType::SeqPacket,
        nix::sys::socket::SockFlag::SOCK_CLOEXEC,
        None,
    )
    .expect("socket");
    let address = nix::sys::socket::UnixAddr::new(path).expect("address");
    nix::sys::socket::connect(fd.as_raw_fd(), &address).expect("connect");
    fd
}

struct Peer {
    fd: OwnedFd,
    path: std::path::PathBuf,
    _dir: Scratch,
}

impl Peer {
    fn stand_up() -> (
        Peer,
        std::thread::JoinHandle<Result<Outcome, weaver_harness::ChannelFault>>,
    ) {
        Peer::stand_up_with(weaver_harness::LoopIdentity::compiled("test"))
    }

    /// The same, with the composer the binary would hand `serve`, so a test can
    /// read it back off the record and hold it to what was handed.
    fn stand_up_with(
        composer: weaver_harness::LoopIdentity,
    ) -> (
        Peer,
        std::thread::JoinHandle<Result<Outcome, weaver_harness::ChannelFault>>,
    ) {
        let (listener, path, dir) = bound_listener();
        let handle = std::thread::spawn(move || {
            let harness = Harness::listen(
                listener,
                OrganBinaries {
                    classify: None,
                    spu: "/nonexistent/spu".into(),
                    gate: "/nonexistent/gate".into(),
                },
                weaver_harness::OrganParameters::default(),
            )
            .expect("listen");
            // The inert entry: these tests drive directives alone, so no
            // frame ever arrives to call it.
            harness.serve(
                "",
                &[],
                composer,
                |_: &mut weaver_harness::Ports<'_>, _: &str| {
                    Err(weaver_harness::TurnError::Unlicensed {
                        turn: weaver_types::TurnKey("t-0".into()),
                    })
                },
            )
        });
        let peer = Peer {
            fd: dial(&path),
            path,
            _dir: dir,
        };
        (peer, handle)
    }

    /// **The peer plays admin, and hand-rolls its two calls.** Admin's own
    /// channel lives in its own crate, so nothing here reaches for a
    /// convenience this crate would otherwise have no production reason to
    /// publish.
    fn send(&self, ordinal: u64, directive: LifecycleDirective) {
        self.send_payload(ordinal, Payload::Directive(directive));
    }

    /// Sends a directive with descriptors riding beside it, the way admin
    /// hands the harness its sink and, where the member stands, the state
    /// end: `SCM_RIGHTS` on the same message.
    fn send_with(
        &self,
        ordinal: u64,
        directive: LifecycleDirective,
        descriptors: &[std::os::fd::RawFd],
    ) {
        let envelope = OrganEnvelope {
            exchange: ExchangeId {
                opener: Opener::Admin,
                ordinal,
            },
            position: Position::Open,
            payload: Payload::Directive(directive),
        };
        let body = serde_json::to_vec(&envelope).expect("render");
        let slices = [std::io::IoSlice::new(&body)];
        let rights = [nix::sys::socket::ControlMessage::ScmRights(descriptors)];
        nix::sys::socket::sendmsg::<()>(
            self.fd.as_raw_fd(),
            &slices,
            &rights,
            nix::sys::socket::MsgFlags::empty(),
            None,
        )
        .expect("sent with descriptors");
    }

    /// Sends any payload, for the tests that put something other than a
    /// directive on the channel.
    fn send_payload(&self, ordinal: u64, payload: Payload) {
        let envelope = OrganEnvelope {
            exchange: ExchangeId {
                opener: Opener::Admin,
                ordinal,
            },
            position: Position::Open,
            payload,
        };
        let body = serde_json::to_vec(&envelope).expect("render");
        let slices = [std::io::IoSlice::new(&body)];
        nix::sys::socket::sendmsg::<()>(
            self.fd.as_raw_fd(),
            &slices,
            &[],
            nix::sys::socket::MsgFlags::empty(),
            None,
        )
        .expect("sent");
    }

    /// The answer, `None` where the serving side closed in order without
    /// one, or the socket's own error where it reset, so a test can name how
    /// service ended instead of reporting a decode of nothing.
    fn try_read(&self) -> Result<Option<Payload>, nix::Error> {
        let mut buffer = vec![0u8; MAX_ENVELOPE_BYTES];
        let mut slices = [std::io::IoSliceMut::new(&mut buffer)];
        let message = nix::sys::socket::recvmsg::<()>(
            self.fd.as_raw_fd(),
            &mut slices,
            None,
            nix::sys::socket::MsgFlags::empty(),
        )?;
        let read = message.bytes;
        if read == 0 {
            return Ok(None);
        }
        let envelope: OrganEnvelope =
            serde_json::from_slice(&buffer[..read]).expect("answer decodes");
        Ok(Some(envelope.payload))
    }

    fn read(&self) -> Payload {
        let mut buffer = vec![0u8; MAX_ENVELOPE_BYTES];
        let mut slices = [std::io::IoSliceMut::new(&mut buffer)];
        let message = nix::sys::socket::recvmsg::<()>(
            self.fd.as_raw_fd(),
            &mut slices,
            None,
            nix::sys::socket::MsgFlags::empty(),
        )
        .expect("answer received");
        let read = message.bytes;
        let envelope: OrganEnvelope =
            serde_json::from_slice(&buffer[..read]).expect("answer decodes");
        envelope.payload
    }

    /// Closes this verb's connection and dials again, which is what admin
    /// does between verbs: each invocation brings its own connection and the
    /// run outlives all of them.
    fn hang_up_and_redial(&mut self) {
        let fresh = dial(&self.path);
        let old = std::mem::replace(&mut self.fd, fresh);
        drop(old);
    }
}

/// **A directive out of order is refused and not queued.** A leave arriving
/// before any enter answers `OutOfOrder` and reaches no unwind.
///
/// Perturbation one: make the before-enter arm fall through to the unwind and
/// the leave reaches it, ending service where a refusal was owed. Watched
/// under exactly that change.
#[test]
fn leave_before_enter_is_refused() {
    if !root_or_skip("leave_before_enter_is_refused") {
        return;
    }
    let (peer, handle) = Peer::stand_up();
    peer.send(1, LifecycleDirective::Leave);
    match peer.read() {
        Payload::Refusal(LifecycleRefusal::OutOfOrder) => {}
        other => panic!("an early leave must answer OutOfOrder, got {other:?}"),
    }
    // Service continues: the refusal was not a fault and the channel stands.
    peer.send(2, LifecycleDirective::Stop);
    assert!(matches!(
        peer.read(),
        Payload::Refusal(LifecycleRefusal::OutOfOrder)
    ));
    drop(peer);
    // **The worker is still serving and the thread does not end**, because a
    // closed connection is not an ending: it accepts again and waits for the
    // next verb. Joining here would block forever, which is the property
    // rather than a leak, so the handle is dropped and the test process
    // reaps the thread on exit.
    drop(handle);
}

/// A stop arriving before any enter is out of order for the position, which
/// is the before-enter arm rather than the at-rest answer.
///
/// **The at-rest close and the terminal position are not this test's**, and
/// saying so beats a name that overstates: `AtRest` needs an entered run and
/// is tested in `lifecycle.rs`, and the terminal arm needs a completed leave,
/// which needs organ doubles this suite does not buy.
#[test]
fn stop_before_enter_is_out_of_order() {
    if !root_or_skip("stop_before_enter_is_out_of_order") {
        return;
    }
    let (peer, handle) = Peer::stand_up();
    // A stop before any enter is out of order for the position, which is the
    // before-enter arm rather than the at-rest answer.
    peer.send(1, LifecycleDirective::Stop);
    assert!(
        matches!(peer.read(), Payload::Refusal(LifecycleRefusal::OutOfOrder)),
        "a stop before enter is out of order, not an at-rest close"
    );
    drop(peer);
    // **The worker is still serving and the thread does not end**, because a
    // closed connection is not an ending: it accepts again and waits for the
    // next verb. Joining here would block forever, which is the property
    // rather than a leak, so the handle is dropped and the test process
    // reaps the thread on exit.
    drop(handle);
}

/// **The scoped refusal account:** a refusal before the `load` event leaves the
/// stream clean and the state at before-enter, so the channel still serves and
/// a later directive is judged against that position rather than a half-stood
/// one.
///
/// Perturbation: author the load event before the sink is received and a
/// refused enter leaves a bracket on a stream nothing will close. Watched by
/// moving the authoring point ahead of the receive.
#[test]
fn refused_enter_leaves_the_state_at_before_enter() {
    if !root_or_skip("refused_enter_leaves_the_state_at_before_enter") {
        return;
    }
    let (peer, handle) = Peer::stand_up();
    // An enter with no ancillary sink descriptor cannot construct a recorder,
    // so it refuses before anything is authored.
    peer.send(
        1,
        LifecycleDirective::Enter {
            payload: Box::new(weaver_types::EnterPayload {
                session: weaver_types::SessionId("s-1".into()),
                run: weaver_types::RunId("r-1".into()),
                spu_instruction: weaver_types::SpuInstruction {
                    classify: None,
                    decoder: weaver_types::DecoderInstruction {
                        model_binding: weaver_types::ModelBinding {
                            artifact: weaver_types::ArtifactRef("qwen".into()),
                            devices: vec![weaver_types::DeviceOrdinal(0)],
                        },
                        residual_readout_election: false,
                        field_election: None,
                        surprisal_election: false,
                        refeed_permission: false,
                        column_permission: false,
                        identity: vec![],
                        tunable_values: Default::default(),
                    },
                },
                binding: weaver_types::EnterBinding::Serving {
                    gate_instruction: weaver_types::GateInstruction {
                        access_rule: weaver_types::AccessRule {
                            allowed_uids: Default::default(),
                            allowed_gids: Default::default(),
                            denied_uids: Default::default(),
                        },
                    },
                },
                state_store: weaver_types::StateStore::default(),
                declaration: String::new(),
                restore: None,
                stack: Default::default(),
                state_election: weaver_types::StateElection::default(),
            }),
        },
    );
    match peer.read() {
        Payload::Refusal(LifecycleRefusal::DescriptorsUnusable) => {}
        other => panic!("an enter with no sink refuses, got {other:?}"),
    }
    // The position held: a leave is still out of order, which it would not be
    // had the refused enter left the state entered.
    peer.send(2, LifecycleDirective::Leave);
    assert!(
        matches!(peer.read(), Payload::Refusal(LifecycleRefusal::OutOfOrder)),
        "the state stayed at before-enter"
    );
    drop(peer);
    // **The worker is still serving and the thread does not end**, because a
    // closed connection is not an ending: it accepts again and waits for the
    // next verb. Joining here would block forever, which is the property
    // rather than a leak, so the handle is dropped and the test process
    // reaps the thread on exit.
    drop(handle);
}

/// **A connection closing is not an ending, and the run outlives it.** Admin is
/// per-invocation, so each verb arrives on its own connection and closes it
/// when the verb answers, per `weaver-admin-harness-contract` section 4: the
/// ordering is the worker's rather than any connection's.
///
/// Perturbation: return an outcome when `serve_connection` reports a close,
/// which is what the pre-inversion code did, and the second directive below
/// reaches a worker that has stopped serving. Watched under exactly that
/// change.
#[test]
fn a_closed_connection_does_not_end_service() {
    if !root_or_skip("a_closed_connection_does_not_end_service") {
        return;
    }
    let (mut peer, handle) = Peer::stand_up();
    // A leave before an enter is refused deterministically, which is enough to
    // prove the worker answered on this connection.
    peer.send(1, LifecycleDirective::Leave);
    assert!(matches!(
        peer.read(),
        Payload::Refusal(LifecycleRefusal::OutOfOrder)
    ));

    // Admin's verb ends: the connection closes and a new one is dialed.
    peer.hang_up_and_redial();

    // The worker is still serving, and still in the same state.
    peer.send(2, LifecycleDirective::Leave);
    assert!(
        matches!(peer.read(), Payload::Refusal(LifecycleRefusal::OutOfOrder)),
        "the run survived the close and the state came with it"
    );
    drop(peer);
    // Still serving, per the note above.
    drop(handle);
}

/// A message that is not a directive cannot be attributed to an exchange for a
/// refusal to answer, so it is a fault below the exchange layer.
#[test]
fn non_directive_payload_is_a_fault() {
    if !root_or_skip("non_directive_payload_is_a_fault") {
        return;
    }
    let (peer, handle) = Peer::stand_up();
    peer.send_payload(1, Payload::Answer(LifecycleAnswer::Ready));
    let result = handle.join().expect("thread");
    assert!(
        matches!(result, Err(weaver_harness::ChannelFault::Undecodable)),
        "an answer on the coordination channel is below the exchange layer, got {result:?}"
    );
    drop(peer);
}

/// The adopted end is owned: dropping the `Harness` closes it, which is the
/// close being a type property rather than an integer left behind.
///
/// The previous version of this test asserted that a descriptor number was
/// above 2, which the kernel guarantees for any process with the standard
/// streams open - it never constructed a `Harness` and could not fail. This
/// one constructs, drops, and observes the descriptor is gone. Run in a forked
/// child because `listen` clears the dumpable flag process-wide.
#[test]
fn dropping_the_harness_closes_the_listener() {
    let (report_r, report_w) = nix::unistd::pipe().expect("pipe");
    // SAFETY: the child adopts, drops, probes, reports, and _exits.
    // The directory is made and guarded here, before the fork: the child
    // `_exit`s and drops nothing, so the parent's guard is what removes it,
    // pass or fail. Fresh per run by the clock, so a recycled pid never
    // meets a stale socket.
    let dir = Scratch(std::env::temp_dir().join(format!(
        "weaver-drop-{}-{:?}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    )));
    std::fs::create_dir_all(&dir.0).expect("scratch");
    match unsafe { nix::unistd::fork() }.expect("fork") {
        nix::unistd::ForkResult::Child => {
            let listener = bind_coordination(&dir.0.join("c.sock")).expect("bind");
            let raw = listener.as_fd().as_raw_fd();
            let harness = Harness::listen(
                listener,
                OrganBinaries {
                    classify: None,
                    spu: "/nonexistent/spu".into(),
                    gate: "/nonexistent/gate".into(),
                },
                weaver_harness::OrganParameters::default(),
            )
            .expect("listen");
            drop(harness);
            let flags = unsafe { nix::libc::fcntl(raw, nix::libc::F_GETFD) };
            let closed = (flags == -1) as u8;
            let _ = nix::unistd::write(report_w.as_fd(), &[closed]);
            unsafe { nix::libc::_exit(0) };
        }
        nix::unistd::ForkResult::Parent { child } => {
            drop(report_w);
            let mut byte = [0u8; 1];
            let _ = nix::unistd::read(report_r.as_fd(), &mut byte);
            let _ = nix::sys::wait::waitpid(child, None);
            assert_eq!(byte[0], 1, "the adopted end is closed when its owner drops");
        }
    }
}

/// **The accept refuses a peer that is not root.** The socket lives inside the
/// agent's sandbox and is reachable from it by construction, so an elected tool
/// at the agent uid can dial it, and what refuses that tool is the credential
/// rather than a name it could not resolve. The earlier design expected the
/// agent's own uid here, which every tool of the agent's satisfies.
///
/// **This runs unprivileged and only unprivileged**, which is the inverse of
/// the guard everything above carries: the dialing peer must not be root for
/// the refusal to be reachable, so under `unshare -r` it skips and outside it
/// runs. Between the two, both sides of the check are exercised.
///
/// Perturbation: remove the uid comparison from `accept_root` and this test
/// fails, because the connection is accepted and answered. Watched under
/// exactly that removal.
#[test]
fn the_accept_refuses_a_peer_that_is_not_root() {
    if nix::unistd::getuid().is_root() {
        eprintln!(
            "SKIP the_accept_refuses_a_peer_that_is_not_root: needs a non-root dialer, \
             so run this one without `unshare -r`"
        );
        return;
    }
    let (listener, path, _dir) = bound_listener();
    let dialer = std::thread::spawn(move || dial(&path));
    let refused = listener.accept_root();
    let _held = dialer.join().expect("thread");
    match refused {
        Err(weaver_harness::ChannelFault::WrongPeer { uid }) => {
            assert_eq!(uid, nix::unistd::getuid().as_raw(), "the dialer's own uid");
        }
        other => panic!("a non-root dialer is refused at the accept, got {other:?}"),
    }
}

/// **The listener is not closed after an accept.** Admin is per-invocation and
/// every later verb dials again, so a listener that closed after one accept
/// would leave every verb after the load with nothing to dial. This is the
/// property that replaced the pre-inversion closure.
///
/// Perturbation: close the listener inside `accept_root` so it does not
/// survive, and this test stops passing. Watched under exactly that change,
/// where it aborts on the runtime's own IO-safety check for a descriptor
/// closed twice rather than failing an assertion. The abort is the failure,
/// and it is named here so a reader is not surprised by the shape of it.
#[test]
fn the_listener_survives_an_accept() {
    if !root_or_skip("the_listener_survives_an_accept") {
        return;
    }
    let (listener, path, _dir) = bound_listener();
    let first = dial(&path);
    let accepted = listener.accept_root().expect("first accept");
    drop(accepted);
    drop(first);
    // The listener stands, so a second verb reaches it.
    let second = dial(&path);
    let again = listener.accept_root();
    assert!(
        again.is_ok(),
        "a second dial is accepted on the same listener, got {again:?}"
    );
    drop(second);
}

/// The enter payload a serving load carries, against organs that do not
/// exist: the load is authored before the fan-out fails, which is what lets
/// a test read the record the production path wrote.
fn serving_enter(session: &str) -> LifecycleDirective {
    LifecycleDirective::Enter {
        payload: Box::new(weaver_types::EnterPayload {
            session: weaver_types::SessionId(session.into()),
            run: weaver_types::RunId("r-1".into()),
            spu_instruction: weaver_types::SpuInstruction {
                classify: None,
                decoder: weaver_types::DecoderInstruction {
                    model_binding: weaver_types::ModelBinding {
                        artifact: weaver_types::ArtifactRef("unreachable".into()),
                        devices: vec![weaver_types::DeviceOrdinal(0)],
                    },
                    residual_readout_election: false,
                    field_election: None,
                    surprisal_election: false,
                    refeed_permission: false,
                    column_permission: false,
                    identity: Vec::new(),
                    tunable_values: Default::default(),
                },
            },
            binding: weaver_types::EnterBinding::Serving {
                gate_instruction: weaver_types::GateInstruction {
                    access_rule: weaver_types::AccessRule {
                        allowed_uids: Default::default(),
                        allowed_gids: Default::default(),
                        denied_uids: Default::default(),
                    },
                },
            },
            state_store: weaver_types::StateStore::default(),
            declaration: String::new(),
            restore: None,
            stack: Default::default(),
            state_election: weaver_types::StateElection {
                all_kinds: false,
                keys: Vec::new(),
            },
        }),
    }
}

/// **The load names the composer serve was handed and the member the enter
/// carried, through the production path**: `serve` receives the binary's
/// composer, the peer plays admin and sends the enter with its sink over
/// `SCM_RIGHTS`, with and without a state end beside it, and the load is
/// authored before the fan-out. The organs do not exist, so the enter is
/// refused after the load and service then ends on their death, which is
/// the harness's own rule for a dead organ and not this test's doing. The
/// record the path wrote names both. Per `weaver-harness-Spec` section 6.1
/// and `weaver-trace-Spec` section 3.
///
/// Perturbation: have `serve` keep a compiled composer instead of the one it
/// was handed, and this fails on the binary, the first member read back.
/// Watched under exactly that change.
#[test]
fn the_load_names_what_serve_was_handed_and_what_the_enter_carried() {
    if !root_or_skip("the_load_names_what_serve_was_handed_and_what_the_enter_carried") {
        return;
    }
    for standing in [false, true] {
        let (peer, handle) = Peer::stand_up_with(weaver_harness::LoopIdentity::file(
            "pyworker",
            std::path::Path::new("/deployed/loop.py"),
            Some("ef".repeat(32)),
        ));
        let sink_path = peer._dir.0.join("trace.ndjson");
        let sink = OwnedFd::from(std::fs::File::create(&sink_path).expect("sink"));
        let ends = standing.then(|| std::os::unix::net::UnixStream::pair().expect("pair"));
        let mut descriptors = vec![sink.as_raw_fd()];
        if let Some((near, _)) = &ends {
            descriptors.push(near.as_raw_fd());
        }
        peer.send_with(1, serving_enter("s-named"), &descriptors);
        match peer.try_read() {
            Ok(Some(Payload::Refusal(_))) => {}
            Ok(Some(other)) => panic!("the missing organs refuse after the load, got {other:?}"),
            Ok(None) => {
                drop(peer);
                let ended = handle.join().expect("service thread");
                panic!("the serving side closed without answering: service ended with {ended:?}");
            }
            Err(error) => {
                drop(peer);
                let ended = handle.join().expect("service thread");
                panic!("no answer ({error}): service ended with {ended:?}");
            }
        }
        // Service ends on the organs' death, after the refusal was answered:
        // the join is what proves serving did not panic. The peer holds the
        // scratch directory, so it outlives the read of the record.
        let outcome = handle.join().expect("service thread did not panic");
        assert!(
            matches!(outcome, Err(weaver_harness::ChannelFault::Closed)),
            "service ended on the dead organs, not by a panic: {outcome:?}"
        );
        let held = std::fs::read_to_string(&sink_path).expect("the sink reads back");
        let load: serde_json::Value =
            serde_json::from_str(held.lines().next().expect("a load")).expect("parses");
        assert_eq!(load["kind"], "load");
        assert_eq!(
            load["payload"]["state_member"],
            serde_json::json!(standing),
            "the member's standing is the end's presence on the enter"
        );
        assert_eq!(
            load["payload"]["composer"]["binary"], "pyworker",
            "the composer is the one serve was handed"
        );
        assert_eq!(load["payload"]["composer"]["file"], "/deployed/loop.py");
        assert_eq!(load["payload"]["composer"]["sha256"], "ef".repeat(32));
        drop(peer);
    }
}
