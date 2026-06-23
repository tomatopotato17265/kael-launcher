import { type AuthProvider, provideAuth } from '@kael/ui'
import { ref } from 'vue'

export function setupAuthProvider() {
	const authProvider: AuthProvider = {
		session_token: ref(null),
		user: ref(null),
		isReady: ref(true),
		requestSignIn: () => {},
	}
	provideAuth(authProvider)
}
