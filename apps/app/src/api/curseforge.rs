use crate::api::Result;
use theseus::curseforge::{
    Category, CurseForgeMod, File, MinecraftGameVersion, SearchParams,
    SearchResponse,
};

pub fn init<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("curseforge")
        .invoke_handler(tauri::generate_handler![
            curseforge_is_enabled,
            curseforge_search,
            curseforge_get_mod,
            curseforge_get_description,
            curseforge_get_files,
            curseforge_categories,
            curseforge_minecraft_versions,
            curseforge_install_file,
            curseforge_install_modpack,
        ])
        .build()
}

/// Whether CurseForge integration is available in this build (a key was baked in).
#[tauri::command]
pub fn curseforge_is_enabled() -> bool {
    theseus::curseforge::is_enabled()
}

/// Search CurseForge for Minecraft content.
#[tauri::command]
pub async fn curseforge_search(params: SearchParams) -> Result<SearchResponse> {
    Ok(theseus::curseforge::search(params).await?)
}

/// Fetch a single CurseForge mod/project by ID.
#[tauri::command]
pub async fn curseforge_get_mod(mod_id: i64) -> Result<CurseForgeMod> {
    Ok(theseus::curseforge::get_mod(mod_id).await?)
}

/// Fetch the rendered HTML description for a CurseForge mod.
#[tauri::command]
pub async fn curseforge_get_description(mod_id: i64) -> Result<String> {
    Ok(theseus::curseforge::get_description(mod_id).await?)
}

/// Fetch the files (versions) for a CurseForge mod.
#[tauri::command]
pub async fn curseforge_get_files(mod_id: i64) -> Result<Vec<File>> {
    Ok(theseus::curseforge::get_files(mod_id).await?)
}

/// Fetch CurseForge categories for Minecraft, optionally scoped to a class.
#[tauri::command]
pub async fn curseforge_categories(
    class_id: Option<i64>,
) -> Result<Vec<Category>> {
    Ok(theseus::curseforge::categories(class_id).await?)
}

/// Fetch the Minecraft game versions known to CurseForge.
#[tauri::command]
pub async fn curseforge_minecraft_versions()
-> Result<Vec<MinecraftGameVersion>> {
    Ok(theseus::curseforge::minecraft_versions().await?)
}

/// Install a single CurseForge file (mod/resource pack/shader/data pack) into a profile.
#[tauri::command]
pub async fn curseforge_install_file(
    profile_path: String,
    mod_id: i64,
    file_id: i64,
    project_type: Option<String>,
) -> Result<String> {
    Ok(theseus::curseforge::install_cf_file(
        &profile_path,
        mod_id,
        file_id,
        project_type,
    )
    .await?)
}

/// Install a CurseForge modpack (by mod + file ID) as a brand new profile.
#[tauri::command]
pub async fn curseforge_install_modpack(
    mod_id: i64,
    file_id: i64,
) -> Result<String> {
    Ok(theseus::pack::install_curseforge::install_curseforge_modpack(
        mod_id, file_id,
    )
    .await?)
}
