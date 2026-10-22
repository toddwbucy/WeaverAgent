//! Actual checked output, pair receipts, paged read and recorder paths.
//! Scripted members test custody, not a production language or SQL qualification.
use super::*;
use weaver_types::ToolOutcome;

const EMPTY: &[u8] = b"{ \"entries\" : { \"entries\" : [] } }";

fn base64(bytes: &[u8]) -> String {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for c in bytes.chunks(3) {
        let n = u32::from(c[0]) << 16
            | u32::from(*c.get(1).unwrap_or(&0)) << 8
            | u32::from(*c.get(2).unwrap_or(&0));
        out.push(A[(n >> 18 & 63) as usize] as char);
        out.push(A[(n >> 12 & 63) as usize] as char);
        out.push(if c.len() > 1 {
            A[(n >> 6 & 63) as usize] as char
        } else {
            '='
        });
        out.push(if c.len() > 2 {
            A[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum HoldRead {
    None,
    Read,
    Next,
    Release,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Refuse {
    No,
    Initial,
    Later,
}
struct InwardFixture {
    pairs: PairFixture,
    calls: Vec<s::CallId>,
    raw: Vec<u8>,
    held: HoldRead,
    hold_pair: Hold,
    refusal: Refuse,
}
impl InwardFixture {
    fn new(content: Vec<ContentBlock>) -> Self {
        let mut pairs = PairFixture::new();
        let original = pairs.generation(content);
        let effective = super::super::repair::effective_fixture(original);
        let calls = effective
            .requests()
            .iter()
            .map(|request| request.call_id.clone())
            .collect();
        pairs
            .output
            .input
            .run
            .context
            .begin_pair_execution(effective, &mut pairs.output.input.run.clock)
            .unwrap();
        Self {
            pairs,
            calls,
            raw: EMPTY.to_vec(),
            held: HoldRead::None,
            hold_pair: Hold::None,
            refusal: Refuse::No,
        }
    }
    fn begin(&mut self, index: usize) {
        for _ in 0..512 {
            if self
                .pairs
                .output
                .input
                .run
                .context
                .next_pair_execution()
                .is_some()
            {
                break;
            }
            self.step().unwrap();
        }
        assert_eq!(
            self.pairs
                .output
                .input
                .run
                .context
                .begin_inward_recall(&self.calls[index], &mut self.pairs.output.input.run.clock)
                .unwrap(),
            Ok(())
        );
    }
    fn controls(&self) -> Vec<&s::StateControl> {
        self.pairs
            .requests
            .iter()
            .filter_map(|request| match request {
                s::StateFrame::Control { frame, .. } => Some(frame),
                _ => None,
            })
            .collect()
    }
    fn step(&mut self) -> Result<(), RunFailure> {
        let run = &mut self.pairs.output.input.run;
        if run
            .context
            .pair_work
            .as_ref()
            .is_some_and(|work| work.inward.is_some())
        {
            run.context
                .advance_inward_recall(&mut ports(&mut run.clock))?;
        } else {
            run.context.advance_pairs(&mut ports(&mut run.clock))?;
        }
        self.pairs.checker();
        self.member();
        Ok(())
    }
    fn member(&mut self) {
        self.pairs.output.transport(TransportSource::State);
        let mut bytes = [0; 65536];
        loop {
            match self.pairs.output.input.run.state.read(&mut bytes) {
                Ok(0) => panic!("member closed"),
                Ok(n) => self.pairs.bytes.extend_from_slice(&bytes[..n]),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(error) => panic!("member read: {error}"),
            }
        }
        while let Some(end) = self.pairs.bytes.iter().position(|byte| *byte == b'\n') {
            let line: Vec<_> = self.pairs.bytes.drain(..=end).collect();
            self.pairs
                .requests
                .push(s::decode_state_line(&line, s::FrameDirection::Request).unwrap());
            self.pairs.answered.push(false);
        }
        for index in 0..self.pairs.requests.len() {
            if self.pairs.answered[index] {
                continue;
            }
            let request = self.pairs.requests[index].clone();
            let s::StateFrame::Control { frame, .. } = &request else {
                let held = match (&request, self.hold_pair) {
                    (s::StateFrame::File { .. }, Hold::Statement) => true,
                    (s::StateFrame::FilePair { filing, .. }, Hold::Opening) => {
                        matches!(filing.pair, s::PairRecord::Started(_))
                    }
                    (s::StateFrame::FilePair { filing, .. }, Hold::Terminal) => {
                        matches!(filing.pair, s::PairRecord::Ended(_))
                    }
                    _ => false,
                };
                if !held {
                    self.pairs.answer(index);
                }
                continue;
            };
            let held = matches!(
                (frame, self.held),
                (s::StateControl::Read { .. }, HoldRead::Read)
                    | (s::StateControl::Next { .. }, HoldRead::Next)
                    | (s::StateControl::Release { .. }, HoldRead::Release)
            );
            if held {
                continue;
            }
            let refusal = matches!(
                (frame, self.refusal),
                (s::StateControl::Read { .. }, Refuse::Initial)
                    | (s::StateControl::Next { .. }, Refuse::Later)
            );
            let answer = if refusal {
                s::StateControlAnswer::ControlRefused {
                    code: s::ControlRefusalCode::Read(if self.refusal == Refuse::Later {
                        s::ReadRefusalCode::ResultUnavailable
                    } else {
                        s::ReadRefusalCode::ResultTooLarge
                    }),
                    detail: "quoted \"read\" refusal\nline".into(),
                }
            } else {
                let split = self.raw.len() / 2;
                match frame {
                    s::StateControl::Read { .. } => s::StateControlAnswer::ReadPage(s::ReadPage {
                        snapshot_id: 7.into(),
                        offset: 0.into(),
                        total_bytes: (self.raw.len() as u64).into(),
                        octets: base64(&self.raw[..split]),
                        done: false,
                    }),
                    s::StateControl::Next {
                        snapshot_id,
                        offset,
                        ..
                    } => {
                        assert_eq!(snapshot_id.0, 7);
                        assert_eq!(offset.0 as usize, split);
                        s::StateControlAnswer::ReadPage(s::ReadPage {
                            snapshot_id: *snapshot_id,
                            offset: *offset,
                            total_bytes: (self.raw.len() as u64).into(),
                            octets: base64(&self.raw[split..]),
                            done: true,
                        })
                    }
                    s::StateControl::Release { snapshot_id } => s::StateControlAnswer::Released {
                        snapshot_id: *snapshot_id,
                    },
                    _ => panic!("inward emitted another control"),
                }
            };
            let began = self.pairs.output.input.run.clock.now().unwrap() + 1;
            let response = s::StateFrame::ControlAnswer {
                common: request.common().clone(),
                frame: answer,
                observation: operation(&request, self.pairs.operation, began),
            };
            self.pairs.operation += 1;
            self.pairs
                .output
                .input
                .run
                .clock
                .advance_to(began + 1)
                .unwrap();
            self.pairs
                .output
                .input
                .run
                .state
                .write_all(&s::encode_state_line(&response, s::FrameDirection::Response).unwrap())
                .unwrap();
            self.pairs.answered[index] = true;
        }
        self.pairs.output.transport(TransportSource::State);
    }
    fn until_control(&mut self, expected: HoldRead) {
        for _ in 0..512 {
            self.step().unwrap();
            if self.controls().iter().any(|frame| {
                matches!(
                    (frame, expected),
                    (s::StateControl::Read { .. }, HoldRead::Read)
                        | (s::StateControl::Next { .. }, HoldRead::Next)
                        | (s::StateControl::Release { .. }, HoldRead::Release)
                )
            }) {
                return;
            }
        }
        panic!("expected control never arrived");
    }
    fn complete(&mut self, index: usize) {
        for _ in 0..512 {
            self.step().unwrap();
            if self
                .pairs
                .output
                .input
                .run
                .context
                .completed_pair(&self.calls[index], &mut self.pairs.output.input.run.clock)
                .unwrap()
                .is_some()
            {
                return;
            }
        }
        panic!("inward pair never completed");
    }
}

#[test]
fn inward_waits_for_both_request_receipts_and_keeps_exact_read_and_terminal() {
    for held in [Hold::Statement, Hold::Opening] {
        let mut pending = InwardFixture::new(vec![text("Recall file.")]);
        pending.hold_pair = held;
        for _ in 0..24 {
            pending.step().unwrap();
        }
        assert_eq!(
            pending
                .pairs
                .output
                .input
                .run
                .context
                .begin_inward_recall(&pending.calls[0], &mut pending.pairs.output.input.run.clock)
                .unwrap(),
            Err(InwardRefusal::NotReady)
        );
        assert!(pending.controls().is_empty());
    }
    let mut fx = InwardFixture::new(vec![text("Recall file.")]);
    fx.begin(0);
    fx.held = HoldRead::Release;
    fx.until_control(HoldRead::Release);
    assert_eq!(
        fx.controls()[0],
        &s::StateControl::Read {
            selector: s::ReadSelector::History {
                noun: "file".into(),
                verb: None,
                condition: None
            }
        }
    );
    assert!(fx.pairs.output.input.records(Kind::Recall).is_empty());
    assert!(
        fx.pairs
            .output
            .input
            .records(Kind::ToolCallCompleted)
            .is_empty()
    );
    assert!(
        fx.pairs
            .output
            .input
            .run
            .context
            .deadlines
            .get(PumpDeadline::Decision)
            .is_none()
    );
    assert!(
        fx.pairs
            .output
            .input
            .run
            .context
            .deadlines
            .get(PumpDeadline::Cut)
            .is_none()
    );
    fx.held = HoldRead::None;
    fx.hold_pair = Hold::Terminal;
    for _ in 0..32 {
        fx.step().unwrap();
    }
    assert_eq!(fx.pairs.output.input.records(Kind::Recall).len(), 1);
    assert_eq!(
        fx.pairs.output.input.records(Kind::ToolCallCompleted).len(),
        1
    );
    assert!(
        fx.pairs
            .output
            .input
            .run
            .context
            .completed_pair(&fx.calls[0], &mut fx.pairs.output.input.run.clock)
            .unwrap()
            .is_none()
    );
    fx.hold_pair = Hold::None;
    fx.complete(0);
    let pair = fx
        .pairs
        .output
        .input
        .run
        .context
        .completed_pair(&fx.calls[0], &mut fx.pairs.output.input.run.clock)
        .unwrap()
        .unwrap();
    assert_eq!(pair.ended().fate, s::TerminalPairFate::Completed);
    assert!(pair.ended().execution.is_none());
    assert!(pair.ended().reason.is_none());
    assert_eq!(
        pair.ended().outcome,
        Some(ToolOutcome::Result {
            content: String::from_utf8(fx.raw.clone()).unwrap()
        })
    );
    assert_eq!(pair.recalled().unwrap().raw_bytes(), fx.raw);
    assert_eq!(
        pair.result().block().content,
        String::from_utf8(fx.raw.clone()).unwrap()
    );
    let terminal = &fx.pairs.output.input.records(Kind::ToolCallCompleted)[0];
    assert_eq!(terminal["channel"], "tool");
    assert_eq!(terminal["origin"], "loop");
    assert_eq!(terminal["subsystem"], "harness");
}

#[test]
fn inward_typed_initial_and_later_refusals_never_become_empty_success() {
    for refusal in [Refuse::Initial, Refuse::Later] {
        let mut fx = InwardFixture::new(vec![text("Recall file.")]);
        fx.refusal = refusal;
        fx.begin(0);
        fx.complete(0);
        assert!(fx.pairs.output.input.records(Kind::Recall).is_empty());
        let releases = fx
            .controls()
            .iter()
            .filter(|frame| matches!(frame, s::StateControl::Release { .. }))
            .count();
        assert_eq!(releases, usize::from(refusal == Refuse::Later));
        let pair = fx
            .pairs
            .output
            .input
            .run
            .context
            .completed_pair(&fx.calls[0], &mut fx.pairs.output.input.run.clock)
            .unwrap()
            .unwrap();
        assert_eq!(pair.ended().fate, s::TerminalPairFate::Completed);
        assert!(pair.ended().execution.is_none());
        assert!(pair.ended().reason.is_none());
        assert!(pair.recalled().is_none());
        let code = if refusal == Refuse::Later {
            "result_unavailable"
        } else {
            "result_too_large"
        };
        let expected =
            format!("{{\"code\":\"{code}\",\"detail\":\"quoted \\\"read\\\" refusal\\nline\"}}");
        assert_eq!(
            pair.ended().outcome,
            Some(ToolOutcome::Refused { reason: expected })
        );
    }
}

#[test]
fn inward_two_calls_keep_identity_order_and_separate_receipt_windows() {
    let mut fx = InwardFixture::new(vec![text("Recall file. Recall folder.")]);
    fx.begin(0);
    let call0 = fx.calls[0].clone();
    let call1 = fx.calls[1].clone();
    assert_eq!(call0.call.0, 0);
    assert_eq!(call1.call.0, 1);
    fx.held = HoldRead::Read;
    fx.until_control(HoldRead::Read);
    assert_eq!(
        fx.pairs.output.input.run.context.outstanding_pairs()[0].state,
        crate::flow::report::PairState::ExecutingInternal
    );
    assert!(
        fx.pairs
            .output
            .input
            .run
            .context
            .next_pair_execution()
            .is_none()
    );
    assert_eq!(
        fx.pairs
            .output
            .input
            .run
            .context
            .begin_inward_recall(&call1, &mut fx.pairs.output.input.run.clock)
            .unwrap(),
        Err(InwardRefusal::NotReady)
    );
    fx.held = HoldRead::None;
    fx.complete(0);
    let bound = fx
        .pairs
        .output
        .input
        .run
        .context
        .deadlines
        .get(PumpDeadline::Decision)
        .unwrap();
    assert_eq!(
        bound.deadline_ns() - bound.began_ns(),
        fx.pairs
            .output
            .input
            .run
            .context
            .load_duty
            .as_ref()
            .unwrap()
            .instruction
            .decision_wait
            .elapsed_ns
            .0
    );
    fx.begin(1);
    fx.complete(1);
    let selectors: Vec<_> = fx
        .controls()
        .iter()
        .filter_map(|frame| match frame {
            s::StateControl::Read { selector } => Some(selector.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        selectors,
        vec![
            s::ReadSelector::History {
                noun: "file".into(),
                verb: None,
                condition: None
            },
            s::ReadSelector::History {
                noun: "folder".into(),
                verb: None,
                condition: None
            }
        ]
    );
    let a = fx
        .pairs
        .output
        .input
        .run
        .context
        .completed_pair(&call0, &mut fx.pairs.output.input.run.clock)
        .unwrap()
        .unwrap();
    assert_eq!(a.started().statement.binding.0, 0);
    let b = fx
        .pairs
        .output
        .input
        .run
        .context
        .completed_pair(&call1, &mut fx.pairs.output.input.run.clock)
        .unwrap()
        .unwrap();
    assert_eq!(b.started().statement.binding.0, 1);
    assert_eq!(
        fx.pairs.output.input.records(Kind::ToolCallCompleted).len(),
        2
    );
}

#[test]
fn inward_private_read_cannot_be_taken_cancelled_or_replaced() {
    let mut fx = InwardFixture::new(vec![text("Recall file.")]);
    fx.begin(0);
    fx.held = HoldRead::Release;
    fx.until_control(HoldRead::Release);
    let run = &mut fx.pairs.output.input.run;
    assert_eq!(
        run.context.take_read(&mut run.clock).unwrap_err(),
        RunFailure::PairOwned
    );
    assert_eq!(
        run.context.cancel_read(&mut run.clock).unwrap_err(),
        RunFailure::PairOwned
    );
    assert_eq!(
        run.context
            .advance_read(&mut ports(&mut run.clock))
            .unwrap_err(),
        RunFailure::PairOwned
    );
    assert_eq!(
        run.context
            .begin_read(s::ReadSelector::Marks {}, None, &mut run.clock)
            .unwrap_err()
            .refusal,
        ReadBeginRefusal::Busy
    );
    assert_eq!(
        run.context
            .prepare_cut(vec![], &mut run.clock)
            .unwrap_err()
            .refusal,
        FreshnessBeginRefusal::Busy
    );
    assert!(run.context.failure().is_none());
    fx.held = HoldRead::None;
    fx.complete(0);
}

#[test]
fn inward_stop_before_start_closes_without_read_but_started_read_keeps_its_outcome() {
    for started in [false, true] {
        let mut fx = InwardFixture::new(vec![text("Recall file. Recall folder.")]);
        fx.begin(0);
        fx.held = HoldRead::Release;
        if started {
            fx.until_control(HoldRead::Release);
        }
        fx.pairs
            .output
            .input
            .run
            .context
            .bar_pending_pairs(
                s::NonDispatchCause::Stop,
                &mut fx.pairs.output.input.run.clock,
            )
            .unwrap();
        fx.held = HoldRead::None;
        fx.complete(0);
        fx.complete(1);
        assert_eq!(fx.controls().is_empty(), !started);
        let first = fx
            .pairs
            .output
            .input
            .run
            .context
            .completed_pair(&fx.calls[0], &mut fx.pairs.output.input.run.clock)
            .unwrap()
            .unwrap();
        assert_eq!(
            first.ended().fate,
            if started {
                s::TerminalPairFate::Completed
            } else {
                s::TerminalPairFate::Failed
            }
        );
        let next = fx
            .pairs
            .output
            .input
            .run
            .context
            .completed_pair(&fx.calls[1], &mut fx.pairs.output.input.run.clock)
            .unwrap()
            .unwrap();
        assert_eq!(
            next.ended().reason,
            Some(s::PairFailure::NotDispatched {
                cause: s::NonDispatchCause::Stop
            })
        );
    }
}

#[test]
fn inward_read_keeps_its_original_bound_across_progress_and_leave() {
    let mut fx = InwardFixture::new(vec![text("Recall file.")]);
    let request_wait = fx.pairs.output.bound;
    fx.begin(0);
    fx.held = HoldRead::Next;
    fx.until_control(HoldRead::Next);
    let bound = fx
        .pairs
        .output
        .input
        .run
        .context
        .deadlines
        .get(PumpDeadline::StateControl)
        .unwrap();
    assert_eq!(bound.deadline_ns() - bound.began_ns(), 2_000_000_000);
    fx.pairs
        .output
        .input
        .run
        .clock
        .advance_to(request_wait.deadline_ns() + 1)
        .unwrap();
    fx.step().unwrap();
    let now = fx.pairs.output.input.run.clock.now().unwrap();
    let leave = AbsoluteBound::after(now, 10).unwrap();
    fx.pairs
        .output
        .input
        .run
        .context
        .arm(
            PumpDeadline::Leave,
            leave,
            &mut fx.pairs.output.input.run.clock,
        )
        .unwrap();
    fx.pairs
        .output
        .input
        .run
        .clock
        .advance_to(leave.deadline_ns())
        .unwrap();
    assert_eq!(
        fx.step().unwrap_err(),
        RunFailure::Wait {
            key: PumpDeadline::Leave,
            bound: leave
        }
    );
    assert_eq!(
        fx.pairs
            .output
            .input
            .run
            .context
            .complete_wait(PumpDeadline::Leave),
        Some(leave)
    );
    assert_eq!(
        fx.pairs
            .output
            .input
            .run
            .context
            .deadlines
            .get(PumpDeadline::StateControl),
        Some(bound)
    );
    fx.held = HoldRead::None;
    fx.complete(0);
    assert_eq!(
        fx.controls()
            .iter()
            .filter(|f| matches!(f, s::StateControl::Read { .. }))
            .count(),
        1
    );
    assert_eq!(
        fx.controls()
            .iter()
            .filter(|f| matches!(f, s::StateControl::Release { .. }))
            .count(),
        1
    );
}

struct CrossingClock<'a> {
    clock: &'a mut crate::flow::clock::testing::FakePumpIo,
    remaining: usize,
    deadline: u64,
}
impl PumpIo for CrossingClock<'_> {
    fn now(&mut self) -> std::io::Result<u64> {
        if self.remaining == 0 {
            self.clock.advance_to(self.deadline).unwrap();
        } else {
            self.remaining -= 1;
        }
        self.clock.now()
    }
    fn wait(
        &mut self,
        interests: &[crate::flow::clock::WaitInterest<'_>],
        deadline: Option<u64>,
    ) -> std::io::Result<crate::flow::clock::WaitOutcome> {
        self.clock.wait(interests, deadline)
    }
}

#[test]
fn inward_nested_leave_retains_admission_and_recorded_terminal_continuations() {
    let mut admitted_yield = false;
    let mut completed_yield = false;
    for terminal in [false, true] {
        for remaining in 0..24 {
            let mut fx = InwardFixture::new(vec![text("Recall file.")]);
            fx.begin(0);
            if terminal {
                fx.held = HoldRead::Release;
                fx.until_control(HoldRead::Release);
                fx.held = HoldRead::None;
                fx.member();
            }
            let now = fx.pairs.output.input.run.clock.now().unwrap();
            let leave = AbsoluteBound::after(now, 5).unwrap();
            fx.pairs
                .output
                .input
                .run
                .context
                .arm(
                    PumpDeadline::Leave,
                    leave,
                    &mut fx.pairs.output.input.run.clock,
                )
                .unwrap();
            let result = {
                let run = &mut fx.pairs.output.input.run;
                let mut clock = CrossingClock {
                    clock: &mut run.clock,
                    remaining,
                    deadline: leave.deadline_ns(),
                };
                let mut result = Ok(InwardProgress::Pending);
                for _ in 0..128 {
                    result = run.context.advance_inward_recall(&mut ports(&mut clock));
                    if !terminal || result != Ok(InwardProgress::Pending) {
                        break;
                    }
                }
                result
            };
            if let Err(error) = result {
                assert_eq!(
                    error,
                    RunFailure::Wait {
                        key: PumpDeadline::Leave,
                        bound: leave
                    }
                );
                let context = &fx.pairs.output.input.run.context;
                let read_owned = context.read.is_some();
                admitted_yield |= !terminal
                    && read_owned
                    && context.deadlines.get(PumpDeadline::Decision).is_none();
                completed_yield |= terminal
                    && read_owned
                    && context.deadlines.get(PumpDeadline::StateControl).is_none();
            }
            assert_eq!(
                fx.pairs
                    .output
                    .input
                    .run
                    .context
                    .complete_wait(PumpDeadline::Leave),
                Some(leave)
            );
            // A read admitted under Leave retains that earlier absolute cap.
            // Clearing a lifecycle wait cannot extend the accepted control bound.
            if !terminal && fx.pairs.output.input.run.context.read.is_some() {
                let context = &fx.pairs.output.input.run.context;
                let read_bound = context.deadlines.get(PumpDeadline::StateControl).unwrap();
                assert_eq!(read_bound.deadline_ns(), leave.deadline_ns());
                fx.pairs
                    .output
                    .input
                    .run
                    .clock
                    .advance_to(read_bound.deadline_ns())
                    .unwrap();
                {
                    assert!(matches!(
                        fx.step(),
                        Err(RunFailure::Flow {
                            reason: FlowHaltReason::ControlBound,
                            point: HaltPoint::Barrier,
                            ..
                        })
                    ));
                    let context = &fx.pairs.output.input.run.context;
                    assert!(context.read.is_some());
                    assert_eq!(
                        context.deadlines.get(PumpDeadline::StateControl),
                        Some(read_bound)
                    );
                    assert_eq!(
                        context.pairs.outstanding()[0].state,
                        crate::flow::report::PairState::ExecutingInternal
                    );
                    assert!(
                        fx.pairs
                            .output
                            .input
                            .records(Kind::ToolCallCompleted)
                            .is_empty()
                    );
                    continue;
                }
            }
            fx.complete(0);
            assert_eq!(
                fx.controls()
                    .iter()
                    .filter(|f| matches!(f, s::StateControl::Read { .. }))
                    .count(),
                1
            );
            assert_eq!(
                fx.controls()
                    .iter()
                    .filter(|f| matches!(f, s::StateControl::Release { .. }))
                    .count(),
                1
            );
            assert_eq!(fx.pairs.output.input.records(Kind::Recall).len(), 1);
            assert_eq!(
                fx.pairs.output.input.records(Kind::ToolCallCompleted).len(),
                1
            );
            assert!(fx.pairs.output.input.run.context.failure().is_none());
        }
    }
    assert!(
        admitted_yield,
        "clock fixture missed the installed-read admission continuation"
    );
    assert!(
        completed_yield,
        "clock fixture missed the Recorded-before-take continuation"
    );
}

#[test]
fn inward_whole_history_retains_nested_provenance_without_refiling_it() {
    let mut fx = InwardFixture::new(vec![text("Recall file.")]);
    let corpus: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("fixtures/recall-sources.json")).unwrap();
    let case = corpus
        .iter()
        .find(|case| case["case"] == "history_filtered_pair_union")
        .unwrap();
    fx.raw = serde_json::to_vec_pretty(&case["result"]).unwrap();
    fx.begin(0);
    fx.complete(0);
    assert_eq!(
        fx.pairs
            .requests
            .iter()
            .filter(|request| matches!(request, s::StateFrame::File { .. }))
            .count(),
        1
    );
    assert_eq!(
        fx.pairs
            .requests
            .iter()
            .filter(|request| matches!(request, s::StateFrame::FilePair { .. }))
            .count(),
        2
    );
    let pair = fx
        .pairs
        .output
        .input
        .run
        .context
        .completed_pair(&fx.calls[0], &mut fx.pairs.output.input.run.clock)
        .unwrap()
        .unwrap();
    assert_eq!(pair.recalled().unwrap().raw_bytes(), fx.raw);
    assert_eq!(pair.result().block().content.as_bytes(), fx.raw);
    let recall = &fx.pairs.output.input.records(Kind::Recall)[0];
    assert!(
        !recall["payload"]["read"]["sources"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn inward_malformed_result_latches_with_executing_internal_evidence() {
    let mut fx = InwardFixture::new(vec![text("Recall file.")]);
    fx.raw = br#"{"marks":{"next_span_id":"1","current_task":null,"open_scopes":[]}}"#.to_vec();
    fx.begin(0);
    assert!((0..128).find_map(|_| fx.step().err()).is_some());
    assert!(fx.pairs.output.input.run.context.failure().is_some());
    assert!(
        fx.pairs
            .output
            .input
            .records(Kind::ToolCallCompleted)
            .is_empty()
    );
    assert!(fx.pairs.output.input.records(Kind::Recall).is_empty());
    let outstanding = fx.pairs.output.input.run.context.outstanding_pairs();
    assert_eq!(outstanding[0].call_id, fx.calls[0]);
    assert_eq!(
        outstanding[0].state,
        crate::flow::report::PairState::ExecutingInternal
    );
    assert!(outstanding[0].execution.is_none());
    assert!(fx.pairs.output.input.run.context.read.is_some());
}

#[test]
fn inward_uses_source_order_without_claiming_a_later_external_call() {
    let mut fx = InwardFixture::new(vec![text("Recall file."), text("Read files."), call("ls")]);
    assert_eq!(fx.calls[0].call.0, 1);
    assert_eq!(fx.calls[1].call.0, 0);
    fx.begin(0);
    fx.complete(0);
    for _ in 0..128 {
        fx.step().unwrap();
        if fx
            .pairs
            .output
            .input
            .run
            .context
            .next_pair_execution()
            .is_some()
        {
            break;
        }
    }
    assert_eq!(
        fx.pairs
            .output
            .input
            .run
            .context
            .next_pair_execution()
            .unwrap()
            .call_id,
        fx.calls[1]
    );
    assert_eq!(
        fx.pairs
            .output
            .input
            .run
            .context
            .begin_inward_recall(&fx.calls[1], &mut fx.pairs.output.input.run.clock)
            .unwrap(),
        Err(InwardRefusal::OtherLane)
    );
    assert!(fx.pairs.output.input.run.context.failure().is_none());
    assert_eq!(
        fx.controls()
            .iter()
            .filter(|f| matches!(f, s::StateControl::Read { .. }))
            .count(),
        1
    );
}

#[test]
fn inward_history_uses_the_exact_borrowed_subject_without_normalization() {
    // The synthetic checker returns this exact nonempty source span. This
    // exercises byte preservation, not production acceptance of these words.
    let subject = " MiXeD  File ";
    let mut fx = InwardFixture::new(vec![text("Recall  MiXeD  File .")]);
    fx.begin(0);
    fx.complete(0);
    let selector = fx
        .controls()
        .into_iter()
        .find_map(|frame| match frame {
            s::StateControl::Read { selector } => Some(selector.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        selector,
        s::ReadSelector::History {
            noun: subject.into(),
            verb: None,
            condition: None
        }
    );
    let pair = fx
        .pairs
        .output
        .input
        .run
        .context
        .completed_pair(&fx.calls[0], &mut fx.pairs.output.input.run.clock)
        .unwrap()
        .unwrap();
    assert_eq!(
        pair.started().request,
        s::PairRequest::Recall {
            subject: subject.into()
        }
    );
    assert_eq!(pair.recalled().unwrap().selector(), &selector);
}

#[test]
fn inward_recall_recorder_pressure_keeps_one_ticket_before_terminal_filing() {
    let mut fx = InwardFixture::new(vec![text("Recall file.")]);
    fx.begin(0);
    fx.held = HoldRead::Release;
    fx.until_control(HoldRead::Release);
    let bound = fx
        .pairs
        .output
        .input
        .run
        .context
        .deadlines
        .get(PumpDeadline::StateControl)
        .unwrap();
    fx.held = HoldRead::None;
    fx.member();
    // Common reduction records the real Released operation without letting
    // the private inward driver author recall yet. No proof is fabricated.
    for _ in 0..128 {
        let run = &mut fx.pairs.output.input.run;
        run.context.reduce_flow(&mut run.clock).unwrap();
        if run.context.read_blocks_reduction() {
            break;
        }
    }
    assert!(fx.pairs.output.input.run.context.read_blocks_reduction());
    assert!(fx.pairs.output.input.records(Kind::Recall).is_empty());
    fx.pairs.output.input.fill_trace();
    let run = &mut fx.pairs.output.input.run;
    assert_eq!(
        run.context
            .advance_inward_recall(&mut ports(&mut run.clock))
            .unwrap(),
        InwardProgress::Pending
    );
    let ticket = run.context.pending_author().unwrap().ticket;
    let payload =
        serde_json::to_string(&run.context.pending_author().unwrap().event.payload).unwrap();
    for _ in 0..3 {
        let run = &mut fx.pairs.output.input.run;
        assert_eq!(
            run.context
                .advance_inward_recall(&mut ports(&mut run.clock))
                .unwrap(),
            InwardProgress::Pending
        );
        assert_eq!(run.context.pending_author().unwrap().ticket, ticket);
        assert_eq!(
            serde_json::to_string(&run.context.pending_author().unwrap().event.payload).unwrap(),
            payload
        );
        assert_eq!(
            run.context.deadlines.get(PumpDeadline::StateControl),
            Some(bound)
        );
        assert!(run.context.deadlines.get(PumpDeadline::Decision).is_none());
        assert!(
            run.context
                .completed_pair(&fx.calls[0], &mut run.clock)
                .unwrap()
                .is_none()
        );
        assert!(fx.pairs.output.input.records(Kind::Recall).is_empty());
        assert!(
            fx.pairs
                .output
                .input
                .records(Kind::ToolCallCompleted)
                .is_empty()
        );
    }
    fx.pairs.output.input.drain_trace();
    fx.complete(0);
    assert_eq!(fx.pairs.output.input.records(Kind::Recall).len(), 1);
    assert_eq!(
        fx.pairs.output.input.records(Kind::ToolCallCompleted).len(),
        1
    );
    assert_eq!(
        fx.controls()
            .iter()
            .filter(|f| matches!(f, s::StateControl::Release { .. }))
            .count(),
        1
    );
}
