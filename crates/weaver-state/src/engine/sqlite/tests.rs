use super::*;
use crate::store::*;

/// **A distillate lands whole or not at all**, the watch the embedded
/// engine owes since the service engine retired (`weaver-state-Spec`
/// section 10): a failure forced inside the landing's transaction, here a
/// trigger refusing one field row in place of the loop schema's
/// constraint that will be the natural lever, must refuse the landing and
/// leave the holdings unchanged, the event row rolled back with the field
/// rows. Perturbation: commit the event before its rows and the refused
/// landing leaves its event behind.
#[test]
fn a_distillate_lands_whole_or_not_at_all() {
    let mut store = Sqlite::stand().expect("opens");
    store
        .connection
        .execute_batch(
            "CREATE TRIGGER refuse_poison BEFORE INSERT ON field \
             WHEN NEW.key = 'poison' BEGIN SELECT RAISE(ABORT, 'refused'); END;",
        )
        .expect("the lever stands");
    let count = |store: &Sqlite, sql: &str| -> i64 {
        store
            .connection
            .query_row(sql, [], |row| row.get(0))
            .expect("counts")
    };
    store
        .land(&Distillate {
            session: "s".into(),
            run: "r".into(),
            turn: None,
            kind: "load".into(),
            sequence: 0,
            pairs: vec![("kept".into(), "1".into())],
        })
        .expect("a sound distillate lands");
    let refused = store.land(&Distillate {
        session: "s".into(),
        run: "r".into(),
        turn: None,
        kind: "load".into(),
        sequence: 1,
        pairs: vec![("fine".into(), "1".into()), ("poison".into(), "2".into())],
    });
    // **A constraint's refusal is the schema's, never the store's** (the
    // #99 area 1 grade of N2). Perturbation: answer every error
    // `LandingFailed` and this reads the store's fault.
    assert!(
        matches!(refused, Err(CustodyFault::LandingRefused(_))),
        "the schema's refusal: {refused:?}"
    );
    assert_eq!(
        count(&store, "SELECT COUNT(*) FROM event"),
        1,
        "the refused distillate's event rolled back with its rows"
    );
    assert_eq!(
        count(&store, "SELECT COUNT(*) FROM field"),
        1,
        "and none of its field rows stand"
    );
}

/// **A NUL in a message lands verbatim**, the shared test run on this
/// engine and its tables read: the content is a `field` row and no part
/// stands. Perturbation: drop the holdable check from `typed::split`, and
/// this engine holds the part typed, so its count fails.
#[test]
fn a_nul_in_a_message_lands_verbatim() {
    let mut store = Sqlite::stand().expect("opens");
    super::super::a_nul_in_a_message_lands_verbatim_and_serves_whole(&mut store);
    let count = |sql: &str| -> i64 {
        store
            .connection
            .query_row(sql, [], |row| row.get(0))
            .expect("counts")
    };
    assert_eq!(
        count("SELECT COUNT(*) FROM field WHERE key = 'content'"),
        1,
        "the content is held verbatim"
    );
    assert_eq!(count("SELECT COUNT(*) FROM part"), 0, "no part is typed");
}

/// **A recorded line of each typed kind lands typed and serves what the
/// record reads**, the shared test run on this engine and then its tables
/// read for the typed rows. Perturbation: make the split type nothing, so
/// every pair lands verbatim, and the answers still match while the part,
/// message and series counts read zero, which is the test failing on the
/// property rather than on the bytes.
#[test]
fn recorded_lines_land_typed() {
    let mut store = Sqlite::stand().expect("opens");
    super::super::recorded_lines_land_typed_and_serve_what_the_record_reads(&mut store);
    let count = |sql: &str| -> i64 {
        store
            .connection
            .query_row(sql, [], |row| row.get(0))
            .expect("counts")
    };
    assert_eq!(
        count("SELECT COUNT(*) FROM part"),
        3,
        "one part per message"
    );
    assert_eq!(
        count("SELECT COUNT(*) FROM message WHERE role IS NOT NULL AND parts = 1"),
        3
    );
    assert_eq!(count("SELECT COUNT(*) FROM series"), 4, "two readings each");
    assert_eq!(
        count("SELECT COUNT(*) FROM measurement WHERE perplexity IS NULL"),
        1,
        "the absent perplexity is held absent, not zero"
    );
    assert_eq!(
        count(
            "SELECT COUNT(*) FROM field \
             WHERE key IN ('role', 'content', 'perplexity', 'entropies', 'surprisals')"
        ),
        0,
        "no typed member is also held verbatim"
    );
    assert_eq!(
        count("SELECT COUNT(*) FROM field WHERE key = 'input_tokens'"),
        2,
        "a member no Spec names lands verbatim"
    );
}

