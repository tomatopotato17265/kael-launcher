pub mod agent;
pub mod cloudflare;
pub mod playit;

use crate::State;
use crate::event::LoadingBarType;
use crate::event::emit::{emit_loading, init_loading};
use crate::state::{HostedServer, PlayitAccount};
use crate::util::fetch::{fetch_advanced, fetch_json, write};
use crate::util::io::IOError;
use chrono::Utc;
use daedalus::minecraft::{
    DownloadType, VERSION_MANIFEST_URL, VersionInfo, VersionManifest,
};
use reqwest::Method;
use std::path::{Path, PathBuf};
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

/// Kills server/daemon processes left over from a previous app run (the app
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

    let stale: Vec<(String, Option<i64>, Option<i64>)> = servers
        .iter()
        .filter(|s| s.server_pid.is_some() || s.agent_pid.is_some())
        .map(|s| (s.id.clone(), s.server_pid, s.agent_pid))
        .collect();

    if stale.is_empty() {
        cleanup_stray_agents(state).await;
        return;
    }

    let playit_secret = state
        .directories
        .playit_dir()
        .join("agent-secret")
        .to_string_lossy()
        .to_string();

    tokio::task::spawn_blocking(move || {
        let mut system = sysinfo::System::new();
        system.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

        for (id, _server_pid, agent_pid) in &stale {
            for (pid, marker) in [(agent_pid, "playitd")] {
                let Some(pid) = pid else { continue };
                let Ok(pid) = u32::try_from(*pid) else { continue };

                if let Some(process) =
                    system.process(sysinfo::Pid::from_u32(pid))
                    && process.cmd().iter().any(|part| {
                        part.to_string_lossy().contains(marker)
                    })
                {
                    tracing::info!(
                        "Killing orphaned {marker} process {pid} from a previous run (server {id})"
                    );
                    process.kill();
                }
            }
        }

        for (_, process) in system.processes() {
            let cmd: Vec<String> = process
                .cmd()
                .iter()
                .map(|part| part.to_string_lossy().into_owned())
                .collect();
            let is_playitd = cmd.iter().any(|part| part.contains("playitd"));
            let uses_our_secret = cmd.iter().any(|part| part.contains(&playit_secret));
            if is_playitd && uses_our_secret {
                tracing::info!(
                    "Killing stray playitd process {} using the launcher secret",
                    process.pid()
                );
                process.kill();
            }
        }
    })
    .await
    .ok();

    for (id, _, _) in servers
        .iter()
        .filter(|s| s.server_pid.is_some() || s.agent_pid.is_some())
        .map(|s| (s.id.clone(), s.server_pid, s.agent_pid))
    {
        if let Err(e) =
            HostedServer::set_pids(&id, None, None, &state.pool).await
        {
            tracing::warn!("Failed to clear stale pids for server {id}: {e}");
        }
    }

    cleanup_stray_agents(state).await;
}

/// Kills any `playitd` processes still running with this install's secret file.
/// More than one daemon per secret makes playit report connected while tunnels
/// stop forwarding reliably.
async fn cleanup_stray_agents(state: &State) {
    let playit_secret = state
        .directories
        .playit_dir()
        .join("agent-secret")
        .to_string_lossy()
        .to_string();

    tokio::task::spawn_blocking(move || {
        let mut system = sysinfo::System::new();
        system.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

        for (_, process) in system.processes() {
            let cmd: Vec<String> = process
                .cmd()
                .iter()
                .map(|part| part.to_string_lossy().into_owned())
                .collect();
            let is_playitd = cmd.iter().any(|part| part.contains("playitd"));
            let uses_our_secret = cmd.iter().any(|part| part.contains(&playit_secret));
            if is_playitd && uses_our_secret {
                tracing::info!(
                    "Killing stray playitd process {} using the launcher secret",
                    process.pid()
                );
                process.kill();
            }
        }
    })
    .await
    .ok();
}

