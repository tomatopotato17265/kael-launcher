use crate::api::Result;
use tauri::plugin::TauriPlugin;
use theseus::hosting::playit::{ClaimInfo, ClaimPoll};
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
            hosting_playit_has_account,
            hosting_playit_begin_claim,
            hosting_playit_poll_claim,
            hosting_playit_guest_url,
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
pub async fn hosting_playit_has_account() -> Result<bool> {
    Ok(hosting::playit_has_account().await?)
}

#[tauri::command]
pub async fn hosting_playit_begin_claim() -> Result<ClaimInfo> {
    Ok(hosting::playit_begin_claim().await?)
}

#[tauri::command]
pub async fn hosting_playit_poll_claim(
    code: String,
    guest: bool,
) -> Result<ClaimPoll> {
    Ok(hosting::playit_poll_claim(code, guest).await?)
}

#[tauri::command]
pub async fn hosting_playit_guest_url() -> Result<Option<String>> {
    Ok(hosting::playit_guest_url().await?)
}
