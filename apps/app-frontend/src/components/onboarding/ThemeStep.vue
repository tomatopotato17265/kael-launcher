<script setup lang="ts">
import { CheckIcon, PaletteIcon } from '@kael/assets'
import { defineMessages, useVIntl } from '@kael/ui'
import { computed, ref } from 'vue'

import { get as getSettings, set as setSettings } from '@/helpers/settings.ts'
import { useOnboarding } from '@/store/onboarding.ts'
import type { ThemeConfig } from '@/store/theme.ts'
import { useTheming } from '@/store/theme.ts'
import { BUILT_IN_THEME_PRESETS } from '@/store/theme-presets.ts'

const CUSTOMIZE_LATER_ID = 'custom-later'

const themeStore = useTheming()
const onboarding = useOnboarding()
const { formatMessage } = useVIntl()

const messages = defineMessages({
	customizeLater: {
		id: 'app.onboarding.theme.customize-later',
		defaultMessage: 'Customize later in Settings',
	},
})

interface ThemeOption {
	id: string
	name: string
	colorTheme: string
	accentColor: string
}

const themeOptions = computed<ThemeOption[]>(() => [
	...BUILT_IN_THEME_PRESETS,
	...themeStore.installedThemes.map((installed) => ({
		id: installed.id,
		name: installed.theme.name,
		colorTheme: installed.theme.colorTheme ?? snapshot.light.colorTheme,
		accentColor: installed.theme.accentColor ?? snapshot.light.brandColor,
	})),
])

const snapshot: { light: ThemeConfig; dark: ThemeConfig; syncWithSystem: boolean } = {
	light: { ...themeStore.light },
	dark: { ...themeStore.dark },
	syncWithSystem: themeStore.syncWithSystem,
}

const selectedId = ref<string | null>(onboarding.answers.theme)

function select(option: ThemeOption) {
	selectedId.value = option.id
	const config: ThemeConfig = {
		colorTheme: option.colorTheme,
		brandColor: option.accentColor,
		activeThemePreset: option.id,
	}
	themeStore.light = { ...config }
	themeStore.dark = { ...config }
	themeStore.syncWithSystem = false
	themeStore.refreshActiveConfig()
}

function selectCustomizeLater() {
	selectedId.value = CUSTOMIZE_LATER_ID
	revertPreview()
}

function revertPreview() {
	themeStore.light = { ...snapshot.light }
	themeStore.dark = { ...snapshot.dark }
	themeStore.syncWithSystem = snapshot.syncWithSystem
	themeStore.refreshActiveConfig()
}

async function confirm() {
	onboarding.answers.theme = selectedId.value

	if (!selectedId.value || selectedId.value === CUSTOMIZE_LATER_ID) {
		return
	}

	const settings = await getSettings()
	settings.color_theme = themeStore.light.colorTheme
	settings.brand_color = themeStore.light.brandColor
	settings.active_theme_preset = themeStore.light.activeThemePreset
	settings.dark_color_theme = themeStore.dark.colorTheme
	settings.dark_brand_color = themeStore.dark.brandColor
	settings.dark_active_theme_preset = themeStore.dark.activeThemePreset
	settings.sync_theme_with_system = themeStore.syncWithSystem
	await setSettings(settings)
}

defineExpose({ confirm, revertPreview })
</script>

<template>
	<div class="grid w-full grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
		<button
			v-for="option in themeOptions"
			:key="option.id"
			class="flex cursor-pointer flex-col gap-3 rounded-xl border-2 border-solid bg-bg-raised p-4 text-left transition-colors"
			:class="selectedId === option.id ? 'border-brand' : 'border-transparent hover:border-divider'"
			:aria-pressed="selectedId === option.id"
			@click="select(option)"
		>
			<span
				class="flex h-20 w-full items-center justify-center gap-2 rounded-lg border border-solid border-divider"
				:style="{ backgroundColor: option.colorTheme }"
				aria-hidden="true"
			>
				<span class="h-6 w-6 rounded-full" :style="{ backgroundColor: option.accentColor }"></span>
				<span
					class="h-2 w-16 rounded-full"
					:style="{ backgroundColor: option.accentColor, opacity: 0.4 }"
				></span>
			</span>
			<span class="flex items-center justify-between gap-2 text-base font-medium text-contrast">
				{{ option.name }}
				<CheckIcon
					v-if="selectedId === option.id"
					class="h-5 w-5 shrink-0 text-brand"
					aria-hidden="true"
				/>
			</span>
		</button>
		<button
			class="flex cursor-pointer flex-col items-center justify-center gap-3 rounded-xl border-2 border-dashed p-4 transition-colors"
			:class="
				selectedId === CUSTOMIZE_LATER_ID ? 'border-brand' : 'border-divider hover:border-brand'
			"
			:aria-pressed="selectedId === CUSTOMIZE_LATER_ID"
			@click="selectCustomizeLater"
		>
			<PaletteIcon class="h-8 w-8 text-secondary" aria-hidden="true" />
			<span class="text-base font-medium text-contrast">
				{{ formatMessage(messages.customizeLater) }}
			</span>
		</button>
	</div>
</template>
