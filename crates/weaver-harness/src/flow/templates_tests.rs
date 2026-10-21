use super::*;
use std::sync::OnceLock;
use weaver_types::state_flow::{
    Binding, NounBinding, SlotClass, SourceBlockId, Span, Ste100ComponentCode, Ste100DefectCode,
};

// The authority cannot be cloned or reconstructed through Default.
// A second applicable marker makes each expression fail inference if that
// capability is accidentally added. Integration tests use real recorded checks.
const _: fn() = || {
    trait NotClone<Marker> {
        fn check() {}
    }
    impl<T: ?Sized> NotClone<()> for T {}
    impl<T: Clone> NotClone<u8> for T {}
    let _ = <TemplateCertificate as NotClone<_>>::check;
    let _ = <TemplateAcceptance as NotClone<_>>::check;
    trait NotDefault<Marker> {
        fn check() {}
    }
    impl<T: ?Sized> NotDefault<()> for T {}
    impl<T: Default> NotDefault<u8> for T {}
    let _ = <TemplateCertificate as NotDefault<_>>::check;
};

fn template(id: &str) -> TemplateSpec {
    TemplateSpec {
        template_id: id.into(),
        kind: "loop.instruction".into(),
        parts: vec![
            TemplatePart::Literal("Read ".into()),
            TemplatePart::Slot {
                name: "item".into(),
                class: SlotClass::TechnicalName,
            },
            TemplatePart::Literal(".\n".into()),
        ],
    }
}

fn static_templates() -> &'static [TemplateSpec] {
    static INVENTORY: OnceLock<Vec<TemplateSpec>> = OnceLock::new();
    INVENTORY.get_or_init(|| vec![template("first"), template("second")])
}

fn inventory() -> CompiledInventory {
    CompiledInventory::new(static_templates()).unwrap()
}

fn scheme() -> SchemeDeclaration {
    let artifact_sha256 = Digest::new("a".repeat(64)).unwrap();
    SchemeDeclaration {
        scheme_id: format!("ste100:{artifact_sha256}"),
        artifact_sha256,
        verbs: vec!["read".into()],
    }
}

fn checker() -> Digest {
    Digest::new("b".repeat(64)).unwrap()
}

fn transcript() -> TemplateCheckTranscript {
    let inventory = inventory();
    let load = inventory.record(&scheme(), &checker());
    TemplateCheckTranscript::begin(inventory, "run".into(), scheme(), checker(), &load).unwrap()
}

fn parts(
    index: usize,
    request: u64,
    sequence: u64,
) -> (CheckOperation, Ste100Reply, SourcePosition) {
    let template = &static_templates()[index];
    (
        CheckOperation::Template {
            template_id: template.template_id.clone(),
            text: serde_json::to_string(template).unwrap(),
        },
        Ste100Reply {
            request_id: request.into(),
            target: Ste100Target::Template {
                template_id: template.template_id.clone(),
            },
            scheme_id: scheme().scheme_id,
            result: Ste100Result::Accepted {
                bindings: Vec::new(),
            },
        },
        SourcePosition {
            block_id: SourceBlockId {
                run: "run".into(),
                sequence: sequence.into(),
                block: 0.into(),
            },
            turn: None,
        },
    )
}

fn recorded(index: usize, request: u64, sequence: u64) -> RecordedCheck {
    let (operation, reply, source) = parts(index, request, sequence);
    RecordedCheck::for_test(operation, reply, source)
}

