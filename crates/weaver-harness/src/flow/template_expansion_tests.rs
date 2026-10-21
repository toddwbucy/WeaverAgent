use super::*;
use crate::flow::coordinator::{CheckOperation, RecordedCheck};
use crate::flow::templates::{CompiledInventory, TemplateCheckTranscript};
use weaver_types::state_flow::{
    SlotClass, SourceBlockId, SourcePosition, Ste100Reply, Ste100Result, Ste100Target, TemplateSpec,
};

static RESTATEMENT: RestatementTemplate = RestatementTemplate::new("restate");
static UNKNOWN: RestatementTemplate = RestatementTemplate::new("unknown");

// The run certificate and expansion are capabilities, not wire values. Only
// actual recorded checks certify production inventories. These owner fixtures
// use the existing private test proof to isolate the expansion checks.
const _: fn() = || {
    trait NotClone<Marker> {
        fn check() {}
    }
    impl<T: ?Sized> NotClone<()> for T {}
    impl<T: Clone> NotClone<u8> for T {}
    let _ = <TemplateExpansion as NotClone<_>>::check;
    trait NotDefault<Marker> {
        fn check() {}
    }
    impl<T: ?Sized> NotDefault<()> for T {}
    impl<T: Default> NotDefault<u8> for T {}
    let _ = <TemplateExpansion as NotDefault<_>>::check;
};

fn scheme() -> SchemeDeclaration {
    let seed: weaver_types::state_flow::SeedRequest =
        weaver_types::state_flow::decode_strict(include_str!("fixtures/seed.json")).unwrap();
    SchemeDeclaration {
        artifact_sha256: Digest::new(seed.scheme_id.strip_prefix("ste100:").unwrap()).unwrap(),
        scheme_id: seed.scheme_id,
        verbs: vec!["read".into()],
    }
}

fn checker() -> Digest {
    Digest::new("b".repeat(64)).unwrap()
}

fn template() -> TemplateSpec {
    TemplateSpec {
        template_id: "restate".into(),
        kind: "loop.instruction".into(),
        parts: vec![
            TemplatePart::Literal("Restate ".into()),
            TemplatePart::Literal("the statement.\n".into()),
        ],
    }
}

fn certificate(entries: Vec<TemplateSpec>) -> TemplateCertificate {
    let inventory = CompiledInventory::new(Box::leak(entries.into_boxed_slice())).unwrap();
    let load = inventory.record(&scheme(), &checker());
    let mut transcript =
        TemplateCheckTranscript::begin(inventory, "run".into(), scheme(), checker(), &load)
            .unwrap();
    let mut ordinal = 1;
    while let Some(operation) = transcript.next_operation() {
        let CheckOperation::Template { template_id, .. } = &operation else {
            unreachable!()
        };
        let reply = Ste100Reply {
            request_id: ordinal.into(),
            target: Ste100Target::Template {
                template_id: template_id.clone(),
            },
            scheme_id: scheme().scheme_id,
            result: Ste100Result::Accepted { bindings: vec![] },
        };
        let result_source = SourcePosition {
            block_id: SourceBlockId {
                run: "run".into(),
                sequence: (10 + ordinal).into(),
                block: 0.into(),
            },
            turn: None,
        };
        transcript
            .recorded(RecordedCheck::for_test(operation, reply, result_source))
            .unwrap();
        ordinal += 1;
    }
    transcript.finish().unwrap()
}

fn expected(certificate: &TemplateCertificate) -> ExpansionIdentity<'_> {
    ExpansionIdentity {
        run_id: certificate.run_id(),
        scheme: certificate.scheme(),
        checker_artifact_sha256: certificate.checker_artifact_sha256(),
        inventory_sha256: certificate.inventory_sha256(),
    }
}

#[test]
fn literal_restatement_retains_exact_certified_bytes_and_all_proof_identities() {
    let mut first = template();
    first.template_id = "other".into();
    let certificate = certificate(vec![first, template()]);
    let expanded = RESTATEMENT
        .expand(&certificate, expected(&certificate))
        .unwrap();
    assert_eq!(RESTATEMENT.template_id(), "restate");
    assert_eq!(expanded.template_id(), "restate");
    assert_eq!(expanded.text(), "Restate the statement.\n");
    assert_eq!(expanded.message().role, Role::User);
    assert_eq!(
        expanded.message().content,
        vec![ContentBlock::Text {
            text: "Restate the statement.\n".into(),
        }]
    );
    assert_eq!(expanded.run_id(), "run");
    assert_eq!(expanded.scheme(), &scheme());
    assert_eq!(expanded.checker_artifact_sha256(), &checker());
    assert_eq!(expanded.inventory_sha256(), certificate.inventory_sha256());
    assert_eq!(expanded.acceptance().template_id(), "restate");
    assert_eq!(expanded.acceptance().request_id().0, 2);
    assert_eq!(
        expanded.acceptance().result_source().block_id.sequence.0,
        12
    );
    assert_eq!(expanded.acceptance().result_source().turn, None);
    // Further attempts expand the same fixed certified words. Neither expansion
    // claims a source ID or a recorded runtime instruction.
    let next = RESTATEMENT
        .expand(&certificate, expected(&certificate))
        .unwrap();
    assert_eq!(next.text(), expanded.text());
}

