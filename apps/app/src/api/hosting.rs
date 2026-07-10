use crate::api::Result;
use tauri::plugin::TauriPlugin;
use theseus::prelude::*;

pub fn init<R: tauri::Runtime>() -> TauriPlugin<R> {
    tauri::plugin::Builder::new("hosting")
        .invoke_handler(tauri::generate_handler![
            hosting_list_servers,
            hosting_create_server,
            hosting_remove_server,
            hosting_start_server,
            hosting_stop_server,
            hosting_server_status,
            hosting_running_servers,
            hosting_get_logs,
            hosting_send_command,
            hosting_ensure_tunnel,
            hosting_update_server,
            hosting_change_version,
            hosting_set_server_icon,
            hosting_clear_server_icon,
            hosting_get_server_icon,
        ])
        .build()
}

#[tauri::command]
pub async fn hosting_list_servers() -> Result<Vec<HostedServer>> {
    Ok(hosting::list_servers().await?)
}

#[tauri::command]
pub async fn hosting_create_server(
    name: String,
    version: Option<String>,
) -> Result<HostedServer> {
    Ok(hosting::create_server(name, version).await?)
}

#[tauri::command]
pub async fn hosting_remove_server(id: String) -> Result<()> {
    hosting::remove_server(id).await?;
    Ok(())
}

#[tauri::command]
pub async fn hosting_start_server(id: String) -> Result<()> {
    hosting::start_server(id).await?;
    Ok(())
}

#[tauri::command]
pub async fn hosting_stop_server(id: String) -> Result<()> {
    hosting::stop_server(id).await?;
    Ok(())
}

#[tauri::command]
pub async fn hosting_server_status(id: String) -> Result<bool> {
    Ok(hosting::server_status(id).await?)
}

#[tauri::command]
pub async fn hosting_running_servers() -> Result<Vec<String>> {
    Ok(hosting::running_servers().await?)
}

#[tauri::command]
pub async fn hosting_get_logs(id: String) -> Result<Vec<String>> {
    Ok(hosting::get_logs(id).await?)
}

#[tauri::command]
pub async fn hosting_send_command(id: String, command: String) -> Result<()> {
    hosting::send_command(id, command).await?;
    Ok(())
}

#[tauri::command]
pub async fn hosting_ensure_tunnel(id: String) -> Result<String> {
    Ok(hosting::ensure_tunnel(&id).await?)
}

#[tauri::command]
pub async fn hosting_update_server(
    id: String,
    name: String,
    max_players: u32,
    view_distance: u32,
) -> Result<HostedServer> {
    Ok(hosting::update_server(id, name, max_players, view_distance).await?)
}

#[tauri::command]
pub async fn hosting_change_version(
    id: String,
    version: String,
) -> Result<HostedServer> {
    Ok(hosting::change_version(id, version).await?)
}

#[tauri::command]
pub async fn hosting_set_server_icon(
    id: String,
    image: Vec<u8>,
) -> Result<()> {
    hosting::set_server_icon(id, image).await?;
    Ok(())
}

#[tauri::command]
pub async fn hosting_clear_server_icon(id: String) -> Result<()> {
    hosting::clear_server_icon(id).await?;
    Ok(())
}

#[tauri::command]
pub async fn hosting_get_server_icon(id: String) -> Result<Option<String>> {
    Ok(hosting::get_server_icon(id).await?)
}
