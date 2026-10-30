//! Bounded, host-authorized task expansion with a durable parent continuation.
mod tools;
use crate::{
    domain::*,
    store::{Store, event},
};
use rusqlite::{OptionalExtension, Transaction, params};
use serde_json::{Value, json};
pub use tools::{package, tools};

pub use harness3_domain::delegation::{Child, Plan, Policy, Role};
pub(crate) fn validate(policy: &Policy, tx: &Transaction<'_>) -> PResult<()> {
    if !(1..=crate::limits::DELEGATION_DEPTH).contains(&policy.max_depth)
        || !(1..=crate::limits::DELEGATION_CHILDREN).contains(&policy.max_children)
        || !(1..=crate::limits::DELEGATION_DESCENDANTS).contains(&policy.max_descendants)
        || policy.roles.is_empty()
        || policy.roles.len() > 4
        || policy
            .roles
            .iter()
            .map(|r| &r.name)
            .collect::<std::collections::HashSet<_>>()
            .len()
            != policy.roles.len()
    {
        return Err(fault(400, "invalid_delegation_policy"));
    }
    for role in &policy.roles {
        if !valid_name(&role.name) {
            return Err(fault(400, "invalid_delegation_role"));
        }
        let body: String = tx
            .query_row(
                "SELECT body FROM presets WHERE name=? AND version=?",
                params![role.preset_name, role.preset_version],
                |r| r.get(0),
            )
            .optional()?
            .ok_or_else(|| fault(404, "preset_not_found"))?;
        crate::presets::instantiate(serde_json::from_str(&body)?, &role.parameters)?;
    }
    Ok(())
}

