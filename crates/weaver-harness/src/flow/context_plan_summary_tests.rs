//! The dormant summary consumer through actual checked Open, repaired pairs,
//! classifier packets, filed receipts, Released reads and recorder barriers.
//! Scripted members prove custody, not SQL or the absent plan producer policy.

use super::super::pair_repair_tests::{RepairFixture, old_pair};
use super::*;
use crate::flow::author::AuthoredSource;
use crate::flow::coordinator::{TransportSource, tests::operation};
use crate::flow::ledger::{LedgerFile, SourceDisposition};
use crate::flow::ste100_channel::{Receive, Tag};
use weaver_trace::serving::payloads as t;
use weaver_traits::{ContentBlock, Message, Role};

struct SummaryFixture {
    boot: BootFixture,
    repaired: Option<RepairedBootPairs>,
    _decode: Option<(DecodePort, crate::channel::ChildEnd)>,
    files: Vec<s::StatementFile>,
}

impl SummaryFixture {
    fn new(pairs: Vec<s::ReadPair>) -> Self {
        Self::from_open(OpenFixture::new(), pairs)
    }
    fn with_enter(remaining_enter_ns: u64) -> Self {
        let mut templates = TemplateFixture::with_enter(remaining_enter_ns);
        templates.begin();
        for index in 0..3 {
            templates.accept(index);
        }
        assert_eq!(templates.advance().unwrap(), TemplateProgress::Complete);
        let mut boot = BootFixture::from_templates(templates);
        let entry = boot.entry();
        let request = boot.begin();
        boot.released(&request, entry);
        assert!(matches!(boot.finish(), BootReadCompletion::Seated));
        let (decode, peer) = DecodeChannel::pair().unwrap();
        Self::from_open(
            OpenFixture {
                boot,
                decode: DecodePort::from_channel(decode).unwrap(),
                peer,
            },
            vec![],
        )
    }
    fn from_open(mut open: OpenFixture, pairs: Vec<s::ReadPair>) -> Self {
        open.begin();
        assert!(matches!(open.sent(), TokenDirective::Open { .. }));
        open.answer(open.opened());
        let checked = open.finish();
        let OpenFixture { boot, decode, peer } = open;
        let mut repair = RepairFixture::from_checked(boot, checked, pairs);
        repair.begin();
        let BootPairsCompletion::Repaired(repaired) = repair.complete() else {
            panic!("actual Open and repaired pair chain must complete");
        };
        Self {
            boot: repair.into_boot(),
            repaired: Some(repaired),
            _decode: Some((decode, peer)),
            files: vec![],
        }
    }
    fn run(&mut self) -> &mut Fixture {
        &mut self.boot.member.run
    }
    fn seed(&self) -> &str {
        &self
            .boot
            .member
            .run
            .context
            .open_start
            .as_ref()
            .unwrap()
            .seed_id
    }
    fn records(&self, kind: Kind) -> Vec<serde_json::Value> {
        self.boot.member.records(kind)
    }
    fn begin_boot(&mut self) {
        let repaired = self.repaired.take().unwrap();
        let run = self.run();
        run.context
            .begin_boot_plan_summary(repaired, &mut run.clock)
            .unwrap();
    }
    fn begin_effect(&mut self, filing: FilingCompletion) {
        let run = self.run();
        run.context
            .begin_effect_plan_summary(filing, &mut run.clock)
            .unwrap();
    }
    fn advance(&mut self) -> Result<PlanSummaryProgress, RunFailure> {
        let run = self.run();
        run.context.advance_plan_summary(&mut ports(&mut run.clock))
    }
    fn next_request(&mut self) -> s::StateFrame {
        for _ in 0..128 {
            self.advance().unwrap();
            if !self
                .boot
                .member
                .run
                .context
                .flow
                .state()
                .outstanding()
                .is_empty()
            {
                return self.boot.member.sent();
            }
        }
        panic!("summary owner did not issue its next request");
    }
    fn release(&mut self, snapshot: Option<s::PlanSnapshot>) -> Vec<u8> {
        let request = self.next_request();
        assert!(matches!(
            &request,
            s::StateFrame::Control {
                frame: s::StateControl::Read {
                    selector: s::ReadSelector::PlanCurrent {}
                },
                ..
            }
        ));
        let raw = summary_bytes(snapshot);
        self.boot
            .member
            .respond(&request, page(41, 0, raw.len(), &raw));
        let release = self.next_request();
        assert!(matches!(&release, s::StateFrame::Control {
            frame: s::StateControl::Release { snapshot_id }, .. } if snapshot_id.0 == 41));
        self.boot.member.respond(
            &release,
            s::StateControlAnswer::Released {
                snapshot_id: 41.into(),
            },
        );
        raw
    }
    fn finish(&mut self) -> PlanSummaryCompletion {
        for _ in 0..128 {
            self.advance().unwrap();
            let run = self.run();
            if let Some(done) = run.context.take_plan_summary(&mut run.clock).unwrap() {
                return done;
            }
        }
        panic!("summary owner did not finish");
    }
    fn boot(&mut self, snapshot: Option<s::PlanSnapshot>) -> BootPlanSummary {
        self.begin_boot();
        self.release(snapshot);
        let PlanSummaryCompletion::Boot(proof) = self.finish() else {
            panic!("boot summary refused");
        };
        proof
    }
    fn snapshot(&self) -> s::PlanSnapshot {
        let cases: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("fixtures/recall-sources.json")).unwrap();
        let mut value = cases
            .iter()
            .find(|case| case["case"] == "plan_current_populated")
            .unwrap()["result"]["plan_current"]["current"]
            .clone();
        value["plan"] =
            serde_json::json!({"plan_id":{"seed_id":self.seed(),"local_id":"3"},"revision":"1"});
        for name in ["task", "prompt"] {
            value["task"][name]["seed_id"] = self.seed().into();
        }
        value["status"] = "authorized".into();
        value["operator_source"] =
            serde_json::json!({"run":"prior-run","sequence":"1","block":"0"});
        value["lock_source"] = serde_json::Value::Null;
        value["last_source"] = serde_json::json!({"block_id":{"run":"prior-run","sequence":"2","block":"0"},"turn":null});
        value["steps"] = serde_json::json!([]);
        value["broken"] = serde_json::Value::Null;
        serde_json::from_value(value).unwrap()
    }
    fn transport(&mut self, source: TransportSource) {
        let run = self.run();
        run.context
            .flow
            .transport_one(
                &mut FlowRecording {
                    barrier: &mut run.context.barrier,
                    inbox: &mut run.context.inbox,
                    budget: &mut run.context.budget,
                    author: &run.context.author,
                },
                &mut run.clock,
                source,
            )
            .unwrap();
    }
    fn classifier(&mut self, reject: bool) {
        self.transport(TransportSource::Classifier);
        let Receive::Complete(message) = self.boot._classifier.try_receive().unwrap() else {
            return;
        };
        assert_eq!(message.tag, Tag::Request);
        let request: s::Ste100Request =
            s::decode_strict(std::str::from_utf8(&message.payload).unwrap()).unwrap();
        assert_eq!(request.text, "Read plan.");
        let result = if reject {
            s::Ste100Result::NotSte100 {
                code: s::Ste100DefectCode::NonSte100,
                detail: "fixture correct rejection".into(),
                spans: vec![],
            }
        } else {
            s::Ste100Result::Accepted {
                bindings: vec![s::Binding {
                    statement_span: s::Span {
                        start: 0.into(),
                        end: 10.into(),
                    },
                    verb: "read".into(),
                    verb_span: s::Span {
                        start: 0.into(),
                        end: 4.into(),
                    },
                    condition: None,
                    condition_span: None,
                    nouns: vec![s::NounBinding {
                        value: "plan".into(),
                        span: s::Span {
                            start: 5.into(),
                            end: 9.into(),
                        },
                    }],
                }],
            }
        };
        let reply = s::Ste100Reply {
            request_id: request.request_id,
            target: request.target,
            scheme_id: request.scheme_id,
            result,
        };
        self.boot
            ._classifier
            .begin_send(
                Tag::Reply,
                reply.request_id.0,
                serde_json::to_vec(&reply).unwrap(),
            )
            .unwrap();
        while self.boot._classifier.has_pending_send() {
            self.boot._classifier.try_send().unwrap();
        }
        self.transport(TransportSource::Classifier);
    }
    fn file_reply(&mut self) {
        self.transport(TransportSource::State);
        let mut bytes = [0; 65536];
        let n = match self.boot.member.run.state.read(&mut bytes) {
            Ok(0) => panic!("filing member closed"),
            Ok(n) => n,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return,
            Err(error) => panic!("filing member: {error}"),
        };
        let request = s::decode_state_line(&bytes[..n], s::FrameDirection::Request).unwrap();
        let s::StateFrame::File { filing, .. } = &request else {
            panic!("expected real statement filing");
        };
        self.files.push(filing.clone());
        let began = self.boot.member.now() + 1;
        let observation = operation(&request, self.boot.member.next_operation, began);
        self.boot.member.next_operation += 1;
        self.boot
            .member
            .receive(&s::StateFrame::Filed {
                common: request.common().clone(),
                block_id: filing.block_id.clone(),
                observation,
            })
            .unwrap();
    }
    fn begin_file(&mut self, event: AuthoredEvent) {
        let run = self.run();
        let now = run.clock.now().unwrap();
        let bound = AbsoluteBound::after(now, 1_000_000).unwrap();
        if run.context.deadlines.get(PumpDeadline::Decision).is_none() {
            run.context
                .arm(PumpDeadline::Decision, bound, &mut run.clock)
                .unwrap();
        }
        run.context
            .begin_filing(event, PumpDeadline::Decision, &mut run.clock)
            .unwrap();
    }
    fn file(&mut self, event: AuthoredEvent, reject: bool) -> FilingCompletion {
        self.begin_file(event);
        for _ in 0..256 {
            let run = self.run();
            let progress = run
                .context
                .advance_filing(&mut ports(&mut run.clock))
                .unwrap();
            self.classifier(reject);
            self.file_reply();
            if progress == FilingProgress::Complete {
                let run = self.run();
                return run.context.take_filing(&mut run.clock).unwrap().unwrap();
            }
        }
        panic!("real plan effect filing did not settle");
    }
    fn drain(&mut self) {
        super::super::super::recall_tests::drain_admitted(&mut self.boot.member);
    }
    fn advance_failure(&mut self) -> RunFailure {
        for _ in 0..128 {
            if let Err(failure) = self.advance() {
                return failure;
            }
        }
        panic!("summary must fail without granting a stale cache");
    }
    fn halt_account(&mut self) -> PlanSummaryAccount {
        let run = self.run();
        assert!(run.context.failure().is_some());
        if run.context.halt().is_some() {
            run.context
                .freeze_halt_account(ExternalFacts {
                    outstanding_pairs: vec![],
                    capture: None,
                    wait: run.context.halt_wait().cloned(),
                    input_id: None,
                })
                .unwrap();
            run.context.begin_halt_report().unwrap();
            run.context.advance_halt_trace(&mut run.clock).unwrap();
            if run.context.plan_owns_pending_recall() {
                let ticket = run.context.pending_author().unwrap().ticket;
                assert_eq!(
                    run.context.advance_halt_trace(&mut run.clock).unwrap(),
                    HaltTraceProgress::Pending
                );
                assert_eq!(run.context.pending_author().unwrap().ticket, ticket);
            }
        }
        self.finish_halt_account()
    }
    fn finish_halt_account(&mut self) -> PlanSummaryAccount {
        let run = self.run();
        for _ in 0..32 {
            run.context
                .advance_plan_summary_account(&mut run.clock)
                .unwrap();
            if let Some(account) = run.context.take_stopped_plan_summary().unwrap() {
                return account;
            }
        }
        // A failed unfinished snapshot cannot be retried by this trace-only
        // account. Its original bound licenses incomplete retirement.
        let end = run
            .context
            .deadlines
            .get(PumpDeadline::StateControl)
            .or_else(|| run.context.deadlines.get(PumpDeadline::Decision))
            .or_else(|| run.context.deadlines.get(PumpDeadline::Enter))
            .unwrap()
            .deadline_ns();
        let now = run.clock.now().unwrap();
        run.clock.advance_to(end.max(now)).unwrap();
        run.context
            .advance_plan_summary_account(&mut run.clock)
            .unwrap();
        run.context
            .take_stopped_plan_summary()
            .unwrap()
            .expect("bounded failed account")
    }
    fn ordinary_read(&mut self, snapshot: Option<s::PlanSnapshot>) -> RecordedRead {
        let run = self.run();
        run.context
            .begin_read(s::ReadSelector::PlanCurrent {}, None, &mut run.clock)
            .unwrap();
        let request = self.boot.member.next_request();
        let raw = summary_bytes(snapshot);
        self.boot
            .member
            .respond(&request, page(71, 0, raw.len(), &raw));
        let release = self.boot.member.next_request();
        self.boot.member.respond(
            &release,
            s::StateControlAnswer::Released {
                snapshot_id: 71.into(),
            },
        );
        self.boot.member.finish().unwrap()
    }
}

