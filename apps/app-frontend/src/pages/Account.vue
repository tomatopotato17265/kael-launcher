<script setup lang="ts">
import { CopyIcon, EditIcon, ExternalIcon, RefreshCwIcon } from '@kael/assets'
import {
	defineMessages,
	injectNotificationManager,
	SkinPreviewRenderer,
	useVIntl,
} from '@kael/ui'
import { computed, inject, onUnmounted, ref, useTemplateRef, watch } from 'vue'
import type { Ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'

import AccountsCard from '@/components/ui/AccountsCard.vue'
import { get_default_user, refresh_user, users } from '@/helpers/auth'
import type { Cape, Skin } from '@/helpers/skins.ts'
import {
	get_available_capes,
	get_available_skins,
	get_normalized_skin_texture,
} from '@/helpers/skins.ts'
import { useBreadcrumbs } from '@/store/breadcrumbs'

const { formatMessage } = useVIntl()
const { addNotification, handleError } = injectNotificationManager()
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
const refreshingToken = ref(false)

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

onUnmounted(() => {
	if (providedAccountsCard) providedAccountsCard.value = null
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

async function copyAuthToken() {
	if (!authToken.value) return

	try {
		await navigator.clipboard.writeText(authToken.value)
	} catch (error) {
		handleError(error as Error)
	}
}

async function refreshAuthToken() {
	if (refreshingToken.value) return

	refreshingToken.value = true

	try {
		const refreshed = await refresh_user()
		if (refreshed) currentUser.value = refreshed

		addNotification({
			title: formatMessage(messages.refreshAuthTokenSuccessTitle),
			text: formatMessage(messages.refreshAuthTokenSuccessText),
			type: 'success',
		})
	} catch (error) {
		handleError(error as Error)
	} finally {
		refreshingToken.value = false
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
		defaultMessage: "This account's authentication token. Don't share this with anyone!",
	},
	copyAuthTokenButton: {
		id: 'app.account.stats.auth-token.copy-button',
		defaultMessage: 'Copy auth token',
	},
	refreshAuthTokenButton: {
		id: 'app.account.stats.auth-token.refresh-button',
		defaultMessage: 'Refresh auth token',
	},
	refreshAuthTokenSuccessTitle: {
		id: 'app.account.stats.auth-token.refresh-success.title',
		defaultMessage: 'Auth token refreshed',
	},
	refreshAuthTokenSuccessText: {
		id: 'app.account.stats.auth-token.refresh-success.text',
		defaultMessage: 'Your authentication token was successfully refreshed.',
	},
})
</script>

<template>
	<div class="flex gap-8 box-border min-h-full p-6">
		<div class="flex-1 min-w-0 flex flex-col gap-6">
			<div class="w-96 shrink-0">
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
						class="flex shrink-0 items-center gap-1.5 text-brand hover:underline [&>svg]:size-4"
					>
						{{ formatMessage(messages.namemcTitle) }}
						<ExternalIcon />
					</a>
				</div>

				<div class="mt-6 flex items-center justify-between gap-4">
					<div>
						<h2 class="m-0 text-lg font-semibold text-contrast">
							{{ formatMessage(messages.authTokenTitle) }}
						</h2>
						<p class="m-0 mt-1">{{ formatMessage(messages.authTokenDescription) }}</p>
					</div>
					<div class="flex shrink-0 items-center gap-2">
						<button
							v-tooltip.left="formatMessage(messages.refreshAuthTokenButton)"
							:aria-label="formatMessage(messages.refreshAuthTokenButton)"
							:disabled="refreshingToken"
							class="flex size-9 shrink-0 cursor-pointer items-center justify-center rounded-xl border-0 bg-surface-4 text-primary shadow-md transition-[filter,transform] duration-200 hover:brightness-[--hover-brightness] focus-visible:brightness-[--hover-brightness] active:scale-95 disabled:cursor-not-allowed disabled:opacity-50 [&>svg]:size-4"
							@click="refreshAuthToken"
						>
							<RefreshCwIcon :class="{ 'animate-spin': refreshingToken }" />
						</button>
						<button
							v-tooltip.left="formatMessage(messages.copyAuthTokenButton)"
							:aria-label="formatMessage(messages.copyAuthTokenButton)"
							class="flex size-9 shrink-0 cursor-pointer items-center justify-center rounded-xl border-0 bg-surface-4 text-primary shadow-md transition-[filter,transform] duration-200 hover:brightness-[--hover-brightness] focus-visible:brightness-[--hover-brightness] active:scale-95 [&>svg]:size-4"
							@click="copyAuthToken"
						>
							<CopyIcon />
						</button>
					</div>
				</div>
			</div>
		</div>

		<div class="shrink-0 flex items-center self-start sticky top-6 min-h-[60vh]">
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
