import type { MessageDescriptor } from '@kael/ui'

export type OnboardingLoader = 'fabric' | 'forge' | 'neoforge' | 'quilt' | 'none'

export type OnboardingPlaystyle =
	| 'survival-building'
	| 'modded-adventures'
	| 'pvp'
	| 'speedrunning'
	| 'exploring'

export type OnboardingSocial = 'solo' | 'friends' | 'public-servers' | 'content-creation'

export type OnboardingPriority =
	| 'performance'
	| 'visuals'
	| 'simplicity'
	| 'customization'
	| 'stability'

export type OnboardingExperience = 'new' | 'basics' | 'experienced'

/**
 * Raw onboarding answers, persisted as-is into the `onboarding_answers`
 * settings column so other parts of the app can tune recommendations or UI
 * defaults from them later. Keys are snake_case to round-trip cleanly through
 * the Rust `serde_json::Value` field.
 */
export interface OnboardingAnswers {
	loaders: OnboardingLoader[]
	playstyles: OnboardingPlaystyle[]
	social: OnboardingSocial[]
	priorities: OnboardingPriority[]
	experience: OnboardingExperience | null
	/** Selected theme preset id, or 'custom-later' when deferred to Settings. */
	theme: string | null
	skipped: boolean
	completed_at: string
}

export type OnboardingQuestionId = 'loaders' | 'playstyles' | 'social' | 'priorities' | 'experience'

export type OnboardingStepId = OnboardingQuestionId | 'theme' | 'summary'

export interface QuestionOption {
	id: string
	labelMessage: MessageDescriptor
	/** Selecting this option clears all others in the question, and vice versa. */
	exclusive?: boolean
}

export interface QuestionDefinition {
	id: OnboardingQuestionId
	mode: 'multi' | 'single'
	titleMessage: MessageDescriptor
	options: QuestionOption[]
}
