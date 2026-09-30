//! conforms: state-store-is-a-port
//! conforms: state-distillate-lands-whole
//! conforms: state-indexes-built-at-load
//! conforms: state-serve-restricts-to-the-session
//!
//! The service engine, per `weaver-state-Spec` section 3 and the ruling of
//! 2026-09-04: one database per agent, reached over the store's unix socket
//! under the member's own account, the store's peer authentication mapping
//! that account to the role the binding declares. Behind the `postgres`
//! feature. The same two-table shape as the embedded engine, in this engine's
//! dialect, and the same port, whole.
//!
//! The live suite exercises the port against a scratch PostgreSQL database per
//! test. It is ignored by default with an explicit reason. Run it with
//! `WEAVER_STATE_TEST_PG` naming a scratch socket directory and
//! `cargo test -p weaver-state --features postgres --locked -- --ignored`.
//! The session predicates and both transaction boundaries are watched by
//! perturbation. The catalog is read live for the elected indexes. The
//! startup timing half of indexes-at-load remains outside this port suite.
//!
//! The naming answers `weaver-state-Spec` section 3's election and the
//! encoding is this act's under it, per that clause. The store truncates an
//! identifier past its stated width rather than refusing it, which is the
//! path issue #618 measured to a collision, so a name is made whole here or
//! the election is refused.

use std::cell::{RefCell, RefMut};

use postgres::{Client, GenericClient, NoTls};

use crate::store::{CustodyFault, Distillate, Election, RecalledEvent, RunShape, Store};
use crate::typed::{MeasurementRow, MessageRow, PartRow, SeriesRow, Typed, served, split};

/// The service engine. The port's asks take `&self` and the wire is a
/// stream that needs `&mut`, so the client sits behind a cell: one member
/// holds one connection and serves one ask at a time, so the cell is never
/// contended, and a contended borrow would be a defect worth the panic.
pub struct Postgres {
    client: RefCell<Client>,
    /// The width this store holds an identifier to, read from the store at
    /// open. It is a build-time constant of the server rather than of this
    /// crate, so assuming it is the assumption issue #618 was made of.
    identifier_limit: usize,
}

impl Postgres {
    /// Connect over the store's socket directory as the member's account,
    /// under the declared role and database, and stand the schema: the event
    /// and field tables, the typed landing's four, and the standing indexes. The election's
    /// own indexes arrive with [`Store::index_election`].
    pub fn open(socket_dir: &str, database: &str, role: &str) -> Result<Postgres, CustodyFault> {
        let mut client = postgres::Config::new()
            .host_path(socket_dir)
            .user(role)
            .dbname(database)
            .connect(NoTls)
            .map_err(unavailable)?;
        client
            .batch_execute(
                "CREATE TABLE IF NOT EXISTS event (
                     id       BIGSERIAL PRIMARY KEY,
                     session  TEXT NOT NULL,
                     run      TEXT NOT NULL,
                     turn     TEXT,
                     kind     TEXT NOT NULL,
                     sequence BIGINT NOT NULL
                 );
                 CREATE TABLE IF NOT EXISTS field (
                     event_id BIGINT NOT NULL REFERENCES event(id),
                     key      TEXT NOT NULL,
                     value    TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS event_run_turn ON event (run, turn);
                 CREATE INDEX IF NOT EXISTS event_kind_sequence ON event (kind, sequence);",
            )
            .map_err(unavailable)?;
        client.batch_execute(TYPED_SCHEMA).map_err(unavailable)?;
        // **The ceiling is asked for rather than assumed.** A server built
        // with another `NAMEDATALEN` truncates at another width, and a name
        // measured against the wrong number is the silent collision again.
        let stated: String = client
            .query_one("SHOW max_identifier_length", &[])
            .map_err(unavailable)?
            .get(0);
        let identifier_limit = stated.trim().parse::<usize>().map_err(|_| {
            CustodyFault::StoreUnavailable(format!(
                "the store states its identifier width as {stated:?}, which is not a number, \
                 so no elected index name can be measured"
            ))
        })?;
        Ok(Postgres {
            client: RefCell::new(client),
            identifier_limit,
        })
    }

    fn client(&self) -> RefMut<'_, Client> {
        self.client.borrow_mut()
    }
}

fn unavailable(e: postgres::Error) -> CustodyFault {
    CustodyFault::StoreUnavailable(e.to_string())
}

fn landing(e: postgres::Error) -> CustodyFault {
    CustodyFault::LandingFailed(e.to_string())
}

/// The prefix every elected index's name carries, and the reason it is not
/// the `field_elected_` the embedded engine still uses. The pre-#618 names
/// were that prefix and hex, so their space is `field_elected_[0-9a-f]+`, and
/// the encoding below passes a hex digit through as itself: elected path `ab`
/// named `field_elected_6162` under the old scheme and elected path `6162`
/// names the same string under the new one. A store carried across the two
/// elections would find the name taken, `IF NOT EXISTS` would return quietly,
/// and the standing index would carry the other path's predicate, which is
/// issue #618 raised from its own repair. A prefix the old space cannot reach
/// keeps the two disjoint. Indexes the old scheme built are not dropped here,
/// and what a load owes a name its election does not carry is an open
/// question this act names rather than answers.
const ELECTED_PREFIX: &str = "field_key_";