/// Discards a playit account whose agent was deleted on playit.gg's side.
/// The agent's tunnels died with it, so every server's tunnel and custom
/// domain state is cleared too — the next setup + start recreates them.
async fn invalidate_playit_account(state: &State) {
    let owner_key = crate::state::DnsOwner::get_or_create(&state.pool)
        .await
        .ok();
    if let Ok(servers) = HostedServer::get_all(&state.pool).await
        && let Some(backend) = cloudflare::backend_from_env(owner_key)
    {
        for server in servers {
            let Some(custom_domain) = &server.custom_domain else {
                continue;
            };
            let ids = server.cf_record_ids.as_deref().and_then(|json| {
                serde_json::from_str::<cloudflare::DnsRecordIds>(json).ok()
            });
            cloudflare::release_server_records(
                &backend,
                custom_domain,
                ids.as_ref(),
            )
            .await;
        }
    }

    if let Err(e) = HostedServer::clear_all_tunnels(&state.pool).await {
        tracing::warn!("Failed to clear stale tunnel state: {e}");
    }

    if let Err(e) = PlayitAccount::remove(&state.pool).await {
        tracing::warn!("Failed to remove stale playit account: {e}");
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

    emit_loading(&loading_bar, 0.0, Some("Preparing playit agent"))?;
    agent::ensure_playit_daemon(Some((&loading_bar, 30.0))).await?;

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
        playit_tunnel_id: None,
        tunnel_url: None,
        custom_domain: None,
        cf_record_ids: None,
        server_pid: None,
        agent_pid: None,
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
        if let Some(tunnel_id) = &server.playit_tunnel_id
            && let Ok(Some(account)) = PlayitAccount::get(&state.pool).await
            && let Err(e) =
                playit::delete_tunnel(&account.secret_key, tunnel_id).await
        {
            tracing::warn!(
                "Failed to delete playit tunnel {tunnel_id} for server {id}: {e}"
            );
        }

        let owner_key = crate::state::DnsOwner::get_or_create(&state.pool)
            .await
            .ok();
        if let Some(custom_domain) = &server.custom_domain
            && let Some(backend) = cloudflare::backend_from_env(owner_key)
        {
            let ids = server.cf_record_ids.as_deref().and_then(|json| {
                serde_json::from_str::<cloudflare::DnsRecordIds>(json).ok()
            });
            cloudflare::release_server_records(
                &backend,
                custom_domain,
                ids.as_ref(),
            )
            .await;
        }

        let _ =
            crate::util::io::remove_dir_all(PathBuf::from(&server.directory))
                .await;
    }

    HostedServer::remove(&id, &state.pool).await?;
    crate::state::remove_log_buffer(&id);
    Ok(())
}

