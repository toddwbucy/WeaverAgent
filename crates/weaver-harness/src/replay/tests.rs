//! The pass-behavior watches of `weaver-diagnostic-Spec` section 7,
//! bought by the loop act: the seat is granted against a scripted
//! custodian and a scripted decode peer, and the record on disk is what
//! each watch reads.

use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::fd::OwnedFd;
use std::os::unix::net::UnixStream;

use nix::sys::socket::{AddressFamily, MsgFlags, SockFlag, SockType, recv, send, socketpair};

use crate::authorship::Author;
use crate::engine::Ports;
use crate::record::Record;
use weaver_types::SessionId;

fn sink() -> (OwnedFd, crate::scratch::Scratch) {
    let path = crate::scratch::Scratch(std::env::temp_dir().join(format!(
        "weaver-replay-{}-{:?}.ndjson",
        std::process::id(),
        std::thread::current().id()
    )));
    let file = std::fs::File::create(&path).expect("sink");
    (OwnedFd::from(file), path)
}

fn diagnostic_record(sink: OwnedFd, session: &str) -> Record {
    Record::Diagnostic(
        weaver_diagnostic::Recorder::receive(
            sink,
            weaver_diagnostic::RunRef("r-d".into()),
            weaver_diagnostic::SessionRef(session.into()),
        )
        .expect("the recorder receives"),
    )
}