/// One elected key path's index name, or the refusal `weaver-state-Spec`
/// section 3 asks for where the store's identifier limit cannot hold it.
/// `limit` is the store's own `max_identifier_length`, read at open.
///
/// The encoding keeps a lowercase letter or a digit as itself and writes
/// every other byte as an underscore and two hex digits, so an underscore
/// never appears but as an escape and one name reads back to one key path.
/// Two properties come of that. The name is derived from the path rather
/// than from the key's position in the election, which is what keeps a later
/// load's differing election from falling under an earlier load's name. And
/// the name carries nothing the store folds, where passing an uppercase byte
/// through would let two paths differing only in case name one index, an
/// unquoted identifier being folded to lowercase before it is stored.
///
/// The budget is uneven and the refusal message says so: the prefix spends
/// ten, a byte inside `[a-z0-9]` spends one, and every other byte, the dots
/// of a key path among them, spends three.
fn elected_index_name(key: &str, limit: usize) -> Result<String, CustodyFault> {
    use std::fmt::Write;
    let mut name = String::with_capacity(ELECTED_PREFIX.len() + key.len() * 3);
    name.push_str(ELECTED_PREFIX);
    for byte in key.bytes() {
        match byte {
            b'a'..=b'z' | b'0'..=b'9' => name.push(char::from(byte)),
            _ => {
                let _ = write!(name, "_{byte:02x}");
            }
        }
    }
    if name.len() > limit {
        return Err(CustodyFault::StoreUnavailable(format!(
            "the elected key path {key:?} of {} bytes names an index of {} bytes and this \
             store holds an identifier to {limit}, the name spending {} on its prefix, one \
             on a byte inside a-z0-9 and three on every other, so the election is refused \
             rather than built in the part that fits",
            key.len(),
            name.len(),
            ELECTED_PREFIX.len()
        )));
    }
    Ok(name)
}

/// The statements one election's partial indexes are built by, every name
/// made before the first statement is issued.
///
/// The naming is its own pass because the refusal is the whole election's. A
/// name refused in the middle of the build would leave the indexes named
/// before it standing, and a subset of the election is the silent loss the
/// refusal exists to prevent, read from the other end. An execution failure
/// would leave the same subset, so both callers run the statements inside a
/// transaction and this engine's DDL rolls back with it.
///
/// A path elected under two kinds states itself once. The name and the
/// predicate are functions of the path alone, so the second statement would
/// be byte-identical and a round trip spent on an `IF NOT EXISTS` no-op.
///
/// The key is a bound-in literal within the WHERE, an index predicate taking
/// no parameter, and it is written as an escape string constant so the path
/// reads the same whatever the store's `standard_conforming_strings` says.
fn elected_index_statements(
    election: &Election,
    limit: usize,
) -> Result<Vec<String>, CustodyFault> {
    let paths: usize = election.keys.iter().map(|(_, keys)| keys.len()).sum();
    let mut statements = Vec::with_capacity(paths);
    let mut named = std::collections::HashSet::with_capacity(paths);
    for (_kind, keys) in &election.keys {
        for key in keys {
            let name = elected_index_name(key, limit)?;
            if !named.insert(name.clone()) {
                continue;
            }
            statements.push(format!(
                "CREATE INDEX IF NOT EXISTS {name} ON field (key, value) WHERE key = {}",
                quoted(key)
            ));
        }
    }
    Ok(statements)
}

/// The elected keys' partial indexes, one per elected key path, built at
/// load and never mid-serve, on whatever holds the connection: both callers
/// hand this an open transaction, so a failure part way through leaves no
/// part of the election standing.
fn build_indexes(
    executor: &mut impl GenericClient,
    election: &Election,
    limit: usize,
) -> Result<(), CustodyFault> {
    for statement in elected_index_statements(election, limit)? {
        executor.batch_execute(&statement).map_err(unavailable)?;
    }
    Ok(())
}

/// A key path as an escape string constant, `E'...'`, with the backslash and
/// the single quote both escaped.
///
/// The plain literal doubles the quote and leaves the backslash alone, which
/// holds only while the store's `standard_conforming_strings` is on. With it
/// off the store reads a backslash in an ordinary literal as an escape, so a
/// path carrying one before a quote closes the predicate early and the tail
/// of the path is parsed as statement text. An escape string constant reads
/// the backslash the same way in either setting, so the statement stops
/// depending on a server default this code does not read. The exposure is
/// not reachable from outside today, an elected path arriving in the agent's
/// own declaration rather than from a peer.
fn quoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 3);
    out.push_str("E'");
    for character in text.chars() {
        match character {
            '\\' => out.push_str("\\\\"),
            '\'' => out.push_str("\\'"),
            _ => out.push(character),
        }
    }
    out.push('\'');
    out
}

/// The typed landing's tables, per `weaver-state-Spec` section 3: a message's
/// role and part count and its parts, a measurement's named readings and its
/// series. Each row keys on its event, and a table's absent row is a member
/// that did not land typed.
const TYPED_SCHEMA: &str = "CREATE TABLE IF NOT EXISTS message (
         event_id BIGINT PRIMARY KEY REFERENCES event(id),
         role     TEXT,
         parts    BIGINT
     );
     CREATE TABLE IF NOT EXISTS part (
         event_id  BIGINT NOT NULL REFERENCES event(id),
         ordinal   BIGINT NOT NULL,
         block     TEXT NOT NULL,
         text      TEXT,
         name      TEXT,
         arguments TEXT,
         content   TEXT
     );
     CREATE TABLE IF NOT EXISTS measurement (
         event_id   BIGINT PRIMARY KEY REFERENCES event(id),
         perplexity DOUBLE PRECISION,
         entropies  BIGINT,
         surprisals BIGINT
     );
     CREATE TABLE IF NOT EXISTS series (
         event_id BIGINT NOT NULL REFERENCES event(id),
         member   TEXT NOT NULL,
         ordinal  BIGINT NOT NULL,
         value    DOUBLE PRECISION NOT NULL
     );
     CREATE INDEX IF NOT EXISTS part_event ON part (event_id, ordinal);
     CREATE INDEX IF NOT EXISTS series_event ON series (event_id, member, ordinal);";

