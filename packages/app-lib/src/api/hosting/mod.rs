pub mod connect;
pub mod freedomchat;
pub mod paper;

use crate::State;
use crate::event::HostingPayloadType;
use crate::event::LoadingBarType;
use crate::event::emit::{emit_hosting, emit_loading, init_loading};
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
        max_players: DEFAULT_MAX_PLAYERS,
        view_distance: DEFAULT_VIEW_DISTANCE as u32,
        server_pid: None,
        gate_pid: None,
        created: now,
        modified: now,
    };

    write(
        &directory.join("server.properties"),
        initial_server_properties(&server).as_bytes(),
        &state.io_semaphore,
    )
    .await?;

    // Generating the identity now means the forwarding secret exists before
    // paper-global.yml and the Gate config are ever rendered from it.
    connect::identity(&state, &server.id).await?;
    write_paper_global(&state, &directory).await?;
    ensure_freedomchat(&state, &directory, &server.mc_version).await?;

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

const MIN_VIEW_DISTANCE: u32 = 3;
const MAX_VIEW_DISTANCE: u32 = 32;
const MIN_MAX_PLAYERS: u32 = 1;
const MAX_MAX_PLAYERS: u32 = 1000;

/// Saves a server's editable settings. The properties file is rewritten
/// immediately so a stopped server is already correct; a running server picks
/// the changes up on its next start (Minecraft reads `server.properties` and
/// Gate reads its status/config only at startup), so callers should tell the
/// user a restart is needed for a live server.
pub async fn update_server(
    id: String,
    name: String,
    max_players: u32,
    view_distance: u32,
) -> crate::Result<HostedServer> {
    let state = State::get().await?;

    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(crate::ErrorKind::InputError(
            "Server name cannot be empty".to_string(),
        )
        .into());
    }

    let mut server =
        HostedServer::get(&id, &state.pool).await?.ok_or_else(|| {
            crate::ErrorKind::InputError(format!("Server {id} was not found"))
        })?;

    server.name = name;
    server.max_players = max_players.clamp(MIN_MAX_PLAYERS, MAX_MAX_PLAYERS);
    server.view_distance =
        view_distance.clamp(MIN_VIEW_DISTANCE, MAX_VIEW_DISTANCE);
    server.modified = Utc::now().timestamp();
    server.upsert(&state.pool).await?;

    enforce_managed_properties(
        &state,
        &PathBuf::from(&server.directory),
        &server,
    )
    .await?;

    Ok(server)
}

/// Sets the server's icon from an uploaded image of any common format,
/// resizing it to the 64x64 PNG Minecraft requires. Players see it in their
/// server list once they add the server's address there.
pub async fn set_server_icon(
    id: String,
    image_bytes: Vec<u8>,
) -> crate::Result<()> {
    let state = State::get().await?;
    let server =
        HostedServer::get(&id, &state.pool).await?.ok_or_else(|| {
            crate::ErrorKind::InputError(format!("Server {id} was not found"))
        })?;

    let png = tokio::task::spawn_blocking(move || icon_to_png(&image_bytes))
        .await
        .map_err(|e| {
            crate::ErrorKind::InputError(format!(
                "Failed to process the image: {e}"
            ))
        })??;

    write(
        &PathBuf::from(&server.directory).join("server-icon.png"),
        &png,
        &state.io_semaphore,
    )
    .await?;

    Ok(())
}

/// Removes the server's custom icon, reverting it to the default.
pub async fn clear_server_icon(id: String) -> crate::Result<()> {
    let state = State::get().await?;
    let server =
        HostedServer::get(&id, &state.pool).await?.ok_or_else(|| {
            crate::ErrorKind::InputError(format!("Server {id} was not found"))
        })?;

    let path = PathBuf::from(&server.directory).join("server-icon.png");
    let _ = tokio::fs::remove_file(&path).await;
    Ok(())
}