fn listener() -> (
    crate::channel::CoordinationListener,
    crate::scratch::Scratch,
) {
    let dir = crate::scratch::dir(format!(
        "weaver-replay-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let listener = crate::channel::bind_coordination(&dir.join("c.sock")).expect("bind");
    (listener, dir)
}

fn record_lines(path: &std::path::Path) -> Vec<serde_json::Value> {
    let mut text = String::new();
    std::fs::File::open(path)
        .expect("the record reopens")
        .read_to_string(&mut text)
        .expect("the record reads");
    text.lines()
        .map(|line| serde_json::from_str(line).expect("a record line parses"))
        .collect()
}

/// One valid recorded generation, as the custodian would answer it.
fn sealed_answer() -> String {
    concat!(
        r#"{"answer":{"replay":{"events":["#,
        r#"{"envelope":{"kind":"model.request","run":"r-1","turn":"t-1","sequence":"4"},"#,
        r#""pairs":{"rendered":"hi","template":"tmpl","sampling":{"seed":37}}},"#,
        r#"{"envelope":{"kind":"model.measurement","run":"r-1","turn":"t-1","sequence":"6"},"#,
        r#""pairs":{"input_tokens":[1,2],"output_tokens":[3],"model":"art","weights_hash":"h"}}"#,
        r#"]}}}"#,
        "\n"
    )
    .to_string()
}

/// A drive against scripted peers: the custodian's answer (or a closed
/// door), the decode peer's script, and the record read back.
fn run_drive(
    custodian: Option<String>,
    decode_script: fn(std::os::fd::OwnedFd),
) -> (Result<(), crate::engine::TurnError>, Vec<serde_json::Value>) {
    run_drive_as(custodian, decode_script, "s-d")
}

fn run_drive_as(
    custodian: Option<String>,
    decode_script: fn(std::os::fd::OwnedFd),
    destination: &str,
) -> (Result<(), crate::engine::TurnError>, Vec<serde_json::Value>) {
    let (sink_fd, path) = sink();
    let mut record = diagnostic_record(sink_fd, destination);
    let session = SessionId(destination.into());
    let author = Author::new(&session, &weaver_types::RunId("r-d".into()));

    let (ours, theirs) = UnixStream::pair().expect("state pair");
    ours.set_nonblocking(true).expect("nonblocking");
    let mut state = crate::state::StateSeam::new(ours);
    let state_peer = std::thread::spawn(move || {
        let mut peer = theirs;
        let mut taken = [0u8; 256];
        let Some(answer) = custodian else {
            let _ = peer.read(&mut taken);
            return;
        };
        let _ = peer.read(&mut taken).expect("reads the ask");
        peer.write_all(answer.as_bytes()).expect("answers");
        // Held open until the drive finishes with it.
        let _ = peer.read(&mut taken);
    });

    let (near, far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .expect("decode pair");
    let decode = crate::channel::decode_from_owned(near);
    let decode_peer = std::thread::spawn(move || decode_script(far));

    let (coordination, _coordination_dir) = listener();
    let mut turn_ordinal = 0u64;
    let mut turn_in_flight: Option<weaver_types::TurnKey> = None;
    let mut fullness = None;
    let mut pressure_reported = false;
    let outcome = {
        let load_facts = crate::engine::test_load_facts();
        let mut ports = Ports::grant(
            &decode,
            &author,
            &mut record,
            &mut turn_ordinal,
            &mut turn_in_flight,
            &load_facts,
            None,
            &coordination,
            None,
            None,
            Some(&mut state),
            None,
            &mut fullness,
            &mut pressure_reported,
        );
        super::drive(&mut ports, false, "art")
    };
    drop(decode);
    drop(state);
    decode_peer.join().expect("the decode peer finishes");
    state_peer.join().expect("the custodian finishes");
    let lines = record_lines(&path);
    std::fs::remove_file(&path).ok();
    (outcome, lines)
}

fn idle_decode(_far: std::os::fd::OwnedFd) {}

/// **The record identifies itself at the open, and an absent answer
/// invents nothing.** The custodian's door closes without answering,
/// and the record holds exactly the bracket: `replay.opened` first, no
/// `replay.identity` filled from defaults, and the close carrying the
/// abandoned outcome with the ask-unanswered reason.
///
/// Perturbation: drop the opened authoring from the drive and the
/// first-event assertion fails, the record opening with its close.
/// Watched under exactly that removal.
///
/// conforms: diagnostic-record-identifies-itself-at-the-open
/// conforms: diagnostic-identity-absent-not-invented
#[test]
fn an_absent_answer_abandons_and_the_record_opens_with_the_opened_kind() {
    let (outcome, lines) = run_drive(None, idle_decode);
    assert!(outcome.is_ok(), "an abandoned pass is not a fault");
    assert_eq!(
        lines[0]["kind"], "replay.opened",
        "the bracket opens with the opening kind"
    );
    assert!(
        lines.iter().all(|l| l["kind"] != "replay.identity"),
        "no identity is invented from an answer that never arrived"
    );
    let close = lines.last().expect("the close stands");
    assert_eq!(close["kind"], "replay.closed");
    assert_eq!(close["payload"]["outcome"]["kind"], "abandoned");
    assert_eq!(
        close["payload"]["outcome"]["reason"]["kind"],
        "replay_ask_unanswered"
    );
}

/// **Refused holdings author no identity either**, the case that bites:
/// a refused reading has everything an identity event would carry and
/// must still author nothing. The custodian answers a measurement with
/// no preceding request, the grouping refuses, and the close names it.
///
/// Perturbation: author a default-filled identity on the refusal path
/// and this fails, the refused pass carrying an identity it never
/// established. Watched under exactly that addition.
///
/// conforms: diagnostic-identity-absent-not-invented
#[test]
fn refused_holdings_author_no_identity() {
    let unpaired = concat!(
        r#"{"answer":{"replay":{"events":["#,
        r#"{"envelope":{"kind":"model.measurement","run":"r-1","turn":"t-1","sequence":"6"},"#,
        r#""pairs":{"input_tokens":[1,2],"output_tokens":[3],"model":"art","weights_hash":"h"}}"#,
        r#"]}}}"#,
        "\n"
    )
    .to_string();
    let (outcome, lines) = run_drive(Some(unpaired), idle_decode);
    assert!(outcome.is_ok());
    assert!(
        lines.iter().all(|l| l["kind"] != "replay.identity"),
        "a pass that refused its holdings authors none of these"
    );
    let close = lines.last().expect("the close stands");
    assert_eq!(close["payload"]["outcome"]["kind"], "abandoned");
    assert_eq!(
        close["payload"]["outcome"]["reason"]["kind"],
        "identity_refused"
    );
    assert!(
        close["payload"]["outcome"]["reason"]["detail"]
            .as_str()
            .is_some_and(|d| d.contains("t-1")),
        "the refusal names the turn: {close}"
    );
}

/// **A pass that died manufactures no outcome.** The decode peer takes
/// the re-feed directive and closes, the drive loses the channel, and
/// the record ends where the replay stopped: the bracket closed
/// stopped, and no `replay.closed` at all - the fourth outcome is the
/// absence of the event.
///
/// Perturbation: author a `replay.closed` on the channel-lost arm and
/// this fails, a death path carrying an outcome. Watched under exactly
/// that addition.
///
/// conforms: diagnostic-outcome-absent-not-manufactured
#[test]
fn a_dead_seam_mid_replay_manufactures_no_outcome() {
    fn takes_and_closes(far: std::os::fd::OwnedFd) {
        let mut buf = vec![0u8; 65536];
        let _ = recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).expect("takes the re-feed");
        drop(far);
    }
    let (outcome, lines) = run_drive(Some(sealed_answer()), takes_and_closes);
    assert!(
        matches!(outcome, Err(crate::engine::TurnError::ChannelLost)),
        "the loss surfaces to end service"
    );
    assert!(
        lines.iter().any(|l| l["kind"] == "replay.identity"),
        "the identity had been established before the death"
    );
    assert!(
        lines.iter().all(|l| l["kind"] != "replay.closed"),
        "a death path authors no outcome: the absence is the fourth outcome"
    );
    let close = lines.last().expect("the bracket still closed");
    assert_eq!(
        close["kind"], "turn.closed",
        "the turn bracket closed stopped"
    );
    assert_eq!(close["payload"]["close"], "stopped");
}

fn answers_refed(far: std::os::fd::OwnedFd) {
    let mut buf = vec![0u8; 65536];
    let n = recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).expect("takes the re-feed");
    let directive: serde_json::Value =
        serde_json::from_slice(&buf[..n]).expect("the directive parses");
    assert_eq!(
        directive["kind"], "re_feed",
        "the drive crossed: {directive}"
    );
    let refed = concat!(
        r#"{"kind":"re_fed","body":{"emission":"hi","finish":"completed","#,
        r#""content":[{"type":"text","text":"hi"}],"#,
        r#""request":{"rendered":"hi","template":"tmpl","sampling":{"seed":37}},"#,
        r#""measurement":{"input_tokens":[1,2],"output_tokens":[3],"#,
        r#""model":"art","weights_hash":"h"},"#,
        r#""resident":6,"capacity":64}}"#
    );
    send(far.as_raw_fd(), refed.as_bytes(), MsgFlags::empty()).expect("answers");
}

/// **A matching replay certifies, the bracket mirrored whole, under
/// the run's own session.** The decode peer answers the re-feed with
/// the recorded path recomputed, the pass closes certified, and the
/// identity payload names the replayed session by the one name the
/// contract gives it. The envelope's session needs no watch here: the
/// writer binds it by construction at submit, the crossing
/// unrepresentable, and that crate's own record carries the claim -
/// this test's per-line read is the loop confirming what construction
/// already holds.
///
/// Perturbation: fill `replayed_session` from anything but the
/// declared session's name in the drive and the identity assertion
/// fails. Watched under exactly that change.
#[test]
fn a_matching_replay_certifies_under_the_runs_own_session() {
    let (outcome, lines) = run_drive(Some(sealed_answer()), answers_refed);
    assert!(outcome.is_ok());
    let close = lines.last().expect("the close stands");
    assert_eq!(close["kind"], "replay.closed");
    assert_eq!(
        close["payload"]["outcome"]["kind"], "certified",
        "the matching path certifies: {close}"
    );
    for line in &lines {
        assert_eq!(
            line["session"], "s-d",
            "every envelope is the run's own session: {line}"
        );
    }
    let identity = lines
        .iter()
        .find(|l| l["kind"] == "replay.identity")
        .expect("the identity stands");
    assert_eq!(
        identity["payload"]["replayed_session"], "s-d",
        "and the identity names the replayed session"
    );
    let kinds: Vec<&str> = lines.iter().filter_map(|l| l["kind"].as_str()).collect();
    assert_eq!(
        kinds,
        vec![
            "replay.opened",
            "recall",
            "replay.identity",
            "turn.started",
            "model.request",
            "model.output",
            "model.measurement",
            "message.assistant",
            "turn.closed",
            "replay.closed",
        ],
        "the certified pass authors the mirrored bracket whole"
    );
    // The replay ask is recorded by the seat that made it, inside the
    // bracket and ahead of the identity the loop establishes from it,
    // named by the answer's bounds and count.
    let recall = lines
        .iter()
        .find(|l| l["kind"] == "recall")
        .expect("the replay ask is recorded");
    assert_eq!(recall["payload"]["ask"]["verb"], "replay");
    assert_eq!(
        recall["payload"]["count"], 2,
        "the events the member answered"
    );
    assert_eq!(
        recall["payload"]["returned"],
        serde_json::json!([
            {"run": "r-1", "turn": "t-1", "sequence": "4", "kind": "model.request"},
            {"run": "r-1", "turn": "t-1", "sequence": "6", "kind": "model.measurement"}
        ]),
        "named by its first and last events: {recall}"
    );
}

/// E1: the destination names the seat, holdings, and diagnostic identity;
/// the unchanged operator-held record identifies the source. Its digest
/// belongs to the analysis artifact, not a field invented by this loop.
/// Perturbation: replace `seat.session_name()` with the source name; the
/// identity assertions fail although the recomputed token path matches.
#[test]
fn a_diagnostic_destination_certifies_without_renaming_source_evidence() {
    const SOURCE: &str = concat!(
        r#"{"session":"s-source","run":"r-source","turn":"t-1","sequence":"4","kind":"model.request","payload":{"rendered":"hi","template":"tmpl","sampling":{"seed":37}}}"#,
        "\n",
        r#"{"session":"s-source","run":"r-source","turn":"t-1","sequence":"6","kind":"model.measurement","payload":{"input_tokens":[1,2],"output_tokens":[3],"model":"art","weights_hash":"h"}}"#,
        "\n",
    );
    let source: Vec<serde_json::Value> = SOURCE
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let source_before = source.clone();
    // Exercise two destinations so the seat's name cannot be replaced
    // by a fixed diagnostic spelling that happens to fit one fixture.
    for destination in ["s-diagnostic-one", "s-diagnostic-two"] {
        assert_ne!(destination, source[0]["session"].as_str().unwrap());
        let events: Vec<serde_json::Value> = source
            .iter()
            .map(|event| {
                serde_json::json!({
                    "envelope": {
                        "session": destination,
                        "run": event["run"], "turn": event["turn"],
                        "sequence": event["sequence"], "kind": event["kind"],
                    },
                    "pairs": event["payload"],
                })
            })
            .collect();
        for (held, original) in events.iter().zip(&source) {
            assert_eq!(held["envelope"]["session"], destination);
            for field in ["run", "turn", "sequence", "kind"] {
                assert_eq!(held["envelope"][field], original[field]);
            }
            assert_eq!(held["pairs"], original["payload"]);
        }
        let answer = format!(
            "{}\n",
            serde_json::json!({"answer":{"replay":{"events":events}}})
        );
        let (outcome, lines) = run_drive_as(Some(answer), answers_refed, destination);
        assert!(outcome.is_ok());
        let close = lines.last().expect("the pass closes");
        assert_eq!(close["kind"], "replay.closed");
        assert_eq!(close["payload"]["outcome"]["kind"], "certified");
        assert!(lines.iter().all(|line| line["session"] == destination));
        let identity = lines
            .iter()
            .find(|line| line["kind"] == "replay.identity")
            .expect("the input identity was established");
        assert_eq!(identity["payload"]["replayed_session"], destination);
        assert_eq!(identity["payload"]["model"], "art");
        assert_eq!(identity["payload"]["weights_hash"], "h");
        assert_eq!(identity["payload"]["template"], "tmpl");
        assert_eq!(
            source, source_before,
            "destination assignment never rewrites source evidence"
        );
    }
}

/// A drive whose scripted peer answers one re-feed with the given
/// measurement members and closing count, and the close it lands.
fn diverging_close(script: fn(std::os::fd::OwnedFd)) -> serde_json::Value {
    let (outcome, lines) = run_drive(Some(sealed_answer()), script);
    assert!(outcome.is_ok());
    let close = lines.last().expect("the close stands").clone();
    assert_eq!(close["kind"], "replay.closed");
    assert_eq!(
        close["payload"]["outcome"]["kind"], "diverged",
        "the differing path diverges: {close}"
    );
    close
}

fn answers(far: std::os::fd::OwnedFd, refed: &str) {
    let mut buf = vec![0u8; 65536];
    let _ = recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()).expect("takes the re-feed");
    send(far.as_raw_fd(), refed.as_bytes(), MsgFlags::empty()).expect("answers");
}

