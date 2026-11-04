//! TYPES-TEMPLATE's closed offer metadata, without template expansion authority.

use super::{ThesaurusMatch, codec::deserialize_object};
use serde::Deserialize;

pub(super) fn metadata<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<ThesaurusMatch, D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Fields {
        matched: ThesaurusMatch,
    }
    let fields: Fields = deserialize_object(deserializer)?;
    Ok(fields.matched)
}

#[cfg(test)]
mod tests {
    use super::super::{LoopInstructionMeta, SlotClass};

    #[test]
    fn offer_metadata_preserves_the_exact_match_and_closed_object_shape() {
        let fixture = include_str!("../../tests/fixtures/ste-offer-instruction.json").trim();
        let metadata: LoopInstructionMeta = serde_json::from_str(fixture).unwrap();
        assert_eq!(serde_json::to_string(&metadata).unwrap(), fixture);
        let value: serde_json::Value = serde_json::from_str(fixture).unwrap();
        let matched = serde_json::to_string(&value["offer"]["matched"]).unwrap();
        for text in [
            r#"{"offer":{}}"#.to_owned(),
            r#"{"offer":{"matched":null}}"#.to_owned(),
            format!(r#"{{"offer":[{matched}]}}"#),
            format!(r#"{{"offer":{{"matched":{matched},"extra":true}}}}"#),
            format!(r#"{{"offer":{{"matched":{matched},"matched":{matched}}}}}"#),
            format!(r#"{{"offer":{{"matched":{matched}}},"wind_down":{{}}}}"#),
        ] {
            assert!(
                serde_json::from_str::<LoopInstructionMeta>(&text).is_err(),
                "{text}"
            );
        }
        for field in value["offer"]["matched"].as_object().unwrap().keys() {
            let mut missing = value.clone();
            missing["offer"]["matched"]
                .as_object_mut()
                .unwrap()
                .remove(field);
            assert!(
                serde_json::from_value::<LoopInstructionMeta>(missing).is_err(),
                "{field}"
            );
        }
        let mut extra = value;
        extra["offer"]["matched"]["extra"] = true.into();
        assert!(serde_json::from_value::<LoopInstructionMeta>(extra).is_err());
        assert_eq!(
            serde_json::to_string(&SlotClass::ThesaurusOffer).unwrap(),
            "\"thesaurus_offer\""
        );
        assert_eq!(
            serde_json::from_str::<SlotClass>("\"thesaurus_offer\"").unwrap(),
            SlotClass::ThesaurusOffer
        );
    }
}
