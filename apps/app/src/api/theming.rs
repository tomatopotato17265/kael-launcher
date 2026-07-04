use crate::api::Result;
use theseus::prelude::*;

pub fn init<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("theming")
        .invoke_handler(tauri::generate_handler![
            theming_list_installed_themes,
            theming_upload_theme,
        ])
        .build()
}

// List all valid custom theme files in the configured themes folder
// invoke('plugin:theming|theming_list_installed_themes')
#[tauri::command]
pub async fn theming_list_installed_themes()
-> Result<Vec<theming::InstalledTheme>> {
    let res = theming::list_installed_themes().await?;
    Ok(res)
}

// Validate an externally-picked theme file and copy it into the themes folder
// invoke('plugin:theming|theming_upload_theme', { sourcePath })
#[tauri::command]
pub async fn theming_upload_theme(
    source_path: String,
) -> Result<theming::InstalledTheme> {
    let res = theming::upload_theme(&source_path).await?;
    Ok(res)
}
