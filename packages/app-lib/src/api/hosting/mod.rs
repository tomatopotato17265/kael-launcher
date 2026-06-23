pub mod agent;
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
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use uuid::Uuid;

const DEFAULT_PORT: u16 = 25565;

pub async fn list_servers() -> crate::Result<Vec<HostedServer>> {
    let state = State::get().await?;
    HostedServer::get_all(&state.pool).await
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

    let version_id =
        version.unwrap_or_else(|| manifest.latest.release.clone());
    let manifest_version = manifest
        .versions
        .iter()
        .find(|v| v.id == version_id)
        .ok_or_else(|| {
            crate::ErrorKind::InputError(format!(
                "Minecraft version {version_id} was not found"
            ))
        })?;

    emit_loading(&loading_bar, 10.0, Some("Fetching version details"))?;
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

    let java_major =
        info.java_version.as_ref().map(|j| j.major_version).unwrap_or(21);

    emit_loading(&loading_bar, 10.0, Some("Checking Java"))?;
    let java_path = ensure_java(java_major).await?;

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
        Some((&loading_bar, 60.0)),
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
        created: now,
        modified: now,
    };
    server.upsert(&state.pool).await?;

    emit_loading(&loading_bar, 20.0, Some("Server created"))?;
    Ok(server)
}

pub async fn remove_server(id: String) -> crate::Result<()> {
    let state = State::get().await?;
    state.hosting_manager.stop(&id).await?;

    if let Some(server) = HostedServer::get(&id, &state.pool).await? {
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

    if let Some(url) = &server.tunnel_url {
        return Ok(url.clone());
    }

    let account =
        PlayitAccount::get(&state.pool).await?.ok_or_else(|| {
            crate::ErrorKind::InputError(
                "playit.gg has not been set up yet".to_string(),
            )
        })?;

    let tunnel_id = playit::create_tunnel(&account.secret_key, &server.name).await?;
    let url =
        playit::wait_for_tunnel_address(&account.secret_key, &tunnel_id).await?;

    server.playit_tunnel_id = Some(tunnel_id);
    server.tunnel_url = Some(url.clone());
    server.modified = Utc::now().timestamp();
    server.upsert(&state.pool).await?;

    Ok(url)
}

pub async fn start_server(id: String) -> crate::Result<()> {
    let state = State::get().await?;

    if state.hosting_manager.is_running(&id) {
        return Ok(());
    }

    let server =
        HostedServer::get(&id, &state.pool).await?.ok_or_else(|| {
            crate::ErrorKind::InputError(format!("Server {id} was not found"))
        })?;

    let account =
        PlayitAccount::get(&state.pool).await?.ok_or_else(|| {
            crate::ErrorKind::InputError(
                "playit.gg must be set up before starting a server"
                    .to_string(),
            )
        })?;

    let cli_path = agent::ensure_playit_cli().await?;
    ensure_tunnel(&id).await?;

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
        .stdin(Stdio::piped());

    let mut server_child =
        server_command.spawn().map_err(IOError::from)?;
    capture_logs(&id, server_child.stdout.take());
    capture_logs(&id, server_child.stderr.take());

    let mut agent_command = Command::new(&cli_path);
    agent_command
        .env("SECRET_KEY", &account.secret_key)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());

    let mut agent_child = agent_command.spawn().map_err(IOError::from)?;
    capture_logs(&id, agent_child.stdout.take());
    capture_logs(&id, agent_child.stderr.take());

    state.hosting_manager.insert(id, server_child, agent_child);
    Ok(())
}

pub async fn stop_server(id: String) -> crate::Result<()> {
    let state = State::get().await?;
    state.hosting_manager.stop(&id).await
}

pub async fn server_status(id: String) -> crate::Result<bool> {
    let state = State::get().await?;
    Ok(state.hosting_manager.is_running(&id))
}

pub async fn running_servers() -> crate::Result<Vec<String>> {
    let state = State::get().await?;
    Ok(state.hosting_manager.running_ids())
}

pub async fn get_logs(id: String) -> crate::Result<Vec<String>> {
    Ok(crate::state::get_log_buffer(&id))
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
    let poll = playit::poll_claim(&code, "assignable").await?;

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
