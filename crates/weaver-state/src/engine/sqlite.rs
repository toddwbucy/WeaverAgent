//! conforms: state-store-is-a-port
//! conforms: state-indexes-built-at-load
//! conforms: state-distillate-lands-whole
//! conforms: state-serve-restricts-to-the-session
//!
//! The embedded engine, per `weaver-state-Spec` section 3: the store the
//! 2026-08-18 ruling elected, opened in memory in the member's process with
//! no file of its own since the operator's rulings of 2026-10-02 on #58, its
//! holdings reaching the disk only as a save point, behind the `sqlite`
//! feature, the one engine that stands.

use rusqlite::Connection;

use crate::store::{
    CustodyFault, Distillate, Election, ImageFacts, RecalledEvent, RunShape, Store,
};
use crate::typed::{MeasurementRow, MessageRow, PartRow, SeriesRow, Typed, served, split};

/// The embedded engine: one sqlite database in memory, per
/// `weaver-state-Spec` section 3, the store the 2026-08-18 ruling elected and
/// the rulings of 2026-10-02 took off the disk.
pub struct Sqlite {
    connection: Connection,
}

impl Sqlite {
    /// Stand the store in memory with its schema, per the Spec: the event
    /// and field tables, the typed landing's four, and the standing indexes.
    /// The election's own indexes arrive with [`Store::index_election`], read
    /// from the seam's opener, and a save point's holdings with
    /// [`Store::adopt`].
    pub fn stand() -> Result<Sqlite, CustodyFault> {
        let connection = Connection::open_in_memory()
            .map_err(|e| CustodyFault::StoreUnavailable(e.to_string()))?;
        // Durability is the save point's, per the Spec: the live store pays
        // no disk write per landing.
        connection
            .execute_batch(
                "PRAGMA journal_mode = MEMORY;
                 PRAGMA synchronous = OFF;
                 CREATE TABLE IF NOT EXISTS event (
                     id       INTEGER PRIMARY KEY,
                     session  TEXT NOT NULL,
                     run      TEXT NOT NULL,
                     turn     TEXT,
                     kind     TEXT NOT NULL,
                     sequence INTEGER NOT NULL
                 );
                 CREATE TABLE IF NOT EXISTS field (
                     event_id INTEGER NOT NULL REFERENCES event(id),
                     key      TEXT NOT NULL,
                     value    TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS event_run_turn ON event (run, turn);
                 CREATE INDEX IF NOT EXISTS event_kind_sequence ON event (kind, sequence);",
            )
            .map_err(|e| CustodyFault::StoreUnavailable(e.to_string()))?;
        connection
            .execute_batch(TYPED_SCHEMA)
            .map_err(|e| CustodyFault::StoreUnavailable(e.to_string()))?;
        Ok(Sqlite { connection })
    }
}

impl Store for Sqlite {
    /// Build the election's indexes, at load and never mid-serve, per the
    /// Spec: one partial index per elected key path, so extension within a
    /// session is rows accumulating under standing indexes.
    fn index_election(&mut self, election: &Election) -> Result<(), CustodyFault> {
        build_indexes(&self.connection, election)
    }

    /// Land one distillate, whole or not at all, per the Spec: the event
    /// row and its field rows in one transaction that rolls back entire on
    /// any failure, because a distillate held in part would be an
    /// attributable envelope over missing pairs.
    fn land(&mut self, distillate: &Distillate) -> Result<(), CustodyFault> {
        let transaction = self
            .connection
            .transaction()
            .map_err(|e| CustodyFault::LandingFailed(e.to_string()))?;
        // The two inserts ride cached statements: one prepare per schema
        // for the store's life rather than one per event, with the
        // transaction boundary unchanged.
        {
            let mut insert_event = transaction
                .prepare_cached(
                    "INSERT INTO event (session, run, turn, kind, sequence)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                )
                .map_err(|e| CustodyFault::LandingFailed(e.to_string()))?;
            insert_event
                .execute(rusqlite::params![
                    distillate.session,
                    distillate.run,
                    distillate.turn,
                    distillate.kind,
                    distillate.sequence
                ])
                .map_err(|e| CustodyFault::LandingFailed(e.to_string()))?;
        }
        let event_id = transaction.last_insert_rowid();
        // The named members land typed and the rest verbatim, per
        // `weaver-state-Spec` section 3, both inside the one transaction.
        let (typed, verbatim) = split(&distillate.kind, &distillate.pairs);
        {
            let mut insert_field = transaction
                .prepare_cached("INSERT INTO field (event_id, key, value) VALUES (?1, ?2, ?3)")
                .map_err(|e| CustodyFault::LandingFailed(e.to_string()))?;
            for (key, value) in &verbatim {
                insert_field
                    .execute(rusqlite::params![event_id, key, value])
                    .map_err(|e| CustodyFault::LandingFailed(e.to_string()))?;
            }
        }
        land_typed(&transaction, event_id, &typed)
            .map_err(|e| CustodyFault::LandingFailed(e.to_string()))?;
        transaction
            .commit()
            .map_err(|e| CustodyFault::LandingFailed(e.to_string()))
    }

    /// **Retire the declared session's holdings and record the opener in one
    /// transaction**, which is the one act the preload path adds over the
    /// first door's, per `weaver-state-Spec` section 4. The delete runs
    /// before any distillate lands, so re-running a preload replaces the
    /// session's holdings rather than appending to them, and a dead driver's
    /// prefix needs no cleanup act because the next opener is the cleanup.
    ///
    /// **The first door's path performs no retirement and gains no branch.**
    /// This is a second entry point rather than a flag on the first, so a
    /// tee's opener cannot delete holdings by taking a wrong turn.
    fn retire_and_index(&mut self, session: &str, election: &Election) -> Result<(), CustodyFault> {
        let fault = |e: rusqlite::Error| CustodyFault::LandingFailed(e.to_string());
        let transaction = self.connection.transaction().map_err(fault)?;
        // The field and typed rows go by their events' ids rather than by a
        // join, so the delete is bounded to this session and cannot reach a
        // row whose event belongs to another.
        for table in ["field", "part", "message", "series", "measurement"] {
            transaction
                .execute(
                    &format!(
                        "DELETE FROM {table} \
                         WHERE event_id IN (SELECT id FROM event WHERE session = ?1)"
                    ),
                    rusqlite::params![session],
                )
                .map_err(fault)?;
        }
        transaction
            .execute(
                "DELETE FROM event WHERE session = ?1",
                rusqlite::params![session],
            )
            .map_err(fault)?;
        // **The index build joins the delete's transaction**, per the
        // contract's same-transaction claim as the audit of 2026-08-26 read
        // it: the retirement and the opener's recording commit together or
        // not at all, so a death between them cannot leave a retired
        // session with the old election's indexes standing over it.
        build_indexes(&transaction, election)?;
        transaction.commit().map_err(fault)?;
        Ok(())
    }

    /// The replay query, per `weaver-harness-state-contract` section 2: every
    /// held event of the declared session, whole, in landing order. No kind
    /// filter and no bound, which is what separates it from `recall` - a
    /// replay reads the rendered contributions and the recorded measurements
    /// as well as the four message kinds, and a walk that skipped any of them
    /// would replay a conversation the record does not hold.
    fn replay(&self, session: &str) -> Result<Vec<RecalledEvent>, CustodyFault> {
        let fault = |e: rusqlite::Error| CustodyFault::StoreUnavailable(e.to_string());
        let mut events_query = self
            .connection
            .prepare_cached(
                "SELECT id, session, run, turn, kind, sequence FROM event
                 WHERE session = ?1
                 ORDER BY id",
            )
            .map_err(fault)?;
        let rows: Vec<(i64, String, String, Option<String>, String, i64)> = events_query
            .query_map([session], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            })
            .map_err(fault)?
            .collect::<Result<_, _>>()
            .map_err(fault)?;
        let mut replayed = Vec::with_capacity(rows.len());
        for (id, session, run, turn, kind, sequence) in rows {
            let pairs = pairs_of(&self.connection, id)?;
            replayed.push(RecalledEvent {
                session,
                run,
                turn,
                kind,
                sequence,
                pairs,
            });
        }
        Ok(replayed)
    }

