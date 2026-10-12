use super::*;

use std::cell::RefCell;
use std::rc::Rc;

/// What the double saw, held behind a shared handle so a test can read it
/// back after the session has taken ownership of the backend.
#[derive(Default)]
struct Log {
    /// Every reseed the session asked for, in order: the seed it
    /// derived and the penalty window it read from the resident tail.
    reseeds: Vec<(u64, Vec<TokenId>)>,
    decoded: Vec<(Vec<TokenId>, usize)>,
    truncated: Vec<usize>,
    reestablished: usize,
    /// What `sample` returns, in order. Exhausting it yields token 0.
    script: Vec<TokenId>,
    sampled: usize,
    /// Fail the nth `decode_at` call, counting from zero.
    fail_decode_call: Option<usize>,
    /// Fail every `sample`.
    fail_sample: bool,
    /// The order the seam's two read paths were called in, which is what
    /// the pre-sampler claim is about.
    order: Vec<&'static str>,
    /// Fail the nth `distribution` call, counting from zero.
    fail_distribution_call: Option<usize>,
    distributions: std::cell::Cell<usize>,
    /// What `take_columns` answers. `None` is the unarmed tap, which is
    /// what every test that does not reach for the column wants.
    columns: Option<Vec<Vec<f32>>>,
}

/// A backend that records what it was asked to do. The seam this doubles is
/// the one Spec section 4.1 declares, so doubling it is reading the seam
/// rather than inventing one.
/// A fixed distribution the double serves. Non-uniform so the entropy is
/// a number rather than a degenerate zero, and **wide enough to cover
/// every token id the scripts draw**: a real distribution spans the
/// vocabulary, so a fixture narrower than its own script would measure
/// nothing on the tokens past its end and misreport the pairing.
const DISTRIBUTION: &[f32] = &[
    1.0, 2.0, 3.0, 4.0, 1.0, 2.0, 3.0, 4.0, 1.0, 2.0, 3.0, 4.0, 1.0, 2.0, 3.0, 4.0,
];

struct Recorder(Rc<RefCell<Log>>, Vec<f32>);

impl Backend for Recorder {
    /// The seam's own, recorded rather than acted on: this backend
    /// samples from a script, so what a reseed does here is note that
    /// it happened and with what.
    fn reseed(&mut self, seed: u64, window: &[TokenId]) -> Result<(), DecodeFault> {
        self.0.borrow_mut().reseeds.push((seed, window.to_vec()));
        Ok(())
    }

    fn decode_at(&mut self, tokens: &[TokenId], position: usize) -> Result<(), DecodeFault> {
        let mut log = self.0.borrow_mut();
        let call = log.decoded.len();
        log.decoded.push((tokens.to_vec(), position));
        if log.fail_decode_call == Some(call) {
            return Err(DecodeFault::Engine {
                detail: "scripted".into(),
            });
        }
        Ok(())
    }
    fn distribution(&self) -> Result<&[f32], DecodeFault> {
        let call = {
            let mut log = self.0.borrow_mut();
            log.order.push("distribution");
            let call = log.distributions.get();
            log.distributions.set(call + 1);
            (call, log.fail_distribution_call)
        };
        if Some(call.0) == call.1 {
            return Err(DecodeFault::Engine {
                detail: "no distribution".into(),
            });
        }
        // **The double serves a fixed distribution**, which is enough for
        // the ordering claim: what the signals test reads is that the
        // measurement was taken from the state the sampler had not yet
        // consumed, not what the numbers were.
        Ok(&self.1)
    }
    fn sample(&mut self) -> Result<TokenId, DecodeFault> {
        let mut log = self.0.borrow_mut();
        log.order.push("sample");
        if log.fail_sample {
            return Err(DecodeFault::Engine {
                detail: "scripted".into(),
            });
        }
        let token = log.script.get(log.sampled).copied().unwrap_or(TokenId(0));
        log.sampled += 1;
        Ok(token)
    }
    fn take_columns(&mut self) -> Option<Vec<Vec<f32>>> {
        self.0.borrow().columns.clone()
    }
    fn truncate_to(&mut self, position: usize) -> Result<(), DecodeFault> {
        self.0.borrow_mut().truncated.push(position);
        Ok(())
    }
    fn reestablish(&mut self) -> Result<(), DecodeFault> {
        self.0.borrow_mut().reestablished += 1;
        Ok(())
    }
    fn close(&mut self) {}
}

