use super::*;
use weaver_types::state_flow::{SlotClass, Ste100Target};

fn inventory(entries: Vec<TemplateSpec>) -> CompiledInventory {
    CompiledInventory::new(Box::leak(entries.into_boxed_slice())).unwrap()
}

fn transcript(inventory: CompiledInventory) -> TemplateCheckTranscript {
    let load = inventory.record(&scheme(), &checker());
    TemplateCheckTranscript::begin(inventory, "run".into(), scheme(), checker(), &load).unwrap()
}

#[test]
fn offer_inventory_records_the_exact_compiled_text_and_full_digest() {
    assert_eq!(
        serde_json::to_string(offer::template()).unwrap(),
        include_str!("fixtures/offer-template.json").trim()
    );
    let entries = vec![
        template("first"),
        offer::template().clone(),
        template("second"),
    ];
    let expected = serde_json::to_vec(&entries).unwrap();
    let inventory = inventory(entries);
    assert_eq!(inventory.bytes(), expected);
    assert_eq!(
        inventory.digest().as_str(),
        format!("{:x}", Sha256::digest(&expected))
    );
    assert_eq!(inventory.certifiable().count(), 2);
    let elected = inventory.record(&scheme(), &checker());
    let absent = inventory.record_election(&scheme(), None);
    assert_eq!(serde_json::to_vec(&elected.templates).unwrap(), expected);
    assert_eq!(elected.templates, absent.templates);
    assert_eq!(elected.inventory_sha256, absent.inventory_sha256);
    assert_eq!(absent.checker_artifact_sha256, None);
}

#[test]
fn offer_inventory_refuses_changed_words_identity_layout_and_unlicensed_slots() {
    for field in 0..8 {
        let mut changed = offer::template().clone();
        match field {
            0 => changed.template_id.push('x'),
            1 => changed.kind = "loop.instruction".into(),
            2 => changed.parts[0] = TemplatePart::Literal("Use any meaning.\n".into()),
            3 => changed.parts.reverse(),
            4 => changed
                .parts
                .push(TemplatePart::Literal("More prose.".into())),
            5 => {
                changed.parts[1] = TemplatePart::Slot {
                    name: "caller".into(),
                    class: SlotClass::ThesaurusOffer,
                }
            }
            6 => {
                changed.parts[1] = TemplatePart::Slot {
                    name: "material".into(),
                    class: SlotClass::TechnicalName,
                }
            }
            7 => {
                let TemplatePart::Literal(text) = &mut changed.parts[0] else {
                    unreachable!()
                };
                text.pop();
            }
            _ => unreachable!(),
        }
        assert!(
            matches!(
                CompiledInventory::new(Box::leak(vec![changed].into_boxed_slice())),
                Err(TemplateError::InvalidInventory)
            ),
            "field={field}"
        );
    }
    let mut ordinary = template("ordinary");
    ordinary.parts[1] = TemplatePart::Slot {
        name: "material".into(),
        class: SlotClass::ThesaurusOffer,
    };
    assert!(matches!(
        CompiledInventory::new(Box::leak(vec![ordinary].into_boxed_slice())),
        Err(TemplateError::InvalidInventory)
    ));
}

#[test]
fn offer_inventory_skips_only_offer_in_original_certification_order() {
    for position in 0..=2 {
        let mut entries = static_templates().to_vec();
        entries.insert(position, offer::template().clone());
        let mut checks = transcript(inventory(entries));
        assert!(!checks.is_complete());
        for index in 0..2 {
            assert_eq!(checks.next_operation(), Some(parts(index, 7, 11).0));
            assert_eq!(
                checks
                    .recorded(recorded(index, 7 + index as u64, 11 + index as u64))
                    .unwrap(),
                if index == 1 {
                    TranscriptProgress::Complete
                } else {
                    TranscriptProgress::Accepted
                }
            );
        }
        assert!(checks.is_complete());
        assert!(checks.next_operation().is_none());
        let certificate = checks.finish().unwrap();
        assert_eq!(
            certificate
                .accepted()
                .iter()
                .map(|a| a.template_id())
                .collect::<Vec<_>>(),
            vec!["first", "second"]
        );
        assert_eq!(certificate.inventory().templates().len(), 3);
    }
    let mut only_offer = transcript(inventory(vec![offer::template().clone()]));
    assert!(only_offer.is_complete());
    assert!(only_offer.next_operation().is_none());
    assert!(only_offer.finish().unwrap().accepted().is_empty());
}

#[test]
fn offer_inventory_cannot_accept_a_checker_reply_for_the_disabled_entry() {
    let mut checks = transcript(inventory(vec![
        offer::template().clone(),
        template("first"),
    ]));
    let (mut operation, mut reply, source) = parts(0, 7, 11);
    let CheckOperation::Template { template_id, text } = &mut operation else {
        unreachable!()
    };
    *template_id = offer::TEMPLATE_ID.into();
    *text = serde_json::to_string(offer::template()).unwrap();
    reply.target = Ste100Target::Template {
        template_id: offer::TEMPLATE_ID.into(),
    };
    assert_eq!(
        checks.recorded(RecordedCheck::for_test(operation, reply, source)),
        Err(TemplateError::CheckMismatch)
    );
    assert!(matches!(checks.finish(), Err(TemplateError::CheckMismatch)));
}
