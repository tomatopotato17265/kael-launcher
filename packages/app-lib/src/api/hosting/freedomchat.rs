//! Fetching the FreedomChat plugin from Modrinth for Paper-hosted servers.
//!
//! Kael hosts Paper behind Minekube Connect's managed edge, which re-injects
//! each player's `GameProfile` rather than passing the original login through.
//! That breaks Minecraft 1.19+ secure chat: a receiving client silently drops
//! any peer message whose signature chain it can't validate, so players see
//! their own messages and the server logs them, but never see each other's.
//!
//! FreedomChat rewrites outgoing chat as system messages, which clients render
//! without a signature check — sidestepping the whole problem. It runs on the
//! Paper backend (Gate is Go, so a Java plugin can't live on the proxy) and is
//! orthogonal to Velocity forwarding, so real Mojang UUIDs are preserved.

use crate::State;
use crate::util::fetch::{fetch_advanced, fetch_json};
use reqwest::Method;
use serde::Deserialize;

/// Modrinth's API host and the project slug for FreedomChat.
const MODRINTH_API_BASE: &str = "https://api.modrinth.com/v2";
const FREEDOMCHAT_PROJECT: &str = "freedomchat";

#[derive(Debug, Deserialize)]
struct ModrinthVersion {
    files: Vec<ModrinthFile>,
}

#[derive(Debug, Deserialize)]
struct ModrinthFile {
    url: String,
    filename: String,
    primary: bool,
    hashes: ModrinthHashes,
}

#[derive(Debug, Deserialize)]
struct ModrinthHashes {
    sha1: String,
}

/// A resolved FreedomChat plugin jar, ready to drop into `plugins/`.
pub struct PluginJar {
    pub bytes: bytes::Bytes,
    /// Modrinth's own filename, e.g. `FreedomChat-Paper-1.7.9.jar`.
    pub filename: String,
}

/// Downloads the newest FreedomChat build compatible with `mc_version` on Paper,
/// or `Ok(None)` when Modrinth lists no matching build.
///
/// Returning `None` rather than erroring lets callers keep the server running
/// on a version FreedomChat hasn't shipped for yet — the only cost is that chat
/// stays broken there, which is no worse than doing nothing.
///
/// Unlike the Paper jar (sha256, verified by hand), Modrinth publishes sha1, so
/// [`fetch_advanced`] verifies the download itself.
pub async fn download_plugin(
    state: &State,
    mc_version: &str,
) -> crate::Result<Option<PluginJar>> {
    // Modrinth's list filters are JSON arrays passed as query strings; the only
    // characters needing escaping are `[`, `]` and `"`, and a Minecraft version
    // is otherwise URL-safe (digits, dots, and the odd snapshot letter/hyphen).
    let url = format!(
        "{MODRINTH_API_BASE}/project/{FREEDOMCHAT_PROJECT}/version\
         ?loaders=%5B%22paper%22%5D&game_versions=%5B%22{mc_version}%22%5D"
    );

    let versions: Vec<ModrinthVersion> = fetch_json(
        Method::GET,
        &url,
        None,
        None,
        None,
        &state.fetch_semaphore,
        &state.pool,
    )
    .await?;

    // Modrinth returns matches newest-first, so the first entry is the build to
    // use. The primary file is the plugin jar; fall back to the first file for
    // the rare version that flags none primary.
    let Some(mut files) = versions.into_iter().next().map(|v| v.files) else {
        return Ok(None);
    };
    if files.is_empty() {
        return Ok(None);
    }
    let index = files.iter().position(|f| f.primary).unwrap_or(0);
    let file = files.swap_remove(index);

    let bytes = fetch_advanced(
        Method::GET,
        &file.url,
        Some(&file.hashes.sha1),
        None,
        None,
        None,
        None,
        None,
        &state.fetch_semaphore,
        &state.pool,
    )
    .await?;

    Ok(Some(PluginJar {
        bytes,
        filename: file.filename,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The primary-file selection and hash extraction must survive Modrinth's
    /// real response shape; a wrong field name would only surface at runtime.
    #[test]
    fn version_parses_the_real_api_shape_and_prefers_primary() {
        let body = r#"[
            {
                "version_number": "1.7.9",
                "game_versions": ["26.2"],
                "loaders": ["paper", "folia"],
                "files": [
                    {
                        "hashes": { "sha1": "aaa", "sha512": "zzz" },
                        "url": "https://cdn.modrinth.com/data/x/versions/y/sources.jar",
                        "filename": "FreedomChat-Paper-1.7.9-sources.jar",
                        "primary": false
                    },
                    {
                        "hashes": { "sha1": "bbb", "sha512": "yyy" },
                        "url": "https://cdn.modrinth.com/data/x/versions/y/FreedomChat-Paper-1.7.9.jar",
                        "filename": "FreedomChat-Paper-1.7.9.jar",
                        "primary": true
                    }
                ]
            }
        ]"#;

        let versions: Vec<ModrinthVersion> = serde_json::from_str(body).unwrap();
        let mut files = versions.into_iter().next().unwrap().files;
        let index = files.iter().position(|f| f.primary).unwrap_or(0);
        let file = files.swap_remove(index);
        assert_eq!(file.filename, "FreedomChat-Paper-1.7.9.jar");
        assert_eq!(file.hashes.sha1, "bbb");
    }
}
