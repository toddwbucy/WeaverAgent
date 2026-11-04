//! TYPES-LOOKUP and TYPES-EMISSION-CHANGE data, with no runtime grant.
//! The harness owns artifact, source-version and decode-transaction authority.

use serde::{Deserialize, Serialize};

use super::{DecimalU64, Digest, OfferSense, SourceBlockId, Span, ValidationError};

super::codec::closed_object! {
    #[derive(Debug, Clone, PartialEq, Eq, Serialize)]
    pub struct ThesaurusMatch {
        pub source: SourceBlockId,
        #[serde(deserialize_with = "super::required_option")]
        pub previous_change: Option<DecimalU64>,
        #[serde(deserialize_with = "super::codec::deserialize_object")]
        pub sentence_span: Span,
        #[serde(deserialize_with = "super::codec::deserialize_object")]
        pub word_span: Span,
        pub scheme_id: String,
        pub thesaurus_sha256: Digest,
        pub entry_word: String,
    }
}

impl ThesaurusMatch {
    /// Check shape only. The owner must compare the current source and artifact.
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.source.run.is_empty() || self.scheme_id.is_empty() || self.entry_word.is_empty() {
            return Err(ValidationError("empty lookup identity"));
        }
        if self.sentence_span.start >= self.sentence_span.end
            || self.word_span.start >= self.word_span.end
            || !self.sentence_span.contains(&self.word_span)
        {
            return Err(ValidationError("invalid lookup spans"));
        }
        Ok(())
    }

    /// Check byte boundaries in the supplied version, without granting a lookup.
    pub fn validate_text(&self, current: &str) -> Result<(), ValidationError> {
        self.validate()?;
        self.sentence_span.slice(current)?;
        self.word_span.slice(current)?;
        Ok(())
    }
}

super::codec::closed_object! {
    #[derive(Debug, Clone, PartialEq, Eq, Serialize)]
    pub struct OfferMaterial {
        pub matched: ThesaurusMatch,
        pub sentence: String,
        pub senses: Vec<OfferSense>,
    }
}

impl OfferMaterial {
    /// Preserve the sentence's exact UTF-8 width and the artifact's sense order.
    pub fn validate(&self) -> Result<(), ValidationError> {
        self.matched.validate()?;
        let sentence = self.matched.sentence_span;
        if sentence.end.0 - sentence.start.0 != self.sentence.len() as u64 || self.senses.is_empty()
        {
            return Err(ValidationError("invalid offer sentence or senses"));
        }
        let word = self.matched.word_span;
        Span {
            start: (word.start.0 - sentence.start.0).into(),
            end: (word.end.0 - sentence.start.0).into(),
        }
        .slice(&self.sentence)?;
        for sense in &self.senses {
            sense.validate()?;
        }
        Ok(())
    }
}

super::codec::closed_object! {
    #[derive(Debug, Clone, PartialEq, Eq, Serialize)]
    pub struct EmissionChangeRecord {
        pub matched: ThesaurusMatch,
        #[serde(deserialize_with = "super::codec::deserialize_object")]
        pub span: Span,
        pub replaced: String,
        pub replacement: String,
        pub method: EmissionChangeMethod,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EmissionChangeMethod {
    Splice {},
    Offer {
        instruction: SourceBlockId,
        reemission: SourceBlockId,
        reemission_span: Span,
    },
}

impl<'de> Deserialize<'de> for EmissionChangeMethod {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        super::codec::closed_object! {
            #[derive(Serialize)]
            struct Splice {}
        }
        super::codec::closed_object! {
            #[derive(Serialize)]
            struct Offer {
                instruction: SourceBlockId,
                reemission: SourceBlockId,
                #[serde(deserialize_with = "super::codec::deserialize_object")]
                reemission_span: Span,
            }
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "snake_case")]
        enum Method {
            Splice(Splice),
            Offer(Offer),
        }
        Ok(match Method::deserialize(deserializer)? {
            Method::Splice(_) => Self::Splice {},
            Method::Offer(Offer {
                instruction,
                reemission,
                reemission_span,
            }) => Self::Offer {
                instruction,
                reemission,
                reemission_span,
            },
        })
    }
}

impl EmissionChangeRecord {
    /// Check text and span evidence. A decoded record grants no change authority.
    /// The harness must also prove the current chain, artifact and offer binding.
    pub fn validate_texts(
        &self,
        current: &str,
        reemission_text: Option<&str>,
    ) -> Result<(), ValidationError> {
        self.matched.validate_text(current)?;
        if self.replaced.is_empty()
            || self.replacement.is_empty()
            || self.span.slice(current)? != self.replaced
        {
            return Err(ValidationError(
                "change does not name its exact replaced text",
            ));
        }
        match &self.method {
            EmissionChangeMethod::Splice {} => {
                if self.span != self.matched.word_span || reemission_text.is_some() {
                    return Err(ValidationError("invalid splice evidence"));
                }
            }
            EmissionChangeMethod::Offer {
                instruction,
                reemission,
                reemission_span,
            } => {
                if self.span != self.matched.sentence_span
                    || instruction.block.0 != 0
                    || instruction.run != self.matched.source.run
                    || reemission.run != instruction.run
                {
                    return Err(ValidationError("invalid offer references"));
                }
                let text = reemission_text.ok_or(ValidationError("missing offer response"))?;
                if reemission_span.slice(text)? != self.replacement {
                    return Err(ValidationError(
                        "change does not name its exact response slice",
                    ));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "lookup_tests.rs"]
mod tests;
