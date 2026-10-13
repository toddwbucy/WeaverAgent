//! conforms: diagnostic-record-identifies-itself-at-the-open
//! conforms: diagnostic-identity-absent-not-invented
//! conforms: diagnostic-outcome-absent-not-manufactured
//! conforms: diagnostic-divergence-position-is-the-resident-length
//! conforms: diagnostic-identity-refuses-an-unaccounted-input
//!
//! The diagnostic replay, per `diagnostic-replay-loop` and
//! `weaver-harness-Spec` section 6.2's second criterion: the seat granted
//! once at the run's opening, on the run itself as the work, the operator's
//! sealed preload what arrived owed an answer and the certification the
//! answer it is owed. The loop composes what the seat grants - the replay
//! ask, the decode surface, the record - and it mints no port.
//!
//! **This act is the null replay**: no reader pass, the certification of
//! the charter's step 2 alone. The recorded path re-feeds, the recomputed
//! token identifiers match the recorded ones exactly, integers, or the
//! certification fails naming the first divergent position. Every outcome
//! lands in the diagnostic-trace and nothing is answered outward, there
//! being no one on this seam to answer.
//!
//! **The five claims cited above are `weaver-diagnostic-Spec`'s records and
//! this crate's to hold.** That crate is the mechanism and this one is the
//! author, per `weaver-diagnostic-PRD` section 1, so each of the five carries
//! an `asserts` edge from this crate beside the one from the crate whose
//! record it describes, and the instruments are the suite below.
//! `weaver-harness-Spec` section 8 carries the custody from this side and
//! `weaver-diagnostic-Spec` section 7 carries the five claims.

use weaver_diagnostic::{
    AbandonReason, Divergence, Kind, ModelId, Payload, ReplayClosed, ReplayIdentity, ReplayOpened,
    ReplayOutcome, TemplateId, TokenId, WeightsHash,
};
use weaver_types::TurnKey;

use crate::engine::{Ports, TurnError};
use crate::state::Recalled;

/// How long the replay ask waits on the seal, milliseconds. The operator
/// sequences the driver after the load, per the walk's three acts, so this
/// bound spans a human running a program rather than a socket answering,
/// and an expiry is the abandoned outcome rather than a fault.
const REPLAY_ASK_BOUND_MS: u64 = 600_000;

/// One recorded generation, paired from the holdings: the request's members
/// beside the measurement's, in the landing order the pairing rule fixes.
struct SourceGeneration {
    rendered: String,
    template: String,
    sampling: serde_json::Value,
    input_tokens: Vec<u32>,
    output_tokens: Vec<u32>,
    model: String,
    weights_hash: String,
}

/// A resident edit the source made between turns, with the counts the
/// record carried for it. The replay reproduces each where it fell, so the
/// resident a later turn re-feeds onto is the one the source's turn drew on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Edit {
    Flush {
        before: u64,
        after: u64,
    },
    Elision {
        from: u64,
        to: u64,
        before: u64,
        after: u64,
    },
}

/// The holdings grouped: the recorded turns in landing order, and each edit
/// with the number of turns that preceded it.
struct Grouped {
    turns: Vec<(String, Vec<SourceGeneration>)>,
    edits: Vec<(usize, Edit)>,
}

/// Reproduces one recorded edit through the seat's own port and holds it to
/// the record's counts. `Ok(true)` continues the walk. `Ok(false)` means the
/// pass ended here with its account already recorded: a refused or dead
/// port authors its own refusal and no close follows, as a refused re-feed
/// ends the pass, and counts that disagree close it abandoned at identity,
/// since the replay would go on feeding a context the source never held.
fn reproduce(seat: &mut Ports<'_>, edit: Edit) -> Result<bool, TurnError> {
    let (answered, recorded, what) = match edit {
        Edit::Flush { before, after } => (seat.flush(after), (before, after), "flush"),
        Edit::Elision {
            from,
            to,
            before,
            after,
        } => (seat.elide(from, to), (before, after), "elision"),
    };
    let Some(answered) = answered else {
        return Ok(false);
    };
    if answered != recorded {
        refuse_identity(
            seat,
            format!(
                "the replay's {what} took the resident from {} to {} where the record's went from {} to {}",
                answered.0, answered.1, recorded.0, recorded.1
            ),
        )?;
        return Ok(false);
    }
    Ok(true)
}

