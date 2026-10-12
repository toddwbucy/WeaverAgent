use super::*;

const LINE: &str = concat!(
    r#"{"session":"alpha-1","run":"r-1","turn":"t-1","kind":"turn.closed","#,
    r#""sequence":"7","subsystem":"harness","wall_ms":1,"monotonic_ns":"2","#,
    r#""payload":{"close":"clean","request":{"sampling":{"temperature":0.70}}}}"#
);

/// **A `save_point` line never crosses the tee**, under the default
/// election, under `all_kinds`, and under an election naming the kind,
/// per `weaver-trace-PRD` section 3. Perturbation: drop the kind check
/// from `distill` and the `all_kinds` case distills.
#[test]
fn a_save_point_never_crosses_the_tee() {
    const SAVE_POINT: &str = concat!(
        r#"{"session":"alpha-1","run":"r-1","kind":"save_point","sequence":"9","#,
        r#""subsystem":"harness","wall_ms":1,"monotonic_ns":"2","#,
        r#""payload":{"save_point":"ab","run":"r-1","sequence":8,"turn":1,"name":"ab.save-point"}}"#
    );
    assert!(distill(SAVE_POINT, &Election::default()).is_none());
    let naming = Election {
        all_kinds: true,
        keys: vec![ElectedKind {
            kind: "save_point".into(),
            paths: vec!["save_point".into()],
        }],
    };
    assert!(distill(SAVE_POINT, &naming).is_none());
    assert!(distill(LINE, &naming).is_some(), "other kinds still cross");
}

/// The default election: the envelope of every kind and nothing more.
/// The frame carries the five, spelled as the canonical form spelled
/// them, and an empty pairs object.
#[test]
fn the_default_election_is_the_envelope_of_every_kind() {
    let frame = distill(LINE, &Election::default()).expect("distills");
    assert_eq!(
        frame,
        concat!(
            r#"{"envelope":{"session":"alpha-1","run":"r-1","turn":"t-1","#,
            r#""kind":"turn.closed","sequence":"7"},"pairs":{}}"#,
            "\n"
        )
    );
}

/// An elected payload key path crosses with the value the canonical
/// JSON held at it, byte for byte, nesting included, and a path the
/// payload does not hold is simply absent. The temperature is spelled
/// `0.70`, which a re-rendering would collapse to `0.7`, so the
/// byte-comparison below is what holds the projection to splicing.
#[test]
fn elected_paths_cross_verbatim_and_misses_cost_nothing() {
    let election = Election {
        all_kinds: true,
        keys: vec![ElectedKind {
            kind: "turn.closed".into(),
            paths: vec![
                "close".into(),
                "request.sampling".into(),
                "absent.path".into(),
            ],
        }],
    };
    let frame = distill(LINE, &election).expect("distills");
    assert!(frame.contains(r#""close":"clean""#), "{frame}");
    assert!(
        frame.contains(r#""request.sampling":{"temperature":0.70}"#),
        "{frame}"
    );
    assert!(!frame.contains("absent"), "{frame}");
}

/// With `all_kinds` down, a kind the election does not name does not
/// cross at all, and costs nothing.
#[test]
fn an_unelected_kind_is_not_distilled() {
    let election = Election {
        all_kinds: false,
        keys: vec![ElectedKind {
            kind: "turn.started".into(),
            paths: Vec::new(),
        }],
    };
    assert!(distill(LINE, &election).is_none());
}

/// The sequence crosses as the canonical form spells it, a string. A
/// line spelling it as a number is not the canonical form and is not
/// distilled, which is the pin the first live stand was missing.
#[test]
fn a_numeric_sequence_is_refused() {
    let line = r#"{"session":"s","run":"r","kind":"load","sequence":0,"wall_ms":1}"#;
    assert!(distill(line, &Election::default()).is_none());
}

/// An identity string carrying a JSON escape still distills: the
/// session's spelling is the operator's, and a borrowed-only parse
/// would refuse it.
#[test]
fn an_escaped_identity_still_distills() {
    let line = r#"{"session":"s\"x","run":"r","kind":"load","sequence":"0"}"#;
    let frame = distill(line, &Election::default()).expect("distills");
    assert!(frame.contains(r#""session":"s\"x""#), "{frame}");
}

/// A turnless event distills without a turn member, mirroring the
/// canonical form's own omission.
#[test]
fn the_seated_prefix_distills_whole_under_every_election() {
    // **Perturbation:** consult the election before the prefix arm and
    // the narrowed election below refuses the line, or drop the
    // turnless condition and the turned line distills too.
    const PREFIX: &str = concat!(
        r#"{"session":"alpha-1","run":"r-1","kind":"message.system","#,
        r#""sequence":"3","subsystem":"harness","wall_ms":1,"monotonic_ns":"2","#,
        r#""payload":{"role":"system","content":[{"type":"text","text":"You are Karl."}]}}"#
    );
    let narrowed = Election {
        all_kinds: false,
        keys: vec![ElectedKind {
            kind: "turn.closed".into(),
            paths: vec![],
        }],
    };
    let frame = distill(PREFIX, &narrowed).expect("the prefix crosses");
    assert_eq!(
        frame,
        concat!(
            r#"{"envelope":{"session":"alpha-1","run":"r-1","kind":"message.system","#,
            r#""sequence":"3"},"pairs":{"content":[{"type":"text","text":"You are Karl."}],"#,
            r#""role":"system"}}"#,
            "\n"
        ),
        "one pair per top-level member, the value verbatim, under an election naming no such kind"
    );
    let turned = PREFIX.replace(r#""run":"r-1","#, r#""run":"r-1","turn":"t-1","#);
    assert!(
        distill(&turned, &narrowed).is_none(),
        "a system message inside a turn is the election's to keep or drop"
    );
}

/// **A restored message distills whole under every election**, as the
/// seated prefix does (#697): it is the conversation a branch's run opened
/// under, and a restore from the branch rebuilds it from what custody
/// holds.
///
/// Perturbation: drop `message.restored` from the whole-distill arm and
/// the narrowed election below refuses the line.
#[test]
fn a_restored_message_distills_whole_under_every_election() {
    const RESTORED: &str = concat!(
        r#"{"session":"alpha-2","run":"r-1","kind":"message.restored","#,
        r#""sequence":"4","subsystem":"harness","wall_ms":1,"monotonic_ns":"3","#,
        r#""payload":{"role":"user","content":[{"type":"text","text":"inherited"}]}}"#
    );
    let narrowed = Election {
        all_kinds: false,
        keys: vec![ElectedKind {
            kind: "turn.closed".into(),
            paths: vec![],
        }],
    };
    let frame = distill(RESTORED, &narrowed).expect("the restored message crosses");
    assert_eq!(
        frame,
        concat!(
            r#"{"envelope":{"session":"alpha-2","run":"r-1","kind":"message.restored","#,
            r#""sequence":"4"},"pairs":{"content":[{"type":"text","text":"inherited"}],"#,
            r#""role":"user"}}"#,
            "\n"
        ),
        "one pair per top-level member under an election naming no such kind"
    );
}

#[test]
fn a_turnless_event_stays_turnless() {
    let line = r#"{"session":"s","run":"r","kind":"run.load","sequence":"0","wall_ms":1}"#;
    let frame = distill(line, &Election::default()).expect("distills");
    assert!(!frame.contains("turn"), "{frame}");
}