pub async fn ensure_tunnel(id: &str) -> crate::Result<String> {
    let state = State::get().await?;
    let mut server =
        HostedServer::get(id, &state.pool).await?.ok_or_else(|| {
            crate::ErrorKind::InputError(format!("Server {id} was not found"))
        })?;

    let account =
        PlayitAccount::get(&state.pool).await?.ok_or_else(|| {
            crate::ErrorKind::InputError(
                "playit.gg has not been set up yet".to_string(),
            )
        })?;

    let agent_id =
        playit::wait_for_agent_id(&account.secret_key).await?;

    let endpoint = if let Some(tunnel_id) = &server.playit_tunnel_id {
        let mut endpoint = None;
        let mut recreate = false;

        for _ in 0..3 {
            let Some(tunnel) =
                playit::get_tunnel(&account.secret_key, tunnel_id).await?
            else {
                recreate = true;
                break;
            };

            if let Some(issue) =
                playit::tunnel_routing_issue(&tunnel, &agent_id, server.port)
            {
                tracing::warn!(
                    "playit tunnel {tunnel_id} is misconfigured ({issue}), recreating"
                );
                recreate = true;
                break;
            }

            if let Some(current) = playit::parse_tunnel_endpoint(&tunnel) {
                endpoint = Some(current);
                break;
            }

            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }

        match endpoint {
            Some(endpoint) => endpoint,
            None => {
                if recreate {
                    tracing::info!(
                        "Recreating playit tunnel {tunnel_id} for server {id}"
                    );
                } else {
                    tracing::info!(
                        "playit tunnel {tunnel_id} is no longer allocated, recreating"
                    );
                }
                if let Err(e) = playit::delete_tunnel(
                    &account.secret_key,
                    tunnel_id,
                )
                .await
                {
                    tracing::warn!(
                        "Failed to delete stale playit tunnel {tunnel_id}: {e}"
                    );
                }
                create_playit_tunnel(&state, id, &mut server, &account).await?
            }
        }
    } else {
        create_playit_tunnel(&state, id, &mut server, &account).await?
    };

    if !wait_for_tunnel_forwarding(&endpoint.ip_hostname, endpoint.port).await {
        tracing::warn!(
            "Tunnel {} did not respond to status probes, restarting playit agent",
            endpoint.url()
        );
        restart_shared_agent(&state, &account.secret_key, id).await?;
        if !wait_for_tunnel_forwarding(&endpoint.ip_hostname, endpoint.port).await
        {
            return Err(crate::ErrorKind::OtherError(format!(
                "The playit tunnel at {} is not forwarding traffic to your server. Try stopping and starting the server again, or set up playit.gg again from Settings.",
                endpoint.url()
            ))
            .into());
        }
    }

    let tunnel_url = endpoint.url();
    server.tunnel_url = Some(tunnel_url.clone());
    server.modified = Utc::now().timestamp();
    server.upsert(&state.pool).await?;

    // When DNS is configured the kaelmc domain IS the server's address -
    // the app never shows the raw playit URL - so failing to create it
    // fails the whole start rather than silently degrading
    let owner_key = crate::state::DnsOwner::get_or_create(&state.pool)
        .await
        .ok();
    if let Some(backend) = cloudflare::backend_from_env(owner_key) {
        let is_worker =
            matches!(backend, cloudflare::DnsBackend::Worker { .. });

        // Worker mode re-claims on every start: this keeps the records
        // pointed at the current tunnel and renews the subdomain's
        // ownership lease. Direct mode only creates missing domains.
        if server.custom_domain.is_none() || is_worker {
            let dns_host = endpoint.dns_resolve_host();
            let port = endpoint.port;
            let pinned_ipv4 = endpoint.pinned_ipv4();

            // Domains owned by the user's other servers are off-limits
            // even while those servers are offline
            let reserved: Vec<String> = HostedServer::get_all(&state.pool)
                .await?
                .into_iter()
                .filter(|s| s.id != server.id)
                .filter_map(|s| s.custom_domain)
                .collect();

            // On a worker refresh, keep the subdomain the server already
            // has instead of deriving a fresh one from its name
            let domain_suffix = format!(".{}", backend.domain());
            let claim_name = server
                .custom_domain
                .as_ref()
                .and_then(|d| d.strip_suffix(&domain_suffix))
                .unwrap_or(&server.name)
                .to_string();

            let mut last_error = None;
            for _ in 0..3 {
                match cloudflare::create_server_records(
                    &backend,
                    &claim_name,
                    dns_host,
                    port,
                    &reserved,
                    pinned_ipv4,
                )
                .await
                {
                    Ok((fqdn, ids)) => {
                        server.custom_domain = Some(fqdn);
                        server.cf_record_ids =
                            ids.and_then(|i| serde_json::to_string(&i).ok());
                        server.modified = Utc::now().timestamp();
                        server.upsert(&state.pool).await?;
                        last_error = None;
                        break;
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to create a custom domain for server {id}, retrying: {e}"
                        );
                        last_error = Some(e);
                        tokio::time::sleep(std::time::Duration::from_secs(2))
                            .await;
                    }
                }
            }

            if let Some(e) = last_error {
                return Err(crate::ErrorKind::OtherError(format!(
                    "Could not create the server's public address: {e}"
                ))
                .into());
            }
        }
    }

    Ok(server
        .custom_domain
        .clone()
        .unwrap_or(tunnel_url))
}