#[test]
fn raw_objects_survive_the_engine_and_answers() {
    let mut store = Sqlite::stand().expect("opens");
    super::super::raw_objects_survive_the_engine_and_answers(&mut store);
}

/// **The identity ask serves the turnless system messages and no
/// other**, in landing order, with the prefix's pairs. Perturbation:
/// drop `turn IS NULL` from the query and the turned system message
/// joins the answer; drop the kind and the user message does.
#[test]
fn the_identity_ask_serves_the_seated_prefix_alone() {
    let mut store = Sqlite::stand().expect("opens");
    let land = |store: &mut Sqlite, turn: Option<&str>, kind: &str, seq: i64, text: &str| {
        store
            .land(&Distillate {
                session: "s".into(),
                run: "r-1".into(),
                turn: turn.map(str::to_string),
                kind: kind.into(),
                sequence: seq,
                pairs: vec![
                    ("role".into(), "\"system\"".into()),
                    (
                        "content".into(),
                        format!("[{{\"type\":\"text\",\"text\":\"{text}\"}}]"),
                    ),
                ],
            })
            .expect("lands");
    };
    land(&mut store, None, "message.system", 1, "You are Karl.");
    land(&mut store, None, "message.system", 2, "Answer briefly.");
    land(
        &mut store,
        Some("t-1"),
        "message.system",
        3,
        "inside a turn",
    );
    land(&mut store, Some("t-1"), "message.user", 4, "hello");
    let held = store.identity("s").expect("answers");
    assert_eq!(held.len(), 2);
    assert_eq!(held[0].sequence, 1);
    assert_eq!(held[1].sequence, 2);
    // A second load records the prefix it seated under its own run, and
    // the answer is that run's alone. Perturbation: drop the run
    // subquery and the answer holds three.
    store
        .land(&Distillate {
            session: "s".into(),
            run: "r-2".into(),
            turn: None,
            kind: "message.system".into(),
            sequence: 1,
            pairs: vec![
                ("role".into(), "\"system\"".into()),
                ("content".into(), "[]".into()),
            ],
        })
        .expect("lands");
    let newest = store.identity("s").expect("answers");
    assert_eq!(newest.len(), 1, "the newest run's prefix alone");
    assert_eq!(newest[0].run, "r-2");
    assert!(
        held.iter()
            .all(|e| e.turn.is_none() && e.kind == "message.system")
    );
    assert!(
        held[0]
            .pairs
            .contains(&("role".to_string(), "\"system\"".to_string())),
        "the seated prefix carries its role pair: {:?}",
        held[0].pairs
    );
    assert!(
        store.identity("other").expect("answers").is_empty(),
        "an empty list is an answer"
    );
    assert!(matches!(
        parse_ask("{\"ask\":{\"identity\":{}}}"),
        Some(Ask::Identity)
    ));
    let frame = render_identity_answer(&held);
    assert!(frame.starts_with("{\"answer\":{\"identity\":{\"messages\":[{\"envelope\":"));
    assert!(
        frame.contains("\"role\":\"system\""),
        "pairs render as JSON, not as strings"
    );
}

