import config from '@kael/tooling-config/eslint/nuxt.mjs'

export default config.append([
	{
		ignores: ['dist/'],
	},
])
