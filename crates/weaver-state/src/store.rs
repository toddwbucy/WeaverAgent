//! conforms: state-store-is-a-port
//! conforms: state-distillate-lands-whole
//! conforms: state-serve-restricts-to-the-session
//!
//! `state-indexes-built-at-load` is not cited here. This file is the port, a
//! `trait Store` declaring what an engine must do, and it builds no index:
//! each engine's `build_indexes` does, under its own naming and against its
//! own store's limits. Citing it from the declaration would assert it of
//! every engine that implements the trait, including one written later and
//! watched by nothing. Each engine holds it where its code and its
//! instrument are.
//!
//! The custody, per `weaver-state-Spec` section 3: sqlite behind the seam,
//! never reached as a file, the distillate landing whole or not at all.

/// The custodian's parse of the opener's election, whose meaning is the
/// `election` term in `weaver-harness-state-contract`'s Vocabulary.
/// The paths drive index DDL here. Selection belongs to the tee, per
/// `weaver-trace-Spec` section 11, rather than to this representation.
#[derive(Debug, Clone, PartialEq)]
pub struct Election {
    /// Carried from the opener as a seam fact, not consulted by this custodian.
    pub all_kinds: bool,
    /// Parsed kind/path pairs whose paths drive index DDL.
    /// An empty path list requires no indexes here.
    pub keys: Vec<(String, Vec<String>)>,
}

impl Default for Election {
    fn default() -> Self {
        Election {
            all_kinds: true,
            keys: Vec::new(),
        }
    }
}

/// Values parsed from one frame and landed whole by the store, per the
/// `distillate` term in `weaver-harness-state-contract`'s Vocabulary.
/// Pair values remain the frame's raw text; this crate does not select them.
#[derive(Debug, Clone, PartialEq)]
pub struct Distillate {
    pub session: String,
    pub run: String,
    pub turn: Option<String>,
    pub kind: String,
    pub sequence: i64,
    pub pairs: Vec<(String, String)>,
}

/// What custody refuses. The set is small because the charter is: a
/// custodian that answered richly would be growing a voice the serve
/// direction has not given it.
#[derive(Debug)]
pub enum CustodyFault {
    /// The store could not open or the schema could not stand.
    StoreUnavailable(String),
    /// A distillate failed to land. The transaction rolled back whole.
    LandingFailed(String),
}

/// **The store is a port**, per `weaver-state-Spec` section 3 and the ruling of
/// 2026-09-04: every engine implements the whole of it, the ingest and the
/// serve speak to this and never to an engine, and this is the one place a
/// query language is spelled, each engine in its own dialect.
pub trait Store {
    /// Build the elected keys' indexes, read from the seam's opener.
    fn index_election(&mut self, election: &Election) -> Result<(), CustodyFault>;
    /// Land one distillate whole or not at all.
    fn land(&mut self, distillate: &Distillate) -> Result<(), CustodyFault>;
    /// Retire the session's prior holdings and stand the election's indexes,
    /// in one transaction, per the preload door's contract.
    fn retire_and_index(&mut self, session: &str, election: &Election) -> Result<(), CustodyFault>;
    /// Every event of the session with its pairs, in landing order.
    fn replay(&self, session: &str) -> Result<Vec<RecalledEvent>, CustodyFault>;
    /// How many events the store holds, every session counted.
    fn held(&self) -> Result<i64, CustodyFault>;
    /// The session's shape: its runs in order, each with its kinds counted.
    fn shape(&self, session: &str) -> Result<Vec<RunShape>, CustodyFault>;
    /// The session's message events, bounded to the last `last_turns` turns
    /// where a bound is given.
    fn recall(
        &self,
        session: &str,
        last_turns: Option<u64>,
    ) -> Result<Vec<RecalledEvent>, CustodyFault>;
    /// The boundary as the engine states it, per the contract's `grants`
    /// ask of 2026-09-04: an ordered list of lines the engine renders from
    /// its own catalog, spelled so two readings compare and no more.
    fn grants(&self) -> Result<Vec<String>, CustodyFault>;
    /// The session's seated prefix as custody holds it, per the contract's
    /// `identity` ask of 2026-09-04: the turnless `message.system` events
    /// in landing order with their pairs, empty where the session holds
    /// none.
    fn identity(&self, session: &str) -> Result<Vec<RecalledEvent>, CustodyFault>;
}