fn draw_differs(far: std::os::fd::OwnedFd) {
    answers(far, DRAW_DIFFERS);
}

fn input_differs(far: std::os::fd::OwnedFd) {
    answers(far, INPUT_DIFFERS);
}

/// The sealed answer's path, [1, 2] appended and [3] drawn, recomputed
/// as 4 at the one draw and closing at resident 6, the terminator
/// included: the draw sits at 6 - 1 - 1 = 4.
const DRAW_DIFFERS: &str = concat!(
    r#"{"kind":"re_fed","body":{"emission":"yo","finish":"completed","#,
    r#""content":[{"type":"text","text":"yo"}],"#,
    r#""request":{"rendered":"hi","template":"tmpl","sampling":{"seed":37}},"#,
    r#""measurement":{"input_tokens":[1,2],"output_tokens":[4],"#,
    r#""model":"art","weights_hash":"h"},"#,
    r#""resident":6,"capacity":64}}"#
);

/// The same path with the appended input re-tokenized as [1, 9]: the
/// second appended token sits at 4 - 2 + 1 = 3.
const INPUT_DIFFERS: &str = concat!(
    r#"{"kind":"re_fed","body":{"emission":"hi","finish":"completed","#,
    r#""content":[{"type":"text","text":"hi"}],"#,
    r#""request":{"rendered":"hi","template":"tmpl","sampling":{"seed":37}},"#,
    r#""measurement":{"input_tokens":[1,9],"output_tokens":[3],"#,
    r#""model":"art","weights_hash":"h"},"#,
    r#""resident":6,"capacity":64}}"#
);

