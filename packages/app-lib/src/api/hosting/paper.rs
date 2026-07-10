//! Fetching Paper server jars from the PaperMC "Fill" API.
//!
//! Kael hosts on Paper rather than vanilla because only Paper speaks Velocity
//! player-info forwarding, which is what lets Gate hand the backend the real
//! Mojang profile that Minekube Connect authenticated. Vanilla speaks no
//! forwarding protocol at all and can only ever see offline UUIDs.

use crate::State;
use crate::util::fetch::{fetch_advanced, fetch_json};
use reqwest::Method;
use serde::Deserialize;

/// `api.papermc.io/v2` is sunset (HTTP 410) and `/v3` sits behind a Cloudflare
/// challenge; `fill.papermc.io` is the host that actually serves the v3 API.
const PAPER_API_BASE: &str = "https://fill.papermc.io/v3";

/// Paper's own label for a build that is fit for general use. Newer Minecraft
/// versions carry only `ALPHA` builds for a while after release.
const STABLE_CHANNEL: &str = "STABLE";

#[derive(Debug, Deserialize)]
struct BuildResponse {
    channel: String,
    downloads: Downloads,
}

#[derive(Debug, Deserialize)]
struct Downloads {
    /// The JSON key is literally `server:default`.
    #[serde(rename = "server:default")]
    server_default: Download,
}

#[derive(Debug, Deserialize)]
struct Download {
    url: String,
    checksums: Checksums,
}

#[derive(Debug, Deserialize)]
struct Checksums {
    sha256: String,
}

/// A Paper build resolved for one Minecraft version.
pub struct PaperJar {
    pub bytes: bytes::Bytes,
    /// Paper's release channel for this build, e.g. `STABLE` or `ALPHA`.
    pub channel: String,
}

impl PaperJar {
    pub fn is_stable(&self) -> bool {
        self.channel.eq_ignore_ascii_case(STABLE_CHANNEL)
    }
}

/// Downloads the latest Paper build for `mc_version` and verifies it against
/// the checksum Paper publishes.
///
/// Paper's checksums are sha256 while [`fetch_advanced`] only verifies sha1, so
/// the jar is fetched unverified and checked here, exactly as the Gate binary
/// is in [`super::connect::ensure_binary`].
pub async fn download_jar(
    state: &State,
    mc_version: &str,
    loading_bar: Option<(&crate::event::LoadingBarId, f64)>,
) -> crate::Result<PaperJar> {
    let url = format!(
        "{PAPER_API_BASE}/projects/paper/versions/{mc_version}/builds/latest"
    );

    let build: BuildResponse = fetch_json(
        Method::GET,
        &url,
        None,
        None,
        None,
        &state.fetch_semaphore,
        &state.pool,
    )
    .await
    .map_err(|_| {
        crate::ErrorKind::InputError(format!(
            "No Paper build is available for Minecraft {mc_version}. \
             Kael-hosted servers run Paper, so try a different version."
        ))
    })?;

    let download = build.downloads.server_default;

    let bytes = fetch_advanced(
        Method::GET,
        &download.url,
        None,
        None,
        None,
        None,
        loading_bar,
        None,
        &state.fetch_semaphore,
        &state.pool,
    )
    .await?;

    let actual = super::connect::sha256_hex(&bytes);
    if !actual.eq_ignore_ascii_case(&download.checksums.sha256) {
        return Err(crate::ErrorKind::InputError(format!(
            "The downloaded Paper jar for Minecraft {mc_version} did not match \
             its published checksum (expected {}, got {actual}); refusing to \
             run it.",
            download.checksums.sha256
        ))
        .into());
    }

    Ok(PaperJar {
        bytes,
        channel: build.channel,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `server:default` key is not a valid Rust identifier, so the rename
    /// is load-bearing; a typo would only surface at runtime.
    #[test]
    fn build_response_parses_the_real_api_shape() {
        let body = r#"{
            "id": 56,
            "time": "2026-07-01T00:00:00Z",
            "channel": "ALPHA",
            "commits": [],
            "downloads": {
                "server:default": {
                    "name": "paper-26.2-56.jar",
                    "checksums": { "sha256": "abc123" },
                    "size": 61737060,
                    "url": "https://fill-data.papermc.io/v1/objects/abc123/paper-26.2-56.jar"
                }
            }
        }"#;

        let build: BuildResponse = serde_json::from_str(body).unwrap();
        assert_eq!(build.channel, "ALPHA");
        assert_eq!(build.downloads.server_default.checksums.sha256, "abc123");
        assert!(build.downloads.server_default.url.ends_with(".jar"));
    }

    #[test]
    fn channel_stability_is_case_insensitive() {
        let jar = |channel: &str| PaperJar {
            bytes: bytes::Bytes::new(),
            channel: channel.to_string(),
        };
        assert!(jar("STABLE").is_stable());
        assert!(jar("stable").is_stable());
        assert!(!jar("ALPHA").is_stable());
        assert!(!jar("EXPERIMENTAL").is_stable());
    }
}
