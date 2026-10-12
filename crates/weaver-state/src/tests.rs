/// **The answer bound is the contract's one number**, per
/// `weaver-harness-state-contract` section 3: one mebibyte, which the
/// harness's `ANSWER_BOUND_BYTES` holds as the same number under its own
/// watch, the two crates seeing neither's constant.
#[test]
fn the_answer_bound_is_the_contracts_mebibyte() {
    assert_eq!(super::ANSWER_BOUND, 1024 * 1024);
}

/// A path under the temp directory, removed when the test ends, pass or
/// fail: the guard drops on the unwind a failed assertion takes as on a
/// clean return (#690 item C2.9).
struct Scratch(std::path::PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        match std::fs::symlink_metadata(&self.0) {
            Ok(meta) if meta.is_dir() => drop(std::fs::remove_dir_all(&self.0)),
            Ok(_) => drop(std::fs::remove_file(&self.0)),
            Err(_) => {}
        }
    }
}

/// **The vector is the territory and the preload name, and no flag**, per
/// `weaver-state-Spec` section 2: the engine flag and the service engine's
/// three left with that engine. Perturbation: accept `--engine` again and
/// the retired flag parses.
#[test]
fn the_vector_is_the_territory_and_the_preload_name() {
    fn words(v: &[&str]) -> std::vec::IntoIter<String> {
        v.iter()
            .map(|w| w.to_string())
            .collect::<Vec<_>>()
            .into_iter()
    }
    let serving = super::StoreVector::parse(words(&["/t"])).expect("parses");
    assert_eq!(serving.territory, "/t");
    assert!(serving.preload_socket.is_none());
    let diagnostic = super::StoreVector::parse(words(&["/t", "/t/preload.sock"])).expect("parses");
    assert_eq!(diagnostic.territory, "/t");
    assert_eq!(
        diagnostic.preload_socket.as_deref(),
        Some("/t/preload.sock")
    );
    // The retired flags refuse as any unknown flag does.
    for flag in ["--engine", "--store-socket", "--database", "--role"] {
        assert!(
            super::StoreVector::parse(words(&[flag, "x", "/t"])).is_none(),
            "{flag} retired with the service engine"
        );
    }
    assert!(
        super::StoreVector::parse(words(&["--other", "x", "/t"])).is_none(),
        "an unknown flag"
    );
    assert!(
        super::StoreVector::parse(words(&["/t", "/p", "/x"])).is_none(),
        "three positionals"
    );
    assert!(
        super::StoreVector::parse(words(&[])).is_none(),
        "no territory"
    );
}

use super::*;