/// **Each positioned sink fires at the position it names, and the
/// unelected field fires at none.**
///
/// The field's depth and its sink are one member, so what holds the
/// pairing is a run that elects a depth and counts what arrives against a
/// run that elects none and counts nothing. Both sinks are dispatched
/// from the same loop and carry the same shape of position, which is the
/// pair a wrong wiring would swap without the compiler noticing, the
/// ranked candidates and the layer stack being different types only
/// inside their vectors.
#[test]
fn each_positioned_sink_fires_at_the_position_it_names() {
    let columns = vec![vec![0.5_f32, 0.25], vec![0.125, 0.0625]];
    let log = Rc::new(RefCell::new(Log {
        script: vec![TokenId(1), TokenId(2), TokenId(0)],
        columns: Some(columns.clone()),
        ..Default::default()
    }));
    let mut session = Session::new(
        Box::new(Recorder(Rc::clone(&log), DISTRIBUTION.to_vec())),
        64,
        FlushMechanism::TruncateToPosition,
        false,
    );
    session.open(&[TokenId(7)]).expect("the prefix lands");
    session.ask_columns();

    let mut fields: Vec<(u64, usize, u32)> = Vec::new();
    let mut taken: Vec<(u64, Vec<Vec<f32>>)> = Vec::new();
    let mut on_field = |position: u64, ranked: Vec<(u32, f32)>, realized: u32| {
        fields.push((position, ranked.len(), realized));
    };
    session
        .append_and_generate(
            &[TokenId(8)],
            &StopCondition {
                stop_tokens: vec![TokenId(0)],
                terminator: TokenId(0),
                max_tokens: 8,
            },
            &mut NeverCancels,
            &mut |_| {},
            PositionedSinks {
                field: Some((4, &mut on_field as FieldSink)),
                on_column: &mut |position: u64, layers: Vec<Vec<f32>>| {
                    taken.push((position, layers));
                },
            },
            SamplerBuild {
                seed: 11,
                penalty_window: 64,
            },
        )
        .expect("the generation runs");

    assert!(
        !fields.is_empty(),
        "an elected depth reaches the field sink"
    );
    assert!(
        fields.iter().all(|(_, ranked, _)| *ranked == 4),
        "the elected depth is the ranking's width: {fields:?}"
    );
    assert!(!taken.is_empty(), "an armed tap reaches the column sink");
    assert!(
        taken.iter().all(|(_, layers)| *layers == columns),
        "the column sink takes the forward's layers, not the field's: {taken:?}"
    );
    // **The column is taken at the draw site and the field only where the
    // token is retained**, so the stop draw takes a column and owes no
    // field. The positions are therefore a prefix rather than a match, and
    // asserting equality here would be asserting against the rule the loop
    // states above the stop check.
    let field_positions: Vec<u64> = fields.iter().map(|(p, _, _)| *p).collect();
    let column_positions: Vec<u64> = taken.iter().map(|(p, _)| *p).collect();
    // **The absolute positions, because the two checks below are relative
    // and a shared wrong offset satisfies both.** The fixture fixes them:
    // the open leaves one token resident, the delta a second, and the
    // three scripted draws sit at 2, 3 and 4, the last being the stop.
    assert_eq!(
        column_positions,
        vec![2, 3, 4],
        "a column at each draw site, counted from the resident length"
    );
    assert_eq!(
        field_positions,
        vec![2, 3],
        "a field at each retained draw, the stop owing none"
    );
    assert_eq!(
        column_positions.len(),
        field_positions.len() + 1,
        "the stop draw takes a column and owes no field: {column_positions:?} \
         against {field_positions:?}"
    );
    assert_eq!(
        field_positions,
        column_positions[..field_positions.len()],
        "where both fire they name one position, not two readings of it"
    );
}

/// **The re-feed appends the recorded token whatever the draw said, and
/// the draws land in the answer's output slots.** The scripted backend
/// draws one path while the recorded path is another, so the two are
/// distinguishable at every position: what became resident must be the
/// recorded path and the terminator, the drawn identifiers must ride
/// `Generated::tokens`, and the signals must count one measurement per
/// recorded position, per `weaver-spu-Spec` section 4.6.
///
/// Perturbation: append the draw instead of the recorded token in
/// `refeed` and this fails, the decode log carrying the script where the
/// path belongs. Watched under exactly that change.
///
/// conforms: spu-refeed-appends-the-recorded-path
#[test]
fn the_refeed_appends_the_recorded_token_whatever_the_draw_said() {
    let log = Rc::new(RefCell::new(Log {
        script: vec![TokenId(1), TokenId(2), TokenId(3)],
        ..Default::default()
    }));
    let mut session = Session::new(
        Box::new(Recorder(Rc::clone(&log), DISTRIBUTION.to_vec())),
        64,
        FlushMechanism::TruncateToPosition,
        false,
    );
    session.open(&[TokenId(4)]).expect("the prefix lands");
    let generated = session
        .refeed(
            &[TokenId(5)],
            &[TokenId(7), TokenId(8), TokenId(9)],
            TokenId(0),
            &mut NeverCancels,
            PositionedSinks {
                field: None,
                on_column: &mut |_, _| {},
            },
            SamplerBuild {
                seed: 11,
                penalty_window: 64,
            },
        )
        .expect("the re-feed runs");

    assert_eq!(
        generated.tokens,
        vec![TokenId(1), TokenId(2), TokenId(3)],
        "the recomputed draws fill the output slots"
    );
    let decoded = log.borrow().decoded.clone();
    assert_eq!(
        decoded,
        vec![
            (vec![TokenId(4)], 0),
            (vec![TokenId(5)], 1),
            (vec![TokenId(7)], 2),
            (vec![TokenId(8)], 3),
            (vec![TokenId(9)], 4),
            (vec![TokenId(0)], 5),
        ],
        "the recorded token appends at every position, then the terminator, never the draw"
    );
    assert_eq!(
        generated.signals.steps(),
        3,
        "one measurement per recorded position"
    );
    assert_eq!(
        log.borrow().reseeds,
        vec![(11, vec![TokenId(4), TokenId(5)])],
        "the sampler reseeds after the prefill exactly as a generation does"
    );
}

/// **An asked column that does not arrive at a sampled position
/// faults**, per charter section 13.10 and Spec section 7: the ask
/// passed the registry, so a backend whose tap holds no column - the
/// native tap's own shape - faults loudly rather than omitting
/// silently. The scripted backend's `take_columns` answers `None`,
/// which is exactly that backend.
///
/// Perturbation: restore the silent skip at the draw site - emit only
/// where the take answers - and the armed generation completes with no
/// column and no fault. Watched under exactly that restoration.
///
/// conforms: spu-asked-column-arrives-or-faults
#[test]
fn an_asked_column_that_does_not_arrive_faults() {
    let log = Rc::new(RefCell::new(Log {
        script: vec![TokenId(1), TokenId(9)],
        ..Default::default()
    }));
    let mut session = Session::new(
        Box::new(Recorder(Rc::clone(&log), DISTRIBUTION.to_vec())),
        64,
        FlushMechanism::TruncateToPosition,
        false,
    );
    session.open(&[TokenId(7)]).expect("the prefix lands");
    session.ask_columns();
    let outcome = session.append_and_generate(
        &[TokenId(8)],
        &StopCondition {
            stop_tokens: vec![TokenId(9)],
            terminator: TokenId(0),
            max_tokens: 8,
        },
        &mut NeverCancels,
        &mut |_| {},
        PositionedSinks {
            field: None,
            on_column: &mut |_, _| {},
        },
        SamplerBuild {
            seed: 11,
            penalty_window: 64,
        },
    );
    assert!(
        matches!(outcome, Err(DecodeFault::Engine { .. })),
        "the silent absence faults: {outcome:?}"
    );
}

