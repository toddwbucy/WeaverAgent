//! Real load, template classifier, state stream and recorder ownership fixtures.

use super::super::template_tests::TemplateFixture;
use super::*;
use weaver_traits::{ContentBlock, Message, Role};

const PROMPT: &str = "Read caf\u{e9}.";

const _: fn() = || {
    trait MustNotClone<Marker> {
        fn check() {}
    }
    impl<T: ?Sized> MustNotClone<()> for T {}
    impl<T: Clone> MustNotClone<u8> for T {}
    let _ = <BootHead as MustNotClone<_>>::check;
    trait MustNotFromMessage<Marker> {
        fn check() {}
    }
    impl<T: ?Sized> MustNotFromMessage<()> for T {}
    impl<T: From<Message>> MustNotFromMessage<u8> for T {}
    let _ = <BootHead as MustNotFromMessage<_>>::check;
    trait MustNotDefault<Marker> {
        fn check() {}
    }
    impl<T: ?Sized> MustNotDefault<()> for T {}
    impl<T: Default> MustNotDefault<u8> for T {}
    let _ = <BootHead as MustNotDefault<_>>::check;
};

struct BootFixture {
    member: Member,
    _classifier: crate::flow::ste100_channel::Channel,
}

impl BootFixture {
    fn new(certified: bool) -> Self {
        let mut templates = TemplateFixture::new();
        if certified {
            templates.begin();
            for index in 0..3 {
                templates.accept(index);
            }
            assert_eq!(templates.advance().unwrap(), TemplateProgress::Complete);
        }
        Self::from_templates(templates)
    }

    fn from_templates(templates: TemplateFixture) -> Self {
        Self {
            member: Member {
                run: templates.input.run,
                next_operation: 2,
                requests: Vec::new(),
            },
            _classifier: templates.input.peer,
        }
    }

    fn entry(&self) -> s::ReadEntry {
        let seed = self
            .member
            .run
            .context
            .open_start
            .as_ref()
            .unwrap()
            .seed_id
            .clone();
        s::ReadEntry {
            source: s::ReadSource::Seed(s::SeedSource {
                seed_id: seed.clone(),
                ordinal: 0.into(),
            }),
            raw_block: serde_json::value::to_raw_value(&serde_json::json!({
                "type":"text", "text": PROMPT,
            }))
            .unwrap(),
            text: PROMPT.into(),
            bindings: vec![s::ReadBinding {
                ordinal: 0.into(),
                value: s::Binding {
                    statement_span: span(0, PROMPT.len() as u64),
                    verb: "read".into(),
                    verb_span: span(0, 4),
                    condition: None,
                    condition_span: None,
                    nouns: vec![s::NounBinding {
                        value: "caf\u{e9}".into(),
                        span: span(5, 10),
                    }],
                },
            }],
            selected_span: Some(s::SelectedSpan {
                span_ref: s::SpanRef {
                    seed_id: seed,
                    local_id: 0.into(),
                    revision: 1.into(),
                },
                start: 0.into(),
                end: (PROMPT.len() as u64).into(),
            }),
            step_query: None,
            pairs: vec![],
        }
    }

    fn begin(&mut self) -> s::StateFrame {
        self.member
            .run
            .context
            .begin_boot_read(&mut self.member.run.clock)
            .unwrap();
        self.member.next_request()
    }

    fn finish(&mut self) -> BootReadCompletion {
        for _ in 0..128 {
            self.member.advance().unwrap();
            if let Some(result) = self
                .member
                .run
                .context
                .finish_boot_read(&mut self.member.run.clock)
                .unwrap()
            {
                return result;
            }
        }
        panic!("boot read never reached its final owner");
    }

    fn released(&mut self, request: &s::StateFrame, entry: s::ReadEntry) {
        let raw = bytes(entry);
        self.member.respond(request, page(7, 0, raw.len(), &raw));
        let release = self.member.next_request();
        self.member.respond(
            &release,
            s::StateControlAnswer::Released {
                snapshot_id: 7.into(),
            },
        );
        self.member.record_completion_only();
        self.member
            .run
            .context
            .settle_recorded_read(&mut self.member.run.clock)
            .unwrap();
        assert!(self.member.run.context.read_blocks_reduction());
        assert!(self.member.run.context.boot_head.is_none());
        assert!(
            self.member
                .run
                .context
                .finish_boot_read(&mut self.member.run.clock)
                .unwrap()
                .is_none()
        );
    }
}