#[test]
fn restatement_requires_every_recorded_load_identity_and_the_exact_scheme() {
    let certificate = certificate(vec![template()]);
    let foreign_digest = Digest::new("c".repeat(64)).unwrap();
    let mut changed_verbs = scheme();
    changed_verbs.verbs.push("write".into());
    let mut changed_scheme = scheme();
    changed_scheme.artifact_sha256 = foreign_digest.clone();
    changed_scheme.scheme_id = format!("ste100:{foreign_digest}");
    for identity in [
        ExpansionIdentity {
            run_id: "foreign",
            ..expected(&certificate)
        },
        ExpansionIdentity {
            run_id: "",
            ..expected(&certificate)
        },
        ExpansionIdentity {
            scheme: &changed_verbs,
            ..expected(&certificate)
        },
        ExpansionIdentity {
            scheme: &changed_scheme,
            ..expected(&certificate)
        },
        ExpansionIdentity {
            checker_artifact_sha256: &foreign_digest,
            ..expected(&certificate)
        },
        ExpansionIdentity {
            inventory_sha256: &foreign_digest,
            ..expected(&certificate)
        },
    ] {
        assert_eq!(
            RESTATEMENT.expand(&certificate, identity).unwrap_err(),
            ExpansionRefusal::ForeignCertificate
        );
    }
}

#[test]
fn restatement_registration_cannot_select_unknown_or_different_event_family() {
    let valid = certificate(vec![template()]);
    assert_eq!(
        UNKNOWN.expand(&valid, expected(&valid)).unwrap_err(),
        ExpansionRefusal::InvalidTemplate
    );
    let mut wrong_kind = template();
    wrong_kind.kind = "sort.decision".into();
    let wrong = certificate(vec![wrong_kind]);
    assert_eq!(
        RESTATEMENT.expand(&wrong, expected(&wrong)).unwrap_err(),
        ExpansionRefusal::InvalidTemplate
    );
}

#[test]
fn restatement_never_interpolates_slots_or_creates_a_step_spelling() {
    for class in [SlotClass::TechnicalName, SlotClass::StepId] {
        let mut with_slot = template();
        with_slot.parts.insert(
            1,
            TemplatePart::Slot {
                name: "reference".into(),
                class,
            },
        );
        let certificate = certificate(vec![with_slot]);
        assert_eq!(
            RESTATEMENT
                .expand(&certificate, expected(&certificate))
                .unwrap_err(),
            ExpansionRefusal::UnsupportedSlot
        );
    }
    let mut empty = template();
    empty.parts = vec![TemplatePart::Literal(String::new())];
    let certificate = certificate(vec![empty]);
    assert_eq!(
        RESTATEMENT
            .expand(&certificate, expected(&certificate))
            .unwrap_err(),
        ExpansionRefusal::EmptyBody
    );
}

#[test]
fn restatement_checks_the_exact_acceptance_in_its_certificate() {
    for mutate in [
        (|certificate: &mut TemplateCertificate| certificate.accepted.clear())
            as fn(&mut TemplateCertificate),
        |certificate| certificate.accepted[0].template_id = "other".into(),
        |certificate| certificate.accepted[0].request_id = 0.into(),
        |certificate| certificate.accepted[0].result_source.block_id.run = "other".into(),
        |certificate| certificate.accepted[0].result_source.block_id.sequence = 0.into(),
        |certificate| certificate.accepted[0].result_source.block_id.block = 1.into(),
        |certificate| certificate.accepted[0].result_source.turn = Some("turn".into()),
        |certificate| {
            certificate.inventory_sha256 = Digest::new("c".repeat(64)).unwrap();
        },
    ] {
        let mut certificate = certificate(vec![template()]);
        mutate(&mut certificate);
        assert_eq!(
            RESTATEMENT
                .expand(&certificate, expected(&certificate))
                .unwrap_err(),
            ExpansionRefusal::InvalidCertificate
        );
    }
}