/// The walk, whole: ask, group, establish identity, re-feed, compare,
/// close. Returns only on a channel-level failure the service cannot
/// survive: every outcome of the replay itself is the record's.
pub(crate) fn drive(
    seat: &mut Ports<'_>,
    reader_elected: bool,
    declared_artifact: &str,
) -> Result<(), TurnError> {
    // **The bracket opens first**, per `weaver-diagnostic-Spec` section 4:
    // a diagnostic-trace opens every bracket with `replay.opened`, which is
    // how a reader tells the two records apart, so it precedes even the
    // ask whose absence would abandon the pass.
    seat.author_replay(
        Kind::ReplayOpened,
        Payload::ReplayOpened(ReplayOpened { reader_elected }),
    )?;

    let Some(events) = seat.replay(REPLAY_ASK_BOUND_MS) else {
        // A leg that is down, an answer that is malformed, and a bound that
        // expired on a seal that never came are one outcome, per the loop
        // document's failure terms: no answer is no answer, and the account
        // is the record's.
        return close(
            seat,
            ReplayOutcome::Abandoned {
                reason: AbandonReason::ReplayAskUnanswered,
            },
        );
    };

    let Grouped { turns, edits } = match group(&events) {
        Ok(grouped) => grouped,
        Err(detail) => return refuse_identity(seat, detail),
    };

    // **Input identity, from the answered holdings and the declared
    // binding**, per the charter's step 1: what is about to feed is what
    // the record says was fed, established before any forward pass. The
    // items the null replay's claim requires are each required here, and a
    // record missing one fails now, which is completeness being
    // claim-relative. The state claim's own item, the tee's election, is
    // not required: the null replay rests on the recorded identifiers
    // alone, per the charter's step 1, and this pass claims nothing about
    // the state.
    let first = turns
        .first()
        .and_then(|(_, generations)| generations.first());
    let Some(first) = first else {
        return refuse_identity(seat, "the holdings pair no generation".to_string());
    };
    if first.model != declared_artifact {
        return refuse_identity(
            seat,
            format!(
                "the record replays {} and the load declared {}",
                first.model, declared_artifact
            ),
        );
    }
    let identity = ReplayIdentity {
        replayed_session: seat.session_name(),
        model: ModelId(first.model.clone()),
        weights_hash: WeightsHash(first.weights_hash.clone()),
        template: TemplateId(first.template.clone()),
    };
    seat.author_replay(Kind::ReplayIdentity, Payload::ReplayIdentity(identity))?;

    // **The walk, by turn, by generation, in landing order.** Each re-feed
    // computes the draws the source would have computed, and the recorded
    // token appends whatever the draw said, so every later position stays
    // comparable behind a divergence, but the certification itself fails at
    // the first divergent position, per the charter's step 2, and the pass
    // closes naming it.
    // **The source's resident edits are reproduced where they fell**, per
    // `diagnostic-replay-loop` section 2 as of 2026-09-26: a flush or an
    // elision between two turns is driven through the seat with the
    // record's counts before the later turn re-feeds, so that turn draws on
    // the resident the source's did. The M1 run `m1-002` flushed at 255 and
    // re-entered at 259, and a walk that passed the flush by re-fed turn 37
    // onto every earlier turn and diverged there (#673 item 1).
    for (index, (turn_key, generations)) in turns.iter().enumerate() {
        for &(_, edit) in edits.iter().filter(|(at, _)| *at == index) {
            if !reproduce(seat, edit)? {
                return Ok(());
            }
        }
        let turn = TurnKey(turn_key.clone());
        seat.replay_turn_started(&turn)?;
        for source in generations {
            let refed =
                match seat.refeed(&turn, source.rendered.clone(), source.output_tokens.clone()) {
                    Ok(generation) => generation,
                    // Every exit past the open closes the bracket, the serving
                    // close's own rule: the record channel is the sink and may
                    // still hold what the decode channel lost.
                    Err(TurnError::ChannelLost) => {
                        let _ = seat.replay_turn_stopped(&turn, weaver_trace::StopReason::Fault);
                        return Err(TurnError::ChannelLost);
                    }
                    // A seam refusal or fault mid-replay ends the pass where it
                    // stopped: the refusal is already authored inside the
                    // bracket, the bracket closes stopped, and no
                    // `replay.closed` follows - the not-ended outcome is the
                    // absence of one, per `weaver-diagnostic-Spec` section 3.
                    Err(error) => {
                        let reason = match error {
                            TurnError::Refused { .. } => weaver_trace::StopReason::Refused,
                            _ => weaver_trace::StopReason::Fault,
                        };
                        let _ = seat.replay_turn_stopped(&turn, reason);
                        return Ok(());
                    }
                };
            match compare(source, &refed) {
                Comparison::Matches => {}
                Comparison::Diverged(divergence) => {
                    seat.replay_turn_closed(&turn)?;
                    return close(seat, ReplayOutcome::Diverged { divergence });
                }
                Comparison::IdentityBroken(detail) => {
                    seat.replay_turn_closed(&turn)?;
                    return refuse_identity(seat, detail);
                }
            }
        }
        seat.replay_turn_closed(&turn)?;
    }
    for &(_, edit) in edits.iter().filter(|(at, _)| *at == turns.len()) {
        if !reproduce(seat, edit)? {
            return Ok(());
        }
    }

    close(seat, ReplayOutcome::Certified)
}