fn span(start: u64, end: u64) -> s::Span {
    s::Span {
        start: start.into(),
        end: end.into(),
    }
}
fn bytes(entry: s::ReadEntry) -> Vec<u8> {
    serde_json::to_vec(&s::QueryResult::Entries {
        entries: vec![entry],
    })
    .unwrap()
}
fn boot_payload(member: &Member) -> serde_json::Value {
    let records = member.records(Kind::Recall);
    let [record] = records.as_slice() else {
        panic!("one boot recall")
    };
    assert_eq!(record["channel"], "internal");
    assert_eq!(record["origin"], "runtime");
    assert_eq!(record["subsystem"], "harness");
    assert!(record.get("turn").is_none_or(serde_json::Value::is_null));
    assert!(
        record
            .get("causal_parent")
            .is_none_or(serde_json::Value::is_null)
    );
    assert!(record["payload"].get("read").is_none());
    record["payload"]["boot"].clone()
}

#[test]
fn boot_read_seats_exact_recorded_seed_head_once_and_later_prompt_is_an_ordinary_read() {
    let mut fx = BootFixture::new(true);
    // Taking the certificate for later instructions must not erase completion.
    assert!(
        fx.member
            .run
            .context
            .take_template_certificate(&mut fx.member.run.clock)
            .unwrap()
            .is_some()
    );
    let entry = fx.entry();
    let selected = entry.selected_span.clone().unwrap();
    let raw = bytes(entry);
    let request = fx.begin();
    assert!(matches!(
        request,
        s::StateFrame::Control {
            frame: s::StateControl::Read {
                selector: s::ReadSelector::SystemPrompt {}
            },
            ..
        }
    ));
    let bound = fx.member.bound();
    let enter = fx
        .member
        .run
        .context
        .deadlines
        .get(PumpDeadline::Enter)
        .unwrap();
    assert_eq!(bound.deadline_ns(), enter.deadline_ns());
    assert_eq!(
        fx.member.run.context.complete_wait(PumpDeadline::Enter),
        None
    );
    assert_eq!(
        fx.member
            .run
            .context
            .take_read(&mut fx.member.run.clock)
            .unwrap_err(),
        RunFailure::ReadOwned
    );
    let middle = raw.len() / 2;
    fx.member
        .respond(&request, page(7, 0, raw.len(), &raw[..middle]));
    let next = fx.member.next_request();
    assert!(
        matches!(&next, s::StateFrame::Control { frame: s::StateControl::Next { offset, .. }, .. } if offset.0 == middle as u64)
    );
    assert!(fx.member.run.context.boot_head.is_none());
    fx.member
        .respond(&next, page(7, middle, raw.len(), &raw[middle..]));
    let release = fx.member.next_request();
    assert!(fx.member.records(Kind::Recall).is_empty());
    fx.member.respond(
        &release,
        s::StateControlAnswer::Released {
            snapshot_id: 7.into(),
        },
    );
    fx.member.record_completion_only();
    fx.member
        .run
        .context
        .settle_recorded_read(&mut fx.member.run.clock)
        .unwrap();
    assert_eq!(
        fx.member
            .run
            .context
            .reduce_flow(&mut fx.member.run.clock)
            .unwrap(),
        Advance::Pending
    );
    assert!(fx.member.run.context.boot_head.is_none());
    assert_eq!(
        fx.member
            .run
            .context
            .complete_wait(PumpDeadline::StateControl),
        None
    );
    assert_eq!(fx.member.advance().unwrap(), ReadProgress::Complete);
    assert!(fx.member.run.context.boot_head.is_none());
    assert_eq!(
        fx.member.run.context.complete_wait(PumpDeadline::Enter),
        None
    );
    assert_eq!(
        fx.member
            .run
            .context
            .take_read(&mut fx.member.run.clock)
            .unwrap_err(),
        RunFailure::ReadOwned
    );
    let payload = boot_payload(&fx.member);
    let expected_message =
        serde_json::json!({"role":"system", "content":[{"type":"text", "text":PROMPT}]});
    assert_eq!(payload["message"], expected_message);
    assert_eq!(payload["selector"], serde_json::json!({"system_prompt":{}}));
    assert_eq!(payload["snapshot_id"], "7");
    assert_eq!(
        payload["end_mark"],
        serde_json::json!({"span":selected.span_ref, "end":PROMPT.len().to_string()})
    );
    assert_eq!(
        payload["sources"],
        serde_json::json!([{"source":{"seed":{"seed_id":selected.span_ref.seed_id,"ordinal":"0"}},"binding_ordinals":["0"]}])
    );
    assert_eq!(
        payload["spans"],
        serde_json::json!([{"source":payload["sources"][0]["source"],"selected_span":selected}])
    );
    assert!(matches!(fx.finish(), BootReadCompletion::Seated));
    let head = fx
        .member
        .run
        .context
        .boot_head(&mut fx.member.run.clock)
        .unwrap()
        .unwrap();
    let sequence = head.recall_sequence();
    assert_eq!(head.message().role, Role::System);
    assert_eq!(
        head.message().content,
        vec![ContentBlock::Text {
            text: PROMPT.into()
        }]
    );
    assert_eq!(head.span(), &selected.span_ref);
    assert_eq!(head.end().0, PROMPT.len() as u64);
    assert_ne!(head.end().0, PROMPT.chars().count() as u64);
    assert_eq!(head.snapshot_id().0, 7);
    assert_eq!(head.enter_bound(), enter);
    assert_eq!(
        fx.member.records(Kind::Recall)[0]["sequence"],
        sequence.0.to_string()
    );
    assert!(
        fx.member
            .run
            .context
            .boot_head
            .as_ref()
            .unwrap()
            .belongs_to(&fx.member.run.context)
    );
    let other = Member::new();
    assert!(
        !fx.member
            .run
            .context
            .boot_head
            .as_ref()
            .unwrap()
            .belongs_to(&other.run.context)
    );
    assert!(
        fx.member
            .run
            .context
            .finish_boot_read(&mut fx.member.run.clock)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        fx.member
            .run
            .context
            .begin_boot_read(&mut fx.member.run.clock)
            .unwrap_err()
            .refusal,
        BootReadBeginRefusal::AlreadyStarted
    );
    assert!(fx.member.run.context.flow.ledger().facts().next().is_none());
    assert_eq!(fx.member.requests.len(), 3);
    fx.member.no_request();

    fx.member
        .run
        .context
        .begin_read(
            s::ReadSelector::SystemPrompt {},
            None,
            &mut fx.member.run.clock,
        )
        .unwrap();
    let ordinary = fx.member.next_request();
    fx.member.respond(&ordinary, page(8, 0, raw.len(), &raw));
    let release = fx.member.next_request();
    fx.member.respond(
        &release,
        s::StateControlAnswer::Released {
            snapshot_id: 8.into(),
        },
    );
    let read = fx.member.finish().unwrap();
    assert_eq!(read.snapshot_id().0, 8);
    let records = fx.member.records(Kind::Recall);
    assert_eq!(records.len(), 2);
    assert!(records[1]["payload"].get("read").is_some());
    assert_eq!(
        fx.member
            .run
            .context
            .boot_head
            .as_ref()
            .unwrap()
            .recall_sequence(),
        sequence
    );
}