/// A scripted SPU that holds a resident count, as the real one does: a
/// flush returns it to `keep`, an elision removes its span, and a re-feed
/// appends the input, the draws and the terminator. **It draws the
/// recorded path only where the re-feed starts at the resident the
/// source's generation started at**, and draws 9999 otherwise, which is
/// the device's behaviour the M1 run showed: the same rendered delta on a
/// different resident drew a different token.
fn resident_spu(far: std::os::fd::OwnedFd, mut resident: u64, starts: &[u64]) {
    let mut buf = vec![0u8; 65536];
    let mut refeeds = 0;
    while let Ok(n) = recv(far.as_raw_fd(), &mut buf, MsgFlags::empty()) {
        if n == 0 {
            break;
        }
        let directive: serde_json::Value =
            serde_json::from_slice(&buf[..n]).expect("the directive parses");
        let answer = match directive["kind"].as_str() {
            Some("flush") => {
                let before = resident;
                resident = directive["keep"].as_u64().expect("a keep");
                serde_json::to_string(&weaver_types::TokenAnswer::Flushed {
                    resident_before: before,
                    resident_after: resident,
                })
                .expect("renders")
            }
            Some("elide") => {
                let before = resident;
                let span = directive["to"].as_u64().unwrap() - directive["from"].as_u64().unwrap();
                resident -= span;
                serde_json::to_string(&weaver_types::TokenAnswer::Elided {
                    resident_before: before,
                    resident_after: resident,
                })
                .expect("renders")
            }
            Some("re_feed") => {
                let drawn = if starts.get(refeeds) == Some(&resident) {
                    3
                } else {
                    9999
                };
                refeeds += 1;
                resident += 2 + 1 + 1;
                format!(
                    concat!(
                        r#"{{"kind":"re_fed","body":{{"emission":"hi","finish":"completed","#,
                        r#""content":[{{"type":"text","text":"hi"}}],"#,
                        r#""request":{{"rendered":"hi","template":"tmpl","sampling":{{"seed":37}}}},"#,
                        r#""measurement":{{"input_tokens":[1,2],"output_tokens":[{}],"#,
                        r#""model":"art","weights_hash":"h"}},"#,
                        r#""resident":{},"capacity":32768}}}}"#
                    ),
                    drawn, resident
                )
            }
            other => panic!("an unscripted directive: {other:?}"),
        };
        if send(far.as_raw_fd(), answer.as_bytes(), MsgFlags::empty()).is_err() {
            break;
        }
    }
}