/// **The position is the last landing and its run's last turn**, per
/// `weaver-state-Spec` section 3's stamp: by landing order and never by
/// the sequence's size, zero where the run holds no turn, and none where
/// nothing has landed. **The schema leaves the elected indexes out**, so
/// a later load's differing election is not a schema that disagrees.
/// Perturbation: order the position query by `sequence` and the second
/// run's lower sequence stops being the position; include the elected
/// indexes in the schema text and the two digests differ.
#[test]
fn the_position_is_the_last_landing_and_the_schema_omits_elections() {
    let mut store = Sqlite::stand().expect("stands");
    assert_eq!(store.position().expect("reads"), None, "nothing landed");
    let before = store.schema().expect("reads");
    store
        .index_election(&Election {
            all_kinds: true,
            keys: vec![("turn.closed".into(), vec!["close".into()])],
        })
        .expect("indexes");
    assert_eq!(
        store.schema().expect("reads"),
        before,
        "an elected index is a load's and not the schema's"
    );
    assert!(before.contains("table event"), "{before}");
    assert!(before.contains("index part_event"), "{before}");
    let mut turned = landed("s", "r-1", "message.user", 7);
    turned.turn = Some("t-3".into());
    store.land(&turned).expect("lands");
    store.land(&landed("s", "r-1", "flush", 9)).expect("lands");
    let stamp = store.position().expect("reads").expect("a position");
    assert_eq!(
        stamp,
        crate::save_point::Stamp {
            run: "r-1".into(),
            sequence: 9,
            turn: 3
        },
        "the last landing's run and sequence, and that run's last turn"
    );
    store.land(&landed("s", "r-2", "load", 0)).expect("lands");
    let stamp = store.position().expect("reads").expect("a position");
    assert_eq!(
        stamp,
        crate::save_point::Stamp {
            run: "r-2".into(),
            sequence: 0,
            turn: 0
        },
        "by landing order, and a run with no turn reads zero"
    );
    // Another session's run of the same name holds a turn; the stamp
    // of this session's turnless run must not borrow it. Perturbation:
    // drop `session` from the turn lookup and this reads 5.
    let mut foreign = landed("other", "r-3", "message.user", 1);
    foreign.turn = Some("t-5".into());
    store.land(&foreign).expect("lands");
    store.land(&landed("s", "r-3", "load", 0)).expect("lands");
    let stamp = store.position().expect("reads").expect("a position");
    assert_eq!(
        (stamp.run.as_str(), stamp.turn),
        ("r-3", 0),
        "the turn is the last event's session's and never a namesake run's"
    );
    // **A value the stamp cannot represent refuses and never defaults**:
    // a turn key that is not `t-<n>`, and a sequence below zero, each
    // refuse the position. Perturbation: default either to zero and the
    // refusal becomes a stamp.
    let mut odd = landed("s", "r-4", "message.user", 1);
    odd.turn = Some("turn-one".into());
    store.land(&odd).expect("the port lands what it is handed");
    assert!(
        store.position().is_err(),
        "a turn key that is not t-<n> refuses the position"
    );
    store.land(&landed("s", "r-5", "load", -1)).expect("lands");
    assert!(store.position().is_err(), "a sequence below zero refuses");
}

