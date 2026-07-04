import { invoke } from '@tauri-apps/api/core'

export interface ThemeFile {
	schemaVersion: number
	name: string
	colorTheme: string
	accentColor: string
	variables?: Record<string, string>
}

export interface InstalledTheme {
	id: string
	fileName: string
	theme: ThemeFile
}

// List all valid custom theme files in the configured themes folder
export async function listInstalledThemes(): Promise<InstalledTheme[]> {
	return (await invoke('plugin:theming|theming_list_installed_themes')) as InstalledTheme[]
}

// Validate an externally-picked theme file and copy it into the themes folder
export async function uploadTheme(sourcePath: string): Promise<InstalledTheme> {
	return (await invoke('plugin:theming|theming_upload_theme', {
		sourcePath,
	})) as InstalledTheme
}
