<script setup lang="ts">
import { ButtonStyled, defineMessages, useVIntl } from '@kael/ui'
import { computed, ref } from 'vue'

import OnboardingStepFrame from '@/components/onboarding/OnboardingStepFrame.vue'
import { ONBOARDING_QUESTIONS } from '@/components/onboarding/questions.ts'
import QuestionStep from '@/components/onboarding/QuestionStep.vue'
import SummaryStep from '@/components/onboarding/SummaryStep.vue'
import ThemeStep from '@/components/onboarding/ThemeStep.vue'
import type { OnboardingQuestionId } from '@/components/onboarding/types.ts'
import { ONBOARDING_STEPS, useOnboarding } from '@/store/onboarding.ts'

const onboarding = useOnboarding()
const { formatMessage } = useVIntl()

const messages = defineMessages({
	welcomeTitle: {
		id: 'app.onboarding.welcome.title',
		defaultMessage: 'Welcome to Kael',
	},
	welcomeSubtitle: {
		id: 'app.onboarding.welcome.subtitle',
		defaultMessage: 'Answer a few quick questions to personalize your experience.',
	},
	skipEntirely: {
		id: 'app.onboarding.welcome.skip-entirely',
		defaultMessage: 'Skip setup for now',
	},
	themeTitle: {
		id: 'app.onboarding.theme.title',
		defaultMessage: 'Pick a theme',
	},
	themeSubtitle: {
		id: 'app.onboarding.theme.subtitle',
		defaultMessage: 'Changes preview instantly. You can fine-tune everything later in Settings.',
	},
})

const themeStep = ref<InstanceType<typeof ThemeStep>>()
const finishing = ref(false)

const currentQuestion = computed(() =>
	ONBOARDING_QUESTIONS.find((q) => q.id === onboarding.currentStep),
)

const dotSteps = ONBOARDING_STEPS.filter((step) => step !== 'summary')

const stepTitle = computed(() => {
	if (currentQuestion.value) {
		return formatMessage(currentQuestion.value.titleMessage)
	}
	if (onboarding.currentStep === 'theme') {
		return formatMessage(messages.themeTitle)
	}
	return ''
})

function questionModel(id: OnboardingQuestionId) {
	if (id === 'experience') {
		return onboarding.answers.experience
	}
	return onboarding.answers[id]
}

function setQuestionModel(id: OnboardingQuestionId, value: string[] | string | null) {
	if (id === 'experience') {
		onboarding.answers.experience = value as typeof onboarding.answers.experience
	} else {
		onboarding.answers[id] = (value ?? []) as never
	}
}

async function skipEntirely() {
	if (finishing.value) return
	finishing.value = true
	try {
		await onboarding.finish(true)
	} finally {
		finishing.value = false
	}
}

async function handleNext() {
	if (onboarding.currentStep === 'theme') {
		await themeStep.value?.confirm()
	}
	onboarding.next()
}

function handleBack() {
	if (onboarding.currentStep === 'theme') {
		themeStep.value?.revertPreview()
	}
	onboarding.back()
}

function handleSkip() {
	if (onboarding.currentStep === 'theme') {
		themeStep.value?.revertPreview()
	}
	onboarding.skipToSummary()
}
</script>

<template>
	<div class="fixed inset-0 z-[150] flex flex-col bg-bg">
		<div data-tauri-drag-region class="h-8 w-full shrink-0"></div>
		<Transition name="onboarding-fade" mode="out-in">
			<SummaryStep v-if="onboarding.currentStep === 'summary'" />
			<OnboardingStepFrame
				v-else
				:key="onboarding.currentStep"
				:show-back="onboarding.stepIndex > 0"
				@back="handleBack"
				@next="handleNext"
				@skip="handleSkip"
			>
				<template #header>
					<div
						v-if="onboarding.stepIndex === 0"
						class="flex flex-col items-center gap-2 text-center"
					>
						<h1 class="m-0 text-3xl font-extrabold text-contrast">
							{{ formatMessage(messages.welcomeTitle) }}
						</h1>
						<p class="m-0 text-secondary">{{ formatMessage(messages.welcomeSubtitle) }}</p>
					</div>
					<div class="flex items-center gap-2" aria-hidden="true">
						<span
							v-for="(step, index) in dotSteps"
							:key="step"
							class="h-2 w-2 rounded-full transition-colors"
							:class="index <= onboarding.stepIndex ? 'bg-brand' : 'bg-button-bg'"
						></span>
					</div>
					<h2 class="m-0 text-center text-2xl font-bold text-contrast">{{ stepTitle }}</h2>
					<p v-if="onboarding.currentStep === 'theme'" class="m-0 text-center text-secondary">
						{{ formatMessage(messages.themeSubtitle) }}
					</p>
				</template>

				<QuestionStep
					v-if="currentQuestion"
					:question="currentQuestion"
					:model-value="questionModel(currentQuestion.id)"
					@update:model-value="(value) => setQuestionModel(currentQuestion!.id, value)"
				/>
				<ThemeStep v-else-if="onboarding.currentStep === 'theme'" ref="themeStep" />

				<template v-if="onboarding.stepIndex === 0" #footer-start>
					<ButtonStyled type="transparent">
						<button :disabled="finishing" @click="skipEntirely">
							{{ formatMessage(messages.skipEntirely) }}
						</button>
					</ButtonStyled>
				</template>
			</OnboardingStepFrame>
		</Transition>
	</div>
</template>

<style scoped>
.onboarding-fade-enter-active,
.onboarding-fade-leave-active {
	transition: opacity 0.15s ease-in-out;
}

.onboarding-fade-enter-from,
.onboarding-fade-leave-to {
	opacity: 0;
}
</style>
