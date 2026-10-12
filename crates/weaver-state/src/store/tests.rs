use super::*;

/// **The numbered snapshot frames parse whole or not at all** (Codex on
/// #94, round 10): the ask with its ordinal, the acknowledgement with
/// its ordinal and digest; an ask without the ordinal, or an
/// acknowledgement with one member, is not this ask.
#[test]
fn the_numbered_snapshot_frames_parse_whole_or_not_at_all() {
    assert_eq!(
        parse_ask(r#"{"ask":{"snapshot":{"ask":2}}}"#),
        Some(Ask::Snapshot { ordinal: 2 })
    );
    assert_eq!(parse_ask(r#"{"ask":{"snapshot":{}}}"#), None);
    assert_eq!(
        parse_ask(r#"{"acknowledge":{"snapshot":{"ask":2,"digest":"ab"}}}"#),
        Some(Ask::Acknowledge {
            ordinal: 2,
            digest: "ab".into()
        })
    );
    assert_eq!(
        parse_ask(r#"{"acknowledge":{"snapshot":{"digest":"ab"}}}"#),
        None
    );
    // **An extra member refuses as a missing one does** (the #99 area 1
    // review, T16): the acknowledgement's body, its outer object, the
    // whole frame and the snapshot ask's body each carry exactly their
    // members. Perturbations: drop any one member-count guard and its
    // frame parses.
    for frame in [
        r#"{"acknowledge":{"snapshot":{"ask":2,"digest":"ab","extra":1}}}"#,
        r#"{"acknowledge":{"snapshot":{"ask":2,"digest":"ab"},"other":{}}}"#,
        r#"{"acknowledge":{"snapshot":{"ask":2,"digest":"ab"}},"ask":{"shape":{}}}"#,
        r#"{"ask":{"snapshot":{"ask":2,"extra":1}}}"#,
        r#"{"acknowledge":{"snapshot":{"ask":"2","digest":"ab"}}}"#,
    ] {
        assert_eq!(parse_ask(frame), None, "{frame}");
    }
    assert!(
        render_finished_answer("x.save-point", 2)
            .starts_with(r#"{"answer":{"finished":{"ask":2,"save-point":"x.save-point"}}}"#)
    );
}