    /// How many events stand, a custody fact the tests read.
    fn held(&self) -> Result<i64, CustodyFault> {
        self.connection
            .query_row("SELECT COUNT(*) FROM event", [], |row| row.get(0))
            .map_err(|e| CustodyFault::StoreUnavailable(e.to_string()))
    }

    /// The shape ask's query, per `weaver-state-Spec` section 4: the runs
    /// in first-landed order, the `id` column being custody's own order
    /// key, each carrying its kinds and their counts as the envelope
    /// spelled them. An organized envelope fact carrying no judgment about
    /// what any count means to a turn, per the three-way division.
    fn shape(&self, session: &str) -> Result<Vec<RunShape>, CustodyFault> {
        let fault = |e: rusqlite::Error| CustodyFault::StoreUnavailable(e.to_string());
        let mut runs_query = self
            .connection
            .prepare_cached(
                "SELECT run FROM event WHERE session = ?1
                 GROUP BY run ORDER BY MIN(id)",
            )
            .map_err(fault)?;
        let runs: Vec<String> = runs_query
            .query_map([session], |row| row.get(0))
            .map_err(fault)?
            .collect::<Result<_, _>>()
            .map_err(fault)?;
        let mut kinds_query = self
            .connection
            .prepare_cached(
                "SELECT kind, COUNT(*) FROM event WHERE session = ?1 AND run = ?2
                 GROUP BY kind ORDER BY kind",
            )
            .map_err(fault)?;
        let mut shaped = Vec::with_capacity(runs.len());
        for run in runs {
            let kinds: Vec<(String, i64)> = kinds_query
                .query_map([session, run.as_str()], |row| {
                    Ok((row.get(0)?, row.get(1)?))
                })
                .map_err(fault)?
                .collect::<Result<_, _>>()
                .map_err(fault)?;
            shaped.push(RunShape { run, kinds });
        }
        Ok(shaped)
    }

    /// The recall query, per `weaver-state-Spec` section 4: the event rows
    /// of the four message kinds and of `message.restored` with their field
    /// pairs, ordered by the
    /// `id` column, bounded where asked to the distinct session, run, and
    /// turn triples of the most recent turns by id, the rows outside them
    /// left unread. The bound keys the whole turn identity because a turn
    /// label recurs across runs, and a label alone would recall an older
    /// run's turn beside its namesake.
    fn recall(
        &self,
        session: &str,
        last_turns: Option<u64>,
    ) -> Result<Vec<RecalledEvent>, CustodyFault> {
        let fault = |e: rusqlite::Error| CustodyFault::StoreUnavailable(e.to_string());
        let bound: Option<Vec<(String, String, String)>> = match last_turns {
            None => None,
            Some(count) => {
                let mut turns_query = self
                    .connection
                    .prepare_cached(
                        "SELECT session, run, turn FROM (
                             SELECT session, run, turn, MAX(id) AS last FROM event
                             WHERE turn IS NOT NULL AND session = ?1
                             GROUP BY session, run, turn
                         ) ORDER BY last DESC LIMIT ?2",
                    )
                    .map_err(fault)?;
                let turns: Vec<(String, String, String)> = turns_query
                    .query_map(rusqlite::params![session, count as i64], |row| {
                        Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                    })
                    .map_err(fault)?
                    .collect::<Result<_, _>>()
                    .map_err(fault)?;
                Some(turns)
            }
        };
        let mut events_query = self
            .connection
            .prepare_cached(
                "SELECT id, session, run, turn, kind, sequence FROM event
                 WHERE session = ?1
                   AND kind IN ('message.system', 'message.user',
                                'message.assistant', 'message.tool_result',
                                'message.restored')
                 ORDER BY id",
            )
            .map_err(fault)?;
        let rows: Vec<(i64, String, String, Option<String>, String, i64)> = events_query
            .query_map([session], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            })
            .map_err(fault)?
            .collect::<Result<_, _>>()
            .map_err(fault)?;
        let mut recalled = Vec::new();
        for (id, session, run, turn, kind, sequence) in rows {
            if let (Some(kept), Some(turn_ref)) = (&bound, &turn)
                && !kept
                    .iter()
                    .any(|(s, r, t)| s == &session && r == &run && t == turn_ref)
            {
                continue;
            }
            if bound.is_some() && turn.is_none() {
                continue;
            }
            let pairs = pairs_of(&self.connection, id)?;
            recalled.push(RecalledEvent {
                session,
                run,
                turn,
                kind,
                sequence,
                pairs,
            });
        }
        Ok(recalled)
    }

    /// The turnless `message.system` rows of the session's newest run that
    /// holds any, in landing order with their pairs, per `weaver-state-Spec`
    /// section 4's identity ask: every load records the prefix it seated,
    /// so the one in force is the newest run's.
    fn identity(&self, session: &str) -> Result<Vec<RecalledEvent>, CustodyFault> {
        identity_of(&self.connection, session)
    }
    /// The whole database through the engine's serialization interface,
    /// per `weaver-state-Spec` sections 1 and 3.
    fn image(&self) -> Result<Vec<u8>, CustodyFault> {
        let data = self
            .connection
            .serialize(rusqlite::MAIN_DB)
            .map_err(|e| CustodyFault::SavePoint(format!("serialize: {e}")))?;
        Ok(data.to_vec())
    }

    /// What the image says of itself, read from a scratch copy: the engine
    /// adopts an image lazily and bytes that are no database fault on their
    /// first use, so the scratch copy must pass the engine's own check and
    /// hold the event table, and then its catalog and its last landing are
    /// read exactly as the live store's are, so the facts a stamp claims
    /// can be held against the bytes before anything is adopted.
    fn judge_image(&self, image: &[u8], session: &str) -> Result<ImageFacts, CustodyFault> {
        let fault =
            |what: &str, e: rusqlite::Error| CustodyFault::SavePoint(format!("{what}: {e}"));
        let mut probe = Connection::open_in_memory().map_err(|e| fault("probe", e))?;
        probe
            .deserialize_read_exact(rusqlite::MAIN_DB, image, image.len(), true)
            .map_err(|e| fault("deserialize", e))?;
        let verdict: String = probe
            .query_row("PRAGMA quick_check", [], |row| row.get(0))
            .map_err(|e| fault("quick_check", e))?;
        if verdict != "ok" {
            return Err(CustodyFault::SavePoint(format!("quick_check: {verdict}")));
        }
        probe
            .prepare("SELECT id FROM event LIMIT 0")
            .map_err(|e| fault("event table", e))?;
        Ok(ImageFacts {
            schema: schema_of(&probe)?,
            position: position_of(&probe)?,
            identity: identity_of(&probe, session)?,
        })
    }

    /// Replace the holdings whole with the image's, **as a commit step**, per
    /// the operator's rulings of 2026-10-05 and 2026-10-06 on #1: the image
    /// is judged on a scratch copy, every index in the election's generated
    /// form is dropped from that copy and the active election's are built in
    /// their place, so the index set after adoption is exactly this load's,
    /// and the finished copy is serialized and swapped into the live
    /// connection whole, so a failure anywhere before the swap leaves the
    /// live holdings standing and the swap itself takes an image already
    /// proven to deserialize. The statement cache is dropped after the swap
    /// because every cached statement was prepared against the holdings that
    /// left.
    fn adopt(&mut self, image: &[u8], election: &Election) -> Result<(), CustodyFault> {
        let fault =
            |what: &str, e: rusqlite::Error| CustodyFault::SavePoint(format!("{what}: {e}"));
        self.judge_image(image, "")?;
        let mut scratch = Connection::open_in_memory().map_err(|e| fault("scratch", e))?;
        scratch
            .deserialize_read_exact(rusqlite::MAIN_DB, image, image.len(), false)
            .map_err(|e| fault("deserialize", e))?;
        drop_elected_indexes(&scratch)?;
        build_indexes(&scratch, election)?;
        let finished = scratch
            .serialize(rusqlite::MAIN_DB)
            .map_err(|e| fault("serialize finished", e))?
            .to_vec();
        drop(scratch);
        self.connection
            .deserialize_read_exact(rusqlite::MAIN_DB, &finished[..], finished.len(), false)
            .map_err(|e| fault("swap", e))?;
        self.connection.flush_prepared_statement_cache();
        Ok(())
    }

    /// The schema as text, per [`schema_of`].
    fn schema(&self) -> Result<String, CustodyFault> {
        schema_of(&self.connection)
    }

    /// The position the holdings cover, per [`position_of`].
    fn position(&self) -> Result<Option<crate::save_point::Stamp>, CustodyFault> {
        position_of(&self.connection)
    }
}

