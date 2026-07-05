<script setup lang="ts">
import { DownloadIcon, XIcon } from '@kael/assets'
import { Avatar, ButtonStyled, commonMessages, defineMessages, useVIntl } from '@kael/ui'
import { ref } from 'vue'

import { injectContentInstall } from '@/providers/content-install'

const props = defineProps<{
	project: {
		id: string
		title: string
		description: string
		icon_url?: string | null
	}
	reason: string
}>()

const emit = defineEmits<{
	dismiss: []
}>()

const { formatMessage } = useVIntl()
const contentInstall = injectContentInstall()

const messages = defineMessages({
	dismiss: {
		id: 'app.onboarding.recommendations.dismiss',
		defaultMessage: 'Dismiss recommendation',
	},
})

const installing = ref(false)

async function install() {
	if (installing.value) return
	installing.value = true
	try {
		await contentInstall.install(props.project.id, null, null, 'onboarding')
	} finally {
		installing.value = false
	}
}
</script>

<template>
	<div class="flex items-center gap-3 rounded-xl bg-bg-raised p-4">
		<Avatar :src="project.icon_url" size="48px" />
		<div class="flex min-w-0 flex-1 flex-col gap-0.5">
			<span class="font-semibold text-contrast">{{ project.title }}</span>
			<span class="truncate text-sm text-secondary">{{ project.description }}</span>
			<span class="text-sm italic text-brand">{{ reason }}</span>
		</div>
		<ButtonStyled color="brand">
			<button :disabled="installing" @click="install">
				<DownloadIcon aria-hidden="true" />
				{{ formatMessage(commonMessages.installButton) }}
			</button>
		</ButtonStyled>
		<ButtonStyled type="transparent" circular>
			<button :aria-label="formatMessage(messages.dismiss)" @click="emit('dismiss')">
				<XIcon aria-hidden="true" />
			</button>
		</ButtonStyled>
	</div>
</template>
