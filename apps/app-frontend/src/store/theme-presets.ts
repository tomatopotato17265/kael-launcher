export interface BuiltInThemePreset {
	id: string
	name: string
	colorTheme: string
	accentColor: string
}

export const BUILT_IN_THEME_PRESETS: BuiltInThemePreset[] = [
	{ id: 'default', name: 'Default', colorTheme: '#000000', accentColor: '#874EFE' },
	{ id: 'classic-dark', name: 'Classic Dark', colorTheme: '#16181c', accentColor: '#874EFE' },
	{ id: 'classic-light', name: 'Classic Light', colorTheme: '#ebebeb', accentColor: '#874EFE' },
]
