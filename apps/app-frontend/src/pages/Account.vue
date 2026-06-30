<script setup lang="ts">
import { EditIcon } from '@kael/assets'
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
onUnmounted(() => {
	if (providedAccountsCard) providedAccountsCard.value = null
})

const currentUser = ref()
const capes = ref<Cape[]>([])
const equippedSkin = ref<Skin | null>(null)
const skinTexture = ref('')

const username = computed(() => currentUser.value?.profile?.name)
const capeTexture = computed(
	() => capes.value.find((cape) => cape.id === equippedSkin.value?.cape_id)?.texture,
)
const skinVariant = computed(() => equippedSkin.value?.variant)

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
})
</script>

<template>
	<div class="flex gap-8 box-border min-h-full p-6">
		<div class="w-72 shrink-0">
			<h3 class="text-base text-primary font-medium m-0">
				{{ formatMessage(messages.playingAs) }}
			</h3>
			<Suspense>
				<AccountsCard ref="accountsCardEl" @change="loadAccountSkin" />
			</Suspense>
		</div>

		<div class="flex-1 flex items-center justify-center min-h-[60vh]">
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
</template>
