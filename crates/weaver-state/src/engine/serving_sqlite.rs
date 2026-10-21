//! The RAM connection and fixed state-content/1 catalog, under STATE-CONNECTION
//! and STATE-SCHEMA of the state-management weaver-state Spec amendment.
//!
//! This additive foundation is not selected by the member's existing serving
//! path. An empty catalog is not a seed or an adopted serving store. Seed content,
//! semantic image validation and the typed file/read contracts are later layers.

use std::fmt;

use rusqlite::Connection;

mod schema;

/// An empty, verified state-content/1 catalog in RAM.
///
/// The connection is private. This type offers no SQL, file, query or image port,
/// and cannot open an existing file or become the member's serving store by itself.
pub struct ServingCatalog {
    connection: Connection,
}

/// A failure to construct or verify this catalog, before a seed or serving store
/// exists. The eventual seed/open boundary maps it to write_failed/open_failed.
#[derive(Debug)]
pub enum CatalogFault {
    Sqlite(rusqlite::Error),
    ConnectionInvariant {
        setting: &'static str,
        expected: &'static str,
        observed: String,
    },
    TempStoreBuild {
        observed: Vec<String>,
    },
    JsonTableUnavailable,
    CatalogMismatch {
        object: String,
    },
    Integrity {
        detail: String,
    },
}

impl fmt::Display for CatalogFault {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sqlite(error) => write!(formatter, "SQLite catalog failure: {error}"),
            Self::ConnectionInvariant {
                setting,
                expected,
                observed,
            } => write!(
                formatter,
                "{setting}: expected {expected}, observed {observed}"
            ),
            Self::TempStoreBuild { observed } => {
                write!(
                    formatter,
                    "unsupported TEMP_STORE build facts: {observed:?}"
                )
            }
            Self::JsonTableUnavailable => write!(formatter, "json_each failed its fixed probe"),
            Self::CatalogMismatch { object } => {
                write!(formatter, "state-content/1 catalog differs at {object}")
            }
            Self::Integrity { detail } => write!(formatter, "catalog integrity: {detail}"),
        }
    }
}

impl std::error::Error for CatalogFault {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Sqlite(error) => Some(error),
            _ => None,
        }
    }
}

impl From<rusqlite::Error> for CatalogFault {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sqlite(error)
    }
}

impl ServingCatalog {
    /// Create every table and index in a fresh RAM connection, then verify the
    /// settings, catalog and SQLite integrity. No content or identity is seeded.
    pub fn create() -> Result<Self, CatalogFault> {
        let connection = open_memory()?;
        create_schema(&connection)?;
        let catalog = Self { connection };
        catalog.verify_catalog()?;
        Ok(catalog)
    }

    /// Check the RAM settings, SQLite catalog and SQLite structural integrity.
    ///
    /// This deliberately does not certify the seed, bindings, marks, plan or
    /// source position required by STATE-ADOPT. It supplies its catalog checks
    /// without exposing a partially verified image as a serving store.
    pub fn verify_catalog(&self) -> Result<(), CatalogFault> {
        verify_connection(&self.connection)?;
        verify_schema(&self.connection)?;
        verify_integrity(&self.connection)?;
        verify_json_tables(&self.connection)
    }
}

fn open_memory() -> Result<Connection, CatalogFault> {
    let connection = Connection::open_in_memory()?;
    configure(&connection)?;
    Ok(connection)
}

/// The same initialization is required again after image adoption. Each PRAGMA
/// is a separate statement, and success includes its readback and build facts.
fn configure(connection: &Connection) -> Result<(), CatalogFault> {
    connection.execute_batch("PRAGMA foreign_keys = ON")?;
    connection.execute_batch("PRAGMA journal_mode = MEMORY")?;
    connection.execute_batch("PRAGMA synchronous = OFF")?;
    connection.execute_batch("PRAGMA temp_store = MEMORY")?;
    verify_connection(connection)
}

