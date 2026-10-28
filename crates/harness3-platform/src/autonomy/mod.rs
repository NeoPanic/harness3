//! Run-bound requests to grow a task graph or claim the same Agent's next assignment.
mod tools;
use crate::{
    claims::ClaimInput,
    domain::*,
    store::{Store, event},
};
use rusqlite::{OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
pub use tools::{package, tools};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppendInput {
    pub key: String,
    pub instructions: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
    pub preset_name: String,
    pub preset_version: String,
    #[serde(default)]
    pub parameters: serde_json::Map<String, Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NextInput {
    pub task_key: String,
    pub expected_generation: i64,
}
fn scope(tx: &Transaction<'_>, run: &str) -> PResult<Value> {
    let row:Option<(String,String,String,String)>=tx.query_row("SELECT w.goal_id,w.key,r.instance_id,i.snapshot FROM work_items w JOIN goals g ON g.id=w.goal_id JOIN runs r ON r.id=w.run_id JOIN instances i ON i.id=r.instance_id WHERE r.id=? AND r.state='running' AND r.cancel_requested=0 AND i.state='busy' AND g.state='running'",[run],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
    let (goal, task, agent, snapshot) =
        row.ok_or_else(|| fault(403, "autonomy_run_not_authorized"))?;
    if !crate::presets::bindings(&serde_json::from_str(&snapshot)?)
        .iter()
        .any(|p| p == &package())
    {
        return Err(fault(403, "autonomy_capability_required"));
    }
    Ok(
        json!({"goal_id":goal,"source_task":task,"instance_id":agent,"round":crate::artifacts::round(tx,&goal)?}),
    )
}
fn records(tx: &Transaction<'_>, goal: &str) -> PResult<Vec<Value>> {
    let mut stmt=tx.prepare("SELECT json_object('id',id,'goal_id',goal_id,'source_task',source_task,'source_run_id',source_run_id,'instance_id',instance_id,'round',round,'kind',kind,'target_task',target_task,'generation',generation,'body',json(body),'state',state,'error',error,'result',json(result),'created_at',created_at,'finished_at',finished_at) FROM autonomy_intents WHERE goal_id=? ORDER BY created_at,rowid")?;
    stmt.query_map([goal], |r| r.get::<_, String>(0))?
        .map(|r| Ok(serde_json::from_str(&r?)?))
        .collect()
}
pub(crate) fn decorate(tx: &Transaction<'_>, goal: &mut Value) -> PResult<()> {
    goal["autonomy"] = json!({"intents":records(tx,goal["id"].as_str().unwrap())?});
    Ok(())
}
fn replay(tx: &Transaction<'_>, run: &str, key: &str, fingerprint: &str) -> PResult<Option<Value>> {
    let row:Option<(String,String,String,String)>=tx.query_row("SELECT id,goal_id,kind,fingerprint FROM autonomy_intents WHERE source_run_id=? AND request_key=?",params![run,key],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
    match row {
        Some((id, goal, kind, old)) if old == fingerprint => Ok(Some(
            json!({"id":id,"goal_id":goal,"kind":kind,"state":"pending","effective":"after_source_run_succeeds"}),
        )),
        Some(_) => Err(fault(409, "idempotency_conflict")),
        None => Ok(None),
    }
}
fn insert(
    tx: &Transaction<'_>,
    run: &str,
    key: &str,
    s: &Value,
    kind: &str,
    body: &Value,
    fingerprint: &str,
) -> PResult<Value> {
    let id = id("intent");
    let target = body[if kind == "append" { "key" } else { "task_key" }]
        .as_str()
        .unwrap();
    let generation = body["expected_generation"].as_i64();
    tx.execute("INSERT INTO autonomy_intents(id,goal_id,source_task,source_run_id,instance_id,round,kind,request_key,fingerprint,target_task,generation,body,state,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,'pending',?)",params![id,s["goal_id"].as_str().unwrap(),s["source_task"].as_str().unwrap(),run,s["instance_id"].as_str().unwrap(),s["round"].as_i64().unwrap(),kind,key,fingerprint,target,generation,body.to_string(),now()])?;
    event(
        tx,
        "autonomy.requested",
        s["goal_id"].as_str().unwrap(),
        json!({"id":id,"kind":kind,"source_run_id":run,"target_task":target}),
    )?;
    Ok(
        json!({"id":id,"goal_id":s["goal_id"],"kind":kind,"state":"pending","effective":"after_source_run_succeeds"}),
    )
}
fn key_valid(key: &str) -> PResult<()> {
    if key.is_empty() || key.len() > 160 {
        Err(fault(400, "invalid_autonomy_command"))
    } else {
        Ok(())
    }
}
impl Store {
    pub fn task_board(&self, run: &str) -> PResult<Value> {
        self.transaction(|tx|{
            let s=match scope(tx,run) {
                Ok(scope)=>scope,
                Err(error) if error.1=="autonomy_run_not_authorized"=> {
                    let goal=crate::channels::notification_goal(tx,run)?.ok_or(error)?;
                    let (instance,snapshot):(String,String)=tx.query_row("SELECT i.id,i.snapshot FROM runs r JOIN instances i ON i.id=r.instance_id WHERE r.id=?",[run],|r|Ok((r.get(0)?,r.get(1)?)))?;
                    if !crate::presets::bindings(&serde_json::from_str(&snapshot)?).iter().any(|p|p==&package()) {return Err(fault(403,"autonomy_capability_required"));}
                    json!({"goal_id":goal,"source_task":Value::Null,"instance_id":instance,"round":crate::artifacts::round(tx,&goal)?})
                },
                Err(error)=>return Err(error),
            };let g=crate::coordination::goal_json(tx,s["goal_id"].as_str().unwrap())?;
            let tasks=g["tasks"].as_array().unwrap().iter().map(|t|json!({"key":t["key"],"instructions":t["instructions"],"depends_on":t["depends_on"],"state":t["state"],"instance_id":t["instance_id"],"run_id":t["run_id"],"claim":t["claim"]})).collect::<Vec<_>>();
            Ok(json!({"goal_id":s["goal_id"],"source_task":s["source_task"],"can_request_tasks":!s["source_task"].is_null(),"instance_id":s["instance_id"],"round":s["round"],"objective":g["objective"],"acceptance":g["acceptance"],"tasks":tasks,"intents":records(tx,s["goal_id"].as_str().unwrap())?,"append_limit_per_run":8,"goal_task_limit":crate::limits::TASKS_PER_GOAL}))
        })
    }
    pub fn request_next_task(&self, run: &str, input: &NextInput, key: &str) -> PResult<Value> {
        key_valid(key)?;
        if !valid_name(&input.task_key) || input.expected_generation < 0 {
            return Err(fault(400, "invalid_next_task_request"));
        }
        let body = serde_json::to_value(input)?;
        let fingerprint = json!({"kind":"claim","input":body}).to_string();
        self.transaction(|tx|{
            if let Some(r)=replay(tx,run,key,&fingerprint)?{return Ok(r);}
            let s=scope(tx,run)?;let g=crate::coordination::goal_json(tx,s["goal_id"].as_str().unwrap())?;
            let t=g["tasks"].as_array().unwrap().iter().find(|t|t["key"]==input.task_key).ok_or_else(||fault(404,"task_not_found"))?;
            if t["key"]==s["source_task"]||t["state"]!="pending"||!t["run_id"].is_null(){return Err(fault(409,"task_already_claimed"));}
            if t["claim"]["generation"]!=input.expected_generation{return Err(fault(409,"task_generation_conflict"));}
            if !t["claim"]["candidates"].as_array().unwrap().contains(&s["instance_id"]){return Err(fault(403,"task_candidate_not_allowed"));}
            let pending:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM autonomy_intents WHERE source_run_id=? AND kind='claim' AND state='pending')",[run],|r|r.get(0))?;
            if pending{return Err(fault(409,"next_task_already_requested"));}
            insert(tx,run,key,&s,"claim",&body,&fingerprint)
        })
    }
    pub fn request_append_task(&self, run: &str, input: &AppendInput, key: &str) -> PResult<Value> {
        key_valid(key)?;
        if !valid_name(&input.key)
            || input.instructions.trim().is_empty()
            || input.instructions.len() > 8000
            || input.depends_on.len() > crate::limits::TASKS_PER_GOAL
            || !valid_name(&input.preset_name)
            || !valid_name(&input.preset_version)
        {
            return Err(fault(400, "invalid_append_task"));
        }
        let body = serde_json::to_value(input)?;
        let fingerprint = json!({"kind":"append","input":body}).to_string();
        self.transaction(|tx|{
            if let Some(r)=replay(tx,run,key,&fingerprint)?{return Ok(r);}
            let s=scope(tx,run)?;let goal=s["goal_id"].as_str().unwrap();let g=crate::coordination::goal_json(tx,goal)?;
            let pending_key:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM autonomy_intents WHERE goal_id=? AND kind='append' AND target_task=? AND state='pending')",params![goal,input.key],|r|r.get(0))?;
            if pending_key||g["tasks"].as_array().unwrap().iter().any(|t|t["key"]==input.key){return Err(fault(409,"task_key_conflict"));}
            let mut seen=std::collections::HashSet::new();
            for dep in &input.depends_on {if !seen.insert(dep)||!g["tasks"].as_array().unwrap().iter().any(|t|t["key"]==*dep){return Err(fault(400,"invalid_append_dependencies"));}}
            let count:usize=tx.query_row("SELECT count(*) FROM autonomy_intents WHERE source_run_id=? AND kind='append'",[run],|r|r.get(0))?;
            if count>=8{return Err(fault(409,"append_per_run_limit"));}
            let pending:usize=tx.query_row("SELECT (SELECT count(*) FROM autonomy_intents WHERE goal_id=? AND kind='append' AND state='pending')+(SELECT coalesce(sum(json_array_length(children)),0) FROM task_delegations WHERE goal_id=? AND state='pending')",params![goal,goal],|r|r.get(0))?;
            if g["tasks"].as_array().unwrap().len()+pending>=crate::limits::TASKS_PER_GOAL{return Err(fault(409,"goal_task_limit"));}
            let preset:Option<String>=tx.query_row("SELECT body FROM presets WHERE name=? AND version=?",params![input.preset_name,input.preset_version],|r|r.get(0)).optional()?;
            crate::presets::instantiate(serde_json::from_str(&preset.ok_or_else(||fault(404,"preset_not_found"))?)?,&input.parameters)?;
            insert(tx,run,key,&s,"append",&body,&fingerprint)
        })
    }
    pub fn goal_autonomy(&self, goal: &str) -> PResult<Value> {
        self.transaction(|tx| {
            crate::coordination::goal_json(tx, goal)?;
            Ok(json!({"goal_id":goal,"intents":records(tx,goal)?}))
        })
    }
    pub fn reconcile_autonomy(&self) -> PResult<()> {
        let pending=self.transaction(|tx|{
            let mut stmt=tx.prepare("SELECT id,goal_id FROM autonomy_intents WHERE kind='claim' AND state='pending' ORDER BY created_at,rowid LIMIT 64")?;
            Ok(stmt.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?)))?.collect::<Result<Vec<_>,_>>()?)
        })?;
        for (id, goal) in pending {
            let ready = self.transaction(|tx| {
                let intent = records(tx, &goal)?
                    .into_iter()
                    .find(|i| i["id"] == id)
                    .unwrap();
                let command = format!(
                    "task-claim:{}:{}:autonomy-{}",
                    goal,
                    intent["target_task"].as_str().unwrap(),
                    id
                );
                let replay: Option<String> = tx
                    .query_row(
                        "SELECT response FROM commands WHERE key=?",
                        [command],
                        |r| r.get(0),
                    )
                    .optional()?;
                if let Some(body) = replay {
                    let result: Value = serde_json::from_str(&body)?;
                    settle(tx, &intent, "claimed", None, Some(&result))?;
                    return Ok(None);
                }
                if !source_ready(tx, &intent)? {
                    return Ok(None);
                }
                let g = crate::coordination::goal_json(tx, &goal)?;
                let Some(t) = g["tasks"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|t| t["key"] == intent["target_task"])
                else {
                    settle(tx, &intent, "failed", Some("task_not_found"), None)?;
                    return Ok(None);
                };
                if t["state"] != "pending"
                    || !t["run_id"].is_null()
                    || t["claim"]["generation"] != intent["generation"]
                {
                    settle(tx, &intent, "failed", Some("task_claim_conflict"), None)?;
                    return Ok(None);
                }
                let state: String = tx.query_row(
                    "SELECT state FROM instances WHERE id=?",
                    [intent["instance_id"].as_str().unwrap()],
                    |r| r.get(0),
                )?;
                if ["terminated", "draining", "suspended"].contains(&state.as_str()) {
                    settle(
                        tx,
                        &intent,
                        "failed",
                        Some("claim_instance_unavailable"),
                        None,
                    )?;
                    return Ok(None);
                }
                if state != "ready" {
                    return Ok(None);
                }
                for dep in t["depends_on"].as_array().unwrap() {
                    let d = g["tasks"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|t| t["key"] == *dep)
                        .unwrap();
                    if ["failed", "cancelled", "interrupted", "blocked"]
                        .contains(&d["state"].as_str().unwrap_or_default())
                    {
                        settle(tx, &intent, "failed", Some("dependency_failed"), None)?;
                        return Ok(None);
                    }
                    if d["run"]["state"] != "completed" && d["state"] != "completed" {
                        return Ok(None);
                    }
                }
                Ok(Some(intent))
            })?;
            let Some(intent) = ready else { continue };
            let input = ClaimInput {
                expected_round: intent["round"].as_i64().unwrap(),
                expected_generation: intent["generation"].as_i64().unwrap(),
                instance_id: intent["instance_id"].as_str().unwrap().into(),
            };
            let result = self.claim_task(
                &goal,
                intent["target_task"].as_str().unwrap(),
                &input,
                &format!("autonomy-{id}"),
            );
            self.transaction(|tx| match &result {
                Ok(r) => settle(tx, &intent, "claimed", None, Some(r)),
                Err(e) => settle(tx, &intent, "failed", Some(&e.1), None),
            })?;
        }
        Ok(())
    }
}
fn settle(
    tx: &Transaction<'_>,
    intent: &Value,
    state: &str,
    error: Option<&str>,
    result: Option<&Value>,
) -> PResult<()> {
    tx.execute("UPDATE autonomy_intents SET state=?,error=?,result=?,finished_at=? WHERE id=? AND state='pending'",params![state,error,result.map(Value::to_string),now(),intent["id"].as_str().unwrap()])?;
    event(
        tx,
        "autonomy.settled",
        intent["goal_id"].as_str().unwrap(),
        json!({"id":intent["id"],"kind":intent["kind"],"state":state,"error":error}),
    )
}
fn source_ready(tx: &Transaction<'_>, intent: &Value) -> PResult<bool> {
    let goal = intent["goal_id"].as_str().unwrap();
    let gstate: String =
        tx.query_row("SELECT state FROM goals WHERE id=?", [goal], |r| r.get(0))?;
    if gstate != "running" {
        settle(tx, intent, "cancelled", Some("goal_not_running"), None)?;
        return Ok(false);
    }
    if crate::artifacts::round(tx, goal)? != intent["round"] {
        settle(tx, intent, "failed", Some("goal_round_conflict"), None)?;
        return Ok(false);
    }
    let (state, cancel, report): (String, bool, Option<String>) = tx.query_row(
        "SELECT state,cancel_requested,report FROM runs WHERE id=?",
        [intent["source_run_id"].as_str().unwrap()],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    if ["queued", "running"].contains(&state.as_str()) && !cancel {
        return Ok(false);
    }
    let report: Value = report
        .map(|s| serde_json::from_str(&s))
        .transpose()?
        .unwrap_or(Value::Null);
    if state != "completed" || cancel || outcome_unknown(&report) {
        settle(
            tx,
            intent,
            "failed",
            Some("source_run_not_successful"),
            None,
        )?;
        return Ok(false);
    }
    Ok(true)
}
pub(crate) fn advance(tx: &Transaction<'_>, goal: &str) -> PResult<()> {
    for intent in records(tx, goal)?
        .into_iter()
        .filter(|i| i["kind"] == "append" && i["state"] == "pending")
    {
        if !source_ready(tx, &intent)? {
            continue;
        }
        tx.execute_batch("SAVEPOINT autonomous_append;")?;
        let result = activate(tx, &intent);
        match result {
            Ok(result) => {
                tx.execute_batch("RELEASE autonomous_append;")?;
                settle(tx, &intent, "appended", None, Some(&result))?;
            }
            Err(e) => {
                tx.execute_batch("ROLLBACK TO autonomous_append; RELEASE autonomous_append;")?;
                settle(tx, &intent, "failed", Some(&e.1), None)?;
            }
        }
    }
    Ok(())
}
fn activate(tx: &Transaction<'_>, intent: &Value) -> PResult<Value> {
    let input: AppendInput = serde_json::from_value(intent["body"].clone())?;
    let goal = intent["goal_id"].as_str().unwrap();
    let count: usize = tx.query_row(
        "SELECT count(*) FROM work_items WHERE goal_id=?",
        [goal],
        |r| r.get(0),
    )?;
    if count >= crate::limits::TASKS_PER_GOAL {
        return Err(fault(409, "goal_task_limit"));
    }
    let duplicate: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM work_items WHERE goal_id=? AND key=?)",
        params![goal, input.key],
        |r| r.get(0),
    )?;
    if duplicate {
        return Err(fault(409, "task_key_conflict"));
    }
    let mut deps = input.depends_on.clone();
    let source = intent["source_task"].as_str().unwrap();
    if !deps.iter().any(|d| d == source) {
        deps.push(source.into());
    }
    let fleet = crate::fleets::create_in(
        tx,
        &crate::fleets::FleetPlan {
            preset_name: input.preset_name,
            preset_version: input.preset_version,
            parameters: input.parameters,
            count: 1,
            mode: "finite".into(),
            ttl_seconds: Some(1800),
        },
        &format!("autonomy-{}", intent["id"].as_str().unwrap()),
    )?;
    let position: i64 = tx.query_row(
        "SELECT coalesce(max(position),-1)+1 FROM work_items WHERE goal_id=?",
        [goal],
        |r| r.get(0),
    )?;
    tx.execute("INSERT INTO work_items(goal_id,key,position,instance_id,instructions,depends_on,state) VALUES(?,?,?,?,?,?,'pending')",params![goal,input.key,position,intent["instance_id"].as_str().unwrap(),input.instructions,serde_json::to_string(&deps)?])?;
    tx.execute(
        "INSERT INTO goal_rework_allocations(goal_id,round,task_key,fleet_id) VALUES(?,?,?,?)",
        params![
            goal,
            intent["round"].as_i64().unwrap(),
            input.key,
            fleet["fleet_id"].as_str().unwrap()
        ],
    )?;
    let attached = attach_pending_consumers(
        tx,
        goal,
        source,
        &input.key,
        &deps,
        intent["source_run_id"].as_str().unwrap(),
    )?;
    Ok(
        json!({"task_key":input.key,"depends_on":deps,"attached_to":attached,"fleet_id":fleet["fleet_id"],"round":intent["round"]}),
    )
}

