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
use crate::state::{HostedServer, ServerFlavor};
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

/// Gate owns compression on the one link that crosses the internet: the player
/// to the Connect edge and down to this machine. The Paper backend disables its
/// own compression (`network-compression-threshold=-1`), because the only hop
/// between it and Gate is loopback. The two settings are a pair — turning one
/// off without the other either wastes CPU or sends the world uncompressed
/// over a home upload link.
///
/// Packets under the threshold go uncompressed, since the header costs more
/// than they save. Level 6 is zlib's default balance of ratio against CPU; the
/// host is already running the game, so this is not the place to spend cycles.
const COMPRESSION_THRESHOLD: u32 = 256;
const COMPRESSION_LEVEL: u32 = 6;

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
    /// Shared secret proving to a Paper backend that forwarded player identities
    /// really came from this server's Gate. Optional so identity files written
    /// before Velocity forwarding existed still deserialize; [`identity`]
    /// backfills them.
    #[serde(default)]
    pub secret: Option<String>,
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

fn generate_random(len: usize) -> String {
    let mut rng = rand::thread_rng();
    (0..len)
        .map(|_| SLUG_ALPHABET[rng.gen_range(0..SLUG_ALPHABET.len())] as char)
        .collect()
}

/// Connect mints tokens client-side: any unclaimed name becomes ours the first
/// time we present a self-generated token for it.
fn generate_token() -> String {
    format!("T-{}", generate_random(20))
}

/// The Velocity forwarding secret. Anyone who learns it can forge player
/// identities to the backend, so it is long and never leaves this machine.
fn generate_secret() -> String {
    generate_random(32)
}

/// Loads a server's endpoint identity, creating and persisting one on first
/// use. The identity file is the sole thing that keeps a server's hostname
/// stable, so it is written before it is ever presented to Connect.
///
/// Identities written before Velocity forwarding existed carry no secret; they
/// are backfilled and rewritten here rather than regenerated, so the server
/// keeps its established public hostname.
pub async fn identity(
    state: &State,
    id: &str,
) -> crate::Result<EndpointIdentity> {
    let path = server_dir(state, id).join("endpoint.json");

    let existing = tokio::fs::read(&path)
        .await
        .ok()
        .and_then(|bytes| {
            serde_json::from_slice::<EndpointIdentity>(&bytes).ok()
        })
        .map(backfill_secret);

    let (identity, needs_write) = match existing {
        Some((identity, backfilled)) => (identity, backfilled),
        None => (
            EndpointIdentity {
                name: format!("{ENDPOINT_PREFIX}{}", generate_slug()),
                token: generate_token(),
                secret: Some(generate_secret()),
            },
            true,
        ),
    };

    if needs_write {
        write(
            &path,
            &serde_json::to_vec_pretty(&identity)?,
            &state.io_semaphore,
        )
        .await?;
    }

    Ok(identity)
}

/// Returns the identity with a secret guaranteed present, and whether one had
/// to be minted (meaning the file must be rewritten).
fn backfill_secret(mut identity: EndpointIdentity) -> (EndpointIdentity, bool) {
    if identity.secret.is_none() {
        identity.secret = Some(generate_secret());
        return (identity, true);
    }
    (identity, false)
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

/// Shared with the Paper jar download, whose checksums are sha256 while
/// `fetch_advanced` only verifies sha1.
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
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

/// Everything `render_config` needs to emit a server's Gate config.
pub struct GateConfig<'a> {
    pub endpoint: &'a str,
    pub flavor: ServerFlavor,
    /// Velocity forwarding secret. Ignored for [`ServerFlavor::Vanilla`].
    pub secret: &'a str,
    /// Shown in the server list. Only consulted for [`ServerFlavor::Paper`],
    /// because Lite mode forwards pings to the backend instead of answering
    /// them.
    pub motd: &'a str,
    pub max_players: u32,
    pub backend_port: u16,
    pub bind_port: u16,
    pub token_path: &'a Path,
}