/// **A step that cannot be measured makes the whole measurement absent**,
/// not a vector one short.
///
/// The vectors are positionally paired with the tokens the answer carries,
/// so dropping one entry shifts every entry after it onto the wrong token.
/// A reader cannot detect that; a reader can detect absence.
///
/// Perturbation: replace the `None => signals.abandon()` arm with a plain
/// skip and this fails, the run answering two measurements against three
/// tokens. Watched under exactly that change.
#[test]
fn a_step_that_cannot_be_measured_makes_the_whole_run_absent() {
    let log = Rc::new(RefCell::new(Log {
        script: vec![TokenId(1), TokenId(2), TokenId(3), TokenId(9)],
        // The second retained token loses its distribution.
        fail_distribution_call: Some(1),
        ..Default::default()
    }));
    let mut session = Session::new(
        Box::new(Recorder(Rc::clone(&log), DISTRIBUTION.to_vec())),
        64,
        FlushMechanism::TruncateToPosition,
        false,
    );
    session.open(&[TokenId(7)]).expect("the prefix lands");
    let generated = session
        .append_and_generate(
            &[TokenId(8)],
            &StopCondition {
                stop_tokens: vec![TokenId(9)],
                terminator: TokenId(0),
                max_tokens: 8,
            },
            &mut NeverCancels,
            &mut |_| {},
            PositionedSinks {
                field: None,
                on_column: &mut |_, _| {},
            },
            SamplerBuild {
                seed: 11,
                penalty_window: 64,
            },
        )
        .expect("the generation runs");

    assert_eq!(generated.tokens.len(), 3, "the generation still answers");
    assert_eq!(
        generated.signals.steps(),
        0,
        "and the measurement is absent rather than short"
    );
    assert!(generated.signals.entropy_bits.is_none());
    assert!(generated.signals.surprisal_bits.is_none());
}

/// **The signals are measured before the sampler, at every step.**
///
/// The distribution exists until the draw consumes it, so a measurement
/// taken after `sample` is reading a state the draw has already moved
/// past. What this reads is the seam's call order: every `distribution`
/// sits immediately before the `sample` it measured, never after.
///
/// Perturbation: move the `distribution` read below the `sample` call in
/// `append_and_generate` and this fails, the recorded order arriving as
/// sample-then-distribution. Watched under exactly that move.
#[test]
fn the_distribution_is_read_before_the_draw_at_every_step() {
    let log = Rc::new(RefCell::new(Log {
        script: vec![TokenId(1), TokenId(2), TokenId(9)],
        ..Default::default()
    }));
    let mut session = Session::new(
        Box::new(Recorder(Rc::clone(&log), DISTRIBUTION.to_vec())),
        64,
        FlushMechanism::TruncateToPosition,
        false,
    );
    session.open(&[TokenId(7)]).expect("the prefix lands");
    let generated = session
        .append_and_generate(
            &[TokenId(8)],
            &StopCondition {
                stop_tokens: vec![TokenId(9)],
                terminator: TokenId(0),
                max_tokens: 8,
            },
            &mut NeverCancels,
            &mut |_| {},
            PositionedSinks {
                field: None,
                on_column: &mut |_, _| {},
            },
            SamplerBuild {
                seed: 11,
                penalty_window: 64,
            },
        )
        .expect("the generation runs");

    let order = log.borrow().order.clone();
    assert!(!order.is_empty(), "the seam was read at all");
    for pair in order.chunks(2) {
        assert_eq!(
            pair,
            ["distribution", "sample"],
            "every measurement precedes its draw, got {order:?}"
        );
    }

    // **The signals pair with the tokens the answer carries.** The stop
    // token is drawn and measured against, but it is not retained, so it
    // owes no entry: a signal for a token the answer omits shifts every
    // reader's pairing by one.
    assert_eq!(generated.tokens.len(), 2, "two tokens before the stop");
    assert_eq!(
        generated.signals.steps(),
        generated.tokens.len(),
        "one measurement per retained token, positionally paired"
    );
    assert_eq!(order.len() / 2, 3, "three draws, the stop token among them");
}