/// One recorded generation of the sealed answer's shape, in `turn`, at
/// the sequences given, for building a longer holdings list.
fn generation(turn: &str, request: u64, measurement: u64) -> String {
    format!(
        concat!(
            r#"{{"envelope":{{"kind":"model.request","run":"r-1","turn":"{t}","sequence":"{a}"}},"#,
            r#""pairs":{{"rendered":"hi","template":"tmpl","sampling":{{"seed":37}}}}}},"#,
            r#"{{"envelope":{{"kind":"model.measurement","run":"r-1","turn":"{t}","sequence":"{b}"}},"#,
            r#""pairs":{{"input_tokens":[1,2],"output_tokens":[3],"model":"art","weights_hash":"h"}}}}"#
        ),
        t = turn,
        a = request,
        b = measurement
    )
}

fn holdings(events: &[String]) -> String {
    format!(
        "{}{}{}\n",
        r#"{"answer":{"replay":{"events":["#,
        events.join(","),
        r#"]}}}"#
    )
}

fn flush_event(sequence: u64, before: u64, after: u64) -> String {
    format!(
        r#"{{"envelope":{{"kind":"flush","run":"r-1","sequence":"{sequence}"}},"pairs":{{"resident_before":{before},"resident_after":{after}}}}}"#
    )
}

