import { invoke } from '@tauri-apps/api/core'

export interface HostedServer {
	id: string
	name: string
	directory: string
	mc_version: string
	java_path: string | null
	port: number
	playit_tunnel_id: string | null
	tunnel_url: string | null
	custom_domain: string | null
	cf_record_ids: string | null
	created: number
	modified: number
}

export interface ClaimInfo {
	code: string
	url: string
}

export interface ClaimPoll {
	status: string
	secret: string | null
}

export async function listServers(): Promise<HostedServer[]> {
	return await invoke('plugin:hosting|hosting_list_servers')
}

export async function createServer(name: string, version?: string | null): Promise<HostedServer> {
	return await invoke('plugin:hosting|hosting_create_server', { name, version: version ?? null })
}

export async function removeServer(id: string): Promise<void> {
	return await invoke('plugin:hosting|hosting_remove_server', { id })
}

export async function startServer(id: string): Promise<void> {
	return await invoke('plugin:hosting|hosting_start_server', { id })
}

export async function stopServer(id: string): Promise<void> {
	return await invoke('plugin:hosting|hosting_stop_server', { id })
}

export async function serverStatus(id: string): Promise<boolean> {
	return await invoke('plugin:hosting|hosting_server_status', { id })
}

export async function runningServers(): Promise<string[]> {
	return await invoke('plugin:hosting|hosting_running_servers')
}

export async function getLogs(id: string): Promise<string[]> {
	return await invoke('plugin:hosting|hosting_get_logs', { id })
}

export async function sendCommand(id: string, command: string): Promise<void> {
	return await invoke('plugin:hosting|hosting_send_command', { id, command })
}

export async function ensureTunnel(id: string): Promise<string> {
	return await invoke('plugin:hosting|hosting_ensure_tunnel', { id })
}

export async function playitHasAccount(): Promise<boolean> {
	return await invoke('plugin:hosting|hosting_playit_has_account')
}

export async function playitBeginClaim(): Promise<ClaimInfo> {
	return await invoke('plugin:hosting|hosting_playit_begin_claim')
}

export async function playitPollClaim(code: string, guest: boolean): Promise<ClaimPoll> {
	return await invoke('plugin:hosting|hosting_playit_poll_claim', { code, guest })
}

export async function playitGuestUrl(): Promise<string | null> {
	return await invoke('plugin:hosting|hosting_playit_guest_url')
}
