//! Actual load/templates/boot read, sockets, tap and recording barriers. Only
//! the absent prior decoder-Open integration is explicitly assumed by cfg(test).
//! Scripted member replies are not a SQL/reload or production qualification.

use super::*;
use crate::flow::clock::testing::FakePumpIo;
use crate::flow::coordinator::tests::operation;
use std::io::Read;

const _: fn() = || {
    trait MustNotClone<M> {
        fn pin() {}
    }
    impl<T: ?Sized> MustNotClone<()> for T {}
    impl<T: Clone> MustNotClone<u8> for T {}
    let _ = <PriorBootOpen as MustNotClone<_>>::pin;
    let _ = <RepairedBootPairs as MustNotClone<_>>::pin;
    trait MustNotDefault<M> {
        fn pin() {}
    }
    impl<T: ?Sized> MustNotDefault<()> for T {}
    impl<T: Default> MustNotDefault<u8> for T {}
    let _ = <PriorBootOpen as MustNotDefault<_>>::pin;
    let _ = <RepairedBootPairs as MustNotDefault<_>>::pin;
    trait MustNotFromStart<M> {
        fn pin() {}
    }
    impl<T: ?Sized> MustNotFromStart<()> for T {}
    impl<T: From<StartIdentity>> MustNotFromStart<u8> for T {}
    let _ = <PriorBootOpen as MustNotFromStart<_>>::pin;
    trait MustNotFromPairs<M> {
        fn pin() {}
    }
    impl<T: ?Sized> MustNotFromPairs<()> for T {}
    impl<T: From<Vec<s::ReadPair>>> MustNotFromPairs<u8> for T {}
    let _ = <RepairedBootPairs as MustNotFromPairs<_>>::pin;
};

fn old_pair(run: &str) -> s::ReadPair {
    let corpus: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("fixtures/recall-sources.json")).unwrap();
    let mut pair = corpus
        .iter()
        .find(|case| case["case"] == "open_pairs_original_blocks")
        .unwrap()["result"]["open_pairs"]["pairs"][0]
        .clone();
    fn rename(value: &mut serde_json::Value, run: &str) {
        match value {
            serde_json::Value::String(text) if text == "run" => *text = run.into(),
            serde_json::Value::Array(values) => {
                values.iter_mut().for_each(|value| rename(value, run))
            }
            serde_json::Value::Object(values) => {
                values.values_mut().for_each(|value| rename(value, run))
            }
            _ => {}
        }
    }
    rename(&mut pair, run);
    serde_json::from_value(pair).unwrap()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Hold {
    None,
    Read(usize),
    Next,
    Release(usize),
    Terminal(usize),
}

struct RepairFixture {
    boot: BootFixture,
    prior: Option<PriorBootOpen>,
    initial: Vec<u8>,
    final_read: Vec<u8>,
    requests: Vec<s::StateFrame>,
    answered: Vec<bool>,
    bytes: Vec<u8>,
    held: Hold,
    page_size: usize,
    read_refusal: Option<usize>,
    later_refusal: bool,
    file_refusal: bool,
}

