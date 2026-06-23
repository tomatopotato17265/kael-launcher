import type { AbstractModrinthClient } from '@kael/api-client'

import { createContext } from './create-context'

export const [injectModrinthClient, provideModrinthClient] = createContext<AbstractModrinthClient>(
	'root',
	'modrinthClient',
)