async fn create_playit_tunnel(
    state: &State,
    id: &str,
    server: &mut HostedServer,
    account: &PlayitAccount,
) -> crate::Result<playit::TunnelEndpoint> {
    let agent_id =
        resolve_agent_id(state, id, &account.secret_key).await?;
    let tunnel_id = playit::create_tunnel(
        &account.secret_key,
        &server.name,
        &agent_id,
        server.port,
    )
    .await?;
    let endpoint = playit::wait_for_tunnel_endpoint(
        &account.secret_key,
        &tunnel_id,
    )
    .await?;

    server.playit_tunnel_id = Some(tunnel_id);
    Ok(endpoint)
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

    let account = PlayitAccount::get(&state.pool).await?.ok_or_else(|| {
        crate::ErrorKind::InputError(
            "playit.gg must be set up before starting a server".to_string(),
        )
    })?;

    if !playit::agent_alive(&account.secret_key).await? {
        invalidate_playit_account(&state).await;
        return Err(crate::ErrorKind::InputError(
            "The playit agent for this launcher no longer exists — it looks like it was deleted from the playit.gg web portal. Set up playit again to recreate it."
                .to_string(),
        )
        .into());
    }

    let daemon_path = agent::ensure_playit_daemon(None).await?;

    let java_path = match &server.java_path {
        Some(path) => path.clone(),
        None => ensure_java(21).await?,
    };

    let directory = PathBuf::from(&server.directory);
    let max_memory = crate::api::jre::default_memory_max_mb();

    let secret_path = write_secret_file(&state, &account.secret_key).await?;

    if !state.hosting_manager.has_shared_agent().await {
        cleanup_stray_agents(&state).await;
        let agent_child = spawn_daemon(&daemon_path, &secret_path, &id)?;
        state
            .hosting_manager
            .set_shared_agent(agent_child)
            .await;
    }

    playit::wait_for_agent_id(&account.secret_key).await?;

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
    let agent_pid = state.hosting_manager.shared_agent_pid().await;

    state.hosting_manager.insert(id.clone(), server_child);
    SERVER_STARTED.insert(id.clone(), std::time::Instant::now());

    if let Err(e) =
        HostedServer::set_pids(&id, server_pid, agent_pid, &state.pool).await
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

fn spawn_daemon(
    daemon_path: &Path,
    secret_path: &Path,
    id: &str,
) -> crate::Result<tokio::process::Child> {
    let mut command = Command::new(daemon_path);
    command
        .arg("--secret-path")
        .arg(secret_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null())
        .kill_on_drop(true);

    let mut child = command.spawn().map_err(IOError::from)?;
    capture_logs(id, child.stdout.take());
    capture_logs(id, child.stderr.take());
    Ok(child)
}

/// Writes the playit secret to a file so it can be passed to the daemon via
/// `--secret-path` instead of appearing on the command line, where any local
/// process could read it out of `ps`
async fn write_secret_file(
    state: &State,
    secret: &str,
) -> crate::Result<PathBuf> {
    let dir = state.directories.playit_dir();
    crate::util::io::create_dir_all(&dir).await?;

    let path = dir.join("agent-secret");
    tokio::fs::write(&path, secret.as_bytes())
        .await
        .map_err(IOError::from)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(&path)?.permissions();
        permissions.set_mode(0o600);
        std::fs::set_permissions(&path, permissions)?;
    }

    Ok(path)
}

/// Returns the id of a connected playit agent for this account. Tunnels are
/// tied to a registered agent identity, not to a bare secret key, so this
/// reuses the server's already-running daemon when there is one, or briefly
/// spins one up just long enough to register with playit's servers
async fn resolve_agent_id(
    state: &State,
    id: &str,
    secret: &str,
) -> crate::Result<String> {
    if state.hosting_manager.has_shared_agent().await {
        let agent_id = playit::wait_for_agent_id(secret).await?;
        rename_agent_best_effort(secret, &agent_id).await;
        return Ok(agent_id);
    }

    let daemon_path = agent::ensure_playit_daemon(None).await?;
    let secret_path = write_secret_file(state, secret).await?;
    cleanup_stray_agents(state).await;
    let mut child = spawn_daemon(&daemon_path, &secret_path, id)?;

    let result = playit::wait_for_agent_id(secret).await;
    if let Ok(agent_id) = &result {
        rename_agent_best_effort(secret, agent_id).await;
    }

    let _ = child.start_kill();
    let _ = child.wait().await;

    result
}

async fn rename_agent_best_effort(secret: &str, agent_id: &str) {
    if let Err(e) =
        playit::rename_agent(secret, agent_id, playit::AGENT_NAME).await
    {
        tracing::warn!("Failed to rename playit agent {agent_id}: {e}");
    }
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
    // heal a playit daemon that died or wedged while its server kept running
    // (otherwise the server looks online in-app while its tunnel is dead).
    // Runs detached so the poll itself stays fast.
    {
        use std::sync::atomic::{AtomicBool, Ordering};
        static HEALING: AtomicBool = AtomicBool::new(false);

        if !ids.is_empty() && !HEALING.swap(true, Ordering::SeqCst) {
            let ids = ids.clone();
            tokio::spawn(async move {
                heal_daemons(&state, &ids).await;
                HEALING.store(false, Ordering::SeqCst);
            });
        }
    }

    Ok(ids)
}

/// Consecutive tunnel-probe failures per server; a daemon is only declared
/// wedged (and restarted) after several strikes so a transient playit blip
/// doesn't trigger restarts
static TUNNEL_STRIKES: std::sync::LazyLock<dashmap::DashMap<String, u32>> =
    std::sync::LazyLock::new(dashmap::DashMap::new);
const TUNNEL_STRIKE_LIMIT: u32 = 3;

