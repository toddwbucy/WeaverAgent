//! conforms: state-store-is-a-port
//! Shared test-only PostgreSQL custody fixture. Each instance owns a database.

use postgres::{Client, NoTls};

static NEXT_DATABASE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Each test owns a database. Declared before its engine, this guard drops
/// after the connection, including when an assertion unwinds.
pub(crate) struct Scratch {
    maintenance: Client,
    pub(crate) socket: String,
    pub(crate) role: String,
    pub(crate) database: String,
}

impl Scratch {
    pub(crate) fn new() -> Self {
        let socket = std::env::var("WEAVER_STATE_TEST_PG")
            .expect("WEAVER_STATE_TEST_PG must name a scratch PostgreSQL socket directory");
        let role = std::env::var("USER").expect("the scratch instance's own user");
        let database = format!(
            "w5_{}_{}",
            std::process::id(),
            NEXT_DATABASE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        );
        let mut maintenance = postgres::Config::new()
            .host_path(&socket)
            .user(&role)
            .dbname("postgres")
            .connect(NoTls)
            .expect("connect to scratch maintenance database");
        maintenance
            .batch_execute(&format!("CREATE DATABASE \"{database}\""))
            .expect("create per-test database");
        Self {
            maintenance,
            socket,
            role,
            database,
        }
    }

    pub(crate) fn open(&self) -> crate::engine::postgres::Postgres {
        crate::engine::postgres::Postgres::open(&self.socket, &self.database, &self.role)
            .expect("open test engine")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let result = self
            .maintenance
            .batch_execute(&format!("DROP DATABASE \"{}\" WITH (FORCE)", self.database));
        if std::thread::panicking() {
            if let Err(error) = result {
                eprintln!("scratch database cleanup failed: {error}");
            }
        } else {
            result.expect("drop per-test database");
        }
    }
}