/// The schema as text: every object the catalog holds with its statement, in
/// a fixed order, **exempting only an index whose statement is exactly the
/// election's generated form**, per the operator's ruling of 2026-10-05 on
/// #1, because a load's election is the load's and never the schema's, and
/// anything else under the elected prefix, a table, a trigger or an index of
/// another shape, is schema and must match. Read on the live connection and
/// on a scratch copy of an image alike, so the two compare.
fn schema_of(connection: &Connection) -> Result<String, CustodyFault> {
    let fault = |e: rusqlite::Error| CustodyFault::StoreUnavailable(e.to_string());
    let mut query = connection
        .prepare_cached(
            "SELECT type, name, sql FROM sqlite_master
             WHERE sql IS NOT NULL
             ORDER BY type, name",
        )
        .map_err(fault)?;
    let rows: Vec<(String, String, String)> = query
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .map_err(fault)?
        .collect::<Result<_, _>>()
        .map_err(fault)?;
    let mut text = String::new();
    for (kind, name, sql) in rows {
        if kind == "index" && elected_index_form(&name).as_deref() == Some(sql.as_str()) {
            continue;
        }
        text.push_str(&kind);
        text.push(' ');
        text.push_str(&name);
        text.push('\n');
        text.push_str(&sql);
        text.push('\n');
    }
    Ok(text)
}

/// The position the holdings cover: the last landed event by the `id`
/// column, custody's own order key, and the last turn its run carries by the
/// same order, read as the number of a `t-<n>` key and zero where the run
/// holds no turn. **The turn is looked up under the last event's session as
/// well as its run**, because the store holds every session and a run
/// reference is distinct only within one. Read on the live connection and on
/// a scratch copy of an image alike.
fn position_of(connection: &Connection) -> Result<Option<crate::save_point::Stamp>, CustodyFault> {
    use rusqlite::OptionalExtension;
    let fault = |e: rusqlite::Error| CustodyFault::StoreUnavailable(e.to_string());
    let last: Option<(String, String, i64)> = connection
        .prepare_cached("SELECT session, run, sequence FROM event ORDER BY id DESC LIMIT 1")
        .map_err(fault)?
        .query_row([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .optional()
        .map_err(fault)?;
    let Some((session, run, sequence)) = last else {
        return Ok(None);
    };
    let turn: Option<String> = connection
        .prepare_cached(
            "SELECT turn FROM event WHERE session = ?1 AND run = ?2 AND turn IS NOT NULL
             ORDER BY id DESC LIMIT 1",
        )
        .map_err(fault)?
        .query_row([&session, &run], |row| row.get(0))
        .optional()
        .map_err(fault)?;
    let turn = turn
        .as_deref()
        .and_then(|t| t.strip_prefix("t-"))
        .and_then(|n| n.parse::<u64>().ok())
        .unwrap_or(0);
    Ok(Some(crate::save_point::Stamp {
        run,
        sequence: u64::try_from(sequence).unwrap_or(0),
        turn,
    }))
}

/// The session's seated prefix as the `identity` ask serves it, read on the
/// live connection and on a scratch copy of an image alike.
fn identity_of(connection: &Connection, session: &str) -> Result<Vec<RecalledEvent>, CustodyFault> {
    {
        let fault = |e: rusqlite::Error| CustodyFault::StoreUnavailable(e.to_string());
        let mut events_query = connection
            .prepare_cached(
                "SELECT id, session, run, turn, kind, sequence FROM event
                 WHERE session = ?1 AND kind = 'message.system' AND turn IS NULL
                   AND run = (SELECT run FROM event
                              WHERE session = ?1 AND kind = 'message.system'
                                AND turn IS NULL
                              ORDER BY id DESC LIMIT 1)
                 ORDER BY id",
            )
            .map_err(fault)?;
        let rows: Vec<(i64, String, String, Option<String>, String, i64)> = events_query
            .query_map([session], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            })
            .map_err(fault)?
            .collect::<Result<_, _>>()
            .map_err(fault)?;
        let mut held = Vec::with_capacity(rows.len());
        for (id, session, run, turn, kind, sequence) in rows {
            let pairs = pairs_of(connection, id)?;
            held.push(RecalledEvent {
                session,
                run,
                turn,
                kind,
                sequence,
                pairs,
            });
        }
        Ok(held)
    }
}

/// The typed landing's tables, per `weaver-state-Spec` section 3: a message's
/// role and part count and its parts, a measurement's named readings and its
/// series. Each row keys on its event, and a table's absent row is a member
/// that did not land typed.
const TYPED_SCHEMA: &str = "CREATE TABLE IF NOT EXISTS message (
         event_id INTEGER PRIMARY KEY REFERENCES event(id),
         role     TEXT,
         parts    INTEGER
     );
     CREATE TABLE IF NOT EXISTS part (
         event_id  INTEGER NOT NULL REFERENCES event(id),
         ordinal   INTEGER NOT NULL,
         block     TEXT NOT NULL,
         text      TEXT,
         name      TEXT,
         arguments TEXT,
         content   TEXT
     );
     CREATE TABLE IF NOT EXISTS measurement (
         event_id   INTEGER PRIMARY KEY REFERENCES event(id),
         perplexity REAL,
         entropies  INTEGER,
         surprisals INTEGER
     );
     CREATE TABLE IF NOT EXISTS series (
         event_id INTEGER NOT NULL REFERENCES event(id),
         member   TEXT NOT NULL,
         ordinal  INTEGER NOT NULL,
         value    REAL NOT NULL
     );
     CREATE INDEX IF NOT EXISTS part_event ON part (event_id, ordinal);
     CREATE INDEX IF NOT EXISTS series_event ON series (event_id, member, ordinal);";