/// When each server's daemon was last restarted by the wedge probe. If the
/// probe is ever wrong about a daemon (e.g. playit-side outage making the
/// tunnel unreachable while the daemon is fine), this stops the heal loop
/// from flapping the tunnel with endless restarts.
static LAST_WEDGE_RESTART: std::sync::LazyLock<
    dashmap::DashMap<String, std::time::Instant>,
> = std::sync::LazyLock::new(dashmap::DashMap::new);
const WEDGE_RESTART_COOLDOWN: std::time::Duration =
    std::time::Duration::from_secs(60);

static SERVER_STARTED: std::sync::LazyLock<
    dashmap::DashMap<String, std::time::Instant>,
> = std::sync::LazyLock::new(dashmap::DashMap::new);
const SERVER_STARTUP_GRACE: std::time::Duration =
    std::time::Duration::from_secs(90);

static LAST_DAEMON_RESTART: std::sync::LazyLock<
    std::sync::Mutex<Option<std::time::Instant>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(None));
const DAEMON_RESTART_COOLDOWN: std::time::Duration =
    std::time::Duration::from_secs(60);

async fn heal_daemons(state: &State, ids: &[String]) {
    if ids.is_empty() {
        return;
    }

    if LAST_DAEMON_RESTART
        .lock()
        .ok()
        .and_then(|guard| *guard)
        .is_some_and(|t| t.elapsed() < DAEMON_RESTART_COOLDOWN)
    {
        return;
    }

    let daemon_exited = state.hosting_manager.shared_agent_needs_restart().await;
    let mut daemon_wedged = false;

    for id in ids {
        if daemon_exited {
            break;
        }

        if SERVER_STARTED
            .get(id)
            .is_some_and(|t| t.elapsed() < SERVER_STARTUP_GRACE)
        {
            continue;
        }

        let Ok(Some(server)) = HostedServer::get(id, &state.pool).await else {
            continue;
        };

        let playit_probe = server.tunnel_url.as_ref().and_then(|tunnel_url| {
            let (host, port_str) = tunnel_url.rsplit_once(':')?;
            let port = port_str.parse().ok()?;
            Some((host.to_owned(), port))
        });

        let custom_probe = if let Some(domain) = &server.custom_domain {
            resolve_custom_domain(domain).await
        } else {
            None
        };

        let playit_ok = if let Some((host, port)) = &playit_probe {
            Some(minecraft_responds(host, *port).await)
        } else {
            None
        };

        let custom_ok = if let Some((host, port)) = &custom_probe {
            Some(minecraft_responds(host, *port).await)
        } else {
            None
        };

        let tunnel_ok = custom_ok.or(playit_ok).unwrap_or(false);

        if playit_ok == Some(true)
            && custom_ok == Some(false)
            && let (Some((playit_host, playit_port)), Some((custom_host, custom_port))) =
                (&playit_probe, &custom_probe)
            && let Some(domain) = &server.custom_domain
        {
            tracing::warn!(
                "Tunnel for server {id} works via playit ({playit_host}:{playit_port}) but not via {domain} ({custom_host}:{custom_port}) — DNS may be stale or misconfigured"
            );
        }

        if tunnel_ok {
            TUNNEL_STRIKES.remove(id);
            continue;
        }

        if !minecraft_responds("127.0.0.1", server.port).await {
            continue;
        }

        let strikes = {
            let mut entry = TUNNEL_STRIKES.entry(id.clone()).or_insert(0);
            *entry += 1;
            *entry
        };

        if strikes < TUNNEL_STRIKE_LIMIT {
            continue;
        }

        TUNNEL_STRIKES.remove(id);

        let recently_restarted = LAST_WEDGE_RESTART
            .get(id)
            .is_some_and(|t| t.elapsed() < WEDGE_RESTART_COOLDOWN);
        if recently_restarted {
            tracing::warn!(
                "Tunnel for server {id} is still unresponsive after a recent daemon restart; waiting before restarting again"
            );
            continue;
        }

        LAST_WEDGE_RESTART.insert(id.clone(), std::time::Instant::now());
        daemon_wedged = true;
        break;
    }

    if !daemon_exited && !daemon_wedged {
        return;
    }

    let restart = async {
        let account = PlayitAccount::get(&state.pool).await?.ok_or_else(|| {
            crate::Error::from(crate::ErrorKind::InputError(
                "playit.gg has not been set up".to_string(),
            ))
        })?;
        let daemon_path = agent::ensure_playit_daemon(None).await?;
        let secret_path = write_secret_file(state, &account.secret_key).await?;
        cleanup_stray_agents(state).await;
        let child = spawn_daemon(&daemon_path, &secret_path, "shared")?;
        crate::Result::Ok(child)
    };

    match restart.await {
        Ok(child) => {
            tracing::info!(
                "Restarted the shared playit daemon ({})",
                if daemon_exited {
                    "previous daemon had exited"
                } else {
                    "tunnel stopped forwarding while servers were still up"
                }
            );
            state.hosting_manager.replace_shared_agent(child).await;
            if let Ok(mut guard) = LAST_DAEMON_RESTART.lock() {
                *guard = Some(std::time::Instant::now());
            }
            if let Some(agent_pid) =
                state.hosting_manager.shared_agent_pid().await
                && let Ok(running) = HostedServer::get_all(&state.pool).await
            {
                for server in running
                    .into_iter()
                    .filter(|s| ids.contains(&s.id))
                {
                    if let Err(e) = HostedServer::set_pids(
                        &server.id,
                        server.server_pid,
                        Some(agent_pid),
                        &state.pool,
                    )
                    .await
                    {
                        tracing::warn!(
                            "Failed to update agent pid for server {}: {e}",
                            server.id
                        );
                    }
                }
            }
        }
        Err(e) => {
            tracing::warn!("Failed to restart the shared playit daemon: {e}");
        }
    }
}

