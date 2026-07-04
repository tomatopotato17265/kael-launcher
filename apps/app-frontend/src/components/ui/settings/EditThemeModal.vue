<script setup lang="ts">
import { FolderSearchIcon, PaletteIcon, UploadIcon } from '@kael/assets'
import {
	ButtonStyled,
	Combobox,
	defineMessages,
	injectNotificationManager,
	NewModal,
	StyledInput,
	Toggle,
	useVIntl,
} from '@kael/ui'
import { open } from '@tauri-apps/plugin-dialog'
import { computed, ref, watch } from 'vue'

import { get, set } from '@/helpers/settings.ts'
import { uploadTheme } from '@/helpers/theming.ts'
import { useTheming } from '@/store/state'
import { BUILT_IN_THEME_PRESETS } from '@/store/theme-presets.ts'

const { formatMessage } = useVIntl()
const { addNotification } = injectNotificationManager()
const themeStore = useTheming()

const modal = ref<InstanceType<typeof NewModal>>()
defineExpose({
	show: (event?: MouseEvent) => modal.value?.show(event),
	hide: () => modal.value?.hide(),
})

const messages = defineMessages({
	header: {
		id: 'app.appearance-settings.edit-theme-modal.header',
		defaultMessage: 'Edit Theme',
	},
	accentColorTitle: {
		id: 'app.appearance-settings.edit-theme-modal.accent-color.title',
		defaultMessage: 'Accent color',
	},
	colorThemeTitle: {
		id: 'app.appearance-settings.edit-theme-modal.color-theme.title',
		defaultMessage: 'Color Theme',
	},
	presetTitle: {
		id: 'app.appearance-settings.edit-theme-modal.preset.title',
		defaultMessage: 'Preset',
	},
	customPreset: {
		id: 'app.appearance-settings.edit-theme-modal.preset.custom',
		defaultMessage: 'Custom',
	},
	syncWithSystemTitle: {
		id: 'app.appearance-settings.edit-theme-modal.sync-with-system.title',
		defaultMessage: 'Sync with system',
	},
	syncWithSystemDescription: {
		id: 'app.appearance-settings.edit-theme-modal.sync-with-system.description',
		defaultMessage: 'Configure separate themes for light and dark system preference.',
	},
	darkVariantHeading: {
		id: 'app.appearance-settings.edit-theme-modal.dark-variant.heading',
		defaultMessage: 'Dark mode theme',
	},
	themeFolderTitle: {
		id: 'app.appearance-settings.edit-theme-modal.theme-folder.title',
		defaultMessage: 'Themes folder',
	},
	themeFolderDescription: {
		id: 'app.appearance-settings.edit-theme-modal.theme-folder.description',
		defaultMessage: 'Where custom theme files are stored.',
	},
	themeFolderPlaceholder: {
		id: 'app.appearance-settings.edit-theme-modal.theme-folder.placeholder',
		defaultMessage: 'Default folder',
	},
	uploadThemeButton: {
		id: 'app.appearance-settings.edit-theme-modal.upload-theme.button',
		defaultMessage: 'Upload Theme',
	},
	uploadSuccessTitle: {
		id: 'app.appearance-settings.edit-theme-modal.upload-theme.success-title',
		defaultMessage: 'Theme uploaded',
	},
	uploadSuccessText: {
		id: 'app.appearance-settings.edit-theme-modal.upload-theme.success-text',
		defaultMessage: 'Your theme was added and is ready to use.',
	},
	uploadErrorTitle: {
		id: 'app.appearance-settings.edit-theme-modal.upload-theme.error-title',
		defaultMessage: 'Invalid theme file',
	},
})

const settings = ref(await get())

watch(
	settings,
	async () => {
		await set(settings.value)

		themeStore.light = {
			colorTheme: settings.value.color_theme,
			brandColor: settings.value.brand_color,
			activeThemePreset: settings.value.active_theme_preset,
		}
		themeStore.dark = {
			colorTheme: settings.value.dark_color_theme,
			brandColor: settings.value.dark_brand_color,
			activeThemePreset: settings.value.dark_active_theme_preset,
		}
		themeStore.syncWithSystem = settings.value.sync_theme_with_system
		themeStore.themeDir = settings.value.theme_dir
		themeStore.refreshActiveConfig()
	},
	{ deep: true },
)

