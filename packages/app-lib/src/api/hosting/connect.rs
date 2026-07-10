//! Minekube Connect tunnelling for locally hosted servers.
//!
//! Kael bundles [Gate](https://gate.minekube.com) and runs one Gate process
//! per hosted server, on the hosting user's machine, as a Connect
//! *connector*. Gate holds an outbound session to the Connect network, which
//! terminates raw Minecraft TCP at its own edge and tunnels players back down
//! that session. Friends therefore join a real hostname in vanilla Minecraft
//! with nothing installed on their end, and the host needs neither an open
//! port nor a public IP.
//!
//! The public address is `<endpoint>.play.minekube.net`, where `<endpoint>` is
//! a globally unique name generated once per server and persisted in that
//! server's own directory. Connect mints its own bearer token locally, so
//! linking never requires a dashboard, an OAuth flow, or a browser.

use crate::State;
use crate::util::fetch::{fetch_advanced, write};
use rand::Rng;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::TcpStream;
use tokio::process::{Child, Command};

const GATE_VERSION: &str = "v0.68.26";
const GATE_RELEASE_BASE: &str =
    "https://github.com/minekube/gate/releases/download";

/// Public suffix every Connect endpoint is reachable under.
const CONNECT_PUBLIC_SUFFIX: &str = "play.minekube.net";

/// Namespaces our endpoint names so a Kael slug is recognizable as ours. Kept
/// short (no hyphen) because Minekube's watch service rejects endpoint names
/// outside 4-16 characters total, confirmed by probing it directly — the
/// original `kael-` (5) + a 12-char slug came to 17 and was rejected on every
/// attempt, deterministically, not flakily.
const ENDPOINT_PREFIX: &str = "kl";

/// Lowercase only: Gate lowercases the handshake virtual host before matching
/// routes, so a mixed-case slug would never match. Ambiguous glyphs (`l`, `o`,
/// `0`, `1`) are omitted because users read these aloud to friends.
const SLUG_ALPHABET: &[u8] = b"abcdefghijkmnpqrstuvwxyz23456789";

/// Endpoint names are globally unique and first-come. The name is derivable
/// from the public hostname, so a short slug would let anyone pre-claim a
/// user's endpoint and receive their friends. 32^14 keeps that infeasible.
/// Combined with [`ENDPOINT_PREFIX`] this must total at most 16 characters —
/// Minekube's watch service hard-rejects anything longer (or under 4).
const SLUG_LEN: usize = 14;

/// How long to wait for Gate to link to the Connect network before giving up.
const LINK_TIMEOUT: Duration = Duration::from_secs(60);

/// How long a single TCP connection attempt to the local server may take.
const LOCAL_PROBE_TIMEOUT: Duration = Duration::from_secs(2);

/// How long to keep retrying before giving up on the local server ever
/// coming up. First boot (world generation, datapack processing) can take
/// well over a minute on a slow machine, so a single probe isn't enough.
const LOCAL_SERVER_STARTUP_TIMEOUT: Duration = Duration::from_secs(120);
const LOCAL_SERVER_POLL_INTERVAL: Duration = Duration::from_secs(2);

/// Substring present in every Gate release asset name, used to recognize our
/// own process (and no one else's) during orphan cleanup.
pub const PROCESS_MARKER: &str = "gate_";

/// How Gate treats player authentication for tunnelled sessions.
///
/// This is the one setting that changes user-visible behaviour, so it is
/// explicit rather than implied by the rest of the config.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMode {
    /// Gate runs in Lite mode and pipes the connection straight through, so the
    /// player authenticates end-to-end with the backend exactly as they did
    /// over playit's raw TCP tunnel. The backend keeps `online-mode=true` and
    /// sees real Mojang UUIDs, so existing worlds, ops and whitelists continue
    /// to work unchanged.
    ///
    /// Requires the Connect edge to propose pass-through sessions. If it does
    /// not, Gate rejects every session and logs the reason, which surfaces as a
    /// [`ConnectError::LinkFailed`] rather than as silently wrong behaviour.
    Passthrough,
    /// Gate terminates login itself and Connect injects the authenticated
    /// `GameProfile` from the session proposal. The backend must then run with
    /// `online-mode=false` bound to loopback, and — because vanilla supports
    /// neither Velocity nor BungeeCord forwarding — it will observe *offline*
    /// UUIDs, which are not the players' real ones.
    EdgeAuthenticated,
}