/// A cancel mid-path stops the re-feed where the poll saw it, and the
/// session is left well framed: the recorded prefix that landed, the
/// terminator after it, and one recomputed draw and one measurement per
/// landed position. This is what the caller reads to know that a cancelled
/// re-feed certifies a prefix rather than the path it was handed.
///
/// **The caller faults on it rather than answering.** The drive is given
/// the whole path upfront, so the text it owes is the record's while the
/// measurement is the run's, and a cancel parts them. A part-way answer
/// could only certify a prefix, which is the nothing the empty-path arm
/// refuses, so `main.rs` treats a cancelled re-feed as a fault. This test
/// pins what the session does under the cancel, not what the binary
/// decides about it.
///
/// **The whole decode log is asserted, positions included**, the way the
/// sibling above asserts it. A value-only reading cannot see a recorded
/// token landed at the wrong absolute position, which the append-only
/// discipline this module pins forbids, and it drops the terminator
/// entirely because the terminator is in no path.
///
/// Perturbations watched, each run under exactly the change named. Land
/// the recorded token before honouring the cancel and the log carries
/// three recorded positions against two draws. Return from the cancel arm
/// without landing the terminator and the log ends at the last recorded
/// token, which `spu-terminator-on-every-path` forbids. Move the signals
/// record above the cancel check and the step count runs one past the
/// draws it is paired with.
///
/// **What this does not watch.** The caller's fault is taken in `main.rs`,
/// which is the binary, so nothing here would go red if that branch were
/// dropped and a cancelled re-feed answered instead. A seam-level
/// cancelled re-feed would catch it and there is none, which is bounded
/// by the fact that nothing can produce one: the decode socket's peer is
/// the harness by descriptor possession, and the harness refuses every
/// directive during this drive rather than cancelling it.
#[test]
fn a_cancelled_refeed_lands_only_the_prefix_its_draws_count() {
    let log = Rc::new(RefCell::new(Log {
        script: vec![TokenId(1), TokenId(2), TokenId(3)],
        ..Default::default()
    }));
    let mut session = Session::new(
        Box::new(Recorder(Rc::clone(&log), DISTRIBUTION.to_vec())),
        64,
        FlushMechanism::TruncateToPosition,
        false,
    );
    session.open(&[TokenId(4)]).expect("the prefix lands");
    let path = [TokenId(7), TokenId(8), TokenId(9)];
    let generated = session
        .refeed(
            &[TokenId(5)],
            &path,
            TokenId(0),
            &mut CancelsAfter { polls: 0, limit: 2 },
            PositionedSinks {
                field: None,
                on_column: &mut |_, _| {},
            },
            SamplerBuild {
                seed: 11,
                penalty_window: 64,
            },
        )
        .expect("the re-feed runs");

    assert_eq!(
        generated.stopped,
        Stopped::Cancelled,
        "the cancel is what stopped it"
    );
    assert_eq!(
        generated.tokens.len(),
        2,
        "one recomputed draw per recorded token landed, and the third never was"
    );
    assert_eq!(
        generated.signals.steps(),
        generated.tokens.len(),
        "one measurement per landed position, the pairing a break could part"
    );
    assert_eq!(
        log.borrow().decoded.clone(),
        vec![
            (vec![TokenId(4)], 0),
            (vec![TokenId(5)], 1),
            (vec![TokenId(7)], 2),
            (vec![TokenId(8)], 3),
            (vec![TokenId(0)], 4),
        ],
        "two recorded positions and then the terminator, the cancelled third \
         never landing, so the prefix the draw count names is what the \
         emission may report as said"
    );
}

/// Cancels after a set number of polls, which is how a token-boundary bound
/// is made observable without a clock.
struct CancelsAfter {
    polls: usize,
    limit: usize,
}

impl CancelPoll for CancelsAfter {
    fn cancelled(&mut self) -> bool {
        let now = self.polls >= self.limit;
        self.polls += 1;
        now
    }
}

fn stop_at(terminator: u32) -> StopCondition {
    StopCondition {
        stop_tokens: vec![TokenId(99)],
        terminator: TokenId(terminator),
        max_tokens: 64,
    }
}

fn with_mechanism(
    script: Vec<TokenId>,
    capacity: usize,
    mechanism: FlushMechanism,
) -> (Session<'static>, Rc<RefCell<Log>>) {
    let log = Rc::new(RefCell::new(Log {
        script,
        ..Default::default()
    }));
    let mut session = Session::new(
        Box::new(Recorder(Rc::clone(&log), DISTRIBUTION.to_vec())),
        capacity,
        mechanism,
        false,
    );
    session.open(&[TokenId(1), TokenId(2)]).expect("open");
    (session, log)
}

fn opened(script: Vec<TokenId>, capacity: usize) -> (Session<'static>, Rc<RefCell<Log>>) {
    with_mechanism(script, capacity, FlushMechanism::TruncateToPosition)
}

fn last_decoded(log: &Rc<RefCell<Log>>) -> Vec<TokenId> {
    log.borrow()
        .decoded
        .last()
        .map(|(tokens, _)| tokens.clone())
        .unwrap_or_default()
}

/// **Every generation decodes the delta at the resident end**, never back
/// at the prefix. This reads the position the backend was asked to decode
/// at, which is the property itself.
///
/// An earlier version of this test asserted only that `resident_len` grew
/// across turns, and **it could not fail**: under the rewind, the second
/// turn decodes at the prefix, runs past its exhausted script to
/// `max_tokens`, and ends up longer than the first turn anyway. Growth was
/// passing for non-rewinding. The position is the thing.
///
/// Perturbation: decode the delta at `self.prefix_len` instead of
/// `self.resident_len`, which is the re-prefill rewind the archived tree's
/// proof warns about, and this test fails on the recorded position. Watched
/// under exactly that change.
/// **The stream fires for exactly the retained tokens, in order.** The
/// on_token callback is the seam's stream, so what it fires must equal the
/// generation's tokens, no more and no fewer: the stop token is drawn and
/// not retained, so it is never streamed, and a consumer accumulating the
/// stream holds the same sequence the close carries.
///
/// Perturbation: fire on_token before the stop-token check instead of
/// after the retain, and this test fails, the stop token appearing in the
/// stream but not in `Generated.tokens`. Watched under exactly that change.
#[test]
fn the_stream_fires_for_exactly_the_retained_tokens() {
    // A script whose third token is the stop token, so the run retains two
    // and stops on the third, which must not stream.
    let (mut session, _log) = opened(vec![TokenId(7), TokenId(8), TokenId(9)], 128);
    let mut cancel = NeverCancels;
    let mut streamed: Vec<TokenId> = Vec::new();
    let generated = session
        .append_and_generate(
            &[TokenId(3)],
            &StopCondition {
                stop_tokens: vec![TokenId(9)],
                terminator: TokenId(0),
                max_tokens: 50,
            },
            &mut cancel,
            &mut |token| streamed.push(token),
            PositionedSinks {
                field: None,
                on_column: &mut |_, _| {},
            },
            SamplerBuild {
                seed: 11,
                penalty_window: 64,
            },
        )
        .expect("the generation runs");
    assert_eq!(
        streamed, generated.tokens,
        "the stream equals the retained tokens in order"
    );
    assert!(
        !streamed.contains(&TokenId(9)),
        "the stop token is drawn and not streamed"
    );
}

