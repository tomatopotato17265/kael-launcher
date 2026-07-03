//! CurseForge (CurseForge for Studios) API client.
//!
//! All requests flow through this backend module so the API key stays embedded
//! in the compiled binary via [`cf_api_key`] and is never exposed to the
//! frontend/webview. The key is provided at build time through the
//! `CURSEFORGE_API_KEY` environment variable (forwarded by `build.rs`); when it
//! is absent, [`is_enabled`] returns `false` and the feature is disabled.

use crate::ErrorKind;
use crate::util::fetch::fetch_advanced;
use reqwest::{Method, Url};
use serde::{Deserialize, Serialize};

pub mod install;

pub use install::install_cf_file;

pub const API_BASE: &str = "https://api.curseforge.com";
pub const MINECRAFT_GAME_ID: i64 = 432;

/// CurseForge content class IDs for Minecraft.
pub mod class_id {
    pub const MODS: i64 = 6;
    pub const MODPACKS: i64 = 4471;
    pub const RESOURCE_PACKS: i64 = 12;
    pub const WORLDS: i64 = 17;
    pub const SHADERS: i64 = 6552;
    pub const BUKKIT_PLUGINS: i64 = 5;
    pub const DATA_PACKS: i64 = 6945;
    pub const CUSTOMIZATION: i64 = 4546;
}

/// The build-time CurseForge API key, if one was provided.
pub fn cf_api_key() -> Option<&'static str> {
    // `option_env!` keeps keyless self-builds compiling; the feature is simply
    // disabled when the key is absent.
    match option_env!("CURSEFORGE_API_KEY") {
        Some(key) if !key.is_empty() => Some(key),
        _ => None,
    }
}

/// Whether CurseForge integration is available in this build.
pub fn is_enabled() -> bool {
    cf_api_key().is_some()
}

fn require_key() -> crate::Result<&'static str> {
    cf_api_key().ok_or_else(|| {
        ErrorKind::OtherError(
            "CurseForge integration is not available in this build."
                .to_string(),
        )
        .as_error()
    })
}

#[derive(Deserialize)]
struct DataResponse<T> {
    data: T,
}

