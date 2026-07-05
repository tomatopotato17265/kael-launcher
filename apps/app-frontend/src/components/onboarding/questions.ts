import { defineMessages } from '@kael/ui'

import type { QuestionDefinition } from '@/components/onboarding/types.ts'

const loaderMessages = defineMessages({
	title: {
		id: 'app.onboarding.q.loaders.title',
		defaultMessage: 'Which mod loader(s) do you use?',
	},
	fabric: {
		id: 'app.onboarding.q.loaders.fabric',
		defaultMessage: 'Fabric',
	},
	forge: {
		id: 'app.onboarding.q.loaders.forge',
		defaultMessage: 'Forge',
	},
	neoforge: {
		id: 'app.onboarding.q.loaders.neoforge',
		defaultMessage: 'NeoForge',
	},
	quilt: {
		id: 'app.onboarding.q.loaders.quilt',
		defaultMessage: 'Quilt',
	},
	none: {
		id: 'app.onboarding.q.loaders.none',
		defaultMessage: "I don't use mods",
	},
})

const playstyleMessages = defineMessages({
	title: {
		id: 'app.onboarding.q.playstyles.title',
		defaultMessage: 'What kind of player are you?',
	},
	survivalBuilding: {
		id: 'app.onboarding.q.playstyles.survival-building',
		defaultMessage: 'Survival & building',
	},
	moddedAdventures: {
		id: 'app.onboarding.q.playstyles.modded-adventures',
		defaultMessage: 'Modded adventures',
	},
	pvp: {
		id: 'app.onboarding.q.playstyles.pvp',
		defaultMessage: 'PvP & competitive',
	},
	speedrunning: {
		id: 'app.onboarding.q.playstyles.speedrunning',
		defaultMessage: 'Speedrunning',
	},
	exploring: {
		id: 'app.onboarding.q.playstyles.exploring',
		defaultMessage: 'Just exploring',
	},
})

const socialMessages = defineMessages({
	title: {
		id: 'app.onboarding.q.social.title',
		defaultMessage: 'How do you usually play?',
	},
	solo: {
		id: 'app.onboarding.q.social.solo',
		defaultMessage: 'Solo',
	},
	friends: {
		id: 'app.onboarding.q.social.friends',
		defaultMessage: 'With friends',
	},
	publicServers: {
		id: 'app.onboarding.q.social.public-servers',
		defaultMessage: 'On public servers',
	},
	contentCreation: {
		id: 'app.onboarding.q.social.content-creation',
		defaultMessage: 'Content creation/streaming',
	},
})

const priorityMessages = defineMessages({
	title: {
		id: 'app.onboarding.q.priorities.title',
		defaultMessage: 'What matters most to you in a launcher?',
	},
	performance: {
		id: 'app.onboarding.q.priorities.performance',
		defaultMessage: 'Performance',
	},
	visuals: {
		id: 'app.onboarding.q.priorities.visuals',
		defaultMessage: 'Visual/shader quality',
	},
	simplicity: {
		id: 'app.onboarding.q.priorities.simplicity',
		defaultMessage: 'Simplicity',
	},
	customization: {
		id: 'app.onboarding.q.priorities.customization',
		defaultMessage: 'Customization',
	},
	stability: {
		id: 'app.onboarding.q.priorities.stability',
		defaultMessage: 'Stability/no crashes',
	},
})

const experienceMessages = defineMessages({
	title: {
		id: 'app.onboarding.q.experience.title',
		defaultMessage: 'How experienced are you with modding?',
	},
	new: {
		id: 'app.onboarding.q.experience.new',
		defaultMessage: 'New to mods',
	},
	basics: {
		id: 'app.onboarding.q.experience.basics',
		defaultMessage: 'I know the basics',
	},
	experienced: {
		id: 'app.onboarding.q.experience.experienced',
		defaultMessage: "I'm experienced/I make my own packs",
	},
})

export const ONBOARDING_QUESTIONS: QuestionDefinition[] = [
	{
		id: 'loaders',
		mode: 'multi',
		titleMessage: loaderMessages.title,
		options: [
			{ id: 'fabric', labelMessage: loaderMessages.fabric },
			{ id: 'forge', labelMessage: loaderMessages.forge },
			{ id: 'neoforge', labelMessage: loaderMessages.neoforge },
			{ id: 'quilt', labelMessage: loaderMessages.quilt },
			{ id: 'none', labelMessage: loaderMessages.none, exclusive: true },
		],
	},
	{
		id: 'playstyles',
		mode: 'multi',
		titleMessage: playstyleMessages.title,
		options: [
			{ id: 'survival-building', labelMessage: playstyleMessages.survivalBuilding },
			{ id: 'modded-adventures', labelMessage: playstyleMessages.moddedAdventures },
			{ id: 'pvp', labelMessage: playstyleMessages.pvp },
			{ id: 'speedrunning', labelMessage: playstyleMessages.speedrunning },
			{ id: 'exploring', labelMessage: playstyleMessages.exploring },
		],
	},
	{
		id: 'social',
		mode: 'multi',
		titleMessage: socialMessages.title,
		options: [
			{ id: 'solo', labelMessage: socialMessages.solo },
			{ id: 'friends', labelMessage: socialMessages.friends },
			{ id: 'public-servers', labelMessage: socialMessages.publicServers },
			{ id: 'content-creation', labelMessage: socialMessages.contentCreation },
		],
	},
	{
		id: 'priorities',
		mode: 'multi',
		titleMessage: priorityMessages.title,
		options: [
			{ id: 'performance', labelMessage: priorityMessages.performance },
			{ id: 'visuals', labelMessage: priorityMessages.visuals },
			{ id: 'simplicity', labelMessage: priorityMessages.simplicity },
			{ id: 'customization', labelMessage: priorityMessages.customization },
			{ id: 'stability', labelMessage: priorityMessages.stability },
		],
	},
	{
		id: 'experience',
		mode: 'single',
		titleMessage: experienceMessages.title,
		options: [
			{ id: 'new', labelMessage: experienceMessages.new },
			{ id: 'basics', labelMessage: experienceMessages.basics },
			{ id: 'experienced', labelMessage: experienceMessages.experienced },
		],
	},
]