/// **The holdings survive the process by image and nothing else**, per
/// `weaver-state-Spec` section 3: a store stood from the image a first
/// store gave holds its rows, the typed rows and the elected index with
/// them, and a failed adoption leaves the live holdings standing.
/// Perturbation: make `adopt` ignore its error and the poisoned image
/// empties the store, the last assertion failing.
#[test]
fn a_distillate_lands_whole_and_survives_by_image() {
    let mut store = Sqlite::stand().expect("opens");
    let election = Election {
        all_kinds: true,
        keys: vec![("turn.started".into(), vec!["payload.close".into()])],
    };
    store
        .index_election(&election)
        .expect("the election indexes");
    let distillate = Distillate {
        session: "alpha-1".into(),
        run: "2026-08-18T19:03:31.198Z-alpha-7d53a936e".into(),
        turn: Some("t-1".into()),
        kind: "turn.started".into(),
        sequence: 4,
        pairs: vec![("payload.close".into(), "\"clean\"".into())],
    };
    store.land(&distillate).expect("lands");
    assert_eq!(store.held().expect("held"), 1);
    let image = store.image().expect("serializes");
    drop(store);
    let mut restored = Sqlite::stand().expect("stands empty");
    assert_eq!(restored.held().expect("held"), 0);
    // **A restore judges the image once** (the operator's ruling of
    // 2026-10-08 on #99, N1): the member judges before it commits, so
    // the commit runs no second whole-image check. Perturbation: judge
    // again in `adopt` and the count reads two.
    JUDGED.with(|count| count.set(0));
    restored.judge_image(&image, "alpha-1").expect("judged");
    restored.adopt(&image, &election).expect("adopts");
    assert_eq!(
        JUDGED.with(std::cell::Cell::get),
        1,
        "one judgment per restore"
    );
    // **The adopted store has no one-gibibyte cap** (N2): its size limit
    // is lifted past the save point's bound, as a first run's store has
    // none. Perturbation: drop the lift and the limit reads 1073741824.
    assert_eq!(size_limit(&restored.connection), i64::MAX);
    assert_eq!(
        restored.held().expect("held"),
        1,
        "holdings survive the process by save point, per the charter"
    );
    assert_eq!(
        restored.replay("alpha-1").expect("replays")[0].pairs,
        distillate.pairs,
        "served as it crossed"
    );
    let elected: i64 = restored
        .connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master
                 WHERE type = 'index' AND name LIKE 'field_elected_%'",
            [],
            |row| row.get(0),
        )
        .expect("counts");
    assert_eq!(
        elected, 1,
        "adopted under the same election, that election's index stands"
    );
    // The image says of itself what the store it came from says, the
    // elected index left out of the schema. Perturbation: read the
    // schema from the live connection in `judge_image` and the extra
    // table below goes unseen.
    let facts = restored.judge_image(&image, "alpha-1").expect("judged");
    assert_eq!(facts.schema, restored.schema().expect("schema"));
    assert_eq!(facts.position, restored.position().expect("position"));
    assert_eq!(
        facts.identity,
        restored.identity("alpha-1").expect("identity"),
        "the prefix is read from the scratch copy as the live store serves it"
    );
    let mut altered = Sqlite::stand().expect("stands");
    altered.adopt(&image, &Election::default()).expect("adopts");
    altered
        .connection
        .execute_batch("CREATE TABLE extra (x)")
        .expect("alters");
    let altered_image = altered.image().expect("serializes");
    assert_ne!(
        restored
            .judge_image(&altered_image, "alpha-1")
            .expect("judged")
            .schema,
        facts.schema,
        "an image with another catalog says so"
    );
    assert!(
        restored
            .adopt(b"not an image", &Election::default())
            .is_err(),
        "bytes that are no database are refused"
    );
    assert_eq!(
        restored.held().expect("held"),
        1,
        "and the holdings stand after the refusal"
    );
    // **The swap is a commit step**: an election whose index the engine
    // cannot build fails before anything moves, so the holdings stand,
    // the second image's row never arriving. Perturbation: swap the
    // image in before building the election and the count reads two.
    let mut second = Sqlite::stand().expect("stands");
    second.adopt(&image, &Election::default()).expect("adopts");
    second.land(&distillate).expect("lands a second row");
    let two_rows = second.image().expect("serializes");
    let poisoned = Election {
        all_kinds: true,
        keys: vec![("turn.started".into(), vec!["a\u{0}b".into()])],
    };
    assert!(
        restored.adopt(&two_rows, &poisoned).is_err(),
        "the poisoned election fails the build"
    );
    assert_eq!(
        restored.held().expect("held"),
        1,
        "and the live holdings never moved"
    );
    // **The exemption is exact**: an index in the election's generated
    // form is left out of the schema, and a table or a trigger under
    // the elected prefix, or an index of another shape, is schema.
    // Perturbation: exempt by prefix alone and the three hidden objects
    // go unseen.
    let standing = restored.schema().expect("schema");
    for hidden in [
        "CREATE TABLE field_elected_7a (x)",
        "CREATE TRIGGER field_elected_7b BEFORE INSERT ON field BEGIN SELECT 1; END",
        "CREATE UNIQUE INDEX field_elected_7c ON field (key)",
    ] {
        let mut connection = rusqlite::Connection::open_in_memory().unwrap();
        connection
            .deserialize_read_exact(rusqlite::MAIN_DB, &image[..], image.len(), false)
            .unwrap();
        connection.execute_batch(hidden).unwrap();
        let hiding = connection.serialize(rusqlite::MAIN_DB).unwrap().to_vec();
        assert_ne!(
            restored
                .judge_image(&hiding, "alpha-1")
                .expect("judged")
                .schema,
            standing,
            "{hidden} is schema"
        );
    }
    assert_eq!(
        elected_index_form(&elected_index_name("payload.close")).as_deref(),
        Some(
            "CREATE INDEX field_elected_7061796c6f61642e636c6f7365 ON field (key, value) \
             WHERE key = 'payload.close'"
        )
    );
    assert_eq!(elected_index_form("field_elected_zz"), None);
    assert_eq!(elected_index_form("other"), None);
    // **The index set after adoption is exactly the active election's**,
    // per the ruling of 2026-10-06: an image carrying another election's
    // generated indexes comes out with only this load's. Perturbation:
    // skip `drop_elected_indexes` in `adopt` and the old index stands
    // beside the new one.
    let other = Election {
        all_kinds: true,
        keys: vec![("turn.closed".into(), vec!["other.path".into()])],
    };
    let mut relected = Sqlite::stand().expect("stands");
    relected.adopt(&image, &other).expect("adopts");
    let names: Vec<String> = relected
        .connection
        .prepare("SELECT name FROM sqlite_master WHERE type = 'index' AND name LIKE 'field_elected_%' ORDER BY name")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        names,
        vec![elected_index_name("other.path")],
        "only the active election's index stands"
    );
}