/// The M1 run's shape around its flush: turn 36 closed the resident at
/// 26551, the flush at 255 took it to 38, and turn 37 re-entered there.
fn m1_holdings(flush_before: u64) -> String {
    holdings(&[
        generation("t-36", 250, 252),
        flush_event(255, flush_before, 38),
        generation("t-37", 259, 261),
    ])
}

/// Turn 36 starts at 26547, so its re-feed of two input tokens, one draw
/// and the terminator closes at 26551, and turn 37 starts at 38.
fn m1_spu(far: std::os::fd::OwnedFd) {
    resident_spu(far, 26547, &[26547, 38]);
}

/// **The replay certifies past the flush, `m1-002`'s shape**, per
/// `diagnostic-replay-loop` section 2 as of 2026-09-26 (#673 item 1):
/// the recorded flush is reproduced through the seat between the two
/// turns with its counts, so turn 37 re-feeds onto the 38 tokens the
/// source's did and draws the recorded path, and the destination
/// records the flush where it fell.
///
/// Perturbation: drop the `"flush" | "elision"` arm from `group`, or the
/// `reproduce` call before each turn, and turn 37 re-feeds onto the
/// 26551 tokens of every earlier turn, draws 9999 and the pass diverges.
/// Watched under both.
#[test]
fn the_replay_certifies_past_the_flush() {
    let (outcome, lines) = run_drive(Some(m1_holdings(26551)), m1_spu);
    assert!(outcome.is_ok());
    let close = lines.last().expect("the close stands");
    assert_eq!(
        close["payload"]["outcome"]["kind"], "certified",
        "the post-flush turn certifies: {close}"
    );
    let kinds: Vec<&str> = lines.iter().filter_map(|l| l["kind"].as_str()).collect();
    let flush = kinds
        .iter()
        .position(|k| *k == "flush")
        .expect("the flush is recorded");
    let turns: Vec<usize> = kinds
        .iter()
        .enumerate()
        .filter(|(_, k)| **k == "turn.started")
        .map(|(i, _)| i)
        .collect();
    assert!(
        turns[0] < flush && flush < turns[1],
        "between the two turns, where the source's fell: {kinds:?}"
    );
    assert_eq!(lines[flush]["payload"]["resident_before"], 26551);
    assert_eq!(lines[flush]["payload"]["resident_after"], 38);
}

/// **A reproduction whose counts disagree with the record closes the pass
/// abandoned at identity, naming both**: the replay would otherwise go on
/// feeding a resident the source never held.
///
/// Perturbation: drop the comparison in `reproduce` and the pass walks on
/// and certifies over a flush that did not match.
#[test]
fn a_flush_that_disagrees_with_the_record_abandons_at_identity() {
    let (outcome, lines) = run_drive(Some(m1_holdings(26550)), m1_spu);
    assert!(outcome.is_ok());
    let close = lines.last().expect("the close stands");
    assert_eq!(close["payload"]["outcome"]["kind"], "abandoned");
    let detail = close["payload"]["outcome"]["reason"]["detail"]
        .as_str()
        .expect("a detail");
    assert!(
        detail.contains("26551") && detail.contains("26550"),
        "names both counts: {detail}"
    );
}

/// **An elision is reproduced the same way**, the source's other resident
/// edit: its span removed through the seat and its counts held.
///
/// Perturbation: drop the elision's branch in `group` and the later turn
/// re-feeds onto the unelided resident and diverges. Drop `elision` from
/// the diagnostic mapping and the event goes unrecorded while the pass
/// certifies, which the recorded-event assertion catches.
#[test]
fn the_replay_certifies_past_an_elision() {
    fn spu(far: std::os::fd::OwnedFd) {
        resident_spu(far, 100, &[100, 84]);
    }
    let elision = r#"{"envelope":{"kind":"elision","run":"r-1","sequence":"7"},"pairs":{"from":10,"to":30,"resident_before":104,"resident_after":84}}"#;
    let answer = holdings(&[
        generation("t-1", 4, 6),
        elision.to_string(),
        generation("t-2", 9, 11),
    ]);
    let (outcome, lines) = run_drive(Some(answer), spu);
    assert!(outcome.is_ok());
    let close = lines.last().expect("the close stands");
    assert_eq!(close["payload"]["outcome"]["kind"], "certified", "{close}");
    // The replay's own record holds the elision it performed, between the
    // two turns: a record that could not carry it would drop an act its
    // own loop performs, the flush's reason.
    let kinds: Vec<&str> = lines.iter().filter_map(|l| l["kind"].as_str()).collect();
    let elision = kinds
        .iter()
        .position(|k| *k == "elision")
        .unwrap_or_else(|| panic!("the elision is recorded: {kinds:?}"));
    let turns: Vec<usize> = kinds
        .iter()
        .enumerate()
        .filter(|(_, k)| **k == "turn.started")
        .map(|(i, _)| i)
        .collect();
    assert!(turns[0] < elision && elision < turns[1], "{kinds:?}");
    assert_eq!(lines[elision]["payload"]["from"], 10);
    assert_eq!(lines[elision]["payload"]["resident_after"], 84);
}

