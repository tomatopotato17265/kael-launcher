use crate::State;
use crate::util::fetch::{fetch_advanced, write};
use reqwest::Method;
use std::path::PathBuf;

pub async fn ensure_playit_cli() -> crate::Result<PathBuf> {
    let state = State::get().await?;
    let dir = state.directories.playit_dir();
    crate::util::io::create_dir_all(&dir).await?;

    let binary_name = if cfg!(windows) {
        "playit-cli.exe"
    } else {
        "playit-cli"
    };
    let target = dir.join(binary_name);

    if target.exists() {
        return Ok(target);
    }

    if let Some(found) = find_on_path() {
        return Ok(found);
    }

    let asset = asset_name()?;
    let url = format!(
        "https://github.com/playit-cloud/playit-agent/releases/latest/download/{asset}"
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

    write(&target, &bytes, &state.io_semaphore).await?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(&target)?.permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&target, permissions)?;
    }

    Ok(target)
}

fn find_on_path() -> Option<PathBuf> {
    let names: &[&str] = if cfg!(windows) {
        &["playit.exe", "playit-cli.exe"]
    } else {
        &["playit", "playit-cli"]
    };

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

fn asset_name() -> crate::Result<String> {
    let arch = std::env::consts::ARCH;

    match std::env::consts::OS {
        "linux" => {
            let mapped = match arch {
                "x86_64" => "amd64",
                "aarch64" => "aarch64",
                "arm" => "armv7",
                "x86" => "i686",
                other => {
                    return Err(crate::ErrorKind::OtherError(format!(
                        "Unsupported Linux architecture for playit: {other}"
                    ))
                    .into());
                }
            };
            Ok(format!("playit-cli-linux-{mapped}"))
        }
        "windows" => {
            let mapped = match arch {
                "x86_64" => "x86_64",
                "x86" => "x86",
                other => {
                    return Err(crate::ErrorKind::OtherError(format!(
                        "Unsupported Windows architecture for playit: {other}"
                    ))
                    .into());
                }
            };
            Ok(format!("playit-windows-{mapped}.exe"))
        }
        "macos" => Err(crate::ErrorKind::OtherError(
            "The playit agent cannot be downloaded automatically on macOS. Install it (for example, 'brew install playit'), make sure it is on your PATH, then try again.".to_string(),
        )
        .into()),
        other => Err(crate::ErrorKind::OtherError(format!(
            "Unsupported platform for playit: {other}"
        ))
        .into()),
    }
}