fn close(seat: &mut Ports<'_>, outcome: ReplayOutcome) -> Result<(), TurnError> {
    seat.author_replay(
        Kind::ReplayClosed,
        Payload::ReplayClosed(ReplayClosed { outcome }),
    )
}

fn refuse_identity(seat: &mut Ports<'_>, detail: String) -> Result<(), TurnError> {
    close(
        seat,
        ReplayOutcome::Abandoned {
            reason: AbandonReason::IdentityRefused { detail },
        },
    )
}

/// The grouping rule of the loop document's section 2: events group by run
/// and turn from their envelopes, in landing order, requests pairing to the
/// first unpaired measurement after them within one turn, and a grouping
/// the record does not determine rejects the replay before any forward
/// pass, naming what failed.
///
/// **One run is this pass's bound, named rather than silent**: the decode
/// session opened once at this run's enter, and a second recorded run
/// opened a second session the replay has no fresh context for, so
/// holdings spanning runs refuse at identity instead of replaying a
/// conversation across a boundary the source never crossed.
fn group(events: &[Recalled]) -> Result<Grouped, String> {
    let mut edits: Vec<(usize, Edit)> = Vec::new();
    // **A post-flush request is accounted for by a recall**, per
    // `diagnostic-replay-loop` section 2 as of 2026-09-26 (#690 item C2.10):
    // the seat's recall falls between a flush and the re-entry the next
    // request carries. `flushed` holds the sequence of a flush no recall has
    // followed yet, and `unaccounted` the first flush a request met instead.
    // Whether it refuses waits on `carries_recall`, read over the whole
    // holdings, since the record carries no format marker and the kind's
    // presence is what says its recorder and seat record recalls.
    let mut carries_recall = false;
    let mut flushed: Option<&str> = None;
    let mut unaccounted: Option<(String, String)> = None;
    let mut run: Option<&str> = None;
    let mut order: Vec<String> = Vec::new();
    let mut pending: std::collections::BTreeMap<String, PendingRequest> = Default::default();
    let mut paired: std::collections::BTreeMap<String, Vec<SourceGeneration>> = Default::default();
    for event in events {
        match run {
            None => run = Some(&event.run),
            Some(held) if held == event.run => {}
            Some(held) => {
                return Err(format!(
                    "the holdings span runs {held} and {}, and this pass replays one",
                    event.run
                ));
            }
        }
        match event.kind.as_str() {
            "model.request" => {
                let Some(turn) = &event.turn else {
                    return Err("a model.request carries no turn".to_string());
                };
                if !order.contains(turn) {
                    order.push(turn.clone());
                } else if order.last() != Some(turn) {
                    return Err(format!("turn {turn} resumes after another turn intervened"));
                }
                if pending.contains_key(turn) {
                    return Err(format!(
                        "two model.request events stand unpaired in turn {turn}"
                    ));
                }
                if let Some(flush) = flushed.take() {
                    unaccounted.get_or_insert((flush.to_string(), turn.clone()));
                }
                pending.insert(turn.clone(), request_members(event, turn)?);
            }
            "model.measurement" => {
                let Some(turn) = &event.turn else {
                    return Err("a model.measurement carries no turn".to_string());
                };
                let Some(request) = pending.remove(turn) else {
                    return Err(format!(
                        "a model.measurement in turn {turn} has no preceding unpaired request"
                    ));
                };
                paired
                    .entry(turn.clone())
                    .or_default()
                    .push(measurement_members(event, turn, request)?);
            }
            // **A resident edit is a step of the walk**, placed after the
            // turns that preceded it: it lands between turns, so one
            // arriving while a request stands unpaired is refused.
            "flush" | "elision" => {
                if let Some(turn) = pending.keys().next() {
                    return Err(format!("a {} lands inside turn {turn}", event.kind));
                }
                let count = |key: &str| -> Result<u64, String> {
                    pair(event, key)
                        .and_then(|raw| raw.parse().ok())
                        .ok_or_else(|| {
                            format!(
                                "the {} at sequence {} holds no {key}",
                                event.kind, event.sequence
                            )
                        })
                };
                let edit = if event.kind == "flush" {
                    Edit::Flush {
                        before: count("resident_before")?,
                        after: count("resident_after")?,
                    }
                } else {
                    Edit::Elision {
                        from: count("from")?,
                        to: count("to")?,
                        before: count("resident_before")?,
                        after: count("resident_after")?,
                    }
                };
                if event.kind == "flush" {
                    flushed = Some(&event.sequence);
                }
                edits.push((order.len(), edit));
            }
            // Only the seat's own recall accounts for a re-entry: the
            // enter's identity ask and the replay port's answer are recalls
            // of other asks, and an ask this walk cannot read accounts for
            // nothing.
            "recall" => {
                carries_recall = true;
                let verb = pair(event, "ask")
                    .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
                    .and_then(|ask| ask.get("verb").and_then(|v| v.as_str()).map(str::to_string));
                if verb.as_deref() == Some("recall") {
                    flushed = None;
                }
            }
            // **An open whose prefix went unrecorded does not certify**, per
            // `weaver-trace-Spec` section 3's recall clause: the harness names
            // a recall or a seated prefix the recorder would not take in an
            // `identity_prefix_unrecorded` fault, and a record missing the
            // provenance of the input its run opened under is not one a
            // certification can stand on.
            "fault" if pair(event, "case") == Some("\"identity_prefix_unrecorded\"") => {
                return Err(format!(
                    "the source's open recorded an identity_prefix_unrecorded fault at sequence {}, so its opening input is not in the record",
                    event.sequence
                ));
            }
            // Turnless events - the run brackets, the seated prefix, the
            // load - inform identity and feed nothing
            // positionally, and kinds this walk does not read pass by, per
            // the versionless-schema rule.
            _ => {}
        }
    }
    if let Some((turn, _)) = pending.iter().next() {
        return Err(format!("a model.request in turn {turn} pairs with nothing"));
    }
    if carries_recall && let Some((flush, turn)) = unaccounted {
        return Err(format!(
            "the flush at sequence {flush} is followed by turn {turn}'s request with no recall between them, on a record that carries the recall kind, so the post-flush input's provenance is not in the record"
        ));
    }
    let mut grouped = Vec::new();
    for turn in order {
        let generations = paired.remove(&turn).unwrap_or_default();
        if generations.is_empty() {
            return Err(format!("turn {turn} pairs no generation"));
        }
        grouped.push((turn, generations));
    }
    Ok(Grouped {
        turns: grouped,
        edits,
    })
}

