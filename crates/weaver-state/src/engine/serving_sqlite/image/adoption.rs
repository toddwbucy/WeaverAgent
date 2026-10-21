use super::sha256;
use crate::engine::serving_sqlite::{
    CatalogFault, ServingContext, ServingSqlite, configure, filing, open_memory, reads,
    verify_integrity, verify_json_tables, verify_schema,
};
use rusqlite::{Connection, Row};
use serde_json::value::RawValue;
use weaver_types::state_flow::*;

mod pairs;

#[derive(Debug)]
pub struct OpenFault {
    pub code: StateRefusalCode,
    pub detail: String,
}

impl std::fmt::Display for OpenFault {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(out, "{:?}: {}", self.code, self.detail)
    }
}
impl std::error::Error for OpenFault {}
impl From<rusqlite::Error> for OpenFault {
    fn from(error: rusqlite::Error) -> Self {
        invalid(error.to_string())
    }
}
impl From<CatalogFault> for OpenFault {
    fn from(error: CatalogFault) -> Self {
        invalid(error.to_string())
    }
}

fn invalid(detail: impl Into<String>) -> OpenFault {
    OpenFault {
        code: StateRefusalCode::OpenFailed,
        detail: detail.into(),
    }
}

macro_rules! decode {
    ($row:expr, $column:expr, $ty:ty) => {{
        let bytes: String = $row.get($column)?;
        decode_strict::<$ty>(&bytes).map_err(|_| invalid(concat!("invalid stored ", $column)))?
    }};
}
use decode;

impl ServingSqlite {
    /// Verify and adopt one raw SQLite image into a fresh private RAM connection.
    /// Admin supplies the verified start. This checks A and held facts, not B or
    /// operator authorization. Failure never exposes a partially adopted store.
    pub fn adopt(
        image: &[u8],
        common: &Common,
        schema_id: &str,
        scheme_id: &str,
        start: &StartIdentity,
        read_limits: ReadLimits,
    ) -> Result<Self, OpenFault> {
        if schema_id != STATE_CONTENT_SCHEMA {
            return Err(OpenFault {
                code: StateRefusalCode::UnsupportedSchema,
                detail: "unsupported serving schema".into(),
            });
        }
        read_limits.validate().map_err(|error| OpenFault {
            code: StateRefusalCode::InvalidReadLimits,
            detail: error.to_string(),
        })?;
        common
            .validate()
            .map_err(|error| invalid(error.to_string()))?;
        if image.is_empty()
            || image.len() as u64 > crate::save_point::SAVE_POINT_BOUND
            || sha256(image) != start.dump_sha256
        {
            return Err(invalid("image size or digest mismatch"));
        }
        let mut connection = open_memory()?;
        connection.deserialize_read_exact(rusqlite::MAIN_DB, image, image.len(), false)?;
        lift_memory_limit(&connection)?;
        configure(&connection)?;
        verify_schema(&connection)?;
        verify_integrity(&connection)?;
        // json_each is first used only after final schema adoption.
        verify_json_tables(&connection)?;
        let count: i64 =
            connection.query_row("SELECT COUNT(*) FROM store_meta", [], |r| r.get(0))?;
        if count != 1 {
            return Err(invalid("store_meta must contain exactly one row"));
        }
        let (stored_schema, seed, scheme): (String, String, String) = connection.query_row(
            "SELECT schema_id,seed_id,scheme_id FROM store_meta WHERE singleton=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        if scheme != scheme_id {
            return Err(OpenFault {
                code: StateRefusalCode::SchemeMismatch,
                detail: "image scheme differs from open".into(),
            });
        }
        if stored_schema != schema_id
            || seed != start.seed_id
            || !seed_id_valid(&seed)
            || scheme
                .strip_prefix("ste100:")
                .and_then(|d| Digest::new(d).ok())
                .is_none()
        {
            return Err(invalid("image metadata mismatch"));
        }
        verify_content(&connection, &scheme)?;
        pairs::verify(&connection, start)?;
        filing::marks::verify_image_marks(&connection, &seed)
            .map_err(|error| invalid(error.to_string()))?;
        filing::plan::verify_image_plan(&connection, &seed, &read_limits)
            .map_err(|error| invalid(error.to_string()))?;
        reads::verify_image_observations(&connection)
            .map_err(|error| invalid(format!("stored observation: {error:?}")))?;
        let position =
            reads::position(&connection).map_err(|error| invalid(format!("{error:?}")))?;
        if position != start.position {
            return Err(invalid("image source frontier mismatch"));
        }
        Ok(Self {
            connection,
            context: ServingContext {
                session_id: common.session_id.clone(),
                run_id: common.run_id.clone(),
                scheme_id: scheme,
                seed_id: seed,
                start: start.clone(),
            },
            read_limits,
            #[cfg(test)]
            filing_hooks: None,
        })
    }
}

