//! Spec 02 SCHEME and THESAURUS, workflow 3.02 and 3.03.
//! These are artifact data, with no lookup or runtime-operation authority.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::{Digest, SchemeDeclaration, ValidationError};

super::codec::closed_object! {
    /// TYPES-CONFIG: the elective artifact identity, separate from its scheme.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize)]
    pub struct ThesaurusDeclaration {
        pub artifact_sha256: Digest,
        pub scheme_id: String,
    }
}

impl ThesaurusDeclaration {
    pub fn validate(&self, scheme: &SchemeDeclaration) -> Result<(), ValidationError> {
        scheme.validate()?;
        if self.scheme_id != scheme.scheme_id {
            return Err(ValidationError("thesaurus declaration scheme mismatch"));
        }
        Ok(())
    }
}

super::codec::closed_object! {
    #[derive(Debug, Clone, PartialEq, Eq, Serialize)]
    pub struct Ste100Thesaurus {
        pub format: String,
        pub scheme_id: String,
        pub source: ThesaurusSource,
        pub entries: Vec<ThesaurusEntry>,
    }
}

super::codec::closed_object! {
    #[derive(Debug, Clone, PartialEq, Eq, Serialize)]
    pub struct ThesaurusSource {
        pub repository: String,
        pub commit: String,
        pub path: String,
        pub sha256: Digest,
    }
}

super::codec::closed_object! {
    #[derive(Debug, Clone, PartialEq, Eq, Serialize)]
    pub struct ThesaurusEntry {
        pub word: String,
        pub tag: ThesaurusTag,
        pub senses: Vec<OfferSense>,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThesaurusTag {
    Splice,
    Offer,
}

impl Serialize for ThesaurusTag {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match self {
            Self::Splice => "splice",
            Self::Offer => "offer",
        })
    }
}

impl<'de> Deserialize<'de> for ThesaurusTag {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match String::deserialize(deserializer)?.as_str() {
            "splice" => Ok(Self::Splice),
            "offer" => Ok(Self::Offer),
            other => Err(serde::de::Error::unknown_variant(
                other,
                &["splice", "offer"],
            )),
        }
    }
}

super::codec::closed_object! {
    /// TYPES-LOOKUP: sense and use order is the artifact's order.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize)]
    pub struct OfferSense {
        pub sense: String,
        pub r#use: Vec<String>,
    }
}

impl OfferSense {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.sense.is_empty() || self.r#use.is_empty() || self.r#use.iter().any(String::is_empty)
        {
            return Err(ValidationError("empty thesaurus sense or use"));
        }
        Ok(())
    }
}

impl Ste100Thesaurus {
    /// The owner hashes the exact received bytes, including final whitespace.
    /// This checks shape and binding, not source provenance or language approval.
    pub fn validate(
        &self,
        actual_digest: &Digest,
        declared: &ThesaurusDeclaration,
        scheme: &SchemeDeclaration,
    ) -> Result<(), ValidationError> {
        declared.validate(scheme)?;
        if self.format != "ste100-thesaurus/1"
            || actual_digest != &declared.artifact_sha256
            || self.scheme_id != declared.scheme_id
        {
            return Err(ValidationError("thesaurus artifact identity mismatch"));
        }
        if self.source.repository.is_empty()
            || self.source.commit.is_empty()
            || self.source.path.is_empty()
        {
            return Err(ValidationError("incomplete thesaurus source"));
        }
        let mut words = BTreeSet::new();
        for entry in &self.entries {
            if entry.word.is_empty() || !words.insert(&entry.word) || entry.senses.is_empty() {
                return Err(ValidationError("invalid thesaurus entry"));
            }
            for sense in &entry.senses {
                sense.validate()?;
            }
            if entry.tag == ThesaurusTag::Splice
                && (entry.senses.len() != 1 || entry.senses[0].r#use.len() != 1)
            {
                return Err(ValidationError("ambiguous splice entry"));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "thesaurus_tests.rs"]
mod tests;
