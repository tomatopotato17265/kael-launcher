//! Custom theme file discovery, validation, and installation.

use crate::state::Settings;
use crate::util::io;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const THEME_FILE_EXTENSION: &str = "mrtheme.json";

/// On-disk schema for a theme preset/uploaded theme file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeFile {
    pub schema_version: u32,
    pub name: String,
    pub color_theme: String,
    pub accent_color: String,
    #[serde(default)]
    pub variables: HashMap<String, String>,
}

/// A theme file that has been located and successfully validated.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledTheme {
    /// File stem (without extension), used as a stable identifier for selection.
    pub id: String,
    pub file_name: String,
    pub theme: ThemeFile,
}

fn validate_theme_file(raw: &[u8]) -> crate::Result<ThemeFile> {
    let theme: ThemeFile = serde_json::from_slice(raw).map_err(|e| {
        crate::ErrorKind::OtherError(format!("Invalid theme file: {e}"))
    })?;

    if theme.schema_version != 1 {
        return Err(crate::ErrorKind::OtherError(format!(
            "Unsupported theme schema version: {}",
            theme.schema_version
        ))
        .into());
    }

    let name = theme.name.trim();
    if name.is_empty() || name.chars().count() > 64 {
        return Err(crate::ErrorKind::OtherError(
            "Theme name must be 1-64 characters".to_string(),
        )
        .into());
    }

    let hex_re = regex::Regex::new(r"^#[0-9a-fA-F]{6}$").unwrap();
    if !hex_re.is_match(&theme.color_theme) {
        return Err(crate::ErrorKind::OtherError(
            "colorTheme must be a 6-digit hex color".to_string(),
        )
        .into());
    }
    if !hex_re.is_match(&theme.accent_color) {
        return Err(crate::ErrorKind::OtherError(
            "accentColor must be a 6-digit hex color".to_string(),
        )
        .into());
    }

    let var_key_re = regex::Regex::new(r"^--[a-z0-9-]+$").unwrap();
    for (key, value) in &theme.variables {
        if !var_key_re.is_match(key) {
            return Err(crate::ErrorKind::OtherError(format!(
                "Invalid CSS variable name in theme: {key}"
            ))
            .into());
        }
        if value.is_empty() || value.chars().count() > 512 {
            return Err(crate::ErrorKind::OtherError(format!(
                "Invalid value for CSS variable {key}"
            ))
            .into());
        }
    }

    Ok(theme)
}

/// Directory where custom theme files live: `settings.theme_dir` if set,
/// otherwise `config_dir/themes`.
async fn themes_dir() -> crate::Result<std::path::PathBuf> {
    let state = crate::State::get().await?;
    let settings = Settings::get(&state.pool).await?;

    let dir = match settings.theme_dir {
        Some(dir) => std::path::PathBuf::from(dir),
        None => state.directories.config_dir.join("themes"),
    };
    io::create_dir_all(&dir).await?;

    Ok(dir)
}

/// Lists and parses all valid theme files in the configured themes folder.
/// Files that fail to parse/validate are silently skipped, so one bad or
/// hand-edited file doesn't break the whole list.
pub async fn list_installed_themes() -> crate::Result<Vec<InstalledTheme>> {
    let dir = themes_dir().await?;
    let mut result = Vec::new();

    let mut entries = io::read_dir(&dir).await?;
    while let Some(entry) = entries.next_entry().await.map_err(|e| {
        crate::ErrorKind::FSError(format!("Error reading themes dir: {e}"))
    })? {
        let path = entry.path();
        let file_name =
            path.file_name().unwrap_or_default().to_string_lossy().to_string();
        if !file_name.ends_with(THEME_FILE_EXTENSION) {
            continue;
        }

        let Ok(bytes) = io::read(&path).await else {
            continue;
        };
        if let Ok(theme) = validate_theme_file(&bytes) {
            let id = file_name
                .strip_suffix(&format!(".{THEME_FILE_EXTENSION}"))
                .unwrap_or(&file_name)
                .to_string();
            result.push(InstalledTheme {
                id,
                file_name,
                theme,
            });
        }
    }

    Ok(result)
}

/// Validates an externally-picked file (an arbitrary path from a native file
/// picker) and, only if it's valid, copies it into the themes folder.
/// Invalid files are never written to disk.
pub async fn upload_theme(source_path: &str) -> crate::Result<InstalledTheme> {
    let source = std::path::PathBuf::from(source_path);
    let bytes = io::read(&source).await?;
    let theme = validate_theme_file(&bytes)?;

    let dir = themes_dir().await?;
    let sanitized_stem: String = theme
        .name
        .trim()
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect();
    let file_name = format!("{sanitized_stem}.{THEME_FILE_EXTENSION}");
    let dest = dir.join(&file_name);
    io::write(&dest, &bytes).await?;

    Ok(InstalledTheme {
        id: sanitized_stem,
        file_name,
        theme,
    })
}
