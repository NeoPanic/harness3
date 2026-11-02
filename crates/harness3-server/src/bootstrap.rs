use crate::api::{App, router};
use fs2::FileExt;
use harness3::CancellationToken;
use harness3_models::Models;
use harness3_platform::store::Store;
use harness3_worker as runtime;
use std::{collections::HashMap, fs::OpenOptions, path::PathBuf, sync::Arc};
use tokio::sync::Mutex;
pub struct Options {
    pub port: u16,
    pub data_dir: PathBuf,
    pub codex_config_dir: Option<PathBuf>,
    pub model: String,
    pub allow_process_plugins: bool,
}
pub async fn up(options: Options) -> crate::ServerResult<()> {
    let Options {
        port,
        data_dir,
        codex_config_dir,
        model,
        allow_process_plugins,
    } = options;
    std::fs::create_dir_all(&data_dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&data_dir, std::fs::Permissions::from_mode(0o700))?;
    }
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(data_dir.join("daemon.lock"))?;
    lock.try_lock_exclusive()
        .map_err(|_| "another daemon already owns this data directory")?;
    let token_path = data_dir.join("token");
    let token = match std::fs::read_to_string(&token_path) {
        Ok(token) => token.trim().to_owned(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            use std::io::Write;
            let token = format!(
                "{}{}",
                uuid::Uuid::new_v4().simple(),
                uuid::Uuid::new_v4().simple()
            );
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&token_path)?;
            file.write_all(token.as_bytes())?;
            token
        }
        Err(error) => return Err(error.into()),
    };
    if token.len() != 64 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("invalid local token file".into());
    }
    let store = Store::open(&data_dir.join("platform.sqlite"))?;
    store.set_workspace_root(&data_dir.join("workspaces"))?;
    let mut models = Models::from_env();
    if let Some(root) = codex_config_dir {
        models.load_codex_responses(&root, &model)?;
    }
    // Reject incomplete classifier configuration before accepting work.
    harness3_platform::routing::Router::from_env(&models)?;
    let plugins = if allow_process_plugins {
        harness3_plugins::ProcessHost::enabled(store.clone(), data_dir.join("plugins"))?
    } else {
        harness3_plugins::ProcessHost::disabled(store.clone())
    };
    runtime::recover(&store).await?;
    let cancellations = Arc::new(Mutex::new(HashMap::new()));
    let shutdown = CancellationToken::new();
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).await?;
    let actual = listener.local_addr()?;
    store.record(
        "platform.started",
        "daemon",
        serde_json::json!({"version":env!("CARGO_PKG_VERSION")}),
    )?;
    println!(
        "Harness³ kernel preview: http://{actual}\nData: {}\nLogin / API token file: {}\nWorker slots: 8 · instance quota: 256\nOpen the console and paste the local token to log in. Ctrl-C drains active runs.",
        data_dir.display(),
        token_path.display()
    );
    let worker = tokio::spawn(runtime::worker_loop_with_plugins(
        store.clone(),
        models.clone(),
        cancellations.clone(),
        shutdown.clone(),
        plugins.clone(),
    ));
    let app = router(App {
        store,
        token,
        cancellations,
        models,
        plugins,
    });
    let stop = shutdown.clone();
    let signal = async move {
        let _ = tokio::signal::ctrl_c().await;
        stop.cancel();
    };
    let result = axum::serve(listener, app)
        .with_graceful_shutdown(signal)
        .await;
    shutdown.cancel();
    worker.await?;
    result?;
    drop(lock);
    Ok(())
}