#[test]
fn boot_admission_requires_positive_template_completion_and_preserves_busy_inputs() {
    let mut fx = BootFixture::new(false);
    assert!(!fx.member.run.context.templates_block_content());
    let error = fx
        .member
        .run
        .context
        .begin_boot_read(&mut fx.member.run.clock)
        .unwrap_err();
    assert_eq!(error.refusal, BootReadBeginRefusal::Templates);
    assert_eq!(error.failure, Some(RunFailure::TemplateBlocked));
    assert!(!fx.member.run.context.boot_read_started);
    assert!(!fx.member.run.context.content_read_started);
    assert!(fx.member.run.context.read.is_none());
    fx.member.no_request();

    let mut templates = TemplateFixture::new();
    templates.begin();
    templates.accept(0);
    let mut fx = BootFixture::from_templates(templates);
    let error = fx
        .member
        .run
        .context
        .begin_boot_read(&mut fx.member.run.clock)
        .unwrap_err();
    assert_eq!(error.refusal, BootReadBeginRefusal::Templates);
    assert_eq!(error.failure, Some(RunFailure::TemplateBlocked));
    assert!(!fx.member.run.context.content_read_started);
    assert!(fx.member.run.context.flow.state().outstanding().is_empty());
    fx.member.no_request();

    let mut fx = BootFixture::new(true);
    let original = fx
        .member
        .run
        .context
        .complete_wait(PumpDeadline::Enter)
        .unwrap();
    let now = fx.member.now();
    let changed = AbsoluteBound::after(now, 1000).unwrap();
    assert_ne!(changed, original);
    fx.member
        .run
        .context
        .arm(PumpDeadline::Enter, changed, &mut fx.member.run.clock)
        .unwrap();
    let error = fx
        .member
        .run
        .context
        .begin_boot_read(&mut fx.member.run.clock)
        .unwrap_err();
    assert_eq!(error.refusal, BootReadBeginRefusal::Templates);
    assert_eq!(error.failure, Some(RunFailure::TemplateBlocked));
    assert!(!fx.member.run.context.content_read_started);
    fx.member.no_request();

    let mut fx = BootFixture::new(true);
    let now = fx.member.now();
    let gate = AbsoluteBound::after(now, 1000).unwrap();
    fx.member
        .run
        .context
        .arm(PumpDeadline::Gate, gate, &mut fx.member.run.clock)
        .unwrap();
    let error = fx
        .member
        .run
        .context
        .begin_boot_read(&mut fx.member.run.clock)
        .unwrap_err();
    assert_eq!(error.refusal, BootReadBeginRefusal::Busy);
    assert!(error.failure.is_none());
    assert!(!fx.member.run.context.boot_read_started);
    assert!(!fx.member.run.context.content_read_started);
    assert!(fx.member.run.context.read.is_none());
    assert!(fx.member.run.context.flow.state().outstanding().is_empty());
    assert_eq!(
        fx.member.run.context.complete_wait(PumpDeadline::Gate),
        Some(gate)
    );
    let request = fx.begin();
    assert_eq!(request.common().request_id.0, 2);
}

