<script setup lang="ts">
import { PlusIcon } from '@kael/assets'
import { ButtonStyled, injectNotificationManager } from '@kael/ui'
import { useQuery, useQueryClient } from '@tanstack/vue-query'
import { openUrl } from '@tauri-apps/plugin-opener'
import { computed, onUnmounted, reactive, ref } from 'vue'

import { loading_listener } from '@/helpers/events.js'
import {
	createServer,
	ensureTunnel,
	type HostedServer,
	listServers,
	playitBeginClaim,
	playitHasAccount,
	playitPollClaim,
	removeServer,
	runningServers,
	startServer,
	stopServer,
} from '@/helpers/hosting'

const { handleError } = injectNotificationManager()
const queryClient = useQueryClient()

const { data: servers } = useQuery({
	queryKey: ['hosting', 'servers'],
	queryFn: () => listServers(),
})

const { data: running } = useQuery({
	queryKey: ['hosting', 'running'],
	queryFn: () => runningServers(),
	refetchInterval: 4000,
})

const runningSet = computed(() => new Set(running.value ?? []))
const isRunning = (id: string) => runningSet.value.has(id)

const busy = reactive<Record<string, boolean>>({})

function invalidate() {
	void queryClient.invalidateQueries({ queryKey: ['hosting'] })
}

async function activate(server: HostedServer) {
	busy[server.id] = true
	try {
		await startServer(server.id)
	} catch (error) {
		handleError(error)
	} finally {
		busy[server.id] = false
		invalidate()
	}
}

async function stop(server: HostedServer) {
	busy[server.id] = true
	try {
		await stopServer(server.id)
	} catch (error) {
		handleError(error)
	} finally {
		busy[server.id] = false
		invalidate()
	}
}

async function destroy(server: HostedServer) {
	if (!window.confirm(`Delete "${server.name}"? This removes its files and tunnel.`)) {
		return
	}
	busy[server.id] = true
	try {
		await removeServer(server.id)
	} catch (error) {
		handleError(error)
	} finally {
		busy[server.id] = false
		invalidate()
	}
}

async function copy(text: string) {
	try {
		await navigator.clipboard.writeText(text)
	} catch (error) {
		handleError(error)
	}
}

interface LoadingEvent {
	event?: { type?: string; name?: string }
	fraction?: number | null
	message?: string
}

function messageOf(error: unknown): string {
	return error instanceof Error ? error.message : String(error)
}

type Step = 'name' | 'creating' | 'playit' | 'tunnel' | 'done' | 'error'

const wizardOpen = ref(false)
const step = ref<Step>('name')
const newName = ref('')
const progress = ref(0)
const progressMessage = ref('')
const claimUrl = ref('')
const claimStatus = ref('')
const resultUrl = ref('')
const errorMessage = ref('')
const createdId = ref('')
let flowToken = 0

let unlistenLoading: (() => void) | undefined

loading_listener((event: LoadingEvent) => {
	if (event?.event?.type === 'server_download' && step.value === 'creating') {
		progress.value = event.fraction ?? 1
		if (event.message) progressMessage.value = event.message
	}
}).then((unlisten) => {
	unlistenLoading = unlisten
})

onUnmounted(() => {
	flowToken += 1
	unlistenLoading?.()
})

function openWizard() {
	flowToken += 1
	step.value = 'name'
	newName.value = ''
	progress.value = 0
	progressMessage.value = ''
	claimUrl.value = ''
	claimStatus.value = ''
	resultUrl.value = ''
	errorMessage.value = ''
	createdId.value = ''
	wizardOpen.value = true
}

function closeWizard() {
	flowToken += 1
	wizardOpen.value = false
	invalidate()
}

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms))

async function beginCreate() {
	const name = newName.value.trim()
	if (!name) return

	const token = ++flowToken
	step.value = 'creating'
	progress.value = 0
	progressMessage.value = 'Starting…'

	try {
		const server = await createServer(name)
		if (token !== flowToken) return
		createdId.value = server.id
		invalidate()

		const hasAccount = await playitHasAccount()
		if (token !== flowToken) return

		if (hasAccount) {
			await finishTunnel(server.id, token)
		} else {
			step.value = 'playit'
		}
	} catch (error) {
		if (token !== flowToken) return
		errorMessage.value = messageOf(error)
		step.value = 'error'
	}
}

