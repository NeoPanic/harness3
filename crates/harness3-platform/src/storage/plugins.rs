use crate::{
    domain::*,
    store::{Store, event},
};
use harness3_plugins::{Repository, Snapshot};
use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};
impl Store {
    pub fn process_plugins(&self) -> PResult<Vec<Value>> {
        let connection = self.lock()?;
        let mut stmt=connection.prepare("SELECT json_object('id',id,'version',version,'definition',json(body),'digest',digest,'state',state,'health_at',health_at,'health_error',health_error,'created_at',created_at) FROM process_plugins ORDER BY created_at DESC,rowid DESC LIMIT 200")?;
        stmt.query_map([], |r| r.get::<_, String>(0))?
            .map(|r| Ok(serde_json::from_str(&r?)?))
            .collect()
    }
    pub fn process_plugin(&self, id: &str, version: &str) -> PResult<Value> {
        let body:Option<String>=self.lock()?.query_row("SELECT json_object('id',id,'version',version,'definition',json(body),'digest',digest,'state',state,'health_at',health_at,'health_error',health_error,'created_at',created_at) FROM process_plugins WHERE id=? AND version=?",params![id,version],|r|r.get(0)).optional()?;
        serde_json::from_str(&body.ok_or_else(|| fault(404, "process_plugin_not_found"))?)
            .map_err(Into::into)
    }
}

impl Repository for Store {
    fn is_builtin(&self, id: &str) -> bool {
        crate::organization::package(id).is_some()
    }
    fn process_plugin(&self, id: &str, version: &str) -> harness3_domain::PResult<Value> {
        Store::process_plugin(self, id, version).map_err(Into::into)
    }
    fn record(&self, kind: &str, subject: &str, body: Value) -> harness3_domain::PResult<()> {
        Store::record(self, kind, subject, body).map_err(Into::into)
    }
    fn install_process_plugin(&self, snapshot: &Snapshot) -> harness3_domain::PResult<()> {
        self.transaction(|tx| {
            if tx.query_row("SELECT EXISTS(SELECT 1 FROM plugins WHERE id=?)",[&snapshot.id],|r|r.get::<_,bool>(0))? {return Err(fault(409,"plugin_kind_conflict"));}
            if let Some(old)=tx.query_row("SELECT digest FROM process_plugins WHERE id=? AND version=?",params![snapshot.id,snapshot.version],|r|r.get::<_,String>(0)).optional()? {
                if old!=snapshot.digest {return Err(fault(409,"plugin_version_immutable"));}
                return Ok(());
            }
            tx.execute("INSERT INTO process_plugins(id,version,body,digest,state,health_at,created_at) VALUES(?,?,?,?,'enabled',?,?)",params![snapshot.id,snapshot.version,serde_json::to_string(&snapshot)?,snapshot.digest,now(),now()])?;
            event(tx,"plugin.process_installed",&snapshot.id,json!({"version":snapshot.version,"digest":snapshot.digest,"tool_count":snapshot.manifest.tools.len()}))
        })
.map_err(Into::into)
    }
    fn record_process_health(
        &self,
        id: &str,
        version: &str,
        error: Option<&str>,
    ) -> harness3_domain::PResult<()> {
        self.transaction(|tx| {
            tx.execute(
                "UPDATE process_plugins SET health_at=?,health_error=? WHERE id=? AND version=?",
                params![now(), error, id, version],
            )?;
            event(
                tx,
                "plugin.health_checked",
                id,
                json!({"version":version,"healthy":error.is_none(),"error":error}),
            )
        })
        .map_err(Into::into)
    }
    fn set_process_state(
        &self,
        id: &str,
        version: &str,
        action: &str,
    ) -> harness3_domain::PResult<()> {
        self.transaction(|tx| {
            tx.execute(
                "UPDATE process_plugins SET state=? WHERE id=? AND version=?",
                params![
                    if action == "disable" {
                        "disabled"
                    } else {
                        "enabled"
                    },
                    id,
                    version
                ],
            )?;
            event(
                tx,
                "plugin.state_changed",
                id,
                json!({"version":version,"action":action}),
            )
        })
        .map_err(Into::into)
    }
}
