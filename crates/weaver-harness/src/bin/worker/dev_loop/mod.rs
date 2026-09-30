//! Your loop lives here.
//!
//! This is `dev_loop`, the developer's directory, per `weaver-harness-Spec`
//! sections 1 and 6: any directory carrying the `dev_` prefix is yours to
//! edit, and no directory without it is. The loop below is the operator's
//! first control loop, occupying the exact location the bare decode loop
//! held, so the example and the extension point remain the same place.
//!
//! The crossing is the one function loop 0 calls: the granted seat and the
//! parsed request come in, the turn's outcome goes back, and that is the
//! whole of the traffic across this boundary. The seat is the only surface
//! you hold, and a loop that needs a port the seat does not offer is a
//! charter change entering through the front door, not an import.

use weaver_harness::{Ports, Recalled, SessionShape, TurnError, TurnOutcome};
use weaver_traits::{ContentBlock, Message, Role};

/// The system prompt, the loop's own content, per the operator's direction
/// of 2026-08-18: contained with the control loop's code rather than the
/// agent declaration, injected as the first contribution, and thereby
/// recorded, distilled, and held like everything else the model ever
/// received. In the field vocabulary it is the first narrowing, applied
/// once per decode session.
///
/// **Edit this constant to shape your agent.** It rides as a system-role
/// message. It rode as `User` until 2026-08-28 because the floor carried no
/// system variant when this loop was written, and the clause here promised
/// the constant would move roles without moving homes once the
/// `Role::System` floor act landed. That act landed 2026-08-19 and this
/// clause went stale for nine days, which is the second half of what issue
/// #369 found: the declarations and the loop both kept a role the floor had
/// already retired.
///
/// **Moving it changes the prompt**, `<|im_start|>system` rendering where
/// `<|im_start|>user` did, so every run made under the old role stops being
/// a baseline. It moves here rather than later because the operator's
/// ruling of 2026-08-28 re-baselines both boxes on one frozen build, which
/// is the moment such a change costs nothing.
const SYSTEM_PROMPT: &str = "\
You are a careful assistant. Answer from what you know, say plainly when \
you do not know, and keep answers as short as the question allows.";

/// The first control loop, loop 1: the system prompt once per session's
/// standing, then one turn per request.
///
/// The first-turn test reads the seat's assembly: an empty message history
/// means no turn has run against this decode session, so the prompt leads
/// the delta and the narrowing base is established before the first
/// request's content. Every later turn finds history standing and
/// contributes only itself. A fresh run of the same session re-establishes
/// the base, which is what a fresh decode session needs: the resident
/// prefix died with the last process, and the record shows each run's
/// narrowing from its own first event.
pub fn drive(seat: &mut Ports<'_>, text: &str) -> Result<TurnOutcome, TurnError> {
    let first_turn = seat
        .assembled()
        .is_none_or(|prompt| prompt.messages.is_empty());
    // The context management, per the loop's charter: the decode context
    // is a working set this loop keeps under its ceiling. At pressure the
    // loop elects a flush - the decode context returns to its prefix, the
    // record carries the event - and rebuilds its own re-entry from
    // custody's recall, so the session survives its wall instead of dying
    // at it. Every judgment here is the loop's: the threshold, the recall
    // bound, and what the re-entry says.
    let reentry = if !first_turn
        && seat
            .fullness()
            .is_some_and(|(resident, capacity)| pressured(resident, capacity))
    {
        // The flush is driven for its effect and its record event, and its
        // confirmation gates nothing here: a missing answer cannot prove
        // the flush did not land, so the re-entry is composed either way.
        // Where the flush landed the model needs the restoration, and
        // where it did not the recap costs a few duplicated tokens on a
        // seam already failing.
        // The cut is the loop's: zero keeps the identity prefix alone,
        // which is this loop's whole standing context, its system prompt
        // riding as message content rather than as the open's prefix.
        let _ = seat.flush(0);
        Some(reentry_text(seat.recall(Some(RECALL_TURNS))))
    } else {
        None
    };
    // The context injection, per the loop's charter as the state seam's
    // first asker: on a run's first turn the loop consults the session's
    // shape and narrows the field with what the session already holds. The
    // ask costs one bounded exchange and a missing answer costs nothing
    // but the line, the leg being optional by presence.
    let continuity = if first_turn {
        seat.session_shape()
            .and_then(|shape| continuity_line(&shape))
    } else {
        None
    };
    seat.turn(contribution(
        first_turn,
        continuity.as_deref(),
        reentry.as_deref(),
        text,
    ))
}

/// How many recent turns the re-entry recalls. A judgment of this loop's,
/// sized so the rebuilt context is a working set rather than the history
/// the flush just retired.
const RECALL_TURNS: u64 = 4;

/// The flush threshold: four fifths of capacity. The loop's own election,
/// checked before each turn so the wall is met by a flush rather than by
/// the overflow refusal.
fn pressured(resident: u64, capacity: u64) -> bool {
    capacity > 0 && resident.saturating_mul(5) >= capacity.saturating_mul(4)
}

