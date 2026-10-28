//! Durable fleet intent, bounded reconciliation and operation receipts.
use crate::{
    domain::*,
    store::{Store, event},
};
use rusqlite::{OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FleetPlan {
    pub preset_name: String,
    pub preset_version: String,
    pub count: usize,
    #[serde(default = "finite")]
    pub mode: String,
    #[serde(default)]
    pub ttl_seconds: Option<u64>,
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub parameters: serde_json::Map<String, Value>,
}
fn finite() -> String {
    "finite".into()
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FleetAction {
    pub action: String,
    pub expected_revision: i64,
    #[serde(default)]
    pub count: Option<usize>,
}

fn replay(tx: &Transaction<'_>, command: &str, fingerprint: &str) -> PResult<Option<Value>> {
    let row = tx
        .query_row(
            "SELECT fingerprint,response FROM commands WHERE key=?",
            [command],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )
        .optional()?;
    match row {
        Some((old, body)) if old == fingerprint => Ok(Some(serde_json::from_str(&body)?)),
        Some(_) => Err(fault(409, "idempotency_conflict")),
        None => Ok(None),
    }
}
fn key_valid(key: &str) -> PResult<()> {
    if key.is_empty() || key.len() > 160 {
        return Err(fault(400, "invalid_idempotency_key"));
    }
    Ok(())
}
fn receipt(
    tx: &Transaction<'_>,
    fleet: &str,
    kind: &str,
    revision: i64,
    command: &str,
    fingerprint: &str,
) -> PResult<Value> {
    let target: i64 = tx.query_row(
        "SELECT desired_count FROM fleets WHERE id=?",
        [fleet],
        |r| r.get(0),
    )?;
    let op = id("operation");
    tx.execute("INSERT INTO operations(id,fleet_id,kind,revision,target_count,state,created_at) VALUES(?,?,?,?,?,'pending',?)",params![op,fleet,kind,revision,target,now()])?;
    let response = json!({"operation_id":op,"fleet_id":fleet,"revision":revision,"desired_count":target,"state":"pending","links":{"operation":format!("/api/v1/operations/{op}"),"fleet":format!("/api/v1/agent-fleets/{fleet}")}});
    tx.execute(
        "INSERT INTO commands(key,fingerprint,response) VALUES(?,?,?)",
        params![command, fingerprint, response.to_string()],
    )?;
    event(
        tx,
        "operation.accepted",
        &op,
        json!({"fleet_id":fleet,"kind":kind,"revision":revision}),
    )?;
    Ok(response)
}

// Reserve missing desired slots as well as all still-live instances, including retirees.
// A retiring slot keeps its capacity until the old generation is terminal.
pub(crate) fn quota_used(tx: &Transaction<'_>) -> PResult<usize> {
    Ok(tx.query_row("SELECT (SELECT count(*) FROM instances WHERE state!='terminated') + (SELECT count(*) FROM fleet_slots s JOIN fleets f ON f.id=s.fleet_id LEFT JOIN instances i ON i.id=s.instance_id WHERE s.enabled=1 AND f.state NOT IN ('draining','terminated') AND (f.mode='maintain' OR s.fulfilled=0) AND (i.id IS NULL OR i.state='terminated'))",[],|r|r.get(0))?)
}
fn enable_slots(tx: &Transaction<'_>, fleet: &str, count: usize) -> PResult<()> {
    tx.execute(
        "UPDATE fleet_slots SET enabled=0 WHERE fleet_id=? AND slot>=?",
        params![fleet, count],
    )?;
    for slot in 0..count {
        tx.execute("INSERT INTO fleet_slots(fleet_id,slot,generation,enabled,fulfilled) VALUES(?,?,0,1,0) ON CONFLICT(fleet_id,slot) DO UPDATE SET enabled=1,fulfilled=CASE WHEN fleet_slots.enabled=0 THEN 0 ELSE fleet_slots.fulfilled END",params![fleet,slot])?;
    }
    Ok(())
}

impl Store {
    pub fn capacity(&self) -> PResult<Value> {
        self.transaction(|tx| {
            let used=quota_used(tx)?;
            let live:usize=tx.query_row("SELECT count(*) FROM instances WHERE state!='terminated'",[],|r|r.get(0))?;
            Ok(json!({"limit":64,"reserved":used,"active":live,"available":64usize.saturating_sub(used)}))
        })
    }
    pub fn create_fleet(&self, plan: &FleetPlan, key: &str) -> PResult<Value> {
        key_valid(key)?;
        if plan.count == 0
            || plan.count > 64
            || !["finite", "maintain"].contains(&plan.mode.as_str())
            || plan
                .ttl_seconds
                .is_some_and(|ttl| ttl == 0 || ttl > 2_592_000)
        {
            return Err(fault(400, "invalid_fleet_plan"));
        }
        self.transaction(|tx| {
            let command=format!("fleet-create:{key}");let fingerprint=serde_json::to_string(plan)?;
            if let Some(value)=replay(tx,&command,&fingerprint)? {return Ok(value);}
            let legacy = tx.query_row("SELECT fingerprint,response FROM commands WHERE key=?",[format!("spawn:{key}")],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?))).optional()?;
            if let Some((old,response))=legacy {
                let input=Spawn{preset_name:plan.preset_name.clone(),preset_version:plan.preset_version.clone(),count:plan.count};
                let fingerprint=match plan.ttl_seconds {Some(ttl)=>json!({"spawn":input,"ttl_seconds":ttl}).to_string(),None=>serde_json::to_string(&input)?};
                if plan.mode!="finite" || !plan.parameters.is_empty() || old!=fingerprint {return Err(fault(409,"idempotency_conflict"));}
                return Ok(serde_json::from_str(&response)?);
            }
            crate::lifecycle::reconcile(tx)?;
            if quota_used(tx)?+plan.count>64 {return Err(fault(409,"instance_quota_exceeded"));}
            let template:String=tx.query_row("SELECT body FROM presets WHERE name=? AND version=?",params![plan.preset_name,plan.preset_version],|r|r.get(0)).optional()?.ok_or_else(||fault(404,"preset_not_found"))?;
            let snapshot=crate::presets::instantiate(serde_json::from_str(&template)?, &plan.parameters)?.to_string();
            let fleet=id("fleet");
            tx.execute("INSERT INTO fleets(id,preset_name,preset_version,snapshot,mode,desired_count,state,revision,ttl_seconds,created_at) VALUES(?,?,?,?,?,?,'provisioning',1,?,?)",params![fleet,plan.preset_name,plan.preset_version,snapshot,plan.mode,plan.count,plan.ttl_seconds,now()])?;
            enable_slots(tx,&fleet,plan.count)?;
            event(tx,"fleet.created",&fleet,json!({"desired_count":plan.count,"mode":plan.mode}))?;
            receipt(tx,&fleet,"create",1,&command,&fingerprint)
        })
    }
    pub fn fleet_action(&self, fleet: &str, action: &FleetAction, key: &str) -> PResult<Value> {
        key_valid(key)?;
        if !["scale", "terminate"].contains(&action.action.as_str())
            || action.expected_revision < 1
            || (action.action == "scale" && action.count.is_none_or(|n| n > 64))
            || (action.action == "terminate" && action.count.is_some())
        {
            return Err(fault(400, "invalid_fleet_action"));
        }
        self.transaction(|tx| {
            let command=format!("fleet-action:{fleet}:{key}");let fingerprint=serde_json::to_string(action)?;
            if let Some(value)=replay(tx,&command,&fingerprint)? {return Ok(value);}
            let (state,revision):(String,i64)=tx.query_row("SELECT state,revision FROM fleets WHERE id=?",[fleet],|r|Ok((r.get(0)?,r.get(1)?))).optional()?.ok_or_else(||fault(404,"fleet_not_found"))?;
            if revision!=action.expected_revision {return Err(fault(409,"fleet_revision_conflict"));}
            if ["draining","terminated"].contains(&state.as_str()) {return Err(fault(409,"fleet_terminal"));}
            let active:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM operations WHERE fleet_id=? AND state IN ('pending','running'))",[fleet],|r|r.get(0))?;
            if active && action.action!="terminate" {return Err(fault(409,"fleet_operation_in_progress"));}
            if active {
                tx.execute("UPDATE operations SET state='superseded',finished_at=? WHERE fleet_id=? AND state IN ('pending','running')",params![now(),fleet])?;
                event(tx,"fleet.operations_superseded",fleet,json!({"reason":"terminate"}))?;
            }
            let count=if action.action=="terminate" {0} else {action.count.unwrap()};
            let next=if action.action=="terminate" {"draining"} else {"provisioning"};
            tx.execute("UPDATE fleets SET desired_count=?,state=?,revision=revision+1 WHERE id=?",params![count,next,fleet])?;
            enable_slots(tx,fleet,count)?;
            if action.action=="scale" {
                tx.execute("UPDATE fleet_slots SET fulfilled=0 WHERE fleet_id=? AND enabled=1 AND (instance_id IS NULL OR instance_id IN (SELECT id FROM instances WHERE state IN ('terminated','draining')))",[fleet])?;
            }
            retire(tx,fleet)?;
            if quota_used(tx)?>64 {return Err(fault(409,"instance_quota_exceeded"));}
            event(tx,"fleet.intent_changed",fleet,json!({"desired_count":count,"state":next,"revision":revision+1}))?;
            receipt(tx,fleet,&action.action,revision+1,&command,&fingerprint)
        })
    }
    pub fn fleet(&self, fleet: &str) -> PResult<Value> {
        self.transaction(|tx| fleet_json(tx, fleet, true))
    }
    pub fn fleets(&self) -> PResult<Vec<Value>> {
        self.transaction(|tx| {
            let mut stmt =
                tx.prepare("SELECT id FROM fleets ORDER BY created_at DESC,rowid DESC LIMIT 200")?;
            let ids = stmt
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            ids.iter().map(|id| fleet_json(tx, id, false)).collect()
        })
    }
    pub fn operation(&self, id: &str) -> PResult<Value> {
        self.transaction(|tx| operation_json(tx, id))
    }
    pub fn reconcile_fleets(&self) -> PResult<()> {
        self.transaction(reconcile)
    }
}
fn operation_json(tx: &Transaction<'_>, id: &str) -> PResult<Value> {
    let body:String=tx.query_row("SELECT json_object('id',id,'fleet_id',fleet_id,'kind',kind,'revision',revision,'target_count',target_count,'state',state,'created_at',created_at,'started_at',started_at,'finished_at',finished_at) FROM operations WHERE id=?",[id],|r|r.get(0)).optional()?.ok_or_else(||fault(404,"operation_not_found"))?;
    Ok(serde_json::from_str(&body)?)
}
fn fleet_json(tx: &Transaction<'_>, fleet: &str, details: bool) -> PResult<Value> {
    let body:String=tx.query_row("SELECT json_object('id',id,'preset_name',preset_name,'preset_version',preset_version,'mode',mode,'desired_count',desired_count,'state',state,'revision',revision,'ttl_seconds',ttl_seconds,'created_at',created_at) FROM fleets WHERE id=?",[fleet],|r|r.get(0)).optional()?.ok_or_else(||fault(404,"fleet_not_found"))?;
    let mut value: Value = serde_json::from_str(&body)?;
    for (name, predicate) in [
        ("current_count", "state!='terminated'"),
        ("ready_count", "state='ready'"),
        ("busy_count", "state='busy'"),
        ("draining_count", "state='draining'"),
        ("terminated_count", "state='terminated'"),
    ] {
        let n: i64 = tx.query_row(
            &format!("SELECT count(*) FROM instances WHERE fleet_id=? AND {predicate}"),
            [fleet],
            |r| r.get(0),
        )?;
        value[name] = json!(n);
    }
    value["pending_operations"] = json!(tx.query_row(
        "SELECT count(*) FROM operations WHERE fleet_id=? AND state IN ('pending','running')",
        [fleet],
        |r| r.get::<_, i64>(0)
    )?);
    if !details {
        return Ok(value);
    }
    let mut stmt=tx.prepare("SELECT json_object('slot',s.slot,'generation',s.generation,'instance_id',s.instance_id,'enabled',json(CASE WHEN s.enabled=1 THEN 'true' ELSE 'false' END),'fulfilled',json(CASE WHEN s.fulfilled=1 THEN 'true' ELSE 'false' END),'state',i.state) FROM fleet_slots s LEFT JOIN instances i ON i.id=s.instance_id WHERE s.fleet_id=? ORDER BY s.slot")?;
    value["slots"] = Value::Array(
        stmt.query_map([fleet], |r| r.get::<_, String>(0))?
            .map(|r| Ok(serde_json::from_str(&r?)?))
            .collect::<PResult<Vec<_>>>()?,
    );
    let mut stmt = tx.prepare(
        "SELECT id FROM operations WHERE fleet_id=? ORDER BY created_at DESC,rowid DESC LIMIT 20",
    )?;
    let ids = stmt
        .query_map([fleet], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    value["operations"] = Value::Array(
        ids.iter()
            .map(|id| operation_json(tx, id))
            .collect::<PResult<Vec<_>>>()?,
    );
    Ok(value)
}

fn retire(tx: &Transaction<'_>, fleet: &str) -> PResult<()> {
    let mut stmt=tx.prepare("SELECT i.id FROM instances i LEFT JOIN fleet_slots s ON s.instance_id=i.id AND s.fleet_id=i.fleet_id WHERE i.fleet_id=? AND i.state!='terminated' AND (s.enabled=0 OR s.instance_id IS NULL)")?;
    let retirees = stmt
        .query_map([fleet], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    for agent in retirees {
        crate::lifecycle::drain(tx, &agent, false)?;
    }
    Ok(())
}

pub(crate) fn reconcile(tx: &Transaction<'_>) -> PResult<()> {
    crate::lifecycle::reconcile(tx)?;
    let mut stmt=tx.prepare("SELECT id,mode,state,snapshot,preset_name,preset_version,ttl_seconds,desired_count FROM fleets WHERE state!='terminated' ORDER BY created_at,rowid")?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, Option<i64>>(6)?,
                r.get::<_, i64>(7)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    let mut remaining = 8;
    for (fleet, mode, state, snapshot, name, version, ttl, desired) in rows {
        tx.execute("UPDATE operations SET state='running',started_at=? WHERE fleet_id=? AND state='pending'",params![now(),fleet])?;
        retire(tx, &fleet)?;
        if state != "draining" {
            let mut stmt=tx.prepare("SELECT s.slot,s.generation FROM fleet_slots s LEFT JOIN instances i ON i.id=s.instance_id WHERE s.fleet_id=? AND s.enabled=1 AND (i.id IS NULL OR i.state='terminated') AND (?='maintain' OR s.fulfilled=0) ORDER BY s.slot LIMIT ?")?;
            let missing = stmt
                .query_map(params![fleet, mode, remaining], |r| {
                    Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            drop(stmt);
            for (slot, generation) in missing {
                let agent = id("agent");
                let generation = generation + 1;
                tx.execute("INSERT INTO instances(id,fleet_id,preset_name,preset_version,state,snapshot,created_at) VALUES(?,?,?,?,'ready',?,?)",params![agent,fleet,name,version,snapshot,now()])?;
                tx.execute("INSERT INTO instance_allocations(instance_id,fleet_id,slot,generation) VALUES(?,?,?,?)",params![agent,fleet,slot,generation])?;
                tx.execute("UPDATE fleet_slots SET generation=?,instance_id=?,fulfilled=1 WHERE fleet_id=? AND slot=?",params![generation,agent,fleet,slot])?;
                if let Some(ttl) = ttl {
                    tx.execute(
                        "INSERT INTO instance_deadlines(instance_id,expires_at) VALUES(?,?)",
                        params![agent, now() + ttl * 1000],
                    )?;
                }
                event(
                    tx,
                    "instance.created",
                    &agent,
                    json!({"fleet_id":fleet,"slot":slot,"generation":generation}),
                )?;
                remaining -= 1;
            }
        }
        let live: i64 = tx.query_row(
            "SELECT count(*) FROM instances WHERE fleet_id=? AND state!='terminated'",
            [&fleet],
            |r| r.get(0),
        )?;
        let retiring:i64=tx.query_row("SELECT count(*) FROM instances i LEFT JOIN fleet_slots s ON s.instance_id=i.id WHERE i.fleet_id=? AND i.state!='terminated' AND (s.enabled=0 OR s.instance_id IS NULL OR i.state='draining')",[&fleet],|r|r.get(0))?;
        let unmet:i64=tx.query_row("SELECT count(*) FROM fleet_slots s LEFT JOIN instances i ON i.id=s.instance_id WHERE s.fleet_id=? AND s.enabled=1 AND (s.fulfilled=0 OR (?='maintain' AND (i.id IS NULL OR i.state='terminated')))",params![fleet,mode],|r|r.get(0))?;
        let settled = retiring == 0 && unmet == 0 && (mode != "maintain" || live == desired);
        let next = if state == "draining" {
            if live == 0 { "terminated" } else { "draining" }
        } else if settled {
            if mode == "finite" && live == 0 {
                "completed"
            } else {
                "active"
            }
        } else {
            "provisioning"
        };
        if state != next {
            tx.execute("UPDATE fleets SET state=? WHERE id=?", params![next, fleet])?;
            event(
                tx,
                "fleet.reconciled",
                &fleet,
                json!({"from":state,"to":next,"current_count":live,"desired_count":desired}),
            )?;
        }
        if (state == "draining" && live == 0) || (state != "draining" && settled) {
            let changed=tx.execute("UPDATE operations SET state='completed',finished_at=? WHERE fleet_id=? AND state='running'",params![now(),fleet])?;
            if changed > 0 {
                event(
                    tx,
                    "fleet.operation_completed",
                    &fleet,
                    json!({"state":next}),
                )?;
            }
        }
    }
    Ok(())
}