#[test]
fn the_delta_decodes_at_the_resident_end_never_at_the_prefix() {
    // A script that stops immediately, so generation adds a known amount
    // and cannot mask a wrong position by running long.
    let (mut session, log) = opened(vec![TokenId(99), TokenId(99)], 128);
    let mut cancel = NeverCancels;

    session
        .append_and_generate(
            &[TokenId(3)],
            &stop_at(50),
            &mut cancel,
            &mut |_| {},
            PositionedSinks {
                field: None,
                on_column: &mut |_, _| {},
            },
            SamplerBuild {
                seed: 11,
                penalty_window: 64,
            },
        )
        .expect("first turn");
    let after_first = session.resident_len();
    assert!(
        after_first > session.prefix_len(),
        "the first turn extended"
    );

    let before = log.borrow().decoded.len();
    session
        .append_and_generate(
            &[TokenId(4)],
            &stop_at(50),
            &mut cancel,
            &mut |_| {},
            PositionedSinks {
                field: None,
                on_column: &mut |_, _| {},
            },
            SamplerBuild {
                seed: 11,
                penalty_window: 64,
            },
        )
        .expect("second turn");

    let (tokens, position) = log.borrow().decoded[before].clone();
    assert_eq!(tokens, vec![TokenId(4)], "the second delta is what landed");
    assert_eq!(
        position, after_first,
        "the delta decodes at the resident end, not at the prefix"
    );
}

/// The resident length never falls across turns that ask no reducer, which
/// is the behavioural half beside the position check above.
#[test]
fn the_resident_length_never_rewinds_across_turns() {
    let (mut session, _log) = opened(vec![TokenId(99), TokenId(99)], 128);
    let mut cancel = NeverCancels;
    let mut seen = session.resident_len();
    for delta in [TokenId(3), TokenId(4), TokenId(5)] {
        session
            .append_and_generate(
                &[delta],
                &stop_at(50),
                &mut cancel,
                &mut |_| {},
                PositionedSinks {
                    field: None,
                    on_column: &mut |_, _| {},
                },
                SamplerBuild {
                    seed: 11,
                    penalty_window: 64,
                },
            )
            .expect("a turn");
        assert!(
            session.resident_len() > seen,
            "each turn extends, never rewinding"
        );
        seen = session.resident_len();
    }
}

/// **The turn terminator is made resident on the clean path.**
///
/// Perturbation: move the terminator decode inside the `Stopped::Complete`
/// arm, or delete it, and this test fails because the last decoded token is
/// no longer the terminator. Watched under exactly that deletion.
#[test]
fn the_terminator_lands_on_the_clean_path() {
    let (mut session, log) = opened(vec![TokenId(7), TokenId(99)], 128);
    let mut cancel = NeverCancels;
    let generated = session
        .append_and_generate(
            &[TokenId(3)],
            &stop_at(50),
            &mut cancel,
            &mut |_| {},
            PositionedSinks {
                field: None,
                on_column: &mut |_, _| {},
            },
            SamplerBuild {
                seed: 11,
                penalty_window: 64,
            },
        )
        .expect("a turn");
    assert_eq!(generated.stopped, Stopped::Complete);
    assert_eq!(
        generated.tokens,
        vec![TokenId(7)],
        "99 stops, 7 is produced"
    );
    assert_eq!(last_decoded(&log), vec![TokenId(50)]);
}

/// **And on the cancelled path alike**, which is what makes the cancel leave
/// a well-framed session rather than one that ends mid-message.
///
/// Perturbation: return early from the `Stopped::Cancelled` break without
/// falling through to the terminator decode and this test fails. Watched
/// under exactly that early return.
#[test]
fn the_terminator_lands_on_the_cancelled_path_too() {
    let (mut session, log) = opened(vec![TokenId(7), TokenId(8), TokenId(9)], 128);
    let mut cancel = CancelsAfter { polls: 0, limit: 2 };
    let generated = session
        .append_and_generate(
            &[TokenId(3)],
            &stop_at(50),
            &mut cancel,
            &mut |_| {},
            PositionedSinks {
                field: None,
                on_column: &mut |_, _| {},
            },
            SamplerBuild {
                seed: 11,
                penalty_window: 64,
            },
        )
        .expect("a cancelled turn");
    assert_eq!(generated.stopped, Stopped::Cancelled);
    assert_eq!(
        last_decoded(&log),
        vec![TokenId(50)],
        "the terminator is resident even though the turn was stopped"
    );
}

