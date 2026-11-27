//! Today's loop as wires (`weaver-harness-loop-contract` section 8, the brief of
//! 2026-11-25 item 9): the compiled [`compose`] and the wire file
//! `tests/fixtures/loop-wires-today.py`, run by `loop-runner/runner.py` under
//! `/usr/bin/python3 -I -B -S` behind the loop relay, each against a recording seat given
//! the same answers. The seat records every read and every ask with its arguments, and
//! the delta's roles and bytes, so the two records compare whole. The inputs reach each
//! fixed sentence and each branch of section 8. The comparison of whole trace records is
//! act 2's.

use super::super::compose;
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, BorrowedFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::rc::Rc;
use weaver_harness::{
    LoopRelay, Recalled, RunShape, Seat, SessionShape, TurnError, TurnOutcome, place_child_ends,
};
use weaver_traits::{ContentBlock, Message, Role};
use weaver_types::{TurnContext, TurnKey};

const WIRES: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/loop-wires-today.py"
);
const RUNNER: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../loop-runner/runner.py");
const PYTHON: &str = "/usr/bin/python3";

/// The relay's reason lines, read back.
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

/// The runner as deployed, on the far end of a pair placed at descriptor 3, and the
/// relay made on the near end, which has read the wires' `hello`.
fn wired(wires: &str) -> (LoopRelay<Lines>, Lines, Child) {
    let (near, far) = nix::sys::socket::socketpair(
        nix::sys::socket::AddressFamily::Unix,
        nix::sys::socket::SockType::SeqPacket,
        None,
        nix::sys::socket::SockFlag::SOCK_CLOEXEC,
    )
    .expect("the pair");
    let digest = format!(
        "{:x}",
        Sha256::digest(std::fs::read(wires).expect("the wires"))
    );
    let raw = far.as_raw_fd();
    let mut command = Command::new(PYTHON);
    command
        .args(["-I", "-B", "-S", RUNNER, wires])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    // SAFETY: place_child_ends runs between fork and exec, on an end this process made.
    unsafe {
        command.pre_exec(move || {
            place_child_ends(&[BorrowedFd::borrow_raw(raw)]).map_err(std::io::Error::from)
        });
    }
    // No skip: a seat with no interpreter fails here, by the brief's word.
    let child = command
        .spawn()
        .expect("the interpreter at /usr/bin/python3");
    drop::<OwnedFd>(far);
    let lines = Lines::default();
    let relay = LoopRelay::make(near, &digest, lines.clone()).expect("the wires' hello");
    (relay, lines, child)
}

/// The relay closed, the runner's exit and what it said. Standard error is read to its
/// end before the wait: a runner that fills the pipe would otherwise never exit.
fn ended(relay: LoopRelay<Lines>, mut child: Child) -> (bool, String) {
    drop(relay);
    let mut said = String::new();
    child
        .stderr
        .take()
        .expect("piped")
        .read_to_string(&mut said)
        .expect("the runner's standard error");
    let status = child.wait().expect("the runner's exit");
    (status.success(), said)
}

/// The runner's one line at the harness's end of file, as `runner.py` writes it.
fn end_line(passes: usize, turns: usize, refused: usize) -> String {
    format!(
        "{{\"loop\": \"end\", \"passes\": {passes}, \"turns\": {turns}, \"refused\": {refused}}}\n"
    )
}

