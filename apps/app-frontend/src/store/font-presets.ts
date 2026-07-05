export interface BuiltInFontPreset {
	id: string
	name: string
	family: string
}

// The built-in "Default" font (Inter) is always present and can never be
// deleted — selecting it removes any font override and reverts to the app's
// stock font stack.
export const BUILT_IN_FONT_PRESETS: BuiltInFontPreset[] = [
	{ id: 'default', name: 'Default', family: 'Inter' },
]

export const DEFAULT_FONT_ID = 'default'