fn summary_bytes(snapshot: Option<s::PlanSnapshot>) -> Vec<u8> {
    serde_json::to_vec_pretty(&s::QueryResult::PlanCurrent {
        current: snapshot,
        next_plan_id: 40.into(),
        next_step_id: 50.into(),
    })
    .unwrap()
}

fn effect(seed: &str, name: &str, turn: Option<&str>) -> AuthoredEvent {
    let plan = |revision: u64| t::PlanRef {
        plan_id: t::PlanId {
            seed_id: seed.into(),
            local_id: 3.into(),
        },
        revision: revision.into(),
    };
    let source = || t::SourceBlockId {
        run: "prior-run".into(),
        sequence: 1.into(),
        block: 0.into(),
    };
    let span = |id: u64| t::SpanRef {
        seed_id: seed.into(),
        local_id: id.into(),
        revision: 1.into(),
    };
    let task = || t::TaskScopeRef {
        task: span(1),
        prompt: span(2),
    };
    let review = || t::ReviewSetupRef {
        setup_id: "review".into(),
        sha256: "b".repeat(64),
    };
    let step = || t::StepId {
        seed_id: seed.into(),
        local_id: 2.into(),
    };
    let context = match name {
        "authorize" => t::PlanEffect::Authorize {
            plan: plan(1),
            source: source(),
            task: task(),
            review: review(),
        },
        "lock" => t::PlanEffect::Lock {
            plan: plan(1),
            source: t::PlanLockSource::Operator { input: source() },
            task: task(),
            steps: vec![],
            review: review(),
        },
        "replace" => t::PlanEffect::Replace {
            expected: plan(1),
            next: plan(2),
            source: source(),
            task: task(),
            steps: vec![],
            review: review(),
        },
        "activate" => t::PlanEffect::Activate {
            expected: plan(1),
            step_id: step(),
            step_span: span(4),
            group_span: None,
            successor: t::StepQueryRef {
                plan: plan(1),
                previous_step: None,
                step_id: step(),
                instruction: t::StatementRef {
                    block_id: source(),
                    binding: 0.into(),
                },
            },
        },
        "close" => t::PlanEffect::Close {
            expected: plan(1),
            step_id: step(),
            review_call: None,
            work: t::WorkRef {
                plan: plan(1),
                step_id: step(),
                span: span(4),
                through: t::SourcePosition {
                    block_id: source(),
                    turn: turn.map(str::to_owned),
                },
                content_sha256: "a".repeat(64),
            },
            outcome_span: span(5),
        },
        "break" => t::PlanEffect::Break {
            expected: plan(1),
            step_id: None,
            case: t::PlanBreakCase::WorkChanged,
            proposed_source: None,
        },
        _ => panic!("unlisted effect"),
    };
    let message = serde_json::value::to_raw_value(&Message {
        role: Role::User,
        content: vec![ContentBlock::Text {
            text: "Read plan.".into(),
        }],
    })
    .unwrap();
    let (kind, payload) = if let t::PlanEffect::Close {
        expected,
        step_id,
        review_call,
        work,
        ..
    } = &context
    {
        (
            Kind::StepOutcome,
            Payload::StepOutcome(Box::new(t::StepOutcomeRecord {
                message,
                template_id: "step.outcome".into(),
                plan: expected.clone(),
                step_id: step_id.clone(),
                review_call: review_call.clone(),
                work: work.clone(),
                outcome: t::ReviewOutcome::Closed {},
                context: Some(context),
            })),
        )
    } else {
        (
            Kind::PlanEvent,
            Payload::PlanEffect(Box::new(t::PlanEffectRecord {
                message,
                context,
                template_id: format!("plan.{name}"),
            })),
        )
    };
    AuthoredEvent {
        kind,
        subsystem: weaver_trace::Subsystem::Harness,
        turn: turn.map(|turn| weaver_trace::TurnRef(turn.into())),
        source: AuthoredSource::Loop,
        causal_parent: None,
        payload: Some(payload),
    }
}

fn committed_position(filing: &FilingCompletion) -> s::SourcePosition {
    assert!(filing.receipts().is_complete() && !filing.receipts().is_cancelled());
    let mut sources = filing.receipts().dispositions();
    let Some(SourceDisposition::Committed(source)) = sources.next() else {
        panic!("real committed source required");
    };
    assert!(sources.next().is_none());
    let LedgerFile::Statement(file) = source.file() else {
        panic!("statement required");
    };
    assert!(matches!(file.effect, Some(s::StatementEffect::Plan(_))));
    s::SourcePosition {
        block_id: file.block_id.clone(),
        turn: file.source.turn.clone(),
    }
}

#[test]
fn plan_summary_boot_consumes_actual_open_repair_and_records_populated_or_null_once() {
    for populated in [false, true] {
        let pairs = vec![old_pair("old-run")];
        let mut fx = SummaryFixture::new(pairs);
        assert!(fx.run().context.committed_plan_summary().is_err());
        let prior_recalls = fx.records(Kind::Recall).len();
        assert_eq!(fx.repaired.as_ref().unwrap().terminals().len(), 1);
        let snapshot = populated.then(|| fx.snapshot());
        let expected = snapshot.as_ref().map(|snapshot| s::PlanSummary {
            plan: snapshot.plan.clone(),
            status: snapshot.status,
            document_sha256: snapshot.document_sha256.clone(),
            last_source: snapshot.last_source.clone(),
        });
        fx.begin_boot();
        assert!(fx.run().context.committed_plan_summary().is_err());
        let raw = fx.release(snapshot);
        assert!(fx.run().context.committed_plan_summary().is_err());
        let PlanSummaryCompletion::Boot(proof) = fx.finish() else {
            panic!("boot proof required");
        };
        assert_eq!(proof.summary(), expected.as_ref());
        assert_eq!(proof.read().raw_bytes(), raw);
        assert_eq!(proof.repaired().terminals().len(), 1);
        assert_eq!(
            fx.run().context.committed_plan_summary().unwrap(),
            expected.as_ref()
        );
        assert_eq!(fx.records(Kind::Recall).len(), prior_recalls + 1);
        assert_eq!(
            fx.records(Kind::Recall).last().unwrap()["sequence"],
            proof.read().recall_sequence().0.to_string()
        );
        let run = fx.run();
        assert!(
            run.context
                .take_plan_summary(&mut run.clock)
                .unwrap()
                .is_none()
        );
        assert!(run.context.completed_author.is_none());
    }
}