/// The re-entry the loop composes from custody's recall: the system prompt
/// again, because the flush retired the resident copy, then the recent
/// conversation quoted so the model re-enters mid-dialogue rather than
/// cold. The quotes share one budget across the whole recall, so the
/// re-entry cannot rebuild the pressure the flush just relieved. A missing
/// recall composes the honest fallback: the model is told the context was
/// reset and nothing more.
fn reentry_text(recalled: Option<Vec<Recalled>>) -> String {
    let mut text = String::from(SYSTEM_PROMPT);
    text.push_str(
        "\n\nThe working context was reset to stay within its limit. \
         Recent conversation, restored from the session's record:",
    );
    let Some(events) = recalled else {
        text.push_str("\n(The record could not be reached: continue from the request alone.)");
        return text;
    };
    let mut budget = QUOTE_BOUND;
    for event in &events {
        let speaker = match event.kind.as_str() {
            "message.user" => "user",
            "message.assistant" => "assistant",
            "message.system" => continue,
            _ => continue,
        };
        let Some((_, content)) = event.pairs.iter().find(|(key, _)| key == "content") else {
            continue;
        };
        for line in quoted_texts(content, &mut budget) {
            text.push('\n');
            text.push_str(speaker);
            text.push_str(": ");
            text.push_str(&line);
        }
    }
    text
}

/// The text blocks out of a recalled content value, which is the canonical
/// JSON the election kept: an array of blocks, the text ones quoted and
/// everything else left where custody holds it. Every quote draws on the
/// one budget the whole re-entry shares, and a block arriving after the
/// budget is spent quotes nothing.
fn quoted_texts(content: &str, budget: &mut usize) -> Vec<String> {
    let Ok(blocks) = serde_json::from_str::<serde_json::Value>(content) else {
        return Vec::new();
    };
    let Some(blocks) = blocks.as_array() else {
        return Vec::new();
    };
    blocks
        .iter()
        .filter_map(|block| {
            let text = block.get("text")?.as_str()?;
            if *budget == 0 {
                return None;
            }
            let mut kept = String::new();
            for piece in text.split_whitespace() {
                if piece.len() > *budget {
                    *budget = 0;
                    kept.push_str(" ...");
                    break;
                }
                if !kept.is_empty() {
                    kept.push(' ');
                }
                kept.push_str(piece);
                *budget -= piece.len();
            }
            Some(kept)
        })
        .collect()
}

/// How much recalled text the whole re-entry quotes, one budget shared
/// across every message and block. The loop's own bound: a re-entry that
/// quoted everything would rebuild the pressure the flush just relieved.
const QUOTE_BOUND: usize = 600;

/// The loop's judgment over the served shape, and the place the three-way
/// division puts it: the custodian counted kinds without opinion, and
/// deciding that `turn.closed` counts a completed turn is this loop's
/// business. The shape's last run is the run now open - its load event
/// landed before any turn, per the tee attaching ahead of the run's
/// opening - so the earlier runs are the session's past, and a session
/// with no past injects nothing.
fn continuity_line(shape: &SessionShape) -> Option<String> {
    let prior = shape.runs.len().checked_sub(1)?;
    if prior == 0 {
        return None;
    }
    let turns: u64 = shape.runs[..prior]
        .iter()
        .map(|run| {
            run.kinds
                .iter()
                .find(|(kind, _)| kind == "turn.closed")
                .map_or(0, |(_, count)| *count)
        })
        .sum();
    let runs_word = if prior == 1 { "run" } else { "runs" };
    let turns_word = if turns == 1 { "turn" } else { "turns" };
    Some(format!(
        "Continuity: this session held {prior} earlier {runs_word} with {turns} \
         completed {turns_word} before this one. The conversation content of \
         those runs is not in your context."
    ))
}

/// The turn's delta from the loop's one judgment: the prompt leads a first
/// turn and no other, the continuity line riding beneath it where the
/// session holds a past. Pure, because the seat cannot be minted outside
/// loop zero - the E0624 blade of `weaver-harness-Spec` section 8 - so
/// what this loop can pin is its own contribution, and the seat's half is
/// loop zero's suite's to hold.
fn contribution(
    first_turn: bool,
    continuity: Option<&str>,
    reentry: Option<&str>,
    text: &str,
) -> Vec<Message> {
    let mut delta = Vec::new();
    if first_turn {
        let opening = match continuity {
            Some(line) => format!("{SYSTEM_PROMPT}\n\n{line}"),
            None => SYSTEM_PROMPT.to_string(),
        };
        delta.push(Message {
            role: Role::System,
            content: vec![ContentBlock::Text { text: opening }],
        });
    }
    if let Some(reentry) = reentry {
        // **The re-entry is the loop's voice and carries the loop's role.**
        // It opens with `SYSTEM_PROMPT`, the flush having retired the
        // resident copy, so under `Role::User` the whole prompt plus the
        // restored transcript came back quoted as the operator's words - and
        // a later recall would return it from the record as `message.user`,
        // compounding it. The opening and the re-entry are one voice framing
        // the field and they carry one role.
        delta.push(Message {
            role: Role::System,
            content: vec![ContentBlock::Text {
                text: reentry.to_string(),
            }],
        });
    }
    delta.push(Message {
        role: Role::User,
        content: vec![ContentBlock::Text {
            text: text.to_string(),
        }],
    });
    delta
}

#[cfg(test)]
mod tests {
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
}