fn seed_id_valid(seed: &str) -> bool {
    seed.len() == 32
        && seed
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn lift_memory_limit(connection: &Connection) -> Result<(), OpenFault> {
    let mut limit: i64 = i64::MAX;
    // SAFETY: the live private connection and the sqlite3_int64 argument remain
    // valid for this synchronous call. The control names its main RAM database.
    let code = unsafe {
        rusqlite::ffi::sqlite3_file_control(
            connection.handle(),
            c"main".as_ptr(),
            rusqlite::ffi::SQLITE_FCNTL_SIZE_LIMIT,
            (&mut limit as *mut i64).cast(),
        )
    };
    if code != rusqlite::ffi::SQLITE_OK || limit != i64::MAX {
        return Err(invalid("adopted RAM image size limit did not lift"));
    }
    Ok(())
}

fn counter(row: &Row<'_>, name: &str) -> Result<DecimalU64, OpenFault> {
    let bytes: Vec<u8> = row.get(name)?;
    Ok(u64::from_be_bytes(
        bytes
            .try_into()
            .map_err(|_| invalid("invalid counter width"))?,
    )
    .into())
}
fn span(row: &Row<'_>, start: &str, end: &str) -> Result<Span, OpenFault> {
    Ok(Span {
        start: counter(row, start)?,
        end: counter(row, end)?,
    })
}

fn bindings(connection: &Connection, id: i64) -> Result<Vec<Binding>, OpenFault> {
    let mut stmt =
        connection.prepare("SELECT * FROM statement_binding WHERE block_id=?1 ORDER BY ordinal")?;
    let mut rows = stmt.query([id])?;
    let mut bindings = Vec::new();
    while let Some(row) = rows.next()? {
        if counter(row, "ordinal")?.0 != bindings.len() as u64 {
            return Err(invalid("binding ordinal gap"));
        }
        let binding_id: i64 = row.get("id")?;
        if binding_id <= 0 {
            return Err(invalid("invalid binding ID"));
        }
        let mut stmt = connection
            .prepare("SELECT * FROM binding_noun WHERE binding_id=?1 ORDER BY ordinal")?;
        let mut noun_rows = stmt.query([binding_id])?;
        let mut nouns = Vec::new();
        while let Some(noun) = noun_rows.next()? {
            if counter(noun, "ordinal")?.0 != nouns.len() as u64 {
                return Err(invalid("noun ordinal gap"));
            }
            nouns.push(NounBinding {
                value: noun.get("value")?,
                span: span(noun, "start", "end")?,
            });
        }
        let condition: Option<String> = row.get("condition")?;
        let condition_span = if condition.is_some() {
            Some(span(row, "condition_start", "condition_end")?)
        } else {
            None
        };
        bindings.push(Binding {
            statement_span: span(row, "start", "end")?,
            verb: row.get("verb")?,
            verb_span: span(row, "verb_start", "verb_end")?,
            condition,
            condition_span,
            nouns,
        });
    }
    Ok(bindings)
}

fn source(row: &Row<'_>) -> Result<(SourceBlockId, SourceEnvelope), OpenFault> {
    let block_id = SourceBlockId {
        run: row.get("run")?,
        sequence: counter(row, "sequence")?,
        block: counter(row, "block")?,
    };
    let envelope: SourceEnvelope = decode!(row, "envelope", SourceEnvelope);
    let session: String = row.get("session")?;
    let turn: Option<String> = row.get("turn")?;
    let kind: String = row.get("kind")?;
    let channel: String = row.get("channel")?;
    let origin: String = row.get("origin")?;
    if envelope.session != session
        || envelope.turn != turn
        || envelope.kind != kind
        || envelope.channel != channel
        || envelope.origin != origin
    {
        return Err(invalid("stored envelope disagrees with columns"));
    }
    Ok((block_id, envelope))
}

fn verify_content(connection: &Connection, scheme: &str) -> Result<(), OpenFault> {
    let mut stmt = connection.prepare("SELECT * FROM content_block ORDER BY id")?;
    let mut rows = stmt.query([])?;
    let mut seed_seen = false;
    let mut seen_any = false;
    while let Some(row) = rows.next()? {
        let id: i64 = row.get("id")?;
        if id <= 0 {
            return Err(invalid("invalid content ID"));
        }
        let stored_scheme: String = row.get("scheme_id")?;
        if stored_scheme != scheme {
            return Err(invalid("mixed content scheme"));
        }
        let raw: Box<RawValue> = decode!(row, "raw_block", Box<RawValue>);
        let source_kind: String = row.get("source_kind")?;
        let content_type: String = row.get("content_type")?;
        if source_kind == "seed" {
            if seed_seen
                || seen_any
                || counter(row, "seed_ordinal")?.0 != 0
                || content_type != "statement"
            {
                return Err(invalid("invalid seed source"));
            }
            seed_seen = true;
            let text: String = row.get("statement_text")?;
            let fields: std::collections::BTreeMap<String, Box<RawValue>> =
                decode_strict(raw.get()).map_err(|_| invalid("invalid seed Text"))?;
            if fields.len() != 2
                || fields
                    .get("type")
                    .and_then(|v| decode_strict::<String>(v.get()).ok())
                    .as_deref()
                    != Some("text")
                || fields
                    .get("text")
                    .and_then(|v| decode_strict::<String>(v.get()).ok())
                    .as_deref()
                    != Some(&text)
            {
                return Err(invalid("seed bytes differ from decoded text"));
            }
            let bindings = bindings(connection, id)?;
            let verbs = bindings.iter().map(|b| b.verb.clone()).collect::<Vec<_>>();
            validate_bindings(&bindings, &text, &verbs).map_err(|e| invalid(e.to_string()))?;
        } else if content_type == "statement" {
            let (block_id, source) = source(row)?;
            let bindings = bindings(connection, id)?;
            let verbs = bindings.iter().map(|b| b.verb.clone()).collect::<Vec<_>>();
            let context_raw: Option<String> = row.get("context_raw")?;
            let context = context_raw
                .map(|raw| -> Result<SourceContext, OpenFault> {
                    Ok(SourceContext {
                        raw: decode_strict(&raw).map_err(|_| invalid("invalid source context"))?,
                        source_path: decode!(row, "context_path", Vec<SourcePathPart>),
                    })
                })
                .transpose()?;
            let effect: Option<String> = row.get("effect")?;
            let effect = effect
                .map(|v| {
                    decode_strict::<StatementEffect>(&v)
                        .map_err(|_| invalid("invalid stored effect"))
                })
                .transpose()?;
            let filing = StatementFile {
                block_id,
                source,
                partition_rule_id: row.get("rule_id")?,
                source_path: decode!(row, "source_path", Vec<SourcePathPart>),
                admission: Admission::Admitted,
                raw_block: raw,
                text: row.get("statement_text")?,
                scheme_id: stored_scheme,
                bindings,
                context,
                effect,
            };
            filing
                .validate_structure(&verbs)
                .map_err(|e| invalid(e.to_string()))?;
            filing::validate_effect_license(&filing).map_err(|e| invalid(e.to_string()))?;
        } else if content_type == "pair" {
            let (block_id, source) = source(row)?;
            let kind: String = connection.query_row(
                "SELECT record_type FROM pair_event WHERE id=?1",
                [id],
                |r| r.get(0),
            )?;
            let pair = match kind.as_str() {
                "started" => PairRecord::Started(
                    decode_strict(raw.get()).map_err(|_| invalid("invalid pair start"))?,
                ),
                "ended" => PairRecord::Ended(
                    decode_strict(raw.get()).map_err(|_| invalid("invalid pair end"))?,
                ),
                _ => return Err(invalid("invalid pair event type")),
            };
            let filing = PairFile {
                block_id,
                source,
                partition_rule_id: row.get("rule_id")?,
                source_path: decode!(row, "source_path", Vec<SourcePathPart>),
                admission: Admission::Admitted,
                raw_block: raw,
                scheme_id: stored_scheme,
                pair,
            };
            filing
                .validate_structure()
                .map_err(|e| invalid(e.to_string()))?;
            let count: i64 = connection.query_row(
                "SELECT COUNT(*) FROM statement_binding WHERE block_id=?1",
                [id],
                |r| r.get(0),
            )?;
            if count != 0 {
                return Err(invalid("pair has statement bindings"));
            }
        } else {
            return Err(invalid("unknown source or content type"));
        }
        seen_any = true;
    }
    if !seed_seen {
        return Err(invalid("missing seed"));
    }
    Ok(())
}