/// A later load's differing election builds its own index rather than
/// falling silently under an earlier load's name, which is what a
/// positional index name would allow under `IF NOT EXISTS`.
#[test]
fn a_changed_election_builds_its_own_indexes() {
    let mut store = Sqlite::stand().expect("opens");
    let elect = |key: &str| Election {
        all_kinds: true,
        keys: vec![("turn.closed".into(), vec![key.into()])],
    };
    store.index_election(&elect("close")).expect("first");
    store.index_election(&elect("tokens")).expect("second");
    let elected: i64 = store
        .connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master
                 WHERE type = 'index' AND name LIKE 'field_elected_%'",
            [],
            |row| row.get(0),
        )
        .expect("counts");
    assert_eq!(elected, 2, "each key path owns its index");
}

fn landed(session: &str, run: &str, kind: &str, sequence: i64) -> Distillate {
    Distillate {
        session: session.into(),
        run: run.into(),
        turn: None,
        kind: kind.into(),
        sequence,
        pairs: Vec::new(),
    }
}

/// **Custody answers within its session and not across it**, per
/// `weaver-state-Spec` section 4 and `weaver-state-PRD` section 4's
/// boundary. A store file holding more than one session is the normal
/// case: sessions outlive runs and the file outlives sessions, so both
/// serve queries bound to the session the opener declared.
///
/// The defect this pins was invisible in exactly the way that matters.
/// Unbounded, both queries answered over every session the file held
/// and every answer looked well formed - a shape ask reporting a
/// lifetime's runs as this session's, and a recall reaching a fact the
/// operator believed a session cut had retired.
///
/// Perturbation: drop any of the three `WHERE session` predicates and
/// this fails. Dropping the shape's or the recall's event predicate
/// surfaces the older session's run and message in the newer session's
/// answers. Dropping the turn-selection subquery's spends the
/// `last-turns` bound on an older session's turn and leaves the bounded
/// recall empty - fail-closed, because the event predicate still holds,
/// but the answer is wrong either way.
#[test]
fn the_answers_stay_inside_the_running_session() {
    let mut store = Sqlite::stand().expect("opens");
    store.index_election(&Election::default()).expect("indexes");

    // An earlier session's holdings, still on disk where a session cut
    // left them, and a message it may not serve into the new session.
    store
        .land(&landed("old", "r-old", "load", 0))
        .expect("lands");
    let mut stale = landed("old", "r-old", "message.user", 1);
    stale.turn = Some("t-1".into());
    stale.pairs = vec![("payload.content".into(), "\"the vault code\"".into())];
    store.land(&stale).expect("lands");

    store
        .land(&landed("new", "r-new", "load", 0))
        .expect("lands");
    let mut fresh = landed("new", "r-new", "message.user", 1);
    fresh.turn = Some("t-1".into());
    fresh.pairs = vec![("payload.content".into(), "\"hello\"".into())];
    store.land(&fresh).expect("lands");

    assert_eq!(store.held().expect("held"), 4, "the file holds both");

    let shape = store.shape("new").expect("shapes");
    assert_eq!(
        shape.len(),
        1,
        "the shape holds the running session's runs alone: {shape:?}"
    );
    assert_eq!(shape[0].run, "r-new");

    let recalled = store.recall("new", None).expect("recalls");
    assert_eq!(
        recalled.len(),
        1,
        "the recall reads the running session's messages alone: {recalled:?}"
    );
    assert_eq!(recalled[0].run, "r-new");
    assert!(
        !recalled[0].pairs.iter().any(|(_, v)| v.contains("vault")),
        "and never the retired session's content"
    );

    // A bounded recall reads the turn-selection subquery, which the
    // unbounded ask above never touches. The older session's second
    // turn lands last so it holds the highest id: unbounded by session,
    // `LIMIT 1` would elect it, and the event query - still bounded -
    // would then find no row of it to read.
    let mut later_stale = landed("old", "r-old", "message.user", 2);
    later_stale.turn = Some("t-2".into());
    later_stale.pairs = vec![("payload.content".into(), "\"the vault code again\"".into())];
    store.land(&later_stale).expect("lands");

    let bounded = store.recall("new", Some(1)).expect("recalls");
    assert_eq!(
        bounded.len(),
        1,
        "the bound selects the running session's turn, not the newest \
         turn on the file: {bounded:?}"
    );
    assert_eq!(bounded[0].run, "r-new");
    assert_eq!(bounded[0].turn.as_deref(), Some("t-1"));

    // The older session is not destroyed, only unreachable: removal is
    // section 6's open question, deliberately not this act's.
    let old_shape = store.shape("old").expect("shapes");
    assert_eq!(old_shape.len(), 1, "the older session's rows stand");
}