#[test]
fn plan_summary_rejects_assumed_open_and_foreign_context_even_with_identical_ids() {
    let mut repair = RepairFixture::new(vec![]);
    repair.begin();
    let BootPairsCompletion::Repaired(assumed) = repair.complete() else {
        panic!();
    };
    let mut boot = repair.into_boot();
    let rejected = boot
        .member
        .run
        .context
        .begin_boot_plan_summary(assumed, &mut boot.member.run.clock)
        .unwrap_err();
    assert_eq!(rejected.prerequisite.terminals().len(), 0);
    assert!(boot.member.run.context.committed_plan_summary().is_err());

    let mut first = SummaryFixture::new(vec![]);
    let mut second = SummaryFixture::new(vec![]);
    let proof = first.repaired.take().unwrap();
    assert_eq!(
        proof.recheck().recall_sequence(),
        second
            .repaired
            .as_ref()
            .unwrap()
            .recheck()
            .recall_sequence()
    );
    let run = second.run();
    let rejected = run
        .context
        .begin_boot_plan_summary(proof, &mut run.clock)
        .unwrap_err();
    let run = first.run();
    run.context
        .begin_boot_plan_summary(rejected.prerequisite, &mut run.clock)
        .unwrap();
    first.release(None);
    assert!(matches!(first.finish(), PlanSummaryCompletion::Boot(_)));
}

#[test]
fn plan_summary_each_real_effect_refreshes_exact_source_and_replacement_digest() {
    for name in ["authorize", "lock", "replace", "activate", "close", "break"] {
        // The actual trace grammar is turnless for operator-driven effects;
        // model/step effects carry a turn. Nullable-source checks cover both.
        let turn = matches!(name, "activate" | "close" | "break").then_some("exact-effect-turn");
        {
            let mut fx = SummaryFixture::new(vec![]);
            let mut snapshot = fx.snapshot();
            fx.boot(Some(snapshot.clone()));
            let event = effect(fx.seed(), name, turn);
            let filing = fx.file(event, false);
            let source = committed_position(&filing);
            let filed_sequence = filing.sequence();
            assert_eq!(source.turn.as_deref(), turn);
            assert!(
                fx.run().context.committed_plan_summary().is_err(),
                "receipt requires refresh before handoff"
            );
            snapshot.last_source = source.clone();
            if name == "replace" {
                snapshot.plan.revision = 2.into();
                snapshot.document_sha256 = s::Digest::new("d".repeat(64)).unwrap();
            }
            fx.begin_effect(filing);
            let raw = fx.release(Some(snapshot.clone()));
            let PlanSummaryCompletion::Effect(proof) = fx.finish() else {
                panic!("{name}: effect proof required");
            };
            assert_eq!(proof.filing().sequence(), filed_sequence);
            assert_eq!(proof.read().raw_bytes(), raw);
            let summary = proof.summary().unwrap();
            assert_eq!(summary.plan, snapshot.plan);
            assert_eq!(summary.document_sha256, snapshot.document_sha256);
            assert_eq!(summary.last_source, source);
            assert_eq!(
                fx.run().context.committed_plan_summary().unwrap(),
                Some(summary)
            );
            assert!(fx.run().context.completed_author.is_none());
        }
    }
}

#[test]
fn plan_summary_foreign_real_filing_cannot_replace_same_ids_from_this_context() {
    let mut a = SummaryFixture::new(vec![]);
    let mut b = SummaryFixture::new(vec![]);
    let sa = a.snapshot();
    let mut sb = b.snapshot();
    a.boot(Some(sa));
    b.boot(Some(sb.clone()));
    let fa = a.file(effect(a.seed(), "authorize", None), false);
    let fb = b.file(effect(b.seed(), "authorize", None), false);
    assert_eq!(fa.sequence(), fb.sequence());
    assert_eq!(committed_position(&fa), committed_position(&fb));
    let run = b.run();
    let error = run
        .context
        .begin_effect_plan_summary(fa, &mut run.clock)
        .unwrap_err();
    assert_eq!(error.refusal, PlanSummaryBeginRefusal::WrongProof);
    assert_eq!(error.prerequisite.sequence(), fb.sequence());
    assert!(run.context.committed_plan_summary().is_err());
    sb.last_source = committed_position(&fb);
    b.begin_effect(fb);
    b.release(Some(sb));
    assert!(matches!(b.finish(), PlanSummaryCompletion::Effect(_)));
    // Refusal returns the foreign proof intact to its actual origin.
    let mut sa = a.snapshot();
    sa.last_source = committed_position(&error.prerequisite);
    a.begin_effect(error.prerequisite);
    a.release(Some(sa));
    assert!(matches!(a.finish(), PlanSummaryCompletion::Effect(_)));
}

#[test]
fn plan_summary_non_effect_empty_and_correctly_rejected_filings_cannot_create_refresh() {
    for case in ["ordinary", "empty", "rejected"] {
        let mut fx = SummaryFixture::new(vec![]);
        let snapshot = fx.snapshot();
        let previous = fx.boot(Some(snapshot)).summary().unwrap().clone();
        let event = if case == "rejected" {
            effect(fx.seed(), "authorize", None)
        } else {
            AuthoredEvent {
                kind: Kind::MessageAssistant,
                subsystem: Subsystem::Harness,
                turn: Some(TurnRef("ordinary".into())),
                source: AuthoredSource::Model,
                causal_parent: None,
                payload: Some(Payload::Message(
                    serde_json::value::to_raw_value(&Message {
                        role: Role::Assistant,
                        content: if case == "empty" {
                            vec![]
                        } else {
                            vec![ContentBlock::Text {
                                text: "Read plan.".into(),
                            }]
                        },
                    })
                    .unwrap(),
                )),
            }
        };
        let filing = fx.file(event, case == "rejected");
        assert!(filing.receipts().is_complete());
        assert_eq!(
            fx.run().context.committed_plan_summary().unwrap(),
            Some(&previous)
        );
        let sequence = filing.sequence();
        let run = fx.run();
        let error = run
            .context
            .begin_effect_plan_summary(filing, &mut run.clock)
            .unwrap_err();
        assert_eq!(error.refusal, PlanSummaryBeginRefusal::WrongProof);
        assert_eq!(error.prerequisite.sequence(), sequence);
        assert_eq!(
            run.context.committed_plan_summary().unwrap(),
            Some(&previous)
        );
        assert!(run.context.failure().is_none());
    }
}

#[test]
fn plan_summary_committed_receipt_blocks_cache_and_competing_work_before_handoff() {
    let mut fx = SummaryFixture::new(vec![]);
    let snapshot = fx.snapshot();
    let previous = fx.boot(Some(snapshot.clone())).summary().unwrap().clone();
    let filing = fx.file(effect(fx.seed(), "authorize", None), false);
    let source = committed_position(&filing);
    let run = fx.run();
    for _ in 0..3 {
        assert_eq!(
            run.context.committed_plan_summary(),
            Err(PlanSummaryUnavailable::Pending)
        );
        assert_eq!(run.context.previous_plan_summary(), Some(&previous));
        assert!(
            run.context
                .begin_read(s::ReadSelector::PlanCurrent {}, None, &mut run.clock)
                .is_err()
        );
        assert!(
            run.context
                .begin_inspection(InspectionKind::Shape, None, &mut run.clock)
                .is_err()
        );
        assert!(
            run.context
                .begin_filing(
                    effect("seed", "authorize", None),
                    PumpDeadline::Decision,
                    &mut run.clock
                )
                .is_err()
        );
        assert!(run.context.complete_wait(PumpDeadline::Decision).is_none());
    }
    // Holding the proof instead of immediately handing it off never exposes
    // the old cache as successful. The exact proof still starts its required read.
    fx.begin_effect(filing);
    let mut next = snapshot;
    next.last_source = source;
    fx.release(Some(next));
    assert!(matches!(fx.finish(), PlanSummaryCompletion::Effect(_)));
}

#[test]
fn plan_summary_internal_read_cannot_be_taken_cancelled_or_replaced_by_generic_owner() {
    let mut fx = SummaryFixture::new(vec![]);
    fx.begin_boot();
    let request = fx.next_request();
    let run = fx.run();
    assert_eq!(
        run.context.take_read(&mut run.clock).unwrap_err(),
        RunFailure::PlanSummaryOwned
    );
    assert_eq!(
        run.context.cancel_read(&mut run.clock).unwrap_err(),
        RunFailure::PlanSummaryOwned
    );
    assert_eq!(
        run.context
            .advance_read(&mut ports(&mut run.clock))
            .unwrap_err(),
        RunFailure::PlanSummaryOwned
    );
    assert!(
        run.context
            .begin_read(s::ReadSelector::PlanCurrent {}, None, &mut run.clock)
            .is_err()
    );
    let raw = summary_bytes(None);
    fx.boot
        .member
        .respond(&request, page(41, 0, raw.len(), &raw));
    let release = fx.next_request();
    fx.boot.member.respond(
        &release,
        s::StateControlAnswer::Released {
            snapshot_id: 41.into(),
        },
    );
    assert!(matches!(fx.finish(), PlanSummaryCompletion::Boot(_)));
}