impl RepairFixture {
    fn new(pairs: Vec<s::ReadPair>) -> Self {
        let mut boot = BootFixture::new(true);
        let entry = boot.entry();
        let request = boot.begin();
        boot.released(&request, entry);
        assert!(matches!(boot.finish(), BootReadCompletion::Seated));
        let prior = PriorBootOpen::assume_checked_open_for_test(&boot.member.run.context);
        let initial = s::QueryResult::OpenPairs { pairs };
        crate::flow::read_validate::validate(&s::ReadSelector::OpenPairs {}, &initial).unwrap();
        boot.member.run.state.set_nonblocking(true).unwrap();
        Self {
            boot,
            prior: Some(prior),
            initial: serde_json::to_vec_pretty(&initial).unwrap(),
            final_read: br#"{"open_pairs":{"pairs":[]}}"#.to_vec(),
            requests: vec![],
            answered: vec![],
            bytes: vec![],
            held: Hold::None,
            page_size: usize::MAX,
            read_refusal: None,
            later_refusal: false,
            file_refusal: false,
        }
    }
    fn run(&mut self) -> &mut Fixture {
        &mut self.boot.member.run
    }
    fn begin(&mut self) {
        let prior = self.prior.take().unwrap();
        let run = self.run();
        run.context.begin_boot_pairs(prior, &mut run.clock).unwrap();
    }
    fn advance(&mut self) -> Result<BootPairsProgress, RunFailure> {
        let run = self.run();
        run.context.advance_boot_pairs(&mut ports(&mut run.clock))
    }
    fn classify(&self, index: usize) -> Hold {
        match &self.requests[index] {
            s::StateFrame::Control {
                frame: s::StateControl::Read { .. },
                ..
            } => Hold::Read(
                self.requests[..=index]
                    .iter()
                    .filter(|request| {
                        matches!(
                            request,
                            s::StateFrame::Control {
                                frame: s::StateControl::Read { .. },
                                ..
                            }
                        )
                    })
                    .count(),
            ),
            s::StateFrame::Control {
                frame: s::StateControl::Next { .. },
                ..
            } => Hold::Next,
            s::StateFrame::Control {
                frame: s::StateControl::Release { .. },
                ..
            } => Hold::Release(
                self.requests[..=index]
                    .iter()
                    .filter(|request| {
                        matches!(
                            request,
                            s::StateFrame::Control {
                                frame: s::StateControl::Release { .. },
                                ..
                            }
                        )
                    })
                    .count(),
            ),
            s::StateFrame::FilePair { .. } => Hold::Terminal(
                self.requests[..=index]
                    .iter()
                    .filter(|request| matches!(request, s::StateFrame::FilePair { .. }))
                    .count(),
            ),
            _ => panic!("unexpected boot repair request: {:?}", self.requests[index]),
        }
    }
    fn io(&mut self) -> Result<(), RunFailure> {
        for _ in 0..32 {
            if self
                .boot
                .member
                .run
                .context
                .flow
                .state()
                .outstanding()
                .iter()
                .all(|request| request.bytes_sent == request.frame_bytes)
            {
                break;
            }
            let now = self.boot.member.now() + 1;
            self.boot.member.pump_at(now, PollFlags::POLLOUT)?;
        }
        let mut buffer = [0; 65536];
        loop {
            match self.boot.member.run.state.read(&mut buffer) {
                Ok(0) => panic!("boot repair state closed"),
                Ok(n) => self.bytes.extend_from_slice(&buffer[..n]),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(error) => panic!("boot repair state: {error}"),
            }
        }
        while let Some(end) = self.bytes.iter().position(|byte| *byte == b'\n') {
            let line: Vec<_> = self.bytes.drain(..=end).collect();
            self.requests
                .push(s::decode_state_line(&line, s::FrameDirection::Request).unwrap());
            self.answered.push(false);
        }
        for index in 0..self.requests.len() {
            if !self.answered[index] && self.classify(index) != self.held {
                self.answer(index)?;
            }
        }
        Ok(())
    }
    fn answer(&mut self, index: usize) -> Result<(), RunFailure> {
        assert!(!self.answered[index]);
        let request = self.requests[index].clone();
        let answer = match &request {
            s::StateFrame::Control { frame, .. } => {
                let answer = match frame {
                    s::StateControl::Read { selector } => {
                        assert_eq!(selector, &s::ReadSelector::OpenPairs {});
                        let Hold::Read(number) = self.classify(index) else {
                            unreachable!()
                        };
                        if self.read_refusal == Some(number) {
                            refused(s::ReadRefusalCode::ResultTooLarge)
                        } else {
                            let raw = if number == 1 {
                                &self.initial
                            } else {
                                &self.final_read
                            };
                            page(
                                10 + number as u64,
                                0,
                                raw.len(),
                                &raw[..raw.len().min(self.page_size)],
                            )
                        }
                    }
                    s::StateControl::Next {
                        snapshot_id,
                        offset,
                    } => {
                        if self.later_refusal {
                            refused(s::ReadRefusalCode::ResultUnavailable)
                        } else {
                            let raw = if snapshot_id.0 == 11 {
                                &self.initial
                            } else {
                                &self.final_read
                            };
                            let offset = offset.0 as usize;
                            let end = raw.len().min(offset.saturating_add(self.page_size));
                            page(snapshot_id.0, offset, raw.len(), &raw[offset..end])
                        }
                    }
                    s::StateControl::Release { snapshot_id } => s::StateControlAnswer::Released {
                        snapshot_id: *snapshot_id,
                    },
                    _ => panic!("non-read control"),
                };
                self.boot.member.answer(&request, answer)
            }
            s::StateFrame::FilePair { filing, .. } => {
                let began = self.boot.member.now() + 1;
                let observation = operation(&request, self.boot.member.next_operation, began);
                self.boot.member.next_operation += 1;
                if self.file_refusal {
                    s::StateFrame::Refused {
                        common: request.common().clone(),
                        code: s::StateRefusalCode::UnknownPair,
                        detail: "scripted missing old pair".into(),
                        block_id: Some(filing.block_id.clone()),
                        observation,
                    }
                } else {
                    s::StateFrame::Filed {
                        common: request.common().clone(),
                        block_id: filing.block_id.clone(),
                        observation,
                    }
                }
            }
            _ => panic!("boot repair wrote content outside file_pair"),
        };
        self.answered[index] = true;
        self.boot.member.receive(&answer)
    }
    fn step(&mut self) -> Result<BootPairsProgress, RunFailure> {
        let progress = self.advance()?;
        self.io()?;
        Ok(progress)
    }
    fn until_held(&mut self, held: Hold) {
        self.held = held;
        for _ in 0..256 {
            self.step().unwrap();
            if (0..self.requests.len()).any(|i| !self.answered[i] && self.classify(i) == held) {
                return;
            }
        }
        panic!("missing held {held:?}");
    }
    fn complete(&mut self) -> BootPairsCompletion {
        for _ in 0..512 {
            if self.step().unwrap() == BootPairsProgress::Complete {
                let run = self.run();
                return run
                    .context
                    .take_boot_pairs(&mut run.clock)
                    .unwrap()
                    .unwrap();
            }
        }
        panic!("boot repair did not finish");
    }
    fn records(&self, kind: Kind) -> Vec<serde_json::Value> {
        self.boot.member.records(kind)
    }
    fn queries(&self) -> usize {
        self.requests
            .iter()
            .filter(|request| {
                matches!(
                    request,
                    s::StateFrame::Control {
                        frame: s::StateControl::Read { .. },
                        ..
                    }
                )
            })
            .count()
    }
    fn terminals(&self) -> Vec<&s::PairFile> {
        self.requests
            .iter()
            .filter_map(|request| match request {
                s::StateFrame::FilePair { filing, .. } => Some(filing),
                _ => None,
            })
            .collect()
    }
}