const presetOptions = computed(() => [
	...BUILT_IN_THEME_PRESETS.map((preset) => ({ value: preset.id, label: preset.name })),
	...themeStore.installedThemes.map((installed) => ({
		value: installed.id,
		label: installed.theme.name,
	})),
])

function presetDisplayValue(id: string | null) {
	return presetOptions.value.find((option) => option.value === id)?.label ?? formatMessage(messages.customPreset)
}

function applyPreset(variant: 'light' | 'dark', id: string) {
	const builtIn = BUILT_IN_THEME_PRESETS.find((preset) => preset.id === id)
	const installed = themeStore.installedThemes.find((theme) => theme.id === id)
	const colorTheme = builtIn?.colorTheme ?? installed?.theme.colorTheme
	const accentColor = builtIn?.accentColor ?? installed?.theme.accentColor
	if (!colorTheme || !accentColor) return

	if (variant === 'light') {
		settings.value.color_theme = colorTheme
		settings.value.brand_color = accentColor
		settings.value.active_theme_preset = id
	} else {
		settings.value.dark_color_theme = colorTheme
		settings.value.dark_brand_color = accentColor
		settings.value.dark_active_theme_preset = id
	}
}

function onColorInputChange(variant: 'light' | 'dark', field: 'colorTheme' | 'accentColor', e: Event) {
	const value = (e.target as HTMLInputElement).value
	if (variant === 'light') {
		if (field === 'colorTheme') settings.value.color_theme = value
		else settings.value.brand_color = value
		settings.value.active_theme_preset = null
	} else {
		if (field === 'colorTheme') settings.value.dark_color_theme = value
		else settings.value.dark_brand_color = value
		settings.value.dark_active_theme_preset = null
	}
}

async function browseThemeDir() {
	const newDir = await open({
		multiple: false,
		directory: true,
		title: 'Select a themes folder',
	})
	if (newDir) {
		settings.value.theme_dir = newDir
		await themeStore.loadInstalledThemes()
	}
}