/// The authentication mode Kael hosts with.
///
/// `Passthrough` preserves the semantics the playit tunnel had. Flip this only
/// if a live test shows the Connect edge never proposes pass-through sessions.
pub const AUTH_MODE: AuthMode = AuthMode::Passthrough;

#[derive(Debug, thiserror::Error)]
pub enum ConnectError {
    #[error(
        "no Gate build is published for this platform ({os} {arch}), so hosting is unavailable"
    )]
    UnsupportedPlatform {
        os: &'static str,
        arch: &'static str,
    },

    #[error(
        "the downloaded Gate binary did not match its published checksum (expected {expected}, got {actual}); refusing to run it"
    )]
    ChecksumMismatch { expected: String, actual: String },

    #[error("the release checksum list did not contain an entry for {asset}")]
    ChecksumMissing { asset: String },

    #[error(
        "your server did not start accepting connections on 127.0.0.1:{port} within {}s; check its logs and try again",
        LOCAL_SERVER_STARTUP_TIMEOUT.as_secs()
    )]
    LocalServerUnreachable { port: u16 },

    #[error(
		"could not link to the Minekube Connect network within {}s: {reason}",
		LINK_TIMEOUT.as_secs()
	)]
    LinkFailed { reason: String },

    #[error("the Gate tunnel process exited before it finished linking")]
    GateExited,
}

impl From<ConnectError> for crate::ErrorKind {
    fn from(value: ConnectError) -> Self {
        crate::ErrorKind::InputError(value.to_string())
    }
}

/// A Connect endpoint owned by one hosted server, persisted so it keeps the
/// same public hostname across restarts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointIdentity {
    pub name: String,
    pub token: String,
}

impl EndpointIdentity {
    pub fn public_address(&self) -> String {
        public_address(&self.name)
    }
}

/// The address friends type into vanilla Minecraft to join a given endpoint.
pub fn public_address(endpoint: &str) -> String {
    format!("{endpoint}.{CONNECT_PUBLIC_SUFFIX}")
}

/// A live tunnel: the running Gate process plus the address to share with
/// friends. Kept out of the database — only the endpoint name is persisted —
/// so the caller is responsible for handing `child` to a process supervisor.
pub struct LinkedGate {
    pub endpoint: String,
    pub address: String,
    pub child: Child,
}

/// Root directory holding every server's Gate state, used both to place a
/// server's own subdirectory and to recognize our own processes during
/// orphan cleanup.
pub fn root_dir(state: &State) -> PathBuf {
    state.directories.metadata_dir().join("gate")
}

fn bin_dir(state: &State) -> PathBuf {
    root_dir(state).join("bin")
}

/// A given server's own directory under [`root_dir`], holding its identity,
/// Connect token, and rendered Gate config. Public so callers can remove it
/// when the server itself is deleted.
pub fn server_dir(state: &State, id: &str) -> PathBuf {
    root_dir(state).join("servers").join(id)
}

fn generate_slug() -> String {
    let mut rng = rand::thread_rng();
    (0..SLUG_LEN)
        .map(|_| SLUG_ALPHABET[rng.gen_range(0..SLUG_ALPHABET.len())] as char)
        .collect()
}

/// Connect mints tokens client-side: any unclaimed name becomes ours the first
/// time we present a self-generated token for it.
fn generate_token() -> String {
    let mut rng = rand::thread_rng();
    let body: String = (0..20)
        .map(|_| SLUG_ALPHABET[rng.gen_range(0..SLUG_ALPHABET.len())] as char)
        .collect();
    format!("T-{body}")
}