#[test]
fn boot_pairs_repairs_old_calls_in_snapshot_order_with_exact_start_and_two_recorded_reads() {
    let calls = vec![old_pair("z-old-run"), old_pair("a-old-run")];
    let mut fx = RepairFixture::new(calls.clone());
    let enter = fx.run().context.deadlines.get(PumpDeadline::Enter).unwrap();
    let start = fx.run().context.open_start.as_ref().unwrap().clone();
    fx.page_size = 180;
    fx.begin();
    let BootPairsCompletion::Repaired(proof) = fx.complete() else {
        panic!("repair refused")
    };
    assert!(proof.belongs_to(&fx.run().context));
    assert_eq!(proof.initial().raw_bytes(), fx.initial);
    assert_eq!(proof.recheck().raw_bytes(), fx.final_read);
    assert_eq!(proof.terminals().len(), 2);
    assert_eq!(fx.queries(), 2);
    assert_eq!(fx.records(Kind::Recall).len(), 3); // Mandatory prompt plus both queries.
    for (index, filing) in fx.terminals().iter().enumerate() {
        let s::PairRecord::Ended(ended) = &filing.pair else {
            panic!("invented opening")
        };
        assert_eq!(ended.call_id, calls[index].call_id);
        assert_eq!(ended.fate, s::TerminalPairFate::Failed);
        assert_eq!(ended.reason, Some(s::PairFailure::AbandonedAtLoad {}));
        assert_eq!(ended.loaded_from, Some(start.clone()));
        assert!(ended.execution.is_none() && ended.outcome.is_none());
        assert_eq!(filing.block_id.run, "run");
        assert_eq!(filing.block_id.block.0, 0);
        assert!(filing.source.turn.is_none());
        assert_eq!(filing.source.origin, "loop");
        assert_eq!(filing.source.channel, "tool");
        assert_eq!(
            filing.raw_block.get(),
            serde_json::to_string(ended).unwrap()
        );
    }
    assert!(fx.records(Kind::ToolCallStarted).is_empty());
    assert!(fx.records(Kind::MessageToolResult).is_empty());
    assert_eq!(fx.records(Kind::Ste100Result).len(), 3); // Templates only.
    assert_eq!(
        fx.run().context.deadlines.get(PumpDeadline::Enter),
        Some(enter)
    );
    assert!(
        fx.run()
            .context
            .deadlines
            .get(PumpDeadline::Decision)
            .is_none()
    );
    assert!(
        fx.run()
            .context
            .deadlines
            .get(PumpDeadline::StateControl)
            .is_none()
    );
    let prior = PriorBootOpen::assume_checked_open_for_test(&fx.run().context);
    let run = fx.run();
    assert_eq!(
        run.context
            .begin_boot_pairs(prior, &mut run.clock)
            .unwrap_err()
            .refusal,
        BootPairsBeginRefusal::AlreadyStarted
    );
}

#[test]
fn boot_pairs_empty_initial_still_waits_for_distinct_recheck_release_and_recording() {
    let mut fx = RepairFixture::new(vec![]);
    fx.begin();
    fx.until_held(Hold::Release(2));
    assert_eq!(fx.queries(), 2);
    assert!(fx.terminals().is_empty());
    assert_eq!(fx.records(Kind::Recall).len(), 2);
    let run = fx.run();
    assert!(
        run.context
            .take_boot_pairs(&mut run.clock)
            .unwrap()
            .is_none()
    );
    fx.held = Hold::None;
    assert!(matches!(fx.complete(), BootPairsCompletion::Repaired(_)));
    assert_eq!(fx.records(Kind::Recall).len(), 3);
}

#[test]
fn boot_pairs_each_terminal_receipt_precedes_next_terminal_and_final_query() {
    let mut fx = RepairFixture::new(vec![old_pair("old-b"), old_pair("old-a")]);
    fx.begin();
    fx.until_held(Hold::Terminal(1));
    for _ in 0..8 {
        fx.step().unwrap();
    }
    assert_eq!(fx.terminals().len(), 1);
    assert_eq!(fx.queries(), 1);
    let first_bound = fx
        .run()
        .context
        .deadlines
        .get(PumpDeadline::Decision)
        .unwrap();
    fx.held = Hold::Terminal(2);
    for _ in 0..256 {
        fx.step().unwrap();
        if fx.terminals().len() == 2 {
            break;
        }
    }
    assert_eq!(fx.terminals().len(), 2);
    assert_eq!(fx.queries(), 1);
    let next_bound = fx
        .run()
        .context
        .deadlines
        .get(PumpDeadline::Decision)
        .unwrap();
    assert!(next_bound.began_ns() > first_bound.began_ns());
    assert_eq!(
        next_bound.deadline_ns() - next_bound.began_ns(),
        fx.run()
            .context
            .load_duty
            .as_ref()
            .unwrap()
            .instruction
            .decision_wait
            .elapsed_ns
            .0
    );
    fx.held = Hold::None;
    assert!(matches!(fx.complete(), BootPairsCompletion::Repaired(_)));
}

#[test]
fn boot_pairs_refused_or_nonempty_recheck_never_grants_repair_or_retries_batch() {
    for refusal in [Some(1), Some(2), None] {
        let mut fx = RepairFixture::new(vec![old_pair("old-run")]);
        fx.read_refusal = refusal;
        if refusal.is_none() {
            fx.final_read = fx.initial.clone();
        }
        fx.begin();
        let BootPairsCompletion::Refused {
            refusal: got,
            account,
        } = fx.complete()
        else {
            panic!("failed boot read became successful repair")
        };
        assert_eq!(
            got,
            if refusal.is_some() {
                BootPairsRefusal::Read
            } else {
                BootPairsRefusal::NonemptyRecheck
            }
        );
        assert_eq!(account.read_refusal().is_some(), refusal.is_some());
        assert_eq!(fx.queries(), if refusal == Some(1) { 1 } else { 2 });
        assert_eq!(fx.terminals().len(), if refusal == Some(1) { 0 } else { 1 });
        assert!(fx.run().context.failure().is_none());
    }
}

