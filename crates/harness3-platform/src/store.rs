use crate::domain::*;
use harness3::{
    Error, Result as HResult,
    session::{Session, SessionStore},
};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    path::Path,
    sync::{Arc, Mutex, MutexGuard},
};

#[derive(Clone)]
pub struct Store(Arc<Mutex<Connection>>);
impl Store {
    pub fn open(path: &Path) -> PResult<Self> {
        let connection = Connection::open(path)?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        let version: i64 = connection.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version > 5 {
            return Err(fault(500, "database_version_unsupported"));
        }
        connection.execute_batch(include_str!("schema.sql"))?;
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
        connection.execute_batch("PRAGMA user_version=5;")?;
        Ok(Self(Arc::new(Mutex::new(connection))))
    }
    pub fn memory() -> PResult<Self> {
        Self::open(Path::new(":memory:"))
    }
    pub(crate) fn lock(&self) -> PResult<MutexGuard<'_, Connection>> {
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
    pub fn publish(&self, preset: Preset) -> PResult<Value> {
        preset.validate()?;
        self.transaction(|tx| {
            let mut snapshot = serde_json::to_value(&preset)?;
            if let Some(plugin) = &preset.plugin {
                let bundle: Option<String> = tx
                    .query_row(
                        "SELECT body FROM plugins WHERE id=? AND version=?",
                        params![plugin.id, plugin.version],
                        |r| r.get(0),
                    )
                    .optional()?;
                snapshot["plugin_snapshot"] = if let Some(bundle) = bundle {
                    serde_json::from_str(&bundle)?
                } else {
                    let (body, state): (String, String) = tx
                        .query_row(
                            "SELECT body,state FROM process_plugins WHERE id=? AND version=?",
                            params![plugin.id, plugin.version],
                            |r| Ok((r.get(0)?, r.get(1)?)),
                        )
                        .optional()?
                        .ok_or_else(|| fault(404, "plugin_not_found"))?;
                    if state != "enabled" {
                        return Err(fault(403, "plugin_disabled"));
                    }
                    serde_json::from_str(&body)?
                };
            }
            let body = serde_json::to_string(&snapshot)?;
            if let Some(existing) = tx
                .query_row(
                    "SELECT body FROM presets WHERE name=? AND version=?",
                    params![preset.name, preset.version],
                    |r| r.get::<_, String>(0),
                )
                .optional()?
            {
                if existing != body {
                    return Err(fault(409, "preset_version_immutable"));
                }
                return Ok(snapshot);
            }
            tx.execute(
                "INSERT INTO presets(name,version,body,created_at) VALUES(?,?,?,?)",
                params![preset.name, preset.version, body, now()],
            )?;
            event(
                tx,
                "preset.published",
                &preset.name,
                json!({"version":preset.version}),
            )?;
            Ok(snapshot)
        })
    }
    pub fn install(&self, bundle: Bundle) -> PResult<Value> {
        bundle.validate()?;
        self.transaction(|tx| {
            if tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM process_plugins WHERE id=?)",
                [&bundle.id],
                |r| r.get::<_, bool>(0),
            )? {
                return Err(fault(409, "plugin_kind_conflict"));
            }
            let body = serde_json::to_string(&bundle)?;
            if let Some(existing) = tx
                .query_row(
                    "SELECT body FROM plugins WHERE id=? AND version=?",
                    params![bundle.id, bundle.version],
                    |r| r.get::<_, String>(0),
                )
                .optional()?
            {
                if existing != body {
                    return Err(fault(409, "plugin_version_immutable"));
                }
                return Ok(serde_json::from_str(&body)?);
            }
            let digest = format!("sha256:{:x}", Sha256::digest(body.as_bytes()));
            tx.execute(
                "INSERT INTO plugins(id,version,body,digest,created_at) VALUES(?,?,?,?,?)",
                params![bundle.id, bundle.version, body, digest, now()],
            )?;
            event(
                tx,
                "plugin.installed",
                &bundle.id,
                json!({"version":bundle.version,"digest":digest,"kind":"prompt-bundle"}),
            )?;
            Ok(json!({"id":bundle.id,"version":bundle.version,"digest":digest}))
        })
    }
    pub fn spawn(&self, input: &Spawn, key: &str) -> PResult<Value> {
        self.spawn_with_ttl(input, key, None)
    }
    pub fn spawn_with_ttl(
        &self,
        input: &Spawn,
        key: &str,
        ttl_seconds: Option<u64>,
    ) -> PResult<Value> {
        if ttl_seconds.is_some_and(|ttl| ttl == 0 || ttl > 30 * 24 * 60 * 60) {
            return Err(fault(400, "invalid_ttl"));
        }
        if input.count == 0 || input.count > 64 || key.is_empty() || key.len() > 160 {
            return Err(fault(400, "invalid_spawn_or_idempotency_key"));
        }
        self.transaction(|tx| {
            let fingerprint = match ttl_seconds {
                Some(ttl) => json!({"spawn":input,"ttl_seconds":ttl}).to_string(),
                None => serde_json::to_string(input)?,
            };
            if let Some((old,response)) = tx.query_row("SELECT fingerprint,response FROM commands WHERE key=?",[format!("spawn:{key}")], |r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?))).optional()? {
                if old != fingerprint { return Err(fault(409,"idempotency_conflict")); }
                return Ok(serde_json::from_str(&response)?);
            }
            crate::lifecycle::reconcile(tx)?;
            let current = crate::fleets::quota_used(tx)?;
            if current + input.count > 64 { return Err(fault(409,"instance_quota_exceeded")); }
            let snapshot: String = tx.query_row("SELECT body FROM presets WHERE name=? AND version=?",params![input.preset_name,input.preset_version],|r|r.get(0)).optional()?.ok_or_else(||fault(404,"preset_not_found"))?;
            let fleet = id("fleet");
            let expires_at = ttl_seconds.map(|ttl| now() + (ttl * 1000) as i64);
            tx.execute("INSERT INTO fleets(id,preset_name,preset_version,snapshot,mode,desired_count,state,revision,ttl_seconds,created_at) VALUES(?,?,?,?,'finite',?,'active',1,?,?)",params![fleet,input.preset_name,input.preset_version,snapshot,input.count,ttl_seconds,now()])?;
            let mut ids=vec![];
            for slot in 0..input.count {
                let agent=id("agent");
                tx.execute("INSERT INTO instances(id,fleet_id,preset_name,preset_version,state,snapshot,created_at) VALUES(?,?,?,?,'ready',?,?)",params![agent,fleet,input.preset_name,input.preset_version,snapshot,now()])?;
                if let Some(expires_at) = expires_at {
                    tx.execute("INSERT INTO instance_deadlines(instance_id,expires_at) VALUES(?,?)",params![agent,expires_at])?;
                }
                tx.execute("INSERT INTO fleet_slots(fleet_id,slot,generation,enabled,fulfilled,instance_id) VALUES(?,?,1,1,1,?)",params![fleet,slot,agent])?;
                tx.execute("INSERT INTO instance_allocations(instance_id,fleet_id,slot,generation) VALUES(?,?,?,1)",params![agent,fleet,slot])?;
                event(tx,"instance.created",&agent,json!({"fleet_id":fleet}))?;
                ids.push(agent);
            }
            let response=json!({"fleet_id":fleet,"instance_ids":ids,"count":input.count,"mode":"finite","state":"completed","expires_at":expires_at});
            tx.execute("INSERT INTO commands(key,fingerprint,response) VALUES(?,?,?)",params![format!("spawn:{key}"),fingerprint,response.to_string()])?;
            Ok(response)
        })
    }
    pub fn transition(&self, agent: &str, action: &str) -> PResult<Value> {
        self.transaction(|tx| {
            crate::lifecycle::reconcile(tx)?;
            if action == "drain" {
                return crate::lifecycle::drain(tx, agent, false);
            }
            let state: String = tx
                .query_row("SELECT state FROM instances WHERE id=?", [agent], |r| {
                    r.get(0)
                })
                .optional()?
                .ok_or_else(|| fault(404, "instance_not_found"))?;
            let next = match (state.as_str(), action) {
                ("ready" | "suspended", "terminate") => "terminated",
                ("ready", "suspend") => "suspended",
                ("suspended", "resume") => "ready",
                ("terminated", "terminate") => "terminated",
                ("suspended", "suspend") => "suspended",
                ("ready", "resume") => "ready",
                _ => return Err(fault(409, "invalid_transition_or_instance_busy")),
            };
            tx.execute(
                "UPDATE instances SET state=? WHERE id=?",
                params![next, agent],
            )?;
            event(
                tx,
                "instance.transitioned",
                agent,
                json!({"from":state,"to":next}),
            )?;
            Ok(json!({"id":agent,"state":next}))
        })
    }
    pub fn enqueue(&self, agent: &str, input: &RunInput, key: &str) -> PResult<Value> {
        if input.text.trim().is_empty()
            || input.text.len() > 64000
            || key.is_empty()
            || key.len() > 160
        {
            return Err(fault(400, "invalid_run_or_idempotency_key"));
        }
        self.transaction(|tx| {
            let fingerprint = serde_json::to_string(input)?;
            let command = format!("run:{agent}:{key}");
            if let Some((old, response)) = tx
                .query_row(
                    "SELECT fingerprint,response FROM commands WHERE key=?",
                    [&command],
                    |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
                )
                .optional()?
            {
                if old != fingerprint {
                    return Err(fault(409, "idempotency_conflict"));
                }
                return Ok(serde_json::from_str(&response)?);
            }
            let state: String = tx
                .query_row("SELECT state FROM instances WHERE id=?", [agent], |r| {
                    r.get(0)
                })
                .optional()?
                .ok_or_else(|| fault(404, "instance_not_found"))?;
            let expired: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM instance_deadlines WHERE instance_id=? AND expires_at<=?)",params![agent,now()],|r|r.get(0))?;
            if state != "ready" || expired {
                return Err(fault(409, "instance_not_ready"));
            }
            let run = id("run");
            tx.execute(
                "INSERT INTO runs(id,instance_id,state,input,created_at) VALUES(?,?,'queued',?,?)",
                params![run, agent, input.text, now()],
            )?;
            tx.execute("UPDATE instances SET state='busy' WHERE id=?", [agent])?;
            event(tx, "run.queued", &run, json!({"instance_id":agent}))?;
            let response = json!({"id":run,"instance_id":agent,"state":"queued"});
            tx.execute(
                "INSERT INTO commands(key,fingerprint,response) VALUES(?,?,?)",
                params![command, fingerprint, response.to_string()],
            )?;
            Ok(response)
        })
    }
    pub fn claim(&self) -> PResult<Option<Value>> {
        self.transaction(|tx| {
            crate::lifecycle::reconcile(tx)?;
            let row=tx.query_row("SELECT r.id,r.instance_id,r.input,i.snapshot FROM runs r JOIN instances i ON i.id=r.instance_id WHERE r.state='queued' ORDER BY r.created_at,r.rowid LIMIT 1",[],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?))).optional()?;
            let Some((run,agent,text,snapshot))=row else{return Ok(None)};
            tx.execute("UPDATE runs SET state='running',started_at=? WHERE id=?",params![now(),run])?;
            event(tx,"run.started",&run,json!({"instance_id":agent}))?;
            Ok(Some(json!({"id":run,"instance_id":agent,"input":text,"snapshot":serde_json::from_str::<Value>(&snapshot)?})))
        })
    }
    pub fn finish(&self, run: &str, agent: &str, state: &str, report: Value) -> PResult<()> {
        self.transaction(|tx| {
            let changed = tx.execute(
                "UPDATE runs SET state=?,report=?,finished_at=? WHERE id=? AND state='running'",
                params![state, report.to_string(), now(), run],
            )?;
            if changed != 1 {
                return Err(fault(409, "run_not_owned"));
            }
            tx.execute(
                "UPDATE instances SET state='ready' WHERE id=? AND state='busy'",
                [agent],
            )?;
            event(
                tx,
                "run.settled",
                run,
                json!({"instance_id":agent,"state":state}),
            )?;
            crate::lifecycle::reconcile(tx)?;
            Ok(())
        })
    }
    pub fn cancel(&self, run: &str) -> PResult<Value> {
        self.transaction(|tx| {
            let (state,agent):(String,String)=tx.query_row("SELECT state,instance_id FROM runs WHERE id=?",[run],|r|Ok((r.get(0)?,r.get(1)?))).optional()?.ok_or_else(||fault(404,"run_not_found"))?;
            if state=="queued" {
                tx.execute("UPDATE runs SET state='cancelled',cancel_requested=1,finished_at=? WHERE id=?",params![now(),run])?;
                tx.execute("UPDATE instances SET state='ready' WHERE id=? AND state='busy'",[&agent])?;
            } else if state=="running" {tx.execute("UPDATE runs SET cancel_requested=1 WHERE id=?",[run])?;}
            event(tx,"run.cancel_requested",run,json!({"previous_state":state}))?;
            Ok(json!({"id":run,"state":if state=="queued"{"cancelled"}else{&state},"cancel_requested":state=="queued"||state=="running"}))
        })
    }
    pub fn cancel_requested(&self, run: &str) -> PResult<bool> {
        Ok(self
            .lock()?
            .query_row("SELECT cancel_requested FROM runs WHERE id=?", [run], |r| {
                r.get(0)
            })?)
    }
    pub fn recover_startup(&self) -> PResult<()> {
        self.transaction(|tx| {
            // Exclusive process lock must be held by the daemon before this method.
            let mut statement =
                tx.prepare("SELECT id,instance_id FROM runs WHERE state='running'")?;
            let rows = statement
                .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
                .collect::<Result<Vec<_>, _>>()?;
            drop(statement);
            for (run, agent) in rows {
                let existing = tx
                    .query_row("SELECT body FROM sessions WHERE id=?", [&agent], |r| {
                        r.get::<_, String>(0)
                    })
                    .optional()?;
                let session = existing
                    .map(|body| serde_json::from_str::<Session>(&body))
                    .transpose()?;
                // If the harness settled before the platform crash, preserve its known outcome.
                let delivery = serde_json::to_string(&("platform", &run))?;
                let known = session.as_ref().and_then(|s| s.completed.get(&delivery));
                let (state, report) = if let Some(report) = known {
                    let state = match report.error.as_ref().map(|e| e.code.as_str()) {
                        None => "completed",
                        Some("interrupted_outcome_unknown") => "interrupted",
                        Some("cancelled") => "cancelled",
                        Some(_) => "failed",
                    };
                    (state, serde_json::to_value(report)?)
                } else {
                    (
                        "interrupted",
                        json!({"error":{"code":"interrupted_outcome_unknown"}}),
                    )
                };
                tx.execute(
                    "UPDATE runs SET state=?,report=?,finished_at=? WHERE id=?",
                    params![state, report.to_string(), now(), run],
                )?;
                tx.execute(
                    "UPDATE instances SET state='ready' WHERE id=? AND state='busy'",
                    [&agent],
                )?;
                event(tx, "run.recovered", &run, json!({"state":state}))?;
            }
            Ok(())
        })
    }
    pub fn instance_exists(&self, agent: &str) -> PResult<bool> {
        Ok(self.lock()?.query_row(
            "SELECT EXISTS(SELECT 1 FROM instances WHERE id=?)",
            [agent],
            |r| r.get(0),
        )?)
    }
    pub fn active_sessions(&self) -> PResult<Vec<String>> {
        let connection = self.lock()?;
        let mut stmt =
            connection.prepare("SELECT id FROM instances WHERE state IN ('busy','draining')")?;
        Ok(stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?)
    }
    pub fn list(&self, table: &str) -> PResult<Vec<Value>> {
        let connection = self.lock()?;
        let sql = match table {
            "presets" => {
                "SELECT json_object('name',name,'version',version,'definition',json(body),'created_at',created_at) FROM presets ORDER BY created_at DESC LIMIT 200"
            }
            "plugins" => {
                "SELECT json_object('id',id,'version',version,'definition',json(body),'digest',digest,'created_at',created_at) FROM plugins ORDER BY created_at DESC LIMIT 200"
            }
            "instances" => {
                "SELECT json_object('id',id,'fleet_id',fleet_id,'preset_name',preset_name,'preset_version',preset_version,'state',state,'created_at',created_at,'snapshot',json(snapshot),'expires_at',(SELECT expires_at FROM instance_deadlines WHERE instance_id=instances.id),'slot',(SELECT slot FROM instance_allocations WHERE instance_id=instances.id),'generation',(SELECT generation FROM instance_allocations WHERE instance_id=instances.id)) FROM instances ORDER BY (state != 'terminated') DESC,created_at DESC,id LIMIT 200"
            }
            "runs" => {
                "SELECT json_object('id',id,'instance_id',instance_id,'state',state,'input',input,'report',json(report),'created_at',created_at,'started_at',started_at,'finished_at',finished_at) FROM runs ORDER BY created_at DESC,rowid DESC LIMIT 100"
            }
            "goals" => {
                "SELECT json_object('id',g.id,'title',g.title,'objective',g.objective,'state',g.state,'created_at',g.created_at,'finished_at',g.finished_at,'task_count',count(w.key),'completed_count',sum(w.state='completed')) FROM goals g JOIN work_items w ON w.goal_id=g.id GROUP BY g.id ORDER BY g.created_at DESC,g.rowid DESC LIMIT 100"
            }
            _ => return Err(fault(404, "collection_not_found")),
        };
        let mut stmt = connection.prepare(sql)?;
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|body| Ok(serde_json::from_str(&body)?))
            .collect()
    }
    pub fn events(&self, after: i64) -> PResult<Vec<Value>> {
        let connection = self.lock()?;
        let mut stmt = connection.prepare(
            "SELECT seq,kind,subject,body,at FROM events WHERE seq>? ORDER BY seq LIMIT 200",
        )?;
        let rows = stmt
            .query_map([after], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, i64>(4)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter().map(|(seq,kind,subject,body,at)|Ok(json!({"seq":seq,"kind":kind,"subject":subject,"data":serde_json::from_str::<Value>(&body)?,"at":at}))).collect()
    }
    pub fn record(&self, kind: &str, subject: &str, body: Value) -> PResult<()> {
        self.transaction(|tx| event(tx, kind, subject, body))
    }
}
pub(crate) fn event(tx: &Transaction<'_>, kind: &str, subject: &str, body: Value) -> PResult<()> {
    tx.execute(
        "INSERT INTO events(kind,subject,body,at) VALUES(?,?,?,?)",
        params![kind, subject, body.to_string(), now()],
    )?;
    Ok(())
}

#[async_trait::async_trait]
impl SessionStore for Store {
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