/// One run's shape, the answer's material: the run reference and the held
/// event counts by kind.
#[derive(Debug, Clone, PartialEq)]
pub struct RunShape {
    pub run: String,
    pub kinds: Vec<(String, i64)>,
}

/// An ask as the seam's closed vocabulary spells it: three names, per the
/// contract's section 2 as amended 2026-08-24, and a frame carrying any
/// other ask name is malformed and answers nothing.
#[derive(Debug, Clone, PartialEq)]
pub enum Ask {
    /// The session's shape: runs in first-seen order, counts by kind.
    Shape,
    /// The conversation as custody holds it, bounded to the most recent
    /// turns where a bound is given.
    Recall { last_turns: Option<u64> },
    /// Every held event of the declared session, whole, in landing order.
    /// Carries no members: what a replay reads is the session, and the
    /// message kinds `recall` serves are less than it needs.
    Replay,
    /// The boundary as the store states it, read at the enter and the
    /// leave, per the contract as of 2026-09-04. Carries no members.
    Grants,
    /// The session's seated prefix, asked once at every enter before the
    /// decode open, per the contract as of 2026-09-04. Carries no members.
    Identity,
}

/// Parse a seam frame as an ask, or nothing where it is not one.
pub fn parse_ask(frame: &str) -> Option<Ask> {
    let value: serde_json::Value = serde_json::from_str(frame).ok()?;
    let ask = value.get("ask")?;
    if ask.get("shape").is_some() {
        return Some(Ask::Shape);
    }
    if ask.get("replay").is_some() {
        return Some(Ask::Replay);
    }
    if ask.get("grants").is_some() {
        return Some(Ask::Grants);
    }
    if ask.get("identity").is_some() {
        return Some(Ask::Identity);
    }
    let recall = ask.get("recall")?;
    let last_turns = match recall.get("last-turns") {
        None => None,
        Some(bound) => Some(bound.as_u64()?),
    };
    Some(Ask::Recall { last_turns })
}

/// Whether a seam frame is the shape ask, kept for the standing tests: the
/// dispatch reads [`parse_ask`].
pub fn is_shape_ask(frame: &str) -> bool {
    matches!(parse_ask(frame), Some(Ask::Shape))
}

/// One recalled event, the recall answer's material: the envelope's facts
/// and the elected pairs as custody kept them.
#[derive(Debug, Clone, PartialEq)]
pub struct RecalledEvent {
    pub session: String,
    pub run: String,
    pub turn: Option<String>,
    pub kind: String,
    pub sequence: i64,
    pub pairs: Vec<(String, String)>,
}

/// Render the recall answer as the contract's frame: each event in the
/// distillate's own shape, envelope and pairs, because custody serves what
/// it kept in the form it kept it.
pub fn render_recall_answer(events: &[RecalledEvent]) -> String {
    format!(
        r#"{{"answer":{{"recall":{{"events":{}}}}}}}"#,
        rendered_events(events)
    ) + "\n"
}

