//! Pre-agent seed construction and verified RAM images. No caller path or SQL is
//! accepted here. Publication and the member-room protocol belong to their owners.

use rusqlite::{Connection, params};
use sha2::{Digest as _, Sha256};
use weaver_types::state_flow::{
    CaptureRefusalCode, Digest, SchemeDeclaration, SeedRefusal, SeedRequest, Ste100Scheme,
    decode_strict,
};

use super::{CatalogFault, ServingCatalog};

mod adoption;
pub use adoption::{ObservedAdoption, OpenFault};

#[derive(Debug)]
pub struct CaptureFault {
    pub code: CaptureRefusalCode,
    pub detail: String,
}

impl std::fmt::Display for CaptureFault {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(out, "{:?}: {}", self.code, self.detail)
    }
}
impl std::error::Error for CaptureFault {}

impl super::ServingSqlite {
    /// Capture a complete immutable RAM image. The member may write these bytes
    /// only inside its paused capture protocol. This method opens no disk file.
    pub fn image(&self) -> Result<Vec<u8>, CaptureFault> {
        bounded_image(&self.connection, crate::save_point::SAVE_POINT_BOUND).map_err(|failure| {
            CaptureFault {
                code: if failure.code == SeedRefusal::ImageTooLarge {
                    CaptureRefusalCode::ImageTooLarge
                } else {
                    CaptureRefusalCode::SnapshotFailed
                },
                detail: failure.detail,
            }
        })
    }
}

/// A seed failure in the closed state-seed/1 vocabulary.
#[derive(Debug)]
pub struct SeedFault {
    pub code: SeedRefusal,
    pub detail: String,
}

impl SeedFault {
    fn new(code: SeedRefusal, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }
}

impl std::fmt::Display for SeedFault {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(out, "{:?}: {}", self.code, self.detail)
    }
}

impl std::error::Error for SeedFault {}

impl From<rusqlite::Error> for SeedFault {
    fn from(error: rusqlite::Error) -> Self {
        Self::new(SeedRefusal::WriteFailed, error.to_string())
    }
}

impl From<CatalogFault> for SeedFault {
    fn from(error: CatalogFault) -> Self {
        Self::new(SeedRefusal::WriteFailed, error.to_string())
    }
}

/// A complete, bounded seed image. Only seed construction produces this type.
/// The bytes grant no live filing or publication authority.
pub struct SeedImage {
    bytes: Vec<u8>,
}

