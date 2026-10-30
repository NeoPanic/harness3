use crate::error::*;
use harness3::{
    Error, Result as HResult,
    session::{Session, SessionStore},
};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
};

pub mod error;
pub use error::{Fault, PResult};
#[derive(Clone)]
pub struct Database(Arc<Mutex<Connection>>, Arc<Mutex<Option<PathBuf>>>, i64);
impl Database {
    pub fn open(path: &Path) -> PResult<Self> {
        let connection = Connection::open(path)?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        let version: i64 = connection.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version > 15 {
            return Err(fault(500, "database_version_unsupported"));
        }
        connection.execute_batch(include_str!("../migrations/sqlite/schema.sql"))?;
        connection.execute_batch(include_str!("../migrations/sqlite/channels.sql"))?;
        connection.execute_batch(include_str!("../migrations/sqlite/autonomy.sql"))?;
        connection.execute_batch(include_str!("../migrations/sqlite/board.sql"))?;
        connection.execute_batch(include_str!("../migrations/sqlite/routing.sql"))?;
        if version < 4 {
            connection.execute_batch("BEGIN IMMEDIATE;
                INSERT OR IGNORE INTO fleets(id,preset_name,preset_version,snapshot,mode,desired_count,state,revision,created_at)
                SELECT fleet_id,preset_name,preset_version,snapshot,'finite',count(*),CASE WHEN sum(state!='terminated')=0 THEN 'completed' ELSE 'active' END,1,min(created_at) FROM instances GROUP BY fleet_id;
                INSERT OR IGNORE INTO instance_allocations(instance_id,fleet_id,slot,generation)
                SELECT id,fleet_id,row_number() OVER (PARTITION BY fleet_id ORDER BY created_at,id)-1,1 FROM instances;
                INSERT OR IGNORE INTO fleet_slots(fleet_id,slot,generation,enabled,fulfilled,instance_id)
                SELECT fleet_id,slot,generation,1,1,instance_id FROM instance_allocations;
                COMMIT;")?;
        }
        Ok(Self(
            Arc::new(Mutex::new(connection)),
            Arc::new(Mutex::new(None)),
            version,
        ))
    }
    pub fn previous_version(&self) -> i64 {
        self.2
    }
    pub fn set_workspace_root(&self, root: &Path) -> PResult<()> {
        std::fs::create_dir_all(root).map_err(|_| fault(500, "workspace_root_unavailable"))?;
        let root = root
            .canonicalize()
            .map_err(|_| fault(500, "workspace_root_unavailable"))?;
        *self
            .1
            .lock()
            .map_err(|_| fault(500, "storage_lock_poisoned"))? = Some(root);
        Ok(())
    }
    pub fn workspace_root(&self) -> PResult<PathBuf> {
        self.1
            .lock()
            .map_err(|_| fault(500, "storage_lock_poisoned"))?
            .clone()
            .ok_or_else(|| fault(409, "workspace_not_configured"))
    }
    pub fn lock(&self) -> PResult<MutexGuard<'_, Connection>> {
        self.0
            .lock()
            .map_err(|_| fault(500, "storage_lock_poisoned"))
    }
    pub fn transaction<T>(&self, f: impl FnOnce(&Transaction<'_>) -> PResult<T>) -> PResult<T> {
        let mut connection = self.lock()?;
        let tx = connection.transaction()?;
        let value = f(&tx)?;
        tx.commit()?;
        Ok(value)
    }
}
#[async_trait::async_trait]
impl SessionStore for Database {
    async fn load(&self, id: &str) -> HResult<Session> {
        let connection = self.lock().map_err(|_| Error::new("session_storage"))?;
        let body = connection
            .query_row("SELECT body FROM sessions WHERE id=?", [id], |r| {
                r.get::<_, String>(0)
            })
            .optional()
            .map_err(|_| Error::new("session_storage"))?;
        let session = body
            .map(|s| serde_json::from_str::<Session>(&s))
            .transpose()
            .map_err(|_| Error::new("session_storage"))?
            .unwrap_or_default();
        session.ensure_supported()?;
        Ok(session)
    }
    async fn save(&self, id: &str, expected_revision: u64, session: &Session) -> HResult<()> {
        let mut connection = self.lock().map_err(|_| Error::new("session_storage"))?;
        let tx = connection
            .transaction()
            .map_err(|_| Error::new("session_storage"))?;
        let previous = tx
            .query_row("SELECT body FROM sessions WHERE id=?", [id], |r| {
                r.get::<_, String>(0)
            })
            .optional()
            .map_err(|_| Error::new("session_storage"))?;
        let previous = previous
            .map(|body| serde_json::from_str::<Session>(&body))
            .transpose()
            .map_err(|_| Error::new("session_storage"))?
            .unwrap_or_default();
        previous.ensure_supported()?;
        session.ensure_supported()?;
        if previous.revision != expected_revision {
            return Err(Error::new("session_conflict"));
        }
        let mut saved = session.clone();
        saved.seal()?;
        saved.validate_append(&previous)?;
        saved.revision = expected_revision
            .checked_add(1)
            .ok_or_else(|| Error::new("session_revision_overflow"))?;
        let body = serde_json::to_string(&saved).map_err(|_| Error::new("session_storage"))?;
        tx.execute("INSERT INTO sessions(id,body) VALUES(?,?) ON CONFLICT(id) DO UPDATE SET body=excluded.body",params![id,body]).map_err(|_|Error::new("session_storage"))?;
        tx.commit().map_err(|_| Error::new("session_storage"))?;
        Ok(())
    }
}