async function setupPlayit(guest: boolean) {
	const token = flowToken
	try {
		const info = await playitBeginClaim()
		if (token !== flowToken) return
		claimUrl.value = info.url
		claimStatus.value = 'Waiting for you to approve playit in your browser…'
		await openUrl(info.url)

		while (token === flowToken && step.value === 'playit') {
			const poll = await playitPollClaim(info.code, guest)
			if (token !== flowToken) return

			if (poll.secret) {
				if (createdId.value) await finishTunnel(createdId.value, token)
				return
			}

			if (poll.status === 'rejected') {
				throw new Error('playit setup was rejected in the browser.')
			}

			claimStatus.value =
				poll.status === 'waiting_user'
					? 'Approve this program in your browser…'
					: 'Waiting for you to open the playit link…'
			await sleep(2500)
		}
	} catch (error) {
		if (token !== flowToken) return
		errorMessage.value = messageOf(error)
		step.value = 'error'
	}
}

async function finishTunnel(id: string, token: number) {
	step.value = 'tunnel'
	const url = await ensureTunnel(id)
	if (token !== flowToken) return
	resultUrl.value = url
	step.value = 'done'
	invalidate()
}

const sortedServers = computed(() => servers.value ?? [])
</script>

<template>
	<div class="hosting-page">
		<div class="hosting-header">
			<div>
				<h1>Server Hosting</h1>
				<p>Host a Minecraft server and share it with friends over a playit.gg tunnel.</p>
			</div>
			<ButtonStyled color="brand">
				<button @click="openWizard">
					<PlusIcon />
					New Server
				</button>
			</ButtonStyled>
		</div>

		<div v-if="sortedServers.length === 0" class="empty">
			<h3>No servers yet</h3>
			<p>Create one to get started.</p>
		</div>

		<ul v-else class="server-list">
			<li v-for="server in sortedServers" :key="server.id" class="server-card">
				<div class="server-info">
					<div class="server-title">
						<span class="status-dot" :class="{ online: isRunning(server.id) }" />
						<span class="name">{{ server.name }}</span>
						<span class="version">{{ server.mc_version }}</span>
					</div>
					<div v-if="server.tunnel_url" class="tunnel">
						<span class="tunnel-url">{{ server.tunnel_url }}</span>
						<button class="link-button" @click="copy(server.tunnel_url!)">Copy</button>
					</div>
					<div v-else class="tunnel muted">No tunnel yet — activate to create one.</div>
				</div>
				<div class="server-actions">
					<ButtonStyled v-if="!isRunning(server.id)" color="green">
						<button :disabled="busy[server.id]" @click="activate(server)">
							{{ busy[server.id] ? 'Starting…' : 'Activate' }}
						</button>
					</ButtonStyled>
					<ButtonStyled v-else color="red">
						<button :disabled="busy[server.id]" @click="stop(server)">Stop</button>
					</ButtonStyled>
					<ButtonStyled color="red">
						<button :disabled="busy[server.id]" @click="destroy(server)">Delete</button>
					</ButtonStyled>
				</div>
			</li>
		</ul>

		<div v-if="wizardOpen" class="wizard-backdrop" @click.self="closeWizard">
			<div class="wizard">
				<template v-if="step === 'name'">
					<h2>New Server</h2>
					<p>We'll download the latest Minecraft server and set everything up for you.</p>
					<input
						v-model="newName"
						class="name-input"
						placeholder="Server name"
						@keydown.enter="beginCreate"
					/>
					<div class="wizard-actions">
						<ButtonStyled>
							<button @click="closeWizard">Cancel</button>
						</ButtonStyled>
						<ButtonStyled color="brand">
							<button :disabled="!newName.trim()" @click="beginCreate">Create</button>
						</ButtonStyled>
					</div>
				</template>

				<template v-else-if="step === 'creating'">
					<h2>Setting up your server</h2>
					<p>{{ progressMessage }}</p>
					<div class="progress">
						<div class="progress-fill" :style="{ width: `${Math.round(progress * 100)}%` }" />
					</div>
				</template>

				<template v-else-if="step === 'playit'">
					<h2>Connect playit.gg</h2>
					<p>
						playit.gg exposes your server to the internet. Pick how to set it up — a one-time
						browser approval is required either way.
					</p>
					<div v-if="!claimUrl" class="playit-buttons">
						<ButtonStyled color="brand">
							<button @click="setupPlayit(true)">Quick setup (guest)</button>
						</ButtonStyled>
						<ButtonStyled>
							<button @click="setupPlayit(false)">Use my playit.gg account</button>
						</ButtonStyled>
					</div>
					<div v-else class="playit-waiting">
						<p>{{ claimStatus }}</p>
						<button class="link-button" @click="openUrl(claimUrl)">Reopen playit link</button>
					</div>
					<div class="wizard-actions">
						<ButtonStyled>
							<button @click="closeWizard">Cancel</button>
						</ButtonStyled>
					</div>
				</template>

				<template v-else-if="step === 'tunnel'">
					<h2>Creating tunnel</h2>
					<p>Reserving a public address from playit.gg…</p>
					<div class="progress">
						<div class="progress-fill indeterminate" />
					</div>
				</template>

				<template v-else-if="step === 'done'">
					<h2>Server ready</h2>
					<p>Share this address with your friends:</p>
					<div class="result-url">
						<span>{{ resultUrl }}</span>
						<button class="link-button" @click="copy(resultUrl)">Copy</button>
					</div>
					<div class="wizard-actions">
						<ButtonStyled color="brand">
							<button @click="closeWizard">Done</button>
						</ButtonStyled>
					</div>
				</template>

				<template v-else>
					<h2>Something went wrong</h2>
					<p class="error-text">{{ errorMessage }}</p>
					<div class="wizard-actions">
						<ButtonStyled>
							<button @click="closeWizard">Close</button>
						</ButtonStyled>
					</div>
				</template>
			</div>
		</div>
	</div>
