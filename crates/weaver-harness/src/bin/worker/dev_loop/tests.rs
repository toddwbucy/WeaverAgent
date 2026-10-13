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

/// The one-time injection contract: a first turn leads with the prompt,
/// in order, and every later turn contributes the request alone.
///
/// **The roles are asserted apart rather than together.** This read
/// `all(|m| m.role == Role::User)` until 2026-08-28, which passed
/// because the prompt rode the role the request did, and would have gone
/// on passing had the prompt moved to `Assistant`. The opening is the
/// loop's framing of the field and the request is the operator's, so
/// they are two roles and the test names both.
#[test]
fn the_prompt_leads_the_first_turn_and_no_other() {
    let first = contribution(true, None, None, "hello");
    assert_eq!(texts(&first), vec![SYSTEM_PROMPT, "hello"]);
    assert_eq!(
        first.iter().map(|m| m.role.clone()).collect::<Vec<_>>(),
        vec![Role::System, Role::User],
        "the prompt is the field's framing and the request is not"
    );

    let later = contribution(false, None, None, "and again");
    assert_eq!(texts(&later), vec!["and again"]);
    assert!(later.iter().all(|m| m.role == Role::User));
}

/// The continuity line rides beneath the prompt in the same opening
/// message, so the injection stays one message and one narrowing.
#[test]
fn continuity_rides_beneath_the_prompt() {
    let first = contribution(true, Some("Continuity: held."), None, "hello");
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
    let text = reentry_text(Some(vec![
        event("message.user", "what is the plan"),
        event("message.assistant", "the plan is threefold"),
        event("turn.closed", "not a message"),
    ]));
    assert!(text.starts_with(SYSTEM_PROMPT), "{text}");
    assert!(text.contains("user: what is the plan"), "{text}");
    assert!(text.contains("assistant: the plan is threefold"), "{text}");
    assert!(!text.contains("not a message"), "{text}");
    let missing = reentry_text(None);
    assert!(missing.contains("could not be reached"), "{missing}");
    let long = "word ".repeat(400);
    let bounded = reentry_text(Some(vec![event("message.user", long.trim())]));
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
    let text = reentry_text(Some(many));
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
    let delta = contribution(false, None, Some("restored context"), "next ask");
    assert_eq!(texts(&delta), vec!["restored context", "next ask"]);
    let plain = contribution(false, None, None, "next ask");
    assert_eq!(texts(&plain), vec!["next ask"]);
}
