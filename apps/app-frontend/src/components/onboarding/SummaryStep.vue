<script setup lang="ts">
import { CheckIcon, PaletteIcon, RightArrowIcon, SpinnerIcon, XIcon } from '@kael/assets'
import { ButtonStyled, defineMessages, useVIntl } from '@kael/ui'
import { computed, onMounted, onUnmounted, ref } from 'vue'

import RecommendationCard from '@/components/onboarding/RecommendationCard.vue'
import { buildRecommendations } from '@/components/onboarding/recommendations.ts'
import type { OnboardingLoader } from '@/components/onboarding/types.ts'
import ProgressBar from '@/components/ui/ProgressBar.vue'
import { get_project, get_project_versions } from '@/helpers/cache.js'
import { loading_listener } from '@/helpers/events.js'
import { add_project_from_version, create, list } from '@/helpers/profile.ts'
import { get_game_versions } from '@/helpers/tags.js'
import type { InstanceLoader } from '@/helpers/types'
import { useOnboarding } from '@/store/onboarding.ts'

const onboarding = useOnboarding()
const { formatMessage } = useVIntl()

const messages = defineMessages({
	title: {
		id: 'app.onboarding.summary.title',
		defaultMessage: "You're all set",
	},
	subtitle: {
		id: 'app.onboarding.summary.subtitle',
		defaultMessage: "Here's what Kael is setting up based on your answers.",
	},
	loadersHeading: {
		id: 'app.onboarding.summary.loaders.heading',
		defaultMessage: 'Setting up your mod loaders',
	},
	loaderPending: {
		id: 'app.onboarding.summary.loaders.pending',
		defaultMessage: 'Waiting…',
	},
	loaderInstalling: {
		id: 'app.onboarding.summary.loaders.installing',
		defaultMessage: 'Installing…',
	},
	loaderDone: {
		id: 'app.onboarding.summary.loaders.done',
		defaultMessage: 'Installed',
	},
	loaderAlreadyInstalled: {
		id: 'app.onboarding.summary.loaders.already-installed',
		defaultMessage: 'Already installed',
	},
	loaderFailed: {
		id: 'app.onboarding.summary.loaders.failed',
		defaultMessage: "Couldn't install — you can retry from your library later",
	},
	recommendationsHeading: {
		id: 'app.onboarding.summary.recommendations.heading',
		defaultMessage: 'Recommended for you',
	},
	themeApplied: {
		id: 'app.onboarding.summary.theme-applied',
		defaultMessage: 'Your theme is applied — change it any time in Settings.',
	},
	continue: {
		id: 'app.onboarding.summary.continue',
		defaultMessage: 'Continue to Kael',
	},
})

const LOADER_NAMES: Record<string, string> = {
	fabric: 'Fabric',
	forge: 'Forge',
	neoforge: 'NeoForge',
	quilt: 'Quilt',
}

const LOADER_API_MODS: Partial<Record<string, string>> = {
	fabric: 'fabric-api',
	quilt: 'qsl',
}

type LoaderInstallStatus = 'pending' | 'installing' | 'done' | 'already-installed' | 'failed'

interface LoaderInstall {
	loader: InstanceLoader
	name: string
	status: LoaderInstallStatus
	progress: number
}

const selectedLoaders = onboarding.answers.loaders.filter(
	(loader): loader is Exclude<OnboardingLoader, 'none'> => loader !== 'none',
)

const loaderInstalls = ref<LoaderInstall[]>(
	selectedLoaders.map((loader) => ({
		loader,
		name: LOADER_NAMES[loader],
		status: 'pending',
		progress: 0,
	})),
)

interface RecommendationItem {
	project: { id: string; title: string; description: string; icon_url?: string | null }
	reason: string
}

const recommendationItems = ref<RecommendationItem[]>([])

const finishing = ref(false)

const statusMessage = computed(() => ({
	pending: messages.loaderPending,
	installing: messages.loaderInstalling,
	done: messages.loaderDone,
	'already-installed': messages.loaderAlreadyInstalled,
	failed: messages.loaderFailed,
}))

let unlistenLoading: (() => void) | undefined
let installingProfileName: string | null = null
let installingLoaderIndex = -1

async function installLoaders() {
	if (loaderInstalls.value.length === 0) return

	const gameVersions = await get_game_versions()
	const latest = gameVersions.find(
		(v: { version_type: string }) => v.version_type === 'release',
	)?.version
	if (!latest) {
		for (const item of loaderInstalls.value) {
			item.status = 'failed'
		}
		return
	}

	const existing = await list().catch(() => [])

	for (const [index, item] of loaderInstalls.value.entries()) {
		if (existing.some((i) => i.loader === item.loader && i.game_version === latest)) {
			item.status = 'already-installed'
			continue
		}

		item.status = 'installing'
		installingProfileName = `${item.name} ${latest}`
		installingLoaderIndex = index
		try {
			const path = await create(installingProfileName, latest, item.loader, 'stable', null, false)

			const apiModSlug = LOADER_API_MODS[item.loader]
			if (apiModSlug) {
				const versions = await get_project_versions(apiModSlug)
				const version = versions.find(
					(v: { game_versions: string[]; loaders: string[] }) =>
						v.game_versions.includes(latest) && v.loaders.includes(item.loader),
				)
				if (version) {
					await add_project_from_version(path, version.id, 'standalone').catch(() => {})
				}
			}

			item.status = 'done'
			item.progress = 100
		} catch {
			item.status = 'failed'
		} finally {
			installingProfileName = null
			installingLoaderIndex = -1
		}
	}
}

