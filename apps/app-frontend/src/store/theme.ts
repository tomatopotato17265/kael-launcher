import { defineStore } from 'pinia'

import type { InstalledTheme } from '@/helpers/theming.ts'
import { listInstalledThemes } from '@/helpers/theming.ts'

let systemThemeMq: MediaQueryList | null = null

export const DEFAULT_FEATURE_FLAGS = {
	project_background: false,
	page_path: false,
	worlds_tab: false,
	worlds_in_home: true,
	server_project_qa: false,
	server_ram_as_bytes_always_on: false,
	always_show_app_controls: false,
	skip_unknown_pack_warning: false,
	i18n_debug: false,
	show_instance_play_time: true,
}

export type FeatureFlag = keyof typeof DEFAULT_FEATURE_FLAGS
export type FeatureFlags = Record<FeatureFlag, boolean>

/// The surface-shade CSS custom property names (without the leading `--`)
/// derived from a Color Theme hex seed, in surface-1 -> surface-5 order.
const SURFACE_KEYS = [
	'surface-1',
	'surface-1-5',
	'surface-2',
	'surface-2-5',
	'surface-3',
	'surface-4',
	'surface-5',
] as const

export interface ThemeConfig {
	colorTheme: string
	brandColor: string
	activeThemePreset: string | null
}

export const DEFAULT_THEME_CONFIG: ThemeConfig = {
	colorTheme: '#000000',
	brandColor: '#874EFE',
	activeThemePreset: 'default',
}

export type ThemeStore = {
	light: ThemeConfig
	dark: ThemeConfig
	syncWithSystem: boolean

	themeDir: string | null
	installedThemes: InstalledTheme[]

	advancedRendering: boolean
	hideNametagSkinsPage: boolean

	devMode: boolean
	featureFlags: FeatureFlags
}

export const DEFAULT_THEME_STORE: ThemeStore = {
	light: { ...DEFAULT_THEME_CONFIG },
	dark: { ...DEFAULT_THEME_CONFIG },
	syncWithSystem: false,

	themeDir: null,
	installedThemes: [],

	advancedRendering: true,
	hideNametagSkinsPage: false,

	devMode: false,
	featureFlags: DEFAULT_FEATURE_FLAGS,
}

function hexToRgb(hex: string): [number, number, number] {
	return [parseInt(hex.slice(1, 3), 16), parseInt(hex.slice(3, 5), 16), parseInt(hex.slice(5, 7), 16)]
}

function rgbToHex(r: number, g: number, b: number): string {
	const clamp = (v: number) => Math.max(0, Math.min(255, Math.round(v)))
	const toHex = (v: number) => clamp(v).toString(16).padStart(2, '0')
	return `#${toHex(r)}${toHex(g)}${toHex(b)}`
}

// WCAG sRGB relative luminance, used to decide whether a Color Theme hex
// should use the light-base or dark-base palette for text/semantic colors.
function relativeLuminance(hex: string): number {
	const [r, g, b] = hexToRgb(hex).map((c) => c / 255)
	const lin = (v: number) => (v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4)
	return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)
}

function mixHex(a: string, b: string, pct: number): string {
	const [ar, ag, ab] = hexToRgb(a)
	const [br, bg, bb] = hexToRgb(b)
	const t = pct / 100
	return rgbToHex(ar + (br - ar) * t, ag + (bg - ag) * t, ab + (bb - ab) * t)
}

// Below this relative luminance, a Color Theme hex is treated as "dark base"
// (light text on dark surfaces); above it, "light base" (dark text on light
// surfaces). Calibrated so today's dark surface-1 (#16181c) and light
// surface-1 (#ebebeb) classify unambiguously, and #000000 (the new default)
// classifies as dark.
const LUMINANCE_THRESHOLD = 0.18

function deriveSurfaceRamp(colorThemeHex: string): {
	isDark: boolean
	surfaces: Record<string, string>
} {
	const isDark = relativeLuminance(colorThemeHex) <= LUMINANCE_THRESHOLD

	let values: string[]
	if (isDark) {
		// Matches the existing OLED ramp's actual brightness steps (seed
		// #000000 reproduces today's OLED look almost exactly).
		values = [0, 2, 3.5, 5.5, 6.5, 11, 15].map((pct) => mixHex(colorThemeHex, '#ffffff', pct))
	} else {
		// Matches today's light-mode ramp: surface-4 pinned to pure white,
		// surface-5 slightly darker than surface-1 rather than lighter.
		values = [0, 10, 50, 57, 65, 100].map((pct) => mixHex(colorThemeHex, '#ffffff', pct))
		values.push(mixHex(colorThemeHex, '#000000', 6))
	}

	const surfaces: Record<string, string> = {}
	SURFACE_KEYS.forEach((key, i) => {
		surfaces[key] = values[i]
	})

	return { isDark, surfaces }
}

// CSS custom property keys most recently applied by an active theme preset's
// `variables` overrides, so switching away from a preset clears them instead
// of leaving stale inline overrides behind.
let appliedVariableKeys: string[] = []

export const useTheming = defineStore('themeStore', {
	state: () => DEFAULT_THEME_STORE,
	actions: {
		applyTheme(config: ThemeConfig) {
			const html = document.getElementsByTagName('html')[0]
			const root = document.documentElement
			const { isDark, surfaces } = deriveSurfaceRamp(config.colorTheme)

			html.classList.remove('light-mode', 'dark-mode', 'oled-mode')
			html.classList.add(isDark ? 'dark-mode' : 'light-mode')

			for (const [key, value] of Object.entries(surfaces)) {
				root.style.setProperty(`--${key}`, value)
			}

			this.applyBrandColor(config.brandColor)

			for (const key of appliedVariableKeys) {
				root.style.removeProperty(key)
			}
			appliedVariableKeys = []

			const installed = this.installedThemes.find((t) => t.id === config.activeThemePreset)
			if (installed?.theme.variables) {
				for (const [key, value] of Object.entries(installed.theme.variables)) {
					root.style.setProperty(key, value)
				}
				appliedVariableKeys = Object.keys(installed.theme.variables)
			}
		},
		applyBrandColor(color: string) {
			const root = document.documentElement
			const [r, g, b] = hexToRgb(color)
			root.style.setProperty('--color-brand', color)
			root.style.setProperty('--color-brand-highlight', `rgba(${r}, ${g}, ${b}, 0.25)`)
			root.style.setProperty('--color-brand-shadow', `rgba(${r}, ${g}, ${b}, 0.7)`)
			const [dr, dg, db] = hexToRgb(mixHex(color, '#000000', 60))
			root.style.setProperty('--color-brand-overlay', `rgba(${dr}, ${dg}, ${db}, 0.35)`)
		},
		setSyncWithSystem(enabled: boolean) {
			this.syncWithSystem = enabled
			this.refreshActiveConfig()
		},
		refreshActiveConfig() {
			systemThemeMq?.removeEventListener('change', this.refreshActiveConfig)
			systemThemeMq = null

			if (!this.syncWithSystem) {
				this.applyTheme(this.light)
				return
			}

			systemThemeMq = window.matchMedia('(prefers-color-scheme: dark)')
			systemThemeMq.addEventListener('change', this.refreshActiveConfig)
			this.applyTheme(systemThemeMq.matches ? this.dark : this.light)
		},
		async loadInstalledThemes() {
			this.installedThemes = await listInstalledThemes()
		},
		getFeatureFlag(key: FeatureFlag) {
			return this.featureFlags[key] ?? DEFAULT_FEATURE_FLAGS[key]
		},
	},
})