#[test]
fn boot_pairs_later_read_refusal_releases_exact_acquired_snapshot_once() {
    let mut fx = RepairFixture::new(vec![old_pair("old-run")]);
    fx.page_size = 100;
    fx.later_refusal = true;
    fx.begin();
    let BootPairsCompletion::Refused { account, .. } = fx.complete() else {
        panic!()
    };
    assert!(matches!(
        account.read_refusal(),
        Some(crate::flow::read::ReadFailure::Refused {
            code: s::ReadRefusalCode::ResultUnavailable,
            ..
        })
    ));
    assert_eq!(fx.queries(), 1);
    assert_eq!(
        fx.requests
            .iter()
            .filter(|request| matches!(request,
        s::StateFrame::Control { frame: s::StateControl::Release { snapshot_id }, .. }
            if snapshot_id.0 == 11))
            .count(),
        1
    );
    assert!(fx.terminals().is_empty());
    assert_eq!(fx.records(Kind::Recall).len(), 1);
}

#[test]
fn boot_pairs_member_refusal_or_malformed_initial_latches_and_retains_owner() {
    for malformed in [false, true] {
        let mut fx = RepairFixture::new(vec![old_pair("old-run")]);
        fx.file_refusal = !malformed;
        if malformed {
            fx.initial = br#"{"entries":{"entries":[]}}"#.to_vec();
        }
        fx.begin();
        assert!((0..256).find_map(|_| fx.step().err()).is_some());
        assert!(fx.run().context.failure().is_some());
        assert!(fx.run().context.boot_pairs.is_some());
        let run = fx.run();
        assert!(run.context.take_boot_pairs(&mut run.clock).is_err());
        assert_eq!(fx.queries(), 1);
        assert_eq!(fx.terminals().len(), usize::from(!malformed));
    }
}

#[test]
fn boot_pairs_foreign_prior_open_and_retired_enter_do_not_admit() {
    let mut fx = RepairFixture::new(vec![]);
    let mut other = RepairFixture::new(vec![]);
    let prior = other.prior.take().unwrap();
    let run = fx.run();
    let error = run
        .context
        .begin_boot_pairs(prior, &mut run.clock)
        .unwrap_err();
    assert_eq!(error.refusal, BootPairsBeginRefusal::WrongOpen);
    assert!(run.context.boot_pairs.is_none());
    run.context.complete_wait(PumpDeadline::Enter).unwrap();
    let prior = fx.prior.take().unwrap();
    let run = fx.run();
    assert_eq!(
        run.context
            .begin_boot_pairs(prior, &mut run.clock)
            .unwrap_err()
            .refusal,
        BootPairsBeginRefusal::WrongOpen
    );
    assert!(run.context.boot_pairs.is_none());
}

