//! Custom font discovery, installation (Google Fonts download / local files),
//! and management. Mirrors the custom-themes system in `theming.rs`.

use crate::state::Settings;
use crate::util::io;
use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use serde::Serialize;
use std::path::{Path, PathBuf};

/// Extension of a self-contained downloaded Google font (an `@font-face`
/// stylesheet with the font binaries embedded as base64 data URIs).
pub const GOOGLE_FONT_FILE_EXTENSION: &str = "mrfont.css";
/// Raw font file extensions a user can drop into the fonts folder.
const LOCAL_FONT_EXTENSIONS: &[&str] = &["ttf", "otf", "woff", "woff2"];

/// A modern-browser User-Agent so the Google Fonts CSS API returns woff2
/// (the served format is User-Agent-driven).
const BROWSER_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledFont {
    /// File stem (without extension), the stable selection id.
    pub id: String,
    /// The `font-family` name to apply.
    pub name: String,
    /// `"google"` (downloaded) or `"local"` (user-dropped file).
    pub source: String,
    pub file_name: String,
}

/// Directory where custom fonts live: `settings.font_dir` if set, otherwise
/// `config_dir/fonts`.
pub async fn fonts_dir() -> crate::Result<PathBuf> {
    let state = crate::State::get().await?;
    let settings = Settings::get(&state.pool).await?;

    let dir = match settings.font_dir {
        Some(dir) => PathBuf::from(dir),
        None => state.directories.config_dir.join("fonts"),
    };
    io::create_dir_all(&dir).await?;

    Ok(dir)
}

/// (Re-)registers the configured fonts directory with the file watcher so
/// dropped/edited font files hot-reload.
pub async fn refresh_fonts_watch() -> crate::Result<()> {
    let state = crate::State::get().await?;
    let dir = fonts_dir().await?;
    crate::state::fs_watcher::watch_fonts_dir(&state.file_watcher, &dir).await;
    Ok(())
}

fn sanitize_stem(name: &str) -> String {
    name.trim()
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect()
}

fn file_stem(file_name: &str) -> String {
    file_name
        .rsplit_once('.')
        .map(|(stem, _)| stem.to_string())
        .unwrap_or_else(|| file_name.to_string())
}

fn font_mime(ext: &str) -> &'static str {
    match ext {
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "otf" => "font/otf",
        _ => "font/ttf",
    }
}

/// Reads the `/* @font-family: X */` header a downloaded font file carries.
fn extract_font_family_header(css: &str) -> Option<String> {
    let re = regex::Regex::new(r"/\*\s*@font-family:\s*(.+?)\s*\*/").unwrap();
    re.captures(css)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().trim().to_string())
        .filter(|s| !s.is_empty())
}