#[test]
fn compiled_inventory_has_exact_ordered_bytes_digest_and_trace_mirror() {
    let inventory = inventory();
    let expected = concat!(
        "[{\"template_id\":\"first\",\"kind\":\"loop.instruction\",\"parts\":[",
        "{\"literal\":\"Read \"},{\"slot\":{\"name\":\"item\",",
        "\"class\":\"technical_name\"}},{\"literal\":\".\\n\"}]},",
        "{\"template_id\":\"second\",\"kind\":\"loop.instruction\",\"parts\":[",
        "{\"literal\":\"Read \"},{\"slot\":{\"name\":\"item\",",
        "\"class\":\"technical_name\"}},{\"literal\":\".\\n\"}]}]"
    );
    assert_eq!(inventory.bytes(), expected.as_bytes());
    assert_eq!(
        inventory.digest().as_str(),
        format!("{:x}", Sha256::digest(expected.as_bytes()))
    );
    let record = inventory.record(&scheme(), &checker());
    assert_eq!(
        serde_json::to_vec(&record.templates).unwrap(),
        inventory.bytes()
    );
    assert_eq!(record.inventory_sha256, inventory.digest().as_str());
    assert_eq!(record.scheme_id, scheme().scheme_id);
    assert_eq!(record.checker_artifact_sha256, checker().as_str());

    let mut reversed = static_templates().to_vec();
    reversed.reverse();
    let reversed = CompiledInventory::new(Box::leak(reversed.into_boxed_slice())).unwrap();
    assert_ne!(inventory.digest(), reversed.digest());
    assert_ne!(inventory.bytes(), reversed.bytes());
}

#[test]
fn duplicate_identity_invalid_shape_and_conflicting_slots_refuse_inventory() {
    for entries in [
        vec![template("duplicate"), template("duplicate")],
        vec![template("")],
        vec![TemplateSpec {
            kind: "unregistered.kind".into(),
            ..template("unknown")
        }],
        vec![TemplateSpec {
            kind: "message.user".into(),
            ..template("input")
        }],
        vec![TemplateSpec {
            kind: "save_point".into(),
            ..template("metadata")
        }],
        vec![TemplateSpec {
            parts: Vec::new(),
            ..template("empty")
        }],
        vec![TemplateSpec {
            parts: vec![
                TemplatePart::Slot {
                    name: "same".into(),
                    class: SlotClass::StepId,
                },
                TemplatePart::Slot {
                    name: "same".into(),
                    class: SlotClass::TechnicalName,
                },
            ],
            ..template("conflicting")
        }],
    ] {
        assert!(CompiledInventory::new(Box::leak(entries.into_boxed_slice())).is_err());
    }
    let mut repeated = template("repeated");
    repeated.parts.push(repeated.parts[1].clone());
    assert!(CompiledInventory::new(Box::leak(vec![repeated].into_boxed_slice())).is_ok());
}

#[test]
fn load_agreement_covers_every_identity_and_exact_template_part() {
    let inventory = inventory();
    let exact = inventory.record(&scheme(), &checker());
    assert_eq!(
        inventory.matches_load(&exact, &scheme(), &checker()),
        Ok(())
    );
    for mutate in [
        (|load: &mut trace::TemplateInventory| load.scheme_id.push('x'))
            as fn(&mut trace::TemplateInventory),
        |load| load.inventory_sha256 = "c".repeat(64),
        |load| load.checker_artifact_sha256 = "c".repeat(64),
        |load| load.templates.reverse(),
        |load| load.templates[0].template_id.push('x'),
        |load| load.templates[0].kind = "sort.decision".into(),
        |load| load.templates[0].parts[0] = trace::TemplatePart::Literal("Read  ".into()),
        |load| {
            load.templates[0].parts[1] = trace::TemplatePart::Slot {
                name: "other".into(),
                class: trace::SlotClass::TechnicalName,
            }
        },
        |load| {
            load.templates[0].parts[1] = trace::TemplatePart::Slot {
                name: "item".into(),
                class: trace::SlotClass::StepId,
            }
        },
    ] {
        let mut changed = exact.clone();
        mutate(&mut changed);
        assert_eq!(
            inventory.matches_load(&changed, &scheme(), &checker()),
            Err(TemplateError::LoadMismatch)
        );
    }
    let mut invalid = scheme();
    invalid.scheme_id = "unbound".into();
    assert!(matches!(
        TemplateCheckTranscript::begin(inventory, "run".into(), invalid, checker(), &exact),
        Err(TemplateError::InvalidIdentity)
    ));
}

