<script setup lang="ts">
import { PlusIcon } from '@kael/assets'
import { ButtonStyled, injectNotificationManager } from '@kael/ui'
import { useQuery, useQueryClient } from '@tanstack/vue-query'
import { computed, onUnmounted, reactive, ref } from 'vue'

import ServerConsole from '@/components/ui/ServerConsole.vue'
import { loading_listener } from '@/helpers/events.js'
import {
	createServer,
	ensureTunnel,
	type HostedServer,
	listServers,
	removeServer,
	runningServers,
	serverAddress,
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
	if (error instanceof Error) return error.message
	if (error && typeof error === 'object' && 'message' in error) {
		return String((error as { message: unknown }).message)
	}
	return String(error)
}

type Step = 'name' | 'creating' | 'tunnel' | 'done' | 'error'

const wizardOpen = ref(false)
const step = ref<Step>('name')
const newName = ref('')
const progress = ref(0)
const progressMessage = ref('')
const resultUrl = ref('')
const errorMessage = ref('')
const buildWarning = ref('')
let flowToken = 0

let unlistenLoading: (() => void) | undefined

loading_listener((event: LoadingEvent) => {
	if (event?.event?.type === 'server_download' && step.value === 'creating') {
		progress.value = event.fraction ?? 1
		if (event.message) {
			progressMessage.value = event.message
			// Progress messages are transient, but a server built on a non-stable
			// Paper build stays that way — keep it visible through to the end.
			if (event.message.startsWith('Warning:')) {
				buildWarning.value = event.message.replace(/^Warning:\s*/, '')
			}
		}
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
	resultUrl.value = ''
	errorMessage.value = ''
	buildWarning.value = ''
	wizardOpen.value = true
}

function closeWizard() {
	flowToken += 1
	wizardOpen.value = false
	invalidate()
}

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
		invalidate()

		step.value = 'tunnel'
		await startServer(server.id)
		if (token !== flowToken) return
		const url = await ensureTunnel(server.id)
		if (token !== flowToken) return
		resultUrl.value = url
		step.value = 'done'
		invalidate()
	} catch (error) {
		if (token !== flowToken) return
		errorMessage.value = messageOf(error)
		step.value = 'error'
	}
}

const sortedServers = computed(() => servers.value ?? [])
</script>

<template>
	<div class="hosting-page">
		<div class="hosting-header">
			<div>
				<h1>Server Hosting</h1>
				<p>Host a Minecraft server and share it with friends over Minekube Connect.</p>
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
				<div class="server-row">
					<div class="server-info">
						<div class="server-title">
							<span class="status-dot" :class="{ online: isRunning(server.id) }" />
							<span class="name">{{ server.name }}</span>
							<span class="version">{{ server.mc_version }}</span>
							<span
								v-if="server.flavor === 'vanilla'"
								class="version"
								title="Created before Kael hosted on Paper. Players join with offline UUIDs, so whitelists and ops do not verify identity. Create a new server to get real Mojang accounts."
							>
								legacy
							</span>
						</div>
						<div v-if="serverAddress(server)" class="tunnel">
							<span class="tunnel-url">{{ serverAddress(server) }}</span>
							<button class="link-button" @click="copy(serverAddress(server)!)">Copy</button>
						</div>
						<div v-else class="tunnel muted">No address yet — activate to create one.</div>
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
				</div>
				<ServerConsole
					v-if="isRunning(server.id)"
					:server-id="server.id"
					:running="isRunning(server.id)"
				/>
			</li>
		</ul>

		<div v-if="wizardOpen" class="wizard-backdrop" @click.self="closeWizard">
			<div class="wizard">
				<template v-if="step === 'name'">
					<h2>New Server</h2>
					<p>
						We'll download a Paper server and set everything up for you. Paper lets friends join
						with their real Minecraft accounts, so whitelists and ops work.
					</p>
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

				<template v-else-if="step === 'tunnel'">
					<h2>Linking your tunnel</h2>
					<p>Connecting to the Minekube Connect network…</p>
					<div class="progress">
						<div class="progress-fill indeterminate" />
					</div>
				</template>

				<template v-else-if="step === 'done'">
					<h2>Server ready</h2>
					<p>Everyone joins with this address — including you, on this device:</p>
					<div class="result-url">
						<span>{{ resultUrl }}</span>
						<button class="link-button" @click="copy(resultUrl)">Copy</button>
					</div>
					<p v-if="buildWarning" class="warning-text">{{ buildWarning }}</p>
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
	flex-direction: column;
	gap: var(--gap-md);
	padding: var(--gap-md);
	background: var(--color-raised-bg);
	border-radius: var(--radius-md);
}

.server-row {
	display: flex;
	align-items: center;
	justify-content: space-between;
	gap: var(--gap-md);
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

.wizard-actions {
	display: flex;
	gap: var(--gap-sm);
	justify-content: flex-end;
	flex-wrap: wrap;
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

.warning-text {
	color: var(--color-orange, var(--color-red));
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
