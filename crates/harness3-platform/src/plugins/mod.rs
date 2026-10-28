//! Content-addressed process plugins with explicit local operator opt-in.
mod package;
mod process;
use crate::{
    domain::*,
    store::{Store, event},
};
use harness3::{CancellationToken, Context, Error, Tool, ToolOutput, ToolSpec};
use harness3_plugin_protocol::{Manifest, validate_value};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub kind: String,
    pub id: String,
    pub version: String,
    pub digest: String,
    pub manifest: Manifest,
}
#[derive(Clone)]
pub struct ProcessHost {
    store: Store,
    root: Option<Arc<PathBuf>>,
    active: Arc<Mutex<HashMap<String, (String, CancellationToken)>>>,
    slots: Arc<tokio::sync::Semaphore>,
}
struct Lease {
    id: String,
    active: Arc<Mutex<HashMap<String, (String, CancellationToken)>>>,
}
impl Drop for Lease {
    fn drop(&mut self) {
        if let Ok(mut active) = self.active.lock() {
            active.remove(&self.id);
        }
    }
}
impl ProcessHost {
    pub fn disabled(store: Store) -> Self {
        Self {
            store,
            root: None,
            slots: Arc::new(tokio::sync::Semaphore::new(8)),
            active: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    pub fn enabled(store: Store, root: PathBuf) -> PResult<Self> {
        std::fs::create_dir_all(&root).map_err(|_| fault(500, "plugin_cache_unwritable"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700))
                .map_err(|_| fault(500, "plugin_cache_unwritable"))?;
        }
        let root = root
            .canonicalize()
            .map_err(|_| fault(500, "plugin_cache_unreadable"))?;
        Ok(Self {
            store,
            root: Some(Arc::new(root)),
            slots: Arc::new(tokio::sync::Semaphore::new(8)),
            active: Arc::new(Mutex::new(HashMap::new())),
        })
    }
    pub fn is_enabled(&self) -> bool {
        self.root.is_some()
    }
    async fn slot(&self) -> PResult<tokio::sync::SemaphorePermit<'_>> {
        tokio::time::timeout(Duration::from_secs(2), self.slots.acquire())
            .await
            .map_err(|_| fault(429, "plugin_process_capacity_busy"))?
            .map_err(|_| fault(503, "plugin_host_stopped"))
    }
    fn root(&self) -> PResult<&Path> {
        self.root
            .as_deref()
            .map(PathBuf::as_path)
            .ok_or_else(|| fault(403, "process_plugins_disabled"))
    }
    pub async fn install(&self, source: PathBuf) -> PResult<Value> {
        let cache = self.root()?.to_owned();
        let (manifest, digest) =
            tokio::task::spawn_blocking(move || package::prepare(&cache, &source))
                .await
                .map_err(|_| fault(500, "plugin_package_task_failed"))??;
        if crate::organization::package(&manifest.id).is_some() {
            return Err(fault(409, "plugin_kind_conflict"));
        }
        let snapshot = Snapshot {
            kind: "process".into(),
            id: manifest.id.clone(),
            version: manifest.version.clone(),
            digest,
            manifest,
        };
        let existing = self.store.process_plugin(&snapshot.id, &snapshot.version);
        if let Ok(existing) = existing {
            if existing["digest"] != snapshot.digest {
                return Err(fault(409, "plugin_version_immutable"));
            }
            return Ok(existing);
        } else if let Err(error) = existing
            && error.0 != 404
        {
            return Err(error);
        }
        let _slot = self.slot().await?;
        let path = self.verified(&snapshot).await?;
        process::health(&path, &snapshot.manifest)
            .await
            .map_err(|code| fault(400, code))?;
        self.store.transaction(|tx| {
            if tx.query_row("SELECT EXISTS(SELECT 1 FROM plugins WHERE id=?)",[&snapshot.id],|r|r.get::<_,bool>(0))? {return Err(fault(409,"plugin_kind_conflict"));}
            if let Some(old)=tx.query_row("SELECT digest FROM process_plugins WHERE id=? AND version=?",params![snapshot.id,snapshot.version],|r|r.get::<_,String>(0)).optional()? {
                if old!=snapshot.digest {return Err(fault(409,"plugin_version_immutable"));}
                return Ok(());
            }
            tx.execute("INSERT INTO process_plugins(id,version,body,digest,state,health_at,created_at) VALUES(?,?,?,?,'enabled',?,?)",params![snapshot.id,snapshot.version,serde_json::to_string(&snapshot)?,snapshot.digest,now(),now()])?;
            event(tx,"plugin.process_installed",&snapshot.id,json!({"version":snapshot.version,"digest":snapshot.digest,"tool_count":snapshot.manifest.tools.len()}))
        })?;
        self.store.process_plugin(&snapshot.id, &snapshot.version)
    }
    async fn verified(&self, snapshot: &Snapshot) -> PResult<PathBuf> {
        let root = self.root()?.to_owned();
        let snapshot = snapshot.clone();
        tokio::task::spawn_blocking(move || {
            package::verify(&root, &snapshot.digest, &snapshot.manifest)
        })
        .await
        .map_err(|_| fault(500, "plugin_package_task_failed"))?
    }
    pub async fn action(&self, id: &str, version: &str, action: &str) -> PResult<Value> {
        self.root()?;
        let row = self.store.process_plugin(id, version)?;
        let snapshot: Snapshot = serde_json::from_value(row["definition"].clone())?;
        if !["enable", "disable", "check"].contains(&action) {
            return Err(fault(400, "invalid_plugin_action"));
        }
        if action != "disable" {
            let _slot = self.slot().await?;
            let result = match self.verified(&snapshot).await {
                Ok(path) => process::health(&path, &snapshot.manifest)
                    .await
                    .map_err(|code| fault(409, code)),
                Err(error) => Err(error),
            };
            self.store.transaction(|tx| {
                tx.execute("UPDATE process_plugins SET health_at=?,health_error=? WHERE id=? AND version=?",params![now(),result.as_ref().err().map(|e|&e.1),id,version])?;
                event(tx,"plugin.health_checked",id,json!({"version":version,"healthy":result.is_ok(),"error":result.as_ref().err().map(|e|&e.1)}))
            })?;
            result?;
        }
        if action != "check" {
            // Admission and revocation share one short lock. Calls already admitted
            // receive cancellation; later admission observes the disabled state.
            let active = self
                .active
                .lock()
                .map_err(|_| fault(500, "plugin_host_lock_poisoned"))?;
            self.store.transaction(|tx| {
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
            })?;
            if action == "disable" {
                let key = format!("{id}@{version}");
                for (owner, token) in active.values() {
                    if owner == &key {
                        token.cancel();
                    }
                }
            }
        }
        self.store.process_plugin(id, version)
    }
    pub fn tools(&self, snapshot: Snapshot) -> PResult<Vec<Arc<dyn Tool>>> {
        self.root()?;
        snapshot
            .manifest
            .validate()
            .map_err(|_| fault(500, "plugin_snapshot_invalid"))?;
        self.admissible(&snapshot)?;
        Ok(snapshot
            .manifest
            .tools
            .iter()
            .enumerate()
            .map(|(index, _)| {
                Arc::new(ProcessTool {
                    host: self.clone(),
                    snapshot: snapshot.clone(),
                    index,
                }) as Arc<dyn Tool>
            })
            .collect())
    }
    fn admissible(&self, snapshot: &Snapshot) -> PResult<()> {
        let row = self.store.process_plugin(&snapshot.id, &snapshot.version)?;
        if row["state"] != "enabled" {
            return Err(fault(403, "plugin_disabled"));
        }
        if row["digest"] != snapshot.digest {
            return Err(fault(409, "plugin_snapshot_digest_mismatch"));
        }
        Ok(())
    }
    pub async fn invoke(
        &self,
        snapshot: &Snapshot,
        name: &str,
        arguments: Value,
        context: &Context,
        cancel: CancellationToken,
    ) -> harness3::Result<ToolOutput> {
        self.root().map_err(|e| Error::new(e.1))?;
        let _slot = tokio::select! {
            _=cancel.cancelled()=>return Err(Error::new("cancelled")),
            slot=self.slot()=>slot.map_err(|e|Error::new(e.1))?,
        };
        let tool = snapshot
            .manifest
            .tools
            .iter()
            .find(|t| t.name == name)
            .ok_or_else(|| Error::new("plugin_tool_missing"))?;
        validate_value(&tool.input_schema, &arguments)
            .map_err(|_| Error::new("plugin_input_schema_mismatch"))?;
        if cancel.is_cancelled() {
            return Err(Error::new("cancelled"));
        }
        let invocation = id("invocation");
        let token = cancel.child_token();
        let lease = {
            let mut active = self
                .active
                .lock()
                .map_err(|_| Error::new("plugin_host_lock_poisoned"))?;
            self.admissible(snapshot).map_err(|e| Error::new(e.1))?;
            active.insert(
                invocation.clone(),
                (
                    format!("{}@{}", snapshot.id, snapshot.version),
                    token.clone(),
                ),
            );
            Lease {
                id: invocation.clone(),
                active: self.active.clone(),
            }
        };
        let path = self.verified(snapshot).await.map_err(|e| Error::new(e.1))?;
        self.store.record("plugin.invocation_started",&context.event_id,json!({"invocation_id":invocation,"plugin_id":snapshot.id,"version":snapshot.version,"digest":snapshot.digest,"tool":name,"arguments_digest":package::digest(&serde_json::to_vec(&arguments).map_err(|_|Error::new("plugin_input_invalid"))?)})).map_err(|e|Error::new(e.1))?;
        let result = process::invoke(
            process::Invocation {
                path: &path,
                manifest: &snapshot.manifest,
                tool: name,
                context,
                invocation: &invocation,
                store: &self.store,
            },
            arguments,
            token,
        )
        .await;
        let (state, cause) = match &result {
            Ok(_) => ("completed", None),
            Err((code, true)) => ("outcome_unknown", Some(*code)),
            Err((code, false)) => ("not_called", Some(*code)),
        };
        self.store.record("plugin.invocation_settled",&context.event_id,json!({"invocation_id":invocation,"plugin_id":snapshot.id,"version":snapshot.version,"state":state,"cause":cause})).map_err(|e|Error::new(e.1))?;
        drop(lease);
        result.map_err(|(cause,unknown)|Error::new(if unknown {"plugin_outcome_unknown"}else{cause}).explain("Inspect the plugin invocation receipt; automatic replay is disabled.",json!({"invocation_id":invocation,"cause":cause,"outcome_unknown":unknown,"plugin_id":snapshot.id,"version":snapshot.version,"digest":snapshot.digest})))
    }
}
struct ProcessTool {
    host: ProcessHost,
    snapshot: Snapshot,
    index: usize,
}
#[async_trait::async_trait]
impl Tool for ProcessTool {
    fn spec(&self) -> ToolSpec {
        let t = &self.snapshot.manifest.tools[self.index];
        ToolSpec {
            name: format!("{}.{}", self.snapshot.id, t.name),
            description: t.description.clone(),
            parameters: t.input_schema.clone(),
            mutates: t.mutates,
        }
    }
    fn parallel_safe(&self) -> bool {
        self.snapshot.manifest.tools[self.index].parallel_safe
    }
    async fn execute(
        &self,
        context: &Context,
        arguments: Value,
        cancel: CancellationToken,
    ) -> harness3::Result<ToolOutput> {
        self.host
            .invoke(
                &self.snapshot,
                &self.snapshot.manifest.tools[self.index].name,
                arguments,
                context,
                cancel,
            )
            .await
    }
}
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