/// Loads a server's endpoint identity, creating and persisting one on first
/// use. The identity file is the sole thing that keeps a server's hostname
/// stable, so it is written before it is ever presented to Connect.
pub async fn identity(
    state: &State,
    id: &str,
) -> crate::Result<EndpointIdentity> {
    let path = server_dir(state, id).join("endpoint.json");

    if let Ok(existing) = tokio::fs::read(&path).await
        && let Ok(identity) =
            serde_json::from_slice::<EndpointIdentity>(&existing)
    {
        return Ok(identity);
    }

    let identity = EndpointIdentity {
        name: format!("{ENDPOINT_PREFIX}{}", generate_slug()),
        token: generate_token(),
    };

    write(
        &path,
        &serde_json::to_vec_pretty(&identity)?,
        &state.io_semaphore,
    )
    .await?;

    Ok(identity)
}

fn asset_name() -> Result<String, ConnectError> {
    let (os, arch) = (std::env::consts::OS, std::env::consts::ARCH);

    let suffix = match (os, arch) {
        ("macos", "aarch64") => "darwin_arm64",
        ("macos", "x86_64") => "darwin_amd64",
        ("linux", "aarch64") => "linux_arm64",
        ("linux", "x86_64") => "linux_amd64",
        ("windows", "aarch64") => "windows_arm64.exe",
        ("windows", "x86_64") => "windows_amd64.exe",
        _ => return Err(ConnectError::UnsupportedPlatform { os, arch }),
    };

    let version = GATE_VERSION.trim_start_matches('v');
    Ok(format!("gate_{version}_{suffix}"))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().fold(String::new(), |mut acc, byte| {
        use std::fmt::Write;
        let _ = write!(acc, "{byte:02x}");
        acc
    })
}

/// Parses a `checksums.txt` line set of the form `<sha256>  <asset>`.
fn expected_checksum(
    checksums: &str,
    asset: &str,
) -> Result<String, ConnectError> {
    checksums
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            Some((parts.next()?, parts.next()?))
        })
        .find(|(_, name)| *name == asset)
        .map(|(hash, _)| hash.to_string())
        .ok_or_else(|| ConnectError::ChecksumMissing {
            asset: asset.to_string(),
        })
}

/// Downloads the pinned Gate release once (shared across every hosted
/// server) and verifies it against the published checksum before it is ever
/// executed.
pub async fn ensure_binary(state: &State) -> crate::Result<PathBuf> {
    let asset = asset_name()?;
    let path = bin_dir(state).join(&asset);

    if tokio::fs::try_exists(&path).await.unwrap_or(false) {
        return Ok(path);
    }

    let checksums = fetch_advanced(
        Method::GET,
        &format!("{GATE_RELEASE_BASE}/{GATE_VERSION}/checksums.txt"),
        None,
        None,
        None,
        None,
        None,
        None,
        &state.fetch_semaphore,
        &state.pool,
    )
    .await?;
    let checksums = String::from_utf8_lossy(&checksums);
    let expected = expected_checksum(&checksums, &asset)?;

    let binary = fetch_advanced(
        Method::GET,
        &format!("{GATE_RELEASE_BASE}/{GATE_VERSION}/{asset}"),
        None,
        None,
        None,
        None,
        None,
        None,
        &state.fetch_semaphore,
        &state.pool,
    )
    .await?;

    let actual = sha256_hex(&binary);
    if actual != expected {
        return Err(ConnectError::ChecksumMismatch { expected, actual }.into());
    }

    write(&path, &binary, &state.io_semaphore).await?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = tokio::fs::metadata(&path).await?.permissions();
        perms.set_mode(0o755);
        tokio::fs::set_permissions(&path, perms).await?;
    }

    Ok(path)
}

/// Reserves a loopback port for Gate's own listener. Players never reach this
/// listener — they arrive through the Connect tunnel — but Gate always binds.
async fn reserve_loopback_port() -> crate::Result<u16> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    Ok(listener.local_addr()?.port())
}