/// **The preload door denies every uid but its owner.**
///
/// `bind` sets no mode, so this name would land at `0777 & ~umask` and
/// the boundary's permissions would be whatever umask the process
/// inherited. Only `uid() == 0` is admitted at the accept, so there is no
/// group to reach the door, and `0700` is the access it offers.
///
/// **The ambient umask is read and left alone**, and where it would
/// produce `0700` by itself the test says so and skips rather than
/// asserting against a run that cannot distinguish.
///
/// Perturbation: drop the `PreloadUmask::deny_all_but_owner()` guard from
/// `stand_preload_name` and this reports `0777`. Watched under exactly
/// that removal, and watched **here** - the form this replaced skipped
/// wherever the ambient umask already produced `0700`, which `0077` does,
/// so on a hardened image it ran nowhere while its record said otherwise.
#[test]
fn the_preload_door_denies_every_uid_but_its_owner() {
    use std::os::unix::fs::PermissionsExt;
    let scratch = Scratch(std::env::temp_dir().join(format!(
        "weaver-state-preload-mode-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    )));
    let path = scratch.0.to_str().expect("a utf-8 scratch path");
    // **The ambient umask is elected rather than read, so this runs
    // everywhere.** An earlier form read it and skipped where it already
    // produced `0700`, which `0077` does - a common hardened default, so
    // the watch skipped on a hardened CI image and a later removal of
    // the guard would have shipped green. Reading avoided a false red at
    // the cost of the watch running nowhere, which is the trade this
    // program does not take.
    //
    // `0o000` cannot produce `0700` by itself, so under it the mode on
    // disk is the election or nothing. The lock is held across the whole
    // window, and it is reentrant, so `stand_preload_name` taking it
    // again inside is a no-op rather than a wait.
    let elected = with_umask_held(|| {
        let previous = nix::sys::stat::umask(nix::sys::stat::Mode::empty());
        let stood = stand_preload_name(path);
        let seen = std::fs::metadata(path).map(|meta| meta.permissions().mode() & 0o777);
        nix::sys::stat::umask(previous);
        (stood, seen)
    });
    let (listener, seen) = elected;
    assert!(listener.is_some(), "the preload name stands");
    let mode = seen.expect("the socket is on disk");
    assert_eq!(
        mode, 0o700,
        "the door states its mode rather than inheriting one, got {mode:04o}"
    );
    drop(listener);
}

/// **A replay ask parks at an open preload until the seal**, per
/// `weaver-harness-state-contract` section 2 and
/// `state-replay-answers-at-the-seal`: where the door stands and no
/// seal has landed the ask parks, a second replaces the first with one
/// answer still owed, the seal readies exactly one answer, and a
/// sealless close readies nothing because the seal alone answers.
/// Where no door stands the ask answers immediately.
///
/// Perturbation: make `parks` ignore the seal (park whenever the door
/// stands) and the after-the-seal case fails; make `take_ready` ignore
/// the seal and the sealless case fails.
#[test]
fn a_replay_parks_at_an_open_preload_until_the_seal() {
    // No door: never parks.
    let mut open_door = ReplayParking::new(false);
    assert!(
        !open_door.parks(&Ask::Replay),
        "no door, the ask answers now"
    );
    // Door standing, no seal: parks, and a second ask replaces the
    // first rather than queueing a second answer.
    let mut parking = ReplayParking::new(true);
    assert!(parking.parks(&Ask::Replay), "an open preload parks the ask");
    assert!(
        parking.parks(&Ask::Replay),
        "a second ask replaces the first"
    );
    assert!(parking.take_ready().is_empty(), "the seal alone answers");
    parking.seal();
    assert_eq!(
        parking.take_ready(),
        vec![Ask::Replay],
        "the seal readies one"
    );
    assert!(parking.take_ready().is_empty(), "one answer per parked ask");
    // After the seal, an ask answers immediately: sealed never unsets.
    assert!(!parking.parks(&Ask::Replay), "after the seal nothing parks");
}

/// **The identity and recall asks park on the seal beside the replay**,
/// per `weaver-state-Spec` section 4 as of 2026-09-06 and the contract's
/// parking clause of 2026-09-04: where the door stands and no seal has
/// landed each parks, they answer at the seal in arrival order, the
/// replay's replacement rule reaches the replay alone, and the shape
/// and grants asks never park.
///
/// Perturbation: park the replay alone, as the law did before this
/// act, and the identity ask answers before the seal from empty
/// holdings, the first assertion failing. Watched under exactly that
/// removal.
#[test]
fn the_enters_asks_park_on_the_seal_in_arrival_order() {
    let mut parking = ReplayParking::new(true);
    assert!(parking.parks(&Ask::Identity), "the identity ask parks");
    assert!(
        parking.parks(&Ask::Recall { last_turns: None }),
        "the recall ask parks"
    );
    assert!(parking.parks(&Ask::Replay));
    assert!(parking.parks(&Ask::Replay), "the replay replaces its own");
    assert!(!parking.parks(&Ask::Shape), "a shape ask answers now");
    assert!(!parking.parks(&Ask::Grants), "a grants ask answers now");
    assert!(parking.take_ready().is_empty(), "nothing before the seal");
    parking.seal();
    assert_eq!(
        parking.take_ready(),
        vec![Ask::Identity, Ask::Recall { last_turns: None }, Ask::Replay],
        "arrival order, the replay held once"
    );
    let mut doorless = ReplayParking::new(false);
    assert!(
        !doorless.parks(&Ask::Identity),
        "no door, the identity answers now"
    );
}

/// **The preload door admits the operator principal and refuses every
/// other peer**, per `weaver-state-Spec` section 4 as ruled 2026-08-26:
/// this member's one credential judgment, the agent's uid among the
/// refused and no longer knowable by number. The dial in this test
/// carries the running uid, which is not the operator's, so the door
/// refuses it. The refusal half is what a suite can exercise, the admit
/// half wanting the operator principal a test does not run as.
///
/// Perturbation: drop the `credentials.uid() == 0` arm from
/// `admit_operator` and this fails, a non-operator peer admitted to a
/// door that exists to keep everything but the operator out.
#[test]
fn the_preload_door_refuses_every_peer_but_the_operator() {
    if nix::unistd::getuid().is_root() {
        // The refusal half is meaningful only unrooted: a root dial is
        // the operator principal and is admitted.
        return;
    }
    let scratch =
        Scratch(std::env::temp_dir().join(format!("weaver-state-preload-{}", std::process::id())));
    let path = scratch.0.to_string_lossy().into_owned();
    let listener = stand_preload_name(&path).expect("the name stands");
    let _dialer = std::os::unix::net::UnixStream::connect(&path).expect("dials");
    // **Wait for the door to be readable before judging it.** The
    // listener is non-blocking and `admit_operator` answers `None`
    // both for a refused peer and for an accept that would block, so
    // a bare call could report a refusal the credential never made.
    // Polling first makes the accept certain and the `None` mean the
    // one thing this test is about.
    use std::os::fd::AsFd;
    let mut fds = [nix::poll::PollFd::new(
        listener.as_fd(),
        nix::poll::PollFlags::POLLIN,
    )];
    let ready = nix::poll::poll(&mut fds, 5_000u16).expect("the door answers poll");
    assert_eq!(ready, 1, "the dial reaches the door");
    assert!(
        admit_operator(&listener).is_none(),
        "a non-operator peer is refused"
    );
}

/// **The door stands only where the party that stands the member names
/// it**, per `weaver-state-Spec` section 4, and this crate's half of that
/// is the second argument: no name, no door. The kind is the caller's
/// fact and reaches here as the name's presence alone, which is what
/// keeps this crate from holding an opinion about the binding.
///
/// **The perturbation is at the vector and not here.** No name reaches
/// this function on a serving load, so the absence is decided in
/// `StoreVector::parse`, and defaulting the preload positional there
/// stands a door nothing should dial. Watched under exactly that change,
/// by `the_vector_is_the_territory_and_the_preload_name`, whose
/// `preload_socket.is_none()` fails on it.
///
/// **This test watches the positive half**, that a named one stands. Its
/// first line takes a `None` through `Option::map` and is a property of
/// that combinator rather than of this crate, `stand_preload_name` taking
/// a `&str` and never an absence. It is disclosed rather than deleted
/// because whether it goes is a decision about the suite.
#[test]
fn no_preload_name_means_no_preload_door() {
    let absent: Option<String> = None;
    assert!(
        absent.as_deref().map(stand_preload_name).is_none(),
        "a serving load names no preload socket and stands no door"
    );
    let scratch =
        Scratch(std::env::temp_dir().join(format!("weaver-state-named-{}", std::process::id())));
    let path = scratch.0.to_string_lossy().into_owned();
    assert!(
        stand_preload_name(&path).is_some(),
        "and a named one stands"
    );
}

/// **An acknowledgement finishes a part only on its ask and its digest
/// both** (the #99 area 1 review, T17): the right digest under another
/// ask's number is a late or misrouted frame, and the part is discarded,
/// no finished name given; the matching pair finishes it. Perturbation:
/// compare the digest alone and the misnumbered acknowledgement finishes
/// the part.
#[cfg(feature = "sqlite")]
#[test]
fn an_acknowledgement_finishes_only_on_its_number_and_digest() {
    let scratch = Scratch(std::env::temp_dir().join(format!(
        "weaver-state-ack-number-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    )));
    std::fs::create_dir_all(&scratch.0).expect("room");
    let room = Room::open(&scratch.0).expect("opens");
    let mut store = weaver_state::engine::sqlite::Sqlite::stand().expect("stands");
    let mut custody = Custody {
        store: &mut store,
        room: &room,
        session: "s-1",
        restored: weaver_state::Restored::Empty,
        pending: None,
        save_point_bound: weaver_state::save_point::SAVE_POINT_BOUND,
        unlanded: None,
    };
    let finished = |dir: &std::path::Path| {
        std::fs::read_dir(dir)
            .expect("lists")
            .filter_map(|entry| entry.ok())
            .filter(|entry| !entry.file_name().to_string_lossy().starts_with('.'))
            .count()
    };
    answer_frame(&Ask::Snapshot { ordinal: 1 }, &mut custody).expect("answers");
    let digest = custody.pending.as_ref().expect("pending").digest.clone();
    let misnumbered = answer_frame(
        &Ask::Acknowledge {
            ordinal: 2,
            digest: digest.clone(),
        },
        &mut custody,
    );
    assert!(
        misnumbered.is_err(),
        "another ask's number finishes nothing"
    );
    assert_eq!(finished(&scratch.0), 0, "no finished name was given");
    answer_frame(&Ask::Snapshot { ordinal: 3 }, &mut custody).expect("answers");
    let digest = custody.pending.as_ref().expect("pending").digest.clone();
    answer_frame(&Ask::Acknowledge { ordinal: 3, digest }, &mut custody)
        .expect("its own number and digest finish it");
    assert_eq!(finished(&scratch.0), 1);
}

/// A store whose landings all fail, the rest delegated to the embedded
/// engine, for the watch on a landing that does not land.
#[cfg(feature = "sqlite")]
struct RefusingLand(
    weaver_state::engine::sqlite::Sqlite,
    fn() -> weaver_state::CustodyFault,
);

#[cfg(feature = "sqlite")]
impl weaver_state::Store for RefusingLand {
    fn index_election(&mut self, e: &Election) -> Result<(), weaver_state::CustodyFault> {
        self.0.index_election(e)
    }
    fn land(&mut self, _: &weaver_state::Distillate) -> Result<(), weaver_state::CustodyFault> {
        Err((self.1)())
    }
    fn retire_and_index(
        &mut self,
        s: &str,
        e: &Election,
    ) -> Result<(), weaver_state::CustodyFault> {
        self.0.retire_and_index(s, e)
    }
    fn replay(
        &self,
        s: &str,
    ) -> Result<Vec<weaver_state::RecalledEvent>, weaver_state::CustodyFault> {
        self.0.replay(s)
    }
    fn held(&self) -> Result<i64, weaver_state::CustodyFault> {
        self.0.held()
    }
    fn shape(&self, s: &str) -> Result<Vec<weaver_state::RunShape>, weaver_state::CustodyFault> {
        self.0.shape(s)
    }
    fn recall(
        &self,
        s: &str,
        last_turns: Option<u64>,
    ) -> Result<Vec<weaver_state::RecalledEvent>, weaver_state::CustodyFault> {
        self.0.recall(s, last_turns)
    }
    fn identity(
        &self,
        s: &str,
    ) -> Result<Vec<weaver_state::RecalledEvent>, weaver_state::CustodyFault> {
        self.0.identity(s)
    }
    fn image(&self) -> Result<Vec<u8>, weaver_state::CustodyFault> {
        self.0.image()
    }
    fn judge_image(
        &self,
        image: &[u8],
        s: &str,
    ) -> Result<weaver_state::ImageFacts, weaver_state::CustodyFault> {
        self.0.judge_image(image, s)
    }
    fn adopt(&mut self, image: &[u8], e: &Election) -> Result<(), weaver_state::CustodyFault> {
        self.0.adopt(image, e)
    }
    fn schema(&self) -> Result<String, weaver_state::CustodyFault> {
        self.0.schema()
    }
    fn position(
        &self,
    ) -> Result<Option<weaver_state::save_point::Stamp>, weaver_state::CustodyFault> {
        self.0.position()
    }
}

/// **A distillate the schema refuses poisons nothing** (the #99 area 1
/// grade of N2): the schema's refusal is `weaver-state-Spec` section 3's
/// designed outcome, so it notes nothing and the next `snapshot` still
/// answers; only a fault of the store does not. Perturbation: hold every
/// landing error as a store fault and the snapshot answers nothing.
#[cfg(feature = "sqlite")]
#[test]
fn a_landing_the_schema_refuses_poisons_nothing() {
    let scratch = Scratch(std::env::temp_dir().join(format!(
        "weaver-state-refused-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    )));
    std::fs::create_dir_all(&scratch.0).expect("room");
    let room = Room::open(&scratch.0).expect("opens");
    let mut store = RefusingLand(
        weaver_state::engine::sqlite::Sqlite::stand().expect("stands"),
        || weaver_state::CustodyFault::LandingRefused("CHECK constraint failed".into()),
    );
    let mut custody = Custody {
        store: &mut store,
        room: &room,
        session: "s-1",
        restored: weaver_state::Restored::Empty,
        pending: None,
        save_point_bound: weaver_state::save_point::SAVE_POINT_BOUND,
        unlanded: None,
    };
    let (mut ours, _theirs) = std::os::unix::net::UnixStream::pair().expect("pair");
    let mut reader = LineReader::new(&mut ours);
    reader.buffer = concat!(
        r#"{"envelope":{"session":"s-1","run":"r-1","kind":"turn.started","sequence":"1"}}"#,
        "\n"
    )
    .as_bytes()
    .to_vec();
    let mut parking = ReplayParking::new(false);
    drain_harness_lines(&mut reader, &mut custody, &mut parking);
    assert!(
        custody.unlanded.is_none(),
        "the schema's refusal is no fault"
    );
    answer_frame(&Ask::Snapshot { ordinal: 1 }, &mut custody).expect("the snapshot still answers");
}

/// **A landing that does not land is never silent, and no save point is
/// taken over it** (the operator's ruling of 2026-10-08 on #99, N2): the
/// fault is held on the custody, and the next `snapshot` answers
/// nothing, so the harness misses the leg and no save point publishes
/// holdings that lack what the trace holds. The landing arrives through
/// the drain the harness's lines take. Perturbations: discard the
/// landing's result in the drain again, drop the custody's note, or the
/// snapshot's look at it, and the snapshot answers.
#[cfg(feature = "sqlite")]
#[test]
fn a_landing_that_fails_refuses_every_later_snapshot() {
    let scratch = Scratch(std::env::temp_dir().join(format!(
        "weaver-state-unlanded-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    )));
    std::fs::create_dir_all(&scratch.0).expect("room");
    let room = Room::open(&scratch.0).expect("opens");
    let mut store = RefusingLand(
        weaver_state::engine::sqlite::Sqlite::stand().expect("stands"),
        || weaver_state::CustodyFault::LandingFailed("database or disk is full".into()),
    );
    let mut custody = Custody {
        store: &mut store,
        room: &room,
        session: "s-1",
        restored: weaver_state::Restored::Empty,
        pending: None,
        save_point_bound: weaver_state::save_point::SAVE_POINT_BOUND,
        unlanded: None,
    };
    answer_frame(&Ask::Snapshot { ordinal: 1 }, &mut custody)
        .expect("before any failed landing the snapshot answers");
    // The landing arrives as the harness sends it, through the drain.
    let (mut ours, _theirs) = std::os::unix::net::UnixStream::pair().expect("pair");
    let mut reader = LineReader::new(&mut ours);
    reader.buffer = concat!(
        r#"{"envelope":{"session":"s-1","run":"r-1","kind":"turn.started","sequence":"1"}}"#,
        "\n"
    )
    .as_bytes()
    .to_vec();
    let mut parking = ReplayParking::new(false);
    drain_harness_lines(&mut reader, &mut custody, &mut parking);
    assert!(custody.unlanded.is_some(), "the failed landing is held");
    let refused = answer_frame(&Ask::Snapshot { ordinal: 2 }, &mut custody)
        .expect_err("no snapshot over a failed landing");
    assert!(
        format!("{refused:?}").contains("failed to land"),
        "{refused:?}"
    );
}

/// **A snapshot past the save point's bound answers nothing and leaves
/// no part**, per `weaver-state-Spec` section 3 on the operator's ruling
/// of 2026-10-08: with the custody's bound lowered under the file, the
/// `snapshot` ask is an `Err`, which the drain says on standard error and
/// answers with silence, no part is pending and the room holds no file,
/// the part the snapshot before it left cleared too; at the standing
/// bound the same holdings answer. Perturbation: drop the bound check
/// from `Room::write_part_within` and the lowered snapshot answers.
#[cfg(feature = "sqlite")]
#[test]
fn a_snapshot_past_the_bound_answers_nothing_and_leaves_no_part() {
    let scratch = Scratch(std::env::temp_dir().join(format!(
        "weaver-state-snapshot-bound-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    )));
    std::fs::create_dir_all(&scratch.0).expect("room");
    let room = Room::open(&scratch.0).expect("opens");
    let mut store = weaver_state::engine::sqlite::Sqlite::stand().expect("stands");
    let mut custody = Custody {
        store: &mut store,
        room: &room,
        session: "s-1",
        restored: weaver_state::Restored::Empty,
        pending: None,
        save_point_bound: weaver_state::save_point::SAVE_POINT_BOUND,
        unlanded: None,
    };
    let answered = answer_frame(&Ask::Snapshot { ordinal: 1 }, &mut custody)
        .expect("at the standing bound the snapshot answers");
    assert!(answered.contains("\"ask\":1"), "{answered}");
    assert!(custody.pending.is_some(), "a part is pending");
    custody.save_point_bound = 16;
    let refused = answer_frame(&Ask::Snapshot { ordinal: 2 }, &mut custody)
        .expect_err("past the bound the snapshot answers nothing");
    assert!(
        format!("{refused:?}").contains("past the bound of 16"),
        "{refused:?}"
    );
    assert!(custody.pending.is_none(), "no part is pending");
    let left: Vec<_> = std::fs::read_dir(&scratch.0)
        .expect("lists")
        .map(|entry| entry.expect("entry").file_name())
        .collect();
    assert!(left.is_empty(), "the room holds no file: {left:?}");
}

/// **A snapshot over a stamp the readers would refuse answers nothing
/// and leaves no part** (#99, K7): holdings whose last landing names a
/// run of 129 bytes stamp a save point no reader admits, so the
/// `snapshot` ask is an `Err`, answered with silence, no part is pending
/// and the room holds no file. Perturbation: drop the `stamp_bounds`
/// call from `Room::write_part_within` and the snapshot answers.
#[cfg(feature = "sqlite")]
#[test]
fn a_snapshot_over_a_run_past_the_bound_answers_nothing() {
    let scratch = Scratch(std::env::temp_dir().join(format!(
        "weaver-state-snapshot-run-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    )));
    std::fs::create_dir_all(&scratch.0).expect("room");
    let room = Room::open(&scratch.0).expect("opens");
    let mut store = weaver_state::engine::sqlite::Sqlite::stand().expect("stands");
    store
        .land(&weaver_state::Distillate {
            session: "s-1".into(),
            run: "r".repeat(129),
            turn: None,
            kind: "load".into(),
            sequence: 0,
            pairs: vec![],
        })
        .expect("lands");
    let mut custody = Custody {
        store: &mut store,
        room: &room,
        session: "s-1",
        restored: weaver_state::Restored::Empty,
        pending: None,
        save_point_bound: weaver_state::save_point::SAVE_POINT_BOUND,
        unlanded: None,
    };
    let refused = answer_frame(&Ask::Snapshot { ordinal: 1 }, &mut custody)
        .expect_err("a run past 128 bytes answers nothing");
    assert!(
        format!("{refused:?}").contains("128 printable"),
        "{refused:?}"
    );
    assert!(custody.pending.is_none(), "no part is pending");
    let left: Vec<_> = std::fs::read_dir(&scratch.0)
        .expect("lists")
        .map(|entry| entry.expect("entry").file_name())
        .collect();
    assert!(left.is_empty(), "the room holds no file: {left:?}");
}

/// The operator's 2026-09-22 ruling admits trace only as test scaffolding.
/// Read Cargo's dependency kinds, including aliases and target-qualified
/// declarations, so no normal or build edge can silently reintroduce it.
/// Perturbation: move the trace dependency back to production.
#[test]
fn trace_is_a_dev_dependency_only() {
    let metadata = std::process::Command::new(env!("CARGO"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args([
            "metadata",
            "--no-deps",
            "--format-version",
            "1",
            "--locked",
            "--offline",
        ])
        .output()
        .expect("cargo metadata runs");
    assert!(
        metadata.status.success(),
        "metadata failed: {}",
        String::from_utf8_lossy(&metadata.stderr)
    );
    let metadata: serde_json::Value = serde_json::from_slice(&metadata.stdout).unwrap();
    let package = metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|package| package["name"] == "weaver-state")
        .expect("state is in the manifest's workspace");
    let trace: Vec<_> = package["dependencies"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|dependency| dependency["name"] == "weaver-trace")
        .collect();
    assert!(
        !trace.is_empty(),
        "the test scaffolding must remain declared"
    );
    assert!(
        trace.iter().all(|dependency| dependency["kind"] == "dev"),
        "trace must be test-only across every declaration: {trace:?}"
    );
}

/// The contract's election round trip, section 8: the opener as the
/// tee renders it parses to the same election on this end, so a
/// restarted member rebuilds the identical index set.
#[test]
fn the_opener_round_trips_across_the_seam() {
    let sent = weaver_trace::Election {
        all_kinds: false,
        keys: vec![weaver_trace::ElectedKind {
            kind: "turn.closed".into(),
            paths: vec!["close".into(), "request.sampling".into()],
        }],
    };
    let opener = weaver_trace::opener("s-1", &sent);
    let received = parse_election(opener.trim_end()).expect("the opener parses");
    assert!(!received.all_kinds);
    assert_eq!(
        received.keys,
        vec![(
            "turn.closed".to_string(),
            vec!["close".to_string(), "request.sampling".to_string()]
        )]
    );
}

// Shared by the parser and both real-door watches. Valid entries surround
// invalid entries so silently filtering a partial election cannot pass.
pub(super) fn malformed_openers() -> Vec<String> {
    use serde_json::{Value, json};
    let wrap = |election| json!({"session":"target", "election":election}).to_string();
    let mut cases = vec![
        "not json".into(),
        "null".into(),
        "[]".into(),
        json!({"session":"target"}).to_string(),
    ];
    for election in [
        Value::Null,
        json!([]),
        json!(false),
        json!({}),
        json!({"keys":[]}),
        json!({"all_kinds":false}),
    ] {
        cases.push(wrap(election));
    }
    for bad in [Value::Null, json!("false"), json!(0), json!([]), json!({})] {
        cases.push(wrap(json!({"all_kinds":bad,"keys":[]})));
    }
    for bad in [Value::Null, json!(false), json!(0), json!(""), json!({})] {
        cases.push(wrap(json!({"all_kinds":false,"keys":bad})));
    }
    let good = json!({"kind":"load", "paths":["new.index"]});
    let mut bad_entries = vec![
        Value::Null,
        json!([]),
        json!(false),
        json!("kind"),
        json!({}),
        json!({"kind":"load"}),
        json!({"paths":[]}),
    ];
    for bad in [Value::Null, json!(false), json!(0), json!([]), json!({})] {
        bad_entries.push(json!({"kind":bad,"paths":[]}));
    }
    for bad in [Value::Null, json!(false), json!(0), json!(""), json!({})] {
        bad_entries.push(json!({"kind":"load","paths":bad}));
    }
    for bad in [Value::Null, json!(false), json!(0), json!([]), json!({})] {
        bad_entries.push(json!({"kind":"load","paths":["before",bad,"after"]}));
    }
    for bad in bad_entries {
        cases.push(wrap(json!({"all_kinds":false,"keys":[good,bad,good]})));
    }
    cases
}

#[test]
fn a_malformed_election_refuses_whole() {
    for opener in malformed_openers() {
        assert!(parse_election(&opener).is_none(), "accepted {opener}");
    }
}

#[test]
fn explicit_empty_keys_and_paths_remain_elections() {
    for all_kinds in [false, true] {
        for keys in [vec![], vec![("load".to_string(), vec![])]] {
            let wire_keys: Vec<_> = keys
                .iter()
                .map(|(kind, paths)| serde_json::json!({"kind":kind,"paths":paths}))
                .collect();
            let opener = serde_json::json!({"election":{
                "all_kinds":all_kinds,"keys":wire_keys,"future":null}})
            .to_string();
            assert_eq!(parse_election(&opener), Some(Election { all_kinds, keys }));
        }
    }
}

/// A distillate as the tee renders it parses whole on this end: the
/// envelope's five attributable, the elected pair carried verbatim.
#[test]
fn a_distillate_crosses_from_tee_to_row() {
    let line = concat!(
        r#"{"session":"alpha-1","run":"r-1","turn":"t-1","kind":"turn.closed","#,
        r#""sequence":"7","subsystem":"harness","wall_ms":1,"monotonic_ns":"2","#,
        r#""payload":{"close":"clean"}}"#
    );
    let election = weaver_trace::Election {
        all_kinds: true,
        keys: vec![weaver_trace::ElectedKind {
            kind: "turn.closed".into(),
            paths: vec!["close".into()],
        }],
    };
    let frame = weaver_trace::distill(line, &election).expect("distills");
    let distillate = parse_distillate(frame.trim_end()).expect("parses");
    assert_eq!(distillate.session, "alpha-1");
    assert_eq!(distillate.run, "r-1");
    assert_eq!(distillate.turn.as_deref(), Some("t-1"));
    assert_eq!(distillate.kind, "turn.closed");
    assert_eq!(distillate.sequence, 7);
    assert_eq!(
        distillate.pairs,
        vec![("close".to_string(), "\"clean\"".to_string())]
    );
}