#[test]
fn boot_pairs_owns_read_author_and_waits_until_completion_is_taken() {
    let mut fx = RepairFixture::new(vec![]);
    fx.begin();
    let run = fx.run();
    let event = AuthoredEvent {
        kind: Kind::ModelMeasurement,
        subsystem: Subsystem::Harness,
        turn: None,
        source: crate::flow::author::AuthoredSource::Runtime,
        causal_parent: None,
        payload: Some(Payload::ModelMeasurement(
            serde_json::value::to_raw_value(&serde_json::json!({"other":"owner"})).unwrap(),
        )),
    };
    assert_eq!(
        run.context
            .author_and_wait(&mut ports(&mut run.clock), event)
            .unwrap_err(),
        RunFailure::BootPairsOwned
    );
    assert!(run.context.refused_author().is_some());
    for active_read in [false, true] {
        if active_read {
            fx.until_held(Hold::Release(1));
        }
        let run = fx.run();
        assert!(run.context.complete_wait(PumpDeadline::Enter).is_none());
        assert_eq!(
            run.context
                .begin_read(s::ReadSelector::OpenPairs {}, None, &mut run.clock)
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
        assert_eq!(
            run.context.take_read(&mut run.clock).unwrap_err(),
            RunFailure::BootPairsOwned
        );
        assert_eq!(
            run.context.cancel_read(&mut run.clock).unwrap_err(),
            RunFailure::BootPairsOwned
        );
        assert_eq!(
            run.context
                .advance_read(&mut ports(&mut run.clock))
                .unwrap_err(),
            RunFailure::BootPairsOwned
        );
        assert_eq!(
            run.context
                .take_operation_completion(&mut run.clock)
                .unwrap_err(),
            RunFailure::BootPairsOwned
        );
    }
    fx.held = Hold::None;
    assert!((0..256).any(|_| fx.step().unwrap() == BootPairsProgress::Complete));
    let run = fx.run();
    assert_eq!(
        run.context
            .begin_read(s::ReadSelector::OpenPairs {}, None, &mut run.clock)
            .unwrap_err()
            .refusal,
        ReadBeginRefusal::Busy
    );
    assert!(matches!(
        run.context.take_boot_pairs(&mut run.clock).unwrap(),
        Some(BootPairsCompletion::Repaired(_))
    ));
    assert!(
        run.context
            .begin_read(s::ReadSelector::OpenPairs {}, None, &mut run.clock)
            .is_ok()
    );
}

#[test]
fn boot_pairs_cancel_revokes_success_but_accounts_sent_read_and_terminal() {
    for point in [
        Hold::None,
        Hold::Release(1),
        Hold::Terminal(1),
        Hold::Release(2),
    ] {
        let mut fx = RepairFixture::new(vec![old_pair("old-b"), old_pair("old-a")]);
        fx.begin();
        if point != Hold::None {
            fx.until_held(point);
        }
        let before = fx.terminals().len();
        let run = fx.run();
        run.context.cancel_boot_pairs(&mut run.clock).unwrap();
        fx.held = Hold::None;
        assert!(matches!(fx.complete(), BootPairsCompletion::Cancelled(_)));
        assert_eq!(fx.terminals().len(), before);
        assert_eq!(
            fx.queries(),
            match point {
                Hold::None => 0,
                Hold::Release(2) => 2,
                _ => 1,
            }
        );
        assert!(fx.run().context.read.is_none());
        assert!(fx.run().context.filing.is_none());
        assert!(fx.run().context.failure().is_none());
        assert!(fx.records(Kind::MessageToolResult).is_empty());
    }
}

#[test]
fn boot_pairs_cancel_withdraws_staged_unstarted_terminal_without_record() {
    let mut fx = RepairFixture::new(vec![old_pair("old-run")]);
    fx.begin();
    for _ in 0..128 {
        fx.step().unwrap();
        if fx.run().context.filing.is_some() {
            break;
        }
    }
    assert!(fx.run().context.waiting_content.is_some());
    assert!(fx.records(Kind::ToolCallCompleted).is_empty());
    let run = fx.run();
    run.context.cancel_boot_pairs(&mut run.clock).unwrap();
    assert!(matches!(fx.complete(), BootPairsCompletion::Cancelled(_)));
    assert!(fx.terminals().is_empty());
    assert!(fx.records(Kind::ToolCallCompleted).is_empty());
}

#[test]
fn boot_pairs_recall_pressure_preserves_exact_ticket_before_any_terminal() {
    let mut fx = RepairFixture::new(vec![old_pair("old-run")]);
    fx.begin();
    fx.until_held(Hold::Release(1));
    let index = fx.answered.iter().position(|answered| !answered).unwrap();
    fx.answer(index).unwrap();
    // Reduce actual Released plus its operation record without advancing the
    // repair's provenance owner into the recall author yet.
    for _ in 0..64 {
        let run = fx.run();
        run.context.reduce_flow(&mut run.clock).unwrap();
        if run.context.read_blocks_reduction() {
            break;
        }
    }
    assert!(fx.run().context.read_blocks_reduction());
    let bound = fx
        .run()
        .context
        .deadlines
        .get(PumpDeadline::StateControl)
        .unwrap();
    fx.boot.member.fill_trace();
    assert_eq!(fx.advance().unwrap(), BootPairsProgress::Pending);
    let ticket = fx.run().context.pending_author().unwrap().ticket;
    let payload =
        serde_json::to_string(&fx.run().context.pending_author().unwrap().event.payload).unwrap();
    for _ in 0..4 {
        assert_eq!(fx.advance().unwrap(), BootPairsProgress::Pending);
        assert_eq!(fx.run().context.pending_author().unwrap().ticket, ticket);
        assert_eq!(
            serde_json::to_string(&fx.run().context.pending_author().unwrap().event.payload)
                .unwrap(),
            payload
        );
        assert_eq!(
            fx.run().context.deadlines.get(PumpDeadline::StateControl),
            Some(bound)
        );
        assert!(fx.terminals().is_empty());
    }
    recall_tests::drain_admitted(&mut fx.boot.member);
    fx.held = Hold::None;
    assert!(matches!(fx.complete(), BootPairsCompletion::Repaired(_)));
    assert_eq!(fx.records(Kind::Recall).len(), 3);
    assert_eq!(fx.records(Kind::ToolCallCompleted).len(), 1);
}

#[test]
fn boot_pairs_keeps_original_enter_and_short_read_bounds_across_progress() {
    let mut fx = RepairFixture::new(vec![old_pair("old-run")]);
    let enter = fx.run().context.deadlines.get(PumpDeadline::Enter).unwrap();
    fx.page_size = 100;
    fx.begin();
    fx.until_held(Hold::Next);
    let read = fx
        .run()
        .context
        .deadlines
        .get(PumpDeadline::StateControl)
        .unwrap();
    assert_eq!(read.deadline_ns(), enter.deadline_ns()); // Fixture's 1s Enter is nearer.
    for _ in 0..8 {
        fx.step().unwrap();
    }
    assert_eq!(
        fx.run().context.deadlines.get(PumpDeadline::StateControl),
        Some(read)
    );
    assert_eq!(
        fx.run().context.deadlines.get(PumpDeadline::Enter),
        Some(enter)
    );
    fx.run().clock.advance_to(enter.deadline_ns()).unwrap();
    assert!(
        matches!(fx.advance(), Err(RunFailure::Wait { key: PumpDeadline::Enter, bound }) if bound == enter)
    );
    assert!(fx.run().context.read.is_some());
    assert!(fx.terminals().is_empty());
}

struct CrossingClock<'a> {
    clock: &'a mut FakePumpIo,
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
        interests: &[WaitInterest<'_>],
        deadline: Option<u64>,
    ) -> std::io::Result<WaitOutcome> {
        self.clock.wait(interests, deadline)
    }
}

