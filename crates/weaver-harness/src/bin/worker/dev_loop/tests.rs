use super::*;

fn texts(delta: &[Message]) -> Vec<&str> {
    delta
        .iter()
        .flat_map(|message| {
            message.content.iter().map(|block| match block {
                ContentBlock::Text { text } => text.as_str(),
                _ => "",
            })
        })
        .collect()
}

/// The one-time injection contract where nothing is seated: a first turn
/// leads with the compiled prompt, in order, and every later turn
/// contributes the request alone. The seated case is
/// `a_seated_prefix_takes_no_opening`.
///
/// **The roles are asserted apart rather than together.** This read
/// `all(|m| m.role == Role::User)` until 2026-08-28, which passed
/// because the prompt rode the role the request did, and would have gone
/// on passing had the prompt moved to `Assistant`. The opening is the
/// loop's framing of the field and the request is the operator's, so
/// they are two roles and the test names both.
#[test]
fn the_prompt_leads_the_first_turn_and_no_other() {
    let first = contribution(true, false, None, None, "hello");
    assert_eq!(texts(&first), vec![SYSTEM_PROMPT, "hello"]);
    assert_eq!(
        first.iter().map(|m| m.role.clone()).collect::<Vec<_>>(),
        vec![Role::System, Role::User],
        "the prompt is the field's framing and the request is not"
    );

    let later = contribution(false, false, None, None, "and again");
    assert_eq!(texts(&later), vec!["and again"]);
    assert!(later.iter().all(|m| m.role == Role::User));
}

/// **A seated prefix takes no opening**, per ADP-4
/// (`weaver-spu-adapter-binding-Spec-amendment` section 4): where the open
/// seated the agent's prompt, the first turn carries neither the compiled
/// prompt nor the continuity line, which pushed alone would still be a
/// second system turn, and the re-entry carries the recall text alone, the
/// seated prefix having survived the flush. The seat's half, the flag
/// itself, is loop zero's suite's (`lifecycle/seated_tests.rs`).
///
/// Perturbation: push the opening or the re-entry's prompt whatever the
/// flag and this fails.
#[test]
fn a_seated_prefix_takes_no_opening() {
    let first = contribution(true, true, Some("Continuity: held."), None, "hello");
    assert_eq!(texts(&first), vec!["hello"]);
    assert_eq!(
        first.iter().map(|m| m.role.clone()).collect::<Vec<_>>(),
        vec![Role::User],
        "no system turn rides the seated prefix"
    );

    let reentry = reentry_text(true, None);
    assert!(
        reentry.starts_with("The working context was reset"),
        "the re-entry is the recall text alone: {reentry}"
    );
    assert!(!reentry.contains(SYSTEM_PROMPT), "{reentry}");
    let flushed = contribution(false, true, None, Some(&reentry), "next ask");
    assert_eq!(texts(&flushed), vec![reentry.as_str(), "next ask"]);
}

/// The continuity line rides beneath the prompt in the same opening
/// message, so the injection stays one message and one narrowing.
#[test]
fn continuity_rides_beneath_the_prompt() {
    let first = contribution(true, false, Some("Continuity: held."), None, "hello");
    assert_eq!(first.len(), 2);
    let opening = &texts(&first)[0];
    assert!(opening.starts_with(SYSTEM_PROMPT), "{opening}");
    assert!(opening.ends_with("Continuity: held."), "{opening}");
}

/// The loop's judgment over the served shape: a session with no past
/// injects nothing, the current run being the shape's last, and the
/// earlier runs sum their closed turns.
#[test]
fn the_continuity_line_counts_the_past_and_only_the_past() {
    let run = |name: &str, closed: u64| weaver_harness::RunShape {
        run: name.to_string(),
        kinds: vec![("load".to_string(), 1), ("turn.closed".to_string(), closed)],
    };
    let alone = SessionShape {
        runs: vec![run("r-1", 0)],
    };
    assert!(continuity_line(&alone).is_none(), "no past, no line");
    let storied = SessionShape {
        runs: vec![run("r-1", 3), run("r-2", 1), run("r-3", 0)],
    };
    let line = continuity_line(&storied).expect("a past injects");
    assert!(line.contains("2 earlier runs"), "{line}");
    assert!(line.contains("4 completed turns"), "{line}");
}