/// One event's rendering, envelope and pairs, shared by the recall and the
/// replay answers because both serve an event as the distillate's own
/// shape and a second rendering would be a second spelling of one form.
fn rendered_events(events: &[RecalledEvent]) -> String {
    use serde_json::value::{RawValue, to_raw_value};
    use std::collections::BTreeMap;

    let rendered: Vec<_> = events
        .iter()
        .map(|event| {
            let mut envelope = serde_json::Map::new();
            envelope.insert("session".into(), event.session.clone().into());
            envelope.insert("run".into(), event.run.clone().into());
            if let Some(turn) = &event.turn {
                envelope.insert("turn".into(), turn.clone().into());
            }
            envelope.insert("kind".into(), event.kind.clone().into());
            envelope.insert("sequence".into(), event.sequence.to_string().into());
            let pairs: BTreeMap<String, Box<RawValue>> = event
                .pairs
                .iter()
                .map(|(key, value)| {
                    // Splice the stored JSON without interpreting its object order,
                    // number spelling, escapes, or interior whitespace.
                    let parsed = RawValue::from_string(value.clone())
                        .unwrap_or_else(|_| to_raw_value(value).expect("string serializes"));
                    (key.clone(), parsed)
                })
                .collect();
            BTreeMap::from([
                (
                    "envelope",
                    to_raw_value(&envelope).expect("envelope serializes"),
                ),
                ("pairs", to_raw_value(&pairs).expect("raw pairs serialize")),
            ])
        })
        .collect();
    serde_json::to_string(&rendered).expect("raw events serialize")
}

/// Render the replay answer as the contract's frame: every event whole, in
/// landing order, each as the distillate's own shape. The answer names the
/// ask it answers, which is what pairs it without a correlation member, per
/// `weaver-harness-state-contract` section 2.
pub fn render_replay_answer(events: &[RecalledEvent]) -> String {
    format!(
        r#"{{"answer":{{"replay":{{"events":{}}}}}}}"#,
        rendered_events(events)
    ) + "\n"
}

/// The grants answer: the surface's lines in the engine's order, per the
/// contract's answer shape `{"answer":{"grants":{"surface":[...]}}}`.
pub fn render_grants_answer(surface: &[String]) -> String {
    let mut frame = serde_json::json!({"answer": {"grants": {"surface": surface}}}).to_string();
    frame.push('\n');
    frame
}

/// The identity answer: the seated prefix's events, each the distillate's
/// own shape, per the contract's `{"answer":{"identity":{"messages":[...]}}}`.
/// An empty list is an answer, the first load of the session.
pub fn render_identity_answer(events: &[RecalledEvent]) -> String {
    format!(
        r#"{{"answer":{{"identity":{{"messages":{}}}}}}}"#,
        rendered_events(events)
    ) + "\n"
}

/// Render the shape answer as the contract's frame, one answer frame on
/// the channel, the runs in the order the query gave them.
pub fn render_shape_answer(runs: &[RunShape]) -> String {
    let entries: Vec<serde_json::Value> = runs
        .iter()
        .map(|shape| {
            let kinds: serde_json::Map<String, serde_json::Value> = shape
                .kinds
                .iter()
                .map(|(kind, count)| (kind.clone(), serde_json::Value::from(*count)))
                .collect();
            serde_json::json!({"run": shape.run, "kinds": kinds})
        })
        .collect();
    let mut frame = serde_json::json!({"answer": {"shape": {"runs": entries}}}).to_string();
    frame.push('\n');
    frame
}

/// Read a distillate from the frame the contract carries, or refuse it.
pub fn parse_distillate(frame: &str) -> Option<Distillate> {
    use serde_json::value::RawValue;
    let top: std::collections::BTreeMap<&str, &RawValue> = serde_json::from_str(frame).ok()?;
    let envelope: serde_json::Value = serde_json::from_str(top.get("envelope")?.get()).ok()?;
    // The pair values land as the raw text that crossed, never re-rendered,
    // because the distillate is a projection of the canonical form and a
    // reshaping here would break that on the last step.
    let pairs = match top.get("pairs") {
        Some(raw) => {
            serde_json::from_str::<std::collections::BTreeMap<String, &RawValue>>(raw.get())
                .ok()?
                .into_iter()
                .map(|(key, value)| (key, value.get().to_string()))
                .collect()
        }
        None => Vec::new(),
    };
    Some(Distillate {
        session: envelope.get("session")?.as_str()?.to_string(),
        run: envelope.get("run")?.as_str()?.to_string(),
        turn: envelope
            .get("turn")
            .and_then(|t| t.as_str())
            .map(str::to_string),
        kind: envelope.get("kind")?.as_str()?.to_string(),
        // The canonical form spells the sequence as a string and the
        // distillate carries that spelling, so the conversion to the row's
        // integer happens here, at the landing, and a spelling that does
        // not convert refuses the frame whole.
        sequence: envelope.get("sequence")?.as_str()?.parse().ok()?,
        pairs,
    })
}