/// Returns the server's current icon as a `data:` URI, or `None` if it has
/// none, so the settings UI can preview it.
pub async fn get_server_icon(id: String) -> crate::Result<Option<String>> {
    use base64::Engine;

    let state = State::get().await?;
    let server =
        HostedServer::get(&id, &state.pool).await?.ok_or_else(|| {
            crate::ErrorKind::InputError(format!("Server {id} was not found"))
        })?;

    let path = PathBuf::from(&server.directory).join("server-icon.png");
    match tokio::fs::read(&path).await {
        Ok(bytes) => {
            let encoded =
                base64::engine::general_purpose::STANDARD.encode(bytes);
            Ok(Some(format!("data:image/png;base64,{encoded}")))
        }
        Err(_) => Ok(None),
    }
}

/// Decodes an uploaded image and re-encodes it as the exact 64x64 PNG a
/// Minecraft server icon must be. Runs on a blocking thread — decoding an
/// attacker-sized image is CPU-bound.
fn icon_to_png(bytes: &[u8]) -> crate::Result<Vec<u8>> {
    use image::imageops::FilterType;

    let image = image::load_from_memory(bytes).map_err(|e| {
        crate::ErrorKind::InputError(format!(
            "That image couldn't be read; try a PNG or JPEG: {e}"
        ))
    })?;

    let resized = image.resize_exact(64, 64, FilterType::Lanczos3);

    let mut out = std::io::Cursor::new(Vec::new());
    resized
        .write_to(&mut out, image::ImageFormat::Png)
        .map_err(|e| {
            crate::ErrorKind::InputError(format!(
                "Failed to encode the server icon: {e}"
            ))
        })?;

    Ok(out.into_inner())
}