/// **The cancel is bounded by one token.** Cancelling at the second poll
/// yields exactly the tokens sampled before it, never more, so the stop
/// costs one token's decode rather than a kernel's completion.
///
/// Perturbation: move the cancel check to after the sample-and-decode, and
/// this test fails because a third token is produced. Watched under exactly
/// that move.
#[test]
fn the_cancel_is_bounded_by_one_token() {
    let (mut session, _log) = opened(vec![TokenId(7), TokenId(8), TokenId(9)], 128);
    let mut cancel = CancelsAfter { polls: 0, limit: 2 };
    let generated = session
        .append_and_generate(
            &[TokenId(3)],
            &stop_at(50),
            &mut cancel,
            &mut |_| {},
            PositionedSinks {
                field: None,
                on_column: &mut |_, _| {},
            },
            SamplerBuild {
                seed: 11,
                penalty_window: 64,
            },
        )
        .expect("a cancelled turn");
    assert_eq!(
        generated.tokens,
        vec![TokenId(7), TokenId(8)],
        "two polls passed, so exactly two tokens were produced"
    );
}

/// **Overflow refuses and sheds nothing**, and the refusal carries the
/// session's own account so the harness can decide with the numbers.
#[test]
fn an_overflowing_delta_refuses_with_the_session_s_account() {
    let (mut session, _log) = opened(vec![], 8);
    let mut cancel = NeverCancels;
    let refusal = session.append_and_generate(
        &[
            TokenId(1),
            TokenId(2),
            TokenId(3),
            TokenId(4),
            TokenId(5),
            TokenId(6),
            TokenId(7),
        ],
        &stop_at(50),
        &mut cancel,
        &mut |_| {},
        PositionedSinks {
            field: None,
            on_column: &mut |_, _| {},
        },
        SamplerBuild {
            seed: 11,
            penalty_window: 64,
        },
    );
    match refusal {
        Err(DecodeFault::Overflow {
            resident,
            requested,
            capacity,
        }) => {
            assert_eq!((resident, requested, capacity), (2, 7, 8));
        }
        other => panic!("an overflowing delta refuses, got {other:?}"),
    }
    assert_eq!(
        session.resident_len(),
        2,
        "and sheds nothing: the session is untouched"
    );
}

/// **The flush mechanism is read from the declaration**, not inferred. A
/// truncating family truncates to the prefix.
#[test]
fn a_truncating_family_flushes_by_truncating_to_the_prefix() {
    let (mut session, log) = opened(vec![TokenId(99)], 128);
    let mut cancel = NeverCancels;
    session
        .append_and_generate(
            &[TokenId(3)],
            &stop_at(50),
            &mut cancel,
            &mut |_| {},
            PositionedSinks {
                field: None,
                on_column: &mut |_, _| {},
            },
            SamplerBuild {
                seed: 11,
                penalty_window: 64,
            },
        )
        .expect("a turn");
    session.flush(0).expect("flush");
    assert_eq!(session.resident_len(), session.prefix_len());
    assert_eq!(log.borrow().truncated.clone(), vec![2]);
}

/// **The cut's bounds hold at their edges**, per the decode contract's
/// conformance list: zero, a middle length, the resident count, and
/// beyond it each resolve to the clamped length, and the identity
/// prefix is never cut.
#[test]
fn the_cuts_bounds_hold_at_their_edges() {
    let grown = |mechanism| {
        let (mut session, log) = with_mechanism(vec![TokenId(99), TokenId(98)], 128, mechanism);
        let mut cancel = NeverCancels;
        session
            .append_and_generate(
                &[TokenId(3), TokenId(4)],
                &stop_at(50),
                &mut cancel,
                &mut |_| {},
                PositionedSinks {
                    field: None,
                    on_column: &mut |_, _| {},
                },
                SamplerBuild {
                    seed: 11,
                    penalty_window: 64,
                },
            )
            .expect("a turn");
        (session, log)
    };
    for mechanism in [
        FlushMechanism::TruncateToPosition,
        FlushMechanism::ReestablishAndReprefill,
    ] {
        // Beyond the resident count: nothing to cut, nothing moves.
        let (mut session, _log) = grown(mechanism);
        let standing = session.resident_len();
        session.flush(standing + 10).expect("flush");
        assert_eq!(session.resident_len(), standing, "{mechanism:?} beyond");

        // At the resident count: the cut lands exactly there.
        session.flush(standing).expect("flush");
        assert_eq!(session.resident_len(), standing, "{mechanism:?} at");

        // A middle length, inside the span: the cut is the ask's.
        let middle = session.prefix_len() + 1;
        session.flush(middle).expect("flush");
        assert_eq!(session.resident_len(), middle, "{mechanism:?} middle");

        // Zero: clamped up to the identity prefix, never below it.
        session.flush(0).expect("flush");
        assert_eq!(
            session.resident_len(),
            session.prefix_len(),
            "{mechanism:?} zero clamps to the prefix"
        );
    }
}

/// A family that cannot roll back is re-established and re-prefilled, which
/// is expensive and correct where the cheap path is silently wrong.
#[test]
fn a_non_rewinding_family_flushes_by_reestablishing() {
    let (mut session, log) = with_mechanism(
        vec![TokenId(99)],
        128,
        FlushMechanism::ReestablishAndReprefill,
    );
    session.flush(0).expect("flush");
    assert_eq!(session.resident_len(), 2);
    assert_eq!(log.borrow().reestablished, 1);
    assert_eq!(log.borrow().truncated.clone(), Vec::<usize>::new());
}

/// **Open is first and happens once.** A second open refuses rather than
/// silently rewinding the resident length over accumulated turns.
///
/// Perturbation: remove the `opened` guard from `open` and this test fails,
/// because the second open succeeds and the length rewinds to the prefix.
/// Watched under exactly that removal.
#[test]
fn a_second_open_refuses_rather_than_rewinding() {
    let (mut session, _log) = opened(vec![TokenId(99)], 128);
    let mut cancel = NeverCancels;
    session
        .append_and_generate(
            &[TokenId(3)],
            &stop_at(50),
            &mut cancel,
            &mut |_| {},
            PositionedSinks {
                field: None,
                on_column: &mut |_, _| {},
            },
            SamplerBuild {
                seed: 11,
                penalty_window: 64,
            },
        )
        .expect("a turn");
    let grown = session.resident_len();
    assert_eq!(
        session.open(&[TokenId(1)]),
        Err(DecodeFault::AlreadyOpen),
        "a second open refuses"
    );
    assert_eq!(session.resident_len(), grown, "and rewinds nothing");
}