/// **An edit the record cannot place, or cannot count, refuses the
/// grouping before any forward pass**: a flush while a request stands
/// unpaired, and a flush holding no counts, as a source elected without
/// them would.
#[test]
fn an_edit_without_its_place_or_its_counts_refuses_the_grouping() {
    let inside = holdings(&[
        r#"{"envelope":{"kind":"model.request","run":"r-1","turn":"t-1","sequence":"4"},"pairs":{"rendered":"hi","template":"tmpl","sampling":{"seed":37}}}"#.to_string(),
        flush_event(5, 6, 3),
        r#"{"envelope":{"kind":"model.measurement","run":"r-1","turn":"t-1","sequence":"6"},"pairs":{"input_tokens":[1,2],"output_tokens":[3],"model":"art","weights_hash":"h"}}"#.to_string(),
    ]);
    let bare = holdings(&[
        generation("t-1", 4, 6),
        r#"{"envelope":{"kind":"flush","run":"r-1","sequence":"7"},"pairs":{}}"#.to_string(),
        generation("t-2", 9, 11),
    ]);
    for (answer, expected) in [
        (inside, "lands inside turn t-1"),
        (bare, "holds no resident_before"),
    ] {
        let (outcome, lines) = run_drive(Some(answer), idle_decode);
        assert!(outcome.is_ok());
        let close = lines.last().expect("the close stands");
        assert_eq!(close["payload"]["outcome"]["kind"], "abandoned");
        assert!(
            close["payload"]["outcome"]["reason"]["detail"]
                .as_str()
                .is_some_and(|d| d.contains(expected)),
            "{expected}: {close}"
        );
    }
}

/// conforms: diagnostic-identity-refuses-an-unaccounted-input
///
/// **A source whose open recorded `identity_prefix_unrecorded` does not
/// certify, and an unrelated fault does not stop one** (#690 item C2.8).
///
/// Perturbation: drop the fault arm from `group` and the first holdings
/// certify; match any fault case and the second abandons.
#[test]
fn an_unrecorded_opening_refuses_certification_and_another_fault_does_not() {
    let fault = |case: &str| {
        format!(
            r#"{{"envelope":{{"kind":"fault","run":"r-1","sequence":"2"}},"pairs":{{"case":"{case}"}}}}"#
        )
    };
    let unrecorded = holdings(&[fault("identity_prefix_unrecorded"), generation("t-1", 4, 6)]);
    let (outcome, lines) = run_drive(Some(unrecorded), idle_decode);
    assert!(outcome.is_ok());
    let close = lines.last().expect("the close stands");
    assert_eq!(close["payload"]["outcome"]["kind"], "abandoned", "{close}");
    assert!(
        close["payload"]["outcome"]["reason"]["detail"]
            .as_str()
            .is_some_and(|d| d.contains("identity_prefix_unrecorded")),
        "{close}"
    );
    let unrelated = holdings(&[fault("recorder_commit_pressure"), generation("t-1", 4, 6)]);
    let (outcome, lines) = run_drive(Some(unrelated), answers_refed);
    assert!(outcome.is_ok());
    assert_eq!(
        lines.last().expect("the close stands")["payload"]["outcome"]["kind"],
        "certified"
    );
}

fn recall_event(sequence: u64, verb: &str) -> String {
    format!(
        concat!(
            r#"{{"envelope":{{"kind":"recall","run":"r-1","sequence":"{s}"}},"#,
            r#""pairs":{{"ask":{{"verb":"{v}"}},"returned":[],"count":0}}}}"#
        ),
        s = sequence,
        v = verb
    )
}

/// `m1-002`'s shape on a record new enough to carry the recall kind: the
/// enter's identity recall at the open, and between the flush and turn
/// 37's request, whatever `between` holds.
fn m1_recalled_holdings(between: &[String]) -> String {
    let mut events = vec![
        recall_event(1, "identity"),
        generation("t-36", 250, 252),
        flush_event(255, 26551, 38),
    ];
    events.extend_from_slice(between);
    events.push(generation("t-37", 259, 261));
    holdings(&events)
}

