use crate::api::Result;
use theseus::prelude::*;

pub fn init<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("fonts")
        .invoke_handler(tauri::generate_handler![
            fonts_list_installed_fonts,
            fonts_load_font_face,
            fonts_download_google_font,
            fonts_delete_font,
            fonts_watch_dir,
        ])
        .build()
}

// List all installed fonts (metadata only) in the configured fonts folder
// invoke('plugin:fonts|fonts_list_installed_fonts')
#[tauri::command]
pub async fn fonts_list_installed_fonts() -> Result<Vec<fonts::InstalledFont>> {
    let res = fonts::list_installed_fonts().await?;
    Ok(res)
}

// Return the ready-to-inject @font-face CSS (base64 data URIs) for one font
// invoke('plugin:fonts|fonts_load_font_face', { id })
#[tauri::command]
pub async fn fonts_load_font_face(id: String) -> Result<String> {
    let res = fonts::load_font_face(&id).await?;
    Ok(res)
}

// Download a font family from Google Fonts into the fonts folder
// invoke('plugin:fonts|fonts_download_google_font', { family })
#[tauri::command]
pub async fn fonts_download_google_font(
    family: String,
) -> Result<fonts::InstalledFont> {
    let res = fonts::download_google_font(&family).await?;
    Ok(res)
}

// Delete an installed font by id
// invoke('plugin:fonts|fonts_delete_font', { id })
#[tauri::command]
pub async fn fonts_delete_font(id: String) -> Result<()> {
    fonts::delete_font(&id).await?;
    Ok(())
}

// (Re-)register the configured fonts folder with the file watcher
// invoke('plugin:fonts|fonts_watch_dir')
#[tauri::command]
pub async fn fonts_watch_dir() -> Result<()> {
    fonts::refresh_fonts_watch().await?;
    Ok(())
}