/// Land one event's typed rows inside the caller's transaction.
fn land_typed(
    transaction: &mut postgres::Transaction<'_>,
    event_id: i64,
    typed: &Typed,
) -> Result<(), postgres::Error> {
    if let Some(message) = &typed.message {
        transaction.execute(
            "INSERT INTO message (event_id, role, parts) VALUES ($1, $2, $3)",
            &[&event_id, &message.role, &message.parts],
        )?;
    }
    for part in &typed.parts {
        transaction.execute(
            "INSERT INTO part (event_id, ordinal, block, text, name, arguments, content)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
            &[
                &event_id,
                &part.ordinal,
                &part.block,
                &part.text,
                &part.name,
                &part.arguments,
                &part.content,
            ],
        )?;
    }
    if let Some(measurement) = &typed.measurement {
        transaction.execute(
            "INSERT INTO measurement (event_id, perplexity, entropies, surprisals)
             VALUES ($1, $2, $3, $4)",
            &[
                &event_id,
                &measurement.perplexity,
                &measurement.entropies,
                &measurement.surprisals,
            ],
        )?;
    }
    for reading in &typed.series {
        transaction.execute(
            "INSERT INTO series (event_id, member, ordinal, value) VALUES ($1, $2, $3, $4)",
            &[&event_id, &reading.member, &reading.ordinal, &reading.value],
        )?;
    }
    Ok(())
}

/// The typed rows of a set of events, one read per table rather than one per
/// event, on the argument [`with_pairs`] makes for the field rows.
fn typed_of(
    client: &mut Client,
    ids: &[i64],
) -> Result<std::collections::HashMap<i64, Typed>, postgres::Error> {
    let mut typed: std::collections::HashMap<i64, Typed> = std::collections::HashMap::new();
    for row in client.query(
        "SELECT event_id, role, parts FROM message WHERE event_id = ANY($1)",
        &[&ids],
    )? {
        typed.entry(row.get(0)).or_default().message = Some(MessageRow {
            role: row.get(1),
            parts: row.get(2),
        });
    }
    for row in client.query(
        "SELECT event_id, ordinal, block, text, name, arguments, content FROM part
         WHERE event_id = ANY($1) ORDER BY event_id, ordinal",
        &[&ids],
    )? {
        typed.entry(row.get(0)).or_default().parts.push(PartRow {
            ordinal: row.get(1),
            block: row.get(2),
            text: row.get(3),
            name: row.get(4),
            arguments: row.get(5),
            content: row.get(6),
        });
    }
    for row in client.query(
        "SELECT event_id, perplexity, entropies, surprisals FROM measurement
         WHERE event_id = ANY($1)",
        &[&ids],
    )? {
        typed.entry(row.get(0)).or_default().measurement = Some(MeasurementRow {
            perplexity: row.get(1),
            entropies: row.get(2),
            surprisals: row.get(3),
        });
    }
    for row in client.query(
        "SELECT event_id, member, ordinal, value FROM series
         WHERE event_id = ANY($1) ORDER BY event_id, member, ordinal",
        &[&ids],
    )? {
        typed.entry(row.get(0)).or_default().series.push(SeriesRow {
            member: row.get(1),
            ordinal: row.get(2),
            value: row.get(3),
        });
    }
    Ok(typed)
}

/// The events of one query with their pairs, in the query's order, each
/// event's verbatim pairs beside its typed members rendered back, per
/// `weaver-state-Spec` section 4: every answer reads an event through this.
fn with_pairs(
    client: &mut Client,
    rows: Vec<postgres::Row>,
) -> Result<Vec<RecalledEvent>, CustodyFault> {
    // One read for every event's pairs rather than one per event: the
    // answer is a session's worth of rows and a round trip per event would
    // charge the ask for its own length. Grouped by event on this side, the
    // pairs of one event kept in the order the store returns them, which
    // the answer renders as a map and so does not depend on.
    let ids: Vec<i64> = rows.iter().map(|row| row.get(0)).collect();
    let mut pairs_by_event: std::collections::HashMap<i64, Vec<(String, String)>> =
        std::collections::HashMap::with_capacity(ids.len());
    for pair in client
        .query(
            "SELECT event_id, key, value FROM field WHERE event_id = ANY($1) ORDER BY event_id",
            &[&ids],
        )
        .map_err(unavailable)?
    {
        let event_id: i64 = pair.get(0);
        pairs_by_event
            .entry(event_id)
            .or_default()
            .push((pair.get(1), pair.get(2)));
    }
    let mut typed_by_event = typed_of(client, &ids).map_err(unavailable)?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let id: i64 = row.get(0);
        let pairs = served(
            pairs_by_event.remove(&id).unwrap_or_default(),
            &typed_by_event.remove(&id).unwrap_or_default(),
        )
        .map_err(CustodyFault::StoreUnavailable)?;
        out.push(RecalledEvent {
            session: row.get(1),
            run: row.get(2),
            turn: row.get(3),
            kind: row.get(4),
            sequence: row.get(5),
            pairs,
        });
    }
    Ok(out)
}

/// The kinds a recall serves: the four message kinds and a restored prefix
/// message, which a branch's record holds its inherited conversation as, per
/// `weaver-state-Spec` section 4.
const MESSAGE_KINDS: &str = "('message.system', 'message.user', 'message.assistant', \
     'message.tool_result', 'message.restored')";

impl Store for Postgres {
    fn index_election(&mut self, election: &Election) -> Result<(), CustodyFault> {
        // **The build is one transaction.** The naming pass refuses an
        // election it cannot name whole, and this is the other half of the
        // same property: a statement failing on a lock, a permission or the
        // disk would otherwise leave the indexes issued before it standing
        // and the election half built, with the same silence. The other
        // door is inside the retirement's transaction already.
        let limit = self.identifier_limit;
        let client = self.client.get_mut();
        let mut transaction = client.transaction().map_err(unavailable)?;
        build_indexes(&mut transaction, election, limit)?;
        transaction.commit().map_err(unavailable)
    }