#[test]
fn boot_pairs_nested_leave_preserves_read_and_terminal_receipt_continuations() {
    let mut retained_read = false;
    let mut retained_terminal = false;
    for terminal in [false, true] {
        for remaining in 0..24 {
            let mut fx = RepairFixture::new(vec![old_pair("old-run")]);
            fx.begin();
            fx.until_held(if terminal {
                Hold::Terminal(1)
            } else {
                Hold::Release(1)
            });
            let index = fx.answered.iter().position(|answered| !answered).unwrap();
            fx.answer(index).unwrap();
            let now = fx.boot.member.now();
            let leave = AbsoluteBound::after(now, 5).unwrap();
            let run = fx.run();
            run.context
                .arm(PumpDeadline::Leave, leave, &mut run.clock)
                .unwrap();
            let mut clock = CrossingClock {
                clock: &mut run.clock,
                remaining,
                deadline: leave.deadline_ns(),
            };
            let mut result = Ok(BootPairsProgress::Pending);
            for _ in 0..64 {
                result = run.context.advance_boot_pairs(&mut ports(&mut clock));
                if result.is_err()
                    || run.context.filing.is_none() && terminal
                    || !terminal && run.context.read.is_none()
                {
                    break;
                }
            }
            if let Err(error) = result {
                assert_eq!(
                    error,
                    RunFailure::Wait {
                        key: PumpDeadline::Leave,
                        bound: leave
                    }
                );
                retained_read |= !terminal && run.context.read.is_some();
                retained_terminal |= terminal && run.context.filing.is_some();
            }
            assert_eq!(run.context.complete_wait(PumpDeadline::Leave), Some(leave));
            if !terminal
                && run
                    .context
                    .deadlines
                    .get(PumpDeadline::Decision)
                    .is_some_and(|bound| bound.deadline_ns() == leave.deadline_ns())
            {
                // A terminal admitted during Leave keeps that inherited cap.
                // Removing Leave never grants this barrier a new window.
                run.clock.advance_to(leave.deadline_ns()).unwrap();
                assert!(matches!(
                    fx.advance(),
                    Err(RunFailure::Flow {
                        reason: FlowHaltReason::DecisionBound,
                        ..
                    })
                ));
                assert!(fx.run().context.boot_pairs.is_some());
                assert!(fx.records(Kind::ToolCallCompleted).is_empty());
                continue;
            }
            fx.held = Hold::None;
            assert!(matches!(fx.complete(), BootPairsCompletion::Repaired(_)));
            assert_eq!(fx.records(Kind::ToolCallCompleted).len(), 1);
            assert_eq!(fx.records(Kind::Recall).len(), 3);
            assert_eq!(fx.queries(), 2);
        }
    }
    assert!(
        retained_read && retained_terminal,
        "nested lifecycle checkpoints were missed"
    );
}

fn recorded_terminal_handoff() -> (RepairFixture, AbsoluteBound) {
    let mut fx = RepairFixture::new(vec![old_pair("old-run")]);
    fx.begin();
    fx.until_held(Hold::Terminal(1));
    let bound = fx
        .run()
        .context
        .deadlines
        .get(PumpDeadline::Decision)
        .unwrap();
    let index = fx.answered.iter().position(|answered| !answered).unwrap();
    fx.answer(index).unwrap();
    for _ in 0..64 {
        let run = fx.run();
        run.context.reduce_flow(&mut run.clock).unwrap();
        if run
            .context
            .owned_filing_recorded_before(receipt_driver::FilingOwner::BootPairs, bound)
        {
            break;
        }
    }
    assert!(
        fx.run()
            .context
            .owned_filing_recorded_before(receipt_driver::FilingOwner::BootPairs, bound)
    );
    assert!(fx.run().context.filing.is_some());
    let now = fx.boot.member.now();
    let leave = AbsoluteBound::after(now, 5).unwrap();
    let run = fx.run();
    run.context
        .arm(PumpDeadline::Leave, leave, &mut run.clock)
        .unwrap();
    (fx, leave)
}

#[test]
fn boot_pairs_recorded_terminal_transfer_never_yields_without_its_receipt_owner() {
    // Start at the actual Recorded receipt, so earlier observation recording
    // cannot hide the later transfer behind a guessed number of clock samples.
    let (mut baseline, leave) = recorded_terminal_handoff();
    let run = baseline.run();
    let mut clock = CrossingClock {
        clock: &mut run.clock,
        remaining: usize::MAX,
        deadline: leave.deadline_ns(),
    };
    assert_eq!(
        run.context
            .advance_boot_pairs(&mut ports(&mut clock))
            .unwrap(),
        BootPairsProgress::Pending
    );
    let samples = usize::MAX - clock.remaining;
    assert!(samples > 0);
    assert!(run.context.filing.is_none());
    let mut retained = false;
    let mut transferred = false;
    for remaining in 0..=samples {
        let (mut fx, leave) = recorded_terminal_handoff();
        let run = fx.run();
        let mut clock = CrossingClock {
            clock: &mut run.clock,
            remaining,
            deadline: leave.deadline_ns(),
        };
        match run.context.advance_boot_pairs(&mut ports(&mut clock)) {
            Err(error) => {
                assert_eq!(
                    error,
                    RunFailure::Wait {
                        key: PumpDeadline::Leave,
                        bound: leave
                    }
                );
                assert!(
                    run.context.filing.is_some(),
                    "sample {remaining}/{samples} yielded after consuming the exact terminal receipt"
                );
                retained = true;
            }
            Ok(progress) => {
                assert_eq!(progress, BootPairsProgress::Pending);
                assert!(run.context.filing.is_none());
                transferred = true;
            }
        }
        assert_eq!(run.context.complete_wait(PumpDeadline::Leave), Some(leave));
        fx.held = Hold::None;
        let BootPairsCompletion::Repaired(proof) = fx.complete() else {
            panic!("repair");
        };
        assert_eq!(proof.terminals().len(), 1);
        assert!(proof.terminals()[0].receipts().is_complete());
        assert_eq!(fx.records(Kind::ToolCallCompleted).len(), 1);
        assert_eq!(fx.records(Kind::Recall).len(), 3);
        assert_eq!(fx.queries(), 2);
    }
    assert!(
        retained && transferred,
        "both sides of the actual transfer must run"
    );
}

