<script setup lang="ts">
import { injectNotificationManager } from '@kael/ui'
import type { SearchResult } from '@kael/utils'
import dayjs from 'dayjs'
import { computed, onUnmounted, ref } from 'vue'
import { useRoute } from 'vue-router'

import RowDisplay from '@/components/RowDisplay.vue'
import RecentWorldsList from '@/components/ui/world/RecentWorldsList.vue'
import { get_default_user, users } from '@/helpers/auth.js'
import { get_search_results } from '@/helpers/cache.js'
import { profile_listener } from '@/helpers/events'
import { list } from '@/helpers/profile.js'
import type { GameInstance } from '@/helpers/types'
import { useBreadcrumbs } from '@/store/breadcrumbs'

const { handleError } = injectNotificationManager()
const route = useRoute()
const breadcrumbs = useBreadcrumbs()

breadcrumbs.setRootContext({ name: 'Home', link: route.path })

const instances = ref<GameInstance[]>([])

const currentUsername = ref<string | null>(null)
const defaultUserId = await get_default_user().catch(() => null)
if (defaultUserId) {
	const allUsers = await users().catch(() => [])
	const match = allUsers.find((u) => u.profile.id === defaultUserId)
	currentUsername.value = match?.profile?.name ?? null
}

const now = ref(new Date())
const nowInterval = setInterval(() => {
	now.value = new Date()
}, 30_000)

const greeting = computed(() => {
	const h = now.value.getHours()
	const m = now.value.getMinutes()
	if (h === 3 && m === 14) return 'Go to sleep bleh.'
	const name = currentUsername.value
	let period: string
	if (h >= 4 && h < 12) period = 'Good morning'
	else if (h >= 12 && h < 17) period = 'Good afternoon'
	else period = 'Good evening'
	return name ? `${period}, ${name}.` : `${period}.`
})

const featuredModpacks = ref<SearchResult[]>([])
const featuredMods = ref<SearchResult[]>([])
const installedModpacksFilter = ref('')

const recentInstances = computed(() =>
	instances.value
		.filter((x) => x.last_played)
		.slice()
		.sort((a, b) => dayjs(b.last_played).diff(dayjs(a.last_played))),
)

const hasFeaturedProjects = computed(
	() => (featuredModpacks.value?.length ?? 0) + (featuredMods.value?.length ?? 0) > 0,
)

const offline = ref<boolean>(!navigator.onLine)
window.addEventListener('offline', () => {
	offline.value = true
})
window.addEventListener('online', () => {
	offline.value = false
})

async function fetchInstances() {
	instances.value = await list().catch(handleError)

	const filters = []
	for (const instance of instances.value) {
		if (instance.linked_data && instance.linked_data.project_id) {
			filters.push(`NOT"project_id"="${instance.linked_data.project_id}"`)
		}
	}
	installedModpacksFilter.value = filters.join(' AND ')
}

async function fetchFeaturedModpacks() {
	const response = await get_search_results(
		`?facets=[["project_type:modpack"]]&limit=10&index=follows&filters=${installedModpacksFilter.value}`,
	)

	if (response) {
		featuredModpacks.value = response.result.hits
	} else {
		featuredModpacks.value = []
	}
}

async function fetchFeaturedMods() {
	const response = await get_search_results('?facets=[["project_type:mod"]]&limit=10&index=follows')

	if (response) {
		featuredMods.value = response.result.hits
	} else {
		featuredModpacks.value = []
	}
}

async function refreshFeaturedProjects() {
	await Promise.all([fetchFeaturedModpacks(), fetchFeaturedMods()])
}

await fetchInstances()
await refreshFeaturedProjects()

const unlistenProfile = await profile_listener(
	async (e: { event: string; profile_path_id: string }) => {
		await fetchInstances()

		if (e.event === 'added' || e.event === 'created' || e.event === 'removed') {
			await refreshFeaturedProjects()
		}
	},
)

onUnmounted(() => {
	unlistenProfile()
	clearInterval(nowInterval)
})
</script>

<template>
	<div class="p-6 flex flex-col gap-2">
		<h1 class="m-0 text-2xl font-extrabold">{{ greeting }}</h1>
		<RecentWorldsList :recent-instances="recentInstances" />
		<RowDisplay
			v-if="hasFeaturedProjects"
			:instances="[
				{
					label: 'Discover a modpack',
					route: '/browse/modpack',
					instances: featuredModpacks,
					downloaded: false,
				},
				{
					label: 'Discover mods',
					route: '/browse/mod',
					instances: featuredMods,
					downloaded: false,
				},
			]"
			:can-paginate="true"
		/>
	</div>
</template>
