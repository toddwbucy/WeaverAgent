use super::*;

/// **The answer bound is the contract's one number**, per
/// `weaver-harness-state-contract` section 3: one mebibyte, which the
/// member's `ANSWER_BOUND` in `weaver-state` holds as the same number
/// under its own watch, the two crates seeing neither's constant. A
/// change to either without the other is the defect this pins.
#[test]
fn the_answer_bound_is_the_contracts_mebibyte() {
    assert_eq!(ANSWER_BOUND_BYTES, 1024 * 1024);
    // One entry's cost is never under what the member renders for it:
    // the envelope's fixed text with the kind and a twenty-digit
    // sequence fits the allowance beside the names.
    let envelope = r#"{"envelope":{"session":"","run":"","kind":"message.system","sequence":"12345678901234567890"},"pairs":},"#;
    assert!(envelope.len() <= ENVELOPE_BYTES, "{}", envelope.len());
    // The frame's own text and its delimiter fit the allowance.
    assert!(r#"{"answer":{"identity":{"messages":[]}}}"#.len() < ANSWER_FRAME_BYTES);
    assert_eq!(
        identity_entry_cost(10, "s-1", "r-1"),
        10 + 3 + 3 + ENVELOPE_BYTES
    );
    // **The names cost what the member writes, escapes included**: a
    // quote and a backslash each render as two bytes, so a name of one
    // costs one more than its length (Codex on #92, round 6).
    // Perturbation: count `name.len()` again and this fails by two.
    assert_eq!(
        identity_entry_cost(10, "s\"1", "r\\1"),
        10 + 4 + 4 + ENVELOPE_BYTES
    );
    assert_eq!(rendered_name("s\"\\\n"), 7, "three escapes, two bytes each");
}

/// The answer's wire spelling parses to the shape, and a frame missing
/// any member of it answers nothing.
#[test]
fn the_shape_answer_parses_whole_or_not_at_all() {
    let line = concat!(
        r#"{"answer":{"shape":{"runs":[{"run":"r-1","kinds":{"load":1,"#,
        r#""turn.closed":3}},{"run":"r-2","kinds":{"load":1}}]}}}"#
    );
    let shape = parse_shape_answer(line).expect("parses");
    assert_eq!(shape.runs.len(), 2);
    assert_eq!(shape.runs[0].run, "r-1");
    assert!(
        shape.runs[0]
            .kinds
            .contains(&("turn.closed".to_string(), 3))
    );
    for malformed in [
        r#"{"answer":{"shape":{}}}"#,
        r#"{"answer":{}}"#,
        r#"{"answer":{"shape":{"runs":[{"kinds":{}}]}}}"#,
        "not json",
    ] {
        assert!(parse_shape_answer(malformed).is_none(), "{malformed}");
    }
}

/// The bounded wait converts a silent peer into a missing answer: a
/// member that never speaks costs the answer inside the bound, never a
/// hang. The bound is the constant's, so the test waits it out once.
#[test]
fn a_silent_peer_costs_the_answer_inside_the_bound() {
    let (ours, _theirs) = UnixStream::pair().expect("pair");
    ours.set_nonblocking(true).expect("nonblocking");
    let mut seam = StateSeam::new(ours);
    let started = std::time::Instant::now();
    assert!(seam.ask_shape().is_none());
    assert!(started.elapsed() < std::time::Duration::from_secs(10));
}

/// The serve direction retires on its first failure: a malformed
/// answer costs its ask, and a well-formed answer already waiting
/// behind it is never read, because attributing it to a later ask
/// would misplace its position in the stream.
#[test]
fn the_seam_retires_on_its_first_failure() {
    let (ours, theirs) = UnixStream::pair().expect("pair");
    ours.set_nonblocking(true).expect("nonblocking");
    let mut seam = StateSeam::new(ours);
    let mut peer = theirs;
    peer.write_all(b"garbage\n{\"answer\":{\"shape\":{\"runs\":[]}}}\n")
        .expect("answers in advance");
    assert!(seam.ask_shape().is_none(), "a malformed answer misses");
    assert!(
        seam.ask_shape().is_none(),
        "the retired seam never reads the late well-formed answer"
    );
}

/// **The grants ask reads the surface as strings and nothing else**, per
/// the contract's fourth ask of 2026-09-04. Perturbation: accept a
/// non-string entry and the second case answers a surface with a hole
/// in it, which two readings would then compare as equal across.
#[test]
fn the_grants_ask_reads_the_surface_whole_or_not_at_all() {
    let (ours, theirs) = UnixStream::pair().expect("pair");
    ours.set_nonblocking(true).expect("nonblocking");
    let mut seam = StateSeam::new(ours);
    let mut peer = theirs;
    peer.write_all(b"{\"answer\":{\"grants\":{\"surface\":[\"owner 0:0\",\"mode 0640\"]}}}\n")
        .expect("answers in advance");
    let surface = seam.ask_grants().expect("answered");
    assert_eq!(
        surface,
        vec!["owner 0:0".to_string(), "mode 0640".to_string()]
    );
    let mut asked = [0u8; 64];
    let n = peer.read(&mut asked).expect("reads the ask");
    assert_eq!(
        &asked[..n],
        b"{\"ask\":{\"grants\":{}}}\n",
        "the ask carries no members"
    );

    let (ours, theirs) = UnixStream::pair().expect("pair");
    ours.set_nonblocking(true).expect("nonblocking");
    let mut seam = StateSeam::new(ours);
    let mut peer = theirs;
    peer.write_all(b"{\"answer\":{\"grants\":{\"surface\":[\"owner 0:0\",7]}}}\n")
        .expect("answers in advance");
    assert!(
        seam.ask_grants().is_none(),
        "a non-string entry is a malformed answer"
    );
}

/// **The two save-point asks cross and their answers parse whole or
/// not at all**, per `weaver-harness-state-contract` section 2:
/// `restored` answers a stamp, nothing, or a refusal, and a body that is
/// none of the three is malformed; `snapshot` answers the five stamp
/// members. Each ask is spelled as the contract spells it. The live
/// `restore` ask is retired (the operator's ruling of 2026-10-08 on #99).
///
/// Perturbation: drop `turn` from `parse_stamped` and the snapshot
/// assertion on the turn fails; accept a restored body with an unknown
/// member as `Empty` and the malformed case parses.
#[test]
fn the_save_point_asks_cross_and_parse() {
    let exchange = |answer: &str, ask: &dyn Fn(&mut StateSeam) -> Option<String>| {
        let (ours, theirs) = UnixStream::pair().expect("pair");
        ours.set_nonblocking(true).expect("nonblocking");
        let mut seam = StateSeam::new(ours);
        let mut peer = theirs;
        peer.write_all(format!("{answer}\n").as_bytes())
            .expect("answers in advance");
        let parsed = ask(&mut seam);
        let mut asked = [0u8; 256];
        let n = peer.read(&mut asked).expect("reads the ask");
        (parsed, String::from_utf8_lossy(&asked[..n]).into_owned())
    };
    let (parsed, asked) = exchange(
        r#"{"answer":{"restored":{"lineage":{"digest":"ab","run":"r-1","sequence":41,"turn":2}}}}"#,
        &|seam| seam.ask_restored().map(|a| format!("{a:?}")),
    );
    assert_eq!(asked, "{\"ask\":{\"restored\":{}}}\n");
    assert_eq!(
        parsed.as_deref(),
        Some(
            format!(
                "{:?}",
                RestoredAnswer::Lineage(SavePointStamp {
                    digest: "ab".into(),
                    run: "r-1".into(),
                    sequence: 41,
                    turn: 2
                })
            )
            .as_str()
        )
    );
    for (answer, expected) in [
        (r#"{"answer":{"restored":{}}}"#, Some(RestoredAnswer::Empty)),
        (
            r#"{"answer":{"restored":{"refused":"schema-mismatch"}}}"#,
            Some(RestoredAnswer::Refused("schema-mismatch".into())),
        ),
        (r#"{"answer":{"restored":{"other":1}}}"#, None),
        (
            r#"{"answer":{"restored":{"lineage":{"digest":"ab"}}}}"#,
            None,
        ),
        (r#"{"answer":{"shape":{"runs":[]}}}"#, None),
    ] {
        assert_eq!(parse_restored_answer(answer), expected, "{answer}");
    }
    // The four legs: both answers stand in advance, and what was asked
    // is the snapshot ask followed by the acknowledgement of the digest.
    let (parsed, asked) = exchange(
        concat!(
            r#"{"answer":{"snapshot":{"ask":1,"save-point":"ab.save-point","run":"r-1","sequence":41,"turn":2,"digest":"ab"}}}"#,
            "\n",
            r#"{"answer":{"finished":{"ask":1,"save-point":"ab.save-point"}}}"#
        ),
        &|seam| seam.ask_snapshot().ok().map(|a| format!("{a:?}")),
    );
    assert_eq!(
        asked,
        concat!(
            "{\"ask\":{\"snapshot\":{\"ask\":1}}}\n",
            "{\"acknowledge\":{\"snapshot\":{\"ask\":1,\"digest\":\"ab\"}}}\n"
        )
    );
    // A finished answer naming another file, or none, is the finished
    // leg missed. Perturbation: skip the name's comparison and the first
    // case parses.
    let (parsed_other, _) = exchange(
        concat!(
            r#"{"answer":{"snapshot":{"ask":1,"save-point":"ab.save-point","run":"r-1","sequence":41,"turn":2,"digest":"ab"}}}"#,
            "\n",
            r#"{"answer":{"finished":{"ask":1,"save-point":"other.save-point"}}}"#
        ),
        &|seam| seam.ask_snapshot().err().map(|leg| format!("{leg:?}")),
    );
    assert_eq!(parsed_other.as_deref(), Some("Finished"));
    let (parsed_none, _) = exchange(
        r#"{"answer":{"snapshot":{"ask":1,"save-point":"ab.save-point","run":"r-1","sequence":41,"turn":2,"digest":"ab"}}}"#,
        &|seam| seam.ask_snapshot().err().map(|leg| format!("{leg:?}")),
    );
    assert_eq!(parsed_none.as_deref(), Some("Finished"));
    let taken = SavePointTaken {
        name: "ab.save-point".into(),
        stamp: SavePointStamp {
            digest: "ab".into(),
            run: "r-1".into(),
            sequence: 41,
            turn: 2,
        },
    };
    assert_eq!(parsed, Some(format!("{taken:?}")));
    assert!(
        parse_snapshot_answer(
            r#"{"answer":{"snapshot":{"ask":1,"save-point":"x","run":"r","sequence":1,"digest":"d"}}}"#
        )
        .is_none(),
        "a stamp without its turn is malformed"
    );
    assert!(
        parse_snapshot_answer(
            r#"{"answer":{"snapshot":{"save-point":"x","run":"r","sequence":1,"turn":1,"digest":"d"}}}"#
        )
        .is_none(),
        "an answer without its ordinal is malformed (Codex on #94, round 10)"
    );
}

/// **The identity material's one source**, per `weaver-harness-Spec`
/// section 6.1 on the ruling of 2026-10-06: an empty answer is an agent
/// not yet seeded and opens with no prefix, a prefix rebuilds from its
/// pairs, and a half-read one refuses. Perturbation: skip the rebuild's
/// `content` and the rebuilt prefix carries no text.
#[test]
fn the_identity_material_has_one_source_and_one_rule() {
    assert_eq!(
        identity_material(&[]),
        Some(Vec::new()),
        "an empty answer is an unseeded agent, which opens with no prefix"
    );
    let (ours, theirs) = UnixStream::pair().expect("pair");
    ours.set_nonblocking(true).expect("nonblocking");
    let mut seam = StateSeam::new(ours);
    let mut peer = theirs;
    peer.write_all(
        concat!(
            r#"{"answer":{"identity":{"messages":[{"envelope":{"session":"s","run":"r-1","#,
            r#""kind":"message.system","sequence":"3"},"pairs":{"role":"system","#,
            r#""content":[{"type":"text","text":"You are Karl."}]}}]}}}"#,
            "\n"
        )
        .as_bytes(),
    )
    .expect("answers in advance");
    let held = seam.ask_identity_within(ANSWER_BOUND_MS).expect("answered");
    let material = identity_material(&held).expect("rebuilds");
    assert_eq!(material.len(), 1);
    assert!(matches!(material[0].role, weaver_traits::Role::System));
    assert!(
        matches!(&material[0].content[0], weaver_traits::ContentBlock::Text { text } if text == "You are Karl.")
    );
    let mut asked = [0u8; 64];
    let n = peer.read(&mut asked).expect("reads the ask");
    assert_eq!(&asked[..n], b"{\"ask\":{\"identity\":{}}}\n");
    let half = vec![Recalled {
        kind: "message.system".into(),
        run: "r-1".into(),
        turn: None,
        sequence: "3".into(),
        pairs: vec![("role".into(), "\"system\"".into())],
    }];
    assert!(
        identity_material(&half).is_none(),
        "a prefix without content refuses"
    );
}

/// An answer still unframed past the byte bound retires the seam
/// inside the bound rather than growing the turn path's memory.
#[test]
fn an_unframed_flood_retires_the_seam_inside_the_bound() {
    let (ours, theirs) = UnixStream::pair().expect("pair");
    ours.set_nonblocking(true).expect("nonblocking");
    let mut seam = StateSeam::new(ours);
    let responder = std::thread::spawn(move || {
        let mut peer = theirs;
        let mut taken = [0u8; 256];
        let _ = peer.read(&mut taken).expect("reads the ask");
        let flood = vec![b'x'; 2 * ANSWER_BOUND_BYTES];
        let _ = peer.write_all(&flood);
    });
    let started = std::time::Instant::now();
    assert!(seam.ask_shape().is_none());
    assert!(started.elapsed() < std::time::Duration::from_secs(10));
    drop(seam);
    responder.join().expect("responder");
}

/// A peer that answers is read whole across the seam.
#[test]
fn an_answering_peer_is_read() {
    let (ours, theirs) = UnixStream::pair().expect("pair");
    ours.set_nonblocking(true).expect("nonblocking");
    let mut seam = StateSeam::new(ours);
    let responder = std::thread::spawn(move || {
        let mut peer = theirs;
        let mut taken = [0u8; 256];
        let count = peer.read(&mut taken).expect("reads the ask");
        assert_eq!(&taken[..count], b"{\"ask\":{\"shape\":{}}}\n");
        peer.write_all(
            b"{\"answer\":{\"shape\":{\"runs\":[{\"run\":\"r-1\",\"kinds\":{\"load\":1}}]}}}\n",
        )
        .expect("answers");
    });
    let shape = seam.ask_shape().expect("answered");
    assert_eq!(shape.runs[0].run, "r-1");
    responder.join().expect("responder");
}

/// **A bound past the poll's ceiling is waited out, not answered at
/// the ceiling**, per the replay port's clause: `poll` takes a `u16` of
/// milliseconds, and a wait that ended when one poll expired would
/// answer `None` early to a caller who asked for longer - on precisely
/// the ask whose clause exists so the loop may wait as long as a
/// preload is worth. Both halves are watched here, the arithmetic and
/// the re-arm, the ceiling being lowered under `cfg(test)` so the
/// behavioural half costs milliseconds rather than the minute the
/// production ceiling would.
///
/// Perturbation: read a clamped poll's expiry as the deadline and the
/// silent-peer half fails, returning near the ceiling instead of near
/// the bound. The defect this watch exists for was measured live at
/// the review seat before the re-arm landed, an `ask_replay(120_000)`
/// answering `None` at 65.551 seconds.
#[test]
fn a_bound_past_the_ceiling_is_waited_out() {
    use std::time::Duration;
    // The arithmetic: one poll is a slice of what remains.
    assert_eq!(
        StateSeam::poll_slice(Duration::from_millis(120_000)),
        POLL_CEILING_MS
    );
    assert_eq!(StateSeam::poll_slice(Duration::from_millis(10)), 10);
    assert_eq!(StateSeam::poll_slice(Duration::from_millis(0)), 0);

    // The re-arm: a silent peer and a bound many ceilings long is
    // waited out to the bound, the deadline ending the wait and not
    // the poll.
    let bound_ms = u64::from(POLL_CEILING_MS) * 8;
    let (ours, _theirs) = UnixStream::pair().expect("pair");
    ours.set_nonblocking(true).expect("nonblocking");
    let mut seam = StateSeam::new(ours);
    let started = std::time::Instant::now();
    assert!(seam.ask_replay(bound_ms).is_none(), "the silence costs it");
    let waited = started.elapsed();
    assert!(
        waited >= Duration::from_millis(bound_ms),
        "the wait reaches the caller's bound, not the ceiling: {waited:?}"
    );
}

/// **The recall answer parses under its own name**, the sibling of the
/// replay parse and the other caller of the shared event read: the
/// envelope's facts including the run, the pairs as custody kept them,
/// and a frame under the replay's name is not a recall answer. Nothing
/// watched this path before the audit of 2026-08-26 found the shared
/// refactor unwatched on the recall side.
#[test]
fn the_recall_answer_parses_under_its_own_name() {
    let events = parse_recall_answer(concat!(
        r#"{"answer":{"recall":{"events":[{"envelope":{"session":"s","run":"r-2","#,
        r#""turn":"t-9","kind":"message.user","sequence":"11"},"#,
        r#""pairs":{"content":[{"type":"text","text":"the plan"}]}}]}}}"#
    ))
    .expect("parses");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].run, "r-2");
    assert_eq!(events[0].turn.as_deref(), Some("t-9"));
    assert_eq!(events[0].kind, "message.user");
    assert_eq!(events[0].pairs.len(), 1);
    assert_eq!(events[0].pairs[0].0, "content");
    assert!(
        events[0].pairs[0].1.contains("the plan"),
        "the pairs cross as custody kept them"
    );
    assert!(
        parse_recall_answer(r#"{"answer":{"replay":{"events":[]}}}"#).is_none(),
        "the two answers stay apart by name"
    );
    assert!(
        parse_recall_answer(concat!(
            r#"{"answer":{"recall":{"events":[{"envelope":{"session":"s",""#,
            r#"turn":"t-9","kind":"message.user","sequence":"11"},"pairs":{}}]}}}"#
        ))
        .is_none(),
        "an envelope missing the run fails the parse whole"
    );
}

/// **The replay ask crosses under its own name and reads the answer's
/// shape back**, per `weaver-harness-Spec` section 6's 2026-08-24
/// clause: the wire ask is the contract's, the caller's bound is the
/// wait, and the answer parses as the distillate's shape served back,
/// envelope and pairs, whole or not at all. A frame under the recall's
/// name is not a replay answer, which is what pins the two parses
/// apart.
#[test]
fn the_replay_ask_crosses_and_its_answer_parses() {
    let (ours, theirs) = UnixStream::pair().expect("pair");
    ours.set_nonblocking(true).expect("nonblocking");
    let mut seam = StateSeam::new(ours);
    let responder = std::thread::spawn(move || {
        let mut peer = theirs;
        let mut taken = [0u8; 256];
        let count = peer.read(&mut taken).expect("reads the ask");
        assert_eq!(&taken[..count], b"{\"ask\":{\"replay\":{}}}\n");
        peer.write_all(
            b"{\"answer\":{\"replay\":{\"events\":[{\"envelope\":{\"session\":\"s\",\
            \"run\":\"r-1\",\"kind\":\"model.output\",\"sequence\":\"4\"},\
            \"pairs\":{}}]}}}\n",
        )
        .expect("answers");
    });
    let events = seam.ask_replay(2_000).expect("answered");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind, "model.output");
    assert_eq!(events[0].run, "r-1");
    assert_eq!(events[0].sequence, "4");
    responder.join().expect("responder");

    // The recall's name does not parse as a replay answer.
    assert!(
        parse_replay_answer("{\"answer\":{\"recall\":{\"events\":[]}}}").is_none(),
        "the two answers stay apart by name"
    );
}

/// **A missed snapshot leg leaves the seam alive and the next ask drains
/// what arrived late**, per `weaver-harness-Spec` section 6 (Codex on
/// #94, round 4): the member answering past the bound costs the answer
/// leg, the late answer and the part of a line behind it are drained at
/// the retry, and the retry reads its own answer. Perturbation: keep the
/// dead flag on the missed answer and the retry answers `MemberDead`;
/// skip the drain and the retry acknowledges the late digest and misses
/// its finished leg.
#[test]
fn a_missed_snapshot_leg_is_retried_over_a_drained_seam() {
    use std::io::BufRead;
    let (ours, theirs) = UnixStream::pair().expect("pair");
    ours.set_nonblocking(true).expect("nonblocking");
    let mut seam = StateSeam::new(ours);
    seam.snapshot_answer_bound_ms = ANSWER_BOUND_MS;
    let (to_peer, at_peer) = std::sync::mpsc::channel::<()>();
    let (to_us, at_us) = std::sync::mpsc::channel::<()>();
    let peer = std::thread::spawn(move || {
        let mut reader = std::io::BufReader::new(theirs.try_clone().expect("clone"));
        let mut writer = theirs;
        let mut asked = Vec::new();
        let mut line = String::new();
        reader.read_line(&mut line).expect("the first ask");
        asked.push(line.clone());
        at_peer.recv().expect("told the bound expired");
        writer
            .write_all(
                concat!(
                    r#"{"answer":{"snapshot":{"ask":1,"save-point":"late.save-point","run":"r-1","sequence":1,"turn":1,"digest":"late"}}}"#,
                    "\n",
                    "{\"part"
                )
                .as_bytes(),
            )
            .expect("the late answer and a part of a line");
        to_us.send(()).expect("says so");
        line.clear();
        reader.read_line(&mut line).expect("the retry");
        asked.push(line.clone());
        writer
            .write_all(
                concat!(
                    r#"{"answer":{"snapshot":{"ask":2,"save-point":"fresh.save-point","run":"r-1","sequence":1,"turn":1,"digest":"fresh"}}}"#,
                    "\n"
                )
                .as_bytes(),
            )
            .expect("answers the retry");
        line.clear();
        reader.read_line(&mut line).expect("the acknowledgement");
        asked.push(line.clone());
        writer
            .write_all(
                b"{\"answer\":{\"finished\":{\"ask\":2,\"save-point\":\"fresh.save-point\"}}}\n",
            )
            .expect("finishes");
        asked
    });
    assert!(matches!(
        seam.ask_snapshot(),
        Err(weaver_types::SavePointLeg::Answer)
    ));
    to_peer.send(()).unwrap();
    at_us.recv().unwrap();
    let taken = seam
        .ask_snapshot()
        .expect("the retry is read as its own over the drained seam");
    assert_eq!(taken.stamp.digest, "fresh");
    let asked = peer.join().unwrap();
    assert_eq!(
        asked,
        vec![
            "{\"ask\":{\"snapshot\":{\"ask\":1}}}\n".to_string(),
            "{\"ask\":{\"snapshot\":{\"ask\":2}}}\n".to_string(),
            "{\"acknowledge\":{\"snapshot\":{\"ask\":2,\"digest\":\"fresh\"}}}\n".to_string(),
        ]
    );
}

/// **A late answer is dropped by its number, not by timing**, per
/// `weaver-harness-Spec` section 6 (Codex on #94, round 10): the member
/// answers the first ask only after the retry's ask has arrived, past
/// the drain, then answers the retry; the harness reads past the answer
/// carrying ask 1 to the one carrying ask 2 and acknowledges that one,
/// so a slow member never leaves the retry one answer behind.
/// Perturbation: ignore the ordinal and the stale answer is taken as the
/// retry's, the acknowledgement names "late" and the finished leg misses.
#[test]
fn a_late_answer_is_dropped_by_its_number_and_the_retrys_is_read() {
    use std::io::BufRead;
    let (ours, theirs) = UnixStream::pair().expect("pair");
    ours.set_nonblocking(true).expect("nonblocking");
    let mut seam = StateSeam::new(ours);
    seam.snapshot_answer_bound_ms = ANSWER_BOUND_MS;
    let peer = std::thread::spawn(move || {
        let mut reader = std::io::BufReader::new(theirs.try_clone().expect("clone"));
        let mut writer = theirs;
        let mut asked = Vec::new();
        let mut line = String::new();
        reader.read_line(&mut line).expect("the first ask");
        asked.push(line.clone());
        // Silent past the bound: the first ask misses.
        line.clear();
        reader.read_line(&mut line).expect("the retry");
        asked.push(line.clone());
        // The late answer to ask 1 lands after the retry's ask, past any
        // drain, then the retry's own.
        writer
            .write_all(
                concat!(
                    r#"{"answer":{"snapshot":{"ask":1,"save-point":"late.save-point","run":"r-1","sequence":1,"turn":1,"digest":"late"}}}"#,
                    "\n",
                    r#"{"answer":{"snapshot":{"ask":2,"save-point":"fresh.save-point","run":"r-1","sequence":1,"turn":1,"digest":"fresh"}}}"#,
                    "\n"
                )
                .as_bytes(),
            )
            .expect("answers late and then the retry");
        line.clear();
        reader.read_line(&mut line).expect("the acknowledgement");
        asked.push(line.clone());
        writer
            .write_all(
                b"{\"answer\":{\"finished\":{\"ask\":2,\"save-point\":\"fresh.save-point\"}}}\n",
            )
            .expect("finishes the retry's");
        asked
    });
    assert!(matches!(
        seam.ask_snapshot(),
        Err(weaver_types::SavePointLeg::Answer)
    ));
    let taken = seam
        .ask_snapshot()
        .expect("the retry reads past the late answer to its own");
    assert_eq!(taken.stamp.digest, "fresh");
    let asked = peer.join().unwrap();
    assert_eq!(
        asked[2],
        "{\"acknowledge\":{\"snapshot\":{\"ask\":2,\"digest\":\"fresh\"}}}\n"
    );
}

/// **A late `finished` frame of the ask before is dropped by its number
/// while the retry's answer is awaited**, per `weaver-harness-Spec`
/// section 6 (Codex on #94, round 11): the member finishes ask 1 only
/// after the retry's ask arrived, past the drain, then answers ask 2;
/// the harness drops the finished frame carrying ask 1 and reads ask
/// 2's answer. Perturbation: judge the awaited kind alone again and the
/// retry misses its answer leg on the finished frame.
#[test]
fn a_late_finished_frame_is_dropped_by_its_number_while_the_retry_is_awaited() {
    use std::io::BufRead;
    let (ours, theirs) = UnixStream::pair().expect("pair");
    ours.set_nonblocking(true).expect("nonblocking");
    let mut seam = StateSeam::new(ours);
    let peer = std::thread::spawn(move || {
        let mut reader = std::io::BufReader::new(theirs.try_clone().expect("clone"));
        let mut writer = theirs;
        let mut line = String::new();
        reader.read_line(&mut line).expect("the first ask");
        // Ask 1 is answered at once; its finished frame comes only after
        // the retry's ask.
        writer
            .write_all(
                concat!(
                    r#"{"answer":{"snapshot":{"ask":1,"save-point":"one.save-point","run":"r-1","sequence":1,"turn":1,"digest":"one"}}}"#,
                    "\n"
                )
                .as_bytes(),
            )
            .expect("answers ask 1");
        line.clear();
        reader
            .read_line(&mut line)
            .expect("the acknowledgement of ask 1");
        // Silent past the bound: the finished leg misses.
        line.clear();
        reader.read_line(&mut line).expect("the retry's ask");
        writer
            .write_all(
                concat!(
                    r#"{"answer":{"finished":{"ask":1,"save-point":"one.save-point"}}}"#,
                    "\n",
                    r#"{"answer":{"snapshot":{"ask":2,"save-point":"two.save-point","run":"r-1","sequence":1,"turn":1,"digest":"two"}}}"#,
                    "\n"
                )
                .as_bytes(),
            )
            .expect("finishes ask 1 late, then answers ask 2");
        line.clear();
        reader
            .read_line(&mut line)
            .expect("the acknowledgement of ask 2");
        writer
            .write_all(
                b"{\"answer\":{\"finished\":{\"ask\":2,\"save-point\":\"two.save-point\"}}}\n",
            )
            .expect("finishes ask 2");
        line
    });
    assert!(matches!(
        seam.ask_snapshot(),
        Err(weaver_types::SavePointLeg::Finished)
    ));
    let taken = seam
        .ask_snapshot()
        .expect("the retry reads past the late finished frame to its own answer");
    assert_eq!(taken.stamp.digest, "two");
    assert_eq!(
        peer.join().unwrap(),
        "{\"acknowledge\":{\"snapshot\":{\"ask\":2,\"digest\":\"two\"}}}\n"
    );
}

/// **A late snapshot-protocol frame is never another ask's answer**,
/// per `weaver-harness-Spec` section 6: the answer leg of snapshot ask
/// 1 misses, the seam settles, and only after the recall ask has
/// arrived does the member send the late answer for ask 1 and then the
/// recall's; the recall reads past the late frame to its own answer
/// inside the same bound and the seam stays alive for the snapshot ask
/// that follows. Perturbation: take the first line as the answer again
/// and the recall misses on the late frame, retiring the seam.
#[test]
fn a_late_snapshot_frame_is_dropped_by_a_later_ask_of_another_kind() {
    use std::io::BufRead;
    let (ours, theirs) = UnixStream::pair().expect("pair");
    ours.set_nonblocking(true).expect("nonblocking");
    let mut seam = StateSeam::new(ours);
    seam.snapshot_answer_bound_ms = ANSWER_BOUND_MS;
    let peer = std::thread::spawn(move || {
        let mut reader = std::io::BufReader::new(theirs.try_clone().expect("clone"));
        let mut writer = theirs;
        let mut asked = Vec::new();
        let mut line = String::new();
        reader.read_line(&mut line).expect("the snapshot ask");
        asked.push(line.clone());
        // Silent past the bound: the answer leg misses.
        line.clear();
        reader.read_line(&mut line).expect("the recall ask");
        asked.push(line.clone());
        writer
            .write_all(
                concat!(
                    r#"{"answer":{"snapshot":{"ask":1,"save-point":"late.save-point","run":"r-1","sequence":1,"turn":1,"digest":"late"}}}"#,
                    "\n",
                    r#"{"answer":{"recall":{"events":[]}}}"#,
                    "\n"
                )
                .as_bytes(),
            )
            .expect("answers ask 1 late, then the recall");
        line.clear();
        reader
            .read_line(&mut line)
            .expect("the second snapshot ask");
        asked.push(line.clone());
        writer
            .write_all(
                concat!(
                    r#"{"answer":{"snapshot":{"ask":2,"save-point":"two.save-point","run":"r-1","sequence":1,"turn":1,"digest":"two"}}}"#,
                    "\n"
                )
                .as_bytes(),
            )
            .expect("answers ask 2");
        line.clear();
        reader.read_line(&mut line).expect("the acknowledgement");
        writer
            .write_all(
                b"{\"answer\":{\"finished\":{\"ask\":2,\"save-point\":\"two.save-point\"}}}\n",
            )
            .expect("finishes ask 2");
        asked
    });
    assert!(matches!(
        seam.ask_snapshot(),
        Err(weaver_types::SavePointLeg::Answer)
    ));
    let recalled = seam
        .ask_recall(None)
        .expect("the recall reads past the late snapshot frame to its own answer");
    assert!(recalled.is_empty());
    let taken = seam
        .ask_snapshot()
        .expect("the seam is alive for the snapshot ask that follows");
    assert_eq!(taken.stamp.digest, "two");
    let asked = peer.join().unwrap();
    assert_eq!(asked[1], "{\"ask\":{\"recall\":{}}}\n");
}

/// **The `restored` ask has its own bound** (the operator's ruling of
/// 2026-10-08 on #99, N1): a member that answers `restored` two and a
/// half seconds after the ask, past the small asks' two, as a member
/// still seating a large image does, is read as answered. Perturbation:
/// wait it on `ANSWER_BOUND_MS` again and the ask answers nothing.
#[test]
fn a_restored_answer_past_the_small_bound_is_read() {
    use std::io::BufRead;
    assert_eq!(RESTORED_ANSWER_BOUND_MS, 120_000);
    let (ours, theirs) = UnixStream::pair().expect("pair");
    ours.set_nonblocking(true).expect("nonblocking");
    let mut seam = StateSeam::new(ours);
    let peer = std::thread::spawn(move || {
        let mut reader = std::io::BufReader::new(theirs.try_clone().expect("clone"));
        let mut writer = theirs;
        let mut line = String::new();
        reader.read_line(&mut line).expect("the restored ask");
        std::thread::sleep(std::time::Duration::from_millis(ANSWER_BOUND_MS + 500));
        writer
            .write_all(b"{\"answer\":{\"restored\":{}}}\n")
            .expect("answers past the small bound");
    });
    assert_eq!(seam.ask_restored(), Some(RestoredAnswer::Empty));
    peer.join().unwrap();
}

/// **The snapshot's answer leg has its own bound**, per
/// `weaver-harness-Spec` section 6 on the operator's ruling of
/// 2026-10-08: a member that answers the snapshot two and a half seconds
/// after the ask, past the small asks' two, is read as taken, the image
/// being written before the answer. Perturbation: wait the answer leg
/// on `ANSWER_BOUND_MS` again and the leg misses as `Answer`.
#[test]
fn a_snapshot_answered_past_the_small_bound_is_taken() {
    use std::io::BufRead;
    assert_eq!(SNAPSHOT_ANSWER_BOUND_MS, 120_000);
    let (ours, theirs) = UnixStream::pair().expect("pair");
    ours.set_nonblocking(true).expect("nonblocking");
    let mut seam = StateSeam::new(ours);
    let peer = std::thread::spawn(move || {
        let mut reader = std::io::BufReader::new(theirs.try_clone().expect("clone"));
        let mut writer = theirs;
        let mut line = String::new();
        reader.read_line(&mut line).expect("the snapshot ask");
        std::thread::sleep(std::time::Duration::from_millis(ANSWER_BOUND_MS + 500));
        writer
            .write_all(
                concat!(
                    r#"{"answer":{"snapshot":{"ask":1,"save-point":"slow.save-point","run":"r-1","sequence":1,"turn":1,"digest":"slow"}}}"#,
                    "\n"
                )
                .as_bytes(),
            )
            .expect("answers past the small bound");
        line.clear();
        reader.read_line(&mut line).expect("the acknowledgement");
        writer
            .write_all(
                b"{\"answer\":{\"finished\":{\"ask\":1,\"save-point\":\"slow.save-point\"}}}\n",
            )
            .expect("finishes");
    });
    let taken = seam
        .ask_snapshot()
        .expect("the slow answer is inside the answer leg's own bound");
    assert_eq!(taken.stamp.digest, "slow");
    peer.join().unwrap();
}
