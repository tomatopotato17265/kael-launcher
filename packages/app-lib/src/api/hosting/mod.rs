pub mod connect;
pub mod paper;

use crate::State;
use crate::event::LoadingBarType;
use crate::event::emit::{emit_loading, init_loading};
use crate::state::{HostedServer, ServerFlavor};
use crate::util::fetch::{fetch_json, write};
use crate::util::io::IOError;
use chrono::Utc;
use daedalus::minecraft::{VERSION_MANIFEST_URL, VersionInfo, VersionManifest};
use reqwest::Method;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use uuid::Uuid;

const DEFAULT_PORT: u16 = 25565;

/// Written into `server.properties` and mirrored into Gate's `status` block,
/// since a full proxy answers server-list pings itself rather than forwarding
/// them to the backend.
pub(crate) const DEFAULT_MAX_PLAYERS: u32 = 20;

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

    struct Stale {
        id: String,
        directory: String,
        server_pid: Option<i64>,
        gate_pid: Option<i64>,
    }

    let stale: Vec<Stale> = servers
        .iter()
        .filter(|s| s.server_pid.is_some() || s.gate_pid.is_some())
        .map(|s| Stale {
            id: s.id.clone(),
            directory: s.directory.clone(),
            server_pid: s.server_pid,
            gate_pid: s.gate_pid,
        })
        .collect();

    if stale.is_empty() {
        cleanup_stray_gates(state).await;
        return;
    }

    let gate_root = connect::root_dir(state).to_string_lossy().to_string();

    let ids: Vec<String> = stale.iter().map(|s| s.id.clone()).collect();
    tokio::task::spawn_blocking(move || {
        let mut system = sysinfo::System::new();
        system.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

        for server in &stale {
            // A leftover Minecraft process keeps the world's session.lock and,
            // more painfully, keeps holding the port — the next start then dies
            // with "Address already in use" partway through world creation,
            // leaving a world that can never be loaded again.
            //
            // Java is launched as `java -jar server.jar nogui` from inside the
            // server directory, so the directory only appears as the process's
            // working directory, never on its command line.
            kill_if_matches(
                &system,
                server.server_pid,
                "server.jar",
                Some(&server.directory),
                &format!("Minecraft server for {}", server.id),
            );

            kill_if_matches(
                &system,
                server.gate_pid,
                connect::PROCESS_MARKER,
                None,
                &format!("Gate tunnel for {}", server.id),
            );
        }

        kill_stray_gates(&mut system, &gate_root);
    })
    .await
    .ok();

    for id in &ids {
        if let Err(e) =
            HostedServer::set_pids(id, None, None, &state.pool).await
        {
            tracing::warn!("Failed to clear stale pids for server {id}: {e}");
        }
    }

    cleanup_stray_gates(state).await;
}