/// The request's elected members, half a [`SourceGeneration`] until its
/// measurement lands.
struct PendingRequest {
    rendered: String,
    template: String,
    sampling: serde_json::Value,
}

fn pair<'a>(event: &'a Recalled, key: &str) -> Option<&'a str> {
    event
        .pairs
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

fn request_members(event: &Recalled, turn: &str) -> Result<PendingRequest, String> {
    let missing = |key: &str| format!("the model.request of turn {turn} holds no {key}");
    let rendered = pair(event, "rendered").ok_or_else(|| missing("rendered"))?;
    let rendered: String = serde_json::from_str(rendered)
        .map_err(|_| format!("the rendered form of turn {turn} does not parse"))?;
    let template = pair(event, "template").ok_or_else(|| missing("template"))?;
    let template: String = serde_json::from_str(template)
        .map_err(|_| format!("the template of turn {turn} does not parse"))?;
    let sampling = pair(event, "sampling").ok_or_else(|| missing("sampling"))?;
    let sampling: serde_json::Value = serde_json::from_str(sampling)
        .map_err(|_| format!("the sampling of turn {turn} does not parse"))?;
    Ok(PendingRequest {
        rendered,
        template,
        sampling,
    })
}

fn measurement_members(
    event: &Recalled,
    turn: &str,
    request: PendingRequest,
) -> Result<SourceGeneration, String> {
    let missing = |key: &str| format!("the model.measurement of turn {turn} holds no {key}");
    let parse_tokens = |raw: &str, key: &str| -> Result<Vec<u32>, String> {
        serde_json::from_str(raw).map_err(|_| format!("the {key} of turn {turn} do not parse"))
    };
    let input = pair(event, "input_tokens").ok_or_else(|| missing("input_tokens"))?;
    let output = pair(event, "output_tokens").ok_or_else(|| missing("output_tokens"))?;
    let model = pair(event, "model").ok_or_else(|| missing("model"))?;
    let model: String = serde_json::from_str(model)
        .map_err(|_| format!("the model of turn {turn} does not parse"))?;
    let weights_hash = pair(event, "weights_hash").ok_or_else(|| missing("weights_hash"))?;
    let weights_hash: String = serde_json::from_str(weights_hash)
        .map_err(|_| format!("the weights hash of turn {turn} does not parse"))?;
    let output_tokens = parse_tokens(output, "output_tokens")?;
    if output_tokens.is_empty() {
        return Err(format!("turn {turn} records a generation with no output"));
    }
    Ok(SourceGeneration {
        rendered: request.rendered,
        template: request.template,
        sampling: request.sampling,
        input_tokens: parse_tokens(input, "input_tokens")?,
        output_tokens,
        model,
        weights_hash,
    })
}

