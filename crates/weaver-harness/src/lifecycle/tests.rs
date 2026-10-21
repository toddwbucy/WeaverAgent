//! The announce-after-record test of section 8, run from inside the crate
//! because an entered run needs organ doubles the integration suite does
//! not buy, while the run struct itself is constructible here.

use super::*;

/// **What a diagnostic enter waits on is named**, per
/// `weaver-harness-Spec` section 6.1 as of 2026-09-06: one typed line
/// naming the seal and the bound.
#[test]
fn the_wait_is_named_where_the_door_stands() {
    let line = parked_ask_notice(600_000);
    let parsed: serde_json::Value = serde_json::from_str(&line).expect("one typed line");
    assert_eq!(parsed["binding"], "diagnostic");
    assert_eq!(parsed["bound_ms"], 600_000);
    assert_eq!(parsed["waiting"], "the state member's preload seal");
}

fn lineage(save_point: &str, run: &str, sequence: u64, turn: u64) -> weaver_types::Lineage {
    weaver_types::Lineage {
        save_point: save_point.into(),
        run: weaver_types::RunId(run.into()),
        sequence,
        turn,
        named_at_restore: false,
        built_from: None,
    }
}

/// **A restoring run numbers its turns from one**, per `weaver-harness-Spec`
/// section 6.1 on the operator's ruling of 2026-10-02 on #59: the ordinal
/// a run starts from is zero whatever the lineage's last turn is.
///
/// Perturbation: take the ordinal from the lineage's `turn` and the
/// second assertion fails. Watched under exactly that change.
#[test]
fn a_restoring_run_numbers_its_turns_from_one() {
    assert_eq!(initial_turn_ordinal(None), 0);
    assert_eq!(initial_turn_ordinal(Some(&lineage("ab", "r-a", 41, 7))), 0);
}

/// **The restored answer agrees with the enter's lineage in four members
/// and in no other way**, per `weaver-harness-Spec` section 6.1: a miss
/// and a refusal disagree, an empty answer agrees only with an enter
/// naming no save point, and a stamp agrees only where its digest, run,
/// sequence and turn all equal the lineage's, `named_at_restore` being
/// admin's and never compared.
///
/// Perturbation: drop the `turn` comparison from `restored_agrees` and
/// the turn case agrees. Watched under exactly that removal.
#[test]
fn the_restored_answer_agrees_in_four_members() {
    use crate::state::{RestoredAnswer, SavePointStamp};
    let stamp = |digest: &str, run: &str, sequence: u64, turn: u64| {
        RestoredAnswer::Lineage(SavePointStamp {
            digest: digest.into(),
            run: run.into(),
            sequence,
            turn,
        })
    };
    let named = lineage("ab", "r-a", 41, 7);
    assert!(restored_agrees(None, None).is_err(), "a miss");
    assert!(
        restored_agrees(None, Some(&named)).is_err(),
        "a miss under a lineage"
    );
    assert!(
        restored_agrees(
            Some(&RestoredAnswer::Refused("schema-mismatch".into())),
            Some(&named)
        )
        .is_err()
    );
    assert!(restored_agrees(Some(&RestoredAnswer::Empty), None).is_ok());
    assert!(restored_agrees(Some(&RestoredAnswer::Empty), Some(&named)).is_err());
    assert!(restored_agrees(Some(&stamp("ab", "r-a", 41, 7)), None).is_err());
    assert!(restored_agrees(Some(&stamp("ab", "r-a", 41, 7)), Some(&named)).is_ok());
    let mut supplied = named.clone();
    supplied.named_at_restore = true;
    assert!(
        restored_agrees(Some(&stamp("ab", "r-a", 41, 7)), Some(&supplied)).is_ok(),
        "named_at_restore is admin's and never compared"
    );
    for (digest, run, sequence, turn) in [
        ("zz", "r-a", 41, 7),
        ("ab", "r-b", 41, 7),
        ("ab", "r-a", 40, 7),
        ("ab", "r-a", 41, 6),
    ] {
        assert!(
            restored_agrees(Some(&stamp(digest, run, sequence, turn)), Some(&named)).is_err(),
            "{digest} {run} {sequence} {turn}"
        );
    }
}
use std::fs::File;
use weaver_trace::Kind;

/// A bound listener for tests that exercise `dispatch_on` directly. The
/// listener is never accepted on: these tests supply the connection, and
/// the field exists because the type does.
///
/// **The directory lives as long as the test holds the guard**, because
/// the listener keeps its path and an enter names the gate's socket
/// beside it, so it is not removed at the bind.
fn test_listener() -> (CoordinationListener, crate::scratch::Scratch) {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    // A directory left by an earlier run would hold a stale socket, and
    // the bind would refuse a name nothing is using, so `dir` removes it
    // first.
    let dir = crate::scratch::dir(format!("weaver-harness-unit-{}-{n}", std::process::id()));
    let listener = crate::channel::bind_coordination(&dir.join("c.sock")).expect("bind");
    (listener, dir)
}

fn test_exchange() -> ExchangeId {
    ExchangeId {
        opener: Opener::Admin,
        ordinal: 1,
    }
}

fn entered_run(turn: Option<&str>) -> (Run, OrganChannel, crate::scratch::Scratch) {
    let path = crate::scratch::Scratch(std::env::temp_dir().join(format!(
        "weaver-harness-stop-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    )));
    let sink = OwnedFd::from(File::create(&path).expect("sink"));
    let session = SessionId("s-1".to_string());
    let mut recorder = crate::record::Record::from(
        Recorder::receive(sink, RunRef("r-1".into()), SessionRef(session.0.clone()))
            .expect("recorder"),
    );
    let author = Author::new(&session, &weaver_types::RunId("r-1".into()));
    author
        .author(
            &mut recorder,
            Kind::Load,
            Subsystem::Harness,
            None,
            Some(Payload::Elections(Box::new(weaver_trace::Elections {
                residual_readout: false,
                field: None,
                surprisal: false,
                tee: Some(weaver_trace::Election::default()),
                state_member: false,
                declaration: Default::default(),
                lineage: None,
                reset: None,
                stack: Default::default(),
                boundary: String::new(),
                cause: weaver_trace::Cause { uid: 0 },
                library_path: None,
                state_store: Default::default(),
                composer: weaver_trace::LoopIdentity::compiled("test"),
            }))),
        )
        .expect("load");
    if let Some(turn) = turn {
        let key = TurnKey(turn.to_string());
        author
            .author(
                &mut recorder,
                Kind::TurnStarted,
                Subsystem::Harness,
                Some(&key),
                None,
            )
            .expect("turn started");
    }
    let (near, _far) = OrganChannel::pair().expect("pair");
    // A loaded-and-idle run holds a resident SPU, so the helper builds one
    // over a socketpair: the decode end is what the granted seat drives,
    // and a run with no SPU is not loaded.
    let (lifecycle, _spu_lifecycle) = OrganChannel::pair().expect("pair");
    let (decode, spu_decode) = DecodeChannel::pair().expect("decode pair");
    answer_appends(spu_decode, || {});
    (
        Run {
            classify: None,
            recorder,
            author,
            session,
            operator: 1000,
            seeded_bytes: 0,
            run: weaver_types::RunId("r-1".into()),
            spu: Some(SpuChannels {
                lifecycle,
                decode,
                pid: nix::unistd::Pid::from_raw(1),
                last_word: crate::spawn::LastWord::quiet(),
            }),
            gate: None,
            tools: None,
            leave_anchor: None,
            lower_anchor: None,
            gate_contained: None,
            diagnostic: None,
            state: None,
            grants_at_enter: None,
            load: weaver_types::LoadFacts {
                session: SessionId("s".into()),
                run: weaver_types::RunId("r-1".into()),
                declaration: String::new(),
                artifact: weaver_types::ArtifactRef("a".into()),
                residual_readout: false,
                field: None,
                surprisal: false,
                state_election: Default::default(),
                state_store: Default::default(),
                state_member: false,
                composer: weaver_types::Composer {
                    binary: "test".into(),
                    file: None,
                    sha256: None,
                },
                drain_bound: None,
                wind_down_bound: None,
            },
            fullness: None,
            pressure_reported: false,
            turn_in_flight: turn.map(|t| TurnKey(t.to_string())),
            spu_ordinal: 0,
            gate_ordinal: 0,
            held_frames: std::collections::VecDeque::new(),
            turn_ordinal: 0,
            gate_lowered: false,
            lower_refusal: None,
            drain_bound: None,
            wind_down_bound: None,
            leave: None,
        },
        near,
        path,
    )
}

/// **The run's position walks 3.0's states for every kind of leave**
/// (the Planner's sequence check, `weaver-admin-Spec` 3.0): derived from
/// the run and the leave's progress, never stored, so the table's input
/// cannot drift from the run. A graceful leave walks S2, S3, S4, S5, S6,
/// S7; a sole force walks S10 to S7; a leave a join or a bound turned
/// forced walks S4 forced and skips S5 to S6; a diagnostic binding skips
/// S5; a stopped leave stands in S9; an SPU dead at rest is `S2(Dead)`.
/// Perturbation: read a stored position, or map a forced leave's draining
/// step to graceful, and the walk fails.
#[test]
fn position_walks_three_point_zero_for_every_kind_of_leave() {
    use crate::lifecycle_table::{Binding, Leave, LeaveProgress, LeaveStep, Position, Spu};
    let progress = |directed, leave, step| LeaveProgress {
        directed,
        leave,
        step,
    };
    let (mut run, _admin, _scratch) = entered_run(None);
    assert_eq!(run.position(None), Position::S2(Spu::Resident));
    run.turn_in_flight = Some(TurnKey("t-1".into()));
    assert_eq!(run.position(None), Position::S3);

    // A graceful leave.
    let graceful = |step| progress(Leave::Graceful, Leave::Graceful, step);
    assert_eq!(
        run.position(Some(&graceful(LeaveStep::Draining))),
        Position::S4(Leave::Graceful, Binding::Serving)
    );
    run.turn_in_flight = None;
    assert_eq!(
        run.position(Some(&graceful(LeaveStep::WindingDown))),
        Position::S5
    );
    assert_eq!(
        run.position(Some(&graceful(LeaveStep::Lowering))),
        Position::S6(Leave::Graceful)
    );
    assert_eq!(
        run.position(Some(&graceful(LeaveStep::Saving))),
        Position::S6(Leave::Graceful)
    );
    assert_eq!(
        run.position(Some(&graceful(LeaveStep::Releasing))),
        Position::S7
    );

    // A sole force: S10 until the release.
    let sole = |step| progress(Leave::Forced, Leave::Forced, step);
    for step in [LeaveStep::Draining, LeaveStep::Lowering, LeaveStep::Saving] {
        assert_eq!(run.position(Some(&sole(step))), Position::S10, "{step:?}");
    }
    assert_eq!(
        run.position(Some(&sole(LeaveStep::Releasing))),
        Position::S7
    );

    // A graceful leave a join or a bound turned forced: S4 forced, no S5.
    let joined = |step| progress(Leave::Graceful, Leave::Forced, step);
    assert_eq!(
        run.position(Some(&joined(LeaveStep::Draining))),
        Position::S4(Leave::Forced, Binding::Serving)
    );
    assert_eq!(
        run.position(Some(&joined(LeaveStep::Lowering))),
        Position::S6(Leave::Forced)
    );
    assert_eq!(
        run.position(Some(&joined(LeaveStep::Releasing))),
        Position::S7
    );

    // A stopped graceful leave: S9, the gate lowered and the run at rest.
    run.gate_lowered = true;
    assert_eq!(run.position(None), Position::S9);
    run.gate_lowered = false;

    // An SPU dead at rest.
    let spu = run.spu.take();
    assert_eq!(run.position(None), Position::S2(Spu::Dead));
    run.spu = spu;

    // A diagnostic binding: S4 on the diagnostic arm, then S6, no S5.
    run.diagnostic = Some(DiagnosticSeat {
        reader_elected: false,
        artifact: String::new(),
        replayed: false,
    });
    assert_eq!(
        run.position(Some(&graceful(LeaveStep::Draining))),
        Position::S4(Leave::Graceful, Binding::Diagnostic)
    );
    assert_eq!(
        run.position(Some(&graceful(LeaveStep::Lowering))),
        Position::S6(Leave::Graceful)
    );
}

/// The declared election reaches the tee, through the real enter path:
/// a fake member listens at the seam's name, the enter runs with a
/// non-default election and organ binaries that cannot exec, and by
/// the time the fan-out fails after-load the member has already
/// received the opener carrying exactly the declared kinds and paths,
/// followed by the load event's distillate. The pairs-for-elected-keys
/// behavior itself is the tee suite's to pin - what this buys is the
/// plumbing from `EnterPayload` to `Tee::open` that nothing else
/// watches. Needs no device, no fixture, and no built organs.
#[test]
fn the_declared_election_reaches_the_tee() {
    let dir = crate::scratch::dir(format!(
        "weaver-election-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let listener = crate::channel::bind_coordination(&dir.join("coordination.sock")).expect("bind");

    // The fake member: the far end of the pair the enter would carry,
    // per the ruling of 2026-08-26 - no name, no dial, the end handed
    // in. Hand back every line that arrives until the peer closes, and
    // answer the enter's `restored` ask as a member that stood empty,
    // so the load is authored.
    let (state_end, member_end) =
        std::os::unix::net::UnixStream::pair().expect("the member's pair");
    let (send, receive) = std::sync::mpsc::channel::<String>();
    let reader = std::thread::spawn(move || {
        for line in member_lines(member_end) {
            let _ = send.send(line);
        }
    });

    let sink_path = dir.join("trace.ndjson");
    let sink = OwnedFd::from(File::create(&sink_path).expect("sink"));
    let mut harness = Harness {
        coordination: listener,
        organs: OrganBinaries {
            classify: None,
            spu: "/nonexistent/weaver-spu".into(),
            gate: "/nonexistent/weaver-gate".into(),
        },
        parameters: OrganParameters::default(),
        state: ChannelState::BeforeEnter,
        composer: Some(weaver_trace::LoopIdentity::compiled("test")),
    };
    let payload = weaver_types::EnterPayload {
        session: SessionId("s-election".into()),
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
            shell: None,
        },
        state_store: weaver_types::StateStore::default(),
        declaration: String::new(),
        restore: None,
        reset: None,
        stack: Default::default(),
        boundary: String::new(),
        cause: weaver_types::Cause { uid: 0 },
        operator: 1000,
        library_path: None,
        drain_bound: None,
        wind_down_bound: None,
        state_election: weaver_types::StateElection {
            all_kinds: false,
            keys: vec![weaver_types::ElectedKindConfig {
                kind: "load".into(),
                paths: vec!["origin".into()],
            }],
        },
    };

    // The fan-out fails after-load at the SPU exec, which is the point:
    // the tee attach and the load event precede the forks. The bracket
    // stands on that failure, so the run is retained and left the way
    // the serving loop leaves it - dropping it unleft would orphan the
    // forked child unreaped and leave the bracket unclosed.
    let mut run = match harness.enter(
        payload,
        Some(sink),
        Some(std::os::fd::OwnedFd::from(state_end)),
    ) {
        Err(EnterFailure::AfterLoad(run, _)) => run,
        Ok(_) => panic!("the bogus fan-out cannot succeed"),
        Err(EnterFailure::BeforeLoad(refusal)) => {
            panic!("failed before the load: {refusal:?}")
        }
    };
    let _ = leave(&mut run, None, false);
    drop(run);
    // The leave closed the bracket and the drop closed the tee's
    // channel, so the reader drains to end-of-stream and finishes.
    reader.join().expect("the member read to closure");

    let opener = receive.recv().expect("the opener arrived first");
    let parsed: serde_json::Value = serde_json::from_str(&opener).expect("opener parses");
    let election = &parsed["election"];
    assert_eq!(election["all_kinds"], serde_json::Value::Bool(false));
    assert_eq!(election["keys"][0]["kind"], "load");
    assert_eq!(election["keys"][0]["paths"][0], "origin");

    // The restored ask follows the opener and precedes the load's
    // distillate, per `weaver-harness-Spec` section 6.1.
    let restored = receive.recv().expect("the restored ask arrived second");
    assert!(restored.starts_with(r#"{"ask":{"restored""#), "{restored}");
    let distilled = receive.recv().expect("the load event distilled");
    let frame: serde_json::Value = serde_json::from_str(&distilled).expect("frame parses");
    assert_eq!(frame["envelope"]["kind"], "load");
    assert_eq!(frame["envelope"]["session"], "s-election");
    let _ = std::fs::remove_dir_all(&dir);
}

/// **A refused enter leaves its reason in the run's record.** Before
/// 2026-08-22 a lifecycle refusal reached admin's answer and nothing
/// else, so a trace showed a run that loaded and then stopped, with the
/// reason living only in an answer no reader of the record holds.
///
/// The run here is the one a real after-load refusal produces, taken
/// from the enter path rather than assembled, so the record it carries
/// is the record that would have been written.
///
/// **It pins the shape and the call site** (#99 area 3, P15): the enter
/// is driven through `dispatch_on`, the arm the serve loop takes, so a
/// lifecycle refusal reaching a standing run becomes a `refusal` naming
/// the seam and the ask, carrying the seam's own case, belonging to no
/// turn and closing nothing, and is answered to the caller after it is
/// clerked. Perturbation: remove `author_lifecycle_refusal` from the
/// after-load arm and no refusal is on the record.
#[test]
fn a_refused_enter_leaves_its_reason_in_the_record() {
    let dir = crate::scratch::dir(format!(
        "weaver-refused-enter-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let listener = crate::channel::bind_coordination(&dir.join("coordination.sock")).expect("bind");
    let sink = OwnedFd::from(File::create(dir.join("trace.ndjson")).expect("sink"));
    let mut harness = Harness {
        coordination: listener,
        organs: OrganBinaries {
            classify: None,
            spu: "/nonexistent/weaver-spu".into(),
            gate: "/nonexistent/weaver-gate".into(),
        },
        parameters: OrganParameters::default(),
        state: ChannelState::BeforeEnter,
        composer: Some(weaver_trace::LoopIdentity::compiled("test")),
    };
    let payload = weaver_types::EnterPayload {
        session: SessionId("s-refused".into()),
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
            shell: None,
        },
        state_store: weaver_types::StateStore::default(),
        declaration: String::new(),
        restore: None,
        reset: None,
        stack: Default::default(),
        boundary: String::new(),
        cause: weaver_types::Cause { uid: 0 },
        operator: 1000,
        library_path: None,
        drain_bound: None,
        wind_down_bound: None,
        state_election: weaver_types::StateElection {
            all_kinds: false,
            keys: Vec::new(),
        },
    };
    // The fan-out fails after-load at the SPU exec, which is the arm
    // this test exists for: the bracket stands and the refusal is the
    // enter's own, answered to the caller.
    let (harness_end, peer_end) = OrganChannel::pair().expect("pair");
    harness
        .dispatch_on(
            &harness_end,
            test_exchange(),
            LifecycleDirective::Enter {
                payload: Box::new(payload),
            },
            Some(sink),
            None,
        )
        .expect("the enter dispatches");
    let refusal = match peer_end.into_channel().recv().expect("answer").payload {
        weaver_types::Payload::Refusal(refusal) => refusal,
        other => panic!("the bogus fan-out is refused after the load, got {other:?}"),
    };
    let ChannelState::Entered(run) = &mut harness.state else {
        panic!("an after-load refusal leaves the run entered for its leave")
    };
    let run = run.as_mut();

    let refusals: Vec<&weaver_trace::Record> = run
        .recorder
        .structure()
        .expect("the serving record")
        .iter()
        .filter(|r| r.kind == Kind::Refusal)
        .collect();
    assert_eq!(refusals.len(), 1, "the refusal reached the run's record");
    let event: serde_json::Value =
        serde_json::from_str(refusals[0].line.as_ref()).expect("the line parses");
    assert_eq!(event["payload"]["seam"], "lifecycle");
    assert_eq!(
        event["payload"]["asked"]["ask"], "enter",
        "the ask is named"
    );
    // **Compared against the refusal itself, not against its shape.** A
    // type check passes for any case the seam could have sent, so it
    // would have held while the record carried the wrong refusal, which
    // is the failure this whole class was written to end.
    assert_eq!(
        event["payload"]["refusal"],
        serde_json::to_value(&refusal).expect("the refusal renders"),
        "the seam's own case travels whole: {event}"
    );
    assert!(event.get("turn").is_none(), "an enter belongs to no turn");

    // **The bracket is still open**, which is the property the shape
    // rests on: a refused enter authors an event inside a standing run
    // and the close belongs to the leave.
    assert_eq!(
        run.recorder
            .structure()
            .expect("the serving record")
            .iter()
            .filter(|r| r.kind == Kind::TurnClosed)
            .count(),
        0,
        "a refused enter closes no turn"
    );

    let _ = leave(run, None, false);
    let _ = std::fs::remove_dir_all(&dir);
}

/// **The seated prefix reaches the record through the real enter path.**
/// The door itself is watched in the authorship suite. What this buys is
/// the call site: that `enter` reads the identity out of the SPU
/// instruction it was handed and authors it, in order, after the load.
/// Without it the door could be correct and never called, which is the
/// shape the defect took before this act, the prefix having been seated
/// at open and written down nowhere.
///
/// The fan-out fails after-load at the SPU exec as its sibling above
/// does, which is what makes the test cheap: the load event and the
/// prefix precede the forks, so no organ has to run for the record to
/// be complete at the point this reads it.
/// **The load names the member it was handed and the composer serve set**,
/// through `enter` itself and not the payload's shape alone, per Spec
/// section 6.1's third route: `state_member` is the presence of the end
/// on this enter, true with one and false without, and `composer` is what
/// the binary handed in, read back with its file and digest so a default
/// could not pass as the handed value.
///
/// Perturbation: author `state_member: false` regardless of the end, or
/// author a compiled composer regardless of what serve set, and this
/// fails on the standing case.
#[test]
fn the_load_names_the_member_it_was_handed_and_the_composer_serve_set() {
    for standing in [false, true] {
        let dir = crate::scratch::dir(format!(
            "weaver-harness-named-{}-{standing}",
            std::process::id()
        ));
        let socket = dir.join("c.sock");
        std::fs::remove_file(&socket).ok();
        let listener = crate::channel::bind_coordination(&socket).expect("bind");
        let sink_path = dir.join("trace.ndjson");
        let sink = OwnedFd::from(File::create(&sink_path).expect("sink"));
        let mut harness = Harness {
            coordination: listener,
            organs: OrganBinaries {
                classify: None,
                spu: "/nonexistent/weaver-spu".into(),
                gate: "/nonexistent/weaver-gate".into(),
            },
            parameters: OrganParameters::default(),
            state: ChannelState::BeforeEnter,
            composer: Some(weaver_trace::LoopIdentity::file(
                "pyworker",
                std::path::Path::new("/deployed/loop.py"),
                Some("cd".repeat(32)),
            )),
        };
        let payload = weaver_types::EnterPayload {
            session: SessionId("s-named".into()),
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
                shell: None,
            },
            state_store: weaver_types::StateStore::default(),
            declaration: String::new(),
            restore: None,
            reset: None,
            stack: Default::default(),
            boundary: String::new(),
            cause: weaver_types::Cause { uid: 0 },
            operator: 1000,
            library_path: None,
            drain_bound: None,
            wind_down_bound: None,
            state_election: weaver_types::StateElection {
                all_kinds: false,
                keys: Vec::new(),
            },
        };
        // The member's end, where it stands: one half of a pair the test
        // holds, the way admin's spawn hands the harness its half, its
        // far end answering the enter's `restored` ask as a member that
        // stood empty.
        let held_end = standing.then(|| {
            let (near, far) = std::os::unix::net::UnixStream::pair().expect("pair");
            std::thread::spawn(move || member_lines(far).count());
            OwnedFd::from(near)
        });
        let state_end = held_end
            .as_ref()
            .map(|near| near.try_clone().expect("clone"));
        let mut run = match harness.enter(payload, Some(sink), state_end) {
            Err(EnterFailure::AfterLoad(run, _)) => run,
            Ok(_) => panic!("the bogus fan-out cannot succeed"),
            Err(EnterFailure::BeforeLoad(refusal)) => {
                panic!("failed before the load: {refusal:?}")
            }
        };
        let _ = leave(&mut run, None, false);
        drop(run);
        let held = std::fs::read_to_string(&sink_path).expect("the sink reads back");
        let load: serde_json::Value =
            serde_json::from_str(held.lines().next().expect("a load")).expect("parses");
        assert_eq!(load["kind"], "load");
        assert_eq!(
            load["payload"]["state_member"],
            serde_json::json!(standing),
            "the member's standing is the end's presence on this enter"
        );
        assert_eq!(load["payload"]["composer"]["binary"], "pyworker");
        assert_eq!(load["payload"]["composer"]["file"], "/deployed/loop.py");
        assert_eq!(load["payload"]["composer"]["sha256"], "cd".repeat(32));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// **An enter naming a lineage with no state end refuses before the
/// load** (#99, K8; survey S19): with no member standing, nothing can
/// have restored the save point the enter names, so the enter refuses
/// `DescriptorsUnusable` with the stream still clean, where before it
/// skipped `restored_agrees` and authored a `load` naming the lineage.
///
/// Perturbation: drop the `payload.restore.is_some()` arm and the enter
/// reaches the fan-out after the load.
#[test]
fn an_enter_naming_a_lineage_with_no_member_refuses_before_the_load() {
    let dir = crate::scratch::dir(format!(
        "weaver-harness-lineage-no-member-{}",
        std::process::id()
    ));
    let socket = dir.join("c.sock");
    std::fs::remove_file(&socket).ok();
    let listener = crate::channel::bind_coordination(&socket).expect("bind");
    let sink_path = dir.join("trace.ndjson");
    let sink = OwnedFd::from(File::create(&sink_path).expect("sink"));
    let mut harness = Harness {
        coordination: listener,
        organs: OrganBinaries {
            classify: None,
            spu: "/nonexistent/weaver-spu".into(),
            gate: "/nonexistent/weaver-gate".into(),
        },
        parameters: OrganParameters::default(),
        state: ChannelState::BeforeEnter,
        composer: Some(weaver_trace::LoopIdentity::file(
            "pyworker",
            std::path::Path::new("/deployed/loop.py"),
            Some("cd".repeat(32)),
        )),
    };
    let payload = weaver_types::EnterPayload {
        session: SessionId("s-lineage".into()),
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
            shell: None,
        },
        state_store: weaver_types::StateStore::default(),
        declaration: String::new(),
        restore: Some(lineage("ab", "r-0", 5, 1)),
        reset: None,
        stack: Default::default(),
        boundary: String::new(),
        cause: weaver_types::Cause { uid: 0 },
        operator: 1000,
        library_path: None,
        drain_bound: None,
        wind_down_bound: None,
        state_election: weaver_types::StateElection {
            all_kinds: false,
            keys: Vec::new(),
        },
    };
    let refusal = match harness.enter(payload, Some(sink), None) {
        Err(EnterFailure::BeforeLoad(refusal)) => refusal,
        Err(EnterFailure::AfterLoad(mut run, _)) => {
            let _ = leave(&mut run, None, false);
            panic!("a lineage with no member reached the load");
        }
        Ok(_) => panic!("the bogus fan-out cannot succeed"),
    };
    assert!(
        matches!(refusal, LifecycleRefusal::DescriptorsUnusable),
        "{refusal:?}"
    );
    let held = std::fs::read_to_string(&sink_path).expect("the sink reads back");
    assert!(held.is_empty(), "the stream stays clean: {held}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A scripted member's far end that stood empty: every line the harness
/// writes is yielded in order, the `restored` ask answered as empty and
/// the `grants` ask with an empty surface, until the harness closes the
/// channel.
fn member_lines(far: std::os::unix::net::UnixStream) -> impl Iterator<Item = String> {
    use std::io::{BufRead, BufReader, Write};
    let mut answers = far.try_clone().expect("clone");
    BufReader::new(far).lines().map_while(move |line| {
        let line = line.ok()?;
        let answer = if line.starts_with(r#"{"ask":{"restored""#) {
            Some(EMPTY_RESTORED)
        } else if line.starts_with(r#"{"ask":{"grants""#) {
            Some(r#"{"answer":{"grants":{"surface":[]}}}"#)
        } else {
            None
        };
        if let Some(answer) = answer {
            let _ = answers.write_all(format!("{answer}\n").as_bytes());
        }
        Some(line)
    })
}

/// Enters a serving run against a scripted state member, one half of a
/// socket pair the way admin's spawn hands the harness its end, and
/// returns the record the enter left and, where the enter refused before
/// the load, that refusal. The member answers the grants ask with an
/// empty surface, the identity ask with one seated prefix event, the
/// restored ask with the answer given, the snapshot ask with one stamp,
/// and drains the tee's traffic otherwise, logging every line it reads in
/// order so a test can read the asks against the distillates. The fan-out
/// fails past the load, as every enter test's bogus organ paths make it.
fn enter_against_a_member(
    restore: Option<weaver_types::Lineage>,
    diagnostic: bool,
) -> Vec<serde_json::Value> {
    let (events, refusal, _) =
        enter_against_a_member_answering(restore, diagnostic, EMPTY_RESTORED);
    assert!(
        refusal.is_none(),
        "the enter stood past the load: {refusal:?}"
    );
    events
}

/// The restored answer of a member that stood empty.
const EMPTY_RESTORED: &str = r#"{"answer":{"restored":{}}}"#;

fn enter_against_a_member_answering(
    restore: Option<weaver_types::Lineage>,
    diagnostic: bool,
    restored_answer: &'static str,
) -> (
    Vec<serde_json::Value>,
    Option<LifecycleRefusal>,
    Vec<String>,
) {
    let (events, refusal, read, _, _) =
        enter_against_a_member_leaving(restore, diagnostic, restored_answer, LeaveMode::SelfUnwind);
    (events, refusal, read)
}

/// How the entered run is left: unwound by the harness itself, as an
/// after-load failure is, or through the `Leave` directive on the
/// coordination channel, forced or not, against a member that answers
/// the acknowledgement with `finished` or never does.
#[derive(Clone, Copy)]
enum LeaveMode {
    SelfUnwind,
    Directive {
        forced: bool,
        finished: bool,
    },
    /// The `SavePoint` directive against the entered run, then the
    /// fixture's own unwind: the answer captured is the save point's.
    SavePoint,
    /// As `SavePoint`, with admin's end of the connection closed before
    /// the directive is dispatched: the caller went away mid-verb.
    SavePointAbandoned,
    /// An unforced `Leave` with a frame the gate stand-in queued before
    /// it, untaken by the loop (Codex on #94, round 13).
    QueuedFrame,
    /// An unforced `Leave` whose gate stand-in has a frame it admitted
    /// standing ahead of `GateStopped` when it takes the `Lower` (Codex
    /// on #94, rounds 14 and 15).
    FrameDuringLower,
    /// A load's rollback: `Leave { forced: true, rollback: true }`.
    Rollback,
    /// An unforced `Leave` whose gate stand-in refuses the `Lower` and whose
    /// `finished` answer never comes, so the leave stops in S9; then a
    /// forced `Leave` from S9, whose answer is the one captured (#140, S15).
    RefusedLowerThenForced,
}

/// The enter against a member stub, left as `mode` says. Answers the
/// record's events, the enter's refusal where it refused before the
/// load, every line the member read, the leave's answer or refusal
/// where the directive drove it, and whether the harness still stands
/// entered after it.
fn enter_against_a_member_leaving(
    restore: Option<weaver_types::Lineage>,
    diagnostic: bool,
    restored_answer: &'static str,
    mode: LeaveMode,
) -> (
    Vec<serde_json::Value>,
    Option<LifecycleRefusal>,
    Vec<String>,
    Option<weaver_types::Payload>,
    bool,
) {
    use std::io::{BufRead, BufReader, Write};
    let finished_answered = !matches!(
        mode,
        LeaveMode::Directive {
            finished: false,
            ..
        } | LeaveMode::RefusedLowerThenForced
    );

    let dir = crate::scratch::dir(format!(
        "weaver-enter-ask-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let listener = crate::channel::bind_coordination(&dir.join("coordination.sock")).expect("bind");
    let sink_path = dir.join("trace.ndjson");
    let sink = OwnedFd::from(File::create(&sink_path).expect("sink"));
    let mut harness = Harness {
        coordination: listener,
        organs: OrganBinaries {
            classify: None,
            spu: "/nonexistent/weaver-spu".into(),
            gate: "/nonexistent/weaver-gate".into(),
        },
        parameters: OrganParameters::default(),
        state: ChannelState::BeforeEnter,
        composer: Some(weaver_trace::LoopIdentity::compiled("test")),
    };
    let payload = weaver_types::EnterPayload {
        session: SessionId("s-1".into()),
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
                tunable_values: Default::default(),
            },
        },
        binding: if diagnostic {
            weaver_types::EnterBinding::Diagnostic
        } else {
            weaver_types::EnterBinding::Serving {
                gate_instruction: weaver_types::GateInstruction {
                    access_rule: weaver_types::AccessRule {
                        allowed_uids: Default::default(),
                        allowed_gids: Default::default(),
                        denied_uids: Default::default(),
                    },
                },
                shell: None,
            }
        },
        state_store: weaver_types::StateStore::default(),
        declaration: String::new(),
        restore,
        reset: None,
        stack: Default::default(),
        boundary: String::new(),
        cause: weaver_types::Cause { uid: 0 },
        operator: 1000,
        library_path: None,
        drain_bound: None,
        wind_down_bound: None,
        // Every kind crosses, so the member's log shows the unload's
        // distillate beside the asks.
        state_election: weaver_types::StateElection {
            all_kinds: true,
            keys: Vec::new(),
        },
    };
    let (near, far) = std::os::unix::net::UnixStream::pair().expect("pair");
    // Every line the member reads and every directive the gate stand-in
    // takes, in arrival order, so a test reads the leave's sequence
    // across the two organs.
    let order: std::sync::Arc<std::sync::Mutex<Vec<String>>> = Default::default();
    let member_order = order.clone();
    let member = std::thread::spawn(move || {
        let mut answers = far.try_clone().expect("clone");
        let mut read: Vec<String> = Vec::new();
        let mut echoed: String;
        for line in BufReader::new(far).lines() {
            let Ok(line) = line else { break };
            read.push(line.clone());
            member_order.lock().unwrap().push(line.clone());
            let answer = if line.starts_with(r#"{"ask":{"grants""#) {
                r#"{"answer":{"grants":{"surface":[]}}}"#
            } else if line.starts_with(r#"{"ask":{"identity""#) {
                concat!(
                    r#"{"answer":{"identity":{"messages":[{"envelope":{"session":"s-0","#,
                    r#""run":"r-0","kind":"message.system","sequence":"3"},"#,
                    r#""pairs":{"role":"system","content":[{"type":"text","#,
                    r#""text":"You are Karl."}]}}]}}}"#
                )
            } else if line.starts_with(r#"{"ask":{"restored""#) {
                restored_answer
            } else if line.starts_with(r#"{"ask":{"snapshot""#) {
                // The answer echoes the ask's ordinal (Codex on #94,
                // round 10).
                let ordinal = serde_json::from_str::<serde_json::Value>(&line)
                    .ok()
                    .and_then(|v| v["ask"]["snapshot"]["ask"].as_u64())
                    .expect("the snapshot ask carries its ordinal");
                echoed = format!(
                    concat!(
                        r#"{{"answer":{{"snapshot":{{"ask":{},"save-point":"ab.save-point","run":"r-0","#,
                        r#""sequence":5,"turn":1,"digest":"ab"}}}}}}"#
                    ),
                    ordinal
                );
                echoed.as_str()
            } else if line.starts_with(r#"{"acknowledge":{"snapshot""#) {
                if !finished_answered {
                    continue;
                }
                let ordinal = serde_json::from_str::<serde_json::Value>(&line)
                    .ok()
                    .and_then(|v| v["acknowledge"]["snapshot"]["ask"].as_u64())
                    .expect("the acknowledgement carries its ordinal");
                echoed = format!(
                    r#"{{"answer":{{"finished":{{"ask":{},"save-point":"ab.save-point"}}}}}}"#,
                    ordinal
                );
                echoed.as_str()
            } else {
                continue;
            };
            if answers.write_all(format!("{answer}\n").as_bytes()).is_err() {
                break;
            }
        }
        read
    });
    let mut answer = None;
    let mut still_entered = false;
    let mut gate_stand_in: Option<std::thread::JoinHandle<()>> = None;
    let refusal = match harness.enter(payload, Some(sink), Some(OwnedFd::from(near))) {
        Err(EnterFailure::AfterLoad(run, _)) => {
            match mode {
                LeaveMode::SelfUnwind => {
                    let mut run = *run;
                    let _ = leave(&mut run, None, false);
                    drop(run);
                }
                LeaveMode::Directive { .. }
                | LeaveMode::SavePoint
                | LeaveMode::SavePointAbandoned
                | LeaveMode::QueuedFrame
                | LeaveMode::FrameDuringLower
                | LeaveMode::Rollback
                | LeaveMode::RefusedLowerThenForced => {
                    // The fixture's SPU never admitted, its exec having
                    // failed: its dead arm is dropped and reaped here so
                    // the leave's answer is the save point's and not the
                    // release's refusal.
                    let mut run = run;
                    // **A gate stand-in**, so the leave's lower is an
                    // exchange the sequence records (Codex on #94, round
                    // 12): it answers `GateStopped` to the `Lower` and
                    // marks the order; a child stands in for its pid. It
                    // keeps the real gate's order (Codex on #94, round
                    // 15): at the `Lower` the gate drops its relay and
                    // every served connection (`weaver-gate/src/main.rs`,
                    // the `(Raised, Lower)` arm at line 550, `drop(relay)`
                    // at line 559) and then answers `GateStopped` (line
                    // 562), never waiting on a response, and anything
                    // the harness writes for a frame after that is
                    // discarded, which the stand-in marks.
                    let (gate_near, gate_far) =
                        crate::channel::OrganChannel::pair().expect("gate pair");
                    // Reaped by pid in `lower_gate`, as the real gate is.
                    #[allow(clippy::zombie_processes)]
                    let gate_child = std::process::Command::new("true")
                        .spawn()
                        .expect("a child to reap");
                    run.gate = Some(GateChannel {
                        channel: gate_near,
                        pid: nix::unistd::Pid::from_raw(gate_child.id() as i32),
                        last_word: crate::spawn::LastWord::quiet(),
                        shell_granted: true,
                    });
                    let gate_order = order.clone();
                    let queue_frame = matches!(mode, LeaveMode::QueuedFrame);
                    let frame_during_lower = matches!(mode, LeaveMode::FrameDuringLower);
                    let refuse_lower = matches!(mode, LeaveMode::RefusedLowerThenForced);
                    gate_stand_in = Some(std::thread::spawn(move || {
                        let channel = gate_far.into_channel();
                        if queue_frame {
                            // Admitted by the gate and sent before the
                            // leave arrives, untaken by the loop.
                            channel
                                .send(&OrganEnvelope {
                                    exchange: ExchangeId {
                                        opener: Opener::Gate,
                                        ordinal: 1,
                                    },
                                    position: Position::Open,
                                    payload: weaver_types::Payload::Frame(
                                        weaver_types::TurnFrame::carry(b"{\"text\":\"late\"}"),
                                    ),
                                })
                                .expect("the frame queues");
                        }
                        // **The drain comes first on a graceful leave**:
                        // the quiesce is answered `GateQuiesced`, and each
                        // refusal the harness answers a frame with is
                        // marked, until the `Lower`.
                        let mut lowering = None;
                        while let Ok(envelope) = channel.recv() {
                            match &envelope.payload {
                                weaver_types::Payload::Directive(LifecycleDirective::Quiesce) => {
                                    gate_order.lock().unwrap().push("<gate quiesced>".into());
                                    let _ = channel.send(&OrganEnvelope {
                                        exchange: envelope.exchange.clone(),
                                        position: Position::Close,
                                        payload: weaver_types::Payload::Answer(
                                            LifecycleAnswer::GateQuiesced,
                                        ),
                                    });
                                }
                                weaver_types::Payload::Directive(LifecycleDirective::Lower) => {
                                    lowering = Some(envelope);
                                    break;
                                }
                                weaver_types::Payload::Frame(frame) => {
                                    let line = frame
                                        .octets()
                                        .map(|octets| String::from_utf8_lossy(&octets).into_owned())
                                        .unwrap_or_default();
                                    gate_order.lock().unwrap().push(format!(
                                        "<answered {}> {line}",
                                        envelope.exchange.ordinal
                                    ));
                                }
                                other => {
                                    gate_order
                                        .lock()
                                        .unwrap()
                                        .push(format!("<other> {other:?}"));
                                }
                            }
                        }
                        if let Some(envelope) = lowering {
                            assert!(matches!(
                                envelope.payload,
                                weaver_types::Payload::Directive(LifecycleDirective::Lower)
                            ));
                            gate_order.lock().unwrap().push("<gate lowered>".into());
                            if frame_during_lower {
                                // The relay's send of a line it admitted
                                // stands ahead of the answer.
                                channel
                                    .send(&OrganEnvelope {
                                        exchange: ExchangeId {
                                            opener: Opener::Gate,
                                            ordinal: 7,
                                        },
                                        position: Position::Open,
                                        payload: weaver_types::Payload::Frame(
                                            weaver_types::TurnFrame::carry(b"{\"text\":\"late\"}"),
                                        ),
                                    })
                                    .expect("the frame sends");
                            }
                            // The relay is dropped, then the answer, with
                            // no wait on any response.
                            let _ = channel.send(&OrganEnvelope {
                                exchange: envelope.exchange,
                                position: Position::Close,
                                payload: if refuse_lower {
                                    weaver_types::Payload::Refusal(LifecycleRefusal::Malformed)
                                } else {
                                    weaver_types::Payload::Answer(LifecycleAnswer::GateStopped)
                                },
                            });
                            // What the harness writes now reaches a gate
                            // with no connection to carry it.
                            while let Ok(late) = channel.recv() {
                                gate_order.lock().unwrap().push(format!(
                                    "<undeliverable> exchange {}",
                                    late.exchange.ordinal
                                ));
                                // A second `Lower` is answered, so a
                                // harness that lowers twice finishes and
                                // its test reads the mark, not a hang.
                                if matches!(
                                    late.payload,
                                    weaver_types::Payload::Directive(LifecycleDirective::Lower)
                                ) {
                                    let _ = channel.send(&OrganEnvelope {
                                        exchange: late.exchange,
                                        position: Position::Close,
                                        payload: weaver_types::Payload::Answer(
                                            LifecycleAnswer::GateStopped,
                                        ),
                                    });
                                }
                            }
                        }
                    }));
                    if let Some(spu) = run.spu.take() {
                        drop(spu.decode);
                        drop(spu.lifecycle);
                        reap(spu.pid);
                    }
                    // **A standing SPU in its place**, answering its
                    // release: an empty SPU slot is a dead SPU (3.0
                    // ruling (A)), which would turn these leaves forced.
                    let (lifecycle, spu_far) = OrganChannel::pair().expect("spu pair");
                    let (decode, decode_far) = DecodeChannel::pair().expect("decode pair");
                    run.spu = Some(SpuChannels {
                        lifecycle,
                        decode,
                        pid: nix::unistd::Pid::from_raw(i32::MAX),
                        last_word: crate::spawn::LastWord::quiet(),
                    });
                    let spu_far = spu_far.into_channel();
                    // The SPU answers the wind-down's one generation,
                    // marking it in the order.
                    let decode_order = order.clone();
                    answer_appends(decode_far, move || {
                        decode_order.lock().unwrap().push("<wind-down>".into())
                    });
                    let release_order = order.clone();
                    std::thread::spawn(move || {
                        while let Ok(asked) = spu_far.recv() {
                            release_order.lock().unwrap().push("<spu released>".into());
                            let _ = spu_far.send(&OrganEnvelope {
                                exchange: asked.exchange,
                                position: Position::Close,
                                payload: weaver_types::Payload::Answer(LifecycleAnswer::Released),
                            });
                        }
                    });
                    // **A diagnostic binding has no gate** (3.0: its
                    // Gate column reads absent), so the stand-in is
                    // dropped there, meeting its channel's close.
                    if diagnostic {
                        run.gate = None;
                    }
                    harness.state = ChannelState::Entered(run);
                    let (connection, admin_peer) = OrganChannel::pair().expect("leave pair");
                    let admin_peer = admin_peer.into_channel();
                    if queue_frame {
                        // The frame must stand in the channel before the
                        // leave is dispatched: the stand-in sends it
                        // first thing, and the wait here is for its
                        // arrival, not a sleep for luck.
                        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
                        while std::time::Instant::now() < until
                            && !matches!(&harness.state, ChannelState::Entered(run)
                                if run.gate.as_ref().is_some_and(|gate| channel_has_pending(&gate.channel)))
                        {
                            std::thread::sleep(std::time::Duration::from_millis(5));
                        }
                    }
                    let directive = match mode {
                        LeaveMode::Directive { forced, .. } => LifecycleDirective::Leave {
                            cause: weaver_types::Cause { uid: 1000 },
                            forced,
                            rollback: false,
                        },
                        LeaveMode::Rollback => LifecycleDirective::Leave {
                            cause: weaver_types::Cause { uid: 1000 },
                            forced: true,
                            rollback: true,
                        },
                        LeaveMode::QueuedFrame
                        | LeaveMode::FrameDuringLower
                        | LeaveMode::RefusedLowerThenForced => LifecycleDirective::Leave {
                            cause: weaver_types::Cause { uid: 1000 },
                            forced: false,
                            rollback: false,
                        },
                        _ => LifecycleDirective::SavePoint {
                            cause: weaver_types::Cause { uid: 1000 },
                        },
                    };
                    if matches!(mode, LeaveMode::SavePointAbandoned) {
                        drop(admin_peer);
                        harness
                            .dispatch_on(&connection, test_exchange(), directive, None, None)
                            .expect("an undelivered answer is no fault of the run");
                    } else {
                        harness
                            .dispatch_on(&connection, test_exchange(), directive, None, None)
                            .expect("the directive dispatches");
                        answer = Some(admin_peer.recv().expect("the leave answers").payload);
                    }
                    if matches!(mode, LeaveMode::RefusedLowerThenForced) {
                        assert_eq!(
                            answer,
                            Some(weaver_types::Payload::Refusal(
                                LifecycleRefusal::SavePointNotTaken {
                                    missed: weaver_types::SavePointLeg::Finished,
                                }
                            )),
                            "the graceful leave stops in S9"
                        );
                        let (connection, admin_peer) = OrganChannel::pair().expect("force pair");
                        let admin_peer = admin_peer.into_channel();
                        harness
                            .dispatch_on(
                                &connection,
                                test_exchange(),
                                LifecycleDirective::Leave {
                                    cause: weaver_types::Cause { uid: 1000 },
                                    forced: true,
                                    rollback: false,
                                },
                                None,
                                None,
                            )
                            .expect("the force dispatches");
                        answer = Some(admin_peer.recv().expect("the force answers").payload);
                    }
                    still_entered = matches!(harness.state, ChannelState::Entered(_));
                    if let ChannelState::Entered(run) =
                        std::mem::replace(&mut harness.state, ChannelState::Left)
                    {
                        let mut run = *run;
                        let _ = leave(&mut run, None, false);
                        drop(run);
                    }
                }
            }
            None
        }
        Ok(_) => panic!("the bogus fan-out cannot succeed"),
        Err(EnterFailure::BeforeLoad(refusal)) => Some(refusal),
    };
    drop(harness);
    member
        .join()
        .expect("the member finishes when the channel closes");
    if let Some(gate) = gate_stand_in {
        gate.join()
            .expect("the gate stand-in finishes when its channel closes");
    }
    // The member's lines and the gate's marks, in arrival order.
    let read = order.lock().unwrap().clone();
    let held = std::fs::read_to_string(&sink_path).expect("the sink reads back");
    let _ = std::fs::remove_dir_all(&dir);
    let events = held
        .lines()
        .map(|line| serde_json::from_str(line).expect("each line parses"))
        .collect();
    (events, refusal, read, answer, still_entered)
}

/// **A serving enter asks `restored` before it authors `load`, then asks
/// `identity`, records the answer as a `recall` and writes no prefix**,
/// per `weaver-harness-Spec` section 6.1 and `weaver-harness-state-contract`
/// section 2 on the operator's rulings of 2026-10-06 that the system
/// prompt is state and that a session's trace and the state it loaded
/// from are not tied together: the member reads the restored ask before
/// any distillate of the load event, the identity ask follows, the answer
/// is recorded as a `recall`, and the record carries no `message.system`,
/// the prefix the open seats being the member's and its text standing
/// where it was seeded. **This is what makes a later seeding replace**:
/// the member serves the newest run holding a turnless `message.system`,
/// so a load that wrote the old prefix into the new run would have the
/// next load serve old and new both.
///
/// Perturbations: author `load` before the restored ask and the first
/// assertion fails; skip the identity ask under a serving binding and the
/// second fails; drop the `record_enter_ask` call and the recall
/// assertion fails; author the seated prefix at the load again and the
/// last assertion fails, the replace broken. Watched under each.
#[test]
fn a_serving_enter_asks_restored_before_load_records_the_recall_and_writes_no_prefix() {
    let (events, refusal, read) = enter_against_a_member_answering(None, false, EMPTY_RESTORED);
    assert!(refusal.is_none(), "{refusal:?}");
    let at = |needle: &str| {
        read.iter()
            .position(|line| line.contains(needle))
            .unwrap_or_else(|| panic!("{needle} reached the member: {read:?}"))
    };
    assert!(
        at(r#"{"ask":{"restored""#) < at(r#""kind":"load""#),
        "the restored ask precedes the load's distillate: {read:?}"
    );
    assert!(
        at(r#""kind":"load""#) < at(r#"{"ask":{"identity""#),
        "the identity ask follows the load, before the open: {read:?}"
    );
    let kinds: Vec<&str> = events.iter().map(|e| e["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds[0], "load", "the load opens the run");
    assert_eq!(
        kinds[1], "recall",
        "the answered ask is recorded before the open is built from it: {kinds:?}"
    );
    assert_eq!(
        events[1]["payload"]["ask"]["verb"], "identity",
        "the recall names its verb: {}",
        events[1]
    );
    assert!(
        !kinds.contains(&"message.restored"),
        "no restored conversation: {kinds:?}"
    );
    assert!(
        !kinds.contains(&"message.system"),
        "the load writes no prefix, the member's answer being seated and not re-authored: {kinds:?}"
    );
    assert!(
        !events
            .iter()
            .any(|e| e.to_string().contains("You are Karl.")),
        "the prefix's text is on the trace where it was seeded and nowhere else: {events:?}"
    );
    assert!(
        events[0]["payload"].get("lineage").is_none(),
        "a load from nothing names no lineage: {}",
        events[0]
    );
}

/// **A restored answer that disagrees refuses the enter before the load,
/// and one that agrees lands the lineage on the load event**, per
/// `weaver-harness-Spec` section 6.1: a miss, a refusal, an empty answer
/// under a lineage, and a stamp that differs in any of its four members
/// each refuse with the stream still clean, and the agreeing stamp's
/// enter authors `load` naming the lineage's six members.
///
/// Perturbation: author `load` whatever `restored_agrees` answers and the
/// refused records hold a load line. Watched under exactly that change.
#[test]
fn a_restored_disagreement_refuses_before_the_load() {
    let named = || Some(lineage("ab", "r-0", 5, 1));
    for (answer, why) in [
        (
            r#"{"answer":{"restored":{"refused":"schema-mismatch"}}}"#,
            "a refusal",
        ),
        (EMPTY_RESTORED, "an empty answer under a lineage"),
        (
            r#"{"answer":{"restored":{"lineage":{"digest":"zz","run":"r-0","sequence":5,"turn":1}}}}"#,
            "another digest",
        ),
        (
            r#"{"answer":{"restored":{"lineage":{"digest":"ab","run":"r-0","sequence":6,"turn":1}}}}"#,
            "another sequence",
        ),
        (
            r#"{"answer":{"shape":{"runs":[]}}}"#,
            "a malformed answer, the miss",
        ),
    ] {
        let (events, refusal, _) = enter_against_a_member_answering(named(), false, answer);
        assert!(
            matches!(refusal, Some(LifecycleRefusal::DescriptorsUnusable)),
            "{why} refuses: {refusal:?}"
        );
        assert!(
            events.is_empty(),
            "{why} leaves the stream clean: {events:?}"
        );
    }
    let (events, refusal, _) = enter_against_a_member_answering(
        Some(lineage("ab", "r-0", 5, 1)),
        false,
        r#"{"answer":{"restored":{"lineage":{"digest":"ab","run":"r-0","sequence":5,"turn":1}}}}"#,
    );
    assert!(refusal.is_none(), "an agreeing stamp admits: {refusal:?}");
    assert_eq!(events[0]["kind"], "load");
    assert_eq!(
        events[0]["payload"]["lineage"],
        serde_json::json!({
            "save_point": "ab", "run": "r-0", "sequence": 5, "turn": 1,
            "named_at_restore": false
        }),
        "the load names the lineage: {}",
        events[0]
    );
    let (_, refusal, _) = enter_against_a_member_answering(
        None,
        false,
        r#"{"answer":{"restored":{"lineage":{"digest":"ab","run":"r-0","sequence":5,"turn":1}}}}"#,
    );
    assert!(
        matches!(refusal, Some(LifecycleRefusal::DescriptorsUnusable)),
        "a restored save point the enter does not name refuses: {refusal:?}"
    );
}

/// **The leave takes its save point first, through four legs, records it
/// and names it on `Left`**, per `weaver-harness-Spec` section 6,
/// `weaver-harness-state-contract` section 2 and `weaver-trace-Spec`
/// section 3 on the operator's rulings of 2026-10-06 on #1: the member
/// reads the snapshot ask, then the acknowledgement naming the digest,
/// before the unload's distillate; the record carries `save_point`
/// before `unload`, on the `finished` answer, with the stamp; `Left`
/// carries the report with the event's position; the `unload` event is
/// not forced; and the tee never sends the save point's event to state.
///
/// Perturbations: send the snapshot ask after authoring `unload` and
/// the first assertion fails; author `save_point` on the snapshot answer
/// instead of the `finished` answer and the acknowledgement's ordering
/// assertion fails; drop the kind check from the tee's `distill` and the
/// member reads the save point's distillate, the last assertion failing.
#[test]
fn the_leave_takes_the_save_point_before_unload_and_records_it() {
    let (events, refusal, read, answer, still_entered) = enter_against_a_member_leaving(
        None,
        false,
        EMPTY_RESTORED,
        LeaveMode::Directive {
            forced: false,
            finished: true,
        },
    );
    assert!(refusal.is_none(), "{refusal:?}");
    assert!(!still_entered, "the leave completed");
    let at = |needle: &str| {
        read.iter()
            .position(|line| line.contains(needle))
            .unwrap_or_else(|| panic!("{needle} reached the member: {read:?}"))
    };
    assert!(
        at("<gate quiesced>") < at("<wind-down>"),
        "the drain before the wind-down: {read:?}"
    );
    assert!(
        at("<wind-down>") < at("<gate lowered>"),
        "the wind-down before the lower (3.0, graceful steps 2 to 5): {read:?}"
    );
    let held: Vec<String> = events.iter().map(|e| e.to_string()).collect();
    assert!(
        held.iter()
            .any(|line| line.contains("The agent is unloading now. Summarize the work so far")),
        "the wind-down's request is the constant, on the record: {held:?}"
    );
    assert!(
        held.iter().any(|line| line.contains("the summary")),
        "the summary is on the record: {held:?}"
    );
    assert!(
        at("<gate lowered>") < at(r#"{"ask":{"snapshot""#),
        "the gate is lowered before the leave's save point is taken (Codex on #94, round 12): {read:?}"
    );
    assert!(
        at(r#"{"ask":{"snapshot""#) < at(r#"{"acknowledge":{"snapshot":{"ask":1,"digest":"ab"}}"#),
        "the acknowledgement names the answered digest: {read:?}"
    );
    assert!(
        at(r#"{"acknowledge":{"snapshot""#) < at(r#""kind":"unload""#),
        "the four legs precede the unload's distillate: {read:?}"
    );
    let kinds: Vec<&str> = events.iter().map(|e| e["kind"].as_str().unwrap()).collect();
    let unload = kinds
        .iter()
        .position(|k| *k == "unload")
        .expect("an unload");
    let save_point = kinds
        .iter()
        .position(|k| *k == "save_point")
        .expect("a save point");
    assert!(save_point < unload, "{kinds:?}");
    assert!(events[save_point].get("turn").is_none(), "turnless");
    assert_eq!(
        events[save_point]["payload"],
        serde_json::json!({
            "save_point": "ab", "run": "r-0", "sequence": 5, "turn": 1, "name": "ab.save-point",
            "cause": {"uid": 1000}
        })
    );
    assert_eq!(
        events[unload]["payload"]["forced"], false,
        "{}",
        events[unload]
    );
    let position = events[save_point]["sequence"]
        .as_str()
        .expect("the sequence")
        .parse::<u64>()
        .expect("a number");
    assert_eq!(
        answer,
        Some(weaver_types::Payload::Answer(LifecycleAnswer::Left {
            save_point: Some(weaver_types::SavePointReport {
                save_point: "ab".into(),
                name: "ab.save-point".into(),
                run: weaver_types::RunId("r-0".into()),
                sequence: 5,
                turn: 1,
                event_run: weaver_types::RunId("r-1".into()),
                position,
            }),
            forced: false,
            no_state: false,
        })),
        "Left names the leave's save point with the event's position in this run, the covered one in the prior"
    );
    assert!(
        !read
            .iter()
            .any(|line| line.contains(r#""kind":"save_point""#)),
        "the save point's event never crosses the tee: {read:?}"
    );
}

/// **A run with no state to keep says so on `Left`**, per
/// `weaver-types-Spec` section 4.2 (`Left.no_state`): a diagnostic
/// binding has no member, so its leave answers `no_state` true with no save
/// point, where the serving cases above answer it false. Perturbations:
/// answer `no_state` false always and this case fails; true always and
/// the serving cases fail. **It runs no wind-down and no gate step**
/// (`weaver-harness-Spec` section 6, item 3; 3.0, a diagnostic binding
/// goes from S4 to S6): run the wind-down regardless, and the SPU
/// stand-in marks one.
#[test]
fn a_diagnostic_runs_leave_answers_no_state() {
    for forced in [false, true] {
        let (_, _, read, answer, still_entered) = enter_against_a_member_leaving(
            None,
            true,
            EMPTY_RESTORED,
            LeaveMode::Directive {
                forced,
                finished: true,
            },
        );
        assert!(!still_entered, "forced = {forced}: the leave completes");
        assert_eq!(
            answer,
            Some(weaver_types::Payload::Answer(LifecycleAnswer::Left {
                save_point: None,
                forced,
                no_state: true,
            })),
            "forced = {forced}"
        );
        assert!(
            !read
                .iter()
                .any(|line| line == "<wind-down>" || line.starts_with("<gate")),
            "forced = {forced}: no wind-down and no gate step: {read:?}"
        );
    }
}

/// **A save point demanded of a run that takes none is out of order**,
/// per `weaver-harness-Spec` section 6 and
/// `weaver-admin-harness-contract`: a diagnostic binding has no member
/// to ask, so the `SavePoint` directive refuses `OutOfOrder`, asks no
/// snapshot and records nothing, the run standing. Perturbation: refuse
/// `SavePointNotTaken { missed: MemberDead }` again and the answer
/// reports a healthy member dead.
#[test]
fn a_save_point_demanded_of_a_diagnostic_run_is_out_of_order() {
    let (events, _, read, answer, still_entered) =
        enter_against_a_member_leaving(None, true, EMPTY_RESTORED, LeaveMode::SavePoint);
    assert_eq!(
        answer,
        Some(weaver_types::Payload::Refusal(LifecycleRefusal::OutOfOrder)),
        "a run that takes no save point is not a dead member"
    );
    assert!(still_entered, "the refusal leaves the run standing");
    assert!(
        !read
            .iter()
            .any(|line| line.starts_with(r#"{"ask":{"snapshot""#)),
        "no snapshot is asked: {read:?}"
    );
    assert!(
        !events.iter().any(|e| e["kind"] == "save_point"),
        "nothing is recorded"
    );
}

/// **No save point is taken after the tee was lost** (the operator's
/// ruling of 2026-10-08 on #99, R1): the member stops reading, the
/// socket's buffer fills and the tee detaches, its distillates since
/// lost; the member then drains and the seam would answer, but the save
/// point refuses as the dead member's and no snapshot is asked, so stale
/// holdings are never published as whole. Perturbation: drop the look
/// at the lost tee and the snapshot is asked and misses its answer.
#[test]
fn a_save_point_after_the_tee_was_lost_is_refused() {
    use std::io::Read;
    let (mut run, _spare, _sink) = entered_run(None);
    let (state_end, mut member_end) =
        std::os::unix::net::UnixStream::pair().expect("the member's pair");
    state_end.set_nonblocking(true).expect("nonblocking");
    let mut seam = crate::state::StateSeam::new(state_end.try_clone().expect("clone"));
    seam.snapshot_answer_bound_ms = 50;
    run.state = Some(seam);
    run.recorder.attach_tee(
        weaver_trace::Tee::open(
            state_end,
            "s-1".into(),
            weaver_trace::Election {
                all_kinds: true,
                keys: Vec::new(),
            },
        )
        .expect("the tee opens"),
    );
    // The member reads nothing, so the buffer fills and the tee goes.
    let mut authored = 0;
    while !run.recorder.serving().expect("serving").tee_lost() {
        authored += 1;
        assert!(authored < 1_000_000, "the tee never detached");
        let key = TurnKey(format!("t-{authored}"));
        run.author
            .author(
                &mut run.recorder,
                Kind::TurnStarted,
                Subsystem::Harness,
                Some(&key),
                None,
            )
            .expect("authored");
    }
    // The member catches up: the seam could carry an ask again.
    member_end.set_nonblocking(true).expect("nonblocking");
    let mut drained = vec![0u8; 1 << 16];
    while matches!(member_end.read(&mut drained), Ok(n) if n > 0) {}
    assert_eq!(
        take_save_point(&mut run, None),
        Err(weaver_types::SavePointLeg::MemberDead)
    );
    let mut after = Vec::new();
    let _ = member_end.read_to_end(&mut after);
    assert!(
        !String::from_utf8_lossy(&after).contains(r#"{"ask":{"snapshot""#),
        "no snapshot is asked over lost distillates"
    );
}

/// **A save point whose caller went away leaves the run standing** (the
/// operator's ruling of 2026-10-08 on #99, N4): admin's end closes
/// before the answer, so the answer is not delivered, and the dispatch
/// returns as a delivered one does, the run still entered and the save
/// point recorded, never an unwind. Perturbation: answer a failed send
/// with its fault again and the dispatch errs.
#[test]
fn a_save_point_whose_caller_went_away_leaves_the_run_standing() {
    let (events, _, _, answer, still_entered) =
        enter_against_a_member_leaving(None, false, EMPTY_RESTORED, LeaveMode::SavePointAbandoned);
    assert_eq!(answer, None, "nobody was there to take the answer");
    assert!(still_entered, "the run stands");
    assert!(
        events.iter().any(|e| e["kind"] == "save_point"),
        "the save point is recorded"
    );
}

/// **A save point on demand is taken at rest, recorded and reported**,
/// per `weaver-harness-Spec` section 6 on the operator's rulings of
/// 2026-10-06 on #1: the `SavePoint` directive against an entered run
/// runs the four legs, authors `save_point`, and answers
/// `SavePointTaken` with the report, the event's position among it, the
/// run standing entered after it. Perturbation: answer the report
/// before authoring the event and the position is not on the record.
#[test]
fn a_save_point_on_demand_is_taken_at_rest_recorded_and_reported() {
    let (events, _, read, answer, still_entered) =
        enter_against_a_member_leaving(None, false, EMPTY_RESTORED, LeaveMode::SavePoint);
    assert!(still_entered, "the run stands after a save point on demand");
    assert!(
        read.iter()
            .any(|line| line.starts_with(r#"{"acknowledge":{"snapshot""#)),
        "the four legs ran: {read:?}"
    );
    let taken = events
        .iter()
        .find(|e| e["kind"] == "save_point")
        .expect("the save point is recorded");
    // **The cause the directive carried is on the event** (Codex on #94,
    // round 7). Perturbation: author the payload with `cause: None` and
    // this reads null.
    assert_eq!(taken["payload"]["cause"], serde_json::json!({"uid": 1000}));
    let position = taken["sequence"]
        .as_str()
        .expect("the sequence")
        .parse::<u64>()
        .expect("a number");
    assert_eq!(
        answer,
        Some(weaver_types::Payload::Answer(
            LifecycleAnswer::SavePointTaken {
                // **The event's run is this run's, the covered position
                // the stamp's prior run** (Codex on #94, round 6).
                // Perturbation: report the stamp's run as the event's
                // and `event_run` reads r-0.
                report: weaver_types::SavePointReport {
                    save_point: "ab".into(),
                    name: "ab.save-point".into(),
                    run: weaver_types::RunId("r-0".into()),
                    sequence: 5,
                    turn: 1,
                    event_run: weaver_types::RunId("r-1".into()),
                    position,
                },
            }
        ))
    );
}

/// **A leave whose `finished` answer never comes does not complete**, per
/// `weaver-harness-Spec` section 6 on the operator's ruling of 2026-10-06
/// on #1 (A3.0 item 6): the harness refuses `SavePointNotTaken` naming
/// the finished leg, authors no `save_point` and no `unload`, and stays
/// entered at rest with the run open; and **a forced leave takes its save
/// point as any leave does** (`weaver-admin-Spec` 3.0: `forced` no longer
/// means lost), `Left` naming it, or, its leg missed, comes down
/// `Left { forced: true }` with none and the miss recorded as a refusal of
/// the leave naming the leg (S6 forced x a missed leg, S7). **The
/// stopped run keeps its gate lowered** (S9, `weaver-admin-Spec` 3.0): the
/// leave that follows, here the fixture's own unwind on the same
/// `lower_gate` path a retried `Leave` takes, sends no second `Lower`.
///
/// Perturbations: convert the missed leg under the dead-peer rule and
/// the leave completes, the first case's `unload` appearing; skip the
/// save point on a forced leave and the second case's record carries
/// none; stop a forced leave at a missed leg as a graceful one stops,
/// and the third case is refused rather than answered; keep the gate in `lower_gate` and the S9 case's stand-in meets a
/// second `Lower`.
#[test]
fn a_leave_without_its_save_point_stops_and_a_forced_leave_takes_one() {
    let (events, _, s9_read, answer, still_entered) = enter_against_a_member_leaving(
        None,
        false,
        EMPTY_RESTORED,
        LeaveMode::Directive {
            forced: false,
            finished: false,
        },
    );
    assert_eq!(
        answer,
        Some(weaver_types::Payload::Refusal(
            LifecycleRefusal::SavePointNotTaken {
                missed: weaver_types::SavePointLeg::Finished,
            }
        )),
        "the missed leg is named"
    );
    assert!(still_entered, "the run stays open at rest");
    let lowers: Vec<&String> = s9_read
        .iter()
        .filter(|line| line.starts_with("<gate lowered>") || line.starts_with("<undeliverable>"))
        .collect();
    assert_eq!(
        lowers,
        vec!["<gate lowered>"],
        "S9 keeps its gate lowered: the leave after it sends no second `Lower`"
    );
    let kinds: Vec<&str> = events.iter().map(|e| e["kind"].as_str().unwrap()).collect();
    // The run was then unwound by the fixture itself; what the refused
    // leave authored is nothing, so the first unload is the fixture's.
    assert!(
        !kinds.contains(&"save_point"),
        "no save point was recorded: {kinds:?}"
    );
    let (events, _, read, answer, still_entered) = enter_against_a_member_leaving(
        None,
        false,
        EMPTY_RESTORED,
        LeaveMode::Directive {
            forced: true,
            finished: true,
        },
    );
    assert!(!still_entered, "a forced leave completes");
    assert!(
        matches!(
            answer,
            Some(weaver_types::Payload::Answer(LifecycleAnswer::Left {
                save_point: Some(_),
                forced: true,
                no_state: false,
            }))
        ),
        "a forced leave names the save point it took: {answer:?}"
    );
    assert!(
        read.iter()
            .any(|line| line.starts_with(r#"{"ask":{"snapshot""#)),
        "the snapshot is asked on a forced leave: {read:?}"
    );
    assert!(
        !read.iter().any(|line| line == "<wind-down>"),
        "a forced leave runs no wind-down: {read:?}"
    );
    let unload = events
        .iter()
        .find(|e| e["kind"] == "unload")
        .expect("an unload");
    assert_eq!(unload["payload"]["forced"], true, "{unload}");
    assert!(
        events.iter().any(|e| e["kind"] == "save_point"),
        "the forced leave's save point is on the record"
    );

    // A forced leave whose finished leg is missed still comes down.
    let (events, _, _, answer, still_entered) = enter_against_a_member_leaving(
        None,
        false,
        EMPTY_RESTORED,
        LeaveMode::Directive {
            forced: true,
            finished: false,
        },
    );
    assert!(
        !still_entered,
        "a forced leave with a missed leg still completes"
    );
    assert_eq!(
        answer,
        Some(weaver_types::Payload::Answer(LifecycleAnswer::Left {
            save_point: None,
            forced: true,
            no_state: false,
        }))
    );
    let refusal = events
        .iter()
        .find(|e| e["kind"] == "refusal")
        .unwrap_or_else(|| panic!("the miss is recorded: {events:?}"));
    assert_eq!(refusal["payload"]["asked"]["ask"], "leave", "{refusal}");
    assert!(
        !events.iter().any(|e| e["kind"] == "save_point"),
        "no save point where its leg missed"
    );
}

/// **A load's rollback takes no save point** (`weaver-admin-Spec` 3.0, S1
/// x `Leave`: a run that never served has nothing to keep), told by the
/// directive's `rollback` and never by the position: no snapshot is
/// asked, and `Left` names none; `unload` is written `forced` true
/// (`weaver-harness-Spec` section 6, the rollback's row). Perturbation:
/// take the save point on a rollback as on a forced leave, and the
/// snapshot is asked.
#[test]
fn a_rollback_leave_in_entered_takes_no_save_point_and_writes_forced() {
    let (events, _, read, answer, still_entered) =
        enter_against_a_member_leaving(None, false, EMPTY_RESTORED, LeaveMode::Rollback);
    assert!(!still_entered);
    assert!(
        matches!(
            answer,
            Some(weaver_types::Payload::Answer(LifecycleAnswer::Left {
                save_point: None,
                forced: true,
                ..
            }))
        ),
        "{answer:?}"
    );
    assert!(
        !read
            .iter()
            .any(|line| line.starts_with(r#"{"ask":{"snapshot""#)),
        "no snapshot is asked on a rollback: {read:?}"
    );
    assert!(!events.iter().any(|e| e["kind"] == "save_point"));
    let unload = events
        .iter()
        .find(|e| e["kind"] == "unload")
        .expect("the rollback's unload");
    assert_eq!(unload["payload"]["forced"], true, "{unload}");
}

/// **A frame the gate admitted as it is lowered is recorded and not
/// answered**, per `weaver-harness-Spec` section 6 (Codex on #94, rounds
/// 14 and 15): the gate stand-in, in the real gate's order, has a frame
/// standing ahead of `GateStopped` and drops its connections before it
/// answers; the lower authors a refusal of the leave for the frame,
/// writes nothing the gate could not deliver, reads on to `GateStopped`,
/// and the leave completes with its save point. Perturbations: read one
/// message for the lower again and the frame is taken as a malformed
/// answer, the leave refusing; send the response again and it reaches
/// the stand-in after the answer, undeliverable. The stand-in in round
/// 14's order, waiting on the response before it answers, passes that
/// broken send, which is why it is not the order kept.
#[test]
fn a_frame_admitted_while_the_gate_lowers_is_recorded_and_not_answered() {
    let (events, _, read, answer, still_entered) =
        enter_against_a_member_leaving(None, false, EMPTY_RESTORED, LeaveMode::FrameDuringLower);
    assert!(
        matches!(
            answer,
            Some(weaver_types::Payload::Answer(LifecycleAnswer::Left {
                save_point: Some(_),
                ..
            }))
        ),
        "the leave completes: {answer:?} {read:?}"
    );
    assert!(!still_entered);
    assert!(
        !read.iter().any(|line| line.starts_with("<undeliverable>")),
        "nothing is written the gate cannot deliver: {read:?}"
    );
    let refusal = events
        .iter()
        .find(|e| e["kind"] == "refusal")
        .unwrap_or_else(|| panic!("the refused frame is on the record: {events:?}"));
    assert_eq!(refusal["payload"]["asked"]["ask"], "leave", "{refusal}");
    assert_eq!(
        refusal["payload"]["refusal"]["kind"], "unloading",
        "{refusal}"
    );
}

/// **A frame queued at the gate when a graceful leave arrives is refused
/// at the drain** (`weaver-admin-Spec` 3.0, graceful step 2; S4 x dialer
/// request): the harness quiesces the gate, meets the frame ahead of
/// `GateQuiesced`, answers it the refused close naming the unload on its
/// own exchange, records it as a refusal of the leave carrying
/// `Unloading`, and the leave goes on: lowered after the quiesce, its
/// save point taken, `Left` answered. Perturbation: refuse the leave
/// `ActivityNotAtRest` over the queued frame as before, or answer the
/// frame after the lower, and this fails.
#[test]
fn a_frame_queued_at_the_gate_is_refused_unloading_at_the_drain() {
    let (events, _, read, answer, still_entered) =
        enter_against_a_member_leaving(None, false, EMPTY_RESTORED, LeaveMode::QueuedFrame);
    assert!(
        matches!(
            answer,
            Some(weaver_types::Payload::Answer(LifecycleAnswer::Left {
                save_point: Some(_),
                forced: false,
                ..
            }))
        ),
        "the leave goes on: {answer:?} {read:?}"
    );
    assert!(!still_entered);
    let at = |needle: &str| {
        read.iter()
            .position(|line| line.starts_with(needle))
            .unwrap_or_else(|| panic!("{needle} in {read:?}"))
    };
    assert!(at("<gate quiesced>") < at("<gate lowered>"), "{read:?}");
    assert!(
        at("<answered 1>") < at("<gate lowered>"),
        "refused before the lower: {read:?}"
    );
    assert!(
        read[at("<answered 1>")].contains("unloading"),
        "the refused close names the unload: {read:?}"
    );
    let refusal = events
        .iter()
        .find(|e| e["kind"] == "refusal")
        .unwrap_or_else(|| panic!("the refusal is recorded: {events:?}"));
    assert_eq!(refusal["payload"]["asked"]["ask"], "leave", "{refusal}");
    assert_eq!(
        refusal["payload"]["refusal"]["kind"], "unloading",
        "{refusal}"
    );
}

/// **Under a diagnostic binding the enter records nothing, so the record
/// opens with `replay.opened`**, per `weaver-diagnostic-Spec` section 4 and
/// `weaver-trace-Spec` section 3's recall clause: the enter authors no load
/// and no prefix there, and its asks are setup the diagnostic record does
/// not carry, the ask that feeds the model being the replay port's, inside
/// the bracket. The member answers the enter's asks exactly as it does for
/// a serving enter, so a recall written here would be the first event.
///
/// Perturbation: drop `!diagnostic` from the identity ask's guard and the
/// assertion fails, the record opening with `recall`. Watched under
/// exactly that removal.
#[test]
fn a_diagnostic_enter_records_nothing_ahead_of_the_bracket() {
    let events = enter_against_a_member(None, true);
    assert!(
        events.iter().all(|e| e["kind"] != "recall"),
        "no recall from the enter: {events:?}"
    );
    if let Some(first) = events.first() {
        assert_eq!(
            first["kind"], "replay.opened",
            "whatever the record holds opens with the bracket: {events:?}"
        );
    }
}

/// **An enter with no member seats no prefix**, per `weaver-harness-Spec`
/// section 6.1 on the operator's ruling of 2026-10-06: the member is the
/// identity's one source, the enter carries no prompt, and a serving load
/// with no member elected has nothing to ask, so the record opens with
/// the load and the next event is the leave's. The member's answer
/// seating is watched in
/// `a_serving_enter_asks_restored_before_load_and_seats_the_members_answer`.
/// Perturbation: seat a message from the enter's instruction again and
/// the second event is a `message.system`.
#[test]
fn an_enter_with_no_member_seats_no_prefix() {
    let dir = crate::scratch::dir(format!(
        "weaver-identity-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let listener = crate::channel::bind_coordination(&dir.join("coordination.sock")).expect("bind");
    let sink_path = dir.join("trace.ndjson");
    let sink = OwnedFd::from(File::create(&sink_path).expect("sink"));
    let mut harness = Harness {
        coordination: listener,
        organs: OrganBinaries {
            classify: None,
            spu: "/nonexistent/weaver-spu".into(),
            gate: "/nonexistent/weaver-gate".into(),
        },
        parameters: OrganParameters::default(),
        state: ChannelState::BeforeEnter,
        composer: Some(weaver_trace::LoopIdentity::compiled("test")),
    };
    let payload = weaver_types::EnterPayload {
        session: SessionId("s-identity".into()),
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
            shell: None,
        },
        state_store: weaver_types::StateStore::default(),
        declaration: String::new(),
        restore: None,
        reset: None,
        stack: Default::default(),
        boundary: String::new(),
        cause: weaver_types::Cause { uid: 0 },
        operator: 1000,
        library_path: None,
        drain_bound: None,
        wind_down_bound: None,
        state_election: weaver_types::StateElection {
            all_kinds: false,
            keys: Vec::new(),
        },
    };
    let mut run = match harness.enter(payload, Some(sink), None) {
        Err(EnterFailure::AfterLoad(run, _)) => run,
        Ok(_) => panic!("the bogus fan-out cannot succeed"),
        Err(EnterFailure::BeforeLoad(refusal)) => {
            panic!("failed before the load: {refusal:?}")
        }
    };
    let _ = leave(&mut run, None, false);
    drop(run);

    let held = std::fs::read_to_string(&sink_path).expect("the sink reads back");
    let events: Vec<serde_json::Value> = held
        .lines()
        .map(|line| serde_json::from_str(line).expect("each line parses"))
        .collect();
    assert_eq!(events[0]["kind"], "load", "the load opens the run");
    assert!(
        events
            .iter()
            .all(|e| e["kind"] != "message.system" && e["kind"] != "recall"),
        "no prefix and no recall, the enter carrying no prompt and asking no one: {events:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// **The declared elections reach the load record, all three of them.**
/// The authorship suite watches the door and this watches the mapping:
/// that `enter` reads each election out of the SPU instruction it was
/// handed and writes it into the record's `load`, rather than writing a
/// default that happens to match the fixture.
///
/// **Both states are run because a boolean that is always false passes
/// a test that never sets it true.** The surprisal's election is the
/// one this matters most for: its `false` is a written member rather
/// than an omission, so a mapping that dropped the value entirely and a
/// mapping that carried it would look alike on a declined load.
#[test]
fn the_declared_elections_reach_the_load_record() {
    for elected in [false, true] {
        let dir = crate::scratch::dir(format!(
            "weaver-elections-{}-{elected}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let listener =
            crate::channel::bind_coordination(&dir.join("coordination.sock")).expect("bind");
        let sink_path = dir.join("trace.ndjson");
        let sink = OwnedFd::from(File::create(&sink_path).expect("sink"));
        let mut harness = Harness {
            coordination: listener,
            organs: OrganBinaries {
                classify: None,
                spu: "/nonexistent/weaver-spu".into(),
                gate: "/nonexistent/weaver-gate".into(),
            },
            parameters: OrganParameters::default(),
            state: ChannelState::BeforeEnter,
            composer: Some(weaver_trace::LoopIdentity::compiled("test")),
        };
        let payload = weaver_types::EnterPayload {
            session: SessionId("s-elections".into()),
            run: weaver_types::RunId("r-1".into()),
            spu_instruction: weaver_types::SpuInstruction {
                classify: None,
                decoder: weaver_types::DecoderInstruction {
                    model_binding: weaver_types::ModelBinding {
                        artifact: weaver_types::ArtifactRef("unreachable".into()),
                        devices: vec![weaver_types::DeviceOrdinal(0)],
                    },
                    residual_readout_election: elected,
                    field_election: elected.then_some(weaver_types::FieldElection { depth: 50 }),
                    surprisal_election: elected,
                    refeed_permission: false,
                    column_permission: false,
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
                shell: None,
            },
            state_store: weaver_types::StateStore::default(),
            declaration: String::new(),
            restore: None,
            reset: None,
            stack: Default::default(),
            boundary: "b0b0".to_string(),
            cause: weaver_types::Cause { uid: 1000 },
            operator: 1000,
            library_path: Some("/opt/weaver/lib".to_string()),
            drain_bound: None,
            wind_down_bound: None,
            state_election: weaver_types::StateElection {
                all_kinds: false,
                keys: vec![weaver_types::ElectedKindConfig {
                    kind: "turn.closed".to_string(),
                    paths: vec!["close".to_string()],
                }],
            },
        };
        let mut run = match harness.enter(payload, Some(sink), None) {
            Err(EnterFailure::AfterLoad(run, _)) => run,
            Ok(_) => panic!("the bogus fan-out cannot succeed"),
            Err(EnterFailure::BeforeLoad(refusal)) => {
                panic!("failed before the load: {refusal:?}")
            }
        };
        let _ = leave(&mut run, Some(weaver_types::Cause { uid: 1001 }), false);
        drop(run);

        let held = std::fs::read_to_string(&sink_path).expect("the sink reads back");
        let load: serde_json::Value =
            serde_json::from_str(held.lines().next().expect("the load opens the run"))
                .expect("the load parses");
        assert_eq!(load["kind"], "load");
        // **Admin's three facts of 2026-10-03 land as the enter carried
        // them**, per `weaver-trace-Spec` section 3, with a uid no
        // fixture's stub could produce, and the leave's own cause lands
        // on the unload. Perturbation: author the load's cause as uid 0
        // or the unload's as absent and the matching assertion fails.
        assert_eq!(load["payload"]["boundary"], "b0b0");
        assert_eq!(load["payload"]["cause"], serde_json::json!({"uid": 1000}));
        assert_eq!(load["payload"]["library_path"], "/opt/weaver/lib");
        let unload: serde_json::Value = serde_json::from_str(
            held.lines()
                .find(|line| line.contains("\"kind\":\"unload\""))
                .expect("the leave closes the bracket"),
        )
        .expect("the unload parses");
        assert_eq!(unload["payload"]["cause"], serde_json::json!({"uid": 1001}));
        assert_eq!(
            load["payload"]["residual_readout"],
            serde_json::json!(elected),
            "the readout's election is the one that was declared"
        );
        assert_eq!(
            load["payload"]["surprisal"],
            serde_json::json!(elected),
            "and so is the surprisal's, written down either way"
        );
        assert_eq!(
            load["payload"]["tee"],
            serde_json::json!({
                "all_kinds": false,
                "keys": [{"kind": "turn.closed", "paths": ["close"]}]
            }),
            "the tee's rule is the enter's declared election, keys and \
             all, not a default"
        );
        if elected {
            assert_eq!(
                load["payload"]["field"],
                serde_json::json!(50),
                "the field carries the declared depth"
            );
        } else {
            assert!(
                load["payload"].get("field").is_none(),
                "and an unelected field is an absence rather than a null"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// **A diagnostic enter forks no gate.** The real SPU admits the real
/// fixture and the fan-out ends at that confirmation: ready, with the
/// gate arm never stood. The gate binary handed in is deliberately
/// nonexistent, which is the discriminator - perturb `enter` to run the
/// gate step whatever the kind and this fork fails `BindFailed`, so a
/// passing run is one the gate step never touched rather than one it
/// survived. Watched to fail under exactly that perturbation, run on the
/// device 2026-08-24. Same gating as the rehearsal below: the
/// device-featured SPU built and the fixture present, under
/// `WEAVER_REHEARSAL=1`.
#[test]
fn the_diagnostic_enter_forks_no_gate() {
    let fixture = std::path::Path::new("/opt/weaver/models/qwen2.5-0.5b-instruct-q6_k.gguf");
    let target = std::env::current_exe()
        .expect("the test binary knows itself")
        .parent()
        .and_then(std::path::Path::parent)
        .expect("the target profile dir")
        .to_path_buf();
    let spu = std::env::var("WEAVER_SPU_BIN")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| target.join("weaver-spu"));
    if std::env::var("WEAVER_REHEARSAL").is_err() || !fixture.exists() || !spu.exists() {
        eprintln!(
            "SKIP the_diagnostic_enter_forks_no_gate: set WEAVER_REHEARSAL=1 with the \
             device-featured SPU built, needs {} and {}",
            fixture.display(),
            spu.display()
        );
        return;
    }

    let scratch = crate::scratch::dir(format!("weaver-diagnostic-{}", std::process::id()));
    let sink_path = scratch.join("diagnostic.ndjson");
    let sink = OwnedFd::from(File::create(&sink_path).expect("sink"));

    let (coordination, _coordination_dir) = test_listener();
    let mut harness = Harness {
        coordination,
        organs: OrganBinaries {
            classify: None,
            spu: spu.clone(),
            // The discriminator: a gate step that runs cannot fork this.
            gate: "/nonexistent/weaver-gate".into(),
        },
        parameters: OrganParameters::default(),
        state: ChannelState::BeforeEnter,
        composer: Some(weaver_trace::LoopIdentity::compiled("test")),
    };
    let payload = weaver_types::EnterPayload {
        session: SessionId("diagnostic".into()),
        run: weaver_types::RunId("r-1".into()),
        spu_instruction: weaver_types::SpuInstruction {
            classify: None,
            decoder: weaver_types::DecoderInstruction {
                model_binding: weaver_types::ModelBinding {
                    artifact: weaver_types::ArtifactRef(fixture.to_string_lossy().into_owned()),
                    devices: vec![weaver_types::DeviceOrdinal(0)],
                },
                residual_readout_election: false,
                field_election: None,
                surprisal_election: false,
                refeed_permission: false,
                column_permission: false,
                tunable_values: [
                    ("max-tokens-per-turn".to_string(), 4096.0),
                    ("context-capacity".to_string(), 4096.0),
                    ("seed".to_string(), 11.0),
                ]
                .into_iter()
                .collect(),
            },
        },
        binding: weaver_types::EnterBinding::Diagnostic,
        state_store: weaver_types::StateStore::default(),
        declaration: String::new(),
        restore: None,
        reset: None,
        stack: Default::default(),
        boundary: String::new(),
        cause: weaver_types::Cause { uid: 0 },
        operator: 1000,
        library_path: None,
        drain_bound: None,
        wind_down_bound: None,
        state_election: weaver_types::StateElection::default(),
    };

    let mut run = match harness.enter(payload, Some(sink), None) {
        Ok(run) => run,
        Err(EnterFailure::BeforeLoad(refusal)) => panic!("enter refused early: {refusal:?}"),
        Err(EnterFailure::AfterLoad(_, refusal)) => panic!("enter refused: {refusal:?}"),
    };

    // Ready with the arm never stood: the fan-out ended at the SPU's
    // confirmation, per `weaver-harness-Spec` section 3.
    assert!(run.gate.is_none(), "a diagnostic run stands no gate arm");

    // The leave lowers what stands, and the gate arm is not among it.
    leave(&mut run, None, false).expect("the leave unwinds the diagnostic run");
    drop(run);
    let _ = std::fs::remove_dir_all(&scratch);
}

/// **The turn rehearses on the device**, the live run's shape inside
/// the suite: the real enter fan-out forks the real organ binaries,
/// the SPU admits real weights on the device, the session opens with
/// the instruction's identity, a frame grants the seat, the decode
/// runs, the response frame returns, the leave unwinds, and the
/// artifact is read back whole from a real sink. The real client dial
/// stays the live run's, where uids differ: the gate denies its own
/// uid by construction, so after the real raise proves the gate arm,
/// the turn is driven on a scripted gate end.
///
/// Skips loudly without the fixture and both built binaries. Build the
/// SPU with the device features first:
/// `cargo build -p weaver-spu --features cuda,gguf`.
#[test]
fn the_turn_rehearses_on_the_device() {
    let fixture = std::path::Path::new("/opt/weaver/models/qwen2.5-0.5b-instruct-q6_k.gguf");
    let target = std::env::current_exe()
        .expect("the test binary knows itself")
        .parent()
        .and_then(std::path::Path::parent)
        .expect("the target profile dir")
        .to_path_buf();
    let spu = std::env::var("WEAVER_SPU_BIN")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| target.join("weaver-spu"));
    let gate = std::env::var("WEAVER_GATE_BIN")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| target.join("weaver-gate"));
    if std::env::var("WEAVER_REHEARSAL").is_err()
        || !fixture.exists()
        || !spu.exists()
        || !gate.exists()
    {
        eprintln!(
            "SKIP the_turn_rehearses_on_the_device: set WEAVER_REHEARSAL=1 with the \
             device-featured SPU built, needs {} and {} and {}",
            fixture.display(),
            spu.display(),
            gate.display()
        );
        return;
    }

    let scratch = crate::scratch::dir(format!("weaver-rehearsal-{}", std::process::id()));
    let sink_path = scratch.join("rehearsal.ndjson");
    let sink = OwnedFd::from(File::create(&sink_path).expect("sink"));
    let gate_socket = scratch.join("gate.sock");
    std::fs::remove_file(&gate_socket).ok();

    let (coordination, _coordination_dir) = test_listener();
    let mut harness = Harness {
        coordination,
        organs: OrganBinaries {
            classify: None,
            spu: spu.clone(),
            gate: gate.clone(),
        },
        parameters: OrganParameters::default(),
        state: ChannelState::BeforeEnter,
        composer: Some(weaver_trace::LoopIdentity::compiled("test")),
    };
    let payload = weaver_types::EnterPayload {
        session: SessionId("rehearsal".into()),
        run: weaver_types::RunId("r-1".into()),
        spu_instruction: weaver_types::SpuInstruction {
            classify: None,
            decoder: weaver_types::DecoderInstruction {
                model_binding: weaver_types::ModelBinding {
                    artifact: weaver_types::ArtifactRef(fixture.to_string_lossy().into_owned()),
                    devices: vec![weaver_types::DeviceOrdinal(0)],
                },
                residual_readout_election: false,
                field_election: None,
                surprisal_election: false,
                refeed_permission: false,
                column_permission: false,
                // The deployed root's three operator-tunables, required
                // at admit since the seed act of 2026-08-19. The empty
                // map this carried predates that election and refused
                // every rehearsal since, invisibly behind the env gate.
                tunable_values: [
                    ("max-tokens-per-turn".to_string(), 4096.0),
                    ("context-capacity".to_string(), 4096.0),
                    ("seed".to_string(), 11.0),
                ]
                .into_iter()
                .collect(),
            },
        },
        binding: weaver_types::EnterBinding::Serving {
            gate_instruction: weaver_types::GateInstruction {
                access_rule: weaver_types::AccessRule {
                    allowed_uids: std::collections::BTreeSet::new(),
                    allowed_gids: std::collections::BTreeSet::new(),
                    denied_uids: std::collections::BTreeSet::new(),
                },
            },
            shell: None,
        },
        state_store: weaver_types::StateStore::default(),
        declaration: String::new(),
        restore: None,
        reset: None,
        stack: Default::default(),
        boundary: String::new(),
        cause: weaver_types::Cause { uid: 0 },
        operator: 1000,
        library_path: None,
        drain_bound: None,
        wind_down_bound: None,
        state_election: weaver_types::StateElection::default(),
    };

    // The real fan-out: real forks, real admit against real weights,
    // the real session open, the real raise.
    let mut run = match harness.enter(payload, Some(sink), None) {
        Ok(run) => run,
        Err(EnterFailure::BeforeLoad(refusal)) => panic!("enter refused early: {refusal:?}"),
        Err(EnterFailure::AfterLoad(_, refusal)) => panic!("enter refused: {refusal:?}"),
    };

    // The raise proved the gate arm on the real binary. The turn is
    // driven on a scripted end, the real dial being act four's.
    if let Some(gate_arm) = run.gate.take() {
        drop(gate_arm.channel);
        reap(gate_arm.pid);
    }
    let (gate_end, gate_peer) = OrganChannel::pair().expect("gate pair");
    let gate_peer = gate_peer.into_channel();
    run.gate = Some(GateChannel {
        channel: gate_end,
        pid: nix::unistd::Pid::from_raw(1),
        last_word: crate::spawn::LastWord::quiet(),
        shell_granted: true,
    });
    harness.state = ChannelState::Entered(Box::new(run));

    gate_peer
        .send(&OrganEnvelope {
            exchange: ExchangeId {
                opener: Opener::Gate,
                ordinal: 1,
            },
            position: Position::Open,
            payload: weaver_types::Payload::Frame(weaver_types::TurnFrame::carry(
                b"{\"text\":\"Reply with exactly one word: hello\"}",
            )),
        })
        .expect("the frame sends");

    let mut verb_slot = None;
    harness
        .serve_gate_wake(
            "",
            &mut |ports: &mut crate::engine::Ports<'_>, text: &str| {
                let delta = vec![weaver_traits::Message {
                    role: weaver_traits::Role::User,
                    content: vec![weaver_traits::ContentBlock::Text {
                        text: text.to_string(),
                    }],
                }];
                ports.turn(delta)
            },
            &mut verb_slot,
        )
        .expect("the turn serves on the device");

    let answer = gate_peer.recv().expect("the response frame returns");
    let weaver_types::Payload::Frame(frame) = answer.payload else {
        panic!("a frame answers a frame");
    };
    let line = String::from_utf8(frame.octets().expect("canonical")).expect("utf8");
    assert!(
        line.starts_with(r#"{"kind":"answered""#),
        "a real decode answers: {line}"
    );

    // The leave unwinds the real SPU and drains the sink.
    let ChannelState::Entered(run) = std::mem::replace(&mut harness.state, ChannelState::Left)
    else {
        panic!("entered");
    };
    let mut run = *run;
    // The scripted gate arm answers no lower: the real lower is the real
    // binary's, proven with the raise, and act four's live run walks it
    // whole. The arm drops here so the leave's unwind meets no exchange
    // nothing will answer.
    if let Some(scripted) = run.gate.take() {
        drop(scripted.channel);
    }
    leave(&mut run, None, false).expect("the leave unwinds");

    // The artifact, read back whole: the first record any human
    // inspects rides exactly this shape in act four.
    let artifact = std::fs::read_to_string(&sink_path).expect("the artifact reads back");
    let kinds: Vec<String> = artifact
        .lines()
        .filter_map(|line| {
            serde_json::from_str::<serde_json::Value>(line)
                .ok()
                .and_then(|v| v.get("kind").and_then(|k| k.as_str()).map(String::from))
        })
        .collect();
    assert_eq!(
        kinds,
        vec![
            "load",
            "turn.started",
            "message.user",
            "model.request",
            "model.output",
            "model.measurement",
            "message.assistant",
            "turn.closed",
            "unload",
        ],
        "the whole bracket in the artifact, in order"
    );
    assert!(
        artifact.contains(r#""template""#),
        "the request splice carries the SPU's rendering"
    );
    assert!(
        artifact.contains("weights_hash"),
        "the measurement names the model and its weights"
    );
    eprintln!(
        "REHEARSAL: {} events at {}",
        kinds.len(),
        sink_path.display()
    );
}

/// **The reset reason reaches the record kebab-case, both of them**
/// (#99 area 3, C5 and the pin P16): the record's `load.reset.reason`
/// is what a reader matches on. Perturbation: drop the wire enum's
/// `rename_all`, or map `ForcedUnload` to the other, and this fails.
#[test]
fn the_reset_reason_is_written_as_the_wire_spells_it() {
    assert_eq!(
        reset_reason(weaver_types::ResetReason::NoCleanUnload),
        "no-clean-unload"
    );
    assert_eq!(
        reset_reason(weaver_types::ResetReason::ForcedUnload),
        "forced-unload"
    );
}

/// A harness at `state`, its coordination listener in a scratch
/// directory and no organ to fork.
fn harness_at(state: ChannelState) -> (Harness, crate::scratch::Scratch) {
    let (coordination, dir) = test_listener();
    let harness = Harness {
        coordination,
        organs: OrganBinaries {
            classify: None,
            spu: "/nonexistent/spu".into(),
            gate: "/nonexistent/gate".into(),
        },
        parameters: OrganParameters::default(),
        state,
        composer: Some(weaver_trace::LoopIdentity::compiled("test")),
    };
    (harness, dir)
}

/// **Observe answers from any position and authors nothing**, per
/// `weaver-admin-harness-contract` section 3 as of 2026-09-04: unloaded
/// with no load before an enter and after a leave, idle or active with
/// the load's facts while entered, the state a read of the turn key.
/// Perturbation: answer `Idle` for a run whose turn key stands and the
/// active case fails; author an event in the observe arm and the record
/// count below moves.
#[test]
fn observe_answers_from_any_position_and_authors_nothing() {
    fn observed(harness: &mut Harness) -> LifecycleAnswer {
        let (harness_end, peer_end) = OrganChannel::pair().expect("pair");
        harness
            .dispatch_on(
                &harness_end,
                test_exchange(),
                LifecycleDirective::Observe,
                None,
                None,
            )
            .expect("observe dispatches");
        match peer_end.into_channel().recv().expect("answer").payload {
            weaver_types::Payload::Answer(answer) => answer,
            other => panic!("observe answers, got {other:?}"),
        }
    }
    let unloaded = LifecycleAnswer::State {
        state: weaver_types::AgentState::Unloaded,
        load: None,
        constituents: Vec::new(),
    };
    assert_eq!(
        observed(&mut harness_at(ChannelState::BeforeEnter).0),
        unloaded
    );
    assert_eq!(observed(&mut harness_at(ChannelState::Left).0), unloaded);

    let (run, _spare, _sink_path) = entered_run(Some("t-1"));
    let before = run.recorder.structure().expect("record").len();
    let (mut harness, _dir) = harness_at(ChannelState::Entered(Box::new(run)));
    match observed(&mut harness) {
        LifecycleAnswer::State {
            state: weaver_types::AgentState::Active,
            load: Some(load),
            ..
        } => assert_eq!(load.run.0, "r-1", "the load's facts ride beside the state"),
        other => panic!("a turn in flight observes active with its load, got {other:?}"),
    }
    let ChannelState::Entered(run) = &mut harness.state else {
        panic!("the position stays entered")
    };
    assert_eq!(
        run.recorder.structure().expect("record").len(),
        before,
        "an observation authors nothing"
    );
    run.turn_in_flight = None;
    match observed(&mut harness) {
        LifecycleAnswer::State {
            state: weaver_types::AgentState::Idle,
            load: Some(_),
            ..
        } => {}
        other => panic!("at rest observes idle with its load, got {other:?}"),
    }
}

/// **`dispatch_on` refuses a save point while a turn stands** (#99 area
/// 3, P09). Mid-turn the engine's arm answers first; this is the loop's
/// own guard for a run left with a turn in flight, refusing
/// `ActivityNotAtRest`, authoring nothing and leaving the run entered.
/// Perturbation: drop the turn-in-flight check and the directive proceeds
/// to the save point.
#[test]
fn a_save_point_with_a_turn_in_flight_is_refused_not_at_rest() {
    let cause = weaver_types::Cause { uid: 0 };
    {
        let directive = LifecycleDirective::SavePoint { cause };
        let named = format!("{directive:?}");
        let (run, _spare, _sink_path) = entered_run(Some("t-1"));
        let before = run.recorder.structure().expect("record").len();
        let (mut harness, _dir) = harness_at(ChannelState::Entered(Box::new(run)));
        let (harness_end, peer_end) = OrganChannel::pair().expect("pair");
        harness
            .dispatch_on(&harness_end, test_exchange(), directive, None, None)
            .expect("the directive dispatches");
        match peer_end.into_channel().recv().expect("answer").payload {
            weaver_types::Payload::Refusal(LifecycleRefusal::ActivityNotAtRest) => {}
            other => panic!("{named} with a turn in flight refuses not at rest, got {other:?}"),
        }
        let ChannelState::Entered(run) = &harness.state else {
            panic!("{named}: the position stays entered")
        };
        assert_eq!(
            run.recorder.structure().expect("record").len(),
            before,
            "{named}: a refusal for activity authors nothing"
        );
    }
}

/// **A leave is never refused for activity** (`weaver-admin-harness-contract`
/// section 3; the Planner's grade of #119): mid-turn the engine holds it.
/// The loop meets a turn still standing at rest only where its close
/// could not be authored, the engine leaving the key for the unwind to
/// name: the record has failed, so service ends, answering no `Left`
/// over a record without the turn's close (I2), admin reading S8b.
/// Perturbation: refuse it `ActivityNotAtRest` again, and the dialer
/// reads that refusal.
#[test]
fn a_leave_meeting_an_unclosed_turn_ends_service_answering_nothing() {
    let cause = weaver_types::Cause { uid: 0 };
    for forced in [false, true] {
        let (run, _spare, _sink_path) = entered_run(Some("t-1"));
        let (mut harness, _dir) = harness_at(ChannelState::Entered(Box::new(run)));
        let (harness_end, peer_end) = OrganChannel::pair().expect("pair");
        let dispatched = harness.dispatch_on(
            &harness_end,
            test_exchange(),
            LifecycleDirective::Leave {
                cause,
                forced,
                rollback: false,
            },
            None,
            None,
        );
        assert!(dispatched.is_err(), "forced = {forced}: service ends");
        let peer = peer_end.into_channel();
        assert!(
            !channel_has_pending(&harness_end) && !channel_has_pending(&peer),
            "forced = {forced}: nothing is answered"
        );
    }
}

/// A stop places the turn's close event and answers `TurnAborted`.
///
/// **This test proves both effects happened, not that the record preceded
/// the answer**, and the distinction is worth stating rather than papering
/// over: `dispatch` returns after doing both, so every assertion here
/// observes the finished state and reversing the two operations inside the
/// stop arm leaves this test passing. An ordering watch needs an observer
/// that reads the answer while `dispatch` is still running, which needs
/// shared access to the recorder this crate deliberately keeps behind one
/// owner. So the ordering itself is held by statement order and read at
/// review, and `weaver-harness-Spec` section 8 tags
/// `harness-announce-after-record` `perturbation` - a tension named here
/// for the operator rather than hidden behind a green test.
#[test]
fn stop_records_the_close_and_answers_turn_aborted() {
    let (run, _spare, _sink_path) = entered_run(Some("t-1"));
    let (harness_end, peer_end) = OrganChannel::pair().expect("pair");
    let (coordination, _coordination_dir) = test_listener();
    let mut harness = Harness {
        coordination,
        organs: OrganBinaries {
            classify: None,
            spu: "/nonexistent/spu".into(),
            gate: "/nonexistent/gate".into(),
        },
        parameters: OrganParameters::default(),
        state: ChannelState::Entered(Box::new(run)),
        composer: Some(weaver_trace::LoopIdentity::compiled("test")),
    };
    harness
        .dispatch_on(
            &harness_end,
            test_exchange(),
            LifecycleDirective::Stop {
                cause: weaver_types::Cause { uid: 1000 },
            },
            None,
            None,
        )
        .expect("stop dispatches");

    // The close is in the record, carrying the stop's own cause.
    // Perturbation: author the directive's close with `cause: None` and
    // the cause assertion fails.
    let closes: Vec<String> = match &harness.state {
        ChannelState::Entered(run) => run
            .recorder
            .structure()
            .expect("the serving record")
            .by_kind(Kind::TurnClosed)
            .map(|record| record.line.to_string())
            .collect(),
        _ => panic!("the position stays entered after a stop"),
    };
    assert_eq!(closes.len(), 1, "the turn's close event was placed");
    let close: serde_json::Value = serde_json::from_str(&closes[0]).expect("parses");
    assert_eq!(close["payload"]["cause"], serde_json::json!({"uid": 1000}));

    // ...and only then does the answer carry TurnAborted.
    let peer = peer_end.into_channel();
    match peer.recv().expect("answer").payload {
        weaver_types::Payload::Answer(LifecycleAnswer::TurnAborted { turn }) => {
            assert_eq!(turn.0, "t-1")
        }
        other => panic!("a stop on a turn answers TurnAborted, got {other:?}"),
    }
}

/// **The lower's refusal is not lost to a stop in S9** (#140, S15;
/// `weaver-harness-Spec` section 6): a gate that refuses the `Lower` of a
/// graceful leave whose `finished` never comes stops the leave in S9,
/// answered `SavePointNotTaken`; the force that follows completes the unwind
/// and names the lower's refusal at its seal, never `Left`, its own leg
/// missed and its lower already done. Perturbation: drop the refusal at the
/// stop, as before, and the force answers `Left`.
#[test]
fn a_lower_refused_before_a_stop_in_s9_is_named_by_the_force_after() {
    let (_, _, _, answer, still_entered) = enter_against_a_member_leaving(
        None,
        false,
        EMPTY_RESTORED,
        LeaveMode::RefusedLowerThenForced,
    );
    assert_eq!(
        answer,
        Some(weaver_types::Payload::Refusal(
            LifecycleRefusal::OrganRefused {
                organ: weaver_types::RefusingOrgan::Gate,
                reason: Box::new(LifecycleRefusal::Malformed),
            }
        )),
        "the force names the lower's refusal"
    );
    assert!(!still_entered, "the force completes the unwind");
}

/// **A gate a leave lowered leaves the run in S9, not S2** (3.0, S9: a
/// leave stopped at its save point, the run entered at rest with the
/// gate lowered), so the table meets a retried leave and a dialer where
/// the run stands. Perturbation: lower without marking it, and the run
/// reads `S2`.
#[test]
fn a_gate_a_leave_lowered_stands_the_run_in_s9() {
    let (mut run, _spare, _sink) = entered_run(None);
    let (channel, gate_peer) = OrganChannel::pair().expect("pair");
    run.gate = Some(GateChannel {
        channel,
        pid: nix::unistd::Pid::from_raw(i32::MAX),
        last_word: crate::spawn::LastWord::quiet(),
        shell_granted: true,
    });
    let gate_peer = gate_peer.into_channel();
    let stand_in = std::thread::spawn(move || {
        let lower = gate_peer.recv().expect("the lower");
        gate_peer
            .send(&OrganEnvelope {
                exchange: lower.exchange,
                position: Position::Close,
                payload: weaver_types::Payload::Answer(LifecycleAnswer::GateStopped),
            })
            .expect("stopped");
    });
    assert_eq!(
        lower_gate(&mut run, None),
        (None, false),
        "the lower is answered"
    );
    stand_in.join().expect("the stand-in finishes");
    assert_eq!(run.position(None), crate::lifecycle_table::Position::S9);
}

/// **A graceful leave directed with a dead SPU or gate starts forced**
/// (`weaver-admin-Spec` 3.0 ruling (A)): the run at rest, its SPU dead,
/// is left forced with no `forced_by`, the graceful caller's cause kept,
/// and `Left` says `forced` true. A diagnostic binding, which has no gate
/// by design, and a stopped leave's lowered gate are not dead gates.
/// Perturbation: carry the directive's graceful leave out unchanged, and
/// `Left` says `forced` false.
#[test]
fn a_graceful_leave_with_a_dead_spu_or_gate_starts_forced() {
    // (case, SPU dead, gate standing, gate lowered by a leave, diagnostic, forced)
    for (case, spu_dead, gate_standing, lowered, diagnostic, want) in [
        (
            "the SPU dead, the gate standing",
            true,
            true,
            false,
            false,
            true,
        ),
        (
            "the gate dead, the SPU standing",
            false,
            false,
            false,
            false,
            true,
        ),
        (
            "S9: the gate a leave lowered",
            false,
            false,
            true,
            false,
            false,
        ),
        (
            "a diagnostic binding, no gate by design",
            false,
            false,
            false,
            true,
            false,
        ),
    ] {
        let (mut run, _spare, _sink) = entered_run(None);
        // The SPU's lifecycle end answers its release where it stands.
        let (spu_lifecycle, spu_far) = OrganChannel::pair().expect("pair");
        run.spu.as_mut().expect("the arm").lifecycle = spu_lifecycle;
        let spu_far = spu_far.into_channel();
        std::thread::spawn(move || {
            while let Ok(asked) = spu_far.recv() {
                let _ = spu_far.send(&OrganEnvelope {
                    exchange: asked.exchange,
                    position: Position::Close,
                    payload: weaver_types::Payload::Answer(LifecycleAnswer::Released),
                });
            }
        });
        if spu_dead {
            run.spu = None;
        }
        if gate_standing {
            let (channel, gate_peer) = OrganChannel::pair().expect("pair");
            run.gate = Some(GateChannel {
                channel,
                pid: nix::unistd::Pid::from_raw(i32::MAX),
                last_word: crate::spawn::LastWord::quiet(),
                shell_granted: true,
            });
            let gate_peer = gate_peer.into_channel();
            std::thread::spawn(move || {
                while let Ok(asked) = gate_peer.recv() {
                    let _ = gate_peer.send(&OrganEnvelope {
                        exchange: asked.exchange,
                        position: Position::Close,
                        payload: weaver_types::Payload::Answer(LifecycleAnswer::GateStopped),
                    });
                }
            });
        }
        run.gate_lowered = lowered;
        if diagnostic {
            run.diagnostic = Some(DiagnosticSeat {
                reader_elected: false,
                artifact: String::new(),
                replayed: true,
            });
        }
        let (mut harness, _dir) = harness_entered(run);
        let (verb_end, admin_end) = OrganChannel::pair().expect("admin pair");
        let admin = admin_end.into_channel();
        let outcome = harness
            .dispatch_on(
                &verb_end,
                ExchangeId {
                    opener: Opener::Admin,
                    ordinal: 3,
                },
                LifecycleDirective::Leave {
                    cause: weaver_types::Cause { uid: 7 },
                    forced: false,
                    rollback: false,
                },
                None,
                None,
            )
            .expect("the leave dispatches");
        assert!(matches!(outcome, Some(Outcome::Left)), "{case}");
        let left = admin.recv().expect("the answer");
        let weaver_types::Payload::Answer(LifecycleAnswer::Left { forced, .. }) = left.payload
        else {
            panic!("{case}: {left:?}");
        };
        assert_eq!(forced, want, "{case}");
    }
}

/// Whether the gate's end has something to read or has hung up, for a
/// fixture waiting on a frame it queued.
fn channel_has_pending(channel: &OrganChannel) -> bool {
    use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
    use std::os::fd::AsFd;
    let mut fds = [PollFd::new(channel.as_fd(), PollFlags::POLLIN)];
    matches!(poll(&mut fds, PollTimeout::ZERO), Ok(n) if n > 0)
}

/// **The lower reads its channel promptly** (the plan's Review Focus 4,
/// `weaver-harness-Spec` section 6): a gate that sends every frame it
/// admitted ahead of `GateStopped` has each recorded refused `Unloading`
/// and its answer read well inside the lower bound, the lower refusing
/// nothing. Perturbation: give each frame slow work, and the answer lands
/// past the bound.
#[test]
fn the_lower_reads_the_channel_promptly_and_meets_gate_stopped_inside_the_bound() {
    const FRAMES: u64 = 200;
    let (mut run, _spare, _sink) = entered_run(None);
    let (channel, gate_peer) = OrganChannel::pair().expect("pair");
    run.gate = Some(GateChannel {
        channel,
        pid: nix::unistd::Pid::from_raw(i32::MAX),
        last_word: crate::spawn::LastWord::quiet(),
        shell_granted: true,
    });
    let gate_peer = gate_peer.into_channel();
    let stand_in = std::thread::spawn(move || {
        let asked = gate_peer.recv().expect("the lower");
        assert!(matches!(
            asked.payload,
            weaver_types::Payload::Directive(LifecycleDirective::Lower)
        ));
        for ordinal in 1..=FRAMES {
            gate_peer
                .send(&OrganEnvelope {
                    exchange: ExchangeId {
                        opener: Opener::Gate,
                        ordinal,
                    },
                    position: Position::Open,
                    payload: weaver_types::Payload::Frame(weaver_types::TurnFrame::carry(
                        b"{\"text\":\"late\"}",
                    )),
                })
                .expect("a frame sends");
        }
        gate_peer
            .send(&OrganEnvelope {
                exchange: asked.exchange,
                position: Position::Close,
                payload: weaver_types::Payload::Answer(LifecycleAnswer::GateStopped),
            })
            .expect("the answer sends");
    });
    let started = std::time::Instant::now();
    let (refused, _) = lower_gate_within(&mut run, 2_000, None);
    let took = started.elapsed();
    stand_in.join().expect("the stand-in");
    assert!(refused.is_none(), "{refused:?}");
    assert!(
        took < std::time::Duration::from_millis(2_000),
        "the answer inside the bound: {took:?}"
    );
    let refusals = run
        .recorder
        .structure()
        .expect("the serving record")
        .iter()
        .filter(|r| r.kind == Kind::Refusal)
        .count();
    assert_eq!(refusals as u64, FRAMES, "each frame refused on the record");
    assert!(run.gate.is_none() && run.gate_lowered);
}

/// **A lower unanswered inside the bound closes the gate's channel and
/// reaps it** (`weaver-harness-Spec` section 6, the lower bound): the
/// harness waits the bound and no longer, closure being death to the gate
/// (`weaver-harness-gate-contract` section 1), and the leave goes on, the
/// lower refusing nothing. Perturbation: wait on `GateStopped` unbounded,
/// and the lower never returns.
#[test]
fn a_lower_unanswered_inside_the_bound_closes_and_reaps_the_gate() {
    const BOUND_MS: u64 = 300;
    let (mut run, _spare, _sink) = entered_run(None);
    let (channel, gate_peer) = OrganChannel::pair().expect("pair");
    run.gate = Some(GateChannel {
        channel,
        pid: nix::unistd::Pid::from_raw(i32::MAX),
        last_word: crate::spawn::LastWord::quiet(),
        shell_granted: true,
    });
    let gate_peer = gate_peer.into_channel();
    let (closed_tx, closed_rx) = std::sync::mpsc::channel();
    // A gate that takes the lower and never answers, until its channel
    // closes.
    std::thread::spawn(move || {
        let _lower = gate_peer.recv();
        let _ = closed_tx.send(gate_peer.recv().is_err());
    });
    let started = std::time::Instant::now();
    let (refused, _) = lower_gate_within(&mut run, BOUND_MS, None);
    let took = started.elapsed();
    assert!(refused.is_none(), "the leave goes on: {refused:?}");
    assert!(
        took >= std::time::Duration::from_millis(BOUND_MS),
        "the bound waited: {took:?}"
    );
    assert!(
        closed_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("the lower returned and closed the channel"),
        "the gate meets its channel closed"
    );
    assert!(run.gate.is_none() && run.gate_lowered);
}

/// **The release comes before `unload`, which carries its outcome**
/// (`weaver-harness-Spec` section 6, item 6; I9): the SPU released by its
/// `Release`, answered `Released` (`Confirmed`), the member's seam closed
/// (`Closed`), and only then `unload`, the record's last event.
/// Perturbation: author `unload` before the release, and the SPU's
/// release lands after the unload's distillate.
#[test]
fn the_release_precedes_unload_and_unload_carries_its_outcome() {
    let (events, refusal, read, answer, _) = enter_against_a_member_leaving(
        None,
        false,
        EMPTY_RESTORED,
        LeaveMode::Directive {
            forced: false,
            finished: true,
        },
    );
    assert!(refusal.is_none(), "{refusal:?}");
    assert!(
        matches!(
            answer,
            Some(weaver_types::Payload::Answer(LifecycleAnswer::Left { .. }))
        ),
        "{answer:?}"
    );
    let at = |needle: &str| {
        read.iter()
            .position(|line| line.contains(needle))
            .unwrap_or_else(|| panic!("{needle} reached the member: {read:?}"))
    };
    assert!(
        at("<spu released>") < at(r#""kind":"unload""#),
        "the SPU released before unload is authored: {read:?}"
    );
    let unload = events.last().expect("a record");
    assert_eq!(
        unload["kind"], "unload",
        "unload is the last event: {events:?}"
    );
    assert_eq!(
        unload["payload"]["release"],
        serde_json::json!({"spu": "confirmed", "member": "closed"}),
        "{unload}"
    );
}

/// The `unload` event of `run`'s serving record, and every kind before it.
fn unload_of(run: &Run) -> (serde_json::Value, Vec<String>) {
    let records = run.recorder.structure().expect("the serving record");
    let kinds = records
        .iter()
        .map(|r| {
            serde_json::from_str::<serde_json::Value>(r.line.as_ref()).unwrap()["kind"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    let unload = records
        .iter()
        .find(|r| r.kind == Kind::Unload)
        .map(|r| serde_json::from_str(r.line.as_ref()).unwrap())
        .expect("an unload");
    (unload, kinds)
}

/// **An SPU already dead reads `Unconfirmed`, and the leave goes on**
/// (`weaver-admin-Spec` 3.0, the organ-death table, S6 and S9 x the SPU
/// dies): nothing to release, no second fault, `Left` answered.
/// Perturbation: refuse the leave on an absent SPU, and it errs.
#[test]
fn a_release_with_no_spu_reads_unconfirmed_and_the_leave_goes_on() {
    let (mut run, _spare, _sink) = entered_run(None);
    run.spu = None;
    let left = leave_after(
        &mut run,
        Some(weaver_types::Cause { uid: 7 }),
        true,
        None,
        None,
        None,
    );
    assert!(left.is_ok(), "{left:?}");
    let (unload, kinds) = unload_of(&run);
    assert_eq!(
        unload["payload"]["release"],
        serde_json::json!({"spu": "unconfirmed"}),
        "{unload}"
    );
    assert!(!kinds.iter().any(|k| k == "fault"), "{kinds:?}");
}

/// **An SPU that dies in its release is a `fault` before `unload`, and
/// the leave goes on** (`weaver-admin-Spec` 3.0, S7 x the SPU dies; I9):
/// `release.spu` `Unconfirmed`, `Left` answered rather than refused.
/// Perturbation: drop the fault, and none precedes `unload`.
#[test]
fn an_spu_that_dies_in_its_release_is_a_fault_before_unload() {
    let (mut run, _spare, _sink) = entered_run(None);
    let (lifecycle, spu_far) = OrganChannel::pair().expect("pair");
    run.spu.as_mut().expect("the arm").lifecycle = lifecycle;
    // The SPU takes its Release and dies without answering.
    let spu_far = spu_far.into_channel();
    std::thread::spawn(move || {
        let _ = spu_far.recv();
        drop(spu_far);
    });
    let left = leave_after(
        &mut run,
        Some(weaver_types::Cause { uid: 7 }),
        false,
        None,
        None,
        None,
    );
    assert!(left.is_ok(), "{left:?}");
    let (unload, kinds) = unload_of(&run);
    assert_eq!(
        unload["payload"]["release"]["spu"], "unconfirmed",
        "{unload}"
    );
    let fault = kinds.iter().position(|k| k == "fault").expect("a fault");
    let last = kinds.iter().position(|k| k == "unload").expect("unload");
    assert!(fault < last, "{kinds:?}");
    assert_eq!(last, kinds.len() - 1, "unload last: {kinds:?}");
}

/// **A member seam found dead at the release reads `Unconfirmed`, with no
/// fault** (`weaver-admin-Spec` 3.0, S7 x the member dies;
/// `weaver-harness-state-contract` section 5, a dead peer being no
/// fault). Perturbation: read every member `Closed`, and this one is.
#[test]
fn a_member_seam_dead_at_the_release_reads_unconfirmed() {
    let (mut run, _spare, _sink) = entered_run(None);
    let (spu_lifecycle, spu_far) = OrganChannel::pair().expect("pair");
    run.spu.as_mut().expect("the arm").lifecycle = spu_lifecycle;
    let spu_far = spu_far.into_channel();
    std::thread::spawn(move || {
        while let Ok(asked) = spu_far.recv() {
            let _ = spu_far.send(&OrganEnvelope {
                exchange: asked.exchange,
                position: Position::Close,
                payload: weaver_types::Payload::Answer(LifecycleAnswer::Released),
            });
        }
    });
    let (near, far) = std::os::unix::net::UnixStream::pair().expect("pair");
    drop(far);
    run.state = Some(crate::state::StateSeam::new(near));
    let left = leave_after(
        &mut run,
        Some(weaver_types::Cause { uid: 7 }),
        false,
        None,
        None,
        None,
    );
    assert!(left.is_ok(), "{left:?}");
    let (unload, kinds) = unload_of(&run);
    assert_eq!(
        unload["payload"]["release"],
        serde_json::json!({"spu": "confirmed", "member": "unconfirmed"}),
        "{unload}"
    );
    assert!(!kinds.iter().any(|k| k == "fault"), "{kinds:?}");
}

/// **A declared bound is checked time** (the plan's Review Focus 3): a
/// bound whose deadline the clock cannot hold reads as unbounded, never a
/// panic, and an absent bound is unbounded. Perturbation: add the bound
/// with `+`, and the near-`u64::MAX` case panics.
#[test]
fn a_bound_near_u64_max_reads_as_unbounded() {
    assert!(deadline_after(None).is_none());
    assert!(deadline_after(Some(u64::MAX)).is_none());
    let soon = deadline_after(Some(1)).expect("a deadline");
    assert!(soon > std::time::Instant::now());
}

/// A graceful leave at rest against stand-ins that answer late, 3 s
/// after they are asked: the gate's quiesce, and the SPU's wind-down
/// generation unless it is cancelled first. The run declares `drain` and
/// `wind_down` as its bounds. Answers admin's answer, the record, and
/// whether the wind-down's generation was cancelled.
fn a_leave_against_late_stand_ins(
    drain: Option<u64>,
    wind_down: Option<u64>,
) -> (weaver_types::Payload, Vec<serde_json::Value>, bool) {
    use nix::poll::{PollFd, PollFlags, poll};
    use nix::sys::socket::{
        AddressFamily, MsgFlags, SockFlag, SockType, recv as sock_recv, send as sock_send,
        socketpair,
    };
    use std::os::fd::{AsFd, AsRawFd};
    const LATE_MS: u16 = 3_000;
    let (mut run, _spare, sink_path) = entered_run(None);
    run.drain_bound = drain;
    run.wind_down_bound = wind_down;
    let (spu_lifecycle, spu_far) = OrganChannel::pair().expect("pair");
    run.spu.as_mut().expect("the arm").lifecycle = spu_lifecycle;
    let spu_far = spu_far.into_channel();
    std::thread::spawn(move || {
        while let Ok(asked) = spu_far.recv() {
            let _ = spu_far.send(&OrganEnvelope {
                exchange: asked.exchange,
                position: Position::Close,
                payload: weaver_types::Payload::Answer(LifecycleAnswer::Released),
            });
        }
    });
    let (near, far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .expect("socketpair");
    run.spu.as_mut().expect("the arm").decode = crate::channel::decode_from_owned(near);
    // The SPU: the wind-down's append, answered late, or at once stopped
    // where a cancel arrives first.
    let decode_peer = std::thread::spawn(move || {
        let mut buf = vec![0u8; 65536];
        let Ok(n) = sock_recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()) else {
            return false;
        };
        if n == 0 {
            return false;
        }
        let mut fds = [PollFd::new(far.as_fd(), PollFlags::POLLIN)];
        let cancelled = poll(&mut fds, LATE_MS).unwrap_or(0) > 0
            && sock_recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).is_ok_and(|n| {
                matches!(
                    serde_json::from_slice(&buf[..n]),
                    Ok(weaver_types::TokenDirective::Cancel { .. })
                )
            });
        let answer = weaver_types::TokenAnswer::Generated(weaver_types::Generation {
            unrecovered: vec![],
            content: vec![],
            emission: "the summary".into(),
            finish: if cancelled {
                weaver_types::Finish::Stopped
            } else {
                weaver_types::Finish::Completed
            },
            resident: 64,
            capacity: 4096,
            request: serde_json::value::RawValue::from_string(r#"{"rendered":"r"}"#.into())
                .unwrap(),
            measurement: serde_json::value::RawValue::from_string("{}".into()).unwrap(),
        });
        let _ = sock_send(
            far.as_raw_fd(),
            &serde_json::to_vec(&answer).unwrap(),
            MsgFlags::empty(),
        );
        cancelled
    });
    // The gate: the quiesce answered late, or not at all where the
    // lower comes first; the lower answered at once.
    let (gate_end, gate_peer) = OrganChannel::pair().expect("gate pair");
    run.gate = Some(GateChannel {
        channel: gate_end,
        pid: nix::unistd::Pid::from_raw(i32::MAX),
        last_word: crate::spawn::LastWord::quiet(),
        shell_granted: true,
    });
    let gate_peer = gate_peer.into_channel();
    std::thread::spawn(move || {
        while let Ok(asked) = gate_peer.recv() {
            let answer = match asked.payload {
                weaver_types::Payload::Directive(LifecycleDirective::Quiesce) => {
                    let mut fds = [PollFd::new(gate_peer.as_fd(), PollFlags::POLLIN)];
                    if poll(&mut fds, LATE_MS).unwrap_or(0) > 0 {
                        continue;
                    }
                    LifecycleAnswer::GateQuiesced
                }
                _ => LifecycleAnswer::GateStopped,
            };
            let _ = gate_peer.send(&OrganEnvelope {
                exchange: asked.exchange,
                position: Position::Close,
                payload: weaver_types::Payload::Answer(answer),
            });
        }
    });
    let (mut harness, _dir) = harness_entered(run);
    let (verb_end, admin_end) = OrganChannel::pair().expect("admin pair");
    let admin = admin_end.into_channel();
    harness
        .dispatch_on(
            &verb_end,
            ExchangeId {
                opener: Opener::Admin,
                ordinal: 3,
            },
            LifecycleDirective::Leave {
                cause: weaver_types::Cause { uid: 7 },
                forced: false,
                rollback: false,
            },
            None,
            None,
        )
        .expect("the leave dispatches");
    let left = admin.recv().expect("the answer").payload;
    drop(harness);
    let cancelled = decode_peer.join().expect("the decode peer");
    let events = std::fs::read_to_string(&sink_path)
        .expect("the sink")
        .lines()
        .map(|line| serde_json::from_str(line).expect("a record line"))
        .collect();
    (left, events, cancelled)
}

/// **A declared drain bound turns the leave forced, with no `forced_by`**
/// (`weaver-admin-Spec` 3.0, S4 x a bound passes; `weaver-harness-Spec`
/// section 6, the bounds): the quiesce unanswered past the bound, the
/// leave skips the wind-down, and `unload` and `Left` carry `forced` true,
/// the leave's state at its end, the graceful caller's cause kept and no
/// join named. Perturbation: wait on the quiesce unbounded, and the leave
/// ends graceful.
#[test]
fn a_drain_bound_turns_the_leave_forced_without_forced_by() {
    let (left, events, _) = a_leave_against_late_stand_ins(Some(1), None);
    assert!(
        matches!(
            left,
            weaver_types::Payload::Answer(LifecycleAnswer::Left { forced: true, .. })
        ),
        "{left:?}"
    );
    let unload = events.last().expect("a record");
    assert_eq!(unload["kind"], "unload", "{events:?}");
    assert_eq!(unload["payload"]["forced"], true, "{unload}");
    assert_eq!(unload["payload"]["cause"]["uid"], 7, "{unload}");
    assert!(unload["payload"].get("forced_by").is_none(), "{unload}");
    assert!(
        !events
            .iter()
            .any(|e| e.to_string().contains("The agent is unloading now")),
        "no wind-down after the drain bound: {events:?}"
    );
}

/// **`Left.forced` is the leave's state at its end**
/// (`weaver-admin-Spec` section 3's definition, I2): directed graceful, a
/// declared wind-down bound passing turns it forced, the wind-down's
/// generation cancelled (S5 x a bound passes). Perturbation: answer the
/// directive's `forced`, and `Left` reads false.
#[test]
fn left_forced_is_the_leaves_state_at_its_end() {
    let (left, events, cancelled) = a_leave_against_late_stand_ins(None, Some(1));
    assert!(
        cancelled,
        "the wind-down's generation cancelled at its bound"
    );
    assert!(
        matches!(
            left,
            weaver_types::Payload::Answer(LifecycleAnswer::Left { forced: true, .. })
        ),
        "{left:?}"
    );
    let unload = events.last().expect("a record");
    assert_eq!(unload["payload"]["forced"], true, "{unload}");
    assert!(unload["payload"].get("forced_by").is_none(), "{unload}");
    // The wind-down its bound cut closes stopped by the unload, with the
    // leave's cause, the graceful caller's (the Planner's ruling of
    // 2026-10-09).
    let close = events
        .iter()
        .find(|e| e["kind"] == "turn.closed")
        .expect("the wind-down's close");
    assert_eq!(close["payload"]["reason"], "unload", "{close}");
    assert_eq!(close["payload"]["cause"]["uid"], 7, "{close}");
}

/// **With no bound declared the leave waits and stays graceful**
/// (`weaver-admin-Spec` section 3, the bounds: the drain and the
/// wind-down are unbounded by default), and a bound past what the stand-ins
/// take turns nothing. Perturbation: default either bound, and the leave
/// turns forced.
#[test]
fn a_leave_with_no_bound_passing_stays_graceful() {
    let (left, _, cancelled) = a_leave_against_late_stand_ins(None, Some(u64::MAX));
    assert!(!cancelled);
    assert!(
        matches!(
            left,
            weaver_types::Payload::Answer(LifecycleAnswer::Left { forced: false, .. })
        ),
        "{left:?}"
    );
}

/// `run`'s record moved onto a pipe nobody reads yet, holding `events`
/// faults of `octets` each queued behind a writer the pipe has stopped.
/// Answers the pipe's read end.
fn stalled_writer(run: &mut Run, events: usize, octets: usize) -> std::io::PipeReader {
    let (reader, writer) = std::io::pipe().expect("pipe");
    run.recorder = crate::record::Record::from(
        Recorder::receive(
            OwnedFd::from(writer),
            RunRef("r-1".into()),
            SessionRef("s-1".into()),
        )
        .expect("recorder"),
    );
    let account = format!("{{\"pad\":\"{}\"}}", "x".repeat(octets));
    for _ in 0..events {
        let _ = run.author.author_fault(
            &mut run.recorder,
            Subsystem::Harness,
            None,
            &crate::authorship::harness_report(
                weaver_types::FaultCase::StreamWriteFailed,
                &account,
            ),
        );
    }
    reader
}

/// The record a stalled writer's run admitted, read from its structure,
/// then **the run dropped with its sink still stuck**: the drop must
/// return, the writer abandoned and detached (#117), so the worker can
/// exit. Answers the record.
fn record_then_drop(run: Run, reader: std::io::PipeReader) -> Vec<serde_json::Value> {
    let events = run
        .recorder
        .structure()
        .expect("the serving record")
        .iter()
        .map(|r| serde_json::from_str(r.line.as_ref()).expect("a record line"))
        .collect();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        drop(run);
        let _ = tx.send(());
    });
    assert!(
        rx.recv_timeout(std::time::Duration::from_secs(2)).is_ok(),
        "the run drops with its sink stuck (#117)"
    );
    drop(reader);
    events
}

/// **The leave writes the trace queue's peak to standard error** (#120;
/// `weaver-harness-Spec` section 4): after its last drain, one line naming
/// the run's peak depth, the capacity and the high-water mark, which admin's
/// start step points at worker.log. The leave runs in a child of this test
/// binary, whose standard error is read here, against a sink that takes
/// nothing, so the queue grows with every record the leave authors and the
/// line's peak equals the run's final one only where it was written after
/// `unload`. Perturbations: drop the line, and none is found; write it before
/// `unload`, and its peak falls one short of the run's.
#[test]
fn the_leave_writes_the_queue_peak_line() {
    if peak_child("serving") {
        return;
    }
    let child = run_peak_child("serving", std::process::Stdio::piped());
    assert!(child.status.success(), "{child:?}");
    let lines = stderr_objects(&child);
    let line = lines
        .iter()
        .find(|value| value["queue"] == "trace")
        .unwrap_or_else(|| panic!("a queue-peak line on standard error: {lines:?}"));
    let final_peak = lines
        .iter()
        .find_map(|value| value["child_peak"].as_u64())
        .expect("the child names the run's final peak");
    assert!(final_peak > 1, "records queued behind the stuck sink");
    assert_eq!(line["organ"], "harness", "{line}");
    assert_eq!(
        line["peak"], final_peak,
        "written after the leave's last enqueue"
    );
    assert_eq!(line["depth"], weaver_trace::QUEUE_DEPTH, "{line}");
    assert_eq!(line["mark"], weaver_trace::HIGH_WATER_MARK, "{line}");
}

/// **A diagnostic leave writes no queue-peak line** (#120;
/// `weaver-harness-Spec` section 4): its record queues nothing. Perturbation:
/// write the line whatever the arm, and a line with peak zero is found.
#[test]
fn a_diagnostic_leave_writes_no_queue_peak_line() {
    if peak_child("diagnostic") {
        return;
    }
    let child = run_peak_child("diagnostic", std::process::Stdio::piped());
    assert!(child.status.success(), "{child:?}");
    let lines = stderr_objects(&child);
    assert!(
        !lines.iter().any(|value| value["queue"] == "trace"),
        "{lines:?}"
    );
}

/// **A queue-peak line standard error cannot take ends nothing** (#120): the
/// leave runs with standard error on a device that refuses every write, as
/// a full disk under worker.log does, and returns. Perturbation: write the
/// line with `eprintln!`, and the child panics.
#[test]
fn a_queue_peak_line_that_cannot_be_written_ends_nothing() {
    if peak_child("full") {
        return;
    }
    let full = std::fs::OpenOptions::new()
        .write(true)
        .open("/dev/full")
        .expect("/dev/full");
    let child = run_peak_child("full", std::process::Stdio::from(full));
    assert!(child.status.success(), "{child:?}");
}

/// The variable that runs a queue-peak test as the child whose standard
/// error its parent reads, naming the child's mode.
const PEAK_CHILD: &str = "WEAVER_HARNESS_TEST_PEAK_CHILD";

/// The child's half of a queue-peak test: where [`PEAK_CHILD`] names `mode`,
/// a run leaves and `true` is answered, the parent reading what it wrote.
fn peak_child(mode: &str) -> bool {
    if std::env::var(PEAK_CHILD).as_deref() != Ok(mode) {
        return false;
    }
    let (mut run, _spare, _sink) = entered_run(None);
    releasing_spu(&mut run);
    let reader = match mode {
        "diagnostic" => {
            let fd = OwnedFd::from(
                std::fs::OpenOptions::new()
                    .write(true)
                    .open("/dev/null")
                    .expect("/dev/null"),
            );
            run.recorder = crate::record::Record::from(
                weaver_diagnostic::Recorder::receive(
                    fd,
                    weaver_diagnostic::RunRef("r-1".into()),
                    weaver_diagnostic::SessionRef("s-1".into()),
                )
                .expect("recorder"),
            );
            None
        }
        _ => Some(stalled_writer(&mut run, 32, 8_192)),
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(300);
    let _ = leave_after(
        &mut run,
        Some(weaver_types::Cause { uid: 7 }),
        false,
        None,
        None,
        Some(deadline),
    );
    if mode == "serving" {
        eprintln!(
            "{}",
            serde_json::json!({ "child_peak": run.recorder.pressure().peak })
        );
    }
    if let Some(reader) = reader {
        let _ = record_then_drop(run, reader);
    }
    true
}

/// The child run, the test of the same mode alone, its standard error as
/// asked.
fn run_peak_child(mode: &str, stderr: std::process::Stdio) -> std::process::Output {
    let test = match mode {
        "serving" => "lifecycle::tests::the_leave_writes_the_queue_peak_line",
        "diagnostic" => "lifecycle::tests::a_diagnostic_leave_writes_no_queue_peak_line",
        _ => "lifecycle::tests::a_queue_peak_line_that_cannot_be_written_ends_nothing",
    };
    std::process::Command::new(std::env::current_exe().expect("this binary"))
        .args([test, "--exact", "--nocapture", "--test-threads=1"])
        .env(PEAK_CHILD, mode)
        .stdout(std::process::Stdio::null())
        .stderr(stderr)
        .output()
        .expect("the child runs")
}

/// The JSON objects among a child's standard error lines.
fn stderr_objects(child: &std::process::Output) -> Vec<serde_json::Value> {
    String::from_utf8_lossy(&child.stderr)
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

/// An SPU lifecycle stand-in answering every directive `Released`.
fn releasing_spu(run: &mut Run) {
    let (spu_lifecycle, spu_far) = OrganChannel::pair().expect("pair");
    run.spu.as_mut().expect("the arm").lifecycle = spu_lifecycle;
    let spu_far = spu_far.into_channel();
    std::thread::spawn(move || {
        while let Ok(asked) = spu_far.recv() {
            let _ = spu_far.send(&OrganEnvelope {
                exchange: asked.exchange,
                position: Position::Close,
                payload: weaver_types::Payload::Answer(LifecycleAnswer::Released),
            });
        }
    });
}

/// **A writer that never drains is forfeited at the shared deadline**
/// (`weaver-harness-Spec` section 6, the shared deadline; the plan's
/// Review Focus 5): the leave stops waiting at the deadline, a `fault`
/// naming the forfeit authored before `unload`, which is still the
/// record's last event, and answers `DescriptorsUnusable`, no `unload`
/// having reached the stream (I2, on the Planner's ruling of 2026-10-10).
/// Perturbation: drain unbounded, and the leave never returns.
#[test]
fn a_writer_that_never_drains_is_forfeited_at_the_deadline() {
    let (mut run, _spare, _sink) = entered_run(None);
    releasing_spu(&mut run);
    let reader = stalled_writer(&mut run, 32, 8_192);
    let started = std::time::Instant::now();
    let deadline = started + std::time::Duration::from_millis(300);
    let left = leave_after(
        &mut run,
        Some(weaver_types::Cause { uid: 7 }),
        false,
        None,
        None,
        Some(deadline),
    );
    assert_eq!(
        left,
        Err(LifecycleRefusal::DescriptorsUnusable),
        "no unload on the stream, no Left"
    );
    assert!(started.elapsed() < std::time::Duration::from_secs(3));
    let events = record_then_drop(run, reader);
    let kinds: Vec<&str> = events.iter().map(|e| e["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds.last(), Some(&"unload"), "{kinds:?}");
    let forfeit = events
        .iter()
        .position(|e| e.to_string().contains("forfeited"))
        .unwrap_or_else(|| panic!("the forfeit is named: {kinds:?}"));
    assert!(forfeit < kinds.len() - 1);
}

/// **A full queue past the deadline answers no `Left`** (I2: a run with
/// no `unload` on the stream never answered `Left`; the Planner's rulings
/// of 2026-10-09 and 2026-10-10): nothing optional is authored, a submit
/// blocking on the full queue, `unload` takes the slot the writer
/// reserves for it, and the drain not reaching it by the deadline refuses
/// the leave, admin concluding as for no answer. The leave still returns
/// by the deadline. Perturbation: answer `Left` on an expired drain, and
/// the leave's answer is `Ok`.
#[test]
fn a_full_queue_past_the_deadline_answers_no_left() {
    let (mut run, _spare, _sink) = entered_run(None);
    releasing_spu(&mut run);
    let reader = stalled_writer(&mut run, 0, 0);
    let account = format!("{{\"pad\":\"{}\"}}", "x".repeat(1_024));
    while run.recorder.pressure().queued < weaver_trace::QUEUE_DEPTH - weaver_trace::UNLOAD_RESERVE
    {
        let _ = run.author.author_fault(
            &mut run.recorder,
            Subsystem::Harness,
            None,
            &crate::authorship::harness_report(
                weaver_types::FaultCase::StreamWriteFailed,
                &account,
            ),
        );
    }
    let started = std::time::Instant::now();
    let deadline = started + std::time::Duration::from_millis(300);
    let left = leave_after(
        &mut run,
        Some(weaver_types::Cause { uid: 7 }),
        false,
        None,
        None,
        Some(deadline),
    );
    assert!(started.elapsed() < std::time::Duration::from_secs(3));
    assert!(left.is_err(), "no unload, so no Left: {left:?}");
    let events = record_then_drop(run, reader);
    assert_eq!(
        events.last().map(|e| e["kind"].clone()),
        Some(serde_json::json!("unload")),
        "unload took its reserved slot and is the record's last line"
    );
}

/// **`unload` is terminal under recorder pressure** (I9; the plan's
/// Task 7c): a queue over its mark at the leave has the pressure `fault`
/// authored before `unload`, the record's last event. Perturbation: drop
/// the leave's pressure reading, and no pressure fault precedes it.
#[test]
fn unload_is_terminal_under_recorder_pressure() {
    let (mut run, _spare, _sink) = entered_run(None);
    releasing_spu(&mut run);
    let reader = stalled_writer(&mut run, weaver_trace::HIGH_WATER_MARK + 100, 1_024);
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(300);
    let left = leave_after(
        &mut run,
        Some(weaver_types::Cause { uid: 7 }),
        false,
        None,
        None,
        Some(deadline),
    );
    assert_eq!(
        left,
        Err(LifecycleRefusal::DescriptorsUnusable),
        "the sink stalled"
    );
    let events = record_then_drop(run, reader);
    let kinds: Vec<&str> = events.iter().map(|e| e["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds.last(), Some(&"unload"), "{kinds:?}");
    assert!(
        events
            .iter()
            .any(|e| e["payload"]["case"] == "recorder_commit_pressure"),
        "the pressure fault precedes unload"
    );
}

/// **An SPU that never answers its `Release` reads `Unconfirmed` at the
/// shared deadline** (Review Focus 5), and the leave returns by it.
/// Perturbation: wait on the release unbounded, and the leave never
/// returns.
#[test]
fn a_spu_that_never_answers_release_reads_unconfirmed() {
    let (mut run, _spare, _sink) = entered_run(None);
    let (spu_lifecycle, spu_far) = OrganChannel::pair().expect("pair");
    run.spu.as_mut().expect("the arm").lifecycle = spu_lifecycle;
    // Held open and never answered.
    let _spu_far = spu_far;
    let started = std::time::Instant::now();
    let deadline = started + std::time::Duration::from_millis(300);
    let left = leave_after(
        &mut run,
        Some(weaver_types::Cause { uid: 7 }),
        false,
        None,
        None,
        Some(deadline),
    );
    // The deadline spent here leaves the drain none, so `unload` has not
    // reached the stream and the leave answers no `Left` (I2).
    assert_eq!(left, Err(LifecycleRefusal::DescriptorsUnusable));
    assert!(started.elapsed() < std::time::Duration::from_secs(3));
    let (unload, _) = unload_of(&run);
    assert_eq!(
        unload["payload"]["release"]["spu"], "unconfirmed",
        "{unload}"
    );
}

/// **An organ that never exits is killed, then reaped, at the shared
/// deadline** (Review Focus 5): the SPU answers its release and stays
/// running, and the leave returns by the deadline with the process gone.
/// Perturbation: reap without the kill, and the leave waits on the
/// process.
#[test]
fn an_organ_that_never_exits_is_killed_then_reaped() {
    let (mut run, _spare, _sink) = entered_run(None);
    releasing_spu(&mut run);
    let mut child = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .expect("a process that stays");
    let pid = nix::unistd::Pid::from_raw(child.id() as i32);
    run.spu.as_mut().expect("the arm").pid = pid;
    let started = std::time::Instant::now();
    let deadline = started + std::time::Duration::from_millis(300);
    let left = leave_after(
        &mut run,
        Some(weaver_types::Cause { uid: 7 }),
        false,
        None,
        None,
        Some(deadline),
    );
    // The deadline spent here leaves the drain none, so `unload` has not
    // reached the stream and the leave answers no `Left` (I2).
    assert_eq!(left, Err(LifecycleRefusal::DescriptorsUnusable));
    assert!(started.elapsed() < std::time::Duration::from_secs(3));
    assert_eq!(
        nix::sys::signal::kill(pid, None),
        Err(nix::errno::Errno::ESRCH),
        "killed and reaped"
    );
    // Already reaped by the leave: the handle's own wait finds no child.
    assert!(child.wait().is_err(), "the leave reaped it");
}

/// A pending leave on admin's connection, directed `directed` and
/// standing at `leave`, for the join's tests. Answers admin's far end.
fn pending_leave(
    directed: crate::lifecycle_table::Leave,
    leave: crate::lifecycle_table::Leave,
    rollback: bool,
) -> (PendingLeave, OrganChannel) {
    let (connection, admin) = OrganChannel::pair().expect("admin pair");
    (
        PendingLeave {
            connection,
            exchange: ExchangeId {
                opener: Opener::Admin,
                ordinal: 4,
            },
            cause: weaver_types::Cause { uid: 7 },
            rollback,
            forced_by: None,
            progress: crate::lifecycle_table::LeaveProgress {
                directed,
                leave,
                step: crate::lifecycle_table::LeaveStep::Draining,
            },
            drain_deadline: None,
            joins: Vec::new(),
        },
        admin.into_channel(),
    )
}

/// A dialer's connection carrying `directive`, as a dial accepted from
/// the listener holds it. Answers the dialer's far end.
fn dialed(directive: LifecycleDirective) -> (OrganChannel, OrganChannel) {
    let (near, far) = OrganChannel::pair().expect("dial pair");
    let far = far.into_channel();
    far.send(&OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Admin,
            ordinal: 1,
        },
        position: Position::Open,
        payload: weaver_types::Payload::Directive(directive),
    })
    .expect("the directive sends");
    (near, far)
}

/// **A join turns a still-graceful leave forced and names `forced_by`
/// once** (`weaver-admin-Spec` section 3, the forced unload, joining; S4
/// and S6 x force-unload): the first join's cause is named, a later
/// join's is not, and both are held for the same `Left`. From S7 on (the
/// outcome fixed) a join is held and names nothing. Perturbation: name
/// `forced_by` on every join, and the later join overwrites it.
#[test]
fn a_join_from_s4_s5_and_s6_turns_the_leave_forced_naming_forced_by() {
    use crate::lifecycle_table::Leave;
    let (mut leave, _admin) = pending_leave(Leave::Graceful, Leave::Graceful, false);
    let (first, _f) = dialed(LifecycleDirective::JoinLeave {
        cause: weaver_types::Cause { uid: 9 },
    });
    hear_dial(first, &mut leave, false, false);
    assert_eq!(leave.progress.leave, Leave::Forced);
    assert_eq!(leave.forced_by, Some(weaver_types::Cause { uid: 9 }));
    let (later, _l) = dialed(LifecycleDirective::JoinLeave {
        cause: weaver_types::Cause { uid: 10 },
    });
    hear_dial(later, &mut leave, false, false);
    assert_eq!(leave.forced_by, Some(weaver_types::Cause { uid: 9 }));
    assert_eq!(leave.joins.len(), 2, "both held for the same Left");
    // From S7 on, a graceful leave's outcome is fixed: the join is held
    // and turns nothing.
    let (mut sealed, _admin) = pending_leave(Leave::Graceful, Leave::Graceful, false);
    let (join, _j) = dialed(LifecycleDirective::JoinLeave {
        cause: weaver_types::Cause { uid: 9 },
    });
    hear_dial(join, &mut sealed, true, false);
    assert_eq!(sealed.progress.leave, Leave::Graceful);
    assert_eq!(sealed.forced_by, None);
    assert_eq!(sealed.joins.len(), 1);
}

/// **A join into a leave a bound already turned forced names no
/// `forced_by`** (`weaver-harness-Spec` section 6: a later join, or one
/// into a leave a declared bound already turned forced, is named
/// nowhere), and is held all the same. Perturbation: name the join
/// whenever none is named yet, and it is named.
#[test]
fn a_join_after_a_bound_turned_the_leave_forced_names_no_forced_by() {
    use crate::lifecycle_table::Leave;
    let (mut leave, _admin) = pending_leave(Leave::Graceful, Leave::Forced, false);
    let (join, _j) = dialed(LifecycleDirective::JoinLeave {
        cause: weaver_types::Cause { uid: 9 },
    });
    hear_dial(join, &mut leave, false, false);
    assert_eq!(leave.forced_by, None);
    assert_eq!(leave.joins.len(), 1);
}

/// **A join against a force's leave or a rollback's is `OutOfOrder`**
/// (`weaver-harness-Spec` section 6: joins are admitted only into an
/// unload's leave), held nowhere. Perturbation: admit every join, and
/// the dialer reads no refusal.
#[test]
fn a_join_against_a_forces_leave_or_a_rollback_is_out_of_order() {
    use crate::lifecycle_table::Leave;
    for (case, directed, rollback) in [
        ("a force's leave (S10)", Leave::Forced, false),
        ("a rollback's leave", Leave::Forced, true),
    ] {
        let (mut leave, _admin) = pending_leave(directed, Leave::Forced, rollback);
        let (join, far) = dialed(LifecycleDirective::JoinLeave {
            cause: weaver_types::Cause { uid: 9 },
        });
        hear_dial(join, &mut leave, false, false);
        assert!(leave.joins.is_empty(), "{case}");
        assert_eq!(
            far.recv().expect("an answer").payload,
            weaver_types::Payload::Refusal(LifecycleRefusal::OutOfOrder),
            "{case}"
        );
    }
}

/// **An observation while a leave is pending answers `InTransition`**
/// (`weaver-types-Spec` section 3.1), and any other directive meeting the
/// pending leave, a second `Leave` included, is `OutOfOrder`
/// (`weaver-harness-Spec` section 6). Perturbation: answer the
/// observation `OutOfOrder`, and it reads so.
#[test]
fn observe_answers_in_transition_while_a_leave_is_pending() {
    use crate::lifecycle_table::Leave;
    let (mut leave, _admin) = pending_leave(Leave::Graceful, Leave::Graceful, false);
    let (observe, far) = dialed(LifecycleDirective::Observe);
    hear_dial(observe, &mut leave, false, false);
    assert_eq!(
        far.recv().expect("an answer").payload,
        weaver_types::Payload::Answer(LifecycleAnswer::InTransition {
            constituents: Vec::new(),
        })
    );
    let (second, far) = dialed(LifecycleDirective::Leave {
        cause: weaver_types::Cause { uid: 9 },
        forced: true,
        rollback: false,
    });
    hear_dial(second, &mut leave, false, false);
    assert_eq!(
        far.recv().expect("an answer").payload,
        weaver_types::Payload::Refusal(LifecycleRefusal::OutOfOrder)
    );
    assert_eq!(
        leave.progress.leave,
        Leave::Graceful,
        "a Leave joins nothing"
    );
}

/// **A join held at the seal is answered the same `Left`, after the
/// writer drains** (`weaver-admin-Spec` section 3, the seal, step 7): the
/// leave's own dialer and the join read one answer, and when either reads
/// it the record already holds `unload`. **The seal unlinks the
/// pathname** (step 2), so a dial after it finds no socket. Perturbations:
/// answer the leave's dialer alone, and the join reads nothing; skip the
/// unlink, and the socket stands.
#[test]
fn a_join_held_at_the_seal_is_answered_left_after_the_writer_drains() {
    use crate::lifecycle_table::Leave;
    let (mut run, _spare, sink_path) = entered_run(None);
    releasing_spu(&mut run);
    let (mut leave, admin) = pending_leave(Leave::Graceful, Leave::Graceful, false);
    let (join, joiner) = dialed(LifecycleDirective::JoinLeave {
        cause: weaver_types::Cause { uid: 9 },
    });
    hear_dial(join, &mut leave, false, false);
    run.leave = Some(leave);
    let (mut harness, dir) = harness_entered(run);
    assert!(
        dir.join("c.sock").exists(),
        "the socket stands before the leave"
    );
    let outcome = harness.carry_out_held_leave().expect("the leave runs");
    assert!(matches!(outcome, Some(Outcome::Left)));
    for (who, end) in [("the leave's dialer", &admin), ("the join", &joiner)] {
        let answer = end.recv().unwrap_or_else(|_| panic!("{who} is answered"));
        assert!(
            matches!(
                answer.payload,
                weaver_types::Payload::Answer(LifecycleAnswer::Left { forced: true, .. })
            ),
            "{who}: {answer:?}"
        );
        let stream = std::fs::read_to_string(&sink_path).expect("the sink");
        assert!(
            stream
                .lines()
                .last()
                .is_some_and(|line| line.contains(r#""kind":"unload""#)),
            "{who} is answered after unload reached the stream"
        );
    }
    let unload: serde_json::Value = serde_json::from_str(
        std::fs::read_to_string(&sink_path)
            .expect("the sink")
            .lines()
            .last()
            .expect("a line"),
    )
    .expect("json");
    assert_eq!(unload["payload"]["forced_by"]["uid"], 9, "{unload}");
    assert!(
        !dir.join("c.sock").exists(),
        "the seal unlinked the pathname: a dial after it finds no socket"
    );
}

/// **A join dialed during the drain turns the leave forced at once**
/// (`weaver-harness-Spec` section 6: every wait of the leave hears the
/// coordination listener; S4 x force-unload): the gate never answers its
/// quiesce, a force dials the worker's socket and joins, and the leave
/// leaves the drain, skips the wind-down, and answers both dialers
/// `Left { forced: true }`, `unload` naming the join's cause. It needs a
/// root dialer, so it skips as any other uid and the watch below runs
/// it inside a user namespace. Perturbation: wait on the gate alone, and
/// the drain never ends.
#[test]
fn dialed_a_join_during_the_drain_turns_the_leave_forced() {
    use std::os::fd::AsRawFd;
    if !nix::unistd::getuid().is_root() {
        eprintln!("SKIP dialed join: the harness accepts root alone");
        return;
    }
    let (mut run, _spare, sink_path) = entered_run(None);
    releasing_spu(&mut run);
    // A gate that takes the quiesce and never answers it, and answers
    // the lower.
    let (gate_end, gate_peer) = OrganChannel::pair().expect("gate pair");
    run.gate = Some(GateChannel {
        channel: gate_end,
        pid: nix::unistd::Pid::from_raw(i32::MAX),
        last_word: crate::spawn::LastWord::quiet(),
        shell_granted: true,
    });
    let gate_peer = gate_peer.into_channel();
    std::thread::spawn(move || {
        while let Ok(asked) = gate_peer.recv() {
            if matches!(
                asked.payload,
                weaver_types::Payload::Directive(LifecycleDirective::Lower)
            ) {
                let _ = gate_peer.send(&OrganEnvelope {
                    exchange: asked.exchange,
                    position: Position::Close,
                    payload: weaver_types::Payload::Answer(LifecycleAnswer::GateStopped),
                });
            }
        }
    });
    let (mut harness, dir) = harness_entered(run);
    let socket = dir.join("c.sock");
    // The force: it dials once the leave is draining, and joins.
    let joiner = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(300));
        let fd = nix::sys::socket::socket(
            nix::sys::socket::AddressFamily::Unix,
            nix::sys::socket::SockType::SeqPacket,
            nix::sys::socket::SockFlag::SOCK_CLOEXEC,
            None,
        )
        .expect("socket");
        let address = nix::sys::socket::UnixAddr::new(&socket).expect("address");
        nix::sys::socket::connect(fd.as_raw_fd(), &address).expect("connect");
        let join = OrganEnvelope {
            exchange: ExchangeId {
                opener: Opener::Admin,
                ordinal: 1,
            },
            position: Position::Open,
            payload: weaver_types::Payload::Directive(LifecycleDirective::JoinLeave {
                cause: weaver_types::Cause { uid: 9 },
            }),
        };
        nix::sys::socket::send(
            fd.as_raw_fd(),
            &serde_json::to_vec(&join).unwrap(),
            nix::sys::socket::MsgFlags::empty(),
        )
        .expect("the join sends");
        let mut buf = vec![0u8; 65536];
        let n = nix::sys::socket::recv(
            fd.as_raw_fd(),
            &mut buf,
            nix::sys::socket::MsgFlags::empty(),
        )
        .expect("the join's answer");
        serde_json::from_slice::<OrganEnvelope>(&buf[..n]).expect("an envelope")
    });
    let (verb_end, admin_end) = OrganChannel::pair().expect("admin pair");
    let admin = admin_end.into_channel();
    harness
        .dispatch_on(
            &verb_end,
            ExchangeId {
                opener: Opener::Admin,
                ordinal: 3,
            },
            LifecycleDirective::Leave {
                cause: weaver_types::Cause { uid: 7 },
                forced: false,
                rollback: false,
            },
            None,
            None,
        )
        .expect("the leave dispatches");
    let left = admin.recv().expect("the leave's answer").payload;
    let joined = joiner.join().expect("the joiner").payload;
    for answer in [&left, &joined] {
        assert!(
            matches!(
                answer,
                weaver_types::Payload::Answer(LifecycleAnswer::Left { forced: true, .. })
            ),
            "{answer:?}"
        );
    }
    let stream = std::fs::read_to_string(&sink_path).expect("the sink");
    assert!(
        !stream.contains("The agent is unloading now"),
        "no wind-down after the join"
    );
    let unload: serde_json::Value =
        serde_json::from_str(stream.lines().last().expect("a line")).expect("json");
    assert_eq!(unload["payload"]["forced_by"]["uid"], 9, "{unload}");
}

/// **A join heard during the save point's legs turns a missed leg forced**
/// (`weaver-admin-Spec` 3.0, S6 x force-unload and x a missed leg; the
/// Planner's grade of #119): a leave retried from S9 (its gate lowered,
/// no drain or wind-down) meets a member that takes the snapshot's ask and
/// dies half a second later; a force joins while the legs run. The leg
/// misses, and the leave comes down forced with no save point, `Left {
/// forced: true }` to both dialers and `unload` naming the join, rather
/// than stopping in S9. Root, watched below. Perturbation: drop the
/// sweep after the legs, and the leave stops in S9, refused
/// `SavePointNotTaken`.
#[test]
fn dialed_a_join_during_the_legs_turns_a_missed_leg_forced() {
    use std::io::Read;
    use std::os::fd::AsRawFd;
    if !nix::unistd::getuid().is_root() {
        eprintln!("SKIP dialed join in the legs: the harness accepts root alone");
        return;
    }
    let (mut run, _spare, sink_path) = entered_run(None);
    releasing_spu(&mut run);
    run.gate_lowered = true;
    let (near, far) = std::os::unix::net::UnixStream::pair().expect("pair");
    run.state = Some(crate::state::StateSeam::new(near));
    std::thread::spawn(move || {
        let mut far = far;
        let mut buf = [0u8; 4096];
        let _ = far.read(&mut buf);
        std::thread::sleep(std::time::Duration::from_millis(500));
    });
    let (mut harness, dir) = harness_entered(run);
    let socket = dir.join("c.sock");
    let joiner = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(200));
        let fd = nix::sys::socket::socket(
            nix::sys::socket::AddressFamily::Unix,
            nix::sys::socket::SockType::SeqPacket,
            nix::sys::socket::SockFlag::SOCK_CLOEXEC,
            None,
        )
        .expect("socket");
        let address = nix::sys::socket::UnixAddr::new(&socket).expect("address");
        nix::sys::socket::connect(fd.as_raw_fd(), &address).expect("connect");
        let join = OrganEnvelope {
            exchange: ExchangeId {
                opener: Opener::Admin,
                ordinal: 1,
            },
            position: Position::Open,
            payload: weaver_types::Payload::Directive(LifecycleDirective::JoinLeave {
                cause: weaver_types::Cause { uid: 9 },
            }),
        };
        nix::sys::socket::send(
            fd.as_raw_fd(),
            &serde_json::to_vec(&join).unwrap(),
            nix::sys::socket::MsgFlags::empty(),
        )
        .expect("the join sends");
        let mut buf = vec![0u8; 65536];
        let n = nix::sys::socket::recv(
            fd.as_raw_fd(),
            &mut buf,
            nix::sys::socket::MsgFlags::empty(),
        )
        .expect("the join's answer");
        serde_json::from_slice::<OrganEnvelope>(&buf[..n]).expect("an envelope")
    });
    let (verb_end, admin_end) = OrganChannel::pair().expect("admin pair");
    let admin = admin_end.into_channel();
    harness
        .dispatch_on(
            &verb_end,
            ExchangeId {
                opener: Opener::Admin,
                ordinal: 3,
            },
            LifecycleDirective::Leave {
                cause: weaver_types::Cause { uid: 7 },
                forced: false,
                rollback: false,
            },
            None,
            None,
        )
        .expect("the leave dispatches");
    let left = admin.recv().expect("the leave's answer").payload;
    let joined = joiner.join().expect("the joiner").payload;
    for answer in [&left, &joined] {
        assert!(
            matches!(
                answer,
                weaver_types::Payload::Answer(LifecycleAnswer::Left {
                    forced: true,
                    save_point: None,
                    ..
                })
            ),
            "{answer:?}"
        );
    }
    let stream = std::fs::read_to_string(&sink_path).expect("the sink");
    let unload: serde_json::Value =
        serde_json::from_str(stream.lines().last().expect("a line")).expect("json");
    assert_eq!(unload["payload"]["forced_by"]["uid"], 9, "{unload}");
}

/// **A join is heard while the gate floods frames at the drain** (Codex
/// on #119, round 3, the same class as the turn's wait): the drain scans
/// the listener before the gate, so a gate whose channel never falls
/// quiet cannot starve a force joining the unload. The gate floods frames
/// and never answers its quiesce; the join turns the leave forced and
/// both dialers read `Left { forced: true }`. Root, watched below.
/// Perturbation: scan the gate first, and the drain never ends.
#[test]
fn dialed_a_join_while_the_gate_floods_is_heard() {
    use std::os::fd::AsRawFd;
    if !nix::unistd::getuid().is_root() {
        eprintln!("SKIP dialed join under a flood: the harness accepts root alone");
        return;
    }
    let (mut run, _spare, _sink) = entered_run(None);
    releasing_spu(&mut run);
    let (gate_end, gate_peer) = OrganChannel::pair().expect("gate pair");
    run.gate = Some(GateChannel {
        channel: gate_end,
        pid: nix::unistd::Pid::from_raw(i32::MAX),
        last_word: crate::spawn::LastWord::quiet(),
        shell_granted: true,
    });
    let gate_peer = std::sync::Arc::new(gate_peer.into_channel());
    let reader = gate_peer.clone();
    // Reads every refusal, and answers the lower when it comes.
    let answers = gate_peer.clone();
    std::thread::spawn(move || {
        while let Ok(asked) = reader.recv() {
            if matches!(
                asked.payload,
                weaver_types::Payload::Directive(LifecycleDirective::Lower)
            ) {
                let _ = answers.send(&OrganEnvelope {
                    exchange: asked.exchange,
                    position: Position::Close,
                    payload: weaver_types::Payload::Answer(LifecycleAnswer::GateStopped),
                });
            }
        }
    });
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (flooder, halt) = (gate_peer.clone(), stop.clone());
    std::thread::spawn(move || {
        let mut ordinal = 1;
        while !halt.load(std::sync::atomic::Ordering::Relaxed) {
            let _ = flooder.send(&OrganEnvelope {
                exchange: ExchangeId {
                    opener: Opener::Gate,
                    ordinal,
                },
                position: Position::Open,
                payload: weaver_types::Payload::Frame(weaver_types::TurnFrame::carry(
                    b"{\"text\":\"more\"}",
                )),
            });
            ordinal += 1;
        }
    });
    let (mut harness, dir) = harness_entered(run);
    let socket = dir.join("c.sock");
    let joiner = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(300));
        let fd = nix::sys::socket::socket(
            nix::sys::socket::AddressFamily::Unix,
            nix::sys::socket::SockType::SeqPacket,
            nix::sys::socket::SockFlag::SOCK_CLOEXEC,
            None,
        )
        .expect("socket");
        let address = nix::sys::socket::UnixAddr::new(&socket).expect("address");
        nix::sys::socket::connect(fd.as_raw_fd(), &address).expect("connect");
        let join = OrganEnvelope {
            exchange: ExchangeId {
                opener: Opener::Admin,
                ordinal: 1,
            },
            position: Position::Open,
            payload: weaver_types::Payload::Directive(LifecycleDirective::JoinLeave {
                cause: weaver_types::Cause { uid: 9 },
            }),
        };
        nix::sys::socket::send(
            fd.as_raw_fd(),
            &serde_json::to_vec(&join).unwrap(),
            nix::sys::socket::MsgFlags::empty(),
        )
        .expect("the join sends");
        let mut buf = vec![0u8; 65536];
        let n = nix::sys::socket::recv(
            fd.as_raw_fd(),
            &mut buf,
            nix::sys::socket::MsgFlags::empty(),
        )
        .expect("the join's answer");
        serde_json::from_slice::<OrganEnvelope>(&buf[..n]).expect("an envelope")
    });
    let (verb_end, admin_end) = OrganChannel::pair().expect("admin pair");
    let admin = admin_end.into_channel();
    harness
        .dispatch_on(
            &verb_end,
            ExchangeId {
                opener: Opener::Admin,
                ordinal: 3,
            },
            LifecycleDirective::Leave {
                cause: weaver_types::Cause { uid: 7 },
                forced: false,
                rollback: false,
            },
            None,
            None,
        )
        .expect("the leave dispatches");
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    let left = admin.recv().expect("the leave's answer").payload;
    let joined = joiner.join().expect("the joiner").payload;
    for answer in [&left, &joined] {
        assert!(
            matches!(
                answer,
                weaver_types::Payload::Answer(LifecycleAnswer::Left { forced: true, .. })
            ),
            "{answer:?}"
        );
    }
}

/// **The watch for the dialed tests** (#99 area 3's pattern): every
/// `dialed_` test needs a root dialer and skips as any other uid, which
/// is how every gate runs, so this re-executes them inside `unshare
/// --map-root-user`, where the caller is uid 0, and requires each to run
/// there and pass, none skipping. Where no user namespace can be
/// entered it prints a SKIP naming why and passes; run as root, the tests
/// run in place and this does nothing. Perturbation: have the dialed
/// test fail, and this fails naming it.
#[test]
fn the_dialed_tests_are_watched_inside_a_user_namespace() {
    if nix::unistd::getuid().is_root() {
        return;
    }
    let exe = std::env::current_exe().expect("the test binary names itself");
    let run = std::process::Command::new("timeout")
        .arg("120")
        .arg("unshare")
        .arg("--map-root-user")
        .arg(&exe)
        .args([
            "lifecycle::tests::dialed_",
            "--nocapture",
            "--test-threads",
            "1",
        ])
        .stdin(std::process::Stdio::null())
        .output();
    let output = match run {
        Ok(output) => output,
        Err(e) => {
            eprintln!("SKIP dialed watch: unshare could not run: {e}");
            return;
        }
    };
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if said.contains("unshare: ") {
        eprintln!("SKIP dialed watch: no user namespace: {said}");
        return;
    }
    assert!(output.status.success(), "the dialed tests failed: {said}");
    assert!(
        !said.contains("SKIP dialed"),
        "a dialed test skipped: {said}"
    );
    for name in [
        "dialed_a_join_during_the_drain_turns_the_leave_forced",
        "dialed_a_join_during_the_legs_turns_a_missed_leg_forced",
        "dialed_a_join_while_the_gate_floods_is_heard",
    ] {
        assert!(
            said.contains(&format!("{name} ... ok")),
            "{name} ran: {said}"
        );
    }
}

/// **A frame held behind a turn that heard a leave is not served**
/// (`weaver-admin-Spec` 3.0, S4 x dialer request; the self-check of B2):
/// the shelf stops at a held leave, the frame left for the drain to
/// refuse `Unloading`. Perturbation: serve the shelf under a held leave,
/// and the turn's entry runs.
#[test]
fn a_frame_held_behind_a_turn_that_heard_a_leave_is_not_served() {
    use crate::lifecycle_table::Leave;
    let (mut run, _spare, _sink) = entered_run(None);
    let (leave, _admin) = pending_leave(Leave::Graceful, Leave::Graceful, false);
    run.leave = Some(leave);
    run.held_frames.push_back(OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Gate,
            ordinal: 2,
        },
        position: Position::Open,
        payload: weaver_types::Payload::Frame(weaver_types::TurnFrame::carry(
            b"{\"text\":\"queued\"}",
        )),
    });
    let (mut harness, _dir) = harness_entered(run);
    let mut pending = None;
    harness
        .serve_shelf(
            "identity",
            &mut |_: &mut crate::engine::Ports<'_>,
                  _: &str|
             -> Result<crate::engine::TurnOutcome, crate::engine::TurnError> {
                panic!("a frame served under a held leave")
            },
            &mut pending,
        )
        .expect("the shelf");
    let ChannelState::Entered(run) = &harness.state else {
        panic!("the run stands");
    };
    assert_eq!(run.held_frames.len(), 1, "left for the drain");
}

/// **A gate that answers its lower and never exits is killed at the
/// lower's bound, not the shared deadline** (the self-check of B2): the
/// save point's legs come after the lower, so a gate held to the shared
/// deadline would spend their budget. Perturbation: reap the gate by the
/// shared deadline, and the lower holds past its bound.
#[test]
fn a_gate_that_never_exits_is_killed_at_the_lower_bound() {
    const BOUND_MS: u64 = 300;
    let (mut run, _spare, _sink) = entered_run(None);
    let (channel, gate_peer) = OrganChannel::pair().expect("pair");
    #[allow(clippy::zombie_processes)]
    let child = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .expect("a process that stays");
    let pid = nix::unistd::Pid::from_raw(child.id() as i32);
    run.gate = Some(GateChannel {
        channel,
        pid,
        last_word: crate::spawn::LastWord::quiet(),
        shell_granted: true,
    });
    let gate_peer = gate_peer.into_channel();
    std::thread::spawn(move || {
        let lower = gate_peer.recv().expect("the lower");
        let _ = gate_peer.send(&OrganEnvelope {
            exchange: lower.exchange,
            position: Position::Close,
            payload: weaver_types::Payload::Answer(LifecycleAnswer::GateStopped),
        });
    });
    let shared = std::time::Instant::now() + std::time::Duration::from_secs(60);
    let started = std::time::Instant::now();
    let (refused, _) = lower_gate_within(&mut run, BOUND_MS, Some(shared));
    assert!(refused.is_none(), "{refused:?}");
    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "killed at the lower's bound: {:?}",
        started.elapsed()
    );
    assert_eq!(
        nix::sys::signal::kill(pid, None),
        Err(nix::errno::Errno::ESRCH),
        "killed and reaped"
    );
}

/// **A gate's fault report held behind the turn is authored at the
/// drain** (the self-check of B2), as the shelf authors one: the drain
/// refuses frames and records faults, dropping neither. Perturbation:
/// pass the fault over, and the record holds no gate fault.
#[test]
fn a_gate_fault_held_behind_the_turn_is_authored_at_the_drain() {
    use crate::lifecycle_table::Leave;
    let (mut run, _spare, _sink) = entered_run(None);
    run.held_frames.push_back(OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Gate,
            ordinal: 3,
        },
        position: Position::Open,
        payload: weaver_types::Payload::Fault(crate::authorship::harness_report(
            weaver_types::FaultCase::ListenerLost,
            "{}",
        )),
    });
    let (listener, _dir) = test_listener();
    let (mut leave, _admin) = pending_leave(Leave::Graceful, Leave::Graceful, false);
    drain_gate(&mut run, &listener, &mut leave);
    assert!(
        run.recorder
            .structure()
            .expect("the serving record")
            .iter()
            .any(|r| r.kind == Kind::Fault && r.line.as_ref().contains(r#""subsystem":"gate""#)),
        "the gate's fault is on the record"
    );
}

/// A member that answers the save point's legs and the grants read-back,
/// as the leave fixture's does, standing on `run`'s state seam.
fn saving_member(run: &mut Run) {
    use std::io::{BufRead, BufReader, Write};
    let (near, far) = std::os::unix::net::UnixStream::pair().expect("pair");
    run.state = Some(crate::state::StateSeam::new(near));
    std::thread::spawn(move || {
        let mut answers = far.try_clone().expect("clone");
        for line in BufReader::new(far).lines() {
            let Ok(line) = line else { break };
            let value: serde_json::Value = serde_json::from_str(&line).unwrap_or_default();
            let answer = if line.starts_with(r#"{"ask":{"grants""#) {
                r#"{"answer":{"grants":{"surface":[]}}}"#.to_string()
            } else if let Some(ordinal) = value["ask"]["snapshot"]["ask"].as_u64() {
                format!(
                    concat!(
                        r#"{{"answer":{{"snapshot":{{"ask":{},"save-point":"ab.save-point","run":"r-0","#,
                        r#""sequence":5,"turn":1,"digest":"ab"}}}}}}"#
                    ),
                    ordinal
                )
            } else if let Some(ordinal) = value["acknowledge"]["snapshot"]["ask"].as_u64() {
                format!(
                    r#"{{"answer":{{"finished":{{"ask":{},"save-point":"ab.save-point"}}}}}}"#,
                    ordinal
                )
            } else {
                continue;
            };
            if answers.write_all(format!("{answer}\n").as_bytes()).is_err() {
                break;
            }
        }
    });
}

/// Where the gate dies in a graceful leave at rest.
#[derive(Clone, Copy, Debug)]
enum GateDeath {
    /// Before the drain, between the turn's end and the quiesce, so the
    /// quiesce's send itself fails (S4).
    BeforeTheDrain,
    /// At the quiesce (S4), unanswered.
    AtQuiesce,
    /// After answering the quiesce, while the wind-down runs (S5).
    InWindDown,
    /// At the lower, unanswered (S6).
    AtLower,
}

/// A graceful leave at rest whose gate dies where `death` says, against
/// an SPU whose wind-down generation answers 3 s late unless cancelled.
/// Answers admin's answer, the record, and whether the wind-down was
/// cancelled.
fn a_leave_whose_gate_dies(
    death: GateDeath,
) -> (weaver_types::Payload, Vec<serde_json::Value>, bool) {
    use nix::poll::{PollFd, PollFlags, poll};
    use nix::sys::socket::{
        AddressFamily, MsgFlags, SockFlag, SockType, recv as sock_recv, send as sock_send,
        socketpair,
    };
    use std::os::fd::{AsFd, AsRawFd};
    let (mut run, _spare, sink_path) = entered_run(None);
    releasing_spu(&mut run);
    saving_member(&mut run);
    let (near, far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .expect("socketpair");
    run.spu.as_mut().expect("the arm").decode = crate::channel::decode_from_owned(near);
    let decode_peer = std::thread::spawn(move || {
        let mut buf = vec![0u8; 65536];
        match sock_recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()) {
            Ok(n) if n > 0 => {}
            _ => return false,
        }
        let mut fds = [PollFd::new(far.as_fd(), PollFlags::POLLIN)];
        let cancelled = poll(&mut fds, 3_000u16).unwrap_or(0) > 0
            && sock_recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).is_ok_and(|n| {
                matches!(
                    serde_json::from_slice(&buf[..n]),
                    Ok(weaver_types::TokenDirective::Cancel { .. })
                )
            });
        let answer = weaver_types::TokenAnswer::Generated(weaver_types::Generation {
            unrecovered: vec![],
            content: vec![],
            emission: "the summary".into(),
            finish: if cancelled {
                weaver_types::Finish::Stopped
            } else {
                weaver_types::Finish::Completed
            },
            resident: 64,
            capacity: 4096,
            request: serde_json::value::RawValue::from_string(r#"{"rendered":"r"}"#.into())
                .unwrap(),
            measurement: serde_json::value::RawValue::from_string("{}".into()).unwrap(),
        });
        let _ = sock_send(
            far.as_raw_fd(),
            &serde_json::to_vec(&answer).unwrap(),
            MsgFlags::empty(),
        );
        cancelled
    });
    let (gate_end, gate_peer) = OrganChannel::pair().expect("gate pair");
    run.gate = Some(GateChannel {
        channel: gate_end,
        pid: nix::unistd::Pid::from_raw(i32::MAX),
        last_word: crate::spawn::LastWord::quiet(),
        shell_granted: true,
    });
    let gate_peer = gate_peer.into_channel();
    if matches!(death, GateDeath::BeforeTheDrain) {
        drop(gate_peer);
    } else {
        std::thread::spawn(move || {
            while let Ok(asked) = gate_peer.recv() {
                match (death, &asked.payload) {
                    (
                        GateDeath::AtQuiesce,
                        weaver_types::Payload::Directive(LifecycleDirective::Quiesce),
                    ) => return,
                    (_, weaver_types::Payload::Directive(LifecycleDirective::Quiesce)) => {
                        let _ = gate_peer.send(&OrganEnvelope {
                            exchange: asked.exchange,
                            position: Position::Close,
                            payload: weaver_types::Payload::Answer(LifecycleAnswer::GateQuiesced),
                        });
                        if matches!(death, GateDeath::InWindDown) {
                            std::thread::sleep(std::time::Duration::from_millis(300));
                            return;
                        }
                    }
                    (_, weaver_types::Payload::Directive(LifecycleDirective::Lower)) => return,
                    _ => {}
                }
            }
        });
    }
    let (mut harness, _dir) = harness_entered(run);
    let (verb_end, admin_end) = OrganChannel::pair().expect("admin pair");
    let admin = admin_end.into_channel();
    harness
        .dispatch_on(
            &verb_end,
            ExchangeId {
                opener: Opener::Admin,
                ordinal: 3,
            },
            LifecycleDirective::Leave {
                cause: weaver_types::Cause { uid: 7 },
                forced: false,
                rollback: false,
            },
            None,
            None,
        )
        .expect("the leave dispatches");
    let left = admin.recv().expect("the leave's answer").payload;
    drop(harness);
    let cancelled = decode_peer.join().expect("the decode peer");
    let events = std::fs::read_to_string(&sink_path)
        .expect("the sink")
        .lines()
        .map(|line| serde_json::from_str(line).expect("a record line"))
        .collect();
    (left, events, cancelled)
}

/// **A gate dying during a graceful leave turns it forced, and the leave
/// still answers `Left`** (`weaver-admin-Spec` 3.0, S4, S5 and S6 x the
/// gate dies, ruling (A); the Planner's grade of #119): at the drain, in
/// the wind-down (whose generation is cancelled at that moment) or at the
/// lower, the gate's death is a `fault`, the gate is reaped, the save
/// point's legs go on, and `unload` and `Left` say `forced` true with no
/// `forced_by` and the graceful caller's cause. Perturbation: read the
/// dead channel as before, and the lower's failure refuses the leave.
#[test]
fn a_gate_dying_during_a_graceful_leave_turns_it_forced_and_answers_left() {
    for death in [
        GateDeath::BeforeTheDrain,
        GateDeath::AtQuiesce,
        GateDeath::InWindDown,
        GateDeath::AtLower,
    ] {
        let (left, events, cancelled) = a_leave_whose_gate_dies(death);
        assert!(
            matches!(
                left,
                weaver_types::Payload::Answer(LifecycleAnswer::Left {
                    forced: true,
                    save_point: Some(_),
                    ..
                })
            ),
            "{death:?}: forced, the state still saved: {left:?}"
        );
        assert!(
            events.iter().any(|e| e["kind"] == "save_point"),
            "{death:?}: the save point's event: {events:?}"
        );
        let unload = events.last().expect("a record");
        assert_eq!(unload["kind"], "unload", "{death:?}: {events:?}");
        assert_eq!(unload["payload"]["forced"], true, "{death:?}: {unload}");
        assert_eq!(unload["payload"]["cause"]["uid"], 7, "{death:?}: {unload}");
        assert!(unload["payload"].get("forced_by").is_none(), "{death:?}");
        assert!(
            events
                .iter()
                .any(|e| e["kind"] == "fault" && e["payload"]["account"]["organ"] == "gate"),
            "{death:?}: the gate's death is a fault: {events:?}"
        );
        let wound_down = events
            .iter()
            .any(|e| e.to_string().contains("The agent is unloading now"));
        match death {
            GateDeath::BeforeTheDrain | GateDeath::AtQuiesce => {
                assert!(!wound_down, "{death:?}: no wind-down after the drain")
            }
            GateDeath::InWindDown => assert!(cancelled, "the wind-down cancelled at the death"),
            GateDeath::AtLower => assert!(wound_down && !cancelled),
        }
    }
}

/// A frame's turn whose gate dies 300 ms into a generation that answers
/// 3 s late unless cancelled; with `leave`, a graceful leave is heard on
/// admin's connection first. Answers whether service stood, whether the
/// harness still stands entered with no gate, admin's answer where a
/// leave was sent, whether the generation was cancelled, and the record.
fn a_turn_whose_gate_dies(
    leave: bool,
) -> (
    bool,
    bool,
    Option<weaver_types::Payload>,
    bool,
    Vec<serde_json::Value>,
) {
    use nix::poll::{PollFd, PollFlags, poll};
    use nix::sys::socket::{
        AddressFamily, MsgFlags, SockFlag, SockType, recv as sock_recv, send as sock_send,
        socketpair,
    };
    use std::os::fd::{AsFd, AsRawFd};
    let (mut run, _spare, sink_path) = entered_run(None);
    releasing_spu(&mut run);
    let (near, far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .expect("socketpair");
    run.spu.as_mut().expect("the arm").decode = crate::channel::decode_from_owned(near);
    let decode_peer = std::thread::spawn(move || {
        let mut buf = vec![0u8; 65536];
        match sock_recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()) {
            Ok(n) if n > 0 => {}
            _ => return false,
        }
        let mut fds = [PollFd::new(far.as_fd(), PollFlags::POLLIN)];
        let cancelled = poll(&mut fds, 3_000u16).unwrap_or(0) > 0
            && sock_recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).is_ok_and(|n| {
                matches!(
                    serde_json::from_slice(&buf[..n]),
                    Ok(weaver_types::TokenDirective::Cancel { .. })
                )
            });
        let answer = weaver_types::TokenAnswer::Generated(weaver_types::Generation {
            unrecovered: vec![],
            content: vec![],
            emission: "late".into(),
            finish: if cancelled {
                weaver_types::Finish::Stopped
            } else {
                weaver_types::Finish::Completed
            },
            resident: 64,
            capacity: 4096,
            request: serde_json::value::RawValue::from_string(r#"{"rendered":"r"}"#.into())
                .unwrap(),
            measurement: serde_json::value::RawValue::from_string("{}".into()).unwrap(),
        });
        let _ = sock_send(
            far.as_raw_fd(),
            &serde_json::to_vec(&answer).unwrap(),
            MsgFlags::empty(),
        );
        cancelled
    });
    let (gate_end, gate_peer) = OrganChannel::pair().expect("gate pair");
    run.gate = Some(GateChannel {
        channel: gate_end,
        pid: nix::unistd::Pid::from_raw(i32::MAX),
        last_word: crate::spawn::LastWord::quiet(),
        shell_granted: true,
    });
    let gate_peer = gate_peer.into_channel();
    gate_peer
        .send(&OrganEnvelope {
            exchange: ExchangeId {
                opener: Opener::Gate,
                ordinal: 1,
            },
            position: Position::Open,
            payload: weaver_types::Payload::Frame(weaver_types::TurnFrame::carry(
                b"{\"text\":\"say a word\"}",
            )),
        })
        .expect("the frame sends");
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(300));
        drop(gate_peer);
    });
    let (verb_end, admin_end) = OrganChannel::pair().expect("admin pair");
    let admin = admin_end.into_channel();
    if leave {
        admin
            .send(&OrganEnvelope {
                exchange: ExchangeId {
                    opener: Opener::Admin,
                    ordinal: 4,
                },
                position: Position::Open,
                payload: weaver_types::Payload::Directive(LifecycleDirective::Leave {
                    cause: weaver_types::Cause { uid: 7 },
                    forced: false,
                    rollback: false,
                }),
            })
            .expect("the leave sends");
    }
    let (mut harness, _dir) = harness_entered(run);
    let mut verb_slot = Some(verb_end);
    let served = harness.serve_gate_wake(
        "identity",
        &mut |ports: &mut crate::engine::Ports<'_>, text: &str| {
            ports.turn(vec![weaver_traits::Message {
                role: weaver_traits::Role::User,
                content: vec![weaver_traits::ContentBlock::Text {
                    text: text.to_string(),
                }],
            }])
        },
        &mut verb_slot,
    );
    let standing = matches!(&harness.state, ChannelState::Entered(run) if run.gate.is_none());
    // Dropped first, so a leave never answered reads closed, not a hang.
    drop(harness);
    drop(verb_slot);
    let answer = leave
        .then(|| admin.recv().ok().map(|envelope| envelope.payload))
        .flatten();
    let cancelled = decode_peer.join().expect("the decode peer");
    let events = std::fs::read_to_string(&sink_path)
        .expect("the sink")
        .lines()
        .map(|line| serde_json::from_str(line).expect("a record line"))
        .collect();
    (served.is_ok(), standing, answer, cancelled, events)
}

/// **A gate dying while the SPU generates, no leave held, is a `fault`
/// and the run serves on** (`weaver-admin-Spec` 3.0, S3 x the gate dies;
/// the Planner's grade of #119): the turn goes on, its answer lost with
/// the gate, and the death is authored, the run entered with no gate.
/// Perturbation: send the answer with `?` again, and service ends.
#[test]
fn a_gate_dying_while_the_spu_generates_is_a_fault_and_the_run_serves_on() {
    let (served, standing, _, cancelled, events) = a_turn_whose_gate_dies(false);
    assert!(served, "service stands");
    assert!(standing, "the run stands entered, its gate taken");
    assert!(!cancelled, "with no leave the turn goes on");
    assert!(
        events
            .iter()
            .any(|e| e["kind"] == "fault" && e["payload"]["account"]["organ"] == "gate"),
        "the gate's death is a fault: {events:?}"
    );
}

/// **A gate dying mid-turn with a graceful leave held turns the leave
/// forced at that moment** (`weaver-admin-Spec` 3.0, S4 x the gate dies,
/// ruling (A); the Planner's grade of #119): the turn is cancelled, and
/// the leave carried out after it answers `Left { forced: true }`, no
/// wind-down run. Perturbation: leave the gate unwatched in the turn, and
/// the generation answers late and complete.
#[test]
fn a_gate_dying_mid_turn_with_a_leave_held_turns_it_forced() {
    let (served, _, answer, cancelled, events) = a_turn_whose_gate_dies(true);
    assert!(served, "the wake serves");
    assert!(cancelled, "the turn is cancelled at the gate's death");
    assert!(
        matches!(
            answer,
            Some(weaver_types::Payload::Answer(LifecycleAnswer::Left {
                forced: true,
                ..
            }))
        ),
        "{answer:?}"
    );
    assert!(
        !events
            .iter()
            .any(|e| e.to_string().contains("The agent is unloading now")),
        "no wind-down"
    );
}

/// **A rollback's `unload` is written forced, whatever the wire says**
/// (`weaver-admin-Spec` 3.0, S1 x `Leave`: a rollback is forced; the
/// Planner's grade of #119): a rollback directed with `forced` false
/// still runs no wind-down and authors `unload` and `Left` forced.
/// Perturbation: take `forced` from the wire, and both read false.
#[test]
fn a_rollback_is_written_forced_whatever_the_wire_says() {
    use crate::lifecycle_table::Leave;
    let (mut run, _spare, sink_path) = entered_run(None);
    releasing_spu(&mut run);
    // A gate standing, so ruling (A) does not turn the leave forced.
    let (gate_end, gate_peer) = OrganChannel::pair().expect("gate pair");
    run.gate = Some(GateChannel {
        channel: gate_end,
        pid: nix::unistd::Pid::from_raw(i32::MAX),
        last_word: crate::spawn::LastWord::quiet(),
        shell_granted: true,
    });
    let gate_peer = gate_peer.into_channel();
    std::thread::spawn(move || {
        while let Ok(asked) = gate_peer.recv() {
            let _ = gate_peer.send(&OrganEnvelope {
                exchange: asked.exchange,
                position: Position::Close,
                payload: weaver_types::Payload::Answer(LifecycleAnswer::GateStopped),
            });
        }
    });
    let (leave, admin) = pending_leave(Leave::Graceful, Leave::Graceful, true);
    run.leave = Some(leave);
    let (mut harness, _dir) = harness_entered(run);
    harness.carry_out_held_leave().expect("the leave runs");
    assert!(
        matches!(
            admin.recv().expect("the answer").payload,
            weaver_types::Payload::Answer(LifecycleAnswer::Left { forced: true, .. })
        ),
        "Left says forced"
    );
    let stream = std::fs::read_to_string(&sink_path).expect("the sink");
    let unload: serde_json::Value =
        serde_json::from_str(stream.lines().last().expect("a line")).expect("json");
    assert_eq!(unload["payload"]["forced"], true, "{unload}");
    assert!(
        !stream.contains("The agent is unloading now"),
        "no wind-down"
    );
}

/// **A diagnostic leave never stands in S5** (`weaver-admin-Spec` 3.0: a
/// diagnostic binding runs no wind-down, S4 to S6; the Planner's grade of
/// #119): the wind-down that does not run leaves the leave's step where
/// it was. Perturbation: step into `WindingDown` before the wind-down's
/// own checks, and the step reads S5.
#[test]
fn a_diagnostic_leave_never_steps_into_the_wind_down() {
    use crate::lifecycle_table::{Leave, LeaveStep};
    for (diagnostic, want) in [(true, LeaveStep::Draining), (false, LeaveStep::WindingDown)] {
        let (mut run, _spare, _sink) = entered_run(None);
        if diagnostic {
            run.diagnostic = Some(DiagnosticSeat {
                reader_elected: false,
                artifact: String::new(),
                replayed: true,
            });
        }
        let (leave, _admin) = pending_leave(Leave::Graceful, Leave::Graceful, false);
        let (mut harness, _dir) = harness_entered(run);
        let mut held = Some(leave);
        harness.wind_down(&mut held).expect("the wind-down");
        assert_eq!(
            held.expect("the leave").progress.step,
            want,
            "diagnostic = {diagnostic}: the step is S5 only where the wind-down runs"
        );
    }
}

/// **A turn refused with the SPU alive and a leave held leaves the leave
/// graceful** (the Planner's grade of #119; (A) turns a leave forced for
/// a dead organ alone): the SPU refuses the turn's append after the leave
/// is held, the turn closes stopped with its dialer answered, and the
/// leave is carried out graceful, the wind-down run and `Left` saying
/// `forced` false. Perturbation: turn the held leave forced on any turn
/// error, and `Left` says true.
#[test]
fn a_turn_refused_with_the_spu_alive_and_a_leave_held_stays_graceful() {
    use nix::sys::socket::{
        AddressFamily, MsgFlags, SockFlag, SockType, recv as sock_recv, send as sock_send,
        socketpair,
    };
    use std::os::fd::AsRawFd;
    let (mut run, _spare, sink_path) = entered_run(None);
    releasing_spu(&mut run);
    let (near, far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .expect("socketpair");
    run.spu.as_mut().expect("the arm").decode = crate::channel::decode_from_owned(near);
    // The turn's append refused after the leave is heard; the wind-down's
    // answered.
    let decode_peer = std::thread::spawn(move || {
        let mut buf = vec![0u8; 65536];
        sock_recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).expect("the turn's append");
        std::thread::sleep(std::time::Duration::from_millis(200));
        sock_send(
            far.as_raw_fd(),
            &serde_json::to_vec(&weaver_types::TokenRefusal::NotOpen).unwrap(),
            MsgFlags::empty(),
        )
        .expect("the refusal");
        let n = sock_recv(far.as_raw_fd(), &mut buf, MsgFlags::empty())
            .expect("the wind-down's append");
        assert!(n > 0, "the wind-down ran");
        let answer = weaver_types::TokenAnswer::Generated(weaver_types::Generation {
            unrecovered: vec![],
            content: vec![],
            emission: "the summary".into(),
            finish: weaver_types::Finish::Completed,
            resident: 64,
            capacity: 4096,
            request: serde_json::value::RawValue::from_string(r#"{"rendered":"r"}"#.into())
                .unwrap(),
            measurement: serde_json::value::RawValue::from_string("{}".into()).unwrap(),
        });
        sock_send(
            far.as_raw_fd(),
            &serde_json::to_vec(&answer).unwrap(),
            MsgFlags::empty(),
        )
        .expect("the summary");
        far
    });
    let (gate_end, gate_peer) = OrganChannel::pair().expect("gate pair");
    run.gate = Some(GateChannel {
        channel: gate_end,
        pid: nix::unistd::Pid::from_raw(i32::MAX),
        last_word: crate::spawn::LastWord::quiet(),
        shell_granted: true,
    });
    let gate_peer = gate_peer.into_channel();
    gate_peer
        .send(&OrganEnvelope {
            exchange: ExchangeId {
                opener: Opener::Gate,
                ordinal: 1,
            },
            position: Position::Open,
            payload: weaver_types::Payload::Frame(weaver_types::TurnFrame::carry(
                b"{\"text\":\"say a word\"}",
            )),
        })
        .expect("the frame sends");
    std::thread::spawn(move || {
        while let Ok(asked) = gate_peer.recv() {
            let answer = match asked.payload {
                weaver_types::Payload::Directive(LifecycleDirective::Quiesce) => {
                    LifecycleAnswer::GateQuiesced
                }
                weaver_types::Payload::Directive(LifecycleDirective::Lower) => {
                    LifecycleAnswer::GateStopped
                }
                _ => continue,
            };
            let _ = gate_peer.send(&OrganEnvelope {
                exchange: asked.exchange,
                position: Position::Close,
                payload: weaver_types::Payload::Answer(answer),
            });
        }
    });
    let (verb_end, admin_end) = OrganChannel::pair().expect("admin pair");
    let admin = admin_end.into_channel();
    admin
        .send(&OrganEnvelope {
            exchange: ExchangeId {
                opener: Opener::Admin,
                ordinal: 4,
            },
            position: Position::Open,
            payload: weaver_types::Payload::Directive(LifecycleDirective::Leave {
                cause: weaver_types::Cause { uid: 7 },
                forced: false,
                rollback: false,
            }),
        })
        .expect("the leave sends");
    let (mut harness, _dir) = harness_entered(run);
    let mut verb_slot = Some(verb_end);
    harness
        .serve_gate_wake(
            "identity",
            &mut |ports: &mut crate::engine::Ports<'_>, text: &str| {
                ports.turn(vec![weaver_traits::Message {
                    role: weaver_traits::Role::User,
                    content: vec![weaver_traits::ContentBlock::Text {
                        text: text.to_string(),
                    }],
                }])
            },
            &mut verb_slot,
        )
        .expect("the wake serves");
    drop(harness);
    drop(verb_slot);
    let _far = decode_peer.join().expect("the decode peer");
    let left = admin.recv().expect("the leave's answer").payload;
    assert!(
        matches!(
            left,
            weaver_types::Payload::Answer(LifecycleAnswer::Left { forced: false, .. })
        ),
        "{left:?}"
    );
    let stream = std::fs::read_to_string(&sink_path).expect("the sink");
    assert!(
        stream.contains("The agent is unloading now"),
        "the wind-down ran"
    );
}

/// **A gate flooding frames cannot hold the drain past its bound** (Codex
/// on #119, round 1, the same class as the turn's wait): the drain's wait
/// reads the clock before every wait, so a channel that never falls quiet
/// still meets the deadline, and the leave turns forced. Perturbation:
/// read the deadline only on a timed-out wait, and the drain runs on.
#[test]
fn a_gate_flooding_frames_cannot_hold_the_drain_past_its_bound() {
    use crate::lifecycle_table::Leave;
    let (mut run, _spare, _sink) = entered_run(None);
    let (gate_end, gate_peer) = OrganChannel::pair().expect("gate pair");
    run.gate = Some(GateChannel {
        channel: gate_end,
        pid: nix::unistd::Pid::from_raw(i32::MAX),
        last_word: crate::spawn::LastWord::quiet(),
        shell_granted: true,
    });
    let gate_peer = std::sync::Arc::new(gate_peer.into_channel());
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    // One thread reads (the quiesce, then every refusal), one floods.
    let reader = gate_peer.clone();
    std::thread::spawn(move || while reader.recv().is_ok() {});
    let (flooder, halt) = (gate_peer.clone(), stop.clone());
    let flood = std::thread::spawn(move || {
        let mut ordinal = 1;
        while !halt.load(std::sync::atomic::Ordering::Relaxed) {
            let _ = flooder.send(&OrganEnvelope {
                exchange: ExchangeId {
                    opener: Opener::Gate,
                    ordinal,
                },
                position: Position::Open,
                payload: weaver_types::Payload::Frame(weaver_types::TurnFrame::carry(
                    b"{\"text\":\"more\"}",
                )),
            });
            ordinal += 1;
        }
    });
    let (listener, _dir) = test_listener();
    let (mut leave, _admin) = pending_leave(Leave::Graceful, Leave::Graceful, false);
    leave.drain_deadline = Some(std::time::Instant::now() + std::time::Duration::from_millis(300));
    let started = std::time::Instant::now();
    drain_gate(&mut run, &listener, &mut leave);
    let took = started.elapsed();
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    drop(run);
    let _ = flood.join();
    assert_eq!(
        leave.progress.leave,
        Leave::Forced,
        "the bound turned it forced"
    );
    assert!(took < std::time::Duration::from_secs(3), "{took:?}");
}

/// **The lower stops only on its own `GateStopped`** (Codex on #119,
/// round 3; `weaver-harness-gate-contract` section 2): a drain cut by a
/// join leaves its `GateQuiesced` queued on the channel, and the lower
/// reads past it, on its own exchange, to the gate's `GateStopped`, so a
/// frame the gate still owes ahead of that answer is read and recorded
/// before the channel closes. Perturbation: take any answer as the stop,
/// and the stale `GateQuiesced` closes the lower with the frame unread.
#[test]
fn the_lower_reads_past_a_stale_answer_to_its_own_gate_stopped() {
    let (mut run, _spare, _sink) = entered_run(None);
    let (channel, gate_peer) = OrganChannel::pair().expect("pair");
    run.gate = Some(GateChannel {
        channel,
        pid: nix::unistd::Pid::from_raw(i32::MAX),
        last_word: crate::spawn::LastWord::quiet(),
        shell_granted: true,
    });
    // The drain's quiesce was exchange 1; its answer is still queued.
    run.gate_ordinal = 1;
    let gate_peer = gate_peer.into_channel();
    gate_peer
        .send(&OrganEnvelope {
            exchange: ExchangeId {
                opener: Opener::Harness,
                ordinal: 1,
            },
            position: Position::Close,
            payload: weaver_types::Payload::Answer(LifecycleAnswer::GateQuiesced),
        })
        .expect("the stale answer");
    let stand_in = std::thread::spawn(move || {
        let lower = gate_peer.recv().expect("the lower");
        // A frame the gate still owes, then its own answer.
        gate_peer
            .send(&OrganEnvelope {
                exchange: ExchangeId {
                    opener: Opener::Gate,
                    ordinal: 7,
                },
                position: Position::Open,
                payload: weaver_types::Payload::Frame(weaver_types::TurnFrame::carry(
                    b"{\"text\":\"owed\"}",
                )),
            })
            .expect("the owed frame");
        gate_peer
            .send(&OrganEnvelope {
                exchange: lower.exchange,
                position: Position::Close,
                payload: weaver_types::Payload::Answer(LifecycleAnswer::GateStopped),
            })
            .expect("the stop");
    });
    let (refused, died) = lower_gate_within(&mut run, 2_000, None);
    stand_in.join().expect("the stand-in");
    assert_eq!((refused, died), (None, false));
    let refusals = run
        .recorder
        .structure()
        .expect("the serving record")
        .iter()
        .filter(|r| r.kind == Kind::Refusal)
        .count();
    assert_eq!(
        refusals, 1,
        "the owed frame read and recorded before the close"
    );
}

/// **A standing SPU's decode end**: every append is answered with one
/// completed generation, `mark` run first, until the harness's end
/// closes. A graceful leave's wind-down turn reaches it.
fn answer_appends(far: crate::channel::ChildEnd, mark: impl Fn() + Send + 'static) {
    use nix::sys::socket::{MsgFlags, recv, send};
    use std::os::fd::AsRawFd;
    std::thread::spawn(move || {
        let fd = far.as_fd().as_raw_fd();
        let mut buf = vec![0u8; 65536];
        while let Ok(n) = recv(fd, &mut buf, MsgFlags::empty()) {
            if n == 0 {
                break;
            }
            let Ok(weaver_types::TokenDirective::AppendAndGenerate { .. }) =
                serde_json::from_slice(&buf[..n])
            else {
                continue;
            };
            mark();
            let summary = weaver_types::TokenAnswer::Generated(weaver_types::Generation {
                unrecovered: vec![],
                content: vec![],
                emission: "the summary".into(),
                finish: weaver_types::Finish::Completed,
                resident: 64,
                capacity: 4096,
                request: serde_json::value::RawValue::from_string(
                    r#"{"rendered":"r","template":"qwen2","sampling":{}}"#.into(),
                )
                .unwrap(),
                measurement: serde_json::value::RawValue::from_string(
                    r#"{"model":"m","weights_hash":"h","input_tokens":[1],"output_tokens":[2],"blocks":[],"timings":{"prefill_ns":"1","decode_ns":"2"}}"#.into(),
                )
                .unwrap(),
            });
            let _ = send(
                fd,
                &serde_json::to_vec(&summary).unwrap(),
                MsgFlags::empty(),
            );
        }
    });
}

/// A harness standing entered on `run`, for the organ-death cases.
fn harness_entered(run: Run) -> (Harness, crate::scratch::Scratch) {
    harness_at(ChannelState::Entered(Box::new(run)))
}

/// **A gate that dies at rest leaves the run serving** (`weaver-admin-Spec`
/// 3.0, S2 x the gate dies): a `fault` authored for the loss of
/// reachability, the gate's channel dropped, and the run still entered,
/// with no `unload`. Perturbation: unwind on the death as before, and the
/// run leaves.
#[test]
fn the_gate_dying_at_rest_authors_a_fault_and_serves_on() {
    let (mut run, _spare, sink_path) = entered_run(None);
    let (channel, far) = OrganChannel::pair().expect("pair");
    drop(far);
    run.gate = Some(GateChannel {
        channel,
        pid: nix::unistd::Pid::from_raw(i32::MAX),
        last_word: crate::spawn::LastWord::quiet(),
        shell_granted: true,
    });
    let (mut harness, _dir) = harness_entered(run);
    harness.organ_died(OrganDeath::Gate).expect("serves on");
    let ChannelState::Entered(run) = &harness.state else {
        panic!("the run serves on");
    };
    assert!(run.gate.is_none(), "the dead gate's channel is dropped");
    // The writer is asynchronous: drained before the sink is read.
    if let ChannelState::Entered(run) = &mut harness.state {
        run.recorder.drain().expect("drains");
    }
    let stream = std::fs::read_to_string(&sink_path).expect("the sink");
    assert!(stream.contains("\"kind\":\"fault\""), "{stream}");
    assert!(!stream.contains("\"kind\":\"unload\""), "{stream}");
}

/// **An SPU that dies at rest leaves the run standing with no model**
/// (S2 x the SPU dies, ruling (B)): a `fault` authored, the run entered,
/// its position `S2(Dead)`. Perturbation: unwind on the death, and the run
/// leaves.
#[test]
fn the_spu_dying_at_rest_authors_a_fault_and_the_run_stands_with_no_model() {
    let (run, _spare, sink_path) = entered_run(None);
    let (mut harness, _dir) = harness_entered(run);
    harness.organ_died(OrganDeath::Spu).expect("serves on");
    let ChannelState::Entered(run) = &harness.state else {
        panic!("the run stands");
    };
    assert_eq!(
        run.position(None),
        crate::lifecycle_table::Position::S2(crate::lifecycle_table::Spu::Dead)
    );
    // The writer is asynchronous: drained before the sink is read.
    if let ChannelState::Entered(run) = &mut harness.state {
        run.recorder.drain().expect("drains");
    }
    let stream = std::fs::read_to_string(&sink_path).expect("the sink");
    assert!(stream.contains("\"kind\":\"fault\""), "{stream}");
    assert!(!stream.contains("\"kind\":\"unload\""), "{stream}");
}

/// **The serve loop meets an organ's death through the table** (the
/// wiring of `organ_died`): with the loop serving an entered run, the
/// gate's channel and then the SPU's decode channel close at their far
/// ends; the loop authors a `fault` for each and serves on, the record
/// carrying no `unload`. The loop stays in its wait, which ends with the
/// test process. Perturbation: unwind on a closed gate or decode wake as
/// before, and the record closes with an `unload`.
#[test]
fn the_serve_loop_serves_on_past_an_organs_death_at_rest() {
    let (mut run, _spare, sink_path) = entered_run(None);
    let (channel, gate_far) = OrganChannel::pair().expect("pair");
    run.gate = Some(GateChannel {
        channel,
        pid: nix::unistd::Pid::from_raw(i32::MAX),
        last_word: crate::spawn::LastWord::quiet(),
        shell_granted: true,
    });
    let (decode, decode_far) = DecodeChannel::pair().expect("decode pair");
    if let Some(spu) = run.spu.as_mut() {
        spu.decode = decode;
    }
    let (harness, dir) = harness_entered(run);
    std::thread::spawn(move || {
        let _dir = dir;
        let _ = harness.serve(
            "identity",
            weaver_trace::LoopIdentity::compiled("test"),
            |_, _| unreachable!("no frame arrives"),
        );
    });
    let faults = |stream: &str| stream.matches("\"kind\":\"fault\"").count();
    let read_until = |want: usize| {
        for _ in 0..200 {
            let stream = std::fs::read_to_string(&sink_path).unwrap_or_default();
            if faults(&stream) >= want {
                return stream;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        std::fs::read_to_string(&sink_path).unwrap_or_default()
    };
    drop(gate_far);
    let stream = read_until(1);
    assert_eq!(faults(&stream), 1, "the gate's death: {stream}");
    drop(decode_far);
    let stream = read_until(2);
    assert_eq!(faults(&stream), 2, "the SPU's death: {stream}");
    std::thread::sleep(std::time::Duration::from_millis(50));
    let stream = std::fs::read_to_string(&sink_path).unwrap_or_default();
    assert!(
        !stream.contains("\"kind\":\"unload\""),
        "the run serves on: {stream}"
    );
}

/// **A request after the SPU died at rest is refused, no model being
/// resident** (`weaver-admin-Spec` 3.0 ruling (B);
/// `weaver-gate-world-contract` section 5): the gate's refused close
/// `no model is resident`, the turn never entered and the run standing.
/// Perturbation: admit the request as at `S2(Resident)`, and the loop is
/// entered against a dead decoder.
#[test]
fn a_request_after_the_spu_died_is_refused_no_model_is_resident() {
    let (mut run, _spare, _sink) = entered_run(None);
    run.spu = None;
    let (gate_end, gate_peer) = OrganChannel::pair().expect("gate pair");
    run.gate = Some(GateChannel {
        channel: gate_end,
        pid: nix::unistd::Pid::from_raw(1),
        last_word: crate::spawn::LastWord::quiet(),
        shell_granted: true,
    });
    let gate_peer = gate_peer.into_channel();
    let (mut harness, _dir) = harness_entered(run);
    let mut entered = false;
    let mut entry = |_: &mut crate::engine::Ports<'_>, _: &str| {
        entered = true;
        Err(crate::engine::TurnError::Unlicensed {
            turn: TurnKey("t-0".into()),
        })
    };
    gate_peer
        .send(&OrganEnvelope {
            exchange: ExchangeId {
                opener: Opener::Gate,
                ordinal: 1,
            },
            position: Position::Open,
            payload: weaver_types::Payload::Frame(weaver_types::TurnFrame::carry(
                br#"{"text":"hello"}"#,
            )),
        })
        .expect("frame sends");
    let mut verb_slot = None;
    harness
        .serve_gate_wake("", &mut entry, &mut verb_slot)
        .expect("the wake serves");
    let answer = gate_peer.recv().expect("the refusal returns");
    let weaver_types::Payload::Frame(frame) = answer.payload else {
        panic!("a frame answers a frame");
    };
    let line = String::from_utf8(frame.octets().expect("decodes")).expect("utf8");
    assert!(line.contains("no model is resident"), "{line}");
    assert!(line.contains("refused"), "{line}");
    assert!(!entered, "the loop is never entered");
    assert!(
        matches!(harness.state, ChannelState::Entered(_)),
        "the run stands"
    );
}

/// **A closure observed from the entered position unwinds before it
/// returns.** The recorder is drained and the bracket closed with an
/// `unload`, rather than the run being dropped with events still queued
/// and the organs left as orphans of a living parent.
///
/// Perturbation: empty `unwind_if_entered` and the position stays
/// entered. Watched under exactly that removal.
#[test]
fn closure_from_entered_unwinds_the_run() {
    let (run, _spare, sink_path) = entered_run(None);
    let (coordination, _coordination_dir) = test_listener();
    let mut harness = Harness {
        coordination,
        organs: OrganBinaries {
            classify: None,
            spu: "/nonexistent/spu".into(),
            gate: "/nonexistent/gate".into(),
        },
        parameters: OrganParameters::default(),
        state: ChannelState::Entered(Box::new(run)),
        composer: Some(weaver_trace::LoopIdentity::compiled("test")),
    };
    harness.unwind_if_entered(
        weaver_types::FaultCase::OrganDeathObserved,
        &ChannelFault::Undecodable,
    );
    match &harness.state {
        ChannelState::Left => {}
        _ => panic!("the position is terminal after an unwind"),
    }
    // The transition alone would pass with an unwind that did nothing, so
    // the effects are read off the sink: the bracket is closed and the
    // drain that closes it ran.
    let stream = std::fs::read_to_string(&sink_path).expect("the sink");
    assert!(
        stream.contains("\"kind\":\"unload\""),
        "the unwind closes the bracket with an unload: {stream}"
    );
    // Nobody asked for this unload, so it names no cause.
    let unload: serde_json::Value = serde_json::from_str(
        stream
            .lines()
            .find(|line| line.contains("\"kind\":\"unload\""))
            .unwrap(),
    )
    .unwrap();
    assert!(unload["payload"].get("cause").is_none(), "{unload}");
    assert!(
        stream.contains("\"kind\":\"load\""),
        "and the run it closes is the one that opened: {stream}"
    );
    // Issue #221's gap: the death recorded its why before the bracket
    // closed, inside the record whose reason for existing is that it
    // never lies.
    assert!(
        stream.contains("\"kind\":\"fault\""),
        "the ending authored a fault event: {stream}"
    );
    assert!(
        stream.contains("organ_death_observed") && stream.contains("Undecodable"),
        "the fault names its case and carries the fault's spelling: {stream}"
    );
}

/// **The death's account carries the organ's last word**, per
/// `weaver-harness-Spec` section 2.2 and issue #360: the SPU dies
/// printing a typed reason, and before this act the record held only
/// that a death was observed while the reason died with the journal.
///
/// Perturbation: drop the words loop from `unwind_if_entered` and the
/// account carries no `last_word` member. Watched under exactly that
/// removal, 2026-09-02.
///
/// conforms: harness-death-carries-the-last-word
#[test]
fn the_death_account_carries_the_last_word() {
    let (run, _spare, sink_path) = entered_run(None);
    run.spu
        .as_ref()
        .expect("the arm stands")
        .last_word
        .speak(r#"{"decode_fault":"device lost mid-forward"}"#);
    let (coordination, _coordination_dir) = test_listener();
    let mut harness = Harness {
        coordination,
        organs: OrganBinaries {
            classify: None,
            spu: "/nonexistent/spu".into(),
            gate: "/nonexistent/gate".into(),
        },
        parameters: OrganParameters::default(),
        state: ChannelState::Entered(Box::new(run)),
        composer: Some(weaver_trace::LoopIdentity::compiled("test")),
    };
    harness.unwind_if_entered(
        weaver_types::FaultCase::OrganDeathObserved,
        &ChannelFault::Undecodable,
    );
    let stream = std::fs::read_to_string(&sink_path).expect("the sink");
    assert!(
        stream.contains("last_word") && stream.contains("device lost mid-forward"),
        "the account carries the typed reason: {stream}"
    );
    // Parsed shape, not a quoted blob: the reason is queryable.
    assert!(
        stream.contains(r#"\"decode_fault\""#) || stream.contains("decode_fault"),
        "the spoken JSON rides as structure: {stream}"
    );
}

/// **The seat is granted at loaded-and-idle and not mid-turn.** Loop 0
/// has not taken the interior back while a turn is in flight, so there is
/// no standing interior to hand across.
///
/// Perturbation: drop the turn-in-flight guard from `grant_seat` and a
/// seat is handed over mid-turn. Watched under exactly that removal.
#[test]
fn no_seat_is_granted_while_a_turn_is_in_flight() {
    let (run, _spare, _path) = entered_run(Some("t-1"));
    let (coordination, _coordination_dir) = test_listener();
    let mut harness = Harness {
        coordination,
        organs: OrganBinaries {
            classify: None,
            spu: "/nonexistent/spu".into(),
            gate: "/nonexistent/gate".into(),
        },
        parameters: OrganParameters::default(),
        state: ChannelState::Entered(Box::new(run)),
        composer: Some(weaver_trace::LoopIdentity::compiled("test")),
    };
    assert!(
        harness.grant_seat("identity").is_none(),
        "an active run hands nothing across the seam"
    );

    // ...and an idle one does, so the guard is the turn and not the
    // position.
    let (idle, _spare, _path) = entered_run(None);
    let (coordination, _coordination_dir) = test_listener();
    let mut harness = Harness {
        coordination,
        organs: OrganBinaries {
            classify: None,
            spu: "/nonexistent/spu".into(),
            gate: "/nonexistent/gate".into(),
        },
        parameters: OrganParameters::default(),
        state: ChannelState::Entered(Box::new(idle)),
        composer: Some(weaver_trace::LoopIdentity::compiled("test")),
    };
    assert!(
        harness.grant_seat("identity").is_some(),
        "a loaded-and-idle run grants the seat"
    );
}

/// **The enter's grant reaches the raise uninterpreted, and the run's flag
/// is its presence** (#29, `weaver-harness-Spec` sections 1 and 5): a serving
/// binding with a grant raises carrying it and grants the shell, one without
/// raises carrying none and grants nothing, and a diagnostic binding raises
/// no gate.
///
/// Perturbation: raise with `shell: None`, or set the flag `true` always, and
/// one case fails.
#[test]
fn the_enters_grant_reaches_the_raise_and_sets_the_flag() {
    let instruction = weaver_types::GateInstruction {
        access_rule: weaver_types::AccessRule {
            allowed_uids: Default::default(),
            allowed_gids: Default::default(),
            denied_uids: Default::default(),
        },
    };
    let grant = weaver_types::ShellGrant {
        binary: "/usr/local/libexec/weaver/busybox".into(),
        commands: "/run/weaver.run/a/commands".into(),
    };
    let socket = PathBuf::from("/run/weaver.run/a/gate.sock");
    for shell in [Some(grant.clone()), None] {
        let binding = weaver_types::EnterBinding::Serving {
            gate_instruction: instruction.clone(),
            shell: shell.clone(),
        };
        let (raise, granted) = gate_raise(&binding, socket.clone()).expect("serving raises");
        assert_eq!(
            raise,
            LifecycleDirective::Raise {
                instruction: instruction.clone(),
                socket: socket.clone(),
                shell: shell.clone(),
            },
            "the grant crosses as the enter carried it"
        );
        assert_eq!(granted, shell.is_some(), "the flag is the grant's presence");
    }
    assert!(gate_raise(&weaver_types::EnterBinding::Diagnostic, socket).is_none());
}

/// **The seat's prompt offers the elected schemas alone** (#29,
/// `weaver-harness-Spec` section 5): the shell's schema where the run's gate
/// carries the grant, and none where the gate carries none or no gate stands.
/// The schema names `sh`, describes ash, and takes one `command`.
///
/// Perturbation: let `offered_schemas` ignore the grant and the ungranted
/// cases carry the schema.
#[test]
fn the_seat_offers_the_shell_schema_only_where_granted() {
    for granted in [None, Some(false), Some(true)] {
        let (mut run, _spare, _path) = entered_run(None);
        let (channel, _gate_peer) = OrganChannel::pair().expect("pair");
        run.gate = granted.map(|shell_granted| GateChannel {
            channel,
            pid: nix::unistd::Pid::from_raw(i32::MAX),
            last_word: crate::spawn::LastWord::quiet(),
            shell_granted,
        });
        let (coordination, _coordination_dir) = test_listener();
        let mut harness = Harness {
            coordination,
            organs: OrganBinaries {
                classify: None,
                spu: "/nonexistent/spu".into(),
                gate: "/nonexistent/gate".into(),
            },
            parameters: OrganParameters::default(),
            state: ChannelState::Entered(Box::new(run)),
            composer: Some(weaver_trace::LoopIdentity::compiled("test")),
        };
        let seat = harness
            .grant_seat("identity")
            .expect("an idle run grants the seat");
        let schemas = &seat.assembled().expect("the seat's prompt").tool_schemas;
        if granted == Some(true) {
            assert_eq!(schemas.len(), 1, "the shell's schema alone");
            let schema: serde_json::Value =
                serde_json::from_str(&schemas[0]).expect("the schema is JSON");
            assert_eq!(schema["name"], "sh");
            assert!(
                schema["description"]
                    .as_str()
                    .unwrap()
                    .contains("busybox ash")
            );
            assert_eq!(
                schema["parameters"]["required"],
                serde_json::json!(["command"])
            );
        } else {
            assert!(
                schemas.is_empty(),
                "{granted:?}: no tool offered: {schemas:?}"
            );
        }
    }
}

/// **A serving harness never writes the column ask.** The derivation
/// reads the binding and the election, and every combination but
/// diagnostic-with-readout asks nothing: the serving pair stay silent
/// whatever the election says, and a diagnostic load without the
/// election asks nothing rather than meeting the registry's second
/// arm.
///
/// Perturbation: derive the ask from the election alone and the
/// serving-with-readout combination writes it. Watched under exactly
/// that change.
///
/// conforms: harness-serving-writes-no-column-ask
#[test]
fn a_serving_harness_never_writes_the_column_ask() {
    assert!(!column_ask_for(false, false), "serving, unelected");
    assert!(
        !column_ask_for(false, true),
        "serving, elected: never the ask"
    );
    assert!(
        !column_ask_for(true, false),
        "diagnostic, unelected: nothing to continue"
    );
    assert!(
        column_ask_for(true, true),
        "diagnostic, elected: the one asking posture"
    );
}

/// **A diagnostic run grants no frame seat, and answers rather than
/// panics.** The extension seam is the frame path's: a diagnostic
/// record holds no working structure and no frame ever arrives, so the
/// grant answers the same absence the mid-turn arm answers, per
/// `weaver-harness-Spec` section 6.2's partition of the two criteria.
///
/// Perturbation: restore the `expect` on the structure read in
/// `grant_seat` and this panics instead of answering. Watched under
/// exactly that restoration. Found by the review seat on PR 393.
#[test]
fn a_diagnostic_run_grants_no_frame_seat() {
    let (mut run, _spare, _path) = entered_run(None);
    let sink_path = crate::scratch::Scratch(std::env::temp_dir().join(format!(
        "weaver-harness-diag-seat-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    )));
    let sink = OwnedFd::from(File::create(&sink_path).expect("sink"));
    run.recorder = crate::record::Record::from(
        weaver_diagnostic::Recorder::receive(
            sink,
            weaver_diagnostic::RunRef("r-1".into()),
            weaver_diagnostic::SessionRef("s-1".into()),
        )
        .expect("the recorder receives"),
    );
    let (coordination, _coordination_dir) = test_listener();
    let mut harness = Harness {
        coordination,
        organs: OrganBinaries {
            classify: None,
            spu: "/nonexistent/spu".into(),
            gate: "/nonexistent/gate".into(),
        },
        parameters: OrganParameters::default(),
        state: ChannelState::Entered(Box::new(run)),
        composer: Some(weaver_trace::LoopIdentity::compiled("test")),
    };
    assert!(
        harness.grant_seat("identity").is_none(),
        "a diagnostic run hands nothing across the frame seam"
    );
    let _ = std::fs::remove_file(&sink_path);
}

/// A stop at rest answers `AtRest`, a clean close and not a refusal, and
/// places no close event because there was no turn to close.
#[test]
fn stop_at_rest_answers_at_rest() {
    let (run, _spare, _sink_path) = entered_run(None);
    let (harness_end, peer_end) = OrganChannel::pair().expect("pair");
    let (coordination, _coordination_dir) = test_listener();
    let mut harness = Harness {
        coordination,
        organs: OrganBinaries {
            classify: None,
            spu: "/nonexistent/spu".into(),
            gate: "/nonexistent/gate".into(),
        },
        parameters: OrganParameters::default(),
        state: ChannelState::Entered(Box::new(run)),
        composer: Some(weaver_trace::LoopIdentity::compiled("test")),
    };
    harness
        .dispatch_on(
            &harness_end,
            test_exchange(),
            LifecycleDirective::Stop {
                cause: weaver_types::Cause { uid: 0 },
            },
            None,
            None,
        )
        .expect("stop");
    let peer = peer_end.into_channel();
    assert!(matches!(
        peer.recv().expect("answer").payload,
        weaver_types::Payload::Answer(LifecycleAnswer::AtRest)
    ));
}

/// **A close that opens no turn names none**, per
/// `weaver-gate-world-contract` section 3. A line that does not parse is
/// refused before the seat is granted, so there is no turn for the close
/// to identify and it says only what it is.
///
/// The distinction is the point rather than an omission: a named close
/// reports what became of a turn, and an unnamed one reports that a line
/// never became a turn at all. Perturbation: render the refusal with a
/// reconstructed key and this fails, which is what it is here to prevent,
/// a key rebuilt from the ordinal being a second chance to disagree with
/// the record.
#[test]
fn a_close_that_opens_no_turn_names_none() {
    let refused = render_close("refused", "reason", "the line is not a request", None);
    assert_eq!(
        refused,
        r#"{"kind":"refused","reason":"the line is not a request"}"#
    );
    assert!(!refused.contains("turn"), "no turn opened: {refused}");
    assert!(!refused.contains("run"), "no turn opened: {refused}");
}

/// conforms: harness-idle-report-authors-without-a-turn
/// An idle gate report is clerked once, without a turn, seat, or answer.
/// Every gate-owned case uses this dispatch arm; its account crosses as
/// handed over the real gate pair. The next coordination wake observes Idle.
/// Perturbations: open a turn in the Fault arm; send an answer there.
/// Each is detected independently, after the actual entered-state wait.
#[test]
fn an_idle_gate_report_authors_without_a_turn_or_answer() {
    use nix::poll::{PollFd, PollFlags, poll};
    use std::os::fd::AsFd;

    for case in [
        weaver_types::FaultCase::ListenerLost,
        weaver_types::FaultCase::ClientConnectionFailedMidTurn,
        weaver_types::FaultCase::AdmissionFailingSystematically,
    ] {
        let (mut run, _spare, sink) = entered_run(None);
        // Keep the decode peer alive so the real idle poll sees the
        // report, not the fixture's otherwise closed decode channel.
        let (decode, _decode_peer) = DecodeChannel::pair().expect("decode pair");
        run.spu.as_mut().unwrap().decode = decode;
        let (gate, gate_peer) = OrganChannel::pair().expect("gate pair");
        run.gate = Some(GateChannel {
            channel: gate,
            pid: nix::unistd::Pid::from_raw(1),
            last_word: crate::spawn::LastWord::quiet(),
            shell_granted: true,
        });
        let gate_peer = gate_peer.into_channel();
        let (coordination, _coordination_dir) = test_listener();
        let mut harness = Harness {
            coordination,
            organs: OrganBinaries {
                classify: None,
                spu: "/nonexistent".into(),
                gate: "/nonexistent".into(),
            },
            parameters: OrganParameters::default(),
            state: ChannelState::Entered(Box::new(run)),
            composer: Some(weaver_trace::LoopIdentity::compiled("test")),
        };
        let report = weaver_types::FaultReport {
            case,
            account: serde_json::value::RawValue::from_string(
                r#"{"z":1.00, "reason":"reported at rest","a":null}"#.into(),
            )
            .unwrap(),
        };
        gate_peer
            .send(&OrganEnvelope {
                exchange: ExchangeId {
                    opener: Opener::Gate,
                    ordinal: 7,
                },
                position: Position::Open,
                payload: weaver_types::Payload::Fault(report.clone()),
            })
            .expect("the fault-report sends");
        assert!(matches!(harness.wait(None), Ok(Wake::Gate)));
        harness
            .serve_gate_wake(
                "",
                &mut |_, _| panic!("an idle report must not grant the seat"),
                &mut None,
            )
            .expect("the report is clerked");

        let ChannelState::Entered(run) = &harness.state else {
            panic!("report must leave the run standing");
        };
        let rows: Vec<_> = run.recorder.structure().unwrap().iter().collect();
        assert_eq!(
            rows.iter().map(|row| row.kind).collect::<Vec<_>>(),
            vec![Kind::Load, Kind::Fault],
            "exactly one fault, no turn.started or other work"
        );
        assert!(rows[1].turn.is_none(), "fault has no turn");
        let fault: std::collections::BTreeMap<String, Box<serde_json::value::RawValue>> =
            serde_json::from_str(&rows[1].line).unwrap();
        assert!(!fault.contains_key("turn"), "turn is absent at the wire");
        assert_eq!(fault["subsystem"].get(), r#""gate""#);
        let carried: weaver_types::FaultReport =
            serde_json::from_str(fault["payload"].get()).unwrap();
        assert_eq!(
            carried, report,
            "the gate's case and raw account cross unchanged"
        );
        assert_eq!(run.turn_ordinal, 0);
        assert!(run.turn_in_flight.is_none());
        let mut fds = [PollFd::new(gate_peer.as_fd(), PollFlags::POLLIN)];
        assert_eq!(
            poll(&mut fds, 0u16).unwrap(),
            0,
            "a report is owed no gate answer"
        );

        let (connection, admin_peer) = OrganChannel::pair().expect("observe pair");
        let admin_peer = admin_peer.into_channel();
        admin_peer
            .send(&OrganEnvelope {
                exchange: test_exchange(),
                position: Position::Open,
                payload: weaver_types::Payload::Directive(LifecycleDirective::Observe),
            })
            .unwrap();
        assert!(matches!(
            harness.wait(Some(&connection)),
            Ok(Wake::Connection)
        ));
        let observe = connection.recv().unwrap();
        let weaver_types::Payload::Directive(directive) = observe.payload else {
            panic!("observe is a directive");
        };
        harness
            .dispatch_on(&connection, observe.exchange, directive, None, None)
            .unwrap();
        assert!(
            matches!(
                admin_peer.recv().unwrap().payload,
                weaver_types::Payload::Answer(LifecycleAnswer::State {
                    state: weaver_types::AgentState::Idle,
                    load: Some(_),
                    ..
                })
            ),
            "the next observe answers Idle with the standing load"
        );
        drop(harness);
        std::fs::remove_file(sink).unwrap();
    }
}

/// **A graceful leave heard mid-turn is held and carried out after the
/// turn** (`weaver-admin-Spec` 3.0, S3 x unload): the frame's turn hears
/// the leave on admin's connection, runs to its own close and answers its
/// dialer, and only then is the leave carried out: the gate lowered, the
/// run left, and admin answered `Left` on the connection the leave came
/// on. Perturbation: grant the frame's seat no leave slot, and the leave
/// is refused `ActivityNotAtRest` mid-turn as before.
#[test]
fn a_graceful_leave_heard_mid_turn_is_carried_out_after_the_turn() {
    use std::os::fd::AsRawFd;

    use nix::sys::socket::{
        AddressFamily, MsgFlags, SockFlag, SockType, recv as sock_recv, send as sock_send,
        socketpair,
    };

    let (mut run, _spare, _sink) = entered_run(None);
    let (near, far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .expect("socketpair");
    run.spu.as_mut().expect("the arm stands").decode = crate::channel::decode_from_owned(near);
    // The SPU's lifecycle end answers its release, as a standing SPU does.
    let (spu_lifecycle, spu_far) = OrganChannel::pair().expect("spu lifecycle pair");
    run.spu.as_mut().expect("the arm stands").lifecycle = spu_lifecycle;
    let spu_far = spu_far.into_channel();
    let _spu_stand_in = std::thread::spawn(move || {
        while let Ok(asked) = spu_far.recv() {
            let _ = spu_far.send(&OrganEnvelope {
                exchange: asked.exchange,
                position: Position::Close,
                payload: weaver_types::Payload::Answer(LifecycleAnswer::Released),
            });
        }
    });
    let (gate_end, gate_peer) = OrganChannel::pair().expect("gate pair");
    run.gate = Some(GateChannel {
        channel: gate_end,
        pid: nix::unistd::Pid::from_raw(i32::MAX),
        last_word: crate::spawn::LastWord::quiet(),
        shell_granted: true,
    });
    let gate_peer = gate_peer.into_channel();
    let decode_peer = std::thread::spawn(move || {
        let mut buf = vec![0u8; 65536];
        let n = sock_recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).expect("recv append");
        let _: weaver_types::TokenDirective =
            serde_json::from_slice(&buf[..n]).expect("append parses");
        let request = serde_json::value::RawValue::from_string(
            r#"{"rendered":"user: say a word","template":"qwen2","sampling":{}}"#.to_string(),
        )
        .unwrap();
        let measurement = serde_json::value::RawValue::from_string(
            r#"{"model":"m","weights_hash":"h","input_tokens":[1],"output_tokens":[2],"blocks":[],"timings":{"prefill_ns":"1","decode_ns":"2"}}"#
                .to_string(),
        )
        .unwrap();
        let answer = weaver_types::TokenAnswer::Generated(weaver_types::Generation {
            unrecovered: vec![],
            content: vec![],
            emission: "one word".into(),
            finish: weaver_types::Finish::Completed,
            request,
            measurement,
            resident: 64,
            capacity: 4096,
        });
        let bytes = serde_json::to_vec(&answer).expect("answer renders");
        sock_send(far.as_raw_fd(), &bytes, MsgFlags::empty()).expect("send answer");
        // The held leave's wind-down, after the turn, answered the same.
        let n =
            sock_recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).expect("recv the wind-down");
        let _: weaver_types::TokenDirective =
            serde_json::from_slice(&buf[..n]).expect("the wind-down parses");
        sock_send(far.as_raw_fd(), &bytes, MsgFlags::empty()).expect("send the summary");
        far
    });
    // The frame stands on the channel before the wake is served.
    gate_peer
        .send(&OrganEnvelope {
            exchange: ExchangeId {
                opener: Opener::Gate,
                ordinal: 1,
            },
            position: Position::Open,
            payload: weaver_types::Payload::Frame(weaver_types::TurnFrame::carry(
                b"{\"text\":\"say a word\"}",
            )),
        })
        .expect("frame sends");
    // The gate stand-in: the frame's answer first, then the drain's
    // quiesce and the lower, each answered.
    let gate_stand_in = std::thread::spawn(move || {
        let answered = gate_peer.recv().expect("the frame's answer");
        loop {
            let asked = gate_peer.recv().expect("the leave's gate steps");
            let answer = match asked.payload {
                weaver_types::Payload::Directive(LifecycleDirective::Quiesce) => {
                    LifecycleAnswer::GateQuiesced
                }
                weaver_types::Payload::Directive(LifecycleDirective::Lower) => {
                    LifecycleAnswer::GateStopped
                }
                other => panic!("unexpected gate traffic: {other:?}"),
            };
            let stopped = answer == LifecycleAnswer::GateStopped;
            gate_peer
                .send(&OrganEnvelope {
                    exchange: asked.exchange,
                    position: Position::Close,
                    payload: weaver_types::Payload::Answer(answer),
                })
                .expect("answered");
            if stopped {
                return answered;
            }
        }
    });
    let (verb_end, admin_end) = OrganChannel::pair().expect("admin pair");
    let admin = admin_end.into_channel();
    admin
        .send(&OrganEnvelope {
            exchange: ExchangeId {
                opener: Opener::Admin,
                ordinal: 4,
            },
            position: Position::Open,
            payload: weaver_types::Payload::Directive(LifecycleDirective::Leave {
                cause: weaver_types::Cause { uid: 7 },
                forced: false,
                rollback: false,
            }),
        })
        .expect("the leave sends");
    let (mut harness, _dir) = harness_entered(run);
    let mut verb_slot = Some(verb_end);
    harness
        .serve_gate_wake(
            "identity",
            &mut |ports: &mut crate::engine::Ports<'_>, text: &str| {
                let delta = vec![weaver_traits::Message {
                    role: weaver_traits::Role::User,
                    content: vec![weaver_traits::ContentBlock::Text {
                        text: text.to_string(),
                    }],
                }];
                ports.turn(delta)
            },
            &mut verb_slot,
        )
        .expect("the wake serves");
    let _decode = decode_peer.join().expect("the decode peer finishes");
    // Admin's answer is read first: a leave refused mid-turn lowers no
    // gate, and the stand-in would wait on its lower.
    let left = admin.recv().expect("the leave's answer");
    assert!(
        matches!(
            left.payload,
            weaver_types::Payload::Answer(LifecycleAnswer::Left { .. })
        ),
        "{left:?}"
    );
    assert_eq!(left.exchange.ordinal, 4, "on the leave's own exchange");
    let answered = gate_stand_in.join().expect("the gate stand-in finishes");
    let weaver_types::Payload::Frame(frame) = answered.payload else {
        panic!("the dialer's answer first: {answered:?}");
    };
    let line = String::from_utf8(frame.octets().expect("canonical")).expect("utf8");
    assert!(line.contains(r#""kind":"answered""#), "{line}");
    assert!(matches!(harness.state, ChannelState::Left));
}

/// **A turn that errors while a leave is held does not unwind the
/// leave** (the Planner's ruling of 2026-10-09, by the pattern of (B)):
/// the decode channel lost mid-turn closes the turn failed, `Stopped {
/// reason: fault }`, its dialer answered, and the held leave carries on,
/// turned forced by (A) since the SPU is unusable, admin answered `Left`
/// on the leave's own connection. Only a worker that dies leaves admin no
/// answer. Perturbation: end service on the lost channel as before, and
/// admin reads no answer.
#[test]
fn a_turn_that_errors_with_a_leave_held_still_answers_left() {
    use std::os::fd::AsRawFd;

    use nix::sys::socket::{
        AddressFamily, MsgFlags, SockFlag, SockType, recv as sock_recv, socketpair,
    };

    let (mut run, _spare, sink_path) = entered_run(None);
    let (near, far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .expect("socketpair");
    run.spu.as_mut().expect("the arm stands").decode = crate::channel::decode_from_owned(near);
    let (gate_end, gate_peer) = OrganChannel::pair().expect("gate pair");
    run.gate = Some(GateChannel {
        channel: gate_end,
        pid: nix::unistd::Pid::from_raw(i32::MAX),
        last_word: crate::spawn::LastWord::quiet(),
        shell_granted: true,
    });
    let gate_peer = gate_peer.into_channel();
    // The SPU reads the append, lets the leave be heard, then dies.
    let decode_peer = std::thread::spawn(move || {
        let mut buf = vec![0u8; 65536];
        let _ = sock_recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).expect("the append");
        std::thread::sleep(std::time::Duration::from_millis(150));
        drop(far);
    });
    gate_peer
        .send(&OrganEnvelope {
            exchange: ExchangeId {
                opener: Opener::Gate,
                ordinal: 1,
            },
            position: Position::Open,
            payload: weaver_types::Payload::Frame(weaver_types::TurnFrame::carry(
                b"{\"text\":\"say a word\"}",
            )),
        })
        .expect("frame sends");
    let gate_stand_in = std::thread::spawn(move || {
        let mut seen = Vec::new();
        while let Ok(envelope) = gate_peer.recv() {
            if let weaver_types::Payload::Directive(LifecycleDirective::Lower) = envelope.payload {
                let _ = gate_peer.send(&OrganEnvelope {
                    exchange: envelope.exchange.clone(),
                    position: Position::Close,
                    payload: weaver_types::Payload::Answer(LifecycleAnswer::GateStopped),
                });
            }
            seen.push(envelope);
        }
        seen
    });
    let (verb_end, admin_end) = OrganChannel::pair().expect("admin pair");
    let admin = admin_end.into_channel();
    admin
        .send(&OrganEnvelope {
            exchange: ExchangeId {
                opener: Opener::Admin,
                ordinal: 4,
            },
            position: Position::Open,
            payload: weaver_types::Payload::Directive(LifecycleDirective::Leave {
                cause: weaver_types::Cause { uid: 7 },
                forced: false,
                rollback: false,
            }),
        })
        .expect("the leave sends");
    let (mut harness, _dir) = harness_entered(run);
    let mut verb_slot = Some(verb_end);
    let served = harness.serve_gate_wake(
        "identity",
        &mut |ports: &mut crate::engine::Ports<'_>, text: &str| {
            let delta = vec![weaver_traits::Message {
                role: weaver_traits::Role::User,
                content: vec![weaver_traits::ContentBlock::Text {
                    text: text.to_string(),
                }],
            }];
            ports.turn(delta)
        },
        &mut verb_slot,
    );
    decode_peer.join().expect("the decode peer finishes");
    assert!(
        served.is_ok(),
        "service does not end on the turn's error: {served:?}"
    );
    let left = admin.recv().expect("admin is answered");
    let weaver_types::Payload::Answer(LifecycleAnswer::Left { forced, .. }) = left.payload else {
        panic!("the held leave is answered Left: {left:?}");
    };
    assert!(forced, "the SPU unusable, (A) turns the leave forced");
    assert!(matches!(harness.state, ChannelState::Left));
    drop(harness);
    let seen = gate_stand_in.join().expect("the gate stand-in");
    let answered = seen
        .iter()
        .find_map(|envelope| match &envelope.payload {
            weaver_types::Payload::Frame(frame) => frame.octets(),
            _ => None,
        })
        .expect("the dialer is answered");
    assert!(String::from_utf8_lossy(&answered).contains("stopped"));
    let stream = std::fs::read_to_string(&sink_path).expect("the sink");
    assert!(
        stream.contains(r#""reason":"fault""#),
        "the turn closes failed: {stream}"
    );
}

/// **A gate that dies during a call with a leave held turns the leave
/// forced and leaves the SPU standing** (the same ruling, met in the tool
/// wait): the error out of the turn is the gate's, told by its channel's
/// hang-up, so the SPU is not taken for dead; the leave carries on forced
/// by (A) and admin is answered `Left`. Perturbation: blame the SPU for
/// every lost channel, and the SPU's release is never asked.
#[test]
fn a_gate_dying_during_a_call_with_a_leave_held_forces_the_leave() {
    use std::os::fd::AsRawFd;

    use nix::sys::socket::{
        AddressFamily, MsgFlags, SockFlag, SockType, recv as sock_recv, send as sock_send,
        socketpair,
    };

    let (mut run, _spare, sink_path) = entered_run(None);
    let (near, far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .expect("socketpair");
    run.spu.as_mut().expect("the arm stands").decode = crate::channel::decode_from_owned(near);
    let (spu_lifecycle, spu_far) = OrganChannel::pair().expect("spu lifecycle pair");
    run.spu.as_mut().expect("the arm stands").lifecycle = spu_lifecycle;
    let spu_far = spu_far.into_channel();
    let releases = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = releases.clone();
    std::thread::spawn(move || {
        while let Ok(asked) = spu_far.recv() {
            counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let _ = spu_far.send(&OrganEnvelope {
                exchange: asked.exchange,
                position: Position::Close,
                payload: weaver_types::Payload::Answer(LifecycleAnswer::Released),
            });
        }
    });
    let (gate_end, gate_peer) = OrganChannel::pair().expect("gate pair");
    run.gate = Some(GateChannel {
        channel: gate_end,
        pid: nix::unistd::Pid::from_raw(i32::MAX),
        last_word: crate::spawn::LastWord::quiet(),
        shell_granted: true,
    });
    let gate_peer = gate_peer.into_channel();
    // The SPU asks one tool call and stays standing.
    let decode_peer = std::thread::spawn(move || {
        let mut buf = vec![0u8; 65536];
        let _ = sock_recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).expect("the append");
        let call = weaver_types::TokenAnswer::Generated(weaver_types::Generation {
            unrecovered: vec![],
            content: vec![weaver_traits::ContentBlock::ToolCall(
                weaver_traits::ToolCall {
                    name: "sh".into(),
                    arguments: r#"{"command":"sleep 5"}"#.into(),
                },
            )],
            emission: "calling".into(),
            finish: weaver_types::Finish::Completed,
            resident: 64,
            capacity: 4096,
            request: serde_json::value::RawValue::from_string(r#"{"rendered":"r"}"#.into())
                .unwrap(),
            measurement: serde_json::value::RawValue::from_string(r#"{}"#.into()).unwrap(),
        });
        sock_send(
            far.as_raw_fd(),
            &serde_json::to_vec(&call).unwrap(),
            MsgFlags::empty(),
        )
        .expect("the call");
        far
    });
    gate_peer
        .send(&OrganEnvelope {
            exchange: ExchangeId {
                opener: Opener::Gate,
                ordinal: 1,
            },
            position: Position::Open,
            payload: weaver_types::Payload::Frame(weaver_types::TurnFrame::carry(
                b"{\"text\":\"run something\"}",
            )),
        })
        .expect("frame sends");
    let (verb_end, admin_end) = OrganChannel::pair().expect("admin pair");
    let admin = admin_end.into_channel();
    // The gate takes the call; the leave is heard during the call's wait;
    // the gate then dies with the call open.
    let gate_stand_in = std::thread::spawn(move || {
        let open = gate_peer.recv().expect("the call");
        assert!(
            matches!(open.payload, weaver_types::Payload::Tool(_)),
            "{open:?}"
        );
        admin
            .send(&OrganEnvelope {
                exchange: ExchangeId {
                    opener: Opener::Admin,
                    ordinal: 4,
                },
                position: Position::Open,
                payload: weaver_types::Payload::Directive(LifecycleDirective::Leave {
                    cause: weaver_types::Cause { uid: 7 },
                    forced: false,
                    rollback: false,
                }),
            })
            .expect("the leave sends");
        std::thread::sleep(std::time::Duration::from_millis(150));
        drop(gate_peer);
        admin
    });
    let (mut harness, _dir) = harness_entered(run);
    let mut verb_slot = Some(verb_end);
    let served = harness.serve_gate_wake(
        "identity",
        &mut |ports: &mut crate::engine::Ports<'_>, text: &str| {
            let delta = vec![weaver_traits::Message {
                role: weaver_traits::Role::User,
                content: vec![weaver_traits::ContentBlock::Text {
                    text: text.to_string(),
                }],
            }];
            ports.turn(delta)
        },
        &mut verb_slot,
    );
    let admin = gate_stand_in.join().expect("the gate stand-in");
    assert!(served.is_ok(), "{served:?}");
    let left = admin.recv().expect("admin is answered");
    let weaver_types::Payload::Answer(LifecycleAnswer::Left { forced, .. }) = left.payload else {
        panic!("{left:?}");
    };
    assert!(forced, "a dead gate turns the leave forced (A)");
    assert_eq!(
        releases.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "the standing SPU is released, not taken for dead"
    );
    let stream = std::fs::read_to_string(&sink_path).expect("the sink");
    assert!(
        stream.contains(r#""organ":"gate""#),
        "the gate's death: {stream}"
    );
    assert!(
        stream.contains(r#""by":"fault""#),
        "the call open at the dead gate closes killed by the fault: {stream}"
    );
    drop(decode_peer.join());
}

/// **The SPU dying mid-turn fails the turn and the run stands** (3.0, S3 x
/// the SPU dies, ruling (B)): the turn closes `Stopped { reason: fault }`,
/// its dialer is answered, the SPU's death is recorded, and a later
/// request is refused `no model is resident`, the run still entered.
/// Perturbation: end service on the lost decode channel as before, and the
/// run leaves.
#[test]
fn the_spu_dying_mid_turn_fails_the_turn_and_the_run_stands() {
    use std::os::fd::AsRawFd;

    use nix::sys::socket::{
        AddressFamily, MsgFlags, SockFlag, SockType, recv as sock_recv, socketpair,
    };

    let (mut run, _spare, sink_path) = entered_run(None);
    let (near, far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .expect("socketpair");
    run.spu.as_mut().expect("the arm stands").decode = crate::channel::decode_from_owned(near);
    let (gate_end, gate_peer) = OrganChannel::pair().expect("gate pair");
    run.gate = Some(GateChannel {
        channel: gate_end,
        pid: nix::unistd::Pid::from_raw(i32::MAX),
        last_word: crate::spawn::LastWord::quiet(),
        shell_granted: true,
    });
    let gate_peer = gate_peer.into_channel();
    let decode_peer = std::thread::spawn(move || {
        let mut buf = vec![0u8; 65536];
        let _ = sock_recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).expect("the append");
        drop(far);
    });
    let frame = |ordinal| OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Gate,
            ordinal,
        },
        position: Position::Open,
        payload: weaver_types::Payload::Frame(weaver_types::TurnFrame::carry(
            b"{\"text\":\"say a word\"}",
        )),
    };
    gate_peer.send(&frame(1)).expect("frame sends");
    let (mut harness, _dir) = harness_entered(run);
    let mut entry = |ports: &mut crate::engine::Ports<'_>, text: &str| {
        let delta = vec![weaver_traits::Message {
            role: weaver_traits::Role::User,
            content: vec![weaver_traits::ContentBlock::Text {
                text: text.to_string(),
            }],
        }];
        ports.turn(delta)
    };
    let served = harness.serve_gate_wake("identity", &mut entry, &mut None);
    decode_peer.join().expect("the decode peer");
    assert!(served.is_ok(), "the run stands: {served:?}");
    let answered = gate_peer.recv().expect("the dialer's answer");
    let weaver_types::Payload::Frame(answer) = answered.payload else {
        panic!("{answered:?}");
    };
    assert!(String::from_utf8_lossy(&answer.octets().expect("utf8")).contains("stopped"));
    // A later request meets no model.
    gate_peer.send(&frame(2)).expect("frame sends");
    harness
        .serve_gate_wake("identity", &mut entry, &mut None)
        .expect("the wake serves");
    let refused = gate_peer.recv().expect("the refusal");
    let weaver_types::Payload::Frame(refused) = refused.payload else {
        panic!("{refused:?}");
    };
    assert!(
        String::from_utf8_lossy(&refused.octets().expect("utf8")).contains("no model is resident")
    );
    let ChannelState::Entered(run) = &mut harness.state else {
        panic!("the run stands");
    };
    run.recorder.drain().expect("drains");
    let stream = std::fs::read_to_string(&sink_path).expect("the sink");
    assert!(
        stream.contains(r#""reason":"fault""#),
        "the turn closes failed: {stream}"
    );
    assert!(
        stream.contains(r#""organ":"spu""#),
        "the SPU's death: {stream}"
    );
}

/// **A lost channel with no organ hung up ends service, a leave held or
/// not, and tears no organ down** (the security review of 1b50cae): the
/// decode seam answers octets that do not decode, its channel standing,
/// so the loss is not an organ's death the table can meet; nothing is
/// taken for dead and no `Left` claims a clean close. Perturbation: take
/// the SPU for the one lost where nothing hung up, and the SPU is torn
/// down and the leave answered.
#[test]
fn a_lost_channel_with_no_hang_up_ends_service_and_tears_nothing_down() {
    use std::os::fd::AsRawFd;

    use nix::sys::socket::{
        AddressFamily, MsgFlags, SockFlag, SockType, recv as sock_recv, send as sock_send,
        socketpair,
    };

    let (mut run, _spare, _sink) = entered_run(None);
    let (near, far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .expect("socketpair");
    run.spu.as_mut().expect("the arm stands").decode = crate::channel::decode_from_owned(near);
    let (gate_end, gate_peer) = OrganChannel::pair().expect("gate pair");
    run.gate = Some(GateChannel {
        channel: gate_end,
        pid: nix::unistd::Pid::from_raw(i32::MAX),
        last_word: crate::spawn::LastWord::quiet(),
        shell_granted: true,
    });
    let gate_peer = gate_peer.into_channel();
    // The SPU lets the leave be heard, then answers what does not decode,
    // its channel left standing.
    let decode_peer = std::thread::spawn(move || {
        let mut buf = vec![0u8; 65536];
        let _ = sock_recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).expect("the append");
        std::thread::sleep(std::time::Duration::from_millis(150));
        sock_send(far.as_raw_fd(), b"not an answer", MsgFlags::empty()).expect("garbage");
        far
    });
    gate_peer
        .send(&OrganEnvelope {
            exchange: ExchangeId {
                opener: Opener::Gate,
                ordinal: 1,
            },
            position: Position::Open,
            payload: weaver_types::Payload::Frame(weaver_types::TurnFrame::carry(
                b"{\"text\":\"say a word\"}",
            )),
        })
        .expect("frame sends");
    // The gate answers whatever it is asked, so a wrongly carried-out
    // leave shows as an answer rather than a hang.
    std::thread::spawn(move || {
        while let Ok(envelope) = gate_peer.recv() {
            if let weaver_types::Payload::Directive(LifecycleDirective::Lower) = envelope.payload {
                let _ = gate_peer.send(&OrganEnvelope {
                    exchange: envelope.exchange,
                    position: Position::Close,
                    payload: weaver_types::Payload::Answer(LifecycleAnswer::GateStopped),
                });
            }
        }
    });
    let (verb_end, admin_end) = OrganChannel::pair().expect("admin pair");
    let admin = admin_end.into_channel();
    admin
        .send(&OrganEnvelope {
            exchange: ExchangeId {
                opener: Opener::Admin,
                ordinal: 4,
            },
            position: Position::Open,
            payload: weaver_types::Payload::Directive(LifecycleDirective::Leave {
                cause: weaver_types::Cause { uid: 7 },
                forced: false,
                rollback: false,
            }),
        })
        .expect("the leave sends");
    let (mut harness, _dir) = harness_entered(run);
    let mut verb_slot = Some(verb_end);
    let served = harness.serve_gate_wake(
        "identity",
        &mut |ports: &mut crate::engine::Ports<'_>, text: &str| {
            let delta = vec![weaver_traits::Message {
                role: weaver_traits::Role::User,
                content: vec![weaver_traits::ContentBlock::Text {
                    text: text.to_string(),
                }],
            }];
            ports.turn(delta)
        },
        &mut verb_slot,
    );
    let _far = decode_peer.join().expect("the decode peer");
    assert!(
        served.is_err(),
        "service ends on a loss no organ explains: {served:?}"
    );
    let ChannelState::Entered(run) = &harness.state else {
        panic!("the serve loop, not this wake, unwinds the run");
    };
    assert!(run.spu.is_some(), "the SPU is not torn down as dead");
}

/// **An organ's death met with the record broken ends service, tearing
/// nothing down** (the Planner's ruling of 2026-10-09 on the review of
/// 7313a2b): the trace is the deliverable, so where the turn's close or
/// the death's fault cannot be authored, the run does not serve on with
/// those records missing, and no `Left` is answered over it. The record
/// here writes to a device that refuses every write. Perturbation: ignore
/// the authoring's result, and the run serves on.
#[test]
fn an_organ_death_met_with_the_record_broken_ends_service() {
    let (mut run, _spare, _sink) = entered_run(None);
    let full = OwnedFd::from(
        std::fs::OpenOptions::new()
            .write(true)
            .open("/dev/full")
            .expect("the full device"),
    );
    let mut recorder = crate::record::Record::from(
        Recorder::receive(full, RunRef("r-1".into()), SessionRef("s-1".into())).expect("recorder"),
    );
    let _ = run.author.author_fault(
        &mut recorder,
        Subsystem::Harness,
        None,
        &crate::authorship::harness_report(weaver_types::FaultCase::OrganDeathObserved, "{}"),
    );
    assert!(recorder.drain().is_err(), "the writer has failed");
    run.recorder = recorder;
    // The SPU has hung up before the turn begins.
    let (near, far) = nix::sys::socket::socketpair(
        nix::sys::socket::AddressFamily::Unix,
        nix::sys::socket::SockType::SeqPacket,
        None,
        nix::sys::socket::SockFlag::SOCK_CLOEXEC,
    )
    .expect("socketpair");
    drop(far);
    run.spu.as_mut().expect("the arm stands").decode = crate::channel::decode_from_owned(near);
    let (gate_end, gate_peer) = OrganChannel::pair().expect("gate pair");
    run.gate = Some(GateChannel {
        channel: gate_end,
        pid: nix::unistd::Pid::from_raw(i32::MAX),
        last_word: crate::spawn::LastWord::quiet(),
        shell_granted: true,
    });
    let gate_peer = gate_peer.into_channel();
    gate_peer
        .send(&OrganEnvelope {
            exchange: ExchangeId {
                opener: Opener::Gate,
                ordinal: 1,
            },
            position: Position::Open,
            payload: weaver_types::Payload::Frame(weaver_types::TurnFrame::carry(
                b"{\"text\":\"say a word\"}",
            )),
        })
        .expect("frame sends");
    let (mut harness, _dir) = harness_entered(run);
    let served = harness.serve_gate_wake(
        "identity",
        &mut |ports: &mut crate::engine::Ports<'_>, text: &str| {
            let delta = vec![weaver_traits::Message {
                role: weaver_traits::Role::User,
                content: vec![weaver_traits::ContentBlock::Text {
                    text: text.to_string(),
                }],
            }];
            ports.turn(delta)
        },
        &mut None,
    );
    assert!(
        served.is_err(),
        "service ends, the record broken: {served:?}"
    );
    let ChannelState::Entered(run) = &harness.state else {
        panic!("the serve loop, not this wake, unwinds the run");
    };
    assert!(run.spu.is_some(), "nothing is torn down");
}

/// **A gate that dies at rest with the record broken ends service** (the
/// same ruling, at rest): the death's `fault` cannot be authored, so the
/// run does not serve on without it. Perturbation: ignore the authoring's
/// result in `organ_died`, and the wake serves on.
#[test]
fn a_gate_dying_at_rest_with_the_record_broken_ends_service() {
    let (mut run, _spare, _sink) = entered_run(None);
    let full = OwnedFd::from(
        std::fs::OpenOptions::new()
            .write(true)
            .open("/dev/full")
            .expect("the full device"),
    );
    let mut recorder = crate::record::Record::from(
        Recorder::receive(full, RunRef("r-1".into()), SessionRef("s-1".into())).expect("recorder"),
    );
    let _ = run.author.author_fault(
        &mut recorder,
        Subsystem::Harness,
        None,
        &crate::authorship::harness_report(weaver_types::FaultCase::OrganDeathObserved, "{}"),
    );
    assert!(recorder.drain().is_err(), "the writer has failed");
    run.recorder = recorder;
    let (channel, far) = OrganChannel::pair().expect("pair");
    drop(far);
    run.gate = Some(GateChannel {
        channel,
        pid: nix::unistd::Pid::from_raw(i32::MAX),
        last_word: crate::spawn::LastWord::quiet(),
        shell_granted: true,
    });
    let (mut harness, _dir) = harness_entered(run);
    let served = harness.serve_gate_wake(
        "identity",
        &mut |_: &mut crate::engine::Ports<'_>, _: &str| unreachable!("no frame"),
        &mut None,
    );
    assert!(
        served.is_err(),
        "service ends, the record broken: {served:?}"
    );
}

/// **A frame grants the seat and the whole bracket authors**, per Spec
/// section 6.2 and the dev boundary: the parse at the threshold, the
/// entry across the crossing, the turn against a scripted decode peer,
/// and the response frame answered on the exchange, kind-named.
#[test]
fn a_frame_grants_the_seat_and_is_answered() {
    use std::os::fd::AsRawFd;

    use nix::sys::socket::{
        AddressFamily, MsgFlags, SockFlag, SockType, recv as sock_recv, send as sock_send,
        socketpair,
    };

    let (mut run, _spare, _sink) = entered_run(None);
    // The decode arm is re-pointed at a scripted peer, engine-test style.
    let (near, far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .expect("socketpair");
    run.spu.as_mut().expect("the arm stands").decode = crate::channel::decode_from_owned(near);
    let (gate_end, gate_peer) = OrganChannel::pair().expect("gate pair");
    run.gate = Some(GateChannel {
        channel: gate_end,
        pid: nix::unistd::Pid::from_raw(1),
        last_word: crate::spawn::LastWord::quiet(),
        shell_granted: true,
    });
    let gate_peer = gate_peer.into_channel();

    let decode_peer = std::thread::spawn(move || {
        let mut buf = vec![0u8; 65536];
        let n = sock_recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).expect("recv append");
        let directive: weaver_types::TokenDirective =
            serde_json::from_slice(&buf[..n]).expect("append parses");
        assert!(matches!(
            directive,
            weaver_types::TokenDirective::AppendAndGenerate { .. }
        ));
        let request = serde_json::value::RawValue::from_string(
            r#"{"rendered":"user: say a word","template":"qwen2","sampling":{}}"#.to_string(),
        )
        .unwrap();
        let measurement = serde_json::value::RawValue::from_string(
            r#"{"model":"m","weights_hash":"h","input_tokens":[1],"output_tokens":[2],"blocks":[],"timings":{"prefill_ns":"1","decode_ns":"2"}}"#
                .to_string(),
        )
        .unwrap();
        let answer = weaver_types::TokenAnswer::Generated(weaver_types::Generation {
            unrecovered: vec![],
            content: vec![],
            emission: "one word".into(),
            finish: weaver_types::Finish::Completed,
            request,
            measurement,
            resident: 64,
            capacity: 4096,
        });
        let bytes = serde_json::to_vec(&answer).expect("answer renders");
        sock_send(far.as_raw_fd(), &bytes, MsgFlags::empty()).expect("send answer");
    });

    let (coordination, _coordination_dir) = test_listener();
    let mut harness = Harness {
        coordination,
        organs: OrganBinaries {
            classify: None,
            spu: "/nonexistent".into(),
            gate: "/nonexistent".into(),
        },
        parameters: OrganParameters::default(),
        state: ChannelState::Entered(Box::new(run)),
        composer: Some(weaver_trace::LoopIdentity::compiled("test")),
    };

    // The frame stands on the channel before the wake is served, the way
    // the poll's readiness would have found it.
    gate_peer
        .send(&OrganEnvelope {
            exchange: ExchangeId {
                opener: Opener::Gate,
                ordinal: 1,
            },
            position: Position::Open,
            payload: weaver_types::Payload::Frame(weaver_types::TurnFrame::carry(
                b"{\"text\":\"say a word\"}",
            )),
        })
        .expect("frame sends");

    harness
        .serve_gate_wake(
            "identity",
            &mut |ports: &mut crate::engine::Ports<'_>, text: &str| {
                let delta = vec![weaver_traits::Message {
                    role: weaver_traits::Role::User,
                    content: vec![weaver_traits::ContentBlock::Text {
                        text: text.to_string(),
                    }],
                }];
                ports.turn(delta)
            },
            &mut None,
        )
        .expect("the wake serves");
    decode_peer.join().expect("the decode peer finishes");

    // The response frame, kind-named, on the exchange the frame opened.
    let answer = gate_peer.recv().expect("the response frame returns");
    assert_eq!(answer.exchange.ordinal, 1);
    let weaver_types::Payload::Frame(frame) = answer.payload else {
        panic!("a frame answers a frame");
    };
    let line = String::from_utf8(frame.octets().expect("canonical")).expect("utf8");
    // **The close names the turn it answers and the run it belongs to**,
    // per `weaver-gate-world-contract` section 3. Pinned as the whole line
    // rather than by membership, so a member added here has to be argued
    // rather than slipping past a contains check.
    assert_eq!(
        line,
        r#"{"kind":"answered","run":"r-1","text":"one word","turn":"t-1"}"#
    );

    // The bracket authored whole, in order.
    let ChannelState::Entered(run) = &harness.state else {
        panic!("the position stays entered");
    };
    let kinds: Vec<Kind> = run
        .recorder
        .structure()
        .expect("the serving record")
        .iter()
        .filter(|r| r.turn.is_some())
        .map(|r| r.kind)
        .collect();
    assert_eq!(
        kinds,
        vec![
            Kind::TurnStarted,
            Kind::MessageUser,
            Kind::ModelRequest,
            Kind::ModelOutput,
            Kind::ModelMeasurement,
            Kind::MessageAssistant,
            Kind::TurnClosed,
        ]
    );
}

/// **The parse refuses rather than faults**, per Spec 6.2's threshold
/// clause: a line that is not the request answers `refused` as content,
/// no seat is granted, and the channel stands for the next frame, which
/// is the layer split the frame election bought.
#[test]
fn an_unparseable_line_refuses_and_the_channel_stands() {
    let (mut run, _spare, _sink) = entered_run(None);
    let (gate_end, gate_peer) = OrganChannel::pair().expect("gate pair");
    run.gate = Some(GateChannel {
        channel: gate_end,
        pid: nix::unistd::Pid::from_raw(1),
        last_word: crate::spawn::LastWord::quiet(),
        shell_granted: true,
    });
    let gate_peer = gate_peer.into_channel();
    let (coordination, _coordination_dir) = test_listener();
    let mut harness = Harness {
        coordination,
        organs: OrganBinaries {
            classify: None,
            spu: "/nonexistent".into(),
            gate: "/nonexistent".into(),
        },
        parameters: OrganParameters::default(),
        state: ChannelState::Entered(Box::new(run)),
        composer: Some(weaver_trace::LoopIdentity::compiled("test")),
    };
    let mut entered = false;
    let mut entry = |_: &mut crate::engine::Ports<'_>, _: &str| {
        entered = true;
        Err(crate::engine::TurnError::Unlicensed {
            turn: TurnKey("t-0".into()),
        })
    };

    // Not the canonical carriage at all.
    gate_peer
        .send(&OrganEnvelope {
            exchange: ExchangeId {
                opener: Opener::Gate,
                ordinal: 1,
            },
            position: Position::Open,
            payload: weaver_types::Payload::Frame(weaver_types::TurnFrame {
                octets: "not base64!".into(),
                dialer: None,
            }),
        })
        .expect("frame sends");
    let mut verb_slot = None;
    harness
        .serve_gate_wake("", &mut entry, &mut verb_slot)
        .expect("the wake serves");
    let answer = gate_peer.recv().expect("the refusal returns");
    let weaver_types::Payload::Frame(frame) = answer.payload else {
        panic!("a frame answers a frame");
    };
    let line = String::from_utf8(frame.octets().expect("canonical")).expect("utf8");
    assert!(line.contains(r#""kind":"refused""#), "{line}");

    // Canonical carriage, wrong shape: an unknown member refuses too,
    // and the channel is alive to say so, which is the claim.
    gate_peer
        .send(&OrganEnvelope {
            exchange: ExchangeId {
                opener: Opener::Gate,
                ordinal: 2,
            },
            position: Position::Open,
            payload: weaver_types::Payload::Frame(weaver_types::TurnFrame::carry(
                b"{\"text\":\"hi\",\"kind\":\"answered\"}",
            )),
        })
        .expect("frame sends");
    harness
        .serve_gate_wake("", &mut entry, &mut verb_slot)
        .expect("the channel stands");
    let answer = gate_peer.recv().expect("the second refusal returns");
    let weaver_types::Payload::Frame(frame) = answer.payload else {
        panic!("a frame answers a frame");
    };
    let line = String::from_utf8(frame.octets().expect("canonical")).expect("utf8");
    assert!(line.contains(r#""kind":"refused""#), "{line}");

    assert!(
        !entered,
        "no seat was granted for a line that did not parse"
    );
}

/// **The request parse admits `role` with the one value `system` and
/// nothing else beside `text`, each member once**, per `weaver-gate-Spec`
/// section 4 and the seeding clause of `weaver-harness-Spec` section 6.1:
/// a bare line is the user's, the seeding line is marked, any other role
/// refuses naming it, `null` among them, an unknown member refuses as
/// before, and a repeated member refuses, since a value's map would read
/// the last spelling and admit `{"role":"user","role":"system"}` as the
/// seeding line. Perturbations: admit any string under `role` and the
/// third case parses; read the line as a `serde_json::Value` again and
/// the repeated-role case parses as the seeding line.
#[test]
fn the_request_parse_admits_the_system_role_alone() {
    let parsed = |line: &[u8]| parse_request(&weaver_types::TurnFrame::carry(line));
    let bare = parsed(b"{\"text\":\"hi\"}").expect("a bare line parses");
    assert!(!bare.system && bare.text == "hi");
    let seeding = parsed(b"{\"role\":\"system\",\"text\":\"You are Karl.\"}")
        .expect("the seeding line parses");
    assert!(seeding.system && seeding.text == "You are Karl.");
    let other =
        parsed(b"{\"role\":\"assistant\",\"text\":\"hi\"}").expect_err("another role refuses");
    assert!(other.contains("\"assistant\""), "names the role: {other}");
    assert!(
        parsed(b"{\"role\":\"system\",\"text\":\"hi\",\"kind\":\"x\"}").is_err(),
        "an unknown member still refuses"
    );
    assert!(
        parsed(b"{\"role\":\"system\"}").is_err(),
        "the text is owed"
    );
    let null = parsed(b"{\"role\":null,\"text\":\"hi\"}").expect_err("null is not absent");
    assert!(null.contains("null"), "names the role: {null}");
    for line in [
        &b"{\"text\":\"x\",\"role\":\"user\",\"role\":\"system\"}"[..],
        b"{\"text\":\"x\",\"role\":\"system\",\"role\":\"system\"}",
        b"{\"text\":\"a\",\"text\":\"b\"}",
    ] {
        let repeated = parsed(line).expect_err("a repeated member refuses");
        assert!(repeated.contains("duplicate field"), "{repeated}");
    }
    assert!(
        parsed(b"[\"x\",\"system\"]").is_err(),
        "an array is not a request, whatever serde would read by position"
    );
}

/// **A `system` line from any dialer but the operator refuses, and so
/// does the operator's line carrying no text, and the channel stands**,
/// per `weaver-harness-Spec` section 6.1's seeding clause and
/// `weaver-types-Spec` section 5: a frame carrying no dialer, a frame
/// carrying another uid, the connector's service user's say, and the
/// operator's frame whose text is empty each answer `refused`, the first
/// two naming the role and the third the door's rule, no seat is
/// granted, nothing is authored, and the next frame is served.
/// Perturbations: carry the frame without its dialer at the gate and
/// every seeding line is the first case; skip the dialer's comparison
/// here and the second case seats a prefix from the connector's uid, the
/// seat granted and the record carrying a `message.system`; drop the
/// empty-text rule from `author_identity` and the third case authors an
/// empty prefix and turns on it.
#[test]
fn a_system_line_from_any_other_dialer_refuses_and_the_channel_stands() {
    let (mut run, _spare, _sink) = entered_run(None);
    let (gate_end, gate_peer) = OrganChannel::pair().expect("gate pair");
    run.gate = Some(GateChannel {
        channel: gate_end,
        pid: nix::unistd::Pid::from_raw(1),
        last_word: crate::spawn::LastWord::quiet(),
        shell_granted: true,
    });
    let gate_peer = gate_peer.into_channel();
    let (coordination, _coordination_dir) = test_listener();
    let mut harness = Harness {
        coordination,
        organs: OrganBinaries {
            classify: None,
            spu: "/nonexistent".into(),
            gate: "/nonexistent".into(),
        },
        parameters: OrganParameters::default(),
        state: ChannelState::Entered(Box::new(run)),
        composer: Some(weaver_trace::LoopIdentity::compiled("test")),
    };
    let mut entered = false;
    let mut entry = |_: &mut crate::engine::Ports<'_>, _: &str| {
        entered = true;
        Err(crate::engine::TurnError::Unlicensed {
            turn: TurnKey("t-0".into()),
        })
    };
    let line = b"{\"role\":\"system\",\"text\":\"You are Karl.\"}";
    let empty = b"{\"role\":\"system\",\"text\":\"\"}";
    let mut verb_slot = None;
    for (ordinal, frame, names) in [
        (1, weaver_types::TurnFrame::carry(line), "role system"),
        (
            2,
            weaver_types::TurnFrame::carry_from(line, 1001),
            "role system",
        ),
        (
            3,
            weaver_types::TurnFrame::carry_from(empty, 1000),
            "identity-door-empty-text",
        ),
    ] {
        gate_peer
            .send(&OrganEnvelope {
                exchange: ExchangeId {
                    opener: Opener::Gate,
                    ordinal,
                },
                position: Position::Open,
                payload: weaver_types::Payload::Frame(frame),
            })
            .expect("frame sends");
        harness
            .serve_gate_wake("", &mut entry, &mut verb_slot)
            .expect("the channel stands");
        let answer = gate_peer.recv().expect("the refusal returns");
        assert_eq!(answer.exchange.ordinal, ordinal);
        let weaver_types::Payload::Frame(frame) = answer.payload else {
            panic!("a frame answers a frame");
        };
        let line = String::from_utf8(frame.octets().expect("canonical")).expect("utf8");
        assert!(line.contains(r#""kind":"refused""#), "{line}");
        assert!(line.contains(names), "names its reason: {line}");
    }
    assert!(!entered, "no seat was granted for a refused seeding line");
    let ChannelState::Entered(run) = &harness.state else {
        panic!("the position stays entered");
    };
    let kinds: Vec<Kind> = run
        .recorder
        .structure()
        .expect("the serving record")
        .iter()
        .map(|r| r.kind)
        .collect();
    assert_eq!(kinds, vec![Kind::Load], "nothing was authored: {kinds:?}");
}

/// **A seeding that would carry the residency's prefix past the state
/// seam's answer bound refuses, and one that fits seats**, per
/// `weaver-harness-Spec` section 6.1 and `weaver-harness-state-contract`
/// section 3: the run's count stands just under the bound, the n-th
/// seeding, whose cost would pass it, answers `refused` naming the bound
/// and the size with nothing authored, and the (n-1)-th, which fits,
/// seats and grows the count by its cost. The count is set rather than
/// grown by a mebibyte of seedings, each costing the same rendering.
/// Perturbation: drop the check and the n-th seeds, the record carrying
/// its `message.system`, which the member would then fail to answer at
/// the next load.
#[test]
fn a_seeding_past_the_answer_bound_refuses_and_one_that_fits_seats() {
    use std::os::fd::AsRawFd;

    use nix::sys::socket::{
        AddressFamily, MsgFlags, SockFlag, SockType, recv as sock_recv, send as sock_send,
        socketpair,
    };

    let (mut run, _spare, _sink) = entered_run(None);
    let line = b"{\"role\":\"system\",\"text\":\"You are Karl.\"}";
    let message = weaver_traits::Message {
        role: weaver_traits::Role::System,
        content: vec![weaver_traits::ContentBlock::Text {
            text: "You are Karl.".into(),
        }],
    };
    let cost = crate::state::identity_entry_cost(
        serde_json::to_string(&message).expect("renders").len(),
        &run.session.0,
        &run.run.0,
    );
    // Room for exactly one more seeding of this cost and not two.
    run.seeded_bytes = crate::state::ANSWER_BOUND_BYTES - crate::state::ANSWER_FRAME_BYTES - cost;
    let (near, far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .expect("socketpair");
    run.spu.as_mut().expect("the arm stands").decode = crate::channel::decode_from_owned(near);
    let (gate_end, gate_peer) = OrganChannel::pair().expect("gate pair");
    run.gate = Some(GateChannel {
        channel: gate_end,
        pid: nix::unistd::Pid::from_raw(1),
        last_word: crate::spawn::LastWord::quiet(),
        shell_granted: true,
    });
    let gate_peer = gate_peer.into_channel();
    let decode_peer = std::thread::spawn(move || {
        let mut buf = vec![0u8; 65536];
        let n = sock_recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).expect("recv append");
        let directive: weaver_types::TokenDirective =
            serde_json::from_slice(&buf[..n]).expect("append parses");
        assert!(matches!(
            directive,
            weaver_types::TokenDirective::AppendAndGenerate { .. }
        ));
        let request = serde_json::value::RawValue::from_string(
            r#"{"rendered":"system: You are Karl.","template":"qwen2","sampling":{}}"#.to_string(),
        )
        .unwrap();
        let measurement = serde_json::value::RawValue::from_string(
            r#"{"model":"m","weights_hash":"h","input_tokens":[1],"output_tokens":[2],"blocks":[],"timings":{"prefill_ns":"1","decode_ns":"2"}}"#
                .to_string(),
        )
        .unwrap();
        let answer = weaver_types::TokenAnswer::Generated(weaver_types::Generation {
            unrecovered: vec![],
            content: vec![],
            emission: "I am Karl.".into(),
            finish: weaver_types::Finish::Completed,
            request,
            measurement,
            resident: 64,
            capacity: 4096,
        });
        let bytes = serde_json::to_vec(&answer).expect("answer renders");
        sock_send(far.as_raw_fd(), &bytes, MsgFlags::empty()).expect("send answer");
    });
    let (coordination, _coordination_dir) = test_listener();
    let mut harness = Harness {
        coordination,
        organs: OrganBinaries {
            classify: None,
            spu: "/nonexistent".into(),
            gate: "/nonexistent".into(),
        },
        parameters: OrganParameters::default(),
        state: ChannelState::Entered(Box::new(run)),
        composer: Some(weaver_trace::LoopIdentity::compiled("test")),
    };
    let mut entry = |_: &mut crate::engine::Ports<'_>,
                     _: &str|
     -> Result<crate::engine::TurnOutcome, crate::engine::TurnError> {
        panic!("the seeding never enters the loop")
    };
    let mut verb_slot = None;
    let mut answer_of = |ordinal: u64, harness: &mut Harness| {
        gate_peer
            .send(&OrganEnvelope {
                exchange: ExchangeId {
                    opener: Opener::Gate,
                    ordinal,
                },
                position: Position::Open,
                payload: weaver_types::Payload::Frame(weaver_types::TurnFrame::carry_from(
                    line, 1000,
                )),
            })
            .expect("frame sends");
        harness
            .serve_gate_wake("identity", &mut entry, &mut verb_slot)
            .expect("the wake serves");
        let answer = gate_peer.recv().expect("the response returns");
        let weaver_types::Payload::Frame(frame) = answer.payload else {
            panic!("a frame answers a frame");
        };
        String::from_utf8(frame.octets().expect("canonical")).expect("utf8")
    };
    // The (n-1)-th fits: seated and turned, the count grown by its cost.
    let first = answer_of(1, &mut harness);
    decode_peer.join().expect("the decode peer finishes");
    assert!(first.starts_with(r#"{"kind":"answered""#), "{first}");
    let ChannelState::Entered(run) = &harness.state else {
        panic!("entered");
    };
    assert_eq!(
        run.seeded_bytes,
        crate::state::ANSWER_BOUND_BYTES - crate::state::ANSWER_FRAME_BYTES
    );
    // The n-th would pass the bound: refused naming it, nothing authored.
    let second = answer_of(2, &mut harness);
    assert!(second.starts_with(r#"{"kind":"refused""#), "{second}");
    assert!(
        second.contains("past its bound of 1048576 octets"),
        "names the bound: {second}"
    );
    let ChannelState::Entered(run) = &harness.state else {
        panic!("entered");
    };
    let prefixes = run
        .recorder
        .structure()
        .expect("serving")
        .iter()
        .filter(|r| r.kind == Kind::MessageSystem)
        .count();
    assert_eq!(prefixes, 1, "the first seeding alone is on the record");
}

/// **The operator's `system` line seats the prompt and turns**, per
/// `weaver-harness-Spec` section 6.1's seeding clause and
/// `weaver-trace-Spec` section 3: the frame whose dialer is the enter's
/// operator authors one turnless `message.system` of the line's text
/// through the identity door, which the tee carries to the member, then
/// a turn whose `AppendAndGenerate` carries that one system message and
/// no user message, the loop never entered, and the model's answer is
/// the response frame's. Perturbations: skip the identity door and the
/// turnless `message.system` is absent from the record and the tee;
/// author the delta inside the turn as well and the record carries the
/// prompt twice, the second one turned; route the seeding through the
/// entry and the closure panics.
#[test]
fn the_operators_system_line_seats_the_prompt_and_turns() {
    use std::os::fd::AsRawFd;

    use nix::sys::socket::{
        AddressFamily, MsgFlags, SockFlag, SockType, recv as sock_recv, send as sock_send,
        socketpair,
    };

    let (mut run, _spare, _sink) = entered_run(None);
    // The tee, attached as the enter attaches it, with a member reading
    // its lines to closure.
    let (state_end, member_end) =
        std::os::unix::net::UnixStream::pair().expect("the member's pair");
    run.recorder.attach_tee(
        weaver_trace::Tee::open(
            state_end,
            "s-1".into(),
            weaver_trace::Election {
                all_kinds: true,
                keys: Vec::new(),
            },
        )
        .expect("the tee opens"),
    );
    let member = std::thread::spawn(move || member_lines(member_end).collect::<Vec<_>>());
    let (near, far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .expect("socketpair");
    run.spu.as_mut().expect("the arm stands").decode = crate::channel::decode_from_owned(near);
    let (gate_end, gate_peer) = OrganChannel::pair().expect("gate pair");
    run.gate = Some(GateChannel {
        channel: gate_end,
        pid: nix::unistd::Pid::from_raw(1),
        last_word: crate::spawn::LastWord::quiet(),
        shell_granted: true,
    });
    let gate_peer = gate_peer.into_channel();
    let decode_peer = std::thread::spawn(move || {
        let mut buf = vec![0u8; 65536];
        let n = sock_recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).expect("recv append");
        let directive: weaver_types::TokenDirective =
            serde_json::from_slice(&buf[..n]).expect("append parses");
        let weaver_types::TokenDirective::AppendAndGenerate { delta, .. } = directive else {
            panic!("the seeding appends and generates: {directive:?}");
        };
        assert_eq!(delta.len(), 1, "the one system message and no user message");
        assert!(matches!(delta[0].role, weaver_traits::Role::System));
        assert!(matches!(
            &delta[0].content[..],
            [weaver_traits::ContentBlock::Text { text }] if text == "You are Karl."
        ));
        let request = serde_json::value::RawValue::from_string(
            r#"{"rendered":"system: You are Karl.","template":"qwen2","sampling":{}}"#.to_string(),
        )
        .unwrap();
        let measurement = serde_json::value::RawValue::from_string(
            r#"{"model":"m","weights_hash":"h","input_tokens":[1],"output_tokens":[2],"blocks":[],"timings":{"prefill_ns":"1","decode_ns":"2"}}"#
                .to_string(),
        )
        .unwrap();
        let answer = weaver_types::TokenAnswer::Generated(weaver_types::Generation {
            unrecovered: vec![],
            content: vec![],
            emission: "I am Karl.".into(),
            finish: weaver_types::Finish::Completed,
            request,
            measurement,
            resident: 64,
            capacity: 4096,
        });
        let bytes = serde_json::to_vec(&answer).expect("answer renders");
        sock_send(far.as_raw_fd(), &bytes, MsgFlags::empty()).expect("send answer");
    });

    let (coordination, _coordination_dir) = test_listener();
    let mut harness = Harness {
        coordination,
        organs: OrganBinaries {
            classify: None,
            spu: "/nonexistent".into(),
            gate: "/nonexistent".into(),
        },
        parameters: OrganParameters::default(),
        state: ChannelState::Entered(Box::new(run)),
        composer: Some(weaver_trace::LoopIdentity::compiled("test")),
    };
    gate_peer
        .send(&OrganEnvelope {
            exchange: ExchangeId {
                opener: Opener::Gate,
                ordinal: 1,
            },
            position: Position::Open,
            payload: weaver_types::Payload::Frame(weaver_types::TurnFrame::carry_from(
                b"{\"role\":\"system\",\"text\":\"You are Karl.\"}",
                1000,
            )),
        })
        .expect("frame sends");
    harness
        .serve_gate_wake(
            "identity",
            &mut |_: &mut crate::engine::Ports<'_>, _: &str| {
                panic!("the seeding turn is loop 0's own and never enters the loop")
            },
            &mut None,
        )
        .expect("the wake serves");
    decode_peer.join().expect("the decode peer finishes");

    let answer = gate_peer.recv().expect("the response frame returns");
    let weaver_types::Payload::Frame(frame) = answer.payload else {
        panic!("a frame answers a frame");
    };
    let line = String::from_utf8(frame.octets().expect("canonical")).expect("utf8");
    assert_eq!(
        line,
        r#"{"kind":"answered","run":"r-1","text":"I am Karl.","turn":"t-1"}"#
    );

    let ChannelState::Entered(run) = &harness.state else {
        panic!("the position stays entered");
    };
    let records: Vec<(Kind, bool)> = run
        .recorder
        .structure()
        .expect("the serving record")
        .iter()
        .map(|r| (r.kind, r.turn.is_some()))
        .collect();
    assert_eq!(
        records,
        vec![
            (Kind::Load, false),
            (Kind::MessageSystem, false),
            (Kind::TurnStarted, true),
            (Kind::ModelRequest, true),
            (Kind::ModelOutput, true),
            (Kind::ModelMeasurement, true),
            (Kind::MessageAssistant, true),
            (Kind::TurnClosed, true),
        ],
        "the prompt once, turnless, then the turn with no user message"
    );
    // The tee carried the prompt to the member, which is how it reaches
    // the save point the next load restores.
    drop(harness);
    let read = member.join().expect("the member read to closure");
    assert!(
        read.iter()
            .any(|line| line.contains(r#""kind":"message.system""#)
                && line.contains("You are Karl.")),
        "the seeding crossed the tee: {read:?}"
    );
}

/// **The run's turn key stands while a turn runs**, per
/// `weaver-harness-Spec` section 3's observe and stop clauses and issue
/// #441. Driven through the gate's wake, the slot on the run is empty
/// before any turn, empty after a turn that closed clean, and empty
/// after a turn the decode seam refused, whose close lands on the
/// engine's error path. The record pairs each `turn.started` with its
/// `turn.closed`, so the slot's emptiness after each wake is the close
/// having cleared it and not the set never having happened.
///
/// Perturbation: drop the clear in the engine's clean close and the
/// slot holds `t-1` after the first wake; drop the clear on the
/// error-path close and it holds `t-2` after the second. **What this
/// watch cannot see is the key standing mid-turn.** The slot lives on
/// the serving thread and the bracket runs to its close inside the
/// seat's turn with no test-supplied code on that thread inside it, so
/// dropping the set is invisible here. That is issue #441's other half
/// stated as a test's limit, and the set's reader, the observe arm, is
/// watched with a standing key in
/// `observe_answers_from_any_position_and_authors_nothing`.
#[test]
fn the_turn_key_stands_while_a_turn_runs() {
    use std::os::fd::AsRawFd;

    use nix::sys::socket::{
        AddressFamily, MsgFlags, SockFlag, SockType, recv as sock_recv, send as sock_send,
        socketpair,
    };

    let (mut run, _spare, _sink) = entered_run(None);
    assert!(
        run.turn_in_flight.is_none(),
        "an idle run holds no turn key"
    );
    let (near, far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .expect("socketpair");
    run.spu.as_mut().expect("the arm stands").decode = crate::channel::decode_from_owned(near);
    let (gate_end, gate_peer) = OrganChannel::pair().expect("gate pair");
    run.gate = Some(GateChannel {
        channel: gate_end,
        pid: nix::unistd::Pid::from_raw(1),
        last_word: crate::spawn::LastWord::quiet(),
        shell_granted: true,
    });
    let gate_peer = gate_peer.into_channel();

    let decode_peer = std::thread::spawn(move || {
        let mut buf = vec![0u8; 65536];
        // The first append is answered, so the bracket closes clean.
        let n = sock_recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).expect("recv append");
        let directive: weaver_types::TokenDirective =
            serde_json::from_slice(&buf[..n]).expect("append parses");
        assert!(matches!(
            directive,
            weaver_types::TokenDirective::AppendAndGenerate { .. }
        ));
        let request = serde_json::value::RawValue::from_string(
            r#"{"rendered":"user: say a word","template":"qwen2","sampling":{}}"#.to_string(),
        )
        .unwrap();
        let measurement = serde_json::value::RawValue::from_string(
            r#"{"model":"m","weights_hash":"h","input_tokens":[1],"output_tokens":[2],"blocks":[],"timings":{"prefill_ns":"1","decode_ns":"2"}}"#
                .to_string(),
        )
        .unwrap();
        let answer = weaver_types::TokenAnswer::Generated(weaver_types::Generation {
            unrecovered: vec![],
            content: vec![],
            emission: "one word".into(),
            finish: weaver_types::Finish::Completed,
            request,
            measurement,
            resident: 64,
            capacity: 4096,
        });
        let bytes = serde_json::to_vec(&answer).expect("answer renders");
        sock_send(far.as_raw_fd(), &bytes, MsgFlags::empty()).expect("send answer");
        // The second append is refused, so the bracket closes on the
        // engine's error path with the refusal as its reason.
        let n = sock_recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).expect("recv append");
        let directive: weaver_types::TokenDirective =
            serde_json::from_slice(&buf[..n]).expect("append parses");
        assert!(matches!(
            directive,
            weaver_types::TokenDirective::AppendAndGenerate { .. }
        ));
        let refusal = weaver_types::TokenRefusal::OutOfOrder;
        let bytes = serde_json::to_vec(&refusal).expect("refusal renders");
        sock_send(far.as_raw_fd(), &bytes, MsgFlags::empty()).expect("send refusal");
    });

    let (coordination, _coordination_dir) = test_listener();
    let mut harness = Harness {
        coordination,
        organs: OrganBinaries {
            classify: None,
            spu: "/nonexistent".into(),
            gate: "/nonexistent".into(),
        },
        parameters: OrganParameters::default(),
        state: ChannelState::Entered(Box::new(run)),
        composer: Some(weaver_trace::LoopIdentity::compiled("test")),
    };
    let mut entry = |ports: &mut crate::engine::Ports<'_>, text: &str| {
        let delta = vec![weaver_traits::Message {
            role: weaver_traits::Role::User,
            content: vec![weaver_traits::ContentBlock::Text {
                text: text.to_string(),
            }],
        }];
        ports.turn(delta)
    };

    let expected = [
        r#"{"kind":"answered","run":"r-1","text":"one word","turn":"t-1"}"#,
        r#"{"kind":"stopped","reason":"the ask was out of order for the seam","run":"r-1","turn":"t-2"}"#,
    ];
    for (ordinal, line) in (1u64..).zip(expected) {
        gate_peer
            .send(&OrganEnvelope {
                exchange: ExchangeId {
                    opener: Opener::Gate,
                    ordinal,
                },
                position: Position::Open,
                payload: weaver_types::Payload::Frame(weaver_types::TurnFrame::carry(
                    b"{\"text\":\"say a word\"}",
                )),
            })
            .expect("frame sends");
        harness
            .serve_gate_wake("identity", &mut entry, &mut None)
            .expect("the wake serves");
        let answer = gate_peer.recv().expect("the response frame returns");
        assert_eq!(answer.exchange.ordinal, ordinal);
        let weaver_types::Payload::Frame(frame) = answer.payload else {
            panic!("a frame answers a frame");
        };
        assert_eq!(
            String::from_utf8(frame.octets().expect("canonical")).expect("utf8"),
            line
        );
        let ChannelState::Entered(run) = &harness.state else {
            panic!("the position stays entered");
        };
        assert!(
            run.turn_in_flight.is_none(),
            "the close of turn {ordinal} cleared the run's key"
        );
    }
    decode_peer.join().expect("the decode peer finishes");

    // Both brackets stand closed in the record, each close naming the
    // turn its open did, which is what makes the empty slot the clear's
    // work.
    let ChannelState::Entered(run) = &harness.state else {
        panic!("the position stays entered");
    };
    let brackets: Vec<(String, Kind)> = run
        .recorder
        .structure()
        .expect("the serving record")
        .iter()
        .filter(|r| matches!(r.kind, Kind::TurnStarted | Kind::TurnClosed))
        .map(|r| {
            (
                r.turn
                    .as_ref()
                    .expect("a bracket event names its turn")
                    .0
                    .clone(),
                r.kind,
            )
        })
        .collect();
    assert_eq!(
        brackets,
        vec![
            ("t-1".to_string(), Kind::TurnStarted),
            ("t-1".to_string(), Kind::TurnClosed),
            ("t-2".to_string(), Kind::TurnStarted),
            ("t-2".to_string(), Kind::TurnClosed),
        ]
    );
}

/// The run's record replaced by one whose writer has failed: one event
/// written to a device that refuses every write, the drain's failure
/// standing, as the B2 pins build it (62aa181).
fn break_record(run: &mut Run) {
    let full = OwnedFd::from(
        std::fs::OpenOptions::new()
            .write(true)
            .open("/dev/full")
            .expect("the full device"),
    );
    let mut recorder = crate::record::Record::from(
        Recorder::receive(full, RunRef("r-1".into()), SessionRef("s-1".into())).expect("recorder"),
    );
    let _ = run.author.author_fault(
        &mut recorder,
        Subsystem::Harness,
        None,
        &crate::authorship::harness_report(weaver_types::FaultCase::OrganDeathObserved, "{}"),
    );
    assert!(recorder.drain().is_err(), "the writer has failed");
    run.recorder = recorder;
}

/// The run's record latched by a submission refused on its own, the writer
/// sound (#116, Q5): an event bound to another session, `RefusedOnSubmit`,
/// which no standing failure and no drain would see.
fn latch_by_refusal(run: &mut Run) {
    let foreign = crate::authorship::Author::new(
        &weaver_types::SessionId("s-other".into()),
        &weaver_types::RunId("r-1".into()),
    );
    let refused = foreign.author_fault(
        &mut run.recorder,
        Subsystem::Harness,
        None,
        &crate::authorship::harness_report(weaver_types::FaultCase::OrganDeathObserved, "{}"),
    );
    assert!(
        matches!(
            refused,
            Err(crate::record::RecordFailure::Serving(
                weaver_trace::Failure::RefusedOnSubmit { .. }
            ))
        ),
        "{refused:?}"
    );
    assert!(run.recorder.failed(), "the refusal is latched");
    assert!(run.recorder.drain().is_ok(), "the writer is sound");
}

/// Whether an answer arrives on `channel` within 200 ms; a hang-up, which
/// wakes the poll as an answer does, is none.
fn arrives(channel: &OrganChannel) -> bool {
    use std::os::fd::AsFd;
    let mut fds = [nix::poll::PollFd::new(
        channel.as_fd(),
        nix::poll::PollFlags::POLLIN,
    )];
    nix::poll::poll(&mut fds, nix::poll::PollTimeout::from(200u16)).expect("poll") > 0
        && channel.recv().is_ok()
}

/// **A stop mid-turn whose close the record cannot take ends service
/// unanswered** (#116, site `dispatch_on`'s stop): the close's authoring
/// fails and is latched, and the answer reads the latch before
/// `TurnAborted` is announced. Perturbation: drop the latch read from
/// `Harness::answer`, and `TurnAborted` is answered over the missing close.
#[test]
fn a_stop_mid_turn_with_the_record_broken_ends_service_unanswered() {
    let (mut run, _spare, _sink) = entered_run(Some("t-1"));
    break_record(&mut run);
    let (harness_end, peer_end) = OrganChannel::pair().expect("pair");
    let (mut harness, _dir) = harness_entered(run);
    let dispatched = harness.dispatch_on(
        &harness_end,
        test_exchange(),
        LifecycleDirective::Stop {
            cause: weaver_types::Cause { uid: 1000 },
        },
        None,
        None,
    );
    assert!(dispatched.is_err(), "service ends: {dispatched:?}");
    assert!(
        !arrives(&peer_end.into_channel()),
        "no answer is announced over the missing close"
    );
}

/// **A refusal to admin is not announced over a latched record** (#116,
/// `Harness::refuse`, the road `author_refusal_on`'s enter refusal and
/// every other refusal take): with the record latched by a submission
/// refused on its own, an out-of-order directive ends service rather than
/// being refused. Perturbation: drop the latch read from `Harness::refuse`,
/// and `OutOfOrder` is answered.
#[test]
fn a_refusal_is_not_announced_over_a_latched_record() {
    let (mut run, _spare, _sink) = entered_run(None);
    latch_by_refusal(&mut run);
    let (harness_end, peer_end) = OrganChannel::pair().expect("pair");
    let (mut harness, _dir) = harness_entered(run);
    let dispatched = harness.dispatch_on(
        &harness_end,
        test_exchange(),
        LifecycleDirective::JoinLeave {
            cause: weaver_types::Cause { uid: 0 },
        },
        None,
        None,
    );
    assert!(dispatched.is_err(), "service ends: {dispatched:?}");
    assert!(!arrives(&peer_end.into_channel()), "nothing is announced");
}

/// **A gate's fault report at rest that the record cannot take ends
/// service** (#116, `serve_gate_wake`'s fault arm): the authoring is
/// latched and the wake reads it. Perturbation: answer the arm `Ok(())`
/// as before, and the run serves on without the report.
#[test]
fn a_gate_fault_at_rest_with_the_record_broken_ends_service() {
    let (mut run, _spare, _sink) = entered_run(None);
    break_record(&mut run);
    let (channel, gate_peer) = OrganChannel::pair().expect("pair");
    run.gate = Some(GateChannel {
        channel,
        pid: nix::unistd::Pid::from_raw(i32::MAX),
        last_word: crate::spawn::LastWord::quiet(),
        shell_granted: true,
    });
    let gate_peer = gate_peer.into_channel();
    gate_peer
        .send(&OrganEnvelope {
            exchange: ExchangeId {
                opener: Opener::Gate,
                ordinal: 1,
            },
            position: Position::Open,
            payload: weaver_types::Payload::Fault(crate::authorship::harness_report(
                weaver_types::FaultCase::ClientConnectionFailedMidTurn,
                "{}",
            )),
        })
        .expect("the report sends");
    let (mut harness, _dir) = harness_entered(run);
    let served = harness.serve_gate_wake(
        "identity",
        &mut |_: &mut crate::engine::Ports<'_>, _: &str| unreachable!("no frame"),
        &mut None,
    );
    assert!(served.is_err(), "service ends: {served:?}");
}

/// **A dialer is not answered over a latched record** (#116, the frame
/// path's read before its close is sent; the Planner's addition: the turn's
/// close path reads the latch before any answer is announced): with the
/// record latched by a submission refused on its own, the writer sound, a
/// frame's close is not sent and service ends. The read covers every
/// authoring inside a frame: the ports' (flush, recall, score, classify,
/// the decode refusals, the pressure report), the held-frames fault and the
/// hole's fault. Perturbation: drop the read, and the refused close is sent.
#[test]
fn a_dialer_is_not_answered_over_a_latched_record() {
    let (mut run, _spare, _sink) = entered_run(None);
    latch_by_refusal(&mut run);
    let (gate_end, gate_peer) = OrganChannel::pair().expect("gate pair");
    run.gate = Some(GateChannel {
        channel: gate_end,
        pid: nix::unistd::Pid::from_raw(i32::MAX),
        last_word: crate::spawn::LastWord::quiet(),
        shell_granted: true,
    });
    let gate_peer = gate_peer.into_channel();
    gate_peer
        .send(&OrganEnvelope {
            exchange: ExchangeId {
                opener: Opener::Gate,
                ordinal: 1,
            },
            position: Position::Open,
            payload: weaver_types::Payload::Frame(weaver_types::TurnFrame::carry(b"not a request")),
        })
        .expect("frame sends");
    let (mut harness, _dir) = harness_entered(run);
    let served = harness.serve_gate_wake(
        "identity",
        &mut |_: &mut crate::engine::Ports<'_>, _: &str| unreachable!("the parse refuses"),
        &mut None,
    );
    assert!(served.is_err(), "service ends: {served:?}");
    assert!(!arrives(&gate_peer), "the dialer is not answered");
}

/// **A leave over a latched record answers no `Left`** (#116, the leave's
/// seal): the drain sees only a writer that failed, so a submission refused
/// on its own, the writer sound, is read from the latch, and the leave
/// refuses `DescriptorsUnusable`. Perturbation: drop the latch read from
/// `leave_after`, and the leave completes.
#[test]
fn a_leave_over_a_latched_record_refuses_descriptors_unusable() {
    let (mut run, _spare, _sink) = entered_run(None);
    latch_by_refusal(&mut run);
    assert_eq!(
        leave(&mut run, None, true),
        Err(LifecycleRefusal::DescriptorsUnusable)
    );
}

/// **The serve loop does not wait over a latched record** (#116, the
/// backstop before each wait): an authoring that announced nothing, such as
/// `grant_seat`'s hole fault or a replay's, is still not served past.
/// Perturbation: drop the read before the wait, and the loop waits on.
#[test]
fn the_serve_loop_does_not_wait_over_a_latched_record() {
    let (mut run, _spare, _sink) = entered_run(None);
    latch_by_refusal(&mut run);
    let (harness, dir) = harness_entered(run);
    let (sent, served) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _dir = dir;
        let outcome = harness.serve(
            "identity",
            weaver_trace::LoopIdentity::compiled("test"),
            |_, _| unreachable!("no frame arrives"),
        );
        let _ = sent.send(outcome.is_err());
    });
    assert_eq!(
        served.recv_timeout(std::time::Duration::from_secs(5)),
        Ok(true),
        "service ends before the wait"
    );
}

/// **A dial heard during a leave is not answered over a latched record**
/// (#116, `hear_dial`): an observation is dropped unanswered, admin
/// concluding as for no answer. Perturbation: drop the latch read, and
/// `InTransition` is answered.
#[test]
fn a_dial_heard_in_a_leave_is_not_answered_over_a_latched_record() {
    use crate::lifecycle_table::Leave;
    let (mut leave, _admin) = pending_leave(Leave::Graceful, Leave::Graceful, false);
    let (observe, far) = dialed(LifecycleDirective::Observe);
    hear_dial(observe, &mut leave, false, true);
    assert!(!arrives(&far), "nothing is answered");
}

/// **The leave's held callers are refused `DescriptorsUnusable` over a
/// latched record** (#116, `answer_all`), whatever the leave would have
/// answered: the seal's `Left` and S9's `SavePointNotTaken` alike.
/// Perturbation: drop the override, and the leave's own answer is sent.
#[test]
fn the_leaves_callers_are_refused_over_a_latched_record() {
    use crate::lifecycle_table::Leave;
    let (leave, admin) = pending_leave(Leave::Graceful, Leave::Graceful, false);
    answer_all(
        &leave,
        Err(LifecycleRefusal::SavePointNotTaken {
            missed: weaver_types::SavePointLeg::MemberDead,
        }),
        true,
    );
    assert_eq!(
        admin.recv().expect("an answer").payload,
        weaver_types::Payload::Refusal(LifecycleRefusal::DescriptorsUnusable)
    );
}

/// **No held frame is served over a latched record** (#116, the shelf): a
/// frame queued behind the turn would run a turn, and the tools it asks in
/// the world, past a record already missing an event. Perturbation: drop
/// the read at the top of the shelf's loop, and the frame's turn is entered.
#[test]
fn a_held_frame_is_not_served_over_a_latched_record() {
    let (mut run, _spare, _sink) = entered_run(None);
    latch_by_refusal(&mut run);
    let (gate_end, _gate_peer) = OrganChannel::pair().expect("gate pair");
    run.gate = Some(GateChannel {
        channel: gate_end,
        pid: nix::unistd::Pid::from_raw(i32::MAX),
        last_word: crate::spawn::LastWord::quiet(),
        shell_granted: true,
    });
    run.held_frames.push_back(OrganEnvelope {
        exchange: ExchangeId {
            opener: Opener::Gate,
            ordinal: 2,
        },
        position: Position::Open,
        payload: weaver_types::Payload::Frame(weaver_types::TurnFrame::carry(
            b"{\"text\":\"say a word\"}",
        )),
    });
    let (mut harness, _dir) = harness_entered(run);
    let served = harness.serve_shelf(
        "identity",
        &mut |_: &mut crate::engine::Ports<'_>, _: &str| panic!("the held frame's turn ran"),
        &mut None,
    );
    assert!(served.is_err(), "service ends: {served:?}");
}

/// **An unwind over a latched record names `StreamWriteFailed`** (#116):
/// whichever reader met the latch, the record's last event says the record
/// failed, the writer here being sound enough to take it. Perturbation:
/// drop the override, and the caller's case is named.
#[test]
fn an_unwind_over_a_latched_record_names_stream_write_failed() {
    let (mut run, _spare, sink) = entered_run(None);
    latch_by_refusal(&mut run);
    let (mut harness, _dir) = harness_entered(run);
    harness.unwind_if_entered(
        weaver_types::FaultCase::OrganDeathObserved,
        &ChannelFault::Closed,
    );
    let stream = std::fs::read_to_string(&sink.0).expect("the sink reads back");
    let fault = stream
        .lines()
        .find(|line| line.contains("\"kind\":\"fault\"") && line.contains("service-ended"))
        .expect("the unwind's fault");
    assert!(fault.contains("stream_write_failed"), "{fault}");
}

/// **A join heard in a leave is not answered over a latched record**
/// (#116, `hear_dial`): `join` refuses a join into a forced leave on its
/// own, so the latch is read before the join. Perturbation: read it after,
/// and `OutOfOrder` is answered.
#[test]
fn a_join_heard_in_a_leave_is_not_answered_over_a_latched_record() {
    use crate::lifecycle_table::Leave;
    let (mut leave, _admin) = pending_leave(Leave::Forced, Leave::Forced, false);
    let (join, far) = dialed(LifecycleDirective::JoinLeave {
        cause: weaver_types::Cause { uid: 9 },
    });
    hear_dial(join, &mut leave, false, true);
    assert!(!arrives(&far), "nothing is answered");
}

/// **A gate's report that will not render is latched, and ends service**
/// (#116, `author_fault`): an account the gate sent with a newline inside
/// it parses, and the record's canonical line refuses it. Perturbation:
/// return the render's failure without latching it, and the run serves on
/// without the report.
#[test]
fn a_gate_report_that_will_not_render_ends_service() {
    let (mut run, _spare, _sink) = entered_run(None);
    let (channel, gate_peer) = OrganChannel::pair().expect("pair");
    run.gate = Some(GateChannel {
        channel,
        pid: nix::unistd::Pid::from_raw(i32::MAX),
        last_word: crate::spawn::LastWord::quiet(),
        shell_granted: true,
    });
    let gate_peer = gate_peer.into_channel();
    let report = weaver_types::FaultReport {
        case: weaver_types::FaultCase::ClientConnectionFailedMidTurn,
        account: serde_json::value::RawValue::from_string("{\"x\":\n1}".into())
            .expect("an account that parses"),
    };
    gate_peer
        .send(&OrganEnvelope {
            exchange: ExchangeId {
                opener: Opener::Gate,
                ordinal: 1,
            },
            position: Position::Open,
            payload: weaver_types::Payload::Fault(report),
        })
        .expect("the report sends");
    let (mut harness, _dir) = harness_entered(run);
    let served = harness.serve_gate_wake(
        "identity",
        &mut |_: &mut crate::engine::Ports<'_>, _: &str| unreachable!("no frame"),
        &mut None,
    );
    assert!(served.is_err(), "service ends: {served:?}");
}

/// **No seat is granted over a latched record** (#116, `grant_seat`).
/// Perturbation: drop the arm, and the seat is granted.
#[test]
fn no_seat_is_granted_over_a_latched_record() {
    let (mut run, _spare, _sink) = entered_run(None);
    latch_by_refusal(&mut run);
    let (mut harness, _dir) = harness_entered(run);
    assert!(harness.grant_seat("identity").is_none());
}

/// **C1, the gate's death: a gate dying with a call open leaves `tools/`
/// empty once the worker closes the execution** (`weaver-admin-Spec` 6.1,
/// C1's instrument; the gate dies while serving): the SPU asks a call, the
/// gate stand-in starts a command in a scratch `tools/` that leaves its
/// session and double-forks, and dies with the call open. The worker kills
/// `tools/` and waits before it closes the call `Killed { by: fault }`, the
/// gate's death naming the containment and the call, and the run serves on.
/// **The level:** the stand-in gate is a thread holding the gate's channel
/// and the command a process the test starts in `tools/`, since no fixture
/// here forks a real gate; the worker's side is the serve loop's own.
/// Perturbation: drop the engine's kill, and the account names no call
/// closed after the containment; drop the kill from `contain`, and the
/// command outlives the call.
#[test]
#[ignore = "needs a delegated cgroup scope"]
fn containment_a_gate_dying_with_a_call_open_leaves_tools_empty() {
    use std::os::fd::AsRawFd;

    use nix::sys::socket::{
        AddressFamily, MsgFlags, SockFlag, SockType, recv as sock_recv, send as sock_send,
        socketpair,
    };

    let layout = crate::tools_cgroup::tests::ScratchLayout::new("gate-death");
    let (mut run, _spare, sink_path) = entered_run(None);
    run.tools =
        Some(crate::tools_cgroup::ToolsCgroup::open_beside_run(&layout.run).expect("tools/ opens"));
    let (near, far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .expect("socketpair");
    run.spu.as_mut().expect("the arm stands").decode = crate::channel::decode_from_owned(near);
    let (gate_end, gate_peer) = OrganChannel::pair().expect("gate pair");
    run.gate = Some(GateChannel {
        channel: gate_end,
        pid: nix::unistd::Pid::from_raw(i32::MAX),
        last_word: crate::spawn::LastWord::quiet(),
        shell_granted: true,
    });
    let gate_peer = gate_peer.into_channel();
    // The SPU asks one tool call, then answers whatever follows plainly.
    let decode_peer = std::thread::spawn(move || {
        let mut buf = vec![0u8; 65536];
        let _ = sock_recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).expect("the append");
        let call = weaver_types::TokenAnswer::Generated(weaver_types::Generation {
            unrecovered: vec![],
            content: vec![weaver_traits::ContentBlock::ToolCall(
                weaver_traits::ToolCall {
                    name: "sh".into(),
                    arguments: r#"{"command":"sleep 300"}"#.into(),
                },
            )],
            emission: "calling".into(),
            finish: weaver_types::Finish::Completed,
            resident: 64,
            capacity: 4096,
            request: serde_json::value::RawValue::from_string(r#"{"rendered":"r"}"#.into())
                .unwrap(),
            measurement: serde_json::value::RawValue::from_string(r#"{}"#.into()).unwrap(),
        });
        sock_send(
            far.as_raw_fd(),
            &serde_json::to_vec(&call).unwrap(),
            MsgFlags::empty(),
        )
        .expect("the call");
        far
    });
    gate_peer
        .send(&OrganEnvelope {
            exchange: ExchangeId {
                opener: Opener::Gate,
                ordinal: 1,
            },
            position: Position::Open,
            payload: weaver_types::Payload::Frame(weaver_types::TurnFrame::carry(
                b"{\"text\":\"run something\"}",
            )),
        })
        .expect("frame sends");
    let tools_leaf = layout.leaf("worker/tools");
    // The gate takes the call, starts its command in tools/, and dies with
    // the call open.
    let gate_stand_in = std::thread::spawn(move || {
        let open = gate_peer.recv().expect("the call");
        assert!(
            matches!(open.payload, weaver_types::Payload::Tool(_)),
            "{open:?}"
        );
        let command = crate::tools_cgroup::tests::escaper_in(&tools_leaf);
        drop(gate_peer);
        command
    });
    let (verb_end, _admin_end) = OrganChannel::pair().expect("admin pair");
    let (mut harness, _dir) = harness_entered(run);
    let mut verb_slot = Some(verb_end);
    let served = harness.serve_gate_wake(
        "identity",
        &mut |ports: &mut crate::engine::Ports<'_>, text: &str| {
            ports.turn(vec![weaver_traits::Message {
                role: weaver_traits::Role::User,
                content: vec![weaver_traits::ContentBlock::Text {
                    text: text.to_string(),
                }],
            }])
        },
        &mut verb_slot,
    );
    let mut command = gate_stand_in.join().expect("the gate stand-in");
    assert!(served.is_ok(), "the run serves on: {served:?}");
    let tools = crate::tools_cgroup::ToolsCgroup::open_beside_run(&layout.run).unwrap();
    assert_eq!(tools.populated(), Some(false), "tools/ is empty");
    assert!(
        command.try_wait().unwrap().is_some(),
        "the command is ended"
    );
    drop(harness);
    drop(decode_peer.join());
    let events: Vec<serde_json::Value> = std::fs::read_to_string(&sink_path)
        .expect("the sink")
        .lines()
        .map(|line| serde_json::from_str(line).expect("a record line"))
        .collect();
    let death = events
        .iter()
        .find(|e| e["kind"] == "fault" && e["payload"]["account"]["organ"] == "gate")
        .unwrap_or_else(|| panic!("the gate's death: {events:?}"));
    assert_eq!(
        death["payload"]["account"]["containment"], "tools/ killed and emptied",
        "{death}"
    );
    assert_eq!(
        death["payload"]["account"]["call"], "closed killed by the fault once tools/ was contained",
        "{death}"
    );
}

/// **The second walk owed: the gate is forked with exactly its end and the
/// handle on `tools/`** (`weaver-harness-Spec` section 8's second walk, with
/// the handle beside the end, section 2.2): a stand-in exec'd by the gate's
/// own fork lists its descriptors, which are the standard streams, its end
/// at 3 and a directory at 4, the one handed. Perturbation: drop the handle
/// from the gate's fork, and 4 is missing.
#[test]
fn the_gate_is_forked_with_its_end_at_3_and_tools_at_4() {
    use std::os::fd::AsFd;
    let base = std::env::temp_dir().join(format!("weaver-gate-walk-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("tools")).unwrap();
    let report = base.join("report");
    let stand_in = base.join("gate");
    std::fs::write(
        &stand_in,
        format!(
            "#!/bin/sh\nls /proc/$$/fd > {report}.tmp\nreadlink /proc/$$/fd/4 >> {report}.tmp\nmv {report}.tmp {report}\n",
            report = report.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(
        &stand_in,
        std::os::unix::fs::PermissionsExt::from_mode(0o755),
    )
    .unwrap();
    let tools = std::fs::File::open(base.join("tools")).unwrap();
    let (_gate, gate_child) = OrganChannel::pair().expect("gate pair");
    // SAFETY: forked from the test's own thread, which outlives the
    // stand-in's few instructions.
    let pid = unsafe { fork_gate(&stand_in, &gate_child, tools.as_fd(), None) }.expect("the fork");
    let _ = nix::sys::wait::waitpid(pid, None);
    let listed = std::fs::read_to_string(&report).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&base);
    let mut lines = listed.lines();
    let mut numbers: Vec<u32> = lines
        .by_ref()
        .take_while(|line| line.parse::<u32>().is_ok())
        .map(|line| line.parse().unwrap())
        // bash, where it is `/bin/sh`, opens its own script at 255 after its
        // exec: the stand-in's, not one the fork handed.
        .filter(|&number| number != 255)
        .collect();
    numbers.sort_unstable();
    assert_eq!(numbers, vec![0, 1, 2, 3, 4], "{listed}");
    assert!(
        listed.trim_end().ends_with("/tools"),
        "4 is the handle on tools/: {listed}"
    );
}

/// **A worker that cannot open `tools/` stands no gate** (`weaver-harness-Spec`
/// section 2.2; `weaver-admin-Spec` 6.1, the worker's row): the suite's own
/// process sits in no `worker/run`, so the open refuses, and the gate's arm
/// answers `OrganRefused { Gate, BoundaryUnverified }`. The enter opens it
/// before the gate's pair and fork, so no gate is forked. Perturbation:
/// refuse anything else, and this fails.
#[test]
fn a_worker_that_cannot_open_tools_refuses_the_gates_arm() {
    assert_eq!(
        open_tools().err(),
        Some(LifecycleRefusal::OrganRefused {
            organ: weaver_types::RefusingOrgan::Gate,
            reason: Box::new(LifecycleRefusal::BoundaryUnverified),
        })
    );
}

/// **The forced reap past the bound is `SIGKILL`, then the reap**
/// (`weaver-admin-Spec` 6.1, a forced leave's reap past the lower's bound;
/// `weaver-harness-Spec` section 6): a child that ignores `SIGTERM` and
/// never exits is killed at the deadline and reaped, so no wait of the
/// gate's own on its containment holds the reap. Perturbation: send
/// `SIGTERM` in its place, and the reap never returns inside the bound.
#[test]
fn the_forced_reap_past_its_deadline_kills_and_reaps() {
    let mut child = std::process::Command::new("sh")
        .args(["-c", "trap '' TERM; while :; do sleep 1; done"])
        .spawn()
        .expect("a stand-in that ignores SIGTERM");
    let pid = nix::unistd::Pid::from_raw(child.id() as i32);
    let began = std::time::Instant::now();
    let (done, finished) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        reap_by(
            pid,
            Some(std::time::Instant::now() + std::time::Duration::from_millis(200)),
        );
        let _ = done.send(());
    });
    let reaped = finished
        .recv_timeout(std::time::Duration::from_secs(5))
        .is_ok();
    if !reaped {
        let _ = nix::sys::signal::kill(pid, nix::sys::signal::Signal::SIGKILL);
    }
    assert!(reaped, "reaped past the deadline");
    assert!(began.elapsed() >= std::time::Duration::from_millis(200));
    assert!(
        nix::sys::signal::kill(pid, None).is_err(),
        "the stand-in is gone and reaped"
    );
    // Reaped by `reap_by` already: the standard library's wait finds no child.
    assert!(child.wait().is_err());
}

/// **Service ends on a serving containment that failed, never inside a
/// leave** (`weaver-admin-Spec` 6.1, the worker's rows 2680 and 2682, and
/// the leave's): `PastBound` and `Undelivered` while serving end it; an
/// emptied `tools/`, and anything inside a leave, do not. Perturbation:
/// change an arm, and its row fails.
#[test]
fn service_ends_on_a_serving_containment_that_failed() {
    use crate::tools_cgroup::{Containment::*, Context::*};
    assert!(!ends_service(None));
    assert!(!ends_service(Some((Serving, Emptied))));
    assert!(ends_service(Some((Serving, PastBound))));
    assert!(ends_service(Some((Serving, Undelivered))));
    assert!(!ends_service(Some((Leave, Delivered))));
    assert!(!ends_service(Some((Leave, Undelivered))));
}

/// **The unwind kills `tools/` at the gate's lower, before `Left`**
/// (`weaver-admin-Spec` 6.1, the worker's unwind; `weaver-harness-Spec`
/// section 2.2): every leave lowers the gate first, and a command still in
/// `tools/` there is killed by the lower's sweep. Here the gate has died, so
/// the lower meets its channel closed. Perturbation: drop the lower's
/// sweep, and the command outlives the leave.
#[test]
#[ignore = "needs a delegated cgroup scope"]
fn containment_the_lower_sweeps_tools() {
    let layout = crate::tools_cgroup::tests::ScratchLayout::new("lower");
    let (mut run, _spare, _sink) = entered_run(None);
    run.tools =
        Some(crate::tools_cgroup::ToolsCgroup::open_beside_run(&layout.run).expect("tools/ opens"));
    let (gate_end, gate_peer) = OrganChannel::pair().expect("gate pair");
    run.gate = Some(GateChannel {
        channel: gate_end,
        pid: nix::unistd::Pid::from_raw(i32::MAX),
        last_word: crate::spawn::LastWord::quiet(),
        shell_granted: true,
    });
    let mut command = crate::tools_cgroup::tests::escaper_in(&layout.leaf("worker/tools"));
    drop(gate_peer);
    let _ = lower_gate_within(&mut run, 100, None);
    let tools = crate::tools_cgroup::ToolsCgroup::open_beside_run(&layout.run).unwrap();
    assert!(
        tools.wait_empty(std::time::Duration::from_secs(5)),
        "the lower's sweep emptied tools/"
    );
    assert!(command.wait().is_ok());
}

/// **A forced leave never waits on the interrupted call past the lower's
/// bound** (`weaver-admin-Spec` 6.1, a forced leave's reap past the lower's
/// bound; the self-check of #111's PR C): the gate takes the interrupt and
/// never answers it, and the engine closes the call `Killed { by: fault }`
/// at the lower's bound from the interrupt, the leave going on to `Left`,
/// and the lower reaps the gate in what remains of that same window.
/// Perturbation: drop the interrupt's bound, and the leave waits for the
/// gate's channel to close, a minute later; start the lower's window afresh,
/// and the leave takes two lower bounds.
#[test]
fn a_forced_leave_closes_a_call_its_gate_never_answers_within_the_lower_bound() {
    use std::os::fd::AsRawFd;

    use nix::sys::socket::{
        AddressFamily, MsgFlags, SockFlag, SockType, recv as sock_recv, send as sock_send,
        socketpair,
    };

    let (mut run, _spare, sink_path) = entered_run(None);
    let (near, far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .expect("socketpair");
    run.spu.as_mut().expect("the arm stands").decode = crate::channel::decode_from_owned(near);
    let (spu_lifecycle, spu_far) = OrganChannel::pair().expect("spu lifecycle pair");
    run.spu.as_mut().expect("the arm stands").lifecycle = spu_lifecycle;
    let spu_far = spu_far.into_channel();
    let releases = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = releases.clone();
    std::thread::spawn(move || {
        while let Ok(asked) = spu_far.recv() {
            counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let _ = spu_far.send(&OrganEnvelope {
                exchange: asked.exchange,
                position: Position::Close,
                payload: weaver_types::Payload::Answer(LifecycleAnswer::Released),
            });
        }
    });
    let (gate_end, gate_peer) = OrganChannel::pair().expect("gate pair");
    run.gate = Some(GateChannel {
        channel: gate_end,
        pid: nix::unistd::Pid::from_raw(i32::MAX),
        last_word: crate::spawn::LastWord::quiet(),
        shell_granted: true,
    });
    let gate_peer = gate_peer.into_channel();
    // The SPU asks one tool call and stays standing.
    let decode_peer = std::thread::spawn(move || {
        let mut buf = vec![0u8; 65536];
        let _ = sock_recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).expect("the append");
        let call = weaver_types::TokenAnswer::Generated(weaver_types::Generation {
            unrecovered: vec![],
            content: vec![weaver_traits::ContentBlock::ToolCall(
                weaver_traits::ToolCall {
                    name: "sh".into(),
                    arguments: r#"{"command":"sleep 5"}"#.into(),
                },
            )],
            emission: "calling".into(),
            finish: weaver_types::Finish::Completed,
            resident: 64,
            capacity: 4096,
            request: serde_json::value::RawValue::from_string(r#"{"rendered":"r"}"#.into())
                .unwrap(),
            measurement: serde_json::value::RawValue::from_string(r#"{}"#.into()).unwrap(),
        });
        sock_send(
            far.as_raw_fd(),
            &serde_json::to_vec(&call).unwrap(),
            MsgFlags::empty(),
        )
        .expect("the call");
        far
    });
    gate_peer
        .send(&OrganEnvelope {
            exchange: ExchangeId {
                opener: Opener::Gate,
                ordinal: 1,
            },
            position: Position::Open,
            payload: weaver_types::Payload::Frame(weaver_types::TurnFrame::carry(
                b"{\"text\":\"run something\"}",
            )),
        })
        .expect("frame sends");
    let (verb_end, admin_end) = OrganChannel::pair().expect("admin pair");
    let admin = admin_end.into_channel();
    // The gate takes the call; the leave is heard during the call's wait;
    // the gate then dies with the call open.
    let (handed, admin_back) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let open = gate_peer.recv().expect("the call");
        assert!(
            matches!(open.payload, weaver_types::Payload::Tool(_)),
            "{open:?}"
        );
        admin
            .send(&OrganEnvelope {
                exchange: ExchangeId {
                    opener: Opener::Admin,
                    ordinal: 4,
                },
                position: Position::Open,
                payload: weaver_types::Payload::Directive(LifecycleDirective::Leave {
                    cause: weaver_types::Cause { uid: 7 },
                    forced: true,
                    rollback: false,
                }),
            })
            .expect("the leave sends");
        let _ = handed.send(admin);
        // The gate takes the interrupt and never answers it, its channel
        // held open well past the lower's bound.
        let interrupt = gate_peer.recv().expect("the interrupt");
        assert!(
            matches!(interrupt.payload, weaver_types::Payload::ToolInterrupt),
            "{interrupt:?}"
        );
        std::thread::sleep(std::time::Duration::from_secs(60));
        drop(gate_peer);
    });
    let (mut harness, _dir) = harness_entered(run);
    let began = std::time::Instant::now();
    let mut verb_slot = Some(verb_end);
    let served = harness.serve_gate_wake(
        "identity",
        &mut |ports: &mut crate::engine::Ports<'_>, text: &str| {
            let delta = vec![weaver_traits::Message {
                role: weaver_traits::Role::User,
                content: vec![weaver_traits::ContentBlock::Text {
                    text: text.to_string(),
                }],
            }];
            ports.turn(delta)
        },
        &mut verb_slot,
    );
    let took = began.elapsed();
    let admin = admin_back.recv().expect("admin handed back");
    assert!(served.is_ok(), "{served:?}");
    let left = admin.recv().expect("admin is answered");
    let weaver_types::Payload::Answer(LifecycleAnswer::Left { forced, .. }) = left.payload else {
        panic!("{left:?}");
    };
    assert!(forced);
    // **One anchor per leave** (Codex on #167): the lower's window began at
    // the interrupt, so the gate is reaped when that one window ends and the
    // whole leave takes one lower bound, not two.
    eprintln!("a forced leave over an unanswering gate took {took:?}");
    assert!(
        took < std::time::Duration::from_millis(weaver_types::LOWER_BOUND_MS + 4_000),
        "the leave spent the lower's bound once: {took:?}"
    );
    let stream = std::fs::read_to_string(&sink_path).expect("the sink");
    assert!(
        stream.contains(r#""by":"fault""#),
        "the call the gate never answered closes killed by the fault: {stream}"
    );
    drop(decode_peer.join());
}

/// **A live gate's unreadable frame mid-call kills nothing in `tools/`**
/// (the self-check of #111's PR C): the engine kills `tools/` only where
/// the gate's channel hung up, so a gate that is alive and sends what the
/// harness cannot read ends service without a kill, and leaves no stale
/// containment for the unwind to read. Perturbation: kill on every receive
/// error, and the command in `tools/` dies.
#[test]
#[ignore = "needs a delegated cgroup scope"]
fn containment_a_live_gates_bad_frame_kills_nothing() {
    use std::os::fd::AsRawFd;

    use nix::sys::socket::{
        AddressFamily, MsgFlags, SockFlag, SockType, recv as sock_recv, send as sock_send,
        socketpair,
    };

    let layout = crate::tools_cgroup::tests::ScratchLayout::new("bad-frame");
    let (mut run, _spare, _sink) = entered_run(None);
    run.tools =
        Some(crate::tools_cgroup::ToolsCgroup::open_beside_run(&layout.run).expect("tools/ opens"));
    let (near, far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .expect("socketpair");
    run.spu.as_mut().expect("the arm stands").decode = crate::channel::decode_from_owned(near);
    let (gate_end, gate_peer) = OrganChannel::pair().expect("gate pair");
    run.gate = Some(GateChannel {
        channel: gate_end,
        pid: nix::unistd::Pid::from_raw(i32::MAX),
        last_word: crate::spawn::LastWord::quiet(),
        shell_granted: true,
    });
    let gate_peer = gate_peer.into_channel();
    let decode_peer = std::thread::spawn(move || {
        let mut buf = vec![0u8; 65536];
        let _ = sock_recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).expect("the append");
        let call = weaver_types::TokenAnswer::Generated(weaver_types::Generation {
            unrecovered: vec![],
            content: vec![weaver_traits::ContentBlock::ToolCall(
                weaver_traits::ToolCall {
                    name: "sh".into(),
                    arguments: r#"{"command":"sleep 300"}"#.into(),
                },
            )],
            emission: "calling".into(),
            finish: weaver_types::Finish::Completed,
            resident: 64,
            capacity: 4096,
            request: serde_json::value::RawValue::from_string(r#"{"rendered":"r"}"#.into())
                .unwrap(),
            measurement: serde_json::value::RawValue::from_string(r#"{}"#.into()).unwrap(),
        });
        sock_send(
            far.as_raw_fd(),
            &serde_json::to_vec(&call).unwrap(),
            MsgFlags::empty(),
        )
        .expect("the call");
        far
    });
    gate_peer
        .send(&OrganEnvelope {
            exchange: ExchangeId {
                opener: Opener::Gate,
                ordinal: 1,
            },
            position: Position::Open,
            payload: weaver_types::Payload::Frame(weaver_types::TurnFrame::carry(
                b"{\"text\":\"run something\"}",
            )),
        })
        .expect("frame sends");
    let tools_leaf = layout.leaf("worker/tools");
    let gate_stand_in = std::thread::spawn(move || {
        let _open = gate_peer.recv().expect("the call");
        let command = crate::tools_cgroup::tests::escaper_in(&tools_leaf);
        // Octets the harness cannot read, the gate standing.
        let _ = sock_send(
            std::os::fd::AsFd::as_fd(&gate_peer).as_raw_fd(),
            b"not an envelope",
            MsgFlags::empty(),
        );
        (gate_peer, command)
    });
    let (verb_end, _admin_end) = OrganChannel::pair().expect("admin pair");
    let (mut harness, _dir) = harness_entered(run);
    let mut verb_slot = Some(verb_end);
    let _ = harness.serve_gate_wake(
        "identity",
        &mut |ports: &mut crate::engine::Ports<'_>, text: &str| {
            ports.turn(vec![weaver_traits::Message {
                role: weaver_traits::Role::User,
                content: vec![weaver_traits::ContentBlock::Text {
                    text: text.to_string(),
                }],
            }])
        },
        &mut verb_slot,
    );
    let (gate_peer, mut command) = gate_stand_in.join().expect("the gate stand-in");
    let alive = command.try_wait().unwrap().is_none();
    let _ = command.kill();
    let _ = command.wait();
    drop(gate_peer);
    drop(harness);
    drop(decode_peer.join());
    assert!(
        alive,
        "the command in tools/ was not killed by a live gate's bad frame"
    );
}

/// **The unwind always sweeps `tools/` itself** (`weaver-admin-Spec` 6.1, the
/// worker's unwind; the Planner's grade of #167): an earlier kill's result
/// left in the run, an interrupt's pass that delivered nothing among them,
/// is never taken for the unwind's own write or pass. Perturbation: take the
/// earlier result, and the command in `tools/` outlives the lower.
#[test]
#[ignore = "needs a delegated cgroup scope"]
fn containment_the_lower_sweeps_tools_whatever_an_earlier_kill_said() {
    let layout = crate::tools_cgroup::tests::ScratchLayout::new("lower-stale");
    let (mut run, _spare, _sink) = entered_run(None);
    run.tools =
        Some(crate::tools_cgroup::ToolsCgroup::open_beside_run(&layout.run).expect("tools/ opens"));
    run.gate_contained = Some((
        crate::tools_cgroup::Context::Leave,
        crate::tools_cgroup::Containment::Undelivered,
    ));
    let (gate_end, gate_peer) = OrganChannel::pair().expect("gate pair");
    run.gate = Some(GateChannel {
        channel: gate_end,
        pid: nix::unistd::Pid::from_raw(i32::MAX),
        last_word: crate::spawn::LastWord::quiet(),
        shell_granted: true,
    });
    let mut command = crate::tools_cgroup::tests::escaper_in(&layout.leaf("worker/tools"));
    drop(gate_peer);
    let _ = lower_gate_within(&mut run, 100, None);
    let tools = crate::tools_cgroup::ToolsCgroup::open_beside_run(&layout.run).unwrap();
    let emptied = tools.wait_empty(std::time::Duration::from_secs(5));
    let _ = command.kill();
    let _ = command.wait();
    assert!(emptied, "the unwind's own sweep emptied tools/");
}

/// **A gate's death at rest with a leave held never waits on `tools/`**
/// (`weaver-admin-Spec` 6.1, inside a leave nothing waits; the Planner's
/// grade of #167): the table's at-rest position (S2, `GateFaultServeOn`)
/// does not show a leave the turn held, so the context comes from the run.
/// With `cgroup.events` unreadable a serving wait would run its whole bound
/// and end service; with the leave held the death is met at once and the
/// run stands for the leave. Perturbation: meet it serving, and it waits
/// the bound and ends service.
#[test]
#[ignore = "needs a delegated cgroup scope"]
fn containment_a_gate_dying_at_rest_with_a_leave_held_never_waits() {
    use std::os::unix::fs::PermissionsExt;
    let layout = crate::tools_cgroup::tests::ScratchLayout::new("rest-leave");
    let (mut run, _spare, _sink) = entered_run(None);
    run.tools =
        Some(crate::tools_cgroup::ToolsCgroup::open_beside_run(&layout.run).expect("tools/ opens"));
    let (gate_end, gate_peer) = OrganChannel::pair().expect("gate pair");
    run.gate = Some(GateChannel {
        channel: gate_end,
        pid: nix::unistd::Pid::from_raw(i32::MAX),
        last_word: crate::spawn::LastWord::quiet(),
        shell_granted: true,
    });
    let (held, _admin) = pending_leave(
        crate::lifecycle_table::Leave::Graceful,
        crate::lifecycle_table::Leave::Graceful,
        false,
    );
    run.leave = Some(held);
    let mut command = crate::tools_cgroup::tests::escaper_in(&layout.leaf("worker/tools"));
    drop(gate_peer);
    let events = layout.leaf("worker/tools/cgroup.events");
    std::fs::set_permissions(&events, std::fs::Permissions::from_mode(0o000)).unwrap();
    let (mut harness, _dir) = harness_entered(run);
    let began = std::time::Instant::now();
    let died = harness.organ_died(OrganDeath::Gate);
    let took = began.elapsed();
    std::fs::set_permissions(&events, std::fs::Permissions::from_mode(0o444)).unwrap();
    let _ = command.kill();
    let _ = command.wait();
    assert!(died.is_ok(), "the run stands for the leave: {died:?}");
    assert!(
        took < std::time::Duration::from_secs(5),
        "nothing waited on tools/: {took:?}"
    );
}

/// **The unwind's undelivered sweep is its own fault, never a death**
/// (`weaver-admin-Spec` 6.1, the unwind with the write and the pass both
/// failing; `weaver-harness-PRD` section 5; the operator's ruling of
/// 2026-10-15): the gate lowers without dying, `cgroup.kill` and
/// `cgroup.procs` closed to the worker so no kill is delivered, and the
/// lower authors `tools_containment_undelivered`, the gate's account naming
/// the containment and no death. Perturbation: author it as
/// `organ_death_observed`, and this fails.
#[test]
#[ignore = "needs a delegated cgroup scope"]
fn containment_an_undelivered_unwind_sweep_is_its_own_fault() {
    use std::os::unix::fs::PermissionsExt;
    let layout = crate::tools_cgroup::tests::ScratchLayout::new("undelivered");
    let (mut run, _spare, sink_path) = entered_run(None);
    run.tools =
        Some(crate::tools_cgroup::ToolsCgroup::open_beside_run(&layout.run).expect("tools/ opens"));
    let (gate_end, gate_peer) = OrganChannel::pair().expect("gate pair");
    run.gate = Some(GateChannel {
        channel: gate_end,
        pid: nix::unistd::Pid::from_raw(i32::MAX),
        last_word: crate::spawn::LastWord::quiet(),
        shell_granted: true,
    });
    let kill = layout.leaf("worker/tools/cgroup.kill");
    let procs = layout.leaf("worker/tools/cgroup.procs");
    std::fs::set_permissions(&kill, std::fs::Permissions::from_mode(0o444)).unwrap();
    std::fs::set_permissions(&procs, std::fs::Permissions::from_mode(0o200)).unwrap();
    // The gate stands and does not answer the lower: it never dies here.
    let _ = lower_gate_within(&mut run, 100, None);
    std::fs::set_permissions(&kill, std::fs::Permissions::from_mode(0o644)).unwrap();
    std::fs::set_permissions(&procs, std::fs::Permissions::from_mode(0o644)).unwrap();
    drop(gate_peer);
    drop(run);
    let events: Vec<serde_json::Value> = std::fs::read_to_string(&sink_path)
        .expect("the sink")
        .lines()
        .map(|line| serde_json::from_str(line).expect("a record line"))
        .collect();
    let faults: Vec<&serde_json::Value> = events.iter().filter(|e| e["kind"] == "fault").collect();
    let undelivered = faults
        .iter()
        .find(|e| e["payload"]["case"] == "tools_containment_undelivered")
        .unwrap_or_else(|| panic!("the undelivered sweep's fault: {faults:?}"));
    assert_eq!(undelivered["payload"]["account"]["organ"], "gate");
    assert!(
        undelivered["payload"]["account"]["containment"]
            .as_str()
            .is_some_and(|words| words.starts_with("containment failed")),
        "{undelivered}"
    );
    assert!(
        undelivered["payload"]["account"].get("death").is_none(),
        "no death is named: {undelivered}"
    );
    assert!(
        !faults
            .iter()
            .any(|e| e["payload"]["case"] == "organ_death_observed"),
        "no death is recorded: {faults:?}"
    );
}

/// The retiring boot adapter must consume the complete paired Opened shape.
/// It owns no placement budget. The foundation boot owner supplies that check.
#[test]
fn opening_requires_the_complete_paired_answer_and_not_a_measurement() {
    let opened = r#"{"kind":"opened","body":{"prefix_tokens":20,"capacity":4096,"generation_limit":128,"terminal_tokens":1}}"#;
    let cases = [
        (opened, Ok(())),
        (r#"{"kind":"opened"}"#, Err(LifecycleRefusal::NoResidency)),
        (
            r#"{"kind":"opened","body":{"prefix_tokens":20,"capacity":4096,"generation_limit":128}}"#,
            Err(LifecycleRefusal::NoResidency),
        ),
        (
            r#"{"kind":"delta_measured","body":{"resident":20,"prefix_tokens":20,"keep":20,"input_tokens":9,"capacity":4096}}"#,
            Err(LifecycleRefusal::DeviceCannotAdmit),
        ),
        (
            r#"{"kind":"not_open"}"#,
            Err(LifecycleRefusal::DeviceCannotAdmit),
        ),
    ];
    for (body, expected) in cases {
        use nix::sys::socket::{MsgFlags, recv, send};
        use std::os::fd::AsRawFd;
        let (decode, peer) = DecodeChannel::pair().unwrap();
        let task = std::thread::spawn(move || {
            let mut buffer = [0u8; 4096];
            let count = recv(peer.as_fd().as_raw_fd(), &mut buffer, MsgFlags::empty()).unwrap();
            let directive: TokenDirective = serde_json::from_slice(&buffer[..count]).unwrap();
            assert_eq!(
                directive,
                TokenDirective::Open {
                    session: SessionId("paired-open".into()),
                    messages: vec![],
                    column_ask: false,
                }
            );
            send(peer.as_fd().as_raw_fd(), body.as_bytes(), MsgFlags::empty()).unwrap();
        });
        assert_eq!(
            open_session(&decode, SessionId("paired-open".into()), vec![], false),
            expected
        );
        task.join().unwrap();
    }
}
