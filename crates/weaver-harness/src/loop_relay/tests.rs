//! Test-only: the loop relay's suite (#118), against `weaver-harness-loop-contract`.
//! The far end of a `SOCK_SEQPACKET` pair plays the loop, frame by frame, in a scoped
//! thread, and a stand-in seat records every call the relay makes.

use super::*;
use crate::state::RunShape;
use nix::sys::socket::{AddressFamily, Shutdown, SockFlag, SockType, shutdown, socketpair};
use std::cell::RefCell;
use std::rc::Rc;
use std::thread;
use weaver_types::{TokenRefusal, TurnKey};

const DIGEST: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const PREFIX: &str = "weaver-harness: the loop relay broke: ";
const HELLO_PREFIX: &str = "weaver-harness: the loop relay refused the loop's hello: ";

// ---- The reasons, read back.

#[derive(Clone, Default)]
struct Lines(Rc<RefCell<Vec<u8>>>);

impl Write for Lines {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.borrow_mut().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Lines {
    fn text(&self) -> String {
        String::from_utf8(self.0.borrow().clone()).expect("ASCII reasons")
    }
}

// ---- The loop's end.

struct Far {
    end: OwnedFd,
}

impl Far {
    fn send(&self, text: &str) {
        self.send_bytes(text.as_bytes());
    }
    fn send_bytes(&self, bytes: &[u8]) {
        let slices = [IoSlice::new(bytes)];
        sendmsg::<()>(
            self.end.as_raw_fd(),
            &slices,
            &[],
            MsgFlags::MSG_NOSIGNAL,
            None,
        )
        .expect("the loop's send");
    }
    /// One raw message, or None after `ms` or at end of file.
    fn raw(&self, ms: u16) -> Option<Vec<u8>> {
        let mut fds = [PollFd::new(self.end.as_fd(), PollFlags::POLLIN)];
        if poll(&mut fds, ms).ok()? == 0 {
            return None;
        }
        let mut buffer = vec![0u8; 2 * MAX_ENVELOPE_BYTES];
        let mut slices = [IoSliceMut::new(&mut buffer)];
        let read = recvmsg::<()>(
            self.end.as_raw_fd(),
            &mut slices,
            None,
            MsgFlags::MSG_DONTWAIT,
        )
        .ok()?
        .bytes;
        if read == 0 {
            return None;
        }
        buffer.truncate(read);
        Some(buffer)
    }
    /// One frame, its series reassembled, with the count of messages it took.
    fn frame_counted(&self) -> Option<(Value, usize)> {
        let first = self.raw(3000)?;
        let value: Value = serde_json::from_slice(&first).ok()?;
        if value.get("kind").is_some() {
            return Some((value, 1));
        }
        let segments = value["segments"].as_u64()? as usize;
        let mut whole = Vec::new();
        for _ in 0..segments {
            whole.extend_from_slice(&self.raw(3000)?);
        }
        Some((serde_json::from_slice(&whole).ok()?, 1 + segments))
    }
    fn frame(&self) -> Option<Value> {
        self.frame_counted().map(|(value, _)| value)
    }
    fn ask(&self, pass: u64, tag: u64, name: &str, extra: &str) {
        self.send(&format!(
            r#"{{"kind":"ask","pass":{pass},"tag":{tag},"ask":"{name}"{extra}}}"#
        ));
    }
    fn turn(&self, pass: u64, tag: u64, text: &str) {
        let delta = json!([{"role": "user", "content": [{"type": "text", "text": text}]}]);
        self.send(&format!(
            r#"{{"kind":"ask","pass":{pass},"tag":{tag},"ask":"turn","delta":{delta}}}"#
        ));
    }
    fn refuse(&self, pass: u64) {
        self.send(&format!(r#"{{"kind":"refuse","pass":{pass}}}"#));
    }
    /// After `ms` the loop's side shuts its end, so a relay that waits past its
    /// bound ends with another reason instead of waiting for good.
    fn close_after(&self, ms: u64) {
        thread::sleep(Duration::from_millis(ms));
        let _ = shutdown(self.end.as_raw_fd(), Shutdown::Both);
    }
}

fn hello_text() -> String {
    format!(r#"{{"kind":"hello","version":"weaver-loop/1","wires_sha256":"{DIGEST}"}}"#)
}

fn quick() -> Bounds {
    Bounds {
        hello: Duration::from_millis(400),
        pass: Duration::from_millis(400),
        asks: LOOP_PASS_ASKS,
    }
}

fn pair() -> (OwnedFd, Far) {
    let (near, far) = socketpair(
        AddressFamily::Unix,
        SockType::SeqPacket,
        None,
        SockFlag::SOCK_CLOEXEC,
    )
    .expect("pair");
    (near, Far { end: far })
}

/// A relay that has read a good `hello`, its far end, and its reasons.
fn made(bounds: Bounds) -> (LoopRelay<Lines>, Far, Lines) {
    let (near, far) = pair();
    far.send(&hello_text());
    let lines = Lines::default();
    let relay = LoopRelay::make_bounded(near, DIGEST, lines.clone(), bounds).expect("made");
    (relay, far, lines)
}

/// One pass, the loop's side scripted in a scoped thread.
fn drive<T: Send>(
    relay: &mut LoopRelay<Lines>,
    far: &Far,
    seat: &mut Stand,
    text: &str,
    script: impl FnOnce(&Far) -> T + Send,
) -> (Result<TurnOutcome, TurnError>, T) {
    thread::scope(|scope| {
        let loop_side = scope.spawn(|| script(far));
        let result = relay.pass(seat, text);
        (result, loop_side.join().expect("the loop's side"))
    })
}

fn lost(result: &Result<TurnOutcome, TurnError>) -> bool {
    matches!(result, Err(TurnError::ChannelLost))
}

// ---- The stand-in seat.

#[derive(Debug, Clone, PartialEq)]
enum Call {
    Flush(u64),
    Recall(Option<u64>),
    Shape,
    Turn(Vec<(String, Vec<String>)>),
}

#[derive(Clone, Copy)]
enum Answer {
    Done,
    Refused,
    Faulted,
    Unlicensed,
    Lost,
}

/// The seat's reads take `&self`, so they are told apart by their answers;
/// its asks record their arguments.
struct Stand {
    calls: Vec<Call>,
    seated: bool,
    independent: bool,
    first_turn: bool,
    fullness: Option<(u64, u64)>,
    flush: Option<(u64, u64)>,
    recall: Option<Vec<Recalled>>,
    shape: Option<SessionShape>,
    answer: Answer,
    /// Time the seat takes in `seated`, which is the harness's and not the loop's.
    slow: Duration,
}

fn stand() -> Stand {
    Stand {
        calls: Vec::new(),
        seated: true,
        independent: true,
        first_turn: false,
        fullness: Some((7, 100)),
        flush: Some((50, 3)),
        recall: Some(vec![Recalled {
            kind: "message.user".to_string(),
            run: "r-1".to_string(),
            turn: Some("t-2".to_string()),
            sequence: "9".to_string(),
            pairs: vec![(
                "content".to_string(),
                r#"[{"type":"text","text":"hi"}]"#.to_string(),
            )],
        }]),
        shape: Some(SessionShape {
            runs: vec![RunShape {
                run: "r-1".to_string(),
                kinds: vec![("turn.closed".to_string(), 3), ("load".to_string(), 1)],
            }],
        }),
        answer: Answer::Done,
        slow: Duration::ZERO,
    }
}

fn key() -> TurnKey {
    TurnKey("t-1".to_string())
}

impl Seat for Stand {
    fn first_turn(&self) -> bool {
        self.first_turn
    }
    fn seated(&self) -> bool {
        thread::sleep(self.slow);
        self.seated
    }
    fn turn_context(&self) -> TurnContext {
        if self.independent {
            TurnContext::Independent
        } else {
            TurnContext::Continuous
        }
    }
    fn fullness(&self) -> Option<(u64, u64)> {
        self.fullness
    }
    fn flush(&mut self, keep: u64) -> Option<(u64, u64)> {
        self.calls.push(Call::Flush(keep));
        self.flush
    }
    fn recall(&mut self, last_turns: Option<u64>) -> Option<Vec<Recalled>> {
        self.calls.push(Call::Recall(last_turns));
        self.recall.clone()
    }
    fn session_shape(&mut self) -> Option<SessionShape> {
        self.calls.push(Call::Shape);
        self.shape.clone()
    }
    fn turn(&mut self, delta: Vec<Message>) -> Result<TurnOutcome, TurnError> {
        self.calls.push(Call::Turn(
            delta
                .iter()
                .map(|message| {
                    let role = match message.role {
                        Role::System => "system",
                        Role::User => "user",
                        _ => "other",
                    };
                    let texts = message
                        .content
                        .iter()
                        .map(|block| match block {
                            ContentBlock::Text { text } => text.clone(),
                            _ => "(other)".to_string(),
                        })
                        .collect();
                    (role.to_string(), texts)
                })
                .collect(),
        ));
        match self.answer {
            Answer::Done => Ok(TurnOutcome {
                turn: key(),
                emission: "the answer".to_string(),
                stopped: false,
                aborted: false,
                truncated: false,
                unrecovered: Vec::new(),
            }),
            Answer::Refused => Err(TurnError::Refused {
                turn: key(),
                refusal: TokenRefusal::NotOpen,
            }),
            Answer::Faulted => Err(TurnError::Faulted {
                turn: key(),
                report: serde_json::from_str(r#"{"case":"listener_lost","account":{}}"#)
                    .expect("a report"),
            }),
            Answer::Unlicensed => Err(TurnError::Unlicensed { turn: key() }),
            Answer::Lost => Err(TurnError::ChannelLost),
        }
    }
}

// ---- Making the relay: the hello (the contract's section 3.1).

#[test]
fn the_hello_is_read_when_the_relay_is_made() {
    let (_relay, _far, lines) = made(quick());
    assert_eq!(lines.text(), "", "a good hello names nothing");
}

#[test]
fn no_hello_within_its_bound_refuses_the_making() {
    let (near, _far) = pair();
    let lines = Lines::default();
    let started = Instant::now();
    let refused = LoopRelay::make_bounded(near, DIGEST, lines.clone(), quick()).err();
    assert_eq!(
        refused,
        Some(HelloRefused {
            reason: "the bound expired in a read".to_string()
        })
    );
    assert!(started.elapsed() >= Duration::from_millis(400));
    assert_eq!(
        lines.text(),
        format!("{HELLO_PREFIX}the bound expired in a read\n")
    );
}

#[test]
fn a_loop_that_closes_before_its_hello_refuses_the_making() {
    let (near, far) = pair();
    drop(far);
    let refused = LoopRelay::make_bounded(near, DIGEST, Lines::default(), quick()).err();
    assert_eq!(
        refused.map(|r| r.reason).as_deref(),
        Some("the loop's end of the pair closed")
    );
}

// ---- The frames file, read by the relay and the board client alike.

fn case_bytes(case: &Value) -> Vec<u8> {
    match case["text"].as_str() {
        Some(text) => text.as_bytes().to_vec(),
        None => {
            let hex = case["bytes_hex"].as_str().expect("text or bytes_hex");
            (0..hex.len() / 2)
                .map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).expect("hex"))
                .collect()
        }
    }
}

/// What the relay makes of one case: "valid", or its reason.
fn verdict_of(case: &Value) -> String {
    let bytes = case_bytes(case);
    if case["phase"] == "hello" {
        let (near, far) = pair();
        far.send_bytes(&bytes);
        let lines = Lines::default();
        return match LoopRelay::make_bounded(near, DIGEST, lines.clone(), quick()) {
            Ok(_) if lines.text().is_empty() => "valid".to_string(),
            Ok(_) => format!("made, but said {}", lines.text()),
            Err(refused) if lines.text() == format!("{HELLO_PREFIX}{}\n", refused.reason) => {
                refused.reason
            }
            Err(refused) => format!("refused {refused:?}, but said {}", lines.text()),
        };
    }
    let (mut relay, far, lines) = made(quick());
    let mut seat = stand();
    let goes_on = case["verdict"] == "valid" && case["ends"] != json!(true);
    let (result, _) = drive(&mut relay, &far, &mut seat, "x", |far| {
        far.frame().expect("the pass");
        far.send_bytes(&bytes);
        if goes_on && far.frame().is_some() {
            far.refuse(1);
        }
    });
    let said = lines.text();
    if !said.is_empty() {
        if !lost(&result) {
            return format!("named {said} but answered {result:?}");
        }
        return said
            .strip_prefix(PREFIX)
            .and_then(|t| t.strip_suffix('\n'))
            .unwrap_or(&said)
            .to_string();
    }
    let (again, _) = drive(&mut relay, &far, &mut seat, "y", |far| {
        far.frame().expect("pass 2");
        far.refuse(2);
    });
    if lost(&again) && lines.text().is_empty() {
        "valid".to_string()
    } else {
        format!("valid, but the next pass gave {again:?}")
    }
}

#[test]
fn every_frame_of_the_frames_file_meets_its_verdict() {
    let file: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/loop-frames.json"
    )))
    .expect("the frames file");
    let cases = file["cases"].as_array().expect("cases");
    assert!(cases.len() >= 80, "the file holds every case");
    let wrong: Vec<String> = cases
        .iter()
        .filter_map(|case| {
            let said = verdict_of(case);
            let verdict = case["verdict"].as_str().expect("a verdict");
            (said != verdict)
                .then(|| format!("{}: expected {verdict:?}, got {said:?}", case["name"]))
        })
        .collect();
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

// ---- Each listed ask reaches its seat call, with its arguments (invariant 5).

const RECALLED: &str = r#"[{"kind": "message.user", "run": "r-1", "turn": "t-2", "sequence": "9", "pairs": [["content", "[{\"type\":\"text\",\"text\":\"hi\"}]"]]}]"#;

#[test]
fn each_listed_ask_reaches_its_seat_call_and_returns_its_value() {
    let (mut relay, far, lines) = made(quick());
    let mut seat = stand();
    let script = [
        ("seated", ""),
        ("turn_context", ""),
        ("first_turn", ""),
        ("fullness", ""),
        ("flush", r#","keep":1"#),
        ("recall", r#","last_turns":null"#),
        ("recall", r#","last_turns":4"#),
        ("shape", ""),
    ];
    let (result, (opened, values)) = drive(&mut relay, &far, &mut seat, "the turn's text", |far| {
        let opened = far.frame().expect("the pass");
        let mut values = Vec::new();
        for (tag, (name, extra)) in script.iter().enumerate() {
            far.ask(1, tag as u64 + 10, name, extra);
            values.push(far.frame().expect("a return"));
        }
        far.turn(1, 99, "the request");
        (opened, values)
    });
    assert_eq!(
        opened,
        json!({"kind": "pass", "pass": 1, "text": "the turn's text"})
    );
    let recalled: Value = serde_json::from_str(RECALLED).unwrap();
    let expected = [
        json!(true),
        json!("independent"),
        json!(false),
        json!({"resident": 7, "capacity": 100}),
        json!({"resident_before": 50, "resident_after": 3}),
        recalled.clone(),
        recalled,
        json!({"runs": [{"run": "r-1", "kinds": [["turn.closed", 3], ["load", 1]]}]}),
    ];
    for (tag, (value, want)) in values.iter().zip(expected.iter()).enumerate() {
        assert_eq!(
            value,
            &json!({"kind": "return", "pass": 1, "tag": tag as u64 + 10, "value": want})
        );
    }
    assert_eq!(
        seat.calls,
        vec![
            Call::Flush(1),
            Call::Recall(None),
            Call::Recall(Some(4)),
            Call::Shape,
            Call::Turn(vec![("user".to_string(), vec!["the request".to_string()])]),
        ],
        "the seat's asks, with the arguments the asks carried"
    );
    assert!(matches!(&result, Ok(outcome) if outcome.emission == "the answer"));
    assert_eq!(lines.text(), "");
}

#[test]
fn a_seat_that_answers_nothing_returns_null() {
    let (mut relay, far, _lines) = made(quick());
    let mut seat = stand();
    seat.fullness = None;
    seat.flush = None;
    seat.recall = None;
    seat.shape = None;
    seat.seated = false;
    seat.independent = false;
    seat.first_turn = true;
    let (_, values) = drive(&mut relay, &far, &mut seat, "x", |far| {
        far.frame().expect("the pass");
        let mut values = Vec::new();
        for (tag, (name, extra)) in [
            ("seated", ""),
            ("turn_context", ""),
            ("first_turn", ""),
            ("fullness", ""),
            ("flush", r#","keep":0"#),
            ("recall", r#","last_turns":4"#),
            ("shape", ""),
        ]
        .iter()
        .enumerate()
        {
            far.ask(1, tag as u64, name, extra);
            values.push(far.frame().expect("a return")["value"].clone());
        }
        far.refuse(1);
        values
    });
    assert_eq!(
        values,
        vec![
            json!(false),
            json!("continuous"),
            json!(true),
            Value::Null,
            Value::Null,
            Value::Null,
            Value::Null
        ]
    );
}

// ---- The entry's result (invariant 8).

#[test]
fn the_turns_own_result_reaches_the_harness_unchanged() {
    for answer in [
        Answer::Done,
        Answer::Refused,
        Answer::Faulted,
        Answer::Unlicensed,
        Answer::Lost,
    ] {
        let (mut relay, far, lines) = made(quick());
        let mut seat = stand();
        seat.answer = answer;
        let (result, _) = drive(&mut relay, &far, &mut seat, "x", |far| {
            far.frame().expect("the pass");
            far.turn(1, 0, "go");
        });
        let same = match (answer, &result) {
            (Answer::Done, Ok(_)) => true,
            (Answer::Refused, Err(TurnError::Refused { turn, refusal })) => {
                *turn == key() && *refusal == TokenRefusal::NotOpen
            }
            (Answer::Faulted, Err(TurnError::Faulted { turn, .. })) => *turn == key(),
            (Answer::Unlicensed, Err(TurnError::Unlicensed { turn })) => *turn == key(),
            (Answer::Lost, Err(TurnError::ChannelLost)) => true,
            _ => false,
        };
        assert!(
            same,
            "the seat's answer reaches the harness unchanged: {result:?}"
        );
        assert_eq!(lines.text(), "", "a turn's own error is no break");
        // Row 8 leaves the relay Idle whatever the turn answered.
        seat.answer = Answer::Done;
        let (again, opened) = drive(&mut relay, &far, &mut seat, "y", |far| {
            let opened = far.frame().expect("pass 2");
            far.turn(2, 0, "again");
            opened
        });
        assert_eq!(opened["pass"], json!(2));
        assert!(again.is_ok());
    }
}

// ---- A refusal, and the terminal Broken (the contract's section 4, invariant 7).

#[test]
fn a_refusal_answers_channel_lost_and_leaves_the_relay_idle() {
    let (mut relay, far, lines) = made(quick());
    let mut seat = stand();
    let (result, _) = drive(&mut relay, &far, &mut seat, "x", |far| {
        far.frame().expect("the pass");
        far.ask(1, 0, "seated", "");
        far.frame().expect("the return");
        far.refuse(1);
    });
    assert!(lost(&result));
    assert_eq!(lines.text(), "", "a refusal names no break");
    let (again, opened) = drive(&mut relay, &far, &mut seat, "y", |far| {
        let opened = far.frame().expect("pass 2 opens");
        far.turn(2, 0, "after the refusal");
        opened
    });
    assert_eq!(opened, json!({"kind": "pass", "pass": 2, "text": "y"}));
    assert!(again.is_ok(), "the relay is Idle after a refusal");
}

#[test]
fn a_broken_relay_answers_at_once_and_sends_nothing() {
    let (mut relay, far, lines) = made(quick());
    let mut seat = stand();
    let (result, _) = drive(&mut relay, &far, &mut seat, "x", |far| {
        far.frame().expect("the pass");
        far.send("not json");
    });
    assert!(lost(&result));
    let started = Instant::now();
    let again = relay.pass(&mut seat, "y");
    assert!(lost(&again));
    assert!(started.elapsed() < Duration::from_millis(100), "at once");
    assert!(far.raw(200).is_none(), "nothing is sent once broken");
    assert_eq!(
        lines.text(),
        format!("{PREFIX}a frame that does not parse as JSON\n"),
        "the break is named once"
    );
}

// ---- Passes and tags (the contract's section 3, invariants 1 and 3).

#[test]
fn passes_count_up_from_one_and_tags_may_repeat_across_passes() {
    let (mut relay, far, _lines) = made(quick());
    let mut seat = stand();
    for pass in 1..=3u64 {
        let (result, opened) = drive(&mut relay, &far, &mut seat, "x", |far| {
            let opened = far.frame().expect("the pass");
            far.ask(pass, 0, "seated", "");
            far.frame().expect("the return");
            far.turn(pass, 1, "go");
            opened
        });
        assert_eq!(opened["pass"], json!(pass));
        assert!(result.is_ok());
    }
}

#[test]
fn a_tag_used_twice_in_one_pass_is_a_break() {
    let (mut relay, far, lines) = made(quick());
    let mut seat = stand();
    let (result, _) = drive(&mut relay, &far, &mut seat, "x", |far| {
        far.frame().expect("the pass");
        far.ask(1, 5, "seated", "");
        far.frame().expect("the return");
        far.ask(1, 5, "seated", "");
    });
    assert!(lost(&result));
    assert_eq!(
        lines.text(),
        format!("{PREFIX}the tag 5 used twice in one pass\n")
    );
}

#[test]
fn sixty_four_asks_end_a_pass_and_a_sixty_fifth_is_a_break() {
    let (mut relay, far, lines) = made(quick());
    let mut seat = stand();
    let (result, _) = drive(&mut relay, &far, &mut seat, "x", |far| {
        far.frame().expect("the pass");
        for tag in 0..63 {
            far.ask(1, tag, "seated", "");
            far.frame().expect("a return");
        }
        far.turn(1, 63, "the sixty-fourth ask");
    });
    assert!(result.is_ok(), "sixty-four asks, the turn among them");
    assert_eq!(lines.text(), "");
    let (result, _) = drive(&mut relay, &far, &mut seat, "x", |far| {
        far.frame().expect("the pass");
        for tag in 0..64 {
            far.ask(2, tag, "seated", "");
            far.frame().expect("a return");
        }
        far.ask(2, 64, "seated", "");
    });
    assert!(lost(&result));
    assert_eq!(
        lines.text(),
        format!("{PREFIX}an ask past the 64 one pass may make\n")
    );
}

// ---- The look before a pass (the contract's section 3.2, ruling 4).

#[test]
fn a_frame_standing_when_a_pass_opens_is_a_break() {
    let (mut relay, far, lines) = made(quick());
    let mut seat = stand();
    let (result, _) = drive(&mut relay, &far, &mut seat, "x", |far| {
        far.frame().expect("the pass");
        far.turn(1, 0, "go");
        // Sent while the relay is Idle: it names pass 1, already ended.
        far.refuse(1);
    });
    assert!(result.is_ok());
    let again = relay.pass(&mut seat, "y");
    assert!(lost(&again));
    assert_eq!(
        lines.text(),
        format!("{PREFIX}a frame or an end of file standing when a pass opens\n")
    );
    // The refuse is still there, and no pass came after it.
    assert_eq!(far.raw(200), None, "no pass is sent over a standing frame");
}

#[test]
fn an_end_of_file_standing_when_a_pass_opens_is_a_break() {
    let (mut relay, far, lines) = made(quick());
    let mut seat = stand();
    drop(far);
    assert!(lost(&relay.pass(&mut seat, "x")));
    assert_eq!(
        lines.text(),
        format!("{PREFIX}a frame or an end of file standing when a pass opens\n")
    );
}

// ---- The bound (the contract's section 5, ruling 2).

#[test]
fn silence_past_the_bound_is_a_break_in_a_read() {
    let (mut relay, far, lines) = made(quick());
    let mut seat = stand();
    let started = Instant::now();
    let (result, _) = drive(&mut relay, &far, &mut seat, "x", |far| {
        far.frame().expect("the pass");
        far.close_after(1500);
    });
    assert!(lost(&result));
    assert_eq!(
        lines.text(),
        format!("{PREFIX}the bound expired in a read\n")
    );
    assert!(started.elapsed() >= Duration::from_millis(400));
}

#[test]
fn the_bound_is_summed_across_the_waits_of_one_pass() {
    // Each wait is 150 ms, under the 400 ms bound, and three sum past it.
    let (mut relay, far, lines) = made(quick());
    let mut seat = stand();
    let (result, _) = drive(&mut relay, &far, &mut seat, "x", |far| {
        far.frame().expect("the pass");
        for tag in 0..4 {
            thread::sleep(Duration::from_millis(150));
            far.ask(1, tag, "seated", "");
            if far.frame().is_none() {
                return;
            }
        }
        far.turn(1, 9, "go");
    });
    assert!(lost(&result), "the waits sum past the bound: {result:?}");
    assert_eq!(
        lines.text(),
        format!("{PREFIX}the bound expired in a read\n")
    );
}

#[test]
fn the_seats_own_time_is_not_counted_against_the_bound() {
    let (mut relay, far, lines) = made(quick());
    let mut seat = stand();
    seat.slow = Duration::from_millis(600);
    let (result, _) = drive(&mut relay, &far, &mut seat, "x", |far| {
        far.frame().expect("the pass");
        far.ask(1, 0, "seated", "");
        far.frame().expect("the return");
        far.turn(1, 1, "go");
    });
    assert!(
        result.is_ok(),
        "600 ms in the seat against a 400 ms bound: {result:?}"
    );
    assert_eq!(lines.text(), "");
}

#[test]
fn a_loop_that_stops_reading_meets_the_bound_in_a_send() {
    let (mut relay, far, lines) = made(quick());
    let mut seat = stand();
    // A recall's answer of 2 MiB crosses as a series, past what the pair buffers.
    seat.recall = Some(vec![Recalled {
        kind: "message.user".to_string(),
        run: "r-1".to_string(),
        turn: None,
        sequence: "1".to_string(),
        pairs: vec![("content".to_string(), "x".repeat(2 * 1024 * 1024))],
    }]);
    let (result, _) = drive(&mut relay, &far, &mut seat, "x", |far| {
        far.frame().expect("the pass");
        far.ask(1, 0, "recall", r#","last_turns":4"#);
        far.close_after(1500);
    });
    assert!(lost(&result));
    assert_eq!(
        lines.text(),
        format!("{PREFIX}the bound expired in a send\n")
    );
}

// ---- A loop that goes away (the contract's section 4).

#[test]
fn an_end_of_file_inside_a_pass_is_a_break() {
    let (mut relay, far, lines) = made(quick());
    let mut seat = stand();
    let (result, _) = drive(&mut relay, &far, &mut seat, "x", |far| {
        far.frame().expect("the pass");
        let _ = shutdown(far.end.as_raw_fd(), Shutdown::Write);
    });
    assert!(lost(&result));
    assert_eq!(
        lines.text(),
        format!("{PREFIX}the loop's end of the pair closed\n")
    );
}

#[test]
fn a_send_to_a_loop_that_has_gone_is_a_break() {
    let (mut relay, far, lines) = made(quick());
    let mut seat = stand();
    let (result, _) = drive(&mut relay, &far, &mut seat, "x", |far| {
        far.frame().expect("the pass");
        far.ask(1, 0, "seated", "");
        let _ = shutdown(far.end.as_raw_fd(), Shutdown::Read);
    });
    assert!(lost(&result));
    assert_eq!(lines.text(), format!("{PREFIX}a send to the loop failed\n"));
}

#[test]
fn a_message_past_the_envelope_is_a_break() {
    let (mut relay, far, lines) = made(quick());
    let mut seat = stand();
    let (result, _) = drive(&mut relay, &far, &mut seat, "x", |far| {
        far.frame().expect("the pass");
        far.send(&"x".repeat(MAX_ENVELOPE_BYTES + 1));
    });
    assert!(lost(&result));
    assert_eq!(
        lines.text(),
        format!("{PREFIX}a message past the envelope's 64 KiB\n")
    );
}

// ---- The segment series, both ways (the contract's section 2.4).

#[test]
fn a_recall_past_the_envelope_crosses_to_the_loop_as_a_series() {
    let (mut relay, far, _lines) = made(quick());
    let mut seat = stand();
    let content = "y".repeat(200_000);
    seat.recall = Some(vec![Recalled {
        kind: "message.assistant".to_string(),
        run: "r-1".to_string(),
        turn: None,
        sequence: "1".to_string(),
        pairs: vec![("content".to_string(), content.clone())],
    }]);
    let (result, (value, messages)) = drive(&mut relay, &far, &mut seat, "x", |far| {
        far.frame().expect("the pass");
        far.ask(1, 0, "recall", r#","last_turns":4"#);
        let seen = far.frame_counted().expect("the series");
        far.refuse(1);
        seen
    });
    assert!(lost(&result));
    assert!(messages > 2, "a preamble and slices: {messages} messages");
    assert_eq!(value["value"][0]["pairs"][0][1], json!(content));
}

#[test]
fn a_turn_past_the_envelope_crosses_from_the_loop_as_a_series() {
    let (mut relay, far, lines) = made(quick());
    let mut seat = stand();
    let text = "z".repeat(150_000);
    let (result, _) = drive(&mut relay, &far, &mut seat, "x", |far| {
        far.frame().expect("the pass");
        let delta = json!([{"role": "user", "content": [{"type": "text", "text": text}]}]);
        let body = format!(r#"{{"kind":"ask","pass":1,"tag":0,"ask":"turn","delta":{delta}}}"#)
            .into_bytes();
        send_segmented(&body, |slice| {
            far.send_bytes(slice);
            Ok(())
        })
        .expect("the series");
    });
    assert!(result.is_ok(), "{result:?} {}", lines.text());
    assert_eq!(
        seat.calls,
        vec![Call::Turn(vec![("user".to_string(), vec![text.clone()])])]
    );
}

#[test]
fn a_series_that_breaks_its_rules_is_a_break() {
    let body = vec![b'x'; 100_000];
    let series =
        "a frame with no kind that is not a segment preamble, or a series that breaks its rules";
    for (name, preamble, slices, want) in [
        (
            "a count that is not the length's",
            r#"{"segments":3,"bytes":100000}"#,
            2,
            series,
        ),
        (
            "a length under the envelope",
            r#"{"segments":1,"bytes":100}"#,
            0,
            series,
        ),
        (
            "a slice that never comes",
            r#"{"segments":2,"bytes":100000}"#,
            1,
            "the bound expired in a read",
        ),
    ] {
        let (mut relay, far, lines) = made(quick());
        let mut seat = stand();
        let (result, _) = drive(&mut relay, &far, &mut seat, "x", |far| {
            far.frame().expect("the pass");
            far.send(preamble);
            for slice in body.chunks(MAX_ENVELOPE_BYTES).take(slices) {
                far.send_bytes(slice);
            }
            far.close_after(1500);
        });
        assert!(lost(&result), "{name}");
        assert_eq!(lines.text(), format!("{PREFIX}{want}\n"), "{name}");
    }
}

#[test]
fn a_series_whose_frame_has_no_kind_is_a_break() {
    let (mut relay, far, lines) = made(quick());
    let mut seat = stand();
    let (result, _) = drive(&mut relay, &far, &mut seat, "x", |far| {
        far.frame().expect("the pass");
        let body = format!(r#"{{"pass":1,"pad":"{}"}}"#, "p".repeat(70_000)).into_bytes();
        send_segmented(&body, |slice| {
            far.send_bytes(slice);
            Ok(())
        })
        .expect("the series");
    });
    assert!(lost(&result));
    assert_eq!(lines.text(), format!("{PREFIX}a frame with no kind\n"));
}