/// The shape holds the runs in first-landed order by the id column,
/// interleaved landings included, each with its counts by kind, and
/// the answer frame renders the contract's spelling.
#[test]
fn the_shape_orders_runs_by_first_landing() {
    let mut store = Sqlite::stand().expect("opens");
    for (run, kind, sequence) in [
        ("r-1", "load", 0),
        ("r-1", "turn.closed", 1),
        ("r-2", "load", 0),
        ("r-1", "turn.closed", 2),
        ("r-2", "turn.closed", 1),
    ] {
        store
            .land(&landed("s", run, kind, sequence))
            .expect("lands");
    }
    let shape = store.shape("s").expect("shapes");
    assert_eq!(shape.len(), 2);
    assert_eq!(shape[0].run, "r-1", "first landed leads");
    assert_eq!(
        shape[0].kinds,
        vec![("load".to_string(), 1), ("turn.closed".to_string(), 2)]
    );
    assert_eq!(shape[1].run, "r-2");
    let frame = render_shape_answer(&shape);
    assert!(
        frame.starts_with(r#"{"answer":{"shape":{"runs":["#),
        "{frame}"
    );
    assert!(frame.ends_with("}\n"), "{frame}");
}

/// The answered-against clause, in time: an ask sees every landing
/// before it and nothing after, because the shape reads the holdings
/// at its own position in the stream.
#[test]
fn an_ask_sees_the_holdings_at_its_position_and_no_more() {
    let mut store = Sqlite::stand().expect("opens");
    store.land(&landed("s", "r-1", "load", 0)).expect("lands");
    let before = store.shape("s").expect("shapes");
    assert_eq!(before[0].kinds, vec![("load".to_string(), 1)]);
    store
        .land(&landed("s", "r-1", "turn.closed", 1))
        .expect("lands");
    let after = store.shape("s").expect("shapes");
    assert_eq!(
        after[0].kinds,
        vec![("load".to_string(), 1), ("turn.closed".to_string(), 1)]
    );
    assert_eq!(before[0].kinds.len(), 1, "the earlier answer never grew");
}

/// The ask vocabulary is closed at two names: both are recognized,
/// the recall's optional bound parses, and every other frame is not
/// an ask at all.
/// **A restore from a branch is answered with the branch's inherited
/// conversation** (#697, answering Codex's finding on #702). The branch's
/// record goes through the tee under an election that does not name
/// `message.restored`, lands, and is recalled whole and bounded.
///
/// Perturbation: drop `'message.restored'` from the recall query's kind
/// list and the inherited exchange is missing from the whole answer.
#[test]
fn a_restore_from_a_branch_recalls_its_inherited_conversation() {
    let mut store = Sqlite::stand().expect("opens");
    for distillate in crate::store::branch_record() {
        store.land(&distillate).expect("lands");
    }
    crate::store::assert_branch_recall(
        &store.recall("s-branch", None).expect("recall"),
        &store.recall("s-branch", Some(1)).expect("bounded recall"),
    );
}

/// **A replay reads what a recall does not**, which is the whole reason
/// the ask exists: `recall` serves the message kinds and a replay
/// walks the rendered contributions and the recorded measurements too.
/// Perturbation: give `replay` the kind filter `recall` carries and this
/// fails on the two events it would drop.
#[test]
fn a_replay_reads_every_kind_and_a_recall_reads_the_messages() {
    let mut store = Sqlite::stand().expect("opens");
    for (kind, sequence) in [
        ("message.user", 1),
        ("model.request", 2),
        ("model.measurement", 3),
        ("message.assistant", 4),
    ] {
        let mut event = landed("s", "r", kind, sequence);
        event.turn = Some("t1".into());
        store.land(&event).expect("lands");
    }
    let replayed = store.replay("s").expect("replay");
    assert_eq!(replayed.len(), 4, "a replay serves every held event");
    let kinds: Vec<&str> = replayed.iter().map(|e| e.kind.as_str()).collect();
    assert_eq!(
        kinds,
        [
            "message.user",
            "model.request",
            "model.measurement",
            "message.assistant"
        ],
        "and in landing order"
    );
    let recalled = store.recall("s", None).expect("recall");
    assert_eq!(recalled.len(), 2, "where a recall serves the message kinds");
    let frame = render_replay_answer(&replayed);
    assert!(
        frame.starts_with(r#"{"answer":{"replay":{"events":["#),
        "{frame}"
    );
    assert!(frame.ends_with("}\n"), "{frame}");
}

/// **The retirement and the opener's indexes commit together**, per the
/// contract's same-transaction claim as the audit of 2026-08-26 read
/// it: a retire under a non-empty election leaves the election's index
/// standing over the replaced holdings, and a retire whose index build
/// fails leaves the holdings exactly as they stood, the delete rolled
/// back with it. The failing build is bought with an election key
/// carrying an interior NUL, which sqlite refuses as a statement.
///
/// Perturbation: commit the delete before the build runs and the
/// atomicity half fails, the holdings gone under a build that never
/// happened.
#[test]
fn the_retirement_and_its_index_commit_together() {
    let mut store = Sqlite::stand().expect("opens");
    store
        .land(&landed("replayed", "r", "message.user", 1))
        .expect("lands");

    // The index half: a non-empty election's index stands after the
    // retire that carried it.
    let election = Election {
        all_kinds: true,
        keys: vec![("message.user".into(), vec!["content".into()])],
    };
    store
        .retire_and_index("replayed", &election)
        .expect("retires and indexes");
    let indexed: i64 = store
        .connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index'                  AND name LIKE 'field_elected_%'",
            [],
            |row| row.get(0),
        )
        .expect("counts indexes");
    assert!(indexed >= 1, "the election's index stands");

    // The atomicity half: a build sqlite refuses rolls the delete back
    // with it.
    store
        .land(&landed("replayed", "r", "message.user", 2))
        .expect("lands again");
    let poisoned = Election {
        all_kinds: true,
        keys: vec![("message.user".into(), vec!["a\u{0}b".into()])],
    };
    assert!(
        store.retire_and_index("replayed", &poisoned).is_err(),
        "the poisoned build fails"
    );
    let held: i64 = store
        .connection
        .query_row(
            "SELECT COUNT(*) FROM event WHERE session = 'replayed'",
            [],
            |row| row.get(0),
        )
        .expect("counts holdings");
    assert_eq!(held, 1, "the holdings survive the failed build whole");
}

/// **The retirement is bounded to the declared session**, per the Spec:
/// re-running a preload replaces that session's holdings and reaches no
/// other session's rows. Perturbation: drop the `WHERE session` from
/// either delete and the untouched session loses its events.
#[test]
fn the_preload_opener_retires_its_own_session_alone() {
    let mut store = Sqlite::stand().expect("opens");
    store
        .land(&landed("replayed", "r", "message.user", 1))
        .expect("lands");
    store
        .land(&landed("other", "r", "message.user", 1))
        .expect("lands");
    store
        .retire_and_index("replayed", &Election::default())
        .expect("retire");
    assert!(
        store.replay("replayed").expect("replay").is_empty(),
        "the declared session's holdings are gone"
    );
    assert_eq!(
        store.replay("other").expect("replay").len(),
        1,
        "and no other session's are"
    );
    assert_eq!(
        store.held().expect("held"),
        1,
        "the field rows go with them"
    );
}

#[test]
fn the_ask_vocabulary_is_closed() {
    assert_eq!(parse_ask(r#"{"ask":{"shape":{}}}"#), Some(Ask::Shape));
    assert_eq!(
        parse_ask(r#"{"ask":{"recall":{}}}"#),
        Some(Ask::Recall { last_turns: None })
    );
    assert_eq!(
        parse_ask(r#"{"ask":{"recall":{"last-turns":3}}}"#),
        Some(Ask::Recall {
            last_turns: Some(3)
        })
    );
    // The third name, added 2026-08-24. It carries no members, so a
    // members object and a bare one parse alike and neither carries a
    // bound the way `recall` does.
    assert_eq!(parse_ask(r#"{"ask":{"replay":{}}}"#), Some(Ask::Replay));
    for not_an_ask in [
        r#"{"ask":{"summarize":{}}}"#,
        // One ask per frame, its body exactly the contract's: a frame
        // naming two asks, a snapshot with a body, a snapshot that is
        // null, a bodiless shape and a recall with a stranger each answer
        // nothing, and the snapshot cases write nothing. Perturbation:
        // parse by the presence of a name again and the compound frame
        // takes a save point.
        r#"{"ask":{"snapshot":{"ask":1},"shape":{}}}"#,
        r#"{"ask":{"snapshot":{"now":true}}}"#,
        r#"{"ask":{"snapshot":null}}"#,
        r#"{"ask":{"shape":null}}"#,
        r#"{"ask":{"shape":{},"grants":{}}}"#,
        // **The live `restore` ask is retired** (the operator's ruling of
        // 2026-10-08 on #99): its frame, well formed as the contract once
        // spelled it, is an unknown name and answers nothing.
        // Perturbation: map `restore` to any ask in `parse_ask` and the
        // first case parses.
        r#"{"ask":{"restore":{"save-point":"x"}}}"#,
        r#"{"ask":{"restore":{}}}"#,
        r#"{"ask":{"recall":{"stranger":1}}}"#,
        r#"{"ask":{"recall":{"last-turns":1,"stranger":1}}}"#,
        r#"{"ask":[]}"#,
        r#"{"ask":{"recall":{"last-turns":-3}}}"#,
        r#"{"ask":{"recall":{"last-turns":"three"}}}"#,
        r#"{"ask":{"recall":{"last-turns":2.5}}}"#,
        r#"{"envelope":{}}"#,
        "not json",
    ] {
        assert!(parse_ask(not_an_ask).is_none(), "{not_an_ask}");
    }
}

/// The bounded recall keys its turns by session, run, and turn
/// together: a turn label recurring across runs names two different
/// turns, and the bound must not recall the older run's events beside
/// its namesake's.
#[test]
fn a_bounded_recall_keeps_colliding_turn_labels_apart() {
    let mut store = Sqlite::stand().expect("opens");
    let message = |run: &str, turn: &str, text: &str, sequence: i64| Distillate {
        session: "s".into(),
        run: run.into(),
        turn: Some(turn.into()),
        kind: "message.user".into(),
        sequence,
        pairs: vec![("content".into(), format!("\"{text}\""))],
    };
    for landing in [
        message("r-1", "t-1", "old one", 1),
        message("r-1", "t-2", "old two", 2),
        message("r-2", "t-1", "new one", 1),
        message("r-2", "t-2", "new two", 2),
    ] {
        store.land(&landing).expect("lands");
    }
    let bounded = store.recall("s", Some(2)).expect("recalls");
    let quoted: Vec<&str> = bounded
        .iter()
        .map(|event| event.pairs[0].1.as_str())
        .collect();
    assert_eq!(
        quoted,
        vec!["\"new one\"", "\"new two\""],
        "the bound keeps the newer run's turns and no namesakes"
    );
    let whole = store.recall("s", None).expect("recalls");
    assert_eq!(whole.len(), 4, "the unbounded recall reads every message");
}

/// The parse demands the envelope whole: a frame missing any envelope
/// member is nobody's row.
#[test]
fn an_unattributable_frame_is_refused() {
    assert!(
        parse_distillate(r#"{"envelope":{"session":"s","run":"r","kind":"load","sequence":"0"}}"#)
            .is_some()
    );
    for missing in [
        // The canonical form never spells a sequence below zero.
        r#"{"envelope":{"session":"s","run":"r","kind":"load","sequence":"-1"}}"#,
        r#"{"envelope":{"run":"r","kind":"load","sequence":"0"}}"#,
        r#"{"envelope":{"session":"s","kind":"load","sequence":"0"}}"#,
        r#"{"envelope":{"session":"s","run":"r","sequence":"0"}}"#,
        r#"{"envelope":{"session":"s","run":"r","kind":"load"}}"#,
        r#"{"pairs":{}}"#,
        "not json",
    ] {
        assert!(parse_distillate(missing).is_none(), "{missing} must refuse");
    }
}