/// A branch's record as the tee distils it, for both engines' restore tests
/// (#697): its identity, the conversation it inherited as `message.restored`
/// rows, then its own turn. The election names the turned message kinds and
/// not the restored one, so the restored rows reach custody by the tee's
/// whole-distill rule alone.
#[cfg(test)]
pub(crate) fn branch_record() -> Vec<Distillate> {
    let election = weaver_trace::Election {
        all_kinds: false,
        keys: ["message.user", "message.assistant"]
            .iter()
            .map(|kind| weaver_trace::ElectedKind {
                kind: (*kind).into(),
                paths: vec!["role".into(), "content".into()],
            })
            .collect(),
    };
    let line = |sequence: u32, turn: Option<&str>, kind: &str, role: &str, text: &str| {
        let turn = turn
            .map(|t| format!(r#","turn":"{t}""#))
            .unwrap_or_default();
        format!(
            concat!(
                r#"{{"session":"s-branch","run":"r-branch"{turn},"kind":"{kind}","#,
                r#""sequence":"{sequence}","subsystem":"harness","wall_ms":1,"#,
                r#""monotonic_ns":"{sequence}","payload":{{"role":"{role}","#,
                r#""content":[{{"type":"text","text":"{text}"}}]}}}}"#
            ),
            turn = turn,
            kind = kind,
            sequence = sequence,
            role = role,
            text = text
        )
    };
    [
        line(1, None, "message.system", "system", "You are Karl."),
        line(2, None, "message.restored", "user", "inherited question"),
        line(3, None, "message.restored", "assistant", "inherited answer"),
        line(4, Some("t-1"), "message.user", "user", "hello"),
        line(5, Some("t-1"), "message.assistant", "assistant", "hi"),
    ]
    .iter()
    .map(|line| {
        let frame = weaver_trace::distill(line, &election).expect("the tee distils the line");
        parse_distillate(&frame).expect("custody parses the frame")
    })
    .collect()
}

/// What a restore from that branch is answered with: the identity, the
/// inherited exchange with its role and content, then the branch's own
/// turn, in landing order. A bounded recall, the seat's after a flush,
/// answers the newest turns alone and none of the turnless rows.
#[cfg(test)]
pub(crate) fn assert_branch_recall(whole: &[RecalledEvent], bounded: &[RecalledEvent]) {
    let kinds: Vec<&str> = whole.iter().map(|e| e.kind.as_str()).collect();
    assert_eq!(
        kinds,
        [
            "message.system",
            "message.restored",
            "message.restored",
            "message.user",
            "message.assistant"
        ],
        "the inherited exchange is recalled between the identity and the branch's turn"
    );
    let pair = |event: &RecalledEvent, key: &str| {
        event
            .pairs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    };
    assert_eq!(pair(&whole[1], "role"), r#""user""#);
    assert_eq!(
        pair(&whole[1], "content"),
        r#"[{"type":"text","text":"inherited question"}]"#
    );
    assert_eq!(pair(&whole[2], "role"), r#""assistant""#);
    let bounded: Vec<&str> = bounded.iter().map(|e| e.kind.as_str()).collect();
    assert_eq!(
        bounded,
        ["message.user", "message.assistant"],
        "a bounded recall answers the newest turn and no turnless row"
    );
}
