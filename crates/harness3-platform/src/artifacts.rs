//! Immutable text contributions and bounded, explicitly authorized Goal review rounds.
use crate::{
    domain::*,
    store::{Store, event},
};
use rusqlite::{OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReworkInput {
    pub expected_round: i64,
    pub note: String,
    #[serde(default)]
    pub assignments: Vec<Assignment>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub task_keys: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Assignment {
    pub key: String,
    pub instance_id: String,
}

const META: &str = "json_object('id',id,'goal_id',goal_id,'task_key',task_key,'version',version,'run_id',run_id,'instance_id',instance_id,'media_type',media_type,'digest',digest,'bytes',bytes,'checks',json(checks),'created_at',created_at)";
pub(crate) fn round(tx: &Transaction<'_>, goal: &str) -> PResult<i64> {
    Ok(tx.query_row(
        "SELECT coalesce(max(number),1) FROM goal_rounds WHERE goal_id=?",
        [goal],
        |r| r.get(0),
    )?)
}
pub(crate) fn list_in(tx: &Transaction<'_>, goal: &str) -> PResult<Vec<Value>> {
    let mut stmt = tx.prepare(&format!(
        "SELECT {META} FROM artifacts WHERE goal_id=? ORDER BY version,task_key"
    ))?;
    stmt.query_map([goal], |r| r.get::<_, String>(0))?
        .map(|v| Ok(serde_json::from_str(&v?)?))
        .collect()
}
pub(crate) fn capture(tx: &Transaction<'_>, goal: &str, task: &Value) -> PResult<Option<Value>> {
    if task["run"]["state"] != "completed" || outcome_unknown(&task["run"]["report"]) {
        return Ok(None);
    }
    let run = task["run_id"].as_str().unwrap();
    if crate::delegation::is_planning_run(tx, run)? {
        return Ok(None);
    }
    if let Some(body) = tx
        .query_row(
            &format!("SELECT {META} FROM artifacts WHERE run_id=?"),
            [run],
            |r| r.get::<_, String>(0),
        )
        .optional()?
    {
        return Ok(Some(serde_json::from_str(&body)?));
    }
    let content = task["run"]["report"]["output"].as_str().unwrap_or_default();
    let digest = format!("sha256:{:x}", Sha256::digest(content.as_bytes()));
    let checks = json!([
        {"rule":"non_empty","passed":!content.trim().is_empty(),"scope":"non-whitespace text; no semantic quality judgment"},
        {"rule":"text_size","passed":content.len()<=1_048_576,"scope":"at most 1 MiB of UTF-8 text; original content is retained"}
    ]);
    let artifact = id("artifact");
    tx.execute("INSERT INTO artifacts(id,goal_id,task_key,version,run_id,instance_id,media_type,content,digest,bytes,checks,created_at) VALUES(?,?,?,?,?,?,'text/plain; charset=utf-8',?,?,?,?,?)",params![artifact,goal,task["key"].as_str().unwrap(),round(tx,goal)?,run,task["instance_id"].as_str().unwrap(),content,digest,content.len(),checks.to_string(),task["run"]["finished_at"].as_i64().unwrap_or_else(now)])?;
    event(
        tx,
        "artifact.published",
        &artifact,
        json!({"goal_id":goal,"task_key":task["key"],"run_id":run,"digest":digest,"version":round(tx,goal)?}),
    )?;
    Ok(Some(
        tx.query_row(
            &format!("SELECT {META} FROM artifacts WHERE id=?"),
            [artifact],
            |r| r.get::<_, String>(0),
        )
        .map(|s| serde_json::from_str(&s))??,
    ))
}
pub(crate) fn decorate(tx: &Transaction<'_>, goal: &mut Value) -> PResult<()> {
    let id = goal["id"].as_str().unwrap().to_owned();
    let artifacts = list_in(tx, &id)?;
    for task in goal["tasks"].as_array_mut().unwrap() {
        task["artifact"] = artifacts
            .iter()
            .find(|a| a["run_id"] == task["run_id"])
            .cloned()
            .unwrap_or(Value::Null);
    }
    goal["artifacts"] = json!(artifacts);
    let mut stmt=tx.prepare("SELECT json_object('number',number,'note',note,'state',state,'review',review,'manifest',json(manifest),'tasks',json(tasks),'created_at',created_at,'finished_at',finished_at) FROM goal_rounds WHERE goal_id=? ORDER BY number")?;
    goal["rounds"] = json!(
        stmt.query_map([&id], |r| r.get::<_, String>(0))?
            .map(|v| Ok(serde_json::from_str::<Value>(&v?)?))
            .collect::<PResult<Vec<_>>>()?
    );
    for r in goal["rounds"].as_array_mut().unwrap() {
        let scope: Option<String> = tx
            .query_row(
                "SELECT body FROM goal_rework_scopes WHERE goal_id=? AND round=?",
                params![&id, r["number"].as_i64().unwrap()],
                |row| row.get(0),
            )
            .optional()?;
        r["rework_scope"] = scope
            .map(|body| serde_json::from_str::<Value>(&body))
            .transpose()?
            .unwrap_or(Value::Null);
    }
    Ok(())
}
pub(crate) fn backfill(tx: &Transaction<'_>) -> PResult<()> {
    // Schema upgrades preserve historical contributions; successful Runs alone are eligible.
    let mut stmt=tx.prepare("SELECT w.goal_id,w.key,w.instance_id,w.run_id,r.state,r.report,r.finished_at FROM work_items w JOIN runs r ON r.id=w.run_id WHERE r.state='completed' AND NOT EXISTS(SELECT 1 FROM artifacts a WHERE a.run_id=r.id)")?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, Option<String>>(5)?,
                r.get::<_, Option<i64>>(6)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    for (goal, key, instance, run, state, report, finished) in rows {
        let report: Value = report
            .as_deref()
            .map(serde_json::from_str)
            .transpose()?
            .unwrap_or(Value::Null);
        if !outcome_unknown(&report) {
            capture(
                tx,
                &goal,
                &json!({"key":key,"instance_id":instance,"run_id":run,"run":{"state":state,"report":report,"finished_at":finished}}),
            )?;
        }
    }
    Ok(())
}
impl Store {
    pub fn artifacts(&self, goal: Option<&str>, after: i64, limit: usize) -> PResult<Value> {
        if after < 0 || !(1..=100).contains(&limit) {
            return Err(fault(400, "invalid_artifact_query"));
        }
        self.transaction(|tx|{
            let mut stmt=tx.prepare(&format!("SELECT seq,{META} FROM artifacts WHERE seq>? AND (? IS NULL OR goal_id=?) ORDER BY seq LIMIT ?"))?;
            let mut cursor=after;
            let items=stmt.query_map(params![after,goal,goal,limit],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?)))?.map(|row|{let (seq,body)=row?;cursor=seq;Ok(serde_json::from_str::<Value>(&body)?)}).collect::<PResult<Vec<_>>>()?;
            Ok(json!({"artifacts":items,"next_cursor":cursor}))
        })
    }
    pub fn recent_artifacts(&self) -> PResult<Vec<Value>> {
        self.transaction(|tx| {
            let mut stmt = tx.prepare(&format!(
                "SELECT {META} FROM artifacts ORDER BY seq DESC LIMIT 100"
            ))?;
            stmt.query_map([], |r| r.get::<_, String>(0))?
                .map(|v| Ok(serde_json::from_str(&v?)?))
                .collect()
        })
    }
    pub fn artifact(&self, id: &str) -> PResult<Value> {
        self.transaction(|tx| {
            let (body, content): (String, String) = tx
                .query_row(
                    &format!("SELECT {META},content FROM artifacts WHERE id=?"),
                    [id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?
                .ok_or_else(|| fault(404, "artifact_not_found"))?;
            let mut value: Value = serde_json::from_str(&body)?;
            value["content"] = json!(content);
            value["integrity_verified"] = json!(
                format!("sha256:{:x}", Sha256::digest(content.as_bytes()))
                    == value["digest"].as_str().unwrap_or_default()
            );
            Ok(value)
        })
    }
    pub fn rework_goal(&self, id: &str, input: &ReworkInput, key: &str) -> PResult<Value> {
        self.transaction(|tx| rework_in(tx, id, input, key))
    }
}
pub(crate) fn rework_in(
    tx: &Transaction<'_>,
    id: &str,
    input: &ReworkInput,
    key: &str,
) -> PResult<Value> {
    if key.is_empty()
        || key.len() > 160
        || input.note.trim().is_empty()
        || input.note.len() > 8000
        || !(1..3).contains(&input.expected_round)
        || input.assignments.len() > 16
        || input.task_keys.len() > 16
    {
        return Err(fault(400, "invalid_goal_rework"));
    }

    let command = format!("goal-rework:{id}:{key}");
    let fingerprint = serde_json::to_string(input)?;
    if let Some((old, response)) = tx
        .query_row(
            "SELECT fingerprint,response FROM commands WHERE key=?",
            [&command],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )
        .optional()?
    {
        return if old == fingerprint {
            Ok(serde_json::from_str(&response)?)
        } else {
            Err(fault(409, "idempotency_conflict"))
        };
    }
    let goal = crate::coordination::goal_json(tx, id)?;
    if goal["state"] != "awaiting-review" {
        return Err(fault(409, "goal_not_reviewable"));
    }
    if round(tx, id)? != input.expected_round {
        return Err(fault(409, "goal_round_conflict"));
    }
    let research: Option<String> = tx
        .query_row("SELECT id FROM research_jobs WHERE goal_id=?", [id], |r| {
            r.get(0)
        })
        .optional()?;
    if research.is_some() {
        let active:i64=tx.query_row("SELECT count(*) FROM research_jobs WHERE state IN ('provisioning','planning','running','verifying','cancelling') AND (goal_id IS NULL OR goal_id!=?)",[id],|r|r.get(0))?;
        if active >= 8 {
            return Err(fault(409, "research_quota_exceeded"));
        }
    }
    if research.is_some() && !input.assignments.is_empty() {
        return Err(fault(400, "research_rework_uses_fresh_team"));
    }
    let tasks = goal["tasks"].as_array().unwrap();
    if tasks.iter().any(|t| outcome_unknown(&t["run"]["report"])) {
        return Err(fault(409, "goal_unknown_outcome"));
    }
    let mut selected = std::collections::HashSet::new();
    for key in &input.task_keys {
        if !selected.insert(key.clone()) || !tasks.iter().any(|t| t["key"] == *key) {
            return Err(fault(400, "invalid_rework_task_selection"));
        }
    }
    if selected.is_empty() {
        selected.extend(tasks.iter().map(|t| t["key"].as_str().unwrap().to_owned()));
    }
    loop {
        let before = selected.len();
        for task in tasks {
            if task["depends_on"]
                .as_array()
                .unwrap()
                .iter()
                .any(|d| selected.contains(d.as_str().unwrap()))
            {
                selected.insert(task["key"].as_str().unwrap().to_owned());
            }
        }
        if selected.len() == before {
            break;
        }
    }
    let mut reused = Vec::new();
    for task in tasks
        .iter()
        .filter(|t| !selected.contains(t["key"].as_str().unwrap()))
    {
        let artifact =
            capture(tx, id, task)?.ok_or_else(|| fault(409, "rework_reused_artifact_invalid"))?;
        if !verify_integrity(tx, artifact["id"].as_str().unwrap())?
            || artifact["checks"]
                .as_array()
                .unwrap()
                .iter()
                .any(|c| c["passed"] != true)
        {
            return Err(fault(409, "rework_reused_artifact_invalid"));
        }
        reused.push(artifact);
    }
    let selected_keys: Vec<_> = tasks
        .iter()
        .filter_map(|t| {
            let k = t["key"].as_str().unwrap();
            selected.contains(k).then_some(k)
        })
        .collect();
    let scope = json!({"requested":input.task_keys,"rerun":selected_keys,"reused":reused});
    let mut assigned = std::collections::HashMap::new();
    for assignment in &input.assignments {
        if assigned
            .insert(&assignment.key, &assignment.instance_id)
            .is_some()
            || !selected.contains(&assignment.key)
        {
            return Err(fault(400, "invalid_goal_rework"));
        }
        let state = tx
            .query_row(
                "SELECT state FROM instances WHERE id=?",
                [&assignment.instance_id],
                |r| r.get::<_, String>(0),
            )
            .optional()?
            .ok_or_else(|| fault(404, "instance_not_found"))?;
        let expired: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM instance_deadlines WHERE instance_id=? AND expires_at<=?)",
            params![assignment.instance_id, now()],
            |r| r.get(0),
        )?;
        if state != "ready" || expired {
            return Err(fault(409, "instance_not_ready"));
        }
    }
    if !input.assignments.is_empty() && assigned.len() != selected.len() {
        return Err(fault(400, "invalid_goal_rework"));
    }
    tx.execute("UPDATE goal_rounds SET state='changes-requested',review=?,tasks=?,finished_at=? WHERE goal_id=? AND number=?",params![input.note,serde_json::to_string(tasks)?,now(),id,input.expected_round])?;
    tx.execute(
        "INSERT INTO goal_rounds(goal_id,number,note,state,created_at) VALUES(?,?,?,'running',?)",
        params![id, input.expected_round + 1, input.note, now()],
    )?;
    tx.execute(
        "INSERT INTO goal_rework_scopes(goal_id,round,body) VALUES(?,?,?)",
        params![id, input.expected_round + 1, scope.to_string()],
    )?;
    for task in tasks
        .iter()
        .filter(|t| selected.contains(t["key"].as_str().unwrap()))
    {
        let task_key = task["key"].as_str().unwrap();
        if input.assignments.is_empty() {
            let (name, version, snapshot): (String, String, String) = tx.query_row(
                "SELECT preset_name,preset_version,snapshot FROM instances WHERE id=?",
                [task["instance_id"].as_str().unwrap()],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )?;
            let snapshot: Value = serde_json::from_str(&snapshot)?;
            let fleet = crate::fleets::create_in(
                tx,
                &crate::fleets::FleetPlan {
                    preset_name: name,
                    preset_version: version,
                    count: 1,
                    mode: "finite".into(),
                    ttl_seconds: Some(1800),
                    parameters: snapshot["parameters"]
                        .as_object()
                        .cloned()
                        .unwrap_or_default(),
                },
                &format!("rework-{}", crate::domain::id("allocation")),
            )?;
            tx.execute("INSERT INTO goal_rework_allocations(goal_id,round,task_key,fleet_id) VALUES(?,?,?,?)",params![id,input.expected_round+1,task_key,fleet["fleet_id"].as_str().unwrap()])?;
            tx.execute("UPDATE work_items SET state='pending',run_id=NULL,error=NULL WHERE goal_id=? AND key=?",params![id,task_key])?;
        } else {
            tx.execute("UPDATE work_items SET instance_id=?,state='pending',run_id=NULL,error=NULL WHERE goal_id=? AND key=?",params![assigned[&task_key.to_owned()],id,task_key])?;
        }
    }
    if let Some(research) = research {
        tx.execute(
            "UPDATE research_jobs SET state='running',expires_at=?,error=NULL WHERE id=?",
            params![now() + 900_000, research],
        )?;
        event(
            tx,
            "research.rework_requested",
            &research,
            json!({"goal_id":id,"round":input.expected_round+1}),
        )?;
    }
    tx.execute(
        "UPDATE goals SET state='running',review=NULL,finished_at=NULL WHERE id=?",
        [id],
    )?;
    let response =
        json!({"id":id,"state":"running","round":input.expected_round+1,"rework_scope":scope});
    tx.execute(
        "INSERT INTO commands(key,fingerprint,response) VALUES(?,?,?)",
        params![command, fingerprint, response.to_string()],
    )?;
    event(
        tx,
        "goal.rework_requested",
        id,
        json!({"round":input.expected_round+1,"note":input.note,"assignments":input.assignments,"rework_scope":scope}),
    )?;
    Ok(response)
}

