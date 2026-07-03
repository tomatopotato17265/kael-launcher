use crate::State;
use crate::event::LoadingBarId;
use crate::event::emit::emit_loading;
use crate::util::fetch::{fetch_advanced, write};
use crate::util::io::IOError;
use reqwest::Method;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

const PLAYIT_AGENT_TAG: &str = "v1.0.10";
// We build and run the `playitd` daemon directly rather than `playit-cli`:
// bare `playit-cli` installs and attaches to a persistent OS-level service,
// while `playitd --secret <key>` runs standalone in the foreground and takes
// the secret as a plain argument, which is what we need for a per-server
// background process
const PLAYIT_DAEMON_PACKAGE: &str = "playitd";
const RUSTUP_DIST_BASE: &str = "https://static.rust-lang.org/rustup/dist";

/// Tracks a slice of a loading bar and doles it out across streamed
/// subprocess output, since `emit_loading` drops messages that come with a
/// zero increment.
struct BarSlice<'a> {
    bar: Option<&'a LoadingBarId>,
    remaining: f64,
}

impl<'a> BarSlice<'a> {
    fn new(loading_bar: Option<(&'a LoadingBarId, f64)>, share: f64) -> Self {
        match loading_bar {
            Some((bar, total)) => Self {
                bar: Some(bar),
                remaining: total * share,
            },
            None => Self {
                bar: None,
                remaining: 0.0,
            },
        }
    }

    fn tick(&mut self, message: &str) {
        if let Some(bar) = self.bar {
            let increment = self.remaining * 0.05;
            self.remaining -= increment;
            let _ = emit_loading(bar, increment, Some(message));
        }
    }

    fn finish(&mut self, message: &str) {
        if let Some(bar) = self.bar {
            let _ = emit_loading(bar, self.remaining, Some(message));
        }
        self.remaining = 0.0;
    }
}

pub async fn ensure_playit_daemon(
    loading_bar: Option<(&LoadingBarId, f64)>,
) -> crate::Result<PathBuf> {
    let state = State::get().await?;
    let dir = state.directories.playit_dir();
    crate::util::io::create_dir_all(&dir).await?;

    let binary_name = if cfg!(windows) {
        "playitd.exe"
    } else {
        "playitd"
    };
    let target = dir.join(binary_name);

    if target.exists() || find_executable(playit_names()).is_some() {
        BarSlice::new(loading_bar, 1.0).finish("playit agent ready");
        if target.exists() {
            return Ok(target);
        }
        return Ok(find_executable(playit_names()).unwrap());
    }

    preflight_native_toolchain()?;

    let mut toolchain_bar = BarSlice::new(loading_bar, 0.4);
    let (cargo, toolchain_env) = ensure_cargo(&dir, &mut toolchain_bar).await?;
    toolchain_bar.finish("Rust toolchain ready");

    let mut source_bar = BarSlice::new(loading_bar, 0.15);
    source_bar.tick("Downloading playit agent source");
    let build_dir = dir.join("build");
    let source_root = fetch_playit_source(&build_dir).await?;
    source_bar.finish("playit agent source ready");

    let mut build_bar = BarSlice::new(loading_bar, 0.4);
    build_bar.tick("Building playit agent (one-time setup)");

    let mut command = Command::new(&cargo);
    command.current_dir(&source_root).args([
        "build",
        "--release",
        "-p",
        PLAYIT_DAEMON_PACKAGE,
    ]);
    if let Some((cargo_home, rustup_home)) = &toolchain_env {
        command.env("CARGO_HOME", cargo_home);
        command.env("RUSTUP_HOME", rustup_home);
    }
    run_streaming(command, &mut build_bar, "Building playit agent").await?;
    build_bar.finish("playit agent built");

    let built = source_root.join("target").join("release").join(binary_name);
    tokio::fs::copy(&built, &target)
        .await
        .map_err(IOError::from)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(&target)?.permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&target, permissions)?;
    }

    let _ = crate::util::io::remove_dir_all(&build_dir).await;

    BarSlice::new(loading_bar, 0.05).finish("playit agent ready");
    Ok(target)
}