#[test]
fn plan_summary_ordinary_inspection_never_replaces_committed_cache_and_next_owner_runs() {
    let mut fx = SummaryFixture::new(vec![]);
    let snapshot = fx.snapshot();
    let first = fx.boot(Some(snapshot.clone())).summary().unwrap().clone();
    let recall = fx.ordinary_read(None);
    assert!(matches!(
        recall.result(),
        s::QueryResult::PlanCurrent { current: None, .. }
    ));
    assert_eq!(
        fx.run().context.committed_plan_summary().unwrap(),
        Some(&first)
    );
    for _ in 0..2 {
        let filing = fx.file(effect(fx.seed(), "authorize", None), false);
        let mut next = snapshot.clone();
        next.last_source = committed_position(&filing);
        fx.begin_effect(filing);
        fx.release(Some(next));
        let PlanSummaryCompletion::Effect(proof) = fx.finish() else {
            panic!("refresh");
        };
        assert_eq!(
            fx.run().context.committed_plan_summary().unwrap(),
            proof.summary()
        );
        assert!(fx.run().context.completed_author.is_none());
        let _next_owner = fx.ordinary_read(None);
        assert_eq!(
            fx.run().context.committed_plan_summary().unwrap(),
            proof.summary()
        );
    }
}

#[test]
fn plan_summary_effect_rejects_each_independently_held_identity_and_digest_mismatch() {
    for case in [
        "null",
        "seed",
        "plan",
        "revision",
        "run",
        "sequence",
        "block",
        "null_turn",
        "some_turn",
        "digest",
    ] {
        let mut fx = SummaryFixture::new(vec![]);
        let mut snapshot = fx.snapshot();
        let previous = fx.boot(Some(snapshot.clone())).summary().unwrap().clone();
        let (name, turn) = if case == "null_turn" {
            ("activate", Some("actual-effect-turn"))
        } else {
            ("authorize", None)
        };
        let filing = fx.file(effect(fx.seed(), name, turn), false);
        let original_sequence = filing.sequence();
        snapshot.last_source = committed_position(&filing);
        match case {
            "seed" => {
                let foreign = if fx.seed() == "f".repeat(32) {
                    "e"
                } else {
                    "f"
                }
                .repeat(32);
                snapshot.plan.plan_id.seed_id = foreign.clone();
                snapshot.task.task.seed_id = foreign.clone();
                snapshot.task.prompt.seed_id = foreign;
            }
            "plan" => snapshot.plan.plan_id.local_id = 99.into(),
            "revision" => snapshot.plan.revision = 99.into(),
            "run" => snapshot.last_source.block_id.run = "foreign-run".into(),
            "sequence" => snapshot.last_source.block_id.sequence.0 += 1,
            "block" => snapshot.last_source.block_id.block = 1.into(),
            "null_turn" => snapshot.last_source.turn = None,
            "some_turn" => snapshot.last_source.turn = Some("invented-turn".into()),
            "digest" => snapshot.document_sha256 = s::Digest::new("f".repeat(64)).unwrap(),
            "null" => {}
            _ => unreachable!(),
        }
        fx.begin_effect(filing);
        let before = fx.records(Kind::Recall).len();
        let raw = fx.release((case != "null").then_some(snapshot));
        let first = fx.advance_failure();
        assert!(fx.run().context.failure().is_some(), "{case}");
        assert_eq!(
            fx.run().context.committed_plan_summary(),
            Err(PlanSummaryUnavailable::Failed),
            "{case}"
        );
        assert_eq!(fx.run().context.previous_plan_summary(), Some(&previous));
        // The untrusted result was truthfully Released/Recorded before semantic
        // comparison with the independently held receipt/cache expectations.
        assert_eq!(fx.records(Kind::Recall).len(), before + 1);
        let account = fx.halt_account();
        assert_eq!(account.filing().unwrap().sequence(), original_sequence);
        assert_eq!(account.recorded_read().unwrap().raw_bytes(), raw);
        assert_eq!(fx.run().context.failure(), Some(&first));
        assert!(fx.run().context.committed_plan_summary().is_err());
    }
}

#[test]
fn plan_summary_required_read_refusals_latch_and_preserve_committed_effect() {
    for code in [
        s::ReadRefusalCode::ResultTooLarge,
        s::ReadRefusalCode::CounterExhausted,
        s::ReadRefusalCode::ResultUnavailable,
        s::ReadRefusalCode::WrongSeed,
    ] {
        let mut fx = SummaryFixture::new(vec![]);
        let snapshot = fx.snapshot();
        let previous = fx.boot(Some(snapshot)).summary().unwrap().clone();
        let filing = fx.file(effect(fx.seed(), "authorize", None), false);
        let sequence = filing.sequence();
        fx.begin_effect(filing);
        let request = fx.next_request();
        fx.boot.member.respond(&request, refused(code));
        let first = fx.advance_failure();
        assert!(fx.run().context.failure().is_some());
        assert_eq!(
            fx.run().context.committed_plan_summary(),
            Err(PlanSummaryUnavailable::Failed)
        );
        assert_eq!(fx.run().context.previous_plan_summary(), Some(&previous));
        let account = fx.halt_account();
        assert_eq!(account.filing().unwrap().sequence(), sequence);
        assert_eq!(
            account.refusal(),
            Some(&crate::flow::read::ReadFailure::Refused {
                code,
                detail: "scripted read refusal".into(),
            })
        );
        assert!(account.recorded_read().is_none());
        assert_eq!(fx.run().context.failure(), Some(&first));
    }
}

fn hold_summary_leg(
    fx: &mut SummaryFixture,
    request: &s::StateFrame,
    answer: s::StateControlAnswer,
    hold: bool,
    bound: AbsoluteBound,
) {
    if hold {
        fx.boot.member.fill_trace();
    }
    fx.boot.member.respond(request, answer);
    if hold {
        let exact = fx
            .run()
            .context
            .flow
            .state()
            .held_measurement()
            .unwrap()
            .clone();
        for _ in 0..8 {
            assert_ne!(fx.advance().unwrap(), PlanSummaryProgress::Complete);
            assert!(fx.run().context.committed_plan_summary().is_err());
            assert_eq!(fx.boot.member.bound(), bound);
            assert_eq!(
                fx.run().context.flow.state().held_measurement(),
                Some(&exact)
            );
        }
        fx.drain();
    }
}

#[test]
fn plan_summary_every_page_release_and_recall_barrier_preserves_the_same_bound_and_bytes() {
    for pressure in 0..4 {
        let mut fx = SummaryFixture::new(vec![]);
        fx.begin_boot();
        let first = fx.next_request();
        let bound = fx.boot.member.bound();
        let raw = summary_bytes(Some(fx.snapshot()));
        let cut = raw.len() / 2;
        hold_summary_leg(
            &mut fx,
            &first,
            page(41, 0, raw.len(), &raw[..cut]),
            pressure == 0,
            bound,
        );
        let next = fx.next_request();
        assert!(
            matches!(&next, s::StateFrame::Control { frame: s::StateControl::Next {
            snapshot_id, offset }, .. } if snapshot_id.0 == 41 && offset.0 == cut as u64)
        );
        hold_summary_leg(
            &mut fx,
            &next,
            page(41, cut, raw.len(), &raw[cut..]),
            pressure == 1,
            bound,
        );
        let release = fx.next_request();
        assert!(matches!(
            &release,
            s::StateFrame::Control {
                frame: s::StateControl::Release { .. },
                ..
            }
        ));
        hold_summary_leg(
            &mut fx,
            &release,
            s::StateControlAnswer::Released {
                snapshot_id: 41.into(),
            },
            pressure == 2,
            bound,
        );
        if pressure == 3 {
            fx.boot.member.record_completion_only();
            let run = fx.run();
            run.context.settle_recorded_read(&mut run.clock).unwrap();
            assert!(run.context.read_blocks_reduction());
            fx.boot.member.fill_trace();
            let before = fx.records(Kind::Recall).len();
            assert_eq!(fx.advance().unwrap(), PlanSummaryProgress::Pending);
            let ticket = fx.run().context.pending_author().unwrap().ticket;
            for _ in 0..8 {
                assert_eq!(fx.advance().unwrap(), PlanSummaryProgress::Pending);
                assert_eq!(fx.run().context.pending_author().unwrap().ticket, ticket);
                assert_eq!(fx.boot.member.bound(), bound);
                assert_eq!(fx.records(Kind::Recall).len(), before);
                assert!(fx.run().context.committed_plan_summary().is_err());
            }
            fx.drain();
        }
        let PlanSummaryCompletion::Boot(proof) = fx.finish() else {
            panic!("summary");
        };
        assert_eq!(proof.read().raw_bytes(), raw);
        assert!(fx.run().context.completed_author.is_none());
        let _next_owner = fx.ordinary_read(None);
        assert_eq!(
            fx.run().context.committed_plan_summary().unwrap(),
            proof.summary()
        );
    }
}