enum Comparison {
    Matches,
    Diverged(Divergence),
    IdentityBroken(String),
}

/// One generation's certification: the re-fed answer against the recorded
/// members. The token comparison is exact, integers, and the first
/// divergent position names both identifiers. **A position is the resident
/// length at the draw**, the coordinate `model.field` keys on, per
/// `weaver-diagnostic-Spec` section 3.3 on the ruling of 2026-09-09: the
/// re-fed answer's closing count less the drawn tokens less the terminator
/// is the first draw's position, and the appended input sits below it by
/// its own length, so a tokenization divergence and a draw divergence land
/// on the one scale the field row shares, and a reader holding only the
/// close event converts nothing.
fn compare(source: &SourceGeneration, refed: &weaver_types::Generation) -> Comparison {
    let measurement: serde_json::Value = match serde_json::from_str(refed.measurement.get()) {
        Ok(value) => value,
        Err(_) => {
            return Comparison::IdentityBroken("the re-fed measurement does not parse".into());
        }
    };
    let request: serde_json::Value = match serde_json::from_str(refed.request.get()) {
        Ok(value) => value,
        Err(_) => return Comparison::IdentityBroken("the re-fed request does not parse".into()),
    };
    let tokens = |value: &serde_json::Value, key: &str| -> Option<Vec<u32>> {
        serde_json::from_value(value.get(key)?.clone()).ok()
    };
    let Some(refed_input) = tokens(&measurement, "input_tokens") else {
        return Comparison::IdentityBroken("the re-fed measurement holds no input_tokens".into());
    };
    let Some(refed_output) = tokens(&measurement, "output_tokens") else {
        return Comparison::IdentityBroken("the re-fed measurement holds no output_tokens".into());
    };
    // The identity items whose live values only a pass can produce: the
    // weights hash and the template ride the re-fed answer, and the
    // sampling block must be the values the record says the source drew
    // under, the derived seed among them.
    if measurement.get("weights_hash")
        != Some(&serde_json::Value::String(source.weights_hash.clone()))
    {
        return Comparison::IdentityBroken(format!(
            "the loaded weights hash {:?} is not the recorded {}",
            measurement.get("weights_hash"),
            source.weights_hash
        ));
    }
    if request.get("template") != Some(&serde_json::Value::String(source.template.clone())) {
        return Comparison::IdentityBroken(format!(
            "the loaded template {:?} is not the recorded {}",
            request.get("template"),
            source.template
        ));
    }
    if request.get("sampling") != Some(&source.sampling) {
        return Comparison::IdentityBroken(format!(
            "the effective sampling {:?} is not the recorded {}",
            request.get("sampling"),
            source.sampling
        ));
    }
    // The floor of the tape this generation drew on, from the answer's
    // own closing count: the terminator landed after the last draw, so the
    // first draw sits the draws and one below the count, and the appended
    // input sits its own length below that.
    let Some(first_draw) = refed.resident.checked_sub(refed_output.len() as u64 + 1) else {
        return Comparison::IdentityBroken(format!(
            "the re-fed closing count {} cannot hold its {} draws and the terminator",
            refed.resident,
            refed_output.len()
        ));
    };
    let Some(input_floor) = first_draw.checked_sub(refed_input.len() as u64) else {
        return Comparison::IdentityBroken(format!(
            "the re-fed closing count {} cannot hold its {} appended input tokens",
            refed.resident,
            refed_input.len()
        ));
    };
    // Tokenization identity: the rendered form re-tokenized must be the
    // recorded appended input, per the loop document's re-feed clause,
    // exercised rather than assumed.
    for (position, (recorded, recomputed)) in source
        .input_tokens
        .iter()
        .zip(refed_input.iter())
        .enumerate()
    {
        if recorded != recomputed {
            return Comparison::Diverged(Divergence::TokenPath {
                position: input_floor + position as u64,
                recorded: TokenId(*recorded),
                recomputed: TokenId(*recomputed),
            });
        }
    }
    if source.input_tokens.len() != refed_input.len() {
        return Comparison::Diverged(Divergence::TokenPath {
            position: input_floor + source.input_tokens.len().min(refed_input.len()) as u64,
            recorded: TokenId(
                source
                    .input_tokens
                    .get(refed_input.len())
                    .copied()
                    .unwrap_or(0),
            ),
            recomputed: TokenId(
                refed_input
                    .get(source.input_tokens.len())
                    .copied()
                    .unwrap_or(0),
            ),
        });
    }
    // **The null comparison itself**: the recomputed draws in the output
    // slots against the recorded path, exactly, integers.
    for (ordinal, (recorded, recomputed)) in source
        .output_tokens
        .iter()
        .zip(refed_output.iter())
        .enumerate()
    {
        if recorded != recomputed {
            return Comparison::Diverged(Divergence::TokenPath {
                position: first_draw + ordinal as u64,
                recorded: TokenId(*recorded),
                recomputed: TokenId(*recomputed),
            });
        }
    }
    if source.output_tokens.len() != refed_output.len() {
        return Comparison::Diverged(Divergence::TokenPath {
            position: first_draw + source.output_tokens.len().min(refed_output.len()) as u64,
            recorded: TokenId(
                source
                    .output_tokens
                    .get(refed_output.len())
                    .copied()
                    .unwrap_or(0),
            ),
            recomputed: TokenId(
                refed_output
                    .get(source.output_tokens.len())
                    .copied()
                    .unwrap_or(0),
            ),
        });
    }
    Comparison::Matches
}

#[cfg(test)]
mod tests;
