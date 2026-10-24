//! Actual template/input packets, member framing and recorder barriers.
//! Scripted members/checkers prove ownership mechanics, not production qualification.
use super::input_tests::InputFixture;
use super::*;
use crate::flow::coordinator::{TransportSource, tests::operation};
use crate::flow::input::AcceptedInput;
use crate::flow::pump::LifecyclePorts;
use crate::flow::ste100_channel::{Receive, Tag};
use crate::flow::templates::{CompiledInventory, TemplateCertificate};
use marks_driver::TaskRefusal;
use std::io::{Read, Write};
use std::sync::OnceLock;
use weaver_trace::TurnRef;
use weaver_types::{TurnFrame, state_flow as s};

#[path = "context_task_material_tests.rs"]
mod task_material;

#[test]
fn author_review_exempt_marks_author_cannot_replace_a_held_original() {
    let mut fx = TaskFixture::new();
    fx.reject_mark = true;
    fx.begin();
    for _ in 0..512 {
        fx.step().unwrap();
        if marks_driver::validation_tests::fault_author_is_next(&fx.input.run.context) {
            break;
        }
    }
    assert!(marks_driver::validation_tests::fault_author_is_next(
        &fx.input.run.context
    ));

    // Preserve a genuine read owner from a second task. Ordinary admission
    // excludes this overlap; move it only to exercise the defensive competing
    // owner branch during the real marks fault continuation. No read is driven
    // or treated as evidence in the other context.
    let mut other = TaskFixture::new();
    other.begin();
    for _ in 0..512 {
        other.step().unwrap();
        if other.input.run.context.read.is_some() {
            break;
        }
    }
    let read = other
        .input
        .run
        .context
        .read
        .take()
        .expect("actual admitted read");
    let original = tests::event(
        Kind::Fault,
        Some(Payload::Fault(
            serde_json::value::to_raw_value(&serde_json::json!({"earlier":"caller"})).unwrap(),
        )),
    );
    let expected = format!("{original:?}");
    let Some(Payload::Fault(raw)) = original.payload.as_ref() else {
        unreachable!()
    };
    let pointer = raw.get().as_ptr();
    let run = &mut fx.input.run;
    let failure = run
        .context
        .author_and_wait(&mut ports(&mut run.clock), original)
        .unwrap_err();
    assert_eq!(failure.reason, RunFailure::MarksOwned);
    assert!(failure.original.is_none());
    let bound = run.context.deadlines.get(PumpDeadline::Decision);
    assert!(run.context.read.is_none());
    run.context.read = Some(read);
    let inserted_read = std::ptr::from_ref(run.context.read.as_ref().unwrap());
    assert_eq!(fx.step(), Err(RunFailure::ReadOwned));
    let run = &mut fx.input.run;
    assert!(run.context.failure().is_none());
    assert!(run.context.pending_author().is_none());
    assert_eq!(run.context.deadlines.get(PumpDeadline::Decision), bound);
    assert_eq!(
        std::ptr::from_ref(run.context.read.as_ref().unwrap()),
        inserted_read
    );
    assert_eq!(
        format!("{:?}", run.context.held_author().unwrap()),
        expected
    );
    let Some(Payload::Fault(raw)) = &run.context.held_author().unwrap().payload else {
        unreachable!()
    };
    assert_eq!(raw.get().as_ptr(), pointer);
    let (returned, started) = marks_driver::validation_tests::returned_fault_author(&run.context)
        .expect("marks owns its returned original");
    assert_eq!(returned.reason, RunFailure::ReadOwned);
    assert!(!started);
    let event = returned.original.as_ref().unwrap();
    assert_eq!(event.kind, Kind::Fault);
    let Some(Payload::Fault(raw)) = &event.payload else {
        unreachable!()
    };
    let returned_pointer = raw.get().as_ptr();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(raw.get()).unwrap()["case"],
        "state_template_rejected"
    );
    run.context.take_held_author().unwrap();
    assert_eq!(fx.step(), Err(RunFailure::ReadOwned));
    let run = &mut fx.input.run;
    let (returned, _) =
        marks_driver::validation_tests::returned_fault_author(&run.context).unwrap();
    let Some(Payload::Fault(raw)) = &returned.original.as_ref().unwrap().payload else {
        unreachable!()
    };
    assert_eq!(raw.get().as_ptr(), returned_pointer);
    // Move the read back to its actual owner; its successful proof was never borrowed.
    other.input.run.context.read = run.context.read.take();
    assert!(fx.input.records(Kind::Fault).is_empty());
}

// Public task/source values cannot recreate the private receipt-bearing owners.
const _: fn() = || {
    trait MustNotClone<Marker> {
        fn check() {}
    }
    impl<T: ?Sized> MustNotClone<()> for T {}
    impl<T: Clone> MustNotClone<u8> for T {}
    let _ = <marks_driver::BegunTask as MustNotClone<_>>::check;
    let _ = <marks_driver::FiledTaskInput as MustNotClone<_>>::check;
    trait MustNotFromScope<Marker> {
        fn check() {}
    }
    impl<T: ?Sized> MustNotFromScope<()> for T {}
    impl<T: From<s::TaskScopeRef>> MustNotFromScope<u8> for T {}
    let _ = <marks_driver::BegunTask as MustNotFromScope<_>>::check;
    trait MustNotFromSeatedInput<Marker> {
        fn check() {}
    }
    impl<T: ?Sized> MustNotFromSeatedInput<()> for T {}
    impl<T: From<SeatedInput>> MustNotFromSeatedInput<u8> for T {}
    let _ = <marks_driver::FiledTaskInput as MustNotFromSeatedInput<_>>::check;
};

