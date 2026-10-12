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
        let transaction = self.connection.transaction().map_err(landing_fault)?;
        // The two inserts ride cached statements: one prepare per schema
        // for the store's life rather than one per event, with the
        // transaction boundary unchanged.
        {
            let mut insert_event = transaction
                .prepare_cached(
                    "INSERT INTO event (session, run, turn, kind, sequence)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                )
                .map_err(landing_fault)?;
            insert_event
                .execute(rusqlite::params![
                    distillate.session,
                    distillate.run,
                    distillate.turn,
                    distillate.kind,
                    distillate.sequence
                ])
                .map_err(landing_fault)?;
        }
        let event_id = transaction.last_insert_rowid();
        // The named members land typed and the rest verbatim, per
        // `weaver-state-Spec` section 3, both inside the one transaction.
        let (typed, verbatim) = split(&distillate.kind, &distillate.pairs);
        {
            let mut insert_field = transaction
                .prepare_cached("INSERT INTO field (event_id, key, value) VALUES (?1, ?2, ?3)")
                .map_err(landing_fault)?;
            for (key, value) in &verbatim {
                insert_field
                    .execute(rusqlite::params![event_id, key, value])
                    .map_err(landing_fault)?;
            }
        }
        land_typed(&transaction, event_id, &typed).map_err(landing_fault)?;
        transaction.commit().map_err(landing_fault)
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
                    // A bound past the engine's integer bounds nothing less
                    // than everything, so it saturates rather than wrapping
                    // into a negative limit the engine reads as none.
                    .query_map(
                        rusqlite::params![session, i64::try_from(count).unwrap_or(i64::MAX)],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                    )
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
        // Test-only hook kept here (#118): counts judgments for the restore test.
        #[cfg(test)]
        JUDGED.with(|count| count.set(count.get() + 1));
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
    /// left. **The image is the one `judge_image` passed**: the member judges
    /// a save point before it commits one, so the commit does not run the
    /// engine's whole-image check a second time over up to a gibibyte (the
    /// operator's ruling of 2026-10-08 on #99, N1); an image that is no
    /// database still refuses here, at the index rebuild.
    fn adopt(&mut self, image: &[u8], election: &Election) -> Result<(), CustodyFault> {
        let fault =
            |what: &str, e: rusqlite::Error| CustodyFault::SavePoint(format!("{what}: {e}"));
        let mut scratch = Connection::open_in_memory().map_err(|e| fault("scratch", e))?;
        scratch
            .deserialize_read_exact(rusqlite::MAIN_DB, image, image.len(), false)
            .map_err(|e| fault("deserialize", e))?;
        lift_size_limit(&scratch)?;
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
        lift_size_limit(&self.connection)?;
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
    // **A value the stamp cannot represent refuses the position and never
    // defaults**: a sequence below zero, or a turn key that is not `t-<n>`,
    // is holdings this member never landed from the canonical form, and a
    // stamp reading zero over them would pass the judgment with a false
    // lineage. Zero is the reading only where the run holds no turn.
    let turn = match turn.as_deref() {
        None => 0,
        Some(key) => key
            .strip_prefix("t-")
            .and_then(|n| n.parse::<u64>().ok())
            .ok_or_else(|| CustodyFault::SavePoint(format!("turn key {key:?} is not t-<n>")))?,
    };
    let sequence = u64::try_from(sequence)
        .map_err(|_| CustodyFault::SavePoint(format!("sequence {sequence} is below zero")))?;
    Ok(Some(crate::save_point::Stamp {
        run,
        sequence,
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

/// **An adopted image grows as a first run's store does** (the operator's
/// ruling of 2026-10-08 on #99, N2): a deserialized database is SQLite's
/// in-memory file, capped at `SQLITE_MEMDB_DEFAULT_MAXSIZE`, one gibibyte,
/// which `open_in_memory`'s store never meets, so a restored run's landings
/// past it failed `SQLITE_FULL` and an index build near it refused the
/// restore. The cap is lifted to the largest the engine admits, so the save
/// point's bound in the snapshot stays the one ceiling, as on a first run.
fn lift_size_limit(connection: &Connection) -> Result<(), CustodyFault> {
    let mut limit: i64 = i64::MAX;
    // SAFETY: the handle is this live connection's, used on this thread for
    // the one call; `main` names its attached database; `limit` is the
    // `sqlite3_int64` the size-limit control reads and writes, alive across
    // the call.
    let code = unsafe {
        rusqlite::ffi::sqlite3_file_control(
            connection.handle(),
            c"main".as_ptr(),
            rusqlite::ffi::SQLITE_FCNTL_SIZE_LIMIT,
            (&mut limit as *mut i64).cast(),
        )
    };
    if code != rusqlite::ffi::SQLITE_OK || limit != i64::MAX {
        return Err(CustodyFault::SavePoint(format!(
            "the adopted image's size limit does not lift (code {code}, limit {limit})"
        )));
    }
    Ok(())
}

/// The adopted database's size limit, read without changing it.
// Test-only hook kept here (#118): beside `lift_size_limit`, whose call it mirrors.
#[cfg(test)]
fn size_limit(connection: &Connection) -> i64 {
    let mut limit: i64 = -1;
    // SAFETY: as in `lift_size_limit`; a negative argument reads the limit.
    unsafe {
        rusqlite::ffi::sqlite3_file_control(
            connection.handle(),
            c"main".as_ptr(),
            rusqlite::ffi::SQLITE_FCNTL_SIZE_LIMIT,
            (&mut limit as *mut i64).cast(),
        );
    }
    limit
}

/// **A refused landing is the schema's, a failed one the store's** (the #99
/// area 1 grade of N2): a constraint the schema stands, `SQLITE_CONSTRAINT`
/// (a `RAISE(ABORT)` trigger among them), refusing a distillate is the
/// designed outcome of `weaver-state-Spec` section 3 and answers
/// `LandingRefused`; anything else, the engine full or past its size limit,
/// an I/O fault or a damaged image, is a fault of the store and answers
/// `LandingFailed`.
fn landing_fault(e: rusqlite::Error) -> CustodyFault {
    match e.sqlite_error_code() {
        Some(rusqlite::ErrorCode::ConstraintViolation) => {
            CustodyFault::LandingRefused(e.to_string())
        }
        _ => CustodyFault::LandingFailed(e.to_string()),
    }
}

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

// Whole-image judgments on this thread, for the test that holds a restore
// to one.
// Test-only hook kept here (#118): the counter the production path above increments.
#[cfg(test)]
thread_local! {
    static JUDGED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
mod tests;