    fn land(&mut self, distillate: &Distillate) -> Result<(), CustodyFault> {
        let client = self.client.get_mut();
        let mut transaction = client.transaction().map_err(landing)?;
        let row = transaction
            .query_one(
                "INSERT INTO event (session, run, turn, kind, sequence)
                 VALUES ($1, $2, $3, $4, $5) RETURNING id",
                &[
                    &distillate.session,
                    &distillate.run,
                    &distillate.turn,
                    &distillate.kind,
                    &distillate.sequence,
                ],
            )
            .map_err(landing)?;
        let event_id: i64 = row.get(0);
        // The named members land typed and the rest verbatim, per
        // `weaver-state-Spec` section 3, both inside the one transaction.
        let (typed, verbatim) = split(&distillate.kind, &distillate.pairs);
        for (key, value) in &verbatim {
            transaction
                .execute(
                    "INSERT INTO field (event_id, key, value) VALUES ($1, $2, $3)",
                    &[&event_id, key, value],
                )
                .map_err(landing)?;
        }
        land_typed(&mut transaction, event_id, &typed).map_err(landing)?;
        transaction.commit().map_err(landing)
    }

    fn retire_and_index(&mut self, session: &str, election: &Election) -> Result<(), CustodyFault> {
        let limit = self.identifier_limit;
        let client = self.client.get_mut();
        let mut transaction = client.transaction().map_err(landing)?;
        for table in ["field", "part", "message", "series", "measurement"] {
            transaction
                .execute(
                    &format!(
                        "DELETE FROM {table} \
                         WHERE event_id IN (SELECT id FROM event WHERE session = $1)"
                    ),
                    &[&session],
                )
                .map_err(landing)?;
        }
        transaction
            .execute("DELETE FROM event WHERE session = $1", &[&session])
            .map_err(landing)?;
        build_indexes(&mut transaction, election, limit)?;
        transaction.commit().map_err(landing)
    }

    fn replay(&self, session: &str) -> Result<Vec<RecalledEvent>, CustodyFault> {
        let mut client = self.client();
        let rows = client
            .query(
                "SELECT id, session, run, turn, kind, sequence FROM event
                 WHERE session = $1 ORDER BY id",
                &[&session],
            )
            .map_err(unavailable)?;
        with_pairs(&mut client, rows)
    }

    fn held(&self) -> Result<i64, CustodyFault> {
        let row = self
            .client()
            .query_one("SELECT COUNT(*) FROM event", &[])
            .map_err(unavailable)?;
        Ok(row.get(0))
    }

    fn shape(&self, session: &str) -> Result<Vec<RunShape>, CustodyFault> {
        let mut client = self.client();
        let runs: Vec<String> = client
            .query(
                "SELECT run FROM event WHERE session = $1 GROUP BY run ORDER BY MIN(id)",
                &[&session],
            )
            .map_err(unavailable)?
            .into_iter()
            .map(|r| r.get(0))
            .collect();
        let mut shaped = Vec::with_capacity(runs.len());
        for run in runs {
            let kinds: Vec<(String, i64)> = client
                .query(
                    "SELECT kind, COUNT(*) FROM event WHERE session = $1 AND run = $2
                     GROUP BY kind ORDER BY kind",
                    &[&session, &run],
                )
                .map_err(unavailable)?
                .into_iter()
                .map(|r| (r.get(0), r.get(1)))
                .collect();
            shaped.push(RunShape { run, kinds });
        }
        Ok(shaped)
    }

    fn recall(
        &self,
        session: &str,
        last_turns: Option<u64>,
    ) -> Result<Vec<RecalledEvent>, CustodyFault> {
        let mut client = self.client();
        let rows = match last_turns {
            None => client
                .query(
                    &format!(
                        "SELECT id, session, run, turn, kind, sequence FROM event
                         WHERE session = $1 AND kind IN {MESSAGE_KINDS} ORDER BY id"
                    ),
                    &[&session],
                )
                .map_err(unavailable)?,
            Some(count) => client
                .query(
                    &format!(
                        "SELECT e.id, e.session, e.run, e.turn, e.kind, e.sequence FROM event e
                         JOIN (
                             SELECT run, turn, MAX(id) AS last FROM event
                             WHERE session = $1 AND turn IS NOT NULL
                             GROUP BY run, turn ORDER BY last DESC LIMIT $2
                         ) t ON e.run = t.run AND e.turn = t.turn
                         WHERE e.session = $1 AND e.kind IN {MESSAGE_KINDS} ORDER BY e.id"
                    ),
                    &[&session, &(count as i64)],
                )
                .map_err(unavailable)?,
        };
        with_pairs(&mut client, rows)
    }

    /// Under the service engine the boundary is the catalog's: the role's
    /// attributes, its memberships, the database's access list, and the
    /// table grants the role holds, each one line, ordered by the store so
    /// two readings compare as text.
    fn grants(&self) -> Result<Vec<String>, CustodyFault> {
        let rows = self
            .client()
            .query(
                "SELECT line FROM (
                     SELECT 'role ' || rolname || ' super=' || rolsuper::text
                         || ' createrole=' || rolcreaterole::text
                         || ' createdb=' || rolcreatedb::text AS line
                     FROM pg_roles WHERE rolname = current_user
                     UNION ALL
                     SELECT 'member ' || r.rolname FROM pg_auth_members m
                     JOIN pg_roles r ON r.oid = m.roleid
                     WHERE m.member = (SELECT oid FROM pg_roles WHERE rolname = current_user)
                     UNION ALL
                     SELECT 'database ' || datname || ' acl=' || COALESCE(datacl::text, '')
                     FROM pg_database WHERE datname = current_database()
                     UNION ALL
                     SELECT 'table ' || table_schema || '.' || table_name || ' ' || privilege_type
                     FROM information_schema.role_table_grants
                     WHERE grantee = current_user
                 ) surface ORDER BY line",
                &[],
            )
            .map_err(unavailable)?;
        Ok(rows.into_iter().map(|r| r.get(0)).collect())
    }

    fn identity(&self, session: &str) -> Result<Vec<RecalledEvent>, CustodyFault> {
        let mut client = self.client();
        let rows = client
            .query(
                "SELECT id, session, run, turn, kind, sequence FROM event
                 WHERE session = $1 AND kind = 'message.system' AND turn IS NULL
                   AND run = (SELECT run FROM event
                              WHERE session = $1 AND kind = 'message.system'
                                AND turn IS NULL
                              ORDER BY id DESC LIMIT 1)
                 ORDER BY id",
                &[&session],
            )
            .map_err(unavailable)?;
        with_pairs(&mut client, rows)
    }
}