pub(crate) fn authorize(tx: &Transaction<'_>, goal: &str, task: &TaskPlan) -> PResult<()> {
    if let Some(policy) = &task.delegation {
        validate(policy, tx)?;
        for agent in std::iter::once(&task.instance_id).chain(task.candidates.iter()) {
            let body: String =
                tx.query_row("SELECT snapshot FROM instances WHERE id=?", [agent], |r| {
                    r.get(0)
                })?;
            if !crate::presets::bindings(&serde_json::from_str(&body)?)
                .iter()
                .any(|p| p == &package())
            {
                return Err(fault(400, "delegation_capability_required"));
            }
        }
        tx.execute("INSERT INTO task_delegation_scopes(goal_id,task_key,root_task,depth,policy) VALUES(?,?,?,0,?)",params![goal,task.key,task.key,serde_json::to_string(policy)?])?;
    }
    Ok(())
}
fn scope(tx: &Transaction<'_>, run: &str) -> PResult<Value> {
    let row:Option<(String,String,String,String)>=tx.query_row("SELECT w.goal_id,w.key,r.instance_id,i.snapshot FROM work_items w JOIN goals g ON g.id=w.goal_id JOIN runs r ON r.id=w.run_id JOIN instances i ON i.id=r.instance_id WHERE r.id=? AND r.state='running' AND r.cancel_requested=0 AND i.state='busy' AND g.state='running'",[run],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
    let (goal, key, agent, snapshot) =
        row.ok_or_else(|| fault(403, "delegation_run_not_authorized"))?;
    if !crate::presets::bindings(&serde_json::from_str(&snapshot)?)
        .iter()
        .any(|p| p == &package())
    {
        return Err(fault(403, "delegation_capability_required"));
    }
    let policy:Option<(String,usize,String)>=tx.query_row("SELECT root_task,depth,policy FROM task_delegation_scopes WHERE goal_id=? AND task_key=?",params![goal,key],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
    let mut result = json!({"goal_id":goal,"task_key":key,"source_run_id":run,"instance_id":agent,"can_delegate":false,"reason":"not_enabled","roles":[]});
    if let Some((root, depth, body)) = policy {
        let policy: Policy = serde_json::from_str(&body)?;
        let used:usize=tx.query_row("SELECT count(*) FROM task_delegation_scopes WHERE goal_id=? AND root_task=? AND depth>0",params![goal,root],|r|r.get(0))?;
        let reserved:usize=tx.query_row("SELECT coalesce(sum(json_array_length(children)),0) FROM task_delegations WHERE goal_id=? AND root_task=? AND state='pending'",params![goal,root],|r|r.get(0))?;
        let total:usize=tx.query_row("SELECT (SELECT count(*) FROM work_items WHERE goal_id=?)+(SELECT coalesce(sum(json_array_length(children)),0) FROM task_delegations WHERE goal_id=? AND state='pending')",params![goal,goal],|r|r.get(0))?;
        let existing: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM task_delegations WHERE goal_id=? AND parent_task=?)",
            params![goal, key],
            |r| r.get(0),
        )?;
        let attempts: usize = tx.query_row(
            "SELECT count(*) FROM task_attempts WHERE goal_id=? AND task_key=? AND round=?",
            params![goal, key, crate::artifacts::round(tx, &goal)?],
            |r| r.get(0),
        )?;
        let remaining = policy
            .max_descendants
            .saturating_sub(used + reserved)
            .min(crate::limits::TASKS_PER_GOAL.saturating_sub(total))
            .min(policy.max_children);
        let reason = if existing {
            "already_delegated"
        } else if depth >= policy.max_depth {
            "depth_limit"
        } else if attempts >= 8 {
            "attempt_limit"
        } else if remaining == 0 {
            "task_limit"
        } else {
            "allowed"
        };
        result["can_delegate"] = json!(reason == "allowed");
        result["reason"] = json!(reason);
        result["root_task"] = json!(root);
        result["depth"] = json!(depth);
        result["remaining_children"] = json!(remaining);
        result["policy"] = serde_json::to_value(&policy)?;
        let mut roles = Vec::new();
        for role in &policy.roles {
            let body: String = tx.query_row(
                "SELECT body FROM presets WHERE name=? AND version=?",
                params![role.preset_name, role.preset_version],
                |r| r.get(0),
            )?;
            let snapshot =
                crate::presets::instantiate(serde_json::from_str(&body)?, &role.parameters)?;
            roles.push(json!({"name":role.name,"preset_name":role.preset_name,"preset_version":role.preset_version,"purpose":snapshot["purpose"],"objective":snapshot["objective"],"protocol":snapshot["protocol"],"plugins":crate::presets::bindings(&snapshot).iter().map(|p|p["id"].clone()).collect::<Vec<_>>()}));
        }
        result["roles"] = json!(roles);
    }
    Ok(result)
}
impl Store {
    pub fn delegation_scope(&self, run: &str) -> PResult<Value> {
        self.transaction(|tx| scope(tx, run))
    }
    pub fn submit_delegation(&self, run: &str, plan: &Plan) -> PResult<Value> {
        if plan.tasks.is_empty()
            || plan.tasks.len() > 4
            || plan.continuation.trim().is_empty()
            || plan.continuation.len() > 4000
            || plan.tasks.iter().any(|t| {
                !valid_name(&t.role)
                    || t.title.trim().is_empty()
                    || t.title.len() > 200
                    || t.instructions.trim().is_empty()
                    || t.instructions.len() > 4000
            })
        {
            return Err(fault(400, "invalid_delegation_plan"));
        }
        self.transaction(|tx|{
            let fingerprint=serde_json::to_string(plan)?;
            let old:Option<(String,String,String)>=tx.query_row("SELECT id,plan,children FROM task_delegations WHERE source_run_id=?",[run],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
            // A committed receipt is safe to retrieve after settlement; it never dispatches again.
            if let Some((id,old,children))=old {return if old==fingerprint{Ok(json!({"id":id,"accepted":true,"children":serde_json::from_str::<Value>(&children)?,"activation":"after_source_run_completes"}))}else{Err(fault(409,"delegation_plan_immutable"))};}
            let scope=scope(tx,run)?;
            if scope["can_delegate"]!=true{return Err(fault(409,&format!("delegation_{}",scope["reason"].as_str().unwrap())));}
            if plan.tasks.len()>scope["remaining_children"].as_u64().unwrap() as usize{return Err(fault(409,"delegation_task_limit"));}
            let policy:Policy=serde_json::from_value(scope["policy"].clone())?;
            let goal=scope["goal_id"].as_str().unwrap();let key=scope["task_key"].as_str().unwrap();let root=scope["root_task"].as_str().unwrap();
            let id=id("delegation");let mut children=Vec::new();
            for child in &plan.tasks {
                let role=policy.roles.iter().find(|r|r.name==child.role).ok_or_else(||fault(403,"delegation_role_not_allowed"))?;
                let fleet=crate::fleets::create_in(tx,&crate::fleets::FleetPlan{preset_name:role.preset_name.clone(),preset_version:role.preset_version.clone(),count:1,mode:"finite".into(),ttl_seconds:Some(1800),parameters:role.parameters.clone()},&crate::domain::id("delegate"))?;
                children.push(json!({"key":crate::domain::id("task"),"role":child.role,"title":child.title,"instructions":child.instructions,"fleet_id":fleet["fleet_id"]}));
            }
            tx.execute("INSERT INTO task_delegations(id,goal_id,parent_task,root_task,round,source_run_id,source_instance_id,depth,plan,children,state,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,'pending',?)",params![id,goal,key,root,crate::artifacts::round(tx,goal)?,run,scope["instance_id"].as_str().unwrap(),scope["depth"].as_i64().unwrap(),fingerprint,serde_json::to_string(&children)?,now()])?;
            event(tx,"task.delegation_submitted",goal,json!({"id":id,"task_key":key,"source_run_id":run,"children":children}))?;
            Ok(json!({"id":id,"accepted":true,"children":children,"activation":"after_source_run_completes"}))
        })
    }
}
pub(crate) fn is_planning_run(tx: &Transaction<'_>, run: &str) -> PResult<bool> {
    Ok(tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM task_delegations WHERE source_run_id=?)",
        [run],
        |r| r.get(0),
    )?)
}
pub(crate) fn pending(tx: &Transaction<'_>, run: &str) -> PResult<bool> {
    Ok(tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM task_delegations WHERE source_run_id=? AND state='pending')",
        [run],
        |r| r.get(0),
    )?)
}
pub(crate) fn advance(tx: &Transaction<'_>, goal: &str, cancelling: bool) -> PResult<()> {
    let mut stmt=tx.prepare("SELECT id,parent_task,root_task,round,source_run_id,source_instance_id,depth,plan,children FROM task_delegations WHERE goal_id=? AND state='pending' ORDER BY created_at,id")?;
    let records = stmt
        .query_map([goal], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, usize>(6)?,
                r.get::<_, String>(7)?,
                r.get::<_, String>(8)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    for (id, parent, root, round, run, agent, depth, _plan, children) in records {
        let children: Vec<Value> = serde_json::from_str(&children)?;
        let (state, cancel): (String, bool) = tx.query_row(
            "SELECT state,cancel_requested FROM runs WHERE id=?",
            [&run],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if cancelling || cancel || ["failed", "interrupted", "cancelled"].contains(&state.as_str())
        {
            abandon(tx, &id, &children, "source_not_successful")?;
            if state == "completed" && !cancelling {
                tx.execute("UPDATE work_items SET state='blocked',error='delegation_source_cancelled' WHERE goal_id=? AND key=?",params![goal,parent])?;
            }
            continue;
        }
        if state != "completed" {
            continue;
        }
        let mut members = Vec::new();
        let mut unavailable = false;
        for child in &children {
            let fleet = child["fleet_id"].as_str().unwrap();
            let row:Option<(String,String,bool)>=tx.query_row("SELECT i.id,i.state,EXISTS(SELECT 1 FROM instance_deadlines d WHERE d.instance_id=i.id AND d.expires_at<=?) FROM fleet_slots s JOIN instances i ON i.id=s.instance_id WHERE s.fleet_id=?",params![now(),fleet],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
            if let Some((id, state, expired)) = row {
                unavailable |= ["terminated", "draining"].contains(&state.as_str()) || expired;
                members.push(id);
            } else {
                let terminal: bool = tx.query_row(
                    "SELECT state IN ('draining','terminated','completed') FROM fleets WHERE id=?",
                    [fleet],
                    |r| r.get(0),
                )?;
                unavailable |= terminal;
            }
        }
        if unavailable {
            abandon(tx, &id, &children, "child_instance_unavailable")?;
            tx.execute("UPDATE work_items SET state='blocked',error='delegation_child_unavailable' WHERE goal_id=? AND key=?",params![goal,parent])?;
            continue;
        }
        if members.len() != children.len() {
            continue;
        }
        let (position, deps): (i64, String) = tx.query_row(
            "SELECT position,depends_on FROM work_items WHERE goal_id=? AND key=? AND run_id=?",
            params![goal, parent, run],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let policy: String = tx.query_row(
            "SELECT policy FROM task_delegation_scopes WHERE goal_id=? AND task_key=?",
            params![goal, parent],
            |r| r.get(0),
        )?;
        tx.execute(
            "UPDATE work_items SET position=position+? WHERE goal_id=? AND position>=?",
            params![children.len(), goal, position],
        )?;
        for (index, (child, member)) in children.iter().zip(&members).enumerate() {
            let key = child["key"].as_str().unwrap();
            tx.execute("INSERT INTO work_items(goal_id,key,position,instance_id,instructions,depends_on,state) VALUES(?,?,?,?,?,?,'pending')",params![goal,key,position+index as i64,member,child["instructions"].as_str().unwrap(),deps])?;
            crate::channels::join_team(tx, goal, member)?;
            tx.execute("INSERT INTO task_delegation_scopes(goal_id,task_key,root_task,depth,policy,parent_task,delegation_id,title) VALUES(?,?,?,?,?,?,?,?)",params![goal,key,root,depth+1,policy,parent,id,child["title"].as_str().unwrap()])?;
        }
        let mut deps: Vec<String> = serde_json::from_str(&deps)?;
        deps.extend(
            children
                .iter()
                .map(|c| c["key"].as_str().unwrap().to_owned()),
        );
        tx.execute("UPDATE work_items SET state='pending',run_id=NULL,error=NULL,depends_on=? WHERE goal_id=? AND key=?",params![serde_json::to_string(&deps)?,goal,parent])?;
        tx.execute("INSERT INTO task_claim_policies(goal_id,task_key,round,candidates,mode) VALUES(?,?,?,?,'automatic') ON CONFLICT(goal_id,task_key,round) DO UPDATE SET candidates=excluded.candidates,mode='automatic'",params![goal,parent,round,json!([agent]).to_string()])?;
        tx.execute(
            "UPDATE task_delegations SET state='expanded',activated_at=? WHERE id=?",
            params![now(), id],
        )?;
        event(
            tx,
            "task.delegation_expanded",
            goal,
            json!({"id":id,"parent_task":parent,"source_run_id":run,"child_count":children.len()}),
        )?;
    }
    Ok(())
}
fn abandon(tx: &Transaction<'_>, id: &str, children: &[Value], error: &str) -> PResult<()> {
    tx.execute(
        "UPDATE task_delegations SET state='abandoned',error=? WHERE id=?",
        params![error, id],
    )?;
    for child in children {
        crate::artifacts::retire_fleet(tx, child["fleet_id"].as_str().unwrap())?;
    }
    event(tx, "task.delegation_abandoned", id, json!({"error":error}))
}
pub(crate) fn retire(tx: &Transaction<'_>, goal: &str) -> PResult<()> {
    let mut stmt = tx.prepare("SELECT children FROM task_delegations WHERE goal_id=?")?;
    let rows = stmt
        .query_map([goal], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    drop(stmt);
    for row in rows {
        for child in serde_json::from_str::<Vec<Value>>(&row)? {
            crate::artifacts::retire_fleet(tx, child["fleet_id"].as_str().unwrap())?;
        }
    }
    Ok(())
}
pub(crate) fn decorate(tx: &Transaction<'_>, goal: &str, task: &mut Value) -> PResult<()> {
    let key = task["key"].as_str().unwrap();
    let row:Option<String>=tx.query_row("SELECT json_object('root_task',root_task,'depth',depth,'parent_task',parent_task,'delegation_id',delegation_id,'title',title,'policy',json(policy)) FROM task_delegation_scopes WHERE goal_id=? AND task_key=?",params![goal,key],|r|r.get(0)).optional()?;
    task["delegation_scope"] = row
        .map(|v| serde_json::from_str::<Value>(&v))
        .transpose()?
        .unwrap_or(Value::Null);
    let mut stmt=tx.prepare("SELECT json_object('id',d.id,'round',d.round,'source_run_id',d.source_run_id,'source_instance_id',d.source_instance_id,'state',d.state,'error',d.error,'source_state',r.state,'children',json(d.children),'continuation',json_extract(d.plan,'$.continuation'),'created_at',d.created_at,'activated_at',d.activated_at) FROM task_delegations d JOIN runs r ON r.id=d.source_run_id WHERE d.goal_id=? AND d.parent_task=?")?;
    task["delegations"] = json!(
        stmt.query_map(params![goal, task["key"].as_str().unwrap()], |r| r
            .get::<_, String>(0))?
            .map(|r| Ok(serde_json::from_str::<Value>(&r?)?))
            .collect::<PResult<Vec<_>>>()?
    );
    if task["delegation_scope"]["title"].is_string() {
        task["display_title"] = task["delegation_scope"]["title"].clone();
    }
    Ok(())
}

pub(crate) fn continuation_prompt(tx: &Transaction<'_>, goal: &str, key: &str) -> PResult<String> {
    let row:Option<(String,String)>=tx.query_row("SELECT source_run_id,json_extract(plan,'$.continuation') FROM task_delegations WHERE goal_id=? AND parent_task=? AND state='expanded'",params![goal,key],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    Ok(row.map(|(run,text)|format!("\nYou previously delegated this task in Run {run}. This is the continuation after all child contributions succeeded. Complete the original task, inspect child evidence and cite their Run IDs. Continuation requirements: {text}\n")).unwrap_or_default())
}