/// Locates a Rust toolchain, installing one non-interactively via rustup if
/// none is present. Returns the cargo path and, when the app-scoped
/// toolchain is used, the CARGO_HOME/RUSTUP_HOME overrides for builds.
async fn ensure_cargo(
    playit_dir: &Path,
    bar: &mut BarSlice<'_>,
) -> crate::Result<(PathBuf, Option<(PathBuf, PathBuf)>)> {
    let cargo_bin = if cfg!(windows) { "cargo.exe" } else { "cargo" };

    if let Some(cargo) = find_executable(&[cargo_bin]) {
        return Ok((cargo, None));
    }

    if let Some(home) = dirs::home_dir() {
        let cargo = home.join(".cargo").join("bin").join(cargo_bin);
        if cargo.is_file() {
            return Ok((cargo, None));
        }
    }

    let toolchain_dir = playit_dir.join("toolchain");
    let cargo_home = toolchain_dir.join("cargo");
    let rustup_home = toolchain_dir.join("rustup");
    let cargo = cargo_home.join("bin").join(cargo_bin);

    if cargo.is_file() {
        return Ok((cargo, Some((cargo_home, rustup_home))));
    }

    bar.tick("Downloading Rust installer (one-time setup)");
    let state = State::get().await?;
    crate::util::io::create_dir_all(&toolchain_dir).await?;

    let triple = rustup_target_triple()?;
    let installer_name = if cfg!(windows) {
        "rustup-init.exe"
    } else {
        "rustup-init"
    };
    let installer = toolchain_dir.join(installer_name);
    let url = format!("{RUSTUP_DIST_BASE}/{triple}/{installer_name}");

    let bytes = fetch_advanced(
        Method::GET,
        &url,
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
    write(&installer, &bytes, &state.io_semaphore).await?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(&installer)?.permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&installer, permissions)?;
    }

    bar.tick("Installing Rust toolchain (one-time setup)");
    let mut command = Command::new(&installer);
    command
        .args([
            "-y",
            "--profile",
            "minimal",
            "--default-toolchain",
            "stable",
            "--no-modify-path",
        ])
        .env("CARGO_HOME", &cargo_home)
        .env("RUSTUP_HOME", &rustup_home);
    if cfg!(windows) {
        command.args(["--default-host", triple]);
    }
    run_streaming(command, bar, "Installing Rust toolchain").await?;

    if !cargo.is_file() {
        return Err(crate::ErrorKind::OtherError(
            "The Rust toolchain install finished but cargo was not found"
                .to_string(),
        )
        .into());
    }

    Ok((cargo, Some((cargo_home, rustup_home))))
}

async fn fetch_playit_source(build_dir: &Path) -> crate::Result<PathBuf> {
    let state = State::get().await?;

    if build_dir.exists() {
        let _ = crate::util::io::remove_dir_all(build_dir).await;
    }
    crate::util::io::create_dir_all(build_dir).await?;

    let url = format!(
        "https://github.com/playit-cloud/playit-agent/archive/refs/tags/{PLAYIT_AGENT_TAG}.zip"
    );
    let bytes = fetch_advanced(
        Method::GET,
        &url,
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

    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|_| {
            crate::Error::from(crate::ErrorKind::InputError(
                "Failed to read the playit agent source archive".to_string(),
            ))
        })?;
    archive.extract(build_dir).map_err(|_| {
        crate::Error::from(crate::ErrorKind::InputError(
            "Failed to extract the playit agent source archive".to_string(),
        ))
    })?;

    for entry in std::fs::read_dir(build_dir).map_err(IOError::from)? {
        let path = entry.map_err(IOError::from)?.path();
        if path.is_dir() && path.join("Cargo.toml").is_file() {
            return Ok(path);
        }
    }

    Err(crate::ErrorKind::OtherError(
        "The playit agent source archive did not contain a Cargo project"
            .to_string(),
    )
    .into())
}

/// Runs a command with piped stderr, streaming its output into the loading
/// bar, and fails with the output tail if the command exits unsuccessfully.
async fn run_streaming(
    mut command: Command,
    bar: &mut BarSlice<'_>,
    prefix: &str,
) -> crate::Result<()> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

    let mut child = command.spawn().map_err(IOError::from)?;
    let mut tail: Vec<String> = Vec::new();

    if let Some(stderr) = child.stderr.take() {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let line = line.trim().to_string();
            if line.is_empty() {
                continue;
            }
            tracing::info!("{prefix}: {line}");
            bar.tick(&format!("{prefix}: {line}"));
            tail.push(line);
            if tail.len() > 20 {
                tail.remove(0);
            }
        }
    }

    let status = child.wait().await.map_err(IOError::from)?;
    if !status.success() {
        return Err(crate::ErrorKind::OtherError(format!(
            "{prefix} failed ({status}):\n{}",
            tail.join("\n")
        ))
        .into());
    }

    Ok(())
}

fn preflight_native_toolchain() -> crate::Result<()> {
    #[cfg(target_os = "macos")]
    {
        let ok = std::process::Command::new("xcode-select")
            .arg("-p")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ok {
            return Err(crate::ErrorKind::OtherError(
				"Building the playit agent requires the Xcode Command Line Tools. Run 'xcode-select --install', then try again.".to_string(),
			)
			.into());
        }
    }

    #[cfg(target_os = "linux")]
    {
        if find_executable(&["cc", "gcc", "clang"]).is_none() {
            return Err(crate::ErrorKind::OtherError(
				"Building the playit agent requires a C compiler. Install gcc or clang with your package manager, then try again.".to_string(),
			)
			.into());
        }
    }

    Ok(())
}

fn rustup_target_triple() -> crate::Result<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Ok("aarch64-apple-darwin"),
        ("macos", "x86_64") => Ok("x86_64-apple-darwin"),
        ("linux", "x86_64") => Ok("x86_64-unknown-linux-gnu"),
        ("linux", "aarch64") => Ok("aarch64-unknown-linux-gnu"),
        // windows-gnu ships a self-contained MinGW linker; MSVC would need a
        // Visual Studio Build Tools install we cannot do non-interactively
        ("windows", "x86_64") => Ok("x86_64-pc-windows-gnu"),
        (os, arch) => Err(crate::ErrorKind::OtherError(format!(
            "Building the playit agent is not supported on {os}/{arch}"
        ))
        .into()),
    }
}

fn playit_names() -> &'static [&'static str] {
    if cfg!(windows) {
        &["playitd.exe"]
    } else {
        &["playitd"]
    }
}

fn find_executable(names: &[&str]) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        for name in names {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    None
}