fn render_config(
    endpoint: &str,
    backend_port: u16,
    bind_port: u16,
    token_path: &Path,
) -> String {
    let token_path = token_path.to_string_lossy().replace('\\', "\\\\");

    let core = match AUTH_MODE {
        AuthMode::Passthrough => format!(
            "  lite:\n    \
			 enabled: true\n    \
			 routes:\n      \
			 - host: '*'\n        \
			 backend: 127.0.0.1:{backend_port}\n        \
			 fallback:\n          \
			 motd: |\n            \
			 §cThis Kael server is offline.\n            \
			 §eAsk your friend to start hosting again.\n          \
			 version:\n            \
			 name: '§cOffline'\n            \
			 protocol: -1\n"
        ),
        AuthMode::EdgeAuthenticated => format!(
            "  onlineMode: true\n  \
			 servers:\n    \
			 local: 127.0.0.1:{backend_port}\n  \
			 try:\n    \
			 - local\n"
        ),
    };

    let enforce_passthrough = matches!(AUTH_MODE, AuthMode::Passthrough);

    format!(
        "config:\n  \
		 bind: 127.0.0.1:{bind_port}\n\
		 {core}\n\
		 connect:\n  \
		 enabled: true\n  \
		 name: {endpoint}\n  \
		 enforcePassthrough: {enforce_passthrough}\n  \
		 tokenFilePath: {token_path}\n\
		 \napi:\n  enabled: false\n"
    )
}

/// Returns whether the user's Minecraft server is actually accepting
/// connections right now. Tunnelling a dead port produces a hostname that
/// silently fails for everyone who joins, so this is checked before linking.
pub async fn probe_local_server(port: u16) -> bool {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    matches!(
        tokio::time::timeout(LOCAL_PROBE_TIMEOUT, TcpStream::connect(addr))
            .await,
        Ok(Ok(_))
    )
}