impl SeedImage {
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

pub(super) fn sha256(bytes: &[u8]) -> Digest {
    let rendered = format!("{:x}", Sha256::digest(bytes));
    Digest::new(rendered).expect("SHA-256 renders exactly 64 lowercase hex digits")
}

/// Create one seed from certified bytes in a new RAM catalog. This performs
/// attributional and structural checks, never language inference. The checker
/// digest is supplied independently of the certificate in the seed request.
pub fn seed_image(request: &SeedRequest) -> Result<SeedImage, SeedFault> {
    // Do not return an artifact parse error before the prescribed schema/shape
    // checks. An invalid artifact supplies no dictionary, then validate_structure
    // reports the first applicable schema, shape or certificate refusal.
    let verbs = decode_strict::<Ste100Scheme>(&request.ste100_scheme)
        .map(|artifact| artifact.verbs.into_iter().map(|verb| verb.key).collect())
        .unwrap_or_default();
    let scheme = SchemeDeclaration {
        scheme_id: request.scheme_id.clone(),
        artifact_sha256: sha256(request.ste100_scheme.as_bytes()),
        verbs,
    };
    request
        .validate_structure(sha256, &scheme, &request.checker_artifact_sha256)
        .map_err(|code| SeedFault::new(code, "seed structure or certificate mismatch"))?;

    let mut catalog = ServingCatalog::create()?;
    {
        let tx = catalog.connection.transaction()?;
        tx.execute(
            "INSERT INTO store_meta(singleton,schema_id,seed_id,scheme_id) VALUES(1,?1,?2,?3)",
            params![request.schema_id, request.seed_id, request.scheme_id],
        )?;
        tx.execute(
            "INSERT INTO content_block(source_kind,seed_ordinal,admission,raw_block,
                statement_text,scheme_id,content_type)
             VALUES('seed',?1,'seeded',?2,?3,?4,'statement')",
            params![
                0_u64.to_be_bytes().as_slice(),
                request.raw_block.get(),
                request.prompt_text,
                request.scheme_id
            ],
        )?;
        let block = tx.last_insert_rowid();
        for (ordinal, binding) in request.bindings.iter().enumerate() {
            tx.execute(
                "INSERT INTO statement_binding(block_id,ordinal,start,end,verb,verb_start,
                    verb_end,condition,condition_start,condition_end)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                params![
                    block,
                    (ordinal as u64).to_be_bytes().as_slice(),
                    binding.statement_span.start.0.to_be_bytes().as_slice(),
                    binding.statement_span.end.0.to_be_bytes().as_slice(),
                    binding.verb,
                    binding.verb_span.start.0.to_be_bytes().as_slice(),
                    binding.verb_span.end.0.to_be_bytes().as_slice(),
                    binding.condition,
                    binding
                        .condition_span
                        .map(|span| span.start.0.to_be_bytes()),
                    binding.condition_span.map(|span| span.end.0.to_be_bytes())
                ],
            )?;
            let binding_id = tx.last_insert_rowid();
            for (noun_ordinal, noun) in binding.nouns.iter().enumerate() {
                tx.execute(
                    "INSERT INTO binding_noun(binding_id,ordinal,value,start,end)
                     VALUES(?1,?2,?3,?4,?5)",
                    params![
                        binding_id,
                        (noun_ordinal as u64).to_be_bytes().as_slice(),
                        noun.value,
                        noun.span.start.0.to_be_bytes().as_slice(),
                        noun.span.end.0.to_be_bytes().as_slice()
                    ],
                )?;
            }
        }
        tx.execute(
            "INSERT INTO span(local_id,revision,kind,status)
             VALUES(?1,?2,'system_prompt','closed')",
            params![
                0_u64.to_be_bytes().as_slice(),
                1_u64.to_be_bytes().as_slice()
            ],
        )?;
        for sql in [
            "INSERT INTO span_prefix(local_id,revision,ordinal,block_id,start,end)
             VALUES(?1,?2,?1,?3,?1,?4)",
            "INSERT INTO span_member(local_id,revision,ordinal,block_id,start,end)
             VALUES(?1,?2,?1,?3,?1,?4)",
        ] {
            tx.execute(
                sql,
                params![
                    0_u64.to_be_bytes().as_slice(),
                    1_u64.to_be_bytes().as_slice(),
                    block,
                    (request.prompt_text.len() as u64).to_be_bytes().as_slice()
                ],
            )?;
        }
        tx.commit()?;
    }
    catalog.verify_catalog()?;
    bounded_image(&catalog.connection, crate::save_point::SAVE_POINT_BOUND)
        .map(|bytes| SeedImage { bytes })
}

fn bounded_image(connection: &Connection, bound: u64) -> Result<Vec<u8>, SeedFault> {
    let image = connection
        .serialize(rusqlite::MAIN_DB)
        .map_err(|error| SeedFault::new(SeedRefusal::SnapshotFailed, error.to_string()))?;
    if image.len() as u64 > bound {
        return Err(SeedFault::new(
            SeedRefusal::ImageTooLarge,
            "RAM image exceeds bound",
        ));
    }
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(image.len())
        .map_err(|error| SeedFault::new(SeedRefusal::SnapshotFailed, error.to_string()))?;
    bytes.extend_from_slice(&image);
    Ok(bytes)
}

#[cfg(test)]
pub(super) mod tests;