/// conforms: diagnostic-identity-refuses-an-unaccounted-input
///
/// **A post-flush request with no recall before it does not certify, on
/// a record that carries the recall kind** (#690 item C2.10), per
/// `diagnostic-replay-loop` section 2: the seat's recall is what the
/// re-entry was drawn from, and a record new enough to carry one and
/// holding none there cannot say what the post-flush input was. The
/// seat's recall certifies, and a recall of another ask does not stand
/// in for it. A record carrying no recall at all is read as older than
/// the kind and certifies as before, which
/// `the_replay_certifies_past_the_flush` holds.
///
/// Perturbation: drop the `carries_recall` refusal after the loop in
/// `group` and the first holdings certify; clear `flushed` on any recall
/// and the third certifies.
#[test]
fn a_post_flush_request_without_a_recall_refuses_certification() {
    let (outcome, lines) = run_drive(Some(m1_recalled_holdings(&[])), m1_spu);
    assert!(outcome.is_ok());
    let close = lines.last().expect("the close stands");
    assert_eq!(close["payload"]["outcome"]["kind"], "abandoned", "{close}");
    assert!(
        close["payload"]["outcome"]["reason"]["detail"]
            .as_str()
            .is_some_and(|d| d.contains("flush at sequence 255") && d.contains("t-37")),
        "names the flush and the turn: {close}"
    );

    let (outcome, lines) = run_drive(
        Some(m1_recalled_holdings(&[recall_event(257, "recall")])),
        m1_spu,
    );
    assert!(outcome.is_ok());
    let close = lines.last().expect("the close stands");
    assert_eq!(close["payload"]["outcome"]["kind"], "certified", "{close}");

    let (outcome, lines) = run_drive(
        Some(m1_recalled_holdings(&[recall_event(257, "identity")])),
        m1_spu,
    );
    assert!(outcome.is_ok());
    let close = lines.last().expect("the close stands");
    assert_eq!(close["payload"]["outcome"]["kind"], "abandoned", "{close}");
}

/// conforms: diagnostic-identity-refuses-an-unaccounted-input
///
/// **Each flush needs its own recall**: the seat's recall after the
/// first flush accounts for turn 37's re-entry and not for turn 38's,
/// which follows a second flush with none, so the pass abandons naming
/// the second flush and its turn.
///
/// Perturbation: stop tracking flushes once a seat recall has been seen
/// and the second flush passes unaccounted, the pass reaching the walk.
#[test]
fn each_flush_needs_its_own_recall() {
    let events = vec![
        recall_event(1, "identity"),
        generation("t-36", 250, 252),
        flush_event(255, 26551, 38),
        recall_event(257, "recall"),
        generation("t-37", 259, 261),
        flush_event(262, 42, 20),
        generation("t-38", 264, 266),
    ];
    let (outcome, lines) = run_drive(Some(holdings(&events)), m1_spu);
    assert!(outcome.is_ok());
    let close = lines.last().expect("the close stands");
    assert_eq!(close["payload"]["outcome"]["kind"], "abandoned", "{close}");
    assert!(
        close["payload"]["outcome"]["reason"]["detail"]
            .as_str()
            .is_some_and(|d| d.contains("flush at sequence 262") && d.contains("t-38")),
        "names the second flush and its turn: {close}"
    );
}

/// conforms: diagnostic-divergence-position-is-the-resident-length
#[test]
fn a_draw_divergence_names_the_resident_length_at_the_draw() {
    let close = diverging_close(draw_differs);
    let divergence = &close["payload"]["outcome"]["divergence"];
    assert_eq!(divergence["kind"], "token_path");
    // Perturbation: index the turn's identifiers instead and this reads
    // 2, the pre-ruling coordinate, one input's length below the key
    // the field row carries.
    assert_eq!(
        divergence["position"], 4,
        "the draw's resident length and not its index: {close}"
    );
    assert_eq!(divergence["recorded"], 3);
    assert_eq!(divergence["recomputed"], 4);
}

/// conforms: diagnostic-divergence-position-is-the-resident-length
#[test]
fn an_input_divergence_names_the_tokens_resident_length() {
    let close = diverging_close(input_differs);
    let divergence = &close["payload"]["outcome"]["divergence"];
    assert_eq!(divergence["kind"], "token_path");
    // Perturbation: index the appended input from zero and this reads 1.
    assert_eq!(
        divergence["position"], 3,
        "the appended token's resident length and not its index: {close}"
    );
    assert_eq!(divergence["recorded"], 2);
    assert_eq!(divergence["recomputed"], 9);
}