#[test]
fn only_complete_ordered_recorded_acceptances_build_the_exact_certificate() {
    let mut transcript = transcript();
    assert_eq!(transcript.accepted_count(), 0);
    assert_eq!(transcript.next_operation(), Some(parts(0, 7, 11).0));
    assert_eq!(
        transcript.recorded(recorded(0, 7, 11)),
        Ok(TranscriptProgress::Accepted)
    );
    assert_eq!(transcript.next_operation(), Some(parts(1, 9, 15).0));
    assert_eq!(
        transcript.recorded(recorded(1, 9, 15)),
        Ok(TranscriptProgress::Complete)
    );
    assert!(transcript.is_complete());
    assert!(transcript.next_operation().is_none());
    assert_eq!(transcript.records().len(), 2);
    assert_eq!(transcript.records()[0].operation(), &parts(0, 7, 11).0);
    assert_eq!(transcript.records()[0].reply(), &parts(0, 7, 11).1);
    assert_eq!(transcript.records()[1].result_source(), &parts(1, 9, 15).2);
    let certificate = transcript.finish().unwrap();
    assert_eq!(transcript.records().len(), 2);
    assert!(matches!(
        transcript.finish(),
        Err(TemplateError::OutOfOrder)
    ));
    assert_eq!(certificate.run_id(), "run");
    assert_eq!(certificate.scheme_id(), scheme().scheme_id);
    assert_eq!(certificate.checker_artifact_sha256(), &checker());
    assert_eq!(certificate.inventory_sha256(), inventory().digest());
    assert_eq!(certificate.inventory().templates(), static_templates());
    assert_eq!(certificate.accepted().len(), 2);
    for (index, (request, sequence)) in [(7, 11), (9, 15)].into_iter().enumerate() {
        let acceptance = &certificate.accepted()[index];
        assert_eq!(
            acceptance.template_id(),
            static_templates()[index].template_id
        );
        assert_eq!(acceptance.request_id(), DecimalU64(request));
        assert_eq!(
            acceptance.result_source(),
            &parts(index, request, sequence).2
        );
    }
}

#[test]
fn partial_or_extra_transcripts_cannot_pass() {
    assert!(matches!(
        transcript().finish(),
        Err(TemplateError::Incomplete)
    ));
    let mut partial = transcript();
    partial.recorded(recorded(0, 1, 2)).unwrap();
    assert!(matches!(partial.finish(), Err(TemplateError::Incomplete)));
    let mut extra = transcript();
    extra.recorded(recorded(0, 1, 2)).unwrap();
    extra.recorded(recorded(1, 2, 3)).unwrap();
    assert_eq!(
        extra.recorded(recorded(1, 3, 4)),
        Err(TemplateError::OutOfOrder)
    );
    assert!(!extra.is_complete());
    assert!(matches!(extra.finish(), Err(TemplateError::OutOfOrder)));
}

#[test]
fn out_of_order_or_changed_exact_template_text_freezes_and_keeps_evidence() {
    let mut reordered = transcript();
    assert_eq!(
        reordered.recorded(recorded(1, 1, 2)),
        Err(TemplateError::CheckMismatch)
    );
    assert_eq!(reordered.records().len(), 1);
    let mut changed = transcript();
    let (mut operation, reply, source) = parts(0, 1, 2);
    let CheckOperation::Template { text, .. } = &mut operation else {
        unreachable!()
    };
    text.push(' ');
    assert_eq!(
        changed.recorded(RecordedCheck::for_test(operation.clone(), reply, source)),
        Err(TemplateError::CheckMismatch)
    );
    assert_eq!(changed.records()[0].operation(), &operation);
    assert_eq!(changed.accepted_count(), 0);
    assert!(changed.next_operation().is_none());
    assert_eq!(
        changed.recorded(recorded(0, 2, 3)),
        Err(TemplateError::CheckMismatch)
    );
    assert_eq!(changed.records().len(), 2);
}