#[test]
fn boot_admission_distinguishes_prior_content_from_metadata_reads() {
    for metadata in [false, true] {
        let mut fx = BootFixture::new(true);
        let (selector, raw) = if metadata {
            (
                s::ReadSelector::PlanCurrent {},
                serde_json::to_vec(&s::QueryResult::PlanCurrent {
                    current: None,
                    next_plan_id: 1.into(),
                    next_step_id: 1.into(),
                })
                .unwrap(),
            )
        } else {
            (selector(), EMPTY.to_vec())
        };
        fx.member
            .run
            .context
            .begin_read(selector, None, &mut fx.member.run.clock)
            .unwrap();
        let request = fx.member.next_request();
        fx.member.respond(&request, page(6, 0, raw.len(), &raw));
        let release = fx.member.next_request();
        fx.member.respond(
            &release,
            s::StateControlAnswer::Released {
                snapshot_id: 6.into(),
            },
        );
        let _read = fx.member.finish().unwrap();
        let before = fx.member.requests.len();
        if metadata {
            let request = fx.begin();
            assert_eq!(request.common().request_id.0, 4);
        } else {
            assert_eq!(
                fx.member
                    .run
                    .context
                    .begin_boot_read(&mut fx.member.run.clock)
                    .unwrap_err()
                    .refusal,
                BootReadBeginRefusal::PriorContent
            );
            assert_eq!(fx.member.requests.len(), before);
            fx.member.no_request();
        }
    }
}

