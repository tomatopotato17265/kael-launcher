pub mod connect;

use crate::State;
use crate::event::LoadingBarType;
use crate::event::emit::{emit_loading, init_loading};
use crate::state::HostedServer;
use crate::util::fetch::{fetch_advanced, fetch_json, write};
use crate::util::io::IOError;
use chrono::Utc;
use daedalus::minecraft::{
    DownloadType, VERSION_MANIFEST_URL, VersionInfo, VersionManifest,
};
use reqwest::Method;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use uuid::Uuid;

const DEFAULT_PORT: u16 = 25565;

pub async fn list_servers() -> crate::Result<Vec<HostedServer>> {
    let state = State::get().await?;
    cleanup_orphans(&state).await;
    HostedServer::get_all(&state.pool).await
}

/// Kills Gate/server processes left over from a previous app run (the app
/// crashing or restarting without Stop leaves them holding the world's
/// session.lock and the port, making every later start fail). Runs once per
/// app run; pids are verified against their command line before killing so
/// pid reuse can't take out an unrelated process.
async fn cleanup_orphans(state: &State) {
    use std::sync::atomic::{AtomicBool, Ordering};
    static DONE: AtomicBool = AtomicBool::new(false);

    if DONE.swap(true, Ordering::SeqCst) {
        return;
    }

    let servers = match HostedServer::get_all(&state.pool).await {
        Ok(servers) => servers,
        Err(e) => {
            tracing::warn!("Orphan cleanup could not list servers: {e}");
            return;
        }
    };

    let stale: Vec<(String, Option<i64>)> = servers
        .iter()
        .filter(|s| s.server_pid.is_some() || s.gate_pid.is_some())
        .map(|s| (s.id.clone(), s.gate_pid))
        .collect();

    if stale.is_empty() {
        cleanup_stray_gates(state).await;
        return;
    }

    let gate_root = connect::root_dir(state).to_string_lossy().to_string();

    let stale_for_kill = stale.clone();
    tokio::task::spawn_blocking(move || {
        let mut system = sysinfo::System::new();
        system.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

        for (id, gate_pid) in &stale_for_kill {
            let Some(pid) = gate_pid else { continue };
            let Ok(pid) = u32::try_from(*pid) else { continue };

            if let Some(process) = system.process(sysinfo::Pid::from_u32(pid))
                && process.cmd().iter().any(|part| {
                    let part = part.to_string_lossy();
                    part.contains(connect::PROCESS_MARKER)
                        || part.contains(id.as_str())
                })
            {
                tracing::info!(
                    "Killing orphaned Gate process {pid} from a previous run (server {id})"
                );
                process.kill();
            }
        }

        kill_stray_gates(&mut system, &gate_root);
    })
    .await
    .ok();

    for (id, _) in &stale {
        if let Err(e) =
            HostedServer::set_pids(id, None, None, &state.pool).await
        {
            tracing::warn!("Failed to clear stale pids for server {id}: {e}");
        }
    }

    cleanup_stray_gates(state).await;
}

/// Kills any Gate process still running under this install's Gate directory.
/// More than one Gate per endpoint makes Connect report linked while the
/// tunnel stops forwarding reliably.
async fn cleanup_stray_gates(state: &State) {
    let gate_root = connect::root_dir(state).to_string_lossy().to_string();

    tokio::task::spawn_blocking(move || {
        let mut system = sysinfo::System::new();
        system.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        kill_stray_gates(&mut system, &gate_root);
    })
    .await
    .ok();
}

fn kill_stray_gates(system: &mut sysinfo::System, gate_root: &str) {
    for (_, process) in system.processes() {
        let cmd: Vec<String> = process
            .cmd()
            .iter()
            .map(|part| part.to_string_lossy().into_owned())
            .collect();
        let is_gate = cmd
            .iter()
            .any(|part| part.contains(connect::PROCESS_MARKER));
        let is_ours = cmd.iter().any(|part| part.contains(gate_root));
        if is_gate && is_ours {
            tracing::info!(
                "Killing stray Gate process {} from this install",
                process.pid()
            );
            process.kill();
        }
    }
}

