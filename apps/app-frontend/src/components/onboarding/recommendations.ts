import type { MessageDescriptor } from '@kael/ui'
import { defineMessages } from '@kael/ui'

import type { OnboardingAnswers } from '@/components/onboarding/types.ts'

const MAX_RECOMMENDATIONS = 6

const reasonMessages = defineMessages({
	performance: {
		id: 'app.onboarding.recommendations.reason.performance',
		defaultMessage: 'Because performance matters to you',
	},
	visuals: {
		id: 'app.onboarding.recommendations.reason.visuals',
		defaultMessage: 'For stunning visuals and shaders',
	},
	simplicity: {
		id: 'app.onboarding.recommendations.reason.simplicity',
		defaultMessage: 'An easy all-in-one setup',
	},
	customization: {
		id: 'app.onboarding.recommendations.reason.customization',
		defaultMessage: 'For making the game your own',
	},
	stability: {
		id: 'app.onboarding.recommendations.reason.stability',
		defaultMessage: 'For a smoother, crash-free game',
	},
	survivalBuilding: {
		id: 'app.onboarding.recommendations.reason.survival-building',
		defaultMessage: 'Great for survival & building',
	},
	moddedAdventures: {
		id: 'app.onboarding.recommendations.reason.modded-adventures',
		defaultMessage: 'For your modded adventures',
	},
	pvp: {
		id: 'app.onboarding.recommendations.reason.pvp',
		defaultMessage: 'Sharpen your competitive edge',
	},
	speedrunning: {
		id: 'app.onboarding.recommendations.reason.speedrunning',
		defaultMessage: 'Essential for speedrunning',
	},
	exploring: {
		id: 'app.onboarding.recommendations.reason.exploring',
		defaultMessage: 'Better worlds to explore',
	},
	friends: {
		id: 'app.onboarding.recommendations.reason.friends',
		defaultMessage: 'Play with friends more easily',
	},
	publicServers: {
		id: 'app.onboarding.recommendations.reason.public-servers',
		defaultMessage: 'Useful on public servers',
	},
	contentCreation: {
		id: 'app.onboarding.recommendations.reason.content-creation',
		defaultMessage: 'Handy for creating content',
	},
	newToMods: {
		id: 'app.onboarding.recommendations.reason.new-to-mods',
		defaultMessage: 'A great starting point for modding',
	},
})

export interface Recommendation {
	slug: string
	projectType: 'mod' | 'modpack' | 'shader'
	reasonMessage: MessageDescriptor
}

type RecommendationSource = Omit<Recommendation, 'reasonMessage'>

const PRIORITY_RECOMMENDATIONS: Record<string, RecommendationSource[]> = {
	performance: [
		{ slug: 'sodium', projectType: 'mod' },
		{ slug: 'lithium', projectType: 'mod' },
		{ slug: 'ferrite-core', projectType: 'mod' },
	],
	visuals: [
		{ slug: 'iris', projectType: 'mod' },
		{ slug: 'complementary-reimagined', projectType: 'shader' },
		{ slug: 'distanthorizons', projectType: 'mod' },
	],
	simplicity: [
		{ slug: 'fabulously-optimized', projectType: 'modpack' },
		{ slug: 'adrenaline', projectType: 'modpack' },
	],
	customization: [
		{ slug: 'continuity', projectType: 'mod' },
		{ slug: '3dskinlayers', projectType: 'mod' },
		{ slug: 'ok-zoomer', projectType: 'mod' },
	],
	stability: [
		{ slug: 'modernfix', projectType: 'mod' },
		{ slug: 'ferrite-core', projectType: 'mod' },
	],
}

const PLAYSTYLE_RECOMMENDATIONS: Record<string, RecommendationSource[]> = {
	'survival-building': [
		{ slug: 'appleskin', projectType: 'mod' },
		{ slug: 'xaeros-minimap', projectType: 'mod' },
	],
	'modded-adventures': [
		{ slug: 'create', projectType: 'mod' },
		{ slug: 'cobblemon', projectType: 'mod' },
	],
	pvp: [
		{ slug: 'krypton', projectType: 'mod' },
		{ slug: 'entityculling', projectType: 'mod' },
	],
	speedrunning: [
		{ slug: 'speedrunigt', projectType: 'mod' },
		{ slug: 'sodium', projectType: 'mod' },
	],
	exploring: [
		{ slug: 'terralith', projectType: 'mod' },
		{ slug: 'xaeros-world-map', projectType: 'mod' },
	],
}

const SOCIAL_RECOMMENDATIONS: Record<string, RecommendationSource[]> = {
	friends: [
		{ slug: 'simple-voice-chat', projectType: 'mod' },
		{ slug: 'e4mc', projectType: 'mod' },
	],
	'public-servers': [
		{ slug: 'no-chat-reports', projectType: 'mod' },
		{ slug: 'viafabricplus', projectType: 'mod' },
	],
	'content-creation': [
		{ slug: 'replaymod', projectType: 'mod' },
		{ slug: 'iris', projectType: 'mod' },
	],
}

const NEW_TO_MODS_RECOMMENDATIONS: RecommendationSource[] = [
	{ slug: 'fabulously-optimized', projectType: 'modpack' },
	{ slug: 'adrenaline', projectType: 'modpack' },
]

const REASON_BY_CATEGORY: Record<string, MessageDescriptor> = {
	performance: reasonMessages.performance,
	visuals: reasonMessages.visuals,
	simplicity: reasonMessages.simplicity,
	customization: reasonMessages.customization,
	stability: reasonMessages.stability,
	'survival-building': reasonMessages.survivalBuilding,
	'modded-adventures': reasonMessages.moddedAdventures,
	pvp: reasonMessages.pvp,
	speedrunning: reasonMessages.speedrunning,
	exploring: reasonMessages.exploring,
	friends: reasonMessages.friends,
	'public-servers': reasonMessages.publicServers,
	'content-creation': reasonMessages.contentCreation,
}

/**
 * Compiles a short, deduplicated list of curated recommendations from the
 * onboarding answers, in priorities > playstyles > social > experience order.
 * Users who don't use mods only get modpack suggestions, since standalone
 * mods and shaders require a loader.
 */
export function buildRecommendations(answers: OnboardingAnswers): Recommendation[] {
	const groups: [string[], Record<string, RecommendationSource[]>][] = [
		[answers.priorities, PRIORITY_RECOMMENDATIONS],
		[answers.playstyles, PLAYSTYLE_RECOMMENDATIONS],
		[answers.social, SOCIAL_RECOMMENDATIONS],
	]

	const result: Recommendation[] = []
	const seen = new Set<string>()

	const push = (source: RecommendationSource, reasonMessage: MessageDescriptor) => {
		if (seen.has(source.slug)) return
		seen.add(source.slug)
		result.push({ ...source, reasonMessage })
	}

	if (answers.experience === 'new') {
		for (const source of NEW_TO_MODS_RECOMMENDATIONS) {
			push(source, reasonMessages.newToMods)
		}
	}

	for (const [selected, mapping] of groups) {
		for (const category of selected) {
			for (const source of mapping[category] ?? []) {
				push(source, REASON_BY_CATEGORY[category])
			}
		}
	}

	const modsUsable = answers.loaders.some((loader) => loader !== 'none')
	const filtered = modsUsable ? result : result.filter((r) => r.projectType === 'modpack')

	return filtered.slice(0, MAX_RECOMMENDATIONS)
}