#[test]
fn boot_pairs_timely_recorded_receipt_survives_late_consumer_before_enter_expires() {
    let mut fx = RepairFixture::new(vec![old_pair("old-run")]);
    fx.begin();
    fx.until_held(Hold::Terminal(1));
    let bound = fx
        .run()
        .context
        .deadlines
        .get(PumpDeadline::Decision)
        .unwrap();
    let index = fx.answered.iter().position(|answered| !answered).unwrap();
    fx.answer(index).unwrap();
    for _ in 0..64 {
        let run = fx.run();
        run.context.reduce_flow(&mut run.clock).unwrap();
        if run
            .context
            .owned_filing_recorded_before(receipt_driver::FilingOwner::BootPairs, bound)
        {
            break;
        }
    }
    assert!(
        fx.run()
            .context
            .owned_filing_recorded_before(receipt_driver::FilingOwner::BootPairs, bound)
    );
    fx.run().clock.advance_to(bound.deadline_ns() + 1).unwrap();
    fx.held = Hold::None;
    assert!(matches!(fx.complete(), BootPairsCompletion::Repaired(_)));
    assert_eq!(fx.records(Kind::ToolCallCompleted).len(), 1);
}

#[test]
fn boot_pairs_read_admitted_under_leave_keeps_that_cap_after_leave_retires() {
    let mut fx = RepairFixture::new(vec![]);
    fx.begin();
    let now = fx.boot.member.now();
    let leave = AbsoluteBound::after(now, 20).unwrap();
    let run = fx.run();
    run.context
        .arm(PumpDeadline::Leave, leave, &mut run.clock)
        .unwrap();
    fx.until_held(Hold::Read(1));
    let read = fx
        .run()
        .context
        .deadlines
        .get(PumpDeadline::StateControl)
        .unwrap();
    assert_eq!(read.deadline_ns(), leave.deadline_ns());
    fx.run().clock.advance_to(leave.deadline_ns()).unwrap();
    assert_eq!(
        fx.advance().unwrap_err(),
        RunFailure::Wait {
            key: PumpDeadline::Leave,
            bound: leave
        }
    );
    assert_eq!(
        fx.run().context.complete_wait(PumpDeadline::Leave),
        Some(leave)
    );
    assert!(matches!(
        fx.advance(),
        Err(RunFailure::Flow {
            reason: FlowHaltReason::ControlBound,
            ..
        })
    ));
    assert!(fx.run().context.read.is_some());
    assert!(fx.run().context.boot_pairs.is_some());
    assert!(fx.terminals().is_empty());
}

#[test]
fn boot_pairs_current_run_call_is_not_relabelled_as_abandoned_at_load() {
    let mut fx = RepairFixture::new(vec![old_pair("run")]);
    fx.begin();
    assert!((0..256).find_map(|_| fx.step().err()).is_some());
    assert!(fx.run().context.failure().is_some());
    assert!(fx.run().context.boot_pairs.is_some());
    assert!(fx.terminals().is_empty());
    assert!(fx.records(Kind::ToolCallCompleted).is_empty());
}

