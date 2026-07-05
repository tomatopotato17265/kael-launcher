<script setup lang="ts">
import { LeftArrowIcon, RightArrowIcon } from '@kael/assets'
import { ButtonStyled, defineMessages, useVIntl } from '@kael/ui'

withDefaults(
	defineProps<{
		showBack?: boolean
		showSkip?: boolean
		nextDisabled?: boolean
	}>(),
	{
		showBack: true,
		showSkip: true,
		nextDisabled: false,
	},
)

const emit = defineEmits<{
	back: []
	next: []
	skip: []
}>()

const { formatMessage } = useVIntl()

const messages = defineMessages({
	skip: {
		id: 'app.onboarding.frame.skip',
		defaultMessage: 'Skip',
	},
	back: {
		id: 'app.onboarding.frame.back',
		defaultMessage: 'Back',
	},
	next: {
		id: 'app.onboarding.frame.next',
		defaultMessage: 'Next',
	},
})
</script>

<template>
	<div class="flex h-full w-full flex-col">
		<div class="flex justify-end p-4">
			<ButtonStyled v-if="showSkip" type="transparent">
				<button @click="emit('skip')">
					{{ formatMessage(messages.skip) }}
					<RightArrowIcon aria-hidden="true" />
				</button>
			</ButtonStyled>
		</div>
		<div class="flex min-h-0 flex-1 flex-col items-center overflow-y-auto px-6">
			<div class="flex w-full max-w-3xl flex-col items-center gap-6 py-6">
				<slot name="header" />
				<slot />
			</div>
		</div>
		<div class="mx-auto flex w-full max-w-3xl items-center justify-between p-6">
			<ButtonStyled v-if="showBack">
				<button @click="emit('back')">
					<LeftArrowIcon aria-hidden="true" />
					{{ formatMessage(messages.back) }}
				</button>
			</ButtonStyled>
			<slot v-else name="footer-start">
				<div></div>
			</slot>
			<slot name="footer-end">
				<ButtonStyled color="brand">
					<button :disabled="nextDisabled" @click="emit('next')">
						{{ formatMessage(messages.next) }}
						<RightArrowIcon aria-hidden="true" />
					</button>
				</ButtonStyled>
			</slot>
		</div>
	</div>
</template>
