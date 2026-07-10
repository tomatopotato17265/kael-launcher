import { invoke } from '@tauri-apps/api/core'

/**
 * Which server jar a hosted server runs. Only Paper can be given players' real
 * Mojang identities; `vanilla` servers predate that and see offline UUIDs.
 */
export type ServerFlavor = 'vanilla' | 'paper'

export interface HostedServer {
	id: string
	name: string
	directory: string
	mc_version: string
	java_path: string | null
	port: number
	endpoint_name: string | null
	flavor: ServerFlavor
	created: number
	modified: number
}

/** Matches the public suffix every Minekube Connect endpoint resolves under. */
const CONNECT_PUBLIC_SUFFIX = 'play.minekube.net'

export function serverAddress(server: HostedServer): string | null {
	return server.endpoint_name ? `${server.endpoint_name}.${CONNECT_PUBLIC_SUFFIX}` : null
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