/// Kills `pid` only once it still looks like the process we started — by its
/// command line mentioning `marker`, or by running out of `cwd`. Without that
/// check, a pid reused by an unrelated process since the last run would be
/// killed instead.
fn kill_if_matches(
    system: &sysinfo::System,
    pid: Option<i64>,
    marker: &str,
    cwd: Option<&str>,
    describe: &str,
) {
    let Some(pid) = pid else { return };
    let Ok(pid) = u32::try_from(pid) else { return };
    let Some(process) = system.process(sysinfo::Pid::from_u32(pid)) else {
        return;
    };

    let matches_cmd = process
        .cmd()
        .iter()
        .any(|part| part.to_string_lossy().contains(marker));
    let matches_cwd = cwd.is_some_and(|expected| {
        process
            .cwd()
            .is_some_and(|actual| actual.to_string_lossy() == expected)
    });

    if matches_cmd || matches_cwd {
        tracing::info!(
            "Killing orphaned {describe} (pid {pid}) from a previous run"
        );
        process.kill();
    }
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

    // The vanilla manifest is still the authority on which Java a given
    // Minecraft version needs; Paper builds target the same major.
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

    emit_loading(&loading_bar, 0.0, Some("Downloading Paper server"))?;
    let jar =
        paper::download_jar(&state, &version_id, Some((&loading_bar, 50.0)))
            .await?;

    if !jar.is_stable() {
        emit_loading(
            &loading_bar,
            0.0,
            Some(&format!(
                "Warning: Paper has no stable build for Minecraft \
                 {version_id} yet — using a {} build",
                jar.channel
            )),
        )?;
        tracing::warn!(
            "Using a non-stable Paper build ({}) for Minecraft {version_id}",
            jar.channel
        );
    }

    write(
        &directory.join("server.jar"),
        &jar.bytes,
        &state.io_semaphore,
    )
    .await?;

    write(
        &directory.join("eula.txt"),
        b"eula=true\n",
        &state.io_semaphore,
    )
    .await?;

    // Nothing outside this machine dials this port, so any free one works.
    // Re-checked on every start, since a port free now may not be free later.
    let port = allocate_port().await?;
    write(
        &directory.join("server.properties"),
        default_server_properties(&name, port).as_bytes(),
        &state.io_semaphore,
    )
    .await?;

    // Generating the identity now means the forwarding secret exists before
    // paper-global.yml and the Gate config are ever rendered from it.
    connect::identity(&state, &id).await?;
    write_paper_global(&state, &directory).await?;

    let now = Utc::now().timestamp();
    let server = HostedServer {
        id,
        name,
        directory: directory.to_string_lossy().to_string(),
        mc_version: version_id,
        java_path: Some(java_path),
        port,
        endpoint_name: None,
        flavor: ServerFlavor::Paper,
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

    let linked = connect::start(&state, &server).await?;

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

    let port = ensure_usable_port(&state, &server).await?;

    enforce_tunnel_compatible_properties(
        &state,
        &directory,
        server.flavor,
        port,
    )
    .await?;

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

    if server.flavor == ServerFlavor::Paper {
        write_paper_global(&state, &directory).await?;

        // Paper lets PAPER_VELOCITY_SECRET override the config value, so the
        // secret Gate signs forwarded profiles with never touches disk here.
        let identity = connect::identity(&state, &id).await?;
        server_command.env(
            "PAPER_VELOCITY_SECRET",
            identity.secret.as_deref().unwrap_or_default(),
        );
    }

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

    // A failed start must not leave a half-started server running: the UI
    // would treat it as active and every later Activate would short-circuit
    // on is_running without ever fixing it.
    let started = async {
        wait_for_server_ready(&state, &id, port).await?;
        ensure_tunnel(&id).await
    }
    .await;

    if let Err(e) = started {
        SERVER_STARTED.remove(&id);
        let _ = state.hosting_manager.stop(&id).await;
        let _ = HostedServer::set_pids(&id, None, None, &state.pool).await;
        return Err(e);
    }

    Ok(())
}

/// How long the Minecraft server may take to open its port. Paper's world
/// migration alone pauses 30s, and first-boot world generation can take
/// minutes on a slow machine.
const SERVER_BIND_TIMEOUT: std::time::Duration =
    std::time::Duration::from_secs(240);
const SERVER_BIND_POLL_INTERVAL: std::time::Duration =
    std::time::Duration::from_secs(1);

/// Whether a local server could bind `port` right now.
///
/// Bound on loopback because that is where hosted servers listen, and because
/// a process holding `0.0.0.0:port` blocks a loopback bind too — so this
/// detects both.
async fn port_is_free(port: u16) -> bool {
    tokio::net::TcpListener::bind(("127.0.0.1", port))
        .await
        .is_ok()
}

/// Picks a port for a server to listen on.
///
/// Nothing outside this machine ever dials it — players reach the server
/// through the Connect tunnel, and Gate dials it on loopback — so any free
/// port will do. [`DEFAULT_PORT`] is preferred only because it is the number
/// users recognise, and an ephemeral port is taken when it is busy.
async fn allocate_port() -> crate::Result<u16> {
    if port_is_free(DEFAULT_PORT).await {
        return Ok(DEFAULT_PORT);
    }

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

/// Returns a port this server can actually bind, moving it off its recorded
/// port when something else already holds that one.
///
/// Spawning into an occupied port kills the server partway through creating
/// its world, which leaves that world permanently unloadable ("Overworld
/// settings missing" on every later start). Re-homing the server is strictly
/// better than failing: the port is an internal detail, so nothing a user can
/// see depends on which one it lands on.
async fn ensure_usable_port(
    state: &State,
    server: &HostedServer,
) -> crate::Result<u16> {
    if port_is_free(server.port).await {
        return Ok(server.port);
    }

    let port = allocate_port().await?;
    tracing::info!(
        "Port {} is in use, moving server {} to port {port}",
        server.port,
        server.id
    );
    HostedServer::set_port(&server.id, port, &state.pool).await?;
    Ok(port)
}

/// Waits until the server is accepting connections, or until it exits.
///
/// Polling the port alone is not enough: a server that crashes on startup
/// never opens it, and the wait would otherwise run to timeout and surface as
/// a tunnel failure rather than as the server crash it actually is. The
/// server's own console output carries the reason.
async fn wait_for_server_ready(
    state: &State,
    id: &str,
    port: u16,
) -> crate::Result<()> {
    let deadline = tokio::time::Instant::now() + SERVER_BIND_TIMEOUT;

    loop {
        if connect::probe_local_server(port).await {
            return Ok(());
        }

        if !state.hosting_manager.is_running(id) {
            return Err(crate::ErrorKind::InputError(
                "The Minecraft server stopped before it finished starting. \
                 Check the server console for the reason — a world that failed \
                 to generate has to be deleted before it will start again."
                    .to_string(),
            )
            .into());
        }

        if tokio::time::Instant::now() >= deadline {
            return Err(crate::ErrorKind::InputError(format!(
                "The Minecraft server did not start accepting connections on \
                 port {port} within {}s.",
                SERVER_BIND_TIMEOUT.as_secs()
            ))
            .into());
        }

        tokio::time::sleep(SERVER_BIND_POLL_INTERVAL).await;
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

/// Properties a hosted server must have to be joinable through the Connect
/// tunnel. See [`default_server_properties`] for why.
///
/// `server-port` is enforced rather than merely written once at creation: a
/// server whose port was reallocated must actually listen on the new port, and
/// `server.properties` is the only thing that decides that.
fn required_properties(
    flavor: ServerFlavor,
    port: u16,
) -> Vec<(&'static str, String)> {
    let mut required = vec![
        ("server-port", port.to_string()),
        ("online-mode", "false".to_string()),
        ("enforce-secure-profile", "false".to_string()),
    ];

    // Paper additionally binds to loopback, because with `online-mode=false`
    // the backend's own authentication is off and Gate is the only thing that
    // should be able to reach it. Paper already rejects connections carrying
    // no Velocity forwarding data; this is the second lock.
    if flavor == ServerFlavor::Paper {
        required.push(("server-ip", "127.0.0.1".to_string()));
    }

    required
}

/// The `proxies` block Paper needs in order to accept Velocity forwarding.
///
/// The forwarding secret is deliberately absent: Paper reads
/// `PAPER_VELOCITY_SECRET` from the environment and lets it override the config
/// value, so the secret never has to be written into the server directory.
const PAPER_GLOBAL_PROXIES: &str = "proxies:\n  \
                                    velocity:\n    \
                                    enabled: true\n    \
                                    online-mode: true\n";

/// Rewrites any required property that a server's `server.properties` is
/// missing or has set differently, then leaves everything else untouched.
///
/// Servers created before these settings were understood would otherwise stay
/// permanently unjoinable, and the failure mode is a disconnect several
/// seconds *after* a seemingly successful join, which is nearly impossible for
/// a user to connect back to a stale config file.
async fn enforce_tunnel_compatible_properties(
    state: &State,
    directory: &Path,
    flavor: ServerFlavor,
    port: u16,
) -> crate::Result<()> {
    let path = directory.join("server.properties");
    let Ok(existing) = tokio::fs::read_to_string(&path).await else {
        return Ok(());
    };

    let Some(repaired) = apply_required_properties(
        &existing,
        &required_properties(flavor, port),
    ) else {
        return Ok(());
    };

    tracing::info!(
        "Repairing server.properties at {} for Connect tunnel compatibility",
        path.display()
    );
    write(&path, repaired.as_bytes(), &state.io_semaphore).await?;

    Ok(())
}

/// Writes Paper's `config/paper-global.yml`, creating `config/` as needed.
async fn write_paper_global(
    state: &State,
    directory: &Path,
) -> crate::Result<()> {
    let path = directory.join("config").join("paper-global.yml");
    let existing = tokio::fs::read_to_string(&path).await.unwrap_or_default();

    let Some(repaired) = apply_velocity_forwarding(&existing) else {
        return Ok(());
    };

    write(&path, repaired.as_bytes(), &state.io_semaphore).await?;
    Ok(())
}

/// Returns the repaired contents of `paper-global.yml`, or `None` when the
/// `proxies` block already matches [`PAPER_GLOBAL_PROXIES`].
///
/// Paper backfills every key it needs at boot, so only the `proxies` block has
/// to be owned here — which avoids taking on a YAML parser for one stanza.
/// `proxies` is a top-level key at column zero, so its extent is unambiguous:
/// it runs until the next column-zero line. Everything outside it, including
/// comments and the user's own Paper tuning, is preserved verbatim.
///
/// The tradeoff is that sub-keys of `proxies` other than `velocity` (Paper's
/// `bungee-cord` and `proxy-protocol`) are reset; Paper re-adds their defaults
/// on the next boot.
fn apply_velocity_forwarding(existing: &str) -> Option<String> {
    if existing.trim().is_empty() {
        return Some(PAPER_GLOBAL_PROXIES.to_string());
    }

    let lines: Vec<&str> = existing.lines().collect();
    let Some(start) = lines.iter().position(|line| *line == "proxies:") else {
        let mut out = existing.trim_end().to_string();
        out.push('\n');
        out.push_str(PAPER_GLOBAL_PROXIES);
        return Some(out);
    };

    // The block runs until the next line that starts at column zero.
    let end = lines[start + 1..]
        .iter()
        .position(|line| {
            !line.trim().is_empty() && !line.starts_with([' ', '\t'])
        })
        .map(|offset| start + 1 + offset)
        .unwrap_or(lines.len());

    let current = format!("{}\n", lines[start..end].join("\n"));
    if current == PAPER_GLOBAL_PROXIES {
        return None;
    }

    let mut out = String::new();
    for line in &lines[..start] {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str(PAPER_GLOBAL_PROXIES);
    for line in &lines[end..] {
        out.push_str(line);
        out.push('\n');
    }
    Some(out)
}

/// Returns the repaired contents, or `None` when nothing needed changing.
/// Only the given keys are touched; every other line, including comments and
/// the user's own edits, is preserved verbatim.
fn apply_required_properties(
    existing: &str,
    required: &[(&str, String)],
) -> Option<String> {
    let mut lines: Vec<String> = existing.lines().map(str::to_string).collect();
    let mut changed = false;

    for (key, value) in required {
        let key = *key;
        let desired = format!("{key}={value}");
        match lines
            .iter()
            .position(|line| line.split('=').next() == Some(key))
        {
            Some(index) if lines[index] != desired => {
                lines[index] = desired;
                changed = true;
            }
            None => {
                lines.push(desired);
                changed = true;
            }
            _ => {}
        }
    }

    if !changed {
        return None;
    }

    let mut contents = lines.join("\n");
    contents.push('\n');
    Some(contents)
}

/// Properties for a freshly created (Paper) server.
///
/// `online-mode=false` because Minekube Connect's edge authenticates the player
/// itself before Gate is ever involved — the backend can never complete a
/// second, independent Mojang handshake with that same client. Gate supplies the
/// real, already-authenticated identity over Velocity forwarding instead.
///
/// `enforce-secure-profile=false` governs chat signing, not identity. Real UUIDs
/// flow regardless, and re-enabling it under a proxy-forwarded backend is
/// historically fragile, so it stays off pending its own live test.
///
/// `server-ip=127.0.0.1` keeps the un-authenticating backend off the network;
/// only Gate, on loopback, should be able to reach it.
fn default_server_properties(name: &str, port: u16) -> String {
    let motd = name.replace(['\n', '\r', '='], " ");
    format!(
        "server-port={port}\n\
         server-ip=127.0.0.1\n\
         online-mode=false\n\
         enforce-secure-profile=false\n\
         motd={motd}\n\
         max-players={DEFAULT_MAX_PLAYERS}\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_PORT: u16 = 25565;

    fn paper() -> Vec<(&'static str, String)> {
        required_properties(ServerFlavor::Paper, TEST_PORT)
    }

    #[test]
    fn required_properties_are_added_when_missing() {
        let repaired =
            apply_required_properties("motd=hi\nmax-players=20\n", &paper())
                .expect("missing properties should be added");
        assert!(repaired.contains("online-mode=false"));
        assert!(repaired.contains("enforce-secure-profile=false"));
        assert!(repaired.contains("server-ip=127.0.0.1"));
        assert!(repaired.contains("motd=hi"));
    }

    #[test]
    fn wrong_values_are_corrected_in_place() {
        let repaired = apply_required_properties(
            "online-mode=true\nmotd=hi\nenforce-secure-profile=true\n",
            &paper(),
        )
        .expect("wrong values should be corrected");
        assert!(repaired.contains("online-mode=false"));
        assert!(repaired.contains("enforce-secure-profile=false"));
        assert!(!repaired.contains("online-mode=true"));
        assert!(!repaired.contains("enforce-secure-profile=true"));
    }

    /// A user's own edits and comments must survive the repair untouched.
    #[test]
    fn unrelated_lines_are_preserved_verbatim() {
        let original = "#Minecraft server properties\n\
                        difficulty=hard\n\
                        online-mode=true\n\
                        view-distance=32\n\
                        enforce-secure-profile=false\n";
        let repaired = apply_required_properties(original, &paper()).unwrap();
        assert!(repaired.contains("#Minecraft server properties"));
        assert!(repaired.contains("difficulty=hard"));
        assert!(repaired.contains("view-distance=32"));
    }

    #[test]
    fn already_correct_properties_are_left_alone() {
        let correct = "server-port=25565\nonline-mode=false\n\
                       enforce-secure-profile=false\nserver-ip=127.0.0.1\n";
        assert!(apply_required_properties(correct, &paper()).is_none());
    }

    /// `enforce-secure-profile` must not be mistaken for `online-mode` (or
    /// vice versa) by a prefix match, and a key must match on the full name.
    #[test]
    fn keys_match_exactly_not_by_prefix() {
        let tricky = "online-mode-extra=true\nserver-port=25565\n\
                      online-mode=false\nenforce-secure-profile=false\n\
                      server-ip=127.0.0.1\n";
        assert!(apply_required_properties(tricky, &paper()).is_none());
    }

    #[test]
    fn generated_defaults_need_no_repair() {
        let defaults = default_server_properties("my server", TEST_PORT);
        assert!(apply_required_properties(&defaults, &paper()).is_none());
    }

    /// A server whose port was reallocated must actually listen on the new
    /// port, so `server.properties` has to be rewritten to match.
    #[test]
    fn a_reallocated_port_is_written_into_server_properties() {
        let existing = default_server_properties("my server", 25565);
        let moved = required_properties(ServerFlavor::Paper, 41234);

        let repaired = apply_required_properties(&existing, &moved)
            .expect("a changed port must rewrite server.properties");
        assert!(repaired.contains("server-port=41234"));
        assert!(!repaired.contains("server-port=25565"));
        // The rest of the file is untouched.
        assert!(repaired.contains("motd=my server"));
        assert!(repaired.contains("server-ip=127.0.0.1"));
    }

    /// `server-port` must not collide with Minecraft's own `query.port` or
    /// `rcon.port` keys, which share the "port" substring.
    #[test]
    fn server_port_does_not_clobber_other_port_keys() {
        let existing = "query.port=25565\nrcon.port=25575\n\
                        server-port=25565\nonline-mode=false\n\
                        enforce-secure-profile=false\nserver-ip=127.0.0.1\n";
        let moved = required_properties(ServerFlavor::Paper, 41234);
        let repaired = apply_required_properties(existing, &moved).unwrap();

        assert!(repaired.contains("server-port=41234"));
        assert!(repaired.contains("query.port=25565"));
        assert!(repaired.contains("rcon.port=25575"));
    }

    /// Existing vanilla servers must not gain `server-ip`; binding them to
    /// loopback is a Paper-only decision.
    #[test]
    fn vanilla_servers_do_not_get_a_loopback_bind() {
        let vanilla = required_properties(ServerFlavor::Vanilla, TEST_PORT);
        assert!(vanilla.iter().all(|(key, _)| *key != "server-ip"));

        let existing = "server-port=25565\nonline-mode=false\n\
                        enforce-secure-profile=false\n";
        assert!(apply_required_properties(existing, &vanilla).is_none());
    }

    #[test]
    fn velocity_forwarding_is_written_into_an_empty_file() {
        let repaired = apply_velocity_forwarding("").unwrap();
        assert_eq!(repaired, PAPER_GLOBAL_PROXIES);
    }

    #[test]
    fn velocity_forwarding_is_appended_when_proxies_absent() {
        let existing = "settings:\n  velocity-support: false\n";
        let repaired = apply_velocity_forwarding(existing).unwrap();
        assert!(repaired.starts_with("settings:\n  velocity-support: false\n"));
        assert!(repaired.ends_with(PAPER_GLOBAL_PROXIES));
    }

    /// The block must be bounded at the next column-zero key, not run to EOF.
    #[test]
    fn existing_proxies_block_is_replaced_and_bounded() {
        let existing = "settings:\n  a: 1\n\
                        proxies:\n  \
                        bungee-cord:\n    online-mode: true\n  \
                        velocity:\n    enabled: false\n    online-mode: false\n\
                        chunk-loading:\n  autoconfig-send-distance: true\n";
        let repaired = apply_velocity_forwarding(existing).unwrap();

        assert!(repaired.contains("settings:\n  a: 1\n"));
        assert!(
            repaired
                .contains("chunk-loading:\n  autoconfig-send-distance: true")
        );
        assert!(repaired.contains("enabled: true"));
        assert!(!repaired.contains("enabled: false"));
        // The stale bungee-cord sub-block was inside `proxies` and is gone.
        assert!(!repaired.contains("bungee-cord"));
    }

    #[test]
    fn canonical_proxies_block_needs_no_rewrite() {
        let existing = format!("settings:\n  a: 1\n{PAPER_GLOBAL_PROXIES}");
        assert!(apply_velocity_forwarding(&existing).is_none());
    }

    /// The secret must never be written into the server directory; Paper reads
    /// it from PAPER_VELOCITY_SECRET instead.
    #[test]
    fn paper_global_never_contains_a_secret() {
        assert!(!PAPER_GLOBAL_PROXIES.contains("secret"));
    }

    /// An occupied port must be reported as such, and the port handed out in
    /// its place must actually be bindable — otherwise the server spawns into
    /// a doomed bind and corrupts its world on the way down.
    #[tokio::test]
    async fn a_busy_port_is_detected_and_routed_around() {
        let occupied = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let busy = occupied.local_addr().unwrap().port();

        assert!(!port_is_free(busy).await, "a bound port must not read free");

        let allocated = allocate_port().await.unwrap();
        assert_ne!(allocated, busy);

        // The allocated port must really be usable.
        let claim = tokio::net::TcpListener::bind(("127.0.0.1", allocated))
            .await
            .expect("allocate_port must hand back a bindable port");
        drop(claim);
        drop(occupied);
    }
}