/// Stops all running servers and daemons; called when the app exits so no
/// children outlive it
pub async fn shutdown() {
    let Ok(state) = State::get().await else {
        return;
    };

    state.hosting_manager.stop_all().await;

    if let Err(e) = HostedServer::clear_all_pids(&state.pool).await {
        tracing::warn!("Failed to clear hosted server pids on shutdown: {e}");
    }
}

pub async fn create_server(
    name: String,
    version: Option<String>,
) -> crate::Result<HostedServer> {
    let state = State::get().await?;
    let name = name.trim().to_string();

    if name.is_empty() {
        return Err(crate::ErrorKind::InputError(
            "Server name cannot be empty".to_string(),
        )
        .into());
    }

    let loading_bar = init_loading(
        LoadingBarType::ServerDownload { name: name.clone() },
        100.0,
        "Creating server",
    )
    .await?;

    emit_loading(&loading_bar, 0.0, Some("Fetching Minecraft versions"))?;
    let manifest: VersionManifest = fetch_json(
        Method::GET,
        VERSION_MANIFEST_URL,
        None,
        None,
        None,
        &state.fetch_semaphore,
        &state.pool,
    )
    .await?;

    let version_id = version.unwrap_or_else(|| manifest.latest.release.clone());
    let manifest_version = manifest
        .versions
        .iter()
        .find(|v| v.id == version_id)
        .ok_or_else(|| {
            crate::ErrorKind::InputError(format!(
                "Minecraft version {version_id} was not found"
            ))
        })?;

    emit_loading(&loading_bar, 5.0, Some("Fetching version details"))?;
    let info: VersionInfo = fetch_json(
        Method::GET,
        &manifest_version.url,
        None,
        None,
        None,
        &state.fetch_semaphore,
        &state.pool,
    )
    .await?;

    let server_download =
        info.downloads.get(&DownloadType::Server).ok_or_else(|| {
            crate::ErrorKind::InputError(format!(
                "Minecraft {version_id} does not provide a server download"
            ))
        })?;
    let server_url = server_download.url.clone();
    let server_sha1 = server_download.sha1.clone();

    let java_major = info
        .java_version
        .as_ref()
        .map(|j| j.major_version)
        .unwrap_or(21);

    emit_loading(&loading_bar, 5.0, Some("Checking Java"))?;
    let java_path = ensure_java(java_major).await?;

    emit_loading(
        &loading_bar,
        5.0,
        Some("Preparing the Minekube Connect tunnel"),
    )?;
    connect::ensure_binary(&state).await?;

    let id = Uuid::new_v4().to_string();
    let directory = state.directories.server_dir(&id);
    crate::util::io::create_dir_all(&directory).await?;

    emit_loading(&loading_bar, 0.0, Some("Downloading server.jar"))?;
    let jar = fetch_advanced(
        Method::GET,
        &server_url,
        Some(server_sha1.as_str()),
        None,
        None,
        None,
        Some((&loading_bar, 50.0)),
        None,
        &state.fetch_semaphore,
        &state.pool,
    )
    .await?;
    write(&directory.join("server.jar"), &jar, &state.io_semaphore).await?;

    write(
        &directory.join("eula.txt"),
        b"eula=true\n",
        &state.io_semaphore,
    )
    .await?;
    write(
        &directory.join("server.properties"),
        default_server_properties(&name).as_bytes(),
        &state.io_semaphore,
    )
    .await?;

    let now = Utc::now().timestamp();
    let server = HostedServer {
        id,
        name,
        directory: directory.to_string_lossy().to_string(),
        mc_version: version_id,
        java_path: Some(java_path),
        port: DEFAULT_PORT,
        endpoint_name: None,
        server_pid: None,
        gate_pid: None,
        created: now,
        modified: now,
    };
    server.upsert(&state.pool).await?;

    emit_loading(&loading_bar, 10.0, Some("Server created"))?;
    Ok(server)
}

