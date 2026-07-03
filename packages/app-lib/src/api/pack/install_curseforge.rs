//! Installing CurseForge modpacks (a `.zip` containing a `manifest.json`) as new
//! profiles.

use std::io::Read;
use std::path::PathBuf;

use serde::Deserialize;

use crate::State;
use crate::state::{ModLoader, Profile, ProfileInstallStage};
use crate::util::fetch;

const SHA1_ALGO: i64 = 1;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CfManifest {
    minecraft: CfMinecraft,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    files: Vec<CfManifestFile>,
    #[serde(default)]
    overrides: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CfMinecraft {
    version: String,
    #[serde(default)]
    mod_loaders: Vec<CfModLoader>,
}

#[derive(Deserialize)]
struct CfModLoader {
    id: String,
    #[serde(default)]
    primary: bool,
}

#[derive(Deserialize)]
struct CfManifestFile {
    #[serde(rename = "fileID")]
    file_id: i64,
}

fn parse_loader(manifest: &CfManifest) -> (ModLoader, Option<String>) {
    let chosen = manifest
        .minecraft
        .mod_loaders
        .iter()
        .find(|l| l.primary)
        .or_else(|| manifest.minecraft.mod_loaders.first());

    let Some(chosen) = chosen else {
        return (ModLoader::Vanilla, None);
    };

    let (name, version) = chosen
        .id
        .split_once('-')
        .unwrap_or((chosen.id.as_str(), ""));
    let loader = match name.to_lowercase().as_str() {
        "forge" => ModLoader::Forge,
        "fabric" => ModLoader::Fabric,
        "quilt" => ModLoader::Quilt,
        "neoforge" => ModLoader::NeoForge,
        _ => ModLoader::Vanilla,
    };
    let version = (!version.is_empty()).then(|| version.to_string());
    (loader, version)
}

fn read_manifest_blocking(bytes: bytes::Bytes) -> crate::Result<CfManifest> {
    let cursor = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor).map_err(|e| {
        crate::ErrorKind::InputError(format!(
            "Invalid CurseForge modpack archive: {e}"
        ))
        .as_error()
    })?;
    let mut manifest_file = archive.by_name("manifest.json").map_err(|_| {
        crate::ErrorKind::InputError(
            "No manifest.json found in CurseForge modpack".to_string(),
        )
        .as_error()
    })?;
    let mut contents = String::new();
    manifest_file.read_to_string(&mut contents)?;
    Ok(serde_json::from_str(&contents)?)
}

fn extract_overrides_blocking(
    bytes: bytes::Bytes,
    dest: PathBuf,
    overrides_dir: String,
) -> crate::Result<()> {
    let cursor = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor).map_err(|e| {
        crate::ErrorKind::InputError(format!(
            "Invalid CurseForge modpack archive: {e}"
        ))
        .as_error()
    })?;

    let prefixes =
        [format!("{overrides_dir}/"), "client-overrides/".to_string()];

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| {
            crate::ErrorKind::InputError(format!(
                "Error reading CurseForge modpack archive: {e}"
            ))
            .as_error()
        })?;

        if entry.is_dir() {
            continue;
        }

        let name = entry.name().to_string();
        let Some(rel) =
            prefixes.iter().find_map(|p| name.strip_prefix(p.as_str()))
        else {
            continue;
        };
        if rel.is_empty() {
            continue;
        }

        let out = dest.join(rel);
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out_file = std::fs::File::create(&out)?;
        std::io::copy(&mut entry, &mut out_file)?;
    }

    Ok(())
}