/// Land one event's typed rows inside the caller's transaction.
fn land_typed(
    connection: &rusqlite::Connection,
    event_id: i64,
    typed: &Typed,
) -> rusqlite::Result<()> {
    if let Some(message) = &typed.message {
        connection
            .prepare_cached("INSERT INTO message (event_id, role, parts) VALUES (?1, ?2, ?3)")?
            .execute(rusqlite::params![event_id, message.role, message.parts])?;
    }
    for part in &typed.parts {
        connection
            .prepare_cached(
                "INSERT INTO part (event_id, ordinal, block, text, name, arguments, content)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )?
            .execute(rusqlite::params![
                event_id,
                part.ordinal,
                part.block,
                part.text,
                part.name,
                part.arguments,
                part.content
            ])?;
    }
    if let Some(measurement) = &typed.measurement {
        connection
            .prepare_cached(
                "INSERT INTO measurement (event_id, perplexity, entropies, surprisals)
                 VALUES (?1, ?2, ?3, ?4)",
            )?
            .execute(rusqlite::params![
                event_id,
                measurement.perplexity,
                measurement.entropies,
                measurement.surprisals
            ])?;
    }
    for reading in &typed.series {
        connection
            .prepare_cached(
                "INSERT INTO series (event_id, member, ordinal, value) VALUES (?1, ?2, ?3, ?4)",
            )?
            .execute(rusqlite::params![
                event_id,
                reading.member,
                reading.ordinal,
                reading.value
            ])?;
    }
    Ok(())
}

/// The typed rows one event holds.
fn typed_of(connection: &rusqlite::Connection, id: i64) -> rusqlite::Result<Typed> {
    use rusqlite::OptionalExtension;
    let message = connection
        .prepare_cached("SELECT role, parts FROM message WHERE event_id = ?1")?
        .query_row([id], |row| {
            Ok(MessageRow {
                role: row.get(0)?,
                parts: row.get(1)?,
            })
        })
        .optional()?;
    let parts = connection
        .prepare_cached(
            "SELECT ordinal, block, text, name, arguments, content FROM part
             WHERE event_id = ?1 ORDER BY ordinal",
        )?
        .query_map([id], |row| {
            Ok(PartRow {
                ordinal: row.get(0)?,
                block: row.get(1)?,
                text: row.get(2)?,
                name: row.get(3)?,
                arguments: row.get(4)?,
                content: row.get(5)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    let measurement = connection
        .prepare_cached(
            "SELECT perplexity, entropies, surprisals FROM measurement WHERE event_id = ?1",
        )?
        .query_row([id], |row| {
            Ok(MeasurementRow {
                perplexity: row.get(0)?,
                entropies: row.get(1)?,
                surprisals: row.get(2)?,
            })
        })
        .optional()?;
    let series = connection
        .prepare_cached(
            "SELECT member, ordinal, value FROM series
             WHERE event_id = ?1 ORDER BY member, ordinal",
        )?
        .query_map([id], |row| {
            Ok(SeriesRow {
                member: row.get(0)?,
                ordinal: row.get(1)?,
                value: row.get(2)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(Typed {
        message,
        parts,
        measurement,
        series,
    })
}

/// The pairs one event serves, its verbatim pairs beside its typed members
/// rendered back, per `weaver-state-Spec` section 4: every answer reads an
/// event through this and nothing else.
fn pairs_of(
    connection: &rusqlite::Connection,
    id: i64,
) -> Result<Vec<(String, String)>, CustodyFault> {
    let fault = |e: rusqlite::Error| CustodyFault::StoreUnavailable(e.to_string());
    let verbatim: Vec<(String, String)> = connection
        .prepare_cached("SELECT key, value FROM field WHERE event_id = ?1")
        .map_err(fault)?
        .query_map([id], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(fault)?
        .collect::<Result<_, _>>()
        .map_err(fault)?;
    let typed = typed_of(connection, id).map_err(fault)?;
    served(verbatim, &typed).map_err(CustodyFault::StoreUnavailable)
}

/// Build the election's partial indexes on whatever holds the connection, the
/// store itself or an open transaction, so the preload path can run the build
/// inside the retirement's transaction while the first door's opener runs it
/// bare. The index name is the key path itself, hex-encoded, so a name can only
/// ever stand for one predicate: a positional name would let a later load's
/// differing election fall silently under `IF NOT EXISTS` on an earlier load's
/// name. The key is a bound-in literal within the WHERE, quoted through
/// sqlite's own quoting to keep a hostile key path from becoming SQL.
fn build_indexes(
    connection: &rusqlite::Connection,
    election: &Election,
) -> Result<(), CustodyFault> {
    for (_kind, keys) in &election.keys {
        for key in keys {
            let statement = elected_index_statement(key).replacen(
                "CREATE INDEX ",
                "CREATE INDEX IF NOT EXISTS ",
                1,
            );
            connection
                .execute(&statement, [])
                .map_err(|e| CustodyFault::StoreUnavailable(e.to_string()))?;
        }
    }
    Ok(())
}

/// Drop every index whose statement is exactly the election's generated
/// form, the exemption's own test, so what stands after the build is the
/// active election's set and never a union with the election of the load
/// that took the image.
fn drop_elected_indexes(connection: &rusqlite::Connection) -> Result<(), CustodyFault> {
    let fault = |e: rusqlite::Error| CustodyFault::SavePoint(e.to_string());
    let generated: Vec<String> = connection
        .prepare("SELECT name, sql FROM sqlite_master WHERE type = 'index' AND sql IS NOT NULL")
        .map_err(fault)?
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(fault)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(fault)?
        .into_iter()
        .filter(|(name, sql)| elected_index_form(name).as_deref() == Some(sql.as_str()))
        .map(|(name, _)| name)
        .collect();
    for name in generated {
        connection
            .execute(&format!("DROP INDEX {}", quoted_identifier(&name)), [])
            .map_err(fault)?;
    }
    Ok(())
}

/// A name as a double-quoted SQL identifier, sqlite's own doubling rule.
fn quoted_identifier(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// An elected index's name: the key path, hex-encoded, under the prefix.
fn elected_index_name(key: &str) -> String {
    use std::fmt::Write;
    let mut name = String::with_capacity(ELECTED_PREFIX.len() + key.len() * 2);
    name.push_str(ELECTED_PREFIX);
    for byte in key.bytes() {
        let _ = write!(name, "{byte:02x}");
    }
    name
}

const ELECTED_PREFIX: &str = "field_elected_";

/// The election's generated statement for one key path, as the catalog
/// stores it: the engine keeps the statement text and drops the `IF NOT
/// EXISTS` clause, so this is the one form an exempt index may carry.
fn elected_index_statement(key: &str) -> String {
    format!(
        "CREATE INDEX {} ON field (key, value) WHERE key = {}",
        elected_index_name(key),
        quoted(key)
    )
}

/// The generated form an index of this name would carry, derived from the
/// name alone, or nothing where the name is not an elected index's: the
/// prefix, then the key path in hex that decodes to UTF-8.
fn elected_index_form(name: &str) -> Option<String> {
    let hex = name.strip_prefix(ELECTED_PREFIX)?;
    if hex.len() % 2 != 0 {
        return None;
    }
    let bytes: Option<Vec<u8>> = (0..hex.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&hex[at..at + 2], 16).ok())
        .collect();
    let key = String::from_utf8(bytes?).ok()?;
    Some(elected_index_statement(&key))
}

/// A string as a single-quoted SQL literal, sqlite's own doubling rule.
fn quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::*;

    /// **A distillate lands whole or not at all**, the watch the embedded
    /// engine owes since the service engine retired (`weaver-state-Spec`
    /// section 10): a failure forced inside the landing's transaction, here a
    /// trigger refusing one field row in place of the loop schema's
    /// constraint that will be the natural lever, must refuse the landing and
    /// leave the holdings unchanged, the event row rolled back with the field
    /// rows. Perturbation: commit the event before its rows and the refused
    /// landing leaves its event behind.
    #[test]
    fn a_distillate_lands_whole_or_not_at_all() {
        let mut store = Sqlite::stand().expect("opens");
        store
            .connection
            .execute_batch(
                "CREATE TRIGGER refuse_poison BEFORE INSERT ON field \
                 WHEN NEW.key = 'poison' BEGIN SELECT RAISE(ABORT, 'refused'); END;",
            )
            .expect("the lever stands");
        let count = |store: &Sqlite, sql: &str| -> i64 {
            store
                .connection
                .query_row(sql, [], |row| row.get(0))
                .expect("counts")
        };
        store
            .land(&Distillate {
                session: "s".into(),
                run: "r".into(),
                turn: None,
                kind: "load".into(),
                sequence: 0,
                pairs: vec![("kept".into(), "1".into())],
            })
            .expect("a sound distillate lands");
        let refused = store.land(&Distillate {
            session: "s".into(),
            run: "r".into(),
            turn: None,
            kind: "load".into(),
            sequence: 1,
            pairs: vec![("fine".into(), "1".into()), ("poison".into(), "2".into())],
        });
        assert!(refused.is_err(), "the forced failure refuses the landing");
        assert_eq!(
            count(&store, "SELECT COUNT(*) FROM event"),
            1,
            "the refused distillate's event rolled back with its rows"
        );
        assert_eq!(
            count(&store, "SELECT COUNT(*) FROM field"),
            1,
            "and none of its field rows stand"
        );
    }

    /// **A NUL in a message lands verbatim**, the shared test run on this
    /// engine and its tables read: the content is a `field` row and no part
    /// stands. Perturbation: drop the holdable check from `typed::split`, and
    /// this engine holds the part typed, so its count fails.
    #[test]
    fn a_nul_in_a_message_lands_verbatim() {
        let mut store = Sqlite::stand().expect("opens");
        super::super::a_nul_in_a_message_lands_verbatim_and_serves_whole(&mut store);
        let count = |sql: &str| -> i64 {
            store
                .connection
                .query_row(sql, [], |row| row.get(0))
                .expect("counts")
        };
        assert_eq!(
            count("SELECT COUNT(*) FROM field WHERE key = 'content'"),
            1,
            "the content is held verbatim"
        );
        assert_eq!(count("SELECT COUNT(*) FROM part"), 0, "no part is typed");
    }

    /// **A recorded line of each typed kind lands typed and serves what the
    /// record reads**, the shared test run on this engine and then its tables
    /// read for the typed rows. Perturbation: make the split type nothing, so
    /// every pair lands verbatim, and the answers still match while the part,
    /// message and series counts read zero, which is the test failing on the
    /// property rather than on the bytes.
    #[test]
    fn recorded_lines_land_typed() {
        let mut store = Sqlite::stand().expect("opens");
        super::super::recorded_lines_land_typed_and_serve_what_the_record_reads(&mut store);
        let count = |sql: &str| -> i64 {
            store
                .connection
                .query_row(sql, [], |row| row.get(0))
                .expect("counts")
        };
        assert_eq!(
            count("SELECT COUNT(*) FROM part"),
            3,
            "one part per message"
        );
        assert_eq!(
            count("SELECT COUNT(*) FROM message WHERE role IS NOT NULL AND parts = 1"),
            3
        );
        assert_eq!(count("SELECT COUNT(*) FROM series"), 4, "two readings each");
        assert_eq!(
            count("SELECT COUNT(*) FROM measurement WHERE perplexity IS NULL"),
            1,
            "the absent perplexity is held absent, not zero"
        );
        assert_eq!(
            count(
                "SELECT COUNT(*) FROM field \
                 WHERE key IN ('role', 'content', 'perplexity', 'entropies', 'surprisals')"
            ),
            0,
            "no typed member is also held verbatim"
        );
        assert_eq!(
            count("SELECT COUNT(*) FROM field WHERE key = 'input_tokens'"),
            2,
            "a member no Spec names lands verbatim"
        );
    }

    #[test]
    fn raw_objects_survive_the_engine_and_answers() {
        let mut store = Sqlite::stand().expect("opens");
        super::super::raw_objects_survive_the_engine_and_answers(&mut store);
    }

    /// **The identity ask serves the turnless system messages and no
    /// other**, in landing order, with the prefix's pairs. Perturbation:
    /// drop `turn IS NULL` from the query and the turned system message
    /// joins the answer; drop the kind and the user message does.
    #[test]
    fn the_identity_ask_serves_the_seated_prefix_alone() {
        let mut store = Sqlite::stand().expect("opens");
        let land = |store: &mut Sqlite, turn: Option<&str>, kind: &str, seq: i64, text: &str| {
            store
                .land(&Distillate {
                    session: "s".into(),
                    run: "r-1".into(),
                    turn: turn.map(str::to_string),
                    kind: kind.into(),
                    sequence: seq,
                    pairs: vec![
                        ("role".into(), "\"system\"".into()),
                        (
                            "content".into(),
                            format!("[{{\"type\":\"text\",\"text\":\"{text}\"}}]"),
                        ),
                    ],
                })
                .expect("lands");
        };
        land(&mut store, None, "message.system", 1, "You are Karl.");
        land(&mut store, None, "message.system", 2, "Answer briefly.");
        land(
            &mut store,
            Some("t-1"),
            "message.system",
            3,
            "inside a turn",
        );
        land(&mut store, Some("t-1"), "message.user", 4, "hello");
        let held = store.identity("s").expect("answers");
        assert_eq!(held.len(), 2);
        assert_eq!(held[0].sequence, 1);
        assert_eq!(held[1].sequence, 2);
        // A second load records the prefix it seated under its own run, and
        // the answer is that run's alone. Perturbation: drop the run
        // subquery and the answer holds three.
        store
            .land(&Distillate {
                session: "s".into(),
                run: "r-2".into(),
                turn: None,
                kind: "message.system".into(),
                sequence: 1,
                pairs: vec![
                    ("role".into(), "\"system\"".into()),
                    ("content".into(), "[]".into()),
                ],
            })
            .expect("lands");
        let newest = store.identity("s").expect("answers");
        assert_eq!(newest.len(), 1, "the newest run's prefix alone");
        assert_eq!(newest[0].run, "r-2");
        assert!(
            held.iter()
                .all(|e| e.turn.is_none() && e.kind == "message.system")
        );
        assert!(
            held[0]
                .pairs
                .contains(&("role".to_string(), "\"system\"".to_string())),
            "the seated prefix carries its role pair: {:?}",
            held[0].pairs
        );
        assert!(
            store.identity("other").expect("answers").is_empty(),
            "an empty list is an answer"
        );
        assert!(matches!(
            parse_ask("{\"ask\":{\"identity\":{}}}"),
            Some(Ask::Identity)
        ));
        let frame = render_identity_answer(&held);
        assert!(frame.starts_with("{\"answer\":{\"identity\":{\"messages\":[{\"envelope\":"));
        assert!(
            frame.contains("\"role\":\"system\""),
            "pairs render as JSON, not as strings"
        );
    }

    /// **The position is the last landing and its run's last turn**, per
    /// `weaver-state-Spec` section 3's stamp: by landing order and never by
    /// the sequence's size, zero where the run holds no turn, and none where
    /// nothing has landed. **The schema leaves the elected indexes out**, so
    /// a later load's differing election is not a schema that disagrees.
    /// Perturbation: order the position query by `sequence` and the second
    /// run's lower sequence stops being the position; include the elected
    /// indexes in the schema text and the two digests differ.
    #[test]
    fn the_position_is_the_last_landing_and_the_schema_omits_elections() {
        let mut store = Sqlite::stand().expect("stands");
        assert_eq!(store.position().expect("reads"), None, "nothing landed");
        let before = store.schema().expect("reads");
        store
            .index_election(&Election {
                all_kinds: true,
                keys: vec![("turn.closed".into(), vec!["close".into()])],
            })
            .expect("indexes");
        assert_eq!(
            store.schema().expect("reads"),
            before,
            "an elected index is a load's and not the schema's"
        );
        assert!(before.contains("table event"), "{before}");
        assert!(before.contains("index part_event"), "{before}");
        let mut turned = landed("s", "r-1", "message.user", 7);
        turned.turn = Some("t-3".into());
        store.land(&turned).expect("lands");
        store.land(&landed("s", "r-1", "flush", 9)).expect("lands");
        let stamp = store.position().expect("reads").expect("a position");
        assert_eq!(
            stamp,
            crate::save_point::Stamp {
                run: "r-1".into(),
                sequence: 9,
                turn: 3
            },
            "the last landing's run and sequence, and that run's last turn"
        );
        store.land(&landed("s", "r-2", "load", 0)).expect("lands");
        let stamp = store.position().expect("reads").expect("a position");
        assert_eq!(
            stamp,
            crate::save_point::Stamp {
                run: "r-2".into(),
                sequence: 0,
                turn: 0
            },
            "by landing order, and a run with no turn reads zero"
        );
        // Another session's run of the same name holds a turn; the stamp
        // of this session's turnless run must not borrow it. Perturbation:
        // drop `session` from the turn lookup and this reads 5.
        let mut foreign = landed("other", "r-3", "message.user", 1);
        foreign.turn = Some("t-5".into());
        store.land(&foreign).expect("lands");
        store.land(&landed("s", "r-3", "load", 0)).expect("lands");
        let stamp = store.position().expect("reads").expect("a position");
        assert_eq!(
            (stamp.run.as_str(), stamp.turn),
            ("r-3", 0),
            "the turn is the last event's session's and never a namesake run's"
        );
    }

    /// **The holdings survive the process by image and nothing else**, per
    /// `weaver-state-Spec` section 3: a store stood from the image a first
    /// store gave holds its rows, the typed rows and the elected index with
    /// them, and a failed adoption leaves the live holdings standing.
    /// Perturbation: make `adopt` ignore its error and the poisoned image
    /// empties the store, the last assertion failing.
    #[test]
    fn a_distillate_lands_whole_and_survives_by_image() {
        let mut store = Sqlite::stand().expect("opens");
        let election = Election {
            all_kinds: true,
            keys: vec![("turn.started".into(), vec!["payload.close".into()])],
        };
        store
            .index_election(&election)
            .expect("the election indexes");
        let distillate = Distillate {
            session: "alpha-1".into(),
            run: "2026-08-18T19:03:31.198Z-alpha-7d53a936e".into(),
            turn: Some("t-1".into()),
            kind: "turn.started".into(),
            sequence: 4,
            pairs: vec![("payload.close".into(), "\"clean\"".into())],
        };
        store.land(&distillate).expect("lands");
        assert_eq!(store.held().expect("held"), 1);
        let image = store.image().expect("serializes");
        drop(store);
        let mut restored = Sqlite::stand().expect("stands empty");
        assert_eq!(restored.held().expect("held"), 0);
        restored.adopt(&image, &election).expect("adopts");
        assert_eq!(
            restored.held().expect("held"),
            1,
            "holdings survive the process by save point, per the charter"
        );
        assert_eq!(
            restored.replay("alpha-1").expect("replays")[0].pairs,
            distillate.pairs,
            "served as it crossed"
        );
        let elected: i64 = restored
            .connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type = 'index' AND name LIKE 'field_elected_%'",
                [],
                |row| row.get(0),
            )
            .expect("counts");
        assert_eq!(
            elected, 1,
            "adopted under the same election, that election's index stands"
        );
        // The image says of itself what the store it came from says, the
        // elected index left out of the schema. Perturbation: read the
        // schema from the live connection in `judge_image` and the extra
        // table below goes unseen.
        let facts = restored.judge_image(&image, "alpha-1").expect("judged");
        assert_eq!(facts.schema, restored.schema().expect("schema"));
        assert_eq!(facts.position, restored.position().expect("position"));
        assert_eq!(
            facts.identity,
            restored.identity("alpha-1").expect("identity"),
            "the prefix is read from the scratch copy as the live store serves it"
        );
        let mut altered = Sqlite::stand().expect("stands");
        altered.adopt(&image, &Election::default()).expect("adopts");
        altered
            .connection
            .execute_batch("CREATE TABLE extra (x)")
            .expect("alters");
        let altered_image = altered.image().expect("serializes");
        assert_ne!(
            restored
                .judge_image(&altered_image, "alpha-1")
                .expect("judged")
                .schema,
            facts.schema,
            "an image with another catalog says so"
        );
        assert!(
            restored
                .adopt(b"not an image", &Election::default())
                .is_err(),
            "bytes that are no database are refused"
        );
        assert_eq!(
            restored.held().expect("held"),
            1,
            "and the holdings stand after the refusal"
        );
        // **The swap is a commit step**: an election whose index the engine
        // cannot build fails before anything moves, so the holdings stand,
        // the second image's row never arriving. Perturbation: swap the
        // image in before building the election and the count reads two.
        let mut second = Sqlite::stand().expect("stands");
        second.adopt(&image, &Election::default()).expect("adopts");
        second.land(&distillate).expect("lands a second row");
        let two_rows = second.image().expect("serializes");
        let poisoned = Election {
            all_kinds: true,
            keys: vec![("turn.started".into(), vec!["a\u{0}b".into()])],
        };
        assert!(
            restored.adopt(&two_rows, &poisoned).is_err(),
            "the poisoned election fails the build"
        );
        assert_eq!(
            restored.held().expect("held"),
            1,
            "and the live holdings never moved"
        );
        // **The exemption is exact**: an index in the election's generated
        // form is left out of the schema, and a table or a trigger under
        // the elected prefix, or an index of another shape, is schema.
        // Perturbation: exempt by prefix alone and the three hidden objects
        // go unseen.
        let standing = restored.schema().expect("schema");
        for hidden in [
            "CREATE TABLE field_elected_7a (x)",
            "CREATE TRIGGER field_elected_7b BEFORE INSERT ON field BEGIN SELECT 1; END",
            "CREATE UNIQUE INDEX field_elected_7c ON field (key)",
        ] {
            let mut connection = rusqlite::Connection::open_in_memory().unwrap();
            connection
                .deserialize_read_exact(rusqlite::MAIN_DB, &image[..], image.len(), false)
                .unwrap();
            connection.execute_batch(hidden).unwrap();
            let hiding = connection.serialize(rusqlite::MAIN_DB).unwrap().to_vec();
            assert_ne!(
                restored
                    .judge_image(&hiding, "alpha-1")
                    .expect("judged")
                    .schema,
                standing,
                "{hidden} is schema"
            );
        }
        assert_eq!(
            elected_index_form(&elected_index_name("payload.close")).as_deref(),
            Some(
                "CREATE INDEX field_elected_7061796c6f61642e636c6f7365 ON field (key, value) \
                 WHERE key = 'payload.close'"
            )
        );
        assert_eq!(elected_index_form("field_elected_zz"), None);
        assert_eq!(elected_index_form("other"), None);
        // **The index set after adoption is exactly the active election's**,
        // per the ruling of 2026-10-06: an image carrying another election's
        // generated indexes comes out with only this load's. Perturbation:
        // skip `drop_elected_indexes` in `adopt` and the old index stands
        // beside the new one.
        let other = Election {
            all_kinds: true,
            keys: vec![("turn.closed".into(), vec!["other.path".into()])],
        };
        let mut relected = Sqlite::stand().expect("stands");
        relected.adopt(&image, &other).expect("adopts");
        let names: Vec<String> = relected
            .connection
            .prepare("SELECT name FROM sqlite_master WHERE type = 'index' AND name LIKE 'field_elected_%' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(
            names,
            vec![elected_index_name("other.path")],
            "only the active election's index stands"
        );
    }

    /// A later load's differing election builds its own index rather than
    /// falling silently under an earlier load's name, which is what a
    /// positional index name would allow under `IF NOT EXISTS`.
    #[test]
    fn a_changed_election_builds_its_own_indexes() {
        let mut store = Sqlite::stand().expect("opens");
        let elect = |key: &str| Election {
            all_kinds: true,
            keys: vec![("turn.closed".into(), vec![key.into()])],
        };
        store.index_election(&elect("close")).expect("first");
        store.index_election(&elect("tokens")).expect("second");
        let elected: i64 = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type = 'index' AND name LIKE 'field_elected_%'",
                [],
                |row| row.get(0),
            )
            .expect("counts");
        assert_eq!(elected, 2, "each key path owns its index");
    }

    fn landed(session: &str, run: &str, kind: &str, sequence: i64) -> Distillate {
        Distillate {
            session: session.into(),
            run: run.into(),
            turn: None,
            kind: kind.into(),
            sequence,
            pairs: Vec::new(),
        }
    }

    /// **Custody answers within its session and not across it**, per
    /// `weaver-state-Spec` section 4 and `weaver-state-PRD` section 4's
    /// boundary. A store file holding more than one session is the normal
    /// case: sessions outlive runs and the file outlives sessions, so both
    /// serve queries bound to the session the opener declared.
    ///
    /// The defect this pins was invisible in exactly the way that matters.
    /// Unbounded, both queries answered over every session the file held
    /// and every answer looked well formed - a shape ask reporting a
    /// lifetime's runs as this session's, and a recall reaching a fact the
    /// operator believed a session cut had retired.
    ///
    /// Perturbation: drop any of the three `WHERE session` predicates and
    /// this fails. Dropping the shape's or the recall's event predicate
    /// surfaces the older session's run and message in the newer session's
    /// answers. Dropping the turn-selection subquery's spends the
    /// `last-turns` bound on an older session's turn and leaves the bounded
    /// recall empty - fail-closed, because the event predicate still holds,
    /// but the answer is wrong either way.
    #[test]
    fn the_answers_stay_inside_the_running_session() {
        let mut store = Sqlite::stand().expect("opens");
        store.index_election(&Election::default()).expect("indexes");

        // An earlier session's holdings, still on disk where a session cut
        // left them, and a message it may not serve into the new session.
        store
            .land(&landed("old", "r-old", "load", 0))
            .expect("lands");
        let mut stale = landed("old", "r-old", "message.user", 1);
        stale.turn = Some("t-1".into());
        stale.pairs = vec![("payload.content".into(), "\"the vault code\"".into())];
        store.land(&stale).expect("lands");

        store
            .land(&landed("new", "r-new", "load", 0))
            .expect("lands");
        let mut fresh = landed("new", "r-new", "message.user", 1);
        fresh.turn = Some("t-1".into());
        fresh.pairs = vec![("payload.content".into(), "\"hello\"".into())];
        store.land(&fresh).expect("lands");

        assert_eq!(store.held().expect("held"), 4, "the file holds both");

        let shape = store.shape("new").expect("shapes");
        assert_eq!(
            shape.len(),
            1,
            "the shape holds the running session's runs alone: {shape:?}"
        );
        assert_eq!(shape[0].run, "r-new");

        let recalled = store.recall("new", None).expect("recalls");
        assert_eq!(
            recalled.len(),
            1,
            "the recall reads the running session's messages alone: {recalled:?}"
        );
        assert_eq!(recalled[0].run, "r-new");
        assert!(
            !recalled[0].pairs.iter().any(|(_, v)| v.contains("vault")),
            "and never the retired session's content"
        );

        // A bounded recall reads the turn-selection subquery, which the
        // unbounded ask above never touches. The older session's second
        // turn lands last so it holds the highest id: unbounded by session,
        // `LIMIT 1` would elect it, and the event query - still bounded -
        // would then find no row of it to read.
        let mut later_stale = landed("old", "r-old", "message.user", 2);
        later_stale.turn = Some("t-2".into());
        later_stale.pairs = vec![("payload.content".into(), "\"the vault code again\"".into())];
        store.land(&later_stale).expect("lands");

        let bounded = store.recall("new", Some(1)).expect("recalls");
        assert_eq!(
            bounded.len(),
            1,
            "the bound selects the running session's turn, not the newest \
             turn on the file: {bounded:?}"
        );
        assert_eq!(bounded[0].run, "r-new");
        assert_eq!(bounded[0].turn.as_deref(), Some("t-1"));

        // The older session is not destroyed, only unreachable: removal is
        // section 6's open question, deliberately not this act's.
        let old_shape = store.shape("old").expect("shapes");
        assert_eq!(old_shape.len(), 1, "the older session's rows stand");
    }

    /// The shape holds the runs in first-landed order by the id column,
    /// interleaved landings included, each with its counts by kind, and
    /// the answer frame renders the contract's spelling.
    #[test]
    fn the_shape_orders_runs_by_first_landing() {
        let mut store = Sqlite::stand().expect("opens");
        for (run, kind, sequence) in [
            ("r-1", "load", 0),
            ("r-1", "turn.closed", 1),
            ("r-2", "load", 0),
            ("r-1", "turn.closed", 2),
            ("r-2", "turn.closed", 1),
        ] {
            store
                .land(&landed("s", run, kind, sequence))
                .expect("lands");
        }
        let shape = store.shape("s").expect("shapes");
        assert_eq!(shape.len(), 2);
        assert_eq!(shape[0].run, "r-1", "first landed leads");
        assert_eq!(
            shape[0].kinds,
            vec![("load".to_string(), 1), ("turn.closed".to_string(), 2)]
        );
        assert_eq!(shape[1].run, "r-2");
        let frame = render_shape_answer(&shape);
        assert!(
            frame.starts_with(r#"{"answer":{"shape":{"runs":["#),
            "{frame}"
        );
        assert!(frame.ends_with("}\n"), "{frame}");
    }

    /// The answered-against clause, in time: an ask sees every landing
    /// before it and nothing after, because the shape reads the holdings
    /// at its own position in the stream.
    #[test]
    fn an_ask_sees_the_holdings_at_its_position_and_no_more() {
        let mut store = Sqlite::stand().expect("opens");
        store.land(&landed("s", "r-1", "load", 0)).expect("lands");
        let before = store.shape("s").expect("shapes");
        assert_eq!(before[0].kinds, vec![("load".to_string(), 1)]);
        store
            .land(&landed("s", "r-1", "turn.closed", 1))
            .expect("lands");
        let after = store.shape("s").expect("shapes");
        assert_eq!(
            after[0].kinds,
            vec![("load".to_string(), 1), ("turn.closed".to_string(), 1)]
        );
        assert_eq!(before[0].kinds.len(), 1, "the earlier answer never grew");
    }

    /// The ask vocabulary is closed at two names: both are recognized,
    /// the recall's optional bound parses, and every other frame is not
    /// an ask at all.
    /// **A restore from a branch is answered with the branch's inherited
    /// conversation** (#697, answering Codex's finding on #702). The branch's
    /// record goes through the tee under an election that does not name
    /// `message.restored`, lands, and is recalled whole and bounded.
    ///
    /// Perturbation: drop `'message.restored'` from the recall query's kind
    /// list and the inherited exchange is missing from the whole answer.
    #[test]
    fn a_restore_from_a_branch_recalls_its_inherited_conversation() {
        let mut store = Sqlite::stand().expect("opens");
        for distillate in crate::store::branch_record() {
            store.land(&distillate).expect("lands");
        }
        crate::store::assert_branch_recall(
            &store.recall("s-branch", None).expect("recall"),
            &store.recall("s-branch", Some(1)).expect("bounded recall"),
        );
    }

    /// **A replay reads what a recall does not**, which is the whole reason
    /// the ask exists: `recall` serves the message kinds and a replay
    /// walks the rendered contributions and the recorded measurements too.
    /// Perturbation: give `replay` the kind filter `recall` carries and this
    /// fails on the two events it would drop.
    #[test]
    fn a_replay_reads_every_kind_and_a_recall_reads_the_messages() {
        let mut store = Sqlite::stand().expect("opens");
        for (kind, sequence) in [
            ("message.user", 1),
            ("model.request", 2),
            ("model.measurement", 3),
            ("message.assistant", 4),
        ] {
            let mut event = landed("s", "r", kind, sequence);
            event.turn = Some("t1".into());
            store.land(&event).expect("lands");
        }
        let replayed = store.replay("s").expect("replay");
        assert_eq!(replayed.len(), 4, "a replay serves every held event");
        let kinds: Vec<&str> = replayed.iter().map(|e| e.kind.as_str()).collect();
        assert_eq!(
            kinds,
            [
                "message.user",
                "model.request",
                "model.measurement",
                "message.assistant"
            ],
            "and in landing order"
        );
        let recalled = store.recall("s", None).expect("recall");
        assert_eq!(recalled.len(), 2, "where a recall serves the message kinds");
        let frame = render_replay_answer(&replayed);
        assert!(
            frame.starts_with(r#"{"answer":{"replay":{"events":["#),
            "{frame}"
        );
        assert!(frame.ends_with("}\n"), "{frame}");
    }

    /// **The retirement and the opener's indexes commit together**, per the
    /// contract's same-transaction claim as the audit of 2026-08-26 read
    /// it: a retire under a non-empty election leaves the election's index
    /// standing over the replaced holdings, and a retire whose index build
    /// fails leaves the holdings exactly as they stood, the delete rolled
    /// back with it. The failing build is bought with an election key
    /// carrying an interior NUL, which sqlite refuses as a statement.
    ///
    /// Perturbation: commit the delete before the build runs and the
    /// atomicity half fails, the holdings gone under a build that never
    /// happened.
    #[test]
    fn the_retirement_and_its_index_commit_together() {
        let mut store = Sqlite::stand().expect("opens");
        store
            .land(&landed("replayed", "r", "message.user", 1))
            .expect("lands");

        // The index half: a non-empty election's index stands after the
        // retire that carried it.
        let election = Election {
            all_kinds: true,
            keys: vec![("message.user".into(), vec!["content".into()])],
        };
        store
            .retire_and_index("replayed", &election)
            .expect("retires and indexes");
        let indexed: i64 = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index'                  AND name LIKE 'field_elected_%'",
                [],
                |row| row.get(0),
            )
            .expect("counts indexes");
        assert!(indexed >= 1, "the election's index stands");

        // The atomicity half: a build sqlite refuses rolls the delete back
        // with it.
        store
            .land(&landed("replayed", "r", "message.user", 2))
            .expect("lands again");
        let poisoned = Election {
            all_kinds: true,
            keys: vec![("message.user".into(), vec!["a\u{0}b".into()])],
        };
        assert!(
            store.retire_and_index("replayed", &poisoned).is_err(),
            "the poisoned build fails"
        );
        let held: i64 = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM event WHERE session = 'replayed'",
                [],
                |row| row.get(0),
            )
            .expect("counts holdings");
        assert_eq!(held, 1, "the holdings survive the failed build whole");
    }

    /// **The retirement is bounded to the declared session**, per the Spec:
    /// re-running a preload replaces that session's holdings and reaches no
    /// other session's rows. Perturbation: drop the `WHERE session` from
    /// either delete and the untouched session loses its events.
    #[test]
    fn the_preload_opener_retires_its_own_session_alone() {
        let mut store = Sqlite::stand().expect("opens");
        store
            .land(&landed("replayed", "r", "message.user", 1))
            .expect("lands");
        store
            .land(&landed("other", "r", "message.user", 1))
            .expect("lands");
        store
            .retire_and_index("replayed", &Election::default())
            .expect("retire");
        assert!(
            store.replay("replayed").expect("replay").is_empty(),
            "the declared session's holdings are gone"
        );
        assert_eq!(
            store.replay("other").expect("replay").len(),
            1,
            "and no other session's are"
        );
        assert_eq!(
            store.held().expect("held"),
            1,
            "the field rows go with them"
        );
    }

    #[test]
    fn the_ask_vocabulary_is_closed() {
        assert_eq!(parse_ask(r#"{"ask":{"shape":{}}}"#), Some(Ask::Shape));
        assert_eq!(
            parse_ask(r#"{"ask":{"recall":{}}}"#),
            Some(Ask::Recall { last_turns: None })
        );
        assert_eq!(
            parse_ask(r#"{"ask":{"recall":{"last-turns":3}}}"#),
            Some(Ask::Recall {
                last_turns: Some(3)
            })
        );
        // The third name, added 2026-08-24. It carries no members, so a
        // members object and a bare one parse alike and neither carries a
        // bound the way `recall` does.
        assert_eq!(parse_ask(r#"{"ask":{"replay":{}}}"#), Some(Ask::Replay));
        for not_an_ask in [
            r#"{"ask":{"summarize":{}}}"#,
            r#"{"ask":{"recall":{"last-turns":-3}}}"#,
            r#"{"ask":{"recall":{"last-turns":"three"}}}"#,
            r#"{"ask":{"recall":{"last-turns":2.5}}}"#,
            r#"{"envelope":{}}"#,
            "not json",
        ] {
            assert!(parse_ask(not_an_ask).is_none(), "{not_an_ask}");
        }
    }

    /// The bounded recall keys its turns by session, run, and turn
    /// together: a turn label recurring across runs names two different
    /// turns, and the bound must not recall the older run's events beside
    /// its namesake's.
    #[test]
    fn a_bounded_recall_keeps_colliding_turn_labels_apart() {
        let mut store = Sqlite::stand().expect("opens");
        let message = |run: &str, turn: &str, text: &str, sequence: i64| Distillate {
            session: "s".into(),
            run: run.into(),
            turn: Some(turn.into()),
            kind: "message.user".into(),
            sequence,
            pairs: vec![("content".into(), format!("\"{text}\""))],
        };
        for landing in [
            message("r-1", "t-1", "old one", 1),
            message("r-1", "t-2", "old two", 2),
            message("r-2", "t-1", "new one", 1),
            message("r-2", "t-2", "new two", 2),
        ] {
            store.land(&landing).expect("lands");
        }
        let bounded = store.recall("s", Some(2)).expect("recalls");
        let quoted: Vec<&str> = bounded
            .iter()
            .map(|event| event.pairs[0].1.as_str())
            .collect();
        assert_eq!(
            quoted,
            vec!["\"new one\"", "\"new two\""],
            "the bound keeps the newer run's turns and no namesakes"
        );
        let whole = store.recall("s", None).expect("recalls");
        assert_eq!(whole.len(), 4, "the unbounded recall reads every message");
    }

    /// The parse demands the envelope whole: a frame missing any envelope
    /// member is nobody's row.
    #[test]
    fn an_unattributable_frame_is_refused() {
        assert!(
            parse_distillate(
                r#"{"envelope":{"session":"s","run":"r","kind":"load","sequence":"0"}}"#
            )
            .is_some()
        );
        for missing in [
            r#"{"envelope":{"run":"r","kind":"load","sequence":"0"}}"#,
            r#"{"envelope":{"session":"s","kind":"load","sequence":"0"}}"#,
            r#"{"envelope":{"session":"s","run":"r","sequence":"0"}}"#,
            r#"{"envelope":{"session":"s","run":"r","kind":"load"}}"#,
            r#"{"pairs":{}}"#,
            "not json",
        ] {
            assert!(parse_distillate(missing).is_none(), "{missing} must refuse");
        }
    }
}