</template>

<style lang="scss" scoped>
.hosting-page {
	padding: var(--gap-lg);
	display: flex;
	flex-direction: column;
	gap: var(--gap-lg);
}

.hosting-header {
	display: flex;
	align-items: flex-start;
	justify-content: space-between;
	gap: var(--gap-md);

	h1 {
		margin: 0;
	}

	p {
		margin: var(--gap-xs) 0 0;
		color: var(--color-secondary);
	}
}

.empty {
	display: flex;
	flex-direction: column;
	align-items: center;
	gap: var(--gap-sm);
	padding: var(--gap-xl);
	color: var(--color-secondary);
}

.server-list {
	list-style: none;
	margin: 0;
	padding: 0;
	display: flex;
	flex-direction: column;
	gap: var(--gap-md);
}

.server-card {
	display: flex;
	align-items: center;
	justify-content: space-between;
	gap: var(--gap-md);
	padding: var(--gap-md);
	background: var(--color-raised-bg);
	border-radius: var(--radius-md);
}

.server-info {
	display: flex;
	flex-direction: column;
	gap: var(--gap-xs);
	min-width: 0;
}

.server-title {
	display: flex;
	align-items: center;
	gap: var(--gap-sm);

	.name {
		font-weight: 600;
	}

	.version {
		color: var(--color-secondary);
		font-size: var(--font-size-sm);
	}
}

.status-dot {
	width: 0.6rem;
	height: 0.6rem;
	border-radius: 50%;
	background: var(--color-secondary);

	&.online {
		background: var(--color-green);
	}
}

.tunnel {
	display: flex;
	align-items: center;
	gap: var(--gap-sm);
	font-family: var(--mono-font, monospace);

	&.muted {
		color: var(--color-secondary);
		font-family: inherit;
	}
}

.server-actions {
	display: flex;
	gap: var(--gap-sm);
	flex-shrink: 0;
}

.link-button {
	background: none;
	border: none;
	color: var(--color-link, var(--color-brand));
	cursor: pointer;
	padding: 0;
	font-size: var(--font-size-sm);
}

.wizard-backdrop {
	position: fixed;
	inset: 0;
	background: rgba(0, 0, 0, 0.5);
	display: flex;
	align-items: center;
	justify-content: center;
	z-index: 100;
}

.wizard {
	background: var(--color-raised-bg);
	border-radius: var(--radius-lg);
	padding: var(--gap-lg);
	width: min(32rem, 90vw);
	display: flex;
	flex-direction: column;
	gap: var(--gap-md);

	h2 {
		margin: 0;
	}

	p {
		margin: 0;
		color: var(--color-secondary);
	}
}

.name-input {
	padding: var(--gap-sm);
	border-radius: var(--radius-md);
	border: 1px solid var(--color-button-bg);
	background: var(--color-bg);
	color: var(--color-base);
}

.wizard-actions,
.playit-buttons {
	display: flex;
	gap: var(--gap-sm);
	justify-content: flex-end;
	flex-wrap: wrap;
}

.playit-buttons {
	justify-content: flex-start;
}

.playit-waiting {
	display: flex;
	flex-direction: column;
	gap: var(--gap-xs);
}

.progress {
	height: 0.6rem;
	border-radius: var(--radius-md);
	background: var(--color-button-bg);
	overflow: hidden;
}

.progress-fill {
	height: 100%;
	background: var(--color-brand);
	transition: width 0.2s ease;

	&.indeterminate {
		width: 40%;
		animation: slide 1.2s ease-in-out infinite;
	}
}

.result-url {
	display: flex;
	align-items: center;
	justify-content: space-between;
	gap: var(--gap-sm);
	padding: var(--gap-sm);
	border-radius: var(--radius-md);
	background: var(--color-bg);
	font-family: var(--mono-font, monospace);
}

.error-text {
	color: var(--color-red);
}

@keyframes slide {
	0% {
		margin-left: 0;
	}
	50% {
		margin-left: 60%;
	}
	100% {
		margin-left: 0;
	}
}
</style>