pub(crate) fn resolve_assignment(
    tx: &Transaction<'_>,
    goal: &str,
    key: &str,
) -> PResult<Option<String>> {
    let fleet:Option<String>=tx.query_row("SELECT fleet_id FROM goal_rework_allocations WHERE goal_id=? AND round=? AND task_key=?",params![goal,round(tx,goal)?,key],|r|r.get(0)).optional()?;
    let Some(fleet) = fleet else { return Ok(None) };
    let member: Option<String> = tx
        .query_row(
            "SELECT instance_id FROM fleet_slots WHERE fleet_id=? AND instance_id IS NOT NULL",
            [&fleet],
            |r| r.get(0),
        )
        .optional()?;
    let Some(member) = member else {
        return Ok(Some(String::new()));
    };
    tx.execute(
        "UPDATE work_items SET instance_id=? WHERE goal_id=? AND key=?",
        params![member, goal, key],
    )?;
    let research:Option<(String,String)>=tx.query_row("SELECT j.id,m.instance_id FROM research_jobs j JOIN research_members m ON m.research_id=j.id AND m.role='planner' WHERE j.goal_id=? ORDER BY m.position LIMIT 1",[goal],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    if let Some((research, parent)) = research {
        let position: i64 = tx.query_row(
            "SELECT position FROM work_items WHERE goal_id=? AND key=?",
            params![goal, key],
            |r| r.get(0),
        )?;
        tx.execute("INSERT OR IGNORE INTO research_members(research_id,instance_id,role,position,parent_instance_id) VALUES(?,?,?,?,?)",params![research,member,if key=="synthesis"{"synthesizer"}else{"researcher"},round(tx,goal)?*100+position,parent])?;
    }
    Ok(Some(member))
}
pub(crate) fn retire_allocations(tx: &Transaction<'_>, goal: &str) -> PResult<()> {
    let mut stmt = tx.prepare("SELECT fleet_id FROM goal_rework_allocations WHERE goal_id=?")?;
    let fleets = stmt
        .query_map([goal], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    for fleet in fleets {
        retire_fleet(tx, &fleet)?;
    }
    Ok(())
}

pub(crate) fn retire_fleet(tx: &Transaction<'_>, fleet: &str) -> PResult<()> {
    tx.execute("UPDATE fleets SET state='draining',desired_count=0,revision=revision+1 WHERE id=? AND state NOT IN ('draining','terminated')",[&fleet])?;
    tx.execute(
        "UPDATE fleet_slots SET enabled=0 WHERE fleet_id=?",
        [&fleet],
    )?;
    tx.execute("UPDATE operations SET state='superseded',finished_at=? WHERE fleet_id=? AND state IN ('pending','running')",params![now(),fleet])?;
    let mut stmt = tx.prepare(
        "SELECT id FROM instances WHERE fleet_id=? AND state NOT IN ('terminated','draining')",
    )?;
    let agents = stmt
        .query_map([&fleet], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    for agent in agents {
        crate::lifecycle::drain(tx, &agent, false)?;
    }
    Ok(())
}

pub(crate) fn verify_integrity(tx: &Transaction<'_>, id: &str) -> PResult<bool> {
    let (content, digest): (String, String) = tx.query_row(
        "SELECT content,digest FROM artifacts WHERE id=?",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    Ok(format!("sha256:{:x}", Sha256::digest(content.as_bytes())) == digest)
}
