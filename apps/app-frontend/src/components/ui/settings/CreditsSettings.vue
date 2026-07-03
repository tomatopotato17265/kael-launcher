<script setup lang="ts">
import { ExternalIcon, PaletteIcon } from '@kael/assets'
import { ButtonStyled, Combobox, defineMessages, ThemeSelector, Toggle, useVIntl } from '@kael/ui'
import { ref, watch } from 'vue'

import { get, set } from '@/helpers/settings.ts'
import { getOS } from '@/helpers/utils'
import { useTheming } from '@/store/state'
import type { ColorTheme, FeatureFlag } from '@/store/theme.ts'

const themeStore = useTheming()
const { formatMessage } = useVIntl()

const worldsInHomeFlag: FeatureFlag = 'worlds_in_home'
const skipUnknownPackWarningFlag: FeatureFlag = 'skip_unknown_pack_warning'
const showPlayTimeFlag: FeatureFlag = 'show_instance_play_time'

const messages = defineMessages({
	skinEditorTitle: {
		id: 'app.credits-settings.skin-editor.title',
		defaultMessage: 'Skin Editor',
	},
	skinEditorDescription: {
		id: 'app.credits-settings.skin-editor.description',
		defaultMessage: 'The in-app Minecraft skin editor, forked from Blockbench.',
	},
	blockbenchLink: {
		id: 'app.credits-settings.skin-editor.blockbench-link',
		defaultMessage: 'Blockbench',
	},
	editColorTitle: {
		id: 'app.appearance-settings.brand-color.title',
		defaultMessage: 'Edit theme',
	},
	editColorDescription: {
		id: 'app.appearance-settings.brand-color.description',
		defaultMessage: 'Customize the accent color used throughout the app.',
	},
	advancedRenderingTitle: {
		id: 'app.appearance-settings.advanced-rendering.title',
		defaultMessage: 'Advanced rendering',
	},
	advancedRenderingDescription: {
		id: 'app.appearance-settings.advanced-rendering.description',
		defaultMessage:
			'Enables advanced rendering such as blur effects that may cause performance issues without hardware-accelerated rendering.',
	},
	hideNametagTitle: {
		id: 'app.appearance-settings.hide-nametag.title',
		defaultMessage: 'Hide nametag',
	},
	hideNametagDescription: {
		id: 'app.appearance-settings.hide-nametag.description',
		defaultMessage: 'Disables the nametag above your player on the skins page.',
	},
	nativeDecorationsTitle: {
		id: 'app.appearance-settings.native-decorations.title',
		defaultMessage: 'Native decorations',
	},
	nativeDecorationsDescription: {
		id: 'app.appearance-settings.native-decorations.description',
		defaultMessage: 'Use system window frame (app restart required).',
	},
	minimizeLauncherTitle: {
		id: 'app.appearance-settings.minimize-launcher.title',
		defaultMessage: 'Minimize launcher',
	},
	minimizeLauncherDescription: {
		id: 'app.appearance-settings.minimize-launcher.description',
		defaultMessage: 'Minimize the launcher when a Minecraft process starts.',
	},
	defaultLandingPageTitle: {
		id: 'app.appearance-settings.default-landing-page.title',
		defaultMessage: 'Default landing page',
	},
	defaultLandingPageDescription: {
		id: 'app.appearance-settings.default-landing-page.description',
		defaultMessage: 'Change the page to which the launcher opens on.',
	},
	defaultLandingPageHome: {
		id: 'app.appearance-settings.default-landing-page.home',
		defaultMessage: 'Home',
	},
	defaultLandingPageLibrary: {
		id: 'app.appearance-settings.default-landing-page.library',
		defaultMessage: 'Library',
	},
	selectOption: {
		id: 'app.appearance-settings.select-option',
		defaultMessage: 'Select an option',
	},
	jumpBackIntoWorldsTitle: {
		id: 'app.appearance-settings.jump-back-into-worlds.title',
		defaultMessage: 'Jump back into worlds',
	},
	jumpBackIntoWorldsDescription: {
		id: 'app.appearance-settings.jump-back-into-worlds.description',
		defaultMessage: 'Includes recent worlds in the "Jump back in" section on the Home page.',
	},
	unknownPackWarningTitle: {
		id: 'app.appearance-settings.unknown-pack-warning.title',
		defaultMessage: 'Warn me before installing unknown modpacks',
	},
	unknownPackWarningDescription: {
		id: 'app.appearance-settings.unknown-pack-warning.description',
		defaultMessage:
			"If you attempt to install a Modrinth Pack file (.mrpack) that isn't hosted on Modrinth, we'll make sure you understand the risks before installing it.",
	},
	showPlayTimeTitle: {
		id: 'app.appearance-settings.show-play-time.title',
		defaultMessage: 'Show play time',
	},
	showPlayTimeDescription: {
		id: 'app.appearance-settings.show-play-time.description',
		defaultMessage: `Displays how much time you've spent playing an instance.`,
	},
})

const os = ref(await getOS())
const settings = ref(await get())
const colorInputRef = ref<HTMLInputElement | null>(null)

watch(
	settings,
	async () => {
		await set(settings.value)
	},
	{ deep: true },
)
</script>
<template>
	<div>
		<div class="mt-6 flex items-center justify-between">
			<div>
				<h2 class="m-0 text-lg font-semibold text-contrast">
					{{ formatMessage(messages.skinEditorTitle) }}
				</h2>
				<p class="m-0 mt-1">{{ formatMessage(messages.skinEditorDescription) }}</p>
			</div>
			<a
				href="https://www.blockbench.net"
				target="_blank"
				rel="noopener noreferrer"
				class="flex shrink-0 items-center gap-1.5 text-brand hover:underline [&>svg]:size-4"
			>
				{{ formatMessage(messages.blockbenchLink) }}
				<ExternalIcon />
			</a>
		</div>
	</div>
</template>
