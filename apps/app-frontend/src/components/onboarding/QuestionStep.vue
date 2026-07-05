<script setup lang="ts">
import { CheckIcon } from '@kael/assets'
import { useVIntl } from '@kael/ui'
import { computed } from 'vue'

import type { QuestionDefinition } from '@/components/onboarding/types.ts'

const props = defineProps<{
	question: QuestionDefinition
	modelValue: string[] | string | null
}>()

const emit = defineEmits<{
	'update:modelValue': [value: string[] | string | null]
}>()

const { formatMessage } = useVIntl()

const selected = computed<string[]>(() => {
	if (props.question.mode === 'single') {
		return props.modelValue ? [props.modelValue as string] : []
	}
	return (props.modelValue as string[]) ?? []
})

function isSelected(id: string) {
	return selected.value.includes(id)
}

function toggle(id: string) {
	if (props.question.mode === 'single') {
		emit('update:modelValue', props.modelValue === id ? null : id)
		return
	}

	const current = selected.value
	if (current.includes(id)) {
		emit(
			'update:modelValue',
			current.filter((x) => x !== id),
		)
		return
	}

	const option = props.question.options.find((x) => x.id === id)
	if (option?.exclusive) {
		emit('update:modelValue', [id])
		return
	}

	const exclusiveIds = props.question.options.filter((x) => x.exclusive).map((x) => x.id)
	emit('update:modelValue', [...current.filter((x) => !exclusiveIds.includes(x)), id])
}
</script>

<template>
	<fieldset class="m-0 w-full border-0 p-0">
		<legend class="sr-only">{{ formatMessage(question.titleMessage) }}</legend>
		<div class="grid w-full grid-cols-1 gap-3 sm:grid-cols-2">
			<button
				v-for="option in question.options"
				:key="option.id"
				class="flex cursor-pointer items-center justify-between gap-3 rounded-xl border-2 border-solid bg-bg-raised px-4 py-4 text-left text-base font-medium text-contrast transition-colors"
				:class="isSelected(option.id) ? 'border-brand' : 'border-transparent hover:border-divider'"
				:aria-pressed="isSelected(option.id)"
				@click="toggle(option.id)"
			>
				{{ formatMessage(option.labelMessage) }}
				<CheckIcon
					v-if="isSelected(option.id)"
					class="h-5 w-5 shrink-0 text-brand"
					aria-hidden="true"
				/>
			</button>
		</div>
	</fieldset>
</template>
