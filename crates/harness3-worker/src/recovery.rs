use super::*;
pub(super) fn core_error(error: Error) -> Fault {
    fault(500, &error.code)
}

pub async fn recover(store: &Store) -> PResult<()> {
    // The daemon's OS file lock proves no older platform process still owns these sessions.
    for agent in store.active_sessions()? {
        let session = store.load(&agent).await.map_err(core_error)?;
        if let Some(active) = session.active_run {
            harness3::session::recover_interrupted(store, &agent, &active)
                .await
                .map_err(core_error)?;
        }
    }
    store.recover_startup()?;
    store.reconcile_fleets()?;
    store.reconcile_research()?;
    store.reconcile_channels()?;
    store.reconcile_messages()?;
    store.reconcile_autonomy()?;
    store.reconcile_board()?;
    store.reconcile_goals()
}