/// Install a CurseForge modpack (by mod + file ID) as a brand new profile.
///
/// Returns the profile-relative path of the created profile.
pub async fn install_curseforge_modpack(
    mod_id: i64,
    file_id: i64,
) -> crate::Result<String> {
    let state = State::get().await?;

    // Resolve and download the modpack archive.
    let files = crate::curseforge::get_files_bulk(&[file_id]).await?;
    let pack_file =
        files.into_iter().find(|f| f.id == file_id).ok_or_else(|| {
            crate::ErrorKind::InputError(format!(
                "CurseForge file {file_id} was not found."
            ))
            .as_error()
        })?;

    let url = match pack_file.download_url {
        Some(url) => url,
        None => crate::curseforge::get_download_url(mod_id, file_id)
            .await?
            .ok_or_else(|| {
                crate::ErrorKind::OtherError(format!(
                    "The author of this modpack has disabled third-party \
                     downloads. Please download it from CurseForge: \
                     https://www.curseforge.com/projects/{mod_id}"
                ))
                .as_error()
            })?,
    };

    let archive_bytes = fetch::fetch(
        &url,
        None,
        None,
        None,
        &state.fetch_semaphore,
        &state.pool,
    )
    .await?;

    let manifest = {
        let bytes = archive_bytes.clone();
        tokio::task::spawn_blocking(move || read_manifest_blocking(bytes))
            .await??
    };

    let game_version = manifest.minecraft.version.clone();
    let (loader, loader_version_hint) = parse_loader(&manifest);
    let loader_version = if loader != ModLoader::Vanilla {
        crate::launcher::get_loader_version_from_profile(
            &game_version,
            loader,
            loader_version_hint.as_deref(),
        )
        .await?
        .map(|x| x.id)
    } else {
        None
    };

    let profile_path = crate::api::profile::create::profile_create(
        manifest
            .name
            .clone()
            .unwrap_or_else(|| "CurseForge Modpack".to_string()),
        game_version,
        loader,
        loader_version,
        None,
        None,
        Some(true),
    )
    .await?;

    let result = install_modpack_contents(
        &state,
        &profile_path,
        &manifest,
        archive_bytes,
    )
    .await;

    if let Err(err) = result {
        let _ = crate::api::profile::remove(&profile_path).await;
        return Err(err);
    }

    Ok(profile_path)
}

async fn install_modpack_contents(
    state: &State,
    profile_path: &str,
    manifest: &CfManifest,
    archive_bytes: bytes::Bytes,
) -> crate::Result<()> {
    crate::api::profile::edit(profile_path, |prof| {
        prof.install_stage = ProfileInstallStage::PackInstalling;
        async { Ok(()) }
    })
    .await?;

    let file_ids: Vec<i64> = manifest.files.iter().map(|f| f.file_id).collect();
    if !file_ids.is_empty() {
        let cf_files = crate::curseforge::get_files_bulk(&file_ids).await?;
        for cf_file in cf_files {
            let download_url = match cf_file.download_url.clone() {
                Some(url) => Some(url),
                None => crate::curseforge::get_download_url(
                    cf_file.mod_id,
                    cf_file.id,
                )
                .await
                .ok()
                .flatten(),
            };

            // Skip files the author disabled for third-party distribution.
            let Some(download_url) = download_url else {
                continue;
            };

            let sha1 = cf_file
                .hashes
                .iter()
                .find(|h| h.algo == SHA1_ALGO)
                .map(|h| h.value.clone());

            let bytes = fetch::fetch(
                &download_url,
                sha1.as_deref(),
                None,
                None,
                &state.fetch_semaphore,
                &state.pool,
            )
            .await?;

            Profile::add_project_bytes(
                profile_path,
                &cf_file.file_name,
                bytes,
                sha1.as_deref(),
                None,
                None,
                &state.io_semaphore,
                &state.pool,
            )
            .await?;
        }
    }

    let dest = state.directories.profiles_dir().join(profile_path);
    let overrides_dir = manifest
        .overrides
        .clone()
        .unwrap_or_else(|| "overrides".to_string());
    {
        let bytes = archive_bytes.clone();
        tokio::task::spawn_blocking(move || {
            extract_overrides_blocking(bytes, dest, overrides_dir)
        })
        .await??;
    }

    if let Some(profile) = crate::api::profile::get(profile_path).await? {
        crate::launcher::install_minecraft(&profile, None, false).await?;
    }

    crate::api::profile::edit(profile_path, |prof| {
        prof.install_stage = ProfileInstallStage::Installed;
        async { Ok(()) }
    })
    .await?;

    Ok(())
}