/// Changes a server's Minecraft version by downloading the matching Paper jar.
///
/// The world is kept. Minecraft upgrades a world in place on first load and the
/// change is one-way; a downgrade is unsupported and can corrupt the world, so
/// the UI warns before calling this. Requires the server to be stopped, since
/// its jar is being replaced underneath it.
pub async fn change_version(
    id: String,
    version: String,
) -> crate::Result<HostedServer> {
    let state = State::get().await?;

    if state.hosting_manager.is_running(&id) {
        return Err(crate::ErrorKind::InputError(
            "Stop the server before changing its Minecraft version."
                .to_string(),
        )
        .into());
    }

    let mut server =
        HostedServer::get(&id, &state.pool).await?.ok_or_else(|| {
            crate::ErrorKind::InputError(format!("Server {id} was not found"))
        })?;

    // The vanilla manifest is the authority on which Java the version needs;
    // Paper targets the same major.
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

    let manifest_version = manifest
        .versions
        .iter()
        .find(|v| v.id == version)
        .ok_or_else(|| {
        crate::ErrorKind::InputError(format!(
            "Minecraft version {version} was not found"
        ))
    })?;

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

    let java_major = info
        .java_version
        .as_ref()
        .map(|j| j.major_version)
        .unwrap_or(21);
    let java_path = ensure_java(java_major).await?;

    // Errors clearly if this version has no Paper build.
    let jar = paper::download_jar(&state, &version, None).await?;
    write(
        &PathBuf::from(&server.directory).join("server.jar"),
        &jar.bytes,
        &state.io_semaphore,
    )
    .await?;

    // The FreedomChat jar was resolved for the old version; drop it so the next
    // start fetches one matching the new Minecraft version.
    remove_freedomchat_jars(&PathBuf::from(&server.directory)).await?;

    server.mc_version = version;
    server.java_path = Some(java_path);
    server.modified = Utc::now().timestamp();
    server.upsert(&state.pool).await?;

    Ok(server)
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

    let mut server =
        HostedServer::get(&id, &state.pool).await?.ok_or_else(|| {
            crate::ErrorKind::InputError(format!("Server {id} was not found"))
        })?;

    let java_path = match &server.java_path {
        Some(path) => path.clone(),
        None => ensure_java(21).await?,
    };

    let directory = PathBuf::from(&server.directory);
    let max_memory = crate::api::jre::default_memory_max_mb();

    // Reallocation persists the new port; reflect it locally so the properties
    // and Gate config rendered below use it too.
    server.port = ensure_usable_port(&state, &server).await?;

    enforce_managed_properties(&state, &directory, &server).await?;

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
        ensure_freedomchat(&state, &directory, &server.mc_version).await?;

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
        wait_for_server_ready(&state, &id, server.port).await?;
        ensure_tunnel(&id).await
    }
    .await;

    if let Err(e) = started {
        SERVER_STARTED.remove(&id);
        let _ = state.hosting_manager.stop(&id).await;
        let _ = HostedServer::set_pids(&id, None, None, &state.pool).await;
        return Err(e);
    }

    emit_hosting(&id, HostingPayloadType::Started).await?;

    // The only place `Stopped` is emitted, so it fires uniformly whether the
    // server was stopped via `stop_server` or crashed on its own.
    let watch_id = id.clone();
    let watch_state = state.clone();
    tokio::spawn(async move {
        while watch_state.hosting_manager.is_running(&watch_id) {
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
        if let Err(e) =
            emit_hosting(&watch_id, HostingPayloadType::Stopped).await
        {
            tracing::warn!(
                "Failed to emit hosting stopped event for {watch_id}: {e}"
            );
        }
    });

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

/// The `server.properties` keys Kael owns, computed from a server's stored
/// settings. Everything not listed here — the user's own edits, comments — is
/// left untouched by [`apply_managed_properties`].
///
/// These are enforced on every start, not written once, for two reasons: a
/// reallocated port and an edited setting must actually reach the running
/// server, and the tunnel-compatibility keys must never drift (getting
/// `online-mode` wrong makes the server silently unjoinable). Editing a value
/// here and restarting is the whole mechanism behind the settings UI.
fn managed_properties(server: &HostedServer) -> Vec<(&'static str, String)> {
    // Newlines/`=` would corrupt the properties line or inject keys.
    let motd = server.name.replace(['\n', '\r', '='], " ");

    let mut props = vec![
        ("server-port", server.port.to_string()),
        ("online-mode", "false".to_string()),
        ("enforce-secure-profile", "false".to_string()),
        ("max-players", server.max_players.to_string()),
        ("view-distance", server.view_distance.to_string()),
        // Simulation radius is a CPU lever, not a bandwidth one; keep it at or
        // below the view distance so it is never the larger of the two.
        (
            "simulation-distance",
            server
                .view_distance
                .min(DEFAULT_SIMULATION_DISTANCE as u32)
                .to_string(),
        ),
        ("motd", motd),
    ];

    // Paper binds to loopback, because with `online-mode=false` the backend's
    // own authentication is off and Gate is the only thing that should reach
    // it. Paper already rejects connections carrying no Velocity forwarding
    // data; this is the second lock.
    //
    // Compression is disabled here because the only hop between Paper and Gate
    // is loopback, where it buys nothing — leaving it on would compress every
    // packet three times on the one machine already running the game. Gate
    // compresses on the link that actually crosses the internet instead; see
    // the `compression` block in `connect::render_config`.
    if server.flavor == ServerFlavor::Paper {
        props.push(("server-ip", "127.0.0.1".to_string()));
        props.push(("network-compression-threshold", "-1".to_string()));
    }

    props
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

/// Rewrites any managed property that a server's `server.properties` is missing
/// or has set differently, then leaves everything else untouched.
///
/// Servers whose settings changed (or that predate a managed key) would
/// otherwise run with stale values; for the tunnel keys the failure mode is a
/// disconnect several seconds *after* a seemingly successful join, which is
/// nearly impossible to trace back to a stale config file.
async fn enforce_managed_properties(
    state: &State,
    directory: &Path,
    server: &HostedServer,
) -> crate::Result<()> {
    let path = directory.join("server.properties");
    let Ok(existing) = tokio::fs::read_to_string(&path).await else {
        return Ok(());
    };

    let Some(repaired) =
        apply_required_properties(&existing, &managed_properties(server))
    else {
        return Ok(());
    };

    tracing::info!(
        "Repairing server.properties at {} to match saved settings",
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

/// True for a filename that is a FreedomChat plugin jar, e.g.
/// `FreedomChat-Paper-1.7.9.jar`.
fn is_freedomchat_jar(file_name: &str) -> bool {
    let lower = file_name.to_ascii_lowercase();
    lower.starts_with("freedomchat") && lower.ends_with(".jar")
}

/// Whether `plugins/` already holds a FreedomChat jar.
async fn has_freedomchat_jar(plugins: &Path) -> bool {
    let Ok(mut entries) = crate::util::io::read_dir(plugins).await else {
        return false;
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        if is_freedomchat_jar(&entry.file_name().to_string_lossy()) {
            return true;
        }
    }
    false
}

/// Drops the FreedomChat plugin into `plugins/` so Minecraft 1.19+ secure chat
/// can't silently discard the messages players send each other.
///
/// Connect's managed edge re-injects each player's profile, which breaks the
/// signature chain clients use to validate peer chat; FreedomChat sidesteps it
/// by rewriting chat as system messages. See [`freedomchat`] for the full why.
///
/// Idempotent (skips when a jar is already present) and never fatal: a
/// Minecraft version FreedomChat hasn't shipped for only logs a warning, since
/// a running server with broken chat still beats a server that won't start.
async fn ensure_freedomchat(
    state: &State,
    directory: &Path,
    mc_version: &str,
) -> crate::Result<()> {
    let plugins = directory.join("plugins");
    if has_freedomchat_jar(&plugins).await {
        return Ok(());
    }

    match freedomchat::download_plugin(state, mc_version).await? {
        Some(jar) => {
            crate::util::io::create_dir_all(&plugins).await?;
            write(
                &plugins.join(&jar.filename),
                &jar.bytes,
                &state.io_semaphore,
            )
            .await?;
        }
        None => {
            tracing::warn!(
                "No FreedomChat build is available for Minecraft {mc_version}; \
                 hosted-server chat may drop the messages players send each \
                 other."
            );
        }
    }

    Ok(())
}

/// Removes any FreedomChat jar from `plugins/`, so a Minecraft version change
/// re-resolves a matching build on the next start rather than keeping a jar
/// built for the old version.
async fn remove_freedomchat_jars(directory: &Path) -> crate::Result<()> {
    let plugins = directory.join("plugins");
    let Ok(mut entries) = crate::util::io::read_dir(&plugins).await else {
        return Ok(());
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        if is_freedomchat_jar(&entry.file_name().to_string_lossy()) {
            crate::util::io::remove_file(entry.path()).await?;
        }
    }
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

/// Default chunk render radius for a new server.
///
/// A hosted server streams every player's chunks up the host's home internet
/// connection, which is asymmetric and far slower than a datacentre link. The
/// chunk count grows with the square of the view distance — 10 means 441
/// chunks per join, 8 means 289 — so this is the single biggest lever on how
/// long a distant player waits to see the world. It is editable per server;
/// this is only the starting point.
const DEFAULT_VIEW_DISTANCE: u8 = 8;
const DEFAULT_SIMULATION_DISTANCE: u8 = 6;

/// The initial `server.properties` for a freshly created server.
///
/// Every key Kael manages is written from [`managed_properties`] so the file
/// starts consistent with what the server is enforced to on each start;
/// Minecraft fills in every other key with its own defaults on first boot.
fn initial_server_properties(server: &HostedServer) -> String {
    let mut contents = String::new();
    for (key, value) in managed_properties(server) {
        contents.push_str(&format!("{key}={value}\n"));
    }
    contents
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_PORT: u16 = 25565;

    fn test_server(flavor: ServerFlavor, port: u16) -> HostedServer {
        HostedServer {
            id: "test".to_string(),
            name: "my server".to_string(),
            directory: "/tmp/test".to_string(),
            mc_version: "1.21".to_string(),
            java_path: None,
            port,
            endpoint_name: None,
            flavor,
            max_players: 20,
            view_distance: DEFAULT_VIEW_DISTANCE as u32,
            server_pid: None,
            gate_pid: None,
            created: 0,
            modified: 0,
        }
    }

    fn paper() -> Vec<(&'static str, String)> {
        managed_properties(&test_server(ServerFlavor::Paper, TEST_PORT))
    }

    #[test]
    fn required_properties_are_added_when_missing() {
        let repaired = apply_required_properties("difficulty=hard\n", &paper())
            .expect("missing properties should be added");
        assert!(repaired.contains("online-mode=false"));
        assert!(repaired.contains("enforce-secure-profile=false"));
        assert!(repaired.contains("server-ip=127.0.0.1"));
        assert!(repaired.contains("max-players=20"));
        // Unmanaged keys are still preserved.
        assert!(repaired.contains("difficulty=hard"));
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

    /// A user's own edits and comments must survive the repair untouched;
    /// only managed keys change.
    #[test]
    fn unrelated_lines_are_preserved_verbatim() {
        let original = "#Minecraft server properties\n\
                        difficulty=hard\n\
                        online-mode=true\n\
                        spawn-protection=16\n\
                        enforce-secure-profile=false\n";
        let repaired = apply_required_properties(original, &paper()).unwrap();
        assert!(repaired.contains("#Minecraft server properties"));
        assert!(repaired.contains("difficulty=hard"));
        assert!(repaired.contains("spawn-protection=16"));
        // A managed key was still corrected.
        assert!(repaired.contains("online-mode=false"));
        assert!(!repaired.contains("online-mode=true"));
    }

    #[test]
    fn already_correct_properties_are_left_alone() {
        let correct = initial_server_properties(&test_server(
            ServerFlavor::Paper,
            TEST_PORT,
        ));
        assert!(apply_required_properties(&correct, &paper()).is_none());
    }

    /// A key must match on its full name, so `online-mode-extra` is not taken
    /// for `online-mode`.
    #[test]
    fn keys_match_exactly_not_by_prefix() {
        let correct = initial_server_properties(&test_server(
            ServerFlavor::Paper,
            TEST_PORT,
        ));
        let tricky = format!("online-mode-extra=true\n{correct}");
        assert!(apply_required_properties(&tricky, &paper()).is_none());
    }

    #[test]
    fn generated_defaults_need_no_repair() {
        let defaults = initial_server_properties(&test_server(
            ServerFlavor::Paper,
            TEST_PORT,
        ));
        assert!(apply_required_properties(&defaults, &paper()).is_none());
    }

    /// A server whose port was reallocated must actually listen on the new
    /// port, so `server.properties` has to be rewritten to match.
    #[test]
    fn a_reallocated_port_is_written_into_server_properties() {
        let existing =
            initial_server_properties(&test_server(ServerFlavor::Paper, 25565));
        let moved =
            managed_properties(&test_server(ServerFlavor::Paper, 41234));

        let repaired = apply_required_properties(&existing, &moved)
            .expect("a changed port must rewrite server.properties");
        assert!(repaired.contains("server-port=41234"));
        assert!(!repaired.contains("server-port=25565"));
        // The rest of the file is untouched.
        assert!(repaired.contains("motd=my server"));
        assert!(repaired.contains("server-ip=127.0.0.1"));
    }

    /// An edited player cap and view distance must reach the properties file.
    #[test]
    fn edited_settings_are_written_into_server_properties() {
        let existing = initial_server_properties(&test_server(
            ServerFlavor::Paper,
            TEST_PORT,
        ));

        let mut edited = test_server(ServerFlavor::Paper, TEST_PORT);
        edited.name = "Renamed".to_string();
        edited.max_players = 8;
        edited.view_distance = 6;

        let repaired =
            apply_required_properties(&existing, &managed_properties(&edited))
                .expect("changed settings must rewrite server.properties");
        assert!(repaired.contains("max-players=8"));
        assert!(repaired.contains("view-distance=6"));
        assert!(repaired.contains("motd=Renamed"));
    }

    /// Simulation distance must never exceed the (possibly lowered) view
    /// distance.
    #[test]
    fn simulation_distance_is_capped_at_view_distance() {
        let mut server = test_server(ServerFlavor::Paper, TEST_PORT);
        server.view_distance = 4;
        let props = managed_properties(&server);
        let sim = props
            .iter()
            .find(|(k, _)| *k == "simulation-distance")
            .map(|(_, v)| v.as_str());
        assert_eq!(sim, Some("4"));
    }

    /// `server-port` must not collide with Minecraft's own `query.port` or
    /// `rcon.port` keys, which share the "port" substring.
    #[test]
    fn server_port_does_not_clobber_other_port_keys() {
        let existing = "query.port=25565\nrcon.port=25575\n\
                        server-port=25565\nonline-mode=false\n\
                        enforce-secure-profile=false\nserver-ip=127.0.0.1\n\
                        network-compression-threshold=-1\n";
        let moved =
            managed_properties(&test_server(ServerFlavor::Paper, 41234));
        let repaired = apply_required_properties(existing, &moved).unwrap();

        assert!(repaired.contains("server-port=41234"));
        assert!(repaired.contains("query.port=25565"));
        assert!(repaired.contains("rcon.port=25575"));
    }

    /// Vanilla servers must not gain the Paper-only loopback bind or
    /// compression override.
    #[test]
    fn vanilla_servers_stay_off_paper_only_keys() {
        let vanilla =
            managed_properties(&test_server(ServerFlavor::Vanilla, TEST_PORT));
        assert!(vanilla.iter().all(|(key, _)| *key != "server-ip"));
        assert!(
            vanilla
                .iter()
                .all(|(key, _)| *key != "network-compression-threshold")
        );
    }

    /// A Minecraft server icon must be exactly a 64x64 PNG regardless of what
    /// the user uploaded, so any-size input has to come back out at 64x64.
    #[test]
    fn icon_is_resized_to_a_64x64_png() {
        // A deliberately not-64x64, not-square source.
        let source = image::RgbaImage::from_pixel(10, 30, image::Rgba([9; 4]));
        let mut source_png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(source)
            .write_to(&mut source_png, image::ImageFormat::Png)
            .unwrap();

        let out = icon_to_png(&source_png.into_inner()).unwrap();

        let decoded = image::load_from_memory(&out).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (64, 64));
    }

    #[test]
    fn icon_rejects_non_images() {
        assert!(icon_to_png(b"this is not an image").is_err());
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

    #[test]
    fn freedomchat_jar_is_recognised_by_name() {
        assert!(is_freedomchat_jar("FreedomChat-Paper-1.7.9.jar"));
        assert!(is_freedomchat_jar("freedomchat-paper-1.7.9.jar"));
        assert!(!is_freedomchat_jar("FreedomChat-Paper-1.7.9.jar.disabled"));
        assert!(!is_freedomchat_jar("SomeOtherPlugin.jar"));
        assert!(!is_freedomchat_jar("FreedomChat"));
    }

    /// `ensure_freedomchat` must be a no-op — no download — once a jar is
    /// present, so restarts don't refetch it. `has_freedomchat_jar` is the
    /// gate that makes that true.
    #[tokio::test]
    async fn an_existing_freedomchat_jar_is_detected() {
        let dir = tempfile::tempdir().unwrap();
        let plugins = dir.path().join("plugins");

        assert!(
            !has_freedomchat_jar(&plugins).await,
            "a missing plugins dir must not read as present"
        );

        tokio::fs::create_dir_all(&plugins).await.unwrap();
        assert!(
            !has_freedomchat_jar(&plugins).await,
            "an empty plugins dir must not read as present"
        );

        tokio::fs::write(plugins.join("SomeOtherPlugin.jar"), b"x")
            .await
            .unwrap();
        assert!(
            !has_freedomchat_jar(&plugins).await,
            "an unrelated plugin must not read as FreedomChat"
        );

        tokio::fs::write(plugins.join("FreedomChat-Paper-1.7.9.jar"), b"x")
            .await
            .unwrap();
        assert!(
            has_freedomchat_jar(&plugins).await,
            "a present FreedomChat jar must be detected"
        );
    }

    /// A version change drops the jar built for the old version, so the next
    /// start resolves one matching the new version.
    #[tokio::test]
    async fn version_change_removes_the_freedomchat_jar() {
        let dir = tempfile::tempdir().unwrap();
        let plugins = dir.path().join("plugins");
        tokio::fs::create_dir_all(&plugins).await.unwrap();
        tokio::fs::write(plugins.join("FreedomChat-Paper-1.7.9.jar"), b"x")
            .await
            .unwrap();
        tokio::fs::write(plugins.join("KeepMe.jar"), b"x").await.unwrap();

        remove_freedomchat_jars(dir.path()).await.unwrap();

        assert!(!has_freedomchat_jar(&plugins).await);
        assert!(
            plugins.join("KeepMe.jar").exists(),
            "only FreedomChat jars are removed"
        );
    }
}