#[test]
fn plan_summary_effect_read_inherits_original_decision_and_cannot_extend_or_retire_it() {
    let mut fx = SummaryFixture::new(vec![]);
    let snapshot = fx.snapshot();
    fx.boot(Some(snapshot));
    let filing = fx.file(effect(fx.seed(), "authorize", None), false);
    let bound = filing.wait_bound();
    let run = fx.run();
    run.clock.advance_to(bound.deadline_ns() - 100).unwrap();
    assert!(run.context.complete_wait(PumpDeadline::Decision).is_none());
    fx.begin_effect(filing);
    let _request = fx.next_request();
    assert_eq!(fx.boot.member.bound().deadline_ns(), bound.deadline_ns());
    assert!(
        fx.run()
            .context
            .complete_wait(PumpDeadline::Decision)
            .is_none()
    );
    let run = fx.run();
    run.clock.advance_to(bound.deadline_ns()).unwrap();
    let first = fx.advance_failure();
    assert!(fx.run().context.failure().is_some());
    assert_eq!(
        fx.run().context.committed_plan_summary(),
        Err(PlanSummaryUnavailable::Failed)
    );
    let account = fx.halt_account();
    assert!(account.filing().is_some());
    assert!(account.recorded_read().is_none());
    assert!(account.incomplete().is_some());
    assert_eq!(fx.run().context.failure(), Some(&first));
}

#[test]
fn plan_summary_effect_bound_removed_after_actual_filing_never_starts_a_new_read() {
    let mut fx = SummaryFixture::new(vec![]);
    let snapshot = fx.snapshot();
    fx.boot(Some(snapshot));
    let filing = fx.file(effect(fx.seed(), "authorize", None), false);
    let original = filing.wait_bound();
    let run = fx.run();
    // Model an erroneous internal reducer bypassing the guarded public retirement.
    assert_eq!(
        run.context.deadlines.disarm(PumpDeadline::Decision),
        Some(original)
    );
    let error = run
        .context
        .begin_effect_plan_summary(filing, &mut run.clock)
        .unwrap_err();
    assert_eq!(error.prerequisite.wait_bound(), original);
    assert!(run.context.committed_plan_summary().is_err());
    assert!(run.context.flow.state().outstanding().is_empty());
}

#[test]
fn plan_summary_cancelled_effect_drains_released_read_but_never_restores_previous_cache() {
    for after_release in [false, true] {
        let mut fx = SummaryFixture::new(vec![]);
        let mut snapshot = fx.snapshot();
        let previous = fx.boot(Some(snapshot.clone())).summary().unwrap().clone();
        let filing = fx.file(effect(fx.seed(), "authorize", None), false);
        snapshot.last_source = committed_position(&filing);
        let sequence = filing.sequence();
        fx.begin_effect(filing);
        let raw = if after_release {
            Some(fx.release(Some(snapshot)))
        } else {
            None
        };
        let run = fx.run();
        run.context.cancel_plan_summary(&mut run.clock).unwrap();
        assert!(run.context.committed_plan_summary().is_err());
        let PlanSummaryCompletion::Cancelled(account) = fx.finish() else {
            panic!("cancelled effect account");
        };
        assert_eq!(account.filing().unwrap().sequence(), sequence);
        assert_eq!(
            account.recorded_read().map(|read| read.raw_bytes()),
            raw.as_deref()
        );
        assert_eq!(fx.run().context.previous_plan_summary(), Some(&previous));
        assert_eq!(
            fx.run().context.committed_plan_summary(),
            Err(PlanSummaryUnavailable::Revoked)
        );
        let run = fx.run();
        assert!(
            run.context
                .begin_read(s::ReadSelector::PlanCurrent {}, None, &mut run.clock)
                .is_err()
        );
        assert!(
            run.context
                .begin_filing(
                    effect("seed", "authorize", None),
                    PumpDeadline::Decision,
                    &mut run.clock
                )
                .is_err()
        );
        assert!(run.context.completed_author.is_none());
    }
}

