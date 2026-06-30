<script setup lang="ts">
import { EditIcon } from '@kael/assets'
import {
	defineMessages,
	injectNotificationManager,
	SkinPreviewRenderer,
	useVIntl,
} from '@kael/ui'
import dayjs from 'dayjs'
import { computed, inject, onMounted, onUnmounted, ref, useTemplateRef, watch } from 'vue'
import type { Ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'

import AccountsCard from '@/components/ui/AccountsCard.vue'
import { get_default_user, users } from '@/helpers/auth'
import type { Cape, Skin } from '@/helpers/skins.ts'
import {
	get_available_capes,
	get_available_skins,
	get_normalized_skin_texture,
} from '@/helpers/skins.ts'
import { useBreadcrumbs } from '@/store/breadcrumbs'

const { formatMessage } = useVIntl()
const { handleError } = injectNotificationManager()
const route = useRoute()
const router = useRouter()
const breadcrumbs = useBreadcrumbs()

breadcrumbs.setRootContext({ name: 'Account', link: route.path })

const accountsCardEl = useTemplateRef<InstanceType<typeof AccountsCard>>('accountsCardEl')

const providedAccountsCard = inject('accountsCard') as
	| Ref<InstanceType<typeof AccountsCard> | null>
	| undefined
watch(
	accountsCardEl,
	(instance) => {
		if (providedAccountsCard) providedAccountsCard.value = instance ?? null
	},
	{ immediate: true },
)

const currentUser = ref()
const capes = ref<Cape[]>([])
const equippedSkin = ref<Skin | null>(null)
const skinTexture = ref('')

const username = computed(() => currentUser.value?.profile?.name)
const capeTexture = computed(
	() => capes.value.find((cape) => cape.id === equippedSkin.value?.cape_id)?.texture,
)
const skinVariant = computed(() => equippedSkin.value?.variant)

const uuid = computed(() => currentUser.value?.profile?.id)
const authToken = computed(() => currentUser.value?.access_token)
const namemcUrl = computed(() =>
	uuid.value ? `https://namemc.com/profile/${uuid.value}` : undefined,
)

const AGE_UNITS: { unit: dayjs.ManipulateType; label: string }[] = [
	{ unit: 'year', label: 'years' },
	{ unit: 'month', label: 'months' },
	{ unit: 'week', label: 'weeks' },
	{ unit: 'day', label: 'days' },
	{ unit: 'hour', label: 'hours' },
	{ unit: 'minute', label: 'minutes' },
	{ unit: 'second', label: 'seconds' },
]

const now = ref(Date.now())
let ageInterval: number | undefined

const age = computed(() => {
	const loggedIn = currentUser.value?.logged_in
	if (!loggedIn) return undefined

	let start = dayjs(loggedIn)
	const end = dayjs(now.value)
	if (!start.isValid() || end.isBefore(start)) return undefined

	return AGE_UNITS.map(({ unit, label }) => {
		const value = end.diff(start, unit)
		start = start.add(value, unit)
		return `${value} ${label}`
	}).join(', ')
})

onMounted(() => {
	ageInterval = window.setInterval(() => {
		now.value = Date.now()
	}, 1000)
})

onUnmounted(() => {
	if (providedAccountsCard) providedAccountsCard.value = null
	if (ageInterval !== undefined) window.clearInterval(ageInterval)
})

async function loadSkinTexture(skin: Skin) {
	try {
		return await get_normalized_skin_texture(skin)
	} catch (error) {
		if (skin.texture.startsWith('data:image/')) {
			return skin.texture
		}

		handleError(error as Error)
		return ''
	}
}

async function loadAccountSkin() {
	const defaultId = await get_default_user().catch(handleError)
	const allAccounts = (await users().catch(handleError)) ?? []
	currentUser.value = allAccounts.find((account) => account.profile.id === defaultId)

	capes.value = (await get_available_capes().catch(handleError)) ?? []

	const skins = (await get_available_skins().catch(handleError)) ?? []
	equippedSkin.value = skins.find((skin) => skin.is_equipped) ?? null
	skinTexture.value = equippedSkin.value ? await loadSkinTexture(equippedSkin.value) : ''
}

await loadAccountSkin()

const messages = defineMessages({
	playingAs: {
		id: 'app.account.playing-as',
		defaultMessage: 'Playing as',
	},
	editSkinButton: {
		id: 'app.account.edit-skin-button',
		defaultMessage: 'Edit in Skin Selector',
	},
	ageTitle: {
		id: 'app.account.stats.age.title',
		defaultMessage: 'Age',
	},
	ageDescription: {
		id: 'app.account.stats.age.description',
		defaultMessage: 'How long this account has been logged in.',
	},
	uuidTitle: {
		id: 'app.account.stats.uuid.title',
		defaultMessage: 'UUID',
	},
	uuidDescription: {
		id: 'app.account.stats.uuid.description',
		defaultMessage: 'The UUID of this account.',
	},
	namemcTitle: {
		id: 'app.account.stats.namemc.title',
		defaultMessage: 'NameMC',
	},
	namemcDescription: {
		id: 'app.account.stats.namemc.description',
		defaultMessage: 'The NameMC page of this account.',
	},
	authTokenTitle: {
		id: 'app.account.stats.auth-token.title',
		defaultMessage: 'Auth Token',
	},
	authTokenDescription: {
		id: 'app.account.stats.auth-token.description',
		defaultMessage: "This account's authentication token.",
	},
})
</script>

<template>
	<div class="flex gap-8 box-border min-h-full p-6">
		<div class="flex-1 min-w-0 flex flex-col gap-6">
			<div class="w-72 shrink-0">
				<h3 class="text-base text-primary font-medium m-0">
					{{ formatMessage(messages.playingAs) }}
				</h3>
				<Suspense>
					<AccountsCard ref="accountsCardEl" @change="loadAccountSkin" />
				</Suspense>
			</div>

			<div v-if="currentUser">
				<div class="flex items-center justify-between gap-4">
					<div>
						<h2 class="m-0 text-lg font-semibold text-contrast">
							{{ formatMessage(messages.ageTitle) }}
						</h2>
						<p class="m-0 mt-1">{{ formatMessage(messages.ageDescription) }}</p>
					</div>
					<span class="text-right text-primary">{{ age }}</span>
				</div>

				<div class="mt-6 flex items-center justify-between gap-4">
					<div>
						<h2 class="m-0 text-lg font-semibold text-contrast">
							{{ formatMessage(messages.uuidTitle) }}
						</h2>
						<p class="m-0 mt-1">{{ formatMessage(messages.uuidDescription) }}</p>
					</div>
					<span class="break-all text-right font-mono text-sm text-primary">{{ uuid }}</span>
				</div>

				<div class="mt-6 flex items-center justify-between gap-4">
					<div>
						<h2 class="m-0 text-lg font-semibold text-contrast">
							{{ formatMessage(messages.namemcTitle) }}
						</h2>
						<p class="m-0 mt-1">{{ formatMessage(messages.namemcDescription) }}</p>
					</div>
					<a
						:href="namemcUrl"
						target="_blank"
						rel="noopener noreferrer"
						class="break-all text-right text-brand hover:underline"
					>
						{{ namemcUrl }}
					</a>
				</div>

				<div class="mt-6 flex items-center justify-between gap-4">
					<div>
						<h2 class="m-0 text-lg font-semibold text-contrast">
							{{ formatMessage(messages.authTokenTitle) }}
						</h2>
						<p class="m-0 mt-1">{{ formatMessage(messages.authTokenDescription) }}</p>
					</div>
					<span class="max-w-sm break-all text-right font-mono text-xs text-primary">{{
						authToken
					}}</span>
				</div>
			</div>
		</div>

		<div class="shrink-0 flex items-center min-h-[60vh]">
			<div class="flex h-[80vh] w-96 items-center justify-center max-[700px]:h-[50vh]">
				<SkinPreviewRenderer
					:cape-src="capeTexture"
					:texture-src="skinTexture || ''"
					:variant="skinVariant"
					:nametag="username"
					:initial-rotation="Math.PI / 8"
				>
					<template #subtitle>
						<button
							class="flex h-10 min-w-0 cursor-pointer items-center justify-center gap-2 rounded-[14px] border-0 bg-surface-4 px-4 py-2.5 text-base font-semibold leading-5 shadow-md transition-[filter,transform] duration-200 enabled:hover:brightness-[--hover-brightness] enabled:focus-visible:brightness-[--hover-brightness] enabled:active:scale-95 disabled:cursor-not-allowed disabled:opacity-50 [&>svg]:size-5 [&>svg]:shrink-0"
							@click="router.push('/skins')"
						>
							<EditIcon />
							{{ formatMessage(messages.editSkinButton) }}
						</button>
					</template>
				</SkinPreviewRenderer>
			</div>
		</div>
	</div>
</template>
