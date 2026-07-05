import { invoke } from '@tauri-apps/api/core'

export interface InstalledFont {
	id: string
	name: string
	source: 'google' | 'local'
	fileName: string
}

// List all installed fonts (metadata only) in the configured fonts folder
export async function listInstalledFonts(): Promise<InstalledFont[]> {
	return (await invoke('plugin:fonts|fonts_list_installed_fonts')) as InstalledFont[]
}

// Return the ready-to-inject @font-face CSS (base64 data URIs) for one font
export async function loadFontFace(id: string): Promise<string> {
	return (await invoke('plugin:fonts|fonts_load_font_face', { id })) as string
}

// Download a font family from Google Fonts into the fonts folder
export async function downloadGoogleFont(family: string): Promise<InstalledFont> {
	return (await invoke('plugin:fonts|fonts_download_google_font', { family })) as InstalledFont
}

// Delete an installed font by id
export async function deleteFont(id: string): Promise<void> {
	await invoke('plugin:fonts|fonts_delete_font', { id })
}

// (Re-)register the configured fonts folder with the file watcher
export async function watchFontsDir(): Promise<void> {
	await invoke('plugin:fonts|fonts_watch_dir')
}

// Accept either a full Google Fonts specimen URL (e.g.
// https://fonts.google.com/specimen/Roboto+Slab) or a bare family name, and
// return the clean family name ("Roboto Slab").
export function extractGoogleFontFamily(input: string): string {
	const trimmed = input.trim()
	const specimenMatch = trimmed.match(/fonts\.google\.com\/specimen\/([^/?#]+)/i)
	const raw = specimenMatch ? specimenMatch[1] : trimmed
	try {
		return decodeURIComponent(raw.replace(/\+/g, ' ')).trim()
	} catch {
		return raw.replace(/\+/g, ' ').trim()
	}
}
