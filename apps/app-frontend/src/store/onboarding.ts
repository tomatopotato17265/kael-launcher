import { defineStore } from 'pinia'

import type { OnboardingAnswers, OnboardingStepId } from '@/components/onboarding/types.ts'
import { get as getSettings, set as setSettings } from '@/helpers/settings.ts'

export const ONBOARDING_STEPS: OnboardingStepId[] = [
	'loaders',
	'playstyles',
	'social',
	'priorities',
	'experience',
	'theme',
	'summary',
]

function emptyAnswers(): OnboardingAnswers {
	return {
		loaders: [],
		playstyles: [],
		social: [],
		priorities: [],
		experience: null,
		theme: null,
		skipped: false,
		completed_at: '',
	}
}

export const useOnboarding = defineStore('onboardingStore', {
	state: () => ({
		active: false,
		stepIndex: 0,
		answers: emptyAnswers(),
	}),
	getters: {
		currentStep(): OnboardingStepId {
			return ONBOARDING_STEPS[this.stepIndex]
		},
		summaryIndex(): number {
			return ONBOARDING_STEPS.indexOf('summary')
		},
	},
	actions: {
		start() {
			this.answers = emptyAnswers()
			this.stepIndex = 0
			this.active = true
		},
		restart() {
			this.start()
		},
		next() {
			if (this.stepIndex < ONBOARDING_STEPS.length - 1) {
				this.stepIndex += 1
			}
		},
		back() {
			if (this.stepIndex > 0) {
				this.stepIndex -= 1
			}
		},
		skipToSummary() {
			this.stepIndex = this.summaryIndex
		},
		/**
		 * Saves the answers gathered so far without marking onboarding as
		 * complete, so they survive the app closing mid-summary.
		 */
		async persistAnswers() {
			const settings = await getSettings()
			settings.onboarding_answers = {
				...this.answers,
				completed_at: new Date().toISOString(),
			}
			await setSettings(settings)
		},
		async finish(skipped: boolean) {
			this.answers.skipped = skipped
			const settings = await getSettings()
			settings.onboarded = true
			settings.onboarding_answers = {
				...this.answers,
				completed_at: new Date().toISOString(),
			}
			await setSettings(settings)
			this.active = false
		},
	},
})