async function loadRecommendations() {
	const recommendations = buildRecommendations(onboarding.answers)
	const results = await Promise.allSettled(
		recommendations.map(async (recommendation) => ({
			project: await get_project(recommendation.slug),
			reason: formatMessage(recommendation.reasonMessage),
		})),
	)
	recommendationItems.value = results
		.filter((result) => result.status === 'fulfilled')
		.map((result) => result.value)
}

function dismissRecommendation(id: string) {
	recommendationItems.value = recommendationItems.value.filter((item) => item.project.id !== id)
}

async function finish() {
	if (finishing.value) return
	finishing.value = true
	try {
		await onboarding.finish(false)
	} finally {
		finishing.value = false
	}
}

onMounted(async () => {
	onboarding.persistAnswers().catch(() => {})

	unlistenLoading = await loading_listener(
		(payload: { event: { type: string; profile_name?: string }; fraction: number | null }) => {
			if (
				payload.event.type === 'minecraft_download' &&
				installingProfileName &&
				payload.event.profile_name === installingProfileName &&
				installingLoaderIndex >= 0
			) {
				loaderInstalls.value[installingLoaderIndex].progress = (payload.fraction ?? 1) * 100
			}
		},
	)

	loadRecommendations().catch(() => {})
	installLoaders().catch(() => {})
})

onUnmounted(() => {
	unlistenLoading?.()
})
</script>

<template>
	<div class="flex min-h-0 flex-1 flex-col items-center overflow-y-auto px-6">
		<div class="flex w-full max-w-3xl flex-col gap-6 py-6">
			<div class="flex flex-col items-center gap-2 text-center">
				<h1 class="m-0 text-3xl font-extrabold text-contrast">
					{{ formatMessage(messages.title) }}
				</h1>
				<p class="m-0 text-secondary">{{ formatMessage(messages.subtitle) }}</p>
			</div>

			<section v-if="loaderInstalls.length > 0" class="flex flex-col gap-3">
				<h2 class="m-0 text-lg font-semibold text-contrast">
					{{ formatMessage(messages.loadersHeading) }}
				</h2>
				<div
					v-for="item in loaderInstalls"
					:key="item.loader"
					class="flex items-center gap-3 rounded-xl bg-bg-raised p-4"
				>
					<SpinnerIcon
						v-if="item.status === 'installing' || item.status === 'pending'"
						class="h-5 w-5 shrink-0 animate-spin text-secondary"
						aria-hidden="true"
					/>
					<CheckIcon
						v-else-if="item.status === 'done' || item.status === 'already-installed'"
						class="h-5 w-5 shrink-0 text-brand"
						aria-hidden="true"
					/>
					<XIcon v-else class="h-5 w-5 shrink-0 text-red" aria-hidden="true" />
					<div class="flex min-w-0 flex-1 flex-col gap-1">
						<span class="font-semibold text-contrast">{{ item.name }}</span>
						<ProgressBar
							v-if="item.status === 'installing'"
							:progress="Math.min(item.progress, 100)"
						/>
						<span v-else class="text-sm text-secondary">
							{{ formatMessage(statusMessage[item.status]) }}
						</span>
					</div>
				</div>
			</section>

			<section v-if="recommendationItems.length > 0" class="flex flex-col gap-3">
				<h2 class="m-0 text-lg font-semibold text-contrast">
					{{ formatMessage(messages.recommendationsHeading) }}
				</h2>
				<RecommendationCard
					v-for="item in recommendationItems"
					:key="item.project.id"
					:project="item.project"
					:reason="item.reason"
					@dismiss="dismissRecommendation(item.project.id)"
				/>
			</section>

			<p class="m-0 flex items-center justify-center gap-2 text-center text-secondary">
				<PaletteIcon class="h-4 w-4 shrink-0" aria-hidden="true" />
				{{ formatMessage(messages.themeApplied) }}
			</p>

			<div class="flex justify-center pb-6">
				<ButtonStyled color="brand" size="large">
					<button :disabled="finishing" @click="finish">
						{{ formatMessage(messages.continue) }}
						<RightArrowIcon aria-hidden="true" />
					</button>
				</ButtonStyled>
			</div>
		</div>
	</div>
</template>