async function handleUploadTheme() {
	const picked = await open({
		multiple: false,
		filters: [{ name: 'Modrinth Theme', extensions: ['json'] }],
	})
	if (!picked) return

	try {
		await uploadTheme(picked)
		await themeStore.loadInstalledThemes()
		addNotification({
			title: formatMessage(messages.uploadSuccessTitle),
			text: formatMessage(messages.uploadSuccessText),
			type: 'success',
		})
	} catch (e) {
		addNotification({
			title: formatMessage(messages.uploadErrorTitle),
			text: e instanceof Error ? e.message : String(e),
			type: 'error',
		})
	}
}
</script>
<template>
	<NewModal ref="modal" :header="formatMessage(messages.header)" scrollable max-width="30rem">
		<div class="flex flex-col gap-4">
			<div class="flex items-center justify-between gap-4">
				<h2 class="m-0 text-lg font-semibold text-contrast">
					{{ formatMessage(messages.accentColorTitle) }}
				</h2>
				<ButtonStyled>
					<label class="button-like relative" for="edit-theme-accent-color">
						<PaletteIcon :style="{ color: settings.brand_color }" />
						<input
							id="edit-theme-accent-color"
							type="color"
							class="absolute inset-0 h-full w-full cursor-pointer opacity-0"
							:value="settings.brand_color"
							@change="(e) => onColorInputChange('light', 'accentColor', e)"
						/>
					</label>
				</ButtonStyled>
			</div>

			<div class="flex items-center justify-between gap-4">
				<h2 class="m-0 text-lg font-semibold text-contrast">
					{{ formatMessage(messages.colorThemeTitle) }}
				</h2>
				<ButtonStyled>
					<label class="button-like relative" for="edit-theme-color-theme">
						<PaletteIcon :style="{ color: settings.color_theme }" />
						<input
							id="edit-theme-color-theme"
							type="color"
							class="absolute inset-0 h-full w-full cursor-pointer opacity-0"
							:value="settings.color_theme"
							@change="(e) => onColorInputChange('light', 'colorTheme', e)"
						/>
					</label>
				</ButtonStyled>
			</div>

			<div class="flex items-center justify-between gap-4">
				<h2 class="m-0 text-lg font-semibold text-contrast">
					{{ formatMessage(messages.presetTitle) }}
				</h2>
				<Combobox
					id="edit-theme-preset"
					class="max-w-48"
					:model-value="settings.active_theme_preset"
					name="Theme preset dropdown"
					:options="presetOptions"
					:display-value="presetDisplayValue(settings.active_theme_preset)"
					@update:model-value="(id) => applyPreset('light', id)"
				/>
			</div>

			<div class="flex items-center justify-between gap-4">
				<div>
					<h2 class="m-0 text-lg font-semibold text-contrast">
						{{ formatMessage(messages.syncWithSystemTitle) }}
					</h2>
					<p class="m-0 mt-1">{{ formatMessage(messages.syncWithSystemDescription) }}</p>
				</div>
				<Toggle
					id="edit-theme-sync-with-system"
					:model-value="settings.sync_theme_with_system"
					@update:model-value="(e) => (settings.sync_theme_with_system = !!e)"
				/>
			</div>

			<template v-if="settings.sync_theme_with_system">
				<h3 class="m-0 text-md font-semibold text-contrast">
					{{ formatMessage(messages.darkVariantHeading) }}
				</h3>

				<div class="flex items-center justify-between gap-4">
					<span>{{ formatMessage(messages.accentColorTitle) }}</span>
					<ButtonStyled>
						<label class="button-like relative" for="edit-theme-dark-accent-color">
							<PaletteIcon :style="{ color: settings.dark_brand_color }" />
							<input
								id="edit-theme-dark-accent-color"
								type="color"
								class="absolute inset-0 h-full w-full cursor-pointer opacity-0"
								:value="settings.dark_brand_color"
								@change="(e) => onColorInputChange('dark', 'accentColor', e)"
							/>
						</label>
					</ButtonStyled>
				</div>

				<div class="flex items-center justify-between gap-4">
					<span>{{ formatMessage(messages.colorThemeTitle) }}</span>
					<ButtonStyled>
						<label class="button-like relative" for="edit-theme-dark-color-theme">
							<PaletteIcon :style="{ color: settings.dark_color_theme }" />
							<input
								id="edit-theme-dark-color-theme"
								type="color"
								class="absolute inset-0 h-full w-full cursor-pointer opacity-0"
								:value="settings.dark_color_theme"
								@change="(e) => onColorInputChange('dark', 'colorTheme', e)"
							/>
						</label>
					</ButtonStyled>
				</div>

				<div class="flex items-center justify-between gap-4">
					<span>{{ formatMessage(messages.presetTitle) }}</span>
					<Combobox
						id="edit-theme-dark-preset"
						class="max-w-48"
						:model-value="settings.dark_active_theme_preset"
						name="Dark theme preset dropdown"
						:options="presetOptions"
						:display-value="presetDisplayValue(settings.dark_active_theme_preset)"
						@update:model-value="(id) => applyPreset('dark', id)"
					/>
				</div>
			</template>

			<div class="flex flex-col gap-2.5">
				<h2 class="m-0 text-lg font-semibold text-contrast">
					{{ formatMessage(messages.themeFolderTitle) }}
				</h2>
				<StyledInput
					id="edit-theme-folder"
					v-model="settings.theme_dir"
					type="text"
					wrapper-class="w-full"
					:placeholder="formatMessage(messages.themeFolderPlaceholder)"
				>
					<template #right>
						<ButtonStyled circular>
							<button class="ml-1.5" @click="browseThemeDir">
								<FolderSearchIcon />
							</button>
						</ButtonStyled>
					</template>
				</StyledInput>
				<p class="m-0 leading-tight text-secondary">
					{{ formatMessage(messages.themeFolderDescription) }}
				</p>
			</div>

			<ButtonStyled>
				<button @click="handleUploadTheme">
					<UploadIcon />
					{{ formatMessage(messages.uploadThemeButton) }}
				</button>
			</ButtonStyled>
		</div>
	</NewModal>
</template>