#[test]
fn plan_summary_cancelled_plan_filing_keeps_its_late_commit_subscription_and_evidence() {
    let mut fx = SummaryFixture::new(vec![]);
    let snapshot = fx.snapshot();
    let previous = fx.boot(Some(snapshot)).summary().unwrap().clone();
    let before = fx.records(Kind::PlanEvent).len();
    fx.begin_file(effect(fx.seed(), "authorize", None));
    for _ in 0..32 {
        let run = fx.run();
        run.context
            .advance_filing(&mut ports(&mut run.clock))
            .unwrap();
        if fx.records(Kind::PlanEvent).len() == before + 1 {
            break;
        }
    }
    assert_eq!(fx.records(Kind::PlanEvent).len(), before + 1);
    assert!(fx.files.is_empty(), "member has not committed the plan yet");
    fx.run().context.cancel_filing();
    assert!(fx.run().context.take_cancelled_filing().is_none());
    assert!(fx.run().context.flow.has_filing_consumer());
    assert!(
        fx.run()
            .context
            .complete_wait(PumpDeadline::Decision)
            .is_none()
    );
    let account = (0..256)
        .find_map(|_| {
            let run = fx.run();
            run.context
                .advance_filing(&mut ports(&mut run.clock))
                .unwrap();
            fx.classifier(false);
            fx.file_reply();
            assert!(
                fx.run()
                    .context
                    .complete_wait(PumpDeadline::Decision)
                    .is_none()
            );
            fx.run().context.take_cancelled_filing()
        })
        .expect("cancelled author still owes its late actual filing");
    assert_eq!(fx.files.len(), 1);
    let receipts = account.receipts.as_ref().unwrap();
    assert!(receipts.is_cancelled());
    let commits: Vec<_> = receipts
        .recorded_dispositions()
        .filter_map(|item| {
            if let SourceDisposition::Committed(source) = item {
                Some(source)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0].source(), &fx.files[0].block_id);
    assert!(
        matches!(commits[0].file(), LedgerFile::Statement(file) if matches!(file.effect, Some(s::StatementEffect::Plan(_))))
    );
    assert_eq!(fx.run().context.previous_plan_summary(), Some(&previous));
    assert_eq!(
        fx.run().context.committed_plan_summary(),
        Err(PlanSummaryUnavailable::Revoked)
    );
    let run = fx.run();
    assert!(
        run.context
            .begin_filing(
                effect("seed", "authorize", None),
                PumpDeadline::Decision,
                &mut run.clock
            )
            .is_err()
    );
    assert!(
        run.context
            .begin_read(s::ReadSelector::PlanCurrent {}, None, &mut run.clock)
            .is_err()
    );
}

fn finish_summary_without_take(fx: &mut SummaryFixture) {
    for _ in 0..128 {
        if fx.advance().unwrap() == PlanSummaryProgress::Complete {
            return;
        }
    }
    panic!("summary did not reach its retained completion");
}
fn rollback_boot_summary(fx: &mut SummaryFixture) -> PlanSummaryAccount {
    let run = fx.run();
    assert_eq!(
        run.context.take_plan_summary(&mut run.clock).unwrap_err(),
        RunFailure::PlanSummaryOwned
    );
    let leave = AbsoluteBound::after(run.clock.now().unwrap(), 1000).unwrap();
    run.context
        .begin_boot_plan_summary_release(leave, &mut run.clock)
        .unwrap();
    assert_eq!(run.context.phase(), RecordingPhase::Releasing);
    assert!(run.context.failure().is_none());
    let account = run.context.take_stopped_plan_summary().unwrap().unwrap();
    assert!(run.context.committed_plan_summary().is_err());
    assert!(account.repaired().unwrap().checked_open().is_some());
    account
}

#[test]
fn plan_summary_boot_refusal_stays_distinct_from_null_and_retains_rollback_until_unload() {
    for code in [
        s::ReadRefusalCode::ResultTooLarge,
        s::ReadRefusalCode::CounterExhausted,
        s::ReadRefusalCode::ResultUnavailable,
    ] {
        let mut fx = SummaryFixture::new(vec![]);
        let before = fx.records(Kind::Recall).len();
        fx.begin_boot();
        let request = fx.next_request();
        fx.boot.member.respond(&request, refused(code));
        finish_summary_without_take(&mut fx);
        assert!(fx.run().context.committed_plan_summary().is_err());
        assert_eq!(fx.records(Kind::Recall).len(), before);
        let account = rollback_boot_summary(&mut fx);
        assert_eq!(
            account.refusal(),
            Some(&crate::flow::read::ReadFailure::Refused {
                code,
                detail: "scripted read refusal".into(),
            })
        );
        assert!(account.recorded_read().is_none());
        assert!(account.incomplete().is_none());
        let (decode, _peer) = fx._decode.take().unwrap();
        assert!(fx.run().context.retire_boot_decode(decode).is_ok());
        let run = fx.run();
        // Rollback already retained and retired the state channel.
        run.context.retire_classifier(&mut run.clock).unwrap();
        let sequence = run
            .context
            .author_and_wait(&mut ports(&mut run.clock), unload())
            .unwrap();
        assert!(sequence.0 > 0);
        assert!(run.context.barrier.is_closed());
        assert_eq!(fx.records(Kind::Unload).len(), 1);
    }
}

#[test]
fn plan_summary_boot_cancellation_before_request_or_after_release_never_grants_cache() {
    for after_release in [false, true] {
        let mut fx = SummaryFixture::new(vec![]);
        fx.begin_boot();
        let raw = if after_release {
            Some(fx.release(Some(fx.snapshot())))
        } else {
            None
        };
        let run = fx.run();
        run.context.cancel_plan_summary(&mut run.clock).unwrap();
        finish_summary_without_take(&mut fx);
        assert!(fx.run().context.committed_plan_summary().is_err());
        let account = rollback_boot_summary(&mut fx);
        assert_eq!(
            account.recorded_read().map(|read| read.raw_bytes()),
            raw.as_deref()
        );
        assert!(account.refusal().is_none());
        assert!(account.incomplete().is_none());
    }
}

#[test]
fn plan_summary_boot_expired_partial_snapshot_uses_original_bound_for_incomplete_rollback() {
    let mut fx = SummaryFixture::new(vec![]);
    fx.begin_boot();
    let request = fx.next_request();
    let original = fx.boot.member.bound();
    assert_eq!(
        original.deadline_ns(),
        fx.run()
            .context
            .deadlines
            .get(PumpDeadline::Enter)
            .unwrap()
            .deadline_ns()
    );
    let raw = summary_bytes(Some(fx.snapshot()));
    let cut = raw.len() / 2;
    fx.boot
        .member
        .respond(&request, page(41, 0, raw.len(), &raw[..cut]));
    let next = fx.next_request();
    assert!(matches!(
        next,
        s::StateFrame::Control {
            frame: s::StateControl::Next { .. },
            ..
        }
    ));
    let run = fx.run();
    let early = AbsoluteBound::after(run.clock.now().unwrap(), 1000).unwrap();
    assert_eq!(
        run.context
            .begin_boot_plan_summary_release(early, &mut run.clock),
        Err(RunFailure::PlanSummaryOwned)
    );
    assert_eq!(run.context.phase(), RecordingPhase::Entered);
    assert!(run.context.deadlines.get(PumpDeadline::Leave).is_none());
    run.clock.advance_to(original.deadline_ns()).unwrap();
    let leave = AbsoluteBound::after(original.deadline_ns(), 1000).unwrap();
    run.context
        .begin_boot_plan_summary_release(leave, &mut run.clock)
        .unwrap();
    let account = run.context.take_stopped_plan_summary().unwrap().unwrap();
    assert!(account.incomplete().is_some());
    let read = account.read_evidence().unwrap();
    assert_eq!(read.assembled_bytes(), &raw[..cut]);
    assert_eq!(read.acquired_snapshot(), Some(41.into()));
    assert!(account.recorded_read().is_none());
    assert!(run.context.failure().is_none());
    assert_eq!(run.context.phase(), RecordingPhase::Releasing);
    assert!(run.context.committed_plan_summary().is_err());
}

struct SummaryBoundaryClock {
    before: u64,
    after: u64,
    boundary: usize,
    samples: usize,
    broken: bool,
}
impl PumpIo for SummaryBoundaryClock {
    fn now(&mut self) -> io::Result<u64> {
        self.samples += 1;
        if self.samples >= self.boundary {
            if self.broken {
                Err(io::Error::other("summary post-Recorded clock fixture"))
            } else {
                Ok(self.after)
            }
        } else {
            Ok(self.before)
        }
    }
    fn wait(&mut self, _: &[WaitInterest<'_>], _: Option<u64>) -> io::Result<WaitOutcome> {
        panic!("released recall author does not wait for a socket");
    }
}

fn released_summary_recall(fx: &mut SummaryFixture, snapshot: Option<s::PlanSnapshot>) -> Vec<u8> {
    let raw = fx.release(snapshot);
    fx.boot.member.record_completion_only();
    let run = fx.run();
    run.context.settle_recorded_read(&mut run.clock).unwrap();
    assert!(run.context.read_blocks_reduction());
    raw
}

#[test]
fn plan_summary_recorded_hook_survives_recoverable_wait_without_duplicate_or_stranded_author() {
    let mut witnessed = false;
    for boundary in 1..=32 {
        let mut fx = SummaryFixture::new(vec![]);
        fx.begin_boot();
        let snapshot = fx.snapshot();
        let raw = released_summary_recall(&mut fx, Some(snapshot));
        let before_records = fx.records(Kind::Recall).len();
        let run = fx.run();
        let before = run.clock.now().unwrap();
        let label = AbsoluteBound::after(before, 10).unwrap();
        run.context
            .arm(PumpDeadline::Label, label, &mut run.clock)
            .unwrap();
        let mut clock = SummaryBoundaryClock {
            before,
            after: label.deadline_ns(),
            boundary,
            samples: 0,
            broken: false,
        };
        let result = run.context.advance_plan_summary(&mut ports(&mut clock));
        if result.is_ok() || fx.records(Kind::Recall).len() != before_records + 1 {
            continue;
        }
        assert_eq!(
            result.unwrap_err(),
            RunFailure::Wait {
                key: PumpDeadline::Label,
                bound: label
            }
        );
        witnessed = true;
        assert!(fx.run().context.committed_plan_summary().is_err());
        assert!(fx.run().context.failure().is_none());
        let sequence = fx.records(Kind::Recall).last().unwrap()["sequence"].clone();
        let run = fx.run();
        assert_eq!(run.context.complete_wait(PumpDeadline::Label), Some(label));
        run.clock.advance_to(label.deadline_ns()).unwrap();
        let PlanSummaryCompletion::Boot(proof) = fx.finish() else {
            panic!("resumed boot proof");
        };
        assert_eq!(proof.read().raw_bytes(), raw);
        assert_eq!(sequence, proof.read().recall_sequence().0.to_string());
        assert_eq!(fx.records(Kind::Recall).len(), before_records + 1);
        assert!(fx.run().context.completed_author.is_none());
        let _next_owner = fx.ordinary_read(None);
        assert_eq!(
            fx.run().context.committed_plan_summary().unwrap(),
            proof.summary()
        );
        break;
    }
    assert!(
        witnessed,
        "must witness an actual Recorded recall before the recoverable wait"
    );
}

#[test]
fn plan_summary_post_record_clock_failure_retains_raw_read_and_committed_effect() {
    for effect_mode in [false, true] {
        let mut witnessed = false;
        for boundary in 1..=32 {
            let mut fx = SummaryFixture::new(vec![]);
            let mut snapshot = fx.snapshot();
            let filing_sequence = if effect_mode {
                fx.boot(Some(snapshot.clone()));
                let filing = fx.file(effect(fx.seed(), "authorize", None), false);
                snapshot.last_source = committed_position(&filing);
                let sequence = filing.sequence();
                fx.begin_effect(filing);
                Some(sequence)
            } else {
                fx.begin_boot();
                None
            };
            let raw = released_summary_recall(&mut fx, Some(snapshot));
            let before_records = fx.records(Kind::Recall).len();
            let run = fx.run();
            let before = run.clock.now().unwrap();
            let mut clock = SummaryBoundaryClock {
                before,
                after: before,
                boundary,
                samples: 0,
                broken: true,
            };
            let result = run.context.advance_plan_summary(&mut ports(&mut clock));
            if result.is_ok() || fx.records(Kind::Recall).len() != before_records + 1 {
                continue;
            }
            witnessed = true;
            let first = result.unwrap_err();
            assert!(fx.run().context.failure().is_some());
            assert_eq!(
                fx.run().context.committed_plan_summary(),
                Err(PlanSummaryUnavailable::Failed)
            );
            // The injected persistent clock failure also prevents the final
            // flow sample. Do not invent a frozen halt account or fault record:
            // retain the already Recorded read until its original bound ends.
            let run = fx.run();
            assert!(matches!(
                run.context.freeze_halt_account(ExternalFacts {
                    outstanding_pairs: vec![],
                    capture: None,
                    wait: run.context.halt_wait().cloned(),
                    input_id: None,
                }),
                Err(HaltAccountFailure::Snapshot(
                    crate::flow::halt_snapshot::SnapshotFailure {
                        error: crate::flow::report::ReportError::Invalid(
                            "final flow sample failed"
                        ),
                        ..
                    }
                ))
            ));
            assert_eq!(
                run.context.begin_halt_report(),
                Err(HaltReportRefusal::MissingAccount)
            );
            let account = fx.finish_halt_account();
            assert!(account.incomplete().is_some());
            assert_eq!(
                account.filing().map(FilingCompletion::sequence),
                filing_sequence
            );
            assert_eq!(account.recorded_read().unwrap().raw_bytes(), raw);
            assert_eq!(fx.records(Kind::Recall).len(), before_records + 1);
            assert_eq!(fx.run().context.failure(), Some(&first));
            assert!(fx.run().context.completed_author.is_none());
            break;
        }
        assert!(
            witnessed,
            "must fail the actual post-Recorded clock boundary"
        );
    }
}

#[test]
fn plan_summary_malformed_member_result_preserves_complete_bad_bytes_and_committed_receipt() {
    for effect_mode in [false, true] {
        let mut fx = SummaryFixture::new(vec![]);
        let filing_sequence = if effect_mode {
            let snapshot = fx.snapshot();
            fx.boot(Some(snapshot));
            let filing = fx.file(effect(fx.seed(), "authorize", None), false);
            let sequence = filing.sequence();
            fx.begin_effect(filing);
            Some(sequence)
        } else {
            fx.begin_boot();
            None
        };
        let request = fx.next_request();
        let raw = br#"{"plan_current":{"current":null,"current":null,"next_plan_id":"40","next_step_id":"50"}}"#;
        fx.boot
            .member
            .respond(&request, page(41, 0, raw.len(), raw));
        let first = fx.advance_failure();
        assert_eq!(
            fx.run().context.committed_plan_summary(),
            Err(PlanSummaryUnavailable::Failed)
        );
        let account = fx.halt_account();
        assert_eq!(
            account.filing().map(FilingCompletion::sequence),
            filing_sequence
        );
        let read = account.read_evidence().unwrap();
        assert_eq!(read.assembled_bytes(), raw);
        assert_eq!(read.acquired_snapshot(), Some(41.into()));
        assert!(read.rejected_answer().is_some());
        assert!(account.recorded_read().is_none());
        assert!(account.incomplete().is_some());
        assert_eq!(fx.run().context.failure(), Some(&first));
    }
}

#[test]
fn plan_summary_lost_state_seam_retains_partial_snapshot_and_never_restores_cache() {
    let mut fx = SummaryFixture::new(vec![]);
    let snapshot = fx.snapshot();
    let previous = fx.boot(Some(snapshot.clone())).summary().unwrap().clone();
    let filing = fx.file(effect(fx.seed(), "authorize", None), false);
    let sequence = filing.sequence();
    fx.begin_effect(filing);
    let request = fx.next_request();
    let raw = summary_bytes(Some(snapshot));
    let cut = raw.len() / 2;
    fx.boot
        .member
        .respond(&request, page(41, 0, raw.len(), &raw[..cut]));
    let _next = fx.next_request();
    fx.run().state.shutdown(Shutdown::Both).unwrap();
    let now = fx.boot.member.now() + 1;
    let first = match fx
        .boot
        .member
        .pump_at(now, PollFlags::POLLHUP | PollFlags::POLLIN)
    {
        Err(failure) => failure,
        Ok(()) => fx.advance_failure(),
    };
    assert_eq!(fx.run().context.previous_plan_summary(), Some(&previous));
    assert_eq!(
        fx.run().context.committed_plan_summary(),
        Err(PlanSummaryUnavailable::Failed)
    );
    let account = fx.halt_account();
    assert_eq!(account.filing().unwrap().sequence(), sequence);
    assert_eq!(
        account.read_evidence().unwrap().assembled_bytes(),
        &raw[..cut]
    );
    assert!(account.incomplete().is_some());
    assert_eq!(fx.run().context.failure(), Some(&first));
}

#[test]
fn plan_summary_actual_held_stop_revokes_boot_without_stealing_lifecycle_completion() {
    let mut fx = SummaryFixture::new(vec![]);
    fx.begin_boot();
    let snapshot = fx.snapshot();
    let raw = released_summary_recall(&mut fx, Some(snapshot));
    let (admin, peer) = OrganChannel::pair().unwrap();
    let mut admin = LifecyclePeer::from_channel(admin).unwrap();
    let stop = directive(weaver_types::LifecycleDirective::Stop {
        cause: weaver_types::Cause { uid: 0 },
    });
    send_bytes(peer.as_fd(), &serde_json::to_vec(&stop).unwrap());
    let run = fx.run();
    run.context
        .pump_once(&mut PumpPorts {
            lifecycle: LifecyclePorts {
                listener: None,
                peer: Some(&mut admin),
                residency: None,
            },
            gate: None,
            decode: None,
            label: None,
            io: &mut run.clock,
        })
        .unwrap();
    assert_eq!(admin.take_completion().unwrap().unwrap().envelope, stop);
    assert!(run.context.committed_plan_summary().is_err());
    finish_summary_without_take(&mut fx);
    let account = rollback_boot_summary(&mut fx);
    assert_eq!(account.recorded_read().unwrap().raw_bytes(), raw);
    assert!(account.incomplete().is_none());
    assert!(fx.run().context.committed_plan_summary().is_err());
}

#[test]
fn plan_summary_pending_recall_after_failure_resumes_exact_ticket_without_live_admission() {
    let mut fx = SummaryFixture::new(vec![]);
    let mut snapshot = fx.snapshot();
    fx.boot(Some(snapshot.clone()));
    let filing = fx.file(effect(fx.seed(), "authorize", None), false);
    snapshot.last_source = committed_position(&filing);
    let sequence = filing.sequence();
    fx.begin_effect(filing);
    let raw = released_summary_recall(&mut fx, Some(snapshot));
    let before = fx.records(Kind::Recall).len();
    fx.boot.member.fill_trace();
    assert_eq!(fx.advance().unwrap(), PlanSummaryProgress::Pending);
    let ticket = fx.run().context.pending_author().unwrap().ticket;
    assert_eq!(
        fx.run().context.pending_author().unwrap().event.kind,
        Kind::Recall
    );
    let run = fx.run();
    let first = run.context.fail(
        &mut run.clock,
        FlowHaltReason::ProtocolFailed,
        HaltPoint::Barrier,
        "independent first failure during exact summary recall pressure",
    );
    assert_eq!(run.context.pending_author().unwrap().ticket, ticket);
    fx.drain();
    let account = fx.halt_account();
    assert_eq!(account.filing().unwrap().sequence(), sequence);
    assert_eq!(account.recorded_read().unwrap().raw_bytes(), raw);
    assert!(account.incomplete().is_none());
    assert_eq!(fx.records(Kind::Recall).len(), before + 1);
    assert!(fx.run().context.pending_author().is_none());
    assert_eq!(fx.run().context.failure(), Some(&first));
    assert_eq!(
        fx.run().context.committed_plan_summary(),
        Err(PlanSummaryUnavailable::Failed)
    );
}

const _: fn() = || {
    trait NoFabricatedSummary<M> {
        fn pin() {}
    }
    impl<T: ?Sized> NoFabricatedSummary<()> for T {}
    impl<T: Clone> NoFabricatedSummary<u8> for T {}
    impl<T: Default> NoFabricatedSummary<u16> for T {}
    impl<T: From<s::PlanSummary>> NoFabricatedSummary<u32> for T {}
    impl<T: From<u64>> NoFabricatedSummary<u64> for T {}
    impl<T: From<Sequence>> NoFabricatedSummary<u128> for T {}
    let _ = <BootPlanSummary as NoFabricatedSummary<_>>::pin;
    let _ = <EffectPlanSummary as NoFabricatedSummary<_>>::pin;
    let _ = <FilingCompletion as NoFabricatedSummary<_>>::pin;
};

#[test]
fn plan_summary_actual_effect_receipt_cannot_retire_wait_before_completion_extraction() {
    let mut fx = SummaryFixture::new(vec![]);
    let mut snapshot = fx.snapshot();
    fx.boot(Some(snapshot.clone()));
    fx.begin_file(effect(fx.seed(), "authorize", None));
    let mut complete = false;
    for _ in 0..256 {
        let run = fx.run();
        let progress = run
            .context
            .advance_filing(&mut ports(&mut run.clock))
            .unwrap();
        fx.classifier(false);
        fx.file_reply();
        if progress == FilingProgress::Complete {
            complete = true;
            break;
        }
    }
    assert!(complete);
    // Do this before take_filing or the public cache getter can observe it.
    assert!(
        fx.run()
            .context
            .complete_wait(PumpDeadline::Decision)
            .is_none()
    );
    let run = fx.run();
    let filing = run.context.take_filing(&mut run.clock).unwrap().unwrap();
    snapshot.last_source = committed_position(&filing);
    fx.begin_effect(filing);
    fx.release(Some(snapshot));
    assert!(matches!(fx.finish(), PlanSummaryCompletion::Effect(_)));
}

#[test]
fn plan_summary_required_post_record_wait_latches_without_overwriting_the_prior_cache() {
    let mut witnessed = false;
    for boundary in 1..=32 {
        let mut fx = SummaryFixture::new(vec![]);
        let mut snapshot = fx.snapshot();
        let previous = fx.boot(Some(snapshot.clone())).summary().unwrap().clone();
        let filing = fx.file(effect(fx.seed(), "authorize", None), false);
        snapshot.last_source = committed_position(&filing);
        let sequence = filing.sequence();
        fx.begin_effect(filing);
        let raw = released_summary_recall(&mut fx, Some(snapshot));
        let records = fx.records(Kind::Recall).len();
        let run = fx.run();
        let before = run.clock.now().unwrap();
        let leave = AbsoluteBound::after(before, 10).unwrap();
        run.context
            .arm(PumpDeadline::Leave, leave, &mut run.clock)
            .unwrap();
        let mut clock = SummaryBoundaryClock {
            before,
            after: leave.deadline_ns(),
            boundary,
            samples: 0,
            broken: false,
        };
        let result = run.context.advance_plan_summary(&mut ports(&mut clock));
        if result.is_ok() || fx.records(Kind::Recall).len() != records + 1 {
            continue;
        }
        witnessed = true;
        assert!(fx.run().context.failure().is_some());
        assert_eq!(fx.run().context.previous_plan_summary(), Some(&previous));
        assert_eq!(
            fx.run().context.committed_plan_summary(),
            Err(PlanSummaryUnavailable::Failed)
        );
        fx.run().clock.advance_to(leave.deadline_ns()).unwrap();
        let account = fx.halt_account();
        assert_eq!(account.filing().unwrap().sequence(), sequence);
        assert_eq!(account.recorded_read().unwrap().raw_bytes(), raw);
        assert_eq!(fx.records(Kind::Recall).len(), records + 1);
        break;
    }
    assert!(
        witnessed,
        "must witness actual Recorded before required refresh wait failure"
    );
}

#[test]
fn plan_summary_recall_cannot_resume_or_bind_a_foreign_valid_ticket() {
    let mut fx = SummaryFixture::new(vec![]);
    fx.begin_boot();
    let snapshot = fx.snapshot();
    released_summary_recall(&mut fx, Some(snapshot));
    let before = fx.records(Kind::Recall).len();
    fx.boot.member.fill_trace();
    assert_eq!(fx.advance().unwrap(), PlanSummaryProgress::Pending);
    let run = fx.run();
    let original = run.context.pending_author().unwrap();
    let ticket = original.ticket;
    // Keep the otherwise valid event intact; only the actual newly minted
    // recorder ticket differs. Invalid payloads would mask the custody check.
    let event = original.event.clone();
    assert!(run.context.barrier.abandon_boot_author(ticket));
    assert!(matches!(
        run.context.barrier.submit_authored_with_clock(
            event,
            RecordPurpose::Observation,
            &run.context.author,
            1,
            &mut run.context.inbox,
            &mut run.context.budget,
            &mut || run.clock.now()
        ),
        RecordProgress::Pending
    ));
    let foreign = run.context.pending_author().unwrap().ticket;
    assert_ne!(foreign, ticket);
    assert!(fx.advance().is_err());
    assert_eq!(fx.run().context.pending_author().unwrap().ticket, foreign);
    assert!(fx.run().context.committed_plan_summary().is_err());
    assert_eq!(fx.records(Kind::Recall).len(), before);
    fx.drain();
}

fn pump_summary_decode(
    fx: &mut SummaryFixture,
    decode: &mut DecodePort,
) -> Result<PumpStep, RunFailure> {
    let run = fx.run();
    let mut io = SummaryReadyClock(&mut run.clock);
    run.context.pump_once(&mut PumpPorts {
        lifecycle: LifecyclePorts::default(),
        gate: None,
        decode: Some(decode),
        label: None,
        io: &mut io,
    })
}
fn finish_existing_filing(fx: &mut SummaryFixture) -> FilingCompletion {
    for _ in 0..256 {
        let run = fx.run();
        let progress = run
            .context
            .advance_filing(&mut ports(&mut run.clock))
            .unwrap();
        fx.classifier(false);
        fx.file_reply();
        if progress == FilingProgress::Complete {
            let run = fx.run();
            return run.context.take_filing(&mut run.clock).unwrap().unwrap();
        }
    }
    panic!("actual existing filing did not complete");
}

#[test]
fn plan_summary_common_pump_holds_actual_generation_until_each_required_refresh_completes() {
    for phase in ["boot", "filing", "required", "refresh"] {
        let mut fx = SummaryFixture::new(vec![]);
        let mut snapshot = fx.snapshot();
        let mut held_filing = None;
        if phase == "boot" {
            fx.begin_boot();
        } else {
            fx.boot(Some(snapshot.clone()));
            if phase == "filing" {
                fx.begin_file(effect(fx.seed(), "authorize", None));
                assert!(fx.files.is_empty());
                assert_eq!(
                    fx.run().context.committed_plan_summary(),
                    Err(PlanSummaryUnavailable::Pending)
                );
            } else {
                let filing = fx.file(effect(fx.seed(), "authorize", None), false);
                snapshot.last_source = committed_position(&filing);
                if phase == "refresh" {
                    fx.begin_effect(filing);
                } else {
                    held_filing = Some(filing);
                }
            }
        }
        let (mut decode, peer) = fx._decode.take().unwrap();
        let append = TokenDirective::AppendAndGenerate {
            turn: weaver_types::TurnKey("next-round".into()),
            delta: vec![],
            tool_schemas: vec![],
        };
        assert!(matches!(
            decode.stage(append.clone()).unwrap(),
            Stage::Queued
        ));
        for _ in 0..2 {
            pump_summary_decode(&mut fx, &mut decode).unwrap();
            assert!(decode.generation_not_started());
            assert!(receive(peer.as_fd()).is_none());
        }
        if phase == "filing" {
            let filing = finish_existing_filing(&mut fx);
            snapshot.last_source = committed_position(&filing);
            fx.begin_effect(filing);
        } else if let Some(filing) = held_filing {
            fx.begin_effect(filing);
        }
        fx.release(Some(snapshot));
        assert!(matches!(
            fx.finish(),
            PlanSummaryCompletion::Boot(_) | PlanSummaryCompletion::Effect(_)
        ));
        let mut packet = None;
        for _ in 0..16 {
            pump_summary_decode(&mut fx, &mut decode).unwrap();
            packet = receive(peer.as_fd());
            if packet.is_some() {
                break;
            }
        }
        assert_eq!(
            serde_json::from_slice::<TokenDirective>(
                &packet.expect("committed cache allows staged send")
            )
            .unwrap(),
            append
        );
    }
}

#[test]
fn plan_summary_revoked_cache_blocks_generation_but_allows_actual_cancel_transport() {
    for cancel in [false, true] {
        let mut fx = SummaryFixture::new(vec![]);
        let snapshot = fx.snapshot();
        fx.boot(Some(snapshot));
        let filing = fx.file(effect(fx.seed(), "authorize", None), false);
        fx.begin_effect(filing);
        let run = fx.run();
        run.context.cancel_plan_summary(&mut run.clock).unwrap();
        assert!(matches!(fx.finish(), PlanSummaryCompletion::Cancelled(_)));
        assert_eq!(
            fx.run().context.committed_plan_summary(),
            Err(PlanSummaryUnavailable::Revoked)
        );
        let (mut decode, peer) = fx._decode.take().unwrap();
        let turn = weaver_types::TurnKey("revoked-round".into());
        let request = if cancel {
            TokenDirective::Cancel { turn }
        } else {
            TokenDirective::AppendAndGenerate {
                turn,
                delta: vec![],
                tool_schemas: vec![],
            }
        };
        assert!(matches!(
            decode.stage(request.clone()).unwrap(),
            Stage::Queued
        ));
        if cancel {
            let mut packet = None;
            for _ in 0..16 {
                pump_summary_decode(&mut fx, &mut decode).unwrap();
                packet = receive(peer.as_fd());
                if packet.is_some() {
                    break;
                }
            }
            assert_eq!(
                serde_json::from_slice::<TokenDirective>(&packet.expect("cancel still sends"))
                    .unwrap(),
                request
            );
        } else {
            pump_summary_decode(&mut fx, &mut decode).unwrap();
            assert!(receive(peer.as_fd()).is_none());
        }
    }
}

#[test]
fn plan_summary_boot_rollback_cannot_reclassify_an_earlier_read_timeout_as_enter_expiry() {
    let mut fx = SummaryFixture::with_enter(5_000_000_000);
    fx.begin_boot();
    let _request = fx.next_request();
    let read = fx.boot.member.bound();
    let run = fx.run();
    let enter = run.context.deadlines.get(PumpDeadline::Enter).unwrap();
    assert!(read.deadline_ns() < enter.deadline_ns());
    run.clock.advance_to(read.deadline_ns()).unwrap();
    let leave = AbsoluteBound::after(read.deadline_ns(), 1000).unwrap();
    let first = run
        .context
        .begin_boot_plan_summary_release(leave, &mut run.clock)
        .unwrap_err();
    assert!(matches!(
        first,
        RunFailure::Flow {
            reason: FlowHaltReason::ControlBound,
            ..
        }
    ));
    assert_eq!(run.context.failure(), Some(&first));
    assert_eq!(run.context.phase(), RecordingPhase::Failed);
    assert!(run.context.deadlines.get(PumpDeadline::Leave).is_none());
    assert!(run.context.take_stopped_plan_summary().unwrap().is_none());
    let account = fx.halt_account();
    assert!(account.repaired().unwrap().checked_open().is_some());
    assert!(account.incomplete().is_some());
    assert!(account.recorded_read().is_none());
    assert_eq!(fx.run().context.failure(), Some(&first));
    assert_eq!(
        fx.run().context.committed_plan_summary(),
        Err(PlanSummaryUnavailable::Failed)
    );
}

#[test]
fn plan_summary_boot_rollback_seal_cannot_retire_a_foreign_recall_ticket() {
    let mut fx = SummaryFixture::new(vec![]);
    fx.begin_boot();
    let snapshot = fx.snapshot();
    released_summary_recall(&mut fx, Some(snapshot));
    fx.boot.member.fill_trace();
    assert_eq!(fx.advance().unwrap(), PlanSummaryProgress::Pending);
    let run = fx.run();
    let held = run.context.pending_author().unwrap();
    let ticket = held.ticket;
    let event = held.event.clone();
    assert!(run.context.barrier.abandon_boot_author(ticket));
    assert!(matches!(
        run.context.barrier.submit_authored_with_clock(
            event,
            RecordPurpose::Observation,
            &run.context.author,
            1,
            &mut run.context.inbox,
            &mut run.context.budget,
            &mut || run.clock.now()
        ),
        RecordProgress::Pending
    ));
    let foreign = run.context.pending_author().unwrap().ticket;
    assert_ne!(foreign, ticket);
    let enter = run.context.deadlines.get(PumpDeadline::Enter).unwrap();
    run.clock.advance_to(enter.deadline_ns()).unwrap();
    let leave = AbsoluteBound::after(enter.deadline_ns(), 1000).unwrap();
    assert_eq!(
        run.context
            .begin_boot_plan_summary_release(leave, &mut run.clock),
        Err(RunFailure::PlanSummaryOwned)
    );
    assert_eq!(run.context.phase(), RecordingPhase::Entered);
    assert!(run.context.failure().is_none());
    assert!(run.context.deadlines.get(PumpDeadline::Leave).is_none());
    assert_eq!(run.context.pending_author().unwrap().ticket, foreign);
    assert!(run.context.take_stopped_plan_summary().is_err());
    fx.drain();
}

#[path = "context_release_integration_tests.rs"]
mod release_integration_tests;

// Source-only review followups use these same actual owner fixtures.
#[path = "context_plan_summary_review_tests.rs"]
mod review_followup_tests;
use review_followup_tests::SummaryReadyClock;