async fn resolve_custom_domain(domain: &str) -> Option<(String, u16)> {
    let (host, port) =
        crate::api::server_address::parse_server_address(domain).ok()?;
    crate::api::server_address::resolve_server_address(host, port)
        .await
        .ok()
}

/// Performs a proper Minecraft status ping. A bare TCP connect is not enough:
/// playit's ingress accepts connections even when the agent behind them is
/// dead, and only resets once data flows.
async fn minecraft_responds(host: &str, port: u16) -> bool {
    const STATUS_PROTOCOL_VERSION: usize = 776;

    let Ok(conn) = async_minecraft_ping::ConnectionConfig::build(host)
        .with_port(port)
        .with_protocol_version(STATUS_PROTOCOL_VERSION)
        .with_timeout(std::time::Duration::from_secs(3))
        .connect()
        .await
    else {
        return false;
    };

    conn.status().await.is_ok()
}

async fn wait_for_tunnel_forwarding(host: &str, port: u16) -> bool {
    for _ in 0..15 {
        if minecraft_responds(host, port).await {
            return true;
        }

        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }

    false
}

async fn restart_shared_agent(
    state: &State,
    secret: &str,
    id: &str,
) -> crate::Result<()> {
    let daemon_path = agent::ensure_playit_daemon(None).await?;
    let secret_path = write_secret_file(state, secret).await?;
    cleanup_stray_agents(state).await;
    let child = spawn_daemon(&daemon_path, &secret_path, id)?;
    state.hosting_manager.replace_shared_agent(child).await;
    playit::wait_for_agent_id(secret).await?;
    Ok(())
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

pub async fn playit_has_account() -> crate::Result<bool> {
    let state = State::get().await?;
    Ok(PlayitAccount::get(&state.pool).await?.is_some())
}

pub async fn playit_begin_claim() -> crate::Result<playit::ClaimInfo> {
    Ok(playit::begin_claim())
}

pub async fn playit_poll_claim(
    code: String,
    guest: bool,
) -> crate::Result<playit::ClaimPoll> {
    let state = State::get().await?;
    let poll = playit::poll_claim(&code, "self-managed").await?;

    if let Some(secret) = &poll.secret {
        let account = PlayitAccount {
            secret_key: secret.clone(),
            account_type: if guest {
                "guest".to_string()
            } else {
                "claimed".to_string()
            },
        };
        account.upsert(&state.pool).await?;

        // Name the agent right away when playit already knows it; if the
        // key only activates once the daemon first connects, the rename in
        // resolve_agent_id covers it instead
        match playit::agent_id(secret).await {
            Ok(agent_id) => {
                rename_agent_best_effort(secret, &agent_id).await;
            }
            Err(e) => {
                tracing::debug!(
                    "Agent not yet available for rename after claim: {e}"
                );
            }
        }
    }

    Ok(poll)
}

pub async fn playit_guest_url() -> crate::Result<Option<String>> {
    let state = State::get().await?;

    match PlayitAccount::get(&state.pool).await? {
        Some(account) => {
            Ok(Some(playit::login_guest(&account.secret_key).await?))
        }
        None => Ok(None),
    }
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
