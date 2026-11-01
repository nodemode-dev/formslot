use std::path::Path;
use std::time::Duration;

use rusqlite::Connection;

const MIGRATIONS: &[&str] = &[
    include_str!("migrations/001_corpus.sql"),
    include_str!("migrations/002_symbols.sql"),
    include_str!("migrations/003_dedup.sql"),
    include_str!("migrations/004_facts_family.sql"),
    include_str!("migrations/005_related.sql"),
    include_str!("migrations/006_slots.sql"),
    include_str!("migrations/007_slot_status.sql"),
    include_str!("migrations/008_memory.sql"),
];

pub struct Store {
    conn: Connection,
}

fn sql_err(e: rusqlite::Error) -> String {
    format!("store: {e}")
}

fn prepare_file(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
        }
    }
    let mut options = std::fs::OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(path)
        .map_err(|e| format!("cannot create {}: {e}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| format!("cannot set mode on {}: {e}", path.display()))?;
    }
    Ok(())
}

impl Store {
    pub fn open(path: &Path) -> crate::Result<Store> {
        prepare_file(path)?;
        let conn = Connection::open(path).map_err(sql_err)?;
        conn.busy_timeout(Duration::from_millis(5000))
            .map_err(sql_err)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON; PRAGMA synchronous = NORMAL;",
        )
        .map_err(sql_err)?;
        let mut store = Store { conn };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&mut self) -> crate::Result<()> {
        let current: i64 = self
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .map_err(sql_err)?;
        for (index, sql) in MIGRATIONS.iter().enumerate() {
            let target = (index + 1) as i64;
            if current >= target {
                continue;
            }
            let tx = self.conn.transaction().map_err(sql_err)?;
            tx.execute_batch(sql).map_err(sql_err)?;
            tx.execute_batch(&format!("PRAGMA user_version = {target};"))
                .map_err(sql_err)?;
            tx.commit().map_err(sql_err)?;
        }
        Ok(())
    }

    pub fn schema_version(&self) -> crate::Result<i64> {
        self.conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .map_err(|e| sql_err(e).into())
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }
}