fn verify_connection(connection: &Connection) -> Result<(), CatalogFault> {
    for (setting, expected, spelling) in [
        ("foreign_keys", 1_i64, "1"),
        ("synchronous", 0, "0"),
        ("temp_store", 2, "2"),
    ] {
        let observed: i64 = connection.pragma_query_value(None, setting, |row| row.get(0))?;
        if observed != expected {
            return Err(CatalogFault::ConnectionInvariant {
                setting,
                expected: spelling,
                observed: observed.to_string(),
            });
        }
    }
    let journal: String = connection.pragma_query_value(None, "journal_mode", |row| row.get(0))?;
    if journal != "memory" {
        return Err(CatalogFault::ConnectionInvariant {
            setting: "journal_mode",
            expected: "memory",
            observed: journal,
        });
    }

    let facts = connection
        .prepare("PRAGMA compile_options")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    verify_temp_store_build(&facts)?;

    let databases = connection
        .prepare("PRAGMA database_list")?
        .query_map([], |row| {
            Ok((row.get::<_, String>(1)?, row.get::<_, String>(2)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if !databases.iter().any(|(name, _)| name == "main")
        || databases
            .iter()
            .any(|(name, path)| !matches!(name.as_str(), "main" | "temp") || !path.is_empty())
    {
        return Err(CatalogFault::ConnectionInvariant {
            setting: "database_list",
            expected: "only in-memory main and temp databases",
            observed: format!("{databases:?}"),
        });
    }

    Ok(())
}

/// Run only after creating or adopting the final schema. The bundled SQLite
/// engine can retain a virtual-table schema pointer across deserialize, so a
/// json_each probe before replacing the image is not a harmless capability read.
fn verify_json_tables(connection: &Connection) -> Result<(), CatalogFault> {
    let values = connection
        .prepare("SELECT key, value, type FROM json_each(?1) ORDER BY key")?
        .query_map([r#"["state","content"]"#], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if values
        != [
            (0, "state".to_owned(), "text".to_owned()),
            (1, "content".to_owned(), "text".to_owned()),
        ]
    {
        return Err(CatalogFault::JsonTableUnavailable);
    }
    Ok(())
}

fn verify_temp_store_build(facts: &[String]) -> Result<(), CatalogFault> {
    let observed: Vec<_> = facts
        .iter()
        .filter(|fact| fact.starts_with("TEMP_STORE="))
        .cloned()
        .collect();
    if observed.is_empty()
        || !observed.iter().all(|fact| {
            matches!(
                fact.as_str(),
                "TEMP_STORE=1" | "TEMP_STORE=2" | "TEMP_STORE=3"
            )
        })
        || observed.iter().any(|fact| fact != &observed[0])
    {
        return Err(CatalogFault::TempStoreBuild { observed });
    }
    Ok(())
}

fn create_schema(connection: &Connection) -> Result<(), CatalogFault> {
    // Each literal is one independently prepared statement. No caller supplies
    // DDL, and none of the statements can silently skip an existing object.
    for statement in schema::DDL {
        connection.execute(statement, [])?;
    }
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
struct CatalogObject {
    kind: String,
    name: String,
    table: String,
    sql: Option<String>,
}

fn catalog_objects(connection: &Connection) -> Result<Vec<CatalogObject>, CatalogFault> {
    Ok(connection
        .prepare("SELECT type, name, tbl_name, sql FROM main.sqlite_schema ORDER BY type, name")?
        .query_map([], |row| {
            Ok(CatalogObject {
                kind: row.get(0)?,
                name: row.get(1)?,
                table: row.get(2)?,
                sql: row.get(3)?,
            })
        })?
        .collect::<Result<_, _>>()?)
}

fn verify_schema(connection: &Connection) -> Result<(), CatalogFault> {
    // The bundled SQLite engine derives its own implicit indexes from the exact
    // fixed DDL. Comparing with that trusted catalog covers those indexes and
    // sqlite_sequence as well as explicit objects, without copying SQLite's
    // implicit-index naming algorithm. This is an open/seed check, not turn work.
    let reference = open_memory()?;
    create_schema(&reference)?;
    let expected = catalog_objects(&reference)?;
    let observed = catalog_objects(connection)?;
    for index in 0..expected.len().max(observed.len()) {
        if expected.get(index) != observed.get(index) {
            let object = observed
                .get(index)
                .or_else(|| expected.get(index))
                .expect("a catalog difference has an object")
                .name
                .clone();
            return Err(CatalogFault::CatalogMismatch { object });
        }
    }
    Ok(())
}

fn verify_integrity(connection: &Connection) -> Result<(), CatalogFault> {
    let results = connection
        .prepare("PRAGMA main.quick_check")?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    if results != ["ok"] {
        return Err(CatalogFault::Integrity {
            detail: format!("quick_check: {results:?}"),
        });
    }
    let mut check = connection.prepare("PRAGMA main.foreign_key_check")?;
    if check.query([])?.next()?.is_some() {
        return Err(CatalogFault::Integrity {
            detail: "foreign_key_check returned a violation".into(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests;
