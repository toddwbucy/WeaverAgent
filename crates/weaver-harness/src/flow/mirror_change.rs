//! TRACE-EMISSION-CHANGE converts each field without granting an applied change.

use super::*;

pub(in crate::flow) fn thesaurus_match(value: &shared::ThesaurusMatch) -> trace::ThesaurusMatch {
    let shared::ThesaurusMatch {
        source: original,
        previous_change,
        sentence_span,
        word_span,
        scheme_id,
        thesaurus_sha256,
        entry_word,
    } = value;
    trace::ThesaurusMatch {
        source: source(original),
        previous_change: previous_change.map(number),
        sentence_span: span(*sentence_span),
        word_span: span(*word_span),
        scheme_id: scheme_id.clone(),
        thesaurus_sha256: thesaurus_sha256.as_str().into(),
        entry_word: entry_word.clone(),
    }
}

pub(in crate::flow) fn emission_change(
    value: &shared::EmissionChangeRecord,
) -> trace::EmissionChangeRecord {
    let shared::EmissionChangeRecord {
        matched,
        span: replaced_span,
        replaced,
        replacement,
        method,
    } = value;
    trace::EmissionChangeRecord {
        matched: thesaurus_match(matched),
        span: span(*replaced_span),
        replaced: replaced.clone(),
        replacement: replacement.clone(),
        method: match method {
            shared::EmissionChangeMethod::Splice {} => trace::EmissionChangeMethod::Splice {},
            shared::EmissionChangeMethod::Offer {
                instruction,
                reemission,
                reemission_span,
            } => trace::EmissionChangeMethod::Offer {
                instruction: source(instruction),
                reemission: source(reemission),
                reemission_span: span(*reemission_span),
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn change_mirror_preserves_both_methods_and_every_exact_field() {
        for text in [
            include_str!("../../../weaver-types/tests/fixtures/ste-emission-splice.json"),
            include_str!("../../../weaver-types/tests/fixtures/ste-emission-offer.json"),
        ] {
            let mut value: shared::EmissionChangeRecord = serde_json::from_str(text).unwrap();
            assert_eq!(
                serde_json::to_string(&emission_change(&value)).unwrap(),
                text.trim()
            );
            value.matched.previous_change = Some(9_007_199_254_740_993.into());
            value.matched.source.block = u64::MAX.into();
            value.replaced = "Read \"caf\u{e9}\".\n".into();
            value.replacement = "Use \"fuel\".\n".into();
            assert_eq!(
                serde_json::to_string(&emission_change(&value)).unwrap(),
                serde_json::to_string(&value).unwrap(),
            );
            assert_eq!(
                serde_json::to_string(&thesaurus_match(&value.matched)).unwrap(),
                serde_json::to_string(&value.matched).unwrap(),
            );
        }
    }
}