#[derive(Debug, Clone, PartialEq)]
enum Asked {
    Seated,
    TurnContext,
    FirstTurn,
    Fullness,
    Flush(u64),
    Recall(Option<u64>),
    Shape,
    /// Each message's role and its blocks' texts, as a list: one block never compares
    /// equal to two.
    Turn(Vec<(&'static str, Vec<String>)>),
}

#[derive(Clone)]
struct Case {
    name: &'static str,
    seated: bool,
    independent: bool,
    first_turn: bool,
    fullness: Option<(u64, u64)>,
    flush: Option<(u64, u64)>,
    recall: Option<Vec<Recalled>>,
    shape: Option<SessionShape>,
    text: String,
}

fn case(name: &'static str) -> Case {
    Case {
        name,
        seated: false,
        independent: false,
        first_turn: false,
        fullness: None,
        flush: Some((40, 3)),
        recall: Some(Vec::new()),
        shape: None,
        text: "the request".to_string(),
    }
}

struct Recording {
    case: Case,
    asked: RefCell<Vec<Asked>>,
}

impl Recording {
    fn new(case: &Case) -> Recording {
        Recording {
            case: case.clone(),
            asked: RefCell::new(Vec::new()),
        }
    }
    fn asked(&self) -> Vec<Asked> {
        self.asked.borrow().clone()
    }
}

impl Seat for Recording {
    fn first_turn(&self) -> bool {
        self.asked.borrow_mut().push(Asked::FirstTurn);
        self.case.first_turn
    }
    fn seated(&self) -> bool {
        self.asked.borrow_mut().push(Asked::Seated);
        self.case.seated
    }
    fn turn_context(&self) -> TurnContext {
        self.asked.borrow_mut().push(Asked::TurnContext);
        if self.case.independent {
            TurnContext::Independent
        } else {
            TurnContext::Continuous
        }
    }
    fn fullness(&self) -> Option<(u64, u64)> {
        self.asked.borrow_mut().push(Asked::Fullness);
        self.case.fullness
    }
    fn flush(&mut self, keep: u64) -> Option<(u64, u64)> {
        self.asked.borrow_mut().push(Asked::Flush(keep));
        self.case.flush
    }
    fn recall(&mut self, last_turns: Option<u64>) -> Option<Vec<Recalled>> {
        self.asked.borrow_mut().push(Asked::Recall(last_turns));
        self.case.recall.clone()
    }
    fn session_shape(&mut self) -> Option<SessionShape> {
        self.asked.borrow_mut().push(Asked::Shape);
        self.case.shape.clone()
    }
    fn turn(&mut self, delta: Vec<Message>) -> Result<TurnOutcome, TurnError> {
        let messages = delta
            .iter()
            .map(|message| {
                let role = match message.role {
                    Role::System => "system",
                    Role::User => "user",
                    _ => "another role",
                };
                let texts: Vec<String> = message
                    .content
                    .iter()
                    .map(|block| match block {
                        ContentBlock::Text { text } => text.clone(),
                        _ => "(another block)".to_string(),
                    })
                    .collect();
                (role, texts)
            })
            .collect();
        self.asked.borrow_mut().push(Asked::Turn(messages));
        Ok(TurnOutcome {
            turn: TurnKey("t-1".to_string()),
            emission: "the answer".to_string(),
            stopped: false,
            aborted: false,
            truncated: false,
            unrecovered: Vec::new(),
        })
    }
}

fn answered(result: &Result<TurnOutcome, TurnError>) -> String {
    match result {
        Ok(outcome) => format!("turned: {}", outcome.emission),
        Err(TurnError::ChannelLost) => "ChannelLost".to_string(),
        Err(other) => format!("{other:?}"),
    }
}

// ---- The inputs.

fn event(kind: &str, pairs: &[(&str, &str)]) -> Recalled {
    Recalled {
        kind: kind.to_string(),
        run: "r-1".to_string(),
        turn: Some("t-0".to_string()),
        sequence: "7".to_string(),
        pairs: pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
    }
}

/// A `content` value as the trace writes it: a list of text blocks.
fn blocks(texts: &[&str]) -> String {
    serde_json::to_string(
        &texts
            .iter()
            .map(|text| serde_json::json!({"type": "text", "text": text}))
            .collect::<Vec<_>>(),
    )
    .unwrap()
}

fn said(kind: &str, texts: &[&str]) -> Recalled {
    event(kind, &[("content", &blocks(texts))])
}

fn shape(runs: &[&[(&str, u64)]]) -> SessionShape {
    SessionShape {
        runs: runs
            .iter()
            .enumerate()
            .map(|(n, kinds)| RunShape {
                run: format!("r-{n}"),
                kinds: kinds.iter().map(|(k, c)| (k.to_string(), *c)).collect(),
            })
            .collect(),
    }
}

/// The White_Space characters the first split case leaves out, each between two words.
const MORE_WHITE_SPACE: [char; 15] = [
    '\u{1680}', '\u{2000}', '\u{2001}', '\u{2002}', '\u{2003}', '\u{2004}', '\u{2005}', '\u{2006}',
    '\u{2007}', '\u{2008}', '\u{2009}', '\u{200a}', '\u{2029}', '\u{202f}', '\u{205f}',
];

fn rest_of_white_space() -> String {
    let mut text = String::from("w0");
    for (n, c) in MORE_WHITE_SPACE.iter().chain(['\u{0c}'].iter()).enumerate() {
        text.push(*c);
        text.push_str(&format!("w{}", n + 1));
    }
    text
}

/// A block whose nesting is `depth` levels in all: the list, the block, and the pads.
fn nested(depth: usize, text: &str) -> String {
    let pads = depth - 2;
    format!(
        r#"[{{"type":"text","text":"{text}","pad":{}{}}}]"#,
        "[".repeat(pads),
        "]".repeat(pads)
    )
}

fn cases() -> Vec<Case> {
    let pressure = Some((80, 100));
    let mut all = vec![
        Case {
            independent: true,
            ..case("independent, nothing seated")
        },
        Case {
            independent: true,
            seated: true,
            ..case("independent, a prompt seated")
        },
        Case {
            independent: true,
            flush: None,
            ..case("independent, the flush answered null")
        },
        Case {
            first_turn: true,
            ..case("a first turn, nothing seated, no shape")
        },
        Case {
            first_turn: true,
            shape: Some(shape(&[])),
            ..case("a first turn, a shape of no run")
        },
        Case {
            first_turn: true,
            shape: Some(shape(&[&[("turn.closed", 3)]])),
            ..case("a first turn, a shape of one run")
        },
        Case {
            first_turn: true,
            shape: Some(shape(&[&[("load", 1), ("turn.closed", 1)], &[("load", 1)]])),
            ..case("a first turn, one earlier run of one turn")
        },
        Case {
            first_turn: true,
            shape: Some(shape(&[
                &[("load", 1), ("turn.closed", 2), ("turn.closed", 9)],
                &[("load", 1)],
                &[("turn.closed", 5)],
            ])),
            ..case("a first turn, two earlier runs, the first count read")
        },
        Case {
            first_turn: true,
            shape: Some(shape(&[&[("load", 1)], &[]])),
            ..case("a first turn, an earlier run of no turn")
        },
        Case {
            first_turn: true,
            seated: true,
            shape: Some(shape(&[&[("turn.closed", 1)], &[]])),
            ..case("a first turn, a prompt seated, no shape asked")
        },
        case("a later turn, no fullness"),
        Case {
            fullness: Some((79, 100)),
            ..case("a later turn, one short of pressure")
        },
        Case {
            fullness: Some((0, 0)),
            ..case("a later turn, a capacity of 0")
        },
        Case {
            fullness: Some((5, 0)),
            ..case("a later turn, resident against a capacity of 0")
        },
        Case {
            fullness: pressure,
            recall: None,
            ..case("pressure, nothing seated, the record unreachable")
        },
        Case {
            fullness: pressure,
            seated: true,
            recall: None,
            ..case("pressure, a prompt seated, the record unreachable")
        },
        Case {
            fullness: Some((1 << 62, 1 << 63)),
            seated: true,
            ..case("pressure by saturating products")
        },
        Case {
            fullness: Some((100, 100)),
            flush: None,
            recall: Some(vec![said("message.user", &["hello"])]),
            ..case("pressure, the flush answered null, the recall asked")
        },
        Case {
            fullness: pressure,
            recall: Some(vec![
                said("message.system", &["skipped"]),
                said("tool.call", &["skipped"]),
                event("message.user", &[("other", &blocks(&["no content pair"]))]),
                event(
                    "message.user",
                    &[
                        ("role", "user"),
                        ("content", &blocks(&["the first content", "in two blocks"])),
                        ("content", &blocks(&["the second content"])),
                    ],
                ),
                said("message.assistant", &["an answer"]),
            ]),
            ..case("the kinds quoted and skipped")
        },
        Case {
            fullness: pressure,
            seated: true,
            recall: Some(
                [
                    "not json".to_string(),
                    r#"[{"type":"text","text":"nan","n":NaN}]"#.to_string(),
                    r#"[{"type":"text","text":"inf","n":Infinity}]"#.to_string(),
                    r#"[{"type":"text","text":"-inf","n":-Infinity}]"#.to_string(),
                    r#"[{"type":"text","text":"\ud800"}]"#.to_string(),
                    r#"[{"type":"text","text":"\udc00 trailing"}]"#.to_string(),
                    r#"[{"type":"text","text":"big","n":1e400}]"#.to_string(),
                    format!(
                        r#"[{{"type":"text","text":"bigint","n":1{}}}]"#,
                        "0".repeat(400)
                    ),
                    r#"[{"type":"text","text":"under u64","n":18446744073709551616}]"#.to_string(),
                    nested(127, "deep127"),
                    nested(128, "deep128"),
                    r#"{"type":"text","text":"not a list"}"#.to_string(),
                    r#"["a string",5,{"type":"text","text":7},{"type":"text","text":"kept"}]"#
                        .to_string(),
                    r#"[{"type":"text","text":"first","text":"second"}]"#.to_string(),
                    // A member a later one of the same name replaces is still read: serde_json
                    // refuses its lone surrogate, and its nesting, as it reads.
                    r#"[{"type":"text","text":"\ud800","text":"twice surrogate"}]"#.to_string(),
                    // The list, the block, and 126 levels in the first n: 128 in all.
                    format!(
                        r#"[{{"type":"text","text":"twice deep","n":{}{},"n":1}}]"#,
                        "[".repeat(126),
                        "]".repeat(126)
                    ),
                    r#"[{"type":"text","text":"a pair \ud83d\ude00"}]"#.to_string(),
                ]
                .iter()
                .map(|content| event("message.user", &[("content", content)]))
                .collect(),
            ),
            ..case("contents serde_json refuses, and the ones it takes")
        },
        Case {
            fullness: pressure,
            seated: true,
            recall: Some(vec![said(
                "message.user",
                &[
                    "a\u{1c}b c\u{a0}d\u{3000}e\u{2028}f\u{180e}g\u{200b}h\u{85}i\u{1f}j\tk\r\nl\u{0b}m",
                ],
            )]),
            ..case("words split on White_Space alone")
        },
        Case {
            fullness: pressure,
            seated: true,
            recall: Some(vec![said("message.user", &[&rest_of_white_space()])]),
            ..case("the rest of White_Space splits a word")
        },
        Case {
            fullness: pressure,
            seated: true,
            recall: Some(
                [
                    r#"[{"type":"text","text":"max157","n":1.7976931348623157e308}]"#,
                    r#"[{"type":"text","text":"max158","n":1.7976931348623158e308}]"#,
                    r#"[{"type":"text","text":"past159","n":1.7976931348623159e308}]"#,
                    r#"[{"type":"text","text":"minus159","n":-1.7976931348623159e308}]"#,
                    r#"[{"type":"text","text":"past18","n":1.8e308}]"#,
                    r#"[{"type":"text","text":"least","n":5e-324}]"#,
                    r#"[{"type":"text","text":"under least","n":2e-324}]"#,
                ]
                .iter()
                .map(|content| event("message.user", &[("content", content)]))
                .collect(),
            ),
            ..case("numbers at the edge of a double's range")
        },
        Case {
            fullness: pressure,
            seated: true,
            recall: Some(vec![
                said(
                    "message.user",
                    &[
                        &format!("{} abcdefghij next", ["0123456789"; 59].join(" ")),
                        "unseen",
                    ],
                ),
                said("message.assistant", &["also unseen"]),
            ]),
            ..case("a word exactly what is left, then the budget spent")
        },
        Case {
            fullness: pressure,
            seated: true,
            recall: Some(vec![
                said("message.user", &[&"\u{e9}".repeat(300), "z"]),
                said("message.user", &["z"]),
            ]),
            ..case("a word's cost in UTF-8 bytes")
        },
        Case {
            fullness: pressure,
            seated: true,
            recall: Some(vec![said("message.user", &[&"x".repeat(601), "after"])]),
            ..case("a first word past the budget")
        },
        Case {
            fullness: pressure,
            recall: Some(vec![said("message.assistant", &["", " \t\n ", "word"])]),
            ..case("an empty block and a block of whitespace")
        },
        Case {
            first_turn: true,
            text: "he said \"hi\"\n\u{1f600}\u{0} \\u0041".to_string(),
            ..case("the turn's text crosses unchanged")
        },
    ];
    // Every case once more with the other seated answer, where that is another branch.
    let flipped: Vec<Case> = all
        .iter()
        .filter(|c| c.fullness == pressure && !c.seated)
        .map(|c| Case {
            seated: true,
            ..c.clone()
        })
        .collect();
    all.extend(flipped);
    all
}

#[test]
fn today_s_loop_as_wires_asks_what_compose_asks_and_sends_its_delta_byte_for_byte() {
    let cases = cases();
    let (mut relay, lines, child) = wired(WIRES);
    let mut compiled_records = Vec::new();
    for case in &cases {
        let mut compiled = Recording::new(case);
        let compiled_answer = compose(&mut compiled, &case.text);
        let mut wires = Recording::new(case);
        let wired_answer = relay.pass(&mut wires, &case.text);
        assert_eq!(
            wires.asked(),
            compiled.asked(),
            "{}: the wires asked otherwise than compose",
            case.name
        );
        assert_eq!(
            answered(&wired_answer),
            answered(&compiled_answer),
            "{}",
            case.name
        );
        compiled_records.push((case.name, compiled.asked()));
    }
    assert_eq!(String::from_utf8_lossy(&lines.0.borrow()), "", "no break");
    let (exited_clean, runner_said) = ended(relay, child);
    assert!(exited_clean, "the runner's exit: {runner_said}");
    // The wires refuse where compose turns nothing: the end line's counts, and no other line.
    let turned = compiled_records
        .iter()
        .filter(|(_, asked)| matches!(asked.last(), Some(Asked::Turn(_))))
        .count();
    assert_eq!(
        runner_said,
        end_line(cases.len(), turned, cases.len() - turned),
        "the runner refused no pass of its own and broke nothing"
    );

    // What the comparison holds, read against compose itself (the contract's section 8).
    let record = |name: &str| {
        compiled_records
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, asked)| asked.clone())
            .expect(name)
    };
    let turn_texts = |name: &str| match record(name).last() {
        Some(Asked::Turn(messages)) => messages
            .iter()
            .map(|(role, blocks)| {
                assert_eq!(blocks.len(), 1, "{name}: one block in each message");
                (*role, blocks[0].clone())
            })
            .collect::<Vec<_>>(),
        other => panic!("{name}: no turn: {other:?}"),
    };
    assert_eq!(
        record("independent, nothing seated")[..3],
        [Asked::Seated, Asked::TurnContext, Asked::Flush(0)],
        "seated, then turn_context, then the flush with keep 0"
    );
    assert_eq!(
        record("independent, the flush answered null"),
        vec![Asked::Seated, Asked::TurnContext, Asked::Flush(0)]
    );
    assert_eq!(
        record("pressure, nothing seated, the record unreachable"),
        vec![
            Asked::Seated,
            Asked::TurnContext,
            Asked::FirstTurn,
            Asked::Fullness,
            Asked::Flush(0),
            Asked::Recall(Some(4)),
            Asked::Turn(vec![
                (
                    "system",
                    vec![format!(
                        "{SYSTEM}\n\nThe working context was reset to stay within its limit. \
                         Recent conversation, restored from the session's record:\n\
                         (The record could not be reached: continue from the request alone.)"
                    )]
                ),
                ("user", vec!["the request".to_string()]),
            ]),
        ]
    );
    let quoted = turn_texts("contents serde_json refuses, and the ones it takes")[0]
        .1
        .clone();
    assert!(
        quoted.contains("user: deep127") && !quoted.contains("deep128"),
        "127 levels parse and 128 do not, measured against compose: {quoted}"
    );
    assert!(
        !quoted.contains("nan") && !quoted.contains("inf") && !quoted.contains("big"),
        "NaN, the infinities and numbers past a double's range yield no line: {quoted}"
    );
    assert!(quoted.contains("user: under u64"), "{quoted}");
    assert!(
        quoted.contains("user: second"),
        "the last member of a name: {quoted}"
    );
    assert!(
        !quoted.contains("twice surrogate") && !quoted.contains("twice deep"),
        "a member replaced by a later one of its name is still judged: {quoted}"
    );
    let split = turn_texts("words split on White_Space alone")[0].1.clone();
    assert!(
        split.ends_with("user: a\u{1c}b c d e f\u{180e}g\u{200b}h i\u{1f}j k l m"),
        "Rust keeps U+001C and U+001F inside a word: {split:?}"
    );
    let rest = turn_texts("the rest of White_Space splits a word")[0]
        .1
        .clone();
    let words: Vec<String> = (0..=MORE_WHITE_SPACE.len() + 1)
        .map(|n| format!("w{n}"))
        .collect();
    assert!(
        rest.ends_with(&format!("user: {}", words.join(" "))),
        "each of the rest of White_Space splits: {rest:?}"
    );
    let edge = turn_texts("numbers at the edge of a double's range")[0]
        .1
        .clone();
    for (word, taken) in [
        ("max157", true),
        ("max158", true),
        ("past159", false),
        ("minus159", false),
        ("past18", false),
        ("least", true),
        ("under least", true),
    ] {
        assert_eq!(
            edge.lines().any(|line| line == format!("user: {word}")),
            taken,
            "{word}: taken {taken} by compose in this build: {edge}"
        );
    }
    assert_eq!(
        turn_texts("the turn's text crosses unchanged")[1].1,
        "he said \"hi\"\n\u{1f600}\u{0} \\u0041"
    );
}