#[derive(Deserialize)]
struct PaginatedResponse<T> {
    data: Vec<T>,
    #[allow(dead_code)]
    pagination: Pagination,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pagination {
    pub index: i64,
    pub page_size: i64,
    pub result_count: i64,
    #[serde(default)]
    pub total_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResponse {
    pub data: Vec<CurseForgeMod>,
    pub pagination: Pagination,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurseForgeMod {
    pub id: i64,
    pub game_id: i64,
    pub name: String,
    pub slug: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub download_count: f64,
    pub class_id: Option<i64>,
    #[serde(default)]
    pub authors: Vec<ModAuthor>,
    pub logo: Option<ModAsset>,
    #[serde(default)]
    pub categories: Vec<Category>,
    pub links: Option<ModLinks>,
    #[serde(default)]
    pub latest_files: Vec<File>,
    #[serde(default)]
    pub screenshots: Vec<ModScreenshot>,
    pub allow_mod_distribution: Option<bool>,
    pub date_created: Option<String>,
    pub date_modified: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModScreenshot {
    pub id: i64,
    pub title: Option<String>,
    pub description: Option<String>,
    pub thumbnail_url: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModAuthor {
    pub id: i64,
    pub name: String,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModAsset {
    pub thumbnail_url: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModLinks {
    pub website_url: Option<String>,
    pub wiki_url: Option<String>,
    pub issues_url: Option<String>,
    pub source_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    pub id: i64,
    pub name: String,
    pub slug: Option<String>,
    pub class_id: Option<i64>,
    pub is_class: Option<bool>,
    pub icon_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct File {
    pub id: i64,
    pub mod_id: i64,
    pub display_name: String,
    pub file_name: String,
    pub file_date: Option<String>,
    pub download_url: Option<String>,
    pub download_count: Option<f64>,
    pub file_length: Option<i64>,
    pub release_type: Option<i64>,
    #[serde(default)]
    pub is_available: bool,
    #[serde(default)]
    pub game_versions: Vec<String>,
    #[serde(default)]
    pub hashes: Vec<FileHash>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileHash {
    pub value: String,
    /// 1 = Sha1, 2 = Md5
    pub algo: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MinecraftGameVersion {
    pub id: i64,
    pub version_string: String,
}

/// Parameters for [`search`]. Field names are camelCase to match how the
/// frontend passes them through the Tauri command.
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchParams {
    pub class_id: Option<i64>,
    pub category_id: Option<i64>,
    pub game_version: Option<String>,
    pub search_filter: Option<String>,
    pub sort_field: Option<i64>,
    pub sort_order: Option<String>,
    pub mod_loader_type: Option<i64>,
    pub index: Option<i64>,
    pub page_size: Option<i64>,
}

async fn cf_request<T>(
    method: Method,
    url: Url,
    body: Option<serde_json::Value>,
) -> crate::Result<T>
where
    T: serde::de::DeserializeOwned,
{
    let key = require_key()?;
    let state = crate::State::get().await?;

    let bytes = fetch_advanced(
        method,
        url.as_str(),
        None,
        body,
        Some(("x-api-key", key)),
        None,
        None,
        Some("curseforge"),
        &state.api_semaphore,
        &state.pool,
    )
    .await?;

    Ok(serde_json::from_slice(&bytes)?)
}

/// Search CurseForge for Minecraft content.
pub async fn search(params: SearchParams) -> crate::Result<SearchResponse> {
    let game_id = MINECRAFT_GAME_ID.to_string();
    let mut query: Vec<(&str, String)> = vec![("gameId", game_id)];

    if let Some(class_id) = params.class_id {
        query.push(("classId", class_id.to_string()));
    }
    if let Some(category_id) = params.category_id {
        query.push(("categoryId", category_id.to_string()));
    }
    if let Some(game_version) = params.game_version {
        query.push(("gameVersion", game_version));
    }
    if let Some(search_filter) = params.search_filter {
        query.push(("searchFilter", search_filter));
    }
    if let Some(sort_field) = params.sort_field {
        query.push(("sortField", sort_field.to_string()));
    }
    query.push((
        "sortOrder",
        params.sort_order.unwrap_or_else(|| "desc".to_string()),
    ));
    if let Some(mod_loader_type) = params.mod_loader_type {
        query.push(("modLoaderType", mod_loader_type.to_string()));
    }
    if let Some(index) = params.index {
        query.push(("index", index.to_string()));
    }
    query.push((
        "pageSize",
        params.page_size.unwrap_or(20).clamp(1, 50).to_string(),
    ));

    let url =
        Url::parse_with_params(&format!("{API_BASE}/v1/mods/search"), &query)?;
    cf_request(Method::GET, url, None).await
}

/// Fetch a single CurseForge mod/project by ID.
pub async fn get_mod(mod_id: i64) -> crate::Result<CurseForgeMod> {
    let url = Url::parse(&format!("{API_BASE}/v1/mods/{mod_id}"))?;
    let res: DataResponse<CurseForgeMod> =
        cf_request(Method::GET, url, None).await?;
    Ok(res.data)
}

/// Fetch the rendered HTML description for a CurseForge mod.
pub async fn get_description(mod_id: i64) -> crate::Result<String> {
    let url = Url::parse(&format!("{API_BASE}/v1/mods/{mod_id}/description"))?;
    let res: DataResponse<String> = cf_request(Method::GET, url, None).await?;
    Ok(res.data)
}

/// Fetch the files (versions) for a CurseForge mod.
pub async fn get_files(mod_id: i64) -> crate::Result<Vec<File>> {
    let url = Url::parse_with_params(
        &format!("{API_BASE}/v1/mods/{mod_id}/files"),
        &[("pageSize", "50")],
    )?;
    let res: PaginatedResponse<File> =
        cf_request(Method::GET, url, None).await?;
    Ok(res.data)
}

/// Fetch many files at once by file ID.
pub async fn get_files_bulk(file_ids: &[i64]) -> crate::Result<Vec<File>> {
    let url = Url::parse(&format!("{API_BASE}/v1/mods/files"))?;
    let body = serde_json::json!({ "fileIds": file_ids });
    let res: DataResponse<Vec<File>> =
        cf_request(Method::POST, url, Some(body)).await?;
    Ok(res.data)
}

/// Resolve a file's download URL. Returns `None` when the author has disabled
/// third-party distribution (`allowModDistribution: false`).
pub async fn get_download_url(
    mod_id: i64,
    file_id: i64,
) -> crate::Result<Option<String>> {
    let url = Url::parse(&format!(
        "{API_BASE}/v1/mods/{mod_id}/files/{file_id}/download-url"
    ))?;
    let res: DataResponse<Option<String>> =
        cf_request(Method::GET, url, None).await?;
    Ok(res.data)
}

/// Fetch CurseForge categories for Minecraft, optionally scoped to a class.
pub async fn categories(class_id: Option<i64>) -> crate::Result<Vec<Category>> {
    let game_id = MINECRAFT_GAME_ID.to_string();
    let mut query: Vec<(&str, String)> = vec![("gameId", game_id)];
    if let Some(class_id) = class_id {
        query.push(("classId", class_id.to_string()));
    }
    let url =
        Url::parse_with_params(&format!("{API_BASE}/v1/categories"), &query)?;
    let res: DataResponse<Vec<Category>> =
        cf_request(Method::GET, url, None).await?;
    Ok(res.data)
}

/// Fetch the list of Minecraft game versions known to CurseForge.
pub async fn minecraft_versions() -> crate::Result<Vec<MinecraftGameVersion>> {
    let url = Url::parse(&format!("{API_BASE}/v1/minecraft/version"))?;
    let res: DataResponse<Vec<MinecraftGameVersion>> =
        cf_request(Method::GET, url, None).await?;
    Ok(res.data)
}