/// The threshold is four fifths, saturating, and a zero capacity never
/// pressures: the wall is met by a flush, not by arithmetic surprise.
#[test]
fn the_pressure_threshold_holds_its_edges() {
    assert!(!pressured(0, 0));
    assert!(!pressured(100, 0));
    assert!(!pressured(3199, 4096));
    assert!(pressured(3277, 4096));
    assert!(pressured(4096, 4096));
    assert!(pressured(u64::MAX, u64::MAX));
}

/// The re-entry quotes the recalled conversation with speakers named,
/// bounds each quote, skips what is not a message, and composes the
/// honest fallback where the recall missed.
#[test]
fn the_reentry_composes_from_recall() {
    let event = |kind: &str, text: &str| Recalled {
        kind: kind.to_string(),
        run: "r-1".into(),
        turn: Some("t-1".into()),
        sequence: "1".into(),
        pairs: vec![(
            "content".into(),
            format!(r#"[{{"type":"text","text":"{text}"}}]"#),
        )],
    };
    let text = reentry_text(
        false,
        Some(vec![
            event("message.user", "what is the plan"),
            event("message.assistant", "the plan is threefold"),
            event("turn.closed", "not a message"),
        ]),
    );
    assert!(text.starts_with(SYSTEM_PROMPT), "{text}");
    assert!(text.contains("user: what is the plan"), "{text}");
    assert!(text.contains("assistant: the plan is threefold"), "{text}");
    assert!(!text.contains("not a message"), "{text}");
    let missing = reentry_text(false, None);
    assert!(missing.contains("could not be reached"), "{missing}");
    let long = "word ".repeat(400);
    let bounded = reentry_text(false, Some(vec![event("message.user", long.trim())]));
    assert!(bounded.contains(" ..."), "the quote is bounded: {bounded}");
}

/// The quote budget is one ceiling across the whole re-entry, not one
/// allowance per message: many recalled messages stop quoting where
/// the budget runs out, so a busy recall cannot rebuild the pressure
/// the flush just relieved.
#[test]
fn the_quote_budget_spans_the_whole_recall() {
    let event = |text: &str| Recalled {
        kind: "message.user".to_string(),
        run: "r-1".into(),
        turn: Some("t-1".into()),
        sequence: "1".into(),
        pairs: vec![(
            "content".into(),
            format!(r#"[{{"type":"text","text":"{text}"}}]"#),
        )],
    };
    let long = "word ".repeat(60);
    let many: Vec<Recalled> = (0..10).map(|_| event(long.trim())).collect();
    let text = reentry_text(false, Some(many));
    assert!(
        text.len() < SYSTEM_PROMPT.len() + 2 * QUOTE_BOUND,
        "one budget bounds the whole re-entry, got {} chars: {text}",
        text.len()
    );
    assert!(text.contains(" ..."), "the cut is marked: {text}");
    assert!(
        text.contains("user: word"),
        "the budget quotes before it cuts: {text}"
    );
}

/// A flushed turn's delta carries the re-entry between the opening and
/// the request, and an unflushed one carries no such member.
#[test]
fn the_reentry_rides_between_opening_and_request() {
    let delta = contribution(false, false, None, Some("restored context"), "next ask");
    assert_eq!(texts(&delta), vec!["restored context", "next ask"]);
    let plain = contribution(false, false, None, None, "next ask");
    assert_eq!(texts(&plain), vec!["next ask"]);
}

/// What the stand-in seat was asked, in order.
#[derive(Debug, PartialEq)]
enum Asked {
    Flush(u64),
    Recall,
    Shape,
    Turn(Vec<(Role, String)>),
}

/// A recording stand-in for the seat: it answers what the test sets and
/// records every ask in order, so `compose` is driven whole, the election,
/// the flush and the composition together. It mints no port: the trait is
/// this loop's own view of the seat, and `Ports` is its one production
/// implementation.
struct Stand {
    first_turn: bool,
    seated: bool,
    turn_context: TurnContext,
    fullness: Option<(u64, u64)>,
    asked: Vec<Asked>,
}

impl Stand {
    fn new(turn_context: TurnContext, first_turn: bool, seated: bool) -> Self {
        Stand {
            first_turn,
            seated,
            turn_context,
            fullness: None,
            asked: Vec::new(),
        }
    }
}

impl Seat for Stand {
    fn first_turn(&self) -> bool {
        self.first_turn
    }
    fn seated(&self) -> bool {
        self.seated
    }
    fn turn_context(&self) -> TurnContext {
        self.turn_context
    }
    fn fullness(&self) -> Option<(u64, u64)> {
        self.fullness
    }
    fn flush(&mut self, keep: u64) -> Option<(u64, u64)> {
        self.asked.push(Asked::Flush(keep));
        None
    }
    fn recall(&mut self, _last_turns: Option<u64>) -> Option<Vec<Recalled>> {
        self.asked.push(Asked::Recall);
        None
    }
    fn session_shape(&mut self) -> Option<SessionShape> {
        self.asked.push(Asked::Shape);
        None
    }
    fn turn(&mut self, delta: Vec<Message>) -> Result<TurnOutcome, TurnError> {
        let spoken = delta
            .iter()
            .map(|message| {
                (
                    message.role.clone(),
                    texts(std::slice::from_ref(message)).join(""),
                )
            })
            .collect();
        self.asked.push(Asked::Turn(spoken));
        Err(TurnError::ChannelLost)
    }
}

/// **Under `independent` every turn is preceded by a flush to the identity
/// prefix and composes nothing else** (#423, `weaver-harness-Spec` section 6,
/// loop 1's turn-context read): the flush cuts to zero, which the seam bounds
/// at the prefix, directly before the turn; no recall and no shape are asked,
/// so neither a re-entry nor a continuity line is composed; and where nothing
/// is seated the compiled prompt leads every turn, the flush having retired
/// the copy an earlier turn appended. A first turn, a later one and a later
/// one past the pressure line all take the same path.
///
/// Perturbation: drop the election from `compose` and the first case asks the
/// shape instead of flushing; move the flush after the turn and the order
/// fails; read the first-turn test under `independent` and the later turns
/// lose the compiled prompt. Watched to fail under each.
#[test]
fn independent_flushes_before_every_turn_and_composes_nothing_else() {
    for seated in [false, true] {
        for (first_turn, fullness) in [
            (true, None),
            (false, Some((1, 100))),
            (false, Some((90, 100))),
        ] {
            let mut seat = Stand::new(TurnContext::Independent, first_turn, seated);
            seat.fullness = fullness;
            let _ = compose(&mut seat, "item");
            let mut delta = Vec::new();
            if !seated {
                delta.push((Role::System, SYSTEM_PROMPT.to_string()));
            }
            delta.push((Role::User, "item".to_string()));
            assert_eq!(
                seat.asked,
                vec![Asked::Flush(0), Asked::Turn(delta)],
                "seated {seated}, first turn {first_turn}, fullness {fullness:?}"
            );
        }
    }
}

/// **`continuous` is unchanged** (#423): a first turn with nothing seated asks
/// the shape and leads with the prompt, a later turn under the line
/// contributes the request alone with no flush, and a later turn past the
/// line flushes, recalls and rides the re-entry, as before the election.
///
/// Perturbation: take the `independent` path for every election and the
/// first case flushes. Watched to fail under exactly that.
#[test]
fn continuous_is_unchanged() {
    let mut first = Stand::new(TurnContext::Continuous, true, false);
    let _ = compose(&mut first, "ask");
    assert_eq!(
        first.asked,
        vec![
            Asked::Shape,
            Asked::Turn(vec![
                (Role::System, SYSTEM_PROMPT.to_string()),
                (Role::User, "ask".to_string()),
            ]),
        ]
    );

    let mut later = Stand::new(TurnContext::Continuous, false, true);
    later.fullness = Some((1, 100));
    let _ = compose(&mut later, "ask");
    assert_eq!(
        later.asked,
        vec![Asked::Turn(vec![(Role::User, "ask".to_string())])]
    );

    let mut pressured = Stand::new(TurnContext::Continuous, false, true);
    pressured.fullness = Some((90, 100));
    let _ = compose(&mut pressured, "ask");
    assert_eq!(
        pressured.asked,
        vec![
            Asked::Flush(0),
            Asked::Recall,
            Asked::Turn(vec![
                (Role::System, reentry_text(true, None)),
                (Role::User, "ask".to_string()),
            ]),
        ]
    );
}