#[test]
fn inspection_rejects_boot_owner_initial_between_legs_and_complete_until_take() {
    for point in 0..4 {
        let mut fx = RepairFixture::new(vec![old_pair("old-run")]);
        let enter = fx.run().context.deadlines.get(PumpDeadline::Enter);
        fx.begin();
        match point {
            0 => {}
            1 => assert!((0..512).any(|_| {
                fx.step().unwrap();
                fx.records(Kind::Recall).len() == 2
                    && fx.run().context.read.is_none()
                    && fx.run().context.filing.is_none()
                    && fx.terminals().is_empty()
            })),
            2 => assert!((0..512).any(|_| {
                fx.step().unwrap();
                fx.records(Kind::ToolCallCompleted).len() == 1
                    && fx.run().context.read.is_none()
                    && fx.run().context.filing.is_none()
                    && fx.queries() == 1
            })),
            3 => assert!((0..512).any(|_| { fx.step().unwrap() == BootPairsProgress::Complete })),
            _ => unreachable!(),
        }
        let requests = fx.requests.len();
        let turn = Some(TurnRef(format!("blocked-inspection-{point}")));
        let run = fx.run();
        assert!(run.context.boot_pairs.is_some());
        assert!(run.context.read.is_none() && run.context.filing.is_none());
        assert!(run.context.flow.state().outstanding().is_empty());
        let decision = run.context.deadlines.get(PumpDeadline::Decision);
        let error = run
            .context
            .begin_inspection(InspectionKind::Grants, turn.clone(), &mut run.clock)
            .unwrap_err();
        assert_eq!(error.kind, InspectionKind::Grants);
        assert_eq!(error.turn, turn);
        assert_eq!(error.refusal, InspectionBeginRefusal::Busy);
        assert!(error.failure.is_none());
        assert!(run.context.inspection.is_none());
        assert!(run.context.flow.state().outstanding().is_empty());
        assert_eq!(run.context.deadlines.get(PumpDeadline::Enter), enter);
        assert_eq!(run.context.deadlines.get(PumpDeadline::Decision), decision);
        assert!(
            run.context
                .deadlines
                .get(PumpDeadline::StateControl)
                .is_none()
        );
        let mut byte = [0];
        assert_eq!(
            run.state.read(&mut byte).unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
        assert_eq!(fx.requests.len(), requests);

        let BootPairsCompletion::Repaired(proof) = fx.complete() else {
            panic!("busy inspection changed the boot repair outcome")
        };
        assert!(proof.belongs_to(&fx.run().context));
        assert_eq!(proof.initial().raw_bytes(), fx.initial);
        assert_eq!(proof.recheck().raw_bytes(), fx.final_read);
        assert_eq!(proof.terminals().len(), 1);
        assert_eq!(fx.terminals().len(), 1);
        assert_eq!(fx.queries(), 2);
        assert_eq!(fx.records(Kind::Recall).len(), 3);
        let member = &mut fx.boot.member;
        member
            .run
            .context
            .begin_inspection(InspectionKind::Grants, None, &mut member.run.clock)
            .unwrap();
        let request = member.sent();
        let answer = member.answer(
            &request,
            s::StateControlAnswer::Grants {
                surface: vec!["observed".into()],
            },
        );
        member.receive(&answer).unwrap();
        assert!((0..128).any(|_| {
            member
                .run
                .context
                .advance_inspection(&mut ports(&mut member.run.clock))
                .unwrap()
                == InspectionProgress::Complete
        }));
        let Some(InspectionCompletion::Grants { surface, evidence }) = member
            .run
            .context
            .take_inspection(&mut member.run.clock)
            .unwrap()
        else {
            panic!("inspection did not start after boot proof transfer")
        };
        assert_eq!(surface, vec!["observed"]);
        assert_eq!(
            s::encode_state_line(evidence.answer(), s::FrameDirection::Response).unwrap(),
            s::encode_state_line(&answer, s::FrameDirection::Response).unwrap()
        );
        assert_eq!(member.run.context.deadlines.get(PumpDeadline::Enter), enter);
    }
}

#[test]
fn inspection_returns_exact_boot_prerequisite_while_pending_or_complete() {
    for completed in [false, true] {
        let mut fx = RepairFixture::new(vec![old_pair("old-run")]);
        let enter = fx.run().context.deadlines.get(PumpDeadline::Enter);
        let member = &mut fx.boot.member;
        member
            .run
            .context
            .begin_inspection(InspectionKind::Shape, None, &mut member.run.clock)
            .unwrap();
        let request = member.sent();
        let answer = member.answer(&request, s::StateControlAnswer::Shape(vec![]));
        if completed {
            member.receive(&answer).unwrap();
            assert!((0..128).any(|_| {
                member
                    .run
                    .context
                    .advance_inspection(&mut ports(&mut member.run.clock))
                    .unwrap()
                    == InspectionProgress::Complete
            }));
        }
        let prior = fx.prior.take().unwrap();
        let run = fx.run();
        let outstanding: Vec<_> = run
            .context
            .flow
            .state()
            .outstanding()
            .iter()
            .map(|item| item.common.request_id)
            .collect();
        let control = run.context.deadlines.get(PumpDeadline::StateControl);
        let error = run
            .context
            .begin_boot_pairs(prior, &mut run.clock)
            .unwrap_err();
        assert_eq!(error.refusal, BootPairsBeginRefusal::Busy);
        assert!(error.failure.is_none());
        assert!(!run.context.boot_pairs_started);
        assert!(run.context.boot_pairs.is_none());
        assert!(run.context.inspection.is_some());
        assert_eq!(
            run.context
                .flow
                .state()
                .outstanding()
                .iter()
                .map(|item| item.common.request_id)
                .collect::<Vec<_>>(),
            outstanding
        );
        assert_eq!(run.context.deadlines.get(PumpDeadline::Enter), enter);
        assert_eq!(
            run.context.deadlines.get(PumpDeadline::StateControl),
            control
        );
        let mut byte = [0];
        assert_eq!(
            run.state.read(&mut byte).unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
        let member = &mut fx.boot.member;
        if !completed {
            member.receive(&answer).unwrap();
            assert!((0..128).any(|_| {
                member
                    .run
                    .context
                    .advance_inspection(&mut ports(&mut member.run.clock))
                    .unwrap()
                    == InspectionProgress::Complete
            }));
        }
        let Some(InspectionCompletion::Shape { shape, evidence }) = member
            .run
            .context
            .take_inspection(&mut member.run.clock)
            .unwrap()
        else {
            panic!("boot admission stole the recorded inspection")
        };
        assert!(shape.runs.is_empty());
        assert_eq!(
            s::encode_state_line(evidence.answer(), s::FrameDirection::Response).unwrap(),
            s::encode_state_line(&answer, s::FrameDirection::Response).unwrap()
        );
        let run = fx.run();
        // Retry only the sole prerequisite returned by the refused admission.
        run.context
            .begin_boot_pairs(error.prior, &mut run.clock)
            .unwrap();
        let BootPairsCompletion::Repaired(proof) = fx.complete() else {
            panic!("returned prerequisite lost its original context")
        };
        assert!(proof.belongs_to(&fx.run().context));
        assert_eq!(proof.initial().raw_bytes(), fx.initial);
        assert_eq!(proof.recheck().raw_bytes(), fx.final_read);
        assert_eq!(proof.terminals().len(), 1);
        assert_eq!(fx.run().context.deadlines.get(PumpDeadline::Enter), enter);
    }
}

#[path = "context_boot_pairs_review_tests.rs"]
mod review_tests;