/// **A flush that fails partway closes the session** rather than leaving a
/// large resident length over an empty backend, where the next append would
/// decode far past the backend's real state.
///
/// Perturbation: remove the poisoning from `flush` and this test fails,
/// because the session stays open with its pre-flush length and the append
/// runs. Watched under exactly that removal.
#[test]
fn a_failed_flush_closes_the_session() {
    let (mut session, log) = with_mechanism(
        vec![TokenId(99)],
        128,
        FlushMechanism::ReestablishAndReprefill,
    );
    // The re-prefill after the reestablish is the next decode call.
    let next_call = log.borrow().decoded.len();
    log.borrow_mut().fail_decode_call = Some(next_call);
    assert!(session.flush(0).is_err(), "the flush reports its failure");
    let mut cancel = NeverCancels;
    assert_eq!(
        session.append_and_generate(
            &[TokenId(3)],
            &stop_at(50),
            &mut cancel,
            &mut |_| {},
            PositionedSinks {
                field: None,
                on_column: &mut |_, _| {},
            },
            SamplerBuild {
                seed: 11,
                penalty_window: 64,
            },
        ),
        Err(DecodeFault::NotOpen),
        "the session no longer serves"
    );
}

/// **An engine fault mid-generation closes the session.** The terminator
/// discipline covers the clean path and the cancelled path; a backend that
/// just faulted cannot be asked to decode a terminator, so the fault path
/// closes the session instead of leaving a terminator-less prefix open for
/// the next append.
///
/// Perturbation: propagate the sample fault without the poison and this
/// test fails on the second append, which runs against the mid-message
/// state. Watched under exactly that removal.
#[test]
fn an_engine_fault_mid_generation_closes_the_session() {
    let (mut session, log) = opened(vec![], 128);
    log.borrow_mut().fail_sample = true;
    let mut cancel = NeverCancels;
    assert!(
        session
            .append_and_generate(
                &[TokenId(3)],
                &stop_at(50),
                &mut cancel,
                &mut |_| {},
                PositionedSinks {
                    field: None,
                    on_column: &mut |_, _| {},
                },
                SamplerBuild {
                    seed: 11,
                    penalty_window: 64,
                },
            )
            .is_err(),
        "the fault surfaces"
    );
    assert_eq!(
        session.append_and_generate(
            &[TokenId(4)],
            &stop_at(50),
            &mut cancel,
            &mut |_| {},
            PositionedSinks {
                field: None,
                on_column: &mut |_, _| {},
            },
            SamplerBuild {
                seed: 11,
                penalty_window: 64,
            },
        ),
        Err(DecodeFault::NotOpen),
        "and the session no longer serves"
    );
    assert_eq!(
        session.open(&[TokenId(1)]),
        Err(DecodeFault::NotOpen),
        "and cannot be reopened over its closed backend"
    );
}

/// **A span describing no removable region refuses, and the session
/// stands.** Charter 13.13 gives four such spans and the refusal is the
/// contract's typed case rather than a fault: the ask was answerable
/// and wrong. **The empty span is the one worth asserting hardest**,
/// because eliding nothing succeeds trivially and would report a state
/// change that did not happen.
///
/// Perturbation: narrow the guard to `to < from` and the empty span
/// answers `Ok` with equal counts, an elision authored over nothing.
/// Watched under exactly that narrowing. A full drop of the bound fails
/// earlier and differently, on the inverted span's underflow, and never
/// reaches the empty case.
///
/// conforms: spu-elision-refuses-an-unremovable-span
#[test]
fn an_unremovable_span_refuses_and_the_session_stands() {
    let (mut session, _log) = with_mechanism(
        vec![TokenId(5), TokenId(6), TokenId(0)],
        64,
        FlushMechanism::TruncateToPosition,
    );
    session
        .append_and_generate(
            &[TokenId(3)],
            &StopCondition {
                stop_tokens: vec![TokenId(9)],
                terminator: TokenId(0),
                max_tokens: 8,
            },
            &mut NeverCancels,
            &mut |_| {},
            PositionedSinks {
                field: None,
                on_column: &mut |_, _| {},
            },
            SamplerBuild {
                seed: 11,
                penalty_window: 64,
            },
        )
        .expect("a generation stands so there is an interior");

    let resident = session.resident_len();
    let prefix = 2;
    for (from, to, why) in [
        (0, resident, "a span over the identity prefix"),
        (prefix, resident + 1, "a span past the resident count"),
        (resident, prefix, "a span ending before it starts"),
        (prefix, prefix, "an empty span"),
    ] {
        let refused = session.elide(from, to);
        assert!(
            matches!(refused, Err(DecodeFault::UnremovableSpan { .. })),
            "{why} refuses, got {refused:?}"
        );
        assert_eq!(
            session.resident_len(),
            resident,
            "and {why} leaves the session as it found it"
        );
    }
}