/// Escapes a value for a YAML double-quoted scalar, so a server name
/// containing `"` or `\` (or a leading `#`, `:` etc.) cannot break the config.
fn yaml_quote(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

/// Renders the Gate config for one hosted server.
///
/// `enforcePassthrough` is always `false`. Connect's managed edge authenticates
/// every real player itself and then proposes their session as
/// `passthrough: false`; requiring passthrough made Gate reject every real join
/// with `only allowing pass-through connections`, confirmed from Gate's logs.
///
/// The two flavors are genuinely different proxies, not a tweak of one:
///
/// - **Vanilla** runs Lite mode, which pipes raw TCP and discards the
///   authenticated profile Connect injects, so the backend only ever sees
///   offline UUIDs. Vanilla Minecraft speaks no forwarding protocol, so this is
///   the best it can do. Lite forwards server-list pings to the backend.
/// - **Paper** runs Gate as a full proxy. With `onlineMode: true` Gate consumes
///   the `GameProfile` Connect injected instead of doing its own handshake, then
///   forwards that real identity over Velocity modern forwarding. A full proxy
///   answers server-list pings *itself*, so `status` must be set or players see
///   Gate's default MOTD and a max of 1000.
fn render_config(config: &GateConfig<'_>) -> String {
    let GateConfig {
        endpoint,
        flavor,
        secret,
        motd,
        max_players,
        backend_port,
        bind_port,
        token_path,
    } = *config;

    let token_path = token_path.to_string_lossy().replace('\\', "\\\\");

    let core = match flavor {
        ServerFlavor::Vanilla => format!(
            "  lite:\n\
             \x20   enabled: true\n\
             \x20   routes:\n\
             \x20     - host: '*'\n\
             \x20       backend: 127.0.0.1:{backend_port}\n\
             \x20       fallback:\n\
             \x20         motd: |\n\
             \x20           §cThis Kael server is offline.\n\
             \x20           §eAsk your friend to start hosting again.\n\
             \x20         version:\n\
             \x20           name: '§cOffline'\n\
             \x20           protocol: -1\n"
        ),
        ServerFlavor::Paper => format!(
            "  onlineMode: true\n\
             \x20 forwarding:\n\
             \x20   mode: velocity\n\
             \x20   velocitySecret: {secret}\n\
             \x20 servers:\n\
             \x20   local: 127.0.0.1:{backend_port}\n\
             \x20 try:\n\
             \x20   - local\n\
             \x20 compression:\n\
             \x20   threshold: {COMPRESSION_THRESHOLD}\n\
             \x20   level: {COMPRESSION_LEVEL}\n\
             \x20 status:\n\
             \x20   motd: {motd}\n\
             \x20   showMaxPlayers: {max_players}\n",
            motd = yaml_quote(motd),
        ),
    };

    format!(
        "config:\n\
         \x20 bind: 127.0.0.1:{bind_port}\n\
         {core}\n\
         connect:\n\
         \x20 enabled: true\n\
         \x20 name: {endpoint}\n\
         \x20 enforcePassthrough: false\n\
         \x20 tokenFilePath: {token_path}\n\
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
    server: &HostedServer,
) -> crate::Result<LinkedGate> {
    let id = &server.id;
    let backend_port = server.port;

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
        render_config(&GateConfig {
            endpoint: &identity.name,
            flavor: server.flavor,
            secret: identity.secret.as_deref().unwrap_or_default(),
            motd: &server.name,
            max_players: server.max_players,
            backend_port,
            bind_port,
            token_path: &token_path,
        })
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
            drain_in_background(id.to_string(), stdout_lines, stderr_lines);
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
        // Individual player session rejections ("rejecting session
        // proposal") happen well after this point in Gate's own logs, once
        // players start joining — not during the initial link handshake —
        // so they surface later through the per-server log buffer
        // (drain_in_background), not as a link failure here.
        if line.contains("connect: enabled=false") {
            return Err("Gate started with Connect disabled".to_string());
        }
    }
}

/// Keeps consuming Gate's output for the rest of the process's life so its
/// stdout/stderr pipes never fill up and stall it. Pushed into the same
/// per-server log buffer the Minecraft process uses (prefixed so the two are
/// distinguishable) rather than only debug-logged: a tunnel that drops mid
/// session — as opposed to failing to link at all — has no other visible
/// trace, and Gate is the only side of that failure we can actually observe.
fn drain_in_background(
    id: String,
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
                Ok(Some(line)) => {
                    crate::state::push_log_line(&id, format!("[gate] {line}"));
                }
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
            secret: Some("s".to_string()),
        };
        assert_eq!(
            identity.public_address(),
            "klabcdefghijklmn.play.minekube.net"
        );
    }

    /// Identity files written before Velocity forwarding existed have no
    /// `secret` key. They must still load, keep their established endpoint
    /// name, and gain a secret.
    #[test]
    fn legacy_identity_deserializes_and_gains_a_secret() {
        let legacy = r#"{"name":"klabcdefghijklmn","token":"T-old"}"#;
        let parsed: EndpointIdentity = serde_json::from_str(legacy).unwrap();
        assert!(parsed.secret.is_none());

        let (backfilled, rewritten) = backfill_secret(parsed);
        assert!(rewritten, "a minted secret must be persisted");
        assert_eq!(backfilled.name, "klabcdefghijklmn");
        assert_eq!(backfilled.token, "T-old");
        assert!(!backfilled.secret.unwrap().is_empty());
    }

    #[test]
    fn identity_with_a_secret_is_not_rewritten() {
        let identity = EndpointIdentity {
            name: "klabcdefghijklmn".to_string(),
            token: "T-x".to_string(),
            secret: Some("kept".to_string()),
        };
        let (identity, rewritten) = backfill_secret(identity);
        assert!(!rewritten);
        assert_eq!(identity.secret.as_deref(), Some("kept"));
    }

    fn config_for(flavor: ServerFlavor, motd: &str) -> String {
        render_config(&GateConfig {
            endpoint: "klabcdefghijklmn",
            flavor,
            secret: "supersecret",
            motd,
            max_players: 20,
            backend_port: 25565,
            bind_port: 40000,
            token_path: Path::new("/tmp/connect.json"),
        })
    }

    /// Paper servers get a full proxy that consumes Connect's authenticated
    /// profile and forwards it; Lite mode would discard it.
    #[test]
    fn paper_config_forwards_real_identities() {
        let config = config_for(ServerFlavor::Paper, "My Server");
        assert!(config.contains("onlineMode: true"));
        assert!(config.contains("mode: velocity"));
        assert!(config.contains("velocitySecret: supersecret"));
        assert!(config.contains("local: 127.0.0.1:25565"));
        assert!(!config.contains("lite:"));
        assert!(config.contains("enforcePassthrough: false"));
    }

    /// A full proxy answers server-list pings itself, so without `status` the
    /// server list would show Gate's own MOTD and a max of 1000.
    #[test]
    fn paper_config_sets_status_so_pings_are_not_gates_defaults() {
        let config = config_for(ServerFlavor::Paper, "My Server");
        assert!(config.contains("status:"));
        assert!(config.contains(r#"motd: "My Server""#));
        assert!(config.contains("showMaxPlayers: 20"));
    }

    /// Gate must compress toward the player, because the Paper backend has its
    /// own compression disabled for the loopback hop. If this block ever goes
    /// missing, the world streams uncompressed over the host's upload link.
    #[test]
    fn paper_config_compresses_the_internet_facing_link() {
        let config = config_for(ServerFlavor::Paper, "My Server");
        assert!(config.contains("compression:"));
        assert!(config.contains("threshold: 256"));
        assert!(config.contains("level: 6"));
    }

    /// Lite mode pipes raw bytes, so compression is negotiated end-to-end
    /// between the player and the backend; Gate must not sit in the middle of
    /// it.
    #[test]
    fn vanilla_config_has_no_compression_block() {
        let config = config_for(ServerFlavor::Vanilla, "My Server");
        assert!(!config.contains("compression:"));
    }

    /// A server name is user input and lands in a YAML scalar.
    #[test]
    fn motd_is_escaped_into_a_quoted_scalar() {
        let config = config_for(ServerFlavor::Paper, r#"we"rd: #name\"#);
        assert!(config.contains(r#"motd: "we\"rd: #name\\""#));
    }

    /// Existing vanilla servers must keep the exact Lite config they run today;
    /// pointing a velocity-forwarding Gate at a vanilla backend breaks them.
    #[test]
    fn vanilla_config_stays_on_lite_mode() {
        let config = config_for(ServerFlavor::Vanilla, "My Server");
        assert!(config.contains("lite:"));
        assert!(config.contains("backend: 127.0.0.1:25565"));
        assert!(!config.contains("forwarding:"));
        assert!(!config.contains("velocitySecret"));
        assert!(!config.contains("status:"));
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
