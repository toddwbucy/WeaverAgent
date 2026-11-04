//! PERSIST-FORMAT, ADMIN-STE100 and workflow 3.02 through 3.05.
//! Exact artifact bytes checked against independently supplied declarations.

use weaver_types::state_flow::{
    Digest, PersistenceCase, SchemeDeclaration, Ste100Scheme, Ste100Thesaurus,
    ThesaurusDeclaration, decode_strict,
};

use super::Fault;
use crate::seed::preparation::hash;

/// Checked bytes carried from seed inputs or an already judged save point.
/// This value grants no file custody, publication or SQLite adoption authority.
#[derive(Clone, Debug)]
pub(crate) struct GoverningFiles {
    scheme: String,
    thesaurus: Option<String>,
    scheme_sha256: Digest,
    thesaurus_sha256: Option<Digest>,
}

fn mismatch(file: &str, detail: impl std::fmt::Display) -> Fault {
    Fault::new(PersistenceCase::SchemeMismatch, format!("{file}: {detail}"))
}

impl GoverningFiles {
    /// The declaration decides presence and expected hashes. Neither is inferred
    /// from a candidate file, a certificate or another file in the candidate.
    pub(crate) fn verify(
        scheme: String,
        thesaurus: Option<String>,
        declared_scheme: &SchemeDeclaration,
        declared_thesaurus: Option<&ThesaurusDeclaration>,
    ) -> Result<Self, Fault> {
        let scheme_sha256 = hash(scheme.as_bytes());
        let artifact: Ste100Scheme =
            decode_strict(&scheme).map_err(|error| mismatch("ste100-scheme.json", error))?;
        artifact
            .validate(&scheme_sha256, declared_scheme)
            .and_then(|()| artifact.verify_blobs(hash))
            .map_err(|error| mismatch("ste100-scheme.json", error))?;
        let thesaurus_sha256 = match (thesaurus.as_ref(), declared_thesaurus) {
            (None, None) => None,
            (Some(bytes), Some(declared)) => {
                let digest = hash(bytes.as_bytes());
                let artifact: Ste100Thesaurus = decode_strict(bytes)
                    .map_err(|error| mismatch("ste100-thesaurus.json", error))?;
                artifact
                    .validate(&digest, declared, declared_scheme)
                    .map_err(|error| mismatch("ste100-thesaurus.json", error))?;
                Some(digest)
            }
            _ => {
                return Err(mismatch(
                    "ste100-thesaurus.json",
                    "election and file differ",
                ));
            }
        };
        Ok(Self {
            scheme,
            thesaurus,
            scheme_sha256,
            thesaurus_sha256,
        })
    }

    pub(crate) fn scheme(&self) -> &str {
        &self.scheme
    }

    pub(crate) fn thesaurus(&self) -> Option<&str> {
        self.thesaurus.as_deref()
    }

    pub(crate) fn scheme_sha256(&self) -> &Digest {
        &self.scheme_sha256
    }

    pub(crate) fn thesaurus_sha256(&self) -> Option<&Digest> {
        self.thesaurus_sha256.as_ref()
    }

    /// Bind and save carry the same bytes forward, checked against the new draft.
    pub(crate) fn verify_declaration(
        &self,
        scheme: &SchemeDeclaration,
        thesaurus: Option<&ThesaurusDeclaration>,
    ) -> Result<(), Fault> {
        let artifact: Ste100Scheme =
            decode_strict(&self.scheme).map_err(|error| mismatch("scheme", error))?;
        artifact
            .validate(&self.scheme_sha256, scheme)
            .map_err(|error| mismatch("scheme", error))?;
        if let Some(thesaurus) = thesaurus {
            thesaurus
                .validate(scheme)
                .map_err(|error| mismatch("thesaurus", error))?;
        }
        if self.thesaurus_sha256.as_ref() != thesaurus.map(|value| &value.artifact_sha256) {
            return Err(mismatch(
                "thesaurus",
                "declaration changes held bytes or election",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
pub(super) mod tests;