fn graph(tx: &Transaction<'_>, goal: &str) -> PResult<Vec<Value>> {
    let mut stmt=tx.prepare("SELECT json_object('key',key,'depends_on',json(depends_on),'state',state,'run_id',run_id) FROM work_items WHERE goal_id=? ORDER BY position")?;
    stmt.query_map([goal], |r| r.get::<_, String>(0))?
        .map(|r| Ok(serde_json::from_str(&r?)?))
        .collect()
}
fn attach_pending_consumers(
    tx: &Transaction<'_>,
    goal: &str,
    source: &str,
    new_key: &str,
    deps: &[String],
    source_run: &str,
) -> PResult<Vec<String>> {
    let tasks = graph(tx, goal)?;
    let mut ancestors: std::collections::HashSet<String> = deps.iter().cloned().collect();
    loop {
        let before = ancestors.len();
        for t in &tasks {
            if ancestors.contains(t["key"].as_str().unwrap()) {
                for d in t["depends_on"].as_array().unwrap() {
                    ancestors.insert(d.as_str().unwrap().into());
                }
            }
        }
        if ancestors.len() == before {
            break;
        }
    }
    let mut stmt = tx.prepare(
        "SELECT target_task FROM autonomy_intents WHERE source_run_id=? AND kind='append'",
    )?;
    let siblings = stmt
        .query_map([source_run], |r| r.get::<_, String>(0))?
        .collect::<Result<std::collections::HashSet<_>, _>>()?;
    drop(stmt);
    let mut attached = Vec::new();
    for t in &tasks {
        let key = t["key"].as_str().unwrap();
        if key == new_key
            || ancestors.contains(key)
            || siblings.contains(key)
            || t["state"] != "pending"
            || !t["run_id"].is_null()
        {
            continue;
        }
        let mut old: Vec<String> = serde_json::from_value(t["depends_on"].clone())?;
        if !old.iter().any(|d| d == source) {
            continue;
        }
        old.push(new_key.into());
        tx.execute(
            "UPDATE work_items SET depends_on=? WHERE goal_id=? AND key=?",
            params![serde_json::to_string(&old)?, goal, key],
        )?;
        attached.push(key.into());
    }
    let tasks = graph(tx, goal)?;
    let mut ordered = std::collections::HashSet::new();
    while ordered.len() < tasks.len() {
        let before = ordered.len();
        for t in &tasks {
            let key = t["key"].as_str().unwrap();
            if ordered.contains(key)
                || !t["depends_on"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|d| ordered.contains(d.as_str().unwrap()))
            {
                continue;
            }
            tx.execute(
                "UPDATE work_items SET position=? WHERE goal_id=? AND key=?",
                params![ordered.len(), goal, key],
            )?;
            ordered.insert(key.to_owned());
        }
        if before == ordered.len() {
            return Err(fault(409, "autonomy_dependency_cycle"));
        }
    }
    Ok(attached)
}