/// **The elision removes exactly its span and resequences nothing**,
/// which is the decode contract's guarantee: what remains is the
/// sequence it was, shorter, with every surviving position in the order
/// it held. Driven under both flush mechanisms, because the elision
/// ignores the declaration on purpose: the interior outcome is reached
/// through the re-establish path for every family and never through a
/// truncate, per Spec section 4.5, and the log asserts both halves.
///
/// Perturbation: build the kept vector from `[to..]` alone, or reverse
/// the tail, and the counts still agree while the membership or the
/// order does not. The counts cannot catch either, which is why this
/// asserts the tokens themselves. A regression consulting the declared
/// mechanism and truncating fails the log's two counters under the
/// truncating family.
///
/// conforms: spu-elision-removes-its-span-in-order
#[test]
fn the_elision_removes_its_span_and_keeps_the_rest_in_order() {
    for mechanism in [
        FlushMechanism::TruncateToPosition,
        FlushMechanism::ReestablishAndReprefill,
    ] {
        let (mut session, log) = with_mechanism(
            vec![
                TokenId(5),
                TokenId(0),
                TokenId(6),
                TokenId(0),
                TokenId(7),
                TokenId(0),
            ],
            64,
            mechanism,
        );
        for token in [TokenId(3), TokenId(4), TokenId(8)] {
            session
                .append_and_generate(
                    &[token],
                    &StopCondition {
                        stop_tokens: vec![TokenId(9)],
                        terminator: TokenId(0),
                        max_tokens: 8,
                    },
                    &mut NeverCancels,
                    &mut |_| {},
                    PositionedSinks {
                        field: None,
                        on_column: &mut |_, _| {},
                    },
                    SamplerBuild {
                        seed: 11,
                        penalty_window: 64,
                    },
                )
                .expect("the generation runs");
        }

        let before_tokens = session.resident_tokens().to_vec();
        let resident = before_tokens.len();
        // An interior span: past the prefix and short of the end, so what
        // survives is a head and a tail rather than a prefix.
        let (from, to) = (3, 5);
        assert!(to < resident, "the fixture leaves a tail to preserve");
        let expected: Vec<TokenId> = before_tokens[..from]
            .iter()
            .chain(before_tokens[to..].iter())
            .copied()
            .collect();
        let truncates_before = log.borrow().truncated.len();
        let reestablished_before = log.borrow().reestablished;

        let (was, now) = session.elide(from, to).expect("the span is removable");
        assert_eq!(was, resident, "the before count is what stood");
        assert_eq!(now, resident - (to - from), "and exactly the span went");
        assert_eq!(
            session.resident_tokens(),
            expected.as_slice(),
            "the survivors keep their order and their membership"
        );
        assert_eq!(
            log.borrow().truncated.len(),
            truncates_before,
            "the elision issued no truncate under {mechanism:?}"
        );
        assert_eq!(
            log.borrow().reestablished,
            reestablished_before + 1,
            "and re-established exactly once under {mechanism:?}"
        );
    }
}

/// **The sampler holds nothing between generations, a flush included**,
/// per `weaver-spu-Spec` section 8.5. Two generations with a flush
/// between them: each is reseeded with the seed its caller derived, and
/// the second's penalty window is the truncated resident tail rather
/// than anything the sampler carried across.
///
/// This is the case the retired coupling lived in. The GGUF chain's
/// reset cleared the penalty window and reseeded the draw together,
/// so a flush moved the stream at a point nothing in the record
/// explained - and two arms of a comparison differing in resident
/// length crossed the flush threshold at different turns.
///
/// Perturbation: reseed once at open rather than per generation and
/// the second call never arrives, so the count fails.
#[test]
fn the_sampler_is_rebuilt_per_generation_across_a_flush() {
    let (mut session, log) = opened(vec![TokenId(7), TokenId(8)], 128);
    let stop = stop_at(50);
    let mut cancel = NeverCancels;

    session
        .append_and_generate(
            &[TokenId(3)],
            &stop,
            &mut cancel,
            &mut |_| {},
            PositionedSinks {
                field: None,
                on_column: &mut |_, _| {},
            },
            SamplerBuild {
                seed: 0xAAAA,
                penalty_window: 2,
            },
        )
        .expect("the first generation runs");
    let window_before = log.borrow().reseeds[0].1.len();
    session.flush(0).expect("the flush lands");
    session
        .append_and_generate(
            &[TokenId(4)],
            &stop,
            &mut cancel,
            &mut |_| {},
            PositionedSinks {
                field: None,
                on_column: &mut |_, _| {},
            },
            SamplerBuild {
                seed: 0xBBBB,
                penalty_window: 2,
            },
        )
        .expect("the second generation runs");

    let reseeds = log.borrow().reseeds.clone();
    assert_eq!(reseeds.len(), 2, "one reseed per generation: {reseeds:?}");
    assert_eq!(reseeds[0].0, 0xAAAA, "each takes its caller's seed");
    assert_eq!(reseeds[1].0, 0xBBBB, "and the flush does not change it");

    // **The window's tokens, not its length.** A length alone passes
    // on a window holding the wrong tokens, and the ordering this
    // guards is exactly about which tokens are in it: the reseed
    // follows the prefill, so each window ends with that generation's
    // own delta. Taken before the prefill both windows end at the
    // prefix and the deltas are missing, which is the native path's
    // disagreement this pins.
    assert_eq!(
        reseeds[0].1,
        vec![TokenId(2), TokenId(3)],
        "the first window is the last two resident, the bound holding: \
         one of the prefix and the delta after it"
    );
    assert_eq!(
        reseeds[1].1,
        vec![TokenId(2), TokenId(4)],
        "and the second is the flush's prefix with the second delta, \
         the first generation's tokens having gone with the flush"
    );
    // **The second window does not grow, which is the whole point.**
    // A generation ran between the two and produced tokens, so a
    // window carried inside the sampler would hold them still. Read
    // from the resident tail after a flush, it is the prefix again,
    // exactly what the first generation saw.
    assert_eq!(
        reseeds[1].1.len(),
        window_before,
        "the window follows the resident state rather than accumulating: \
         {:?} against {:?}",
        reseeds[1].1,
        reseeds[0].1
    );
}
