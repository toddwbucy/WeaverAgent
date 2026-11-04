//! STATE-GOVERNING and STATE-ADOPT, workflow 3.02 through 3.05.
//! Seed owns the first versions. Serving only verifies and reads these rows.

use std::collections::BTreeMap;

use rusqlite::{Connection, Transaction, params};
use weaver_types::state_flow::{
    Digest, SchemeDeclaration, SeedRequest, StateRefusalCode, Ste100Scheme, Ste100Thesaurus,
    ThesaurusDeclaration, decode_strict,
};

use super::image::{OpenFault, SeedFault, sha256};

pub(super) type Schemes = BTreeMap<String, Vec<String>>;

fn mismatch(kind: &str, detail: impl std::fmt::Display) -> OpenFault {
    OpenFault {
        code: StateRefusalCode::SchemeMismatch,
        detail: format!("{kind}: {detail}"),
    }
}

/// Retain exact source bytes, including their original whitespace and encoding.
/// The caller has validated the request and owns the surrounding seed transaction.
pub(super) fn seed(tx: &Transaction<'_>, request: &SeedRequest) -> Result<(), SeedFault> {
    let mut versions = vec![("scheme", request.ste100_scheme.as_str(), None)];
    if let Some(bytes) = &request.ste100_thesaurus {
        let artifact: Ste100Thesaurus = decode_strict(bytes).map_err(|error| SeedFault {
            code: weaver_types::state_flow::SeedRefusal::SeedCertificateInvalid,
            detail: error.to_string(),
        })?;
        versions.push(("thesaurus", bytes.as_str(), Some(artifact.source.sha256)));
    }
    for (ordinal, (kind, bytes, source)) in versions.into_iter().enumerate() {
        let digest = sha256(bytes.as_bytes()).to_string();
        tx.execute(
            "INSERT INTO governing_version
             (kind,sha256,bytes,source_sha256,ruling,install_ordinal)
             VALUES(?1,?2,?3,?4,'seed',?5)",
            params![
                kind,
                digest,
                bytes.as_bytes(),
                source.map(|v| v.to_string()),
                (ordinal as u64).to_be_bytes()
            ],
        )?;
        tx.execute(
            "INSERT INTO governing_current(kind,sha256) VALUES(?1,?2)",
            params![kind, digest],
        )?;
    }
    Ok(())
}

/// Check all retained versions before any filing can read their dictionaries.
/// Historical thesauri name their own scheme, while the current pair must agree.
pub(super) fn verify(
    connection: &Connection,
    scheme_id: &str,
    thesaurus_digest: Option<&Digest>,
) -> Result<Schemes, OpenFault> {
    let mut schemes = BTreeMap::new();
    let mut thesauri = BTreeMap::new();
    let mut statement = connection.prepare(
        "SELECT kind,sha256,bytes,source_sha256 FROM governing_version ORDER BY kind,sha256",
    )?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let kind: String = row.get(0)?;
        let stored: String = row.get(1)?;
        let bytes: Vec<u8> = row.get(2)?;
        let source: Option<String> = row.get(3)?;
        let digest = Digest::new(&stored).map_err(|error| mismatch(&kind, error))?;
        if sha256(&bytes) != digest {
            return Err(mismatch(&kind, "version bytes differ from their digest"));
        }
        let text = std::str::from_utf8(&bytes).map_err(|error| mismatch(&kind, error))?;
        match kind.as_str() {
            "scheme" => {
                let artifact: Ste100Scheme =
                    decode_strict(text).map_err(|error| mismatch(&kind, error))?;
                let declared = SchemeDeclaration {
                    scheme_id: format!("ste100:{digest}"),
                    artifact_sha256: digest,
                    verbs: artifact.verbs.iter().map(|verb| verb.key.clone()).collect(),
                };
                artifact
                    .validate(&declared.artifact_sha256, &declared)
                    .and_then(|()| artifact.verify_blobs(sha256))
                    .map_err(|error| mismatch(&kind, error))?;
                if source.is_some() {
                    return Err(mismatch(&kind, "scheme has a source digest"));
                }
                schemes.insert(declared.scheme_id.clone(), declared);
            }
            "thesaurus" => {
                let artifact: Ste100Thesaurus =
                    decode_strict(text).map_err(|error| mismatch(&kind, error))?;
                if source.as_deref() != Some(artifact.source.sha256.as_str()) {
                    return Err(mismatch(&kind, "source digest differs from artifact"));
                }
                thesauri.insert(stored, artifact);
            }
            _ => return Err(mismatch(&kind, "unknown governing kind")),
        }
    }
    for (digest, artifact) in &thesauri {
        let scheme = schemes
            .get(&artifact.scheme_id)
            .ok_or_else(|| mismatch("thesaurus", "scheme version is absent"))?;
        let digest = Digest::new(digest).map_err(|error| mismatch("thesaurus", error))?;
        artifact
            .validate(
                &digest,
                &ThesaurusDeclaration {
                    artifact_sha256: digest.clone(),
                    scheme_id: artifact.scheme_id.clone(),
                },
                scheme,
            )
            .map_err(|error| mismatch("thesaurus", error))?;
    }
    let current = connection
        .prepare("SELECT kind,sha256 FROM governing_current")?
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<BTreeMap<_, _>>>()?;
    let scheme_digest = scheme_id
        .strip_prefix("ste100:")
        .ok_or_else(|| mismatch("scheme", "invalid open identity"))?;
    if current.get("scheme").map(String::as_str) != Some(scheme_digest)
        || !schemes.contains_key(scheme_id)
    {
        return Err(mismatch("scheme", "current version differs from open"));
    }
    if current.get("thesaurus").map(String::as_str) != thesaurus_digest.map(Digest::as_str) {
        return Err(mismatch(
            "thesaurus",
            "current version differs from election",
        ));
    }
    if let Some(digest) = thesaurus_digest {
        let artifact = thesauri
            .get(digest.as_str())
            .ok_or_else(|| mismatch("thesaurus", "current version is absent"))?;
        if artifact.scheme_id != scheme_id {
            return Err(mismatch("thesaurus", "current scheme differs from open"));
        }
    }
    Ok(schemes
        .into_iter()
        .map(|(id, scheme)| (id, scheme.verbs))
        .collect())
}