/// Polls the local server until it accepts connections or the startup
/// timeout elapses. A freshly started server needs time to generate its
/// world before it opens its port, so a single probe is not enough.
async fn wait_for_local_server(port: u16) -> bool {
    let deadline = tokio::time::Instant::now() + LOCAL_SERVER_STARTUP_TIMEOUT;
    loop {
        if probe_local_server(port).await {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(LOCAL_SERVER_POLL_INTERVAL).await;
    }
}

/// Starts Gate as a Connect connector in front of `id`'s server on
/// `backend_port` and resolves once the tunnel is live.
///
/// Fails loudly at every stage: an unreachable local server, a corrupted
/// download, a rejected session, or a Connect network that cannot be reached
/// all surface as a typed [`ConnectError`] rather than a hostname that quietly
/// refuses connections.
pub async fn start(
    state: &State,
    id: &str,
    backend_port: u16,
) -> crate::Result<LinkedGate> {
    if !wait_for_local_server(backend_port).await {
        return Err(ConnectError::LocalServerUnreachable {
            port: backend_port,
        }
        .into());
    }

    let binary = ensure_binary(state).await?;
    let identity = identity(state, id).await?;
    let dir = server_dir(state, id);

    let token_path = dir.join("connect.json");
    write(
        &token_path,
        serde_json::json!({ "token": identity.token })
            .to_string()
            .as_bytes(),
        &state.io_semaphore,
    )
    .await?;

    let config_path = dir.join("config.yml");
    let bind_port = reserve_loopback_port().await?;
    write(
        &config_path,
        render_config(&identity.name, backend_port, bind_port, &token_path)
            .as_bytes(),
        &state.io_semaphore,
    )
    .await?;

    let mut child = Command::new(&binary)
        .arg("--config")
        .arg(&config_path)
        .current_dir(&dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;

    let stdout = child.stdout.take().ok_or(ConnectError::GateExited)?;
    let stderr = child.stderr.take().ok_or(ConnectError::GateExited)?;

    match tokio::time::timeout(LINK_TIMEOUT, await_link(stdout, stderr)).await {
        Ok(Ok((stdout_lines, stderr_lines))) => {
            drain_in_background(stdout_lines, stderr_lines);
            Ok(LinkedGate {
                endpoint: identity.name.clone(),
                address: identity.public_address(),
                child,
            })
        }
        Ok(Err(reason)) => {
            let _ = child.start_kill();
            Err(ConnectError::LinkFailed { reason }.into())
        }
        Err(_) => {
            let _ = child.start_kill();
            Err(ConnectError::LinkFailed {
                reason: "the Connect network never acknowledged the link"
                    .to_string(),
            }
            .into())
        }
    }
}

type Lines<R> = tokio::io::Lines<BufReader<R>>;

/// Watches Gate's output until it reports a live link, or reports why it could
/// not establish one, then hands the still-open readers back so the caller can
/// keep draining them for the life of the process. Gate logs through `logr`,
/// so both streams are consumed.
async fn await_link(
    stdout: tokio::process::ChildStdout,
    stderr: tokio::process::ChildStderr,
) -> Result<
    (
        Lines<tokio::process::ChildStdout>,
        Lines<tokio::process::ChildStderr>,
    ),
    String,
> {
    let mut stdout = BufReader::new(stdout).lines();
    let mut stderr = BufReader::new(stderr).lines();

    loop {
        let line = tokio::select! {
            line = stdout.next_line() => line,
            line = stderr.next_line() => line,
        };

        let line = match line {
            Ok(Some(line)) => line,
            Ok(None) => return Err("Gate closed its output stream".to_string()),
            Err(e) => return Err(format!("could not read Gate output: {e}")),
        };

        tracing::debug!(target: "gate", "{line}");

        if line.contains("connected") && line.contains("watch") {
            return Ok((stdout, stderr));
        }
        if line.contains("rejecting session proposal") {
            return Err(
                "Connect rejected the session; the edge may not support \
				 pass-through authentication for this endpoint"
                    .to_string(),
            );
        }
        if line.contains("connect: enabled=false") {
            return Err("Gate started with Connect disabled".to_string());
        }
    }
}

/// Keeps consuming Gate's output for the rest of the process's life so its
/// stdout/stderr pipes never fill up and stall it. Nothing here is
/// user-facing — the per-server console shows the Minecraft server's output,
/// not the tunnel's — so lines just go to the debug log.
fn drain_in_background(
    mut stdout: Lines<tokio::process::ChildStdout>,
    mut stderr: Lines<tokio::process::ChildStderr>,
) {
    tokio::spawn(async move {
        loop {
            let line = tokio::select! {
                line = stdout.next_line() => line,
                line = stderr.next_line() => line,
            };

            match line {
                Ok(Some(line)) => tracing::debug!(target: "gate", "{line}"),
                _ => return,
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_is_lowercase_and_unambiguous() {
        let slug = generate_slug();
        assert_eq!(slug.len(), SLUG_LEN);
        assert!(
            slug.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        );
        assert!(!slug.contains(['l', 'o', '0', '1']));
    }

    #[test]
    fn checksum_is_matched_by_asset_name() {
        let checksums = "\
aaaa  gate_0.68.26_linux_amd64
bbbb  gate_0.68.26_darwin_arm64
";
        assert_eq!(
            expected_checksum(checksums, "gate_0.68.26_darwin_arm64").unwrap(),
            "bbbb"
        );
        assert!(
            expected_checksum(checksums, "gate_0.68.26_windows_amd64.exe")
                .is_err()
        );
    }

    #[test]
    fn public_address_uses_connect_suffix() {
        let identity = EndpointIdentity {
            name: "klabcdefghijklmn".to_string(),
            token: "T-x".to_string(),
        };
        assert_eq!(
            identity.public_address(),
            "klabcdefghijklmn.play.minekube.net"
        );
    }

    /// Minekube's watch service hard-rejects endpoint names outside 4-16
    /// characters, confirmed by probing it directly; the generated name is
    /// the one thing standing between "start hosting" and a deterministic,
    /// silent-looking rejection on every single attempt.
    #[test]
    fn generated_endpoint_name_fits_minekube_length_window() {
        let name = format!("{ENDPOINT_PREFIX}{}", generate_slug());
        assert!(
            (4..=16).contains(&name.len()),
            "endpoint name {name:?} is {} chars, outside Minekube's accepted 4-16 range",
            name.len()
        );
    }
}