#[test]
fn boot_checks_loaded_seed_and_forbids_step_context_before_recall_authoring() {
    for extra_step in [false, true] {
        let mut fx = BootFixture::new(true);
        let mut entry = fx.entry();
        if extra_step {
            let seed = entry
                .selected_span
                .as_ref()
                .unwrap()
                .span_ref
                .seed_id
                .clone();
            entry.step_query = Some(s::StepQueryRef {
                plan: s::PlanRef {
                    plan_id: s::PlanId {
                        seed_id: seed.clone(),
                        local_id: 1.into(),
                    },
                    revision: 1.into(),
                },
                previous_step: None,
                step_id: s::StepId {
                    seed_id: seed,
                    local_id: 1.into(),
                },
                instruction: s::StatementRef {
                    block_id: s::SourceBlockId {
                        run: "old-run".into(),
                        sequence: 5.into(),
                        block: 0.into(),
                    },
                    binding: 0.into(),
                },
            });
        } else {
            let wrong = "f".repeat(32);
            assert_ne!(
                entry.selected_span.as_ref().unwrap().span_ref.seed_id,
                wrong
            );
            let s::ReadSource::Seed(source) = &mut entry.source else {
                unreachable!()
            };
            source.seed_id = wrong.clone();
            entry.selected_span.as_mut().unwrap().span_ref.seed_id = wrong;
        }
        let request = fx.begin();
        let raw = bytes(entry);
        fx.member.respond(&request, page(7, 0, raw.len(), &raw));
        let release = fx.member.next_request();
        let answer = fx.member.answer(
            &release,
            s::StateControlAnswer::Released {
                snapshot_id: 7.into(),
            },
        );
        let failure = match fx.member.receive(&answer) {
            Err(failure) => failure,
            Ok(()) => (0..128)
                .find_map(|_| fx.member.advance().err())
                .expect("invalid boot provenance must fail after Released"),
        };
        assert!(matches!(
            failure,
            RunFailure::Flow {
                reason: FlowHaltReason::ProtocolFailed,
                point: HaltPoint::StateReply,
                ..
            }
        ));
        // The exact invalid result stays owned, but Released has already
        // discharged the member cursor. Failure must not invent CarryFailed
        // by closing this completed read's state channel.
        fx.member.no_request();
        assert!(
            !fx.member
                .run
                .context
                .read
                .as_ref()
                .unwrap()
                .needs_failed_close()
        );
        assert!(fx.member.records(Kind::Recall).is_empty());
        assert!(fx.member.run.context.boot_head.is_none());
    }
}

#[test]
fn boot_refusal_remains_distinct_and_does_not_restart_the_mandatory_read() {
    for cancel in [false, true] {
        let mut fx = BootFixture::new(true);
        let request = fx.begin();
        if cancel {
            fx.member
                .run
                .context
                .cancel_read(&mut fx.member.run.clock)
                .unwrap();
        }
        fx.member
            .respond(&request, refused(s::ReadRefusalCode::ResultTooLarge));
        let result = fx.finish();
        let refusal = match &result {
            BootReadCompletion::Refused(refusal) if !cancel => refusal,
            BootReadCompletion::Cancelled(cancelled) if cancel => {
                assert!(cancelled.recall_sequence().is_none());
                cancelled.refusal().unwrap()
            }
            _ => panic!("typed refusal lost its cancellation disposition"),
        };
        assert!(matches!(
            refusal,
            crate::flow::read::ReadFailure::Refused {
                code: s::ReadRefusalCode::ResultTooLarge,
                ..
            }
        ));
        assert!(fx.member.records(Kind::Recall).is_empty());
        assert!(fx.member.run.context.boot_head.is_none());
        assert_eq!(
            fx.member
                .run
                .context
                .begin_boot_read(&mut fx.member.run.clock)
                .unwrap_err()
                .refusal,
            BootReadBeginRefusal::AlreadyStarted
        );
        assert_eq!(fx.member.requests.len(), 1);
        fx.member.no_request();
    }
}

#[test]
fn boot_cancellation_drains_sent_released_and_recorded_owners_without_seating() {
    for stage in 0..3 {
        let mut fx = BootFixture::new(true);
        let entry = fx.entry();
        let request = fx.begin();
        if stage == 0 {
            fx.member
                .run
                .context
                .cancel_read(&mut fx.member.run.clock)
                .unwrap();
        }
        fx.released(&request, entry);
        if stage == 2 {
            assert_eq!(fx.member.advance().unwrap(), ReadProgress::Complete);
        }
        fx.member
            .run
            .context
            .cancel_read(&mut fx.member.run.clock)
            .unwrap();
        let BootReadCompletion::Cancelled(cancelled) = fx.finish() else {
            panic!("cancelled boot")
        };
        let sequence = cancelled.recall_sequence().unwrap();
        assert!(cancelled.refusal().is_none());
        assert_eq!(
            fx.member.records(Kind::Recall)[0]["sequence"],
            sequence.0.to_string()
        );
        assert_eq!(
            boot_payload(&fx.member)["message"]["content"][0]["text"],
            PROMPT
        );
        assert!(fx.member.run.context.boot_head.is_none());
        assert!(fx.member.run.context.read.is_none());
        assert_eq!(fx.member.requests.len(), 2);
        fx.member.no_request();
    }
}

