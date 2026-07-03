//! Installing individual CurseForge files (mods, resource packs, shaders, data
//! packs) into a profile.

use crate::ErrorKind;
use crate::event::{ProfilePayloadType, emit::emit_profile};
use crate::state::{Profile, ProjectType};
use crate::util::fetch;

const SHA1_ALGO: i64 = 1;

fn map_project_type(project_type: Option<&str>) -> Option<ProjectType> {
    match project_type {
        Some("mod") => Some(ProjectType::Mod),
        Some("resourcepack") => Some(ProjectType::ResourcePack),
        Some("shader") => Some(ProjectType::ShaderPack),
        Some("datapack") => Some(ProjectType::DataPack),
        // Fall back to archive inference for anything else.
        _ => None,
    }
}

/// Install a single CurseForge file into a profile.
///
/// Returns the profile-relative path to the installed project. Errors with a
/// helpful message when the author has disabled third-party distribution.
pub async fn install_cf_file(
    profile_path: &str,
    mod_id: i64,
    file_id: i64,
    project_type: Option<String>,
) -> crate::Result<String> {
    let files = super::get_files_bulk(&[file_id]).await?;
    let file =
        files.into_iter().find(|f| f.id == file_id).ok_or_else(|| {
            ErrorKind::InputError(format!(
                "CurseForge file {file_id} was not found."
            ))
            .as_error()
        })?;

    // The bulk file object usually carries the download URL directly; when it
    // doesn't, ask the dedicated endpoint. A null result means the author
    // disabled third-party distribution and we legally cannot download it.
    let download_url = match file.download_url {
        Some(url) => url,
        None => super::get_download_url(mod_id, file_id)
            .await?
            .ok_or_else(|| {
                ErrorKind::OtherError(format!(
                    "The author of this file has disabled third-party downloads. \
                     Please download it from CurseForge: \
                     https://www.curseforge.com/projects/{mod_id}"
                ))
                .as_error()
            })?,
    };

    let state = crate::State::get().await?;
    let sha1 = file
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

    let path = Profile::add_project_bytes(
        profile_path,
        &file.file_name,
        bytes,
        sha1.as_deref(),
        map_project_type(project_type.as_deref()),
        None,
        &state.io_semaphore,
        &state.pool,
    )
    .await?;

    emit_profile(profile_path, ProfilePayloadType::Edited).await?;

    Ok(path)
}
