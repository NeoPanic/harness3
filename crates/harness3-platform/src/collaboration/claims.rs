//! Transactional task ownership and durable execution attempts for one local coordinator.
use crate::{
    domain::*,
    store::{Store, event},
};
use rusqlite::{OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimInput {
    pub expected_round: i64,
    pub expected_generation: i64,
    pub instance_id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseInput {
    pub expected_round: i64,
    pub expected_generation: i64,
    pub note: String,
}
pub(crate) fn generation(tx: &Transaction<'_>, goal: &str, key: &str) -> PResult<i64> {
    Ok(tx.query_row(
        "SELECT coalesce(max(generation),0) FROM task_attempts WHERE goal_id=? AND task_key=?",
        params![goal, key],
        |r| r.get(0),
    )?)
}
pub(crate) fn policy(tx: &Transaction<'_>, goal: &str, key: &str, primary: &str) -> PResult<Value> {
    let row:Option<(String,String)>=tx.query_row("SELECT candidates,mode FROM task_claim_policies WHERE goal_id=? AND task_key=? AND round=?",params![goal,key,crate::artifacts::round(tx,goal)?],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    let (candidates, mode) = if let Some((candidates, mode)) = row {
        (serde_json::from_str::<Value>(&candidates)?, mode)
    } else {
        (json!([primary]), "automatic".into())
    };
    Ok(json!({"candidates":candidates,"mode":mode,"generation":generation(tx,goal,key)?}))
}
pub(crate) fn create_policy(tx: &Transaction<'_>, goal: &str, task: &TaskPlan) -> PResult<()> {
    let candidates: Vec<_> = std::iter::once(&task.instance_id)
        .chain(task.candidates.iter())
        .collect();
    tx.execute(
        "INSERT INTO task_claim_policies(goal_id,task_key,round,candidates,mode) VALUES(?,?,1,?,?)",
        params![
            goal,
            task.key,
            serde_json::to_string(&candidates)?,
            if task.manual_claim {
                "manual"
            } else {
                "automatic"
            }
        ],
    )?;
    Ok(())
}
pub(crate) enum Selection {
    Ready(String),
    Waiting,
    Blocked(&'static str),
}
pub(crate) fn select(
    tx: &Transaction<'_>,
    goal: &str,
    key: &str,
    primary: &str,
    requested: Option<&str>,
) -> PResult<Selection> {
    let p = policy(tx, goal, key, primary)?;
    let candidates = p["candidates"].as_array().unwrap();
    if let Some(instance) = requested {
        if !candidates.iter().any(|c| c == instance) {
            return Err(fault(403, "task_candidate_not_allowed"));
        }
    } else if p["mode"] == "manual" {
        return Ok(Selection::Waiting);
    }
    let mut live = false;
    let mut expired = false;
    for candidate in candidates {
        let candidate = candidate.as_str().unwrap();
        if requested.is_some_and(|r| r != candidate) {
            continue;
        }
        let (state,deadline):(String,bool)=tx.query_row("SELECT state,EXISTS(SELECT 1 FROM instance_deadlines d WHERE d.instance_id=i.id AND d.expires_at<=?) FROM instances i WHERE i.id=?",params![now(),candidate],|r|Ok((r.get(0)?,r.get(1)?)))?;
        expired |= deadline;
        if state == "ready" && !deadline {
            return Ok(Selection::Ready(candidate.into()));
        }
        live |= !["terminated", "draining"].contains(&state.as_str()) && !deadline;
    }
    if requested.is_some() {
        return Err(fault(409, "instance_not_ready"));
    }
    Ok(if live {
        Selection::Waiting
    } else {
        Selection::Blocked(if expired {
            "instance_expired"
        } else {
            "instance_terminated"
        })
    })
}
pub(crate) fn record(
    tx: &Transaction<'_>,
    goal: &str,
    key: &str,
    instance: &str,
    run: &str,
    manual: bool,
) -> PResult<()> {
    let count: i64 = tx.query_row(
        "SELECT count(*) FROM task_attempts WHERE goal_id=? AND task_key=? AND round=?",
        params![goal, key, crate::artifacts::round(tx, goal)?],
        |r| r.get(0),
    )?;
    if count >= 8 {
        return Err(fault(409, "task_attempt_limit"));
    }
    let generation = generation(tx, goal, key)? + 1;
    let attempt = id("attempt");
    tx.execute("INSERT INTO task_attempts(id,goal_id,task_key,round,generation,instance_id,run_id,origin,created_at) VALUES(?,?,?,?,?,?,?,?,?)",params![attempt,goal,key,crate::artifacts::round(tx,goal)?,generation,instance,run,if manual{"operator"}else{"coordinator"},now()])?;
    event(
        tx,
        "task.claimed",
        goal,
        json!({"task_key":key,"attempt_id":attempt,"generation":generation,"instance_id":instance,"run_id":run,"origin":if manual{"operator"}else{"coordinator"}}),
    )
}
pub(crate) fn decorate(tx: &Transaction<'_>, goal: &str, task: &mut Value) -> PResult<()> {
    let key = task["key"].as_str().unwrap().to_owned();
    task["claim"] = policy(tx, goal, &key, task["instance_id"].as_str().unwrap())?;
    let mut stmt=tx.prepare("SELECT json_object('id',a.id,'round',a.round,'generation',a.generation,'instance_id',a.instance_id,'run_id',a.run_id,'origin',a.origin,'state',CASE WHEN a.released_at IS NOT NULL THEN 'released' ELSE r.state END,'created_at',a.created_at,'started_at',r.started_at,'finished_at',r.finished_at,'released_at',a.released_at,'release_note',a.release_note,'error',json_extract(r.report,'$.error.code')) FROM task_attempts a JOIN runs r ON r.id=a.run_id WHERE a.goal_id=? AND a.task_key=? ORDER BY a.generation DESC")?;
    task["attempts"] = json!(
        stmt.query_map(params![goal, key], |r| r.get::<_, String>(0))?
            .map(|s| Ok(serde_json::from_str::<Value>(&s?)?))
            .collect::<PResult<Vec<_>>>()?
    );
    Ok(())
}
pub(crate) fn backfill(tx: &Transaction<'_>) -> PResult<()> {
    tx.execute("INSERT OR IGNORE INTO task_attempts(id,goal_id,task_key,round,generation,instance_id,run_id,origin,created_at) SELECT 'attempt_'||a.run_id,a.goal_id,a.task_key,a.version,a.version,a.instance_id,a.run_id,'legacy',r.created_at FROM artifacts a JOIN runs r ON r.id=a.run_id",[])?;
    let mut stmt=tx.prepare("SELECT w.goal_id,w.key,w.instance_id,w.run_id,r.created_at FROM work_items w JOIN runs r ON r.id=w.run_id WHERE NOT EXISTS(SELECT 1 FROM task_attempts a WHERE a.run_id=w.run_id)")?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    for (goal, key, instance, run, created) in rows {
        tx.execute("INSERT INTO task_attempts(id,goal_id,task_key,round,generation,instance_id,run_id,origin,created_at) VALUES(?,?,?,?,?,?,?,'legacy',?)",params![format!("attempt_{run}"),goal,key,crate::artifacts::round(tx,&goal)?,generation(tx,&goal,&key)?+1,instance,run,created])?;
    }
    Ok(())
}
fn replay(tx: &Transaction<'_>, command: &str, fingerprint: &str) -> PResult<Option<Value>> {
    let row: Option<(String, String)> = tx
        .query_row(
            "SELECT fingerprint,response FROM commands WHERE key=?",
            [command],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    match row {
        Some((old, response)) if old == fingerprint => Ok(Some(serde_json::from_str(&response)?)),
        Some(_) => Err(fault(409, "idempotency_conflict")),
        None => Ok(None),
    }
}
fn receipt(
    tx: &Transaction<'_>,
    command: &str,
    fingerprint: &str,
    response: Value,
) -> PResult<Value> {
    tx.execute(
        "INSERT INTO commands(key,fingerprint,response) VALUES(?,?,?)",
        params![command, fingerprint, response.to_string()],
    )?;
    Ok(response)
}
fn valid_command(goal: &str, task: &str, key: &str, round: i64, generation: i64) -> PResult<()> {
    if !valid_name(goal)
        || !valid_name(task)
        || key.is_empty()
        || key.len() > 160
        || !(1..=3).contains(&round)
        || generation < 0
    {
        return Err(fault(400, "invalid_task_claim"));
    }
    Ok(())
}
fn current_task(
    tx: &Transaction<'_>,
    goal: &str,
    task: &str,
    round: i64,
    expected: i64,
) -> PResult<Value> {
    let g = crate::coordination::goal_json(tx, goal)?;
    if g["round"] != round {
        return Err(fault(409, "goal_round_conflict"));
    }
    if g["state"] != "running" {
        return Err(fault(409, "goal_not_running"));
    }
    if generation(tx, goal, task)? != expected {
        return Err(fault(409, "task_generation_conflict"));
    }
    g["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["key"] == task)
        .cloned()
        .ok_or_else(|| fault(404, "task_not_found"))
}
impl Store {
    pub fn task(&self, goal: &str, key: &str) -> PResult<Value> {
        self.transaction(|tx| {
            let g = crate::coordination::goal_json(tx, goal)?;
            let mut t = g["tasks"]
                .as_array()
                .unwrap()
                .iter()
                .find(|t| t["key"] == key)
                .cloned()
                .ok_or_else(|| fault(404, "task_not_found"))?;
            t["goal_id"] = json!(goal);
            t["goal_state"] = g["state"].clone();
            t["round"] = g["round"].clone();
            Ok(t)
        })
    }
    pub fn claim_task(
        &self,
        goal: &str,
        task: &str,
        input: &ClaimInput,
        key: &str,
    ) -> PResult<Value> {
        valid_command(
            goal,
            task,
            key,
            input.expected_round,
            input.expected_generation,
        )?;
        if !valid_name(&input.instance_id) {
            return Err(fault(400, "invalid_task_claim"));
        }
        self.transaction(|tx|{
            let command=format!("task-claim:{goal}:{task}:{key}");let fingerprint=serde_json::to_string(input)?;
            if let Some(response)=replay(tx,&command,&fingerprint)?{return Ok(response);}
            let t=current_task(tx,goal,task,input.expected_round,input.expected_generation)?;
            if t["state"]!="pending"||!t["run_id"].is_null(){return Err(fault(409,"task_already_claimed"));}
            let run=crate::coordination::claim_in(tx,goal,task,&input.instance_id)?;
            receipt(tx,&command,&fingerprint,json!({"goal_id":goal,"task_key":task,"run_id":run,"instance_id":input.instance_id,"generation":generation(tx,goal,task)?,"state":"queued"}))
        })
    }
    pub fn release_task(
        &self,
        goal: &str,
        task: &str,
        input: &ReleaseInput,
        key: &str,
    ) -> PResult<Value> {
        valid_command(
            goal,
            task,
            key,
            input.expected_round,
            input.expected_generation,
        )?;
        if input.note.trim().is_empty() || input.note.len() > 2000 {
            return Err(fault(400, "invalid_task_release"));
        }
        self.transaction(|tx|{
            let command=format!("task-release:{goal}:{task}:{key}");let fingerprint=serde_json::to_string(input)?;
            if let Some(response)=replay(tx,&command,&fingerprint)?{return Ok(response);}
            let t=current_task(tx,goal,task,input.expected_round,input.expected_generation)?;
            let run=t["run_id"].as_str().ok_or_else(||fault(409,"task_not_claimed"))?;
            if t["run"]["state"]!="queued"{return Err(fault(409,"task_already_started"));}
            let p=policy(tx,goal,task,t["instance_id"].as_str().unwrap())?;
            let changed=tx.execute("UPDATE runs SET state='cancelled',cancel_requested=1,finished_at=? WHERE id=? AND state='queued' AND started_at IS NULL",params![now(),run])?;
            if changed!=1{return Err(fault(409,"task_already_started"));}
            tx.execute("UPDATE task_attempts SET released_at=?,release_note=? WHERE run_id=?",params![now(),input.note,run])?;
            tx.execute("UPDATE work_items SET state='pending',run_id=NULL,error=NULL WHERE goal_id=? AND key=?",params![goal,task])?;
            tx.execute("UPDATE instances SET state='ready' WHERE id=? AND state='busy'",[t["instance_id"].as_str().unwrap()])?;
            tx.execute("INSERT INTO task_claim_policies(goal_id,task_key,round,candidates,mode) VALUES(?,?,?,?,'manual') ON CONFLICT(goal_id,task_key,round) DO UPDATE SET mode='manual'",params![goal,task,input.expected_round,p["candidates"].to_string()])?;
            event(tx,"task.released",goal,json!({"task_key":task,"run_id":run,"generation":input.expected_generation,"note":input.note}))?;
            receipt(tx,&command,&fingerprint,json!({"goal_id":goal,"task_key":task,"generation":input.expected_generation,"state":"pending","claim_mode":"manual"}))
        })
    }
}