#[cfg(test)]
#[path = "postgres_scratch.rs"]
mod scratch;

/// Statement tests run without a server. The ignored port tests require the
/// scratch instance described by `WEAVER_STATE_TEST_PG` and fail if it is absent.
#[cfg(test)]
mod tests {
    use super::scratch::Scratch;
    use super::*;
    use crate::store::*;

    /// **A NUL in a message lands verbatim**, the shared test run on this
    /// engine and its tables read: the content is a `field` row and no part
    /// stands. Perturbation: drop the holdable check from `typed::split`, and
    /// the service engine refuses the landing while the embedded one holds the
    /// part typed, so this engine's count fails.
    #[test]
    #[ignore = "needs WEAVER_STATE_TEST_PG naming a scratch PostgreSQL socket directory; see the W5a goal"]
    fn a_nul_in_a_message_lands_verbatim() {
        let scratch = Scratch::new();
        let mut store = scratch.open();
        super::super::a_nul_in_a_message_lands_verbatim_and_serves_whole(&mut store);
        let count =
            |sql: &str| -> i64 { store.client().query_one(sql, &[]).expect("counts").get(0) };
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
    #[ignore = "needs WEAVER_STATE_TEST_PG naming a scratch PostgreSQL socket directory; see the W5a goal"]
    fn recorded_lines_land_typed() {
        let scratch = Scratch::new();
        let mut store = scratch.open();
        super::super::recorded_lines_land_typed_and_serve_what_the_record_reads(&mut store);
        let count =
            |sql: &str| -> i64 { store.client().query_one(sql, &[]).expect("counts").get(0) };
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
    #[ignore = "needs WEAVER_STATE_TEST_PG naming a scratch PostgreSQL socket directory; see the W5a goal"]
    fn raw_objects_survive_the_engine_and_answers() {
        let scratch = Scratch::new();
        let mut store = scratch.open();
        super::super::raw_objects_survive_the_engine_and_answers(&mut store);
    }

    /// The width this store states, which the suite pins explicitly so a
    /// refusal is measured against a number rather than against whatever the
    /// box the suite runs on happens to have been built with.
    const STATED: usize = 63;

    /// An election carrying the given key paths under one kind.
    fn elect(keys: &[&str]) -> Election {
        Election {
            all_kinds: true,
            keys: vec![(
                "message.assistant".into(),
                keys.iter().map(|key| (*key).to_string()).collect(),
            )],
        }
    }

    /// **The statement carries the name the path derives**, which is the
    /// property issue #618 is about and the one a test over the derivation
    /// alone does not reach: a build emitting a positional name while still
    /// calling the deriving function passes every other test in this suite.
    /// The whole statement is pinned, so the encoding, the name in the
    /// clause and the predicate move together or this fails.
    #[test]
    fn the_statement_carries_the_name_the_path_derives() {
        let built = elected_index_statements(&elect(&["message.assistant.content.text"]), STATED)
            .expect("a nameable election builds");
        assert_eq!(built.len(), 1, "one statement per elected key path");
        assert_eq!(
            built[0],
            "CREATE INDEX IF NOT EXISTS field_key_message_2eassistant_2econtent_2etext \
             ON field (key, value) WHERE key = E'message.assistant.content.text'",
            "the statement names the index from the path and filters to it"
        );
    }

    /// Issue #618's own case, which the hex encoding collided: two elected
    /// paths of thirty bytes sharing their first twenty-six, each owning its
    /// name here and each name short enough that the store stores it whole.
    #[test]
    fn two_elected_paths_never_share_a_name() {
        let text =
            elected_index_name("message.assistant.content.text", STATED).expect("names the path");
        let kind =
            elected_index_name("message.assistant.content.type", STATED).expect("names the path");
        assert_ne!(text, kind, "each elected path owns its index name");
        assert!(
            text.len() <= STATED && kind.len() <= STATED,
            "a name the store would truncate is a name two paths can share"
        );
    }

    /// The store folds an unquoted identifier to lowercase before it stores
    /// it, so a path's uppercase byte owes an escape rather than the byte.
    #[test]
    fn two_paths_differing_only_in_case_name_two_indexes() {
        let lower = elected_index_name("turn.closed", STATED).expect("names the path");
        let upper = elected_index_name("turn.Closed", STATED).expect("names the path");
        assert_ne!(
            lower, upper,
            "case is part of the path and part of the name"
        );
        assert_eq!(
            upper,
            upper.to_lowercase(),
            "the store folds nothing this name carries"
        );
    }

    /// A path the identifier limit cannot hold is refused with a named
    /// fault, rather than standing an index under a name the store truncated.
    #[test]
    fn a_name_that_cannot_be_made_is_a_refusal() {
        let long = "message.assistant.content.text.rendered.for.the.operator";
        match elected_index_name(long, STATED) {
            Err(CustodyFault::StoreUnavailable(said)) => {
                assert!(said.contains(long), "the fault names the path it refused");
                assert!(
                    said.contains("63"),
                    "the fault names the width it was measured against"
                );
            }
            other => panic!("a path over the limit is a refusal, and this answered {other:?}"),
        }
    }

    /// **The limit is the store's and not this file's.** A path a wide store
    /// names is refused by a narrow one, so a server built with another
    /// `NAMEDATALEN` refuses rather than truncating behind the name.
    #[test]
    fn the_refusal_is_measured_against_the_width_the_store_states() {
        let path = "message.assistant.content.text";
        assert!(
            elected_index_name(path, STATED).is_ok(),
            "the path fits the width this store states"
        );
        assert!(
            elected_index_name(path, 40).is_err(),
            "the same path is refused by a store that states a narrower width"
        );
    }

    /// The refusal is the election's and not one key's: an election carrying
    /// a path the limit cannot hold builds none of its indexes rather than
    /// the subset that fits.
    #[test]
    fn an_election_that_cannot_be_named_builds_nothing() {
        let held = "message.assistant.content.text";
        let over = "message.assistant.content.text.rendered.for.the.operator";
        assert!(
            elected_index_statements(&elect(&[held, over]), STATED).is_err(),
            "an election holding a path the limit cannot name is refused whole"
        );
    }

    /// A path elected under two kinds states itself once. The name and the
    /// predicate are the path's, so the second statement would be identical
    /// and the round trip would buy an `IF NOT EXISTS` no-op.
    #[test]
    fn a_path_elected_under_two_kinds_is_stated_once() {
        let election = Election {
            all_kinds: true,
            keys: vec![
                ("message.assistant".into(), vec!["content.text".into()]),
                ("message.user".into(), vec!["content.text".into()]),
            ],
        };
        let built = elected_index_statements(&election, STATED).expect("builds");
        assert_eq!(built.len(), 1, "one statement per elected key path");
    }

    /// A quote in an elected path is escaped inside the predicate rather than
    /// closing the literal.
    #[test]
    fn a_quote_in_a_path_is_escaped_in_the_predicate() {
        let built = elected_index_statements(&elect(&["a'b"]), STATED).expect("builds");
        assert_eq!(built.len(), 1);
        assert!(
            built[0].ends_with(r"WHERE key = E'a\'b'"),
            "the predicate carries the path with its quote escaped, and this reads {}",
            built[0]
        );
    }

    /// **A backslash before a quote does not close the predicate.** The plain
    /// literal this engine wrote before escaped nothing but the quote, so a
    /// store running `standard_conforming_strings` off read the backslash as
    /// an escape, the doubled quote closed the literal early and the tail of
    /// the elected path was parsed as statement text. The predicate is an
    /// escape string constant, which reads the backslash the same way under
    /// either setting.
    #[test]
    fn a_backslash_before_a_quote_does_not_close_the_predicate() {
        let built = elected_index_statements(&elect(&[r"a\'b"]), STATED).expect("builds");
        assert_eq!(built.len(), 1);
        assert_eq!(
            built[0],
            concat!(
                "CREATE INDEX IF NOT EXISTS field_key_a_5c_27b ON field (key, value) ",
                r"WHERE key = E'a\\\'b'"
            ),
            "the statement escapes both the backslash and the quote"
        );
    }

    fn elected_indexes(store: &Postgres) -> i64 {
        store.client().query_one(
            "SELECT COUNT(*) FROM pg_indexes WHERE schemaname = 'public' AND indexname LIKE 'field_key_%'",
            &[],
        ).expect("catalog indexes").get(0)
    }

    /// **The identity ask serves the turnless system messages and no
    /// other**, in landing order, with the prefix's pairs. Perturbation:
    /// drop `turn IS NULL` from the query and the turned system message
    /// joins the answer; drop the kind and the user message does.
    #[test]
    #[ignore = "needs WEAVER_STATE_TEST_PG naming a scratch PostgreSQL socket directory; see the W5a goal"]
    fn the_identity_ask_serves_the_seated_prefix_alone() {
        let scratch = Scratch::new();
        let mut store = scratch.open();
        let land = |store: &mut Postgres, turn: Option<&str>, kind: &str, seq: i64, text: &str| {
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
            "the first held event contains the role pair: {:?}",
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

    /// A good distillate lands whole, and the store
    /// reopened from disk still holds it, which is the persistence the
    /// charter rules for runs within a session.
    #[test]
    #[ignore = "needs WEAVER_STATE_TEST_PG naming a scratch PostgreSQL socket directory; see the W5a goal"]
    fn a_distillate_lands_whole_and_survives_reopen() {
        let scratch = Scratch::new();
        let mut store = scratch.open();
        store
            .index_election(&Election::default())
            .expect("default election indexes");
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
        drop(store);
        let store = scratch.open();
        assert_eq!(
            store.held().expect("held"),
            1,
            "holdings survive the process, per the charter"
        );
        assert_eq!(
            store.replay("alpha-1").expect("whole reopened holding"),
            vec![RecalledEvent {
                session: "alpha-1".into(),
                run: "2026-08-18T19:03:31.198Z-alpha-7d53a936e".into(),
                turn: Some("t-1".into()),
                kind: "turn.started".into(),
                sequence: 4,
                pairs: vec![("payload.close".into(), "\"clean\"".into())],
            }]
        );
    }

    /// A later load's differing election builds its own index rather than
    /// falling silently under an earlier load's name, which is what a
    /// positional index name would allow under `IF NOT EXISTS`.
    #[test]
    #[ignore = "needs WEAVER_STATE_TEST_PG naming a scratch PostgreSQL socket directory; see the W5a goal"]
    fn a_changed_election_builds_its_own_indexes() {
        let scratch = Scratch::new();
        let mut store = scratch.open();
        let elect = |key: &str| Election {
            all_kinds: true,
            keys: vec![("turn.closed".into(), vec![key.into()])],
        };
        store.index_election(&elect("close")).expect("first");
        store.index_election(&elect("tokens")).expect("second");
        let elected = elected_indexes(&store);
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
    /// boundary. A database holding more than one session is the normal
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
    #[ignore = "needs WEAVER_STATE_TEST_PG naming a scratch PostgreSQL socket directory; see the W5a goal"]
    fn the_answers_stay_inside_the_running_session() {
        let scratch = Scratch::new();
        let mut store = scratch.open();
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

        assert_eq!(store.held().expect("held"), 4, "the database holds both");

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
             turn in the database: {bounded:?}"
        );
        assert_eq!(bounded[0].run, "r-new");
        assert_eq!(bounded[0].turn.as_deref(), Some("t-1"));

        // Runs as well as turns may collide across sessions. Exercise the
        // shape's per-run count and the bounded recall's outer predicate.
        let mut collision = landed("old", "r-new", "message.user", 9);
        collision.turn = Some("t-1".into());
        collision.pairs = vec![("payload.content".into(), "\"excluded session\"".into())];
        store.land(&collision).expect("colliding session lands");
        assert_eq!(store.shape("new").expect("isolated counts"), shape);
        assert_eq!(
            store.recall("new", Some(1)).expect("isolated bounded rows"),
            bounded
        );

        // The older session is not destroyed, only unreachable: removal is
        // section 6's open question, deliberately not this act's.
        let old_shape = store.shape("old").expect("shapes");
        assert_eq!(old_shape.len(), 2, "the older session's rows stand");
    }

    /// The shape holds the runs in first-landed order by the id column,
    /// interleaved landings included, each with its counts by kind, and
    /// the answer frame renders the contract's spelling.
    #[test]
    #[ignore = "needs WEAVER_STATE_TEST_PG naming a scratch PostgreSQL socket directory; see the W5a goal"]
    fn the_shape_orders_runs_by_first_landing() {
        let scratch = Scratch::new();
        let mut store = scratch.open();
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
    #[ignore = "needs WEAVER_STATE_TEST_PG naming a scratch PostgreSQL socket directory; see the W5a goal"]
    fn an_ask_sees_the_holdings_at_its_position_and_no_more() {
        let scratch = Scratch::new();
        let mut store = scratch.open();
        store.land(&landed("s", "r-1", "load", 0)).expect("lands");
        assert_eq!(
            store.replay("s").expect("envelope-only cut"),
            vec![RecalledEvent {
                session: "s".into(),
                run: "r-1".into(),
                turn: None,
                kind: "load".into(),
                sequence: 0,
                pairs: vec![],
            }]
        );
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
        let mut elected = landed("s", "r-1", "message.user", 2);
        elected.turn = Some("t-1".into());
        elected.pairs = vec![("content".into(), "\"elected text\"".into())];
        store.land(&elected).expect("elected projection");
        assert_eq!(
            store.recall("s", None).expect("payload cut"),
            vec![RecalledEvent {
                session: "s".into(),
                run: "r-1".into(),
                turn: Some("t-1".into()),
                kind: "message.user".into(),
                sequence: 2,
                pairs: vec![("content".into(), "\"elected text\"".into())],
            }],
            "only the supplied election projection is recallable"
        );
        assert_eq!(before[0].kinds.len(), 1, "the earlier answer never grew");
    }

    /// **A restore from a branch is answered with the branch's inherited
    /// conversation**, the embedded engine's test at this engine (#697).
    ///
    /// Perturbation: drop `'message.restored'` from `MESSAGE_KINDS` and the
    /// inherited exchange is missing from the whole answer.
    #[test]
    #[ignore = "needs WEAVER_STATE_TEST_PG naming a scratch PostgreSQL socket directory; see the W5a goal"]
    fn a_restore_from_a_branch_recalls_its_inherited_conversation() {
        let scratch = Scratch::new();
        let mut store = scratch.open();
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
    #[ignore = "needs WEAVER_STATE_TEST_PG naming a scratch PostgreSQL socket directory; see the W5a goal"]
    fn a_replay_reads_every_kind_and_a_recall_reads_the_messages() {
        let scratch = Scratch::new();
        let mut store = scratch.open();
        for (kind, sequence) in [
            ("message.user", 1),
            ("model.request", 2),
            ("model.measurement", 3),
            ("message.assistant", 4),
            ("message.system", 5),
            ("message.tool_result", 6),
        ] {
            let mut event = landed("s", "r", kind, sequence);
            event.turn = Some("t1".into());
            store.land(&event).expect("lands");
        }
        let replayed = store.replay("s").expect("replay");
        assert_eq!(replayed.len(), 6, "a replay serves every held event");
        let kinds: Vec<&str> = replayed.iter().map(|e| e.kind.as_str()).collect();
        assert_eq!(
            kinds,
            [
                "message.user",
                "model.request",
                "model.measurement",
                "message.assistant",
                "message.system",
                "message.tool_result"
            ],
            "and in landing order"
        );
        let recalled = store.recall("s", None).expect("recall");
        assert_eq!(recalled.len(), 4, "where a recall serves the message kinds");
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
    /// carrying an interior NUL, which PostgreSQL refuses as a statement.
    ///
    /// Perturbation: commit the delete before the build runs and the
    /// atomicity half fails, the holdings gone under a build that never
    /// happened.
    #[test]
    #[ignore = "needs WEAVER_STATE_TEST_PG naming a scratch PostgreSQL socket directory; see the W5a goal"]
    fn the_retirement_and_its_index_commit_together() {
        let scratch = Scratch::new();
        let mut store = scratch.open();
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
        let indexed = elected_indexes(&store);
        assert!(indexed >= 1, "the election's index stands");

        // The atomicity half: a build PostgreSQL refuses rolls the delete back
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
        let held = store.held().expect("counts holdings");
        assert_eq!(held, 1, "the holdings survive the failed build whole");
    }

    /// **The retirement is bounded to the declared session**, per the Spec:
    /// re-running a preload replaces that session's holdings and reaches no
    /// other session's rows. Perturbation: drop the `WHERE session` from
    /// either delete and the untouched session loses its events.
    #[test]
    #[ignore = "needs WEAVER_STATE_TEST_PG naming a scratch PostgreSQL socket directory; see the W5a goal"]
    fn the_preload_opener_retires_its_own_session_alone() {
        let scratch = Scratch::new();
        let mut store = scratch.open();
        let mut original = landed("replayed", "r", "message.user", 1);
        original.turn = Some("t-1".into());
        original.pairs = vec![("content".into(), "\"old preload\"".into())];
        let mut other = original.clone();
        other.session = "other".into();
        other.pairs = vec![("content".into(), "\"other session\"".into())];
        store.land(&original).expect("old preload");
        store.land(&other).expect("other session");
        let untouched = store.replay("other").expect("other before");
        for text in ["first replacement", "second replacement"] {
            store
                .retire_and_index("replayed", &Election::default())
                .expect("retire");
            assert!(store.replay("replayed").expect("retired").is_empty());
            assert_eq!(store.held().expect("held after retire"), 1);
            original.pairs = vec![("content".into(), format!("\"{text}\""))];
            store.land(&original).expect("replacement lands");
            let replaced = store.replay("replayed").expect("replacement");
            assert_eq!(replaced.len(), 1, "a retry replaces rather than appends");
            assert_eq!(replaced[0].pairs, original.pairs);
            assert_eq!(store.replay("other").expect("other after"), untouched);
            assert_eq!(store.held().expect("total"), 2);
            let fields: i64 = store
                .client()
                .query_one("SELECT COUNT(*) FROM field", &[])
                .expect("field count")
                .get(0);
            assert_eq!(fields, 2, "retirement removes the retired pairs too");
        }
    }

    #[test]
    #[ignore = "needs WEAVER_STATE_TEST_PG naming a scratch PostgreSQL socket directory; see the W5a goal"]
    fn the_ask_vocabulary_is_closed() {
        let scratch = Scratch::new();
        let store = scratch.open();
        assert!(store.shape("s").expect("empty shape").is_empty());
        assert_eq!(parse_ask(r#"{"ask":{"grants":{}}}"#), Some(Ask::Grants));
        assert_eq!(parse_ask(r#"{"ask":{"identity":{}}}"#), Some(Ask::Identity));
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
    #[ignore = "needs WEAVER_STATE_TEST_PG naming a scratch PostgreSQL socket directory; see the W5a goal"]
    fn a_bounded_recall_keeps_colliding_turn_labels_apart() {
        let scratch = Scratch::new();
        let mut store = scratch.open();
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
    #[ignore = "needs WEAVER_STATE_TEST_PG naming a scratch PostgreSQL socket directory; see the W5a goal"]
    fn an_unattributable_frame_is_refused() {
        let scratch = Scratch::new();
        let mut store = scratch.open();
        let good = parse_distillate(
            r#"{"envelope":{"session":"s","run":"r","kind":"load","sequence":"0"}}"#,
        )
        .expect("attributable");
        store.land(&good).expect("lands");
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
            let parsed = parse_distillate(missing);
            if let Some(event) = &parsed {
                store.land(event).expect("would land an accepted frame");
            }
            assert!(parsed.is_none(), "{missing} must refuse");
            assert_eq!(store.held().expect("held"), 1);
        }
    }
    #[test]
    #[ignore = "needs WEAVER_STATE_TEST_PG naming a scratch PostgreSQL socket directory; see the W5a goal"]
    fn a_failed_pair_insert_leaves_no_partial_distillate() {
        let scratch = Scratch::new();
        let mut store = scratch.open();
        let mut good = landed("s", "r", "message.user", 1);
        good.pairs = vec![("content".into(), "\"kept\"".into())];
        store.land(&good).expect("initial holding");
        let before = store.replay("s").expect("before");
        // The event and first pair are valid. PostgreSQL TEXT rejects the
        // NUL in the second pair, after those writes have been attempted.
        let mut poisoned = landed("s", "r", "message.user", 2);
        poisoned.pairs = vec![
            ("content".into(), "\"valid first pair\"".into()),
            ("refused".into(), "a\0b".into()),
        ];
        assert!(matches!(
            store.land(&poisoned),
            Err(CustodyFault::LandingFailed(_))
        ));
        assert_eq!(
            store.held().expect("held after refusal"),
            1,
            "a failed pair insert must roll back its event"
        );
        assert_eq!(store.replay("s").expect("after"), before);
        let fields: i64 = store
            .client()
            .query_one("SELECT COUNT(*) FROM field", &[])
            .expect("field count")
            .get(0);
        assert_eq!(fields, 1, "the first pair also rolls back");
    }

    #[test]
    #[ignore = "needs WEAVER_STATE_TEST_PG naming a scratch PostgreSQL socket directory; see the W5a goal"]
    fn the_grants_ask_states_the_catalog_boundary() {
        let scratch = Scratch::new();
        let store = scratch.open();
        let surface = store.grants().expect("catalog surface");
        assert_eq!(surface, store.grants().expect("repeat reading"));
        assert!(surface.windows(2).all(|lines| lines[0] <= lines[1]));
        assert!(surface.contains(&format!(
            "role {} super=true createrole=true createdb=true",
            scratch.role
        )));
        assert!(surface.contains(&format!("database {} acl=", scratch.database)));
        assert!(
            !surface.iter().any(|line| line.starts_with("member ")),
            "the initdb role has no memberships"
        );
        for table in ["event", "field"] {
            for privilege in [
                "SELECT",
                "INSERT",
                "UPDATE",
                "DELETE",
                "TRUNCATE",
                "REFERENCES",
                "TRIGGER",
            ] {
                assert!(surface.contains(&format!("table public.{table} {privilege}")));
            }
        }
        store
            .client()
            .batch_execute("REVOKE SELECT ON field FROM CURRENT_USER")
            .expect("change scratch table grant");
        let changed = store.grants().expect("changed surface");
        let mut expected = surface.clone();
        expected.retain(|line| line != "table public.field SELECT");
        assert_eq!(changed, expected, "the surface observes the catalog change");
        assert_eq!(parse_ask(r#"{"ask":{"grants":{}}}"#), Some(Ask::Grants));
        let frame: serde_json::Value =
            serde_json::from_str(&render_grants_answer(&surface)).expect("frame");
        assert_eq!(
            frame["answer"]["grants"]["surface"],
            serde_json::json!(surface)
        );
    }
}