#[test]
fn a_continuity_line_reads_as_section_8_writes_it() {
    // One earlier run of one turn, read off compose alone: the fixed sentence the wires copy.
    let case = Case {
        first_turn: true,
        shape: Some(shape(&[&[("turn.closed", 1)], &[]])),
        ..case("continuity")
    };
    let mut seat = Recording::new(&case);
    compose(&mut seat, &case.text).expect("the turn");
    let Some(Asked::Turn(messages)) = seat.asked().last().cloned() else {
        panic!("no turn");
    };
    assert_eq!(
        messages[0].1,
        vec![format!(
            "{SYSTEM}\n\nContinuity: this session held 1 earlier run with 1 completed turn \
             before this one. The conversation content of those runs is not in your context."
        )]
    );
}

const SYSTEM: &str = "You are a careful assistant. Answer from what you know, say plainly when \
                      you do not know, and keep answers as short as the question allows.";

#[test]
fn serde_json_s_private_raw_value_member_is_the_one_known_difference() {
    // Contract section 8: with serde_json's raw_value on, Value reads an object whose first
    // member is named `$serde_json::private::RawValue` as that member's string parsed as a
    // fresh document. compose meets it, and the wires stay plain. Each side is pinned here.
    let raw = "$serde_json::private::RawValue";
    let shapes = [
        (
            "the member alone in a block",
            format!(r#"[{{"{raw}":"{{\"type\":\"text\",\"text\":\"x\"}}"}}]"#),
        ),
        (
            "the member alone as the whole content",
            format!(r#"{{"{raw}":"[{{\"type\":\"text\",\"text\":\"x\"}}]"}}"#),
        ),
        (
            "the member first with another after it",
            format!(r#"[{{"{raw}":"{{\"type\":\"text\",\"text\":\"x\"}}","text":"y"}}]"#),
        ),
    ];
    let (mut relay, lines, child) = wired(WIRES);
    let mut seen = Vec::new();
    for (name, content) in &shapes {
        let case = Case {
            fullness: Some((80, 100)),
            seated: true,
            recall: Some(vec![event("message.user", &[("content", content)])]),
            ..case(name)
        };
        let quoted = |asked: Vec<Asked>| match asked.last() {
            Some(Asked::Turn(messages)) => messages[0]
                .1
                .iter()
                .flat_map(|text| text.lines())
                .filter(|line| line.starts_with("user: "))
                .map(str::to_string)
                .collect::<Vec<_>>(),
            other => panic!("{name}: no turn: {other:?}"),
        };
        let mut compiled = Recording::new(&case);
        compose(&mut compiled, &case.text).expect("compose turns");
        let mut wires = Recording::new(&case);
        relay.pass(&mut wires, &case.text).expect("the wires turn");
        seen.push((*name, quoted(compiled.asked()), quoted(wires.asked())));
    }
    assert_eq!(String::from_utf8_lossy(&lines.0.borrow()), "", "no break");
    let (exited_clean, runner_said) = ended(relay, child);
    assert!(exited_clean, "{runner_said}");
    assert_eq!(runner_said, end_line(shapes.len(), shapes.len(), 0));
    let user = |text: &str| vec![format!("user: {text}")];
    assert_eq!(
        seen,
        vec![
            ("the member alone in a block", user("x"), vec![]),
            ("the member alone as the whole content", user("x"), vec![]),
            ("the member first with another after it", vec![], user("y")),
        ],
        "(shape, what compose quotes, what the wires quote)"
    );
}
