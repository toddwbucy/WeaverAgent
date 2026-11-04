//! The exact LOOKUP-OFFER inventory entry, under workflow clauses 5.06 and 5.07.
//! This compiled data supplies neither checked material nor an expansion grant.

use std::sync::OnceLock;
use weaver_types::state_flow::{SlotClass, TemplatePart, TemplateSpec};

pub(in crate::flow) const TEMPLATE_ID: &str = "thesaurus-offer/1";

pub(in crate::flow) fn template() -> &'static TemplateSpec {
    static TEMPLATE: OnceLock<TemplateSpec> = OnceLock::new();
    TEMPLATE.get_or_init(|| TemplateSpec {
        template_id: TEMPLATE_ID.into(),
        kind: "offer".into(),
        parts: vec![
            TemplatePart::Literal(
                concat!(
                    "Use the applicable meaning from the entry below.\n",
                    "Write the sentence again. Keep its meaning and literals unchanged.\n",
                )
                .into(),
            ),
            TemplatePart::Slot {
                name: "material".into(),
                class: SlotClass::ThesaurusOffer,
            },
        ],
    })
}
