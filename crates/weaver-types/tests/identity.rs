//! conforms: types-denial-precedes-permission
//!
//! The denial-precedence test of `weaver-types-Spec` section 3, derived from the
//! threat walk: the adversary is a process on the host that is not a front-end
//! principal, and admission rests on identity rather than reachability.

use std::collections::BTreeSet;

use weaver_types::{AccessRule, PeerIdentity, authorized};

fn rule(allowed_uids: &[u32], allowed_gids: &[u32], denied_uids: &[u32]) -> AccessRule {
    AccessRule {
        allowed_uids: BTreeSet::from_iter(allowed_uids.iter().copied()),
        allowed_gids: BTreeSet::from_iter(allowed_gids.iter().copied()),
        denied_uids: BTreeSet::from_iter(denied_uids.iter().copied()),
    }
}

/// `authorized` returns false for a peer whose uid is the agent's against a
/// rule whose allow sets include the agent's group.
///
/// Perturbation: reorder the predicate so permission is evaluated first and
/// this returns true - the agent's group membership readmits the one principal
/// the boundary exists to keep out. Watched to fail under exactly that
/// reordering.
#[test]
fn denial_precedes_permission() {
    let agent = PeerIdentity {
        uid: 1701,
        gid: 900,
        pid: 4242,
    };
    let boundary = rule(&[], &[900], &[1701]);
    assert!(!authorized(&agent, &boundary));

    let front_end = PeerIdentity {
        uid: 1000,
        gid: 900,
        pid: 5555,
    };
    assert!(authorized(&front_end, &boundary));
}

/// Admission is by uid or by group, and a peer in neither is refused.
#[test]
fn unlisted_peer_is_refused() {
    let boundary = rule(&[1000], &[], &[]);
    assert!(authorized(
        &PeerIdentity {
            uid: 1000,
            gid: 1,
            pid: 1
        },
        &boundary
    ));
    assert!(!authorized(
        &PeerIdentity {
            uid: 1001,
            gid: 1,
            pid: 1
        },
        &boundary
    ));
}

/// The pid never enters the decision: two peers differing only by pid judge
/// identically, because a pid is reused and is unsound as an authorization
/// input.
#[test]
fn pid_is_never_the_basis_of_a_decision() {
    let boundary = rule(&[1000], &[], &[]);
    let first = PeerIdentity {
        uid: 1000,
        gid: 1,
        pid: 10,
    };
    let second = PeerIdentity {
        uid: 1000,
        gid: 1,
        pid: 20,
    };
    assert_eq!(
        authorized(&first, &boundary),
        authorized(&second, &boundary)
    );
}

/// **Every line the trace stream adds is one fixed shape**, per
/// `weaver-types-Spec` section 3.1: an object whose one member is
/// `trace_stream`, holding the case externally tagged, exactly the three lines
/// the Spec publishes. Perturbation: serialize a bare `TraceControl` in place
/// of the `TraceLine` and the header renders `{"header":...}`, which no reader
/// built from the published lines parses.
#[test]
fn the_trace_stream_lines_are_the_published_shape() {
    use weaver_types::{TraceControl, TraceHeader, TraceLine};
    let line = |control| {
        serde_json::to_string(&TraceLine {
            trace_stream: control,
        })
        .unwrap()
    };
    assert_eq!(
        line(TraceControl::Header(TraceHeader {
            device: 2049,
            inode: 77,
            birth_ns: 1_759_500_000_123_456_789,
        })),
        r#"{"trace_stream":{"header":{"device":2049,"inode":77,"birth_ns":1759500000123456789}}}"#
    );
    assert_eq!(
        line(TraceControl::Heartbeat { wall_ms: 5 }),
        r#"{"trace_stream":{"heartbeat":{"wall_ms":5}}}"#
    );
    assert_eq!(
        line(TraceControl::Truncated { size: 9 }),
        r#"{"trace_stream":{"truncated":{"size":9}}}"#
    );
    let back: TraceLine =
        serde_json::from_str(r#"{"trace_stream":{"heartbeat":{"wall_ms":5}}}"#).unwrap();
    assert_eq!(back.trace_stream, TraceControl::Heartbeat { wall_ms: 5 });
}

/// **The request is offset and prior digest, the digest absent at offset
/// zero, and nothing else**, per the same section. Perturbation: drop
/// `deny_unknown_fields` from `TraceRequest` and the request carrying an extra
/// member parses, so a reader's misspelling would read as a fresh start.
#[test]
fn the_trace_request_admits_its_two_members_only() {
    use weaver_types::TraceRequest;
    let first: TraceRequest = serde_json::from_str(r#"{"offset":0}"#).unwrap();
    assert_eq!(first.prior_digest, None);
    assert_eq!(
        serde_json::to_string(&first).unwrap(),
        r#"{"offset":0}"#,
        "an absent digest is absent, never null"
    );
    assert!(
        serde_json::from_str::<TraceRequest>(r#"{"offset":10,"prior_digst":"ab"}"#).is_err(),
        "an unknown member refuses"
    );
}

/// **The boundary file names one trace reader and refuses an unknown key**,
/// per `weaver-types-Spec` section 3.1, read in its kebab-case spelling.
/// Perturbation: drop `deny_unknown_fields` from `BoundaryFile` and a file
/// still carrying the retired lifecycle half parses as if it were current.
#[test]
fn the_boundary_file_holds_one_reader_and_no_other_key() {
    use weaver_types::BoundaryFile;
    let file: BoundaryFile =
        serde_json::from_str(r#"{"trace-reader":"weaver-a-admincon"}"#).unwrap();
    assert_eq!(file.trace_reader, "weaver-a-admincon");
    assert!(
        serde_json::from_str::<BoundaryFile>(
            r#"{"trace-reader":"weaver-a-admincon","roles":{"operator":["load"]}}"#
        )
        .is_err(),
        "the retired lifecycle half refuses"
    );
}
