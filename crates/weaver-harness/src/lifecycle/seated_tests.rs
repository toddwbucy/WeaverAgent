//! Test-only: the seated read and the deployed loop yielding to it, per
//! `weaver-spu-adapter-binding-Spec-amendment` section 4 (ADP-4) and
//! `weaver-harness-Spec` section 6's seated read. A real enter stands a
//! stand-in SPU process (this binary run again) against a scripted state
//! member, then the deployed loop, `dev_loop::drive` compiled in from the
//! worker's own source, serves frames through the serve loop's frame path,
//! and the stand-in records every delta the SPU was handed. The enter is
//! real through the identity ask and the open and stops at the gate's fork,
//! the gate binary being absent: what follows the fork is the gate's step
//! alone (the tools directory, the pair, the raise), which sets nothing the
//! seated read holds, and the test stands a gate peer in its place.

use super::*;
use std::io::{BufRead, BufReader, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::PermissionsExt;

// The deployed loop itself (compiled in at the crate root), so the seat's
// half of the rule is read through the loop that consumes it rather than a
// copy of its judgment.
use crate::dev_loop;

/// The variable that turns [`seated_spu_child`] into the stand-in SPU,
/// naming the directory it records into. Set only by the wrapper script
/// [`enter_seated`] writes.
const SPU_CHILD: &str = "WEAVER_HARNESS_SEATED_SPU_CHILD";

/// The llama family's BOS token, which its renderer prepends to every
/// identity, an empty one included (`weaver-spu`'s
/// `family/llama.rs::render_identity`).
const BOS: u32 = 128_000;

/// The member's identity answer for an agent the seeding turn seeded.
const SEEDED: &str = concat!(
    r#"{"answer":{"identity":{"messages":[{"envelope":{"session":"s-0","#,
    r#""run":"r-0","kind":"message.system","sequence":"3"},"#,
    r#""pairs":{"role":"system","content":[{"type":"text","#,
    r#""text":"You are Karl."}]}}]}}}"#
);

/// The member's identity answer for an agent before its seeding turn.
const UNSEEDED: &str = r#"{"answer":{"identity":{"messages":[]}}}"#;

/// One earlier run with two closed turns, so the unseated opening carries
/// a continuity line and the seated case has one to withhold.
const SHAPE: &str = concat!(
    r#"{"answer":{"shape":{"runs":[{"run":"r-0","kinds":{"load":1,"#,
    r#""turn.closed":2}},{"run":"r-1","kinds":{"load":1}}]}}}"#
);

/// One recalled user message, quoted by the re-entry.
const RECALL: &str = concat!(
    r#"{"answer":{"recall":{"events":[{"envelope":{"session":"s-1","run":"r-1","#,
    r#""turn":"t-1","kind":"message.user","sequence":"11"},"#,
    r#""pairs":{"content":[{"type":"text","text":"the plan"}]}}]}}}"#
);

/// **The stand-in SPU**: admits, opens with a llama-family prefix whose
/// BOS leads whatever identity the Open carried, then answers every append
/// at four fifths of capacity, so the loop's next turn meets pressure, and
/// every flush, recording each delta as one JSON line until the harness
/// closes the decode end. Any other run returns at once.
#[test]
fn seated_spu_child() {
    use nix::sys::socket::{MsgFlags, recv, send};
    let Some(root) = std::env::var_os(SPU_CHILD) else {
        return;
    };
    let root = std::path::PathBuf::from(root);
    // SAFETY: the two child ends `fork_organ` placed for this process, at
    // the numbers `weaver-spu`'s `channel.rs` names.
    let lifecycle = unsafe { OwnedFd::from_raw_fd(3) };
    let decode = unsafe { OwnedFd::from_raw_fd(4) };
    let take = |fd: &OwnedFd| -> Option<Vec<u8>> {
        let mut bytes = vec![0; weaver_types::MAX_ENVELOPE_BYTES];
        match recv(fd.as_raw_fd(), &mut bytes, MsgFlags::empty()) {
            Ok(0) | Err(_) => None,
            Ok(n) => Some(bytes[..n].to_vec()),
        }
    };
    let put = |fd: &OwnedFd, bytes: Vec<u8>| {
        assert_eq!(
            send(fd.as_raw_fd(), &bytes, MsgFlags::MSG_NOSIGNAL),
            Ok(bytes.len())
        );
    };

    let admit: OrganEnvelope =
        serde_json::from_slice(&take(&lifecycle).expect("Admit")).expect("Admit parses");
    assert!(matches!(
        admit.payload,
        weaver_types::Payload::Directive(LifecycleDirective::Admit { .. })
    ));
    put(
        &lifecycle,
        serde_json::to_vec(&OrganEnvelope {
            exchange: admit.exchange,
            position: Position::Close,
            payload: weaver_types::Payload::Answer(LifecycleAnswer::Admitted),
        })
        .unwrap(),
    );

    let TokenDirective::Open {
        session, messages, ..
    } = serde_json::from_slice(&take(&decode).expect("Open")).expect("Open parses")
    else {
        panic!("Open follows Admit");
    };
    std::fs::write(
        root.join("open.json"),
        serde_json::to_vec(&messages).unwrap(),
    )
    .unwrap();
    let mut ids = vec![BOS];
    ids.extend(messages.iter().map(|_| 9_u32));
    let rendered = format!("<|begin_of_text|>{}", "x".repeat(messages.len()));
    let raw = |value: serde_json::Value| {
        serde_json::value::RawValue::from_string(value.to_string()).unwrap()
    };
    let prefix = ids.len() as u64;
    put(
        &decode,
        serde_json::to_vec(&TokenAnswer::Opened(weaver_types::Opened {
            session,
            prefix_tokens: prefix,
            capacity: 100,
            generation_limit: 7,
            terminal_tokens: 1,
            request: raw(serde_json::json!({ "rendered": rendered, "template": "llama" })),
            measurement: raw(serde_json::json!({
                "model": "m", "weights_hash": "h", "adapter": null, "prefix_token_ids": ids,
                "blocks": [{ "label": "identity-prefix", "start": 0, "end": rendered.len() }],
            })),
        }))
        .unwrap(),
    );

    let mut deltas = std::fs::File::create(root.join("deltas.ndjson")).unwrap();
    while let Some(bytes) = take(&decode) {
        match serde_json::from_slice::<TokenDirective>(&bytes) {
            Ok(TokenDirective::AppendAndGenerate { delta, .. }) => {
                writeln!(deltas, "{}", serde_json::to_string(&delta).unwrap()).unwrap();
                put(
                    &decode,
                    serde_json::to_vec(&TokenAnswer::Generated(weaver_types::Generation {
                        unrecovered: vec![],
                        content: vec![],
                        emission: "noted".into(),
                        finish: weaver_types::Finish::Completed,
                        resident: 80,
                        capacity: 100,
                        invalid_utf8: Vec::new(),
                        request: raw(serde_json::json!({
                            "rendered": "r", "template": "llama", "sampling": {}
                        })),
                        measurement: raw(serde_json::json!({
                            "model": "m", "weights_hash": "h", "input_tokens": [1],
                            "output_tokens": [2], "blocks": [],
                            "timings": { "prefill_ns": "1", "decode_ns": "2" },
                        })),
                    }))
                    .unwrap(),
                );
            }
            Ok(TokenDirective::Flush { .. }) => put(
                &decode,
                serde_json::to_vec(&TokenAnswer::Flushed {
                    resident_before: 80,
                    resident_after: prefix,
                })
                .unwrap(),
            ),
            _ => {}
        }
    }
}

/// A serving run entered through the real enter against the stand-in SPU,
/// with a gate peer standing where the gate would, ready for frames.
struct Entered {
    harness: Harness,
    gate: OrganChannel,
    root: crate::scratch::Scratch,
    spu: nix::unistd::Pid,
    _coordination: crate::scratch::Scratch,
}

impl Entered {
    /// What the Open carried as the identity's messages.
    fn opened_with(&self) -> Vec<weaver_traits::Message> {
        serde_json::from_slice(&std::fs::read(self.root.0.join("open.json")).unwrap()).unwrap()
    }

    /// Serves one frame through the serve loop's frame path, the deployed
    /// loop across the crossing, and answers the delta the SPU was handed.
    fn turn(&mut self, ordinal: u64, text: &str) -> Vec<weaver_traits::Message> {
        let line = serde_json::json!({ "text": text }).to_string();
        self.gate
            .send(&OrganEnvelope {
                exchange: ExchangeId {
                    opener: Opener::Gate,
                    ordinal,
                },
                position: Position::Open,
                payload: weaver_types::Payload::Frame(weaver_types::TurnFrame::carry(
                    line.as_bytes(),
                )),
            })
            .expect("the frame sends");
        self.harness
            .serve_gate_wake(&mut dev_loop::drive, &mut None)
            .expect("the wake serves");
        let answer = self.gate.recv().expect("the response frame returns");
        assert_eq!(answer.exchange.ordinal, ordinal);
        let deltas = std::fs::read_to_string(self.root.0.join("deltas.ndjson")).unwrap();
        let last = deltas.lines().last().expect("the SPU was handed a delta");
        serde_json::from_str(last).unwrap()
    }

    /// The seat's read, through the seam a loop crosses.
    fn seated(&mut self) -> bool {
        self.harness
            .grant_seat()
            .expect("a loaded-and-idle serving run grants the seat")
            .seated()
    }
}

impl Drop for Entered {
    fn drop(&mut self) {
        // Dropping the run closes the harness's decode end, which ends the
        // stand-in; the member's thread ends at its own end-of-file.
        self.harness.state = ChannelState::BeforeEnter;
        let _ = nix::sys::wait::waitpid(self.spu, None);
    }
}

/// Enters a serving run whose SPU is the stand-in above, against a member
/// answering `identity` with the line given, or against no member at all.
fn enter_seated(identity: Option<&'static str>) -> Entered {
    let tag = format!(
        "weaver-seated-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    );
    let root = crate::scratch::dir(tag.clone());
    let coordination_dir = crate::scratch::dir(format!("{tag}-c"));
    let quote = |s: &str| format!("'{}'", s.replace('\'', "'\\''"));
    let spu = root.0.join("spu");
    std::fs::write(
        &spu,
        format!(
            "#!/bin/sh\nexport {SPU_CHILD}={}\nexec {} --exact \
             lifecycle::tests::seated::seated_spu_child --nocapture -q\n",
            quote(root.0.to_str().unwrap()),
            quote(std::env::current_exe().unwrap().to_str().unwrap()),
        ),
    )
    .unwrap();
    std::fs::set_permissions(&spu, std::fs::Permissions::from_mode(0o700)).unwrap();
    let listener =
        crate::channel::bind_coordination(&coordination_dir.0.join("c.sock")).expect("bind");
    let sink = OwnedFd::from(File::create(root.0.join("trace.ndjson")).expect("sink"));
    let mut harness = Harness {
        coordination: listener,
        organs: OrganBinaries {
            classify: None,
            spu,
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
                    artifact: weaver_types::ArtifactRef("stand-in".into()),
                    devices: vec![weaver_types::DeviceOrdinal(0)],
                    adapter: None,
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
        turn_context: weaver_types::TurnContext::Continuous,
    };
    let state_end = match identity {
        None => None,
        Some(identity) => {
            let (near, far) = std::os::unix::net::UnixStream::pair().expect("pair");
            std::thread::spawn(move || {
                let mut answers = far.try_clone().expect("clone");
                for line in BufReader::new(far).lines() {
                    let Ok(line) = line else { break };
                    let answer = if line.starts_with(r#"{"ask":{"grants""#) {
                        r#"{"answer":{"grants":{"surface":[]}}}"#
                    } else if line.starts_with(r#"{"ask":{"identity""#) {
                        identity
                    } else if line.starts_with(r#"{"ask":{"restored""#) {
                        EMPTY_RESTORED
                    } else if line.starts_with(r#"{"ask":{"shape""#) {
                        SHAPE
                    } else if line.starts_with(r#"{"ask":{"recall""#) {
                        RECALL
                    } else {
                        continue;
                    };
                    if answers.write_all(format!("{answer}\n").as_bytes()).is_err() {
                        break;
                    }
                }
            });
            Some(OwnedFd::from(near))
        }
    };
    // The gate binary does not exist, so the fan-out stops at the gate's
    // fork with the SPU standing: everything the seat reads is in place,
    // the gate's step being all that the enter leaves unrun.
    let mut run = match harness.enter(payload, Some(sink), state_end) {
        Ok(run) => run,
        Err(EnterFailure::AfterLoad(run, _)) => *run,
        Err(EnterFailure::BeforeLoad(refusal)) => panic!("refused before the load: {refusal:?}"),
    };
    let spu_pid = run
        .spu
        .as_ref()
        .expect("the stand-in admitted and opened")
        .pid;
    let (gate_end, gate_peer) = OrganChannel::pair().expect("gate pair");
    run.gate = Some(GateChannel {
        channel: gate_end,
        pid: nix::unistd::Pid::from_raw(i32::MAX),
        last_word: crate::spawn::LastWord::quiet(),
        shell_granted: false,
    });
    harness.state = ChannelState::Entered(Box::new(run));
    Entered {
        harness,
        gate: gate_peer.into_channel(),
        root,
        spu: spu_pid,
        _coordination: coordination_dir,
    }
}

fn texts(delta: &[weaver_traits::Message]) -> Vec<(weaver_traits::Role, String)> {
    delta
        .iter()
        .map(|message| {
            let text = message
                .content
                .iter()
                .map(|block| match block {
                    weaver_traits::ContentBlock::Text { text } => text.as_str(),
                    _ => "",
                })
                .collect::<String>();
            (message.role.clone(), text)
        })
        .collect()
}

/// **A seated prefix takes no compiled prompt, no continuity line and a
/// bare re-entry**, per ADP-4: an agent loaded as admin loads it, with no
/// positional identity and a member whose identity answer carries the
/// seeded prompt, reads seated, and the deployed loop's first delta is the
/// request alone; at pressure the re-entry is the recall text without the
/// compiled prompt.
///
/// Perturbations: push the compiled prompt unconditionally (in
/// `contribution` or `reentry_text`) and the delta assertions fail; read
/// `Prompt.identity` as the predicate and the seat reads unseated, the
/// assembled identity being empty; read the Open's `prefix_tokens` and the
/// unseated case below fails, the BOS counting.
#[test]
fn a_seated_prefix_takes_no_compiled_prompt_no_continuity_and_a_bare_reentry() {
    let mut entered = enter_seated(Some(SEEDED));
    assert_eq!(
        entered.opened_with().len(),
        1,
        "the Open carried the member's one identity message"
    );
    assert!(entered.seated(), "the member's answer seated a prefix");

    let first = entered.turn(1, "hello");
    assert_eq!(
        texts(&first),
        vec![(weaver_traits::Role::User, "hello".to_string())],
        "the first delta is the request alone: no compiled prompt, no continuity line"
    );

    let second = entered.turn(2, "and again");
    let texts = texts(&second);
    assert_eq!(texts.len(), 2, "the re-entry and the request: {texts:?}");
    assert_eq!(texts[0].0, weaver_traits::Role::System);
    assert!(
        texts[0].1.starts_with("The working context was reset"),
        "the re-entry is the recall text alone: {}",
        texts[0].1
    );
    assert!(texts[0].1.contains("the plan"), "{}", texts[0].1);
    assert!(
        !second
            .iter()
            .chain(first.iter())
            .any(|m| texts_of(m).contains(dev_loop::SYSTEM_PROMPT)),
        "the compiled prompt rides nowhere"
    );
}

/// **An unseeded open with a BOS prefix carries the compiled prompt**, per
/// ADP-4: a llama-family agent before its seeding turn opens with one
/// token, the BOS, and nothing seated, so the seat reads unseated and the
/// deployed loop leads with its compiled prompt and the continuity line,
/// and re-enters with the compiled prompt at the re-entry's head. The
/// storeless agent reads unseated too.
///
/// Perturbation: read the Open's `prefix_tokens` (non-zero here) as the
/// predicate and the seat reads seated.
#[test]
fn an_unseeded_open_with_a_bos_prefix_carries_the_compiled_prompt() {
    let mut entered = enter_seated(Some(UNSEEDED));
    assert!(
        entered.opened_with().is_empty(),
        "the Open carried no identity message"
    );
    assert!(!entered.seated(), "the BOS alone seats nothing");

    let first = texts(&entered.turn(1, "hello"));
    assert_eq!(first.len(), 2, "the opening and the request: {first:?}");
    assert_eq!(first[0].0, weaver_traits::Role::System);
    assert!(
        first[0].1.starts_with(dev_loop::SYSTEM_PROMPT),
        "the compiled prompt leads: {}",
        first[0].1
    );
    assert!(
        first[0]
            .1
            .contains("Continuity: this session held 1 earlier run"),
        "the continuity line rides beneath it: {}",
        first[0].1
    );
    assert_eq!(first[1], (weaver_traits::Role::User, "hello".to_string()));

    let second = texts(&entered.turn(2, "and again"));
    assert_eq!(second.len(), 2, "the re-entry and the request: {second:?}");
    assert!(
        second[0].1.starts_with(dev_loop::SYSTEM_PROMPT),
        "the re-entry restores the compiled prompt: {}",
        second[0].1
    );
    drop(entered);

    let mut storeless = enter_seated(None);
    assert!(storeless.opened_with().is_empty());
    assert!(!storeless.seated(), "no member, nothing seated");
    let first = texts(&storeless.turn(1, "hello"));
    assert!(first[0].1.starts_with(dev_loop::SYSTEM_PROMPT), "{first:?}");
}

fn texts_of(message: &weaver_traits::Message) -> String {
    texts(std::slice::from_ref(message))
        .pop()
        .map(|(_, text)| text)
        .unwrap_or_default()
}