#[test]
fn source_run_sequence_block_turn_and_request_counter_are_bound() {
    for mutate in [
        (|_: &mut Ste100Reply, source: &mut SourcePosition| source.block_id.run = "other".into())
            as fn(&mut Ste100Reply, &mut SourcePosition),
        |_, source| source.block_id.sequence = 0.into(),
        |_, source| source.block_id.block = 1.into(),
        |_, source| source.turn = Some("turn".into()),
        |reply, _| reply.request_id = 0.into(),
        |reply, _| reply.scheme_id = "other".into(),
        |reply, _| {
            reply.target = Ste100Target::Template {
                template_id: "second".into(),
            }
        },
    ] {
        let mut transcript = transcript();
        let (operation, mut reply, mut source) = parts(0, 1, 2);
        mutate(&mut reply, &mut source);
        assert_eq!(
            transcript.recorded(RecordedCheck::for_test(
                operation,
                reply.clone(),
                source.clone()
            )),
            Err(TemplateError::InvalidResult)
        );
        assert_eq!(transcript.records()[0].reply(), &reply);
        assert_eq!(transcript.records()[0].result_source(), &source);
    }
    for (request, sequence) in [(5, 11), (6, 10), (4, 9)] {
        let mut transcript = transcript();
        transcript.recorded(recorded(0, 5, 10)).unwrap();
        assert_eq!(
            transcript.recorded(recorded(1, request, sequence)),
            Err(TemplateError::InvalidResult)
        );
        assert_eq!(transcript.accepted_count(), 1);
    }
}

fn invented_binding() -> Binding {
    let span = |start, end| Span {
        start: DecimalU64(start),
        end: DecimalU64(end),
    };
    Binding {
        statement_span: span(0, 1),
        verb: "read".into(),
        verb_span: span(0, 1),
        condition: None,
        condition_span: None,
        nouns: vec![NounBinding {
            value: "x".into(),
            span: span(0, 1),
        }],
    }
}

#[test]
fn a_template_acceptance_with_invented_text_bindings_never_certifies() {
    let mut transcript = transcript();
    let (operation, mut reply, source) = parts(0, 1, 2);
    reply.result = Ste100Result::Accepted {
        bindings: vec![invented_binding()],
    };
    assert_eq!(
        transcript.recorded(RecordedCheck::for_test(operation, reply, source)),
        Err(TemplateError::InvalidResult)
    );
    assert!(matches!(
        transcript.finish(),
        Err(TemplateError::InvalidResult)
    ));
}

#[test]
fn correct_rejection_is_a_retained_verdict_and_never_a_certificate() {
    let mut transcript = transcript();
    let (operation, mut reply, source) = parts(0, 1, 2);
    reply.result = Ste100Result::NotSte100 {
        code: Ste100DefectCode::NonSte100,
        detail: "fixture grammar rejects this template".into(),
        spans: Vec::new(),
    };
    assert_eq!(
        transcript.recorded(RecordedCheck::for_test(operation, reply.clone(), source)),
        Ok(TranscriptProgress::Rejected)
    );
    assert_eq!(transcript.rejection(), Some(&reply));
    assert_eq!(transcript.failure(), Some(TemplateError::Rejected));
    assert!(transcript.next_operation().is_none());
    assert_eq!(
        transcript.recorded(recorded(0, 2, 3)),
        Err(TemplateError::Rejected)
    );
    assert_eq!(transcript.rejection(), Some(&reply));
    assert!(matches!(transcript.finish(), Err(TemplateError::Rejected)));
}

#[test]
fn component_failure_remains_distinct_from_a_language_verdict() {
    let mut transcript = transcript();
    let (operation, mut reply, source) = parts(0, 1, 2);
    reply.result = Ste100Result::ComponentFailed {
        code: Ste100ComponentCode::NoResult,
        detail: "fixture checker produced no result".into(),
    };
    assert_eq!(
        transcript.recorded(RecordedCheck::for_test(operation, reply.clone(), source)),
        Err(TemplateError::ComponentFailed)
    );
    assert_eq!(transcript.records()[0].reply(), &reply);
    assert!(transcript.rejection().is_none());
    assert!(matches!(
        transcript.finish(),
        Err(TemplateError::ComponentFailed)
    ));
}