pub async fn remove_server(id: String) -> crate::Result<()> {
    let state = State::get().await?;
    state.hosting_manager.stop(&id).await?;

    if let Some(server) = HostedServer::get(&id, &state.pool).await? {
        let _ =
            crate::util::io::remove_dir_all(PathBuf::from(&server.directory))
                .await;
        let _ =
            crate::util::io::remove_dir_all(connect::server_dir(&state, &id))
                .await;
    }

    HostedServer::remove(&id, &state.pool).await?;
    crate::state::remove_log_buffer(&id);
    Ok(())
}

/// Links (or confirms) this server's Gate tunnel and returns its public
/// address. Idempotent: if the server's Gate process is already running, its
/// existing endpoint is returned rather than relinking.
pub async fn ensure_tunnel(id: &str) -> crate::Result<String> {
    let state = State::get().await?;
    let server =
        HostedServer::get(id, &state.pool).await?.ok_or_else(|| {
            crate::ErrorKind::InputError(format!("Server {id} was not found"))
        })?;

    if state.hosting_manager.is_gate_running(id)
        && let Some(endpoint) = &server.endpoint_name
    {
        return Ok(connect::public_address(endpoint));
    }

    let linked = connect::start(&state, id, server.port).await?;

    if server.endpoint_name.as_deref() != Some(linked.endpoint.as_str()) {
        HostedServer::set_endpoint_name(id, &linked.endpoint, &state.pool)
            .await?;
    }

    let gate_pid = linked.child.id().map(i64::from);
    state
        .hosting_manager
        .insert_gate(id.to_string(), linked.child);

    if let Err(e) =
        HostedServer::set_pids(id, server.server_pid, gate_pid, &state.pool)
            .await
    {
        tracing::warn!("Failed to record Gate pid for server {id}: {e}");
    }

    Ok(linked.address)
}

pub async fn start_server(id: String) -> crate::Result<()> {
    let state = State::get().await?;
    cleanup_orphans(&state).await;

    if state.hosting_manager.is_running(&id) {
        return Ok(());
    }

    let server =
        HostedServer::get(&id, &state.pool).await?.ok_or_else(|| {
            crate::ErrorKind::InputError(format!("Server {id} was not found"))
        })?;

    let java_path = match &server.java_path {
        Some(path) => path.clone(),
        None => ensure_java(21).await?,
    };

    let directory = PathBuf::from(&server.directory);
    let max_memory = crate::api::jre::default_memory_max_mb();

    let mut server_command = Command::new(&java_path);
    server_command
        .current_dir(&directory)
        .arg(format!("-Xmx{max_memory}M"))
        .arg("-jar")
        .arg("server.jar")
        .arg("nogui")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::piped())
        .kill_on_drop(true);

    let mut server_child = server_command.spawn().map_err(IOError::from)?;
    capture_logs(&id, server_child.stdout.take());
    capture_logs(&id, server_child.stderr.take());

    let server_pid = server_child.id().map(i64::from);

    state.hosting_manager.insert(id.clone(), server_child);
    SERVER_STARTED.insert(id.clone(), std::time::Instant::now());

    if let Err(e) =
        HostedServer::set_pids(&id, server_pid, server.gate_pid, &state.pool)
            .await
    {
        tracing::warn!("Failed to record pids for server {id}: {e}");
    }

    // A failed tunnel setup must not leave a half-started server running:
    // the UI would treat it as active and every later Activate would
    // short-circuit on is_running without ever fixing the tunnel
    if let Err(e) = ensure_tunnel(&id).await {
        SERVER_STARTED.remove(&id);
        let _ = state.hosting_manager.stop(&id).await;
        let _ = HostedServer::set_pids(&id, None, None, &state.pool).await;
        return Err(e);
    }

    Ok(())
}

pub async fn stop_server(id: String) -> crate::Result<()> {
    let state = State::get().await?;
    state.hosting_manager.stop(&id).await?;
    SERVER_STARTED.remove(&id);

    if let Err(e) = HostedServer::set_pids(&id, None, None, &state.pool).await {
        tracing::warn!("Failed to clear pids for server {id}: {e}");
    }

    Ok(())
}

pub async fn server_status(id: String) -> crate::Result<bool> {
    let state = State::get().await?;
    Ok(state.hosting_manager.is_running(&id))
}

