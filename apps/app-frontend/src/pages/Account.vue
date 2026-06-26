<script setup lang="ts">
import {
	CoffeeIcon,
	GameIcon,
	GaugeIcon,
	LanguagesIcon,
	ModrinthIcon,
	PaintbrushIcon,
	ShieldIcon,
	ToggleRightIcon,
	HeartIcon,
} from '@kael/assets'
import {
	commonMessages,
	commonSettingsMessages,
	defineMessage,
	defineMessages,
	ProgressBar,
	useVIntl,
} from '@kael/ui'
import { getVersion } from '@tauri-apps/api/app'
import { platform as getOsPlatform, version as getOsVersion } from '@tauri-apps/plugin-os'
import { computed, ref, watch } from 'vue'
import { useRoute } from 'vue-router'

import AppearanceSettings from '@/components/ui/settings/AppearanceSettings.vue'
import DefaultInstanceSettings from '@/components/ui/settings/DefaultInstanceSettings.vue'
import FeatureFlagSettings from '@/components/ui/settings/FeatureFlagSettings.vue'
import JavaSettings from '@/components/ui/settings/JavaSettings.vue'
import LanguageSettings from '@/components/ui/settings/LanguageSettings.vue'
import PrivacySettings from '@/components/ui/settings/PrivacySettings.vue'
import ResourceManagementSettings from '@/components/ui/settings/ResourceManagementSettings.vue'
import CreditsSettings from '@/components/ui/settings/CreditsSettings.vue'
import { get, set } from '@/helpers/settings.ts'
import { injectAppUpdateDownloadProgress } from '@/providers/download-progress.ts'
import { useTheming } from '@/store/state'
import { useBreadcrumbs } from '@/store/breadcrumbs'

const themeStore = useTheming()
const { formatMessage } = useVIntl()
const route = useRoute()
const breadcrumbs = useBreadcrumbs()

breadcrumbs.setRootContext({ name: 'Account', link: route.path })

const tabs = [
	{
		name: defineMessage({
			id: 'app.settings.tabs.appearance',
			defaultMessage: 'Appearance',
		}),
		icon: PaintbrushIcon,
		content: AppearanceSettings,
	},
	{
		name: defineMessage({
			id: 'app.settings.tabs.language',
			defaultMessage: 'Language',
		}),
		icon: LanguagesIcon,
		content: LanguageSettings,
		badge: commonMessages.beta,
	},
	{
		name: defineMessage({
			id: 'app.settings.tabs.privacy',
			defaultMessage: 'Privacy',
		}),
		icon: ShieldIcon,
		content: PrivacySettings,
	},
	{
		name: defineMessage({
			id: 'app.settings.tabs.java-installations',
			defaultMessage: 'Java installations',
		}),
		icon: CoffeeIcon,
		content: JavaSettings,
	},
	{
		name: defineMessage({
			id: 'app.settings.tabs.default-instance-options',
			defaultMessage: 'Default instance options',
		}),
		icon: GameIcon,
		content: DefaultInstanceSettings,
	},
	{
		name: defineMessage({
			id: 'app.settings.tabs.resource-management',
			defaultMessage: 'Resource management',
		}),
		icon: GaugeIcon,
		content: ResourceManagementSettings,
	},
	{
		name: defineMessage({
			id: 'app.settings.tabs.credits',
			defaultMessage: 'Credits',
		}),
		icon: HeartIcon,
		content: CreditsSettings,
	},
	{
		name: commonSettingsMessages.featureFlags,
		icon: ToggleRightIcon,
		content: FeatureFlagSettings,
		developerOnly: true,
	},
]

const selectedTab = ref(0)
const visibleTabs = computed(() => tabs.filter((t) => !t.developerOnly || themeStore.devMode))

const devModeCounter = ref(0)
const developerModeEnabled = defineMessage({
	id: 'app.settings.developer-mode-enabled',
	defaultMessage: 'Developer mode enabled.',
})

function devModeCount() {
	devModeCounter.value++
	if (devModeCounter.value > 5) {
		themeStore.devMode = !themeStore.devMode
		settings.value.developer_mode = !!themeStore.devMode
		devModeCounter.value = 0

		if (!themeStore.devMode && visibleTabs.value[selectedTab.value]?.developerOnly) {
			selectedTab.value = 0
		}
	}
}

const { progress, version: downloadingVersion } = injectAppUpdateDownloadProgress()

const version = await getVersion()
const osPlatform = getOsPlatform()
const osVersion = getOsVersion()
const settings = ref(await get())

watch(
	settings,
	async () => {
		await set(settings.value)
	},
	{ deep: true },
)

const messages = defineMessages({
	downloading: {
		id: 'app.settings.downloading',
		defaultMessage: 'Downloading v{version}',
	},
})
</script>

<template>
	<div class="flex">
		<nav class="sticky top-0 self-start flex flex-col gap-1 p-4 w-56 shrink-0 border-r border-divider overflow-y-auto max-h-[calc(100vh_-_var(--top-bar-height))]">
			<button
				v-for="(tab, i) in visibleTabs"
				:key="i"
				class="flex items-center gap-2 px-3 py-2 rounded-lg text-left transition-colors w-full bg-transparent border-none cursor-pointer"
				:class="selectedTab === i ? 'bg-brand text-white' : 'hover:bg-surface-3 text-primary'"
				@click="selectedTab = i"
			>
				<component :is="tab.icon" class="w-4 h-4 shrink-0" />
				<span>{{ formatMessage(tab.name) }}</span>
				<span
					v-if="tab.badge"
					class="ml-auto text-xs font-semibold px-1.5 py-0.5 rounded"
					:class="selectedTab === i ? 'bg-white/20 text-white' : 'bg-brand text-white'"
				>
					{{ formatMessage(tab.badge) }}
				</span>
			</button>

			<div class="mt-auto pt-4 text-secondary text-sm">
				<div class="mb-3">
					<template v-if="progress > 0 && progress < 1">
						<p class="m-0 mb-2">
							{{ formatMessage(messages.downloading, { version: downloadingVersion }) }}
						</p>
						<ProgressBar :progress="progress" />
					</template>
				</div>
				<p v-if="themeStore.devMode" class="text-brand font-semibold m-0 mb-2">
					{{ formatMessage(developerModeEnabled) }}
				</p>
				<div class="flex items-center gap-3">
					<button
						class="p-0 m-0 bg-transparent border-none cursor-pointer button-animation"
						:class="{
							'text-brand': themeStore.devMode,
							'text-secondary': !themeStore.devMode,
						}"
						@click="devModeCount"
					>
						<ModrinthIcon class="w-6 h-6" />
					</button>
					<div class="max-w-[160px]">
						<p class="m-0">Kael Launcher {{ version }}</p>
						<p class="m-0">
							<span v-if="osPlatform === 'macos'">macOS</span>
							<span v-else class="capitalize">{{ osPlatform }}</span>
							{{ osVersion }}
						</p>
					</div>
				</div>
			</div>
		</nav>

		<div class="flex-1 p-6">
			<Suspense>
				<component :is="visibleTabs[selectedTab]?.content" v-if="visibleTabs[selectedTab]?.content" />
			</Suspense>
		</div>
	</div>
</template>
