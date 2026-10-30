use super::runner::execute;
use super::*;
pub async fn worker_loop(
    store: Store,
    models: Models,
    cancellations: Cancellations,
    shutdown: CancellationToken,
) {
    let plugins = harness3_plugins::ProcessHost::disabled(store.clone());
    worker_loop_with_plugins(store, models, cancellations, shutdown, plugins).await
}

pub async fn worker_loop_with_plugins(
    store: Store,
    models: Models,
    cancellations: Cancellations,
    shutdown: CancellationToken,
    plugins: harness3_plugins::ProcessHost,
) {
    let router = match harness3_platform::routing::Router::from_env(&models) {
        Ok(router) => router,
        Err(error) => {
            eprintln!("Task routing unavailable: {error}");
            return;
        }
    };
    worker_loop_with_router(store, models, cancellations, shutdown, plugins, router).await;
}

pub async fn worker_loop_with_router(
    store: Store,
    models: Models,
    cancellations: Cancellations,
    shutdown: CancellationToken,
    plugins: harness3_plugins::ProcessHost,
    router: harness3_platform::routing::Router,
) {
    let mut tasks = tokio::task::JoinSet::new();
    let route_store = store.clone();
    let route_models = models.clone();
    let route_stop = shutdown.clone();
    let processes = plugins.is_enabled();
    let routing = tokio::spawn(async move {
        loop {
            if route_stop.is_cancelled() {
                break;
            }
            if let Err(error) = router
                .route_pending(&route_store, &route_models, processes, route_stop.clone())
                .await
            {
                eprintln!("Task routing failed: {error}");
            }
            tokio::select! {_=route_stop.cancelled()=>break,_=tokio::time::sleep(Duration::from_millis(300))=>{}}
        }
    });
    loop {
        if shutdown.is_cancelled() {
            break;
        }
        if let Err(error) = store.reconcile_fleets() {
            eprintln!("Fleet reconciliation failed: {error}");
        }
        {
            let tokens = cancellations.lock().await;
            for (run, token) in tokens.iter() {
                if store.cancel_requested(run).unwrap_or(true) {
                    token.cancel();
                }
            }
        }
        if let Err(error) = store.reconcile_research() {
            eprintln!("Research coordination failed: {error}");
        }
        if let Err(error) = store.reconcile_channels() {
            eprintln!("channel reconciliation: {}", error.1);
        }
        if let Err(error) = store.reconcile_messages() {
            eprintln!("Message reconciliation failed: {error}");
        }
        if let Err(error) = store.reconcile_autonomy() {
            eprintln!("task autonomy reconciliation: {}", error.1);
        }
        if let Err(error) = store.reconcile_goals() {
            eprintln!("Goal coordination failed: {error}");
        }
        if let Err(error) = store.reconcile_board() {
            eprintln!("Task board reconciliation failed: {error}");
        }
        if tasks.len() < harness3_platform::limits::WORKERS {
            match store.claim() {
                Ok(Some(work)) => {
                    let run = work["id"].as_str().unwrap_or_default().to_string();
                    let agent = work["instance_id"].as_str().unwrap_or_default().to_string();
                    let cancel = shutdown.child_token();
                    cancellations
                        .lock()
                        .await
                        .insert(run.clone(), cancel.clone());
                    let store = store.clone();
                    let cancellations = cancellations.clone();
                    let models = models.clone();
                    let plugins = plugins.clone();
                    tasks.spawn(async move {
                        if store.cancel_requested(&run).unwrap_or(true) {
                            cancel.cancel();
                        }
                        let outcome = execute(&store, &models, &work, cancel, &plugins).await;
                        let report = match outcome {
                            Ok(report) => report,
                            Err(error) => json!({"error":{"code":error.1}}),
                        };
                        let state = match report["error"]["code"].as_str() {
                            None => "completed",
                            Some("cancelled") => "cancelled",
                            Some(_) => "failed",
                        };
                        if let Err(error) = store.finish(&run, &agent, state, report) {
                            eprintln!("Cannot settle {run}: {error}");
                        }
                        cancellations.lock().await.remove(&run);
                    });
                    continue;
                }
                Ok(None) => {}
                Err(error) => eprintln!("Queue claim failed: {error}"),
            }
        }
        tokio::select! {
            _=shutdown.cancelled()=>break,
            _=tokio::time::sleep(Duration::from_millis(150))=>{},
            _=tasks.join_next(),if !tasks.is_empty()=>{},
        }
    }
    // Cancellation is cooperative; drain all started Harness calls before releasing the lock.
    shutdown.cancel();
    if let Err(error) = routing.await {
        eprintln!("Task router failed: {error}");
    }
    while let Some(result) = tasks.join_next().await {
        if let Err(error) = result {
            eprintln!("Worker failed: {error}");
        }
    }
    if let Err(error) = store.reconcile_board() {
        eprintln!("Task board settlement failed: {error}");
    }
}