pub async fn running_servers() -> crate::Result<Vec<String>> {
    let state = State::get().await?;
    let ids = state.hosting_manager.running_ids();

    // The UI polls this every few seconds, which makes it a natural spot to
    // notice a server's Gate tunnel has died while the server itself kept
    // running (otherwise the server looks online in-app while unreachable
    // to friends). Runs detached so the poll itself stays fast.
    {
        use std::sync::atomic::{AtomicBool, Ordering};
        static HEALING: AtomicBool = AtomicBool::new(false);

        if !ids.is_empty() && !HEALING.swap(true, Ordering::SeqCst) {
            let ids = ids.clone();
            tokio::spawn(async move {
                heal_gates(&ids).await;
                HEALING.store(false, Ordering::SeqCst);
            });
        }
    }

    Ok(ids)
}

/// When each server's Gate tunnel was last relinked by the heal check. If a
/// server's local Minecraft process is itself unhealthy, Gate will keep
/// exiting immediately; the cooldown stops that from becoming a relink loop.
static LAST_GATE_RESTART: std::sync::LazyLock<
    dashmap::DashMap<String, std::time::Instant>,
> = std::sync::LazyLock::new(dashmap::DashMap::new);
const GATE_RESTART_COOLDOWN: std::time::Duration =
    std::time::Duration::from_secs(60);

static SERVER_STARTED: std::sync::LazyLock<
    dashmap::DashMap<String, std::time::Instant>,
> = std::sync::LazyLock::new(dashmap::DashMap::new);
const SERVER_STARTUP_GRACE: std::time::Duration =
    std::time::Duration::from_secs(90);

/// Relinks any running server whose Gate process has died. Unlike playit's
/// shared daemon, each server supervises its own Gate child directly, so
/// liveness is an exact process check rather than a network probe that can
/// be transiently wrong — no strike counter is needed, just a cooldown.
async fn heal_gates(ids: &[String]) {
    let Ok(state) = State::get().await else {
        return;
    };

    for id in ids {
        if SERVER_STARTED
            .get(id)
            .is_some_and(|t| t.elapsed() < SERVER_STARTUP_GRACE)
        {
            continue;
        }

        if state.hosting_manager.is_gate_running(id) {
            continue;
        }

        if LAST_GATE_RESTART
            .get(id)
            .is_some_and(|t| t.elapsed() < GATE_RESTART_COOLDOWN)
        {
            continue;
        }

        tracing::warn!("Gate tunnel for server {id} is not running; relinking");
        LAST_GATE_RESTART.insert(id.clone(), std::time::Instant::now());

        if let Err(e) = ensure_tunnel(id).await {
            tracing::warn!(
                "Failed to relink the Gate tunnel for server {id}: {e}"
            );
        }
    }
}

pub async fn get_logs(id: String) -> crate::Result<Vec<String>> {
    Ok(crate::state::get_log_buffer(&id))
}

pub async fn send_command(id: String, command: String) -> crate::Result<()> {
    let state = State::get().await?;
    let command = command.trim().to_string();

    if command.is_empty() {
        return Ok(());
    }

    crate::state::push_log_line(&id, format!("> {command}"));
    state.hosting_manager.send_command(&id, &command).await
}

async fn ensure_java(major: u32) -> crate::Result<String> {
    if let Some(java) = crate::api::jre::find_filtered_jres(Some(major))
        .await?
        .into_iter()
        .next()
    {
        return Ok(java.path);
    }

    let path = crate::api::jre::auto_install_java(major).await?;
    Ok(path.to_string_lossy().to_string())
}

fn capture_logs<R>(id: &str, reader: Option<R>)
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    if let Some(reader) = reader {
        let id = id.to_string();
        tokio::spawn(async move {
            let mut lines = BufReader::new(reader).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                crate::state::push_log_line(&id, line);
            }
        });
    }
}

fn default_server_properties(name: &str) -> String {
    let motd = name.replace(['\n', '\r', '='], " ");
    format!(
        "server-port={DEFAULT_PORT}\nonline-mode=true\nmotd={motd}\nmax-players=20\n"
    )
}
