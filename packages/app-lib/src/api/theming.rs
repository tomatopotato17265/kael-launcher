//! Custom theme file discovery, validation, and installation.

use crate::state::Settings;
use crate::util::io;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const THEME_FILE_EXTENSION: &str = "mrtheme.json";
pub const CSS_THEME_FILE_EXTENSION: &str = "css";

/// Upper bound on a CSS theme file's size, guarding against pathological input.
const MAX_CSS_THEME_BYTES: usize = 512 * 1024;

/// On-disk schema for a theme preset/uploaded theme file.
///
/// A theme is either a structured JSON theme (`color_theme`/`accent_color` +
/// an optional `variables` map of CSS custom-property overrides) or a raw CSS
/// theme (just `css`, synthesized from a standalone `.css` file). The two kinds
/// are distinguished by whether `css` is present.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeFile {
    pub schema_version: u32,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color_theme: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accent_color: Option<String>,
    #[serde(default)]
    pub variables: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub css: Option<String>,
    /// Default font family for this theme (applied when the theme is selected).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font: Option<String>,
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

/// Validates a structured `.mrtheme.json` theme file.
fn validate_json_theme_file(raw: &[u8]) -> crate::Result<ThemeFile> {
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
    if !theme.color_theme.as_deref().is_some_and(|c| hex_re.is_match(c)) {
        return Err(crate::ErrorKind::OtherError(
            "colorTheme must be a 6-digit hex color".to_string(),
        )
        .into());
    }
    if !theme.accent_color.as_deref().is_some_and(|c| hex_re.is_match(c)) {
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

/// Reads the display name from a `/* @name ... */` header comment, if present.
fn extract_css_theme_name(css: &str) -> Option<String> {
    let re = regex::Regex::new(r"/\*\s*@name\s+(.+?)\s*\*/").unwrap();
    re.captures(css)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Reads the default font family from a `/* @font ... */` header comment.
fn extract_css_theme_font(css: &str) -> Option<String> {
    let re = regex::Regex::new(r"/\*\s*@font\s+(.+?)\s*\*/").unwrap();
    re.captures(css)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Builds a raw-CSS theme from a standalone `.css` file's contents, deriving
/// the display name from a `/* @name ... */` header (falling back to the file
/// stem). Deep CSS *syntax* validation is left to the frontend (css-tree);
/// here we only enforce UTF-8 and a size cap.
fn css_theme_from_source(
    raw: &[u8],
    fallback_name: &str,
) -> crate::Result<ThemeFile> {
    if raw.len() > MAX_CSS_THEME_BYTES {
        return Err(crate::ErrorKind::OtherError(
            "CSS theme file is too large (max 512 KB)".to_string(),
        )
        .into());
    }

    let css = String::from_utf8(raw.to_vec()).map_err(|_| {
        crate::ErrorKind::OtherError(
            "CSS theme file must be valid UTF-8".to_string(),
        )
    })?;

    let mut name = extract_css_theme_name(&css)
        .unwrap_or_else(|| fallback_name.to_string());
    if name.chars().count() > 64 {
        name = name.chars().take(64).collect();
    }
    if name.trim().is_empty() {
        name = fallback_name.to_string();
    }

    let font = extract_css_theme_font(&css);

    Ok(ThemeFile {
        schema_version: 1,
        name,
        color_theme: None,
        accent_color: None,
        variables: HashMap::new(),
        css: Some(css),
        font,
    })
}

/// Sanitizes a theme name into a safe file stem.
fn sanitize_stem(name: &str) -> String {
    name.trim()
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect()
}

/// Directory where custom theme files live: `settings.theme_dir` if set,
/// otherwise `config_dir/themes`.
pub async fn themes_dir() -> crate::Result<std::path::PathBuf> {
    let state = crate::State::get().await?;
    let settings = Settings::get(&state.pool).await?;

    let dir = match settings.theme_dir {
        Some(dir) => std::path::PathBuf::from(dir),
        None => state.directories.config_dir.join("themes"),
    };
    io::create_dir_all(&dir).await?;

    Ok(dir)
}

/// (Re-)registers the configured themes directory with the file watcher so
/// edits to theme files are picked up live. Called on startup and whenever the
/// themes folder setting changes.
pub async fn refresh_themes_watch() -> crate::Result<()> {
    let state = crate::State::get().await?;
    let dir = themes_dir().await?;
    crate::state::fs_watcher::watch_themes_dir(&state.file_watcher, &dir).await;
    Ok(())
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

        let Ok(bytes) = io::read(&path).await else {
            continue;
        };

        let parsed = if file_name.ends_with(THEME_FILE_EXTENSION) {
            let id = file_name
                .strip_suffix(&format!(".{THEME_FILE_EXTENSION}"))
                .unwrap_or(&file_name)
                .to_string();
            validate_json_theme_file(&bytes).map(|theme| (id, theme))
        } else if file_name.ends_with(&format!(".{CSS_THEME_FILE_EXTENSION}")) {
            let id = file_name
                .strip_suffix(&format!(".{CSS_THEME_FILE_EXTENSION}"))
                .unwrap_or(&file_name)
                .to_string();
            css_theme_from_source(&bytes, &id).map(|theme| (id, theme))
        } else {
            continue;
        };

        if let Ok((id, theme)) = parsed {
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
/// Invalid files are never written to disk. Supports both structured
/// `.mrtheme.json` themes and raw `.css` themes.
pub async fn upload_theme(source_path: &str) -> crate::Result<InstalledTheme> {
    let source = std::path::PathBuf::from(source_path);
    let bytes = io::read(&source).await?;

    let is_css = source
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case(CSS_THEME_FILE_EXTENSION));

    let source_stem = source
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "theme".to_string());

    let (theme, extension) = if is_css {
        (css_theme_from_source(&bytes, &source_stem)?, CSS_THEME_FILE_EXTENSION)
    } else {
        (validate_json_theme_file(&bytes)?, THEME_FILE_EXTENSION)
    };

    let dir = themes_dir().await?;
    let sanitized_stem = sanitize_stem(&theme.name);
    let file_name = format!("{sanitized_stem}.{extension}");
    let dest = dir.join(&file_name);
    io::write(&dest, &bytes).await?;

    Ok(InstalledTheme {
        id: sanitized_stem,
        file_name,
        theme,
    })
}