#[test]
fn boot_recall_pressure_retains_exact_ticket_and_cancellation_through_retry() {
    let mut fx = BootFixture::new(true);
    let entry = fx.entry();
    let request = fx.begin();
    fx.released(&request, entry);
    let bound = fx.member.bound();
    fx.member.fill_trace();
    assert_eq!(fx.member.advance().unwrap(), ReadProgress::Pending);
    let pending = fx.member.run.context.pending_author().unwrap();
    let ticket = pending.ticket;
    let payload = serde_json::to_string(&pending.event.payload).unwrap();
    for _ in 0..3 {
        assert_eq!(fx.member.advance().unwrap(), ReadProgress::Pending);
        let pending = fx.member.run.context.pending_author().unwrap();
        assert_eq!(pending.ticket, ticket);
        assert_eq!(
            serde_json::to_string(&pending.event.payload).unwrap(),
            payload
        );
        assert!(fx.member.run.context.boot_head.is_none());
        assert_eq!(fx.member.bound(), bound);
    }
    fx.member
        .run
        .context
        .cancel_read(&mut fx.member.run.clock)
        .unwrap();
    recall_tests::drain_admitted(&mut fx.member);
    assert!(matches!(fx.finish(), BootReadCompletion::Cancelled(_)));
    assert!(fx.member.run.context.boot_head.is_none());
    assert_eq!(fx.member.records(Kind::Recall).len(), 1);
    assert_eq!(fx.member.requests.len(), 2);
    fx.member.no_request();
}

