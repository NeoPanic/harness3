//! Persisted deadlines and graceful retirement, independent of live cancellation tokens.
use crate::{
    domain::*,
    store::{Store, event},
};
use rusqlite::{OptionalExtension, Transaction, params};
use serde_json::{Value, json};

pub(crate) fn drain(tx: &Transaction<'_>, agent: &str, expired: bool) -> PResult<Value> {
    let state: String = tx
        .query_row("SELECT state FROM instances WHERE id=?", [agent], |r| {
            r.get(0)
        })
        .optional()?
        .ok_or_else(|| fault(404, "instance_not_found"))?;
    if state == "terminated" {
        return Ok(json!({"id":agent,"state":state}));
    }
    if expired {
        let mut statement = tx.prepare("SELECT id FROM runs WHERE instance_id=? AND state IN ('queued','running') AND cancel_requested=0")?;
        let runs = statement
            .query_map([agent], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(statement);
        for run in runs {
            event(
                tx,
                "run.cancel_requested",
                &run,
                json!({"reason":"ttl_expired","instance_id":agent}),
            )?;
        }
        tx.execute("UPDATE runs SET state='cancelled',cancel_requested=1,finished_at=? WHERE instance_id=? AND state='queued'", params![now(),agent])?;
        tx.execute(
            "UPDATE runs SET cancel_requested=1 WHERE instance_id=? AND state='running'",
            [agent],
        )?;
    }
    let active: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM runs WHERE instance_id=? AND state IN ('queued','running'))",
        [agent],
        |r| r.get(0),
    )?;
    let next = if active { "draining" } else { "terminated" };
    tx.execute(
        "UPDATE instances SET state=? WHERE id=?",
        params![next, agent],
    )?;
    if state != next {
        event(
            tx,
            "instance.transitioned",
            agent,
            json!({"from":state,"to":next,"reason":if expired {"ttl_expired"} else {"drain"}}),
        )?;
    }
    Ok(json!({"id":agent,"state":next}))
}

pub(crate) fn reconcile(tx: &Transaction<'_>) -> PResult<()> {
    let mut statement = tx.prepare("SELECT i.id FROM instances i JOIN instance_deadlines d ON d.instance_id=i.id WHERE i.state!='terminated' AND d.expires_at<=?")?;
    let agents = statement
        .query_map([now()], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);
    for agent in agents {
        drain(tx, &agent, true)?;
    }
    let mut statement = tx.prepare("SELECT id FROM instances WHERE state='draining' AND NOT EXISTS(SELECT 1 FROM runs WHERE instance_id=instances.id AND state IN ('queued','running'))")?;
    let drained = statement
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);
    for agent in drained {
        drain(tx, &agent, false)?;
    }
    Ok(())
}

impl Store {
    pub fn reconcile_lifecycle(&self) -> PResult<()> {
        self.transaction(reconcile)
    }
}