fn is_local_font(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| LOCAL_FONT_EXTENSIONS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

/// Finds a raw local font file in `dir` whose stem matches `id`.
async fn find_local_font(dir: &Path, id: &str) -> crate::Result<Option<PathBuf>> {
    let mut entries = io::read_dir(dir).await?;
    while let Some(entry) = entries.next_entry().await.map_err(|e| {
        crate::ErrorKind::FSError(format!("Error reading fonts dir: {e}"))
    })? {
        let path = entry.path();
        if !is_local_font(&path) {
            continue;
        }
        let file_name =
            path.file_name().unwrap_or_default().to_string_lossy().to_string();
        if file_stem(&file_name) == id {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

/// Lists all installed fonts (metadata only — no font bytes). Files that can't
/// be read are silently skipped.
pub async fn list_installed_fonts() -> crate::Result<Vec<InstalledFont>> {
    let dir = fonts_dir().await?;
    let mut result = Vec::new();

    let mut entries = io::read_dir(&dir).await?;
    while let Some(entry) = entries.next_entry().await.map_err(|e| {
        crate::ErrorKind::FSError(format!("Error reading fonts dir: {e}"))
    })? {
        let path = entry.path();
        let file_name =
            path.file_name().unwrap_or_default().to_string_lossy().to_string();

        if file_name.ends_with(GOOGLE_FONT_FILE_EXTENSION) {
            let id = file_name
                .strip_suffix(&format!(".{GOOGLE_FONT_FILE_EXTENSION}"))
                .unwrap_or(&file_name)
                .to_string();
            let Ok(bytes) = io::read(&path).await else {
                continue;
            };
            let family = extract_font_family_header(&String::from_utf8_lossy(
                &bytes,
            ))
            .unwrap_or_else(|| id.clone());
            result.push(InstalledFont {
                id,
                name: family,
                source: "google".to_string(),
                file_name,
            });
        } else if is_local_font(&path) {
            let id = file_stem(&file_name);
            result.push(InstalledFont {
                id: id.clone(),
                name: id,
                source: "local".to_string(),
                file_name,
            });
        }
    }

    Ok(result)
}

/// Returns the ready-to-inject `@font-face` CSS (base64 data URIs) for one
/// installed font. Heavy base64 is materialized only for the active font.
pub async fn load_font_face(id: &str) -> crate::Result<String> {
    let dir = fonts_dir().await?;

    let google_path = dir.join(format!("{id}.{GOOGLE_FONT_FILE_EXTENSION}"));
    if tokio::fs::metadata(&google_path).await.is_ok() {
        let bytes = io::read(&google_path).await?;
        return Ok(String::from_utf8_lossy(&bytes).to_string());
    }

    if let Some(path) = find_local_font(&dir, id).await? {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .unwrap_or_default();
        let bytes = io::read(&path).await?;
        let data_uri =
            format!("data:{};base64,{}", font_mime(&ext), BASE64_STANDARD.encode(&bytes));
        return Ok(format!(
            "@font-face {{ font-family: '{id}'; src: url('{data_uri}'); font-display: swap; }}"
        ));
    }

    Err(crate::ErrorKind::OtherError(format!("Font not found: {id}")).into())
}

/// Downloads a font family from Google Fonts, embedding every weight's woff2 as
/// a base64 data URI in a self-contained `{Family}.mrfont.css` file.
pub async fn download_google_font(
    family: &str,
) -> crate::Result<InstalledFont> {
    let state = crate::State::get().await?;
    let family = family.trim();
    if family.is_empty() {
        return Err(crate::ErrorKind::OtherError(
            "Font family name is empty".to_string(),
        )
        .into());
    }

    let encoded_family = family.replace(' ', "+");

    // Progressively simpler weight specs — Google's API errors when a requested
    // weight doesn't exist, so fall back until one succeeds.
    let specs = [
        format!("family={encoded_family}:wght@400;500;600;700"),
        format!("family={encoded_family}:wght@400"),
        format!("family={encoded_family}"),
    ];

    let mut css: Option<String> = None;
    for spec in &specs {
        let url = format!("https://fonts.googleapis.com/css2?{spec}&display=swap");
        if let Ok(bytes) = crate::util::fetch::fetch_advanced(
            reqwest::Method::GET,
            &url,
            None,
            None,
            Some(("User-Agent", BROWSER_USER_AGENT)),
            None,
            None,
            None,
            &state.fetch_semaphore,
            &state.pool,
        )
        .await
        {
            css = Some(String::from_utf8_lossy(&bytes).to_string());
            break;
        }
    }

    let css = css.ok_or_else(|| {
        crate::ErrorKind::OtherError(format!(
            "Could not find font '{family}' on Google Fonts"
        ))
    })?;

    let url_re =
        regex::Regex::new(r"url\((https://fonts\.gstatic\.com/[^)]+)\)").unwrap();
    let urls: Vec<String> = url_re
        .captures_iter(&css)
        .map(|c| c[1].to_string())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    if urls.is_empty() {
        return Err(crate::ErrorKind::OtherError(format!(
            "No downloadable font files found for '{family}'"
        ))
        .into());
    }

    let mut rewritten = css.clone();
    for font_url in urls {
        let bytes = crate::util::fetch::fetch(
            &font_url,
            None,
            None,
            None,
            &state.fetch_semaphore,
            &state.pool,
        )
        .await?;
        let ext = font_url.rsplit_once('.').map(|(_, e)| e).unwrap_or("woff2");
        let data_uri = format!(
            "data:{};base64,{}",
            font_mime(ext),
            BASE64_STANDARD.encode(&bytes)
        );
        rewritten = rewritten.replace(
            &format!("url({font_url})"),
            &format!("url({data_uri})"),
        );
    }

    let content = format!("/* @font-family: {family} */\n{rewritten}");

    let dir = fonts_dir().await?;
    let sanitized = sanitize_stem(family);
    let file_name = format!("{sanitized}.{GOOGLE_FONT_FILE_EXTENSION}");
    let dest = dir.join(&file_name);
    io::write(&dest, content.as_bytes()).await?;

    Ok(InstalledFont {
        id: sanitized,
        name: family.to_string(),
        source: "google".to_string(),
        file_name,
    })
}

/// Deletes an installed font (Google download or local file) by id.
pub async fn delete_font(id: &str) -> crate::Result<()> {
    let dir = fonts_dir().await?;

    let google_path = dir.join(format!("{id}.{GOOGLE_FONT_FILE_EXTENSION}"));
    if tokio::fs::metadata(&google_path).await.is_ok() {
        io::remove_file(&google_path).await?;
        return Ok(());
    }

    if let Some(path) = find_local_font(&dir, id).await? {
        io::remove_file(&path).await?;
        return Ok(());
    }

    Err(crate::ErrorKind::OtherError(format!("Font not found: {id}")).into())
}