struct JumpClock {
    before: u64,
    expired: u64,
    remaining: usize,
}
impl PumpIo for JumpClock {
    fn now(&mut self) -> std::io::Result<u64> {
        if self.remaining == 0 {
            Ok(self.expired)
        } else {
            self.remaining -= 1;
            Ok(self.before)
        }
    }
    fn wait(&mut self, _: &[WaitInterest<'_>], _: Option<u64>) -> std::io::Result<WaitOutcome> {
        panic!("boot owner performs bounded local progress");
    }
}

#[test]
fn boot_first_request_ownership_survives_post_enqueue_leave() {
    let mut fx = BootFixture::new(true);
    let now = fx.member.now();
    let leave = AbsoluteBound::after(now, 10).unwrap();
    fx.member
        .run
        .context
        .arm(PumpDeadline::Leave, leave, &mut fx.member.run.clock)
        .unwrap();
    // Boot admission, shared admission, queue admission and actual enqueue.
    let mut clock = JumpClock {
        before: now,
        expired: leave.deadline_ns(),
        remaining: 4,
    };
    let error = fx
        .member
        .run
        .context
        .begin_boot_read(&mut clock)
        .unwrap_err();
    assert_eq!(
        error.failure,
        Some(RunFailure::Wait {
            key: PumpDeadline::Leave,
            bound: leave
        })
    );
    assert!(fx.member.run.context.boot_read_started);
    assert!(fx.member.run.context.content_read_started);
    assert_eq!(fx.member.run.context.flow.state().outstanding().len(), 1);
    assert_eq!(
        fx.member.run.context.complete_wait(PumpDeadline::Leave),
        Some(leave)
    );
    fx.member.run.clock.advance_to(leave.deadline_ns()).unwrap();
    // This read inherited the already armed Leave deadline. Handling Leave
    // cannot extend StateControl or turn an enqueued owner into a new attempt.
    let error = fx
        .member
        .run
        .context
        .begin_boot_read(&mut fx.member.run.clock)
        .unwrap_err();
    assert_eq!(error.refusal, BootReadBeginRefusal::WrongPhase);
    assert!(matches!(
        error.failure,
        Some(RunFailure::Flow {
            reason: FlowHaltReason::ControlBound,
            ..
        })
    ));
    assert!(fx.member.run.context.boot_read_started);
    assert!(fx.member.run.context.content_read_started);
    assert!(fx.member.run.context.read.is_some());
    assert!(fx.member.run.context.boot_head.is_none());
    assert!(fx.member.requests.is_empty());
}

#[test]
fn boot_preserves_released_and_recorded_continuations_across_leave() {
    for remaining in [1, 4] {
        let mut fx = BootFixture::new(true);
        let entry = fx.entry();
        let request = fx.begin();
        fx.released(&request, entry);
        let enter = fx
            .member
            .run
            .context
            .deadlines
            .get(PumpDeadline::Enter)
            .unwrap();
        let now = fx.member.now();
        let leave = AbsoluteBound::after(now, 10).unwrap();
        fx.member
            .run
            .context
            .arm(PumpDeadline::Leave, leave, &mut fx.member.run.clock)
            .unwrap();
        let mut clock = JumpClock {
            before: now,
            expired: leave.deadline_ns(),
            remaining,
        };
        let expected = RunFailure::Wait {
            key: PumpDeadline::Leave,
            bound: leave,
        };
        assert_eq!(
            fx.member
                .run
                .context
                .advance_read(&mut ports(&mut clock))
                .unwrap_err(),
            expected
        );
        assert_eq!(
            fx.member
                .run
                .context
                .finish_boot_read(&mut clock)
                .unwrap_err(),
            expected
        );
        assert!(fx.member.run.context.boot_head.is_none());
        assert_eq!(
            fx.member.records(Kind::Recall).len(),
            usize::from(remaining == 4)
        );
        assert_eq!(
            fx.member.run.context.complete_wait(PumpDeadline::Enter),
            None
        );
        assert_eq!(
            fx.member.run.context.deadlines.get(PumpDeadline::Enter),
            Some(enter)
        );
        assert_eq!(
            fx.member.run.context.complete_wait(PumpDeadline::Leave),
            Some(leave)
        );
        fx.member.run.clock.advance_to(leave.deadline_ns()).unwrap();
        assert!(matches!(fx.finish(), BootReadCompletion::Seated));
        assert_eq!(fx.member.records(Kind::Recall).len(), 1);
        assert_eq!(fx.member.requests.len(), 2);
        fx.member.no_request();
    }
}

#[test]
fn boot_original_enter_expiry_retains_the_read_and_never_seats_a_head() {
    let mut fx = BootFixture::new(true);
    let entry = fx.entry();
    let request = fx.begin();
    fx.released(&request, entry);
    let enter = fx
        .member
        .run
        .context
        .deadlines
        .get(PumpDeadline::Enter)
        .unwrap();
    fx.member.run.clock.advance_to(enter.deadline_ns()).unwrap();
    assert_eq!(
        fx.member.advance().unwrap_err(),
        RunFailure::Wait {
            key: PumpDeadline::Enter,
            bound: enter
        }
    );
    assert!(
        fx.member
            .run
            .context
            .finish_boot_read(&mut fx.member.run.clock)
            .is_err()
    );
    assert_eq!(
        fx.member.run.context.complete_wait(PumpDeadline::Enter),
        None
    );
    assert!(fx.member.run.context.boot_head.is_none());
    assert!(fx.member.run.context.read.is_some());
    assert!(fx.member.records(Kind::Recall).is_empty());
}

#[test]
fn boot_recall_state_eof_keeps_the_pending_author_and_withholds_boot_head() {
    let mut boot = BootFixture::new(true);
    let request = boot.begin();
    let entry = boot.entry();
    boot.released(&request, entry);
    boot.member.fill_trace();
    assert_eq!(boot.member.advance().unwrap(), ReadProgress::Pending);
    let bound = boot.member.bound();
    let ticket = boot.member.run.context.pending_author().unwrap().ticket;
    boot.member.run.state.shutdown(Shutdown::Both).unwrap();
    let now = boot.member.now() + 1;
    let failure = boot.member.pump_at(now, PollFlags::POLLIN).unwrap_err();
    assert!(matches!(
        failure,
        RunFailure::Flow {
            reason: FlowHaltReason::CarryFailed,
            point: HaltPoint::StateReply,
            ..
        }
    ));
    assert!(boot.member.now() < bound.deadline_ns());
    assert!(boot.member.run.context.boot_head.is_none());
    assert_eq!(
        boot.member
            .run
            .context
            .finish_boot_read(&mut boot.member.run.clock)
            .unwrap_err(),
        failure
    );
    assert_eq!(
        boot.member.run.context.pending_author().unwrap().ticket,
        ticket
    );
    assert_eq!(boot.member.bound(), bound);
    assert_eq!(boot.member.requests.len(), 2);
    assert!(boot.member.records(Kind::Recall).is_empty());
}