fn ports<I: PumpIo>(io: &mut I) -> PumpPorts<'_, I> {
    PumpPorts {
        lifecycle: LifecyclePorts::default(),
        gate: None,
        decode: None,
        label: None,
        io,
    }
}
fn inventory() -> CompiledInventory {
    static TEMPLATES: OnceLock<Vec<s::TemplateSpec>> = OnceLock::new();
    CompiledInventory::new(TEMPLATES.get_or_init(|| {
        vec![s::TemplateSpec {
            template_id: "mark.begin_task".into(),
            kind: "state.mark".into(),
            parts: vec![s::TemplatePart::Literal("Read task.".into())],
        }]
    }))
    .unwrap()
}
fn accepted(request: &s::Ste100Request) -> s::Ste100Reply {
    let len = request.text.len() as u64;
    let bindings = if matches!(request.target, s::Ste100Target::Template { .. }) {
        vec![]
    } else {
        vec![s::Binding {
            statement_span: s::Span {
                start: 0.into(),
                end: len.into(),
            },
            verb: "read".into(),
            verb_span: s::Span {
                start: 0.into(),
                end: 4.into(),
            },
            condition: None,
            condition_span: None,
            nouns: vec![s::NounBinding {
                value: request.text[5..request.text.len() - 1].into(),
                span: s::Span {
                    start: 5.into(),
                    end: (len - 1).into(),
                },
            }],
        }]
    };
    s::Ste100Reply {
        request_id: request.request_id,
        target: request.target.clone(),
        scheme_id: request.scheme_id.clone(),
        result: s::Ste100Result::Accepted { bindings },
    }
}
fn base64(bytes: &[u8]) -> String {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = u32::from(chunk[0]) << 16
            | u32::from(*chunk.get(1).unwrap_or(&0)) << 8
            | u32::from(*chunk.get(2).unwrap_or(&0));
        out.push(A[(n >> 18 & 63) as usize] as char);
        out.push(A[(n >> 12 & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            A[(n >> 6 & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            A[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}
struct TaskFixture {
    input: InputFixture,
    certificate: TemplateCertificate,
    received: Vec<u8>,
    pending: Vec<s::StateFrame>,
    requests: Vec<s::StateFrame>,
    operation: u64,
    bound: AbsoluteBound,
    result: s::QueryResult,
    hold_input: bool,
    hold_release: bool,
    hold_mark: bool,
    reject_mark: bool,
    refuse_read: bool,
}
impl TaskFixture {
    fn new() -> Self {
        Self::with_template_prefix(false)
    }
    fn with_template_prefix(prefix: bool) -> Self {
        let inventory = inventory();
        let declaration = tests::config();
        let mut input = InputFixture::with_declaration(Some(&inventory), declaration);
        if prefix {
            input
                .run
                .author(AuthoredEvent {
                    kind: Kind::ModelMeasurement,
                    subsystem: weaver_trace::Subsystem::Harness,
                    turn: Some(TurnRef("template-prefix".into())),
                    source: crate::flow::author::AuthoredSource::Runtime,
                    causal_parent: None,
                    payload: Some(Payload::ModelMeasurement(
                        serde_json::value::to_raw_value(
                            &serde_json::json!({"fixture":"shift template transcript"}),
                        )
                        .unwrap(),
                    )),
                })
                .unwrap();
        }
        input
            .run
            .context
            .begin_template_checks(inventory, &mut input.run.clock)
            .unwrap();
        for _ in 0..128 {
            let progress = input
                .run
                .context
                .advance_template_checks(&mut input.run.clock)
                .unwrap();
            input.classifier_transport();
            if let Receive::Complete(message) = input.peer.try_receive().unwrap() {
                assert_eq!(message.tag, Tag::Request);
                let request: s::Ste100Request =
                    s::decode_strict(std::str::from_utf8(&message.payload).unwrap()).unwrap();
                input.reply(&accepted(&request));
            }
            if progress == TemplateProgress::Complete {
                break;
            }
        }
        let certificate = input
            .run
            .context
            .take_template_certificate(&mut input.run.clock)
            .unwrap()
            .unwrap();
        input
            .run
            .context
            .complete_wait(PumpDeadline::Enter)
            .unwrap();
        input.run.state.set_nonblocking(true).unwrap();
        let now = input.run.clock.now().unwrap();
        let bound = AbsoluteBound::after(now, 1_000_000_000).unwrap();
        input
            .run
            .context
            .arm(PumpDeadline::Decision, bound, &mut input.run.clock)
            .unwrap();
        Self {
            input,
            certificate,
            received: vec![],
            pending: vec![],
            requests: vec![],
            operation: 2,
            bound,
            result: s::QueryResult::Marks {
                next_span_id: 1.into(),
                current_task: None,
                open_scopes: vec![],
            },
            hold_input: false,
            hold_release: false,
            hold_mark: false,
            reject_mark: false,
            refuse_read: false,
        }
    }
    fn token(&mut self) -> AcceptedInput {
        self.input
            .begin(TurnFrame::carry_from(br#"{"text":"Read file."}"#, 42));
        for _ in 0..128 {
            self.input
                .run
                .context
                .advance_input(&mut ports(&mut self.input.run.clock))
                .unwrap();
            self.checker();
            if let Some(token) = self
                .input
                .run
                .context
                .take_accepted_input(&mut self.input.run.clock)
                .unwrap()
            {
                return token;
            }
        }
        panic!("no real AcceptedInput");
    }
    fn begin(&mut self) {
        let token = self.token();
        self.input
            .run
            .context
            .begin_task(
                token,
                TurnRef("task-turn".into()),
                &self.certificate,
                &mut self.input.run.clock,
            )
            .unwrap();
    }
    fn transport(&mut self, source: TransportSource) {
        let context = &mut self.input.run.context;
        context
            .flow
            .transport_one(
                &mut FlowRecording {
                    barrier: &mut context.barrier,
                    inbox: &mut context.inbox,
                    budget: &mut context.budget,
                    author: &context.author,
                },
                &mut self.input.run.clock,
                source,
            )
            .unwrap();
    }
    fn checker(&mut self) {
        self.input.classifier_transport();
        let Receive::Complete(message) = self.input.peer.try_receive().unwrap() else {
            return;
        };
        let request: s::Ste100Request =
            s::decode_strict(std::str::from_utf8(&message.payload).unwrap()).unwrap();
        let mut reply = accepted(&request);
        if self.reject_mark && request.text == "Read task." {
            reply.result = s::Ste100Result::NotSte100 {
                code: s::Ste100DefectCode::NonSte100,
                detail: "scripted certified-template defect".into(),
                spans: vec![],
            };
        }
        self.input.reply(&reply);
    }
    fn member(&mut self) {
        self.transport(TransportSource::State);
        let mut bytes = [0; 65536];
        loop {
            match self.input.run.state.read(&mut bytes) {
                Ok(0) => panic!("member closed"),
                Ok(n) => self.received.extend_from_slice(&bytes[..n]),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(error) => panic!("member read {error}"),
            }
        }
        while let Some(end) = self.received.iter().position(|byte| *byte == b'\n') {
            let line: Vec<_> = self.received.drain(..=end).collect();
            let request = s::decode_state_line(&line, s::FrameDirection::Request).unwrap();
            self.requests.push(request.clone());
            self.pending.push(request);
        }
        if self.pending.first().is_some_and(|request| match request {
            s::StateFrame::File { filing, .. } => {
                if filing.source.kind == "message.user" {
                    self.hold_input
                } else {
                    self.hold_mark
                }
            }
            s::StateFrame::Control {
                frame: s::StateControl::Release { .. },
                ..
            } => self.hold_release,
            _ => false,
        }) {
            return;
        }
        if self.pending.is_empty() {
            return;
        }
        let request = self.pending.remove(0);
        let now = self.input.run.clock.now().unwrap();
        let observation = operation(&request, self.operation, now);
        self.operation += 1;
        self.input.run.clock.advance_to(now + 2).unwrap();
        let response = match &request {
            s::StateFrame::File { filing, .. } => s::StateFrame::Filed {
                common: request.common().clone(),
                block_id: filing.block_id.clone(),
                observation,
            },
            s::StateFrame::Control { frame, .. } => {
                let frame = match frame {
                    s::StateControl::Read {
                        selector: s::ReadSelector::Marks {},
                    } if self.refuse_read => s::StateControlAnswer::ControlRefused {
                        code: s::ControlRefusalCode::Read(s::ReadRefusalCode::CounterExhausted),
                        detail: "scripted catalog refusal".into(),
                    },
                    s::StateControl::Read {
                        selector: s::ReadSelector::Marks {},
                    } => {
                        let raw = serde_json::to_vec(&self.result).unwrap();
                        s::StateControlAnswer::ReadPage(s::ReadPage {
                            snapshot_id: 7.into(),
                            offset: 0.into(),
                            total_bytes: (raw.len() as u64).into(),
                            octets: base64(&raw),
                            done: true,
                        })
                    }
                    s::StateControl::Release { snapshot_id } => s::StateControlAnswer::Released {
                        snapshot_id: *snapshot_id,
                    },
                    other => panic!("unexpected mark control {other:?}"),
                };
                s::StateFrame::ControlAnswer {
                    common: request.common().clone(),
                    frame,
                    observation,
                }
            }
            other => panic!("unexpected mark request {other:?}"),
        };
        self.input
            .run
            .state
            .write_all(&s::encode_state_line(&response, s::FrameDirection::Response).unwrap())
            .unwrap();
        self.transport(TransportSource::State);
    }
    fn step(&mut self) -> Result<TaskProgress, RunFailure> {
        let progress = self
            .input
            .run
            .context
            .advance_task(&mut ports(&mut self.input.run.clock))?;
        self.checker();
        self.member();
        Ok(progress)
    }
    fn finish(&mut self) -> TaskCompletion {
        for _ in 0..512 {
            self.step().unwrap();
            if let Some(result) = self
                .input
                .run
                .context
                .take_task(&mut self.input.run.clock)
                .unwrap()
            {
                return result;
            }
        }
        panic!("task did not complete");
    }
    fn steps(&mut self, count: usize) {
        for _ in 0..count {
            self.step().unwrap();
        }
    }
    fn span(&self, id: u64) -> s::SpanRef {
        s::SpanRef {
            seed_id: self
                .input
                .run
                .context
                .open_start
                .as_ref()
                .unwrap()
                .seed_id
                .clone(),
            local_id: id.into(),
            revision: 1.into(),
        }
    }
    fn until_input_receipt_taken(&mut self) {
        for _ in 0..256 {
            self.step().unwrap();
            let context = &self.input.run.context;
            if context.open_turn.is_some()
                && context.input_turn.is_none()
                && !context.flow.has_filing_consumer()
            {
                assert!(context.read.is_none());
                assert!(
                    self.requests
                        .iter()
                        .all(|request| matches!(request, s::StateFrame::File { .. }))
                );
                return;
            }
        }
        panic!("input receipt did not reach the pre-read handoff");
    }
}

#[test]
fn begin_task_requires_input_release_recall_and_mark_receipt_in_order() {
    let mut fx = TaskFixture::new();
    fx.hold_input = true;
    fx.begin();
    fx.steps(24);
    assert_eq!(fx.input.records(Kind::MessageUser).len(), 1);
    assert_eq!(fx.input.records(Kind::TurnStarted).len(), 1);
    assert!(
        fx.requests
            .iter()
            .all(|request| matches!(request, s::StateFrame::File { .. }))
    );
    assert!(fx.input.records(Kind::StateMark).is_empty());
    assert!(
        fx.input
            .run
            .context
            .take_task(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
    fx.hold_input = false;
    fx.hold_release = true;
    fx.steps(32);
    assert!(fx.input.records(Kind::Recall).is_empty());
    assert!(fx.input.records(Kind::StateMark).is_empty());
    assert!(fx.pending.iter().any(|request| matches!(
        request,
        s::StateFrame::Control {
            frame: s::StateControl::Release { .. },
            ..
        }
    )));
    fx.hold_release = false;
    fx.hold_mark = true;
    fx.steps(32);
    assert_eq!(fx.input.records(Kind::Recall).len(), 1);
    assert_eq!(fx.input.records(Kind::StateMark).len(), 1);
    assert!(
        fx.input
            .run
            .context
            .take_task(&mut fx.input.run.clock)
            .unwrap()
            .is_none()
    );
    assert!(
        fx.input
            .run
            .context
            .complete_wait(PumpDeadline::Decision)
            .is_none()
    );
    assert_eq!(
        fx.input.run.context.deadlines.get(PumpDeadline::Decision),
        Some(fx.bound)
    );
    fx.hold_mark = false;
    let TaskCompletion::Begun(task) = fx.finish() else {
        panic!("no filed task");
    };
    assert_eq!(
        task.scope(),
        &s::TaskScopeRef {
            task: fx.span(1),
            prompt: fx.span(2)
        }
    );
    assert!(task.belongs_to(&fx.input.run.context));
    assert!(task.mark().is_some());
    assert_eq!(
        task.input().message().content,
        vec![weaver_traits::ContentBlock::Text {
            text: "Read file.".into()
        }]
    );
    let mark = &fx.input.records(Kind::StateMark)[0];
    assert_eq!(mark["origin"], "loop");
    assert_eq!(mark["channel"], "internal");
    assert_eq!(mark["turn"], "task-turn");
    assert_eq!(
        mark["payload"]["context"]["begin_task"]["source"]["event"]["block_id"],
        serde_json::to_value(task.input().source()).unwrap()
    );
    assert_eq!(
        fx.requests
            .iter()
            .filter(|r| matches!(
                r,
                s::StateFrame::Control {
                    frame: s::StateControl::Release { .. },
                    ..
                }
            ))
            .count(),
        1
    );
}

#[test]
fn begin_task_returns_existing_open_scope_without_authoring_or_allocating() {
    let mut fx = TaskFixture::new();
    let scope = s::TaskScopeRef {
        task: fx.span(1),
        prompt: fx.span(2),
    };
    fx.result = s::QueryResult::Marks {
        next_span_id: 5.into(),
        current_task: Some(scope.clone()),
        open_scopes: vec![s::SpanInfo {
            span: scope.task.clone(),
            parent: None,
            kind: s::SpanKind::Task,
            status: s::SpanStatus::Open,
            opened_source: Some(s::SourceBlockId {
                run: "prior-run".into(),
                sequence: 4.into(),
                block: 0.into(),
            }),
            ended_source: None,
            replacement: None,
        }],
    };
    fx.begin();
    let TaskCompletion::Begun(task) = fx.finish() else {
        panic!("no existing task");
    };
    assert_eq!(task.scope(), &scope);
    assert!(task.mark().is_none());
    assert!(fx.input.records(Kind::StateMark).is_empty());
}

#[test]
fn begin_task_correct_template_rejection_is_visible_without_a_component_latch() {
    let mut fx = TaskFixture::new();
    fx.reject_mark = true;
    fx.begin();
    let TaskCompletion::Refused {
        refusal: TaskRefusal::TemplateRejected { fault },
        ..
    } = fx.finish()
    else {
        panic!("wrong rejection");
    };
    assert!(fx.input.run.context.failure().is_none());
    let faults = fx.input.records(Kind::Fault);
    assert_eq!(faults.len(), 1);
    assert_eq!(faults[0]["sequence"], fault.0.to_string());
    assert_eq!(faults[0]["payload"]["case"], "state_template_rejected");
    assert!(!fx.requests.iter().any(|request| matches!(request, s::StateFrame::File { filing, .. } if filing.source.kind == "state.mark")));
}

#[test]
fn begin_task_missing_required_input_or_mark_is_refused_at_activation() {
    for (kind, path) in [
        ("message.user", "message.user/input/operator"),
        ("state.mark", "state.mark/internal/loop"),
    ] {
        let mut declaration = tests::config();
        declaration.partition.retain(|rule| rule.kind != kind);
        let mut run = tests::Fixture::new(declaration);
        run.opened();
        let load = run.load_event();
        run.author(load).unwrap();
        let ActivationFailure::Partition(error) = run.context.activate(&mut run.clock).unwrap_err()
        else {
            panic!("missing required selection must refuse partition activation");
        };
        assert_eq!(error.code, serving::PartitionErrorCode::InvalidProjection);
        assert_eq!(error.path.as_deref(), Some(path));
        assert!(!run.context.barrier.is_active());
        assert!(run.context.input.snapshot().is_none());
        assert!(run.context.marks.is_none());
        // Activation refused before an active tap exists. Inspect the real
        // untapped writer after its normal drain instead of its active accessor.
        let limit = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while !matches!(
            run.context
                .barrier
                .drain_status(&mut run.context.budget)
                .unwrap(),
            serving::DrainStatus::Emptied
        ) {
            assert!(std::time::Instant::now() < limit, "writer did not drain");
            std::thread::yield_now();
        }
        run._reader.set_nonblocking(true).unwrap();
        let mut trace = Vec::new();
        let mut bytes = [0; 4096];
        loop {
            match run._reader.read(&mut bytes) {
                Ok(0) => break,
                Ok(count) => trace.extend_from_slice(&bytes[..count]),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(error) => panic!("untapped trace read: {error}"),
            }
        }
        let records: Vec<serde_json::Value> = std::str::from_utf8(&trace)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert!(records.iter().any(|record| record["kind"] == "load"));
        assert!(records.iter().all(|record| record["kind"] != "state.mark"));
    }
}

#[test]
fn begin_task_cancellation_retains_input_and_drains_an_already_authored_mark() {
    let mut before = TaskFixture::new();
    before.begin();
    before.input.run.context.cancel_task();
    let TaskCompletion::Cancelled(mut account) = before.finish() else {
        panic!("cancel granted task");
    };
    assert!(account.take_unconsumed_input().is_some());
    assert!(before.input.records(Kind::TurnStarted).is_empty());
    let mut after = TaskFixture::new();
    after.hold_mark = true;
    after.begin();
    after.steps(64);
    assert_eq!(after.input.records(Kind::StateMark).len(), 1);
    after.input.run.context.cancel_task();
    after.steps(8);
    assert!(
        after
            .input
            .run
            .context
            .take_task(&mut after.input.run.clock)
            .unwrap()
            .is_none()
    );
    after.hold_mark = false;
    assert!(matches!(after.finish(), TaskCompletion::Cancelled(_)));
    assert_eq!(after.input.records(Kind::StateMark).len(), 1);
}

#[test]
fn begin_task_read_refusal_and_id_exhaustion_never_become_empty_catalogs() {
    for read_refusal in [false, true] {
        let mut fx = TaskFixture::new();
        fx.refuse_read = read_refusal;
        fx.result = s::QueryResult::Marks {
            next_span_id: u64::MAX.into(),
            current_task: None,
            open_scopes: vec![],
        };
        fx.begin();
        let TaskCompletion::Refused { refusal, .. } = fx.finish() else {
            panic!("refusal granted task");
        };
        if read_refusal {
            assert!(matches!(refusal, TaskRefusal::Read(_)));
        } else {
            assert!(matches!(refusal, TaskRefusal::CounterExhausted));
        }
        assert!(fx.input.records(Kind::StateMark).is_empty());
        assert!(fx.input.run.context.failure().is_none());
    }
}

#[test]
fn begin_task_rejects_foreign_or_crossed_mark_catalogs() {
    for wrong_seed in [false, true] {
        let mut fx = TaskFixture::new();
        let mut task = fx.span(1);
        if wrong_seed {
            task.seed_id = "b".repeat(32);
        }
        fx.result = s::QueryResult::Marks {
            next_span_id: 4.into(),
            current_task: Some(s::TaskScopeRef {
                task: task.clone(),
                prompt: fx.span(2),
            }),
            open_scopes: vec![s::SpanInfo {
                span: task,
                parent: (!wrong_seed).then(|| fx.span(2)),
                kind: s::SpanKind::Task,
                status: s::SpanStatus::Open,
                opened_source: Some(s::SourceBlockId {
                    run: "run".into(),
                    sequence: 1.into(),
                    block: 0.into(),
                }),
                ended_source: None,
                replacement: None,
            }],
        };
        fx.begin();
        let failure = (0..256)
            .find_map(|_| fx.step().err())
            .expect("catalog fault");
        assert!(matches!(
            failure,
            RunFailure::Flow {
                reason: FlowHaltReason::ProtocolFailed,
                ref detail,
                ..
            } if detail == "malformed begin_task mark catalog"
        ));
        assert!(fx.input.records(Kind::StateMark).is_empty());
    }
}

#[test]
fn begin_task_held_author_keeps_input_read_and_mark_continuations_live() {
    let mut fx = TaskFixture::new();
    fx.begin();
    let original = tests::event(Kind::Fault, None);
    let expected = format!("{original:?}");
    let run = &mut fx.input.run;
    assert_eq!(
        run.context
            .author_and_wait(&mut ports(&mut run.clock), original)
            .map_err(|failure| failure.retained_reason()),
        Err(RunFailure::MarksOwned)
    );
    let TaskCompletion::Begun(task) = fx.finish() else {
        panic!("actual admitted task stalled");
    };
    assert!(task.mark().is_some());
    assert!(task.belongs_to(&fx.input.run.context));
    assert_eq!(fx.input.records(Kind::Recall).len(), 1);
    assert_eq!(fx.input.records(Kind::StateMark).len(), 1);
    let run = &mut fx.input.run;
    assert!(run.clock.now().unwrap() < fx.bound.deadline_ns());
    assert!(run.context.failure().is_none());
    assert_eq!(
        format!("{:?}", run.context.take_held_author().unwrap()),
        expected
    );
}

#[test]
fn begin_task_excludes_public_read_author_and_preserves_original_deadline() {
    let mut fx = TaskFixture::new();
    fx.hold_input = true;
    fx.begin();
    assert_eq!(
        fx.input
            .run
            .context
            .begin_read(s::ReadSelector::Marks {}, None, &mut fx.input.run.clock)
            .unwrap_err()
            .refusal,
        ReadBeginRefusal::Busy
    );
    let event = tests::event(Kind::Fault, None);
    assert_eq!(
        fx.input
            .run
            .context
            .author_and_wait(&mut ports(&mut fx.input.run.clock), event)
            .map_err(|failure| failure.retained_reason())
            .unwrap_err(),
        RunFailure::MarksOwned
    );
    fx.steps(24);
    fx.input
        .run
        .clock
        .advance_to(fx.bound.deadline_ns())
        .unwrap();
    assert!(matches!(
        fx.step(),
        Err(RunFailure::Flow {
            reason: FlowHaltReason::DecisionBound,
            ..
        })
    ));
    assert!(fx.input.records(Kind::StateMark).is_empty());
}

#[test]
fn begin_task_catalog_cannot_reuse_the_closed_prompt_as_an_open_step() {
    let mut fx = TaskFixture::new();
    let task = fx.span(1);
    let prompt = fx.span(2);
    let source = s::SourceBlockId {
        run: "run".into(),
        sequence: 1.into(),
        block: 0.into(),
    };
    fx.result = s::QueryResult::Marks {
        next_span_id: 3.into(),
        current_task: Some(s::TaskScopeRef {
            task: task.clone(),
            prompt: prompt.clone(),
        }),
        open_scopes: vec![
            s::SpanInfo {
                span: task.clone(),
                parent: None,
                kind: s::SpanKind::Task,
                status: s::SpanStatus::Open,
                opened_source: Some(source.clone()),
                ended_source: None,
                replacement: None,
            },
            s::SpanInfo {
                span: prompt,
                parent: Some(task),
                kind: s::SpanKind::Step,
                status: s::SpanStatus::Open,
                opened_source: Some(source),
                ended_source: None,
                replacement: None,
            },
        ],
    };
    fx.begin();
    let failure = (0..256)
        .find_map(|_| fx.step().err())
        .expect("catalog fault");
    assert!(matches!(
        failure,
        RunFailure::Flow {
            reason: FlowHaltReason::ProtocolFailed,
            ref detail,
            ..
        } if detail == "malformed begin_task mark catalog"
    ));
    assert!(fx.input.records(Kind::StateMark).is_empty());
}

#[test]
fn begin_task_timely_completion_retires_the_bound_before_late_retrieval() {
    for refused in [false, true] {
        let mut fx = TaskFixture::new();
        fx.refuse_read = refused;
        fx.begin();
        for _ in 0..256 {
            if fx.step().unwrap() == TaskProgress::Complete {
                break;
            }
        }
        assert!(
            fx.input
                .run
                .context
                .deadlines
                .get(PumpDeadline::Decision)
                .is_none()
        );
        fx.input
            .run
            .clock
            .advance_to(fx.bound.deadline_ns() + 1)
            .unwrap();
        let result = fx
            .input
            .run
            .context
            .take_task(&mut fx.input.run.clock)
            .unwrap()
            .unwrap();
        assert!(matches!(
            (&result, refused),
            (TaskCompletion::Begun(_), false) | (TaskCompletion::Refused { .. }, true)
        ));
        assert!(fx.input.run.context.failure().is_none());
    }
}

#[test]
fn begin_task_active_and_completed_owner_excludes_freshness() {
    let mut fx = TaskFixture::new();
    fx.begin();
    for complete in [false, true] {
        if complete {
            assert!((0..512).any(|_| fx.step().unwrap() == TaskProgress::Complete));
        }
        let decision = fx.input.run.context.deadlines.get(PumpDeadline::Decision);
        let error = fx
            .input
            .run
            .context
            .prepare_cut(vec![], &mut fx.input.run.clock)
            .unwrap_err();
        assert_eq!(error.refusal, FreshnessBeginRefusal::Busy);
        assert!(error.failure.is_none());
        assert!(error.required_ids.is_empty());
        assert!(fx.input.run.context.freshness.is_none());
        assert!(fx.input.run.context.marks.is_some());
        assert_eq!(
            fx.input.run.context.deadlines.get(PumpDeadline::Decision),
            decision
        );
        assert!(
            fx.input
                .run
                .context
                .deadlines
                .get(PumpDeadline::Cut)
                .is_none()
        );
    }
    assert!(matches!(
        fx.input
            .run
            .context
            .take_task(&mut fx.input.run.clock)
            .unwrap(),
        Some(TaskCompletion::Begun(_))
    ));
    assert!(fx.input.run.context.failure().is_none());
}

#[test]
fn begin_task_refuses_active_and_completed_freshness_before_consuming_input() {
    for complete in [false, true] {
        let mut source = TaskFixture::new();
        let token = source.token();
        let mut busy = TaskFixture::new();
        busy.input
            .run
            .context
            .prepare_cut(vec![], &mut busy.input.run.clock)
            .unwrap();
        if complete {
            assert_eq!(
                busy.input
                    .run
                    .context
                    .advance_prepare_cut(&mut ports(&mut busy.input.run.clock))
                    .unwrap(),
                FreshnessProgress::Complete
            );
        }
        let cut = busy.input.run.context.deadlines.get(PumpDeadline::Cut);
        // A real token from another context reaches only admission. A local
        // AcceptedInput cannot coexist with freshness under the public guards.
        // Busy must return custody before validating or consuming this token.
        let error = busy
            .input
            .run
            .context
            .begin_task(
                token,
                TurnRef("task-turn".into()),
                &busy.certificate,
                &mut busy.input.run.clock,
            )
            .unwrap_err();
        assert_eq!(error.refusal, marks_driver::BeginTaskRefusal::Busy);
        assert!(error.failure.is_none());
        assert!(busy.input.run.context.marks.is_none());
        assert!(busy.input.run.context.freshness.is_some());
        assert_eq!(busy.input.run.context.deadlines.get(PumpDeadline::Cut), cut);
        assert!(busy.input.run.context.failure().is_none());
        source
            .input
            .run
            .context
            .begin_task(
                error.input,
                TurnRef("task-turn".into()),
                &source.certificate,
                &mut source.input.run.clock,
            )
            .unwrap();
        assert!(matches!(source.finish(), TaskCompletion::Begun(_)));
        assert_eq!(
            busy.input
                .run
                .context
                .advance_prepare_cut(&mut ports(&mut busy.input.run.clock))
                .unwrap(),
            FreshnessProgress::Complete
        );
        let Some(FreshnessCompletion::Prepared(prepared)) = busy
            .input
            .run
            .context
            .take_prepared_cut(&mut busy.input.run.clock)
            .unwrap()
        else {
            panic!("admission refusal lost the freshness result");
        };
        assert!(prepared.belongs_to(&busy.input.run.context));
        assert!(prepared.required_ids().is_empty());
    }
}

struct HandoffClock<'a> {
    clock: &'a mut crate::flow::clock::testing::FakePumpIo,
    before_reads: usize,
    deadline: u64,
}
impl PumpIo for HandoffClock<'_> {
    fn now(&mut self) -> std::io::Result<u64> {
        if self.before_reads == 0 {
            self.clock.advance_to(self.deadline)?;
        } else {
            self.before_reads -= 1;
        }
        self.clock.now()
    }
    fn wait(
        &mut self,
        descriptors: &[crate::flow::clock::WaitInterest<'_>],
        deadline: Option<u64>,
    ) -> std::io::Result<crate::flow::clock::WaitOutcome> {
        self.clock.wait(descriptors, deadline)
    }
}

#[test]
fn begin_task_nested_leave_yields_preserve_the_input_owner_and_exact_record() {
    // Sweep actual admission/Recorded checkpoints. No fabricated source or
    // receipt enters the owner, even when the nested author has already won.
    for reads in 1..14 {
        let mut fx = TaskFixture::new();
        fx.begin();
        let now = fx.input.run.clock.now().unwrap();
        let leave = AbsoluteBound::after(now, 10).unwrap();
        fx.input
            .run
            .context
            .arm(PumpDeadline::Leave, leave, &mut fx.input.run.clock)
            .unwrap();
        let mut clock = HandoffClock {
            clock: &mut fx.input.run.clock,
            before_reads: reads,
            deadline: leave.deadline_ns(),
        };
        let outcome = fx.input.run.context.advance_task(&mut ports(&mut clock));
        if let Err(failure) = outcome {
            assert!(matches!(
                failure,
                RunFailure::Wait {
                    key: PumpDeadline::Leave,
                    ..
                }
            ));
        }
        assert!(fx.input.run.context.failure().is_none());
        assert_eq!(
            fx.input.run.context.complete_wait(PumpDeadline::Leave),
            Some(leave)
        );
        assert!(matches!(fx.finish(), TaskCompletion::Begun(_)));
        assert_eq!(fx.input.records(Kind::TurnStarted).len(), 1);
        assert_eq!(fx.input.records(Kind::MessageUser).len(), 1);
        assert_eq!(fx.input.records(Kind::StateMark).len(), 1);
    }
}

#[test]
fn begin_task_rejects_out_of_range_and_nonadjacent_catalog_ids() {
    for adjacent in [false, true] {
        let mut fx = TaskFixture::new();
        fx.result = s::QueryResult::Marks {
            next_span_id: if adjacent { 2.into() } else { 4.into() },
            current_task: Some(s::TaskScopeRef {
                task: fx.span(1),
                prompt: fx.span(if adjacent { 2 } else { 3 }),
            }),
            open_scopes: vec![],
        };
        fx.begin();
        let failure = (0..256)
            .find_map(|_| fx.step().err())
            .expect("bad catalog was accepted");
        assert!(matches!(
            failure,
            RunFailure::Flow {
                reason: FlowHaltReason::ProtocolFailed,
                ref detail,
                ..
            } if detail == "malformed begin_task mark catalog"
        ));
        assert!(fx.input.records(Kind::StateMark).is_empty());
        assert!(
            fx.input
                .run
                .context
                .take_task(&mut fx.input.run.clock)
                .is_err()
        );
    }
}

#[test]
fn begin_task_uses_the_last_two_representable_span_ids() {
    let mut fx = TaskFixture::new();
    fx.result = s::QueryResult::Marks {
        next_span_id: (u64::MAX - 1).into(),
        current_task: None,
        open_scopes: vec![],
    };
    fx.begin();
    let TaskCompletion::Begun(task) = fx.finish() else {
        panic!("last valid two-ID proposal refused");
    };
    assert_eq!(task.scope().task, fx.span(u64::MAX - 1));
    assert_eq!(task.scope().prompt, fx.span(u64::MAX));
    assert_eq!(fx.input.records(Kind::StateMark).len(), 1);
    // The scripted receipt proves this owner's proposal, not a SQL commit.
    // MARK-READ specifies CounterExhausted when the next MAX+1 is requested.
}

#[test]
fn begin_task_input_association_defenses_inspect_real_receipt_values() {
    let mut fx = TaskFixture::new();
    fx.begin();
    let TaskCompletion::Begun(task) = fx.finish() else {
        panic!("no actual task");
    };
    marks_driver::validation_tests::input_associations(&task);
}

#[test]
fn begin_task_mark_association_defenses_inspect_real_receipt_values() {
    let mut fx = TaskFixture::new();
    fx.begin();
    let TaskCompletion::Begun(task) = fx.finish() else {
        panic!("no actual task");
    };
    marks_driver::validation_tests::mark_associations(&task);
}

#[test]
fn begin_task_bound_validator_inspects_an_actual_owner_without_replacing_it() {
    let mut fx = TaskFixture::new();
    fx.begin();
    assert_eq!(
        fx.input.run.context.complete_wait(PumpDeadline::Decision),
        None
    );
    let work = fx.input.run.context.marks.as_ref().unwrap();
    assert!(work.has_original_wait(Some(fx.bound)));
    assert!(!work.has_original_wait(None));
    let duration = fx.bound.deadline_ns() - fx.bound.began_ns();
    for bound in [
        AbsoluteBound::after(fx.bound.began_ns(), duration - 1).unwrap(),
        AbsoluteBound::after(fx.bound.began_ns() + 1, duration - 1).unwrap(),
    ] {
        assert!(!work.has_original_wait(Some(bound)));
    }
    // These are inspection values. The real registry and owner still agree.
    assert!(matches!(fx.finish(), TaskCompletion::Begun(_)));
}

#[test]
fn begin_task_rejects_a_decision_rearmed_after_completion() {
    let mut fx = TaskFixture::new();
    fx.begin();
    for _ in 0..256 {
        if fx.step().unwrap() == TaskProgress::Complete {
            break;
        }
    }
    assert!(
        fx.input
            .run
            .context
            .deadlines
            .get(PumpDeadline::Decision)
            .is_none()
    );
    let competing = AbsoluteBound::after(fx.input.run.clock.now().unwrap(), 100).unwrap();
    fx.input
        .run
        .context
        .arm(PumpDeadline::Decision, competing, &mut fx.input.run.clock)
        .unwrap();
    let failure = fx
        .input
        .run
        .context
        .take_task(&mut fx.input.run.clock)
        .unwrap_err();
    assert!(matches!(
        failure,
        RunFailure::Flow {
            reason: FlowHaltReason::ProtocolFailed,
            ..
        }
    ));
    assert!(fx.input.run.context.marks.is_some());
    assert_eq!(fx.input.run.context.failure(), Some(&failure));
}

#[test]
fn begin_task_private_marks_read_purpose_excludes_a_public_read_at_handoff() {
    let mut fx = TaskFixture::new();
    fx.begin();
    fx.until_input_receipt_taken();
    let selector = s::ReadSelector::Marks {};
    let error = fx
        .input
        .run
        .context
        .begin_read(
            selector.clone(),
            Some(TurnRef("task-turn".into())),
            &mut fx.input.run.clock,
        )
        .unwrap_err();
    assert_eq!(error.refusal, ReadBeginRefusal::Busy);
    assert_eq!(error.selector, selector);
    assert!(error.failure.is_none());
    assert!(fx.input.run.context.read.is_none());
    assert!(
        fx.input
            .run
            .context
            .deadlines
            .get(PumpDeadline::StateControl)
            .is_none()
    );
    assert!(matches!(fx.finish(), TaskCompletion::Begun(_)));
}

#[test]
fn begin_task_cancellation_after_input_receipt_starts_no_read_or_mark() {
    let mut fx = TaskFixture::new();
    fx.begin();
    fx.until_input_receipt_taken();
    fx.input.run.context.cancel_task();
    let TaskCompletion::Cancelled(mut account) = fx.finish() else {
        panic!("cancel granted a task");
    };
    assert!(account.take_unconsumed_input().is_none());
    assert_eq!(fx.input.records(Kind::MessageUser).len(), 1);
    assert!(fx.input.records(Kind::StateMark).is_empty());
    assert!(
        fx.requests
            .iter()
            .all(|request| matches!(request, s::StateFrame::File { .. }))
    );
    assert!(fx.input.run.context.read.is_none());
    assert!(fx.input.run.context.failure().is_none());
}

#[test]
fn begin_task_cancelled_input_receipt_completes_without_a_new_read_step() {
    let mut fx = TaskFixture::new();
    fx.hold_input = true;
    fx.begin();
    fx.steps(24);
    assert!(fx.input.run.context.open_turn.is_some());
    assert!(fx.input.run.context.input_turn.is_none());
    assert!(fx.input.run.context.flow.has_filing_consumer());
    assert!(matches!(
        fx.pending.first(),
        Some(s::StateFrame::File { filing, .. }) if filing.source.kind == "message.user"
    ));
    assert_eq!(fx.input.records(Kind::MessageUser).len(), 1);

    // The accepted input is consumed, but the real member has not returned its
    // Filed response. Cancel before the owner consumes that exact receipt.
    fx.input.run.context.cancel_task();
    fx.hold_input = false;
    let mut completed = false;
    for _ in 0..256 {
        let progress = fx.step().unwrap();
        if !fx.input.run.context.flow.has_filing_consumer() {
            // Receipt consumption must settle cancellation and retire Decision
            // together. Moving to Read would leave another fallible live step.
            assert_eq!(progress, TaskProgress::Complete);
            assert!(
                fx.input
                    .run
                    .context
                    .deadlines
                    .get(PumpDeadline::Decision)
                    .is_none()
            );
            completed = true;
            break;
        }
    }
    assert!(completed, "cancelled input filing did not settle");
    fx.input
        .run
        .clock
        .advance_to(fx.bound.deadline_ns() + 1)
        .unwrap();
    let Some(TaskCompletion::Cancelled(mut account)) = fx
        .input
        .run
        .context
        .take_task(&mut fx.input.run.clock)
        .unwrap()
    else {
        panic!("settled cancellation did not retain its account");
    };
    assert!(account.take_unconsumed_input().is_none());
    assert!(fx.input.run.context.read.is_none());
    assert!(fx.input.records(Kind::StateMark).is_empty());
    assert!(
        fx.requests
            .iter()
            .all(|request| matches!(request, s::StateFrame::File { .. }))
    );
    assert!(fx.input.run.context.failure().is_none());
}

#[test]
fn begin_task_rejects_a_certificate_from_another_recorded_transcript() {
    let mut fx = TaskFixture::new();
    let other = TaskFixture::with_template_prefix(true);
    assert_eq!(
        fx.certificate.inventory_sha256(),
        other.certificate.inventory_sha256()
    );
    assert_eq!(fx.certificate.run_id(), other.certificate.run_id());
    assert_ne!(
        fx.certificate.accepted()[0].result_source(),
        other.certificate.accepted()[0].result_source()
    );
    let token = fx.token();
    let error = fx
        .input
        .run
        .context
        .begin_task(
            token,
            TurnRef("task-turn".into()),
            &other.certificate,
            &mut fx.input.run.clock,
        )
        .unwrap_err();
    assert_eq!(error.refusal, BeginTaskRefusal::InvalidTemplate);
    assert!(error.failure.is_none());
    assert!(fx.input.run.context.marks.is_none());
    assert!(!fx.input.run.context.flow.has_filing_consumer());
    assert!(fx.input.records(Kind::TurnStarted).is_empty());
    fx.input
        .run
        .context
        .begin_task(
            error.input,
            TurnRef("task-turn".into()),
            &fx.certificate,
            &mut fx.input.run.clock,
        )
        .unwrap();
    assert!(matches!(fx.finish(), TaskCompletion::Begun(_)));
}

#[test]
fn begin_task_cancellation_after_settlement_retains_private_input_and_mark_evidence() {
    let mut fx = TaskFixture::new();
    fx.begin();
    for _ in 0..256 {
        if fx.step().unwrap() == TaskProgress::Complete {
            break;
        }
    }
    assert!(
        fx.input
            .run
            .context
            .deadlines
            .get(PumpDeadline::Decision)
            .is_none()
    );
    fx.input.run.context.cancel_task();
    let Some(TaskCompletion::Cancelled(mut account)) = fx
        .input
        .run
        .context
        .take_task(&mut fx.input.run.clock)
        .unwrap()
    else {
        panic!("settled cancellation exposed a task grant");
    };
    assert!(account.take_unconsumed_input().is_none());
    marks_driver::validation_tests::cancelled_evidence(&account);
    assert_eq!(fx.input.records(Kind::MessageUser).len(), 1);
    assert_eq!(fx.input.records(Kind::StateMark).len(), 1);
    assert!(fx.input.run.context.failure().is_none());
}
