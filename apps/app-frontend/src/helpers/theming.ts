import { invoke } from '@tauri-apps/api/core'

export interface ThemeFile {
	schemaVersion: number
	name: string
	colorTheme?: string
	accentColor?: string
	variables?: Record<string, string>
	css?: string
	font?: string
}

export interface InstalledTheme {
	id: string
	fileName: string
	theme: ThemeFile
}

export interface CssThemeError {
	message: string
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

// (Re-)register the configured themes folder with the file watcher so live
// edits hot-reload. Call after changing the themes folder path.
export async function watchThemesDir(): Promise<void> {
	await invoke('plugin:theming|theming_watch_dir')
}

// Open the native WebView inspector for debugging themes.
export async function openDevtools(): Promise<void> {
	await invoke('open_devtools')
}

// Parse theme CSS with css-tree in tolerant mode, collecting every syntax
// error so theme creators learn why a rule didn't apply (browsers silently
// drop invalid CSS with no feedback).
export async function validateThemeCss(css: string): Promise<CssThemeError[]> {
	const { parse } = await import('css-tree')
	const errors: CssThemeError[] = []
	parse(css, {
		positions: true,
		onParseError: (error) => {
			errors.push({ message: error.formattedMessage || error.message })
		},
	})
	return errors
}
