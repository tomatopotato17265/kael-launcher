<script setup lang="ts">
import { PlusIcon } from '@kael/assets'
import { ButtonStyled, injectNotificationManager } from '@kael/ui'
import dayjs from 'dayjs'
import { computed, inject, onUnmounted, ref } from 'vue'
import { useRoute } from 'vue-router'

import GridDisplay from '@/components/GridDisplay.vue'
import RecentWorldsList from '@/components/ui/world/RecentWorldsList.vue'
import { get_default_user, users } from '@/helpers/auth.js'
import { profile_listener } from '@/helpers/events'
import { list } from '@/helpers/profile.js'
import type { GameInstance } from '@/helpers/types'
import { useBreadcrumbs } from '@/store/breadcrumbs'

const { handleError } = injectNotificationManager()
const showCreationModal = inject('showCreationModal')
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

const recentInstances = computed(() =>
	instances.value
		.filter((x) => x.last_played)
		.slice()
		.sort((a, b) => dayjs(b.last_played).diff(dayjs(a.last_played))),
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
}

await fetchInstances()

const unlistenProfile = await profile_listener(async () => {
	await fetchInstances()
})

onUnmounted(() => {
	unlistenProfile()
	clearInterval(nowInterval)
})
</script>

<template>
	<div class="p-6 flex flex-col gap-2">
		<h1 class="m-0 text-2xl font-extrabold">{{ greeting }}</h1>
		<div class="flex items-center">
			<ButtonStyled color="brand">
				<button :disabled="offline" @click="showCreationModal?.()">
					<PlusIcon />
					Create Instance
				</button>
			</ButtonStyled>
		</div>
		<RecentWorldsList :recent-instances="recentInstances" />
		<GridDisplay v-if="instances.length > 0" label="Instances" :instances="instances" />
	</div>
</template>
